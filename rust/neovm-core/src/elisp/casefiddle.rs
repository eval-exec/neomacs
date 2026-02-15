//! Case conversion and character builtins.
//!
//! Implements `upcase`, `downcase`, `capitalize`, `upcase-initials`,
//! `characterp`, `char-or-string-p`, `max-char`, `char-width`,
//! `string-width`, `unibyte-char-to-multibyte`, `multibyte-char-to-unibyte`,
//! `char-resolve-modifiers`, `get-byte`, and region/word case stubs.

use super::error::{signal, EvalResult, Flow};
use super::value::*;

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

fn expect_min_max_args(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), Flow> {
    if args.len() < min || args.len() > max {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Character helpers
// ---------------------------------------------------------------------------

/// Maximum Emacs internal character code (includes raw bytes).
const MAX_CHAR: i64 = 0x3FFFFF;
/// Maximum Unicode code point.
const MAX_UNICODE: i64 = 0x10FFFF;

const CHAR_META: i64 = 0x8000000;
const CHAR_CTL: i64 = 0x4000000;
const CHAR_SHIFT: i64 = 0x2000000;
const CHAR_HYPER: i64 = 0x1000000;
const CHAR_SUPER: i64 = 0x0800000;
const CHAR_ALT: i64 = 0x0400000;
const CHAR_MODIFIER_MASK: i64 =
    CHAR_META | CHAR_CTL | CHAR_SHIFT | CHAR_HYPER | CHAR_SUPER | CHAR_ALT;

/// Return true if `code` is a valid Emacs character code.
fn is_valid_char(code: i64) -> bool {
    (0..=MAX_CHAR).contains(&code)
}

/// Extract a character code from a Value, accepting both Char and Int.
fn extract_char_code(val: &Value) -> Option<i64> {
    match val {
        Value::Char(c) => Some(*c as i64),
        Value::Int(n) => Some(*n),
        _ => None,
    }
}

/// Convert a character code to a Rust char (if it's a valid Unicode scalar value).
fn code_to_char(code: i64) -> Option<char> {
    if code >= 0 && code <= 0x10FFFF {
        char::from_u32(code as u32)
    } else {
        None
    }
}

/// Return the display width of a single character.
/// CJK fullwidth and wide characters return 2; most others return 1.
fn char_display_width(c: char) -> usize {
    let cp = c as u32;
    if is_cjk_fullwidth(cp) {
        2
    } else {
        1
    }
}

/// Check whether a Unicode code point is CJK fullwidth / wide.
///
/// Uses the Unicode East Asian Width ranges specified in the task:
/// U+1100-U+115F, U+2E80-U+A4CF, U+F900-U+FAFF, U+FE10-U+FE6F,
/// U+FF01-U+FF60, U+FFE0-U+FFE6, U+20000-U+2FA1F.
fn is_cjk_fullwidth(cp: u32) -> bool {
    matches!(cp,
        0x1100..=0x115F
        | 0x2E80..=0xA4CF
        | 0xF900..=0xFAFF
        | 0xFE10..=0xFE6F
        | 0xFF01..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x20000..=0x2FA1F
    )
}

// ---------------------------------------------------------------------------
// Case conversion helpers
// ---------------------------------------------------------------------------

/// Uppercase a single character code, returning the new code.
fn upcase_char(code: i64) -> i64 {
    match code_to_char(code) {
        Some(c) => {
            let mut upper = c.to_uppercase();
            // to_uppercase() may yield multiple chars (e.g. German eszett);
            // take only the first to stay consistent with Emacs behavior.
            upper.next().map(|u| u as i64).unwrap_or(code)
        }
        None => code,
    }
}

/// Lowercase a single character code, returning the new code.
fn downcase_char(code: i64) -> i64 {
    match code_to_char(code) {
        Some(c) => {
            let mut lower = c.to_lowercase();
            lower.next().map(|l| l as i64).unwrap_or(code)
        }
        None => code,
    }
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// `(upcase OBJ)` -- if OBJ is a string, return uppercased copy.
/// If OBJ is a character (Int or Char), return uppercased character code.
pub(crate) fn builtin_upcase(args: Vec<Value>) -> EvalResult {
    expect_args("upcase", &args, 1)?;
    match &args[0] {
        Value::Str(s) => {
            let upper: String = s.to_uppercase();
            Ok(Value::string(upper))
        }
        Value::Char(c) => {
            let code = *c as i64;
            Ok(Value::Int(upcase_char(code)))
        }
        Value::Int(n) => Ok(Value::Int(upcase_char(*n))),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("char-or-string-p"), other.clone()],
        )),
    }
}

