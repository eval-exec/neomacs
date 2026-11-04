//! Platform-neutral Lisp error construction shared by native adapters.

use super::*;
use std::path::Path;

pub(super) fn file_name_to_lisp(ctx: &crate::emacs_core::eval::Context, path: &Path) -> Value {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;

        Value::heap_string(
            crate::emacs_core::eval::data::file_coding::decode_file_name_lisp(
                ctx,
                path.as_os_str().as_bytes(),
            ),
        )
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        let bytes = path
            .as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        Value::heap_string(neovm_text::encoding::decode_bytes_to_lisp_string(
            &bytes,
            "utf-16le",
            ctx.eol_conversion(),
        ))
    }
}

/// The `file-notify-error` data is `(MESSAGE DETAIL . OBJECTS)`.  DETAIL is an
/// [`ErrorDetail`] and nothing else, so a Rust `Display` string cannot be
/// smuggled into the slot GNU fills from `errno`.
pub(super) fn file_notify_error(
    message: &str,
    detail: Option<ErrorDetail>,
    object: Option<Value>,
) -> Flow {
    let mut tail = match object {
        Some(object) if object.is_cons() => object,
        Some(object) if !object.is_nil() => Value::list(vec![object]),
        _ => Value::NIL,
    };
    if let Some(detail) = detail {
        tail = Value::cons(Value::string(detail.to_string()), tail);
    }
    let raw_data = Value::cons(Value::string(message), tail);
    neovm_value::error::signal_with_data("file-notify-error", raw_data)
}
