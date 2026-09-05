//! Additional fns.c builtins for the Elisp interpreter.
//!
//! Implements: base64 encode/decode, md5, secure-hash, buffer-hash,
//! locale-info, eql, equal-including-properties, widget-get/put/apply,
//! identity, string-to-multibyte/unibyte, string-make-multibyte/unibyte,
//! compare-strings, string-version-lessp, string-collate-lessp/equalp.

use super::error::{EvalResult, Flow, signal};
use super::eval::Context;
use super::intern::{intern, resolve_sym};
use crate::emacs_core::builtins::{FromValue, StringDesignator};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_min_args};
// bytes_to_unibyte_storage_string and encode_nonunicode_char_for_storage
// imports removed — using emacs_char + LispString directly
use super::value::*;
use crate::buffer::{
    BufferManager, CharPos0, CharRange, EmacsByteRange, LispCharPos1, TextEditRange,
};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512};
use std::borrow::Cow;
use std::ffi::CString;

// Sentinel constants removed — no longer needed with Vec<u8> LispString

#[cfg(unix)]
unsafe extern "C" {
    #[link_name = "towlower"]
    fn c_towlower(wc: libc::c_uint) -> libc::c_uint;
    #[link_name = "towlower_l"]
    fn c_towlower_l(wc: libc::c_uint, locale: libc::locale_t) -> libc::c_uint;
    #[link_name = "wcscoll"]
    fn c_wcscoll(ws1: *const libc::wchar_t, ws2: *const libc::wchar_t) -> libc::c_int;
    #[link_name = "wcscoll_l"]
    fn c_wcscoll_l(
        ws1: *const libc::wchar_t,
        ws2: *const libc::wchar_t,
        locale: libc::locale_t,
    ) -> libc::c_int;
}

#[cfg(any(
    target_os = "linux",
    target_os = "emscripten",
    target_os = "dragonfly",
    target_os = "hurd"
))]
unsafe fn collation_errno_location() -> *mut libc::c_int {
    unsafe { libc::__errno_location() }
}

#[cfg(any(target_os = "android", target_os = "netbsd", target_os = "openbsd"))]
unsafe fn collation_errno_location() -> *mut libc::c_int {
    unsafe { libc::__errno() }
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "freebsd"
))]
unsafe fn collation_errno_location() -> *mut libc::c_int {
    unsafe { libc::__error() }
}

#[cfg(unix)]
unsafe fn set_collation_errno(value: libc::c_int) {
    unsafe {
        *collation_errno_location() = value;
    }
}

#[cfg(unix)]
unsafe fn collation_errno() -> libc::c_int {
    unsafe { *collation_errno_location() }
}

fn collation_errno_message(errno: libc::c_int) -> String {
    std::io::Error::from_raw_os_error(errno).to_string()
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn string_designator_text(eval: &mut Context, val: Value) -> Result<String, Flow> {
    let designator = StringDesignator::from_value(eval, val)?;
    Ok(crate::emacs_core::emacs_char::to_utf8_lossy(
        designator.text().as_bytes(),
    ))
}

fn require_int(val: &Value) -> Result<i64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *val],
        )),
    }
}

fn require_int_or_marker(val: &Value) -> Result<i64, Flow> {
    if val.is_marker() {
        return super::marker::marker_position_as_int(val);
    }
    match val.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *val],
        )),
    }
}

fn md5_known_coding_system(name: &str) -> bool {
    super::coding::CodingSystemManager::new().is_known(name)
}

fn validate_md5_coding_system_arg(args: &[Value]) -> Result<(), Flow> {
    let Some(coding_system) = args.get(3) else {
        return Ok(());
    };
    if coding_system.is_nil() {
        return Ok(());
    }

    let noerror = args.get(4).is_some_and(|v| v.is_truthy());
    let valid = match coding_system.kind() {
        ValueKind::Symbol(id) => md5_known_coding_system(resolve_sym(id)),
        _ => false,
    };

    if valid || noerror {
        Ok(())
    } else {
        Err(signal(
            LispCondition::CodingSystemError,
            vec![*coding_system],
        ))
    }
}

fn md5_coding_system_name(args: &[Value]) -> Option<String> {
    let coding_system = args.get(3)?;
    if coding_system.is_nil() {
        return None;
    }
    let noerror = args.get(4).is_some_and(|v| v.is_truthy());
    match coding_system.kind() {
        ValueKind::Symbol(id) if md5_known_coding_system(resolve_sym(id)) => {
            Some(resolve_sym(id).to_owned())
        }
        _ if noerror => Some("raw-text".to_string()),
        _ => None,
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

// ---------------------------------------------------------------------------
// Base64 alphabet tables
// ---------------------------------------------------------------------------

const B64_STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Build a GNU-style decode table.
///
/// GNU `base64_char_to_value` uses:
/// - `-1` for always-ignored whitespace,
/// - `0` for invalid input and `=`,
/// - `1..=64` for represented values plus one.
fn build_decode_table(alphabet: &[u8; 64]) -> [i8; 256] {
    let mut table = [0i8; 256];
    for b in [b'\t', b'\n', 0x0c, b'\r', b' '] {
        table[b as usize] = -1;
    }
    for (i, &ch) in alphabet.iter().enumerate() {
        table[ch as usize] = i as i8 + 1;
    }
    table
}

// ---------------------------------------------------------------------------
// Base64 encode (manual implementation)
// ---------------------------------------------------------------------------

/// Standard-alphabet Base64 with no padding and no line breaks — used by the
/// UTF-7 codec (RFC 2152 modified Base64).
pub(crate) fn base64_standard_encode_unpadded(input: &[u8]) -> String {
    base64_encode(input, B64_STD, false, false)
}

/// Decode standard-alphabet Base64 (the caller re-pads to a multiple of 4),
/// returning the bytes or `None` on malformed input. Used by the UTF-7 codec.
pub(crate) fn base64_standard_decode(input: &[u8]) -> Option<Vec<u8>> {
    let table = build_decode_table(B64_STD);
    base64_decode(input, &table, false, false).ok()
}

/// MIME line length used by GNU's base64 line-breaking (fns.c).
const MIME_LINE_LENGTH: usize = 76;

fn base64_encode(input: &[u8], alphabet: &[u8; 64], pad: bool, line_break: bool) -> String {
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4 + input.len() / 57);
    // Mirror GNU `base64_encode_1` (fns.c): wrap a line every
    // MIME_LINE_LENGTH/4 base64 groups, inserting '\n' *between* lines only.
    // `counter` counts the groups emitted on the current line; the separator
    // is emitted before a group when the previous line is full, so a final
    // full line never gets a trailing newline.
    let mut counter = 0usize;

    let chunks = input.chunks(3);
    for chunk in chunks {
        if line_break {
            if counter < MIME_LINE_LENGTH / 4 {
                counter += 1;
            } else {
                out.push(b'\n');
                counter = 1;
            }
        }

        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(alphabet[((triple >> 18) & 0x3F) as usize]);
        out.push(alphabet[((triple >> 12) & 0x3F) as usize]);

        if chunk.len() > 1 {
            out.push(alphabet[((triple >> 6) & 0x3F) as usize]);
        } else if pad {
            out.push(b'=');
        }

        if chunk.len() > 2 {
            out.push(alphabet[(triple & 0x3F) as usize]);
        } else if pad {
            out.push(b'=');
        }
    }

    // Safety: we only pushed ASCII bytes
    unsafe { String::from_utf8_unchecked(out) }
}

/// Flatten the internal bytes of base64-encoding input into the raw data
/// bytes to be encoded, mirroring GNU `base64_encode_1` (fns.c): when the
/// source is multibyte, each character is decoded; ASCII passes through, an
/// eight-bit raw-byte character collapses to its byte, and any other
/// multibyte character (≥ 128) is rejected with GNU's error.
fn base64_encode_source_bytes(source: &[u8], multibyte: bool) -> Result<Cow<'_, [u8]>, Flow> {
    if !multibyte {
        return Ok(Cow::Borrowed(source));
    }

    let mut bytes = Vec::with_capacity(source.len());
    let mut pos = 0;
    while pos < source.len() {
        let (ch, len) = super::emacs_char::string_char(&source[pos..]);
        if ch <= 0x7f {
            bytes.push(ch as u8);
        } else if super::emacs_char::char_byte8_p(ch) {
            bytes.push(super::emacs_char::char_to_byte8(ch));
        } else {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Multibyte character in data for base64 encoding",
                )],
            ));
        }
        pos += len;
    }
    Ok(Cow::Owned(bytes))
}

