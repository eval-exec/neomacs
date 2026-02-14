//! Charset builtins for the Elisp interpreter.
//!
//! Charsets in Emacs define sets of characters with encoding properties.
//! For neovm we primarily support Unicode; other charsets are registered
//! for compatibility but map through to the Unicode code-point space.
//!
//! The `CharsetRegistry` stores known charset names and basic properties.
//! It is initialized with the standard charsets: ascii, unicode,
//! unicode-bmp, latin-iso8859-1, emacs, and eight-bit.

use super::error::{signal, EvalResult, Flow};
use super::value::*;
use std::collections::{HashMap, HashSet};

const RAW_BYTE_SENTINEL_MIN: u32 = 0xE080;
const RAW_BYTE_SENTINEL_MAX: u32 = 0xE0FF;
const UNIBYTE_BYTE_SENTINEL_MIN: u32 = 0xE300;
const UNIBYTE_BYTE_SENTINEL_MAX: u32 = 0xE3FF;

// ---------------------------------------------------------------------------
// Charset data types
// ---------------------------------------------------------------------------

/// Information about a single charset.
struct CharsetInfo {
    id: i64,
    name: String,
    doc: String,
    dimension: u8,
    min_code: u32,
    max_code: u32,
    plist: Vec<(String, Value)>,
}

/// Registry of known charsets, keyed by name.
pub(crate) struct CharsetRegistry {
    charsets: HashMap<String, CharsetInfo>,
    /// Priority-ordered list of charset names.
    priority: Vec<String>,
}

impl CharsetRegistry {
    /// Create a new registry pre-populated with the standard charsets.
    pub fn new() -> Self {
        let mut reg = Self {
            charsets: HashMap::new(),
            priority: Vec::new(),
        };
        reg.init_standard_charsets();
        reg
    }

    fn init_standard_charsets(&mut self) {
        self.register(CharsetInfo {
            id: 0,
            name: "ascii".to_string(),
            doc: "ASCII (ISO646 IRV)".to_string(),
            dimension: 1,
            min_code: 0,
            max_code: 127,
            plist: vec![],
        });
        self.register(CharsetInfo {
            id: 2,
            name: "unicode".to_string(),
            doc: "Unicode (ISO10646)".to_string(),
            dimension: 3,
            min_code: 0,
            max_code: 0x10FFFF,
            plist: vec![],
        });
        self.register(CharsetInfo {
            id: 144,
            name: "unicode-bmp".to_string(),
            doc: "Unicode Basic Multilingual Plane (BMP)".to_string(),
            dimension: 2,
            min_code: 0,
            max_code: 0xFFFF,
            plist: vec![],
        });
        self.register(CharsetInfo {
            id: 5,
            name: "latin-iso8859-1".to_string(),
            doc: "ISO/IEC 8859/1 (Latin-1)".to_string(),
            dimension: 1,
            min_code: 0,
            max_code: 255,
            plist: vec![],
        });
        self.register(CharsetInfo {
            id: 3,
            name: "emacs".to_string(),
            doc: "Full Emacs character set (Unicode + eight-bit)".to_string(),
            dimension: 3,
            min_code: 0,
            max_code: 0x3FFFFF,
            plist: vec![],
        });
        self.register(CharsetInfo {
            id: 4,
            name: "eight-bit".to_string(),
            doc: "Raw bytes 0x80..0xFF".to_string(),
            dimension: 1,
            min_code: 0x80,
            max_code: 0xFF,
            plist: vec![],
        });

        // Default priority order.
        self.priority = vec![
            "unicode".to_string(),
            "emacs".to_string(),
            "ascii".to_string(),
            "unicode-bmp".to_string(),
            "latin-iso8859-1".to_string(),
            "eight-bit".to_string(),
        ];
    }

    fn register(&mut self, info: CharsetInfo) {
        self.charsets.insert(info.name.clone(), info);
    }

    /// Return true if a charset with the given name exists.
    pub fn contains(&self, name: &str) -> bool {
        self.charsets.contains_key(name)
    }

    /// Return the list of all charset names (unordered).
    pub fn names(&self) -> Vec<String> {
        self.charsets.keys().cloned().collect()
    }

    /// Return the priority-ordered list of charset names.
    pub fn priority_list(&self) -> &[String] {
        &self.priority
    }

