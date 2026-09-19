use super::{CLONE_DISCOVERY_OUTPUT_LIMIT, run_bounded_git_command_with_input};
use std::{
    ffi::{OsStr, OsString},
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, ExitCode, Stdio},
    time::Duration,
};

#[allow(
    dead_code,
    reason = "mounted by the next authenticated HTTPS operation gate"
)]
pub(super) const SANDBOX_CREDENTIAL_PROXY: &str = "/home/rehow/bin/rehow-git-credential";
pub(super) const SANDBOX_CREDENTIAL_HELPER: &str = "/home/rehow/bin/credential-helper";
const MAX_CREDENTIAL_RECORD_BYTES: usize = 32 * 1024;
const MAX_CREDENTIAL_LINES: usize = 64;
const MAX_CREDENTIAL_KEY_BYTES: usize = 32;
const MAX_CREDENTIAL_VALUE_BYTES: usize = 8 * 1024;
const HELPER_TIMEOUT: Duration = Duration::from_secs(10);
const EXPECTED_HOST_ENV: &str = "REHOW_CREDENTIAL_HOST";
const HELPER_KIND_ENV: &str = "REHOW_CREDENTIAL_HELPER_KIND";
type CredentialField<'a> = (&'a [u8], &'a [u8]);

#[derive(Clone, Copy, Eq, PartialEq)]
enum Operation {
    Get,
    Store,
    Erase,
}

impl Operation {
    fn parse(value: &OsStr) -> Option<Self> {
        match value.to_str()? {
            "get" => Some(Self::Get),
            "store" => Some(Self::Store),
            "erase" => Some(Self::Erase),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Store => "store",
            Self::Erase => "erase",
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum HelperKind {
    Direct,
    GithubCli,
}

impl HelperKind {
    fn parse(value: &OsStr) -> Option<Self> {
        match value.to_str()? {
            "direct" => Some(Self::Direct),
            "githubCli" => Some(Self::GithubCli),
            _ => None,
        }
    }
}

#[derive(Debug)]
enum ProxyError {
    Invalid,
    Helper,
}

/// Entrypoint for the dedicated credential proxy. It intentionally emits no
/// diagnostics: helper stderr and all rejected credential data remain private.
pub fn run_git_credential_proxy() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let expected_host = std::env::var_os(EXPECTED_HOST_ENV);
    let helper_kind = std::env::var_os(HELPER_KIND_ENV);
    let mut input = Vec::new();
    if std::io::stdin()
        .take((MAX_CREDENTIAL_RECORD_BYTES + 1) as u64)
        .read_to_end(&mut input)
        .is_err()
        || input.len() > MAX_CREDENTIAL_RECORD_BYTES
    {
        return ExitCode::from(255);
    }
    let result = proxy_exchange(
        &arguments,
        expected_host.as_deref(),
        helper_kind.as_deref(),
        &input,
        Path::new(SANDBOX_CREDENTIAL_HELPER),
    );
    let Ok(output) = result else {
        return ExitCode::from(255);
    };
    let mut stdout = std::io::stdout().lock();
    if stdout.write_all(&output).is_err() || stdout.flush().is_err() {
        ExitCode::from(255)
    } else {
        ExitCode::SUCCESS
    }
}

fn proxy_exchange(
    arguments: &[OsString],
    expected_host: Option<&OsStr>,
    helper_kind: Option<&OsStr>,
    input: &[u8],
    helper_path: &Path,
) -> Result<Vec<u8>, ProxyError> {
    let [operation] = arguments else {
        return Err(ProxyError::Invalid);
    };
    let operation = Operation::parse(operation).ok_or(ProxyError::Invalid)?;
    let expected_host = expected_host
        .and_then(OsStr::to_str)
        .filter(|host| valid_expected_host(host))
        .ok_or(ProxyError::Invalid)?;
    let helper_kind = helper_kind
        .and_then(HelperKind::parse)
        .ok_or(ProxyError::Invalid)?;
    validate_request(input, operation, expected_host.as_bytes())?;
    let output = run_bounded_git_command_with_input(
        helper_command(helper_path, helper_kind, operation),
        HELPER_TIMEOUT,
        MAX_CREDENTIAL_RECORD_BYTES,
        CLONE_DISCOVERY_OUTPUT_LIMIT,
        Some(input.to_vec()),
    )
    .map_err(|_| ProxyError::Helper)?
    .stdout;
    validate_response(&output, operation)?;
    Ok(output)
}

fn valid_expected_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 512
        && host.bytes().all(|byte| {
            byte.is_ascii()
                && !byte.is_ascii_control()
                && !byte.is_ascii_whitespace()
                && byte != b'='
        })
}

fn helper_command(helper_path: &Path, kind: HelperKind, operation: Operation) -> Command {
    let mut command = Command::new(helper_path);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("HOME", "/home/rehow")
        .env("XDG_CONFIG_HOME", "/home/rehow/.config")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "false")
        .env("SSH_ASKPASS", "false");
    if kind == HelperKind::GithubCli {
        command.args(["auth", "git-credential"]);
    }
    command
        .arg(operation.as_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    command
}

fn validate_request(
    input: &[u8],
    operation: Operation,
    expected_host: &[u8],
) -> Result<(), ProxyError> {
    let fields = parse_record(input, true)?;
    let protocol = one_value(&fields, b"protocol").ok_or(ProxyError::Invalid)?;
    let host = one_value(&fields, b"host").ok_or(ProxyError::Invalid)?;
    if protocol != b"https" || host != expected_host {
        return Err(ProxyError::Invalid);
    }
    if operation == Operation::Get && fields.iter().any(|(key, _)| is_secret_key(key)) {
        return Err(ProxyError::Invalid);
    }
    Ok(())
}

fn validate_response(output: &[u8], operation: Operation) -> Result<(), ProxyError> {
    if output.is_empty() {
        return Ok(());
    }
    if operation != Operation::Get {
        return Err(ProxyError::Invalid);
    }
    let fields = parse_record(output, false)?;
    if fields.iter().any(|(key, _)| !response_key(key)) {
        return Err(ProxyError::Invalid);
    }
    Ok(())
}

fn parse_record(
    input: &[u8],
    require_blank_terminator: bool,
) -> Result<Vec<CredentialField<'_>>, ProxyError> {
    if input.len() > MAX_CREDENTIAL_RECORD_BYTES
        || input.iter().any(|byte| *byte == 0 || *byte == b'\r')
    {
        return Err(ProxyError::Invalid);
    }
    let body = if require_blank_terminator {
        input.strip_suffix(b"\n\n").ok_or(ProxyError::Invalid)?
    } else if let Some(body) = input.strip_suffix(b"\n\n") {
        body
    } else {
        input.strip_suffix(b"\n").ok_or(ProxyError::Invalid)?
    };
    if body.is_empty() {
        return Err(ProxyError::Invalid);
    }
    let mut fields = Vec::new();
    for line in body.split(|byte| *byte == b'\n') {
        if line.is_empty() || fields.len() >= MAX_CREDENTIAL_LINES {
            return Err(ProxyError::Invalid);
        }
        let split = line
            .iter()
            .position(|byte| *byte == b'=')
            .ok_or(ProxyError::Invalid)?;
        let key = &line[..split];
        let value = &line[split + 1..];
        if key.is_empty()
            || key.len() > MAX_CREDENTIAL_KEY_BYTES
            || value.len() > MAX_CREDENTIAL_VALUE_BYTES
            || !known_key(key)
            || value.iter().any(|byte| byte.is_ascii_control())
            || (!repeatable_key(key) && fields.iter().any(|(seen, _)| *seen == key))
        {
            return Err(ProxyError::Invalid);
        }
        fields.push((key, value));
    }
    Ok(fields)
}

