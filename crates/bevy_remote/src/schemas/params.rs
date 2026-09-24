//! `OpenRPC` descriptions of [`MethodParams`](crate::MethodParams) types for `rpc.discover`.

use core::any::TypeId;

use bevy_ecs::entity::Entity;
use bevy_reflect::{
    enums::VariantInfo, serde::SerializationData, NamedField, ReflectDeserialize, TypeInfo,
    TypeRegistry, UnnamedField,
};
use bevy_utils::prelude::default;
use serde_json::{json, Map, Value};

use super::open_rpc::Parameter;

/// Returns one `OpenRPC` parameter per field of a params struct.
pub fn method_params(info: &'static TypeInfo, registry: &TypeRegistry) -> Vec<Parameter> {
    let mut visiting = vec![info.type_id()];
    let TypeInfo::Struct(info) = info else {
        return Vec::new();
    };
    info.iter()
        .map(|field| {
            let (schema, is_option) = field_schema(field, registry, &mut visiting);
            Parameter {
                name: field.name().to_owned(),
                description: field.docs().map(clean_docs),
                required: !is_option && !field.has_default(),
                schema,
                extensions: default(),
            }
        })
        .collect()
}

/// Returns the method description from the params type's docs.
pub fn method_description(info: &'static TypeInfo) -> Option<String> {
    info.docs().map(clean_docs)
}

/// Returns the `OpenRPC` `result` for a reflected response type.
pub fn method_result(info: &'static TypeInfo, registry: &TypeRegistry) -> Parameter {
    Parameter {
        name: "result".to_owned(),
        description: info.docs().map(clean_docs),
        required: true,
        schema: inline_schema(info, registry, &mut Vec::new()).0,
        extensions: default(),
    }
}

/// Returns `true` for params types with no fields, like unit structs or `()`.
pub fn takes_no_params(info: &'static TypeInfo) -> bool {
    match info {
        TypeInfo::Struct(info) => info.field_len() == 0,
        TypeInfo::TupleStruct(info) => info.field_len() == 0,
        TypeInfo::Tuple(info) => info.field_len() == 0,
        _ => false,
    }
}