    /// Move the requested charset names to the front of the priority list
    /// (deduplicated, preserving relative order for remaining entries).
    pub fn set_priority(&mut self, requested: &[String]) {
        let mut seen = HashSet::with_capacity(self.priority.len() + requested.len());
        let mut reordered = Vec::with_capacity(self.priority.len() + requested.len());

        for name in requested {
            if seen.insert(name.clone()) {
                reordered.push(name.clone());
            }
        }

        for name in &self.priority {
            if seen.insert(name.clone()) {
                reordered.push(name.clone());
            }
        }

        self.priority = reordered;
    }

    /// Return the plist for a charset, or None if not found.
    pub fn plist(&self, name: &str) -> Option<&[(String, Value)]> {
        self.charsets.get(name).map(|info| info.plist.as_slice())
    }

    /// Return the internal ID for a charset, if known.
    pub fn id(&self, name: &str) -> Option<i64> {
        self.charsets.get(name).map(|info| info.id)
    }
}

// ---------------------------------------------------------------------------
// Singleton registry
// ---------------------------------------------------------------------------

use std::sync::{Mutex, OnceLock};

/// Global charset registry (initialized once, accessed from builtins).
fn global_registry() -> &'static Mutex<CharsetRegistry> {
    static REGISTRY: OnceLock<Mutex<CharsetRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(CharsetRegistry::new()))
}

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

fn expect_max_args(name: &str, args: &[Value], max: usize) -> Result<(), Flow> {
    if args.len() > max {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

/// Extract a charset name from a Value (symbol or string).
fn charset_name(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Symbol(s) => Ok(s.clone()),
        Value::Nil => Ok("nil".to_string()),
        Value::True => Ok("t".to_string()),
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), other.clone()],
        )),
    }
}

fn require_known_charset(value: &Value) -> Result<String, Flow> {
    let name = match value {
        Value::Symbol(s) => s.clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("charsetp"), other.clone()],
            ))
        }
    };
    let reg = global_registry().lock().expect("poisoned");
    if reg.contains(&name) {
        Ok(name)
    } else {
        Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("charsetp"), Value::symbol(name)],
        ))
    }
}

fn decode_char_codepoint_arg(value: &Value) -> Result<i64, Flow> {
    match value {
        Value::Int(n) if *n >= 0 => Ok(*n),
        Value::Float(f)
            if f.is_finite() && *f >= 0.0 && f.fract() == 0.0 && *f <= i64::MAX as f64 =>
        {
            Ok(*f as i64)
        }
        _ => Err(signal(
            "error",
            vec![Value::string(
                "Not an in-range integer, integral float, or cons of integers",
            )],
        )),
    }
}

fn encode_char_input(value: &Value) -> Result<i64, Flow> {
    match value {
        Value::Char(c) => Ok(*c as i64),
        Value::Int(n) if (0..=0x3FFFFF).contains(n) => Ok(*n),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("characterp"), other.clone()],
        )),
    }
}

// ---------------------------------------------------------------------------
// Pure builtins (Vec<Value> -> EvalResult)
// ---------------------------------------------------------------------------

/// `(charsetp OBJECT)` -- return t if OBJECT names a known charset.
pub(crate) fn builtin_charsetp(args: Vec<Value>) -> EvalResult {
    expect_args("charsetp", &args, 1)?;
    let name = match &args[0] {
        Value::Symbol(s) => s.clone(),
        Value::Nil => "nil".to_string(),
        Value::True => "t".to_string(),
        Value::Str(s) => (**s).clone(),
        _ => return Ok(Value::Nil),
    };
    let reg = global_registry().lock().expect("poisoned");
    Ok(Value::bool(reg.contains(&name)))
}

/// `(charset-list)` -- return list of all charset names as symbols.
pub(crate) fn builtin_charset_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("charset-list", &args, 0)?;
    let reg = global_registry().lock().expect("poisoned");
    let names: Vec<Value> = reg.names().into_iter().map(Value::symbol).collect();
    Ok(Value::list(names))
}

/// `(charset-priority-list &optional HIGHESTP)` -- return list of charsets
/// in priority order.  If HIGHESTP is non-nil, return only the highest
/// priority charset.
pub(crate) fn builtin_charset_priority_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("charset-priority-list", &args, 1)?;
    let highestp = args.first().map(|v| v.is_truthy()).unwrap_or(false);
    let reg = global_registry().lock().expect("poisoned");
    let priority = reg.priority_list();
    if highestp {
        if let Some(first) = priority.first() {
            Ok(Value::list(vec![Value::symbol(first.clone())]))
        } else {
            Ok(Value::Nil)
        }
    } else {
        let syms: Vec<Value> = priority.iter().map(|s| Value::symbol(s.clone())).collect();
        Ok(Value::list(syms))
    }
}

