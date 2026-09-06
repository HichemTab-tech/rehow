use super::ValueConstructionError;

const MAX_DEPTH: usize = 16;
const MAX_NODES: usize = 20_000;
const MAX_MAPPING_ENTRIES: usize = 256;
const MAX_SEQUENCE_ENTRIES: usize = 2_048;
const MAX_SCALAR_BYTES: usize = 65_536;
const MAX_NUMERIC_DIGITS: usize = 81_920;
const MAX_ENCODING_BYTES: usize = 8_388_608;
const MAX_COMPARISON_UNITS: usize = 33_554_432;

#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalYamlInteger {
    negative: bool,
    digits: String,
}

impl CanonicalYamlInteger {
    pub fn new(negative: bool, digits: String) -> Result<Self, ValueConstructionError> {
        if digits.is_empty()
            || digits.len() > MAX_SCALAR_BYTES
            || !digits.bytes().all(|byte| byte.is_ascii_digit())
            || (digits.len() > 1 && digits.starts_with('0'))
        {
            return Err(ValueConstructionError);
        }
        Ok(Self {
            negative: negative && digits != "0",
            digits,
        })
    }

    pub fn is_negative(&self) -> bool {
        self.negative
    }
    pub fn digits(&self) -> &str {
        &self.digits
    }
}

#[derive(Clone, Eq, PartialEq)]
enum FloatRepresentation {
    Finite {
        negative: bool,
        coefficient: String,
        exponent: CanonicalYamlInteger,
    },
    PositiveInfinity,
    NegativeInfinity,
    NaN,
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum CanonicalYamlFloatKind {
    Finite,
    PositiveInfinity,
    NegativeInfinity,
    NaN,
}

#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalYamlFloat {
    representation: FloatRepresentation,
}

impl CanonicalYamlFloat {
    pub fn finite(
        negative: bool,
        coefficient_digits: String,
        exponent10: CanonicalYamlInteger,
    ) -> Result<Self, ValueConstructionError> {
        let canonical = !coefficient_digits.is_empty()
            && coefficient_digits.bytes().all(|byte| byte.is_ascii_digit())
            && (coefficient_digits == "0"
                || (!coefficient_digits.starts_with('0') && !coefficient_digits.ends_with('0')));
        let total = coefficient_digits
            .len()
            .checked_add(exponent10.digits.len())
            .ok_or(ValueConstructionError)?;
        if !canonical
            || total > MAX_SCALAR_BYTES
            || (coefficient_digits == "0" && (exponent10.negative || exponent10.digits != "0"))
        {
            return Err(ValueConstructionError);
        }
        Ok(Self {
            representation: FloatRepresentation::Finite {
                negative,
                coefficient: coefficient_digits,
                exponent: exponent10,
            },
        })
    }
    pub fn positive_infinity() -> Self {
        Self {
            representation: FloatRepresentation::PositiveInfinity,
        }
    }
    pub fn negative_infinity() -> Self {
        Self {
            representation: FloatRepresentation::NegativeInfinity,
        }
    }
    pub fn nan() -> Self {
        Self {
            representation: FloatRepresentation::NaN,
        }
    }
    pub fn kind(&self) -> CanonicalYamlFloatKind {
        match self.representation {
            FloatRepresentation::Finite { .. } => CanonicalYamlFloatKind::Finite,
            FloatRepresentation::PositiveInfinity => CanonicalYamlFloatKind::PositiveInfinity,
            FloatRepresentation::NegativeInfinity => CanonicalYamlFloatKind::NegativeInfinity,
            FloatRepresentation::NaN => CanonicalYamlFloatKind::NaN,
        }
    }
    pub fn is_negative(&self) -> Option<bool> {
        match &self.representation {
            FloatRepresentation::Finite { negative, .. } => Some(*negative),
            FloatRepresentation::PositiveInfinity => Some(false),
            FloatRepresentation::NegativeInfinity => Some(true),
            FloatRepresentation::NaN => None,
        }
    }
    pub fn coefficient_digits(&self) -> Option<&str> {
        match &self.representation {
            FloatRepresentation::Finite { coefficient, .. } => Some(coefficient),
            _ => None,
        }
    }
    pub fn exponent10(&self) -> Option<&CanonicalYamlInteger> {
        match &self.representation {
            FloatRepresentation::Finite { exponent, .. } => Some(exponent),
            _ => None,
        }
    }
}

pub enum UnknownYamlEvent {
    BeginSequence,
    EndSequence,
    BeginMapping,
    EndMapping,
    Null,
    Boolean(bool),
    Integer(CanonicalYamlInteger),
    Float(CanonicalYamlFloat),
    String(String),
}

