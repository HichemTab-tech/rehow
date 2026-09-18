pub(crate) mod content;
mod yaml_value;

pub use content::{
    Applicability, ArchiveMetadata, Attachment, Category, CategoryStatus, Guide, GuideReference,
    GuideStatus, RestorableGuideStatus, VerificationMetadata, WorkspaceManifest, WorkspaceSettings,
};

pub use yaml_value::{
    CanonicalYamlFloat, CanonicalYamlFloatKind, CanonicalYamlInteger, UnknownMetadataDocument,
    UnknownMetadataEntry, UnknownYamlBuildError, UnknownYamlBuildErrorKind,
    UnknownYamlDocumentBuilder, UnknownYamlEvent, UnknownYamlMappingEntry, UnknownYamlScalar,
    UnknownYamlScalarKind, UnknownYamlValue, UnknownYamlValueKind,
};

#[cfg(test)]
pub(crate) use yaml_value::UnknownYamlTestMeter;

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
