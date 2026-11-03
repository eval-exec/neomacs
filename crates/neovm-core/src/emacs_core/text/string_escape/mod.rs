//! Shared Lisp string escaping helpers.

// The historical `emacs_core::string_escape` path keeps every moved item
// nameable (stage-3 C7).  The buffer substrate calls the decode cluster
// directly; in a library build the remaining names are reached from tests and
// from the text layer's own callers, so most are unused here.
#[allow(unused_imports)]
pub(crate) use crate::buffer::storage_string::{
    StorageUnit, bytes_to_unibyte_storage_string, decode_storage_char_codes,
    decode_storage_char_codes_auto, decode_storage_units, encode_nonunicode_char_for_storage,
    scan_storage_units, scan_storage_units_auto, storage_byte_len, storage_byte_to_logical_byte,
    storage_char_len, storage_code_step, storage_has_special_units,
    storage_logical_byte_to_storage_byte, storage_string_contains_unibyte_bytes,
    storage_string_to_buffer_bytes,
};

fn push_unibyte_literal_byte(out: &mut Vec<u8>, byte: u8) {
    use crate::emacs_core::emacs_char;
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

fn push_octal_escape(out: &mut Vec<u8>, byte: u8) {
    out.push(b'\\');
    out.push(b'0' + ((byte >> 6) & 7));
    out.push(b'0' + ((byte >> 3) & 7));
    out.push(b'0' + (byte & 7));
}

/// Render a unibyte string's bytes the way GNU's `print_string`
/// (`src/print.c`) does under `princ` (escapeflag=false) when the string is a
/// nested element printed via `print_object`: raw eight-bit bytes (`>= 0x80`)
/// are octal-escaped as `\NNN` (via `string_escape_byte8`), while ASCII and
/// control bytes pass through verbatim and there are no surrounding quotes.
/// Used by the princ printer for nested strings (e.g. a byte-code object's
/// code string), so `(format "%s" (byte-compile …))` matches GNU byte-for-byte.
pub(crate) fn octal_escape_unibyte_eight_bit(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for &b in bytes {
        if b >= 0x80 {
            push_octal_escape(&mut out, b);
        } else {
            out.push(b);
        }
    }
    out
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn push_escaped_literal_byte(out: &mut Vec<u8>, byte: u8) {
    match byte {
        b'"' => out.extend_from_slice(br#"\""#),
        b'\\' => out.extend_from_slice(br#"\\"#),
        0x08 => out.extend_from_slice(br#"\b"#),
        b'\t' => out.extend_from_slice(br#"\t"#),
        b'\n' => out.extend_from_slice(br#"\n"#),
        0x0B => out.extend_from_slice(br#"\v"#),
        0x0C => out.extend_from_slice(br#"\f"#),
        b'\r' => out.extend_from_slice(br#"\r"#),
        0x07 => out.extend_from_slice(br#"\a"#),
        0x1B => out.extend_from_slice(br#"\e"#),
        b if b < 0x20 || b == 0x7F => push_octal_escape(out, b),
        b => out.push(b),
    }
}

use super::print::PrintOptions;

/// Format a `LispString` as an Emacs Lisp string literal (UTF-8 `String` output).
pub(crate) fn format_lisp_string_emacs(
    ls: &crate::heap_types::LispString,
    options: &PrintOptions,
) -> String {
    let bytes = format_lisp_string_bytes_inner_emacs(ls.as_bytes(), ls.is_multibyte(), options);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Format a `LispString` as an Emacs Lisp string literal byte sequence.
pub(crate) fn format_lisp_string_bytes_emacs(
    ls: &crate::heap_types::LispString,
    options: &PrintOptions,
) -> Vec<u8> {
    format_lisp_string_bytes_inner_emacs(ls.as_bytes(), ls.is_multibyte(), options)
}

/// Push an octal escape for a character code, choosing 1-3 digits.
/// If the next character after the octal escape is an octal digit ('0'–'7'),
/// we must use 3 digits to avoid misinterpretation (matching GNU's `octalout`).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn push_octal_escape_contextual(out: &mut Vec<u8>, byte: u8, next_char: Option<char>) {
    let need_three_digits = byte > 0o77
        || next_char.is_some_and(|nc| {
            let nc_u32 = nc as u32;
            // Check if the next character is an ASCII octal digit.
            // For sentinel chars, don't force 3 digits.
            nc_u32 < 0x80 && (b'0'..=b'7').contains(&(nc_u32 as u8))
        });
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
    use crate::emacs_core::emacs_char;

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
pub(crate) fn format_lisp_string_bytes_inner(s: &str, options: &PrintOptions) -> Vec<u8> {
    if storage_has_special_units(s) {
        use crate::emacs_core::emacs_char;

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

fn is_hex_digit(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

// ---------------------------------------------------------------------------
// Emacs-encoding-native utilities (replace sentinel-based versions)
// ---------------------------------------------------------------------------

/// Compute Emacs display width from bytes in Emacs internal encoding.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn display_width_emacs(data: &[u8], is_multibyte: bool) -> usize {
    use crate::emacs_core::emacs_char;
    if is_multibyte {
        let mut width = 0usize;
        let mut pos = 0;
        while pos < data.len() {
            let (code, len) = emacs_char::string_char(&data[pos..]);
            pos += len;
            if emacs_char::char_byte8_p(code.code()) {
                width += 4; // raw bytes display as \xNN (4 chars)
            } else if let Some(ch) = char::from_u32(code.code()) {
                width += crate::encoding::char_width(ch);
            } else {
                width += 1;
            }
        }
        width
    } else {
        let mut width = 0usize;
        for &b in data {
            if b < 0x80 {
                if let Some(ch) = char::from_u32(b as u32) {
                    width += crate::encoding::char_width(ch);
                } else {
                    width += 1;
                }
            } else {
                width += 1; // unibyte non-ASCII bytes are 1 column
            }
        }
        width
    }
}

#[cfg(test)]
#[path = "tests/string_escape_test.rs"]
mod tests;
