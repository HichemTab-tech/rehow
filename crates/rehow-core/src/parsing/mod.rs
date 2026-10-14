mod presentation;
mod reconcile;
mod scalar;
mod semantic;
mod source;

pub(crate) use reconcile::{
    CollectionPresentationStyle, CommentOwner, NodePresentation, ParsedGuideSource,
    ParsedYamlArtifact, ParsedYamlDocument, ReconciledEntryKind, RoundTripStatus,
};
#[cfg(test)]
pub(crate) use reconcile::{
    CommentPlacement, NodeId, NodeRole, ReconciledNodeKind, RoundTripIssue, RoundTripReason,
};
pub(crate) use source::{
    FileByteOffset, GuideEnvelope, ParseFailure, ParseFailureKind, SourceLineEnding, SourcePosition,
};
#[cfg(test)]
pub(crate) use source::{ParseLimitScope, YamlByteRange};

pub(crate) fn parse_yaml_document(source: Box<[u8]>) -> Result<ParsedYamlDocument, ParseFailure> {
    let envelope = source::scan_whole(&source)?;
    let yaml = source::slice(&source, envelope.yaml_file_range());
    let artifact = parse_artifact(
        yaml,
        envelope.yaml_file_range().start(),
        &source,
        envelope.yaml_file_range().start().get() as usize,
    )?;
    Ok(ParsedYamlDocument::new(source, envelope, artifact))
}

pub(crate) fn parse_guide_source(source: Box<[u8]>) -> Result<ParsedGuideSource, ParseFailure> {
    let envelope = source::scan_guide(&source)?;
    let yaml = source::slice(&source, envelope.yaml_file_range());
    let comment_region = source
        .get(..envelope.separator_line().end().get() as usize)
        .ok_or_else(|| ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant))?;
    let artifact = parse_artifact(
        yaml,
        envelope.yaml_file_range().start(),
        comment_region,
        envelope.yaml_file_range().start().get() as usize,
    )?;
    Ok(ParsedGuideSource::new(source, envelope, artifact))
}

/// Scan only the Guide envelope (delimiters, separator, byte policy).
pub(crate) fn scan_guide_envelope(source: &[u8]) -> Result<GuideEnvelope, ParseFailure> {
    source::scan_guide(source)
}

fn parse_artifact(
    yaml: &[u8],
    file_base: FileByteOffset,
    comment_region: &[u8],
    yaml_region_offset: usize,
) -> Result<ParsedYamlArtifact, ParseFailure> {
    let text = core::str::from_utf8(yaml)
        .map_err(|_| ParseFailure::plain(ParseFailureKind::InvalidUtf8))?;
    let presentation = match presentation::inspect(text, comment_region, yaml_region_offset) {
        Ok(value) => value,
        Err(error) => return Err(select_presentation_before_semantic(error, None)),
    };
    let semantic = semantic::parse(text, &presentation)?;
    reconcile::reconcile(semantic, presentation, yaml, file_base)
}

fn select_presentation_before_semantic(
    presentation: ParseFailure,
    _semantic: Option<ParseFailure>,
) -> ParseFailure {
    presentation
}

#[cfg(test)]
fn combined_stage_8_and_9_for_test(source: &str) -> ParseFailureKind {
    let presentation = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let stage_8 = presentation::shape_fault_after_dependency_for_test(source);
    let stage_9 = semantic::scan_fault_after_dependency_for_test(source, &presentation);
    select_presentation_before_semantic(stage_8, Some(stage_9)).kind()
}

#[cfg(test)]
mod tests;
