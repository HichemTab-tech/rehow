use super::super::reconcile::{ScalarPresentationStyle, SourceTag};
use super::super::scalar::{NumericMeter, grammar_form_for_test, resolve};
use super::super::{ParseFailure, ParseFailureKind, ParseLimitScope};
use super::parse_yaml_document;
use crate::domain::{UnknownYamlScalarKind, UnknownYamlValueKind};

fn resolve_error(text: String, meter: &mut NumericMeter) -> ParseFailure {
    match resolve(
        text,
        ScalarPresentationStyle::Plain,
        SourceTag::Implicit,
        meter,
    ) {
        Err(error) => error,
        Ok(_) => panic!("numeric source unexpectedly accepted"),
    }
}

#[test]
fn yaml_12_core_scalar_resolution_matrix() {
    let source = b"nullv: null\nboolv: TRUE\ndec: -0012\noct: 0o17\nhex: 0x10\nfloat: 10e-1\nlegacy: yes\nquoted: '12'\ninf: -.Inf\nnan: .NAN\n";
    let parsed = parse_yaml_document(source.to_vec().into_boxed_slice()).unwrap();
    let entries = parsed.yaml().semantic().entries();
    assert_eq!(entries.len(), 10);
    let kinds: Vec<_> = entries
        .iter()
        .map(|e| {
            assert!(e.value().kind() == UnknownYamlValueKind::Scalar);
            e.value().as_scalar().unwrap().kind()
        })
        .collect();
    assert!(
        kinds
            == [
                UnknownYamlScalarKind::Null,
                UnknownYamlScalarKind::Boolean,
                UnknownYamlScalarKind::Integer,
                UnknownYamlScalarKind::Integer,
                UnknownYamlScalarKind::Integer,
                UnknownYamlScalarKind::Float,
                UnknownYamlScalarKind::String,
                UnknownYamlScalarKind::String,
                UnknownYamlScalarKind::Float,
                UnknownYamlScalarKind::Float
            ]
    );
}

#[test]
fn arbitrary_radix_and_signed_zero_normalize() {
    let parsed = parse_yaml_document(
        b"hex: 0xFFFFFFFFFFFFFFFFFFFFFFFF\nzero: -000\n"
            .to_vec()
            .into_boxed_slice(),
    )
    .unwrap();
    let hex = parsed.yaml().semantic().entries()[0]
        .value()
        .as_scalar()
        .unwrap()
        .as_integer()
        .unwrap();
    assert_eq!(hex.digits(), "79228162514264337593543950335");
    let zero = parsed.yaml().semantic().entries()[1]
        .value()
        .as_scalar()
        .unwrap()
        .as_integer()
        .unwrap();
    assert_eq!(zero.digits(), "0");
    assert!(!zero.is_negative());
}

#[test]
fn literal_core_grammar_accepts_only_the_normative_rows() {
    let implicit = SourceTag::Implicit;
    let cases = [
        ("", 0),
        ("null", 0),
        ("NULL", 0),
        ("~", 0),
        ("true", 1),
        ("FALSE", 1),
        ("+001", 2),
        ("-0", 2),
        ("0o17", 3),
        ("0xAf", 4),
        (".5", 5),
        ("1.", 5),
        ("1e+2", 5),
        ("-1.25E-3", 5),
        ("+.INF", 6),
        ("-.Inf", 7),
        (".NaN", 8),
        ("yes", 9),
        ("OFF", 9),
        ("1_000", 9),
        ("12:34", 9),
        ("2026-09-07", 9),
        ("+0o7", 9),
        ("-0xF", 9),
        (".", 9),
        ("1e", 9),
        (".e1", 9),
    ];
    for (text, expected) in cases {
        assert_eq!(
            grammar_form_for_test(text, true, implicit),
            expected,
            "{text:?}"
        );
    }
    assert_eq!(grammar_form_for_test("12", false, implicit), 9);
    assert_eq!(grammar_form_for_test("true", false, implicit), 9);
    assert_eq!(grammar_form_for_test("true", true, SourceTag::Boolean), 1);
    assert_eq!(grammar_form_for_test("yes", true, SourceTag::Boolean), 10);
    assert_eq!(grammar_form_for_test("0xF", true, SourceTag::Integer), 4);
    assert_eq!(grammar_form_for_test("+0xF", true, SourceTag::Integer), 10);
    assert_eq!(grammar_form_for_test("12", true, SourceTag::Float), 10);
    assert_eq!(grammar_form_for_test("1e2", true, SourceTag::Float), 5);
}

#[test]
fn numeric_meter_exact_one_over_and_overflow_are_ordered() {
    const WORK: u64 = 16 * 1024 * 1024;
    const DIGITS: u64 = 80 * 1024;

    let mut exact = NumericMeter::new();
    exact.seed_for_test(WORK - 8, DIGITS - 1);
    assert!(
        resolve(
            "1".to_owned(),
            ScalarPresentationStyle::Plain,
            SourceTag::Implicit,
            &mut exact
        )
        .is_ok()
    );
    assert_eq!(exact.values_for_test(), (WORK, DIGITS));

    let mut work_over = NumericMeter::new();
    work_over.seed_for_test(WORK - 7, 0);
    let error = resolve_error("1".to_owned(), &mut work_over);
    assert_eq!(error.kind(), ParseFailureKind::NumericWorkLimit);
    assert_eq!(error.limit().unwrap().observed_at_least(), WORK + 1);

    let mut digit_over = NumericMeter::new();
    digit_over.seed_for_test(0, DIGITS);
    let error = resolve_error("1".to_owned(), &mut digit_over);
    assert_eq!(error.kind(), ParseFailureKind::NumericDigitLimit);
    assert_eq!(error.limit().unwrap().observed_at_least(), DIGITS + 1);

    let mut arithmetic = NumericMeter::new();
    arithmetic.seed_for_test(u64::MAX, 0);
    let error = resolve_error("1".to_owned(), &mut arithmetic);
    assert_eq!(error.kind(), ParseFailureKind::MeterArithmeticOverflow);
    assert_eq!(error.limit().unwrap().scope(), ParseLimitScope::NumericWork);
}

#[test]
fn radix_quadratic_precharge_refuses_before_conversion() {
    let mut meter = NumericMeter::new();
    let source = format!("0x{}", "F".repeat(4_095));
    let error = resolve_error(source, &mut meter);
    assert_eq!(error.kind(), ParseFailureKind::NumericWorkLimit);
    assert_eq!(meter.values_for_test(), (0, 0));
}

#[test]
fn finite_float_normalization_keeps_arbitrary_exponents_and_signed_zero() {
    let source =
        b"large: 1200.00e999999999999999999999999999999\nzero: -0.000e-999999999999999999999999\n";
    let parsed = parse_yaml_document(source.to_vec().into_boxed_slice()).unwrap();
    let large = parsed.yaml().semantic().entries()[0]
        .value()
        .as_scalar()
        .unwrap()
        .as_float()
        .unwrap();
    assert_eq!(large.coefficient_digits(), Some("12"));
    assert_eq!(
        large.exponent10().unwrap().digits(),
        "1000000000000000000000000000001"
    );
    let zero = parsed.yaml().semantic().entries()[1]
        .value()
        .as_scalar()
        .unwrap()
        .as_float()
        .unwrap();
    assert_eq!(zero.coefficient_digits(), Some("0"));
    assert_eq!(zero.exponent10().unwrap().digits(), "0");
    assert_eq!(zero.is_negative(), Some(true));
}
