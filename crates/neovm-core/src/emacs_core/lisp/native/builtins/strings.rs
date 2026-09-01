use super::*;
use crate::buffer::text_props::{
    PropertyPlistApplication, ShiftedTextPropertySource, TextPropertyTable,
};
use crate::buffer::{CharLen, CharPos0, CharRange};
use crate::emacs_core::coding::TextQuotingStyle;
use crate::emacs_core::error::{expect_args, expect_fixnum, expect_max_args, expect_min_args};
use crate::emacs_core::value::{
    ValueKind, VecLikeType, get_string_text_properties_table_for_value,
    set_string_text_properties_table_for_value,
};
use malachite::base::num::arithmetic::traits::Abs;
use malachite::base::num::basic::traits::Zero;
use malachite::base::num::conversion::traits::{FromStringBase, RoundingFrom, ToStringBase};
use malachite::base::rounding_modes::RoundingMode;
use malachite::integer::Integer;
use native_regex::Regex as NativeRegex;
use std::sync::LazyLock;

// ===========================================================================
// String operations
// ===========================================================================

fn string_char_range(start: usize, end: usize) -> CharRange {
    CharRange::from_start_len(
        CharPos0::new(start),
        CharLen::new(end.saturating_sub(start)),
    )
}

typed_subr! {
    pub(crate) fn builtin_string_equal_2(_eval, a: StringDesignator, b: StringDesignator) -> EvalResult {
        string_equal_designators(a.text(), b.text())
    }
}

/// The two storage modes GNU's `string_cmp` can compare.
///
/// `OneByteChars` covers both unibyte strings and ASCII-only multibyte
/// strings (`SCHARS == SBYTES`): in either case each stored byte is the
/// character code.  A genuinely multibyte string must be decoded so Emacs raw
/// eight-bit characters retain their 0x3FFF00+ ordering.  Keeping that choice
/// in an enum makes all four encoding pairs explicit and prevents an owned
/// character-code buffer from entering this hot path.
enum StringOrderingView<'a> {
    OneByteChars(&'a [u8]),
    MultibyteChars(EmacsMultibyteChars<'a>),
}

impl<'a> StringOrderingView<'a> {
    fn new(value: &'a crate::heap_types::LispString) -> Self {
        if !value.is_multibyte() || value.schars() == value.sbytes() {
            Self::OneByteChars(value.as_bytes())
        } else {
            Self::MultibyteChars(EmacsMultibyteChars::new(value.as_bytes()))
        }
    }
}

struct EmacsMultibyteChars<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> EmacsMultibyteChars<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
}

impl Iterator for EmacsMultibyteChars<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position == self.bytes.len() {
            return None;
        }
        let (code, length) =
            crate::emacs_core::emacs_char::string_char(&self.bytes[self.position..]);
        self.position += length.max(1);
        Some(code)
    }
}

fn string_ordering(
    left: &crate::heap_types::LispString,
    right: &crate::heap_types::LispString,
) -> std::cmp::Ordering {
    match (
        StringOrderingView::new(left),
        StringOrderingView::new(right),
    ) {
        (StringOrderingView::OneByteChars(left), StringOrderingView::OneByteChars(right)) => {
            left.cmp(right)
        }
        (StringOrderingView::OneByteChars(left), StringOrderingView::MultibyteChars(right)) => {
            left.iter().copied().map(u32::from).cmp(right)
        }
        (StringOrderingView::MultibyteChars(left), StringOrderingView::OneByteChars(right)) => {
            left.cmp(right.iter().copied().map(u32::from))
        }
        (StringOrderingView::MultibyteChars(left), StringOrderingView::MultibyteChars(right)) => {
            left.cmp(right)
        }
    }
}

fn string_equal_designators(
    a: &crate::heap_types::LispString,
    b: &crate::heap_types::LispString,
) -> EvalResult {
    // GNU `Fstring_equal` compares SCHARS, SBYTES, then `memcmp` of the raw
    // internal-form bytes. It deliberately does NOT decode to char codes: a raw
    // unibyte byte >=128 occupies one byte, while the same-numbered multibyte
    // (eight-bit or real) char occupies two bytes, so their byte lengths differ
    // and they compare unequal. Pure-ASCII unibyte/multibyte strings share an
    // identical byte representation and stay equal, matching GNU. Multibyteness
    // itself is not compared (GNU does not), only the byte counts and bytes.
    Ok(Value::bool_val(
        a.schars() == b.schars() && a.sbytes() == b.sbytes() && a.as_bytes() == b.as_bytes(),
    ))
}

typed_subr! {
    pub(crate) fn builtin_string_lessp_2(_eval, a: StringDesignator, b: StringDesignator) -> EvalResult {
        Ok(Value::bool_val(string_ordering(a.text(), b.text()).is_lt()))
    }
}