fn clean_docs(docs: &str) -> String {
    docs.lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// Returns the `T` in `Option<T>`, or `None` if the type isn't an `Option`.
pub(crate) fn option_inner(info: &'static TypeInfo) -> Option<&'static TypeInfo> {
    if !info.type_path().starts_with("core::option::Option<") {
        return None;
    }
    let TypeInfo::Enum(enum_info) = info else {
        return None;
    };
    enum_info
        .variant("Some")
        .and_then(|variant| match variant {
            VariantInfo::Tuple(tuple) => tuple.field_at(0),
            _ => None,
        })
        .and_then(UnnamedField::type_info)
}

/// Returns the field of a newtype, which reflection serializes as just that field.
pub(crate) fn newtype_field(
    info: &'static TypeInfo,
    registry: &TypeRegistry,
) -> Option<&'static UnnamedField> {
    let TypeInfo::TupleStruct(tuple_info) = info else {
        return None;
    };
    let has_skipped_fields = registry
        .get(info.type_id())
        .is_some_and(|registration| registration.data::<SerializationData>().is_some());
    if tuple_info.field_len() == 1 && !has_skipped_fields {
        tuple_info.field_at(0)
    } else {
        None
    }
}

fn field_schema(
    field: &NamedField,
    registry: &TypeRegistry,
    visiting: &mut Vec<TypeId>,
) -> (Value, bool) {
    match field.type_info() {
        Some(info) => inline_schema(info, registry, visiting),
        None => (json!({ "typePath": field.type_path() }), false),
    }
}

fn inline_schema(
    info: &'static TypeInfo,
    registry: &TypeRegistry,
    visiting: &mut Vec<TypeId>,
) -> (Value, bool) {
    let type_id = info.type_id();

    if let Some(inner) = option_inner(info) {
        return (nullable(inline_schema(inner, registry, visiting).0), true);
    }

    let mut schema = Map::new();
    schema.insert("typePath".to_owned(), info.type_path().into());

    if type_id == TypeId::of::<Entity>() {
        schema.insert("type".to_owned(), "integer".into());
        schema.insert("minimum".to_owned(), 0.into());
        return (schema.into(), false);
    }

    let deserializes_through_serde = registry
        .get(type_id)
        .is_some_and(|registration| registration.data::<ReflectDeserialize>().is_some());
    if deserializes_through_serde
        && matches!(
            info,
            TypeInfo::Struct(_) | TypeInfo::TupleStruct(_) | TypeInfo::Enum(_)
        )
    {
        return (schema.into(), false);
    }

    if visiting.contains(&type_id) {
        return (schema.into(), false);
    }
    visiting.push(type_id);

    if let Some(field) = newtype_field(info, registry) {
        if let Value::Object(inner) = unnamed_field_schema(field, registry, visiting) {
            schema.extend(inner);
            schema.insert("typePath".to_owned(), info.type_path().into());
        }
        visiting.pop();
        return (schema.into(), false);
    }

    match info {
        TypeInfo::Struct(struct_info) => {
            let mut properties = Map::new();
            let mut required = Vec::new();
            for field in struct_info.iter() {
                let (mut field_schema, is_option) = field_schema(field, registry, visiting);
                if !is_option && !field.has_default() {
                    required.push(Value::from(field.name()));
                }
                if let (Some(docs), Value::Object(object)) = (field.docs(), &mut field_schema) {
                    object.insert("description".to_owned(), clean_docs(docs).into());
                }
                properties.insert(field.name().to_owned(), field_schema);
            }
            schema.insert("type".to_owned(), "object".into());
            schema.insert("properties".to_owned(), properties.into());
            schema.insert("required".to_owned(), required.into());
            schema.insert("additionalProperties".to_owned(), false.into());
        }
        TypeInfo::TupleStruct(tuple_info) => {
            schema.extend(tuple_schema(tuple_info.iter(), registry, visiting));
        }
        TypeInfo::Tuple(tuple_info) => {
            schema.extend(tuple_schema(tuple_info.iter(), registry, visiting));
        }
        TypeInfo::List(list_info) => {
            schema.insert("type".to_owned(), "array".into());
            schema.insert(
                "items".to_owned(),
                optional_info_schema(list_info.item_info(), registry, visiting),
            );
        }
        TypeInfo::Array(array_info) => {
            schema.insert("type".to_owned(), "array".into());
            schema.insert(
                "items".to_owned(),
                optional_info_schema(array_info.item_info(), registry, visiting),
            );
            schema.insert("minItems".to_owned(), array_info.capacity().into());
            schema.insert("maxItems".to_owned(), array_info.capacity().into());
        }
        TypeInfo::Set(_) => {
            schema.insert("type".to_owned(), "array".into());
            schema.insert("uniqueItems".to_owned(), true.into());
        }
        TypeInfo::Map(map_info) => {
            schema.insert("type".to_owned(), "object".into());
            schema.insert(
                "additionalProperties".to_owned(),
                optional_info_schema(map_info.value_info(), registry, visiting),
            );
        }
        TypeInfo::Enum(enum_info) => {
            let unit_only = enum_info
                .iter()
                .all(|variant| matches!(variant, VariantInfo::Unit(_)));
            if unit_only {
                schema.insert("type".to_owned(), "string".into());
                schema.insert(
                    "enum".to_owned(),
                    enum_info
                        .iter()
                        .map(|variant| Value::from(variant.name()))
                        .collect::<Vec<_>>()
                        .into(),
                );
            } else {
                let variants: Vec<Value> = enum_info
                    .iter()
                    .map(|variant| match variant {
                        VariantInfo::Unit(unit) => json!({ "const": unit.name() }),
                        VariantInfo::Tuple(tuple) => {
                            let payload = if tuple.field_len() == 1 {
                                unnamed_field_schema(tuple.field_at(0).unwrap(), registry, visiting)
                            } else {
                                tuple_schema(tuple.iter(), registry, visiting).into()
                            };
                            variant_object(tuple.name(), payload)
                        }
                        VariantInfo::Struct(structure) => {
                            let mut properties = Map::new();
                            let mut required = Vec::new();
                            for field in structure.iter() {
                                let (field_schema, is_option) =
                                    field_schema(field, registry, visiting);
                                if !is_option && !field.has_default() {
                                    required.push(Value::from(field.name()));
                                }
                                properties.insert(field.name().to_owned(), field_schema);
                            }
                            variant_object(
                                structure.name(),
                                json!({
                                    "type": "object",
                                    "properties": properties,
                                    "required": required,
                                    "additionalProperties": false,
                                }),
                            )
                        }
                    })
                    .collect();
                schema.insert("oneOf".to_owned(), variants.into());
            }
        }
        TypeInfo::Opaque(_) => {
            let json_type = match info.type_path() {
                "bool" => Some("boolean"),
                "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64"
                | "i128" | "isize" => Some("integer"),
                "f32" | "f64" => Some("number"),
                "char" | "str" | "alloc::string::String" => Some("string"),
                _ => None,
            };
            if let Some(json_type) = json_type {
                schema.insert("type".to_owned(), json_type.into());
            }
        }
    }
    visiting.pop();
    (schema.into(), false)
}

fn variant_object(name: &str, payload: Value) -> Value {
    json!({
        "type": "object",
        "properties": { name: payload },
        "required": [name],
        "additionalProperties": false,
    })
}

fn tuple_schema<'a>(
    fields: impl Iterator<Item = &'a UnnamedField>,
    registry: &TypeRegistry,
    visiting: &mut Vec<TypeId>,
) -> Map<String, Value> {
    let items: Vec<_> = fields
        .map(|field| unnamed_field_schema(field, registry, visiting))
        .collect();
    let mut schema = Map::new();
    schema.insert("type".to_owned(), "array".into());
    schema.insert("prefixItems".to_owned(), items.into());
    schema.insert("items".to_owned(), false.into());
    schema
}

