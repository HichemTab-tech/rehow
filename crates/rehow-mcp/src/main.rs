use rehow_mcp::{StdioTransport, open_server, parse_launch_arguments};
use rmcp::ServiceExt;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let config = match parse_launch_arguments(std::env::args_os().skip(1)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("rehow-mcp: {error}");
            std::process::exit(2);
        }
    };
    let server = match open_server(config) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("rehow-mcp: {error}");
            std::process::exit(2);
        }
    };
    let transport = StdioTransport::new(tokio::io::stdin(), tokio::io::stdout());
    match server.serve(transport).await {
        Ok(running) => {
            let _ = running.waiting().await;
        }
        Err(_) => {
            eprintln!("rehow-mcp: protocol startup failed");
            std::process::exit(1);
        }
    }
}
