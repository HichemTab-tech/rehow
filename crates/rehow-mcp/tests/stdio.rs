use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

fn fixture_workspace(root: &std::path::Path) {
    fs::create_dir_all(root.join("categories/general")).unwrap();
    fs::write(
        root.join("rehow.yaml"),
        "schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Process Test\ncreated_at: \"2026-09-20T00:00:00Z\"\nupdated_at: \"2026-09-20T00:00:00Z\"\n",
    )
    .unwrap();
    fs::write(
        root.join("categories/general/category.yaml"),
        "schema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e11\nname: General\norder: 1000\nstatus: active\ncreated_at: \"2026-09-20T00:00:00Z\"\nupdated_at: \"2026-09-20T00:00:00Z\"\n",
    )
    .unwrap();
}

#[test]
fn binary_keeps_stdout_protocol_only_and_paths_private() {
    let workspace = tempfile::tempdir().unwrap();
    fixture_workspace(workspace.path());
    let mut child = Command::new(env!("CARGO_BIN_EXE_rehow-mcp"))
        .args([
            "--workspace",
            &format!("process={}", workspace.path().display()),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "server/discover",
        "params": {"_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {"name": "process-test", "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {}
        }}
    });
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{request}").unwrap();
    stdin.flush().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let response: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(response["id"], 1);
    assert!(!line.contains(workspace.path().to_str().unwrap()));
    drop(stdin);
    drop(stdout);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
}