fn unnamed_field_schema(
    field: &UnnamedField,
    registry: &TypeRegistry,
    visiting: &mut Vec<TypeId>,
) -> Value {
    optional_info_schema(field.type_info(), registry, visiting)
}

fn nullable(schema: Value) -> Value {
    let Value::Object(mut schema) = schema else {
        return schema;
    };
    if let Some(Value::String(ty)) = schema.get("type") {
        let ty = ty.clone();
        schema.insert("type".to_owned(), json!([ty, "null"]));
    } else if let Some(Value::Array(variants)) = schema.get_mut("oneOf") {
        variants.push(json!({ "type": "null" }));
    }
    schema.into()
}

fn optional_info_schema(
    info: Option<&'static TypeInfo>,
    registry: &TypeRegistry,
    visiting: &mut Vec<TypeId>,
) -> Value {
    match info {
        Some(info) => inline_schema(info, registry, visiting).0,
        None => json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{builtin_methods::ComponentSelector, Json};
    use bevy_reflect::{Reflect, Typed};
    /// Very good user-facing description
    #[derive(Reflect)]
    struct TestParams {
        /// testyboy
        entity: Entity,
        components: Vec<String>,
        #[reflect(default)]
        default_aint_required: bool,
        none_aint_required: Option<Entity>,
        value: Json,
        selector: Selector,
        serde_selector: ComponentSelector,
    }

    #[derive(Reflect)]
    enum Selector {
        All,
        Paths(Vec<String>),
    }

    fn params_of<P: Typed>() -> Vec<Parameter> {
        let mut registry = TypeRegistry::new();
        registry.register::<TestParams>();
        method_params(P::type_info(), &registry)
    }

    fn param<'a>(params: &'a [Parameter], name: &str) -> &'a Parameter {
        params.iter().find(|param| param.name == name).unwrap()
    }

    #[test]
    fn describes_fields() {
        let params = params_of::<TestParams>();
        let required: Vec<_> = params
            .iter()
            .map(|param| (param.name.as_str(), param.required))
            .collect();
        assert_eq!(
            required,
            [
                ("entity", true),
                ("components", true),
                ("default_aint_required", false),
                ("none_aint_required", false),
                ("value", true),
                ("selector", true),
                ("serde_selector", true),
            ]
        );
        assert_eq!(
            method_description(TestParams::type_info()).as_deref(),
            Some("Very good user-facing description")
        );
        assert_eq!(
            param(&params, "entity").description.as_deref(),
            Some("testyboy")
        );
    }

    #[test]
    fn schemas_follow_the_reflection_format() {
        let params = params_of::<TestParams>();
        assert_eq!(param(&params, "entity").schema["type"], json!("integer"));
        assert_eq!(
            param(&params, "none_aint_required").schema["type"],
            json!(["integer", "null"])
        );
        assert_eq!(
            param(&params, "components").schema["items"]["type"],
            json!("string")
        );
        assert!(param(&params, "value").schema.get("type").is_none());
        assert_eq!(
            param(&params, "selector").schema["oneOf"][0],
            json!({ "const": "All" })
        );
        assert!(param(&params, "serde_selector")
            .schema
            .get("oneOf")
            .is_none());
    }

    #[test]
    fn recursive_types_are_expanded_once() {
        #[derive(Reflect)]
        #[reflect(no_field_bounds)]
        struct Tree {
            children: Vec<Tree>,
        }

        let params = params_of::<Tree>();
        let nested = &param(&params, "children").schema["items"];
        assert!(nested.get("properties").is_none());
    }
}
