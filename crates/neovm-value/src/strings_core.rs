//! String-construction cores (GNU `fns.c` `substring` and `concat`).
//!
//! `substring_impl` and `concat_impl` are the probe-free cores of the two
//! hot string subrs, moved below the evaluator with the stage-3 split
//! (C12) so the buffer substrate (unique buffer names, mode-line suffix
//! concatenation) reaches them without the lisp layer. The subr wrappers
//! and their `perf_trace` probes stay in `builtins/strings.rs`; edit-layer
//! callers that bypass the wrappers run unprobed (a measurement-coverage
//! change, not a behavior change).
//!
//! The cores depend only on value-layer machinery: the position cache, the
//! text-property tables, `emacs_char` byte conversion, and the list-scan
//! length probe. `expect_int`/`expect_lisp_string` are local copies of the
//! builtins layer's argument extractors, keeping this module free of the
//! evaluator (the per-module copy is established practice: casefiddle,
//! chartable, network, and search each carry one).
//!
//! The unibyte/multibyte conversion subrs (`string-to-multibyte`,
//! `string-as-unibyte`, `string-as-multibyte`) also live here (C12): their
//! bodies are pure `Value` in / `Value` out over `emacs_char` byte
//! conversion, and `lisp/native/misc` re-exports them for its historical
//! callers while the buffer substrate calls them below the lisp layer.

use crate::emacs_char;
use crate::error::{
    EvalResult, Flow, LispCondition, expect_args, expect_max_args, expect_min_args, signal,
};
use crate::list_scan::proper_list_length_or_signal;
use crate::value::positions::{CharLen, CharPos0, CharRange};
use crate::value::text_props::{
    PropertyPlistApplication, ShiftedTextPropertySource, TextPropertyTable,
};
use crate::value::{
    Value, ValueKind, VecLikeType, get_string_text_properties_table_for_value,
    set_string_text_properties_table_for_value,
};

/// Iterate Emacs character codes from a `LispString`.
///
/// For **multibyte** strings each character is decoded straight from the
/// Emacs-internal bytes via `string_char_unchecked`: standard UTF-8 code points
/// (including real Private-Use-Area glyphs such as nerd-font icons) and the
/// extended `0x3FFF00+byte` sequences for eight-bit raw bytes. There is no
/// in-Unicode "sentinel" remapping — that conflated real PUA characters with
/// raw bytes and corrupted them (issue #131). For **unibyte** strings each byte
/// maps to its value directly (0..255).
#[doc(hidden)]
pub fn lisp_string_char_codes(string: &crate::heap_types::LispString) -> Vec<u32> {
    let bytes = string.as_bytes();
    if !string.is_multibyte() {
        return bytes.iter().map(|&b| b as u32).collect();
    }
    let mut out = Vec::with_capacity(string.schars());
    let mut pos = 0;
    for _ in 0..string.schars() {
        let tail = bytes.get(pos..).unwrap_or_default();
        let (cp, len) = crate::heap_types::decode_declared_multibyte_char(tail);
        out.push(cp.code());
        pos += len;
    }
    out
}

/// Return the character code at character index `idx` in `string`, or
/// `None` if `idx` is out of range. Unlike `lisp_string_char_codes`, this
/// does not allocate a `Vec<u32>` — it walks bytes only as far as needed.
/// Mirrors the byte-level access pattern used by GNU's `Faref` on strings
/// (fns.c:3108-3123).
#[doc(hidden)]
pub fn lisp_string_char_at(string: &crate::heap_types::LispString, idx: usize) -> Option<u32> {
    let bytes = string.as_bytes();
    if !string.is_multibyte() {
        return bytes.get(idx).map(|&b| b as u32);
    }
    if idx >= string.schars() {
        return None;
    }
    // `LispString::char_to_byte_pos' answers an all-ASCII multibyte string
    // -- what `buffer-substring' returns for ASCII text -- in O(1).
    let byte_pos = string.char_to_byte_pos(idx);
    let tail = bytes.get(byte_pos..).unwrap_or_default();
    let (cp, _) = crate::heap_types::decode_declared_multibyte_char(tail);
    Some(cp.code())
}