/// `(downcase OBJ)` -- if OBJ is a string, return lowercased copy.
/// If OBJ is a character (Int or Char), return lowercased character code.
pub(crate) fn builtin_downcase(args: Vec<Value>) -> EvalResult {
    expect_args("downcase", &args, 1)?;
    match &args[0] {
        Value::Str(s) => {
            let lower: String = s.to_lowercase();
            Ok(Value::string(lower))
        }
        Value::Char(c) => {
            let code = *c as i64;
            Ok(Value::Int(downcase_char(code)))
        }
        Value::Int(n) => Ok(Value::Int(downcase_char(*n))),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("char-or-string-p"), other.clone()],
        )),
    }
}

/// `(capitalize OBJ)` -- if OBJ is a string, capitalize the first letter
/// (uppercase first, lowercase rest).  If OBJ is a character, uppercase it.
pub(crate) fn builtin_capitalize(args: Vec<Value>) -> EvalResult {
    expect_args("capitalize", &args, 1)?;
    match &args[0] {
        Value::Str(s) => {
            let capitalized = capitalize_string(s);
            Ok(Value::string(capitalized))
        }
        Value::Char(c) => {
            let code = *c as i64;
            Ok(Value::Int(upcase_char(code)))
        }
        Value::Int(n) => Ok(Value::Int(upcase_char(*n))),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("char-or-string-p"), other.clone()],
        )),
    }
}

/// Capitalize a string: uppercase the first letter of each word,
/// lowercase the rest.  A "word" starts after any non-alphanumeric character.
fn capitalize_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut new_word = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if new_word {
                for u in c.to_uppercase() {
                    result.push(u);
                }
                new_word = false;
            } else {
                for l in c.to_lowercase() {
                    result.push(l);
                }
            }
        } else {
            result.push(c);
            new_word = true;
        }
    }
    result
}

/// `(upcase-initials OBJ)` -- uppercase the first letter of each word in
/// a string, leaving the rest unchanged.  For a char, uppercase it.
pub(crate) fn builtin_upcase_initials(args: Vec<Value>) -> EvalResult {
    expect_args("upcase-initials", &args, 1)?;
    match &args[0] {
        Value::Str(s) => {
            let result = upcase_initials_string(s);
            Ok(Value::string(result))
        }
        Value::Char(c) => {
            let code = *c as i64;
            Ok(Value::Int(upcase_char(code)))
        }
        Value::Int(n) => Ok(Value::Int(upcase_char(*n))),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("char-or-string-p"), other.clone()],
        )),
    }
}

/// Uppercase the first letter of each word, leaving the rest unchanged.
fn upcase_initials_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut new_word = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if new_word {
                for u in c.to_uppercase() {
                    result.push(u);
                }
                new_word = false;
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
            new_word = true;
        }
    }
    result
}

/// `(characterp OBJECT &optional IGNORE)` -- return t if OBJECT is a character.
/// A character is a `Value::Char` or a `Value::Int` in the range 0..=0x10FFFF.
pub(crate) fn builtin_characterp(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("characterp", &args, 1, 2)?;
    let is_char = match &args[0] {
        Value::Char(_) => true,
        Value::Int(n) => (0..=MAX_UNICODE).contains(n),
        _ => false,
    };
    Ok(Value::bool(is_char))
}

/// `(char-or-string-p OBJECT)` -- return t if OBJECT is a character or string.
pub(crate) fn builtin_char_or_string_p(args: Vec<Value>) -> EvalResult {
    expect_args("char-or-string-p", &args, 1)?;
    let ok = match &args[0] {
        Value::Char(_) => true,
        Value::Int(n) => (0..=MAX_UNICODE).contains(n),
        Value::Str(_) => true,
        _ => false,
    };
    Ok(Value::bool(ok))
}

