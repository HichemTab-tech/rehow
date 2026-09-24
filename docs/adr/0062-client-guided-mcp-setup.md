# ADR 0062: Client-guided MCP setup

- Status: Accepted and implemented
- Date: 2026-09-24

## Context

The separately usable `rehow-mcp` binary was documented but difficult to discover from the desktop app. Codex can launch a local stdio server directly. Hosted ChatGPT cannot directly start a process on the user's machine and therefore needs a supported private transport.

## Decision

Settings gains an **AI & MCP** section that generates copyable, least-authority setup templates for Codex and ChatGPT. The user chooses a bounded Workspace alias and one explicit permission profile: read-only, narrow write, broad Workspace mutation, or Git mutation. Broad and Git profiles include their required narrow-write grant.

The renderer receives no absolute Workspace path. Generated Bash commands prompt for the path and tunnel ID with `read -r`, keep expansions quoted, and use Bash `%q` argv escaping for the tunnel's command string. Arbitrary path text is never inserted into shell source. Newline-bearing paths are outside this interactive setup helper and remain configurable directly in a client's structured configuration. Rehow does not edit another client's configuration, start a persistent service, create an OpenAI tunnel, handle an API key, or store credentials.

Codex uses `codex mcp add` to register `/usr/bin/rehow-mcp` as a local stdio server. ChatGPT uses OpenAI Secure MCP Tunnel: `tunnel-client` launches the same stdio command, maintains an outbound HTTPS connection, and must remain running. The UI also identifies the required tunnel ID, runtime API key, ChatGPT Developer Mode, doctor/run commands, and ChatGPT Plugins connection step.

## Consequences

- The desktop app makes MCP discoverable while keeping startup authority and credentials in the client/operator boundary.
- Read-only remains the default; no permission is inferred from the open desktop Workspace.
- Prompted, quoted data avoids expanding renderer filesystem authority or creating shell-injection guidance solely for convenience.
- A future one-click setup would require separate native clipboard/config/service and credential-lifecycle decisions.
