pub const ADAPTER_NAME: &str = "rehow-mcp";

#[cfg(test)]
mod tests {
    use super::ADAPTER_NAME;

    #[test]
    fn adapter_name_is_stable() {
        assert_eq!(ADAPTER_NAME, "rehow-mcp");
    }
}