fn substring_impl(name: &str, args: &[Value], preserve_props: bool) -> EvalResult {
    expect_min_args(name, args, 1)?;
    expect_max_args(name, args, 3)?;
    match args[0].kind() {
        ValueKind::String => {
            let src_props = if preserve_props {
                get_string_text_properties_table_for_value(args[0])
                    .filter(|table| !table.is_empty())
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
                    let bf = src.char_to_byte_pos(from);
                    let bt = src.char_to_byte_pos(to);
                    (bf, bt)
                } else {
                    (from, to)
                };
                if byte_to > src_bytes.len() {
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
                let sliced_props = if let Some(src_table) = src_props.as_ref() {
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
        ValueKind::Veclike(VecLikeType::Vector)
            if name == "substring"
                && !super::chartable::is_char_table(&args[0])
                && !super::chartable::is_bool_vector(&args[0]) =>
        {
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

#[cfg(test)]
#[path = "tests/strings.rs"]
mod tests;

pub(crate) fn builtin_substring(args: Vec<Value>) -> EvalResult {
    builtin_substring_slice(&args)
}

pub(crate) fn builtin_substring_slice(args: &[Value]) -> EvalResult {
    crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::Substring,
        || substring_impl("substring", args, true),
    )
}

pub(crate) fn builtin_substring_no_properties(args: Vec<Value>) -> EvalResult {
    crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::Substring,
        || substring_impl("substring-no-properties", &args, false),
    )
}

pub(crate) fn builtin_concat(args: Vec<Value>) -> EvalResult {
    builtin_concat_slice(&args)
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

pub(crate) fn builtin_concat_slice(args: &[Value]) -> EvalResult {
    crate::emacs_core::perf_trace::time_op(crate::emacs_core::perf_trace::HotpathOp::Concat, || {
        use crate::emacs_core::emacs_char;

        fn concat_arg_makes_multibyte(value: Value) -> bool {
            match value.kind() {
                ValueKind::String => value
                    .as_lisp_string()
                    .is_some_and(|string| string.is_multibyte()),
                ValueKind::Veclike(VecLikeType::Vector)
                    if !super::chartable::is_bool_vector(&value)
                        && !super::chartable::is_char_table(&value) =>
                {
                    value
                        .as_vector_data()
                        .is_some_and(|items| items.iter().copied().any(concat_arg_makes_multibyte))
                }
                ValueKind::Cons => {
                    let mut cursor = value;
                    while cursor.is_cons() {
                        let car = cursor.cons_car();
                        if concat_arg_makes_multibyte(car) {
                            return true;
                        }
                        cursor = cursor.cons_cdr();
                    }
                    false
                }
                ValueKind::Fixnum(c) => {
                    c >= 0x80
                        && (c as u32) <= emacs_char::MAX_CHAR
                        && !emacs_char::char_byte8_p(c as u32)
                }
                _ => false,
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

        let dest_multibyte = args.iter().copied().any(concat_arg_makes_multibyte);
        if args.iter().all(|arg| arg.is_string()) {
            let mut combined = Vec::new();
            let mut string_sources: Vec<(Value, usize)> = Vec::new();
            let mut result_chars = 0usize;
            for arg in args {
                if let Some(string) = arg.as_lisp_string() {
                    string_sources.push((*arg, result_chars));
                    result_chars += string.schars();
                    append_lisp_string_bytes(&mut combined, string, dest_multibyte);
                }
            }

            let new_val = Value::heap_string(if dest_multibyte {
                crate::heap_types::LispString::from_emacs_bytes(combined)
            } else {
                crate::heap_types::LispString::from_unibyte(combined)
            });

            if let Some(combined_table) = concatenated_string_text_properties(&string_sources) {
                set_string_text_properties_table_for_value(new_val, combined_table);
            }
            return Ok(new_val);
        }

        let preallocated_len = args.iter().fold(0usize, |acc, arg| match arg.kind() {
            ValueKind::String => acc + arg.as_lisp_string().map(|ls| ls.sbytes()).unwrap_or(0),
            _ => acc,
        });
        let mut result: Vec<u8> = Vec::with_capacity(preallocated_len);
        let mut result_chars = 0usize;
        // Track string sources with their character offsets for property preservation.
        let mut string_sources: Vec<(Value, usize)> = Vec::new();

        for arg in args {
            match arg.kind() {
                ValueKind::String => {
                    let offset = result_chars;
                    if let Some(ls) = arg.as_lisp_string() {
                        append_lisp_string_bytes(&mut result, ls, dest_multibyte);
                        result_chars += ls.schars();
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
                                result_chars +=
                                    push_concat_element(&mut result, &pair_car, dest_multibyte)?;
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
                ValueKind::Veclike(VecLikeType::Vector)
                    if !super::chartable::is_bool_vector(arg)
                        && !super::chartable::is_char_table(arg) =>
                {
                    let items = arg.as_vector_data().unwrap().clone();
                    for item in items.iter() {
                        result_chars += push_concat_element(&mut result, item, dest_multibyte)?;
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
            crate::heap_types::LispString::from_emacs_bytes(result)
        } else {
            crate::heap_types::LispString::from_unibyte(result)
        });

        // Preserve text properties from string sources
        if new_val.is_string() {
            if let Some(combined_table) = concatenated_string_text_properties(&string_sources) {
                set_string_text_properties_table_for_value(new_val, combined_table);
            }
        }

        Ok(new_val)
    })
}

struct DecimalNumberPrefixRegexes {
    special_float: NativeRegex,
    number: NativeRegex,
}

/// The immutable parser machinery for GNU's base-10 numeric-prefix grammar.
///
/// GNU `string_to_number` (`src/lread.c`) scans this grammar directly.  Keep
/// the existing `regex` implementation, but compile its DFAs exactly once for
/// the process instead of once per Lisp call.  M-x completion calls
/// `string-to-number` repeatedly while filtering obsolete command versions,
/// so per-call construction turns an otherwise linear scan into repeated regex
/// compilation.
fn decimal_number_prefix_regexes() -> &'static DecimalNumberPrefixRegexes {
    static REGEXES: LazyLock<DecimalNumberPrefixRegexes> =
        LazyLock::new(|| DecimalNumberPrefixRegexes {
            special_float: NativeRegex::new(
                r"^[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)[eE]\+(?:INF|NaN)",
            )
            .expect("special float prefix regexp should compile"),
            number: NativeRegex::new(r"^[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?")
                .expect("number prefix regexp should compile"),
        });
    &REGEXES
}

pub(crate) fn builtin_string_to_number(
    _eval: &mut super::eval::Context,
    string: Value,
    base_arg: Value,
) -> EvalResult {
    let ls = expect_lisp_string(&string)?;
    let base = if base_arg.is_nil() {
        10
    } else {
        expect_fixnum(&base_arg)?
    };

    if !(2..=16).contains(&base) {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::fixnum(base)],
        ));
    }

    // Fast path: a plain base-10 integer prefix scans directly on the
    // string bytes (ASCII digits are the same bytes in either string
    // encoding) — GNU string_to_number is exactly such a scan. The
    // general path below pays a lossy UTF-8 copy plus two regex-engine
    // searches per call. Bail out whenever the token could be a float,
    // a special float, or an out-of-i64 magnitude.
    if base == 10 {
        let bytes = ls.as_bytes();
        let mut pos = 0usize;
        while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t') {
            pos += 1;
        }
        let mut negative = false;
        if pos < bytes.len() && matches!(bytes[pos], b'+' | b'-') {
            negative = bytes[pos] == b'-';
            pos += 1;
        }
        let digit_start = pos;
        let mut acc: i64 = 0;
        let mut overflow = false;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            let digit = (bytes[pos] - b'0') as i64;
            match acc
                .checked_mul(10)
                .and_then(|value| value.checked_add(digit))
            {
                Some(value) => acc = value,
                None => overflow = true,
            }
            pos += 1;
        }
        if pos > digit_start
            && !overflow
            && !(pos < bytes.len() && matches!(bytes[pos], b'.' | b'e' | b'E'))
        {
            return Ok(Value::make_int(if negative { -acc } else { acc }));
        }
    }

    let s = expect_string_lossy(&string)?;
    let s = s.trim_start_matches([' ', '\t']);
    if base == 10 {
        let regexes = decimal_number_prefix_regexes();
        if let Some(m) = regexes.special_float.find(s)
            && let Some(f) = crate::emacs_core::value_reader::parse_emacs_special_float(m.as_str())
        {
            return Ok(Value::make_float(f));
        }

        // Match GNU Emacs's string_to_number float detection rules:
        // A number is float if it has digits after the decimal point (TRAIL_INT)
        // OR if it has leading digits and an exponent (LEAD_INT & E_EXP).
        // "100." is integer (no trailing digits), "100.0" is float, "1e10" is float.
        if let Some(m) = regexes.number.find(s) {
            let token = m.as_str();
            // Emacs float_syntax: trail_int || (lead_int && e_exp)
            // trail_int = has digits after '.'
            // e_exp = has 'e'/'E' exponent
            let has_trail_int = if let Some(dot_pos) = token.find('.') {
                token[dot_pos + 1..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            } else {
                false
            };
            let has_e_exp = token.contains('e') || token.contains('E');
            let has_lead_int = token
                .trim_start_matches(['+', '-'])
                .starts_with(|c: char| c.is_ascii_digit());
            let is_float = has_trail_int || (has_lead_int && has_e_exp);
            if is_float {
                if let Ok(f) = token.parse::<f64>() {
                    return Ok(Value::make_float(f));
                }
            } else {
                // Parse integer part only (stop at dot if present).
                // Funnel through Value::make_integer so values that
                // exceed fixnum range come back as bignums (mirrors
                // GNU string_to_number which uses bignum_make).
                let int_token = if let Some(dot_pos) = token.find('.') {
                    &token[..dot_pos]
                } else {
                    token
                };
                if let Ok(n) = int_token.parse::<i64>() {
                    return Ok(Value::make_integer(Integer::from(n)));
                }
                if let Ok(parsed) = int_token.parse::<Integer>() {
                    return Ok(Value::make_integer(parsed));
                }
            }
        }
    } else {
        let bytes = s.as_bytes();
        let mut pos = 0usize;
        let mut negative = false;
        if pos < bytes.len() {
            if bytes[pos] == b'+' {
                pos += 1;
            } else if bytes[pos] == b'-' {
                negative = true;
                pos += 1;
            }
        }
        let digit_start = pos;
        while pos < bytes.len() {
            let ch = bytes[pos] as char;
            let Some(d) = ch.to_digit(36) else { break };
            if (d as i64) < base {
                pos += 1;
            } else {
                break;
            }
        }
        if pos > digit_start {
            let token = &s[digit_start..pos];
            if let Ok(parsed) = i64::from_str_radix(token, base as u32) {
                return Ok(Value::make_integer(Integer::from(if negative {
                    -parsed
                } else {
                    parsed
                })));
            }
            // Overflow — fall back to bignum for literal in given base.
            let mut signed = String::with_capacity(token.len() + 1);
            if negative {
                signed.push('-');
            }
            signed.push_str(token);
            if let Some(parsed) = Integer::from_string_base(base as u8, &signed) {
                return Ok(Value::make_integer(parsed));
            }
        }
    }
    Ok(Value::fixnum(0))
}

/// `(number-to-string NUMBER)` — mirrors GNU `Fnumber_to_string`
/// (`src/data.c`). Bignums format via `malachite::Integer`'s Display.
pub(crate) fn builtin_number_to_string(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("number-to-string", &args, 1)?;
    match args[0].kind() {
        ValueKind::Fixnum(n) => Ok(Value::string(n.to_string())),
        ValueKind::Float => Ok(Value::string(
            super::print::format_float_with_output_format(
                args[0].xfloat(),
                ctx.obarray
                    .symbol_value("float-output-format")
                    .filter(|v| v.is_string())
                    .copied(),
            ),
        )),
        ValueKind::Veclike(VecLikeType::Bignum) => {
            Ok(Value::string(args[0].as_bignum().unwrap().to_string()))
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("numberp"), args[0]],
        )),
    }
}

/// Dispatched form: honors the current buffer's case table (`set-case-table`).
pub(crate) fn builtin_upcase_in_state(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let casetab = super::super::casetab::CaseTableOverride::for_current_buffer(eval)?;
    upcase_with_override(args, casetab)
}

fn upcase_with_override(
    args: Vec<Value>,
    casetab: super::super::casetab::CaseTableOverride,
) -> EvalResult {
    expect_args("upcase", &args, 1)?;
    match args[0].kind() {
        ValueKind::String => {
            let source = args[0];
            let string = args[0].as_lisp_string().expect("string");
            let source_props = (!string.is_multibyte())
                .then(|| get_string_text_properties_table_for_value(source))
                .flatten();
            let result =
                Value::heap_string(transform_string_case(string, true, |_| false, &casetab));
            if let Some(table) = source_props {
                set_string_text_properties_table_for_value(result, table);
            }
            Ok(result)
        }
        ValueKind::Fixnum(c) if (0..=0x3F_FFFF).contains(&c) => {
            // GNU `upcase` consults the per-buffer upcase table first
            // (`buffer.h:1656-1663`); only fall through to the hardwired
            // Unicode mapping when the table has no explicit entry.
            let mapped = casetab
                .map(super::super::casetab::CaseMap::Up, c)
                .unwrap_or_else(|| upcase_char_code_emacs_compat(c));
            if let Some(ch) = u32::try_from(mapped).ok().and_then(char::from_u32) {
                Ok(Value::fixnum(ch as i64))
            } else {
                Ok(Value::fixnum(c))
            }
        }
        ValueKind::Fixnum(_) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("char-or-string-p"), args[0]],
        )),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("char-or-string-p"), args[0]],
        )),
    }
}

fn preserve_emacs_upcase_payload(code: i64) -> bool {
    matches!(
        code,
        305
            | 329
            | 383
            | 411
            | 496
            | 612
            | 912
            | 944
            | 1415
            | 7306
            | 7830..=7834
            | 8016
            | 8018
            | 8020
            | 8022
            | 8072..=8079
            | 8088..=8095
            | 8104..=8111
            | 8114
            | 8116
            | 8118..=8119
            | 8124
            | 8130
            | 8132
            | 8134..=8135
            | 8140
            | 8146..=8147
            | 8150..=8151
            | 8162..=8164
            | 8166..=8167
            | 8178
            | 8180
            | 8182..=8183
            | 8188
            | 42957
            | 42959
            | 42963
            | 42965
            | 42971
            | 64256..=64262
            | 64275..=64279
            | 68976..=68997
            | 93883..=93907
    )
}

/// Byte-faithful string case transform over Emacs char codes (issue #131):
/// eight-bit / non-Unicode chars are caseless and pass through unchanged; in a
/// unibyte string only ASCII is cased (raw high bytes pass through), matching the
/// retired storage-String path. Mirrors `upcase`/`downcase_string_emacs_compat`
/// (the `\u{0131}`/`\u{212A}` + payload-preserve specials, multi-char mapping).
fn transform_string_case(
    s: &crate::heap_types::LispString,
    upcase: bool,
    is_word: impl Fn(u32) -> bool,
    casetab: &super::super::casetab::CaseTableOverride,
) -> crate::heap_types::LispString {
    use super::super::casetab::CaseMap;
    // Greek capital sigma down-cases to the final form ς at the end of a word
    // (GNU `casefiddle.c` `case_character`): when the preceding character is a
    // word constituent and the following one is not.
    const GREEK_CAPITAL_SIGMA: u32 = 0x03A3;
    const GREEK_SMALL_SIGMA: u32 = 0x03C3;
    const GREEK_SMALL_FINAL_SIGMA: u32 = 0x03C2;

    let bytes = s.as_bytes();
    let multibyte = s.is_multibyte();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let push = |out: &mut Vec<u8>, code: u32| {
        if multibyte {
            let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
            let n = crate::emacs_core::emacs_char::char_string(code, &mut buf);
            out.extend_from_slice(&buf[..n]);
        } else {
            out.push(code as u8);
        }
    };
    let mut pos = 0;
    let mut prev_word = false;
    while pos < bytes.len() {
        let (code, len) = if multibyte {
            crate::emacs_core::emacs_char::string_char(&bytes[pos..])
        } else {
            (bytes[pos] as u32, 1)
        };
        pos += len;
        // GNU `case_character_impl` resolves each character through the
        // per-buffer up/down case table (`buffer.h` `downcase`/`upcase`) before
        // the Unicode special-casing. When a custom table is installed and has
        // an explicit entry for this character, use it and skip the hardwired
        // path; otherwise fall through unchanged (byte-identical default).
        if casetab.is_custom() {
            let which = if upcase { CaseMap::Up } else { CaseMap::Down };
            if let Some(mapped) = casetab.map(which, code as i64) {
                push(&mut out, mapped as u32);
                prev_word = is_word(code);
                continue;
            }
        }
        match char::from_u32(code).filter(|_| multibyte || code < 0x80) {
            Some(ch) if upcase => {
                if ch == '\u{0131}' || preserve_emacs_upcase_string_payload(code as i64) {
                    push(&mut out, code);
                } else {
                    for up in ch.to_uppercase() {
                        push(&mut out, up as u32);
                    }
                }
            }
            Some(_) if code == GREEK_CAPITAL_SIGMA => {
                let next_word = if pos < bytes.len() {
                    let (next_code, _) = if multibyte {
                        crate::emacs_core::emacs_char::string_char(&bytes[pos..])
                    } else {
                        (bytes[pos] as u32, 1)
                    };
                    is_word(next_code)
                } else {
                    false
                };
                push(
                    &mut out,
                    if prev_word && !next_word {
                        GREEK_SMALL_FINAL_SIGMA
                    } else {
                        GREEK_SMALL_SIGMA
                    },
                );
            }
            Some(ch) => {
                if ch == '\u{212A}' || preserve_emacs_downcase_string_payload(code as i64) {
                    push(&mut out, code);
                } else {
                    for low in ch.to_lowercase() {
                        push(&mut out, low as u32);
                    }
                }
            }
            None => push(&mut out, code),
        }
        prev_word = is_word(code);
    }
    if multibyte {
        crate::heap_types::LispString::from_emacs_bytes(out)
    } else {
        crate::heap_types::LispString::from_unibyte(out)
    }
}

