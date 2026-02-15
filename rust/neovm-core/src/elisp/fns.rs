//! Additional fns.c builtins for the Elisp interpreter.
//!
//! Implements: base64 encode/decode, md5, secure-hash, buffer-hash,
//! locale-info, eql, equal-including-properties, widget-get/put/apply,
//! identity, string-to-multibyte/unibyte, string-make-multibyte/unibyte,
//! compare-strings, string-version-lessp, string-collate-lessp/equalp.

use super::error::{signal, EvalResult, Flow};
use super::string_escape::{
    bytes_to_unibyte_storage_string, decode_storage_char_codes, encode_nonunicode_char_for_storage,
    storage_char_len, storage_substring,
};
use super::value::*;
use sha1::Sha1;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512};

const UNIBYTE_BYTE_SENTINEL_BASE: u32 = 0xE300;
const UNIBYTE_BYTE_SENTINEL_MIN: u32 = 0xE300;
const UNIBYTE_BYTE_SENTINEL_MAX: u32 = 0xE3FF;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_range_args(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), Flow> {
    if args.len() < min || args.len() > max {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn require_string(_name: &str, val: &Value) -> Result<String, Flow> {
    match val {
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

fn require_int(val: &Value) -> Result<i64, Flow> {
    match val {
        Value::Int(n) => Ok(*n),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), other.clone()],
        )),
    }
}

fn require_int_or_marker(val: &Value) -> Result<i64, Flow> {
    match val {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        v if super::marker::is_marker(v) => super::marker::marker_position_as_int(v),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integer-or-marker-p"), other.clone()],
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
    let valid = match coding_system {
        Value::Symbol(name) => md5_known_coding_system(name),
        _ => false,
    };

    if valid || noerror {
        Ok(())
    } else {
        Err(signal("coding-system-error", vec![coding_system.clone()]))
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

/// Build a decode table (256 entries, 0xFF = invalid) from an alphabet.
fn build_decode_table(alphabet: &[u8; 64]) -> [u8; 256] {
    let mut table = [0xFFu8; 256];
    for (i, &ch) in alphabet.iter().enumerate() {
        table[ch as usize] = i as u8;
    }
    table
}

// ---------------------------------------------------------------------------
// Base64 encode (manual implementation)
// ---------------------------------------------------------------------------

fn base64_encode(input: &[u8], alphabet: &[u8; 64], pad: bool, line_break: bool) -> String {
    let mut out = Vec::with_capacity((input.len() + 2) / 3 * 4 + input.len() / 57);
    let mut col = 0usize;

    let chunks = input.chunks(3);
    for chunk in chunks {
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

        col += 4;
        if line_break && col >= 76 {
            out.push(b'\n');
            col = 0;
        }
    }

    // Safety: we only pushed ASCII bytes
    unsafe { String::from_utf8_unchecked(out) }
}

// ---------------------------------------------------------------------------
// Base64 decode (manual implementation)
// ---------------------------------------------------------------------------

fn base64_decode(input: &str, table: &[u8; 256]) -> Result<Vec<u8>, ()> {
    // Strip whitespace (CR, LF, space, tab) per Emacs behaviour
    let bytes: Vec<u8> = input
        .bytes()
        .filter(|&b| b != b'\n' && b != b'\r' && b != b' ' && b != b'\t')
        .collect();

    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for &b in &bytes {
        if b == b'=' {
            // Padding — stop collecting
            break;
        }
        let val = table[b as usize];
        if val == 0xFF {
            return Err(());
        }
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Base64 builtins
// ---------------------------------------------------------------------------

/// (base64-encode-string STRING &optional NO-LINE-BREAK)
pub(crate) fn builtin_base64_encode_string(args: Vec<Value>) -> EvalResult {
    expect_range_args("base64-encode-string", &args, 1, 2)?;
    let s = require_string("base64-encode-string", &args[0])?;
    let no_line_break = args.get(1).map_or(false, |v| v.is_truthy());
    let encoded = base64_encode(s.as_bytes(), B64_STD, true, !no_line_break);
    Ok(Value::string(encoded))
}

/// (base64-decode-string STRING &optional BASE64URL)
pub(crate) fn builtin_base64_decode_string(args: Vec<Value>) -> EvalResult {
    expect_range_args("base64-decode-string", &args, 1, 2)?;
    let s = require_string("base64-decode-string", &args[0])?;
    let use_url = args.get(1).map_or(false, |v| v.is_truthy());
    let table = if use_url {
        build_decode_table(B64_URL)
    } else {
        build_decode_table(B64_STD)
    };
    match base64_decode(&s, &table) {
        Ok(bytes) => {
            let decoded = String::from_utf8_lossy(&bytes).into_owned();
            Ok(Value::string(decoded))
        }
        Err(()) => Ok(Value::Nil),
    }
}

/// (base64url-encode-string STRING &optional NO-PAD)
pub(crate) fn builtin_base64url_encode_string(args: Vec<Value>) -> EvalResult {
    expect_range_args("base64url-encode-string", &args, 1, 2)?;
    let s = require_string("base64url-encode-string", &args[0])?;
    let no_pad = args.get(1).map_or(false, |v| v.is_truthy());
    let encoded = base64_encode(s.as_bytes(), B64_URL, !no_pad, false);
    Ok(Value::string(encoded))
}

// ---------------------------------------------------------------------------
// Hash / digest builtins
// ---------------------------------------------------------------------------

/// (md5 OBJECT &optional START END CODING-SYSTEM NOERROR)
///
/// Pure fallback used when evaluator access is unavailable.
/// Supports string objects and Emacs-compatible range/error semantics.
pub(crate) fn builtin_md5(args: Vec<Value>) -> EvalResult {
    expect_range_args("md5", &args, 1, 5)?;
    validate_md5_coding_system_arg(&args)?;
    let object = &args[0];
    match object {
        Value::Str(_) => Ok(Value::string(md5_hex_for_string(
            object,
            args.get(1),
            args.get(2),
        )?)),
        other => Err(signal(
            "error",
            vec![
                Value::string("Invalid object argument"),
                invalid_object_payload(other),
            ],
        )),
    }
}

/// (md5 OBJECT &optional START END CODING-SYSTEM NOERROR)
///
/// Evaluator-aware implementation that also supports buffer objects.
pub(crate) fn builtin_md5_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_range_args("md5", &args, 1, 5)?;
    validate_md5_coding_system_arg(&args)?;
    let object = &args[0];
    match object {
        Value::Str(_) => Ok(Value::string(md5_hex_for_string(
            object,
            args.get(1),
            args.get(2),
        )?)),
        Value::Buffer(id) => Ok(Value::string(md5_hex_for_buffer(
            eval,
            *id,
            args.get(1),
            args.get(2),
        )?)),
        other => {
            return Err(signal(
                "error",
                vec![
                    Value::string("Invalid object argument"),
                    invalid_object_payload(other),
                ],
            ));
        }
    }
}

/// Minimal MD5 implementation (RFC 1321).
fn md5_digest(message: &[u8]) -> [u8; 16] {
    // Per-round shift amounts
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];

    // Pre-computed T[i] = floor(2^32 * abs(sin(i+1)))
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];

    // Pre-processing: add padding
    let orig_len_bits = (message.len() as u64).wrapping_mul(8);
    let mut msg = message.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0x00);
    }
    // Append original length in bits as 64-bit little-endian
    msg.extend_from_slice(&orig_len_bits.to_le_bytes());

    // Initialize hash values
    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;

    // Process each 512-bit (64-byte) block
    for chunk in msg.chunks_exact(64) {
        let mut m = [0u32; 16];
        for i in 0..16 {
            m[i] = u32::from_le_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }

        let mut a = a0;
        let mut b = b0;
        let mut c = c0;
        let mut d = d0;

        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | ((!b) & d), i),
                16..=31 => ((d & b) | ((!d) & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | (!d)), (7 * i) % 16),
            };

            let f = f.wrapping_add(a).wrapping_add(K[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }

        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    [
        a0 as u8,
        (a0 >> 8) as u8,
        (a0 >> 16) as u8,
        (a0 >> 24) as u8,
        b0 as u8,
        (b0 >> 8) as u8,
        (b0 >> 16) as u8,
        (b0 >> 24) as u8,
        c0 as u8,
        (c0 >> 8) as u8,
        (c0 >> 16) as u8,
        (c0 >> 24) as u8,
        d0 as u8,
        (d0 >> 8) as u8,
        (d0 >> 16) as u8,
        (d0 >> 24) as u8,
    ]
}

