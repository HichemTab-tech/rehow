use std::{
    fs,
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd, RawFd},
    os::unix::fs::PermissionsExt,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    time::Duration,
};

fn pipe() -> [RawFd; 2] {
    let mut descriptors = [-1; 2];
    // SAFETY: descriptors points to two writable integers.
    assert_eq!(
        unsafe { libc::pipe2(descriptors.as_mut_ptr(), libc::O_CLOEXEC) },
        0
    );
    descriptors
}

fn duplicate_above_stdio(descriptor: RawFd) -> RawFd {
    // SAFETY: F_DUPFD_CLOEXEC duplicates an owned test descriptor.
    let duplicate = unsafe { libc::fcntl(descriptor, libc::F_DUPFD_CLOEXEC, 10) };
    assert!(duplicate >= 10);
    duplicate
}

fn close(descriptor: RawFd) {
    // SAFETY: every descriptor is closed exactly once in the parent.
    assert_eq!(unsafe { libc::close(descriptor) }, 0);
}

fn approval_frame() -> Vec<u8> {
    let summary = b"Delete one draft Guide to recoverable trash.";
    let mut frame = Vec::new();
    frame.extend_from_slice(b"RHA1");
    frame.push(2);
    frame.extend_from_slice(&[0, 0, 0]);
    frame.extend_from_slice(&(summary.len() as u32).to_be_bytes());
    frame.extend_from_slice(summary);
    frame
}

#[test]
fn helper_fails_silently_without_private_pipes() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rehow-mcp-approve"));
    // SAFETY: this closure runs in the single-threaded child between fork and exec and only closes
    // the two protocol descriptor numbers.
    unsafe {
        command.pre_exec(|| {
            libc::close(3);
            libc::close(4);
            Ok(())
        });
    }
    let output = command.output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn helper_accepts_only_private_pipes_and_refuses_headless_approval() {
    let request = pipe();
    let result = pipe();
    let child_request = duplicate_above_stdio(request[0]);
    let child_result = duplicate_above_stdio(result[1]);
    let leak = std::fs::File::open("/dev/null").unwrap();
    // SAFETY: clearing CLOEXEC deliberately creates one hostile inherited descriptor for the test.
    assert_eq!(
        unsafe { libc::fcntl(leak.as_raw_fd(), libc::F_SETFD, 0) },
        0
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_rehow-mcp-approve"));
    command
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: the child-only closure performs async-signal-safe descriptor operations before exec.
    unsafe {
        command.pre_exec(move || {
            if libc::dup2(child_request, 3) < 0 || libc::dup2(child_result, 4) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().unwrap();
    close(request[0]);
    close(result[1]);
    close(child_request);
    close(child_result);

    let fd_root = format!("/proc/{}/fd", child.id());
    let mut observed = Vec::new();
    for _ in 0..100 {
        observed = fs::read_dir(&fd_root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        observed.sort();
        if observed == ["0", "1", "2", "3", "4"] {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(observed, ["0", "1", "2", "3", "4"]);
    for descriptor in 0..=2 {
        assert_eq!(
            fs::read_link(format!("{fd_root}/{descriptor}")).unwrap(),
            std::path::Path::new("/dev/null")
        );
    }
    assert!(
        fs::read_link(format!("{fd_root}/3"))
            .unwrap()
            .to_string_lossy()
            .starts_with("pipe:")
    );
    assert!(
        fs::read_link(format!("{fd_root}/4"))
            .unwrap()
            .to_string_lossy()
            .starts_with("pipe:")
    );
    drop(leak);

    // SAFETY: request[1] is the parent's owned write end.
    let mut request_writer = unsafe { std::fs::File::from_raw_fd(request[1]) };
    request_writer.write_all(&approval_frame()).unwrap();
    drop(request_writer);

    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    // SAFETY: result[0] is the parent's owned read end.
    let mut result_reader = unsafe { std::fs::File::from_raw_fd(result[0]) };
    let mut bytes = Vec::new();
    result_reader.read_to_end(&mut bytes).unwrap();
    assert!(bytes.is_empty());
}

#[test]
fn helper_never_resolves_a_dialog_program_from_hostile_path() {
    let hostile = tempfile::tempdir().unwrap();
    let marker = hostile.path().join("forged-approval");
    let zenity = hostile.path().join("zenity");
    fs::write(
        &zenity,
        "#!/bin/sh\ntouch \"$REHOW_ZENITY_MARKER\"\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&zenity, fs::Permissions::from_mode(0o700)).unwrap();

    let request = pipe();
    let result = pipe();
    let child_request = duplicate_above_stdio(request[0]);
    let child_result = duplicate_above_stdio(result[1]);
    let mut command = Command::new(env!("CARGO_BIN_EXE_rehow-mcp-approve"));
    command
        .env("PATH", hostile.path())
        .env("DISPLAY", ":65535")
        .env_remove("WAYLAND_DISPLAY")
        .env("REHOW_ZENITY_MARKER", &marker)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: the child-only closure performs async-signal-safe descriptor operations before exec.
    unsafe {
        command.pre_exec(move || {
            if libc::dup2(child_request, 3) < 0 || libc::dup2(child_result, 4) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().unwrap();
    close(request[0]);
    close(result[1]);
    close(child_request);
    close(child_result);

    // SAFETY: request[1] is the parent's owned write end.
    let mut request_writer = unsafe { std::fs::File::from_raw_fd(request[1]) };
    request_writer.write_all(&approval_frame()).unwrap();
    drop(request_writer);
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(!marker.exists());
    // SAFETY: result[0] is the parent's owned read end.
    let mut result_reader = unsafe { std::fs::File::from_raw_fd(result[0]) };
    let mut bytes = Vec::new();
    result_reader.read_to_end(&mut bytes).unwrap();
    assert!(bytes.is_empty());
}