fn base64_encode_string_bytes(
    string: &crate::heap_types::LispString,
) -> Result<Cow<'_, [u8]>, Flow> {
    base64_encode_source_bytes(string.as_bytes(), string.is_multibyte())
}

// ---------------------------------------------------------------------------
// Base64 decode (manual implementation)
// ---------------------------------------------------------------------------

fn base64_decode(
    input: &[u8],
    table: &[i8; 256],
    base64url: bool,
    ignore_invalid: bool,
) -> Result<Vec<u8>, ()> {
    base64_decode_bytes(input, table, base64url, ignore_invalid)
}

/// Build the LispString that base64-decode-region inserts into the buffer.
///
/// Mirrors GNU `base64_decode_1`'s `multibyte_bit` handling (fns.c): for a
/// multibyte target each decoded raw byte 0x80-0xFF must be stored as its
/// two-byte eight-bit internal encoding (BYTE8_STRING / `str_to_multibyte`),
/// not as a raw byte (which is not a valid multibyte lead byte and would
/// corrupt the buffer's internal representation). For a unibyte target the
/// raw bytes are stored verbatim.
fn decoded_bytes_to_lisp_string(bytes: Vec<u8>, multibyte: bool) -> crate::heap_types::LispString {
    if multibyte {
        crate::heap_types::LispString::from_emacs_bytes(super::emacs_char::str_to_multibyte(&bytes))
    } else {
        crate::heap_types::LispString::from_unibyte(bytes)
    }
}

fn base64_next_value(
    input: &[u8],
    pos: &mut usize,
    table: &[i8; 256],
    ignore_invalid: bool,
) -> Option<(u8, i8)> {
    while *pos < input.len() {
        let byte = input[*pos];
        *pos += 1;
        let value = table[byte as usize];
        if value < 0 || (value == 0 && ignore_invalid) {
            continue;
        }
        return Some((byte, value));
    }
    None
}

fn base64_next_non_ignorable(input: &[u8], pos: &mut usize, table: &[i8; 256]) -> Option<u8> {
    while *pos < input.len() {
        let byte = input[*pos];
        *pos += 1;
        if table[byte as usize] < 0 {
            continue;
        }
        return Some(byte);
    }
    None
}

fn base64_decode_bytes(
    input: &[u8],
    table: &[i8; 256],
    base64url: bool,
    ignore_invalid: bool,
) -> Result<Vec<u8>, ()> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut pos = 0usize;

    loop {
        let Some((_c1, v1)) = base64_next_value(input, &mut pos, table, ignore_invalid) else {
            return Ok(out);
        };
        if v1 == 0 {
            return Err(());
        }
        let mut value = ((v1 - 1) as u32) << 18;

        let Some((_c2, v2)) = base64_next_value(input, &mut pos, table, ignore_invalid) else {
            return Err(());
        };
        if v2 == 0 {
            return Err(());
        }
        value |= ((v2 - 1) as u32) << 12;
        out.push(((value >> 16) & 0xff) as u8);

        let Some((c3, v3)) = base64_next_value(input, &mut pos, table, ignore_invalid) else {
            if !base64url && !ignore_invalid {
                return Err(());
            }
            return Ok(out);
        };
        if c3 == b'=' {
            let Some(c4) = base64_next_non_ignorable(input, &mut pos, table) else {
                return Err(());
            };
            if c4 != b'=' {
                return Err(());
            }
            continue;
        }
        if v3 == 0 {
            return Err(());
        }
        value |= ((v3 - 1) as u32) << 6;
        out.push(((value >> 8) & 0xff) as u8);

        let Some((c4, v4)) = base64_next_value(input, &mut pos, table, ignore_invalid) else {
            if !base64url && !ignore_invalid {
                return Err(());
            }
            return Ok(out);
        };
        if c4 == b'=' {
            continue;
        }
        if v4 == 0 {
            return Err(());
        }
        value |= (v4 - 1) as u32;
        out.push((value & 0xff) as u8);
    }
}

// ---------------------------------------------------------------------------
// Base64 builtins
// ---------------------------------------------------------------------------

/// (base64-encode-string STRING &optional NO-LINE-BREAK)
pub(crate) fn builtin_base64_encode_string(args: Vec<Value>) -> EvalResult {
    expect_args_range("base64-encode-string", &args, 1, 2)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let no_line_break = args.get(1).is_some_and(|v| v.is_truthy());
    let source = base64_encode_string_bytes(ls)?;
    let encoded = base64_encode(&source, B64_STD, true, !no_line_break);
    Ok(Value::string(encoded))
}

/// (base64-decode-string STRING &optional BASE64URL IGNORE-INVALID)
pub(crate) fn builtin_base64_decode_string(args: Vec<Value>) -> EvalResult {
    expect_args_range("base64-decode-string", &args, 1, 3)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let use_url = args.get(1).is_some_and(|v| v.is_truthy());
    let table = if use_url {
        build_decode_table(B64_URL)
    } else {
        build_decode_table(B64_STD)
    };
    let ignore_invalid = args.get(2).is_some_and(|v| v.is_truthy());
    // base64 data is ASCII, so LispString.as_bytes() equals the storage
    // form for valid input. Decoded bytes are arbitrary; return as a
    // unibyte LispString to match GNU Fbase64_decode_string (fns.c) which
    // returns a unibyte string.
    match base64_decode_bytes(ls.as_bytes(), &table, use_url, ignore_invalid) {
        Ok(bytes) => Ok(Value::heap_string(
            crate::heap_types::LispString::from_unibyte(bytes),
        )),
        Err(()) => Err(signal("error", vec![Value::string("Invalid base64 data")])),
    }
}