/// Every Lisp-visible string built from a Rust `&str` (doc strings, parsed
/// file data, filenames, pdump payloads, printer output) goes through here:
/// the str carries no storage-String sentinels, so its bytes are already in
/// Emacs internal form and become the `LispString` directly. The legacy
/// storage-decode round-trip that this replaced has been retired (issue
/// #131); the storage codec now survives only inside the
/// buffer-text/runtime-reader layer (`storage_string_to_buffer_bytes`),
/// which is unrelated to the Lisp-string path.
#[doc(hidden)]
pub fn plain_str_to_lisp_string(text: &str, multibyte: bool) -> crate::heap_types::LispString {
    if multibyte {
        crate::heap_types::LispString::from_emacs_bytes(text.as_bytes().to_vec())
    } else {
        crate::heap_types::LispString::from_unibyte(text.as_bytes().to_vec())
    }
}

/// Extract an integer, signaling wrong-type-argument if not.
fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

/// Extract a Lisp string payload, signaling wrong-type-argument if not.
/// The returned borrow is tied to the argument slice, which roots the
/// string for the whole subr call.
fn expect_lisp_string(value: &Value) -> Result<&crate::heap_types::LispString, Flow> {
    value.as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )
    })
}

#[doc(hidden)]
pub fn string_char_range(start: usize, end: usize) -> CharRange {
    CharRange::from_start_len(
        CharPos0::new(start),
        CharLen::new(end.saturating_sub(start)),
    )
}

