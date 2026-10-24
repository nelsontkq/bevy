//! Serves the BRP methods as [Model Context Protocol] tools.
//!
//! The [`RemoteHttpPlugin`](crate::http::RemoteHttpPlugin) answers MCP requests `POST`ed to [`MCP_PATH`], using
//! the stateless form of the Streamable HTTP transport: every request gets a single JSON response, and there
//! is no session or server-initiated stream. Each method listed by `rpc.discover` becomes one tool, whose
//! description and input schema come from the same reflected types.
//!
//! [Model Context Protocol]: https://modelcontextprotocol.io

use crate::{
    builtin_methods::RPC_DISCOVER_METHOD,
    error_codes,
    schemas::open_rpc::{MethodObject, OpenRpcDocument, Parameter},
    BrpError, BrpMessage, BrpResponse, BrpResult,
};
use async_channel::Sender;
use serde_json::{json, Map, Value};

/// The path that the HTTP transport serves MCP on.
pub const MCP_PATH: &str = "/mcp";

/// The protocol revisions this server can speak, newest first.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// Schema keywords that restrict which JSON values are accepted.
const CONSTRAINING_KEYS: [&str; 7] = ["type", "oneOf", "anyOf", "allOf", "enum", "const", "$ref"];

/// The type path of [`Json`](crate::Json), which is untyped and so not worth naming in a description.
const JSON_TYPE_PATH: &str = "bevy_remote::json::Json";

/// Handles one MCP message, returning `None` for the notifications and responses that expect no reply.
pub(crate) async fn process_message(
    message: Value,
    request_sender: &Sender<BrpMessage>,
) -> Option<BrpResponse> {
    let id = message.get("id").filter(|id| !id.is_null())?.clone();
    let method = message.get("method")?.as_str()?;
    let params = message.get("params");

    let result = match method {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => list_tools(request_sender).await,
        "tools/call" => call_tool(params, request_sender).await,
        _ => Err(BrpError {
            code: error_codes::METHOD_NOT_FOUND,
            message: format!("Method `{method}` not found"),
            data: None,
        }),
    };
    Some(BrpResponse::new(Some(id), result))
}

fn initialize(params: Option<&Value>) -> Value {
    let requested = params
        .and_then(|params| params.get("protocolVersion"))
        .and_then(Value::as_str);
    let version = PROTOCOL_VERSIONS
        .into_iter()
        .find(|version| Some(*version) == requested)
        .unwrap_or(PROTOCOL_VERSIONS[0]);

    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "bevy_remote", "version": env!("CARGO_PKG_VERSION") },
    })
}

async fn list_tools(request_sender: &Sender<BrpMessage>) -> BrpResult {
    let mut tools: Vec<Value> = Vec::new();
    for method in discover(request_sender).await? {
        let tool = tool(&method);
        // Distinct methods can share a tool name, in which case the first one keeps it.
        if !tools.iter().any(|other| other["name"] == tool["name"]) {
            tools.push(tool);
        }
    }
    Ok(json!({ "tools": tools }))
}

async fn call_tool(params: Option<&Value>, request_sender: &Sender<BrpMessage>) -> BrpResult {
    let Some(name) = params
        .and_then(|params| params.get("name"))
        .and_then(Value::as_str)
    else {
        return Err(BrpError {
            code: error_codes::INVALID_PARAMS,
            message: "Missing tool name".to_owned(),
            data: None,
        });
    };
    let Some(method) = discover(request_sender)
        .await?
        .into_iter()
        .find(|method| tool_name(&method.name) == name)
    else {
        return Err(BrpError {
            code: error_codes::INVALID_PARAMS,
            message: format!("Unknown tool: {name}"),
            data: None,
        });
    };
    // Methods without params are called with none, as an empty object is not a valid unit struct.
    let arguments = params
        .and_then(|params| params.get("arguments"))
        .filter(|arguments| arguments.as_object().is_some_and(|map| !map.is_empty()))
        .cloned();

    // A failing method is a tool result rather than a protocol error, so that the model gets to read it.
    let (text, is_error) = match call(&method.name, arguments, request_sender).await {
        Ok(result) => (
            serde_json::to_string_pretty(&result).map_err(BrpError::internal)?,
            false,
        ),
        Err(error) => (
            format!(
                "BRP error from `{}`: {}",
                method.name,
                serde_json::to_string(&error).map_err(BrpError::internal)?
            ),
            true,
        ),
    };
    Ok(json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    }))
}

/// Runs a BRP method in the world. A watching method returns its first update.
async fn call(
    method: &str,
    params: Option<Value>,
    request_sender: &Sender<BrpMessage>,
) -> BrpResult {
    let (sender, receiver) = async_channel::bounded(1);
    request_sender
        .send(BrpMessage {
            method: method.to_owned(),
            params,
            sender,
        })
        .await
        .map_err(BrpError::internal)?;
    receiver.recv().await.map_err(BrpError::internal)?
}

async fn discover(request_sender: &Sender<BrpMessage>) -> BrpResult<Vec<MethodObject>> {
    let document = call(RPC_DISCOVER_METHOD, None, request_sender).await?;
    let document: OpenRpcDocument = serde_json::from_value(document).map_err(BrpError::internal)?;
    Ok(document.methods)
}

