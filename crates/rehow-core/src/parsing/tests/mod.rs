use super::{ParseFailureKind, parse_guide_source, parse_yaml_document};

mod comments;
mod dependency_surface;
mod faults;
mod limits;
mod precedence;
mod presentation;
mod properties;
mod reconcile;
mod scalar;
mod semantic;
mod source;

fn failure(bytes: &[u8]) -> super::ParseFailure {
    match parse_yaml_document(bytes.to_vec().into_boxed_slice()) {
        Err(error) => error,
        Ok(_) => panic!("malformed fixture accepted"),
    }
}

struct FixtureRecipe {
    bytes: Box<[u8]>,
    metadata: Vec<(&'static str, &'static str)>,
}

fn materialize_fixture_recipe(descriptor: &'static str) -> FixtureRecipe {
    fn hex_bytes(value: &str) -> Vec<u8> {
        assert_eq!(value.len() % 2, 0);
        value
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                let text = core::str::from_utf8(pair).unwrap();
                u8::from_str_radix(text, 16).unwrap()
            })
            .collect()
    }

    let mut bytes = Vec::new();
    let mut metadata = Vec::new();
    for line in descriptor.lines().filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once('=').unwrap();
        match key {
            "hex" => bytes.extend(hex_bytes(value)),
            "repeat" => {
                let (fragment, count) = value.split_once(',').unwrap();
                let fragment = hex_bytes(fragment);
                let count = count.parse::<usize>().unwrap();
                let added = fragment.len().checked_mul(count).unwrap();
                bytes.try_reserve_exact(added).unwrap();
                for _ in 0..count {
                    bytes.extend_from_slice(&fragment);
                }
            }
            "nested_depth" => {
                let depth = value.parse::<usize>().unwrap();
                bytes.extend_from_slice(b"value: ");
                bytes.extend(core::iter::repeat_n(b'[', depth));
                bytes.push(b'x');
                bytes.extend(core::iter::repeat_n(b']', depth));
                bytes.push(b'\n');
            }
            "unique_mapping" => {
                for index in 0..value.parse::<usize>().unwrap() {
                    bytes.extend_from_slice(format!("k{index}: x\n").as_bytes());
                }
            }
            "sequence_items" => {
                bytes.extend_from_slice(b"values:\n");
                for _ in 0..value.parse::<usize>().unwrap() {
                    bytes.extend_from_slice(b"  - x\n");
                }
            }
            "node_last_group" => {
                let last = value.parse::<usize>().unwrap();
                bytes.extend_from_slice(b"values:\n");
                for group in 0..10 {
                    bytes.extend_from_slice(b"  -\n");
                    let items = if group == 9 { last } else { 2_000 };
                    for _ in 0..items {
                        bytes.extend_from_slice(b"    - x\n");
                    }
                }
            }
            _ => metadata.push((key, value)),
        }
    }
    FixtureRecipe {
        bytes: bytes.into_boxed_slice(),
        metadata,
    }
}

impl FixtureRecipe {
    fn fact(&self, name: &str) -> Option<&str> {
        self.metadata
            .iter()
            .find_map(|(key, value)| (*key == name).then_some(*value))
    }
}

#[test]
fn parses_basic_whole_yaml_and_guide() {
    let document = parse_yaml_document(b"title: Rehow\ncount: 12\n".to_vec().into_boxed_slice())
        .expect("basic mapping must parse");
    assert_eq!(document.source_bytes(), b"title: Rehow\ncount: 12\n");
    assert_eq!(document.yaml().semantic().entries().len(), 2);

    let guide = parse_guide_source(
        b"---\ntitle: Rehow\n---\n\nBody\n"
            .to_vec()
            .into_boxed_slice(),
    )
    .expect("basic Guide must parse");
    assert_eq!(guide.body_bytes(), b"Body\n");
}

#[test]
fn phase1b_meter_bridge_is_test_only_and_poisoning() {
    use crate::domain::{
        CanonicalYamlInteger, UnknownYamlBuildErrorKind, UnknownYamlDocumentBuilder,
        UnknownYamlEvent, UnknownYamlTestMeter,
    };

    fn expect_overflow(
        builder: &mut UnknownYamlDocumentBuilder,
        meter: UnknownYamlTestMeter,
        event: UnknownYamlEvent,
    ) {
        assert!(builder.set_test_meter(meter, usize::MAX).is_ok());
        let error = match builder.push_event(event) {
            Err(error) => error,
            Ok(()) => panic!("seeded builder meter unexpectedly accepted"),
        };
        assert!(error.kind() == UnknownYamlBuildErrorKind::ArithmeticOverflow);
        let follow_up = builder
            .push_event(UnknownYamlEvent::Null)
            .expect_err("arithmetic overflow must poison the real builder");
        assert!(follow_up.kind() == UnknownYamlBuildErrorKind::Poisoned);
        assert!(matches!(
            builder.set_test_meter(UnknownYamlTestMeter::Nodes, 0),
            Err(UnknownYamlBuildErrorKind::Poisoned)
        ));
    }

    let mut nodes = UnknownYamlDocumentBuilder::new();
    expect_overflow(
        &mut nodes,
        UnknownYamlTestMeter::Nodes,
        UnknownYamlEvent::BeginMapping,
    );

    let mut digits = UnknownYamlDocumentBuilder::new();
    digits.push_event(UnknownYamlEvent::BeginMapping).unwrap();
    digits
        .push_event(UnknownYamlEvent::String("key".to_owned()))
        .unwrap();
    expect_overflow(
        &mut digits,
        UnknownYamlTestMeter::NumericDigits,
        UnknownYamlEvent::Integer(CanonicalYamlInteger::new(false, "1".to_owned()).unwrap()),
    );

    let mut encoding = UnknownYamlDocumentBuilder::new();
    encoding.push_event(UnknownYamlEvent::BeginMapping).unwrap();
    expect_overflow(
        &mut encoding,
        UnknownYamlTestMeter::SemanticEncoding,
        UnknownYamlEvent::String("key".to_owned()),
    );

    let mut comparison = UnknownYamlDocumentBuilder::new();
    for event in [
        UnknownYamlEvent::BeginMapping,
        UnknownYamlEvent::String("a".to_owned()),
        UnknownYamlEvent::String("one".to_owned()),
        UnknownYamlEvent::String("b".to_owned()),
        UnknownYamlEvent::String("two".to_owned()),
    ] {
        comparison.push_event(event).unwrap();
    }
    expect_overflow(
        &mut comparison,
        UnknownYamlTestMeter::SemanticComparison,
        UnknownYamlEvent::EndMapping,
    );
}

#[test]
fn source_limits_and_invalid_utf8_are_sanitized() {
    let oversized = vec![b'a'; 256 * 1024 + 1].into_boxed_slice();
    let error = match parse_yaml_document(oversized) {
        Err(error) => error,
        Ok(_) => panic!("oversized input accepted"),
    };
    assert_eq!(error.kind(), ParseFailureKind::SourceTooLarge);
    let error = match parse_yaml_document(vec![0xff].into_boxed_slice()) {
        Err(error) => error,
        Ok(_) => panic!("invalid UTF-8 accepted"),
    };
    assert_eq!(error.kind(), ParseFailureKind::InvalidUtf8);
}