#[derive(Clone)]
enum ValueRepresentation {
    Scalar(UnknownYamlScalar),
    Sequence(Vec<UnknownYamlValue>),
    Mapping {
        entries: Vec<UnknownYamlMappingEntry>,
        semantic_order: Vec<usize>,
    },
}
#[derive(Clone)]
pub struct UnknownYamlValue {
    representation: ValueRepresentation,
    key_encoding: Vec<u8>,
}
#[derive(Clone)]
enum ScalarRepresentation {
    Null,
    Boolean(bool),
    Integer(CanonicalYamlInteger),
    Float(CanonicalYamlFloat),
    String(String),
}
#[derive(Clone)]
pub struct UnknownYamlScalar {
    representation: ScalarRepresentation,
}
#[derive(Clone)]
pub struct UnknownYamlMappingEntry {
    key: UnknownYamlValue,
    value: UnknownYamlValue,
}
#[derive(Clone)]
pub struct UnknownMetadataEntry {
    key: String,
    value: UnknownYamlValue,
}
#[derive(Clone)]
pub struct UnknownMetadataDocument {
    entries: Vec<UnknownMetadataEntry>,
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum UnknownYamlValueKind {
    Scalar,
    Sequence,
    Mapping,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum UnknownYamlScalarKind {
    Null,
    Boolean,
    Integer,
    Float,
    String,
}

impl UnknownMetadataDocument {
    pub fn entries(&self) -> &[UnknownMetadataEntry] {
        &self.entries
    }
}
impl UnknownMetadataEntry {
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn value(&self) -> &UnknownYamlValue {
        &self.value
    }
}
impl UnknownYamlMappingEntry {
    pub fn key(&self) -> &UnknownYamlValue {
        &self.key
    }
    pub fn value(&self) -> &UnknownYamlValue {
        &self.value
    }
}
impl UnknownYamlValue {
    pub fn kind(&self) -> UnknownYamlValueKind {
        match self.representation {
            ValueRepresentation::Scalar(_) => UnknownYamlValueKind::Scalar,
            ValueRepresentation::Sequence(_) => UnknownYamlValueKind::Sequence,
            ValueRepresentation::Mapping { .. } => UnknownYamlValueKind::Mapping,
        }
    }
    pub fn as_scalar(&self) -> Option<&UnknownYamlScalar> {
        if let ValueRepresentation::Scalar(value) = &self.representation {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_sequence(&self) -> Option<&[UnknownYamlValue]> {
        if let ValueRepresentation::Sequence(value) = &self.representation {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_mapping(&self) -> Option<&[UnknownYamlMappingEntry]> {
        if let ValueRepresentation::Mapping { entries, .. } = &self.representation {
            Some(entries)
        } else {
            None
        }
    }
}
impl UnknownYamlScalar {
    pub fn kind(&self) -> UnknownYamlScalarKind {
        match self.representation {
            ScalarRepresentation::Null => UnknownYamlScalarKind::Null,
            ScalarRepresentation::Boolean(_) => UnknownYamlScalarKind::Boolean,
            ScalarRepresentation::Integer(_) => UnknownYamlScalarKind::Integer,
            ScalarRepresentation::Float(_) => UnknownYamlScalarKind::Float,
            ScalarRepresentation::String(_) => UnknownYamlScalarKind::String,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let ScalarRepresentation::Boolean(value) = self.representation {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_integer(&self) -> Option<&CanonicalYamlInteger> {
        if let ScalarRepresentation::Integer(value) = &self.representation {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_float(&self) -> Option<&CanonicalYamlFloat> {
        if let ScalarRepresentation::Float(value) = &self.representation {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let ScalarRepresentation::String(value) = &self.representation {
            Some(value)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum UnknownYamlBuildErrorKind {
    Poisoned,
    RootMustBeMapping,
    MultipleRoots,
    UnexpectedEnd,
    MismatchedEnd,
    IncompleteMappingEntry,
    TopLevelKeyMustBeString,
    DuplicateMappingKey,
    InvalidString,
    DepthLimit,
    MappingEntryLimit,
    SequenceEntryLimit,
    NodeLimit,
    ScalarLimit,
    NumericDigitLimit,
    SemanticEncodingLimit,
    SemanticComparisonLimit,
    ArithmeticOverflow,
    IncompleteDocument,
}
pub struct UnknownYamlBuildError {
    kind: UnknownYamlBuildErrorKind,
}
impl UnknownYamlBuildError {
    pub fn kind(&self) -> UnknownYamlBuildErrorKind {
        self.kind
    }
}
impl core::fmt::Debug for UnknownYamlBuildError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("UnknownYamlBuildError")
    }
}
impl core::fmt::Display for UnknownYamlBuildError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("invalid unknown YAML event sequence")
    }
}
impl std::error::Error for UnknownYamlBuildError {}

enum Frame {
    Sequence(Vec<UnknownYamlValue>),
    Mapping {
        entries: Vec<UnknownYamlMappingEntry>,
        pending_key: Option<UnknownYamlValue>,
    },
}
enum BuilderState {
    Active {
        frames: Vec<Frame>,
        root: Option<UnknownYamlValue>,
        nodes: usize,
        numeric_digits: usize,
        encoding_bytes: usize,
        comparison_units: usize,
    },
    Poisoned,
}
pub struct UnknownYamlDocumentBuilder {
    state: BuilderState,
}

impl UnknownYamlDocumentBuilder {
    #[expect(
        clippy::new_without_default,
        reason = "the accepted API forbids Default"
    )]
    pub fn new() -> Self {
        Self {
            state: BuilderState::Active {
                frames: Vec::new(),
                root: None,
                nodes: 0,
                numeric_digits: 0,
                encoding_bytes: 0,
                comparison_units: 0,
            },
        }
    }
    pub fn push_event(&mut self, event: UnknownYamlEvent) -> Result<(), UnknownYamlBuildError> {
        let result = self.push_inner(event);
        if let Err(kind) = result {
            self.state = BuilderState::Poisoned;
            return Err(UnknownYamlBuildError { kind });
        }
        Ok(())
    }
    fn push_inner(&mut self, event: UnknownYamlEvent) -> Result<(), UnknownYamlBuildErrorKind> {
        let BuilderState::Active {
            frames,
            root,
            nodes,
            numeric_digits,
            encoding_bytes,
            comparison_units,
        } = &mut self.state
        else {
            return Err(UnknownYamlBuildErrorKind::Poisoned);
        };
        if root.is_some() && frames.is_empty() {
            return Err(UnknownYamlBuildErrorKind::MultipleRoots);
        }
        if frames.is_empty() && root.is_none() && !matches!(event, UnknownYamlEvent::BeginMapping) {
            return Err(
                if matches!(
                    event,
                    UnknownYamlEvent::EndMapping | UnknownYamlEvent::EndSequence
                ) {
                    UnknownYamlBuildErrorKind::UnexpectedEnd
                } else {
                    UnknownYamlBuildErrorKind::RootMustBeMapping
                },
            );
        }
        match event {
            UnknownYamlEvent::EndSequence => close_frame(
                frames,
                root,
                nodes,
                numeric_digits,
                encoding_bytes,
                comparison_units,
                false,
            ),
            UnknownYamlEvent::EndMapping => close_frame(
                frames,
                root,
                nodes,
                numeric_digits,
                encoding_bytes,
                comparison_units,
                true,
            ),
            UnknownYamlEvent::BeginSequence | UnknownYamlEvent::BeginMapping => {
                if matches!(
                    frames.last(),
                    Some(Frame::Mapping {
                        pending_key: None,
                        ..
                    }) if frames.len() == 1
                ) {
                    return Err(UnknownYamlBuildErrorKind::TopLevelKeyMustBeString);
                }
                if frames.len() >= MAX_DEPTH {
                    return Err(UnknownYamlBuildErrorKind::DepthLimit);
                }
                charge(nodes, 1, MAX_NODES, UnknownYamlBuildErrorKind::NodeLimit)?;
                frames.push(if matches!(event, UnknownYamlEvent::BeginMapping) {
                    Frame::Mapping {
                        entries: Vec::new(),
                        pending_key: None,
                    }
                } else {
                    Frame::Sequence(Vec::new())
                });
                Ok(())
            }
            other => {
                if matches!(
                    frames.last(),
                    Some(Frame::Mapping {
                        pending_key: None,
                        ..
                    })
                ) && frames.len() == 1
                    && !matches!(&other, UnknownYamlEvent::String(_))
                {
                    return Err(UnknownYamlBuildErrorKind::TopLevelKeyMustBeString);
                }
                let (value, scalar_len, digits) = scalar_value(other)?;
                if scalar_len > MAX_SCALAR_BYTES {
                    return Err(UnknownYamlBuildErrorKind::ScalarLimit);
                }
                charge(nodes, 1, MAX_NODES, UnknownYamlBuildErrorKind::NodeLimit)?;
                check_attachment(frames, &value)?;
                charge(
                    numeric_digits,
                    digits,
                    MAX_NUMERIC_DIGITS,
                    UnknownYamlBuildErrorKind::NumericDigitLimit,
                )?;
                attach_value(frames, root, value, encoding_bytes)
            }
        }
    }
    pub fn finish_document(mut self) -> Result<UnknownMetadataDocument, UnknownYamlBuildError> {
        let state = core::mem::replace(&mut self.state, BuilderState::Poisoned);
        match state {
            BuilderState::Poisoned => Err(UnknownYamlBuildError {
                kind: UnknownYamlBuildErrorKind::Poisoned,
            }),
            BuilderState::Active { frames, .. } if !frames.is_empty() => {
                let kind = if matches!(
                    frames.last(),
                    Some(Frame::Mapping {
                        pending_key: Some(_),
                        ..
                    })
                ) {
                    UnknownYamlBuildErrorKind::IncompleteMappingEntry
                } else {
                    UnknownYamlBuildErrorKind::IncompleteDocument
                };
                Err(UnknownYamlBuildError { kind })
            }
            BuilderState::Active {
                root:
                    Some(UnknownYamlValue {
                        representation: ValueRepresentation::Mapping { entries, .. },
                        ..
                    }),
                ..
            } => {
                let mut output = Vec::with_capacity(entries.len());
                for entry in entries {
                    let ScalarRepresentation::String(key) = entry
                        .key
                        .representation
                        .into_scalar()
                        .expect("validated root key")
                        .representation
                    else {
                        unreachable!()
                    };
                    output.push(UnknownMetadataEntry {
                        key,
                        value: entry.value,
                    });
                }
                Ok(UnknownMetadataDocument { entries: output })
            }
            BuilderState::Active { .. } => Err(UnknownYamlBuildError {
                kind: UnknownYamlBuildErrorKind::IncompleteDocument,
            }),
        }
    }
    #[cfg(test)]
    fn seed_meters(&mut self, nodes: usize, encoding_bytes: usize, comparison_units: usize) {
        if let BuilderState::Active {
            nodes: n,
            encoding_bytes: e,
            comparison_units: c,
            ..
        } = &mut self.state
        {
            *n = nodes;
            *e = encoding_bytes;
            *c = comparison_units;
        }
    }
    #[cfg(test)]
    fn meters(&self) -> (usize, usize, usize, usize) {
        match &self.state {
            BuilderState::Active {
                nodes,
                numeric_digits,
                encoding_bytes,
                comparison_units,
                ..
            } => (*nodes, *numeric_digits, *encoding_bytes, *comparison_units),
            BuilderState::Poisoned => panic!("poisoned builder has no live meters"),
        }
    }
}

impl ValueRepresentation {
    fn into_scalar(self) -> Option<UnknownYamlScalar> {
        if let Self::Scalar(v) = self {
            Some(v)
        } else {
            None
        }
    }
}
fn scalar_value(
    event: UnknownYamlEvent,
) -> Result<(UnknownYamlValue, usize, usize), UnknownYamlBuildErrorKind> {
    let scalar = match event {
        UnknownYamlEvent::Null => ScalarRepresentation::Null,
        UnknownYamlEvent::Boolean(v) => ScalarRepresentation::Boolean(v),
        UnknownYamlEvent::Integer(v) => {
            let len = v.digits.len();
            return Ok((
                UnknownYamlValue {
                    representation: ValueRepresentation::Scalar(UnknownYamlScalar {
                        representation: ScalarRepresentation::Integer(v),
                    }),
                    key_encoding: Vec::new(),
                },
                len,
                len,
            ));
        }
        UnknownYamlEvent::Float(v) => {
            let len = v.coefficient_digits().map_or(0, str::len)
                + v.exponent10().map_or(0, |e| e.digits.len());
            return Ok((
                UnknownYamlValue {
                    representation: ValueRepresentation::Scalar(UnknownYamlScalar {
                        representation: ScalarRepresentation::Float(v),
                    }),
                    key_encoding: Vec::new(),
                },
                len,
                len,
            ));
        }
        UnknownYamlEvent::String(v) => {
            if v.chars().any(is_noncharacter) {
                return Err(UnknownYamlBuildErrorKind::InvalidString);
            }
            let len = v.len();
            return Ok((
                UnknownYamlValue {
                    representation: ValueRepresentation::Scalar(UnknownYamlScalar {
                        representation: ScalarRepresentation::String(v),
                    }),
                    key_encoding: Vec::new(),
                },
                len,
                0,
            ));
        }
        _ => unreachable!(),
    };
    Ok((
        UnknownYamlValue {
            representation: ValueRepresentation::Scalar(UnknownYamlScalar {
                representation: scalar,
            }),
            key_encoding: Vec::new(),
        },
        1,
        0,
    ))
}
fn charge(
    value: &mut usize,
    amount: usize,
    max: usize,
    kind: UnknownYamlBuildErrorKind,
) -> Result<(), UnknownYamlBuildErrorKind> {
    let next = value
        .checked_add(amount)
        .ok_or(UnknownYamlBuildErrorKind::ArithmeticOverflow)?;
    if next > max {
        return Err(kind);
    }
    *value = next;
    Ok(())
}
fn attach_value(
    frames: &mut [Frame],
    root: &mut Option<UnknownYamlValue>,
    mut value: UnknownYamlValue,
    encoding: &mut usize,
) -> Result<(), UnknownYamlBuildErrorKind> {
    check_attachment(frames, &value)?;
    if value.key_encoding.is_empty() {
        value.key_encoding = encode_value(&value.representation, encoding)?;
    }
    let Some(frame) = frames.last_mut() else {
        *root = Some(value);
        return Ok(());
    };
    match frame {
        Frame::Sequence(values) => {
            values.push(value);
            Ok(())
        }
        Frame::Mapping {
            entries,
            pending_key,
        } => {
            if pending_key.is_none() {
                *pending_key = Some(value);
                Ok(())
            } else {
                let key = pending_key.take().expect("checked");
                entries.push(UnknownYamlMappingEntry { key, value });
                Ok(())
            }
        }
    }
}
fn check_attachment(
    frames: &[Frame],
    value: &UnknownYamlValue,
) -> Result<(), UnknownYamlBuildErrorKind> {
    let at_root = frames.len() == 1;
    match frames.last() {
        Some(Frame::Sequence(values)) if values.len() >= MAX_SEQUENCE_ENTRIES => {
            Err(UnknownYamlBuildErrorKind::SequenceEntryLimit)
        }
        Some(Frame::Mapping {
            entries,
            pending_key: Some(_),
        }) if entries.len() >= MAX_MAPPING_ENTRIES => {
            Err(UnknownYamlBuildErrorKind::MappingEntryLimit)
        }
        Some(Frame::Mapping {
            pending_key: None, ..
        }) if at_root
            && !matches!(
                value.representation,
                ValueRepresentation::Scalar(UnknownYamlScalar {
                    representation: ScalarRepresentation::String(_)
                })
            ) =>
        {
            Err(UnknownYamlBuildErrorKind::TopLevelKeyMustBeString)
        }
        _ => Ok(()),
    }
}
fn close_frame(
    frames: &mut Vec<Frame>,
    root: &mut Option<UnknownYamlValue>,
    _nodes: &mut usize,
    _digits: &mut usize,
    encoding: &mut usize,
    comparisons: &mut usize,
    mapping: bool,
) -> Result<(), UnknownYamlBuildErrorKind> {
    let Some(frame) = frames.last() else {
        return Err(UnknownYamlBuildErrorKind::UnexpectedEnd);
    };
    if mapping != matches!(frame, Frame::Mapping { .. }) {
        return Err(UnknownYamlBuildErrorKind::MismatchedEnd);
    }
    if matches!(
        frame,
        Frame::Mapping {
            pending_key: Some(_),
            ..
        }
    ) {
        return Err(UnknownYamlBuildErrorKind::IncompleteMappingEntry);
    }
    if frames.len() > 1 {
        match &frames[frames.len() - 2] {
            Frame::Sequence(values) if values.len() >= MAX_SEQUENCE_ENTRIES => {
                return Err(UnknownYamlBuildErrorKind::SequenceEntryLimit);
            }
            Frame::Mapping {
                entries,
                pending_key: Some(_),
            } if entries.len() >= MAX_MAPPING_ENTRIES => {
                return Err(UnknownYamlBuildErrorKind::MappingEntryLimit);
            }
            _ => {}
        }
    }
    let frame = frames.pop().expect("validated nonempty frame stack");
    let value = match frame {
        Frame::Sequence(v) => UnknownYamlValue {
            representation: ValueRepresentation::Sequence(v),
            key_encoding: Vec::new(),
        },
        Frame::Mapping {
            entries,
            pending_key,
        } => {
            debug_assert!(pending_key.is_none());
            let semantic_order = validate_mapping(&entries, comparisons)?;
            UnknownYamlValue {
                representation: ValueRepresentation::Mapping {
                    entries,
                    semantic_order,
                },
                key_encoding: Vec::new(),
            }
        }
    };
    attach_value(frames, root, value, encoding)
}
fn append(
    out: &mut Vec<u8>,
    bytes: &[u8],
    meter: &mut usize,
) -> Result<(), UnknownYamlBuildErrorKind> {
    charge(
        meter,
        bytes.len(),
        MAX_ENCODING_BYTES,
        UnknownYamlBuildErrorKind::SemanticEncodingLimit,
    )?;
    out.extend_from_slice(bytes);
    Ok(())
}
fn framed(
    out: &mut Vec<u8>,
    bytes: &[u8],
    meter: &mut usize,
) -> Result<(), UnknownYamlBuildErrorKind> {
    append(out, &(bytes.len() as u64).to_be_bytes(), meter)?;
    append(out, bytes, meter)
}
fn encode_value(
    value: &ValueRepresentation,
    meter: &mut usize,
) -> Result<Vec<u8>, UnknownYamlBuildErrorKind> {
    let mut out = Vec::new();
    match value {
        ValueRepresentation::Scalar(s) => match &s.representation {
            ScalarRepresentation::Null => append(&mut out, &[0x00], meter)?,
            ScalarRepresentation::Boolean(v) => append(&mut out, &[0x01, u8::from(*v)], meter)?,
            ScalarRepresentation::Integer(v) => {
                append(&mut out, &[0x02, u8::from(v.negative)], meter)?;
                framed(&mut out, v.digits.as_bytes(), meter)?
            }
            ScalarRepresentation::Float(v) => {
                append(&mut out, &[0x03], meter)?;
                match &v.representation {
                    FloatRepresentation::Finite {
                        negative,
                        coefficient,
                        exponent,
                    } => {
                        append(&mut out, &[0x00], meter)?;
                        append(
                            &mut out,
                            &[u8::from(*negative && coefficient != "0")],
                            meter,
                        )?;
                        framed(&mut out, coefficient.as_bytes(), meter)?;
                        append(&mut out, &[u8::from(exponent.negative)], meter)?;
                        framed(&mut out, exponent.digits.as_bytes(), meter)?
                    }
                    FloatRepresentation::PositiveInfinity => append(&mut out, &[0x01], meter)?,
                    FloatRepresentation::NegativeInfinity => append(&mut out, &[0x02], meter)?,
                    FloatRepresentation::NaN => append(&mut out, &[0x03], meter)?,
                }
            }
            ScalarRepresentation::String(v) => {
                append(&mut out, &[0x04], meter)?;
                framed(&mut out, v.as_bytes(), meter)?
            }
        },
        ValueRepresentation::Sequence(values) => {
            append(&mut out, &[0x05], meter)?;
            append(&mut out, &(values.len() as u64).to_be_bytes(), meter)?;
            for v in values {
                framed(&mut out, &v.key_encoding, meter)?
            }
        }
        ValueRepresentation::Mapping {
            entries,
            semantic_order,
        } => {
            append(&mut out, &[0x06], meter)?;
            append(&mut out, &(entries.len() as u64).to_be_bytes(), meter)?;
            for &index in semantic_order {
                let entry = &entries[index];
                framed(&mut out, &entry.key.key_encoding, meter)?;
                framed(&mut out, &entry.value.key_encoding, meter)?
            }
        }
    }
    Ok(out)
}
fn stable_merge_order(
    records: &[UnknownYamlMappingEntry],
    meter: &mut usize,
) -> Result<Vec<usize>, UnknownYamlBuildErrorKind> {
    let mut current: Vec<usize> = (0..records.len()).collect();
    let mut scratch = vec![0; records.len()];
    let mut width = 1usize;
    while width < current.len() {
        let mut start = 0usize;
        while start < current.len() {
            let middle = start.saturating_add(width).min(current.len());
            let end = middle.saturating_add(width).min(current.len());
            let (mut left, mut right, mut output) = (start, middle, start);
            while left < middle && right < end {
                let a = &records[current[left]];
                let b = &records[current[right]];
                let mut ordering = compare(&a.key.key_encoding, &b.key.key_encoding, meter)?;
                if ordering == core::cmp::Ordering::Equal {
                    ordering = compare(&a.value.key_encoding, &b.value.key_encoding, meter)?;
                }
                if ordering != core::cmp::Ordering::Greater {
                    scratch[output] = current[left];
                    left += 1;
                } else {
                    scratch[output] = current[right];
                    right += 1;
                }
                output += 1;
            }
            while left < middle {
                scratch[output] = current[left];
                left += 1;
                output += 1;
            }
            while right < end {
                scratch[output] = current[right];
                right += 1;
                output += 1;
            }
            start = end;
        }
        core::mem::swap(&mut current, &mut scratch);
        width = width
            .checked_mul(2)
            .ok_or(UnknownYamlBuildErrorKind::ArithmeticOverflow)?;
    }
    Ok(current)
}
fn validate_mapping(
    entries: &[UnknownYamlMappingEntry],
    meter: &mut usize,
) -> Result<Vec<usize>, UnknownYamlBuildErrorKind> {
    let order = stable_merge_order(entries, meter)?;
    for adjacent in order.windows(2) {
        if compare(
            &entries[adjacent[0]].key.key_encoding,
            &entries[adjacent[1]].key.key_encoding,
            meter,
        )? == core::cmp::Ordering::Equal
        {
            return Err(UnknownYamlBuildErrorKind::DuplicateMappingKey);
        }
    }
    Ok(order)
}
fn compare(
    a: &[u8],
    b: &[u8],
    meter: &mut usize,
) -> Result<core::cmp::Ordering, UnknownYamlBuildErrorKind> {
    for (left, right) in a.iter().zip(b) {
        charge(
            meter,
            2,
            MAX_COMPARISON_UNITS,
            UnknownYamlBuildErrorKind::SemanticComparisonLimit,
        )?;
        match left.cmp(right) {
            core::cmp::Ordering::Equal => {}
            ordering => return Ok(ordering),
        }
    }
    Ok(a.len().cmp(&b.len()))
}
fn is_noncharacter(c: char) -> bool {
    let n = c as u32;
    (0xFDD0..=0xFDEF).contains(&n) || n & 0xFFFE == 0xFFFE
}

#[cfg(test)]
mod tests {
    use super::*;
    fn err(
        builder: &mut UnknownYamlDocumentBuilder,
        event: UnknownYamlEvent,
    ) -> UnknownYamlBuildErrorKind {
        builder.push_event(event).expect_err("must fail").kind()
    }
    #[test]
    fn cached_encoding_exact_limit_and_one_over() {
        let mut b = UnknownYamlDocumentBuilder::new();
        b.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        b.seed_meters(1, MAX_ENCODING_BYTES - 10, 0);
        b.push_event(UnknownYamlEvent::String("a".into())).unwrap();
        assert!(matches!(
            err(&mut b, UnknownYamlEvent::Null),
            UnknownYamlBuildErrorKind::SemanticEncodingLimit
        ));
    }
    #[test]
    fn comparison_meter_exact_and_one_over() {
        for delta in [40, 39] {
            let mut b = UnknownYamlDocumentBuilder::new();
            for e in [
                UnknownYamlEvent::BeginMapping,
                UnknownYamlEvent::String("a".into()),
                UnknownYamlEvent::Null,
                UnknownYamlEvent::String("b".into()),
                UnknownYamlEvent::Null,
            ] {
                b.push_event(e).unwrap()
            }
            b.seed_meters(5, 0, MAX_COMPARISON_UNITS - delta);
            let result = b.push_event(UnknownYamlEvent::EndMapping);
            assert_eq!(result.is_ok(), delta == 40);
        }
    }
    #[test]
    fn checked_overflow_precedes_mutation() {
        let mut b = UnknownYamlDocumentBuilder::new();
        b.seed_meters(usize::MAX, 0, 0);
        assert!(matches!(
            err(&mut b, UnknownYamlEvent::BeginMapping),
            UnknownYamlBuildErrorKind::ArithmeticOverflow
        ));
        assert!(matches!(
            err(&mut b, UnknownYamlEvent::BeginMapping),
            UnknownYamlBuildErrorKind::Poisoned
        ));
    }
    #[test]
    fn node_limit_accepts_equality() {
        let mut b = UnknownYamlDocumentBuilder::new();
        b.seed_meters(MAX_NODES - 1, 0, 0);
        b.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        assert!(matches!(
            err(&mut b, UnknownYamlEvent::String("k".into())),
            UnknownYamlBuildErrorKind::NodeLimit
        ));
    }

    #[test]
    fn cached_encoding_natural_exact_limit_and_one_over() {
        let mut builder = UnknownYamlDocumentBuilder::new();
        builder.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        builder
            .push_event(UnknownYamlEvent::String("k".into()))
            .unwrap();
        builder.push_event(UnknownYamlEvent::BeginSequence).unwrap();
        for _ in 0..127 {
            builder
                .push_event(UnknownYamlEvent::String("v".repeat(MAX_SCALAR_BYTES)))
                .unwrap();
        }
        builder
            .push_event(UnknownYamlEvent::String("v".repeat(64_374)))
            .unwrap();
        assert_eq!(builder.meters().2, MAX_ENCODING_BYTES);
        assert!(
            err(&mut builder, UnknownYamlEvent::String(String::new()))
                == UnknownYamlBuildErrorKind::SemanticEncodingLimit
        );
    }

    #[test]
    fn comparison_meter_natural_exact_limit_and_one_over() {
        let integer = |digits: &str| {
            UnknownYamlEvent::Integer(
                CanonicalYamlInteger::new(false, digits.into()).expect("canonical integer"),
            )
        };
        let mut builder = UnknownYamlDocumentBuilder::new();
        builder.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        builder
            .push_event(UnknownYamlEvent::String("first".into()))
            .unwrap();
        builder.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        for key in [
            UnknownYamlEvent::Null,
            UnknownYamlEvent::Boolean(false),
            integer("0"),
            integer("1"),
            UnknownYamlEvent::Boolean(true),
        ] {
            builder.push_event(key).unwrap();
            builder.push_event(UnknownYamlEvent::Null).unwrap();
        }
        builder.push_event(UnknownYamlEvent::EndMapping).unwrap();
        assert_eq!(builder.meters().3, 66);

        builder
            .push_event(UnknownYamlEvent::String("second".into()))
            .unwrap();
        builder.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        let prefix = "a".repeat(13_106);
        for index in (0..256).rev() {
            builder
                .push_event(UnknownYamlEvent::String(format!("{prefix}{index:03}")))
                .unwrap();
            builder.push_event(UnknownYamlEvent::Null).unwrap();
        }
        builder.push_event(UnknownYamlEvent::EndMapping).unwrap();
        assert_eq!(builder.meters().3, 66 + 29_218 + 2_558 * 13_106);
        assert_eq!(builder.meters().3, MAX_COMPARISON_UNITS);
        assert!(
            err(&mut builder, UnknownYamlEvent::EndMapping)
                == UnknownYamlBuildErrorKind::SemanticComparisonLimit
        );
    }

    #[test]
    fn node_meter_natural_exact_limit_and_one_over() {
        fn fill(extra_last: usize) -> Result<UnknownYamlDocumentBuilder, UnknownYamlBuildError> {
            let mut builder = UnknownYamlDocumentBuilder::new();
            builder.push_event(UnknownYamlEvent::BeginMapping)?;
            for group in 0..10 {
                builder.push_event(UnknownYamlEvent::String(format!("k{group}")))?;
                builder.push_event(UnknownYamlEvent::BeginSequence)?;
                let children = if group == 9 {
                    1_979 + extra_last
                } else {
                    2_000
                };
                for _ in 0..children {
                    builder.push_event(UnknownYamlEvent::Null)?;
                }
                builder.push_event(UnknownYamlEvent::EndSequence)?;
            }
            builder.push_event(UnknownYamlEvent::EndMapping)?;
            Ok(builder)
        }
        let exact = fill(0).expect("20,000 natural nodes accepted");
        assert_eq!(exact.meters().0, MAX_NODES);
        assert!(exact.finish_document().is_ok());
        let over = match fill(1) {
            Ok(_) => panic!("20,001st natural node was accepted"),
            Err(error) => error,
        };
        assert!(over.kind() == UnknownYamlBuildErrorKind::NodeLimit);
    }

    #[test]
    fn stable_merge_and_adjacent_scan_charge_the_reference_count() {
        let mut builder = UnknownYamlDocumentBuilder::new();
        for event in [
            UnknownYamlEvent::BeginMapping,
            UnknownYamlEvent::String("a".into()),
            UnknownYamlEvent::Null,
            UnknownYamlEvent::String("b".into()),
            UnknownYamlEvent::Null,
            UnknownYamlEvent::EndMapping,
        ] {
            builder.push_event(event).unwrap();
        }
        assert_eq!(builder.meters().3, 40);

        let mut multi_width = UnknownYamlDocumentBuilder::new();
        multi_width
            .push_event(UnknownYamlEvent::BeginMapping)
            .unwrap();
        multi_width
            .push_event(UnknownYamlEvent::String("nested".into()))
            .unwrap();
        multi_width
            .push_event(UnknownYamlEvent::BeginMapping)
            .unwrap();
        for index in (0..8).rev() {
            multi_width
                .push_event(UnknownYamlEvent::String(format!("a{index:03}")))
                .unwrap();
            multi_width.push_event(UnknownYamlEvent::Null).unwrap();
        }
        multi_width
            .push_event(UnknownYamlEvent::EndMapping)
            .unwrap();
        // Merge widths 1, 2, and 4 each charge 104; the final seven-pair
        // adjacent scan charges 182.
        assert_eq!(multi_width.meters().3, 104 * 3 + 182);
    }

    #[test]
    fn structural_close_errors_precede_full_parent_limits() {
        let mut builder = UnknownYamlDocumentBuilder::new();
        builder.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        builder
            .push_event(UnknownYamlEvent::String("root".into()))
            .unwrap();
        builder.push_event(UnknownYamlEvent::BeginSequence).unwrap();
        for _ in 0..MAX_SEQUENCE_ENTRIES {
            builder.push_event(UnknownYamlEvent::Null).unwrap();
        }
        builder.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        assert!(
            err(&mut builder, UnknownYamlEvent::EndSequence)
                == UnknownYamlBuildErrorKind::MismatchedEnd
        );

        let mut builder = UnknownYamlDocumentBuilder::new();
        builder.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        builder
            .push_event(UnknownYamlEvent::String("root".into()))
            .unwrap();
        builder.push_event(UnknownYamlEvent::BeginSequence).unwrap();
        for _ in 0..MAX_SEQUENCE_ENTRIES {
            builder.push_event(UnknownYamlEvent::Null).unwrap();
        }
        builder.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        builder
            .push_event(UnknownYamlEvent::String("dangling".into()))
            .unwrap();
        assert!(
            err(&mut builder, UnknownYamlEvent::EndMapping)
                == UnknownYamlBuildErrorKind::IncompleteMappingEntry
        );
    }

    #[test]
    fn every_push_error_kind_poisoned_follow_up_is_uniform() {
        fn poison(
            mut builder: UnknownYamlDocumentBuilder,
            event: UnknownYamlEvent,
            expected: UnknownYamlBuildErrorKind,
        ) {
            assert!(err(&mut builder, event) == expected);
            assert!(
                err(&mut builder, UnknownYamlEvent::Null) == UnknownYamlBuildErrorKind::Poisoned
            );
        }

        poison(
            UnknownYamlDocumentBuilder::new(),
            UnknownYamlEvent::Null,
            UnknownYamlBuildErrorKind::RootMustBeMapping,
        );
        let mut complete = UnknownYamlDocumentBuilder::new();
        complete.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        complete.push_event(UnknownYamlEvent::EndMapping).unwrap();
        poison(
            complete,
            UnknownYamlEvent::BeginMapping,
            UnknownYamlBuildErrorKind::MultipleRoots,
        );
        poison(
            UnknownYamlDocumentBuilder::new(),
            UnknownYamlEvent::EndMapping,
            UnknownYamlBuildErrorKind::UnexpectedEnd,
        );
        let mut mismatch = UnknownYamlDocumentBuilder::new();
        mismatch.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        poison(
            mismatch,
            UnknownYamlEvent::EndSequence,
            UnknownYamlBuildErrorKind::MismatchedEnd,
        );
        let mut incomplete_entry = UnknownYamlDocumentBuilder::new();
        incomplete_entry
            .push_event(UnknownYamlEvent::BeginMapping)
            .unwrap();
        incomplete_entry
            .push_event(UnknownYamlEvent::String("key".into()))
            .unwrap();
        poison(
            incomplete_entry,
            UnknownYamlEvent::EndMapping,
            UnknownYamlBuildErrorKind::IncompleteMappingEntry,
        );
        let mut top_key = UnknownYamlDocumentBuilder::new();
        top_key.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        poison(
            top_key,
            UnknownYamlEvent::Boolean(false),
            UnknownYamlBuildErrorKind::TopLevelKeyMustBeString,
        );
        let mut duplicate = UnknownYamlDocumentBuilder::new();
        duplicate
            .push_event(UnknownYamlEvent::BeginMapping)
            .unwrap();
        duplicate
            .push_event(UnknownYamlEvent::String("outer".into()))
            .unwrap();
        duplicate
            .push_event(UnknownYamlEvent::BeginMapping)
            .unwrap();
        for _ in 0..2 {
            duplicate.push_event(UnknownYamlEvent::Null).unwrap();
            duplicate.push_event(UnknownYamlEvent::Null).unwrap();
        }
        poison(
            duplicate,
            UnknownYamlEvent::EndMapping,
            UnknownYamlBuildErrorKind::DuplicateMappingKey,
        );
        let mut invalid_string = UnknownYamlDocumentBuilder::new();
        invalid_string
            .push_event(UnknownYamlEvent::BeginMapping)
            .unwrap();
        poison(
            invalid_string,
            UnknownYamlEvent::String("\u{fdd0}".into()),
            UnknownYamlBuildErrorKind::InvalidString,
        );
        let mut depth = UnknownYamlDocumentBuilder::new();
        depth.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        depth
            .push_event(UnknownYamlEvent::String("depth".into()))
            .unwrap();
        for _ in 0..15 {
            depth.push_event(UnknownYamlEvent::BeginSequence).unwrap();
        }
        poison(
            depth,
            UnknownYamlEvent::BeginSequence,
            UnknownYamlBuildErrorKind::DepthLimit,
        );
        let mut mapping_limit = UnknownYamlDocumentBuilder::new();
        mapping_limit
            .push_event(UnknownYamlEvent::BeginMapping)
            .unwrap();
        for index in 0..MAX_MAPPING_ENTRIES {
            mapping_limit
                .push_event(UnknownYamlEvent::String(format!("key{index}")))
                .unwrap();
            mapping_limit.push_event(UnknownYamlEvent::Null).unwrap();
        }
        mapping_limit
            .push_event(UnknownYamlEvent::String("over".into()))
            .unwrap();
        poison(
            mapping_limit,
            UnknownYamlEvent::Null,
            UnknownYamlBuildErrorKind::MappingEntryLimit,
        );
        let mut sequence_limit = UnknownYamlDocumentBuilder::new();
        sequence_limit
            .push_event(UnknownYamlEvent::BeginMapping)
            .unwrap();
        sequence_limit
            .push_event(UnknownYamlEvent::String("sequence".into()))
            .unwrap();
        sequence_limit
            .push_event(UnknownYamlEvent::BeginSequence)
            .unwrap();
        for _ in 0..MAX_SEQUENCE_ENTRIES {
            sequence_limit.push_event(UnknownYamlEvent::Null).unwrap();
        }
        poison(
            sequence_limit,
            UnknownYamlEvent::Null,
            UnknownYamlBuildErrorKind::SequenceEntryLimit,
        );
        let mut node_limit = UnknownYamlDocumentBuilder::new();
        node_limit.seed_meters(MAX_NODES, 0, 0);
        poison(
            node_limit,
            UnknownYamlEvent::BeginMapping,
            UnknownYamlBuildErrorKind::NodeLimit,
        );
        let mut scalar_limit = UnknownYamlDocumentBuilder::new();
        scalar_limit
            .push_event(UnknownYamlEvent::BeginMapping)
            .unwrap();
        poison(
            scalar_limit,
            UnknownYamlEvent::String("x".repeat(MAX_SCALAR_BYTES + 1)),
            UnknownYamlBuildErrorKind::ScalarLimit,
        );
        let mut numeric_limit = UnknownYamlDocumentBuilder::new();
        numeric_limit
            .push_event(UnknownYamlEvent::BeginMapping)
            .unwrap();
        numeric_limit
            .push_event(UnknownYamlEvent::String("numbers".into()))
            .unwrap();
        numeric_limit
            .push_event(UnknownYamlEvent::BeginSequence)
            .unwrap();
        for digits in [MAX_SCALAR_BYTES, MAX_NUMERIC_DIGITS - MAX_SCALAR_BYTES] {
            numeric_limit
                .push_event(UnknownYamlEvent::Integer(
                    CanonicalYamlInteger::new(false, "1".repeat(digits)).unwrap(),
                ))
                .unwrap();
        }
        poison(
            numeric_limit,
            UnknownYamlEvent::Integer(CanonicalYamlInteger::new(false, "1".into()).unwrap()),
            UnknownYamlBuildErrorKind::NumericDigitLimit,
        );
        let mut encoding_limit = UnknownYamlDocumentBuilder::new();
        encoding_limit
            .push_event(UnknownYamlEvent::BeginMapping)
            .unwrap();
        encoding_limit.seed_meters(1, MAX_ENCODING_BYTES, 0);
        poison(
            encoding_limit,
            UnknownYamlEvent::String("key".into()),
            UnknownYamlBuildErrorKind::SemanticEncodingLimit,
        );
        let mut comparison_limit = UnknownYamlDocumentBuilder::new();
        for event in [
            UnknownYamlEvent::BeginMapping,
            UnknownYamlEvent::String("a".into()),
            UnknownYamlEvent::Null,
            UnknownYamlEvent::String("b".into()),
            UnknownYamlEvent::Null,
        ] {
            comparison_limit.push_event(event).unwrap();
        }
        comparison_limit.seed_meters(5, 0, MAX_COMPARISON_UNITS);
        poison(
            comparison_limit,
            UnknownYamlEvent::EndMapping,
            UnknownYamlBuildErrorKind::SemanticComparisonLimit,
        );
        let mut overflow = UnknownYamlDocumentBuilder::new();
        overflow.seed_meters(usize::MAX, 0, 0);
        poison(
            overflow,
            UnknownYamlEvent::BeginMapping,
            UnknownYamlBuildErrorKind::ArithmeticOverflow,
        );

        let incomplete = UnknownYamlDocumentBuilder::new()
            .finish_document()
            .err()
            .expect("empty document must be incomplete");
        assert!(incomplete.kind() == UnknownYamlBuildErrorKind::IncompleteDocument);
    }
}
