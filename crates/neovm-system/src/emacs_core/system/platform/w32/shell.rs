//! Windows shell integration: `w32-shell-execute`.
//!
//! GNU implements this in `src/w32fns.c:8682` over `ShellExecuteExW` (or the
//! ANSI entry point when Unicode file names are disabled) with
//! `SEE_MASK_INVOKEIDLIST | SEE_MASK_FLAG_DDEWAIT | SEE_MASK_FLAG_NO_UI`.
//! The flags matter for compatibility: `SEE_MASK_INVOKEIDLIST` lets verbs
//! supplied by shell extensions run, and `SEE_MASK_FLAG_NO_UI` keeps failures
//! in Lisp instead of opening a system error dialog.

#[cfg(windows)]
use super::*;

/// GNU's drive-letter `file:///` rewrite (`src/w32fns.c:8837`, bug#20220).
///
/// Passing `file:///` URLs to `ShellExecute` can start an uncontrollable
/// thread inside `shlwapi.dll`, so GNU converts local drive-letter URLs to
/// plain file names first. Only `file:///X:/...` and `file:///X:\...` are
/// rewritten; URLs without a drive letter are passed through unchanged.
#[allow(dead_code)] // only the Windows entry point calls this in non-test builds
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn rewrite_file_url_document(document: &str) -> &str {
    const FILE_URL_PREFIX: &str = "file:///";

    let Some(rest) = document.strip_prefix(FILE_URL_PREFIX) else {
        return document;
    };
    let bytes = rest.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
    {
        rest
    } else {
        document
    }
}

/// `w32-shell-execute`, GNU `src/w32fns.c:8682`.
///
/// Arguments are `(OPERATION DOCUMENT &optional PARAMETERS SHOW-FLAG)`.
/// Returns `t` on success and signals `(error "ShellExecute failed: ...")`
/// on failure, matching the C implementation (GNU's docstring claims a nil
/// return for success, but the code returns `Qt`; observable behavior wins).
#[cfg(windows)]
pub(crate) fn w32_shell_execute(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_FLAG_DDEWAIT, SEE_MASK_FLAG_NO_UI, SEE_MASK_INVOKEIDLIST, SHELLEXECUTEINFOW,
        ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWDEFAULT;

    // GNU's argument order is (OPERATION DOCUMENT &optional PARAMETERS
    // SHOW-FLAG); every in-tree caller passes the verb first.
    let operation = args.first().copied().unwrap_or(Value::NIL);
    let document = args.get(1).copied().unwrap_or(Value::NIL);
    let parameters = args.get(2).copied().unwrap_or(Value::NIL);
    let show_flag = args.get(3).copied().unwrap_or(Value::NIL);

    // CHECK_STRING (document).
    let document_text = expect_string_runtime(&document)?;

    // BVAR (current_buffer, directory). `default-directory` is a per-buffer
    // variable, so read it from the current buffer rather than the obarray.
    let current_dir = crate::emacs_core::fileio::default_directory_lisp_in_state(
        ctx.variables(),
        &[],
        &ctx.edit.buffers,
    )
    .map(|directory| neovm_value::emacs_char::to_utf8_lossy(directory.as_bytes()))
    .unwrap_or_default();

    let document_text = rewrite_file_url_document(&document_text);

    // GNU expands DOCUMENT only to decide whether it names an existing local
    // file. A URL, a remote name handled by Lisp, or a name that does not
    // exist is passed to ShellExecute exactly as given.
    let expanded = crate::emacs_core::fileio::builtin_expand_file_name(
        ctx,
        vec![Value::string(document_text), Value::NIL],
    )?;
    let handler = crate::emacs_core::fileio::find_file_name_handler_lisp_for_eval(
        ctx,
        expanded
            .as_lisp_string()
            .expect("expand-file-name always returns a string"),
        Value::symbol("file-exists-p"),
    );
    let local_document = if handler.is_nil() {
        let absdoc = expect_string_runtime(&expanded)?;
        // `expand-file-name` produced forward slashes; ShellExecute needs
        // backslashes for local names, including UNC paths.
        let dos_path = absdoc.replace('/', "\\");
        std::path::Path::new(&dos_path).exists().then_some(dos_path)
    } else {
        None
    };
    let shell_document = local_document.as_deref().unwrap_or(document_text);

    let document_wide = wide_nul(shell_document);
    let current_dir_wide = wide_nul(&current_dir);
    // GNU assumes OPERATION is pure ASCII and copies its bytes into `wchar_t`.
    let operation_wide = operation.as_lisp_string().map(|text| {
        text.as_bytes()
            .iter()
            .take(32767)
            .map(|&byte| byte as u16)
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    });
    let parameters_wide = parameters.as_lisp_string().map(|text| {
        let utf8 = neovm_value::emacs_char::to_utf8_lossy(text.as_bytes());
        let mut wide: Vec<u16> = utf8.encode_utf16().take(32767).collect();
        wide.push(0);
        wide
    });
    let n_show = match show_flag.kind() {
        ValueKind::Fixnum(value) => value as i32,
        _ => SW_SHOWDEFAULT,
    };

    let mut info = unsafe { std::mem::zeroed::<SHELLEXECUTEINFOW>() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_INVOKEIDLIST | SEE_MASK_FLAG_DDEWAIT | SEE_MASK_FLAG_NO_UI;
    info.hwnd = std::ptr::null_mut();
    info.lpVerb = operation_wide
        .as_ref()
        .map_or(std::ptr::null(), |verb| verb.as_ptr());
    info.lpFile = document_wide.as_ptr();
    info.lpParameters = parameters_wide
        .as_ref()
        .map_or(std::ptr::null(), |parameters| parameters.as_ptr());
    info.lpDirectory = current_dir_wide.as_ptr();
    info.nShow = n_show;

    if unsafe { ShellExecuteExW(&mut info) } != 0 {
        return Ok(Value::T);
    }

    Err(signal(
        LispCondition::Error,
        vec![Value::string(format!(
            "ShellExecute failed: {}",
            w32_strerror(0)
        ))],
    ))
}

/// GNU `w32_strerror` (`src/w32fns.c:9992`): format the last Windows error
/// without the trailing newline, falling back to `w32 error N` when the
/// system has no message for the code.
#[cfg(windows)]
fn w32_strerror(error_no: u32) -> String {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Diagnostics::Debug::{
        FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS, FormatMessageW,
    };

    let error_no = if error_no == 0 {
        unsafe { GetLastError() }
    } else {
        error_no
    };

    let mut buffer = [0u16; 500];
    let length = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
            std::ptr::null(),
            error_no,
            0,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            std::ptr::null(),
        )
    } as usize;

    let mut length = length;
    while length > 0 && (buffer[length - 1] == b'\r' as u16 || buffer[length - 1] == b'\n' as u16) {
        length -= 1;
    }
    if length == 0 {
        return format!("w32 error {error_no}");
    }
    String::from_utf16_lossy(&buffer[..length])
}
