//! Additional fns.c builtins for the Elisp interpreter.
//!
//! Implements: base64 encode/decode, md5, secure-hash, buffer-hash,
//! locale-info, eql, equal-including-properties, widget-get/put/apply,
//! identity, string-to-multibyte/unibyte, string-make-multibyte/unibyte,
//! compare-strings, string-version-lessp, string-collate-lessp/equalp.
//!
//! | Knob | Values | Default | Purpose |
//! |---|---|---|---|
//! | `NEOVM_COMPARE_STRINGS_PARITY` | `on`, `off` (boolean aliases accepted) | on | Use the current buffer case table and GNU character upcase rules for case-insensitive string comparison. |
//! | `NEOVM_COMPARE_STRINGS_POS_CACHE` | `on`, `off` (boolean aliases accepted) | on | Reuse GNU's rooted position cache when a validated compare-strings START is nonzero. |

use super::eval::Context;
use crate::emacs_core::builtins::{FromValue, StringDesignator};
use crate::emacs_core::eval::AllocationFailureContextExt;
use neovm_value::emacs_char;
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
#[doc(hidden)]
pub use neovm_value::error::{expect_args, expect_args_range, expect_min_args};
use neovm_value::intern::{WellKnown, intern, resolve_sym};
use neovm_value::system_vocab::platform_facts::{STRING_COLLATION, StringCollation};
// bytes_to_unibyte_storage_string and encode_nonunicode_char_for_storage
// imports removed — using emacs_char + LispString directly
use md5::Md5;
use neovm_edit::buffer::{
    BufferManager, CharPos0, CharRange, EmacsByteRange, LispCharPos1, TextEditRange,
};
use neovm_value::strings_core::{
    B64_STD, B64_URL, base64_decode, base64_decode_bytes, base64_encode, build_decode_table,
};
use neovm_value::value::*;
use sha1::Sha1;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512};
use sha3::{Sha3_224, Sha3_256, Sha3_384, Sha3_512};
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

