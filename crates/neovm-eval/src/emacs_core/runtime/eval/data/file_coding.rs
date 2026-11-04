//! File-name coding helpers (stage-4 K2 oracleb batch 4).
//!
//! GNU `decode_file_name` and the write-region coding-system resolution were
//! moved here verbatim from `system/fileio`: they read only evaluator
//! variables/buffer slots plus the lower encoding layer, so the evaluator
//! files that call them (file-notify's path-to-Lisp conversion and the
//! native fns write path) do not name the core system module.  The core
//! fileio module re-exports the names its remaining callers use.

use neovm_value::intern::{SymId, intern, resolve_sym};
use neovm_value::value::{Value, ValueKind};

/// GNU `decode_file_name` (`src/coding.c`): decode FNAME's file-name bytes
/// back to a multibyte string using `file-name-coding-system`, then
/// `default-file-name-coding-system`, then (when both are nil) identity.
/// Returns the decoded multibyte `LispString`.
pub fn decode_file_name_lisp(
    eval: &crate::emacs_core::eval::Context,
    bytes: &[u8],
) -> neovm_value::heap_types::LispString {
    let coding =
        coding_system_value_to_name(&eval.visible_variable_value_or_nil("file-name-coding-system"))
            .or_else(|| {
                coding_system_value_to_name(
                    &eval.visible_variable_value_or_nil("default-file-name-coding-system"),
                )
            });
    let decoded = match coding {
        Some(name) => {
            neovm_text::encoding::decode_bytes_to_lisp_string(bytes, &name, eval.eol_conversion())
        }
        // Both variables nil: GNU returns the name unchanged (unibyte bytes).
        None => neovm_value::heap_types::LispString::from_unibyte(bytes.to_vec()),
    };

    // GNU's `decode_file_name' uses `convert_string_nocopy': when decoding
    // yields a string equal to the original unibyte filename, it returns the
    // original object.  In particular, ASCII directory entries remain
    // unibyte under UTF-8 while a name containing decoded non-ASCII becomes
    // multibyte.  Preserve that representation distinction, not merely the
    // bytes, because Lisp observes it through `multibyte-string-p'.
    if decoded.schars() == bytes.len() && decoded.as_bytes() == bytes {
        neovm_value::heap_types::LispString::from_unibyte(bytes.to_vec())
    } else {
        decoded
    }
}

/// Typed fallback for operations that share GNU's write-coding precedence but
/// differ when neither a dynamic nor a buffer coding system is selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteCodingFallback {
    Utf8,
    RawText,
}

/// Resolve the coding system to use for writing as an interned protocol value.
///
/// Priority:
/// 1. `coding-system-for-write` (if bound and non-nil)
/// 2. the buffer's visible `buffer-file-coding-system`
/// 3. the caller's operation-specific fallback
pub fn resolve_write_coding_system(
    eval: &crate::emacs_core::eval::Context,
    buffer_id: neovm_edit::buffer::BufferId,
    fallback: WriteCodingFallback,
) -> neovm_text::encoding::RuntimeCodingSystem {
    neovm_text::encoding::RuntimeCodingSystem::from_symbol(resolve_write_coding_system_symbol(
        eval, buffer_id, fallback,
    ))
}

pub fn resolve_write_coding_system_symbol(
    eval: &crate::emacs_core::eval::Context,
    buffer_id: neovm_edit::buffer::BufferId,
    fallback: WriteCodingFallback,
) -> SymId {
    // 1. Check coding-system-for-write
    let coding_system_for_write = eval.visible_variable_value_or_nil("coding-system-for-write");
    if let Some(name) = coding_system_value_to_name(&coding_system_for_write) {
        return intern(&name);
    }

    // 2. Read the visible per-buffer slot, not only an explicitly local
    // override.  GNU's BVAR accessor sees the inherited slot value too.
    if let Some(buf) = eval.edit.buffers.get(buffer_id)
        && let Some(val) = buf.buffer_local_value("buffer-file-coding-system")
        && let Some(name) = coding_system_value_to_name(&val)
    {
        return intern(&name);
    }

    match fallback {
        WriteCodingFallback::Utf8 => intern("utf-8"),
        WriteCodingFallback::RawText => intern("raw-text"),
    }
}

/// Extract a coding system name from a `Value` (symbol or string).
/// Returns `None` for nil / unrecognized types.
pub fn coding_system_value_to_name(val: &Value) -> Option<String> {
    match val.kind() {
        ValueKind::Nil => None,
        ValueKind::Symbol(id) => {
            let name = resolve_sym(id).to_owned();
            if name == "nil" { None } else { Some(name) }
        }
        ValueKind::String => {
            // Coding-system names are ASCII protocol identifiers; decode lossily.
            let name = val
                .as_lisp_string()
                .map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes()))
                .unwrap_or_default();
            if name.is_empty() || name == "nil" {
                None
            } else {
                Some(name)
            }
        }
        _ => None,
    }
}
