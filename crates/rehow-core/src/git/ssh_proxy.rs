use super::{valid_remote_host, valid_remote_path, valid_ssh_user};
use std::{
    ffi::{OsStr, OsString},
    process::{Command, ExitCode, Stdio},
};

const AGENT_SOCKET: &str = "/run/rehow-agent/agent.sock";
const SSH_CLIENT: &str = "/home/rehow/bin/ssh";
const USER_KNOWN_HOSTS_OPTION: &str = "UserKnownHostsFile=/home/rehow/.ssh/known_hosts";
const SYSTEM_KNOWN_HOSTS_OPTION: &str = "GlobalKnownHostsFile=/home/rehow/.ssh/system_known_hosts";

#[derive(Debug, Eq, PartialEq)]
struct ProxyTarget {
    port: Option<u16>,
    destination: String,
    remote_command: String,
}

/// Entry point for the dedicated `rehow-git-ssh` binary. It accepts only
/// Git's bounded upload-pack or receive-pack argv and delegates to the root-owned OpenSSH
/// client with a fixed noninteractive policy. It never logs argv or output.
pub fn run_git_ssh_proxy() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let Some(target) = parse_git_ssh_arguments(&arguments) else {
        return ExitCode::from(255);
    };
    let status = match fixed_ssh_command(OsStr::new(SSH_CLIENT), &target).status() {
        Ok(status) => status,
        Err(_) => return ExitCode::from(255),
    };
    ExitCode::from(
        status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(255),
    )
}

fn parse_git_ssh_arguments(arguments: &[OsString]) -> Option<ProxyTarget> {
    let arguments = arguments
        .iter()
        .map(|argument| argument.to_str())
        .collect::<Option<Vec<_>>>()?;
    if arguments.len() < 2 {
        return None;
    }
    let split = arguments.len().checked_sub(2)?;
    let mut port = None;
    let mut send_protocol = false;
    let mut cursor = 0;
    while cursor < split {
        let option = *arguments.get(cursor)?;
        let value = *arguments.get(cursor.checked_add(1)?)?;
        match option {
            "-o" if value == "SendEnv=GIT_PROTOCOL" && !send_protocol => {
                send_protocol = true;
            }
            "-p" if port.is_none() => {
                let parsed = value.parse::<u16>().ok()?;
                if parsed == 0 || parsed.to_string() != value {
                    return None;
                }
                port = Some(parsed);
            }
            _ => return None,
        }
        cursor = cursor.checked_add(2)?;
    }
    let destination = validate_destination(arguments[split])?;
    let remote_command = parse_pack_command(arguments[split + 1])?;
    Some(ProxyTarget {
        port,
        destination,
        remote_command,
    })
}

fn validate_destination(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 320 || !value.is_ascii() {
        return None;
    }
    let (user, host) = match value.split_once('@') {
        Some((user, host)) if !host.contains('@') && valid_ssh_user(user) => (Some(user), host),
        Some(_) => return None,
        None => (None, value),
    };
    if !valid_remote_host(host) {
        return None;
    }
    Some(match user {
        Some(user) => format!("{user}@{host}"),
        None => host.to_owned(),
    })
}

fn parse_pack_command(value: &str) -> Option<String> {
    for service in ["git-upload-pack", "git-receive-pack"] {
        let prefix = format!("{service} '");
        let Some(path) = value
            .strip_prefix(&prefix)
            .and_then(|path| path.strip_suffix('\''))
        else {
            continue;
        };
        if path.len() <= 2_048 && valid_remote_path(path, path.starts_with('/')) {
            return Some(format!("{service} '{path}'"));
        }
        return None;
    }
    None
}