/// GNU's `emacs_strerror`, from `emacs_core::errno`.
///
/// This one was not merely a duplicate: it rendered the errno with Rust's
/// `io::Error` Display, so every collation error it reported carried a
/// " (os error N)" suffix GNU never emits. The shared helper cannot do that.
fn collation_errno_message(errno: libc::c_int) -> String {
    neovm_value::system_vocab::errno::emacs_strerror(errno)
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn string_designator_text(eval: &mut Context, val: Value) -> Result<String, Flow> {
    let designator = StringDesignator::from_value(eval, val)?;
    Ok(neovm_value::emacs_char::to_utf8_lossy(
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
        return neovm_edit::emacs_core::marker::marker_position_as_int(val);
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
    neovm_text::emacs_core::coding::CodingSystemManager::new().is_known(name)
}

fn validate_md5_coding_system_arg(
    args: &[Value],
    mode: neovm_value::tagged::value::SymbolMode,
) -> Result<(), Flow> {
    let Some(coding_system) = args.get(3) else {
        return Ok(());
    };
    if coding_system.is_nil() {
        return Ok(());
    }

    let noerror = args.get(4).is_some_and(|v| v.is_truthy());
    let valid = coding_system
        .as_symbol_id(mode)
        .is_some_and(|id| md5_known_coding_system(resolve_sym(id)));

    if valid || noerror {
        Ok(())
    } else {
        Err(signal(
            LispCondition::CodingSystemError,
            vec![*coding_system],
        ))
    }
}

fn md5_coding_system_name(
    args: &[Value],
    mode: neovm_value::tagged::value::SymbolMode,
) -> Option<String> {
    let coding_system = args.get(3)?;
    if coding_system.is_nil() {
        return None;
    }
    let noerror = args.get(4).is_some_and(|v| v.is_truthy());
    match coding_system.as_symbol_id(mode) {
        Some(id) if md5_known_coding_system(resolve_sym(id)) => Some(resolve_sym(id).to_owned()),
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
        let (ch, len) = neovm_value::emacs_char::string_char(&source[pos..]);
        if ch.code() <= 0x7f {
            bytes.push(ch.code() as u8);
        } else if neovm_value::emacs_char::char_byte8_p(ch.code()) {
            bytes.push(neovm_value::emacs_char::char_to_byte8(ch.code()));
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
    string: &neovm_value::heap_types::LispString,
) -> Result<Cow<'_, [u8]>, Flow> {
    base64_encode_source_bytes(string.as_bytes(), string.is_multibyte())
}

/// Build the LispString that base64-decode-region inserts into the buffer.
///
/// Mirrors GNU `base64_decode_1`'s `multibyte_bit` handling (fns.c): for a
/// multibyte target each decoded raw byte 0x80-0xFF must be stored as its
/// two-byte eight-bit internal encoding (BYTE8_STRING / `str_to_multibyte`),
/// not as a raw byte (which is not a valid multibyte lead byte and would
/// corrupt the buffer's internal representation). For a unibyte target the
/// raw bytes are stored verbatim.
fn decoded_bytes_to_lisp_string(
    bytes: Vec<u8>,
    multibyte: bool,
) -> neovm_value::heap_types::LispString {
    if multibyte {
        neovm_value::heap_types::LispString::from_emacs_bytes(
            neovm_value::emacs_char::str_to_multibyte(&bytes),
        )
    } else {
        neovm_value::heap_types::LispString::from_unibyte(bytes)
    }
}

// ---------------------------------------------------------------------------
// Base64 builtins
// ---------------------------------------------------------------------------

/// (base64-encode-string STRING &optional NO-LINE-BREAK)
#[doc(hidden)]
pub fn builtin_base64_encode_string(args: Vec<Value>) -> EvalResult {
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
#[doc(hidden)]
pub fn builtin_base64_decode_string(args: Vec<Value>) -> EvalResult {
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
            neovm_value::heap_types::LispString::from_unibyte(bytes),
        )),
        Err(()) => Err(signal("error", vec![Value::string("Invalid base64 data")])),
    }
}

/// (base64url-encode-string STRING &optional NO-PAD)
#[doc(hidden)]
pub fn builtin_base64url_encode_string(args: Vec<Value>) -> EvalResult {
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
            neovm_value::heap_types::LispString::from_unibyte(bytes),
        )),
        Err(()) => Ok(Value::NIL),
    }
}

// The current-buffer region trio moved to the edit layer with the stage-3
// split (C12): editfns owns the substrate helpers; these re-exports keep the
// historical `fns`/`super::fns` paths of this module's subr wrappers and the
// encoding/text region consumers.
pub(crate) use neovm_edit::emacs_core::editfns::{
    normalize_current_buffer_region_bounds_in_manager, read_buffer_region_bytes_in_manager,
    replace_buffer_emacs_byte_range_lisp_string,
};

#[derive(Clone, Copy, Debug)]
struct ValidatedBufferLispRegion {
    start: LispCharPos1,
    end: LispCharPos1,
}

