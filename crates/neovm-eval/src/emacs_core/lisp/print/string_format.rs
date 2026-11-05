//! Lisp string-literal formatting with the per-call print options.
//!
//! These are the print layer's `print_string` formatters (GNU `src/print.c`):
//! their escaping decisions come from [`PrintOptions`], which is per-call
//! evaluator/print state.  The option-free escaping helpers stay in the text
//! layer (`text/string_escape`), keeping the print options out of that crate.

use super::PrintOptions;
use neovm_edit::buffer::storage_string::{scan_storage_units_auto, storage_has_special_units};

fn push_unibyte_literal_byte(out: &mut Vec<u8>, byte: u8) {
    use neovm_value::emacs_char;
    match byte {
        b'"' => out.extend_from_slice(br#"\""#),
        b'\\' => out.extend_from_slice(br#"\\"#),
        // GNU `print_object' (print.c) prints a unibyte string's high byte as
        // `printchar (BYTE8_TO_CHAR (c))' when `print-escape-nonascii' is nil --
        // i.e. it emits the raw byte (in its byte8 internal encoding), not an
        // octal escape.  Octal escaping only happens when the print target is
        // multibyte (which sets `print-escape-nonascii'), handled by the caller.
        b if b >= 0x80 => {
            let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
            let n = emacs_char::char_string(emacs_char::byte8_to_char(b), &mut buf);
            out.extend_from_slice(&buf[..n]);
        }
        b => out.push(b),
    }
}

/// Like `push_octal_escape_contextual` but uses a `u32` Emacs character code
/// for the "next character" peek (avoids requiring Rust `char`).
pub(super) fn push_octal_escape_contextual_u32(
    out: &mut Vec<u8>,
    byte: u8,
    next_code: Option<u32>,
) {
    let need_three_digits =
        byte > 0o77 || next_code.is_some_and(|nc| nc < 0x80 && (b'0'..=b'7').contains(&(nc as u8)));
    let need_two_digits = byte > 0o7;

    out.push(b'\\');
    if need_three_digits {
        out.push(b'0' + ((byte >> 6) & 7));
        out.push(b'0' + ((byte >> 3) & 7));
        out.push(b'0' + (byte & 7));
    } else if need_two_digits {
        out.push(b'0' + ((byte >> 3) & 7));
        out.push(b'0' + (byte & 7));
    } else {
        out.push(b'0' + (byte & 7));
    }
}

/// Format Emacs-encoded bytes as a Lisp string literal.
///
/// This is the canonical string printer.  It accepts `&[u8]` in Emacs internal
/// encoding (UTF-8 superset with C0/C1 overlong sequences for raw bytes).
/// The `is_multibyte` flag controls whether raw-byte characters are printed
/// with octal escapes (multibyte) or as literal bytes (unibyte).
pub(crate) fn format_lisp_string_bytes_inner_emacs(
    data: &[u8],
    is_multibyte: bool,
    options: &PrintOptions,
) -> Vec<u8> {
    use neovm_value::emacs_char;

    let mut out = Vec::with_capacity(data.len() + 2);
    out.push(b'"');

    // Track whether we just emitted a hex escape.  If so, the next
    // character might be taken as part of the hex literal; GNU Emacs
    // inserts `\ ` (backslash-space) to disambiguate.
    let mut need_nonhex = false;

    let mut pos = 0;
    while pos < data.len() {
        if is_multibyte {
            let (code, len) = emacs_char::string_char(&data[pos..]);
            // Peek at the next character for contextual octal escaping
            let next_code = if pos + len < data.len() {
                let (nc, _) = emacs_char::string_char(&data[pos + len..]);
                Some(nc.code())
            } else {
                None
            };
            pos += len;

            // Raw-byte character in multibyte string → octal escape
            if emacs_char::char_byte8_p(code.code()) {
                let byte = emacs_char::char_to_byte8(code.code());
                push_octal_escape_contextual_u32(&mut out, byte, next_code);
                need_nonhex = false;
                continue;
            }

            // print-escape-multibyte: non-ASCII in multibyte strings → \xNNNN
            if options.print_escape_multibyte && code.code() > 0x7F {
                let hex = format!("\\x{:04x}", code.code());
                out.extend_from_slice(hex.as_bytes());
                need_nonhex = true;
                continue;
            }

            // ASCII-range handling
            if code.code() <= 0x7F {
                let b = code.code() as u8;
                if is_hex_digit(b) && need_nonhex {
                    out.extend_from_slice(b"\\ ");
                    out.push(b);
                    need_nonhex = false;
                    continue;
                }
                if b == b'\n' && options.print_escape_newlines {
                    out.extend_from_slice(b"\\n");
                    need_nonhex = false;
                    continue;
                }
                if b == 0x0c && options.print_escape_newlines {
                    out.extend_from_slice(b"\\f");
                    need_nonhex = false;
                    continue;
                }
                if b == b'"' || b == b'\\' {
                    out.push(b'\\');
                    out.push(b);
                    need_nonhex = false;
                    continue;
                }
                if options.print_escape_control_characters && (b < 0x20 || b == 0x7f) {
                    push_octal_escape_contextual_u32(&mut out, b, next_code);
                    need_nonhex = false;
                    continue;
                }
                out.push(b);
                need_nonhex = false;
                continue;
            }

            // Non-ASCII Unicode character: emit as Emacs encoding bytes
            let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
            let n = emacs_char::char_string(code.code(), &mut buf);
            out.extend_from_slice(&buf[..n]);
            need_nonhex = false;
        } else {
            // Unibyte string: each byte is one character
            let byte = data[pos];
            let next_byte = data.get(pos + 1).copied();
            pos += 1;

            if byte >= 0x80 {
                if options.print_escape_nonascii {
                    push_octal_escape_contextual_u32(&mut out, byte, next_byte.map(|b| b as u32));
                } else {
                    push_unibyte_literal_byte(&mut out, byte);
                }
                need_nonhex = false;
                continue;
            }

            // ASCII byte
            if is_hex_digit(byte) && need_nonhex {
                out.extend_from_slice(b"\\ ");
                out.push(byte);
                need_nonhex = false;
                continue;
            }
            if byte == b'\n' && options.print_escape_newlines {
                out.extend_from_slice(b"\\n");
                need_nonhex = false;
                continue;
            }
            if byte == 0x0c && options.print_escape_newlines {
                out.extend_from_slice(b"\\f");
                need_nonhex = false;
                continue;
            }
            if byte == b'"' || byte == b'\\' {
                out.push(b'\\');
                out.push(byte);
                need_nonhex = false;
                continue;
            }
            if options.print_escape_control_characters && (byte < 0x20 || byte == 0x7f) {
                push_octal_escape_contextual_u32(&mut out, byte, next_byte.map(|b| b as u32));
                need_nonhex = false;
                continue;
            }
            out.push(byte);
            need_nonhex = false;
        }
    }

    out.push(b'"');
    out
}

/// Backward-compat wrapper: format a Rust `&str` (old sentinel-encoded strings).
/// This delegates to the new Emacs-byte-based formatter via a simple UTF-8 →
/// Emacs encoding pass (for pure Unicode text this is a no-op).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn format_lisp_string_bytes_inner(s: &str, options: &PrintOptions) -> Vec<u8> {
    if storage_has_special_units(s) {
        use neovm_value::emacs_char;

        let units = scan_storage_units_auto(s);
        let is_multibyte = units.iter().any(|unit| unit.code > 0xFF);
        let mut data = Vec::with_capacity(s.len());
        for unit in units {
            if !is_multibyte {
                data.push(unit.code as u8);
                continue;
            }

            let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
            let n = emacs_char::char_string(unit.code, &mut buf);
            data.extend_from_slice(&buf[..n]);
        }
        return format_lisp_string_bytes_inner_emacs(&data, is_multibyte, options);
    }

    format_lisp_string_bytes_inner_emacs(s.as_bytes(), true, options)
}

/// Format a `LispString` as an Emacs Lisp string literal (UTF-8 `String` output).
pub(crate) fn format_lisp_string_emacs(
    ls: &neovm_value::heap_types::LispString,
    options: &PrintOptions,
) -> String {
    let bytes = format_lisp_string_bytes_inner_emacs(ls.as_bytes(), ls.is_multibyte(), options);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Format a `LispString` as an Emacs Lisp string literal byte sequence.
pub(crate) fn format_lisp_string_bytes_emacs(
    ls: &neovm_value::heap_types::LispString,
    options: &PrintOptions,
) -> Vec<u8> {
    format_lisp_string_bytes_inner_emacs(ls.as_bytes(), ls.is_multibyte(), options)
}

fn is_hex_digit(b: u8) -> bool {
    b.is_ascii_hexdigit()
}