pub(crate) fn upcase_char_code_emacs_compat(code: i64) -> i64 {
    if preserve_emacs_upcase_payload(code) {
        return code;
    }
    match code {
        223 => 7838,
        8064..=8071 | 8080..=8087 | 8096..=8103 => code + 8,
        8115 | 8131 | 8179 => code + 9,
        _ => {
            if let Some(c) = u32::try_from(code).ok().and_then(char::from_u32) {
                c.to_uppercase().next().unwrap_or(c) as i64
            } else {
                code
            }
        }
    }
}

fn preserve_emacs_upcase_string_payload(code: i64) -> bool {
    matches!(
        code,
        411
            | 612
            | 7306
            | 42957
            | 42959
            | 42963
            | 42965
            | 42971
            | 68976..=68997
            | 93883..=93907
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn runtime_string_result_multibyte(source_is_multibyte: bool, rendered: &str) -> bool {
    if source_is_multibyte {
        return true;
    }

    // GNU `styled_format` starts with the format/argument string
    // multibyteness, but retries with a multibyte result when `%S`/prin1 or
    // quoting produces multibyte text.  Neomacs runtime strings still use
    // private sentinels for explicit unibyte bytes; those sentinels must not
    // by themselves promote the result.  Ordinary non-ASCII Unicode text
    // (including Latin-1 codepoints like U+00E9) must promote, otherwise the
    // formatted Lisp string stores it as a unibyte byte and later displays the
    // U+E3xx sentinel.
    rendered.chars().any(|ch| {
        let code = ch as u32;
        code > 0x7F
            && !(0xE300..=0xE3FF).contains(&code)
            && !(0xE200..=0xE2FF).contains(&code)
            && !(0xE110..=0xE116).contains(&code)
            && code != 0xE100
    })
}

fn preserve_emacs_downcase_payload(code: i64) -> bool {
    matches!(
        code,
        304
            | 7305
            | 8490
            | 42955
            | 42956
            | 42958
            | 42962
            | 42964
            | 42970
            | 42972
            | 68944..=68965
            | 93856..=93880
    )
}

pub(crate) fn downcase_char_code_emacs_compat(code: i64) -> i64 {
    if preserve_emacs_downcase_payload(code) {
        return code;
    }
    if let Some(c) = u32::try_from(code).ok().and_then(char::from_u32) {
        c.to_lowercase().next().unwrap_or(c) as i64
    } else {
        code
    }
}

/// Build the casing word-constituent predicate used for the Greek final-sigma
/// rule — mirrors GNU `case_ch_is_word(SYNTAX(ch))`: a char is a word
/// constituent when its buffer syntax is `Sword` (or `Ssymbol` with
/// `case-symbols-as-words`), so a `set-case-syntax-pair` char participates too.
pub(crate) fn casing_word_predicate(
    eval: &crate::emacs_core::eval::Context,
) -> impl Fn(u32) -> bool + Copy + 'static {
    crate::emacs_core::syntax::casing_word_predicate(eval)
}

fn downcase_with_word_pred(
    args: Vec<Value>,
    is_word: impl Fn(u32) -> bool,
    casetab: super::super::casetab::CaseTableOverride,
) -> EvalResult {
    expect_args("downcase", &args, 1)?;
    match args[0].kind() {
        ValueKind::String => {
            let source = args[0];
            let string = args[0].as_lisp_string().expect("string");
            let source_props = (!string.is_multibyte())
                .then(|| get_string_text_properties_table_for_value(source))
                .flatten();
            let result =
                Value::heap_string(transform_string_case(string, false, is_word, &casetab));
            if let Some(table) = source_props {
                set_string_text_properties_table_for_value(result, table);
            }
            Ok(result)
        }
        ValueKind::Fixnum(c) if (0..=0x3F_FFFF).contains(&c) => {
            // GNU `downcase` consults the per-buffer downcase table first
            // (`buffer.h:1648-1655`); fall through to the hardwired Unicode
            // mapping only when there is no explicit entry.
            let mapped = casetab
                .map(super::super::casetab::CaseMap::Down, c)
                .unwrap_or_else(|| downcase_char_code_emacs_compat(c));
            if let Some(ch) = u32::try_from(mapped).ok().and_then(char::from_u32) {
                Ok(Value::fixnum(ch as i64))
            } else {
                Ok(Value::fixnum(c))
            }
        }
        ValueKind::Fixnum(_) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("char-or-string-p"), args[0]],
        )),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("char-or-string-p"), args[0]],
        )),
    }
}

/// Pure form (tests + internal callers such as font-name normalization, where
/// the Greek final-sigma word context does not apply); ignores final sigma.
pub(crate) fn builtin_downcase(args: Vec<Value>) -> EvalResult {
    downcase_with_word_pred(
        args,
        |_| false,
        super::super::casetab::CaseTableOverride::none(),
    )
}

/// Dispatched form: applies the Greek final-sigma rule via the buffer syntax
/// table (honoring `case-symbols-as-words`) and the current case table.
pub(crate) fn builtin_downcase_in_state(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let is_word = casing_word_predicate(eval);
    let casetab = super::super::casetab::CaseTableOverride::for_current_buffer(eval)?;
    downcase_with_word_pred(args, is_word, casetab)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn downcase_string_emacs_compat(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let code = ch as i64;
        if ch == '\u{212A}' || preserve_emacs_downcase_string_payload(code) {
            out.push(ch);
            continue;
        }
        for low in ch.to_lowercase() {
            out.push(low);
        }
    }
    out
}

fn preserve_emacs_downcase_string_payload(code: i64) -> bool {
    matches!(
        code,
        7305
            | 42955
            | 42956
            | 42958
            | 42962
            | 42964
            | 42970
            | 42972
            | 68944..=68965
            | 93856..=93880
    )
}

pub(crate) fn builtin_ngettext(args: Vec<Value>) -> EvalResult {
    expect_args("ngettext", &args, 3)?;
    let singular = expect_string_lossy(&args[0])?;
    let plural = expect_string_lossy(&args[1])?;
    let count = expect_int(&args[2])?;
    if count == 1 {
        Ok(Value::string(singular))
    } else {
        Ok(Value::string(plural))
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_format(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    // With specbind, dynamic let-bindings are written directly to the obarray,
    // so print_options_from_state correctly resolves print-* variables.
    builtin_format_slice(eval, &args)
}

pub(crate) fn builtin_format_slice(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    // With specbind, dynamic let-bindings are written directly to the obarray,
    // so print_options_from_state correctly resolves print-* variables.
    builtin_format_wrapper_strict_slice(eval, args)
}

fn format_percent_s_in_state(ctx: &crate::emacs_core::eval::Context, value: &Value) -> Vec<u8> {
    super::misc_eval::print_value_princ_bytes(ctx, value)
}

fn format_not_enough_args_error() -> Flow {
    signal(
        "error",
        vec![Value::string("Not enough arguments for format string")],
    )
}

fn format_spec_type_mismatch_error() -> Flow {
    signal(
        "error",
        vec![Value::string(
            "Format specifier doesn’t match argument type",
        )],
    )
}

struct FormatCharArgument {
    rendered: Vec<u8>,
    force_multibyte_result: bool,
}

fn format_char_argument(n: i64) -> Result<FormatCharArgument, Flow> {
    if !(0..=KEY_CHAR_CODE_MASK).contains(&n) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), Value::fixnum(n)],
        ));
    }

    // Issue #131: emit canonical Emacs internal-encoding bytes directly via
    // EmacsChar (eight-bit / non-Unicode as their disjoint extended sequence),
    // so a real Private-Use glyph passed to %c survives as itself. Returns the
    // characterp error for codes outside 0..=MAX_CHAR.
    let rendered = u32::try_from(n)
        .ok()
        .and_then(crate::emacs_core::emacs_char::EmacsChar::from_code)
        .map(|c| c.to_emacs_bytes())
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), Value::fixnum(n)],
            )
        })?;
    Ok(FormatCharArgument {
        rendered,
        // GNU styled_format retries with a multibyte result when %c receives
        // any non-ASCII character, then handles it through Fchar_to_string.
        force_multibyte_result: n > 0x7f,
    })
}

/// Parsed format specification: %[flags][width][.precision]conversion
#[derive(Clone, Debug)]
struct FormatSpec {
    field_number: Option<usize>,
    minus: bool,
    plus: bool,
    space: bool,
    zero: bool,
    sharp: bool,
    width: Option<usize>,
    precision: Option<usize>,
    conversion: char,
}

#[derive(Clone, Debug)]
struct ParsedFormatSpec {
    spec: FormatSpec,
    consumed_chars: usize,
}

fn format_string_overflow_error() -> Flow {
    signal("error", vec![Value::string("Maximum string size exceeded")])
}