#[doc(hidden)]
pub fn substring_impl(name: &str, args: &[Value], preserve_props: bool) -> EvalResult {
    expect_min_args(name, args, 1)?;
    expect_max_args(name, args, 3)?;
    match args[0].kind() {
        ValueKind::String => {
            // Borrowed, not cloned: the slice below only reads it, and the
            // source string (`args[0]') outlives the call.
            let src_props = if preserve_props {
                crate::value::borrow_string_text_properties_table_for_value(args[0])
            } else {
                None
            };
            let src = args[0].as_lisp_string().unwrap();
            let (result, sliced_props) = (|| {
                let src_bytes = src.as_bytes();
                let normalize_index =
                    |value: &Value, default: i64, len: i64| -> Result<i64, Flow> {
                        let raw = if value.is_nil() {
                            default
                        } else {
                            expect_int(value)?
                        };
                        let idx = if raw < 0 { len + raw } else { raw };
                        if idx < 0 || idx > len {
                            return Err(signal(
                                LispCondition::ArgsOutOfRange,
                                vec![args[0], args[1], args.get(2).cloned().unwrap_or(Value::NIL)],
                            ));
                        }
                        Ok(idx)
                    };

                // Unibyte strings count storage bytes as characters, even when
                // the underlying bytes are not valid UTF-8.
                if src_props.is_none() && !src.is_multibyte() {
                    let len = src_bytes.len() as i64;
                    let from = if args.len() > 1 {
                        normalize_index(&args[1], 0, len)?
                    } else {
                        0
                    } as usize;
                    let to = if args.len() > 2 {
                        normalize_index(&args[2], len, len)?
                    } else {
                        len
                    } as usize;
                    if from > to {
                        return Err(signal(
                            LispCondition::ArgsOutOfRange,
                            vec![
                                args[0],
                                args.get(1).cloned().unwrap_or(Value::fixnum(0)),
                                args.get(2).cloned().unwrap_or(Value::NIL),
                            ],
                        ));
                    }
                    let result = if preserve_props {
                        src.slice(from, to).expect("validated ascii slice")
                    } else {
                        src.slice_no_properties(from, to)
                            .expect("validated ascii slice")
                    };
                    return Ok::<_, Flow>((result, None));
                }

                let len = src.schars() as i64;
                let from = if args.len() > 1 {
                    normalize_index(&args[1], 0, len)?
                } else {
                    0
                } as usize;

                let to = if args.len() > 2 {
                    normalize_index(&args[2], len, len)?
                } else {
                    len
                } as usize;

                if from > to {
                    return Err(signal(
                        LispCondition::ArgsOutOfRange,
                        vec![
                            args[0],
                            args.get(1).cloned().unwrap_or(Value::fixnum(0)),
                            args.get(2).cloned().unwrap_or(Value::NIL),
                        ],
                    ));
                }
                let (byte_from, byte_to) = if src.is_multibyte() {
                    // Through the position cache: `split-string' and friends
                    // take pieces left to right.
                    let supplied = src.character_count_kind()
                        == crate::heap_types::StringCharacterCountKind::Supplied;
                    // GNU fns.c:1601-1604: these physical endpoint proofs
                    // bypass the cache, whose logical anchors may differ.
                    let bf = if supplied && from == 0 {
                        0
                    } else {
                        crate::string_pos_cache::string_char_to_byte(args[0], src, from)
                    };
                    let bt = if supplied && to == src.schars() {
                        src.sbytes()
                    } else {
                        crate::string_pos_cache::string_char_to_byte(args[0], src, to)
                    };
                    (bf, bt)
                } else {
                    (from, to)
                };
                if byte_to > src_bytes.len() || byte_from > byte_to {
                    return Err(signal(
                        LispCondition::ArgsOutOfRange,
                        vec![
                            args[0],
                            args.get(1).cloned().unwrap_or(Value::fixnum(0)),
                            args.get(2).cloned().unwrap_or(Value::NIL),
                        ],
                    ));
                }
                let result = if preserve_props {
                    src.slice_with_char_bounds(byte_from, byte_to, from, to)
                        .expect("validated storage substring bounds")
                } else {
                    src.slice_no_properties_with_char_bounds(byte_from, byte_to, from, to)
                        .expect("validated storage substring bounds")
                };
                let sliced_props = if let Some(src_table) = src_props {
                    let sliced = src_table
                        .slice_copy_text_properties_char_range(string_char_range(from, to));
                    (!sliced.is_empty()).then_some(sliced)
                } else {
                    None
                };
                Ok::<_, Flow>((result, sliced_props))
            })()?;
            let new_val = Value::heap_string(result);

            // Preserve text properties from source string
            if preserve_props
                && new_val.is_string()
                && let Some(sliced) = sliced_props
            {
                set_string_text_properties_table_for_value(new_val, sliced);
            }

            Ok(new_val)
        }
        ValueKind::Veclike(VecLikeType::Vector) if name == "substring" => {
            let items = args[0].as_vector_data().unwrap().clone();
            let len = items.len() as i64;
            let normalize_index = |value: &Value, default: i64| -> Result<i64, Flow> {
                let raw = if value.is_nil() {
                    default
                } else {
                    expect_int(value)?
                };
                let idx = if raw < 0 { len + raw } else { raw };
                if idx < 0 || idx > len {
                    return Err(signal(
                        LispCondition::ArgsOutOfRange,
                        vec![args[0], args[1], args.get(2).cloned().unwrap_or(Value::NIL)],
                    ));
                }
                Ok(idx)
            };
            let from = if args.len() > 1 {
                normalize_index(&args[1], 0)?
            } else {
                0
            } as usize;
            let to = if args.len() > 2 {
                normalize_index(&args[2], len)?
            } else {
                len
            } as usize;
            if from > to {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![
                        args[0],
                        args.get(1).cloned().unwrap_or(Value::fixnum(0)),
                        args.get(2).cloned().unwrap_or(Value::NIL),
                    ],
                ));
            }
            Ok(Value::vector(items[from..to].to_vec()))
        }
        _ => {
            if name == "substring" {
                Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("arrayp"), args[0]],
                ))
            } else {
                let _ = expect_lisp_string(&args[0])?;
                unreachable!("expect_lisp_string either returns a string or signals")
            }
        }
    }
}