fn md5_hash(message: &[u8]) -> String {
    bytes_to_hex(&md5_digest(message))
}

fn md5_hex_for_string(
    object: &Value,
    start_raw: Option<&Value>,
    end_raw: Option<&Value>,
) -> Result<String, Flow> {
    let input = match object {
        Value::Str(s) => (**s).clone(),
        _ => unreachable!("md5_hex_for_string only accepts string object"),
    };
    let len = storage_char_len(&input) as i64;
    let start_arg = start_raw.cloned().unwrap_or(Value::Nil);
    let end_arg = end_raw.cloned().unwrap_or(Value::Nil);
    let start =
        normalize_secure_hash_index(start_raw, 0, len, object, &start_arg, &end_arg)? as usize;
    let end = normalize_secure_hash_index(end_raw, len, len, object, &start_arg, &end_arg)? as usize;

    if start > end {
        return Err(signal(
            "args-out-of-range",
            vec![object.clone(), start_arg.clone(), end_arg.clone()],
        ));
    }

    let slice = storage_substring(&input, start, end).ok_or_else(|| {
        signal(
            "args-out-of-range",
            vec![object.clone(), start_arg.clone(), end_arg.clone()],
        )
    })?;
    Ok(md5_hash(slice.as_bytes()))
}

fn normalize_md5_buffer_position(
    val: Option<&Value>,
    default: i64,
    point_min: i64,
    point_max: i64,
    start_arg: &Value,
    end_arg: &Value,
) -> Result<i64, Flow> {
    let raw = match val {
        None => default,
        Some(v) if v.is_nil() => default,
        Some(v) => require_int_or_marker(v)?,
    };
    if raw < point_min || raw > point_max {
        return Err(signal(
            "args-out-of-range",
            vec![start_arg.clone(), end_arg.clone()],
        ));
    }
    Ok(raw)
}

fn hash_slice_for_buffer(
    eval: &super::eval::Evaluator,
    buffer_id: crate::buffer::BufferId,
    start_raw: Option<&Value>,
    end_raw: Option<&Value>,
) -> Result<String, Flow> {
    let buf = eval
        .buffers
        .get(buffer_id)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;

    let text = buf.buffer_string();
    let point_min = buf.text.byte_to_char(buf.point_min()) as i64 + 1;
    let point_max = buf.text.byte_to_char(buf.point_max()) as i64 + 1;

    let start_arg = start_raw.cloned().unwrap_or(Value::Nil);
    let end_arg = end_raw.cloned().unwrap_or(Value::Nil);
    let start = normalize_md5_buffer_position(
        start_raw, point_min, point_min, point_max, &start_arg, &end_arg,
    )?;
    let end =
        normalize_md5_buffer_position(end_raw, point_max, point_min, point_max, &start_arg, &end_arg)?;

    let (lo, hi) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let lo_idx = (lo - point_min) as usize;
    let hi_idx = (hi - point_min) as usize;
    storage_substring(&text, lo_idx, hi_idx).ok_or_else(|| {
        signal(
            "args-out-of-range",
            vec![start_arg.clone(), end_arg.clone()],
        )
    })
}

fn md5_hex_for_buffer(
    eval: &super::eval::Evaluator,
    buffer_id: crate::buffer::BufferId,
    start_raw: Option<&Value>,
    end_raw: Option<&Value>,
) -> Result<String, Flow> {
    let slice = hash_slice_for_buffer(eval, buffer_id, start_raw, end_raw)?;
    Ok(md5_hash(slice.as_bytes()))
}

fn secure_hash_algorithm_name(val: &Value) -> Result<String, Flow> {
    match val {
        Value::Symbol(s) => Ok(s.clone()),
        Value::Nil => Ok("nil".to_string()),
        Value::True => Ok("t".to_string()),
        Value::Keyword(k) => Ok(format!(":{k}")),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), other.clone()],
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
            "args-out-of-range",
            vec![object.clone(), start_arg.clone(), end_arg.clone()],
        ));
    }
    Ok(idx)
}

fn invalid_object_payload(val: &Value) -> Value {
    if val.is_nil() {
        Value::string("nil")
    } else {
        val.clone()
    }
}

fn bytes_to_lisp_binary_string(bytes: &[u8]) -> String {
    bytes_to_unibyte_storage_string(bytes)
}

fn hash_slice_for_string(
    object: &Value,
    start_raw: Option<&Value>,
    end_raw: Option<&Value>,
) -> Result<String, Flow> {
    let input = match object {
        Value::Str(s) => (**s).clone(),
        _ => unreachable!("hash_slice_for_string only accepts string object"),
    };
    let len = storage_char_len(&input) as i64;
    let start_arg = start_raw.cloned().unwrap_or(Value::Nil);
    let end_arg = end_raw.cloned().unwrap_or(Value::Nil);
    let start =
        normalize_secure_hash_index(start_raw, 0, len, object, &start_arg, &end_arg)? as usize;
    let end =
        normalize_secure_hash_index(end_raw, len, len, object, &start_arg, &end_arg)? as usize;

    if start > end {
        return Err(signal(
            "args-out-of-range",
            vec![object.clone(), start_arg.clone(), end_arg.clone()],
        ));
    }

    storage_substring(&input, start, end).ok_or_else(|| {
        signal(
            "args-out-of-range",
            vec![object.clone(), start_arg.clone(), end_arg.clone()],
        )
    })
}