/// Parse a format spec from the format string's Emacs character codes, with
/// `*pos` positioned just after '%'. Advances `*pos` past the spec and returns
/// the parsed spec plus the number of characters consumed after '%'. All format
/// spec syntax is ASCII, so codes are compared directly; a non-Unicode
/// conversion code becomes U+FFFD, which falls through to the invalid-operation
/// error (issue #131: the format string is iterated as Emacs characters, not as
/// a storage string).
/// Parse the spec after a `%`, on the format string's canonical multibyte
/// BYTES. Every character the spec grammar consumes before the conversion
/// (digits, `$`, flags, `.`) is ASCII — one byte per char in either
/// representation — so `pos` advances in bytes and `consumed_chars` stays
/// byte-accurate up to the conversion character, which alone may be
/// multibyte (decoded whole for the error message, counted as one char).
fn parse_format_spec(bytes: &[u8], pos: &mut usize) -> Result<ParsedFormatSpec, Flow> {
    let start = *pos;
    let mut spec = FormatSpec {
        field_number: None,
        minus: false,
        plus: false,
        space: false,
        zero: false,
        sharp: false,
        width: None,
        precision: None,
        conversion: '\0',
    };

    // Digit-run accumulator; `None` on usize overflow (the caller-visible
    // behavior of the old String `.parse()` failing).
    fn digits(bytes: &[u8], pos: &mut usize) -> (bool, Option<usize>) {
        let mut any = false;
        let mut value: Option<usize> = Some(0);
        while let Some(&byte) = bytes.get(*pos) {
            if !byte.is_ascii_digit() {
                break;
            }
            any = true;
            value = value
                .and_then(|v| v.checked_mul(10))
                .and_then(|v| v.checked_add((byte - b'0') as usize));
            *pos += 1;
        }
        (any, value)
    }

    // GNU `styled_format` first looks for [0-9]+ followed by a literal
    // '$'.  If there is no '$', the digits are left for the flag/width
    // parser, so `%05d` still means zero-padded width 5 rather than
    // field 5.
    let mut look = *pos;
    let (has_field_digits, field_value) = digits(bytes, &mut look);
    if has_field_digits && bytes.get(look) == Some(&b'$') {
        *pos = look + 1;
        spec.field_number = field_value;
    }

    // Parse flags
    while let Some(&byte) = bytes.get(*pos) {
        match byte {
            b'-' => spec.minus = true,
            b'+' => spec.plus = true,
            b' ' => spec.space = true,
            b'0' => spec.zero = true,
            b'#' => spec.sharp = true,
            _ => break,
        }
        *pos += 1;
    }

    // Ignore flags when sprintf ignores them
    if spec.plus {
        spec.space = false;
    }
    if spec.minus {
        spec.zero = false;
    }

    // Parse width
    let (has_width, width_value) = digits(bytes, pos);
    if has_width {
        spec.width = Some(width_value.ok_or_else(format_string_overflow_error)?);
    }

    // Parse precision
    if bytes.get(*pos) == Some(&b'.') {
        *pos += 1;
        let (has_prec, prec_value) = digits(bytes, pos);
        spec.precision = Some(if has_prec {
            prec_value.ok_or_else(format_string_overflow_error)?
        } else {
            0
        });
    }

    // Parse conversion character (the one spec position that may be a
    // multibyte char — decode it whole).
    if *pos >= bytes.len() {
        return Err(signal(
            "error",
            vec![Value::string(
                "Format string ends in middle of format specifier",
            )],
        ));
    }
    let ascii_consumed = *pos - start;
    let (conversion_code, conversion_len) =
        crate::emacs_core::emacs_char::string_char(&bytes[*pos..]);
    *pos += conversion_len;
    spec.conversion = char::from_u32(conversion_code).unwrap_or('\u{FFFD}');
    Ok(ParsedFormatSpec {
        spec,
        consumed_chars: ascii_consumed + 1,
    })
}

/// Apply width/alignment padding to a formatted string.
fn apply_width(s: &str, spec: &FormatSpec) -> String {
    let w = match spec.width {
        Some(w) if w > s.chars().count() => w,
        _ => return s.to_string(),
    };
    let _pad_char = if spec.zero && !spec.minus { '0' } else { ' ' };
    if spec.minus {
        format!("{:<width$}", s, width = w)
    } else if spec.zero && !spec.minus {
        // For zero-padding, handle negative numbers specially
        if let Some(unsigned) = s.strip_prefix('-') {
            format!("-{unsigned:0>width$}", width = w - 1)
        } else if let Some(unsigned) = s.strip_prefix('+') {
            format!("+{unsigned:0>width$}", width = w - 1)
        } else {
            format!("{:0>width$}", s, width = w)
        }
    } else {
        format!("{:>width$}", s, width = w)
    }
}

fn apply_integer_width(sign: &str, prefix: &str, digits: &str, spec: &FormatSpec) -> String {
    let content_len = sign.len() + prefix.len() + digits.len();
    let width = spec.width.unwrap_or(0);
    if width <= content_len {
        return format!("{sign}{prefix}{digits}");
    }

    let padding = width - content_len;
    if spec.minus {
        return format!("{sign}{prefix}{digits}{}", " ".repeat(padding));
    }

    if spec.zero && spec.precision.is_none() {
        return format!("{sign}{prefix}{}{digits}", "0".repeat(padding));
    }

    format!("{}{sign}{prefix}{digits}", " ".repeat(padding))
}

fn format_integer_digits(
    mut digits: String,
    negative: bool,
    zero_value: bool,
    spec: &FormatSpec,
) -> String {
    if spec.precision == Some(0) && zero_value {
        digits.clear();
    }

    let sign = if negative {
        "-"
    } else if spec.plus {
        "+"
    } else if spec.space {
        " "
    } else {
        ""
    };

    let prefix = match spec.conversion {
        'b' if spec.sharp && !zero_value => "0b",
        'B' if spec.sharp && !zero_value => "0B",
        'x' if spec.sharp && !zero_value => "0x",
        'X' if spec.sharp && !zero_value => "0X",
        _ => "",
    };

    if spec.conversion == 'o' && spec.sharp && digits.is_empty() {
        digits.push('0');
    }

    if let Some(precision) = spec.precision {
        if spec.conversion == 'o' && spec.sharp && !digits.starts_with('0') {
            let desired = precision.max(digits.len() + 1);
            if desired > digits.len() {
                digits = format!("{}{digits}", "0".repeat(desired - digits.len()));
            }
        } else if digits.len() < precision {
            digits = format!("{}{digits}", "0".repeat(precision - digits.len()));
        }
    } else if spec.conversion == 'o' && spec.sharp && !digits.starts_with('0') {
        digits = format!("0{digits}");
    }

    apply_integer_width(sign, prefix, &digits, spec)
}

/// Render `n` as plain decimal digits appended to `out`, with no heap
/// traffic: one backward stack-buffer pass, like GNU's sprintf into
/// `sprintf_buf`.
fn push_i64_decimal(out: &mut Vec<u8>, n: i64) {
    // i64::MIN-safe: widen before unsigned_abs; the magnitude fits u64.
    let mut val = (n as i128).unsigned_abs() as u64;
    let mut buf = [0u8; 21];
    let mut pos = buf.len();
    loop {
        pos -= 1;
        buf[pos] = b'0' + (val % 10) as u8;
        val /= 10;
        if val == 0 {
            break;
        }
    }
    if n < 0 {
        pos -= 1;
        buf[pos] = b'-';
    }
    out.extend_from_slice(&buf[pos..]);
}

/// Format an integer with the given spec.
fn format_int_spec(n: i64, spec: &FormatSpec) -> String {
    // Fast path for a plain `%d`/`%i`: one backward stack-buffer digit
    // pass and one allocation. The general path below runs 128-bit
    // `core::fmt` digit rendering plus two more String-building passes
    // (`format_integer_digits` → `apply_integer_width`) — several hundred
    // Ir per conversion, all avoidable when no flag/width/precision is set.
    if matches!(spec.conversion, 'd' | 'i')
        && spec.width.is_none()
        && spec.precision.is_none()
        && !spec.minus
        && !spec.plus
        && !spec.space
        && !spec.zero
        && !spec.sharp
    {
        // i64::MIN-safe: widen before unsigned_abs; the magnitude fits u64.
        let mut val = (n as i128).unsigned_abs() as u64;
        let mut buf = [0u8; 21];
        let mut pos = buf.len();
        loop {
            pos -= 1;
            buf[pos] = b'0' + (val % 10) as u8;
            val /= 10;
            if val == 0 {
                break;
            }
        }
        if n < 0 {
            pos -= 1;
            buf[pos] = b'-';
        }
        // Digits and '-' only — always valid UTF-8.
        return unsafe { std::str::from_utf8_unchecked(&buf[pos..]) }.to_owned();
    }
    let negative = n < 0;
    let abs_val = (n as i128).unsigned_abs();
    let digits = match spec.conversion {
        'b' | 'B' => format!("{abs_val:b}"),
        'o' => format!("{abs_val:o}"),
        'x' => format!("{abs_val:x}"),
        'X' => format!("{abs_val:X}"),
        _ => abs_val.to_string(),
    };
    format_integer_digits(digits, negative, n == 0, spec)
}

/// Format a bignum for `%d` / `%o` / `%x` / `%X` specs. Mirrors the
/// `format_int_spec` fixnum path but uses `malachite::Integer`'s
/// `ToStringBase` trait for the underlying numeric conversion. The
/// flag/width/precision handling is identical.
fn format_bignum_spec(n: &Integer, spec: &FormatSpec) -> String {
    let negative = *n < 0;
    let abs = n.clone().abs();
    let mut digits = match spec.conversion {
        'b' | 'B' => abs.to_string_base(2),
        'o' => abs.to_string_base(8),
        'x' | 'X' => abs.to_string_base(16),
        _ => abs.to_string(),
    };
    if spec.conversion == 'X' {
        digits.make_ascii_uppercase();
    }
    format_integer_digits(digits, negative, *n == Integer::ZERO, spec)
}

fn format_integer_float_spec(f: f64, spec: &FormatSpec) -> Result<String, Flow> {
    if !f.is_finite() && matches!(spec.conversion, 'd' | 'i') {
        let text = if f.is_nan() {
            "nan"
        } else if f.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        };
        return Ok(apply_width(text, spec));
    }

    let big = Integer::rounding_from(f.trunc(), RoundingMode::Down).0;
    Ok(format_bignum_spec(&big, spec))
}

/// Normalize Rust scientific notation to match C printf: sign always
/// present, at least two exponent digits (e.g. `e0` -> `e+00`).
fn normalize_exp_notation(s: &str) -> String {
    if let Some(e_pos) = s.rfind('e').or_else(|| s.rfind('E')) {
        let (mantissa, exp_part) = s.split_at(e_pos);
        let e_char = &exp_part[..1];
        let rest = &exp_part[1..];
        let (sign, digits) = if rest.starts_with('+') || rest.starts_with('-') {
            (&rest[..1], &rest[1..])
        } else {
            ("+", rest)
        };
        let padded = if digits.len() < 2 {
            format!("{:0>2}", digits)
        } else {
            digits.to_string()
        };
        format!("{}{}{}{}", mantissa, e_char, sign, padded)
    } else {
        s.to_string()
    }
}

fn ensure_float_alternate_decimal(mut s: String) -> String {
    let mantissa_end = s.find(['e', 'E']).unwrap_or(s.len());
    if !s[..mantissa_end].contains('.') {
        s.insert(mantissa_end, '.');
    }
    s
}

