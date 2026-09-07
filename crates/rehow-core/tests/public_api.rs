#[test]
fn public_surface_is_deliberate() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass.rs");
    cases.compile_fail("tests/ui/private_*.rs");
    cases.compile_fail("tests/ui/phase1c_parser_module_is_private.rs");
    cases.compile_fail("tests/ui/phase1c_parse_entries_are_private.rs");
    cases.compile_fail("tests/ui/phase1c_artifact_fields_are_private.rs");
    cases.compile_fail("tests/ui/phase1c_coordinate_constructors_are_private.rs");
    cases.compile_fail("tests/ui/phase1c_round_trip_report_cannot_be_forged.rs");
    cases.compile_fail("tests/ui/phase1c_test_controls_absent.rs");
}