/// `(max-char &optional UNICODE)` -- return max character code.
/// If UNICODE is non-nil, return 0x10FFFF; otherwise return 0x3FFFFF.
pub(crate) fn builtin_max_char(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("max-char", &args, 0, 1)?;
    let unicode = args.first().map(|v| v.is_truthy()).unwrap_or(false);
    if unicode {
        Ok(Value::Int(MAX_UNICODE))
    } else {
        Ok(Value::Int(MAX_CHAR))
    }
}

/// `(char-width CHAR)` -- return display width of CHAR.
/// Most characters have width 1; CJK fullwidth characters have width 2.
pub(crate) fn builtin_char_width(args: Vec<Value>) -> EvalResult {
    expect_args("char-width", &args, 1)?;
    let code = extract_char_code(&args[0]).ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("characterp"), args[0].clone()],
        )
    })?;
    if !is_valid_char(code) {
        return Err(signal(
            "args-out-of-range",
            vec![args[0].clone(), Value::Int(0), Value::Int(MAX_CHAR)],
        ));
    }
    let width = match code_to_char(code) {
        Some(c) => char_display_width(c) as i64,
        None => 1, // non-Unicode Emacs internal chars default to width 1
    };
    Ok(Value::Int(width))
}

/// `(string-width STRING &optional FROM TO)` -- return total display width.
pub(crate) fn builtin_string_width(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("string-width", &args, 1, 3)?;
    let s = match &args[0] {
        Value::Str(s) => s.clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ));
        }
    };

    // Collect chars for index-based slicing.
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;

    let from = if args.len() >= 2 {
        match &args[1] {
            Value::Nil => 0i64,
            Value::Int(n) => {
                if *n < 0 || *n > len {
                    return Err(signal(
                        "args-out-of-range",
                        vec![args[0].clone(), args[1].clone()],
                    ));
                }
                *n
            }
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), other.clone()],
                ));
            }
        }
    } else {
        0
    };

    let to = if args.len() >= 3 {
        match &args[2] {
            Value::Nil => len,
            Value::Int(n) => {
                if *n < from || *n > len {
                    return Err(signal(
                        "args-out-of-range",
                        vec![args[0].clone(), args[2].clone()],
                    ));
                }
                *n
            }
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), other.clone()],
                ));
            }
        }
    } else {
        len
    };

    let width: usize = chars[from as usize..to as usize]
        .iter()
        .map(|c| char_display_width(*c))
        .sum();

    Ok(Value::Int(width as i64))
}

/// `(unibyte-char-to-multibyte CH)` -- convert unibyte char to multibyte.
/// For chars < 128, identity.  For 128..=255, map to Emacs raw byte range.
pub(crate) fn builtin_unibyte_char_to_multibyte(args: Vec<Value>) -> EvalResult {
    expect_args("unibyte-char-to-multibyte", &args, 1)?;
    let code = extract_char_code(&args[0]).ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("characterp"), args[0].clone()],
        )
    })?;
    if code < 0 || code > 255 {
        return Err(signal(
            "args-out-of-range",
            vec![args[0].clone(), Value::Int(0), Value::Int(255)],
        ));
    }
    if code < 128 {
        Ok(Value::Int(code))
    } else {
        // Emacs maps unibyte 128-255 to raw-byte range 0x3FFF80-0x3FFFFF
        Ok(Value::Int(code + 0x3FFF00))
    }
}

/// `(multibyte-char-to-unibyte CH)` -- convert multibyte char to unibyte.
/// Returns the character masked to 0xFF.
pub(crate) fn builtin_multibyte_char_to_unibyte(args: Vec<Value>) -> EvalResult {
    expect_args("multibyte-char-to-unibyte", &args, 1)?;
    let code = extract_char_code(&args[0]).ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("characterp"), args[0].clone()],
        )
    })?;
    if !is_valid_char(code) {
        return Err(signal(
            "args-out-of-range",
            vec![args[0].clone(), Value::Int(0), Value::Int(MAX_CHAR)],
        ));
    }
    // Raw byte range 0x3FFF80..=0x3FFFFF maps back to 128..255
    if code >= 0x3FFF80 {
        Ok(Value::Int(code - 0x3FFF00))
    } else {
        Ok(Value::Int(code & 0xFF))
    }
}