/// (base64url-encode-string STRING &optional NO-PAD)
pub(crate) fn builtin_base64url_encode_string(args: Vec<Value>) -> EvalResult {
    expect_args_range("base64url-encode-string", &args, 1, 2)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let no_pad = args.get(1).is_some_and(|v| v.is_truthy());
    let source = base64_encode_string_bytes(ls)?;
    let encoded = base64_encode(&source, B64_URL, !no_pad, false);
    Ok(Value::string(encoded))
}

/// (base64url-decode-string STRING &optional IGNORE-INVALID)
#[cfg(test)]
pub(crate) fn builtin_base64url_decode_string(args: Vec<Value>) -> EvalResult {
    expect_args_range("base64url-decode-string", &args, 1, 2)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let table = build_decode_table(B64_URL);
    let ignore_invalid = args.get(1).is_some_and(|v| v.is_truthy());
    match base64_decode_bytes(ls.as_bytes(), &table, true, ignore_invalid) {
        Ok(bytes) => Ok(Value::heap_string(
            crate::heap_types::LispString::from_unibyte(bytes),
        )),
        Err(()) => Ok(Value::NIL),
    }
}

pub(crate) fn normalize_current_buffer_region_bounds_in_manager(
    buffers: &BufferManager,
    start_arg: &Value,
    end_arg: &Value,
) -> Result<(crate::buffer::BufferId, EmacsByteRange), Flow> {
    let region = super::position::LispRegionArgs::from_values(buffers, *start_arg, *end_arg)?;
    let buffer_id = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        .id;

    let buf = buffers
        .get(buffer_id)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;

    Ok((buffer_id, region.accessible_byte_range(buf)?))
}

#[derive(Clone, Copy, Debug)]
struct ValidatedBufferLispRegion {
    start: LispCharPos1,
    end: LispCharPos1,
}

impl ValidatedBufferLispRegion {
    fn from_optional_values(
        start_raw: Option<&Value>,
        end_raw: Option<&Value>,
        default_start: LispCharPos1,
        default_end: LispCharPos1,
        start_arg: &Value,
        end_arg: &Value,
    ) -> Result<Self, Flow> {
        Self {
            start: normalize_optional_lisp_region_position(start_raw, default_start)?,
            end: normalize_optional_lisp_region_position(end_raw, default_end)?,
        }
        .validate_bounds(default_start, default_end, start_arg, end_arg)
    }

    fn validate_bounds(
        self,
        point_min: LispCharPos1,
        point_max: LispCharPos1,
        start_arg: &Value,
        end_arg: &Value,
    ) -> Result<Self, Flow> {
        if self.start < point_min
            || self.start > point_max
            || self.end < point_min
            || self.end > point_max
        {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![*start_arg, *end_arg],
            ));
        }
        Ok(self)
    }

    fn ordered(self) -> (LispCharPos1, LispCharPos1) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    fn to_accessible_byte_range(self, buf: &crate::buffer::Buffer) -> EmacsByteRange {
        let (lo, hi) = self.ordered();
        EmacsByteRange::new(
            buf.lisp_pos_to_accessible_emacs_byte_pos(lo),
            buf.lisp_pos_to_accessible_emacs_byte_pos(hi),
        )
    }
}

fn normalize_optional_lisp_region_position(
    val: Option<&Value>,
    default: LispCharPos1,
) -> Result<LispCharPos1, Flow> {
    match val {
        None => Ok(default),
        Some(v) if v.is_nil() => Ok(default),
        Some(v) => Ok(LispCharPos1::new(require_int_or_marker(v)?)),
    }
}

fn checked_buffer_hash_lisp_region(
    buf: &crate::buffer::Buffer,
    start_raw: Option<&Value>,
    end_raw: Option<&Value>,
) -> Result<ValidatedBufferLispRegion, Flow> {
    let point_min = buf.point_min_lisp_char_pos();
    let point_max = buf.point_max_lisp_char_pos();
    let start_arg = start_raw.cloned().unwrap_or(Value::NIL);
    let end_arg = end_raw.cloned().unwrap_or(Value::NIL);
    ValidatedBufferLispRegion::from_optional_values(
        start_raw, end_raw, point_min, point_max, &start_arg, &end_arg,
    )
}

pub(crate) fn read_buffer_region_bytes_in_manager(
    buffers: &BufferManager,
    buffer_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
) -> Result<Vec<u8>, Flow> {
    let buf = buffers
        .get(buffer_id)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    Ok(buf.buffer_substring_bytes_range(byte_range))
}

pub(crate) fn replace_buffer_region_lisp_string_in_manager(
    buffers: &mut BufferManager,
    buffer_id: crate::buffer::BufferId,
    range: TextEditRange,
    replacement: &crate::heap_types::LispString,
) -> Result<(), Flow> {
    buffers
        .replace_buffer_measured_region_lisp_string(buffer_id, range, replacement)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    Ok(())
}

pub(crate) fn replace_buffer_emacs_byte_range_lisp_string(
    eval: &mut super::eval::Context,
    buffer_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
    replacement: &crate::heap_types::LispString,
) -> Result<(), Flow> {
    let change = super::editfns::text_change_for_lisp_string_replacement_in_manager(
        &eval.buffers,
        buffer_id,
        byte_range,
        replacement,
    )?;
    super::editfns::signal_before_text_change(eval, change)?;
    replace_buffer_region_lisp_string_in_manager(
        &mut eval.buffers,
        buffer_id,
        change.old_range(),
        replacement,
    )?;
    // Don't inherit text properties from neighbors here.
    // GNU's replace path (del_range + insert_from_gap in decode_coding)
    // also skips adjust_intervals_for_insertion.  Property inheritance
    // belongs to insert-and-inherit (insert_pieces_in_state with
    // inherit=true), not general-purpose replace operations.
    super::editfns::signal_after_text_change(eval, change)?;
    Ok(())
}

