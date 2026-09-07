fn main() {
    let _ = rehow_core::parsing::RoundTripReport {
        issues: Vec::new().into_boxed_slice(),
        total_issues: 0,
        truncated: false,
    };
}
