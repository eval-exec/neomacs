//! Search and regex builtins for the Elisp interpreter.
//!
//! Pure builtins:
//! - `string-match`, `string-match-p`, `regexp-quote`
//! - `match-beginning`, `match-end`, `match-data`, `set-match-data`
//! - `looking-at`, `looking-at-p`, `replace-regexp-in-string`
//!
//! Eval-dependent builtins:
//! - `search-forward`, `search-backward`
//! - `re-search-forward`, `re-search-backward`
//! - `posix-search-forward`, `posix-search-backward`
//! - `replace-match`
//! - `word-search-forward`, `word-search-backward`

use super::regex::MatchGroup;
use neovm_edit::buffer::{CharLen, CharPos0, EmacsBytePos};
use neovm_value::error::LispCondition;
#[cfg(test)]
use neovm_value::error::expect_args_range;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::error::{expect_args, expect_fixnum};
use neovm_value::value::ValueKind;
use neovm_value::value::*;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

#[inline]
fn buffer_match_char_pos_to_byte_pos(
    buf: &neovm_edit::buffer::Buffer,
    lisp_char_pos: usize,
) -> EmacsBytePos {
    buf.char_pos_to_emacs_byte_pos_clamped(
        CharPos0::new(lisp_char_pos).saturating_sub_len(CharLen::new(1)),
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_int(val: &Value) -> Result<i64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *val],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_integer_or_marker(val: &Value) -> Result<i64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *val],
        )),
    }
}

/// See `builtins::expect_lisp_string`: the borrow is tied to VAL's place, not
/// to `'static` (DIVERGENCES.md 163).
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn expect_lisp_string(val: &Value) -> Result<&neovm_value::heap_types::LispString, Flow> {
    val.as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *val],
        )
    })
}

fn cloned_lisp_string_value(string: &neovm_value::heap_types::LispString) -> Value {
    Value::heap_string(string.clone())
}

fn regexp_quote_lisp_string(
    input: &neovm_value::heap_types::LispString,
) -> neovm_value::heap_types::LispString {
    let mut out = Vec::with_capacity(input.as_bytes().len() + 8);
    for &byte in input.as_bytes() {
        match byte {
            b'.' | b'*' | b'+' | b'?' | b'[' | b'^' | b'$' | b'\\' => {
                out.push(b'\\');
                out.push(byte);
            }
            _ => out.push(byte),
        }
    }

    if input.is_multibyte() {
        neovm_value::heap_types::LispString::from_emacs_bytes(out)
    } else {
        neovm_value::heap_types::LispString::from_unibyte(out)
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn normalize_string_start_arg(string: &str, start: Option<&Value>) -> Result<usize, Flow> {
    let Some(start_val) = start else {
        return Ok(0);
    };
    if start_val.is_nil() {
        return Ok(0);
    }

    let raw_start = expect_fixnum(start_val)?;
    let string_bytes = string.as_bytes();
    let len = neovm_value::emacs_char::chars_in_multibyte(string_bytes) as i64;
    let normalized = if raw_start < 0 {
        len.checked_add(raw_start)
    } else {
        Some(raw_start)
    };

    let Some(start_idx) = normalized else {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::string(string), Value::fixnum(raw_start)],
        ));
    };

    if !(0..=len).contains(&start_idx) {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::string(string), Value::fixnum(raw_start)],
        ));
    }

    let start_char_idx = start_idx as usize;
    if start_char_idx == len as usize {
        return Ok(string.len());
    }

    Ok(neovm_value::emacs_char::char_to_byte_pos(
        string_bytes,
        start_char_idx,
    ))
}

pub(crate) fn normalize_lisp_string_start_arg(
    string: &neovm_value::heap_types::LispString,
    start: Option<&Value>,
) -> Result<usize, Flow> {
    normalize_lisp_string_start_arg_for(None, string, start)
}

/// [`normalize_lisp_string_start_arg`] for the string object `value`: START
/// converts through the position cache, which a `string-match' loop that
/// resumes at the previous `match-end' (`split-string') hits every time.
#[doc(hidden)]
pub fn normalize_lisp_string_value_start_arg(
    value: Value,
    start: Option<&Value>,
) -> Result<usize, Flow> {
    let string = value
        .as_lisp_string()
        .expect("caller passes a string object");
    normalize_lisp_string_start_arg_for(Some(value), string, start)
}

