use core::fmt;

pub(super) const MAX_YAML_BYTES: usize = 256 * 1024;
pub(super) const MAX_GUIDE_BYTES: usize = 2 * 1024 * 1024;
const MAX_YAML_BYTES_U64: u64 = 256 * 1024;
const MAX_GUIDE_BYTES_U64: u64 = 2 * 1024 * 1024;
const FIRST_REFUSED_YAML_BYTES_USIZE: usize = MAX_YAML_BYTES + 1;
const FIRST_REFUSED_YAML_BYTES: u64 = MAX_YAML_BYTES_U64 + 1;
const FIRST_REFUSED_GUIDE_BYTES: u64 = MAX_GUIDE_BYTES_U64 + 1;
const BOM: &[u8; 3] = b"\xef\xbb\xbf";

macro_rules! value_debug {
    ($ty:ty, $name:literal) => {
        impl fmt::Debug for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str($name)
            }
        }
    };
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct FileByteOffset(u32);
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct FileByteRange {
    start: FileByteOffset,
    end: FileByteOffset,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct YamlByteOffset(u32);
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct YamlByteRange {
    start: YamlByteOffset,
    end: YamlByteOffset,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct SourcePosition {
    line: u32,
    column: u32,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum SourceLineEnding {
    None,
    Lf,
    CrLf,
}

value_debug!(FileByteOffset, "FileByteOffset");
value_debug!(FileByteRange, "FileByteRange");
value_debug!(YamlByteOffset, "YamlByteOffset");
value_debug!(YamlByteRange, "YamlByteRange");
value_debug!(SourcePosition, "SourcePosition");
value_debug!(SourceLineEnding, "SourceLineEnding");

impl FileByteOffset {
    pub(crate) fn get(self) -> u32 {
        self.0
    }
}
impl YamlByteOffset {
    pub(crate) fn get(self) -> u32 {
        self.0
    }
}
impl FileByteRange {
    pub(crate) fn start(self) -> FileByteOffset {
        self.start
    }
    pub(crate) fn end(self) -> FileByteOffset {
        self.end
    }
    pub(crate) fn len(self) -> u32 {
        self.end.0 - self.start.0
    }
    pub(crate) fn is_empty(self) -> bool {
        self.start == self.end
    }
}
impl YamlByteRange {
    pub(crate) fn start(self) -> YamlByteOffset {
        self.start
    }
    pub(crate) fn end(self) -> YamlByteOffset {
        self.end
    }
    pub(crate) fn len(self) -> u32 {
        self.end.0 - self.start.0
    }
    pub(crate) fn is_empty(self) -> bool {
        self.start == self.end
    }
}
impl SourcePosition {
    pub(crate) fn line(self) -> u32 {
        self.line
    }
    pub(crate) fn column(self) -> u32 {
        self.column
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct WholeYamlEnvelope {
    bom_present: bool,
    yaml_file_range: FileByteRange,
    line_ending: SourceLineEnding,
    final_newline: bool,
}
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct GuideEnvelope {
    bom_present: bool,
    opening_delimiter: FileByteRange,
    yaml_file_range: FileByteRange,
    closing_delimiter: FileByteRange,
    separator_line: FileByteRange,
    body_file_range: FileByteRange,
    yaml_line_ending: SourceLineEnding,
    final_newline: bool,
}
impl WholeYamlEnvelope {
    pub(crate) fn bom_present(&self) -> bool {
        self.bom_present
    }
    pub(crate) fn yaml_file_range(&self) -> FileByteRange {
        self.yaml_file_range
    }
    pub(crate) fn line_ending(&self) -> SourceLineEnding {
        self.line_ending
    }
    pub(crate) fn final_newline(&self) -> bool {
        self.final_newline
    }
}
impl GuideEnvelope {
    pub(crate) fn bom_present(&self) -> bool {
        self.bom_present
    }
    pub(crate) fn opening_delimiter(&self) -> FileByteRange {
        self.opening_delimiter
    }
    pub(crate) fn yaml_file_range(&self) -> FileByteRange {
        self.yaml_file_range
    }
    pub(crate) fn closing_delimiter(&self) -> FileByteRange {
        self.closing_delimiter
    }
    pub(crate) fn separator_line(&self) -> FileByteRange {
        self.separator_line
    }
    pub(crate) fn body_file_range(&self) -> FileByteRange {
        self.body_file_range
    }
    pub(crate) fn yaml_line_ending(&self) -> SourceLineEnding {
        self.yaml_line_ending
    }
    pub(crate) fn final_newline(&self) -> bool {
        self.final_newline
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum ParseLimitScope {
    WholeYamlBytes,
    GuideBytes,
    FrontmatterBytes,
    RawScalarBytes,
    DecodedScalarBytes,
    CanonicalScalarBytes,
    Depth,
    MappingEntries,
    SequenceEntries,
    Nodes,
    NumericWork,
    NumericDigits,
    SemanticEncodingBytes,
    SemanticComparisonBytes,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum ParseFailureKind {
    SourceTooLarge,
    FrontmatterTooLarge,
    InvalidUtf8,
    RawBomDenied,
    BareCarriageReturn,
    MixedYamlLineEndings,
    MissingGuideOpeningDelimiter,
    MissingGuideClosingDelimiter,
    MissingGuideSeparator,
    PresentationSyntax,
    PresentationCoverage,
    PresentationShape,
    DirectiveDenied,
    DocumentMarkerDenied,
    AnchorDenied,
    AliasDenied,
    ExplicitTagDenied,
    ExplicitTagKindMismatch,
    ExplicitTagValueMismatch,
    MergeKeyDenied,
    InvalidUnicodeScalar,
    SemanticSyntax,
    SemanticStreamOrder,
    MultipleDocuments,
    RootMustBeMapping,
    TopLevelKeyMustBeString,
    DepthLimit,
    MappingEntryLimit,
    SequenceEntryLimit,
    NodeLimit,
    RawScalarLimit,
    DecodedScalarLimit,
    CanonicalScalarLimit,
    NumericWorkLimit,
    NumericDigitLimit,
    SemanticEncodingLimit,
    SemanticComparisonLimit,
    DuplicateMappingKey,
    ParserTagDisagreement,
    MeterArithmeticOverflow,
    SemanticBuilderInvariant,
}
value_debug!(ParseLimitScope, "ParseLimitScope");
value_debug!(ParseFailureKind, "ParseFailureKind");

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ParseLimitFacts {
    scope: ParseLimitScope,
    limit: u64,
    observed_at_least: u64,
}
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ParseFailure {
    kind: ParseFailureKind,
    position: Option<SourcePosition>,
    limit: Option<ParseLimitFacts>,
}
value_debug!(ParseLimitFacts, "ParseLimitFacts");
value_debug!(ParseFailure, "ParseFailure");
impl ParseFailure {
    pub(crate) fn kind(&self) -> ParseFailureKind {
        self.kind
    }
    pub(crate) fn position(&self) -> Option<SourcePosition> {
        self.position
    }
    pub(crate) fn limit(&self) -> Option<&ParseLimitFacts> {
        self.limit.as_ref()
    }
    pub(super) fn plain(kind: ParseFailureKind) -> Self {
        Self {
            kind,
            position: None,
            limit: None,
        }
    }
    pub(super) fn at(kind: ParseFailureKind, position: Option<SourcePosition>) -> Self {
        Self {
            kind,
            position,
            limit: None,
        }
    }
    pub(super) fn limited(
        kind: ParseFailureKind,
        scope: ParseLimitScope,
        limit: u64,
        observed: u64,
    ) -> Self {
        Self {
            kind,
            position: None,
            limit: Some(ParseLimitFacts {
                scope,
                limit,
                observed_at_least: observed,
            }),
        }
    }
}
impl ParseLimitFacts {
    pub(crate) fn scope(&self) -> ParseLimitScope {
        self.scope
    }
    pub(crate) fn limit(&self) -> u64 {
        self.limit
    }
    pub(crate) fn observed_at_least(&self) -> u64 {
        self.observed_at_least
    }
}
impl fmt::Display for ParseFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Rehow YAML source")
    }
}
impl std::error::Error for ParseFailure {}

pub(super) fn file_range(start: usize, end: usize) -> Result<FileByteRange, ParseFailure> {
    Ok(FileByteRange {
        start: FileByteOffset(
            u32::try_from(start)
                .map_err(|_| ParseFailure::plain(ParseFailureKind::MeterArithmeticOverflow))?,
        ),
        end: FileByteOffset(
            u32::try_from(end)
                .map_err(|_| ParseFailure::plain(ParseFailureKind::MeterArithmeticOverflow))?,
        ),
    })
}
pub(super) fn yaml_range(start: usize, end: usize) -> Result<YamlByteRange, ParseFailure> {
    Ok(YamlByteRange {
        start: YamlByteOffset(
            u32::try_from(start)
                .map_err(|_| ParseFailure::plain(ParseFailureKind::MeterArithmeticOverflow))?,
        ),
        end: YamlByteOffset(
            u32::try_from(end)
                .map_err(|_| ParseFailure::plain(ParseFailureKind::MeterArithmeticOverflow))?,
        ),
    })
}
pub(super) fn yaml_offset(value: usize) -> Result<YamlByteOffset, ParseFailure> {
    Ok(YamlByteOffset(u32::try_from(value).map_err(|_| {
        ParseFailure::plain(ParseFailureKind::MeterArithmeticOverflow)
    })?))
}
pub(super) fn file_offset(value: u32) -> FileByteOffset {
    FileByteOffset(value)
}
pub(super) fn rebase(range: YamlByteRange, base: FileByteOffset) -> Option<FileByteRange> {
    Some(FileByteRange {
        start: FileByteOffset(base.0.checked_add(range.start.0)?),
        end: FileByteOffset(base.0.checked_add(range.end.0)?),
    })
}
pub(super) fn position(text: &str, offset: usize) -> Option<SourcePosition> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return None;
    }
    let mut line = 1u32;
    let mut column = 1u32;
    let mut index = 0;
    for ch in text.chars() {
        if index >= offset {
            break;
        }
        index += ch.len_utf8();
        if ch == '\n' {
            line = line.checked_add(1)?;
            column = 1;
        } else if ch != '\r' {
            column = column.checked_add(1)?;
        }
    }
    Some(SourcePosition { line, column })
}

#[derive(Clone, Copy)]
struct BytePolicyFacts {
    raw_bom: bool,
    bare_cr: bool,
    saw_lf: bool,
    saw_crlf: bool,
}

impl BytePolicyFacts {
    fn line_ending(self) -> SourceLineEnding {
        if self.saw_crlf {
            SourceLineEnding::CrLf
        } else if self.saw_lf {
            SourceLineEnding::Lf
        } else {
            SourceLineEnding::None
        }
    }

    fn failure(self) -> Option<ParseFailure> {
        if self.raw_bom {
            Some(ParseFailure::plain(ParseFailureKind::RawBomDenied))
        } else if self.bare_cr {
            Some(ParseFailure::plain(ParseFailureKind::BareCarriageReturn))
        } else if self.saw_lf && self.saw_crlf {
            Some(ParseFailure::plain(ParseFailureKind::MixedYamlLineEndings))
        } else {
            None
        }
    }
}

/// Collect every stage-three byte-policy fact without allocating or returning
/// early. `skip_initial_bom` excludes only the one optional byte-zero BOM.
fn collect_byte_policy(bytes: &[u8], skip_initial_bom: bool) -> BytePolicyFacts {
    let start = usize::from(skip_initial_bom) * BOM.len();
    let mut facts = BytePolicyFacts {
        raw_bom: false,
        bare_cr: false,
        saw_lf: false,
        saw_crlf: false,
    };
    let mut index = start.min(bytes.len());
    while index < bytes.len() {
        let bom_end = index.checked_add(BOM.len()).unwrap_or(bytes.len());
        if bytes.get(index..bom_end) == Some(BOM.as_slice()) {
            facts.raw_bom = true;
        }
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                facts.saw_crlf = true;
                index += 2;
            }
            b'\r' => {
                facts.bare_cr = true;
                index += 1;
            }
            b'\n' => {
                facts.saw_lf = true;
                index += 1;
            }
            _ => index += 1,
        }
    }
    facts
}

pub(super) fn scan_whole(source: &[u8]) -> Result<WholeYamlEnvelope, ParseFailure> {
    if source.len() > MAX_YAML_BYTES {
        return Err(ParseFailure::limited(
            ParseFailureKind::SourceTooLarge,
            ParseLimitScope::WholeYamlBytes,
            MAX_YAML_BYTES_U64,
            FIRST_REFUSED_YAML_BYTES,
        ));
    }
    let bom_present = source.starts_with(BOM);
    let start: usize = if bom_present { 3 } else { 0 };
    core::str::from_utf8(source).map_err(|_| ParseFailure::plain(ParseFailureKind::InvalidUtf8))?;
    let facts = collect_byte_policy(source, bom_present);
    if let Some(failure) = facts.failure() {
        return Err(failure);
    }
    Ok(WholeYamlEnvelope {
        bom_present,
        yaml_file_range: file_range(start, source.len())?,
        line_ending: facts.line_ending(),
        final_newline: source.ends_with(b"\n"),
    })
}

pub(super) fn scan_guide(source: &[u8]) -> Result<GuideEnvelope, ParseFailure> {
    if source.len() > MAX_GUIDE_BYTES {
        return Err(ParseFailure::limited(
            ParseFailureKind::SourceTooLarge,
            ParseLimitScope::GuideBytes,
            MAX_GUIDE_BYTES_U64,
            FIRST_REFUSED_GUIDE_BYTES,
        ));
    }
    core::str::from_utf8(source).map_err(|_| ParseFailure::plain(ParseFailureKind::InvalidUtf8))?;
    let bom_present = source.starts_with(BOM);
    let start: usize = if bom_present { 3 } else { 0 };
    let arithmetic = || ParseFailure::plain(ParseFailureKind::MeterArithmeticOverflow);
    let crlf_open_end = start.checked_add(5).ok_or_else(arithmetic)?;
    let lf_open_end = start.checked_add(4).ok_or_else(arithmetic)?;
    let opening = if source.get(start..crlf_open_end) == Some(b"---\r\n") {
        Some((b"\r\n".as_slice(), SourceLineEnding::CrLf, crlf_open_end))
    } else if source.get(start..lf_open_end) == Some(b"---\n") {
        Some((b"\n".as_slice(), SourceLineEnding::Lf, lf_open_end))
    } else {
        None
    };

    // One byte beyond the accepted region is enough to prove the exact first
    // refused frontmatter size. Nothing later can be classified as body until
    // a complete separator has been proved.
    let probe_end = source.len().min(FIRST_REFUSED_YAML_BYTES_USIZE);
    let mut close = None;
    if let Some((_, _, opening_end)) = opening {
        let mut cursor = opening_end;
        while cursor < probe_end {
            let relative_lf = source[cursor..probe_end]
                .iter()
                .position(|byte| *byte == b'\n');
            let line_end = relative_lf.map_or(probe_end, |offset| cursor + offset + 1);
            let content_end = if relative_lf.is_some() {
                if line_end >= cursor + 2 && source[line_end - 2] == b'\r' {
                    line_end - 2
                } else {
                    line_end - 1
                }
            } else {
                line_end
            };
            if cursor.checked_add(3) == Some(content_end)
                && source.get(cursor..content_end) == Some(b"---")
            {
                close = Some((cursor, content_end, line_end, relative_lf.is_some()));
                break;
            }
            if relative_lf.is_none() {
                break;
            }
            cursor = line_end;
        }
    }

    // The separator is the optional blank line after the closing delimiter.
    // Hand-written Guides may start their body directly after `---` or end at
    // the closing delimiter; both are represented by an empty separator range
    // so later edits preserve the original layout byte for byte.
    let valid_separator_end = match (opening, close) {
        (Some((newline, _, _)), Some((_, content_end, close_line_end, true)))
            if source.get(content_end..close_line_end) == Some(newline) =>
        {
            Some(
                close_line_end
                    .checked_add(newline.len())
                    .filter(|end| source.get(close_line_end..*end) == Some(newline))
                    .unwrap_or(close_line_end),
            )
        }
        (Some(_), Some((_, content_end, _, false))) if content_end == source.len() => {
            Some(content_end)
        }
        _ => None,
    };
    let policy_end = valid_separator_end.unwrap_or(probe_end).min(source.len());
    let facts = collect_byte_policy(&source[..policy_end], bom_present);
    if let Some(failure) = facts.failure() {
        return Err(failure);
    }

    let Some((_newline, style, opening_end)) = opening else {
        return Err(ParseFailure::plain(
            ParseFailureKind::MissingGuideOpeningDelimiter,
        ));
    };
    let Some((close_start, _close_content_end, close_line_end, _)) = close else {
        if source.len() > MAX_YAML_BYTES {
            return Err(ParseFailure::limited(
                ParseFailureKind::FrontmatterTooLarge,
                ParseLimitScope::FrontmatterBytes,
                MAX_YAML_BYTES_U64,
                FIRST_REFUSED_YAML_BYTES,
            ));
        }
        return Err(ParseFailure::plain(
            ParseFailureKind::MissingGuideClosingDelimiter,
        ));
    };
    let Some(separator_end) = valid_separator_end else {
        if close_line_end > MAX_YAML_BYTES {
            return Err(ParseFailure::limited(
                ParseFailureKind::FrontmatterTooLarge,
                ParseLimitScope::FrontmatterBytes,
                MAX_YAML_BYTES_U64,
                FIRST_REFUSED_YAML_BYTES,
            ));
        }
        return Err(ParseFailure::plain(ParseFailureKind::MissingGuideSeparator));
    };
    if separator_end > MAX_YAML_BYTES {
        return Err(ParseFailure::limited(
            ParseFailureKind::FrontmatterTooLarge,
            ParseLimitScope::FrontmatterBytes,
            MAX_YAML_BYTES_U64,
            FIRST_REFUSED_YAML_BYTES,
        ));
    }
    Ok(GuideEnvelope {
        bom_present,
        opening_delimiter: file_range(start, opening_end)?,
        yaml_file_range: file_range(opening_end, close_start)?,
        closing_delimiter: file_range(close_start, close_line_end)?,
        separator_line: file_range(close_line_end, separator_end)?,
        body_file_range: file_range(separator_end, source.len())?,
        yaml_line_ending: style,
        final_newline: source.ends_with(b"\n"),
    })
}

pub(super) fn slice(source: &[u8], range: FileByteRange) -> &[u8] {
    &source[range.start.0 as usize..range.end.0 as usize]
}