fn concatenated_string_text_properties(
    string_sources: &[(Value, usize)],
) -> Option<TextPropertyTable> {
    let sources = string_sources
        .iter()
        .filter_map(|(source, offset)| {
            get_string_text_properties_table_for_value(*source)
                .map(|table| ShiftedTextPropertySource::new(table, CharLen::new(*offset)))
        })
        .collect::<Vec<_>>();
    (!sources.is_empty()).then(|| {
        TextPropertyTable::from_shifted_sources(sources, PropertyPlistApplication::AddProperties)
    })
}

fn concat_character_makes_multibyte(value: Value) -> Result<bool, Flow> {
    let Some(c) = value.as_fixnum().filter(|c| (0..=0x3FFFFF).contains(c)) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), value],
        ));
    };
    Ok(c >= 0x80 && !emacs_char::char_byte8_p(c as u32))
}

fn concat_arg_makes_multibyte(value: Value) -> Result<bool, Flow> {
    match value.kind() {
        ValueKind::String => Ok(value
            .as_lisp_string()
            .is_some_and(|string| string.is_multibyte())),
        ValueKind::Veclike(VecLikeType::Vector) => {
            let mut multibyte = false;
            if let Some(items) = value.as_vector_data() {
                for item in items.iter().copied() {
                    multibyte |= concat_character_makes_multibyte(item)?;
                }
            }
            Ok(multibyte)
        }
        ValueKind::Cons => {
            // GNU fns.c:885 validates this list's complete spine
            // before inspecting its characters. Invalid earlier
            // arguments still take precedence over later cycles.
            proper_list_length_or_signal(value)?;
            let mut multibyte = false;
            let mut cursor = value;
            while cursor.is_cons() {
                multibyte |= concat_character_makes_multibyte(cursor.cons_car())?;
                cursor = cursor.cons_cdr();
            }
            Ok(multibyte)
        }
        ValueKind::Nil => Ok(false),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), value],
        )),
    }
}

fn append_lisp_string_bytes(
    result: &mut Vec<u8>,
    string: &crate::heap_types::LispString,
    dest_multibyte: bool,
) {
    if dest_multibyte && !string.is_multibyte() {
        result.extend_from_slice(&emacs_char::str_to_multibyte(string.as_bytes()));
    } else {
        result.extend_from_slice(string.as_bytes());
    }
}

fn push_concat_int(result: &mut Vec<u8>, n: i64, dest_multibyte: bool) -> Result<(), Flow> {
    if !(0..=0x3FFFFF).contains(&n) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), Value::fixnum(n)],
        ));
    }

    let cp = n as u32;
    if dest_multibyte {
        let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = emacs_char::char_string(cp, &mut buf);
        result.extend_from_slice(&buf[..len]);
    } else if let Some(byte) = emacs_char::char_to_byte_safe(cp) {
        result.push(byte);
    } else {
        let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = emacs_char::char_string(cp, &mut buf);
        result.extend_from_slice(&buf[..len]);
    }
    Ok(())
}

fn push_concat_element(
    result: &mut Vec<u8>,
    value: &Value,
    dest_multibyte: bool,
) -> Result<usize, Flow> {
    match value.kind() {
        ValueKind::Fixnum(c) => {
            push_concat_int(result, c, dest_multibyte)?;
            Ok(1)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), *value],
        )),
    }
}

// GNU fns.c:853,903,934 retains SCHARS separately from copied bytes.
fn supplied_concat_count(
    chars: usize,
) -> Result<crate::heap_types::GnuDeclaredCharacterCount, Flow> {
    if chars > crate::tagged::value::TaggedValue::MOST_POSITIVE_FIXNUM as usize {
        return Err(concat_count_overflow());
    }
    crate::heap_types::GnuDeclaredCharacterCount::try_from(chars as i128).map_err(|error| {
        match error {
            crate::heap_types::GnuDeclaredCharacterCountError::Negative
            | crate::heap_types::GnuDeclaredCharacterCountError::TooLarge => {
                concat_count_overflow()
            }
        }
    })
}
fn concat_count_overflow() -> Flow {
    signal(
        LispCondition::Error,
        vec![Value::string(
            "Memory exhausted--use M-x save-some-buffers then exit and restart Emacs",
        )],
    )
}
fn checked_concat_chars(total: usize, more: usize) -> Result<usize, Flow> {
    let sum = total.checked_add(more).ok_or_else(concat_count_overflow)?;
    if sum > crate::tagged::value::TaggedValue::MOST_POSITIVE_FIXNUM as usize {
        return Err(concat_count_overflow());
    }
    Ok(sum)
}
fn concat_multibyte_result(
    bytes: Vec<u8>,
    chars: usize,
    kind: crate::heap_types::StringCharacterCountKind,
) -> Result<crate::heap_types::LispString, Flow> {
    Ok(match kind {
        crate::heap_types::StringCharacterCountKind::Computed => {
            crate::heap_types::LispString::from_emacs_bytes_with_chars(bytes, chars)
        }
        crate::heap_types::StringCharacterCountKind::Supplied => {
            crate::heap_types::LispString::from_emacs_bytes_with_declared_chars(
                bytes,
                supplied_concat_count(chars)?,
            )
        }
    })
}

