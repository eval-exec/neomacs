//! Shared Lisp string escaping helpers.

// The historical `emacs_core::string_escape` path keeps every moved item
// nameable (stage-3 C7).  The buffer substrate calls the decode cluster
// directly; in a library build the remaining names are reached from tests and
// from the text layer's own callers, so most are unused here.
#[allow(unused_imports)]
#[doc(hidden)]
pub use neovm_edit::buffer::storage_string::{
    StorageUnit, bytes_to_unibyte_storage_string, decode_storage_char_codes,
    decode_storage_char_codes_auto, decode_storage_units, encode_nonunicode_char_for_storage,
    scan_storage_units, scan_storage_units_auto, storage_byte_len, storage_byte_to_logical_byte,
    storage_char_len, storage_code_step, storage_has_special_units,
    storage_logical_byte_to_storage_byte, storage_string_contains_unibyte_bytes,
    storage_string_to_buffer_bytes,
};

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
#[doc(hidden)]
pub fn octal_escape_unibyte_eight_bit(bytes: &[u8]) -> Vec<u8> {
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

// ---------------------------------------------------------------------------
// Emacs-encoding-native utilities (replace sentinel-based versions)
// ---------------------------------------------------------------------------

/// Compute Emacs display width from bytes in Emacs internal encoding.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn display_width_emacs(data: &[u8], is_multibyte: bool) -> usize {
    use neovm_value::emacs_char;
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