/// `(set-charset-priority &rest CHARSETS)` -- set charset detection priority.
pub(crate) fn builtin_set_charset_priority(args: Vec<Value>) -> EvalResult {
    expect_min_args("set-charset-priority", &args, 1)?;

    let mut reg = global_registry().lock().expect("poisoned");
    let mut requested = Vec::with_capacity(args.len());
    for arg in &args {
        let name = match arg {
            Value::Symbol(s) => s.clone(),
            _ => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("charsetp"), arg.clone()],
                ));
            }
        };
        if !reg.contains(&name) {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("charsetp"), arg.clone()],
            ));
        }
        requested.push(name);
    }
    reg.set_priority(&requested);
    Ok(Value::Nil)
}

/// `(char-charset CH &optional RESTRICTION)` -- return charset for character.
/// Always returns 'unicode for now.
pub(crate) fn builtin_char_charset(args: Vec<Value>) -> EvalResult {
    expect_min_args("char-charset", &args, 1)?;
    expect_max_args("char-charset", &args, 2)?;
    // Validate that the first argument is a character (int or char).
    match &args[0] {
        Value::Int(_) | Value::Char(_) => {}
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("characterp"), other.clone()],
            ));
        }
    }
    Ok(Value::symbol("unicode"))
}

/// `(charset-plist CHARSET)` -- return property list for CHARSET.
pub(crate) fn builtin_charset_plist(args: Vec<Value>) -> EvalResult {
    expect_args("charset-plist", &args, 1)?;
    let name = charset_name(&args[0])?;
    let reg = global_registry().lock().expect("poisoned");
    match reg.plist(&name) {
        Some(pairs) => {
            let mut elems = Vec::with_capacity(pairs.len() * 2);
            for (key, val) in pairs {
                elems.push(Value::symbol(key.clone()));
                elems.push(val.clone());
            }
            Ok(Value::list(elems))
        }
        None => Err(signal(
            "error",
            vec![Value::string("Invalid charset"), Value::symbol(name)],
        )),
    }
}

/// `(charset-id-internal &optional CHARSET)` -- return internal charset id.
pub(crate) fn builtin_charset_id_internal(args: Vec<Value>) -> EvalResult {
    expect_max_args("charset-id-internal", &args, 1)?;
    let arg = args.first().cloned().unwrap_or(Value::Nil);
    let name = match &arg {
        Value::Symbol(name) => name,
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("charsetp"), arg],
            ))
        }
    };

    let reg = global_registry().lock().expect("poisoned");
    if let Some(id) = reg.id(name) {
        Ok(Value::Int(id))
    } else {
        Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("charsetp"), Value::symbol(name.clone())],
        ))
    }
}

/// `(define-charset-internal &rest ARGS)` -- stub, return nil.
pub(crate) fn builtin_define_charset_internal(args: Vec<Value>) -> EvalResult {
    let _ = args;
    Ok(Value::Nil)
}

/// `(find-charset-region BEG END &optional TABLE)` -- stub, return list
/// with 'ascii.
pub(crate) fn builtin_find_charset_region(args: Vec<Value>) -> EvalResult {
    expect_min_args("find-charset-region", &args, 2)?;
    expect_max_args("find-charset-region", &args, 3)?;
    Ok(Value::list(vec![Value::symbol("ascii")]))
}

/// `(find-charset-string STR &optional TABLE)` -- stub, return list
/// of charsets present in STR.
pub(crate) fn builtin_find_charset_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("find-charset-string", &args, 1)?;
    expect_max_args("find-charset-string", &args, 2)?;
    let s = match &args[0] {
        Value::Str(s) => s.as_ref(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ))
        }
    };

    let charsets = classify_string_charsets(s);
    if charsets.is_empty() {
        Ok(Value::Nil)
    } else {
        Ok(Value::list(
            charsets.into_iter().map(Value::symbol).collect::<Vec<_>>(),
        ))
    }
}

/// `(decode-char CHARSET CODE-POINT)` -- decode code-point in CHARSET space.
pub(crate) fn builtin_decode_char(args: Vec<Value>) -> EvalResult {
    expect_args("decode-char", &args, 2)?;
    let name = require_known_charset(&args[0])?;
    let code_point = decode_char_codepoint_arg(&args[1])?;

    let decoded = match name.as_str() {
        "ascii" => (code_point <= 0x7F).then_some(code_point),
        "eight-bit" => (0x80..=0xFF)
            .contains(&code_point)
            .then_some(0x3FFF00 + code_point),
        "latin-iso8859-1" => (0x20..=0x7F)
            .contains(&code_point)
            .then_some(code_point + 0x80),
        "unicode" => (code_point <= 0x10FFFF).then_some(code_point),
        "unicode-bmp" => (code_point <= 0xFFFF).then_some(code_point),
        "emacs" => (code_point <= 0x3FFF7F).then_some(code_point),
        _ => None,
    };

    Ok(decoded.map_or(Value::Nil, Value::Int))
}