/// `concat`'s core, probe-free: the body of the subr wrapper's
/// `perf_trace` closure, with its helper functions hoisted to module
/// level. `#[inline]` keeps the probe frame flat across the crate
/// boundary.
#[doc(hidden)]
#[inline]
pub fn concat_impl(args: &[Value]) -> EvalResult {
    if args.iter().all(|arg| arg.is_string()) {
        let dest_multibyte = args.iter().any(|arg| {
            arg.as_lisp_string()
                .is_some_and(|string| string.is_multibyte())
        });
        let mut combined = Vec::new();
        let mut string_sources: Vec<(Value, usize)> = Vec::new();
        let mut result_chars = 0usize;
        let mut count_kind = crate::heap_types::StringCharacterCountKind::Computed;
        for arg in args {
            if let Some(string) = arg.as_lisp_string() {
                string_sources.push((*arg, result_chars));
                result_chars = checked_concat_chars(result_chars, string.schars())?;
                if string.character_count_kind()
                    == crate::heap_types::StringCharacterCountKind::Supplied
                {
                    count_kind = crate::heap_types::StringCharacterCountKind::Supplied;
                }
                append_lisp_string_bytes(&mut combined, string, dest_multibyte);
            }
        }

        let new_val = Value::heap_string(if dest_multibyte {
            concat_multibyte_result(combined, result_chars, count_kind)?
        } else {
            crate::heap_types::LispString::from_unibyte(combined)
        });

        if let Some(combined_table) = concatenated_string_text_properties(&string_sources) {
            set_string_text_properties_table_for_value(new_val, combined_table);
        }
        return Ok(new_val);
    }

    let mut dest_multibyte = false;
    for arg in args {
        dest_multibyte |= concat_arg_makes_multibyte(*arg)?;
    }

    let preallocated_len = args.iter().fold(0usize, |acc, arg| match arg.kind() {
        ValueKind::String => acc + arg.as_lisp_string().map(|ls| ls.sbytes()).unwrap_or(0),
        _ => acc,
    });
    let mut result: Vec<u8> = Vec::with_capacity(preallocated_len);
    let mut result_chars = 0usize;
    let mut count_kind = crate::heap_types::StringCharacterCountKind::Computed;
    // Track string sources with their character offsets for property preservation.
    let mut string_sources: Vec<(Value, usize)> = Vec::new();

    for arg in args {
        match arg.kind() {
            ValueKind::String => {
                let offset = result_chars;
                if let Some(ls) = arg.as_lisp_string() {
                    append_lisp_string_bytes(&mut result, ls, dest_multibyte);
                    result_chars = checked_concat_chars(result_chars, ls.schars())?;
                    if ls.character_count_kind()
                        == crate::heap_types::StringCharacterCountKind::Supplied
                    {
                        count_kind = crate::heap_types::StringCharacterCountKind::Supplied;
                    }
                }
                string_sources.push((*arg, offset));
            }
            ValueKind::Nil => {}
            ValueKind::Cons => {
                let mut cursor = *arg;
                loop {
                    match cursor.kind() {
                        ValueKind::Nil => break,
                        ValueKind::Cons => {
                            let pair_car = cursor.cons_car();
                            let pair_cdr = cursor.cons_cdr();
                            result_chars = checked_concat_chars(
                                result_chars,
                                push_concat_element(&mut result, &pair_car, dest_multibyte)?,
                            )?;
                            cursor = pair_cdr;
                        }
                        _tail => {
                            return Err(signal(
                                LispCondition::WrongTypeArgument,
                                vec![Value::symbol("listp"), cursor],
                            ));
                        }
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::Vector) => {
                let items = arg.as_vector_data().unwrap().clone();
                for item in items.iter() {
                    result_chars = checked_concat_chars(
                        result_chars,
                        push_concat_element(&mut result, item, dest_multibyte)?,
                    )?;
                }
            }
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("sequencep"), *arg],
                ));
            }
        }
    }

    let new_val = Value::heap_string(if dest_multibyte {
        concat_multibyte_result(result, result_chars, count_kind)?
    } else {
        crate::heap_types::LispString::from_unibyte(result)
    });

    // Preserve text properties from string sources
    if new_val.is_string()
        && let Some(combined_table) = concatenated_string_text_properties(&string_sources)
    {
        set_string_text_properties_table_for_value(new_val, combined_table);
    }

    Ok(new_val)
}

