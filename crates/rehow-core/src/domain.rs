mod yaml_value;

pub use yaml_value::{
    CanonicalYamlFloat, CanonicalYamlFloatKind, CanonicalYamlInteger, UnknownMetadataDocument,
    UnknownMetadataEntry, UnknownYamlBuildError, UnknownYamlBuildErrorKind,
    UnknownYamlDocumentBuilder, UnknownYamlEvent, UnknownYamlMappingEntry, UnknownYamlScalar,
    UnknownYamlScalarKind, UnknownYamlValue, UnknownYamlValueKind,
};

#[derive(Clone, Copy)]
pub struct ValueConstructionError;

impl core::fmt::Debug for ValueConstructionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ValueConstructionError")
    }
}

impl core::fmt::Display for ValueConstructionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("invalid canonical value")
    }
}

impl std::error::Error for ValueConstructionError {}