impl ValidatedBufferLispRegion {
    #[doc(hidden)]
    pub fn from_optional_values(
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

    #[doc(hidden)]
    pub fn validate_bounds(
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

    #[doc(hidden)]
    pub fn ordered(self) -> (LispCharPos1, LispCharPos1) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    #[doc(hidden)]
    pub fn to_accessible_byte_range(self, buf: &neovm_edit::buffer::Buffer) -> EmacsByteRange {
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
    buf: &neovm_edit::buffer::Buffer,
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

pub(crate) fn replace_buffer_region_lisp_string_in_manager(
    buffers: &mut BufferManager,
    buffer_id: neovm_edit::buffer::BufferId,
    range: TextEditRange,
    replacement: &neovm_value::heap_types::LispString,
) -> Result<(), Flow> {
    buffers
        .replace_buffer_measured_region_lisp_string(buffer_id, range, replacement)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    Ok(())
}

/// (base64-encode-region START END &optional NO-LINE-BREAK)
#[doc(hidden)]
pub fn builtin_base64_encode_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("base64-encode-region", &args, 2, 3)?;
    let (buffer_id, byte_range) =
        normalize_current_buffer_region_bounds_in_manager(&eval.edit.buffers, &args[0], &args[1])?;
    let raw = read_buffer_region_bytes_in_manager(&eval.edit.buffers, buffer_id, byte_range)?;
    let target_multibyte = eval
        .edit
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
#[doc(hidden)]
pub fn builtin_base64url_encode_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("base64url-encode-region", &args, 2, 3)?;
    let (buffer_id, byte_range) =
        normalize_current_buffer_region_bounds_in_manager(&eval.edit.buffers, &args[0], &args[1])?;
    let raw = read_buffer_region_bytes_in_manager(&eval.edit.buffers, buffer_id, byte_range)?;
    let target_multibyte = eval
        .edit
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
#[doc(hidden)]
pub fn builtin_base64_decode_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("base64-decode-region", &args, 2, 4)?;
    let (buffer_id, byte_range) =
        normalize_current_buffer_region_bounds_in_manager(&eval.edit.buffers, &args[0], &args[1])?;
    let source = read_buffer_region_bytes_in_manager(&eval.edit.buffers, buffer_id, byte_range)?;
    let use_url = args.get(2).is_some_and(|v| v.is_truthy());
    let noerror = args.get(3).is_some_and(|v| v.is_truthy());
    let table = if use_url {
        build_decode_table(B64_URL)
    } else {
        build_decode_table(B64_STD)
    };
    let target_multibyte = eval
        .edit
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
#[doc(hidden)]
pub fn builtin_md5(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("md5", &args, 1, 5)?;
    let object = &args[0];
    // GNU only reads CODING-SYSTEM for strings and buffers, so `iv-auto'
    // ignores it.
    if is_iv_auto(eval, object) {
        return Ok(Value::string(md5_hash(&iv_auto_random_bytes(
            eval,
            args.get(1),
        )?)));
    }
    validate_md5_coding_system_arg(&args, eval.symbol_mode())?;
    match object.kind() {
        ValueKind::String => Ok(Value::string(md5_hex_for_string(
            object,
            args.get(1),
            args.get(2),
            md5_coding_system_name(&args, eval.symbol_mode()).as_deref(),
            eval.eol_conversion(),
        )?)),
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let input = encoded_hash_slice_for_buffer_in_context(
                eval,
                object.as_buffer_id().unwrap(),
                args.get(1),
                args.get(2),
                md5_coding_system_name(&args, eval.symbol_mode()).map(|name| intern(&name)),
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

/// Whether OBJECT is the canonical `iv-auto' symbol (GNU `EQ (object, Qiv_auto)').
fn is_iv_auto(eval: &super::eval::Context, object: &Value) -> bool {
    object.as_symbol_id(eval.symbol_mode()) == Some(WellKnown::IvAuto.id())
}

/// GNU `extract_data_from_object` (src/fns.c), shared by `md5' and
/// `secure-hash': OBJECT `iv-auto' stands for START bytes from the system's
/// random source, and START must then be a fixnum >= 0 (`FIXNATP').
fn iv_auto_random_bytes(
    eval: &super::eval::Context,
    start: Option<&Value>,
) -> Result<Vec<u8>, Flow> {
    let length = match start.map(|value| value.kind()) {
        Some(ValueKind::Fixnum(length)) if length >= 0 => length as usize,
        _ => {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Without a length, `iv-auto' can't be used; see Elisp manual",
                )],
            ));
        }
    };
    // Like GNU's `make_uninit_string', a buffer that cannot be allocated
    // signals through `memory-signal-data'.
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).map_err(|failure| {
        neovm_value::alloc::AllocationFailure::from(failure).into_flow_in_context(eval)
    })?;
    bytes.resize(length, 0);
    getrandom::fill(&mut bytes).map_err(|err| {
        signal(
            "file-error",
            vec![
                Value::string("Getting random data"),
                Value::string(err.to_string()),
            ],
        )
    })?;
    Ok(bytes)
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
    eol_conversion: neovm_text::emacs_core::coding::EolConversion,
) -> Result<String, Flow> {
    let string = object
        .as_lisp_string()
        .expect("md5_hex_for_string only accepts string object");
    let encoded;
    let (bytes, len, multibyte) = if string.is_multibyte() {
        encoded = neovm_text::encoding::encode_lisp_string(
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
            neovm_value::emacs_char::char_to_byte_pos(bytes, start),
            neovm_value::emacs_char::char_to_byte_pos(bytes, end),
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
    buffer_id: neovm_edit::buffer::BufferId,
    start_raw: Option<&Value>,
    end_raw: Option<&Value>,
    coding_override: Option<neovm_value::intern::SymId>,
) -> Result<Vec<u8>, Flow> {
    let (text, multibyte) = {
        let buf = eval
            .edit
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
        crate::emacs_core::eval::data::file_coding::WriteCodingFallback::Utf8
    } else {
        crate::emacs_core::eval::data::file_coding::WriteCodingFallback::RawText
    };
    let coding_system = coding_override
        .map(neovm_text::encoding::RuntimeCodingSystem::from_symbol)
        .unwrap_or_else(|| {
            crate::emacs_core::eval::data::file_coding::resolve_write_coding_system(
                eval, buffer_id, fallback,
            )
        });
    Ok(
        neovm_text::encoding::encode_external_text_in_context(eval, text, coding_system)?
            .into_bytes(),
    )
}

fn secure_hash_algorithm_name(
    val: &Value,
    mode: neovm_value::tagged::value::SymbolMode,
) -> Result<String, Flow> {
    val.as_symbol_name(mode).map(str::to_owned).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *val],
        )
    })
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
    Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(
        bytes.to_vec(),
    ))
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
            neovm_value::emacs_char::char_to_byte_pos(bytes, start),
            neovm_value::emacs_char::char_to_byte_pos(bytes, end),
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
        "sha3-224" => Sha3_224::digest(input).to_vec(),
        "sha3-256" => Sha3_256::digest(input).to_vec(),
        "sha3-384" => Sha3_384::digest(input).to_vec(),
        "sha3-512" => Sha3_512::digest(input).to_vec(),
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
#[doc(hidden)]
pub fn builtin_secure_hash(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("secure-hash", &args, 2, 5)?;
    let algo_name = secure_hash_algorithm_name(&args[0], eval.symbol_mode())?;

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
        _ if is_iv_auto(eval, object) => iv_auto_random_bytes(eval, args.get(2))?,
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
#[doc(hidden)]
pub fn builtin_buffer_hash(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("buffer-hash", &args, 0, 1)?;

    let buffer_id = if args.is_empty() || args[0].is_nil() {
        eval.edit
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
            .id()
    } else {
        match args[0].kind() {
            ValueKind::Veclike(VecLikeType::Buffer) => args[0].as_buffer_id().unwrap(),
            ValueKind::String => {
                let name = neovm_value::emacs_char::to_utf8_lossy(
                    require_lisp_string(&args[0])?.as_bytes(),
                );
                eval.edit
                    .buffers
                    .find_buffer_by_name(&name)
                    .ok_or_else(|| {
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
        .edit
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

#[doc(hidden)]
pub fn builtin_equal_including_properties_2(
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
#[doc(hidden)]
pub fn builtin_string_make_multibyte(args: Vec<Value>) -> EvalResult {
    use neovm_value::emacs_char;
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
        neovm_value::heap_types::LispString::from_emacs_bytes(out),
    ))
}

/// (string-make-unibyte STRING) -- convert each character code to a single byte.
#[doc(hidden)]
pub fn builtin_string_make_unibyte(args: Vec<Value>) -> EvalResult {
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
                let (cp, len) = neovm_value::emacs_char::string_char(&src_bytes[pos..]);
                out.push((cp.code() & 0xFF) as u8);
                pos += len;
            }
            Ok(Value::heap_string(
                neovm_value::heap_types::LispString::from_unibyte(out),
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

/// The process-wide switch holds no Lisp state. OnceLock publishes one
/// immutable boolean for all mutators; the existing position cache retains
/// its per-mutator rooting and mutation-invalidation ownership.
#[inline]
fn compare_strings_pos_cache_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(read_compare_strings_pos_cache_knob)
}

#[cold]
#[inline(never)]
fn read_compare_strings_pos_cache_knob() -> bool {
    std::env::var("NEOVM_COMPARE_STRINGS_POS_CACHE")
        .ok()
        .map_or(true, |value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "on" | "1" | "true" | "yes"
            )
        })
}

/// (compare-strings STR1 START1 END1 STR2 START2 END2 &optional IGNORE-CASE)
///
/// Compare substrings of STR1 and STR2.
/// Returns t if they are equal, or the 1-based index of the first differing
/// character (negative if STR1 is less, positive if STR1 is greater).
pub(crate) fn builtin_compare_strings(args: Vec<Value>) -> EvalResult {
    expect_args_range("compare-strings", &args, 6, 7)?;

    let s1 = borrow_lisp_string(&args[0])?;
    let s2 = borrow_lisp_string(&args[3])?;

    // GNU `Fcompare_strings' validates against SCHARS and walks only the two
    // ranges; decoding both whole strings made `string-prefix-p' on a long
    // string O(length) instead of O(prefix).
    let size1 = s1.schars();
    let size2 = s2.schars();
    let end1_arg = compare_strings_clamp_too_large_end(args[2], size1);
    let end2_arg = compare_strings_clamp_too_large_end(args[5], size2);
    let range1 = validate_compare_strings_subarray(args[0], args[1], end1_arg, size1)?;
    let range2 = validate_compare_strings_subarray(args[3], args[4], end2_arg, size2)?;

    let ignore_case = args.get(6).is_some_and(|v| v.is_truthy());

    let len1 = range1.end().get() - range1.start().get();
    let len2 = range2.end().get() - range2.start().get();
    // With both STARTs at zero there is no offset scan to avoid. Keep that
    // common prefix-comparison path stateless, without even reading the knob,
    // and preserve a useful cached offset belonging to another string.
    let has_nonzero_start = range1.start().get() != 0 || range2.start().get() != 0;
    let (mut chars1, mut chars2) = if has_nonzero_start && compare_strings_pos_cache_enabled() {
        // GNU resolves operand 1 then operand 2 through its one-entry cache.
        // ASCII operands leave the cached multibyte string undisturbed; two
        // distinct multibyte operands replace it in this same order.
        let chars1 = CompareStringsChars::new_cached(args[0], s1, range1.start().get());
        let chars2 = CompareStringsChars::new_cached(args[3], s2, range2.start().get());
        (chars1, chars2)
    } else {
        let chars1 = CompareStringsChars::new(s1, range1.start().get());
        let chars2 = CompareStringsChars::new(s2, range2.start().get());
        (chars1, chars2)
    };

    let len = len1.min(len2);
    for i in 0..len {
        let (c1, c2) = (chars1.next_code(), chars2.next_code());
        let (c1, c2) = if ignore_case {
            (
                compare_strings_upcase_code(c1),
                compare_strings_upcase_code(c2),
            )
        } else {
            (c1, c2)
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

    if len1 == len2 {
        Ok(Value::T)
    } else if len1 < len2 {
        Ok(Value::fixnum(-((len + 1) as i64)))
    } else {
        Ok(Value::fixnum((len + 1) as i64))
    }
}

/// Immutable process policy only, published through OnceLock; every mutator
/// shares the scalar policy, while all case-table values remain in its Context.
#[inline]
#[doc(hidden)]
pub fn compare_strings_parity_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(read_compare_strings_parity_knob)
}

#[cold]
#[inline(never)]
fn read_compare_strings_parity_knob() -> bool {
    !std::env::var("NEOVM_COMPARE_STRINGS_PARITY")
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "off" | "0" | "false" | "no"
            )
        })
}

/// Buffer-aware entry point. A case-sensitive call never reads the parity knob
/// and continues through the unchanged comparison and START-cache path.
#[inline]
#[doc(hidden)]
pub fn builtin_compare_strings_in_state(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    if args.get(6).is_some_and(|value| value.is_truthy()) && compare_strings_parity_enabled() {
        compare_strings_with_current_case_table(ctx, args)
    } else {
        builtin_compare_strings(args)
    }
}

#[cfg(test)]
fn compare_strings_in_state_with_parity(
    ctx: &mut Context,
    args: Vec<Value>,
    parity: bool,
) -> EvalResult {
    if parity && args.get(6).is_some_and(|value| value.is_truthy()) {
        compare_strings_with_current_case_table(ctx, args)
    } else {
        builtin_compare_strings(args)
    }
}

/// A call-local projection of the upcase char-table's ASCII cache. This holds
/// only values rooted by the current buffer; no Lisp callback, collection or
/// mutation can intervene while it is read. Mutators own their Context and the
/// projection introduces no shared mutable state or persistent Lisp cache.
enum CompareStringsAsciiCase {
    Uniform(Value),
    Values(Value),
    Lookup,
}

/// Call-local case state belongs to the current mutator's Context. Its table
/// roots are held by the buffer; comparison runs without Lisp calls or writes.
struct CompareStringsCase {
    up: Value,
    ascii: CompareStringsAsciiCase,
    multibyte_buffer: bool,
    symbol_mode: neovm_value::tagged::SymbolMode,
}

impl CompareStringsCase {
    #[doc(hidden)]
    pub fn for_current_buffer(ctx: &mut Context) -> Result<Self, Flow> {
        let multibyte_buffer = ctx
            .edit
            .buffers
            .current_buffer()
            .is_none_or(|buf| buf.get_multibyte());
        let up = crate::emacs_core::eval::case_tables::current_case_upcase_table(ctx)?;
        let ascii = match up.as_char_table_obj() {
            Some(obj) if obj.defalt.get().is_nil() && obj.parent.get().is_nil() => {
                if obj.ascii.get().as_sub_char_table_obj().is_some() {
                    CompareStringsAsciiCase::Values(obj.ascii.get())
                } else {
                    CompareStringsAsciiCase::Uniform(obj.ascii.get())
                }
            }
            _ => CompareStringsAsciiCase::Lookup,
        };
        Ok(Self {
            up,
            ascii,
            multibyte_buffer,
            symbol_mode: ctx.symbol_mode(),
        })
    }

    /// GNU Fupcase's integer path (casefiddle.c do_casify_natnum). The string
    /// iterator has already converted unibyte operands to multibyte codes; the
    /// current buffer then controls how Fupcase interprets codes below 256.
    #[inline]
    #[doc(hidden)]
    pub fn upcase_code(&self, original: u32) -> i32 {
        let multibyte = original >= 256 || self.multibyte_buffer;
        let code = if multibyte {
            original
        } else {
            emacs_char::make_char_multibyte(original as i32)
        };
        let mapped = if code < 128 {
            match self.ascii {
                CompareStringsAsciiCase::Uniform(value) => value,
                CompareStringsAsciiCase::Values(value) => value
                    .as_sub_char_table_obj()
                    .and_then(|obj| obj.contents.as_slice().get(code as usize).copied())
                    .unwrap_or(Value::NIL),
                CompareStringsAsciiCase::Lookup => {
                    neovm_text::emacs_core::chartable::ct_ref_with_symbols(
                        &self.up,
                        code as i64,
                        self.symbol_mode,
                    )
                }
            }
        } else {
            neovm_text::emacs_core::chartable::ct_ref_with_symbols(
                &self.up,
                code as i64,
                self.symbol_mode,
            )
        };
        let cased = match mapped.kind() {
            ValueKind::Fixnum(n) if n >= 0 => n as i32,
            _ => code as i32,
        };
        if cased == code as i32 {
            original as i32
        } else if multibyte {
            cased
        } else if cased > emacs_char::MAX_5_BYTE_CHAR as i32 {
            cased - emacs_char::byte8_to_char(0) as i32
        } else {
            cased & 0xff
        }
    }
}

fn compare_strings_with_current_case_table(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("compare-strings", &args, 6, 7)?;
    let s1 = borrow_lisp_string(&args[0])?;
    let s2 = borrow_lisp_string(&args[3])?;
    let size1 = s1.schars();
    let size2 = s2.schars();
    let end1 = compare_strings_clamp_too_large_end(args[2], size1);
    let end2 = compare_strings_clamp_too_large_end(args[5], size2);
    let range1 = validate_compare_strings_subarray(args[0], args[1], end1, size1)?;
    let range2 = validate_compare_strings_subarray(args[3], args[4], end2, size2)?;
    let len1 = range1.end().get() - range1.start().get();
    let len2 = range2.end().get() - range2.start().get();
    let has_nonzero_start = range1.start().get() != 0 || range2.start().get() != 0;
    let (chars1, chars2) = if has_nonzero_start && compare_strings_pos_cache_enabled() {
        (
            CompareStringsChars::new_cached(args[0], s1, range1.start().get()),
            CompareStringsChars::new_cached(args[3], s2, range2.start().get()),
        )
    } else {
        (
            CompareStringsChars::new(s1, range1.start().get()),
            CompareStringsChars::new(s2, range2.start().get()),
        )
    };
    compare_strings_iterators_with_current_case_table(ctx, chars1, chars2, len1, len2)
}

/// GNU's Fcompare_strings over two prefixes with IGNORE-CASE. Completion
/// scanners use this instead of their hardwired fold only when parity is on.
/// Each call resolves the buffer table anew, since predicates between calls
/// may change the current buffer or modify its case table.
#[inline]
#[doc(hidden)]
pub fn compare_string_prefixes_in_state(
    ctx: &mut Context,
    left: &neovm_value::heap_types::LispString,
    right: &neovm_value::heap_types::LispString,
    len: usize,
) -> EvalResult {
    compare_strings_iterators_with_current_case_table(
        ctx,
        CompareStringsChars::new(left, 0),
        CompareStringsChars::new(right, 0),
        len.min(left.schars()),
        len.min(right.schars()),
    )
}

/// The folded suffix comparison used by GNU file completion's ignored
/// extensions. Owned decoded names carry no Lisp identity for a START cache.
#[doc(hidden)]
pub fn compare_string_suffixes_in_state(
    ctx: &mut Context,
    left: &neovm_value::heap_types::LispString,
    right: &neovm_value::heap_types::LispString,
) -> EvalResult {
    let len = right.schars();
    if left.schars() < len {
        return Ok(Value::NIL);
    }
    compare_strings_iterators_with_current_case_table(
        ctx,
        CompareStringsChars::new(left, left.schars() - len),
        CompareStringsChars::new(right, 0),
        len,
        len,
    )
}

#[inline]
fn compare_strings_iterators_with_current_case_table(
    ctx: &mut Context,
    mut chars1: CompareStringsChars<'_>,
    mut chars2: CompareStringsChars<'_>,
    len1: usize,
    len2: usize,
) -> EvalResult {
    let len = len1.min(len2);
    let mut case = None;
    for i in 0..len {
        let (c1, c2) = (chars1.next_code(), chars2.next_code());
        // GNU bypasses Fupcase when the fetched character codes already match.
        if c1 == c2 {
            continue;
        }
        if case.is_none() {
            case = Some(CompareStringsCase::for_current_buffer(ctx)?);
        }
        let case = case.as_ref().expect("case table was initialized");
        let (c1, c2) = (case.upcase_code(c1), case.upcase_code(c2));
        if c1 != c2 {
            let pos = (i + 1) as i64;
            return Ok(Value::fixnum(if c1 < c2 { -pos } else { pos }));
        }
    }
    Ok(if len1 == len2 {
        Value::T
    } else if len1 < len2 {
        Value::fixnum(-((len + 1) as i64))
    } else {
        Value::fixnum((len + 1) as i64)
    })
}

fn borrow_lisp_string(value: &Value) -> Result<&neovm_value::heap_types::LispString, Flow> {
    value.as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )
    })
}

/// The character codes of a `compare-strings' operand from a character
/// index on.  Multibyte strings decode Emacs chars; a unibyte string's bytes
/// >= 0x80 are eight-bit chars (matching GNU compare-strings, which unifies a
/// unibyte raw byte with the corresponding multibyte eight-bit char).
struct CompareStringsChars<'a> {
    bytes: &'a [u8],
    pos: usize,
    multibyte: bool,
}

impl<'a> CompareStringsChars<'a> {
    #[doc(hidden)]
    pub fn new(string: &'a neovm_value::heap_types::LispString, char_start: usize) -> Self {
        Self {
            bytes: string.as_bytes(),
            pos: string.char_to_byte_pos(char_start),
            multibyte: string.is_multibyte(),
        }
    }

    /// Reuses the existing rooted, per-mutator position cache. The iterator
    /// itself only borrows its operand for this call and introduces no new
    /// cache, GC root or cross-mutator mutable state.
    #[inline]
    #[doc(hidden)]
    pub fn new_cached(
        value: Value,
        string: &'a neovm_value::heap_types::LispString,
        char_start: usize,
    ) -> Self {
        Self {
            bytes: string.as_bytes(),
            pos: neovm_value::string_pos_cache::string_char_to_byte(value, string, char_start),
            multibyte: string.is_multibyte(),
        }
    }

    /// The next code; the caller never reads past the validated range.
    #[doc(hidden)]
    pub fn next_code(&mut self) -> u32 {
        if self.multibyte {
            let (code, len) = neovm_value::emacs_char::string_char(&self.bytes[self.pos..]);
            self.pos += len.max(1);
            code.code()
        } else {
            let b = self.bytes[self.pos];
            self.pos += 1;
            if b < 0x80 {
                b as u32
            } else {
                neovm_value::emacs_char::byte8_to_char(b)
            }
        }
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

fn require_lisp_string(value: &Value) -> Result<neovm_value::heap_types::LispString, Flow> {
    value.as_lisp_string().cloned().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )
    })
}

/// (string-version-lessp S1 S2) -- version-aware string comparison.
#[doc(hidden)]
pub fn builtin_string_version_lessp(eval: &mut Context, args: Vec<Value>) -> EvalResult {
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
            .map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes()))
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
#[doc(hidden)]
pub fn builtin_string_collate_lessp(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("string-collate-lessp", &args, 2, 4)?;
    if STRING_COLLATION == StringCollation::ByteComparison {
        // GNU's macOS build compiles `return Fstring_lessp (s1, s2)'
        // (src/fns.c:643-645): symbols are accepted, and LOCALE and
        // IGNORE-CASE are neither read nor type-checked.
        return crate::emacs_core::builtins::strings::builtin_string_lessp_2(
            eval, args[0], args[1],
        );
    }
    let s1 = string_designator_text(eval, args[0])?;
    let s2 = string_designator_text(eval, args[1])?;
    let locale = require_optional_locale(args.get(2))?;
    let ignore_case = args.get(3).is_some_and(|v| v.is_truthy());

    Ok(Value::bool_val(
        string_collate_compare(&s1, &s2, locale.as_deref(), ignore_case)? < 0,
    ))
}

/// (string-collate-equalp S1 S2 &optional LOCALE IGNORE-CASE)
#[doc(hidden)]
pub fn builtin_string_collate_equalp(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("string-collate-equalp", &args, 2, 4)?;
    if STRING_COLLATION == StringCollation::ByteComparison {
        // GNU's macOS build compiles `return Fstring_equal (s1, s2)'
        // (src/fns.c:692-694).
        return crate::emacs_core::builtins::strings::builtin_string_equal_2(
            eval, args[0], args[1],
        );
    }
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
#[path = "tests/fns_test.rs"]
mod tests;