/// (base64-encode-region START END &optional NO-LINE-BREAK)
pub(crate) fn builtin_base64_encode_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("base64-encode-region", &args, 2, 3)?;
    let (buffer_id, byte_range) =
        normalize_current_buffer_region_bounds_in_manager(&eval.buffers, &args[0], &args[1])?;
    let raw = read_buffer_region_bytes_in_manager(&eval.buffers, buffer_id, byte_range)?;
    let target_multibyte = eval
        .buffers
        .get(buffer_id)
        .map(|buf| buf.get_multibyte())
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    // GNU base64_encode_region_1 passes the buffer's multibyteness to
    // base64_encode_1, which rejects non-eight-bit multibyte chars.
    let source = base64_encode_source_bytes(&raw, target_multibyte)?;
    let no_line_break = args.get(2).is_some_and(|v| v.is_truthy());
    let encoded = base64_encode(&source, B64_STD, true, !no_line_break);
    let encoded_len = encoded.len();
    let replacement =
        super::builtins::lisp_string_from_buffer_bytes(encoded.into_bytes(), target_multibyte);
    replace_buffer_emacs_byte_range_lisp_string(eval, buffer_id, byte_range, &replacement)?;
    Ok(Value::fixnum(encoded_len as i64))
}

/// (base64url-encode-region START END &optional NO-PAD)
pub(crate) fn builtin_base64url_encode_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("base64url-encode-region", &args, 2, 3)?;
    let (buffer_id, byte_range) =
        normalize_current_buffer_region_bounds_in_manager(&eval.buffers, &args[0], &args[1])?;
    let raw = read_buffer_region_bytes_in_manager(&eval.buffers, buffer_id, byte_range)?;
    let target_multibyte = eval
        .buffers
        .get(buffer_id)
        .map(|buf| buf.get_multibyte())
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    // GNU base64_encode_region_1 passes the buffer's multibyteness to
    // base64_encode_1, which rejects non-eight-bit multibyte chars.
    let source = base64_encode_source_bytes(&raw, target_multibyte)?;
    let no_pad = args.get(2).is_some_and(|v| v.is_truthy());
    let encoded = base64_encode(&source, B64_URL, !no_pad, false);
    let encoded_len = encoded.len();
    let replacement =
        super::builtins::lisp_string_from_buffer_bytes(encoded.into_bytes(), target_multibyte);
    replace_buffer_emacs_byte_range_lisp_string(eval, buffer_id, byte_range, &replacement)?;
    Ok(Value::fixnum(encoded_len as i64))
}

/// (base64-decode-region START END &optional BASE64URL NOERROR)
pub(crate) fn builtin_base64_decode_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("base64-decode-region", &args, 2, 4)?;
    let (buffer_id, byte_range) =
        normalize_current_buffer_region_bounds_in_manager(&eval.buffers, &args[0], &args[1])?;
    let source = read_buffer_region_bytes_in_manager(&eval.buffers, buffer_id, byte_range)?;
    let use_url = args.get(2).is_some_and(|v| v.is_truthy());
    let noerror = args.get(3).is_some_and(|v| v.is_truthy());
    let table = if use_url {
        build_decode_table(B64_URL)
    } else {
        build_decode_table(B64_STD)
    };
    let target_multibyte = eval
        .buffers
        .get(buffer_id)
        .map(|buf| buf.get_multibyte())
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    match base64_decode(&source, &table, use_url, noerror) {
        Ok(bytes) => {
            // GNU base64_decode_1 (fns.c): when the target buffer is
            // multibyte, each decoded raw byte 0x80-0xFF is stored as its
            // two-byte eight-bit internal encoding (BYTE8_STRING). The return
            // value is the number of inserted *characters*, which equals the
            // raw decoded byte count (one char per decoded byte).
            let inserted_chars = bytes.len();
            let replacement = decoded_bytes_to_lisp_string(bytes, target_multibyte);
            replace_buffer_emacs_byte_range_lisp_string(eval, buffer_id, byte_range, &replacement)?;
            Ok(Value::fixnum(inserted_chars as i64))
        }
        Err(()) if noerror => {
            let replacement =
                super::builtins::lisp_string_from_buffer_bytes(Vec::new(), target_multibyte);
            replace_buffer_emacs_byte_range_lisp_string(eval, buffer_id, byte_range, &replacement)?;
            Ok(Value::fixnum(0))
        }
        Err(()) => Err(signal("error", vec![Value::string("Invalid base64 data")])),
    }
}

// ---------------------------------------------------------------------------
// Hash / digest builtins
// ---------------------------------------------------------------------------

/// (md5 OBJECT &optional START END CODING-SYSTEM NOERROR)
///
/// Context-aware implementation that also supports buffer objects.
pub(crate) fn builtin_md5(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("md5", &args, 1, 5)?;
    validate_md5_coding_system_arg(&args)?;
    let object = &args[0];
    match object.kind() {
        ValueKind::String => Ok(Value::string(md5_hex_for_string(
            object,
            args.get(1),
            args.get(2),
            md5_coding_system_name(&args).as_deref(),
            eval.eol_conversion(),
        )?)),
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let input = encoded_hash_slice_for_buffer_in_context(
                eval,
                object.as_buffer_id().unwrap(),
                args.get(1),
                args.get(2),
                md5_coding_system_name(&args).map(|name| intern(&name)),
            )?;
            Ok(Value::string(md5_hash(&input)))
        }
        _ => Err(signal(
            "error",
            vec![
                Value::string("Invalid object argument"),
                invalid_object_payload(object),
            ],
        )),
    }
}

/// Compute an MD5 digest behind the GNU-compatible Lisp argument adapter.
fn md5_digest(message: &[u8]) -> [u8; 16] {
    Md5::digest(message).into()
}

fn md5_hash(message: &[u8]) -> String {
    bytes_to_hex(&md5_digest(message))
}

fn md5_hex_for_string(
    object: &Value,
    start_raw: Option<&Value>,
    end_raw: Option<&Value>,
    coding_system: Option<&str>,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> Result<String, Flow> {
    let string = object
        .as_lisp_string()
        .expect("md5_hex_for_string only accepts string object");
    let encoded;
    let (bytes, len, multibyte) = if string.is_multibyte() {
        encoded = crate::encoding::encode_lisp_string(
            string,
            coding_system.unwrap_or("utf-8"),
            eol_conversion,
        );
        (&encoded[..], encoded.len() as i64, false)
    } else {
        (
            string.as_bytes(),
            string.schars() as i64,
            string.is_multibyte(),
        )
    };
    let start_arg = start_raw.cloned().unwrap_or(Value::NIL);
    let end_arg = end_raw.cloned().unwrap_or(Value::NIL);
    let start =
        normalize_secure_hash_index(start_raw, 0, len, object, &start_arg, &end_arg)? as usize;
    let end =
        normalize_secure_hash_index(end_raw, len, len, object, &start_arg, &end_arg)? as usize;

    if start > end {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![*object, start_arg, end_arg],
        ));
    }

    let (byte_from, byte_to) = if multibyte {
        (
            crate::emacs_core::emacs_char::char_to_byte_pos(bytes, start),
            crate::emacs_core::emacs_char::char_to_byte_pos(bytes, end),
        )
    } else {
        (start, end)
    };
    if byte_to > bytes.len() {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![*object, start_arg, end_arg],
        ));
    }
    Ok(md5_hash(&bytes[byte_from..byte_to]))
}

