// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Type registry
use crate::recording::transcoder::{ComRecTranscoderBuilder, RecordingTranscoder};
use serde::Serialize;
use std::collections::HashMap;

/// Registry of types used in the com layer
#[derive(Debug)]
pub struct TypeRegistry {
    // Map user-defined, human-readable type names to type information
    map: RegistryMap,
}

impl TypeRegistry {
    /// Create empty type registry
    pub fn new() -> Self {
        let map = HashMap::default();
        Self { map }
    }

    /// Helper method for adding a new registry entry
    fn add_helper(&mut self, type_info: TypeInfo) -> &mut Self {
        let type_name = type_info.type_name;
        let previous_info = self.map.insert(type_name, type_info);
        assert!(
            previous_info.is_none(),
            "type '{type_name}' already registered"
        );
        self
    }

    /// Add the given type to the registry
    ///
    /// The user may define a unique type name, otherwise the system type name will be used.
    /// Note that system type names may not be unique in which case the method will panic.
    ///
    /// # Panics
    ///
    /// This method will panic if
    /// - a type with identical type id (i.e. the same type) has already been registered
    /// - the explicitly or implicitly provided type name is not unique
    pub fn add<
        T: Serialize + postcard::experimental::max_size::MaxSize + std::fmt::Debug + 'static,
    >(
        &mut self,
        type_name: Option<&'static str>,
    ) -> &mut Self {
        let type_name = type_name.unwrap_or(core::any::type_name::<T>());
        let decser_builder =
            Box::new(|topic: &'static str| RecordingTranscoder::<T>::build(topic, type_name))
                as Box<dyn ComRecTranscoderBuilder>;
        let type_info = TypeInfo {
            type_name,
            comrec_builder: decser_builder,
        };
        self.add_helper(type_info)
    }

    /// Import the given type registry into this registry
    pub fn import(&mut self, other: TypeRegistry) -> &mut Self {
        for (_, type_info) in other.map {
            self.add_helper(type_info);
        }
        self
    }

    /// Retrieve a [`TypeInfo`] for the given type name, or None if not existent
    pub fn info_name(&self, type_name: &str) -> Option<&TypeInfo> {
        self.map.get(type_name)
    }
}

impl Default for TypeRegistry {
    fn default() -> Self {
        TypeRegistry::new()
    }
}

/// Type registry map, mapping from human-readable type names to required objects for each type
type RegistryMap = HashMap<&'static str, TypeInfo>;

/// Type information stored in the type registry
pub struct TypeInfo {
    // Human-readable type name
    pub type_name: &'static str,

    // Corresponding [`ComRecTranscoderBuilder`] object
    pub comrec_builder: Box<dyn ComRecTranscoderBuilder>,
}

impl std::fmt::Debug for TypeInfo {
    fn fmt(&self, writer: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writer.write_fmt(format_args!(
            "[ type_name: {:?}, decser_builder: Box(?) ]",
            self.type_name
        ))
    }
}

#[macro_export]
macro_rules! register_type {
    ($registry:ident, $type:ty: $name:expr) => {
        $registry.add::<$type>(Some($name))
    };
    ($registry:ident, $type:ty) => {
        $registry.add::<$type>(None)
    };
}

#[macro_export]
macro_rules! register_types {
    ($registry:ident, $($type:ty $(:$name:expr)?),+ $(,)?) => {$(register_type!($registry, $type $(:$name)?));+};
}

/////////////
// Tests
/////////////

#[test]
fn test_type_registry() {
    #[derive(Debug, serde::Serialize, postcard::experimental::max_size::MaxSize)]
    struct TestType1 {}

    #[derive(Debug, serde::Serialize, postcard::experimental::max_size::MaxSize)]
    struct TestType2 {}

    #[derive(Debug, serde::Serialize, postcard::experimental::max_size::MaxSize)]
    struct TestType3 {}

    let mut registry = TypeRegistry::default();
    register_types!(registry, TestType1, TestType2, TestType3: "my_test_type3_name");

    // test presence and data of entry for TestType1
    let type_name = core::any::type_name::<TestType1>();
    assert!(registry.map.contains_key(&type_name));
    assert!(registry.info_name(type_name).is_some());
    assert_eq!(registry.info_name(type_name).unwrap().type_name, type_name);

    // test presence and data of entry for TestType3
    let type_name = "my_test_type3_name";
    assert!(registry.map.contains_key(&type_name));
    assert!(registry.info_name(type_name).is_some());
    assert_eq!(registry.info_name(type_name).unwrap().type_name, type_name);

    // test missing type Foo
    struct Foo {}
    let type_name = core::any::type_name::<Foo>();
    assert!(!registry.map.contains_key(&type_name));
    assert!(registry.info_name(type_name).is_none());
}