/// `(encode-char CH CHARSET)` -- encode CH in CHARSET space.
pub(crate) fn builtin_encode_char(args: Vec<Value>) -> EvalResult {
    expect_args("encode-char", &args, 2)?;
    let ch = encode_char_input(&args[0])?;
    let name = require_known_charset(&args[1])?;

    let encoded = match name.as_str() {
        "ascii" => (ch <= 0x7F).then_some(ch),
        "eight-bit" => (0x3FFF80..=0x3FFFFF).contains(&ch).then_some(ch - 0x3FFF00),
        "latin-iso8859-1" => (0xA0..=0xFF).contains(&ch).then_some(ch - 0x80),
        "unicode" => (ch <= 0x10FFFF).then_some(ch),
        "unicode-bmp" => (ch <= 0xFFFF).then_some(ch),
        "emacs" => (ch <= 0x3FFF7F).then_some(ch),
        _ => None,
    };

    Ok(encoded.map_or(Value::Nil, Value::Int))
}

/// `(clear-charset-maps)` -- stub, return nil.
pub(crate) fn builtin_clear_charset_maps(args: Vec<Value>) -> EvalResult {
    expect_max_args("clear-charset-maps", &args, 0)?;
    Ok(Value::Nil)
}

/// `(charset-after &optional POS)` -- stub, return 'unicode.
pub(crate) fn builtin_charset_after(args: Vec<Value>) -> EvalResult {
    expect_max_args("charset-after", &args, 1)?;
    Ok(Value::symbol("unicode"))
}

/// `(unibyte-charset)` -- return symbol 'latin-iso8859-1.
pub(crate) fn builtin_unibyte_charset(args: Vec<Value>) -> EvalResult {
    expect_max_args("unibyte-charset", &args, 0)?;
    Ok(Value::symbol("latin-iso8859-1"))
}