fn fixed_ssh_command(executable: &OsStr, target: &ProxyTarget) -> Command {
    let mut command = Command::new(executable);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/home/rehow")
        .env("SSH_AUTH_SOCK", AGENT_SOCKET)
        .env("GIT_PROTOCOL", "version=2")
        .arg("-F")
        .arg("/dev/null")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "CheckHostIP=yes",
            "-o",
            "UpdateHostKeys=no",
            "-o",
            "VerifyHostKeyDNS=no",
            "-o",
            "CanonicalizeHostname=no",
            "-o",
            "ProxyCommand=none",
            "-o",
            "ProxyJump=none",
            "-o",
            "PermitLocalCommand=no",
            "-o",
            "LocalCommand=none",
            "-o",
            "KnownHostsCommand=none",
            "-o",
            USER_KNOWN_HOSTS_OPTION,
            "-o",
            SYSTEM_KNOWN_HOSTS_OPTION,
            "-o",
            "IdentityAgent=/run/rehow-agent/agent.sock",
            "-o",
            "IdentityFile=none",
            "-o",
            "IdentitiesOnly=no",
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "PasswordAuthentication=no",
            "-o",
            "KbdInteractiveAuthentication=no",
            "-o",
            "NumberOfPasswordPrompts=0",
            "-o",
            "AddKeysToAgent=no",
            "-o",
            "ForwardAgent=no",
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "RequestTTY=no",
            "-o",
            "ControlMaster=no",
            "-o",
            "ControlPath=none",
            "-o",
            "ConnectTimeout=15",
            "-o",
            "ConnectionAttempts=1",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=2",
            "-o",
            "EscapeChar=none",
            "-o",
            "LogLevel=ERROR",
            "-o",
            "SendEnv=GIT_PROTOCOL",
        ]);
    if let Some(port) = target.port {
        command.arg("-p").arg(port.to_string());
    }
    command
        .arg("--")
        .arg(&target.destination)
        .arg(&target.remote_command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn parser_accepts_only_git_pack_service_grammar() {
        let target = parse_git_ssh_arguments(&[
            "-o".into(),
            "SendEnv=GIT_PROTOCOL".into(),
            "-p".into(),
            "2222".into(),
            "git@example.invalid".into(),
            "git-upload-pack '/team/rehow.git'".into(),
        ])
        .unwrap();
        assert_eq!(target.port, Some(2222));
        assert_eq!(target.destination, "git@example.invalid");
        assert_eq!(target.remote_command, "git-upload-pack '/team/rehow.git'");

        let relative = parse_git_ssh_arguments(&[
            "example.invalid".into(),
            "git-upload-pack 'team/rehow.git'".into(),
        ])
        .unwrap();
        assert_eq!(relative.port, None);

        let receive = parse_git_ssh_arguments(&[
            "git@example.invalid".into(),
            "git-receive-pack '/team/rehow.git'".into(),
        ])
        .unwrap();
        assert_eq!(receive.remote_command, "git-receive-pack '/team/rehow.git'");
    }

    #[test]
    fn parser_rejects_option_command_destination_and_path_injection() {
        for arguments in [
            vec!["example.invalid".into()],
            vec![
                "-v".into(),
                "example.invalid".into(),
                "git-upload-pack 'x'".into(),
            ],
            vec![
                "-p".into(),
                "022".into(),
                "example.invalid".into(),
                "git-upload-pack 'x'".into(),
            ],
            vec![
                "-p".into(),
                "0".into(),
                "example.invalid".into(),
                "git-upload-pack 'x'".into(),
            ],
            vec![
                "root@host@evil.invalid".into(),
                "git-upload-pack 'x'".into(),
            ],
            vec!["GitHub.com".into(), "git-upload-pack 'x'".into()],
            vec!["example.invalid".into(), "git-upload-archive 'x'".into()],
            vec!["example.invalid".into(), "git-upload-pack '../x'".into()],
            vec![
                "example.invalid".into(),
                "git-upload-pack 'x;touch-pwned'".into(),
            ],
            vec!["example.invalid".into(), "git-upload-pack 'x' extra".into()],
        ] {
            assert!(parse_git_ssh_arguments(&arguments).is_none());
        }
        assert!(
            parse_git_ssh_arguments(&[
                OsString::from_vec(vec![0xff]),
                "git-upload-pack 'x'".into()
            ])
            .is_none()
        );
    }

    #[test]
    fn command_is_fixed_noninteractive_and_contains_no_shell() {
        let target = ProxyTarget {
            port: Some(2222),
            destination: "git@example.invalid".to_owned(),
            remote_command: "git-upload-pack '/team/rehow.git'".to_owned(),
        };
        let command = fixed_ssh_command(OsStr::new("/usr/bin/ssh"), &target);
        assert_eq!(command.get_program(), "/usr/bin/ssh");
        let arguments = command
            .get_args()
            .map(|argument| argument.to_str().unwrap())
            .collect::<Vec<_>>();
        assert!(arguments.starts_with(&["-F", "/dev/null"]));
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["-o", "StrictHostKeyChecking=yes"])
        );
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["-o", "ProxyCommand=none"])
        );
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["-o", "IdentityFile=none"])
        );
        assert!(arguments.ends_with(&[
            "-p",
            "2222",
            "--",
            "git@example.invalid",
            "git-upload-pack '/team/rehow.git'"
        ]));
        assert!(
            !arguments
                .iter()
                .any(|argument| matches!(*argument, "sh" | "-c"))
        );
        let environment = command
            .get_envs()
            .map(|(key, value)| (key.to_str().unwrap(), value.and_then(OsStr::to_str)))
            .collect::<Vec<_>>();
        assert!(environment.contains(&("SSH_AUTH_SOCK", Some(AGENT_SOCKET))));
        assert!(environment.contains(&("GIT_PROTOCOL", Some("version=2"))));

        let delimiter = arguments
            .iter()
            .position(|argument| *argument == "--")
            .unwrap();
        let status = Command::new("/usr/bin/ssh")
            .env_clear()
            .args(&arguments[..delimiter])
            .arg("-G")
            .args(&arguments[delimiter..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
    }
}
