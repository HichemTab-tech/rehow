use crate::{
    domain::{
        CanonicalYamlFloatKind, UnknownMetadataEntry, UnknownYamlScalarKind, UnknownYamlValue,
        UnknownYamlValueKind,
    },
    protocol::VerificationHash,
};

use super::guide::BoundGuide;

const DOMAIN: &[u8] = b"rehow-guide-verification-v1\0";
const FIELDS: [&str; 10] = [
    "body",
    "title",
    "summary",
    "applies_to",
    "prerequisites",
    "warnings",
    "files_affected",
    "related_guide_ids",
    "references",
    "attachments",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VerificationFailure {
    Invariant,
    ResourceLimit,
}

pub(crate) fn verification_hash(
    guide: &BoundGuide,
) -> Result<VerificationHash, VerificationFailure> {
    let semantic = guide.parsed().yaml().semantic();
    let mut hasher = blake3::Hasher::new();
    hasher.update(DOMAIN);
    hasher.update(&(FIELDS.len() as u64).to_be_bytes());
    for field in FIELDS {
        hash_frame(&mut hasher, field.as_bytes());
        if field == "body" {
            hasher.update(&[1]);
            hash_frame(&mut hasher, guide.record().body());
            continue;
        }
        let value = semantic
            .entries()
            .iter()
            .find(|entry| entry.key() == field)
            .map(UnknownMetadataEntry::value);
        if let Some(value) = value {
            hasher.update(&[1]);
            let encoded = encode_value(value, false)?;
            hash_frame(&mut hasher, &encoded);
        } else {
            hasher.update(&[0]);
        }
    }

    let mut unknown = Vec::new();
    unknown
        .try_reserve_exact(guide.unknown_partition().root().len())
        .map_err(|_| VerificationFailure::ResourceLimit)?;
    for &ordinal in guide.unknown_partition().root() {
        let index = usize::try_from(ordinal).map_err(|_| VerificationFailure::Invariant)?;
        unknown.push(
            semantic
                .entries()
                .get(index)
                .ok_or(VerificationFailure::Invariant)?,
        );
    }
    unknown.sort_unstable_by(|left, right| left.key().as_bytes().cmp(right.key().as_bytes()));
    hasher.update(&(unknown.len() as u64).to_be_bytes());
    for entry in unknown {
        hash_frame(&mut hasher, entry.key().as_bytes());
        let encoded = encode_value(entry.value(), false)?;
        hash_frame(&mut hasher, &encoded);
    }
    Ok(VerificationHash::from_digest_bytes(
        *hasher.finalize().as_bytes(),
    ))
}

fn hash_frame(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn append(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), VerificationFailure> {
    output
        .try_reserve(bytes.len())
        .map_err(|_| VerificationFailure::ResourceLimit)?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn frame(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), VerificationFailure> {
    append(output, &(bytes.len() as u64).to_be_bytes())?;
    append(output, bytes)
}

fn encode_value(
    value: &UnknownYamlValue,
    mapping_key: bool,
) -> Result<Vec<u8>, VerificationFailure> {
    let mut output = Vec::new();
    match value.kind() {
        UnknownYamlValueKind::Scalar => {
            let scalar = value.as_scalar().ok_or(VerificationFailure::Invariant)?;
            match scalar.kind() {
                UnknownYamlScalarKind::Null => append(&mut output, &[0x00])?,
                UnknownYamlScalarKind::Boolean => append(
                    &mut output,
                    &[
                        0x01,
                        u8::from(scalar.as_bool().ok_or(VerificationFailure::Invariant)?),
                    ],
                )?,
                UnknownYamlScalarKind::Integer => {
                    let integer = scalar.as_integer().ok_or(VerificationFailure::Invariant)?;
                    append(&mut output, &[0x02, u8::from(integer.is_negative())])?;
                    frame(&mut output, integer.digits().as_bytes())?;
                }
                UnknownYamlScalarKind::Float => {
                    let float = scalar.as_float().ok_or(VerificationFailure::Invariant)?;
                    append(&mut output, &[0x03])?;
                    match float.kind() {
                        CanonicalYamlFloatKind::Finite => {
                            let coefficient = float
                                .coefficient_digits()
                                .ok_or(VerificationFailure::Invariant)?;
                            let exponent =
                                float.exponent10().ok_or(VerificationFailure::Invariant)?;
                            let negative =
                                float.is_negative().ok_or(VerificationFailure::Invariant)?
                                    && !(mapping_key && coefficient == "0");
                            append(&mut output, &[0x00, u8::from(negative)])?;
                            frame(&mut output, coefficient.as_bytes())?;
                            append(&mut output, &[u8::from(exponent.is_negative())])?;
                            frame(&mut output, exponent.digits().as_bytes())?;
                        }
                        CanonicalYamlFloatKind::PositiveInfinity => {
                            append(&mut output, &[0x01])?;
                        }
                        CanonicalYamlFloatKind::NegativeInfinity => {
                            append(&mut output, &[0x02])?;
                        }
                        CanonicalYamlFloatKind::NaN => append(&mut output, &[0x03])?,
                    }
                }
                UnknownYamlScalarKind::String => {
                    append(&mut output, &[0x04])?;
                    frame(
                        &mut output,
                        scalar
                            .as_str()
                            .ok_or(VerificationFailure::Invariant)?
                            .as_bytes(),
                    )?;
                }
            }
        }
        UnknownYamlValueKind::Sequence => {
            let values = value.as_sequence().ok_or(VerificationFailure::Invariant)?;
            append(&mut output, &[0x05])?;
            append(&mut output, &(values.len() as u64).to_be_bytes())?;
            for child in values {
                let encoded = encode_value(child, mapping_key)?;
                frame(&mut output, &encoded)?;
            }
        }
        UnknownYamlValueKind::Mapping => {
            let entries = value.as_mapping().ok_or(VerificationFailure::Invariant)?;
            let mut encoded = Vec::new();
            encoded
                .try_reserve_exact(entries.len())
                .map_err(|_| VerificationFailure::ResourceLimit)?;
            for entry in entries {
                encoded.push((
                    encode_value(entry.key(), true)?,
                    encode_value(entry.value(), mapping_key)?,
                ));
            }
            encoded.sort_unstable_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
            append(&mut output, &[0x06])?;
            append(&mut output, &(encoded.len() as u64).to_be_bytes())?;
            for (key, child) in encoded {
                frame(&mut output, &key)?;
                frame(&mut output, &child)?;
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::content::guide::bind_guide, parsing::parse_guide_source};

    fn guide(extra: &str, body: &str) -> BoundGuide {
        let source = format!(
            "---\nschema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\ntitle: Install tool\nsummary: Install the downloaded tool\ncategory_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\ntags: []\naliases: []\nstatus: draft\ncreated_at: 2026-09-18T10:00:00Z\nupdated_at: 2026-09-18T10:00:00Z\n{extra}---\n\n{body}"
        );
        bind_guide(parse_guide_source(source.into_bytes().into_boxed_slice()).unwrap())
            .unwrap_or_else(|_| panic!("valid Guide fixture"))
    }

    #[test]
    fn fixed_minimal_vector() {
        let hash = verification_hash(&guide("", "## Steps\n\nRun it.\n")).unwrap();
        assert_eq!(
            hash.to_string(),
            "verify:v1:0966e3140e5e3a51d09c341a6322cf751ec7be076021cbd724bd2362b84dcd54"
        );
    }

    #[test]
    fn organizational_fields_do_not_change_hash_but_content_and_unknowns_do() {
        let base = verification_hash(&guide("", "Body\n")).unwrap();
        let organization = verification_hash(&bind_guide(parse_guide_source(
            "---\nschema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\ntitle: Install tool\nsummary: Install the downloaded tool\ncategory_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5552\ntags: [linux]\naliases: [tool]\nstatus: draft\ncreated_at: 2026-09-18T10:00:00Z\nupdated_at: 2026-09-18T11:00:00Z\n---\n\nBody\n".as_bytes().into(),
        )
        .unwrap())
        .unwrap_or_else(|_| panic!("valid organizational fixture")))
        .unwrap();
        assert_eq!(base, organization);
        assert_ne!(base, verification_hash(&guide("", "Changed\n")).unwrap());
        assert_ne!(
            base,
            verification_hash(&guide("extension: value\n", "Body\n")).unwrap()
        );
    }

    #[test]
    fn absent_and_empty_optional_fields_are_distinct() {
        assert_ne!(
            verification_hash(&guide("", "Body\n")).unwrap(),
            verification_hash(&guide("warnings: []\n", "Body\n")).unwrap()
        );
    }

    #[test]
    fn unknown_mapping_order_is_irrelevant_and_value_negative_zero_is_retained() {
        let first =
            verification_hash(&guide("extension:\n  alpha: 1\n  zero: -0.0\n", "Body\n")).unwrap();
        let reordered =
            verification_hash(&guide("extension:\n  zero: -0.0\n  alpha: 1\n", "Body\n")).unwrap();
        let positive =
            verification_hash(&guide("extension:\n  alpha: 1\n  zero: 0.0\n", "Body\n")).unwrap();
        assert_eq!(first, reordered);
        assert_ne!(first, positive);
        assert_eq!(
            first.to_string(),
            "verify:v1:70e8fae2032f7c823a1f687f1d42ac5dbd82d52245122272faf86f52e2b43786"
        );
    }

    #[test]
    fn unknown_root_order_and_mapping_key_signed_zero_are_canonical() {
        assert_eq!(
            verification_hash(&guide("zeta: 1\nalpha: 2\n", "Body\n")).unwrap(),
            verification_hash(&guide("alpha: 2\nzeta: 1\n", "Body\n")).unwrap()
        );
        assert_eq!(
            verification_hash(&guide("extension:\n  ? -0.0\n  : value\n", "Body\n")).unwrap(),
            verification_hash(&guide("extension:\n  ? 0.0\n  : value\n", "Body\n")).unwrap()
        );
    }

    #[test]
    fn nested_known_extensions_are_verification_relevant() {
        let first = verification_hash(&guide(
            "applies_to:\n  - platform: Linux\n    future: one\nzeta: 1\nalpha: 2\n",
            "Body\n",
        ))
        .unwrap();
        let second = verification_hash(&guide(
            "applies_to:\n  - future: two\n    platform: Linux\n",
            "Body\n",
        ))
        .unwrap();
        assert_ne!(first, second);
        assert_eq!(
            first.to_string(),
            "verify:v1:a122dc67943fc3c5a0bc438b5e1089512c5f8b672117ca21d35300cbd460c30b"
        );
    }
}
