use std::{
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{FromRawHandle, OwnedHandle},
    },
    path::PathBuf,
};
use windows_sys::Win32::{
    System::{
        Environment::GetCommandLineW,
        Threading::{
            CREATE_NEW_CONSOLE, CreateProcessW, PROCESS_INFORMATION, STARTF_USESHOWWINDOW,
            STARTUPINFOW,
        },
    },
    UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW, SW_HIDE},
};

#[derive(Debug)]
pub(super) enum LaunchError {
    LocateEditor(io::Error),
    StartEditor { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LocateEditor(error) => write!(f, "Could not locate Neomacs: {error}"),
            Self::StartEditor { path, source } => {
                write!(f, "Could not start {}: {source}", path.display())
            }
        }
    }
}

pub(super) fn run() -> Result<(), LaunchError> {
    let path = std::env::current_exe()
        .map_err(LaunchError::LocateEditor)?
        .with_file_name("neomacs.exe");
    let executable: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // Forward the original UTF-16 argument tail verbatim. Only argv[0] changes;
    // arguments never pass through UTF-8 or an additional quoting round trip.
    // SAFETY: Windows owns a NUL-terminated command line for this process for
    // its entire lifetime. We only borrow it while constructing our own buffer.
    let original = unsafe {
        let pointer = GetCommandLineW();
        let mut length = 0;
        while *pointer.add(length) != 0 {
            length += 1;
        }
        std::slice::from_raw_parts(pointer, length)
    };
    // argv[0] uses the special executable-name rules: quotes group whitespace;
    // backslashes are literal, unlike the subsequent argument parsing rules.
    let mut quoted = false;
    let tail = original
        .iter()
        .position(|&unit| {
            if unit == u16::from(b'"') {
                quoted = !quoted;
            }
            !quoted && matches!(unit, 9 | 32)
        })
        .unwrap_or(original.len());
    let mut command_line = vec![u16::from(b'"')];
    command_line.extend_from_slice(&executable[..executable.len() - 1]);
    command_line.push(u16::from(b'"'));
    command_line.extend_from_slice(&original[tail..]);
    command_line.push(0);
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        dwFlags: STARTF_USESHOWWINDOW,
        wShowWindow: SW_HIDE as u16,
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: all pointers refer to live, correctly sized buffers/structures;
    // command_line is writable and both strings are NUL-terminated. NULL
    // environment and directory preserve the caller's environment and cwd.
    let started = unsafe {
        CreateProcessW(
            executable.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_NEW_CONSOLE,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut process,
        )
    };
    if started == 0 {
        return Err(LaunchError::StartEditor {
            path,
            source: io::Error::last_os_error(),
        });
    }
    // A separate hidden console also protects a terminal that launched us:
    // SW_HIDE must never be applied to the caller's existing console.
    // SAFETY: successful CreateProcessW transfers two distinct valid handles
    // to us. OwnedHandle closes them; the editor continues independently.
    unsafe {
        let _process = OwnedHandle::from_raw_handle(process.hProcess);
        let _thread = OwnedHandle::from_raw_handle(process.hThread);
    }
    Ok(())
}

pub(super) fn report_error(error: &LaunchError) {
    let message: Vec<u16> = error.to_string().encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "Neomacs".encode_utf16().chain(Some(0)).collect();
    // SAFETY: the buffers remain live and NUL-terminated throughout the dialog.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