// ---------------------------------------------------------------------------
// Unibyte/multibyte conversion subrs (C12, moved from lisp/native/misc)
// ---------------------------------------------------------------------------

/// Convert unibyte LispString bytes to multibyte Emacs encoding.
fn convert_unibyte_to_multibyte_bytes(src: &[u8]) -> Vec<u8> {
    emacs_char::str_to_multibyte(src)
}

/// Reinterpret unibyte bytes as an Emacs multibyte sequence.
///
/// Valid non-eight-bit sequences are preserved. Each byte of a rejected
/// sequence, including a `C0`/`C1` raw-byte pair, becomes an eight-bit
/// character (`str_as_multibyte`, GNU `character.c:586`).
fn reinterpret_unibyte_as_multibyte_bytes(src: &[u8]) -> Vec<u8> {
    emacs_char::str_as_multibyte(src)
}

/// `(string-to-multibyte STRING)` -- convert unibyte storage bytes to multibyte chars.
#[doc(hidden)]
pub fn builtin_string_to_multibyte(args: Vec<Value>) -> EvalResult {
    expect_args("string-to-multibyte", &args, 1)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    if ls.is_multibyte() {
        return Ok(args[0]);
    }
    let out = convert_unibyte_to_multibyte_bytes(ls.as_bytes());
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_emacs_bytes(out),
    ))
}

/// `(string-as-unibyte STRING)` -- reinterpret as unibyte byte sequence.
#[doc(hidden)]
pub fn builtin_string_as_unibyte(args: Vec<Value>) -> EvalResult {
    expect_args("string-as-unibyte", &args, 1)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    if !ls.is_multibyte() {
        return Ok(args[0]);
    }
    // Reinterpret: raw-byte chars become their byte value, other chars keep
    // their UTF-8 encoding as raw bytes.
    let src = ls.as_bytes();
    let mut bytes = Vec::with_capacity(src.len());
    let mut pos = 0;
    while pos < src.len() {
        let (cp, len) = emacs_char::string_char(&src[pos..]);
        if emacs_char::char_byte8_p(cp.code()) {
            bytes.push(emacs_char::char_to_byte8(cp.code()));
        } else {
            // Keep the raw encoding bytes
            bytes.extend_from_slice(&src[pos..pos + len]);
        }
        pos += len;
    }
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_unibyte(bytes),
    ))
}

/// `(string-as-multibyte STRING)` -- reinterpret unibyte storage as multibyte.
#[doc(hidden)]
pub fn builtin_string_as_multibyte(args: Vec<Value>) -> EvalResult {
    expect_args("string-as-multibyte", &args, 1)?;
    let ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    if ls.is_multibyte() {
        return Ok(args[0]);
    }
    let out = reinterpret_unibyte_as_multibyte_bytes(ls.as_bytes());
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_emacs_bytes(out),
    ))
}