/// Format a float with the given spec.
fn format_float_spec(f: f64, spec: &FormatSpec) -> String {
    // C printf spells non-finite values nan/inf (NAN/INF for the uppercase
    // conversions), with the sign bit preserved (e.g. 0.0/0.0 -> "-nan").
    if !f.is_finite() {
        let upper = matches!(spec.conversion, 'E' | 'G' | 'F');
        let body = if f.is_nan() {
            if upper { "NAN" } else { "nan" }
        } else if upper {
            "INF"
        } else {
            "inf"
        };
        let sign = if f.is_sign_negative() { "-" } else { "" };
        return apply_width(&format!("{sign}{body}"), spec);
    }
    let prec = spec.precision.unwrap_or(6);
    let alternate = spec.sharp && f.is_finite();
    let s = match spec.conversion {
        'f' => {
            let s = format!("{:.prec$}", f, prec = prec);
            if alternate {
                ensure_float_alternate_decimal(s)
            } else {
                s
            }
        }
        'e' => {
            let s = normalize_exp_notation(&format!("{:.prec$e}", f, prec = prec));
            if alternate {
                ensure_float_alternate_decimal(s)
            } else {
                s
            }
        }
        'E' => {
            let s = normalize_exp_notation(&format!("{:.prec$E}", f, prec = prec));
            if alternate {
                ensure_float_alternate_decimal(s)
            } else {
                s
            }
        }
        'g' | 'G' => {
            let p = if prec == 0 { 1 } else { prec };
            // %g uses %e if exponent < -4 or >= precision, else %f
            let exp_fmt = format!("{:.prec$e}", f, prec = p.saturating_sub(1));
            // Parse the exponent
            let exp_val = exp_fmt
                .rfind('e')
                .and_then(|i| exp_fmt[i + 1..].parse::<i32>().ok())
                .unwrap_or(0);
            if exp_val < -4 || exp_val >= p as i32 {
                // Use %e style, strip trailing zeros
                let mut s = format!("{:.prec$e}", f, prec = p.saturating_sub(1));
                // Strip trailing zeros before 'e'
                if !alternate && let Some(e_pos) = s.rfind('e') {
                    let mantissa = &s[..e_pos];
                    let exp_part = &s[e_pos..];
                    let trimmed = mantissa.trim_end_matches('0');
                    let trimmed = trimmed.trim_end_matches('.');
                    s = format!("{}{}", trimmed, exp_part);
                }
                s = normalize_exp_notation(&s);
                if spec.conversion == 'G' {
                    s = s.replace('e', "E");
                }
                if alternate {
                    s = ensure_float_alternate_decimal(s);
                }
                s
            } else {
                // Use %f style with appropriate decimals
                let decimal_places = (p as i32 - exp_val - 1).max(0) as usize;
                let mut s = format!("{:.prec$}", f, prec = decimal_places);
                // Strip trailing zeros after decimal point
                if !alternate && s.contains('.') {
                    s = s.trim_end_matches('0').to_string();
                    s = s.trim_end_matches('.').to_string();
                }
                if alternate {
                    s = ensure_float_alternate_decimal(s);
                }
                s
            }
        }
        _ => format!("{:.prec$}", f, prec = prec),
    };
    let s = if spec.plus && f >= 0.0 && !f.is_nan() {
        format!("+{}", s)
    } else if spec.space && f >= 0.0 && !f.is_nan() {
        format!(" {}", s)
    } else {
        s
    };
    apply_width(&s, spec)
}

/// Format a string (%s) with width and precision.
fn format_string_spec(data: &[u8], is_multibyte: bool, spec: &FormatSpec) -> Vec<u8> {
    format_string_spec_tracked(data, is_multibyte, spec).0
}

/// Like `format_string_spec` but also returns the byte range in the output bytes
/// that corresponds to the source argument's content (excluding padding spaces
/// added for width alignment). Used by `do_format` to track where each `%s`
/// argument lands so text properties can be copied from the argument to the
/// result. Mirrors the `info[i].start` / `info[i].end` tracking in GNU's
/// `styled_format` (editfns.c:3651-3806).
///
/// Issue #131: `data` is the argument's Emacs internal-encoding bytes, measured
/// with its own `is_multibyte` flag (a unibyte raw byte is one column, a
/// multibyte eight-bit char is four, matching GNU). The returned content is
/// canonical multibyte (unibyte content promoted), so the caller can splice it
/// into the byte result; padding is always ASCII spaces.
fn format_string_spec_tracked(
    data: &[u8],
    is_multibyte: bool,
    spec: &FormatSpec,
) -> (Vec<u8>, usize, usize) {
    let mut content_width = 0usize;
    let mut truncated_end = data.len();
    let mut saw_limit = spec.precision.is_none();
    if spec.precision.is_some() || spec.width.is_some() {
        truncated_end = 0;
        let mut pos = 0usize;
        while pos < data.len() {
            let (code, len) = next_format_unit(data, pos, is_multibyte);
            let display_width = format_unit_display_width(code, is_multibyte);
            if let Some(prec) = spec.precision
                && content_width + display_width > prec
            {
                saw_limit = true;
                break;
            }
            content_width += display_width;
            pos += len;
            truncated_end = pos;
        }
    }
    if !saw_limit {
        truncated_end = data.len();
    }
    let truncated = &data[..truncated_end];
    // Promote unibyte content to canonical multibyte so it can be spliced into
    // the byte result; multibyte content is already canonical.
    let content: Vec<u8> = if is_multibyte {
        truncated.to_vec()
    } else {
        crate::emacs_core::emacs_char::str_to_multibyte(truncated)
    };
    let content_bytes = content.len();
    if spec.width.is_none() && spec.precision.is_none() {
        content_width = emacs_chars_count(&content);
    }
    let w = match spec.width {
        Some(w) if w > content_width => w,
        _ => return (content, 0, content_bytes),
    };
    let pad_chars = w - content_width;
    // Padding is always ASCII spaces. Each padding char is one byte.
    if spec.minus {
        // Left-aligned: content first, then padding.
        let mut padded = content;
        padded.resize(content_bytes + pad_chars, b' ');
        (padded, 0, content_bytes)
    } else {
        // Right-aligned (default): padding first, then content.
        let mut padded = vec![b' '; pad_chars];
        padded.extend_from_slice(&content);
        (padded, pad_chars, pad_chars + content_bytes)
    }
}

/// Core format implementation shared by both pure and eval variants.
/// Maps a span of bytes in a `%s` argument to its byte range in the
/// formatted result, so the caller can copy text properties from the
/// argument to the corresponding span of the output. Mirrors GNU's
/// `info[].start`/`info[].end` tracking in `styled_format`
/// (editfns.c:3808-3813, applied at 4380-4396).
#[derive(Debug)]
pub(crate) struct FormatPropSpan {
    pub result_char_start: usize,
    pub result_char_end: usize,
    /// The STRING the copied properties come from -- not the argument index.
    ///
    /// GNU `styled_format` rewrites `spec_arguments[spec_index]` in place when it
    /// normalizes an argument (a symbol becomes `SYMBOL_NAME (arg)`), so the
    /// later interval copy reads the NORMALIZED value. Keeping an index here
    /// instead re-read the original argument, which made a symbol's propertized
    /// name impossible to express.
    pub source: Value,
    pub arg_char_len: usize,
}

/// How one run of format-string characters becomes one run of result
/// characters — the distinction GNU draws with its `discarded[]` table in
/// `styled_format` (`src/editfns.c:4325-4372`).
///
/// GNU's property-translation scan walks the format string byte by byte and
/// only advances `translated` for characters that are NOT discarded, with one
/// exception: the FIRST character of a conversion specification is
/// `discarded[] == 1`, and passing it jumps `translated` over the whole
/// converted field.  That exception is what keeps `#("%1d:" 0 2 (face F))`
/// from collapsing to an empty range when `%1d` becomes `3`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FormatSpanKind {
    /// A format-string character copied through (possibly quote-translated):
    /// one source character, one result character.
    Literal,
    /// `%%`: GNU discards the first `%` without any field to jump over, then
    /// copies the second.  A property boundary between the two `%`s therefore
    /// lands at the START of the copied character, not after it.
    PercentEscape,
    /// A conversion specification: every one of its characters is discarded
    /// and the converted field replaces the lot.  Any boundary past the
    /// spec's first character lands at the END of the field.
    Conversion,
}

