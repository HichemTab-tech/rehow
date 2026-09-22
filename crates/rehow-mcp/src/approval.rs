use std::{
    ffi::CString,
    fs::File,
    io::{self, Read, Write},
    os::fd::{FromRawFd, RawFd},
    process::ExitCode,
};

const REQUEST_FD: RawFd = 3;
const RESULT_FD: RawFd = 4;
const REQUEST_MAGIC: [u8; 4] = *b"RHA1";
const REQUEST_HEADER_BYTES: usize = 12;
const MAX_SUMMARY_BYTES: usize = 4_096;
const MAX_SUMMARY_LINES: usize = 32;
const MAX_LINE_SCALARS: usize = 200;
const REQUEST_READ_TIMEOUT_MS: libc::c_int = 5_000;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ApprovalKind {
    Broad = 1,
    Destructive = 2,
    Tested = 3,
    GitWrite = 4,
}

impl ApprovalKind {
    fn parse(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Broad),
            2 => Some(Self::Destructive),
            3 => Some(Self::Tested),
            4 => Some(Self::GitWrite),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Broad => "Broad Workspace change",
            Self::Destructive => "Destructive Workspace change",
            Self::Tested => "Mark Guide as tested",
            Self::GitWrite => "Git write",
        }
    }

    const fn as_byte(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ApprovalRequest {
    kind: ApprovalKind,
    summary: String,
}

/// Runs the separately packaged approval companion on its fixed inherited pipes.
///
/// This function intentionally accepts no arguments and emits no stdout/stderr output.
#[doc(hidden)]
pub fn run_approval_helper() -> ExitCode {
    if !valid_pipe(REQUEST_FD, libc::O_RDONLY) || !valid_pipe(RESULT_FD, libc::O_WRONLY) {
        return ExitCode::from(2);
    }
    if set_close_on_exec(REQUEST_FD).is_err()
        || set_close_on_exec(RESULT_FD).is_err()
        || isolate_standard_streams().is_err()
        || close_extra_descriptors().is_err()
    {
        return ExitCode::from(2);
    }

    // SAFETY: the fixed descriptors were validated above and ownership transfers exactly once.
    let mut request_pipe = unsafe { File::from_raw_fd(REQUEST_FD) };
    // SAFETY: the fixed descriptors were validated above and ownership transfers exactly once.
    let mut result_pipe = unsafe { File::from_raw_fd(RESULT_FD) };
    let request = match read_request(&mut request_pipe, Some(REQUEST_FD)) {
        Ok(request) => request,
        Err(_) => return ExitCode::from(2),
    };
    if !graphical_session_available() {
        return ExitCode::from(3);
    }

    let description = format!(
        "{}\n\n{}\n\nApprove only if this exact summary matches what you intend.",
        request.kind.label(),
        request.summary
    );
    let Some(approved) = show_native_dialog(request.kind.label(), &description) else {
        return ExitCode::from(3);
    };
    let result = if approved { [1_u8] } else { [0_u8] };
    if result_pipe.write_all(&result).is_err() || result_pipe.flush().is_err() {
        return ExitCode::from(4);
    }
    ExitCode::SUCCESS
}

fn set_close_on_exec(descriptor: RawFd) -> io::Result<()> {
    // SAFETY: F_GETFD only reads descriptor flags.
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: F_SETFD updates only this process's descriptor flags.
    if unsafe { libc::fcntl(descriptor, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn show_native_dialog(label: &str, description: &str) -> Option<bool> {
    let title = CString::new("Confirm Rehow change").ok()?;
    let label = CString::new(label).ok()?;
    let description = CString::new(description).ok()?;
    // SAFETY: GTK is initialized once in this single-threaded helper before any GTK calls.
    if unsafe { gtk_sys::gtk_init_check(std::ptr::null_mut(), std::ptr::null_mut()) } != 1 {
        return None;
    }
    // SAFETY: all strings are NUL-terminated, the format strings are fixed "%s", and the
    // returned widget is checked and destroyed exactly once below.
    let dialog = unsafe {
        gtk_sys::gtk_message_dialog_new(
            std::ptr::null_mut(),
            gtk_sys::GTK_DIALOG_MODAL,
            gtk_sys::GTK_MESSAGE_WARNING,
            gtk_sys::GTK_BUTTONS_NONE,
            c"%s".as_ptr(),
            label.as_ptr(),
        )
    };
    if dialog.is_null() {
        return None;
    }
    // SAFETY: dialog is a live GtkMessageDialog/GtkWindow and all strings remain live.
    let response = unsafe {
        gtk_sys::gtk_window_set_title(dialog.cast(), title.as_ptr());
        gtk_sys::gtk_message_dialog_format_secondary_text(
            dialog.cast(),
            c"%s".as_ptr(),
            description.as_ptr(),
        );
        gtk_sys::gtk_dialog_add_button(
            dialog.cast(),
            c"_Cancel".as_ptr(),
            gtk_sys::GTK_RESPONSE_CANCEL,
        );
        gtk_sys::gtk_dialog_add_button(
            dialog.cast(),
            c"_Approve".as_ptr(),
            gtk_sys::GTK_RESPONSE_ACCEPT,
        );
        gtk_sys::gtk_dialog_set_default_response(dialog.cast(), gtk_sys::GTK_RESPONSE_CANCEL);
        let response = gtk_sys::gtk_dialog_run(dialog.cast());
        gtk_sys::gtk_widget_destroy(dialog);
        response
    };
    Some(response_is_approval(response))
}

fn response_is_approval(response: libc::c_int) -> bool {
    response == gtk_sys::GTK_RESPONSE_ACCEPT
}

fn valid_pipe(descriptor: RawFd, access: libc::c_int) -> bool {
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: metadata points to writable storage and descriptor is only inspected.
    if unsafe { libc::fstat(descriptor, metadata.as_mut_ptr()) } != 0 {
        return false;
    }
    // SAFETY: successful fstat initialized the value.
    let metadata = unsafe { metadata.assume_init() };
    if metadata.st_mode & libc::S_IFMT != libc::S_IFIFO {
        return false;
    }
    // SAFETY: F_GETFL only reads the descriptor status flags.
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
    flags >= 0 && flags & libc::O_ACCMODE == access
}

fn isolate_standard_streams() -> io::Result<()> {
    let path = c"/dev/null";
    // SAFETY: path is a fixed NUL-terminated literal and open returns a new descriptor.
    let null = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
    if null < 0 {
        return Err(io::Error::last_os_error());
    }
    for target in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
        // SAFETY: null is open and each target is a fixed standard descriptor.
        if unsafe { libc::dup2(null, target) } < 0 {
            // SAFETY: null remains owned by this function on the error path.
            unsafe { libc::close(null) };
            return Err(io::Error::last_os_error());
        }
    }
    // SAFETY: null is no longer needed after all three dup2 calls.
    unsafe { libc::close(null) };
    Ok(())
}

fn close_extra_descriptors() -> io::Result<()> {
    // SAFETY: descriptors 0..=4 are the only intentional inherited descriptors.
    let result = unsafe { libc::close_range(5, u32::MAX, 0) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn graphical_session_available() -> bool {
    ["WAYLAND_DISPLAY", "DISPLAY"]
        .into_iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

fn read_request(
    reader: &mut impl Read,
    poll_descriptor: Option<RawFd>,
) -> io::Result<ApprovalRequest> {
    let mut header = [0_u8; REQUEST_HEADER_BYTES];
    read_exact_bounded(reader, &mut header, poll_descriptor)?;
    if header[..4] != REQUEST_MAGIC || header[5..8] != [0, 0, 0] {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid request",
        ));
    }
    let kind = ApprovalKind::parse(header[4])
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid request"))?;
    let length = u32::from_be_bytes(header[8..12].try_into().expect("fixed header")) as usize;
    if !(1..=MAX_SUMMARY_BYTES).contains(&length) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid request",
        ));
    }
    let mut bytes = vec![0_u8; length];
    read_exact_bounded(reader, &mut bytes, poll_descriptor)?;
    let mut trailing = [0_u8; 1];
    if let Some(descriptor) = poll_descriptor {
        wait_until_readable(descriptor, REQUEST_READ_TIMEOUT_MS)?;
    }
    if reader.read(&mut trailing)? != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid request",
        ));
    }
    let summary = String::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid request"))?;
    validate_summary(&summary)?;
    Ok(ApprovalRequest { kind, summary })
}