fn secure_hash_digest_bytes(algo_name: &str, input: &str) -> Result<Vec<u8>, Flow> {
    let digest = match algo_name {
        "md5" => md5_digest(input.as_bytes()).to_vec(),
        "sha1" => {
            let mut h = Sha1::new();
            h.update(input.as_bytes());
            h.finalize().to_vec()
        }
        "sha224" => {
            let mut h = Sha224::new();
            h.update(input.as_bytes());
            h.finalize().to_vec()
        }
        "sha256" => {
            let mut h = Sha256::new();
            h.update(input.as_bytes());
            h.finalize().to_vec()
        }
        "sha384" => {
            let mut h = Sha384::new();
            h.update(input.as_bytes());
            h.finalize().to_vec()
        }
        "sha512" => {
            let mut h = Sha512::new();
            h.update(input.as_bytes());
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
/// Returns a digest string for supported hash algorithms.
pub(crate) fn builtin_secure_hash(args: Vec<Value>) -> EvalResult {
    expect_range_args("secure-hash", &args, 2, 5)?;
    let algo_name = secure_hash_algorithm_name(&args[0])?;

    let object = &args[1];
    let input = match object {
        Value::Str(_) => hash_slice_for_string(object, args.get(2), args.get(3))?,
        other => {
            return Err(signal(
                "error",
                vec![
                    Value::string("Invalid object argument"),
                    invalid_object_payload(other),
                ],
            ));
        }
    };

    let digest = secure_hash_digest_bytes(&algo_name, &input)?;

    let binary = args.get(4).is_some_and(|v| v.is_truthy());
    if binary {
        Ok(Value::string(bytes_to_lisp_binary_string(&digest)))
    } else {
        Ok(Value::string(bytes_to_hex(&digest)))
    }
}

/// (secure-hash ALGORITHM OBJECT &optional START END BINARY)
///
/// Evaluator-aware implementation that also supports buffer objects.
pub(crate) fn builtin_secure_hash_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_range_args("secure-hash", &args, 2, 5)?;
    let algo_name = secure_hash_algorithm_name(&args[0])?;

    let object = &args[1];
    let input = match object {
        Value::Str(_) => hash_slice_for_string(object, args.get(2), args.get(3))?,
        Value::Buffer(id) => hash_slice_for_buffer(eval, *id, args.get(2), args.get(3))?,
        other => {
            return Err(signal(
                "error",
                vec![
                    Value::string("Invalid object argument"),
                    invalid_object_payload(other),
                ],
            ));
        }
    };

    let digest = secure_hash_digest_bytes(&algo_name, &input)?;
    let binary = args.get(4).is_some_and(|v| v.is_truthy());
    if binary {
        Ok(Value::string(bytes_to_lisp_binary_string(&digest)))
    } else {
        Ok(Value::string(bytes_to_hex(&digest)))
    }
}

/// (buffer-hash &optional BUFFER-OR-NAME)
/// Stub: always returns "0".
pub(crate) fn builtin_buffer_hash(args: Vec<Value>) -> EvalResult {
    expect_range_args("buffer-hash", &args, 0, 1)?;
    Ok(Value::string("0"))
}

/// (buffer-hash &optional BUFFER-OR-NAME)
/// Evaluator-aware implementation used at runtime.
pub(crate) fn builtin_buffer_hash_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_range_args("buffer-hash", &args, 0, 1)?;

    let buffer_id = if args.is_empty() || args[0].is_nil() {
        eval.buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
            .id
    } else {
        match &args[0] {
            Value::Buffer(id) => *id,
            Value::Str(name) => eval
                .buffers
                .find_buffer_by_name(name)
                .ok_or_else(|| signal("error", vec![Value::string(format!("No buffer named {name}"))]))?,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), other.clone()],
                ))
            }
        }
    };

    // GNU Emacs accepts killed buffer objects and hashes as empty content.
    let text = eval
        .buffers
        .get(buffer_id)
        .map(|buf| buf.buffer_string())
        .unwrap_or_default();

    let mut hasher = Sha1::new();
    hasher.update(text.as_bytes());
    Ok(Value::string(bytes_to_hex(&hasher.finalize())))
}

/// (locale-info ITEM)
/// Stub: always returns nil.
pub(crate) fn builtin_locale_info(args: Vec<Value>) -> EvalResult {
    expect_args("locale-info", &args, 1)?;
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Equality
// ---------------------------------------------------------------------------

/// (eql OBJ1 OBJ2) -- like eq but also equal for same-value floats.
pub(crate) fn builtin_eql(args: Vec<Value>) -> EvalResult {
    expect_args("eql", &args, 2)?;
    let result = match (&args[0], &args[1]) {
        // Float comparison by value (same bit pattern)
        (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
        // For everything else, delegate to eq semantics
        _ => eq_value(&args[0], &args[1]),
    };
    Ok(Value::bool(result))
}

/// (equal-including-properties O1 O2)
/// Like `equal` but also checks text properties. Since our implementation
/// does not yet track text properties on strings, this behaves the same
/// as `equal` for now.
pub(crate) fn builtin_equal_including_properties(args: Vec<Value>) -> EvalResult {
    expect_args("equal-including-properties", &args, 2)?;
    Ok(Value::bool(equal_value(&args[0], &args[1], 0)))
}

// ---------------------------------------------------------------------------
// Widget helpers
// ---------------------------------------------------------------------------

/// (widget-get WIDGET PROPERTY)
/// WIDGET is a list (plist-like).  Extract PROPERTY from the widget's plist
/// tail (skip car which is the widget type).
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
                return Ok(items[i + 1].clone());
            }
            i += 2;
        }
    }
    Ok(Value::Nil)
}