#[derive(Debug)]
struct FormatSourceSpan {
    source_char_start: usize,
    source_char_end: usize,
    result_char_start: usize,
    result_char_end: usize,
    kind: FormatSpanKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FormatMessageQuotingStyle {
    None,
    Style(TextQuotingStyle),
}

impl FormatMessageQuotingStyle {
    fn from_text_quoting_style(style: TextQuotingStyle) -> Self {
        Self::Style(style)
    }
}

/// Push one format-string literal character `code` to the byte `result`,
/// applying `format-message` quote translation. Returns whether the pushed text
/// is genuinely multibyte (the curve quotes), which promotes the result.
fn push_format_literal_code(
    result: &mut Vec<u8>,
    code: u32,
    quoting_style: FormatMessageQuotingStyle,
) -> FormatLiteralPush {
    const BACKTICK: u32 = '`' as u32;
    const APOSTROPHE: u32 = '\'' as u32;
    match (quoting_style, code) {
        (FormatMessageQuotingStyle::Style(TextQuotingStyle::Curve), BACKTICK) => {
            push_emacs_char(result, '‘' as u32);
            FormatLiteralPush {
                multibyte: true,
                translated: true,
            }
        }
        (FormatMessageQuotingStyle::Style(TextQuotingStyle::Curve), APOSTROPHE) => {
            push_emacs_char(result, '’' as u32);
            FormatLiteralPush {
                multibyte: true,
                translated: true,
            }
        }
        (FormatMessageQuotingStyle::Style(TextQuotingStyle::Straight), BACKTICK) => {
            result.push(b'\'');
            FormatLiteralPush {
                multibyte: false,
                translated: true,
            }
        }
        _ => {
            push_emacs_char(result, code);
            FormatLiteralPush {
                multibyte: false,
                translated: false,
            }
        }
    }
}

/// Outcome of pushing one literal format-string character.
///
/// `translated` is GNU's `new_result` trigger for quote translation: it marks
/// that the output genuinely differs from the format string, which decides
/// whether `styled_format` builds a new string at all.
struct FormatLiteralPush {
    multibyte: bool,
    translated: bool,
}

/// Count the Emacs characters in canonical multibyte `data`.
fn emacs_chars_count(data: &[u8]) -> usize {
    let mut count = 0usize;
    let mut pos = 0usize;
    while pos < data.len() {
        let (_code, len) = crate::emacs_core::emacs_char::string_char(&data[pos..]);
        pos += len;
        count += 1;
    }
    count
}

/// Next character unit `(code, byte_len)` at `pos`, honouring the source string's
/// multibyteness: a unibyte string yields one raw byte per unit, a multibyte
/// string yields one Emacs character per unit.
fn next_format_unit(data: &[u8], pos: usize, is_multibyte: bool) -> (u32, usize) {
    if is_multibyte {
        crate::emacs_core::emacs_char::string_char(&data[pos..])
    } else {
        (data[pos] as u32, 1)
    }
}

/// Display width of one character unit, mirroring [`display_width_emacs`].
fn format_unit_display_width(code: u32, is_multibyte: bool) -> usize {
    use crate::emacs_core::emacs_char;
    if is_multibyte {
        if emacs_char::char_byte8_p(code) {
            4
        } else if let Some(ch) = char::from_u32(code) {
            crate::encoding::char_width(ch)
        } else {
            1
        }
    } else if code < 0x80 {
        char::from_u32(code)
            .map(crate::encoding::char_width)
            .unwrap_or(1)
    } else {
        1
    }
}

/// Push a single Emacs character `code` to `out` as canonical internal-encoding
/// bytes (eight-bit / non-Unicode codes become their disjoint extended sequence,
/// so a real Private-Use glyph survives instead of being mistaken for a raw byte).
fn push_emacs_char(out: &mut Vec<u8>, code: u32) {
    let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
    out.extend_from_slice(&buf[..len]);
}

/// Issue #131: does the formatted result carry genuine multibyte content — a real
/// (non eight-bit) character above ASCII? Eight-bit raw bytes alone must NOT
/// promote the result to multibyte (mirrors the old `runtime_string_result_multibyte`
/// sentinel exclusion); a real Latin-1/Unicode/Private-Use char must.
fn result_bytes_imply_multibyte(data: &[u8]) -> bool {
    let mut pos = 0usize;
    while pos < data.len() {
        let (code, len) = crate::emacs_core::emacs_char::string_char(&data[pos..]);
        pos += len;
        if code > 0x7f && !crate::emacs_core::emacs_char::char_byte8_p(code) {
            return true;
        }
    }
    false
}

/// Down-convert canonical multibyte Emacs bytes to unibyte raw bytes. Only valid
/// when the content has no genuine multibyte character (ASCII + eight-bit only),
/// which `build_format_result` guarantees before choosing a unibyte result; a
/// stray multibyte char is preserved defensively rather than dropped.
fn emacs_bytes_to_unibyte(data: &[u8]) -> Vec<u8> {
    use crate::emacs_core::emacs_char;
    let mut out = Vec::with_capacity(data.len());
    let mut pos = 0usize;
    while pos < data.len() {
        let (code, len) = emacs_char::string_char(&data[pos..]);
        pos += len;
        if code <= 0x7f {
            out.push(code as u8);
        } else if emacs_char::char_byte8_p(code) {
            out.push(emacs_char::char_to_byte8(code));
        } else {
            push_emacs_char(&mut out, code);
        }
    }
    out
}

// The tuple is the private, single-return handoff for bytes, property spans,
// source spans, and multibyte state; naming its four parts occurs at the caller.
#[allow(clippy::type_complexity)]
fn do_format(
    args: &[Value],
    princ_fn: &dyn Fn(&Value) -> Vec<u8>,
    prin1_fn: &dyn Fn(&Value) -> Vec<u8>,
    quoting_style: FormatMessageQuotingStyle,
) -> Result<
    (
        Vec<u8>,
        Vec<FormatPropSpan>,
        Vec<FormatSourceSpan>,
        bool,
        bool,
    ),
    Flow,
> {
    // Issue #131: iterate the format string as its real Emacs characters and
    // build the result directly as canonical Emacs internal-encoding bytes —
    // no storage-string round-trip, so a real Private-Use glyph survives and
    // eight-bit / non-Unicode content keeps its disjoint extended encoding.
    let fmt_ls = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    // Iterate the canonical multibyte BYTES directly: literal runs copy
    // whole (`%` is ASCII and can never occur inside a multibyte
    // sequence), spec characters are ASCII, and only the props/quoting
    // paths still step per character. The previous decode of the whole
    // format string into a `Vec<u32>` per call was a top cost of `format`.
    let fmt_storage: Vec<u8>;
    // ASCII unibyte bytes ARE their multibyte promotion — borrow them
    // directly instead of allocating an identity copy per `format` call.
    let fmt_bytes: &[u8] = if fmt_ls.is_multibyte() || fmt_ls.as_bytes().is_ascii() {
        fmt_ls.as_bytes()
    } else {
        fmt_storage = crate::emacs_core::emacs_char::str_to_multibyte(fmt_ls.as_bytes());
        &fmt_storage
    };

    let mut result: Vec<u8> = Vec::with_capacity(fmt_bytes.len() + 32);
    let mut spans: Vec<FormatPropSpan> = Vec::new();
    let mut source_spans: Vec<FormatSourceSpan> = Vec::new();
    let mut force_multibyte_result = false;
    // GNU `styled_format`'s `new_result`: set by a `%` conversion, `%%`, quote
    // translation, or a raw-byte conversion. While it stays false GNU returns
    // the format string ITSELF and copies no properties (editfns.c:4289).
    let mut new_result = false;
    let mut arg_idx = 1;
    let mut i = 0usize;
    let mut format_char_pos = 0usize;
    let mut result_char_pos = 0usize;
    let fmt_has_props =
        crate::emacs_core::value::get_string_text_properties_table_for_value(args[0]).is_some();
    // GNU `styled_format` only does interval bookkeeping when the format
    // string or some argument actually carries text properties
    // (editfns.c `arg_intervals` / `spec->intervals`). The spans collected
    // below feed `apply_format_string_prop_spans`/`apply_format_prop_spans`,
    // which are no-ops without a source property table — so skip all span
    // and char-position accounting for the plain propertyless call.
    let track_props = fmt_has_props
        || args[1..].iter().any(|arg| {
            // A symbol argument becomes its NAME string under %s, and that
            // name can carry text properties — probe the same source value
            // the %s arm formats.
            let source = super::misc_pure::symbol_name_string_for_format(*arg).unwrap_or(*arg);
            crate::emacs_core::value::get_string_text_properties_table_for_value(source).is_some()
        });

    while i < fmt_bytes.len() {
        if fmt_bytes[i] != b'%' {
            // Literal run up to the next `%` (or the end).
            let run_end = memchr::memchr(b'%', &fmt_bytes[i..])
                .map(|offset| i + offset)
                .unwrap_or(fmt_bytes.len());
            if !track_props && matches!(quoting_style, FormatMessageQuotingStyle::None) {
                // Verbatim byte copy: no spans, no quote translation, and
                // plain literals never set `force_multibyte_result` or
                // `new_result` (see `push_format_literal_code`'s default
                // arm) — multibyteness is decided from the result bytes in
                // `build_format_result`.
                result.extend_from_slice(&fmt_bytes[i..run_end]);
                i = run_end;
                continue;
            }
            // Props/quoting path: step per character, exactly as before.
            while i < run_end {
                let (code, char_len) = crate::emacs_core::emacs_char::string_char(&fmt_bytes[i..]);
                let source_start = format_char_pos;
                i += char_len;
                format_char_pos += 1;
                if code < 0x80 && matches!(quoting_style, FormatMessageQuotingStyle::None) {
                    result.push(code as u8);
                } else {
                    let pushed = push_format_literal_code(&mut result, code, quoting_style);
                    force_multibyte_result |= pushed.multibyte;
                    new_result |= pushed.translated;
                }
                if track_props {
                    source_spans.push(FormatSourceSpan {
                        source_char_start: source_start,
                        source_char_end: format_char_pos,
                        result_char_start: result_char_pos,
                        result_char_end: result_char_pos + 1,
                        kind: FormatSpanKind::Literal,
                    });
                    result_char_pos += 1;
                }
            }
            continue;
        }

        let source_start = format_char_pos;
        i += 1;
        format_char_pos += 1;
        let parsed = parse_format_spec(fmt_bytes, &mut i)?;
        let spec = parsed.spec;
        format_char_pos += parsed.consumed_chars;
        let spec_source_end = format_char_pos;

        // Any conversion, `%%` included, makes the result genuinely new.
        new_result = true;
        if spec.conversion == '%' {
            result.push(b'%');
            if track_props {
                source_spans.push(FormatSourceSpan {
                    source_char_start: source_start,
                    source_char_end: spec_source_end,
                    result_char_start: result_char_pos,
                    result_char_end: result_char_pos + 1,
                    kind: FormatSpanKind::PercentEscape,
                });
                result_char_pos += 1;
            }
            continue;
        }

        let this_arg_idx = spec.field_number.unwrap_or(arg_idx);
        if this_arg_idx >= args.len() {
            return Err(format_not_enough_args_error());
        }

        let formatted: Vec<u8> = match spec.conversion {
            's' => {
                // Only `%s` on a string argument preserves text properties: the
                // string's own bytes land verbatim, so we can map byte ranges in
                // the argument to byte ranges in the formatted result. For other
                // types princ_fn produces a fresh printed representation with no
                // property origin. Mirrors GNU styled_format (editfns.c:3808),
                // which sets `spec->intervals` only when `string_intervals (arg)`
                // is non-NULL. The string's content is measured with its own
                // multibyteness (a unibyte raw byte is one column, a multibyte
                // eight-bit char four), matching GNU's width.
                // GNU `styled_format`: a SYMBOL argument becomes its NAME
                // STRING before anything else looks at it, so from here it is a
                // string argument like any other and its own text properties
                // propagate. A symbol interned from propertized text keeps those
                // properties on its name, which is how an error message built
                // with `(error "..." SYM)' carries them.
                // Plain `%s` on a string with no property tracking: the bytes
                // land verbatim (exactly what the general path produces), so
                // append them directly instead of building two transient
                // copies (`to_vec` + the formatted Vec) per conversion.
                if !track_props
                    && spec.width.is_none()
                    && spec.precision.is_none()
                    && !spec.minus
                    && !spec.plus
                    && !spec.space
                    && !spec.zero
                    && !spec.sharp
                {
                    let arg = super::misc_pure::symbol_name_string_for_format(args[this_arg_idx])
                        .unwrap_or(args[this_arg_idx]);
                    if let Some(ls) = arg.as_lisp_string() {
                        // Multibyte bytes are already canonical; ASCII unibyte
                        // promotes to itself. Raw 128-255 unibyte bytes need
                        // the general path's str_to_multibyte promotion
                        // (issue #131 eight-bit chars).
                        if ls.is_multibyte() || ls.as_bytes().is_ascii() {
                            result.extend_from_slice(ls.as_bytes());
                            arg_idx = this_arg_idx + 1;
                            continue;
                        }
                    }
                }
                let arg = super::misc_pure::symbol_name_string_for_format(args[this_arg_idx])
                    .unwrap_or(args[this_arg_idx]);
                let arg_is_string = arg.is_string();
                let (s, src_multibyte) = if let Some(ls) = arg.as_lisp_string() {
                    (ls.as_bytes().to_vec(), ls.is_multibyte())
                } else {
                    (princ_fn(&arg), true)
                };
                let (formatted, content_byte_start_in_formatted, content_byte_end_in_formatted) =
                    format_string_spec_tracked(&s, src_multibyte, &spec);
                if track_props
                    && arg_is_string
                    && content_byte_start_in_formatted < content_byte_end_in_formatted
                {
                    let formatted_chars = emacs_chars_count(&formatted);
                    let content_char_start_in_formatted =
                        emacs_chars_count(&formatted[..content_byte_start_in_formatted]);
                    let field_char_start_in_formatted = if fmt_has_props {
                        0
                    } else {
                        content_char_start_in_formatted
                    };
                    let span_char_len = formatted_chars - field_char_start_in_formatted;
                    let arg_char_len = arg
                        .as_lisp_string()
                        .map(|string| string.schars())
                        .unwrap_or(0);
                    spans.push(FormatPropSpan {
                        result_char_start: result_char_pos + field_char_start_in_formatted,
                        result_char_end: result_char_pos
                            + field_char_start_in_formatted
                            + span_char_len,
                        source: arg,
                        arg_char_len,
                    });
                }
                formatted
            }
            'S' => {
                let s = prin1_fn(&args[this_arg_idx]);
                format_string_spec(&s, true, &spec)
            }
            'd' | 'i' | 'b' | 'B' | 'o' | 'x' | 'X' => {
                // Plain `%d` on a fixnum with no property tracking: render the
                // digits straight into the result, GNU-sprintf style — the
                // general path below builds (and immediately drops) a String
                // per conversion.
                if !track_props
                    && matches!(spec.conversion, 'd' | 'i')
                    && spec.width.is_none()
                    && spec.precision.is_none()
                    && !spec.minus
                    && !spec.plus
                    && !spec.space
                    && !spec.zero
                    && !spec.sharp
                {
                    if let ValueKind::Fixnum(n) = args[this_arg_idx].kind() {
                        push_i64_decimal(&mut result, n);
                        arg_idx = this_arg_idx + 1;
                        continue;
                    }
                }
                let formatted = match args[this_arg_idx].kind() {
                    ValueKind::Fixnum(i) => format_int_spec(i, &spec),
                    ValueKind::Float => {
                        format_integer_float_spec(args[this_arg_idx].xfloat(), &spec)?
                    }
                    ValueKind::Veclike(VecLikeType::Bignum) => {
                        format_bignum_spec(args[this_arg_idx].as_bignum().unwrap(), &spec)
                    }
                    _ => {
                        return Err(format_spec_type_mismatch_error());
                    }
                };
                formatted.into_bytes()
            }
            // GNU only treats lowercase e/f/g as float conversions; uppercase
            // E/G (like F/A) fall through to the "Invalid format operation"
            // error below, matching `float_conversion` in editfns.c.
            'f' | 'e' | 'g' => {
                let f = expect_number(&args[this_arg_idx])
                    .map_err(|_| format_spec_type_mismatch_error())?;
                format_float_spec(f, &spec).into_bytes()
            }
            'c' => {
                let n = expect_int(&args[this_arg_idx])
                    .map_err(|_| format_spec_type_mismatch_error())?;
                let formatted_char = format_char_argument(n)?;
                force_multibyte_result |= formatted_char.force_multibyte_result;
                format_string_spec(&formatted_char.rendered, true, &spec)
            }
            _ => {
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Invalid format operation %{}",
                        spec.conversion
                    ))],
                ));
            }
        };
        arg_idx = this_arg_idx + 1;
        if track_props {
            let formatted_chars = emacs_chars_count(&formatted);
            if formatted_chars > 0 {
                source_spans.push(FormatSourceSpan {
                    source_char_start: source_start,
                    source_char_end: spec_source_end,
                    result_char_start: result_char_pos,
                    result_char_end: result_char_pos + formatted_chars,
                    kind: FormatSpanKind::Conversion,
                });
            }
            result_char_pos += formatted_chars;
        }
        result.extend_from_slice(&formatted);
    }

    Ok((
        result,
        spans,
        source_spans,
        force_multibyte_result,
        new_result,
    ))
}