/// Extract and externally encode a buffer region before hashing it.
///
/// GNU's `extract_data_from_object` does not expose the gap buffer's internal
/// bytes to `md5` or `secure-hash`.  It first makes a Lisp string for the
/// character region, selects the same coding policy used for writing, and runs
/// the complete coding engine.  Keeping that protocol in one helper prevents
/// individual digest algorithms from accidentally choosing a representation.
fn encoded_hash_slice_for_buffer_in_context(
    eval: &mut super::eval::Context,
    buffer_id: crate::buffer::BufferId,
    start_raw: Option<&Value>,
    end_raw: Option<&Value>,
    coding_override: Option<super::intern::SymId>,
) -> Result<Vec<u8>, Flow> {
    let (text, multibyte) = {
        let buf = eval
            .buffers
            .get(buffer_id)
            .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
        let byte_range =
            checked_buffer_hash_lisp_region(buf, start_raw, end_raw)?.to_accessible_byte_range(buf);
        (
            buf.buffer_substring_lisp_string_range(byte_range),
            buf.get_multibyte(),
        )
    };
    let fallback = if multibyte {
        super::fileio::WriteCodingFallback::Utf8
    } else {
        super::fileio::WriteCodingFallback::RawText
    };
    let coding_system = coding_override
        .map(crate::encoding::RuntimeCodingSystem::from_symbol)
        .unwrap_or_else(|| super::fileio::resolve_write_coding_system(eval, buffer_id, fallback));
    Ok(crate::encoding::encode_external_text_in_context(eval, text, coding_system)?.bytes)
}

fn secure_hash_algorithm_name(val: &Value) -> Result<String, Flow> {
    match val.kind() {
        ValueKind::Symbol(id) => Ok(resolve_sym(id).to_owned()),
        ValueKind::Nil => Ok("nil".to_string()),
        ValueKind::T => Ok("t".to_string()),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *val],
        )),
    }
}

fn normalize_secure_hash_index(
    val: Option<&Value>,
    default: i64,
    len: i64,
    object: &Value,
    start_arg: &Value,
    end_arg: &Value,
) -> Result<i64, Flow> {
    let raw = match val {
        None => default,
        Some(v) if v.is_nil() => default,
        Some(v) => require_int(v)?,
    };
    let idx = if raw < 0 { len + raw } else { raw };
    if idx < 0 || idx > len {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![*object, *start_arg, *end_arg],
        ));
    }
    Ok(idx)
}

fn invalid_object_payload(val: &Value) -> Value {
    if val.is_nil() {
        Value::string("nil")
    } else {
        *val
    }
}

fn bytes_to_lisp_binary_value(bytes: &[u8]) -> Value {
    Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes.to_vec()))
}

fn hash_slice_for_string(
    object: &Value,
    start_raw: Option<&Value>,
    end_raw: Option<&Value>,
) -> Result<Vec<u8>, Flow> {
    let string = object
        .as_lisp_string()
        .expect("hash_slice_for_string only accepts string object");
    let len = string.schars() as i64;
    let start_arg = start_raw.cloned().unwrap_or(Value::NIL);
    let end_arg = end_raw.cloned().unwrap_or(Value::NIL);
    let start =
        normalize_secure_hash_index(start_raw, 0, len, object, &start_arg, &end_arg)? as usize;
    let end =
        normalize_secure_hash_index(end_raw, len, len, object, &start_arg, &end_arg)? as usize;

    if start > end {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![*object, start_arg, end_arg],
        ));
    }

    let bytes = string.as_bytes();
    let (byte_from, byte_to) = if string.is_multibyte() {
        (
            crate::emacs_core::emacs_char::char_to_byte_pos(bytes, start),
            crate::emacs_core::emacs_char::char_to_byte_pos(bytes, end),
        )
    } else {
        (start, end)
    };
    if byte_to > bytes.len() {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![*object, start_arg, end_arg],
        ));
    }
    Ok(bytes[byte_from..byte_to].to_vec())
}

fn secure_hash_digest_bytes(algo_name: &str, input: &[u8]) -> Result<Vec<u8>, Flow> {
    let digest = match algo_name {
        "md5" => md5_digest(input).to_vec(),
        "sha1" => {
            let mut h = Sha1::new();
            h.update(input);
            h.finalize().to_vec()
        }
        "sha224" => {
            let mut h = Sha224::new();
            h.update(input);
            h.finalize().to_vec()
        }
        "sha256" => {
            let mut h = Sha256::new();
            h.update(input);
            h.finalize().to_vec()
        }
        "sha384" => {
            let mut h = Sha384::new();
            h.update(input);
            h.finalize().to_vec()
        }
        "sha512" => {
            let mut h = Sha512::new();
            h.update(input);
            h.finalize().to_vec()
        }
        _ => {
            return Err(signal(
                "error",
                vec![Value::string(format!("Invalid algorithm arg: {algo_name}"))],
            ));
        }
    };
    Ok(digest)
}

/// (secure-hash ALGORITHM OBJECT &optional START END BINARY)
///
/// Context-aware implementation that also supports buffer objects.
pub(crate) fn builtin_secure_hash(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("secure-hash", &args, 2, 5)?;
    let algo_name = secure_hash_algorithm_name(&args[0])?;

    let object = &args[1];
    let input = match object.kind() {
        ValueKind::String => hash_slice_for_string(object, args.get(2), args.get(3))?,
        ValueKind::Veclike(VecLikeType::Buffer) => encoded_hash_slice_for_buffer_in_context(
            eval,
            object.as_buffer_id().unwrap(),
            args.get(2),
            args.get(3),
            None,
        )?,
        _ => {
            return Err(signal(
                "error",
                vec![
                    Value::string("Invalid object argument"),
                    invalid_object_payload(object),
                ],
            ));
        }
    };

    let digest = secure_hash_digest_bytes(&algo_name, &input)?;
    let binary = args.get(4).is_some_and(|v| v.is_truthy());
    if binary {
        Ok(bytes_to_lisp_binary_value(&digest))
    } else {
        Ok(Value::string(bytes_to_hex(&digest)))
    }
}

