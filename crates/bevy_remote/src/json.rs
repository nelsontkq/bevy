//! Untyped JSON fields for method params.

use bevy_reflect::{
    serde::TypedReflectDeserializer, std_traits::ReflectDefault, FromReflect, GetTypeRegistration,
    PartialReflect, Reflect, ReflectDeserialize, ReflectSerialize, TypeRegistration, TypeRegistry,
};
use serde::{de::DeserializeSeed as _, Deserialize, Serialize};
use serde_json::Value;

use crate::{error_codes, BrpError};

/// Untyped JSON in [`MethodParams`](crate::MethodParams), for values whose type is only known at runtime.
#[derive(Debug, Clone, Default, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(opaque, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Json(pub Value);

impl Json {
    /// Serializes a typed value into [`Json`].
    pub fn serialize_reflect<T: PartialReflect + GetTypeRegistration>(
        value: &T,
    ) -> Result<Self, serde_json::Error> {
        crate::builtin_methods::serialize_params(value).map(Self)
    }

    /// Deserializes into the type of `registration`.
    pub fn deserialize_reflect(
        &self,
        registration: &TypeRegistration,
        registry: &TypeRegistry,
    ) -> Result<Box<dyn PartialReflect>, BrpError> {
        TypedReflectDeserializer::new(registration, registry)
            .deserialize(&self.0)
            .map_err(|err| BrpError {
                code: error_codes::INVALID_PARAMS,
                message: format!("{} is invalid: {err}", registration.type_info().type_path()),
                data: None,
            })
    }

    /// Deserializes into a `T`. Only `T`'s field types need to be in `registry`.
    pub fn deserialize<T: FromReflect + GetTypeRegistration>(
        &self,
        registry: &TypeRegistry,
    ) -> Result<T, BrpError> {
        let registration = T::get_type_registration();
        let reflected = self.deserialize_reflect(&registration, registry)?;
        T::from_reflect(&*reflected).ok_or_else(|| BrpError {
            code: error_codes::INVALID_PARAMS,
            message: format!(
                "JSON does not form a `{}`",
                registration.type_info().type_path()
            ),
            data: None,
        })
    }
}

impl From<Value> for Json {
    fn from(value: Value) -> Self {
        Self(value)
    }
}

impl From<Json> for Value {
    fn from(json: Json) -> Self {
        json.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_reflect::Typed;
    use serde_json::json;

    #[derive(Reflect, Debug, PartialEq)]
    struct Point {
        x: f32,
        y: f32,
    }

    #[test]
    fn deserializes_by_registration_and_by_type() {
        let mut registry = TypeRegistry::new();
        registry.register::<Point>();
        let json = Json(json!({ "x": 1.0, "y": 2.0 }));

        let point: Point = json.deserialize(&registry).unwrap();
        assert_eq!(point, Point { x: 1.0, y: 2.0 });

        let registration = registry.get(core::any::TypeId::of::<Point>()).unwrap();
        let reflected = json.deserialize_reflect(registration, &registry).unwrap();
        assert!(reflected.represents::<Point>());

        let bad = Json(json!({ "x": "one" }));
        let err = bad.deserialize::<Point>(&registry).unwrap_err();
        assert_eq!(err.code, error_codes::INVALID_PARAMS);

        // Only the field types need to be registered.
        #[derive(Reflect, Debug, PartialEq)]
        struct Unregistered {
            point: Point,
        }
        let nested = Json(json!({ "point": { "x": 1.0, "y": 2.0 } }));
        assert_eq!(
            nested.deserialize::<Unregistered>(&registry).unwrap(),
            Unregistered {
                point: Point { x: 1.0, y: 2.0 }
            }
        );
    }

    #[test]
    fn serializes_typed_values() {
        let mut registry = TypeRegistry::new();
        registry.register::<Point>();
        let json = Json::serialize_reflect(&Point { x: 1.0, y: 2.0 }).unwrap();
        assert_eq!(json, Json(json!({ "x": 1.0, "y": 2.0 })));
        assert_eq!(
            json.deserialize::<Point>(&registry).unwrap(),
            Point { x: 1.0, y: 2.0 }
        );
    }

    #[test]
    fn round_trips_through_reflection() {
        let mut registry = TypeRegistry::new();
        registry.register::<Json>();
        registry.register::<Option<Json>>();
        let registration = registry
            .get(core::any::TypeId::of::<Option<Json>>())
            .unwrap();

        let value = json!({ "any": [1, "two", null] });
        let reflected = TypedReflectDeserializer::new(registration, &registry)
            .deserialize(&value)
            .unwrap();
        let parsed = <Option<Json>>::from_reflect(&*reflected).unwrap();
        assert_eq!(parsed, Some(Json(value)));

        let reflected = TypedReflectDeserializer::new(registration, &registry)
            .deserialize(&Value::Null)
            .unwrap();
        assert_eq!(<Option<Json>>::from_reflect(&*reflected).unwrap(), None);

        assert_eq!(Json::type_info().type_path(), "bevy_remote::json::Json");
    }
}