fn build_format_result(
    args: &[Value],
    bytes: Vec<u8>,
    spans: &[FormatPropSpan],
    source_spans: &[FormatSourceSpan],
    force_multibyte_result: bool,
) -> Value {
    // GNU `styled_format` decides multibyteness from the format/argument strings
    // and from %c/%S/quoting that forces it; neomacs also inspects the result for
    // a genuine (non eight-bit) multibyte character, since %S/printer output can
    // introduce one without a multibyte argument string. Eight-bit raw bytes do
    // NOT promote (issue #131: a raw unibyte byte stays unibyte).
    // An all-ASCII result (the common case) needs neither the per-char
    // multibyte probe nor the unibyte down-conversion — both were decoding
    // every result character out of line on every `format` call.
    let all_ascii = bytes.iter().all(|&byte| byte < 0x80);
    let multibyte = force_multibyte_result
        || args.iter().any(|value| value.string_is_multibyte())
        || (!all_ascii && result_bytes_imply_multibyte(&bytes));
    // `bytes` are canonical multibyte Emacs encoding. A unibyte result has no
    // genuine multibyte character, so down-convert eight-bit chars back to raw
    // bytes (preserving e.g. a raw unibyte payload passed through verbatim).
    let result = Value::heap_string(if multibyte {
        crate::heap_types::LispString::from_emacs_bytes(bytes)
    } else if all_ascii {
        crate::heap_types::LispString::from_unibyte(bytes)
    } else {
        crate::heap_types::LispString::from_unibyte(emacs_bytes_to_unibyte(&bytes))
    });

    // Copy text properties from the format string first, then from each
    // `%s` argument's string, mirroring GNU `styled_format`
    // (editfns.c:4299-4396).  GNU routes each copied plist through
    // `Fadd_text_properties`; adding its pairs prepends them one by one, so a
    // copied plist has the reverse of its source order.  The order is
    // observable through `text-properties-at`.
    apply_format_string_prop_spans(result, args[0], source_spans);

    // Copy text properties from each `%s` argument's string into the
    // corresponding span of the formatted output, mirroring GNU
    // `styled_format` (editfns.c:4380-4396). Without this, `format`
    // silently strips properties that the caller set on the source
    // string — the bug that left Doom's dashboard menu items
    // uncolored (their button faces came from a `(buffer-string)`
    // that got flattened by `(format "%-37s" ...)`).
    apply_format_prop_spans(result, spans);

    result
}

/// Return GNU `styled_format`'s exact `%s` identity result when it is a string.
///
/// GNU short-circuits an unpropertized two-character format string `%s` after
/// resolving the argument.  Reusing a string argument is observable through
/// `eq`, and it preserves the interval plist verbatim instead of copying it
/// through `add-text-properties`.  Keep that reuse decision outside
/// `do_format`: `Some` is the complete result, while `None` requires the normal
/// allocation and property-transfer pipeline.
fn exact_percent_s_string_result(args: &[Value]) -> Option<Value> {
    let format = args.first()?;
    let format_string = format.as_lisp_string()?;
    if format_string.as_bytes() != b"%s"
        || crate::emacs_core::value::get_string_text_properties_table_for_value(*format)
            .is_some_and(|table| !table.is_empty())
    {
        return None;
    }
    args.get(1).copied().filter(|value| value.is_string())
}

pub(crate) fn builtin_format_wrapper_strict_slice(
    ctx: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    crate::emacs_core::perf_trace::time_op(crate::emacs_core::perf_trace::HotpathOp::Format, || {
        expect_min_args("format", args, 1)?;
        if let Some(result) = exact_percent_s_string_result(args) {
            return Ok(result);
        }
        let (bytes, spans, source_spans, force_multibyte_result, new_result) = do_format(
            args,
            &|v| format_percent_s_in_state(ctx, v),
            &|v| super::error::print_value_bytes_escaped_with_eval(ctx, v),
            FormatMessageQuotingStyle::None,
        )?;
        // GNU `styled_format`: `if (! new_result) { val = args[0]; goto return_val; }`
        // (editfns.c:4289). Nothing was formatted, so GNU builds no new string and
        // copies no properties — the format string's own plist keeps its supplied
        // order. Rebuilding it here applied the additive transfer and reversed
        // help-key plists on `(user-error (substitute-command-keys "…"))`.
        if !new_result {
            return Ok(args[0]);
        }
        Ok(build_format_result(
            args,
            bytes,
            &spans,
            &source_spans,
            force_multibyte_result,
        ))
    })
}

fn apply_format_string_prop_spans(result: Value, format_value: Value, spans: &[FormatSourceSpan]) {
    if spans.is_empty() {
        return;
    }
    let Some(src_table) =
        crate::emacs_core::value::get_string_text_properties_table_for_value(format_value)
    else {
        return;
    };
    let Some(result_string) = result.as_lisp_string() else {
        return;
    };
    if result_string.schars() == 0 {
        return;
    }

    let mut table = crate::emacs_core::value::get_string_text_properties_table_for_value(result)
        .unwrap_or_default();
    let mut touched = false;

    for src_interval in src_table.intervals_snapshot() {
        let Some(result_start) = translate_format_source_position(src_interval.start, spans) else {
            continue;
        };
        let Some(result_end) = translate_format_source_position(src_interval.end, spans) else {
            continue;
        };
        if result_start >= result_end {
            continue;
        }

        let properties: Vec<_> = src_interval
            .ordered_properties()
            .map(|(name, value)| (name, *value))
            .collect();
        touched |= table.apply_property_plist_for_object_char_len(
            string_char_range(result_start, result_end),
            CharLen::new(result_string.schars()),
            &properties,
            PropertyPlistApplication::AddProperties,
        );
    }

    if touched {
        crate::emacs_core::value::set_string_text_properties_table_for_value(result, table);
    }
}

fn translate_format_source_position(pos: usize, spans: &[FormatSourceSpan]) -> Option<usize> {
    if let Some(first) = spans.first()
        && pos <= first.source_char_start
    {
        return Some(first.result_char_start);
    }

    for span in spans {
        if pos == span.source_char_start {
            return Some(span.result_char_start);
        }
        if pos == span.source_char_end {
            return Some(span.result_char_end);
        }
        if span.source_char_start < pos && pos < span.source_char_end {
            // GNU's `discarded[]` scan (`src/editfns.c:4331-4372`) decides an
            // interior endpoint by what the characters it has walked past do.
            return Some(match span.kind {
                // Every character of the spec is discarded, and passing the
                // FIRST one jumps `translated` over the whole converted field
                // (`discarded[bytepos] == 1` plus the `info[fieldn]` match).
                // So any boundary past the leading `%` is the field's end —
                // this is what keeps `#("%1d:" 0 2 (face F))` formatting to
                // `#("3:" 0 1 (face F))` instead of losing the property to an
                // empty range.
                FormatSpanKind::Conversion => span.result_char_end,
                // `%%` discards its leading `%` with no field to jump over,
                // so `translated` has not moved yet when the boundary falls
                // between the two `%`s.
                FormatSpanKind::PercentEscape => span.result_char_start,
                // One source character, one result character: no interior.
                FormatSpanKind::Literal => span.result_char_start + (pos - span.source_char_start),
            });
        }
    }

    spans.last().map(|span| span.result_char_end)
}

