use crate::{
    attributes::{impl_custom_attribute_methods, CustomAttributes},
    ty::impl_type_methods,
    MaybeTyped, PartialReflect, Type, TypeInfo, TypePath,
};
use alloc::borrow::Cow;
use core::fmt::{Display, Formatter};

/// The named field of a reflected struct.
#[derive(Clone, Debug)]
pub struct NamedField {
    name: &'static str,
    type_info: fn() -> Option<&'static TypeInfo>,
    ty: Type,
    custom_attributes: CustomAttributes,
    has_default: bool,
    #[cfg(feature = "reflect_documentation")]
    docs: Option<&'static str>,
}

impl NamedField {
    /// Create a new [`NamedField`].
    pub fn new<T: PartialReflect + MaybeTyped + TypePath>(name: &'static str) -> Self {
        Self {
            name,
            type_info: T::maybe_type_info,
            ty: Type::of::<T>(),
            custom_attributes: CustomAttributes::default(),
            has_default: false,
            #[cfg(feature = "reflect_documentation")]
            docs: None,
        }
    }

    /// Sets whether the field has a `#[reflect(default)]` attribute.
    pub fn with_default(self, has_default: bool) -> Self {
        Self {
            has_default,
            ..self
        }
    }

    /// Sets the docstring for this field.
    #[cfg(feature = "reflect_documentation")]
    pub fn with_docs(self, docs: Option<&'static str>) -> Self {
        Self { docs, ..self }
    }

    /// Sets the custom attributes for this field.
    pub fn with_custom_attributes(self, custom_attributes: CustomAttributes) -> Self {
        Self {
            custom_attributes,
            ..self
        }
    }

    /// The name of the field.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Returns `true` if the field has a `#[reflect(default)]` attribute.
    pub fn has_default(&self) -> bool {
        self.has_default
    }

    /// The [`TypeInfo`] of the field.
    ///
    ///
    /// Returns `None` if the field does not contain static type information,
    /// such as for dynamic types.
    pub fn type_info(&self) -> Option<&'static TypeInfo> {
        (self.type_info)()
    }

    impl_type_methods!(ty);

    /// The docstring of this field, if any.
    #[cfg(feature = "reflect_documentation")]
    pub fn docs(&self) -> Option<&'static str> {
        self.docs
    }

    impl_custom_attribute_methods!(self.custom_attributes, "field");
}

/// The unnamed field of a reflected tuple or tuple struct.
#[derive(Clone, Debug)]
pub struct UnnamedField {
    index: usize,
    type_info: fn() -> Option<&'static TypeInfo>,
    ty: Type,
    custom_attributes: CustomAttributes,
    has_default: bool,
    #[cfg(feature = "reflect_documentation")]
    docs: Option<&'static str>,
}

impl UnnamedField {
    /// Create a new [`UnnamedField`].
    pub fn new<T: PartialReflect + MaybeTyped + TypePath>(index: usize) -> Self {
        Self {
            index,
            type_info: T::maybe_type_info,
            ty: Type::of::<T>(),
            custom_attributes: CustomAttributes::default(),
            has_default: false,
            #[cfg(feature = "reflect_documentation")]
            docs: None,
        }
    }

    /// Sets whether the field has a `#[reflect(default)]` attribute.
    pub fn with_default(self, has_default: bool) -> Self {
        Self {
            has_default,
            ..self
        }
    }

    /// Sets the docstring for this field.
    #[cfg(feature = "reflect_documentation")]
    pub fn with_docs(self, docs: Option<&'static str>) -> Self {
        Self { docs, ..self }
    }

    /// Sets the custom attributes for this field.
    pub fn with_custom_attributes(self, custom_attributes: CustomAttributes) -> Self {
        Self {
            custom_attributes,
            ..self
        }
    }

    /// Returns the index of the field.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Returns `true` if the field has a `#[reflect(default)]` attribute.
    pub fn has_default(&self) -> bool {
        self.has_default
    }

    /// The [`TypeInfo`] of the field.
    ///
    ///
    /// Returns `None` if the field does not contain static type information,
    /// such as for dynamic types.
    pub fn type_info(&self) -> Option<&'static TypeInfo> {
        (self.type_info)()
    }

    impl_type_methods!(ty);

    /// The docstring of this field, if any.
    #[cfg(feature = "reflect_documentation")]
    pub fn docs(&self) -> Option<&'static str> {
        self.docs
    }

    impl_custom_attribute_methods!(self.custom_attributes, "field");
}

/// A representation of a field's accessor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldId {
    /// Access a field by name.
    Named(Cow<'static, str>),
    /// Access a field by index.
    Unnamed(usize),
}

impl Display for FieldId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Named(name) => Display::fmt(name, f),
            Self::Unnamed(index) => Display::fmt(index, f),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Reflect, Typed};

    #[test]
    fn fields_report_reflect_default() {
        #[derive(Reflect)]
        struct Foo {
            required: u32,
            #[reflect(default)]
            defaulted: u32,
            #[reflect(default = "seven")]
            computed: u32,
        }

        fn seven() -> u32 {
            7
        }

        let info = Foo::type_info().as_struct().unwrap();
        assert!(!info.field("required").unwrap().has_default());
        assert!(info.field("defaulted").unwrap().has_default());
        assert!(info.field("computed").unwrap().has_default());
    }
}