fn known_key(key: &[u8]) -> bool {
    matches!(
        key,
        b"protocol"
            | b"host"
            | b"path"
            | b"username"
            | b"password"
            | b"password_expiry_utc"
            | b"oauth_refresh_token"
            | b"authtype"
            | b"credential"
            | b"continue"
            | b"capability[]"
            | b"state[]"
            | b"wwwauth[]"
    )
}

fn repeatable_key(key: &[u8]) -> bool {
    matches!(key, b"capability[]" | b"state[]" | b"wwwauth[]")
}

fn response_key(key: &[u8]) -> bool {
    matches!(
        key,
        b"username"
            | b"password"
            | b"password_expiry_utc"
            | b"oauth_refresh_token"
            | b"authtype"
            | b"credential"
            | b"continue"
            | b"capability[]"
            | b"state[]"
    )
}

fn is_secret_key(key: &[u8]) -> bool {
    matches!(key, b"password" | b"oauth_refresh_token" | b"credential")
}

fn one_value<'a>(fields: &[CredentialField<'a>], key: &[u8]) -> Option<&'a [u8]> {
    fields
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(*value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn request_grammar_binds_https_host_operation_and_secrets() {
        let safe = b"protocol=https\nhost=example.invalid\npath=team/rehow.git\nusername=user\ncapability[]=authtype\n\n";
        assert!(validate_request(safe, Operation::Get, b"example.invalid").is_ok());
        assert!(validate_request(safe, Operation::Store, b"example.invalid").is_ok());
        for hostile in [
            b"protocol=http\nhost=example.invalid\n\n".as_slice(),
            b"protocol=https\nhost=other.invalid\n\n".as_slice(),
            b"protocol=https\nprotocol=https\nhost=example.invalid\n\n".as_slice(),
            b"protocol=https\nhost=example.invalid\nunknown=value\n\n".as_slice(),
            b"protocol=https\nhost=example.invalid\npassword=secret\n\n".as_slice(),
            b"protocol=https\nhost=example.invalid\r\n\r\n".as_slice(),
            b"protocol=https\nhost=example.invalid\n".as_slice(),
        ] {
            assert!(validate_request(hostile, Operation::Get, b"example.invalid").is_err());
        }
        assert!(
            validate_request(
                b"protocol=https\nhost=example.invalid\npassword=secret\n\n",
                Operation::Store,
                b"example.invalid"
            )
            .is_ok()
        );
        let oversized = vec![b'x'; MAX_CREDENTIAL_RECORD_BYTES + 1];
        assert!(validate_request(&oversized, Operation::Get, b"example.invalid").is_err());

        let mut exact_lines = b"protocol=https\nhost=example.invalid\n".to_vec();
        for _ in 0..62 {
            exact_lines.extend_from_slice(b"capability[]=state\n");
        }
        exact_lines.push(b'\n');
        assert!(validate_request(&exact_lines, Operation::Get, b"example.invalid").is_ok());
        exact_lines.splice(
            exact_lines.len() - 1..exact_lines.len() - 1,
            b"capability[]=state\n".iter().copied(),
        );
        assert!(validate_request(&exact_lines, Operation::Get, b"example.invalid").is_err());
    }

    #[test]
    fn response_grammar_is_bounded_and_only_get_may_return_fields() {
        let response = b"username=user\npassword=secret\n\n";
        assert!(validate_response(response, Operation::Get).is_ok());
        assert!(validate_response(response, Operation::Store).is_err());
        assert!(validate_response(b"", Operation::Store).is_ok());
        assert!(validate_response(b"password=one\npassword=two\n", Operation::Get).is_err());
        assert!(validate_response(b"url=https://secret\n", Operation::Get).is_err());
        assert!(validate_response(b"protocol=https\n", Operation::Get).is_err());
        assert!(validate_response(b"password=secret\r\n", Operation::Get).is_err());
        assert!(
            validate_response(&vec![b'x'; MAX_CREDENTIAL_RECORD_BYTES + 1], Operation::Get)
                .is_err()
        );
        let canary = b"unknown=credential-canary\n";
        let error = validate_response(canary, Operation::Get).unwrap_err();
        assert!(!format!("{error:?}").contains("credential-canary"));
    }

    #[test]
    fn command_is_direct_fixed_and_environment_is_reconstructed() {
        let direct = helper_command(
            Path::new(SANDBOX_CREDENTIAL_HELPER),
            HelperKind::Direct,
            Operation::Get,
        );
        assert_eq!(direct.get_program(), SANDBOX_CREDENTIAL_HELPER);
        assert_eq!(
            direct
                .get_args()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["get"]
        );
        let environment = direct
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<HashMap<_, _>>();
        assert_eq!(environment["HOME"].as_deref(), Some("/home/rehow"));
        assert_eq!(environment["GIT_TERMINAL_PROMPT"].as_deref(), Some("0"));
        assert_eq!(environment.len(), 7);

        let gh = helper_command(
            Path::new(SANDBOX_CREDENTIAL_HELPER),
            HelperKind::GithubCli,
            Operation::Erase,
        );
        assert_eq!(
            gh.get_args()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["auth", "git-credential", "erase"]
        );
    }

    #[test]
    fn real_direct_process_empty_lookup_uses_no_user_environment() {
        let helper = Path::new("/usr/bin/true");
        assert!(helper.is_file());
        let output = proxy_exchange(
            &[OsString::from("get")],
            Some(OsStr::new("rehow.invalid")),
            Some(OsStr::new("direct")),
            b"protocol=https\nhost=rehow.invalid\n\n",
            helper,
        )
        .unwrap();
        assert!(output.is_empty());
        for (arguments, host, kind) in [
            (
                Vec::new(),
                Some(OsStr::new("rehow.invalid")),
                Some(OsStr::new("direct")),
            ),
            (
                vec![OsString::from("get"), OsString::from("extra")],
                Some(OsStr::new("rehow.invalid")),
                Some(OsStr::new("direct")),
            ),
            (
                vec![OsString::from("get")],
                None,
                Some(OsStr::new("direct")),
            ),
            (
                vec![OsString::from("get")],
                Some(OsStr::new("bad host")),
                Some(OsStr::new("direct")),
            ),
            (
                vec![OsString::from("get")],
                Some(OsStr::new("rehow.invalid")),
                Some(OsStr::new("other")),
            ),
        ] {
            assert!(
                proxy_exchange(
                    &arguments,
                    host,
                    kind,
                    b"protocol=https\nhost=rehow.invalid\n\n",
                    helper,
                )
                .is_err()
            );
        }
    }
}
