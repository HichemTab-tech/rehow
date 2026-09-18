#![allow(
    dead_code,
    reason = "the pure Phase 1E-B1 journal boundary is connected to durable I/O in Phase 1E-B2"
)]

use crate::protocol::{GuideId, OperationId, Revision, Timestamp, WorkspaceId};
use std::{collections::HashSet, str::FromStr};
use unicode_normalization::UnicodeNormalization;

mod store;
mod workspace;

pub(crate) const JOURNAL_VERSION: u64 = 1;
pub(crate) const MAX_JOURNAL_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const MAX_TARGETS: usize = 1_024;
const MAX_PATH_BYTES: usize = 1_024;
const MAX_COMPONENT_BYTES: usize = 255;

pub(crate) type ContentHash = [u8; 32];

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct Journal {
    pub(crate) generation: u64,
    pub(crate) operation_id: OperationId,
    pub(crate) operation_kind: OperationKind,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) root_hash: ContentHash,
    pub(crate) root_device: u64,
    pub(crate) root_inode: u64,
    pub(crate) created_at: Timestamp,
    pub(crate) plan_hash: ContentHash,
    pub(crate) targets: Vec<Target>,
    pub(crate) stage: JournalStage,
    pub(crate) completion: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationKind {
    Create,
    Update,
    Delete,
    MultiFile,
    Recovery,
    Migration,
    GitMaterialize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JournalStage {
    Prepared,
    Applying,
    Committed,
    Conflict,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct Target {
    pub(crate) relative_path: String,
    pub(crate) expected: ExpectedState,
    pub(crate) intended: IntendedState,
    pub(crate) temp_name: String,
    pub(crate) step: TargetStep,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) enum ExpectedState {
    Absent,
    Existing {
        revision: Revision,
        object_hash: ContentHash,
        mode: u32,
    },
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) enum IntendedState {
    Absent,
    Object {
        object_hash: ContentHash,
        mode: u32,
    },
    Trash {
        object_hash: ContentHash,
        mode: u32,
        destination: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TargetStep {
    Prepared,
    Swapped,
    DirectorySynced,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JournalCodecError {
    TooLarge,
    ResourceLimit,
    Invalid,
}

impl Journal {
    pub(crate) fn encode(&self) -> Result<Vec<u8>, JournalCodecError> {
        self.validate()?;
        let mut writer = Writer::new()?;
        writer.map(13)?;
        writer.key(0)?;
        writer.uint(JOURNAL_VERSION)?;
        writer.key(1)?;
        writer.uint(self.generation)?;
        writer.key(2)?;
        writer.text(&self.operation_id.to_string())?;
        writer.key(3)?;
        writer.uint(self.operation_kind.code())?;
        writer.key(4)?;
        writer.text(&self.workspace_id.to_string())?;
        writer.key(5)?;
        writer.bytes(&self.root_hash)?;
        writer.key(6)?;
        writer.uint(self.root_device)?;
        writer.key(7)?;
        writer.uint(self.root_inode)?;
        writer.key(8)?;
        writer.text(&self.created_at.to_string())?;
        writer.key(9)?;
        writer.bytes(&self.plan_hash)?;
        writer.key(10)?;
        writer.array(self.targets.len() as u64)?;
        for target in &self.targets {
            target.encode(&mut writer)?;
        }
        writer.key(11)?;
        writer.uint(self.stage.code())?;
        writer.key(12)?;
        writer.boolean(self.completion)?;
        if writer.bytes.len() > MAX_JOURNAL_BYTES {
            return Err(JournalCodecError::TooLarge);
        }
        Ok(writer.bytes)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, JournalCodecError> {
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(JournalCodecError::TooLarge);
        }
        let mut reader = Reader::new(bytes);
        reader.map(13)?;
        reader.key(0)?;
        if reader.uint()? != JOURNAL_VERSION {
            return Err(JournalCodecError::Invalid);
        }
        reader.key(1)?;
        let generation = reader.uint()?;
        reader.key(2)?;
        let operation_id = reader.protocol_text::<OperationId>(36)?;
        reader.key(3)?;
        let operation_kind = OperationKind::from_code(reader.uint()?)?;
        reader.key(4)?;
        let workspace_id = reader.protocol_text::<WorkspaceId>(36)?;
        reader.key(5)?;
        let root_hash = reader.hash()?;
        reader.key(6)?;
        let root_device = reader.uint()?;
        reader.key(7)?;
        let root_inode = reader.uint()?;
        reader.key(8)?;
        let created_at = reader.protocol_text::<Timestamp>(30)?;
        reader.key(9)?;
        let plan_hash = reader.hash()?;
        reader.key(10)?;
        let target_count = reader.array_len(MAX_TARGETS)?;
        if target_count == 0 {
            return Err(JournalCodecError::Invalid);
        }
        let mut targets = Vec::new();
        targets
            .try_reserve_exact(target_count)
            .map_err(|_| JournalCodecError::ResourceLimit)?;
        for _ in 0..target_count {
            targets.push(Target::decode(&mut reader)?);
        }
        reader.key(11)?;
        let stage = JournalStage::from_code(reader.uint()?)?;
        reader.key(12)?;
        let completion = reader.boolean()?;
        if !reader.finished() {
            return Err(JournalCodecError::Invalid);
        }
        let journal = Self {
            generation,
            operation_id,
            operation_kind,
            workspace_id,
            root_hash,
            root_device,
            root_inode,
            created_at,
            plan_hash,
            targets,
            stage,
            completion,
        };
        journal.validate()?;
        Ok(journal)
    }

    fn validate(&self) -> Result<(), JournalCodecError> {
        if self.generation == 0 || self.targets.is_empty() || self.targets.len() > MAX_TARGETS {
            return Err(JournalCodecError::Invalid);
        }
        let committed = self.stage == JournalStage::Committed;
        if committed != self.completion
            || (committed
                && self
                    .targets
                    .iter()
                    .any(|target| target.step != TargetStep::Complete))
            || (self.stage == JournalStage::Prepared
                && self
                    .targets
                    .iter()
                    .any(|target| target.step != TargetStep::Prepared))
        {
            return Err(JournalCodecError::Invalid);
        }
        let mut paths = HashSet::new();
        paths
            .try_reserve(self.targets.len())
            .map_err(|_| JournalCodecError::ResourceLimit)?;
        for target in &self.targets {
            target.validate(self.operation_kind)?;
            if let IntendedState::Trash { destination, .. } = &target.intended {
                let expected = format!(
                    ".rehow/trash/{}/{}",
                    self.operation_id, target.relative_path
                );
                if destination != &expected {
                    return Err(JournalCodecError::Invalid);
                }
            }
            if !paths.insert(target.relative_path.as_str()) {
                return Err(JournalCodecError::Invalid);
            }
        }
        let mut prior_complete = true;
        for target in &self.targets {
            if !prior_complete && target.step != TargetStep::Prepared {
                return Err(JournalCodecError::Invalid);
            }
            prior_complete = target.step == TargetStep::Complete;
        }
        for (index, target) in self.targets.iter().enumerate() {
            for prior in &self.targets[..index] {
                if parent(&target.relative_path) == parent(&prior.relative_path)
                    && target.temp_name == prior.temp_name
                {
                    return Err(JournalCodecError::Invalid);
                }
            }
        }
        Ok(())
    }

    fn is_next_generation_of(&self, next: &Self) -> bool {
        !self.completion
            && self.generation.checked_add(1) == Some(next.generation)
            && self.operation_id == next.operation_id
            && self.operation_kind == next.operation_kind
            && self.workspace_id == next.workspace_id
            && self.root_hash == next.root_hash
            && self.root_device == next.root_device
            && self.root_inode == next.root_inode
            && self.created_at == next.created_at
            && self.plan_hash == next.plan_hash
            && self.targets.len() == next.targets.len()
            && self
                .targets
                .iter()
                .zip(&next.targets)
                .all(|(current, candidate)| current.same_intent(candidate))
            && match (self.stage, next.stage) {
                (JournalStage::Prepared, JournalStage::Applying)
                | (JournalStage::Prepared | JournalStage::Applying, JournalStage::Conflict)
                | (JournalStage::Applying, JournalStage::Committed) => self.same_target_steps(next),
                (JournalStage::Applying, JournalStage::Applying) => {
                    self.has_one_exact_step_advance(next)
                }
                _ => false,
            }
    }

    fn same_target_steps(&self, other: &Self) -> bool {
        self.targets
            .iter()
            .zip(&other.targets)
            .all(|(current, candidate)| current.step == candidate.step)
    }

    fn has_one_exact_step_advance(&self, other: &Self) -> bool {
        let mut advances = 0usize;
        for (current, candidate) in self.targets.iter().zip(&other.targets) {
            if current.step == candidate.step {
                continue;
            }
            if current.step.next() != Some(candidate.step) {
                return false;
            }
            advances += 1;
        }
        advances == 1
    }
}

impl Target {
    fn same_intent(&self, other: &Self) -> bool {
        self.relative_path == other.relative_path
            && self.expected == other.expected
            && self.intended == other.intended
            && self.temp_name == other.temp_name
    }

    fn encode(&self, writer: &mut Writer) -> Result<(), JournalCodecError> {
        writer.map(5)?;
        writer.key(0)?;
        writer.text(&self.relative_path)?;
        writer.key(1)?;
        self.expected.encode(writer)?;
        writer.key(2)?;
        self.intended.encode(writer)?;
        writer.key(3)?;
        writer.text(&self.temp_name)?;
        writer.key(4)?;
        writer.uint(self.step.code())
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, JournalCodecError> {
        reader.map(5)?;
        reader.key(0)?;
        let relative_path = reader.owned_text(MAX_PATH_BYTES)?;
        reader.key(1)?;
        let expected = ExpectedState::decode(reader)?;
        reader.key(2)?;
        let intended = IntendedState::decode(reader)?;
        reader.key(3)?;
        let temp_name = reader.owned_text(MAX_COMPONENT_BYTES)?;
        reader.key(4)?;
        let step = TargetStep::from_code(reader.uint()?)?;
        Ok(Self {
            relative_path,
            expected,
            intended,
            temp_name,
            step,
        })
    }

    fn validate(&self, operation: OperationKind) -> Result<(), JournalCodecError> {
        if !valid_active_path(&self.relative_path) || !valid_temp(&self.temp_name) {
            return Err(JournalCodecError::Invalid);
        }
        self.expected.validate()?;
        self.intended.validate()?;
        if matches!(self.expected, ExpectedState::Absent)
            && matches!(self.intended, IntendedState::Absent)
        {
            return Err(JournalCodecError::Invalid);
        }
        if let IntendedState::Trash {
            object_hash, mode, ..
        } = &self.intended
        {
            let ExpectedState::Existing {
                object_hash: old_hash,
                mode: old_mode,
                ..
            } = &self.expected
            else {
                return Err(JournalCodecError::Invalid);
            };
            if object_hash != old_hash || mode != old_mode {
                return Err(JournalCodecError::Invalid);
            }
        }
        let valid_operation = match operation {
            OperationKind::Create => {
                matches!(self.expected, ExpectedState::Absent)
                    && matches!(self.intended, IntendedState::Object { .. })
            }
            OperationKind::Update => {
                matches!(self.expected, ExpectedState::Existing { .. })
                    && matches!(self.intended, IntendedState::Object { .. })
            }
            OperationKind::Delete => {
                matches!(self.expected, ExpectedState::Existing { .. })
                    && matches!(
                        self.intended,
                        IntendedState::Absent | IntendedState::Trash { .. }
                    )
            }
            OperationKind::MultiFile
            | OperationKind::Recovery
            | OperationKind::Migration
            | OperationKind::GitMaterialize => true,
        };
        if valid_operation {
            Ok(())
        } else {
            Err(JournalCodecError::Invalid)
        }
    }
}

impl ExpectedState {
    fn encode(&self, writer: &mut Writer) -> Result<(), JournalCodecError> {
        match self {
            Self::Absent => {
                writer.map(1)?;
                writer.key(0)?;
                writer.uint(0)
            }
            Self::Existing {
                revision,
                object_hash,
                mode,
            } => {
                writer.map(4)?;
                writer.key(0)?;
                writer.uint(1)?;
                writer.key(1)?;
                writer.text(&revision.to_string())?;
                writer.key(2)?;
                writer.bytes(object_hash)?;
                writer.key(3)?;
                writer.uint(u64::from(*mode))
            }
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, JournalCodecError> {
        let length = reader.map_len()?;
        reader.key(0)?;
        match (length, reader.uint()?) {
            (1, 0) => Ok(Self::Absent),
            (4, 1) => {
                reader.key(1)?;
                let revision = reader.protocol_text::<Revision>(71)?;
                reader.key(2)?;
                let object_hash = reader.hash()?;
                reader.key(3)?;
                let mode = reader.mode()?;
                Ok(Self::Existing {
                    revision,
                    object_hash,
                    mode,
                })
            }
            _ => Err(JournalCodecError::Invalid),
        }
    }

    fn validate(&self) -> Result<(), JournalCodecError> {
        match self {
            Self::Absent => Ok(()),
            Self::Existing { mode, .. } if valid_mode(*mode) => Ok(()),
            Self::Existing { .. } => Err(JournalCodecError::Invalid),
        }
    }
}

impl IntendedState {
    fn encode(&self, writer: &mut Writer) -> Result<(), JournalCodecError> {
        match self {
            Self::Absent => {
                writer.map(1)?;
                writer.key(0)?;
                writer.uint(0)
            }
            Self::Object { object_hash, mode } => {
                writer.map(3)?;
                writer.key(0)?;
                writer.uint(1)?;
                writer.key(1)?;
                writer.bytes(object_hash)?;
                writer.key(2)?;
                writer.uint(u64::from(*mode))
            }
            Self::Trash {
                object_hash,
                mode,
                destination,
            } => {
                writer.map(4)?;
                writer.key(0)?;
                writer.uint(2)?;
                writer.key(1)?;
                writer.bytes(object_hash)?;
                writer.key(2)?;
                writer.uint(u64::from(*mode))?;
                writer.key(3)?;
                writer.text(destination)
            }
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, JournalCodecError> {
        let length = reader.map_len()?;
        reader.key(0)?;
        match (length, reader.uint()?) {
            (1, 0) => Ok(Self::Absent),
            (3, 1) => {
                reader.key(1)?;
                let object_hash = reader.hash()?;
                reader.key(2)?;
                let mode = reader.mode()?;
                Ok(Self::Object { object_hash, mode })
            }
            (4, 2) => {
                reader.key(1)?;
                let object_hash = reader.hash()?;
                reader.key(2)?;
                let mode = reader.mode()?;
                reader.key(3)?;
                let destination = reader.owned_text(MAX_PATH_BYTES)?;
                Ok(Self::Trash {
                    object_hash,
                    mode,
                    destination,
                })
            }
            _ => Err(JournalCodecError::Invalid),
        }
    }

    fn validate(&self) -> Result<(), JournalCodecError> {
        match self {
            Self::Absent => Ok(()),
            Self::Object { mode, .. } if valid_mode(*mode) => Ok(()),
            Self::Trash {
                mode, destination, ..
            } if valid_mode(*mode) && valid_trash_path(destination) => Ok(()),
            Self::Object { .. } | Self::Trash { .. } => Err(JournalCodecError::Invalid),
        }
    }
}

macro_rules! coded_enum {
    ($type:ty, $($variant:path => $code:literal),+ $(,)?) => {
        impl $type {
            fn code(self) -> u64 {
                match self { $($variant => $code),+ }
            }
            fn from_code(code: u64) -> Result<Self, JournalCodecError> {
                match code { $($code => Ok($variant)),+, _ => Err(JournalCodecError::Invalid) }
            }
        }
    };
}

coded_enum!(OperationKind,
    OperationKind::Create => 0,
    OperationKind::Update => 1,
    OperationKind::Delete => 2,
    OperationKind::MultiFile => 3,
    OperationKind::Recovery => 4,
    OperationKind::Migration => 5,
    OperationKind::GitMaterialize => 6,
);
coded_enum!(JournalStage,
    JournalStage::Prepared => 0,
    JournalStage::Applying => 1,
    JournalStage::Committed => 2,
    JournalStage::Conflict => 3,
);

coded_enum!(TargetStep,
    TargetStep::Prepared => 0,
    TargetStep::Swapped => 1,
    TargetStep::DirectorySynced => 2,
    TargetStep::Complete => 3,
);

impl TargetStep {
    fn next(self) -> Option<Self> {
        match self {
            Self::Prepared => Some(Self::Swapped),
            Self::Swapped => Some(Self::DirectorySynced),
            Self::DirectorySynced => Some(Self::Complete),
            Self::Complete => None,
        }
    }
}

struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn new() -> Result<Self, JournalCodecError> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve(512)
            .map_err(|_| JournalCodecError::ResourceLimit)?;
        Ok(Self { bytes })
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), JournalCodecError> {
        let next = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(JournalCodecError::TooLarge)?;
        if next > MAX_JOURNAL_BYTES {
            return Err(JournalCodecError::TooLarge);
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| JournalCodecError::ResourceLimit)?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn head(&mut self, major: u8, value: u64) -> Result<(), JournalCodecError> {
        let prefix = major << 5;
        if value < 24 {
            self.append(&[prefix | value as u8])
        } else if u8::try_from(value).is_ok() {
            self.append(&[prefix | 24, value as u8])
        } else if u16::try_from(value).is_ok() {
            self.append(&[prefix | 25])?;
            self.append(&(value as u16).to_be_bytes())
        } else if u32::try_from(value).is_ok() {
            self.append(&[prefix | 26])?;
            self.append(&(value as u32).to_be_bytes())
        } else {
            self.append(&[prefix | 27])?;
            self.append(&value.to_be_bytes())
        }
    }

    fn uint(&mut self, value: u64) -> Result<(), JournalCodecError> {
        self.head(0, value)
    }
    fn key(&mut self, value: u64) -> Result<(), JournalCodecError> {
        self.uint(value)
    }
    fn bytes(&mut self, value: &[u8]) -> Result<(), JournalCodecError> {
        self.head(2, value.len() as u64)?;
        self.append(value)
    }
    fn text(&mut self, value: &str) -> Result<(), JournalCodecError> {
        self.head(3, value.len() as u64)?;
        self.append(value.as_bytes())
    }
    fn array(&mut self, length: u64) -> Result<(), JournalCodecError> {
        self.head(4, length)
    }
    fn map(&mut self, length: u64) -> Result<(), JournalCodecError> {
        self.head(5, length)
    }
    fn boolean(&mut self, value: bool) -> Result<(), JournalCodecError> {
        self.append(&[if value { 0xf5 } else { 0xf4 }])
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], JournalCodecError> {
        let end = self
            .offset
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(JournalCodecError::Invalid)?;
        let result = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(result)
    }

    fn head(&mut self) -> Result<(u8, u64), JournalCodecError> {
        let initial = self.take(1)?[0];
        let major = initial >> 5;
        let additional = initial & 0x1f;
        if major == 7 && additional >= 24 {
            return Err(JournalCodecError::Invalid);
        }
        let value = match additional {
            value @ 0..=23 => u64::from(value),
            24 => {
                let value = u64::from(self.take(1)?[0]);
                if value < 24 {
                    return Err(JournalCodecError::Invalid);
                }
                value
            }
            25 => {
                let value = u64::from(u16::from_be_bytes(
                    self.take(2)?
                        .try_into()
                        .map_err(|_| JournalCodecError::Invalid)?,
                ));
                if value <= u64::from(u8::MAX) {
                    return Err(JournalCodecError::Invalid);
                }
                value
            }
            26 => {
                let value = u64::from(u32::from_be_bytes(
                    self.take(4)?
                        .try_into()
                        .map_err(|_| JournalCodecError::Invalid)?,
                ));
                if value <= u64::from(u16::MAX) {
                    return Err(JournalCodecError::Invalid);
                }
                value
            }
            27 => {
                let value = u64::from_be_bytes(
                    self.take(8)?
                        .try_into()
                        .map_err(|_| JournalCodecError::Invalid)?,
                );
                if value <= u64::from(u32::MAX) {
                    return Err(JournalCodecError::Invalid);
                }
                value
            }
            _ => return Err(JournalCodecError::Invalid),
        };
        Ok((major, value))
    }

    fn expected_head(&mut self, major: u8) -> Result<u64, JournalCodecError> {
        let (actual, value) = self.head()?;
        if actual == major {
            Ok(value)
        } else {
            Err(JournalCodecError::Invalid)
        }
    }
    fn uint(&mut self) -> Result<u64, JournalCodecError> {
        self.expected_head(0)
    }
    fn key(&mut self, expected: u64) -> Result<(), JournalCodecError> {
        if self.uint()? == expected {
            Ok(())
        } else {
            Err(JournalCodecError::Invalid)
        }
    }
    fn map(&mut self, expected: u64) -> Result<(), JournalCodecError> {
        if self.expected_head(5)? == expected {
            Ok(())
        } else {
            Err(JournalCodecError::Invalid)
        }
    }
    fn map_len(&mut self) -> Result<u64, JournalCodecError> {
        self.expected_head(5)
    }
    fn array_len(&mut self, maximum: usize) -> Result<usize, JournalCodecError> {
        let value =
            usize::try_from(self.expected_head(4)?).map_err(|_| JournalCodecError::Invalid)?;
        if value <= maximum {
            Ok(value)
        } else {
            Err(JournalCodecError::Invalid)
        }
    }
    fn borrowed_text(&mut self, maximum: usize) -> Result<&'a str, JournalCodecError> {
        let length =
            usize::try_from(self.expected_head(3)?).map_err(|_| JournalCodecError::Invalid)?;
        if length > maximum {
            return Err(JournalCodecError::Invalid);
        }
        std::str::from_utf8(self.take(length)?).map_err(|_| JournalCodecError::Invalid)
    }
    fn owned_text(&mut self, maximum: usize) -> Result<String, JournalCodecError> {
        let borrowed = self.borrowed_text(maximum)?;
        let mut owned = String::new();
        owned
            .try_reserve_exact(borrowed.len())
            .map_err(|_| JournalCodecError::ResourceLimit)?;
        owned.push_str(borrowed);
        Ok(owned)
    }
    fn protocol_text<T: FromStr>(&mut self, maximum: usize) -> Result<T, JournalCodecError> {
        self.borrowed_text(maximum)?
            .parse()
            .map_err(|_| JournalCodecError::Invalid)
    }
    fn hash(&mut self) -> Result<ContentHash, JournalCodecError> {
        if self.expected_head(2)? != 32 {
            return Err(JournalCodecError::Invalid);
        }
        self.take(32)?
            .try_into()
            .map_err(|_| JournalCodecError::Invalid)
    }
    fn mode(&mut self) -> Result<u32, JournalCodecError> {
        let mode = u32::try_from(self.uint()?).map_err(|_| JournalCodecError::Invalid)?;
        if valid_mode(mode) {
            Ok(mode)
        } else {
            Err(JournalCodecError::Invalid)
        }
    }
    fn boolean(&mut self) -> Result<bool, JournalCodecError> {
        match self.take(1)?[0] {
            0xf4 => Ok(false),
            0xf5 => Ok(true),
            _ => Err(JournalCodecError::Invalid),
        }
    }
    fn finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

fn valid_mode(mode: u32) -> bool {
    mode <= 0o777
}

pub(super) fn valid_active_path(path: &str) -> bool {
    if path.len() > MAX_PATH_BYTES {
        return false;
    }
    if path == "rehow.yaml" {
        return true;
    }
    let parts = path.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["categories", directory, "category.yaml"] => super::valid_category_directory(directory),
        ["categories", directory, guide] => {
            super::valid_category_directory(directory) && super::valid_guide_filename(guide)
        }
        ["attachments", guide_id, filename] => {
            GuideId::from_str(guide_id).is_ok() && valid_attachment_filename(filename)
        }
        _ => false,
    }
}

fn valid_attachment_filename(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_COMPONENT_BYTES
        && value != "."
        && value != ".."
        && !value.contains(['/', '\\', '\0'])
        && value.nfc().eq(value.chars())
        && !value.chars().any(|character| {
            matches!(character as u32, 0x00..=0x1f|0x7f..=0x9f|0x061c|0x200e|0x200f|0x2028..=0x202e|0x2066..=0x2069|0xfeff|0xfdd0..=0xfdef)
                || matches!(character as u32 & 0xffff, 0xfffe | 0xffff)
        })
}

pub(super) fn valid_trash_path(path: &str) -> bool {
    let Some(remainder) = path.strip_prefix(".rehow/trash/") else {
        return false;
    };
    let Some((operation, active_path)) = remainder.split_once('/') else {
        return false;
    };
    OperationId::from_str(operation).is_ok() && valid_active_path(active_path)
}

fn valid_temp(value: &str) -> bool {
    value.len() <= MAX_COMPONENT_BYTES
        && value.strip_prefix(".rehow-tmp-").is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Journal {
        Journal {
            generation: 1,
            operation_id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e10".parse().unwrap(),
            operation_kind: OperationKind::Create,
            workspace_id: "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            root_hash: [0x11; 32],
            root_device: 8,
            root_inode: 42,
            created_at: "2026-09-18T00:00:00Z".parse().unwrap(),
            plan_hash: [0x22; 32],
            targets: vec![Target {
                relative_path: "categories/linux/guide.md".to_owned(),
                expected: ExpectedState::Absent,
                intended: IntendedState::Object {
                    object_hash: [0x33; 32],
                    mode: 0o600,
                },
                temp_name: ".rehow-tmp-abcdef012345".to_owned(),
                step: TargetStep::Prepared,
            }],
            stage: JournalStage::Prepared,
            completion: false,
        }
    }

    #[test]
    fn canonical_vector_is_frozen_and_round_trips() {
        let journal = sample();
        let encoded = journal.encode().unwrap();
        let hex = encoded
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "ad0001010102782430313939613164372d376230642d376438652d613338642d663933366335373335653130030004782430313939613139662d346431352d376361372d613634352d6537353331316361613731300558201111111111111111111111111111111111111111111111111111111111111111060807182a0874323032362d30392d31385430303a30303a30305a09582022222222222222222222222222222222222222222222222222222222222222220a81a500781963617465676f726965732f6c696e75782f67756964652e6d6401a1000002a3000101582033333333333333333333333333333333333333333333333333333333333333330219018003772e7265686f772d746d702d61626364656630313233343504000b000cf4"
        );
        assert!(Journal::decode(&encoded).unwrap() == journal);
    }

    #[test]
    fn every_truncated_prefix_and_trailing_byte_is_rejected() {
        let encoded = sample().encode().unwrap();
        for length in 0..encoded.len() {
            assert!(
                Journal::decode(&encoded[..length]).is_err(),
                "prefix {length}"
            );
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert!(matches!(
            Journal::decode(&trailing),
            Err(JournalCodecError::Invalid)
        ));
    }

    #[test]
    fn noncanonical_and_unsupported_cbor_is_rejected() {
        let encoded = sample().encode().unwrap();
        for prefix in [
            vec![0xb8, 13],
            vec![0xbf],
            vec![0xd8, 0],
            vec![0xfa, 0, 0, 0, 0],
        ] {
            let mut hostile = prefix;
            hostile.extend_from_slice(&encoded[1..]);
            assert!(matches!(
                Journal::decode(&hostile),
                Err(JournalCodecError::Invalid)
            ));
        }
        let mut reordered = encoded;
        reordered[1] = 1;
        assert!(matches!(
            Journal::decode(&reordered),
            Err(JournalCodecError::Invalid)
        ));
        let mut non_shortest_key = vec![0xad, 0x18, 0x00];
        non_shortest_key.extend_from_slice(&sample().encode().unwrap()[2..]);
        assert!(matches!(
            Journal::decode(&non_shortest_key),
            Err(JournalCodecError::Invalid)
        ));
        let mut extra_key_claim = sample().encode().unwrap();
        extra_key_claim[0] = 0xae;
        assert!(matches!(
            Journal::decode(&extra_key_claim),
            Err(JournalCodecError::Invalid)
        ));
        let mut bad_operation_id = sample().encode().unwrap();
        let offset = bad_operation_id
            .windows(4)
            .position(|window| window == b"0199")
            .unwrap();
        bad_operation_id[offset] = b'A';
        assert!(matches!(
            Journal::decode(&bad_operation_id),
            Err(JournalCodecError::Invalid)
        ));
    }

    #[test]
    fn closed_state_and_path_invariants_fail_before_encoding() {
        let mut journal = sample();
        journal.generation = 0;
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        journal.targets[0].relative_path = "../escape".to_owned();
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        journal.targets[0].temp_name = "target.md".to_owned();
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        journal.completion = true;
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        if let IntendedState::Object { mode, .. } = &mut journal.targets[0].intended {
            *mode = 0o1_600;
        }
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        journal.operation_kind = OperationKind::Delete;
        journal.targets[0].expected = ExpectedState::Existing {
            revision: format!("rev:v1:{}", "44".repeat(32)).parse().unwrap(),
            object_hash: [0x44; 32],
            mode: 0o600,
        };
        journal.targets[0].intended = IntendedState::Trash {
            object_hash: [0x55; 32],
            mode: 0o600,
            destination:
                ".rehow/trash/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/categories/linux/guide.md"
                    .to_owned(),
        };
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        journal.operation_kind = OperationKind::Delete;
        journal.targets[0].expected = ExpectedState::Existing {
            revision: format!("rev:v1:{}", "44".repeat(32)).parse().unwrap(),
            object_hash: [0x44; 32],
            mode: 0o600,
        };
        journal.targets[0].intended = IntendedState::Trash {
            object_hash: [0x44; 32],
            mode: 0o600,
            destination:
                ".rehow/trash/0199a1d7-7b0d-7d8e-a38d-f936c5735e11/categories/linux/guide.md"
                    .to_owned(),
        };
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal.targets[0].intended = IntendedState::Trash {
            object_hash: [0x44; 32],
            mode: 0o600,
            destination: ".rehow/trash/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/rehow.yaml".to_owned(),
        };
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
    }

    #[test]
    fn every_closed_discriminant_round_trips() {
        for operation in [
            OperationKind::Create,
            OperationKind::Update,
            OperationKind::Delete,
            OperationKind::MultiFile,
            OperationKind::Recovery,
            OperationKind::Migration,
            OperationKind::GitMaterialize,
        ] {
            assert_eq!(OperationKind::from_code(operation.code()), Ok(operation));
        }
        for stage in [
            JournalStage::Prepared,
            JournalStage::Applying,
            JournalStage::Committed,
            JournalStage::Conflict,
        ] {
            assert_eq!(JournalStage::from_code(stage.code()), Ok(stage));
        }
        for step in [
            TargetStep::Prepared,
            TargetStep::Swapped,
            TargetStep::DirectorySynced,
            TargetStep::Complete,
        ] {
            assert_eq!(TargetStep::from_code(step.code()), Ok(step));
        }
        assert_eq!(OperationKind::from_code(7), Err(JournalCodecError::Invalid));
        assert_eq!(JournalStage::from_code(4), Err(JournalCodecError::Invalid));
        assert_eq!(TargetStep::from_code(4), Err(JournalCodecError::Invalid));
    }

    #[test]
    fn bounds_duplicates_and_managed_path_grammar_are_enforced() {
        assert_eq!(
            Journal::decode(&vec![0; MAX_JOURNAL_BYTES + 1]).err(),
            Some(JournalCodecError::TooLarge)
        );
        let mut journal = sample();
        journal.targets = vec![journal.targets[0].clone(); MAX_TARGETS + 1];
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        journal.targets.push(journal.targets[0].clone());
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));

        for path in [
            "/rehow.yaml",
            "categories/Linux/guide.md",
            "categories/linux/nested/guide.md",
            "attachments/not-an-id/file.png",
            "attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/../file.png",
            ".rehow/trash/file.md",
        ] {
            let mut invalid = sample();
            invalid.targets[0].relative_path = path.to_owned();
            assert_eq!(invalid.encode(), Err(JournalCodecError::Invalid), "{path}");
        }
        assert!(valid_active_path("rehow.yaml"));
        assert!(valid_active_path("categories/linux/category.yaml"));
        assert!(valid_active_path("categories/linux/guide.md"));
        assert!(valid_active_path(
            "attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/screenshot.png"
        ));

        let mut out_of_order = sample();
        let mut later = out_of_order.targets[0].clone();
        later.relative_path = "rehow.yaml".to_owned();
        later.step = TargetStep::Swapped;
        out_of_order.targets.push(later);
        out_of_order.stage = JournalStage::Applying;
        assert_eq!(out_of_order.encode(), Err(JournalCodecError::Invalid));
    }

    #[test]
    fn applying_conflict_and_committed_stage_invariants_round_trip() {
        for stage in [JournalStage::Applying, JournalStage::Conflict] {
            let mut journal = sample();
            journal.stage = stage;
            journal.targets[0].step = TargetStep::Swapped;
            let encoded = journal.encode().unwrap();
            assert!(Journal::decode(&encoded).unwrap() == journal);
        }
        let mut invalid = sample();
        invalid.stage = JournalStage::Committed;
        invalid.completion = true;
        assert_eq!(invalid.encode(), Err(JournalCodecError::Invalid));
    }

    #[test]
    fn update_delete_trash_and_committed_states_round_trip() {
        let old = ExpectedState::Existing {
            revision: format!("rev:v1:{}", "44".repeat(32)).parse().unwrap(),
            object_hash: [0x44; 32],
            mode: 0o640,
        };
        let cases = [
            (
                OperationKind::Update,
                IntendedState::Object {
                    object_hash: [0x55; 32],
                    mode: 0o640,
                },
            ),
            (OperationKind::Delete, IntendedState::Absent),
            (
                OperationKind::Delete,
                IntendedState::Trash {
                    object_hash: [0x44; 32],
                    mode: 0o640,
                    destination: ".rehow/trash/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/categories/linux/guide.md".to_owned(),
                },
            ),
        ];
        for (operation, intended) in cases {
            let mut journal = sample();
            journal.operation_kind = operation;
            journal.targets[0].expected = old.clone();
            journal.targets[0].intended = intended;
            journal.targets[0].step = TargetStep::Complete;
            journal.stage = JournalStage::Committed;
            journal.completion = true;
            let encoded = journal.encode().unwrap();
            assert!(Journal::decode(&encoded).unwrap() == journal);
        }
    }
}
