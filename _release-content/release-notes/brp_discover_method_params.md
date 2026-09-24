---
title: "Typed BRP methods described by rpc.discover"
authors: ["@nelsontkq"]
pull_requests: []
---

`rpc.discover` used to return only the names of the methods a BRP server understands.

Now a remote method can take its params as a reflected struct, and `rpc.discover` uses the doc comments and lists every param with its schema and whether it's required:

```rust
/// `super_user.set_score`: Set the player's score.
#[derive(Reflect)]
struct SetScoreParams {
    /// Name of the player.
    player: String,
    /// New score. Defaults to zero.
    #[reflect(default)]
    score: u32,
}

/// The score after the write.
#[derive(Reflect)]
struct SetScoreResponse {
    score: u32,
}

fn set_score(In(params): In<SetScoreParams>, world: &mut World) -> BrpResult<SetScoreResponse> { ... }

RemotePlugin::default().with_method_main("super_user.set_score", set_score)
```

`Reflect` is the only derive a method needs. Your crate doesn't even need `serde` or `serde_json`.

`Option` and `#[reflect(default)]` fields may be left out. Any other missing field is an `invalid_params` error, and the handler never runs.

Fields whose type is only known when the request comes in use `bevy_remote::Json`. Handlers read it with `Json::deserialize`. It converts to and from a `serde_json::Value`, and `Json::serialize_reflect` can serialize one from a real type:

```rust
BrpWriteMessageParams {
    message: type_name::<WindowEvent>().to_string(),
    value: Some(Json::serialize_reflect(&WindowEvent::MouseButtonInput(
        MouseButtonInput {
            button: MouseButton::Left,
            state: ButtonState::Pressed,
            window: window_entity,
        },
    ))?),
},
```

`rpc.discover` now shows the description and type info in the provided schema, pulled through reflection, so it stays in sync with the code.
All built-in methods are described this way:

```json
{
  "name": "world.get_components",
  "description": "`world.get_components`: Retrieves one or more components from the entity with the given\nID. [...]",
  "params": [
    {
      "name": "entity",
      "description": "The ID of the entity from which components are to be requested.",
      "required": true,
      "schema": { "type": "integer", "minimum": 0, "typePath": "bevy_ecs::entity::Entity" }
    },
    [...]
  ]
}
```

Existing handlers that take raw `In<Option<Value>>` params still work. They're just listed without type info.

If your params type already derives `Deserialize`, derive `Reflect` too and add `#[reflect(Deserialize)]` so `rpc.discover` can describe it.