fn classify_string_charsets(s: &str) -> Vec<&'static str> {
    if s.is_empty() {
        return Vec::new();
    }

    let mut has_ascii = false;
    let mut has_unicode = false;
    let mut has_eight_bit = false;
    let mut has_unicode_bmp = false;

    for ch in s.chars() {
        let cp = ch as u32;
        if (RAW_BYTE_SENTINEL_MIN..=RAW_BYTE_SENTINEL_MAX).contains(&cp) {
            has_eight_bit = true;
            continue;
        }
        if (UNIBYTE_BYTE_SENTINEL_MIN..=UNIBYTE_BYTE_SENTINEL_MAX).contains(&cp) {
            let byte = cp - UNIBYTE_BYTE_SENTINEL_MIN;
            if byte <= 0x7F {
                has_ascii = true;
            } else {
                has_eight_bit = true;
            }
            continue;
        }

        if cp <= 0x7F {
            has_ascii = true;
        } else if cp <= 0xFFFF {
            has_unicode_bmp = true;
        } else {
            has_unicode = true;
        }
    }

    // Match Emacs ordering observed for find-charset-string:
    // ascii, unicode, eight-bit, unicode-bmp.
    let mut out = Vec::new();
    if has_ascii {
        out.push("ascii");
    }
    if has_unicode {
        out.push("unicode");
    }
    if has_eight_bit {
        out.push("eight-bit");
    }
    if has_unicode_bmp {
        out.push("unicode-bmp");
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // CharsetRegistry unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn registry_has_standard_charsets() {
        let reg = CharsetRegistry::new();
        assert!(reg.contains("ascii"));
        assert!(reg.contains("unicode"));
        assert!(reg.contains("unicode-bmp"));
        assert!(reg.contains("latin-iso8859-1"));
        assert!(reg.contains("emacs"));
        assert!(reg.contains("eight-bit"));
        assert!(!reg.contains("nonexistent"));
    }

    #[test]
    fn registry_names_returns_all() {
        let reg = CharsetRegistry::new();
        let names = reg.names();
        assert_eq!(names.len(), 6);
        assert!(names.contains(&"ascii".to_string()));
        assert!(names.contains(&"unicode".to_string()));
    }

    #[test]
    fn registry_priority_list() {
        let reg = CharsetRegistry::new();
        let prio = reg.priority_list();
        assert!(!prio.is_empty());
        // unicode should be the highest priority.
        assert_eq!(prio[0], "unicode");
    }

    #[test]
    fn registry_plist_returns_empty_for_standard() {
        let reg = CharsetRegistry::new();
        let plist = reg.plist("ascii").unwrap();
        assert!(plist.is_empty());
    }

    #[test]
    fn registry_plist_none_for_unknown() {
        let reg = CharsetRegistry::new();
        assert!(reg.plist("nonexistent").is_none());
    }

    // -----------------------------------------------------------------------
    // Builtin tests: charsetp
    // -----------------------------------------------------------------------

    #[test]
    fn charsetp_known() {
        let r = builtin_charsetp(vec![Value::symbol("ascii")]).unwrap();
        assert!(matches!(r, Value::True));
    }

    #[test]
    fn charsetp_unknown() {
        let r = builtin_charsetp(vec![Value::symbol("nonexistent")]).unwrap();
        assert!(r.is_nil());
    }

    #[test]
    fn charsetp_string_arg() {
        let r = builtin_charsetp(vec![Value::string("unicode")]).unwrap();
        assert!(matches!(r, Value::True));
    }

    #[test]
    fn charsetp_non_symbol() {
        let r = builtin_charsetp(vec![Value::Int(42)]).unwrap();
        assert!(r.is_nil());
    }

    #[test]
    fn charsetp_wrong_arg_count() {
        assert!(builtin_charsetp(vec![]).is_err());
        assert!(builtin_charsetp(vec![Value::Nil, Value::Nil]).is_err());
    }

    // -----------------------------------------------------------------------
    // Builtin tests: charset-list
    // -----------------------------------------------------------------------

    #[test]
    fn charset_list_returns_list() {
        let r = builtin_charset_list(vec![]).unwrap();
        assert!(r.is_list());
        let items = list_to_vec(&r).unwrap();
        assert_eq!(items.len(), 6);
    }

    #[test]
    fn charset_list_wrong_arg_count() {
        assert!(builtin_charset_list(vec![Value::Nil]).is_err());
    }

    // -----------------------------------------------------------------------
    // Builtin tests: charset-priority-list
    // -----------------------------------------------------------------------

    #[test]
    fn charset_priority_list_full() {
        let r = builtin_charset_priority_list(vec![]).unwrap();
        let items = list_to_vec(&r).unwrap();
        assert!(!items.is_empty());
        // First should be unicode.
        assert!(matches!(&items[0], Value::Symbol(s) if s == "unicode"));
    }

    #[test]
    fn charset_priority_list_highestp() {
        let r = builtin_charset_priority_list(vec![Value::True]).unwrap();
        let items = list_to_vec(&r).unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], Value::Symbol(s) if s == "unicode"));
    }

    #[test]
    fn charset_priority_list_highestp_nil() {
        let r = builtin_charset_priority_list(vec![Value::Nil]).unwrap();
        let items = list_to_vec(&r).unwrap();
        assert!(items.len() > 1);
    }

    // -----------------------------------------------------------------------
    // Builtin tests: set-charset-priority
    // -----------------------------------------------------------------------

    #[test]
    fn registry_set_priority_reorders_and_dedups() {
        let mut reg = CharsetRegistry::new();
        reg.set_priority(&[
            "ascii".to_string(),
            "unicode".to_string(),
            "ascii".to_string(),
        ]);
        assert_eq!(reg.priority[0], "ascii");
        assert_eq!(reg.priority[1], "unicode");
    }

    #[test]
    fn set_charset_priority_requires_at_least_one_arg() {
        assert!(builtin_set_charset_priority(vec![]).is_err());
    }

    #[test]
    fn set_charset_priority_rejects_unknown_charset() {
        let r = builtin_set_charset_priority(vec![Value::symbol("vm-no-such-charset")]);
        match r {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("charsetp"), Value::symbol("vm-no-such-charset")]
                );
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Builtin tests: char-charset
    // -----------------------------------------------------------------------

    #[test]
    fn char_charset_int() {
        let r = builtin_char_charset(vec![Value::Int(65)]).unwrap();
        assert!(matches!(r, Value::Symbol(s) if s == "unicode"));
    }

    #[test]
    fn char_charset_char() {
        let r = builtin_char_charset(vec![Value::Char('A')]).unwrap();
        assert!(matches!(r, Value::Symbol(s) if s == "unicode"));
    }

    #[test]
    fn char_charset_with_restriction() {
        let r = builtin_char_charset(vec![Value::Int(65), Value::Nil]).unwrap();
        assert!(matches!(r, Value::Symbol(s) if s == "unicode"));
    }

    #[test]
    fn char_charset_wrong_type() {
        assert!(builtin_char_charset(vec![Value::string("not a char")]).is_err());
    }

    #[test]
    fn char_charset_wrong_arg_count() {
        assert!(builtin_char_charset(vec![]).is_err());
        assert!(builtin_char_charset(vec![Value::Int(65), Value::Nil, Value::Nil]).is_err());
    }

    // -----------------------------------------------------------------------
    // Builtin tests: charset-plist
    // -----------------------------------------------------------------------

    #[test]
    fn charset_plist_known() {
        let r = builtin_charset_plist(vec![Value::symbol("ascii")]).unwrap();
        // Standard charsets have empty plists.
        assert!(r.is_nil());
    }

    #[test]
    fn charset_plist_unknown() {
        assert!(builtin_charset_plist(vec![Value::symbol("nonexistent")]).is_err());
    }

    #[test]
    fn charset_plist_wrong_arg_count() {
        assert!(builtin_charset_plist(vec![]).is_err());
    }

    // -----------------------------------------------------------------------
    // Builtin tests: charset-id-internal
    // -----------------------------------------------------------------------

    #[test]
    fn charset_id_internal_requires_charset() {
        let r = builtin_charset_id_internal(vec![]);
        match r {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("charsetp"), Value::Nil]);
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }
    }

    #[test]
    fn charset_id_internal_with_ascii() {
        let r = builtin_charset_id_internal(vec![Value::symbol("ascii")]).unwrap();
        assert!(matches!(r, Value::Int(0)));
    }

    #[test]
    fn charset_id_internal_with_unicode() {
        let r = builtin_charset_id_internal(vec![Value::symbol("unicode")]).unwrap();
        assert!(matches!(r, Value::Int(2)));
    }

    #[test]
    fn charset_id_internal_unknown_is_type_error() {
        let r = builtin_charset_id_internal(vec![Value::symbol("vm-no-such")]);
        match r {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("charsetp"), Value::symbol("vm-no-such")]
                );
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }
    }

    #[test]
    fn charset_id_internal_wrong_arg_count() {
        assert!(builtin_charset_id_internal(vec![Value::Nil, Value::Nil]).is_err());
    }

    // -----------------------------------------------------------------------
    // Builtin tests: define-charset-internal
    // -----------------------------------------------------------------------

    #[test]
    fn define_charset_internal_stub() {
        let r = builtin_define_charset_internal(vec![
            Value::symbol("test"),
            Value::Int(1),
            Value::Int(2),
        ])
        .unwrap();
        assert!(r.is_nil());
    }

    // -----------------------------------------------------------------------
    // Builtin tests: find-charset-region
    // -----------------------------------------------------------------------

    #[test]
    fn find_charset_region_stub() {
        let r = builtin_find_charset_region(vec![Value::Int(1), Value::Int(100)]).unwrap();
        let items = list_to_vec(&r).unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], Value::Symbol(s) if s == "ascii"));
    }

    #[test]
    fn find_charset_region_with_table() {
        let r =
            builtin_find_charset_region(vec![Value::Int(1), Value::Int(100), Value::Nil]).unwrap();
        let items = list_to_vec(&r).unwrap();
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn find_charset_region_wrong_arg_count() {
        assert!(builtin_find_charset_region(vec![Value::Int(1)]).is_err());
        assert!(builtin_find_charset_region(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Nil,
            Value::Nil,
        ])
        .is_err());
    }

    // -----------------------------------------------------------------------
    // Builtin tests: find-charset-string
    // -----------------------------------------------------------------------

    #[test]
    fn find_charset_string_ascii() {
        let r = builtin_find_charset_string(vec![Value::string("hello")]).unwrap();
        let items = list_to_vec(&r).unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], Value::Symbol(s) if s == "ascii"));
    }

    #[test]
    fn find_charset_string_empty_is_nil() {
        let r = builtin_find_charset_string(vec![Value::string("")]).unwrap();
        assert!(r.is_nil());
    }

    #[test]
    fn find_charset_string_bmp() {
        let r = builtin_find_charset_string(vec![Value::string("é")]).unwrap();
        let items = list_to_vec(&r).unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], Value::Symbol(s) if s == "unicode-bmp"));
    }

    #[test]
    fn find_charset_string_unicode_supplementary() {
        let r = builtin_find_charset_string(vec![Value::string("😀")]).unwrap();
        let items = list_to_vec(&r).unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], Value::Symbol(s) if s == "unicode"));
    }

    #[test]
    fn find_charset_string_mixed_order_matches_oracle() {
        let mut s = String::from("a😀é");
        s.push(char::from_u32(0xE3FF).expect("valid unibyte sentinel"));
        let r = builtin_find_charset_string(vec![Value::string(s)]).unwrap();
        let items = list_to_vec(&r).unwrap();
        assert_eq!(items.len(), 4);
        assert!(matches!(&items[0], Value::Symbol(v) if v == "ascii"));
        assert!(matches!(&items[1], Value::Symbol(v) if v == "unicode"));
        assert!(matches!(&items[2], Value::Symbol(v) if v == "eight-bit"));
        assert!(matches!(&items[3], Value::Symbol(v) if v == "unicode-bmp"));
    }

    #[test]
    fn find_charset_string_unibyte_ascii_and_eight_bit() {
        let mut s = String::new();
        s.push(char::from_u32(0xE341).expect("valid unibyte ascii sentinel"));
        s.push(char::from_u32(0xE3FF).expect("valid unibyte 255 sentinel"));
        let r = builtin_find_charset_string(vec![Value::string(s)]).unwrap();
        let items = list_to_vec(&r).unwrap();
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], Value::Symbol(v) if v == "ascii"));
        assert!(matches!(&items[1], Value::Symbol(v) if v == "eight-bit"));
    }

    #[test]
    fn find_charset_string_with_table() {
        let r = builtin_find_charset_string(vec![Value::string("hello"), Value::Nil]).unwrap();
        let items = list_to_vec(&r).unwrap();
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn find_charset_string_wrong_type() {
        let r = builtin_find_charset_string(vec![Value::Int(1)]);
        match r {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::Int(1)]);
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }
    }

    #[test]
    fn find_charset_string_wrong_arg_count() {
        assert!(builtin_find_charset_string(vec![]).is_err());
        assert!(
            builtin_find_charset_string(vec![Value::string("a"), Value::Nil, Value::Nil,]).is_err()
        );
    }

    // -----------------------------------------------------------------------
    // Builtin tests: decode-char
    // -----------------------------------------------------------------------

    #[test]
    fn decode_char_ascii() {
        let r = builtin_decode_char(vec![Value::symbol("ascii"), Value::Int(65)]).unwrap();
        assert!(matches!(r, Value::Int(65)));
    }

    #[test]
    fn decode_char_unicode() {
        let r = builtin_decode_char(vec![Value::symbol("unicode"), Value::Int(0x1F600)]).unwrap();
        assert!(matches!(r, Value::Int(0x1F600)));
    }

    #[test]
    fn decode_char_eight_bit_maps_to_raw_byte_range() {
        let r = builtin_decode_char(vec![Value::symbol("eight-bit"), Value::Int(255)]).unwrap();
        assert!(matches!(r, Value::Int(0x3FFFFF)));
    }

    #[test]
    fn decode_char_invalid_code_point() {
        let r = builtin_decode_char(vec![Value::symbol("unicode"), Value::Int(0xD800)]).unwrap();
        assert!(matches!(r, Value::Int(0xD800)));
    }

    #[test]
    fn decode_char_negative() {
        let r = builtin_decode_char(vec![Value::symbol("unicode"), Value::Int(-1)]);
        match r {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string(
                        "Not an in-range integer, integral float, or cons of integers"
                    )]
                );
            }
            other => panic!("expected decode-char error signal, got {other:?}"),
        }
    }

    #[test]
    fn decode_char_out_of_range() {
        let r = builtin_decode_char(vec![Value::symbol("unicode"), Value::Int(0x110000)]).unwrap();
        assert!(r.is_nil());
    }

    #[test]
    fn decode_char_unknown_charset() {
        let r = builtin_decode_char(vec![Value::symbol("nonexistent"), Value::Int(65)]);
        match r {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("charsetp"), Value::symbol("nonexistent")]
                );
            }
            other => panic!("expected wrong-type-argument charsetp, got {other:?}"),
        }
    }

    #[test]
    fn decode_char_wrong_type() {
        let r = builtin_decode_char(vec![Value::symbol("ascii"), Value::string("not an int")]);
        match r {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string(
                        "Not an in-range integer, integral float, or cons of integers"
                    )]
                );
            }
            other => panic!("expected decode-char type error, got {other:?}"),
        }
    }

    #[test]
    fn decode_char_wrong_arg_count() {
        assert!(builtin_decode_char(vec![Value::symbol("ascii")]).is_err());
        assert!(
            builtin_decode_char(vec![Value::symbol("ascii"), Value::Int(65), Value::Nil]).is_err()
        );
    }

    // -----------------------------------------------------------------------
    // Builtin tests: encode-char
    // -----------------------------------------------------------------------

    #[test]
    fn encode_char_basic() {
        let r = builtin_encode_char(vec![Value::Int(65), Value::symbol("ascii")]).unwrap();
        assert!(matches!(r, Value::Int(65)));
    }

    #[test]
    fn encode_char_unicode() {
        let r = builtin_encode_char(vec![Value::Int(0x1F600), Value::symbol("unicode")]).unwrap();
        assert!(matches!(r, Value::Int(0x1F600)));
    }

    #[test]
    fn encode_char_eight_bit_raw_byte_maps_back_to_byte() {
        let r = builtin_encode_char(vec![Value::Int(0x3FFFFF), Value::symbol("eight-bit")]).unwrap();
        assert!(matches!(r, Value::Int(255)));
    }

    #[test]
    fn encode_char_with_char_value() {
        let r = builtin_encode_char(vec![Value::Char('Z'), Value::symbol("unicode")]).unwrap();
        assert!(matches!(r, Value::Int(90)));
    }

    #[test]
    fn encode_char_unknown_charset() {
        let r = builtin_encode_char(vec![Value::Int(65), Value::symbol("nonexistent")]);
        match r {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("charsetp"), Value::symbol("nonexistent")]
                );
            }
            other => panic!("expected wrong-type-argument charsetp, got {other:?}"),
        }
    }

    #[test]
    fn encode_char_wrong_type() {
        assert!(
            builtin_encode_char(vec![Value::string("not a char"), Value::symbol("ascii")]).is_err()
        );
    }

    #[test]
    fn encode_char_wrong_arg_count() {
        assert!(builtin_encode_char(vec![Value::Int(65)]).is_err());
    }

    // -----------------------------------------------------------------------
    // Builtin tests: clear-charset-maps
    // -----------------------------------------------------------------------

    #[test]
    fn clear_charset_maps_stub() {
        let r = builtin_clear_charset_maps(vec![]).unwrap();
        assert!(r.is_nil());
    }

    #[test]
    fn clear_charset_maps_wrong_arg_count() {
        assert!(builtin_clear_charset_maps(vec![Value::Nil]).is_err());
    }

    // -----------------------------------------------------------------------
    // Builtin tests: charset-after
    // -----------------------------------------------------------------------

    #[test]
    fn charset_after_no_arg() {
        let r = builtin_charset_after(vec![]).unwrap();
        assert!(matches!(r, Value::Symbol(s) if s == "unicode"));
    }

    #[test]
    fn charset_after_with_pos() {
        let r = builtin_charset_after(vec![Value::Int(42)]).unwrap();
        assert!(matches!(r, Value::Symbol(s) if s == "unicode"));
    }

    #[test]
    fn charset_after_wrong_arg_count() {
        assert!(builtin_charset_after(vec![Value::Int(1), Value::Int(2)]).is_err());
    }

    // -----------------------------------------------------------------------
    // Builtin tests: unibyte-charset
    // -----------------------------------------------------------------------

    #[test]
    fn unibyte_charset_returns_latin() {
        let r = builtin_unibyte_charset(vec![]).unwrap();
        assert!(matches!(r, Value::Symbol(s) if s == "latin-iso8859-1"));
    }

    #[test]
    fn unibyte_charset_wrong_arg_count() {
        assert!(builtin_unibyte_charset(vec![Value::Nil]).is_err());
    }

    // -----------------------------------------------------------------------
    // Round-trip tests
    // -----------------------------------------------------------------------

    #[test]
    fn decode_encode_round_trip() {
        // decode-char then encode-char should give the same code-point.
        let code = 0x00E9_i64; // e-acute
        let decoded =
            builtin_decode_char(vec![Value::symbol("unicode"), Value::Int(code)]).unwrap();
        let cp = decoded.as_int().unwrap();
        let encoded = builtin_encode_char(vec![Value::Int(cp), Value::symbol("unicode")]).unwrap();
        assert!(matches!(encoded, Value::Int(n) if n == code));
    }

    #[test]
    fn charsetp_all_standard() {
        for name in &[
            "ascii",
            "unicode",
            "unicode-bmp",
            "latin-iso8859-1",
            "emacs",
            "eight-bit",
        ] {
            let r = builtin_charsetp(vec![Value::symbol(*name)]).unwrap();
            assert!(
                matches!(r, Value::True),
                "charsetp should return t for {}",
                name
            );
        }
    }
}