fn apply_format_prop_spans(result: Value, spans: &[FormatPropSpan]) {
    if spans.is_empty() {
        return;
    }
    let Some(result_string) = result.as_lisp_string() else {
        return;
    };
    let mut table = crate::emacs_core::value::get_string_text_properties_table_for_value(result)
        .unwrap_or_default();
    let mut touched = false;
    for span in spans {
        let Some(src_table) =
            crate::emacs_core::value::get_string_text_properties_table_for_value(span.source)
        else {
            continue;
        };
        let new_len = span.result_char_end.saturating_sub(span.result_char_start);
        for interval in src_table.intervals_snapshot() {
            if interval.start >= new_len {
                continue;
            }
            let mut end = interval.end;
            if (end == span.arg_char_len && end != new_len) || end > new_len {
                end = new_len;
            }
            if interval.start >= end {
                continue;
            }
            let properties: Vec<_> = interval
                .ordered_properties()
                .map(|(name, value)| (name, *value))
                .collect();
            touched |= table.apply_property_plist_for_object_char_len(
                string_char_range(
                    span.result_char_start + interval.start,
                    span.result_char_start + end,
                ),
                CharLen::new(result_string.schars()),
                &properties,
                PropertyPlistApplication::AddProperties,
            );
        }
    }
    if touched {
        crate::emacs_core::value::set_string_text_properties_table_for_value(result, table);
    }
}

pub(crate) fn builtin_format_message(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_format_message_slice(ctx, &args)
}

pub(crate) fn builtin_format_message_slice(
    ctx: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    crate::emacs_core::perf_trace::time_op(crate::emacs_core::perf_trace::HotpathOp::Format, || {
        expect_min_args("format-message", args, 1)?;
        if let Some(result) = exact_percent_s_string_result(args) {
            return Ok(result);
        }
        let quoting_style = crate::emacs_core::coding::builtin_text_quoting_style(ctx, vec![])?;
        let quoting_style = TextQuotingStyle::from_symbol_value(quoting_style)
            .map(FormatMessageQuotingStyle::from_text_quoting_style)
            .expect("text-quoting-style builtin returns a GNU quoting style symbol");
        let (bytes, spans, source_spans, force_multibyte_result, new_result) = do_format(
            args,
            &|v| format_percent_s_in_state(ctx, v),
            &|v| super::error::print_value_bytes_escaped_with_eval(ctx, v),
            quoting_style,
        )?;
        // GNU `styled_format`: `if (! new_result) { val = args[0]; goto return_val; }`
        // (editfns.c:4289). Nothing was formatted, so GNU builds no new string and
        // copies no properties — the format string's own plist keeps its supplied
        // order. Rebuilding it here applied the additive transfer and reversed
        // help-key plists on `(user-error (substitute-command-keys "…"))`.
        if !new_result {
            return Ok(args[0]);
        }
        Ok(build_format_result(
            args,
            bytes,
            &spans,
            &source_spans,
            force_multibyte_result,
        ))
    })
}

pub(crate) fn builtin_make_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("make-string", &args, 2)?;
    expect_max_args("make-string", &args, 3)?;
    let count = expect_wholenump(&args[0])? as usize;

    let ch = match args[1].kind() {
        ValueKind::Fixnum(c) => {
            if c < 0 {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("characterp"), args[1]],
                ));
            }
            c as u32
        }
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), args[1]],
            ));
        }
    };

    // GNU Emacs alloc.c: `make-string` returns a unibyte string only when the
    // initializer is ASCII and the optional MULTIBYTE arg is nil/omitted.
    let multibyte = args.get(2).is_some_and(|v| v.is_truthy()) || ch > 0x7f;

    use crate::emacs_core::emacs_char;
    if ch > emacs_char::MAX_CHAR {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), args[1]],
        ));
    }
    if multibyte {
        let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = emacs_char::char_string(ch, &mut buf);
        let unit = &buf[..len];
        let mut data = Vec::with_capacity(len * count);
        for _ in 0..count {
            data.extend_from_slice(unit);
        }
        Ok(Value::heap_string(
            crate::heap_types::LispString::from_emacs_bytes(data),
        ))
    } else {
        if ch > 0xff {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), args[1]],
            ));
        }
        let data = vec![ch as u8; count];
        Ok(Value::heap_string(
            crate::heap_types::LispString::from_unibyte(data),
        ))
    }
}

pub(crate) fn builtin_string_slice(args: &[Value]) -> EvalResult {
    use crate::emacs_core::emacs_char;
    let mut result = Vec::new();
    let mut nbytes = 0usize;
    for arg in args {
        match arg.kind() {
            ValueKind::Fixnum(c) => {
                // GNU `Fstring' checks `CHARACTERP' before encoding; event
                // modifier bits are not valid character codes here.
                if !(0..=emacs_char::MAX_CHAR as i64).contains(&c) {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("characterp"), *arg],
                    ));
                }
                let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
                let len = emacs_char::char_string(c as u32, &mut buf);
                nbytes += len;
                result.extend_from_slice(&buf[..len]);
            }
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("characterp"), *arg],
                ));
            }
        }
    }
    if nbytes == args.len() {
        Ok(Value::heap_string(
            crate::heap_types::LispString::from_unibyte(result),
        ))
    } else {
        Ok(Value::heap_string(
            crate::heap_types::LispString::from_emacs_bytes(result),
        ))
    }
}

/// `(unibyte-string &rest BYTES)` -> unibyte storage string.
pub(crate) fn builtin_unibyte_string(args: Vec<Value>) -> EvalResult {
    let mut bytes = Vec::with_capacity(args.len());
    for arg in args {
        let n = match arg.kind() {
            ValueKind::Fixnum(v) => v,
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), arg],
                ));
            }
        };
        if !(0..=255).contains(&n) {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![Value::fixnum(n), Value::fixnum(0), Value::fixnum(255)],
            ));
        }
        bytes.push(n as u8);
    }
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_unibyte(bytes),
    ))
}

pub(crate) fn builtin_byte_to_string(args: Vec<Value>) -> EvalResult {
    expect_args("byte-to-string", &args, 1)?;
    let byte = expect_fixnum(&args[0])?;
    if !(0..=255).contains(&byte) {
        return Err(signal("error", vec![Value::string("Invalid byte")]));
    }
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_unibyte(vec![byte as u8]),
    ))
}

pub(crate) fn builtin_bitmap_spec_p(args: Vec<Value>) -> EvalResult {
    expect_args("bitmap-spec-p", &args, 1)?;
    Ok(Value::NIL)
}

/// `(clear-face-cache &optional THOROUGHLY)` -- GNU `Fclear_face_cache`
/// (src/xfaces.c:794-803).
///
/// GNU's whole body is `clear_face_cache (...)`, `face_change = true`,
/// `windows_or_buffers_changed = 53`: every realized face is discarded and the
/// next redisplay realizes them again.  Doing nothing is observable, because
/// realization is what reads the palette -- after `tty-color-define` moves a
/// colour, GNU repaints it at its new index and a no-op leaves the old one:
///
///     ;; TERM=xterm-256color, `(tty-color-define "red" 200 '(65535 0 0))'
///     ;; and `(clear-face-cache)' AFTER the first redisplay, then a face with
///     ;; `:foreground "red"' repainted
///     ;; GNU     => ESC [ 31 m   then  ESC [ 38;5;200 m
///     ;; Neomacs => ESC [ 31 m   then  ESC [ 31 m
///
/// Neomacs has no separate realized-face cache to free: the render-facing table
/// IS the realization, rebuilt by `sync_runtime_faces_for_frame`, which is
/// memoized on `face_change_count`.  Bumping that counter is therefore exactly
/// GNU's `face_change = true`.  THOROUGHLY selects font freeing, which has no
/// counterpart here, so it is accepted and ignored as GNU accepts it on a
/// terminal frame.
pub(crate) fn builtin_clear_face_cache(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("clear-face-cache", &args, 1)?;
    ctx.face_change_count += 1;
    Ok(Value::NIL)
}

pub(crate) fn builtin_clear_buffer_auto_save_failure(args: Vec<Value>) -> EvalResult {
    expect_args("clear-buffer-auto-save-failure", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_string_width(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("string-width", &args, 1)?;
    expect_max_args("string-width", &args, 3)?;
    let ls = ctx.lisp_string(args[0]).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let data = ls.as_bytes();
    let is_multibyte = ls.is_multibyte();
    let display_table = crate::encoding::active_display_table(ctx);
    // GNU `lisp_string_width' measures each character with `char_width', which
    // bottoms out in `CHARACTER_WIDTH' returning `SANE_TAB_WIDTH(current_buffer)'
    // for a TAB -- i.e. the dynamically-bound `tab-width', not a hardcoded 8.
    let tab_width = crate::emacs_core::indent::current_buffer_tab_width(ctx);
    let unit_width = |code: u32, width: usize| -> usize {
        if display_table.is_some() {
            crate::encoding::char_width_for_code_with_display_table(code as i64, display_table)
        } else if code == 0x09 {
            tab_width
        } else {
            width
        }
    };
    if args.len() <= 1
        || (args.len() == 2 && args[1] == Value::NIL)
        || (args.len() <= 3
            && (args.len() < 2 || args[1] == Value::NIL || args[1] == Value::fixnum(0))
            && (args.len() < 3 || args[2] == Value::NIL))
    {
        // Fast path: full string width
        let units = super::super::string_escape::decode_units_emacs(data, is_multibyte);
        let width = units
            .iter()
            .map(|(code, width)| unit_width(*code, *width))
            .sum::<usize>();
        return Ok(Value::fixnum(width as i64));
    }
    // Substring range specified — decode units and sum width for [from, to)
    let units = super::super::string_escape::decode_units_emacs(data, is_multibyte);
    let len = units.len() as i64;
    let normalize_index = |value: &Value, default: i64| -> Result<usize, Flow> {
        let raw = if value.is_nil() {
            default
        } else {
            expect_int(value)?
        };
        let idx = if raw < 0 { len + raw } else { raw };
        if idx < 0 || idx > len {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![
                    args[0],
                    args.get(1).copied().unwrap_or(Value::fixnum(0)),
                    args.get(2).copied().unwrap_or(Value::NIL),
                ],
            ));
        }
        Ok(idx as usize)
    };
    let from = if args.len() > 1 && args[1] != Value::NIL {
        normalize_index(&args[1], 0)?
    } else {
        0
    };
    let to = if args.len() > 2 && args[2] != Value::NIL {
        normalize_index(&args[2], len)?
    } else {
        units.len()
    };
    if from > to {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![
                args[0],
                args.get(1).copied().unwrap_or(Value::fixnum(0)),
                args.get(2).copied().unwrap_or(Value::NIL),
            ],
        ));
    }
    let width: usize = units
        .iter()
        .skip(from)
        .take(to - from)
        .map(|(code, width)| unit_width(*code, *width))
        .sum();
    Ok(Value::fixnum(width as i64))
}
