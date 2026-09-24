---
title: "Typed BRP method params"
pull_requests: []
---

The free-form fields of the built-in params (`components` and `value`) hold a `bevy_remote::Json` instead of a `serde_json::Value`. `Json` converts from and into a `serde_json::Value`:

```rust
// Before
BrpInsertResourcesParams { resource, value: json!({ "score": 3 }) }

// After
BrpInsertResourcesParams { resource, value: json!({ "score": 3 }).into() }

// Or, use the resource directly (if it derives Reflect)
BrpInsertResourcesParams { resource, value: Json::serialize_reflect(&Score { score: 3 })? }
```

`RemoteMethodHandler` is gone. `RemoteMethods::insert` now returns the previous `RemoteMethod` instead of its `RemoteMethodSystemId`, which is in its `system` field.

In `bevy_remote::schemas::open_rpc`, building the method list now needs the type registry:

```rust
// Before
let methods: Vec<MethodObject> = remote_methods.into();

// After
let methods = MethodObject::for_methods(remote_methods, &type_registry);
```

`Parameter::schema` is now a `serde_json::Value`, and `Parameter` and `MethodObject` gained the `required` and `result` fields.
