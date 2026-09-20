use std::{ffi::OsString, fmt, path::PathBuf};

pub const MAX_WORKSPACES: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceArgument {
    pub alias: String,
    pub path: PathBuf,
    pub allow_write: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchConfig {
    pub workspaces: Vec<WorkspaceArgument>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchError {
    MissingWorkspace,
    TooManyWorkspaces,
    InvalidArgument,
    InvalidAlias,
    InvalidPath,
    DuplicateAlias,
    DuplicateCapability,
    UnknownCapabilityAlias,
}

impl fmt::Display for LaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingWorkspace => "at least one --workspace alias=/absolute/path is required",
            Self::TooManyWorkspaces => "at most 16 Workspaces may be configured",
            Self::InvalidArgument => "invalid argument; expected --workspace alias=/absolute/path",
            Self::InvalidAlias => "Workspace alias is invalid",
            Self::InvalidPath => "Workspace path must be absolute UTF-8 without NUL bytes",
            Self::DuplicateAlias => "Workspace aliases must be unique",
            Self::DuplicateCapability => "Workspace write capability was repeated",
            Self::UnknownCapabilityAlias => "--allow-write must name a configured Workspace alias",
        })
    }
}

impl std::error::Error for LaunchError {}

pub fn parse_launch_arguments(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<LaunchConfig, LaunchError> {
    let mut arguments = arguments.into_iter();
    let mut workspaces: Vec<WorkspaceArgument> = Vec::new();
    let mut writable_aliases: Vec<String> = Vec::new();
    while let Some(flag) = arguments.next() {
        let value = arguments.next().ok_or(LaunchError::InvalidArgument)?;
        let value = value.to_str().ok_or(LaunchError::InvalidArgument)?;
        if flag == "--allow-write" {
            if !valid_alias(value) {
                return Err(LaunchError::InvalidAlias);
            }
            if writable_aliases.iter().any(|alias| alias == value) {
                return Err(LaunchError::DuplicateCapability);
            }
            writable_aliases.push(value.to_owned());
            continue;
        }
        if flag != "--workspace" {
            return Err(LaunchError::InvalidArgument);
        }
        let (alias, raw_path) = value.split_once('=').ok_or(LaunchError::InvalidArgument)?;
        if !valid_alias(alias) {
            return Err(LaunchError::InvalidAlias);
        }
        if workspaces.iter().any(|item| item.alias == alias) {
            return Err(LaunchError::DuplicateAlias);
        }
        if raw_path.as_bytes().contains(&0) {
            return Err(LaunchError::InvalidPath);
        }
        let path = PathBuf::from(raw_path);
        if !path.is_absolute() {
            return Err(LaunchError::InvalidPath);
        }
        workspaces.push(WorkspaceArgument {
            alias: alias.to_owned(),
            path,
            allow_write: false,
        });
        if workspaces.len() > MAX_WORKSPACES {
            return Err(LaunchError::TooManyWorkspaces);
        }
    }
    if workspaces.is_empty() {
        return Err(LaunchError::MissingWorkspace);
    }
    for alias in writable_aliases {
        let workspace = workspaces
            .iter_mut()
            .find(|workspace| workspace.alias == alias)
            .ok_or(LaunchError::UnknownCapabilityAlias)?;
        workspace.allow_write = true;
    }
    Ok(LaunchConfig { workspaces })
}

fn valid_alias(alias: &str) -> bool {
    if alias.is_empty() || alias.len() > 32 {
        return false;
    }
    let bytes = alias.as_bytes();
    if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
        return false;
    }
    if !bytes[bytes.len() - 1].is_ascii_lowercase() && !bytes[bytes.len() - 1].is_ascii_digit() {
        return false;
    }
    let mut separator = false;
    for byte in bytes {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            separator = false;
        } else if (*byte == b'-' || *byte == b'_') && !separator {
            separator = true;
        } else {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args<'a>(values: &'a [&'a str]) -> impl Iterator<Item = OsString> + 'a {
        values.iter().map(OsString::from)
    }

    #[test]
    fn accepts_bounded_explicit_workspaces() {
        let config = parse_launch_arguments(args(&[
            "--workspace",
            "personal=/tmp/personal",
            "--workspace",
            "arch_2=/tmp/arch",
            "--allow-write",
            "personal",
        ]))
        .unwrap();
        assert_eq!(config.workspaces.len(), 2);
        assert!(config.workspaces[0].allow_write);
        assert!(!config.workspaces[1].allow_write);
    }

    #[test]
    fn rejects_ambient_or_ambiguous_authority() {
        for values in [
            vec![],
            vec!["/tmp/workspace"],
            vec!["--workspace", "x=relative"],
            vec!["--workspace", "Bad=/tmp/x"],
            vec!["--workspace", "a--b=/tmp/x"],
            vec!["--workspace", "a=/tmp/x", "--workspace", "a=/tmp/y"],
            vec!["--workspace", "a=/tmp/x", "--allow-write", "missing"],
            vec![
                "--workspace",
                "a=/tmp/x",
                "--allow-write",
                "a",
                "--allow-write",
                "a",
            ],
        ] {
            assert!(parse_launch_arguments(args(&values)).is_err(), "{values:?}");
        }
    }

    #[test]
    fn enforces_workspace_count_limit() {
        let mut values = Vec::new();
        for index in 0..17 {
            values.push("--workspace".to_owned());
            values.push(format!("w{index}=/tmp/w{index}"));
        }
        assert_eq!(
            parse_launch_arguments(values.into_iter().map(OsString::from)),
            Err(LaunchError::TooManyWorkspaces)
        );
    }
}