/// `(char-resolve-modifiers CHAR)` -- resolve modifier bits in character.
/// Resolve shift/control modifiers into the base character where possible.
pub(crate) fn builtin_char_resolve_modifiers(args: Vec<Value>) -> EvalResult {
    expect_args("char-resolve-modifiers", &args, 1)?;

    let code = match &args[0] {
        Value::Int(n) => *n,
        Value::Char(c) => *c as i64,
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("fixnump"), other.clone()],
            ))
        }
    };

    let modifiers = code & CHAR_MODIFIER_MASK;
    let mut base = code & !CHAR_MODIFIER_MASK;
    let mut remaining_mods = modifiers;

    if remaining_mods & CHAR_SHIFT != 0 {
        if base >= 'a' as i64 && base <= 'z' as i64 {
            base = base - 'a' as i64 + 'A' as i64;
            remaining_mods &= !CHAR_SHIFT;
        } else if base >= 'A' as i64 && base <= 'Z' as i64 {
            remaining_mods &= !CHAR_SHIFT;
        }
    }

    if remaining_mods & CHAR_CTL != 0 {
        if base >= '@' as i64 && base <= '_' as i64 {
            base &= 0x1F;
            remaining_mods &= !CHAR_CTL;
        } else if base >= 'a' as i64 && base <= 'z' as i64 {
            base &= 0x1F;
            remaining_mods &= !CHAR_CTL;
        } else if base == '?' as i64 {
            base = 127;
            remaining_mods &= !CHAR_CTL;
        }
    }

    Ok(Value::Int(base | remaining_mods))
}