/// Returns the MCP tool describing a BRP method.
fn tool(method: &MethodObject) -> Value {
    let mut description = method
        .description
        .clone()
        .unwrap_or_else(|| format!("BRP method `{}`.", method.name));
    let returns = method
        .result
        .as_ref()
        .and_then(|result| result.description.as_deref());
    if let Some(returns) = returns {
        description = format!("{description}\n\nReturns: {returns}");
    }

    json!({
        "name": tool_name(&method.name),
        "description": description,
        "inputSchema": input_schema(&method.params),
    })
}

/// Tool names allow fewer characters than method names: `world.get_components+watch` becomes
/// `world_get_components_watch`.
fn tool_name(method: &str) -> String {
    method
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Builds the schema of a tool's arguments object from the params of its method.
fn input_schema(params: &[Parameter]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for param in params {
        let mut schema = json_schema(&param.schema);
        if let Value::Object(schema) = &mut schema {
            let description = join(
                param.description.as_deref(),
                schema.get("description").and_then(Value::as_str),
            );
            if let Some(description) = description {
                schema.insert("description".to_owned(), description.into());
            }
        }
        properties.insert(param.name.clone(), schema);
        if param.required {
            required.push(param.name.as_str());
        }
    }

    let mut schema = json!({ "type": "object", "properties": properties });
    if !required.is_empty() {
        schema["required"] = required.into();
    }
    schema
}

/// Strips the `typePath` tags from an `rpc.discover` schema, and types the parts it leaves untyped.
fn json_schema(schema: &Value) -> Value {
    // `items: false` and `additionalProperties: false` are schemas too.
    let Value::Object(schema) = schema else {
        return schema.clone();
    };

    let mut out = Map::new();
    for (key, value) in schema {
        let value = match (key.as_str(), value) {
            ("typePath", _) => continue,
            ("properties", Value::Object(properties)) => properties
                .iter()
                .map(|(name, property)| (name.clone(), json_schema(property)))
                .collect::<Map<_, _>>()
                .into(),
            ("prefixItems" | "oneOf" | "anyOf" | "allOf", Value::Array(schemas)) => {
                schemas.iter().map(json_schema).collect()
            }
            ("items" | "additionalProperties", schema) => json_schema(schema),
            (_, value) => value.clone(),
        };
        out.insert(key.clone(), value);
    }

    if !CONSTRAINING_KEYS.iter().any(|key| out.contains_key(*key)) {
        // Clients send scalars as strings unless the schema lists the types it takes.
        out.insert(
            "type".to_owned(),
            json!(["object", "array", "string", "number", "integer", "boolean", "null"]),
        );
        let type_path = schema.get("typePath").and_then(Value::as_str);
        if let Some(type_path) = type_path.filter(|type_path| *type_path != JSON_TYPE_PATH) {
            let hint = format!("A serialized `{type_path}`.");
            let description = join(out.get("description").and_then(Value::as_str), Some(&hint));
            out.insert("description".to_owned(), description.into());
        }
    }
    out.into()
}

fn join(first: Option<&str>, second: Option<&str>) -> Option<String> {
    match (first, second) {
        (Some(first), Some(second)) => Some(format!("{first} {second}")),
        (first, second) => first.or(second).map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_are_sanitised() {
        assert_eq!(
            tool_name("world.get_components+watch"),
            "world_get_components_watch"
        );
    }

    #[test]
    fn untyped_schemas_accept_any_type() {
        let params = [
            Parameter {
                name: "entity".to_owned(),
                description: Some("The target.".to_owned()),
                required: true,
                schema: json!({ "typePath": "bevy_ecs::entity::Entity" }),
                ..Default::default()
            },
            Parameter {
                name: "value".to_owned(),
                schema: json!({ "typePath": JSON_TYPE_PATH }),
                ..Default::default()
            },
            Parameter {
                name: "counts".to_owned(),
                schema: json!({
                    "type": "array",
                    "typePath": "alloc::vec::Vec<u8>",
                    "items": { "type": "integer", "typePath": "u8" },
                }),
                ..Default::default()
            },
        ];

        let schema = input_schema(&params);
        let any = json!(["object", "array", "string", "number", "integer", "boolean", "null"]);
        assert_eq!(
            schema,
            json!({
                "type": "object",
                "properties": {
                    "entity": {
                        "type": any,
                        "description": "The target. A serialized `bevy_ecs::entity::Entity`.",
                    },
                    "value": { "type": any },
                    "counts": { "type": "array", "items": { "type": "integer" } },
                },
                "required": ["entity"],
            })
        );
    }

    #[test]
    fn tools_describe_their_result() {
        let method = MethodObject {
            name: "custom.echo".to_owned(),
            description: Some("Echoes the params.".to_owned()),
            result: Some(Parameter {
                description: Some("The echoed params.".to_owned()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let tool = tool(&method);
        assert_eq!(tool["name"], "custom_echo");
        assert_eq!(
            tool["description"],
            "Echoes the params.\n\nReturns: The echoed params."
        );
        assert_eq!(
            tool["inputSchema"],
            json!({ "type": "object", "properties": {} })
        );
    }

    #[test]
    fn initialize_negotiates_the_version() {
        let version = |requested: &str| {
            initialize(Some(&json!({ "protocolVersion": requested })))["protocolVersion"].clone()
        };
        assert_eq!(version("2025-03-26"), "2025-03-26");
        assert_eq!(version("1999-01-01"), PROTOCOL_VERSIONS[0]);
    }
}