/// (buffer-hash &optional BUFFER-OR-NAME)
/// Context-aware implementation used at runtime.
pub(crate) fn builtin_buffer_hash(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("buffer-hash", &args, 0, 1)?;

    let buffer_id = if args.is_empty() || args[0].is_nil() {
        eval.buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
            .id
    } else {
        match args[0].kind() {
            ValueKind::Veclike(VecLikeType::Buffer) => args[0].as_buffer_id().unwrap(),
            ValueKind::String => {
                let name = crate::emacs_core::emacs_char::to_utf8_lossy(
                    require_lisp_string(&args[0])?.as_bytes(),
                );
                eval.buffers.find_buffer_by_name(&name).ok_or_else(|| {
                    signal(
                        "error",
                        vec![Value::string(format!("No buffer named {name}"))],
                    )
                })?
            }
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), args[0]],
                ));
            }
        }
    };

    // GNU Emacs accepts killed buffer objects and hashes as empty content.
    //
    // GNU Fbuffer_hash (fns.c) hashes BUF_BEG_BYTE..BUF_Z_BYTE — the whole
    // buffer, *ignoring narrowing* (unlike md5/secure-hash, which use the
    // accessible region). Use the full byte range here.
    let text = eval
        .buffers
        .get(buffer_id)
        .map(|buf| buf.buffer_substring_bytes_range(buf.full_emacs_byte_range()))
        .unwrap_or_default();

    let mut hasher = Sha1::new();
    hasher.update(&text);
    Ok(Value::string(bytes_to_hex(&hasher.finalize())))
}

/// (equal-including-properties O1 O2)
/// Like `equal`, but also compares text properties of strings.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_equal_including_properties(args: Vec<Value>) -> EvalResult {
    expect_args("equal-including-properties", &args, 2)?;
    Ok(Value::bool_val(try_equal_value_including_properties(
        &args[0], &args[1], 0,
    )?))
}

pub(crate) fn builtin_equal_including_properties_2(
    eval: &mut crate::emacs_core::eval::Context,
    left: Value,
    right: Value,
) -> EvalResult {
    Ok(Value::bool_val(try_equal_value_including_properties_swp(
        &left,
        &right,
        0,
        eval.symbols_with_pos_enabled,
    )?))
}

// ---------------------------------------------------------------------------
// Widget helpers
// ---------------------------------------------------------------------------

/// (widget-get WIDGET PROPERTY)
/// WIDGET is a list (plist-like).  Extract PROPERTY from the widget's plist
/// tail (skip car which is the widget type).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_widget_get(args: Vec<Value>) -> EvalResult {
    expect_args("widget-get", &args, 2)?;
    let widget = &args[0];
    let property = &args[1];

    // WIDGET is (TYPE :prop1 val1 :prop2 val2 ...)
    // Skip the first element (type), then search plist-style.
    if let Some(items) = list_to_vec(widget) {
        // Start from index 1 (skip type), search plist pairs
        let mut i = 1;
        while i + 1 < items.len() {
            if equal_value(&items[i], property, 0) {
                return Ok(items[i + 1]);
            }
            i += 2;
        }
    }
    Ok(Value::NIL)
}

/// (widget-put WIDGET PROPERTY VALUE)
/// Set PROPERTY to VALUE in the widget plist. Returns VALUE.
/// Since widgets are mutable lists, we modify in-place by walking cons cells.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_widget_put(args: Vec<Value>) -> EvalResult {
    expect_args("widget-put", &args, 3)?;
    let widget = &args[0];
    let property = &args[1];
    let value = &args[2];

    // Walk the cdr of WIDGET (skip the type cons cell) looking for PROPERTY.
    if widget.is_cons() {
        let mut cursor = {
            let _cell_car = widget.cons_car();

            widget.cons_cdr()
        };
        while let ValueKind::Cons = cursor.kind() {
            let key = {
                let cell_car = cursor.cons_car();
                let _cell_cdr = cursor.cons_cdr();
                cell_car
            };
            if equal_value(&key, property, 0) {
                // Found the key cons. The *next* cons cell
                // holds the value (plist layout: KEY VAL KEY
                // VAL ...). Mutate that next cell's car, NOT
                // the current key cell — overwriting the key
                // would break the plist structure.
                let next = cursor.cons_cdr();
                if next.is_cons() {
                    next.set_car(*value);
                    return Ok(*value);
                }
                break;
            }
            // Skip value, move to next key
            let after_key = cursor.cons_cdr();
            if after_key.is_cons() {
                cursor = after_key.cons_cdr();
            } else {
                break;
            }
        }

        // Property not found — append to end of widget plist (after type).
        // Prepend (PROPERTY VALUE ...) to the cdr of the first cons cell.
        let old_cdr = (*widget).cons_cdr();
        let new_tail = Value::cons(*property, Value::cons(*value, old_cdr));
        (*widget).set_cdr(new_tail);
    }

    Ok(*value)
}

/// (widget-apply WIDGET PROPERTY &rest ARGS)
/// Apply WIDGET's PROPERTY function to WIDGET and ARGS.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_widget_apply(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("widget-apply", &args, 2)?;
    let widget = args[0];
    let property = args[1];

    let function = builtin_widget_get(vec![widget, property])?;
    if function.is_nil() {
        return Err(signal(LispCondition::VoidFunction, vec![Value::NIL]));
    }

    let mut call_args = Vec::with_capacity(args.len().saturating_sub(1));
    call_args.push(widget);
    call_args.extend_from_slice(&args[2..]);

    match function.kind() {
        ValueKind::Symbol(id) => {
            let name = resolve_sym(id);
            if let Some(result) = eval.dispatch_subr(name, call_args) {
                result
            } else {
                Err(signal(
                    LispCondition::VoidFunction,
                    vec![Value::symbol(name)],
                ))
            }
        }
        ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr) => {
            if let Some(result) = eval.dispatch_subr_value(function, call_args) {
                result
            } else {
                Err(signal(LispCondition::VoidFunction, vec![function]))
            }
        }
        _ => Err(signal(LispCondition::InvalidFunction, vec![function])),
    }
}

/// (string-make-multibyte STRING) -- convert unibyte storage bytes to multibyte chars.
pub(crate) fn builtin_string_make_multibyte(args: Vec<Value>) -> EvalResult {
    use crate::emacs_core::emacs_char;
    expect_args("string-make-multibyte", &args, 1)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    if ls.is_multibyte() {
        return Ok(args[0]);
    }
    if ls.as_bytes().is_ascii() {
        return Ok(args[0]);
    }
    // Unibyte -> multibyte: each byte 0x80..0xFF becomes a raw-byte char.
    let out = emacs_char::str_to_multibyte(ls.as_bytes());
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_emacs_bytes(out),
    ))
}

/// (string-make-unibyte STRING) -- convert each character code to a single byte.
pub(crate) fn builtin_string_make_unibyte(args: Vec<Value>) -> EvalResult {
    expect_args("string-make-unibyte", &args, 1)?;
    match args[0].kind() {
        ValueKind::String => {
            let string = args[0].as_lisp_string().expect("string");
            if !string.is_multibyte() {
                return Ok(args[0]);
            }
            let src_bytes = string.as_bytes();
            let mut out = Vec::with_capacity(string.schars());
            let mut pos = 0;
            while pos < src_bytes.len() {
                let (cp, len) = crate::emacs_core::emacs_char::string_char(&src_bytes[pos..]);
                out.push((cp & 0xFF) as u8);
                pos += len;
            }
            Ok(Value::heap_string(
                crate::heap_types::LispString::from_unibyte(out),
            ))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )),
    }
}