/// `(get-byte &optional POSITION STRING)` -- return byte at position in string.
/// Stub: return 0.
pub(crate) fn builtin_get_byte(args: Vec<Value>) -> EvalResult {
    expect_min_max_args("get-byte", &args, 0, 2)?;
    Ok(Value::Int(0))
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins (stubs)
// ---------------------------------------------------------------------------

/// `(upcase-region BEG END &optional REGION-NONCONTIGUOUS-P)` -- stub, return nil.
pub(crate) fn builtin_upcase_region(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_max_args("upcase-region", &args, 2, 3)?;
    Ok(Value::Nil)
}

/// `(downcase-region BEG END &optional REGION-NONCONTIGUOUS-P)` -- stub, return nil.
pub(crate) fn builtin_downcase_region(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_max_args("downcase-region", &args, 2, 3)?;
    Ok(Value::Nil)
}

/// `(capitalize-region BEG END &optional REGION-NONCONTIGUOUS-P)` -- stub, return nil.
pub(crate) fn builtin_capitalize_region(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_max_args("capitalize-region", &args, 2, 3)?;
    Ok(Value::Nil)
}

/// `(upcase-initials-region BEG END)` -- stub, return nil.
pub(crate) fn builtin_upcase_initials_region(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("upcase-initials-region", &args, 2)?;
    Ok(Value::Nil)
}

/// `(upcase-word ARG)` -- stub, return nil.
pub(crate) fn builtin_upcase_word(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("upcase-word", &args, 1)?;
    Ok(Value::Nil)
}

/// `(downcase-word ARG)` -- stub, return nil.
pub(crate) fn builtin_downcase_word(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("downcase-word", &args, 1)?;
    Ok(Value::Nil)
}

/// `(capitalize-word ARG)` -- stub, return nil.
pub(crate) fn builtin_capitalize_word(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("capitalize-word", &args, 1)?;
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // =======================================================================
    // upcase
    // =======================================================================

    #[test]
    fn upcase_string() {
        let result = builtin_upcase(vec![Value::string("hello")]).unwrap();
        assert_eq!(result.as_str(), Some("HELLO"));
    }

    #[test]
    fn upcase_string_mixed() {
        let result = builtin_upcase(vec![Value::string("Hello World 123")]).unwrap();
        assert_eq!(result.as_str(), Some("HELLO WORLD 123"));
    }

    #[test]
    fn upcase_char() {
        let result = builtin_upcase(vec![Value::Char('a')]).unwrap();
        assert_eq!(result.as_int(), Some('A' as i64));
    }

    #[test]
    fn upcase_int() {
        let result = builtin_upcase(vec![Value::Int('z' as i64)]).unwrap();
        assert_eq!(result.as_int(), Some('Z' as i64));
    }

    #[test]
    fn upcase_wrong_type() {
        let result = builtin_upcase(vec![Value::Float(1.0)]);
        assert!(result.is_err());
    }

    // =======================================================================
    // downcase
    // =======================================================================

    #[test]
    fn downcase_string() {
        let result = builtin_downcase(vec![Value::string("HELLO")]).unwrap();
        assert_eq!(result.as_str(), Some("hello"));
    }

    #[test]
    fn downcase_char() {
        let result = builtin_downcase(vec![Value::Char('A')]).unwrap();
        assert_eq!(result.as_int(), Some('a' as i64));
    }

    #[test]
    fn downcase_int() {
        let result = builtin_downcase(vec![Value::Int('Z' as i64)]).unwrap();
        assert_eq!(result.as_int(), Some('z' as i64));
    }

    // =======================================================================
    // capitalize
    // =======================================================================

    #[test]
    fn capitalize_string_basic() {
        let result = builtin_capitalize(vec![Value::string("hello world")]).unwrap();
        assert_eq!(result.as_str(), Some("Hello World"));
    }

    #[test]
    fn capitalize_string_mixed() {
        let result = builtin_capitalize(vec![Value::string("hELLO wORLD")]).unwrap();
        assert_eq!(result.as_str(), Some("Hello World"));
    }

    #[test]
    fn capitalize_char() {
        let result = builtin_capitalize(vec![Value::Char('a')]).unwrap();
        assert_eq!(result.as_int(), Some('A' as i64));
    }

    #[test]
    fn capitalize_empty_string() {
        let result = builtin_capitalize(vec![Value::string("")]).unwrap();
        assert_eq!(result.as_str(), Some(""));
    }

    // =======================================================================
    // upcase-initials
    // =======================================================================

    #[test]
    fn upcase_initials_basic() {
        let result = builtin_upcase_initials(vec![Value::string("hello world")]).unwrap();
        assert_eq!(result.as_str(), Some("Hello World"));
    }

    #[test]
    fn upcase_initials_preserves_rest() {
        let result = builtin_upcase_initials(vec![Value::string("hELLO wORLD")]).unwrap();
        // Only first letter of each word is uppercased; rest is left alone.
        assert_eq!(result.as_str(), Some("HELLO WORLD"));
    }

    #[test]
    fn upcase_initials_char() {
        let result = builtin_upcase_initials(vec![Value::Char('a')]).unwrap();
        assert_eq!(result.as_int(), Some('A' as i64));
    }

    // =======================================================================
    // characterp
    // =======================================================================

    #[test]
    fn characterp_char() {
        let result = builtin_characterp(vec![Value::Char('a')]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn characterp_int_valid() {
        let result = builtin_characterp(vec![Value::Int(65)]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn characterp_int_max_unicode() {
        let result = builtin_characterp(vec![Value::Int(0x10FFFF)]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn characterp_int_too_large() {
        let result = builtin_characterp(vec![Value::Int(0x110000)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn characterp_int_negative() {
        let result = builtin_characterp(vec![Value::Int(-1)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn characterp_string() {
        let result = builtin_characterp(vec![Value::string("a")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn characterp_with_ignore() {
        let result = builtin_characterp(vec![Value::Char('a'), Value::Nil]).unwrap();
        assert!(result.is_truthy());
    }

    // =======================================================================
    // char-or-string-p
    // =======================================================================

    #[test]
    fn char_or_string_p_char() {
        let result = builtin_char_or_string_p(vec![Value::Char('x')]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn char_or_string_p_string() {
        let result = builtin_char_or_string_p(vec![Value::string("hello")]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn char_or_string_p_int_valid() {
        let result = builtin_char_or_string_p(vec![Value::Int(65)]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn char_or_string_p_nil() {
        let result = builtin_char_or_string_p(vec![Value::Nil]).unwrap();
        assert!(result.is_nil());
    }

    // =======================================================================
    // max-char
    // =======================================================================

    #[test]
    fn max_char_default() {
        let result = builtin_max_char(vec![]).unwrap();
        assert_eq!(result.as_int(), Some(0x3FFFFF));
    }

    #[test]
    fn max_char_nil() {
        let result = builtin_max_char(vec![Value::Nil]).unwrap();
        assert_eq!(result.as_int(), Some(0x3FFFFF));
    }

    #[test]
    fn max_char_unicode() {
        let result = builtin_max_char(vec![Value::True]).unwrap();
        assert_eq!(result.as_int(), Some(0x10FFFF));
    }

    // =======================================================================
    // char-width
    // =======================================================================

    #[test]
    fn char_width_ascii() {
        let result = builtin_char_width(vec![Value::Int('A' as i64)]).unwrap();
        assert_eq!(result.as_int(), Some(1));
    }

    #[test]
    fn char_width_cjk() {
        // U+4E2D = Chinese character (in 0x2E80..0xA4CF range)
        let result = builtin_char_width(vec![Value::Int(0x4E2D)]).unwrap();
        assert_eq!(result.as_int(), Some(2));
    }

    #[test]
    fn char_width_fullwidth_latin() {
        // U+FF21 = fullwidth 'A' (in 0xFF01..0xFF60 range)
        let result = builtin_char_width(vec![Value::Int(0xFF21)]).unwrap();
        assert_eq!(result.as_int(), Some(2));
    }

    #[test]
    fn char_width_hangul() {
        // U+1100 = Hangul Choseong Kiyeok (start of 0x1100..0x115F)
        let result = builtin_char_width(vec![Value::Int(0x1100)]).unwrap();
        assert_eq!(result.as_int(), Some(2));
    }

    #[test]
    fn char_width_wrong_type() {
        let result = builtin_char_width(vec![Value::string("a")]);
        assert!(result.is_err());
    }

    // =======================================================================
    // string-width
    // =======================================================================

    #[test]
    fn string_width_ascii() {
        let result = builtin_string_width(vec![Value::string("hello")]).unwrap();
        assert_eq!(result.as_int(), Some(5));
    }

    #[test]
    fn string_width_empty() {
        let result = builtin_string_width(vec![Value::string("")]).unwrap();
        assert_eq!(result.as_int(), Some(0));
    }

    #[test]
    fn string_width_with_cjk() {
        // 2 CJK chars + 1 ASCII = 2*2 + 1 = 5
        let s = "\u{4E2D}\u{6587}a";
        let result = builtin_string_width(vec![Value::string(s)]).unwrap();
        assert_eq!(result.as_int(), Some(5));
    }

    #[test]
    fn string_width_with_range() {
        let result =
            builtin_string_width(vec![Value::string("hello"), Value::Int(1), Value::Int(3)])
                .unwrap();
        // chars[1..3] = "el" = width 2
        assert_eq!(result.as_int(), Some(2));
    }

    #[test]
    fn string_width_wrong_type() {
        let result = builtin_string_width(vec![Value::Int(42)]);
        assert!(result.is_err());
    }

    // =======================================================================
    // unibyte-char-to-multibyte
    // =======================================================================

    #[test]
    fn unibyte_to_multibyte_ascii() {
        let result = builtin_unibyte_char_to_multibyte(vec![Value::Int(65)]).unwrap();
        assert_eq!(result.as_int(), Some(65));
    }

    #[test]
    fn unibyte_to_multibyte_high() {
        let result = builtin_unibyte_char_to_multibyte(vec![Value::Int(200)]).unwrap();
        assert_eq!(result.as_int(), Some(200 + 0x3FFF00));
    }

    #[test]
    fn unibyte_to_multibyte_out_of_range() {
        let result = builtin_unibyte_char_to_multibyte(vec![Value::Int(256)]);
        assert!(result.is_err());
    }

    // =======================================================================
    // multibyte-char-to-unibyte
    // =======================================================================

    #[test]
    fn multibyte_to_unibyte_ascii() {
        let result = builtin_multibyte_char_to_unibyte(vec![Value::Int(65)]).unwrap();
        assert_eq!(result.as_int(), Some(65));
    }

    #[test]
    fn multibyte_to_unibyte_raw_byte() {
        let result = builtin_multibyte_char_to_unibyte(vec![Value::Int(0x3FFF80 + 50)]).unwrap();
        assert_eq!(result.as_int(), Some(128 + 50));
    }

    #[test]
    fn multibyte_to_unibyte_mask() {
        // Non-raw multibyte: mask to 0xFF
        let result = builtin_multibyte_char_to_unibyte(vec![Value::Int(0x100)]).unwrap();
        assert_eq!(result.as_int(), Some(0));
    }

    // =======================================================================
    // char-resolve-modifiers
    // =======================================================================

    #[test]
    fn char_resolve_modifiers_resolves_shift_lowercase() {
        let result = builtin_char_resolve_modifiers(vec![Value::Int(0x2000000 | ('a' as i64))])
            .unwrap();
        assert_eq!(result.as_int(), Some('A' as i64));
    }

    #[test]
    fn char_resolve_modifiers_clears_shift_on_uppercase() {
        let result = builtin_char_resolve_modifiers(vec![Value::Int(0x2000000 | ('A' as i64))])
            .unwrap();
        assert_eq!(result.as_int(), Some('A' as i64));
    }

    #[test]
    fn char_resolve_modifiers_wrong_type_predicate() {
        let result = builtin_char_resolve_modifiers(vec![Value::string("a")]).unwrap_err();
        match result {
            super::super::error::Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("fixnump"), Value::string("a")]);
            }
            other => panic!("expected signal flow, got {other:?}"),
        }
    }

    // =======================================================================
    // get-byte
    // =======================================================================

    #[test]
    fn get_byte_stub() {
        let result = builtin_get_byte(vec![]).unwrap();
        assert_eq!(result.as_int(), Some(0));
    }

    #[test]
    fn get_byte_with_args() {
        let result = builtin_get_byte(vec![Value::Int(0), Value::string("hello")]).unwrap();
        assert_eq!(result.as_int(), Some(0));
    }

    // =======================================================================
    // Eval-dependent stubs
    // =======================================================================

    #[test]
    fn upcase_region_stub() {
        let mut eval = super::super::eval::Evaluator::new();
        let result = builtin_upcase_region(&mut eval, vec![Value::Int(1), Value::Int(10)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn downcase_region_stub() {
        let mut eval = super::super::eval::Evaluator::new();
        let result =
            builtin_downcase_region(&mut eval, vec![Value::Int(1), Value::Int(10)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn capitalize_region_stub() {
        let mut eval = super::super::eval::Evaluator::new();
        let result =
            builtin_capitalize_region(&mut eval, vec![Value::Int(1), Value::Int(10)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn upcase_initials_region_stub() {
        let mut eval = super::super::eval::Evaluator::new();
        let result =
            builtin_upcase_initials_region(&mut eval, vec![Value::Int(1), Value::Int(10)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn upcase_word_stub() {
        let mut eval = super::super::eval::Evaluator::new();
        let result = builtin_upcase_word(&mut eval, vec![Value::Int(1)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn downcase_word_stub() {
        let mut eval = super::super::eval::Evaluator::new();
        let result = builtin_downcase_word(&mut eval, vec![Value::Int(1)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn capitalize_word_stub() {
        let mut eval = super::super::eval::Evaluator::new();
        let result = builtin_capitalize_word(&mut eval, vec![Value::Int(1)]).unwrap();
        assert!(result.is_nil());
    }

    // =======================================================================
    // Edge cases
    // =======================================================================

    #[test]
    fn upcase_unicode() {
        let result = builtin_upcase(vec![Value::string("\u{00E9}")]).unwrap(); // e-acute
        assert_eq!(result.as_str(), Some("\u{00C9}")); // E-acute
    }

    #[test]
    fn downcase_unicode() {
        let result = builtin_downcase(vec![Value::string("\u{00C9}")]).unwrap(); // E-acute
        assert_eq!(result.as_str(), Some("\u{00E9}")); // e-acute
    }

    #[test]
    fn capitalize_with_punctuation() {
        let result = builtin_capitalize(vec![Value::string("it's a test")]).unwrap();
        assert_eq!(result.as_str(), Some("It'S A Test"));
    }

    #[test]
    fn string_width_nil_from() {
        let result = builtin_string_width(vec![Value::string("hello"), Value::Nil]).unwrap();
        assert_eq!(result.as_int(), Some(5));
    }

    #[test]
    fn string_width_nil_to() {
        let result =
            builtin_string_width(vec![Value::string("hello"), Value::Int(0), Value::Nil]).unwrap();
        assert_eq!(result.as_int(), Some(5));
    }

    #[test]
    fn wrong_arity_upcase() {
        let result = builtin_upcase(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_arity_downcase() {
        let result = builtin_downcase(vec![Value::string("a"), Value::string("b")]);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_arity_char_width() {
        let result = builtin_char_width(vec![]);
        assert!(result.is_err());
    }
}
