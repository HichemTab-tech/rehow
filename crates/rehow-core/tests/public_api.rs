#[test]
fn public_surface_is_deliberate() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass.rs");
    cases.compile_fail("tests/ui/private_*.rs");
}