// ---------------------------------------------------------------------------
// String comparison
// ---------------------------------------------------------------------------

/// (compare-strings STR1 START1 END1 STR2 START2 END2 &optional IGNORE-CASE)
///
/// Compare substrings of STR1 and STR2.
/// Returns t if they are equal, or the 1-based index of the first differing
/// character (negative if STR1 is less, positive if STR1 is greater).
pub(crate) fn builtin_compare_strings(args: Vec<Value>) -> EvalResult {
    expect_args_range("compare-strings", &args, 6, 7)?;

    let s1 = require_lisp_string(&args[0])?;
    let s2 = require_lisp_string(&args[3])?;

    let chars1: Vec<u32> = compare_strings_codes(&s1);
    let chars2: Vec<u32> = compare_strings_codes(&s2);

    let end1_arg = compare_strings_clamp_too_large_end(args[2], chars1.len());
    let end2_arg = compare_strings_clamp_too_large_end(args[5], chars2.len());
    let range1 = validate_compare_strings_subarray(args[0], args[1], end1_arg, chars1.len())?;
    let range2 = validate_compare_strings_subarray(args[3], args[4], end2_arg, chars2.len())?;

    let ignore_case = args.get(6).is_some_and(|v| v.is_truthy());

    let sub1 = &chars1[range1.start().get()..range1.end().get()];
    let sub2 = &chars2[range2.start().get()..range2.end().get()];

    let len = sub1.len().min(sub2.len());
    for i in 0..len {
        let c1 = if ignore_case {
            compare_strings_upcase_code(sub1[i])
        } else {
            sub1[i]
        };
        let c2 = if ignore_case {
            compare_strings_upcase_code(sub2[i])
        } else {
            sub2[i]
        };
        if c1 != c2 {
            let pos = (i + 1) as i64; // 1-based
            if c1 < c2 {
                return Ok(Value::fixnum(-pos));
            } else {
                return Ok(Value::fixnum(pos));
            }
        }
    }

    if sub1.len() == sub2.len() {
        Ok(Value::T)
    } else if sub1.len() < sub2.len() {
        Ok(Value::fixnum(-((len + 1) as i64)))
    } else {
        Ok(Value::fixnum((len + 1) as i64))
    }
}

fn compare_strings_clamp_too_large_end(end: Value, size: usize) -> Value {
    match end.kind() {
        ValueKind::Fixnum(n) if n > size as i64 => Value::fixnum(size as i64),
        _ => end,
    }
}

fn validate_compare_strings_subarray(
    array: Value,
    from: Value,
    to: Value,
    size: usize,
) -> Result<CharRange, Flow> {
    let size_i64 = size as i64;
    let from_index = match from.kind() {
        ValueKind::Fixnum(n) => {
            if n < 0 {
                n + size_i64
            } else {
                n
            }
        }
        ValueKind::Nil => 0,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integerp"), from],
            ));
        }
    };
    let to_index = match to.kind() {
        ValueKind::Fixnum(n) => {
            if n < 0 {
                n + size_i64
            } else {
                n
            }
        }
        ValueKind::Nil => size_i64,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integerp"), to],
            ));
        }
    };

    if !(0 <= from_index && from_index <= to_index && to_index <= size_i64) {
        return Err(signal(LispCondition::ArgsOutOfRange, vec![array, from, to]));
    }

    Ok(CharRange::new(
        CharPos0::new(from_index as usize),
        CharPos0::new(to_index as usize),
    ))
}

fn compare_strings_upcase_code(code: u32) -> u32 {
    let mapped = super::builtins::upcase_char_code_emacs_compat(code as i64);
    u32::try_from(mapped).unwrap_or(code)
}

/// Decode a `compare-strings` operand to character codes. Multibyte strings
/// decode Emacs chars; a unibyte string's bytes >= 0x80 are eight-bit chars
/// (matching GNU compare-strings, which unifies a unibyte raw byte with the
/// corresponding multibyte eight-bit char).
fn compare_strings_codes(value: &crate::heap_types::LispString) -> Vec<u32> {
    let bytes = value.as_bytes();
    if value.is_multibyte() {
        let mut codes = Vec::new();
        let mut pos = 0;
        while pos < bytes.len() {
            let (code, len) = crate::emacs_core::emacs_char::string_char(&bytes[pos..]);
            codes.push(code);
            pos += len.max(1);
        }
        codes
    } else {
        bytes
            .iter()
            .map(|&b| {
                if b < 0x80 {
                    b as u32
                } else {
                    crate::emacs_core::emacs_char::byte8_to_char(b)
                }
            })
            .collect()
    }
}

fn require_lisp_string(value: &Value) -> Result<crate::heap_types::LispString, Flow> {
    value.as_lisp_string().cloned().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )
    })
}

/// (string-version-lessp S1 S2) -- version-aware string comparison.
pub(crate) fn builtin_string_version_lessp(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("string-version-lessp", &args, 2)?;
    let s1 = StringDesignator::from_value(eval, args[0])?
        .text()
        .as_bytes()
        .to_vec();
    let s2 = StringDesignator::from_value(eval, args[1])?
        .text()
        .as_bytes()
        .to_vec();

    Ok(Value::bool_val(filenvercmp(&s1, &s2) < 0))
}

fn file_prefixlen(s: &[u8]) -> usize {
    let n = s.len();
    let mut prefixlen = 0;
    let mut i = 0;
    loop {
        if i == n {
            return prefixlen;
        }

        i += 1;
        prefixlen = i;
        while i + 1 < n && s[i] == b'.' && (s[i + 1].is_ascii_alphabetic() || s[i + 1] == b'~') {
            i += 2;
            while i < n && (s[i].is_ascii_alphanumeric() || s[i] == b'~') {
                i += 1;
            }
        }
    }
}

fn version_order(s: &[u8], pos: usize, len: usize) -> i32 {
    if pos == len {
        return -1;
    }

    let c = s[pos];
    if c.is_ascii_digit() {
        0
    } else if c.is_ascii_alphabetic() {
        c as i32
    } else if c == b'~' {
        -2
    } else {
        c as i32 + u8::MAX as i32 + 1
    }
}