/// (widget-put WIDGET PROPERTY VALUE)
/// Set PROPERTY to VALUE in the widget plist. Returns VALUE.
/// Since widgets are mutable lists, we modify in-place by walking cons cells.
pub(crate) fn builtin_widget_put(args: Vec<Value>) -> EvalResult {
    expect_args("widget-put", &args, 3)?;
    let widget = &args[0];
    let property = &args[1];
    let value = &args[2];

    // Walk the cdr of WIDGET (skip the type cons cell) looking for PROPERTY.
    if let Value::Cons(first_cell) = widget {
        let mut cursor = {
            let cell = first_cell.lock().expect("poisoned");
            cell.cdr.clone()
        };
        loop {
            match cursor {
                Value::Cons(ref cell_arc) => {
                    let key = {
                        let cell = cell_arc.lock().expect("poisoned");
                        cell.car.clone()
                    };
                    if equal_value(&key, property, 0) {
                        // Found it — the next cons cell holds the value
                        let next = {
                            let cell = cell_arc.lock().expect("poisoned");
                            cell.cdr.clone()
                        };
                        if let Value::Cons(val_cell_arc) = next {
                            let mut val_cell = val_cell_arc.lock().expect("poisoned");
                            val_cell.car = value.clone();
                            return Ok(value.clone());
                        }
                        break;
                    }
                    // Skip value, move to next key
                    let after_key = {
                        let cell = cell_arc.lock().expect("poisoned");
                        cell.cdr.clone()
                    };
                    if let Value::Cons(val_arc) = after_key {
                        let val_cell = val_arc.lock().expect("poisoned");
                        cursor = val_cell.cdr.clone();
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        // Property not found — append to end of widget plist (after type).
        // Prepend (PROPERTY VALUE ...) to the cdr of the first cons cell.
        let old_cdr = {
            let cell = first_cell.lock().expect("poisoned");
            cell.cdr.clone()
        };
        let new_tail = Value::cons(property.clone(), Value::cons(value.clone(), old_cdr));
        {
            let mut cell = first_cell.lock().expect("poisoned");
            cell.cdr = new_tail;
        }
    }

    Ok(value.clone())
}

/// (widget-apply WIDGET PROPERTY &rest ARGS)
/// Apply WIDGET's PROPERTY function to WIDGET and ARGS.
pub(crate) fn builtin_widget_apply(args: Vec<Value>) -> EvalResult {
    expect_min_args("widget-apply", &args, 2)?;
    let widget = args[0].clone();
    let property = args[1].clone();

    let function = builtin_widget_get(vec![widget.clone(), property])?;
    if function.is_nil() {
        return Err(signal("void-function", vec![Value::Nil]));
    }

    let mut call_args = Vec::with_capacity(args.len().saturating_sub(1));
    call_args.push(widget);
    call_args.extend_from_slice(&args[2..]);

    match function {
        Value::Symbol(name) => {
            if let Some(result) = super::builtins::dispatch_builtin_pure(&name, call_args) {
                result
            } else {
                Err(signal("void-function", vec![Value::symbol(name)]))
            }
        }
        Value::Subr(name) => {
            if let Some(result) = super::builtins::dispatch_builtin_pure(&name, call_args) {
                result
            } else {
                Err(signal("void-function", vec![Value::symbol(name)]))
            }
        }
        other => Err(signal("invalid-function", vec![other])),
    }
}

// ---------------------------------------------------------------------------
// Identity and string type coercions
// ---------------------------------------------------------------------------

/// (identity ARG) -- return ARG unchanged.
pub(crate) fn builtin_identity(args: Vec<Value>) -> EvalResult {
    expect_args("identity", &args, 1)?;
    Ok(args[0].clone())
}

/// (string-to-multibyte STRING) -- convert unibyte storage bytes to multibyte chars.
pub(crate) fn builtin_string_to_multibyte(args: Vec<Value>) -> EvalResult {
    super::misc::builtin_string_to_multibyte(args)
}

/// (string-to-unibyte STRING) -- convert to unibyte storage.
pub(crate) fn builtin_string_to_unibyte(args: Vec<Value>) -> EvalResult {
    super::misc::builtin_string_to_unibyte(args)
}

/// (string-make-multibyte STRING) -- convert unibyte storage bytes to multibyte chars.
pub(crate) fn builtin_string_make_multibyte(args: Vec<Value>) -> EvalResult {
    expect_args("string-make-multibyte", &args, 1)?;
    match &args[0] {
        Value::Str(s) => {
            let mut out = String::with_capacity(s.len());
            for ch in s.chars() {
                let cp = ch as u32;
                if (UNIBYTE_BYTE_SENTINEL_MIN..=UNIBYTE_BYTE_SENTINEL_MAX).contains(&cp) {
                    let byte = cp - UNIBYTE_BYTE_SENTINEL_BASE;
                    if byte <= 0x7F {
                        out.push(char::from_u32(byte).expect("ascii scalar"));
                    } else {
                        let raw_code = 0x3FFF00 + byte;
                        let encoded = encode_nonunicode_char_for_storage(raw_code)
                            .expect("raw-byte code should be encodable");
                        out.push_str(&encoded);
                    }
                    continue;
                }
                out.push(ch);
            }
            Ok(Value::string(out))
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

/// (string-make-unibyte STRING) -- convert each character code to a single byte.
pub(crate) fn builtin_string_make_unibyte(args: Vec<Value>) -> EvalResult {
    expect_args("string-make-unibyte", &args, 1)?;
    match &args[0] {
        Value::Str(s) => {
            let bytes: Vec<u8> = decode_storage_char_codes(s)
                .into_iter()
                .map(|cp| (cp & 0xFF) as u8)
                .collect();
            Ok(Value::string(bytes_to_unibyte_storage_string(&bytes)))
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
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
    expect_range_args("compare-strings", &args, 6, 7)?;

    let s1 = require_string("compare-strings", &args[0])?;
    let s2 = require_string("compare-strings", &args[3])?;

    let chars1: Vec<char> = s1.chars().collect();
    let chars2: Vec<char> = s2.chars().collect();

    let start1 = match &args[1] {
        Value::Nil => 0usize,
        Value::Int(n) => ((*n).max(0) as usize).saturating_sub(1), // 1-based to 0-based, nil=0
        _ => 0,
    };
    let end1 = match &args[2] {
        Value::Nil => chars1.len(),
        Value::Int(n) => (*n as usize).min(chars1.len()),
        _ => chars1.len(),
    };
    let start2 = match &args[4] {
        Value::Nil => 0usize,
        Value::Int(n) => ((*n).max(0) as usize).saturating_sub(1),
        _ => 0,
    };
    let end2 = match &args[5] {
        Value::Nil => chars2.len(),
        Value::Int(n) => (*n as usize).min(chars2.len()),
        _ => chars2.len(),
    };

    let ignore_case = args.get(6).map_or(false, |v| v.is_truthy());

    let sub1 = &chars1[start1.min(chars1.len())..end1.min(chars1.len())];
    let sub2 = &chars2[start2.min(chars2.len())..end2.min(chars2.len())];

    let len = sub1.len().min(sub2.len());
    for i in 0..len {
        let c1 = if ignore_case {
            sub1[i].to_lowercase().next().unwrap_or(sub1[i])
        } else {
            sub1[i]
        };
        let c2 = if ignore_case {
            sub2[i].to_lowercase().next().unwrap_or(sub2[i])
        } else {
            sub2[i]
        };
        if c1 != c2 {
            let pos = (i + 1) as i64; // 1-based
            if c1 < c2 {
                return Ok(Value::Int(-pos));
            } else {
                return Ok(Value::Int(pos));
            }
        }
    }

    if sub1.len() == sub2.len() {
        Ok(Value::True)
    } else if sub1.len() < sub2.len() {
        Ok(Value::Int(-((len + 1) as i64)))
    } else {
        Ok(Value::Int((len + 1) as i64))
    }
}

/// (string-version-lessp S1 S2) -- version-aware string comparison.
///
/// Compares strings character by character, but when both strings have a
/// run of digits at the same position, the digit runs are compared as
/// integers (so "foo2" < "foo10").
pub(crate) fn builtin_string_version_lessp(args: Vec<Value>) -> EvalResult {
    expect_args("string-version-lessp", &args, 2)?;
    let s1 = require_string("string-version-lessp", &args[0])?;
    let s2 = require_string("string-version-lessp", &args[1])?;

    let c1: Vec<char> = s1.chars().collect();
    let c2: Vec<char> = s2.chars().collect();

    let mut i = 0;
    let mut j = 0;

    while i < c1.len() && j < c2.len() {
        if c1[i].is_ascii_digit() && c2[j].is_ascii_digit() {
            // Extract numeric runs and compare as integers
            let mut n1: u64 = 0;
            while i < c1.len() && c1[i].is_ascii_digit() {
                n1 = n1
                    .saturating_mul(10)
                    .saturating_add(c1[i] as u64 - '0' as u64);
                i += 1;
            }
            let mut n2: u64 = 0;
            while j < c2.len() && c2[j].is_ascii_digit() {
                n2 = n2
                    .saturating_mul(10)
                    .saturating_add(c2[j] as u64 - '0' as u64);
                j += 1;
            }
            if n1 != n2 {
                return Ok(Value::bool(n1 < n2));
            }
        } else {
            if c1[i] != c2[j] {
                return Ok(Value::bool(c1[i] < c2[j]));
            }
            i += 1;
            j += 1;
        }
    }

    Ok(Value::bool(c1.len() < c2.len()))
}

/// (string-collate-lessp S1 S2 &optional LOCALE IGNORE-CASE)
/// Simple lexicographic comparison (locale is ignored).
pub(crate) fn builtin_string_collate_lessp(args: Vec<Value>) -> EvalResult {
    expect_range_args("string-collate-lessp", &args, 2, 4)?;
    let s1 = require_string("string-collate-lessp", &args[0])?;
    let s2 = require_string("string-collate-lessp", &args[1])?;
    let ignore_case = args.get(3).map_or(false, |v| v.is_truthy());

    let result = if ignore_case {
        s1.to_lowercase() < s2.to_lowercase()
    } else {
        s1 < s2
    };
    Ok(Value::bool(result))
}

/// (string-collate-equalp S1 S2 &optional LOCALE IGNORE-CASE)
/// Simple lexicographic equality (locale is ignored).
pub(crate) fn builtin_string_collate_equalp(args: Vec<Value>) -> EvalResult {
    expect_range_args("string-collate-equalp", &args, 2, 4)?;
    let s1 = require_string("string-collate-equalp", &args[0])?;
    let s2 = require_string("string-collate-equalp", &args[1])?;
    let ignore_case = args.get(3).map_or(false, |v| v.is_truthy());

    let result = if ignore_case {
        s1.to_lowercase() == s2.to_lowercase()
    } else {
        s1 == s2
    };
    Ok(Value::bool(result))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::{print, string_escape};

    // ---- Base64 standard ----

    #[test]
    fn base64_encode_empty() {
        let r = builtin_base64_encode_string(vec![Value::string(""), Value::True]).unwrap();
        assert_eq!(r.as_str(), Some(""));
    }

    #[test]
    fn base64_encode_hello() {
        let r = builtin_base64_encode_string(vec![Value::string("Hello"), Value::True]).unwrap();
        assert_eq!(r.as_str(), Some("SGVsbG8="));
    }

    #[test]
    fn base64_encode_padding_1() {
        // "a" -> "YQ=="
        let r = builtin_base64_encode_string(vec![Value::string("a"), Value::True]).unwrap();
        assert_eq!(r.as_str(), Some("YQ=="));
    }

    #[test]
    fn base64_encode_padding_2() {
        // "ab" -> "YWI="
        let r = builtin_base64_encode_string(vec![Value::string("ab"), Value::True]).unwrap();
        assert_eq!(r.as_str(), Some("YWI="));
    }

    #[test]
    fn base64_encode_no_padding_3() {
        // "abc" -> "YWJj" (no padding needed)
        let r = builtin_base64_encode_string(vec![Value::string("abc"), Value::True]).unwrap();
        assert_eq!(r.as_str(), Some("YWJj"));
    }

    #[test]
    fn base64_roundtrip() {
        let original = "The quick brown fox jumps over the lazy dog";
        let encoded =
            builtin_base64_encode_string(vec![Value::string(original), Value::True]).unwrap();
        let decoded = builtin_base64_decode_string(vec![encoded]).unwrap();
        assert_eq!(decoded.as_str(), Some(original));
    }

    #[test]
    fn base64_decode_invalid() {
        let r = builtin_base64_decode_string(vec![Value::string("!!!!")]).unwrap();
        assert!(r.is_nil());
    }

    // ---- Base64 URL ----

    #[test]
    fn base64url_encode_no_pad() {
        let r = builtin_base64url_encode_string(vec![Value::string("a"), Value::True]).unwrap();
        // URL-safe, no padding
        assert_eq!(r.as_str(), Some("YQ"));
    }

    #[test]
    fn base64url_encode_with_pad() {
        let r = builtin_base64url_encode_string(vec![Value::string("a")]).unwrap();
        assert_eq!(r.as_str(), Some("YQ=="));
    }

    #[test]
    fn base64url_roundtrip() {
        let original = "Hello+World/Foo";
        let encoded =
            builtin_base64url_encode_string(vec![Value::string(original), Value::True]).unwrap();
        let decoded = builtin_base64_decode_string(vec![encoded, Value::True]).unwrap();
        assert_eq!(decoded.as_str(), Some(original));
    }

    #[test]
    fn base64url_uses_dash_underscore() {
        // Standard base64 of "?>" is "Pz4=" which contains no + or /.
        // Use a string that we know produces different chars in std vs url.
        // "abc?+/" in standard base64 is "YWJjPysvg" — contains + and /.
        // Actually, just test that the url alphabet is used:
        // base64url of ">?" is "Pj8" (std would be "Pj8" too — same for ASCII).
        // Instead, directly encode bytes [0xFF] which in std is "/w==" and url is "_w==".
        // Since our strings are UTF-8, we use a string with codepoint U+00FF (latin small y with diaeresis).
        let input = "\u{00FF}"; // UTF-8: [0xC3, 0xBF]
        let std_enc =
            builtin_base64_encode_string(vec![Value::string(input), Value::True]).unwrap();
        let url_enc =
            builtin_base64url_encode_string(vec![Value::string(input), Value::True]).unwrap();
        // Standard and URL should differ if the encoding contains + or /
        // For [0xC3, 0xBF]: std = "w78=" which has no + or /... let's just
        // verify neither + nor / appear in url encoding.
        let s = url_enc.as_str().unwrap();
        assert!(!s.contains('+'), "URL-safe encoding should not contain '+'");
        assert!(!s.contains('/'), "URL-safe encoding should not contain '/'");
        // Also verify the standard encoding does not contain - or _
        let s_std = std_enc.as_str().unwrap();
        assert!(
            !s_std.contains('-'),
            "Standard encoding should not contain '-'"
        );
        assert!(
            !s_std.contains('_'),
            "Standard encoding should not contain '_'"
        );
    }

    // ---- MD5 ----

    #[test]
    fn md5_empty() {
        let r = builtin_md5(vec![Value::string("")]).unwrap();
        assert_eq!(r.as_str(), Some("d41d8cd98f00b204e9800998ecf8427e"));
    }

    #[test]
    fn md5_hello() {
        // md5("Hello") = 8b1a9953c4611296a827abf8c47804d7
        let r = builtin_md5(vec![Value::string("Hello")]).unwrap();
        assert_eq!(r.as_str(), Some("8b1a9953c4611296a827abf8c47804d7"));
    }

    #[test]
    fn md5_abc() {
        let r = builtin_md5(vec![Value::string("abc")]).unwrap();
        assert_eq!(r.as_str(), Some("900150983cd24fb0d6963f7d28e17f72"));
    }

    #[test]
    fn md5_fox() {
        let r = builtin_md5(vec![Value::string(
            "The quick brown fox jumps over the lazy dog",
        )])
        .unwrap();
        assert_eq!(r.as_str(), Some("9e107d9d372bb6826bd81d3542a419d6"));
    }

    #[test]
    fn md5_string_range_errors() {
        match builtin_md5(vec![Value::string("abc"), Value::Int(2), Value::Int(1)]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "args-out-of-range");
                assert_eq!(
                    sig.data,
                    vec![Value::string("abc"), Value::Int(2), Value::Int(1)]
                );
            }
            other => panic!("expected args-out-of-range signal, got {other:?}"),
        }
    }

    #[test]
    fn md5_string_index_type_error() {
        match builtin_md5(vec![Value::string("abc"), Value::True, Value::Int(1)]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data.first(), Some(&Value::symbol("integerp")));
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }
    }

    #[test]
    fn md5_invalid_object_errors() {
        match builtin_md5(vec![Value::Nil]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data.first().and_then(|v| v.as_str()),
                    Some("Invalid object argument")
                );
                assert_eq!(sig.data.get(1), Some(&Value::string("nil")));
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }

    #[test]
    fn md5_unknown_coding_system_errors() {
        match builtin_md5(vec![
            Value::string("abc"),
            Value::Nil,
            Value::Nil,
            Value::symbol("no-such"),
        ]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "coding-system-error");
                assert_eq!(sig.data, vec![Value::symbol("no-such")]);
            }
            other => panic!("expected coding-system-error signal, got {other:?}"),
        }
    }

    #[test]
    fn md5_unknown_coding_system_ignored_with_noerror() {
        let r = builtin_md5(vec![
            Value::string("abc"),
            Value::Nil,
            Value::Nil,
            Value::symbol("no-such"),
            Value::True,
        ])
        .unwrap();
        assert_eq!(r.as_str(), Some("900150983cd24fb0d6963f7d28e17f72"));
    }

    #[test]
    fn md5_non_symbol_coding_system_errors() {
        match builtin_md5(vec![Value::string("abc"), Value::Nil, Value::Nil, Value::Int(1)]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "coding-system-error");
                assert_eq!(sig.data, vec![Value::Int(1)]);
            }
            other => panic!("expected coding-system-error signal, got {other:?}"),
        }
    }

    #[test]
    fn md5_eval_buffer_core_semantics() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        {
            let buf = eval.buffers.current_buffer_mut().expect("current buffer");
            buf.delete_region(buf.point_min(), buf.point_max());
            buf.insert("abc");
        }
        let id = eval.buffers.current_buffer().expect("current buffer").id;

        let full = builtin_md5_eval(&mut eval, vec![Value::Buffer(id)]).unwrap();
        assert_eq!(full.as_str(), Some("900150983cd24fb0d6963f7d28e17f72"));

        let swapped = builtin_md5_eval(&mut eval, vec![Value::Buffer(id), Value::Int(4), Value::Int(3)])
            .unwrap();
        assert_eq!(swapped.as_str(), Some("4a8a08f09d37b73795649038408b5f33"));
    }

    #[test]
    fn md5_eval_buffer_range_errors() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        {
            let buf = eval.buffers.current_buffer_mut().expect("current buffer");
            buf.delete_region(buf.point_min(), buf.point_max());
            buf.insert("abc");
        }
        let id = eval.buffers.current_buffer().expect("current buffer").id;

        match builtin_md5_eval(&mut eval, vec![Value::Buffer(id), Value::Int(5)]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "args-out-of-range");
                assert_eq!(sig.data, vec![Value::Int(5), Value::Nil]);
            }
            other => panic!("expected args-out-of-range signal, got {other:?}"),
        }
    }

    #[test]
    fn md5_eval_buffer_index_type_error() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        let id = eval.buffers.current_buffer().expect("current buffer").id;

        match builtin_md5_eval(&mut eval, vec![Value::Buffer(id), Value::True, Value::Int(3)]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data.first(), Some(&Value::symbol("integer-or-marker-p")));
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }
    }

    #[test]
    fn md5_eval_deleted_buffer_errors() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        let id = eval.buffers.create_buffer("*md5-doomed*");
        assert!(eval.buffers.kill_buffer(id));

        match builtin_md5_eval(&mut eval, vec![Value::Buffer(id)]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data.first().and_then(|v| v.as_str()),
                    Some("Selecting deleted buffer")
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }

    // ---- secure-hash ----

    #[test]
    fn secure_hash_sha256_known() {
        let r = builtin_secure_hash(vec![Value::symbol("sha256"), Value::string("abc")]).unwrap();
        assert_eq!(
            r.as_str(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
    }

    #[test]
    fn secure_hash_sha1_known() {
        let r = builtin_secure_hash(vec![Value::symbol("sha1"), Value::string("abc")]).unwrap();
        assert_eq!(r.as_str(), Some("a9993e364706816aba3e25717850c26c9cd0d89d"));
    }

    #[test]
    fn secure_hash_md5_known() {
        let r = builtin_secure_hash(vec![Value::symbol("md5"), Value::string("abc")]).unwrap();
        assert_eq!(r.as_str(), Some("900150983cd24fb0d6963f7d28e17f72"));
    }

    #[test]
    fn secure_hash_binary_string_uses_unibyte_storage() {
        let r = builtin_secure_hash(vec![
            Value::symbol("sha1"),
            Value::string("abc"),
            Value::Nil,
            Value::Nil,
            Value::True,
        ])
        .unwrap();

        let s = r.as_str().expect("binary secure-hash should return a string");
        assert_eq!(string_escape::storage_byte_len(s), 20);
        assert_eq!(
            string_escape::decode_storage_char_codes(s).first(),
            Some(&169)
        );

        let printed = print::print_value_bytes(&r);
        assert_eq!(&printed[..3], b"\"\xC0\xA9");
    }

    #[test]
    fn secure_hash_subrange_semantics() {
        let r = builtin_secure_hash(vec![
            Value::symbol("sha256"),
            Value::string("abcdef"),
            Value::Int(1),
            Value::Int(4),
        ])
        .unwrap();
        assert_eq!(
            r.as_str(),
            Some("a6b0f90d2ac2b8d1f250c687301aef132049e9016df936680e81fa7bc7d81d70")
        );
    }

    #[test]
    fn secure_hash_invalid_algorithm_errors() {
        match builtin_secure_hash(vec![Value::symbol("no-such"), Value::string("abc")]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data.first().and_then(|v| v.as_str()),
                    Some("Invalid algorithm arg: no-such")
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }

    #[test]
    fn secure_hash_invalid_algorithm_type_errors() {
        match builtin_secure_hash(vec![Value::Int(1), Value::string("abc")]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data.first(), Some(&Value::symbol("symbolp")));
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }
    }

    #[test]
    fn secure_hash_invalid_object_errors() {
        match builtin_secure_hash(vec![Value::symbol("sha256"), Value::Int(123)]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data.first().and_then(|v| v.as_str()),
                    Some("Invalid object argument")
                );
                assert_eq!(sig.data.get(1), Some(&Value::Int(123)));
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }

    #[test]
    fn secure_hash_eval_buffer_sha1() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        {
            let buf = eval.buffers.current_buffer_mut().expect("current buffer");
            buf.delete_region(buf.point_min(), buf.point_max());
            buf.insert("abc");
        }
        let id = eval.buffers.current_buffer().expect("current buffer").id;
        let r =
            builtin_secure_hash_eval(&mut eval, vec![Value::symbol("sha1"), Value::Buffer(id)])
                .unwrap();
        assert_eq!(r.as_str(), Some("a9993e364706816aba3e25717850c26c9cd0d89d"));
    }

    #[test]
    fn secure_hash_eval_buffer_range_errors() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        {
            let buf = eval.buffers.current_buffer_mut().expect("current buffer");
            buf.delete_region(buf.point_min(), buf.point_max());
            buf.insert("abc");
        }
        let id = eval.buffers.current_buffer().expect("current buffer").id;

        match builtin_secure_hash_eval(
            &mut eval,
            vec![Value::symbol("sha1"), Value::Buffer(id), Value::Int(5)],
        ) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "args-out-of-range");
                assert_eq!(sig.data, vec![Value::Int(5), Value::Nil]);
            }
            other => panic!("expected args-out-of-range signal, got {other:?}"),
        }
    }

    #[test]
    fn secure_hash_eval_buffer_index_type_error() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        let id = eval.buffers.current_buffer().expect("current buffer").id;

        match builtin_secure_hash_eval(
            &mut eval,
            vec![
                Value::symbol("sha1"),
                Value::Buffer(id),
                Value::True,
                Value::Int(3),
            ],
        ) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data.first(), Some(&Value::symbol("integer-or-marker-p")));
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }
    }

    #[test]
    fn secure_hash_eval_buffer_marker_range() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        {
            let buf = eval.buffers.current_buffer_mut().expect("current buffer");
            buf.delete_region(buf.point_min(), buf.point_max());
            buf.insert("abc");
        }
        let id = eval.buffers.current_buffer().expect("current buffer").id;
        let marker = crate::elisp::marker::make_marker_value(None, Some(2), false);
        let r = builtin_secure_hash_eval(
            &mut eval,
            vec![
                Value::symbol("sha1"),
                Value::Buffer(id),
                marker,
                Value::Int(4),
            ],
        )
        .unwrap();
        assert_eq!(r.as_str(), Some("5b2505039ac5af9e197f5dad04113906a9cf9a2a"));
    }

    #[test]
    fn secure_hash_eval_deleted_buffer_errors() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        let id = eval.buffers.create_buffer("*secure-doomed*");
        assert!(eval.buffers.kill_buffer(id));

        match builtin_secure_hash_eval(
            &mut eval,
            vec![Value::symbol("sha1"), Value::Buffer(id)],
        ) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data.first().and_then(|v| v.as_str()),
                    Some("Selecting deleted buffer")
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }

    // ---- buffer-hash stub ----

    #[test]
    fn buffer_hash_stub() {
        let r = builtin_buffer_hash(vec![]).unwrap();
        assert_eq!(r.as_str(), Some("0"));
    }

    #[test]
    fn buffer_hash_eval_current_buffer_sha1() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_region(buf.point_min(), buf.point_max());
        buf.insert("abc");
        let r = builtin_buffer_hash_eval(&mut eval, vec![]).unwrap();
        assert_eq!(r.as_str(), Some("a9993e364706816aba3e25717850c26c9cd0d89d"));
    }

    #[test]
    fn buffer_hash_eval_by_name_sha1() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_region(buf.point_min(), buf.point_max());
        buf.insert("abc");
        let name = eval
            .buffers
            .current_buffer()
            .expect("current buffer")
            .name
            .clone();
        let r = builtin_buffer_hash_eval(&mut eval, vec![Value::string(name)]).unwrap();
        assert_eq!(r.as_str(), Some("a9993e364706816aba3e25717850c26c9cd0d89d"));
    }

    #[test]
    fn buffer_hash_eval_missing_name_errors() {
        let mut eval = crate::elisp::eval::Evaluator::new();
        match builtin_buffer_hash_eval(&mut eval, vec![Value::string("*missing*")]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data.first().and_then(|v| v.as_str()),
                    Some("No buffer named *missing*")
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }

    // ---- locale-info stub ----

    #[test]
    fn locale_info_nil() {
        let r = builtin_locale_info(vec![Value::symbol("decimal-point")]).unwrap();
        assert!(r.is_nil());
    }

    // ---- eql ----

    #[test]
    fn eql_same_float() {
        let r = builtin_eql(vec![Value::Float(1.5), Value::Float(1.5)]).unwrap();
        assert!(r.is_truthy());
    }

    #[test]
    fn eql_different_float() {
        let r = builtin_eql(vec![Value::Float(1.5), Value::Float(2.5)]).unwrap();
        assert!(r.is_nil());
    }

    #[test]
    fn eql_int() {
        let r = builtin_eql(vec![Value::Int(42), Value::Int(42)]).unwrap();
        assert!(r.is_truthy());
    }

    #[test]
    fn eql_different_types() {
        let r = builtin_eql(vec![Value::Int(1), Value::Float(1.0)]).unwrap();
        assert!(r.is_nil());
    }

    // ---- equal-including-properties ----

    #[test]
    fn equal_including_properties_strings() {
        let r = builtin_equal_including_properties(vec![
            Value::string("hello"),
            Value::string("hello"),
        ])
        .unwrap();
        assert!(r.is_truthy());
    }

    // ---- identity ----

    #[test]
    fn identity_returns_arg() {
        let r = builtin_identity(vec![Value::Int(42)]).unwrap();
        assert!(matches!(r, Value::Int(42)));
    }

    // ---- string-to-multibyte / unibyte ----

    #[test]
    fn string_to_multibyte_passthrough_ascii() {
        let r = builtin_string_to_multibyte(vec![Value::string("abc")]).unwrap();
        assert_eq!(r.as_str(), Some("abc"));
    }

    #[test]
    fn string_to_multibyte_wrong_type() {
        let r = builtin_string_to_multibyte(vec![Value::Int(42)]);
        assert!(r.is_err());
    }

    #[test]
    fn string_to_unibyte_ascii_passthrough() {
        let r = builtin_string_to_unibyte(vec![Value::string("abc")]).unwrap();
        assert_eq!(string_escape::decode_storage_char_codes(r.as_str().unwrap()), vec![97, 98, 99]);
    }

    #[test]
    fn string_to_multibyte_promotes_unibyte_byte() {
        let r = builtin_string_to_multibyte(vec![Value::string(
            bytes_to_unibyte_storage_string(&[0xFF]),
        )])
        .unwrap();
        assert_eq!(string_escape::decode_storage_char_codes(r.as_str().unwrap()), vec![0x3FFFFF]);
    }

    #[test]
    fn string_to_unibyte_rejects_unicode_scalar() {
        let r = builtin_string_to_unibyte(vec![Value::string("é")]);
        assert!(r.is_err());
    }

    #[test]
    fn string_make_multibyte_passthrough_ascii() {
        let r = builtin_string_make_multibyte(vec![Value::string("abc")]).unwrap();
        assert_eq!(r.as_str(), Some("abc"));
    }

    #[test]
    fn string_make_multibyte_promotes_unibyte_byte() {
        let r = builtin_string_make_multibyte(vec![Value::string(
            bytes_to_unibyte_storage_string(&[0xFF]),
        )])
        .unwrap();
        assert_eq!(string_escape::decode_storage_char_codes(r.as_str().unwrap()), vec![0x3FFFFF]);
    }

    #[test]
    fn string_make_unibyte_passthrough_ascii() {
        let r = builtin_string_make_unibyte(vec![Value::string("abc")]).unwrap();
        assert_eq!(string_escape::decode_storage_char_codes(r.as_str().unwrap()), vec![97, 98, 99]);
    }

    #[test]
    fn string_make_unibyte_truncates_unicode_char_code() {
        let r = builtin_string_make_unibyte(vec![Value::string("😀")]).unwrap();
        assert_eq!(string_escape::decode_storage_char_codes(r.as_str().unwrap()), vec![0]);
    }

    // ---- compare-strings ----

    #[test]
    fn compare_strings_equal() {
        let r = builtin_compare_strings(vec![
            Value::string("hello"),
            Value::Nil,
            Value::Nil,
            Value::string("hello"),
            Value::Nil,
            Value::Nil,
        ])
        .unwrap();
        assert!(matches!(r, Value::True));
    }

    #[test]
    fn compare_strings_less() {
        let r = builtin_compare_strings(vec![
            Value::string("abc"),
            Value::Nil,
            Value::Nil,
            Value::string("abd"),
            Value::Nil,
            Value::Nil,
        ])
        .unwrap();
        // First diff at position 3, "c" < "d" so negative
        assert_eq!(r.as_int(), Some(-3));
    }

    #[test]
    fn compare_strings_greater() {
        let r = builtin_compare_strings(vec![
            Value::string("abd"),
            Value::Nil,
            Value::Nil,
            Value::string("abc"),
            Value::Nil,
            Value::Nil,
        ])
        .unwrap();
        assert_eq!(r.as_int(), Some(3));
    }

    #[test]
    fn compare_strings_ignore_case() {
        let r = builtin_compare_strings(vec![
            Value::string("Hello"),
            Value::Nil,
            Value::Nil,
            Value::string("hello"),
            Value::Nil,
            Value::Nil,
            Value::True, // IGNORE-CASE
        ])
        .unwrap();
        assert!(matches!(r, Value::True));
    }

    #[test]
    fn compare_strings_subrange() {
        // Compare "hel" from "hello" (chars 1-3) with "hel" from "help" (chars 1-3)
        let r = builtin_compare_strings(vec![
            Value::string("hello"),
            Value::Int(1),
            Value::Int(3),
            Value::string("help"),
            Value::Int(1),
            Value::Int(3),
        ])
        .unwrap();
        assert!(matches!(r, Value::True));
    }

    #[test]
    fn compare_strings_length_diff() {
        let r = builtin_compare_strings(vec![
            Value::string("ab"),
            Value::Nil,
            Value::Nil,
            Value::string("abc"),
            Value::Nil,
            Value::Nil,
        ])
        .unwrap();
        // "ab" shorter — negative
        assert!(r.as_int().unwrap() < 0);
    }

    // ---- string-version-lessp ----

    #[test]
    fn version_lessp_basic() {
        let r = builtin_string_version_lessp(vec![Value::string("foo2"), Value::string("foo10")])
            .unwrap();
        assert!(r.is_truthy());
    }

    #[test]
    fn version_lessp_equal() {
        let r = builtin_string_version_lessp(vec![Value::string("foo10"), Value::string("foo10")])
            .unwrap();
        assert!(r.is_nil());
    }

    #[test]
    fn version_lessp_alpha() {
        let r =
            builtin_string_version_lessp(vec![Value::string("abc"), Value::string("abd")]).unwrap();
        assert!(r.is_truthy());
    }

    #[test]
    fn version_lessp_numeric_segments() {
        let r = builtin_string_version_lessp(vec![
            Value::string("emacs-27.1"),
            Value::string("emacs-27.2"),
        ])
        .unwrap();
        assert!(r.is_truthy());
    }

    // ---- string-collate-lessp ----

    #[test]
    fn collate_lessp_basic() {
        let r =
            builtin_string_collate_lessp(vec![Value::string("abc"), Value::string("abd")]).unwrap();
        assert!(r.is_truthy());
    }

    #[test]
    fn collate_lessp_ignore_case() {
        let r = builtin_string_collate_lessp(vec![
            Value::string("ABC"),
            Value::string("abd"),
            Value::Nil,  // locale
            Value::True, // ignore-case
        ])
        .unwrap();
        assert!(r.is_truthy());
    }

    // ---- string-collate-equalp ----

    #[test]
    fn collate_equalp_basic() {
        let r = builtin_string_collate_equalp(vec![Value::string("abc"), Value::string("abc")])
            .unwrap();
        assert!(r.is_truthy());
    }

    #[test]
    fn collate_equalp_ignore_case() {
        let r = builtin_string_collate_equalp(vec![
            Value::string("ABC"),
            Value::string("abc"),
            Value::Nil,
            Value::True,
        ])
        .unwrap();
        assert!(r.is_truthy());
    }

    #[test]
    fn collate_equalp_different() {
        let r = builtin_string_collate_equalp(vec![Value::string("abc"), Value::string("abd")])
            .unwrap();
        assert!(r.is_nil());
    }

    // ---- widget-get / widget-put ----

    #[test]
    fn widget_get_found() {
        // Widget: (button :tag "OK" :value 42)
        let widget = Value::list(vec![
            Value::symbol("button"),
            Value::Keyword("tag".into()),
            Value::string("OK"),
            Value::Keyword("value".into()),
            Value::Int(42),
        ]);
        let r = builtin_widget_get(vec![widget, Value::Keyword("value".into())]).unwrap();
        assert!(matches!(r, Value::Int(42)));
    }

    #[test]
    fn widget_get_not_found() {
        let widget = Value::list(vec![
            Value::symbol("button"),
            Value::Keyword("tag".into()),
            Value::string("OK"),
        ]);
        let r = builtin_widget_get(vec![widget, Value::Keyword("missing".into())]).unwrap();
        assert!(r.is_nil());
    }

    #[test]
    fn widget_put_existing() {
        let widget = Value::list(vec![
            Value::symbol("button"),
            Value::Keyword("value".into()),
            Value::Int(1),
        ]);
        let r = builtin_widget_put(vec![
            widget.clone(),
            Value::Keyword("value".into()),
            Value::Int(99),
        ])
        .unwrap();
        assert!(matches!(r, Value::Int(99)));

        // Verify it was modified
        let got = builtin_widget_get(vec![widget, Value::Keyword("value".into())]).unwrap();
        assert!(matches!(got, Value::Int(99)));
    }

    #[test]
    fn widget_put_new_property() {
        let widget = Value::list(vec![Value::symbol("button")]);
        let r = builtin_widget_put(vec![
            widget.clone(),
            Value::Keyword("tag".into()),
            Value::string("Hello"),
        ])
        .unwrap();
        assert_eq!(r.as_str(), Some("Hello"));

        let got = builtin_widget_get(vec![widget, Value::Keyword("tag".into())]).unwrap();
        assert_eq!(got.as_str(), Some("Hello"));
    }

    #[test]
    fn widget_apply_missing_property_signals_void_function_nil() {
        let widget = Value::list(vec![Value::symbol("button")]);
        let err = builtin_widget_apply(vec![widget, Value::Keyword("action".into())])
            .expect_err("widget-apply should signal void-function for missing property");
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "void-function");
                assert_eq!(sig.data, vec![Value::Nil]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn widget_apply_calls_symbol_property_with_widget_as_first_arg() {
        let widget = Value::list(vec![
            Value::symbol("button"),
            Value::Keyword("action".into()),
            Value::symbol("car"),
        ]);
        let r = builtin_widget_apply(vec![widget, Value::Keyword("action".into())]).unwrap();
        assert_eq!(r, Value::symbol("button"));
    }

    #[test]
    fn widget_apply_passes_rest_arguments() {
        let widget = Value::list(vec![
            Value::symbol("button"),
            Value::Keyword("action".into()),
            Value::symbol("list"),
        ]);
        let r = builtin_widget_apply(vec![
            widget.clone(),
            Value::Keyword("action".into()),
            Value::Int(1),
            Value::Int(2),
        ])
        .unwrap();
        assert_eq!(r, Value::list(vec![widget, Value::Int(1), Value::Int(2)]));
    }

    #[test]
    fn widget_apply_non_callable_property_signals_invalid_function() {
        let widget = Value::list(vec![
            Value::symbol("button"),
            Value::Keyword("action".into()),
            Value::Int(7),
        ]);
        let err =
            builtin_widget_apply(vec![widget, Value::Keyword("action".into())]).expect_err(
                "widget-apply should reject non-callable property values",
            );
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "invalid-function");
                assert_eq!(sig.data, vec![Value::Int(7)]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    // ---- Line break in base64 ----

    #[test]
    fn base64_encode_line_break() {
        // A string long enough to trigger line breaks at column 76
        let long = "a".repeat(100);
        let encoded = builtin_base64_encode_string(vec![Value::string(long.clone())]).unwrap();
        let s = encoded.as_str().unwrap();
        assert!(s.contains('\n'));

        // No line break variant
        let encoded_no_lb =
            builtin_base64_encode_string(vec![Value::string(long), Value::True]).unwrap();
        let s2 = encoded_no_lb.as_str().unwrap();
        assert!(!s2.contains('\n'));
    }

    #[test]
    fn base64_decode_ignores_whitespace() {
        // Encoded "Hello" with embedded whitespace
        let r = builtin_base64_decode_string(vec![Value::string("SGVs\nbG8=")]).unwrap();
        assert_eq!(r.as_str(), Some("Hello"));
    }
}