fn normalize_lisp_string_start_arg_for(
    value: Option<Value>,
    string: &neovm_value::heap_types::LispString,
    start: Option<&Value>,
) -> Result<usize, Flow> {
    let Some(start_val) = start else {
        return Ok(0);
    };
    if start_val.is_nil() {
        return Ok(0);
    }

    let raw_start = expect_fixnum(start_val)?;
    if !string.is_multibyte() {
        let len = string.byte_len() as i64;
        let normalized = if raw_start < 0 {
            len.checked_add(raw_start)
        } else {
            Some(raw_start)
        };
        let Some(start_idx) = normalized else {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![cloned_lisp_string_value(string), Value::fixnum(raw_start)],
            ));
        };
        if !(0..=len).contains(&start_idx) {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![cloned_lisp_string_value(string), Value::fixnum(raw_start)],
            ));
        }
        return Ok(start_idx as usize);
    }

    let len = string.schars() as i64;
    let normalized = if raw_start < 0 {
        len.checked_add(raw_start)
    } else {
        Some(raw_start)
    };
    let Some(start_idx) = normalized else {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![cloned_lisp_string_value(string), Value::fixnum(raw_start)],
        ));
    };
    if !(0..=len).contains(&start_idx) {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![cloned_lisp_string_value(string), Value::fixnum(raw_start)],
        ));
    }
    let start_char_idx = start_idx as usize;
    Ok(match value {
        Some(value) => {
            neovm_value::string_pos_cache::string_char_to_byte(value, string, start_char_idx)
        }
        None => string.char_to_byte_pos(start_char_idx),
    })
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// `(regexp-quote STRING)` -- return a regexp that matches STRING literally,
/// quoting all special regex characters.
#[doc(hidden)]
pub fn builtin_regexp_quote(args: Vec<Value>) -> EvalResult {
    crate::emacs_core::substrate_hooks::time_op(
        crate::emacs_core::substrate_hooks::TextPerfOp::RegexpQuote,
        || {
            expect_args("regexp-quote", &args, 1)?;
            let string = expect_lisp_string(&args[0])?;
            Ok(Value::heap_string(regexp_quote_lisp_string(string)))
        },
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn parse_replace_regexp_subexp_start_lisp(
    args: &[Value],
    string: &neovm_value::heap_types::LispString,
) -> Result<ReplaceRegexpSubexpStart, Flow> {
    let subexp = match args.get(5) {
        Some(v) if v.is_nil() => 0i64,
        None => 0i64,
        Some(value) => expect_int(value)?,
    };
    if subexp < 0 {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![
                Value::fixnum(subexp),
                Value::fixnum(0),
                Value::fixnum(string.schars() as i64),
            ],
        ));
    }
    let start = normalize_lisp_string_start_arg(string, args.get(6))?;
    Ok(ReplaceRegexpSubexpStart {
        subexp: subexp as usize,
        start,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
struct ReplaceRegexpSubexpStart {
    subexp: usize,
    start: usize,
}

/// Test-support view of the subexp/start parse (struct fields cannot carry
/// `visibility::make`; this tuple wrapper is the TEST-SUPPORT-VIS A1 row).
#[cfg(any(test, feature = "test-support"))]
pub fn parse_replace_regexp_subexp_start(
    args: &[Value],
    string: &neovm_value::heap_types::LispString,
) -> Result<(usize, usize), Flow> {
    let parsed = parse_replace_regexp_subexp_start_lisp(args, string)?;
    Ok((parsed.start, parsed.subexp))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn translate_match_data_to_substring(
    match_data: &super::regex::MatchData,
    delta: i64,
    searched_string: super::regex::SearchedString,
) -> super::regex::MatchData {
    super::regex::MatchData::string(
        match_data
            .groups_snapshot()
            .into_iter()
            .map(|group| group.map(|group| group.translate_saturating(delta)))
            .collect(),
        Some(searched_string),
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn replace_match_on_substring(
    source: &neovm_value::heap_types::LispString,
    replacement: &neovm_value::heap_types::LispString,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<super::regex::MatchData>,
) -> Result<neovm_value::heap_types::LispString, Flow> {
    if let Some(md) = match_data
        && subexp >= md.group_count()
    {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![
                Value::fixnum(subexp as i64),
                Value::fixnum(0),
                Value::fixnum(md.group_count().saturating_sub(1) as i64),
            ],
        ));
    }
    replace_match_lisp_string_with_syntax(
        source,
        replacement,
        fixedcase,
        literal,
        subexp,
        match_data,
    )
    .map_err(|msg| signal("error", vec![Value::string(msg)]))
}

fn concat_lisp_string_pieces(
    pieces: Vec<neovm_value::heap_types::LispString>,
) -> neovm_value::heap_types::LispString {
    let mut iter = pieces.into_iter();
    let Some(mut acc) = iter.next() else {
        return neovm_value::heap_types::LispString::from_unibyte(Vec::new());
    };
    for piece in iter {
        acc = acc.concat(&piece);
    }
    acc
}

fn empty_lisp_string(multibyte: bool) -> neovm_value::heap_types::LispString {
    if multibyte {
        neovm_value::heap_types::LispString::from_emacs_bytes(Vec::new())
    } else {
        neovm_value::heap_types::LispString::from_unibyte(Vec::new())
    }
}

fn lisp_char_at_byte(
    string: &neovm_value::heap_types::LispString,
    byte_pos: usize,
) -> Option<(u32, usize)> {
    if byte_pos >= string.byte_len() {
        return None;
    }
    if string.is_multibyte() {
        let (c, n) = neovm_value::emacs_char::string_char(&string.as_bytes()[byte_pos..]);
        Some((c.code(), n))
    } else {
        Some((string.as_bytes()[byte_pos] as u32, 1))
    }
}

fn match_group_to_byte_range(
    source: &neovm_value::heap_types::LispString,
    md: &super::regex::MatchData,
    group: usize,
) -> Option<MatchGroup> {
    let range = md.group_zero_based_char_range(group)?;
    Some(MatchGroup::new(
        super::regex::char_pos_to_byte_lisp_string(source, range.start().get()),
        super::regex::char_pos_to_byte_lisp_string(source, range.end().get()),
    ))
}

fn build_replacement_lisp_string(
    source: &neovm_value::heap_types::LispString,
    newtext: &neovm_value::heap_types::LispString,
    literal: bool,
    md: &super::regex::MatchData,
    // Which group `\&' stands for. GNU replaces the SUBEXP, so `\&' is the
    // text being replaced (`substart = sub_start' in the string branch,
    // src/search.c:2544-2547; `idx = sub' in the buffer branch, :2685-2686),
    // NOT group 0. `\1'..`\9' keep addressing the whole match's groups.
    subexp: usize,
    preserve_substitution_properties: bool,
    string_replacement: bool,
) -> Result<neovm_value::heap_types::LispString, String> {
    const INVALID_BACKSLASH_MSG: &str = "Invalid use of `\\' in replacement text";

    if literal || !newtext.as_bytes().contains(&b'\\') {
        return Ok(newtext.clone());
    }

    let mut pieces = Vec::new();
    let mut pos = 0usize;
    let mut last = 0usize;
    let len = newtext.byte_len();

    while pos < len {
        let Some((ch, ch_len)) = lisp_char_at_byte(newtext, pos) else {
            break;
        };
        let ch_start = pos;
        pos += ch_len;

        if ch != b'\\' as u32 {
            continue;
        }

        let Some((next, next_len)) = lisp_char_at_byte(newtext, pos) else {
            continue;
        };
        let next_start = pos;
        pos += next_len;

        match next {
            c if c == b'&' as u32 => {
                if ch_start != last {
                    pieces.push(
                        lisp_string_slice_for_replace_match(
                            newtext,
                            last,
                            ch_start,
                            preserve_substitution_properties,
                        )
                        .expect("validated replacement literal slice"),
                    );
                }
                if let Some(range) = match_group_to_byte_range(source, md, subexp) {
                    pieces.push(
                        lisp_string_slice_for_replace_match(
                            source,
                            range.start(),
                            range.end(),
                            preserve_substitution_properties,
                        )
                        .expect("validated whole-match replacement slice"),
                    );
                }
                last = pos;
            }
            c if (b'1' as u32..=b'9' as u32).contains(&c) => {
                if ch_start != last {
                    pieces.push(
                        lisp_string_slice_for_replace_match(
                            newtext,
                            last,
                            ch_start,
                            preserve_substitution_properties,
                        )
                        .expect("validated replacement literal slice"),
                    );
                }
                let group = (c as u8 - b'0') as usize;
                if let Some(range) = match_group_to_byte_range(source, md, group) {
                    pieces.push(
                        lisp_string_slice_for_replace_match(
                            source,
                            range.start(),
                            range.end(),
                            preserve_substitution_properties,
                        )
                        .expect("validated submatch replacement slice"),
                    );
                }
                last = pos;
            }
            c if c == b'\\' as u32 => {
                pieces.push(
                    lisp_string_slice_for_replace_match(
                        newtext,
                        last,
                        next_start,
                        preserve_substitution_properties,
                    )
                    .expect("validated escaped-backslash replacement slice"),
                );
                last = pos;
            }
            c if c == b'?' as u32 && string_replacement => {
                // GNU `Freplace_match` (search.c:2567) only tolerates `\?' as a
                // literal when replacing into a STRING (`else if (c != '?')`);
                // it is reserved for query-replace-regexp.  The buffer path
                // (search.c:2694) rejects it like any other invalid escape, so
                // we only whitelist it here when `string_replacement` is set.
            }
            _ => return Err(INVALID_BACKSLASH_MSG.to_string()),
        }
    }

    if last < len {
        pieces.push(
            lisp_string_slice_for_replace_match(
                newtext,
                last,
                len,
                preserve_substitution_properties,
            )
            .expect("validated trailing replacement slice"),
        );
    }

    if pieces.is_empty() {
        Ok(empty_lisp_string(
            source.is_multibyte() || newtext.is_multibyte(),
        ))
    } else {
        Ok(concat_lisp_string_pieces(pieces))
    }
}

fn lisp_string_slice_for_replace_match(
    string: &neovm_value::heap_types::LispString,
    start: usize,
    end: usize,
    preserve_properties: bool,
) -> Option<neovm_value::heap_types::LispString> {
    if preserve_properties {
        string.slice(start, end)
    } else {
        string.slice_no_properties(start, end)
    }
}

#[doc(hidden)]
pub fn replace_match_lisp_string_with_syntax(
    source: &neovm_value::heap_types::LispString,
    newtext: &neovm_value::heap_types::LispString,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<super::regex::MatchData>,
) -> Result<neovm_value::heap_types::LispString, String> {
    // Replacing into a STRING: `\?' is tolerated as a literal (search.c:2567).
    replace_match_lisp_string_with_syntax_and_properties(
        source, newtext, fixedcase, literal, subexp, match_data, true, true, None,
    )
}

#[allow(clippy::too_many_arguments)]
fn replace_match_lisp_string_with_syntax_and_properties(
    source: &neovm_value::heap_types::LispString,
    newtext: &neovm_value::heap_types::LispString,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<super::regex::MatchData>,
    preserve_substitution_properties: bool,
    string_replacement: bool,
    buffer_case_context: Option<(
        &crate::emacs_core::syntax::SyntaxTable,
        &crate::emacs_core::casetab::CaseTableOverride,
        neovm_value::tagged::SymbolMode,
    )>,
) -> Result<neovm_value::heap_types::LispString, String> {
    let md = match match_data {
        Some(md) => md,
        None => return Err(super::regex::REPLACE_MATCH_SUBEXP_MISSING.to_string()),
    };
    let Some(byte_range) = match_group_to_byte_range(source, md, subexp) else {
        return Err(super::regex::REPLACE_MATCH_SUBEXP_MISSING.to_string());
    };
    let byte_start = byte_range.start();
    let byte_end = byte_range.end();
    if byte_end > source.byte_len() || byte_start > byte_end {
        return Err(super::regex::REPLACE_MATCH_SUBEXP_MISSING.to_string());
    }

    let before = source
        .slice(0, byte_start)
        .expect("validated replace-match prefix slice");
    let after = source
        .slice(byte_end, source.byte_len())
        .expect("validated replace-match suffix slice");
    let mut replacement = build_replacement_lisp_string(
        source,
        newtext,
        literal,
        md,
        subexp,
        preserve_substitution_properties,
        string_replacement,
    )?;

    if !fixedcase {
        let matched = source
            .slice(byte_start, byte_end)
            .expect("validated replace-match matched slice");
        // Issue #131: case-preserve over the real Emacs char codes of the
        // matched/replacement LispStrings instead of round-tripping through the
        // PUA-sentinel storage form, so eight-bit bytes and PUA glyphs are not
        // confused.
        let mut cased = if let Some((syntax_table, casetab, mode)) = buffer_case_context {
            // GNU `Freplace_match` case analysis uses the buffer's case table
            // (UPPERCASEP/LOWERCASEP) and syntax table (word boundaries), so a
            // custom case table (set-case-syntax-pair) capitalizes accordingly.
            crate::emacs_core::casefiddle::apply_replace_match_case_lisp_cased(
                &replacement,
                &matched,
                |ch| {
                    syntax_table.char_syntax_with_symbols(ch, mode)
                        == crate::emacs_core::syntax::SyntaxClass::Word
                },
                |ch| casetab.is_upper(ch),
                |ch| casetab.is_lower(ch),
                casetab,
            )
        } else {
            crate::emacs_core::casefiddle::apply_replace_match_case_lisp(&replacement, &matched)
        };
        if cased.schars() == replacement.schars() {
            *cased.intervals_mut() = replacement.intervals().to_owned_table();
        }
        replacement = cased;
    }

    Ok(concat_lisp_string_pieces(vec![before, replacement, after]))
}

#[doc(hidden)]
pub fn compute_buffer_replacement_lisp_string(
    buf: &neovm_edit::buffer::Buffer,
    newtext: &neovm_value::heap_types::LispString,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<super::regex::MatchData>,
    mode: neovm_value::tagged::SymbolMode,
) -> Result<(usize, usize, neovm_value::heap_types::LispString), String> {
    let md = match match_data {
        Some(md) => md,
        None => return Err(super::regex::REPLACE_MATCH_SUBEXP_MISSING.to_string()),
    };
    if md.group(subexp).is_none() {
        return Err(super::regex::REPLACE_MATCH_SUBEXP_MISSING.to_string());
    }

    // Every group here is read as a Lisp buffer position, whatever the match
    // data records as its origin.  GNU's `Freplace_match' (search.c:2396)
    // decides between buffer and string purely from its STRING argument and
    // never consults `last_thing_searched', so a match installed by
    // `set-match-data' from plain integers -- which both editors record as
    // string-sourced, since no element names a buffer (search.c:2966) -- still
    // replaces in the current buffer.
    let group = md.group(subexp).expect("group existence checked above");
    let buffer_start = buffer_match_char_pos_to_byte_pos(buf, group.start()).get();
    let buffer_end = buffer_match_char_pos_to_byte_pos(buf, group.end()).get();

    // GNU `replace_match` reads only the matched text from the buffer; it never
    // copies the whole buffer.  Materialize just group 0 -- which spans the
    // whole match and therefore every subgroup -- and re-base the group indices
    // onto it, so this is O(match) rather than O(buffer).  The whole-buffer copy
    // made a replace loop (query-replace / replace-string) O(n^2).
    let group0 = md
        .group(0)
        .ok_or_else(|| super::regex::REPLACE_MATCH_SUBEXP_MISSING.to_string())?;
    let group0_start = group0.start();
    let group0_byte_start = buffer_match_char_pos_to_byte_pos(buf, group0_start);
    let group0_byte_end = buffer_match_char_pos_to_byte_pos(buf, group0.end());
    let source = buf.buffer_substring_lisp_string_range(neovm_edit::buffer::EmacsByteRange::new(
        group0_byte_start,
        group0_byte_end,
    ));
    // Group positions are 1-based buffer chars; re-base them to 0-based
    // offsets within `source` (whose char 0 is the first char of group 0).
    let replacement_match_data = super::regex::MatchData::string(
        md.groups_snapshot()
            .into_iter()
            .map(|group| group.map(|group| group.saturating_sub(group0_start)))
            .collect(),
        Some(super::regex::SearchedString::Owned(source.clone())),
    );

    let replacement_match_option = Some(replacement_match_data.clone());
    // The buffer's case + syntax tables drive GNU's case analysis, so pass them
    // down (e.g. set-case-syntax-pair makes a char an uppercase word constituent).
    let buffer_syntax_table = crate::emacs_core::syntax::SyntaxTable::for_buffer(buf);
    let buffer_case_override =
        crate::emacs_core::casetab::CaseTableOverride::for_buffer_readonly_with_symbols(buf, mode);
    // Replacing into a BUFFER: `\?' is an invalid escape (search.c:2694).
    let replacement = replace_match_lisp_string_with_syntax_and_properties(
        &source,
        newtext,
        fixedcase,
        literal,
        subexp,
        &replacement_match_option,
        false,
        false,
        Some((&buffer_syntax_table, &buffer_case_override, mode)),
    )?;
    let replace_start = if replacement_match_data.source().is_string() {
        super::regex::char_pos_to_byte_lisp_string(
            &source,
            replacement_match_data
                .group(subexp)
                .map(|group| group.start())
                .unwrap_or(0),
        )
    } else {
        buffer_start
    };
    let replace_end = if replacement_match_data.source().is_string() {
        super::regex::char_pos_to_byte_lisp_string(
            &source,
            replacement_match_data
                .group(subexp)
                .map(|group| group.end())
                .unwrap_or(0),
        )
    } else {
        buffer_end
    };
    let replacement_only = replacement
        .slice(
            replace_start,
            replacement
                .byte_len()
                .saturating_sub(source.byte_len().saturating_sub(replace_end)),
        )
        .expect("computed replacement slice is within replacement string");

    Ok((buffer_start, buffer_end, replacement_only))
}

/// Test-only: production `replace-regexp-in-string` is the subr.el Lisp
/// definition (which goes through the syntax-carrying `string-match`
/// builtin); this Rust port survives for its unit tests.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn replace_regexp_in_string_lisp<F>(
    args: &[Value],
    case_fold: bool,
    mut replacement_for_match: F,
) -> EvalResult
where
    F: FnMut(
        &neovm_value::heap_types::LispString,
        &Option<super::regex::MatchData>,
    ) -> Result<neovm_value::heap_types::LispString, Flow>,
{
    let pattern = expect_lisp_string(&args[0])?;
    let source = expect_lisp_string(&args[2])?;
    let parsed = parse_replace_regexp_subexp_start_lisp(args, source)?;
    let start = parsed.start;
    let mut cursor = start;
    let mut search_at = start;
    let mut pieces = Vec::new();
    let mut match_data = None;
    let total_chars = source.schars();

    // GNU `replace-regexp-in-string` searches the original Lisp string,
    // translates match data onto the matched substring, then runs
    // `replace-match` semantics on that substring.
    while search_at < source.byte_len() {
        let found = super::regex::string_match_full_with_case_fold_source_lisp_pattern_posix(
            pattern,
            source,
            super::regex::SearchedString::Heap(args[2]),
            search_at,
            case_fold,
            false,
            &mut match_data,
        )
        .map_err(|msg| signal(LispCondition::InvalidRegexp, vec![Value::string(msg)]))?;
        if found.is_none() {
            break;
        }

        let Some(current_md) = match_data.clone() else {
            break;
        };
        let Some(full_group) = current_md.group(0) else {
            break;
        };
        let full_start_char = full_group.start();
        let full_end_char = full_group.end();

        let match_span_end_char = if full_start_char == full_end_char {
            (full_start_char + 1).min(total_chars)
        } else {
            full_end_char
        };
        let full_start_byte = super::regex::char_pos_to_byte_lisp_string(source, full_start_char);
        let match_span_end_byte =
            super::regex::char_pos_to_byte_lisp_string(source, match_span_end_char);

        pieces.push(
            source
                .slice(cursor, full_start_byte)
                .expect("validated match prefix must slice"),
        );

        let match_span = source
            .slice(full_start_byte, match_span_end_byte)
            .expect("validated match span must slice");
        let translated_md = Some(translate_match_data_to_substring(
            &current_md,
            -(full_start_char as i64),
            super::regex::SearchedString::Owned(match_span.clone()),
        ));
        pieces.push(replacement_for_match(&match_span, &translated_md)?);
        cursor = match_span_end_byte;
        search_at = match_span_end_byte;
    }

    pieces.push(
        source
            .slice(cursor, source.byte_len())
            .expect("validated match tail must slice"),
    );
    Ok(Value::heap_string(concat_lisp_string_pieces(pieces)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