pub(crate) fn encode_approval_request(kind: ApprovalKind, summary: &str) -> io::Result<Vec<u8>> {
    validate_summary(summary)?;
    let summary = summary.as_bytes();
    let length = u32::try_from(summary.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid request"))?;
    let mut request = Vec::with_capacity(REQUEST_HEADER_BYTES + summary.len());
    request.extend_from_slice(&REQUEST_MAGIC);
    request.push(kind.as_byte());
    request.extend_from_slice(&[0, 0, 0]);
    request.extend_from_slice(&length.to_be_bytes());
    request.extend_from_slice(summary);
    Ok(request)
}

fn read_exact_bounded(
    reader: &mut impl Read,
    mut destination: &mut [u8],
    poll_descriptor: Option<RawFd>,
) -> io::Result<()> {
    while !destination.is_empty() {
        if let Some(descriptor) = poll_descriptor {
            wait_until_readable(descriptor, REQUEST_READ_TIMEOUT_MS)?;
        }
        let read = reader.read(destination)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "short request",
            ));
        }
        destination = &mut destination[read..];
    }
    Ok(())
}

fn wait_until_readable(descriptor: RawFd, timeout_ms: libc::c_int) -> io::Result<()> {
    let mut poll = libc::pollfd {
        fd: descriptor,
        events: libc::POLLIN | libc::POLLHUP,
        revents: 0,
    };
    // SAFETY: poll points to one initialized pollfd for the duration of the call.
    let ready = unsafe { libc::poll(&mut poll, 1, timeout_ms) };
    if ready > 0 {
        Ok(())
    } else if ready == 0 {
        Err(io::Error::new(io::ErrorKind::TimedOut, "request timed out"))
    } else {
        Err(io::Error::last_os_error())
    }
}

