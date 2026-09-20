mod approval;
// The gate is intentionally unreachable from MCP until an operation-specific plan tool lands.
#[allow(dead_code)]
mod approval_gate;
mod cli;
mod server;
mod transport;

pub use approval::run_approval_helper;
pub use cli::{LaunchConfig, LaunchError, WorkspaceArgument, parse_launch_arguments};
pub use server::{McpServer, OpenError, open_server};
pub use transport::{MAX_INPUT_FRAME_BYTES, StdioTransport};

pub const ADAPTER_NAME: &str = "rehow-mcp";
pub const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::{ADAPTER_NAME, ADAPTER_VERSION};

    #[test]
    fn adapter_name_is_stable() {
        assert_eq!(ADAPTER_NAME, "rehow-mcp");
        assert!(!ADAPTER_VERSION.is_empty());
    }
}