fn verrevcmp(s1: &[u8], s2: &[u8]) -> i32 {
    let s1_len = s1.len();
    let s2_len = s2.len();
    let mut s1_pos = 0;
    let mut s2_pos = 0;

    while s1_pos < s1_len || s2_pos < s2_len {
        let mut first_diff = 0;
        while (s1_pos < s1_len && !s1[s1_pos].is_ascii_digit())
            || (s2_pos < s2_len && !s2[s2_pos].is_ascii_digit())
        {
            let s1_c = version_order(s1, s1_pos, s1_len);
            let s2_c = version_order(s2, s2_pos, s2_len);
            if s1_c != s2_c {
                return s1_c - s2_c;
            }
            s1_pos += 1;
            s2_pos += 1;
        }

        while s1_pos < s1_len && s1[s1_pos] == b'0' {
            s1_pos += 1;
        }
        while s2_pos < s2_len && s2[s2_pos] == b'0' {
            s2_pos += 1;
        }

        while s1_pos < s1_len
            && s2_pos < s2_len
            && s1[s1_pos].is_ascii_digit()
            && s2[s2_pos].is_ascii_digit()
        {
            if first_diff == 0 {
                first_diff = s1[s1_pos] as i32 - s2[s2_pos] as i32;
            }
            s1_pos += 1;
            s2_pos += 1;
        }

        if s1_pos < s1_len && s1[s1_pos].is_ascii_digit() {
            return 1;
        }
        if s2_pos < s2_len && s2[s2_pos].is_ascii_digit() {
            return -1;
        }
        if first_diff != 0 {
            return first_diff;
        }
    }
    0
}

fn filenvercmp(a: &[u8], b: &[u8]) -> i32 {
    if a.is_empty() {
        return if b.is_empty() { 0 } else { -1 };
    }
    if b.is_empty() {
        return 1;
    }

    if a[0] == b'.' {
        if b[0] != b'.' {
            return -1;
        }

        let adot = a.len() == 1;
        let bdot = b.len() == 1;
        if adot {
            return if bdot { 0 } else { -1 };
        }
        if bdot {
            return 1;
        }

        let adotdot = a.get(1) == Some(&b'.') && a.len() == 2;
        let bdotdot = b.get(1) == Some(&b'.') && b.len() == 2;
        if adotdot {
            return if bdotdot { 0 } else { -1 };
        }
        if bdotdot {
            return 1;
        }
    } else if b[0] == b'.' {
        return 1;
    }

    let aprefixlen = file_prefixlen(a);
    let bprefixlen = file_prefixlen(b);
    let one_pass_only = aprefixlen == a.len() && bprefixlen == b.len();
    let result = verrevcmp(&a[..aprefixlen], &b[..bprefixlen]);

    if result != 0 || one_pass_only {
        result
    } else {
        verrevcmp(a, b)
    }
}

fn require_optional_locale(locale: Option<&Value>) -> Result<Option<String>, Flow> {
    match locale {
        None => Ok(None),
        Some(value) if value.is_nil() => Ok(None),
        // Locale names are ASCII (e.g. "en_US.UTF-8"); decode lossily.
        Some(value) => value
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
            .map(Some)
            .ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), *value],
                )
            }),
    }
}

#[cfg(unix)]
fn string_collate_compare(
    s1: &str,
    s2: &str,
    locale: Option<&str>,
    ignore_case: bool,
) -> Result<i32, Flow> {
    let mut left: Vec<libc::wchar_t> = s1.chars().map(|ch| ch as libc::wchar_t).collect();
    let mut right: Vec<libc::wchar_t> = s2.chars().map(|ch| ch as libc::wchar_t).collect();
    left.push(0);
    right.push(0);

    if let Some(locale) = locale {
        let locale_c = CString::new(locale).map_err(|_| {
            signal(
                "error",
                vec![Value::string(format!(
                    "Invalid locale {locale}: embedded NUL"
                ))],
            )
        })?;
        // GNU `str_collate' opens explicit locale strings with newlocale
        // and signals `error' when the requested locale does not exist.
        let loc = unsafe {
            libc::newlocale(
                libc::LC_COLLATE_MASK | libc::LC_CTYPE_MASK,
                locale_c.as_ptr(),
                std::ptr::null_mut(),
            )
        };
        if loc.is_null() {
            let errno = unsafe { collation_errno() };
            return Err(signal(
                "error",
                vec![Value::string(format!(
                    "Invalid locale {locale}: {}",
                    collation_errno_message(errno)
                ))],
            ));
        }

        if ignore_case {
            for ch in left.iter_mut().chain(right.iter_mut()) {
                if *ch != 0 {
                    *ch = unsafe { c_towlower_l(*ch as libc::c_uint, loc) as libc::wchar_t };
                }
            }
        }

        unsafe { set_collation_errno(0) };
        let result = unsafe { c_wcscoll_l(left.as_ptr(), right.as_ptr(), loc) };
        let err = unsafe { collation_errno() };
        unsafe { libc::freelocale(loc) };
        if err != 0 {
            return Err(signal(
                "error",
                vec![Value::string(format!(
                    "Invalid string for collation: {}",
                    collation_errno_message(err)
                ))],
            ));
        }
        Ok(result)
    } else {
        if ignore_case {
            for ch in left.iter_mut().chain(right.iter_mut()) {
                if *ch != 0 {
                    *ch = unsafe { c_towlower(*ch as libc::c_uint) as libc::wchar_t };
                }
            }
        }
        unsafe { set_collation_errno(0) };
        let result = unsafe { c_wcscoll(left.as_ptr(), right.as_ptr()) };
        let err = unsafe { collation_errno() };
        if err != 0 {
            return Err(signal(
                "error",
                vec![Value::string(format!(
                    "Invalid string for collation: {}",
                    collation_errno_message(err)
                ))],
            ));
        }
        Ok(result)
    }
}

#[cfg(not(unix))]
fn string_collate_compare(
    s1: &str,
    s2: &str,
    _locale: Option<&str>,
    ignore_case: bool,
) -> Result<i32, Flow> {
    let left = if ignore_case {
        s1.to_lowercase()
    } else {
        s1.to_owned()
    };
    let right = if ignore_case {
        s2.to_lowercase()
    } else {
        s2.to_owned()
    };
    Ok(match left.cmp(&right) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    })
}

/// (string-collate-lessp S1 S2 &optional LOCALE IGNORE-CASE)
pub(crate) fn builtin_string_collate_lessp(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("string-collate-lessp", &args, 2, 4)?;
    let s1 = string_designator_text(eval, args[0])?;
    let s2 = string_designator_text(eval, args[1])?;
    let locale = require_optional_locale(args.get(2))?;
    let ignore_case = args.get(3).is_some_and(|v| v.is_truthy());

    Ok(Value::bool_val(
        string_collate_compare(&s1, &s2, locale.as_deref(), ignore_case)? < 0,
    ))
}

/// (string-collate-equalp S1 S2 &optional LOCALE IGNORE-CASE)
pub(crate) fn builtin_string_collate_equalp(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("string-collate-equalp", &args, 2, 4)?;
    let s1 = string_designator_text(eval, args[0])?;
    let s2 = string_designator_text(eval, args[1])?;
    let locale = require_optional_locale(args.get(2))?;
    let ignore_case = args.get(3).is_some_and(|v| v.is_truthy());

    Ok(Value::bool_val(
        string_collate_compare(&s1, &s2, locale.as_deref(), ignore_case)? == 0,
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