pub(crate) fn validate_summary(summary: &str) -> io::Result<()> {
    let lines: Vec<&str> = summary.lines().collect();
    if lines.is_empty()
        || lines.len() > MAX_SUMMARY_LINES
        || summary.starts_with(char::is_whitespace)
        || summary.ends_with(char::is_whitespace)
        || summary.chars().any(forbidden_summary_character)
        || lines.iter().any(|line| {
            line.is_empty() || line.chars().count() > MAX_LINE_SCALARS || line.trim() != *line
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid request",
        ));
    }
    Ok(())
}

fn forbidden_summary_character(character: char) -> bool {
    (character != '\n' && character.is_control())
        || is_invisible_or_format_character(character)
        || matches!(
            character,
            '\u{2028}'..='\u{2029}' | '\u{fdd0}'..='\u{fdef}'
        )
        || (character as u32) & 0xffff >= 0xfffe
}

pub(crate) fn safe_summary_line(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control()
                || is_invisible_or_format_character(character)
                || matches!(
                    character,
                    '\u{2028}'..='\u{2029}' | '\u{fdd0}'..='\u{fdef}'
                )
                || (character as u32) & 0xffff >= 0xfffe
            {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

pub(crate) fn safe_summary_excerpt(value: &str, maximum_scalars: usize) -> String {
    let safe = safe_summary_line(value);
    if safe.chars().count() <= maximum_scalars {
        return safe;
    }
    let retained = maximum_scalars.saturating_sub(1);
    let mut excerpt = safe.chars().take(retained).collect::<String>();
    if maximum_scalars > 0 {
        excerpt.push('…');
    }
    excerpt
}

fn is_invisible_or_format_character(character: char) -> bool {
    // This is the complete union of Unicode 16's Format (Cf) characters and
    // Default_Ignorable_Code_Point ranges. Keeping the table explicit avoids
    // accepting display-spoofing characters through library/version drift.
    matches!(
        character,
        '\u{00ad}'
            | '\u{034f}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061c}'
            | '\u{06dd}'
            | '\u{070f}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08e2}'
            | '\u{115f}'..='\u{1160}'
            | '\u{17b4}'..='\u{17b5}'
            | '\u{180b}'..='\u{180f}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{3164}'
            | '\u{fe00}'..='\u{fe0f}'
            | '\u{feff}'
            | '\u{ffa0}'
            | '\u{fff0}'..='\u{fffb}'
            | '\u{110bd}'
            | '\u{110cd}'
            | '\u{13430}'..='\u{1343f}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0000}'..='\u{e0fff}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn request(kind: u8, summary: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&REQUEST_MAGIC);
        bytes.push(kind);
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes.extend_from_slice(&(summary.len() as u32).to_be_bytes());
        bytes.extend_from_slice(summary);
        bytes
    }

    #[test]
    fn parses_the_exact_bounded_protocol() {
        let bytes = request(4, b"Commit 2 changed Guides.\nPush no remote changes.");
        assert_eq!(
            read_request(&mut Cursor::new(bytes), None).unwrap(),
            ApprovalRequest {
                kind: ApprovalKind::GitWrite,
                summary: "Commit 2 changed Guides.\nPush no remote changes.".to_owned(),
            }
        );
    }

    #[test]
    fn rejects_malformed_ambiguous_and_spoofable_requests() {
        let mut cases = vec![
            Vec::new(),
            request(0, b"Effect"),
            request(5, b"Effect"),
            request(1, b""),
            request(1, b" leading"),
            request(1, b"trailing "),
            request(1, b"line\n\nline"),
            request(1, b"line\r\nline"),
            request(1, b"line\n leading"),
            request(1, b"effect\x1b[2J"),
            request(1, "effect\u{202e}txt".as_bytes()),
            request(1, "effect\u{2028}txt".as_bytes()),
            request(1, "effect\u{2029}txt".as_bytes()),
            request(1, "effect\u{200b}txt".as_bytes()),
            request(1, "effect\u{00ad}txt".as_bytes()),
            request(1, "effect\u{034f}txt".as_bytes()),
            request(1, "effect\u{180e}txt".as_bytes()),
            request(1, "effect\u{fff9}txt".as_bytes()),
            request(1, "effect\u{e0020}txt".as_bytes()),
            request(1, "effect\u{fdd0}txt".as_bytes()),
            request(1, &[0xff]),
            request(1, "x".repeat(MAX_LINE_SCALARS + 1).as_bytes()),
            request(1, &vec![b'x'; MAX_SUMMARY_BYTES + 1]),
            request(
                1,
                "line\n".repeat(MAX_SUMMARY_LINES + 1).trim_end().as_bytes(),
            ),
        ];
        let mut reserved = request(1, b"Effect");
        reserved[5] = 1;
        cases.push(reserved);
        let mut trailing = request(1, b"Effect");
        trailing.push(0);
        cases.push(trailing);
        let mut wrong_magic = request(1, b"Effect");
        wrong_magic[0] = b'X';
        cases.push(wrong_magic);

        for bytes in cases {
            assert!(read_request(&mut Cursor::new(bytes), None).is_err());
        }
    }

    #[test]
    fn summary_line_display_copy_neutralizes_every_rejected_scalar_class() {
        let safe = safe_summary_line("Title\nwith\u{00ad}hidden\u{202e}text\u{fdd0}\u{10ffff}");
        assert_eq!(safe.chars().filter(|value| *value == '\u{fffd}').count(), 5);
        assert!(validate_summary(&safe).is_ok());
    }

    #[test]
    fn summary_excerpt_is_scalar_bounded_and_discloses_truncation() {
        let excerpt = safe_summary_excerpt(&format!("{}\u{202e}", "a".repeat(240)), 180);
        assert_eq!(excerpt.chars().count(), 180);
        assert!(excerpt.ends_with('…'));
        assert!(!excerpt.contains('\u{202e}'));
    }

    #[test]
    fn stalled_pipe_times_out_without_reading() {
        let mut descriptors = [-1; 2];
        // SAFETY: descriptors points to two writable integers.
        assert_eq!(
            unsafe { libc::pipe2(descriptors.as_mut_ptr(), libc::O_CLOEXEC) },
            0
        );
        let error = wait_until_readable(descriptors[0], 0).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        // SAFETY: both pipe descriptors remain owned by this test.
        unsafe {
            libc::close(descriptors[0]);
            libc::close(descriptors[1]);
        }
    }

    #[test]
    fn only_the_explicit_approve_response_authorizes() {
        assert!(response_is_approval(gtk_sys::GTK_RESPONSE_ACCEPT));
        for response in [
            gtk_sys::GTK_RESPONSE_CANCEL,
            gtk_sys::GTK_RESPONSE_DELETE_EVENT,
            gtk_sys::GTK_RESPONSE_CLOSE,
            gtk_sys::GTK_RESPONSE_NO,
            gtk_sys::GTK_RESPONSE_YES,
            gtk_sys::GTK_RESPONSE_OK,
            gtk_sys::GTK_RESPONSE_NONE,
        ] {
            assert!(!response_is_approval(response));
        }
    }
}
