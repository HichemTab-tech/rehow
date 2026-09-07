use super::{ParseFailureKind, failure, parse_yaml_document};

#[test]
fn mapping_root_and_string_top_keys_are_required() {
    assert_eq!(
        failure(b"scalar\n").kind(),
        ParseFailureKind::RootMustBeMapping
    );
    assert_eq!(
        failure(b"[a, b]\n").kind(),
        ParseFailureKind::RootMustBeMapping
    );
    assert_eq!(
        failure(b"{[a]: b}\n").kind(),
        ParseFailureKind::TopLevelKeyMustBeString
    );
}

#[test]
fn duplicate_semantic_keys_are_rejected() {
    assert_eq!(
        failure(b"1: a\n01: b\n").kind(),
        ParseFailureKind::TopLevelKeyMustBeString
    );
    assert_eq!(
        failure(b"a: 1\na: 2\n").kind(),
        ParseFailureKind::DuplicateMappingKey
    );
    assert!(parse_yaml_document(b"a: 1\nA: 2\n".to_vec().into_boxed_slice()).is_ok());
}
