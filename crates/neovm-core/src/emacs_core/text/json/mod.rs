//! JSON serialization and parsing builtins.
//!
//! Implements the Emacs JSON interface:
//! - `json-serialize` — convert Lisp value to JSON string
//! - `json-parse-string` — parse JSON string to Lisp value
//!
//! Key mapping (Emacs convention):
//! - Lisp nil → JSON object {}
//! - Lisp :null → JSON null
//! - Lisp t → JSON true
//! - Lisp :false → JSON false
//! - Lisp integer/float → JSON number
//! - Lisp string → JSON string
//! - Lisp hash-table → JSON object
//! - Lisp vector → JSON array
//! - Lisp alist/plist → JSON object (when :object-type specifies)
//!
//! No external crate (serde_json etc.) is used — the parser and serializer
//! are implemented from scratch with simple recursive descent.

use super::error::{EvalResult, Flow, signal};
use super::intern::resolve_sym;
use super::value::*;
use crate::buffer::{EmacsByteLen, EmacsBytePos, EmacsByteRange, TextExtent};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_min_args;
use strum::{EnumString, IntoStaticStr};

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Keyword argument parsing
// ---------------------------------------------------------------------------

/// Options that control how Lisp values are serialized to JSON.
#[derive(Clone, Debug)]
struct SerializeOpts {
    /// The Lisp value that maps to JSON null.
    null_object: Value,
    /// The Lisp value that maps to JSON false.
    false_object: Value,
}

impl Default for SerializeOpts {
    fn default() -> Self {
        Self {
            null_object: Value::keyword(":null"),
            false_object: Value::keyword(":false"),
        }
    }
}

/// Options that control how a JSON string is parsed into Lisp values.
#[derive(Clone, Debug)]
struct ParseOpts {
    /// How JSON objects are represented.
    object_type: ObjectType,
    /// How JSON arrays are represented.
    array_type: ArrayType,
    /// Lisp value for JSON null.
    null_object: Value,
    /// Lisp value for JSON false.
    false_object: Value,
}

impl Default for ParseOpts {
    fn default() -> Self {
        Self {
            object_type: ObjectType::HashTable,
            array_type: ArrayType::Vector,
            null_object: Value::keyword(":null"),
            false_object: Value::keyword(":false"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum ObjectType {
    HashTable,
    Alist,
    Plist,
}

impl ObjectType {
    fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    fn symbol_name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
enum ArrayType {
    #[strum(serialize = "array")]
    Vector,
    #[strum(serialize = "list")]
    List,
}

impl ArrayType {
    fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    fn symbol_name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(prefix = ":", serialize_all = "kebab-case")]
enum JsonOptionKey {
    ObjectType,
    ArrayType,
    NullObject,
    FalseObject,
}

impl JsonOptionKey {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.strip_prefix(':')?.parse().ok()
    }

    #[cfg(test)]
    fn keyword(self) -> &'static str {
        self.into()
    }
}

/// Parse keyword arguments from the &rest tail (starting at `start_index`).
/// Returns `ParseOpts`.  Unknown keywords signal `json-error`.
fn parse_parse_kwargs(args: &[Value], start_index: usize) -> Result<ParseOpts, Flow> {
    let mut opts = ParseOpts::default();
    let rest = &args[start_index..];
    if !rest.len().is_multiple_of(2) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("plistp"), Value::list(rest.to_vec())],
        ));
    }

    for i in (0..rest.len()).step_by(2).rev() {
        let key = &rest[i];
        let value = &rest[i + 1];
        match JsonOptionKey::from_value(key) {
            Some(JsonOptionKey::ObjectType) => {
                if let Some(object_type) = ObjectType::from_symbol_value(value) {
                    opts.object_type = object_type;
                } else {
                    return Err(signal(
                        "error",
                        vec![
                            Value::string("One of hash-table, alist or plist should be specified"),
                            *value,
                        ],
                    ));
                }
            }
            Some(JsonOptionKey::ArrayType) => {
                if let Some(array_type) = ArrayType::from_symbol_value(value) {
                    opts.array_type = array_type;
                } else {
                    return Err(signal(
                        "error",
                        vec![
                            Value::string("One of array or list should be specified"),
                            *value,
                        ],
                    ));
                }
            }
            Some(JsonOptionKey::NullObject) => {
                opts.null_object = *value;
            }
            Some(JsonOptionKey::FalseObject) => {
                opts.false_object = *value;
            }
            _ => {
                return Err(signal(
                    "error",
                    vec![
                        Value::string(
                            "One of :object-type, :array-type, :null-object or :false-object should be specified",
                        ),
                        *value,
                    ],
                ));
            }
        }
    }
    Ok(opts)
}

/// Parse keyword arguments relevant to `json-serialize` / `json-insert`.
fn parse_serialize_kwargs(args: &[Value], start_index: usize) -> Result<SerializeOpts, Flow> {
    let mut opts = SerializeOpts::default();
    let rest = &args[start_index..];
    if !rest.len().is_multiple_of(2) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("plistp"), Value::list(rest.to_vec())],
        ));
    }

    for i in (0..rest.len()).step_by(2).rev() {
        let key = &rest[i];
        let value = &rest[i + 1];
        match JsonOptionKey::from_value(key) {
            Some(JsonOptionKey::NullObject) => {
                opts.null_object = *value;
            }
            Some(JsonOptionKey::FalseObject) => {
                opts.false_object = *value;
            }
            _ => {
                return Err(signal(
                    "error",
                    vec![
                        Value::string("One of :null-object or :false-object should be specified"),
                        *value,
                    ],
                ));
            }
        }
    }
    Ok(opts)
}

// ===========================================================================
// JSON Serializer (Lisp → JSON string)
// ===========================================================================

/// Check if two Values are equivalent for the purpose of matching the
/// null/false sentinel objects.
fn value_matches(a: &Value, b: &Value) -> bool {
    super::value::eq_value(a, b)
}

/// Serialize a Lisp value to a JSON string.
fn serialize_to_json(value: &Value, opts: &SerializeOpts, depth: usize) -> Result<String, Flow> {
    if depth > 512 {
        return Err(signal(
            JsonError::Serialize.symbol(),
            vec![Value::string("Nesting too deep")],
        ));
    }

    // Check for null sentinel.
    if value_matches(value, &opts.null_object) {
        return Ok("null".to_string());
    }

    // Check for false sentinel.
    if value_matches(value, &opts.false_object) {
        return Ok("false".to_string());
    }

    match value.kind() {
        // t → true (checked after false sentinel, which is usually :false not t)
        ValueKind::T => Ok("true".to_string()),

        ValueKind::Fixnum(n) => Ok(itoa::Buffer::new().format(n).to_owned()),

        // Bignums serialize as their full decimal expansion (GNU
        // json_out_bignum), so large integers round-trip without loss.
        ValueKind::Veclike(VecLikeType::Bignum) => {
            let n = value
                .as_bignum()
                .expect("ValueKind::Veclike(Bignum) must carry a bignum payload");
            Ok(n.to_string())
        }

        ValueKind::Float => {
            let f = value.xfloat();
            if f.is_nan() || f.is_infinite() {
                // GNU `json_out_float` calls `signal_error ("JSON does not
                // allow Inf or NaN", f)`, which signals a plain `error` whose
                // data is `(MESSAGE FLOAT)` (signal_error wraps the non-list
                // FLOAT in a one-element list).  Mirror that exactly rather
                // than raising a neomacs-only json-serialize-error.
                return Err(signal(
                    "error",
                    vec![Value::string("JSON does not allow Inf or NaN"), *value],
                ));
            }
            // Use neomacs's canonical float printer (the same dtoastr-based
            // routine behind number-to-string / prin1) so json-serialize
            // matches GNU's float_to_string byte-for-byte — e.g. 1e20 →
            // "1e+20", 1e-5 → "1e-05" — and stays consistent with the rest
            // of the runtime. The nan/inf cases are rejected above, so the
            // finite path always yields valid JSON.
            Ok(crate::emacs_core::print::format_float(f))
        }

        ValueKind::String => {
            let string = value
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload");
            let rendered = if string.is_multibyte() {
                string.as_utf8_str().ok_or_else(|| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("json-value-p"), *value],
                    )
                })?
            } else {
                if !string.as_bytes().iter().all(u8::is_ascii) {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("json-value-p"), *value],
                    ));
                }
                std::str::from_utf8(string.as_bytes())
                    .expect("ASCII unibyte strings must be valid UTF-8")
            };
            Ok(json_encode_string(rendered))
        }

        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = value.as_vector_data().unwrap().clone();
            let mut parts = Vec::with_capacity(items.len());
            for item in items.iter() {
                parts.push(serialize_to_json(item, opts, depth + 1)?);
            }
            Ok(format!("[{}]", parts.join(",")))
        }

        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = value.as_hash_table().unwrap().clone();
            let mut parts = Vec::with_capacity(table.data.len());
            for (key, val) in &table.data {
                let key_str = hash_key_to_string(key)?;
                let val_json = serialize_to_json(val, opts, depth + 1)?;
                parts.push(format!("{}:{}", json_encode_string(&key_str), val_json));
            }
            Ok(format!("{{{}}}", parts.join(",")))
        }

        // Alist (list of (KEY . VALUE) conses) or plist (flat KEY VALUE …)
        // → JSON object.
        ValueKind::Cons => serialize_cons_object(value, opts, depth),

        ValueKind::Nil => Ok("{}".to_string()),

        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("json-value-p"), *value],
        )),
    }
}

/// Serialize an alist or plist as a JSON object, mirroring GNU
/// `json_out_object_cons`.
///
/// The list is treated as an alist when its first element is a cons, and as
/// a plist otherwise. Keys must be symbols (keywords are symbols whose name
/// begins with `:`); for plists a single leading `:` is stripped from the
/// emitted key. When a key repeats, the first value wins and later
/// duplicates are dropped, matching GNU.
fn serialize_cons_object(
    value: &Value,
    opts: &SerializeOpts,
    depth: usize,
) -> Result<String, Flow> {
    let items = list_to_vec(value).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *value],
        )
    })?;
    let is_alist = matches!(items.first().map(|v| v.kind()), Some(ValueKind::Cons));

    let mut parts = Vec::with_capacity(items.len());
    let mut seen: Vec<String> = Vec::new();

    if is_alist {
        for item in &items {
            if !matches!(item.kind(), ValueKind::Cons) {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("consp"), *item],
                ));
            }
            let key = item.cons_car();
            let val = item.cons_cdr();
            // Alist keys are emitted verbatim (no colon stripping).
            let name = symbol_object_key(&key)?;
            if push_unique(&mut seen, &name) {
                let val_json = serialize_to_json(&val, opts, depth + 1)?;
                parts.push(format!("{}:{}", json_encode_string(&name), val_json));
            }
        }
    } else {
        let mut i = 0;
        while i < items.len() {
            let name = symbol_object_key(&items[i])?;
            // A plist must supply a value for every key.
            let val = *items.get(i + 1).ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("consp"), Value::NIL],
                )
            })?;
            i += 2;
            // Dedup on the raw symbol name (symbol identity, like GNU's
            // symset) but emit the colon-stripped form.
            if push_unique(&mut seen, &name) {
                let val_json = serialize_to_json(&val, opts, depth + 1)?;
                parts.push(format!(
                    "{}:{}",
                    json_encode_string(strip_plist_colon(&name)),
                    val_json
                ));
            }
        }
    }
    Ok(format!("{{{}}}", parts.join(",")))
}

/// Record `name` as seen; return true if it was not already present
/// (first-occurrence-wins semantics).
fn push_unique(seen: &mut Vec<String>, name: &str) -> bool {
    if seen.iter().any(|s| s == name) {
        false
    } else {
        seen.push(name.to_owned());
        true
    }
}

/// Strip a single leading `:` from a plist key name, but only when more
/// characters follow (mirrors GNU `str[0] == ':' && str[1]`).
fn strip_plist_colon(name: &str) -> &str {
    match name.strip_prefix(':') {
        Some(rest) if !rest.is_empty() => rest,
        _ => name,
    }
}

/// Convert a HashKey to a string suitable as a JSON object key.
fn hash_key_to_string(key: &HashKey) -> Result<String, Flow> {
    match key {
        HashKey::Text(s) => Ok(s.to_string()),
        HashKey::Symbol(id) => Ok(resolve_sym(*id).to_owned()),
        HashKey::Keyword(id) => {
            let s = resolve_sym(*id);
            // Strip leading colon if present.
            if let Some(stripped) = s.strip_prefix(':') {
                Ok(stripped.to_string())
            } else {
                Ok(s.to_owned())
            }
        }
        HashKey::Int(n) => Ok(n.to_string()),
        HashKey::Nil => Ok("nil".to_string()),
        HashKey::True => Ok("t".to_string()),
        _ => Err(signal(
            JsonError::Serialize.symbol(),
            vec![Value::string(
                "Hash table key cannot be converted to JSON object key",
            )],
        )),
    }
}

/// Convert a Lisp value to a string key for a JSON object (used when
/// serializing alists).
///
/// Emacs `json-serialize` expects symbol keys in alists.
fn symbol_object_key(value: &Value) -> Result<String, Flow> {
    match value.kind() {
        ValueKind::Symbol(id) => Ok(resolve_sym(id).to_owned()),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *value],
        )),
    }
}

/// Encode a Rust string as a JSON string with proper escaping.
fn json_encode_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0C' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                // Control characters: emit \u00XX.
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ===========================================================================
// JSON error conditions
// ===========================================================================

/// The JSON error conditions, mirroring GNU `src/json.c` `syms_of_json`.
///
/// Keeping every condition symbol in one exhaustive enum (rather than as
/// scattered string literals) makes the error domain explicit, prevents
/// typos, and lets the compiler check that each signalling site names a
/// real condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JsonError {
    /// `json-parse-error` — generic parse failure.
    Parse,
    /// `json-end-of-file` — input ended in the middle of a value.
    EndOfFile,
    /// `json-trailing-content` — extra non-whitespace after the value.
    TrailingContent,
    /// `json-object-too-deep` — nesting exceeded [`MAX_PARSE_DEPTH`].
    ObjectTooDeep,
    /// `json-utf8-decode-error` — invalid UTF-8 in the input.
    Utf8Decode,
    /// `json-invalid-surrogate-error` — malformed UTF-16 surrogate pair.
    InvalidSurrogate,
    /// `json-escape-sequence-error` — malformed `\` escape in a string.
    EscapeSequence,
    /// `json-number-out-of-range-error` — numeric literal outside range.
    NumberOutOfRange,
    /// `json-serialize-error` — neomacs serialization failure. GNU signals
    /// a plain `error` here; neomacs keeps a dedicated `json-error` subtype.
    Serialize,
}

impl JsonError {
    fn symbol(self) -> &'static str {
        match self {
            JsonError::Parse => "json-parse-error",
            JsonError::EndOfFile => "json-end-of-file",
            JsonError::TrailingContent => "json-trailing-content",
            JsonError::ObjectTooDeep => "json-object-too-deep",
            JsonError::Utf8Decode => "json-utf8-decode-error",
            JsonError::InvalidSurrogate => "json-invalid-surrogate-error",
            JsonError::EscapeSequence => "json-escape-sequence-error",
            JsonError::NumberOutOfRange => "json-number-out-of-range-error",
            JsonError::Serialize => "json-serialize-error",
        }
    }
}

// ===========================================================================
// JSON Parser (JSON string → Lisp value)
// ===========================================================================

fn json_utf8_decode_error(start: usize, end: usize) -> Flow {
    signal(
        JsonError::Utf8Decode.symbol(),
        vec![
            Value::fixnum(start as i64),
            Value::NIL,
            Value::fixnum(end as i64),
        ],
    )
}

/// Parser state: a cursor over the input bytes.
struct JsonParser<'a> {
    input: &'a [u8],
    input_multibyte: bool,
    pos: usize,
    /// Current object/array nesting depth, bounded by [`MAX_PARSE_DEPTH`].
    depth: usize,
    opts: ParseOpts,
}

/// Maximum object/array nesting accepted while parsing, mirroring GNU
/// `src/json.c` `json_parser.available_depth` (10000). Without this bound
/// the recursive-descent parser would overflow the stack on adversarial
/// input instead of signalling a catchable error.
const MAX_PARSE_DEPTH: usize = 10000;

impl<'a> JsonParser<'a> {
    fn new(input: &'a [u8], input_multibyte: bool, opts: ParseOpts) -> Self {
        Self {
            input,
            input_multibyte,
            pos: 0,
            depth: 0,
            opts,
        }
    }

    /// Build a `(LINE nil POS)` signal at the current cursor, matching the
    /// shape and values of GNU `json_signal_error`
    /// (src/json.c:json_signal_error).
    ///
    /// GNU computes `byte = input_current - input_begin`, then
    /// `pos = string_byte_to_pos (byte)` (= `count_chars` for multibyte input,
    /// raw byte offset otherwise) and `line = count_newlines (byte) + 1`.
    /// Because every GNU read advances `input_current` *past* the byte it just
    /// consumed before any error fires, POS is the character offset *after* the
    /// offending character.  Our parser is peek-based, so each signalling site
    /// advances the cursor over the offending char first and then calls this
    /// (mirroring GNU's consume-then-signal order).  The LINE number is
    /// deprecated upstream and provided only for compatibility.
    fn signal_at_pos(&self, kind: JsonError) -> Flow {
        let byte = self.pos.min(self.input.len());
        let line = self.input[..byte].iter().filter(|&&b| b == b'\n').count() as i64 + 1;
        signal(
            kind.symbol(),
            vec![
                Value::fixnum(line),
                Value::NIL,
                Value::fixnum(self.source_char_pos() as i64),
            ],
        )
    }

    /// Read the next byte, advancing the cursor, mirroring GNU
    /// `json_input_get`.  At end of input this signals `json-end-of-file`,
    /// exactly like GNU's `json_input_get_slow_path` (src/json.c).
    fn consume(&mut self) -> Result<u8, Flow> {
        match self.input.get(self.pos).copied() {
            Some(b) => {
                self.pos += 1;
                Ok(b)
            }
            None => Err(self.signal_at_pos(JsonError::EndOfFile)),
        }
    }

    /// Read the next byte if one is available, advancing the cursor, mirroring
    /// GNU `json_input_get_if_possible`.  Returns `None` at end of input.
    fn consume_if_possible(&mut self) -> Option<u8> {
        let b = self.input.get(self.pos).copied()?;
        self.pos += 1;
        Some(b)
    }

    /// Put back the last byte read by [`Self::consume`] /
    /// [`Self::consume_if_possible`], mirroring GNU `json_input_put_back`.
    fn put_back(&mut self) {
        self.pos -= 1;
    }

    /// Skip whitespace and return the first non-whitespace byte, consuming it.
    /// Mirrors GNU `json_skip_whitespace`: it reads through `json_input_get`,
    /// so reaching end of input signals `json-end-of-file`.
    fn skip_ws_consume(&mut self) -> Result<u8, Flow> {
        loop {
            let b = self.consume()?;
            if !matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                return Ok(b);
            }
        }
    }

    /// Skip whitespace and return the first non-whitespace byte, or `None` at
    /// end of input.  Mirrors GNU `json_skip_whitespace_if_possible`.
    fn skip_ws_if_possible(&mut self) -> Option<u8> {
        loop {
            let b = self.consume_if_possible()?;
            if !matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                return Some(b);
            }
        }
    }

    /// Enter one level of object/array nesting, signalling
    /// `json-object-too-deep` if the recursion limit is exceeded. Paired with
    /// [`Self::leave_nesting`] on the success path; on the error path the
    /// whole parse unwinds, so the counter need not be restored.
    fn enter_nesting(&mut self) -> Result<(), Flow> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            return Err(self.signal_at_pos(JsonError::ObjectTooDeep));
        }
        Ok(())
    }

    fn leave_nesting(&mut self) {
        self.depth -= 1;
    }

    /// Current byte (or None if at end).
    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    /// Advance by one byte.
    fn advance(&mut self) {
        self.pos += 1;
    }

    fn source_char_pos(&self) -> usize {
        if self.input_multibyte {
            crate::emacs_core::emacs_char::byte_to_char_pos(self.input, self.pos)
        } else {
            self.pos
        }
    }

    /// Whether `c` may continue a `true`/`false`/`null` token, mirroring GNU
    /// `json_is_token_char` (src/json.c).  Used so that e.g. `truer` is one
    /// invalid token rather than `true` followed by `r`.
    fn is_token_char(c: u8) -> bool {
        c.is_ascii_alphanumeric() || c == b'-'
    }

    /// Parse a single top-level JSON value, mirroring GNU `json_parse`:
    /// `json_parse_value (parser, json_skip_whitespace (parser))`.  Leading
    /// whitespace is skipped via the consuming reader, so empty or
    /// whitespace-only input signals `json-end-of-file`.
    fn parse(&mut self) -> Result<Value, Flow> {
        let c = self.skip_ws_consume()?;
        self.parse_value(c)
    }

    /// Parse one JSON value whose first non-whitespace byte `c` has already
    /// been consumed, mirroring GNU `json_parse_value`.
    fn parse_value(&mut self, c: u8) -> Result<Value, Flow> {
        match c {
            b'"' => self.parse_string_value(),
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'-' | b'0'..=b'9' => self.parse_number(c),
            b't' => self.parse_literal_token(b"rue", Value::T),
            b'f' => self.parse_literal_token(b"alse", self.opts.false_object),
            b'n' => self.parse_literal_token(b"ull", self.opts.null_object),
            _ => Err(self.signal_at_pos(JsonError::Parse)),
        }
    }

    /// Parse the remaining bytes of a `true`/`false`/`null` token (the leading
    /// byte is already consumed) and ensure the token is not immediately
    /// followed by another token char, mirroring GNU `json_parse_value`.
    fn parse_literal_token(&mut self, rest: &[u8], value: Value) -> Result<Value, Flow> {
        for &expected in rest {
            if self.consume_if_possible() != Some(expected) {
                return Err(self.signal_at_pos(JsonError::Parse));
            }
        }
        match self.consume_if_possible() {
            Some(c2) if Self::is_token_char(c2) => Err(self.signal_at_pos(JsonError::Parse)),
            Some(_) => {
                self.put_back();
                Ok(value)
            }
            None => Ok(value),
        }
    }

    /// Parse a JSON string value; the opening `"` has already been consumed.
    fn parse_string_value(&mut self) -> Result<Value, Flow> {
        let s = self.parse_string_body()?;
        Ok(Value::string(s))
    }

    /// Parse a JSON string body (the opening `"` has already been consumed)
    /// and return the decoded Rust String.  Mirrors GNU `json_parse_string`,
    /// which reads bytes through `json_input_get`, so a string left open at
    /// end of input signals `json-end-of-file`.
    fn parse_string_body(&mut self) -> Result<String, Flow> {
        let mut result = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(self.signal_at_pos(JsonError::EndOfFile));
                }
                Some(b'"') => {
                    self.advance();
                    return Ok(result);
                }
                Some(b'\\') => {
                    self.advance();
                    match self.peek() {
                        Some(b'"') => {
                            self.advance();
                            result.push('"');
                        }
                        Some(b'\\') => {
                            self.advance();
                            result.push('\\');
                        }
                        Some(b'/') => {
                            self.advance();
                            result.push('/');
                        }
                        Some(b'n') => {
                            self.advance();
                            result.push('\n');
                        }
                        Some(b'r') => {
                            self.advance();
                            result.push('\r');
                        }
                        Some(b't') => {
                            self.advance();
                            result.push('\t');
                        }
                        Some(b'b') => {
                            self.advance();
                            result.push('\x08');
                        }
                        Some(b'f') => {
                            self.advance();
                            result.push('\x0C');
                        }
                        Some(b'u') => {
                            self.advance();
                            let cp = self.parse_unicode_escape()?;
                            // Handle UTF-16 surrogate pairs. A malformed pair
                            // (high surrogate without a following low
                            // surrogate, or a lone low surrogate) is a
                            // json-invalid-surrogate-error in GNU rather than a
                            // silent U+FFFD substitution.
                            if (0xD800..=0xDBFF).contains(&cp) {
                                // High surrogate — must be followed by a
                                // \uXXXX low surrogate.  GNU reads the next two
                                // bytes with `json_input_get` (which signals
                                // json-end-of-file at end of input) and reports
                                // the position just after the consumed byte.
                                if self.consume()? != b'\\' {
                                    return Err(self.signal_at_pos(JsonError::InvalidSurrogate));
                                }
                                if self.consume()? != b'u' {
                                    return Err(self.signal_at_pos(JsonError::InvalidSurrogate));
                                }
                                let low = self.parse_unicode_escape()?;
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err(self.signal_at_pos(JsonError::InvalidSurrogate));
                                }
                                let combined =
                                    0x10000 + ((cp as u32 - 0xD800) << 10) + (low as u32 - 0xDC00);
                                match char::from_u32(combined) {
                                    Some(c) => result.push(c),
                                    None => {
                                        return Err(self.signal_at_pos(JsonError::InvalidSurrogate));
                                    }
                                }
                            } else if let Some(c) = char::from_u32(cp as u32) {
                                result.push(c);
                            } else {
                                // cp is a lone low surrogate (0xDC00..=0xDFFF):
                                // the only u16 range char::from_u32 rejects.
                                return Err(self.signal_at_pos(JsonError::InvalidSurrogate));
                            }
                        }
                        Some(_) => {
                            // GNU consumes the offending escape char via
                            // `json_input_get` before signalling, so the
                            // reported position is just after it.
                            self.advance();
                            return Err(self.signal_at_pos(JsonError::EscapeSequence));
                        }
                        None => {
                            return Err(self.signal_at_pos(JsonError::EndOfFile));
                        }
                    }
                }
                Some(b) => {
                    if b < 0x20 {
                        // Unescaped control characters are not valid inside a
                        // JSON string (GNU marks 0x00-0x1F as non-plain in
                        // `json_plain_char` and signals a parse error).  GNU has
                        // already consumed the byte (`json_input_get`), so the
                        // position is just after it.
                        self.advance();
                        return Err(self.signal_at_pos(JsonError::Parse));
                    } else if b < 0x80 {
                        self.advance();
                        result.push(b as char);
                    } else {
                        let start = self.pos;
                        let seq_len = match b {
                            0xC2..=0xDF => 2,
                            0xE0..=0xEF => 3,
                            0xF0..=0xF4 => 4,
                            _ => {
                                return Err(json_utf8_decode_error(
                                    start,
                                    (start + 2).min(self.input.len()),
                                ));
                            }
                        };
                        let end = (start + seq_len).min(self.input.len());
                        let seq = self.input.get(start..end).ok_or_else(|| {
                            json_utf8_decode_error(start, (start + seq_len).min(self.input.len()))
                        })?;
                        let decoded = std::str::from_utf8(seq)
                            .ok()
                            .and_then(|s| {
                                let mut chars = s.chars();
                                let ch = chars.next()?;
                                chars.next().is_none().then_some(ch)
                            })
                            .ok_or_else(|| json_utf8_decode_error(start, end))?;
                        self.pos = end;
                        result.push(decoded);
                    }
                }
            }
        }
    }

    /// Parse the 4 hex digits of a `\uXXXX` escape, mirroring GNU
    /// `json_parse_unicode`.  Each digit is read with the consuming reader, so
    /// end of input signals `json-end-of-file` and a non-hex byte (consumed
    /// first) signals `json-escape-sequence-error` at the position just after
    /// it.
    fn parse_unicode_escape(&mut self) -> Result<u16, Flow> {
        let mut value: u16 = 0;
        for _ in 0..4 {
            let b = self.consume()?;
            let digit = match b {
                b'0'..=b'9' => (b - b'0') as u16,
                b'a'..=b'f' => (b - b'a' + 10) as u16,
                b'A'..=b'F' => (b - b'A' + 10) as u16,
                _ => return Err(self.signal_at_pos(JsonError::EscapeSequence)),
            };
            value = value * 16 + digit;
        }
        Ok(value)
    }

    /// Parse a JSON number whose first byte `c` (a digit or `-`) has already
    /// been consumed, mirroring GNU `json_parse_number`.
    fn parse_number(&mut self, c: u8) -> Result<Value, Flow> {
        // `c` was already consumed by the caller, so the literal starts one
        // byte back.
        let start = self.pos - 1;
        let mut is_float = false;

        // After an optional leading minus, an integer part is required.
        let first_digit = if c == b'-' {
            match self.peek() {
                Some(b) if b.is_ascii_digit() => {
                    self.advance();
                    b
                }
                _ => return Err(self.signal_at_pos(JsonError::Parse)),
            }
        } else {
            c
        };

        // Integer part: a single 0, or 1-9 followed by more digits.
        if first_digit != b'0' {
            while let Some(b) = self.peek() {
                if b.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        // Fractional part.
        if self.peek() == Some(b'.') {
            is_float = true;
            self.advance();
            let frac_start = self.pos;
            while let Some(b) = self.peek() {
                if b.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
            if self.pos == frac_start {
                return Err(self.signal_at_pos(JsonError::Parse));
            }
        }

        // Exponent part.
        if let Some(b'e') | Some(b'E') = self.peek() {
            is_float = true;
            self.advance();
            if let Some(b'+') | Some(b'-') = self.peek() {
                self.advance();
            }
            let exp_start = self.pos;
            while let Some(b) = self.peek() {
                if b.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
            if self.pos == exp_start {
                return Err(self.signal_at_pos(JsonError::Parse));
            }
        }

        let num_str = std::str::from_utf8(&self.input[start..self.pos]).unwrap_or("0");

        if is_float {
            let f: f64 = num_str
                .parse()
                .map_err(|_| self.signal_at_pos(JsonError::Parse))?;
            if f.is_infinite() {
                // JSON has no infinity literal, so an infinite result means
                // the magnitude overflowed the double range. GNU signals
                // json-number-out-of-range-error here rather than returning
                // an infinite float.
                return Err(self.signal_at_pos(JsonError::NumberOutOfRange));
            }
            Ok(Value::make_float(f))
        } else {
            // Integers that fit i64 stay fixnums (or promote to bignum at the
            // fixnum boundary via make_int); larger literals become bignums,
            // matching GNU which never loses integer precision.
            match num_str.parse::<i64>() {
                Ok(n) => Ok(Value::make_int(n)),
                Err(_) => Ok(Value::make_integer_from_str_or_zero(num_str)),
            }
        }
    }

    /// Parse a JSON array: the opening `[` has already been consumed.
    /// Mirrors GNU `json_parse_array`.
    fn parse_array(&mut self) -> Result<Value, Flow> {
        let mut c = self.skip_ws_consume()?;
        let mut items: Vec<Value> = Vec::new();

        if c != b']' {
            // GNU only counts nesting depth for non-empty containers.
            self.enter_nesting()?;
            loop {
                let val = self.parse_value(c)?;
                items.push(val);

                c = self.skip_ws_consume()?;
                if c == b']' {
                    self.leave_nesting();
                    break;
                }
                if c != b',' {
                    return Err(self.signal_at_pos(JsonError::Parse));
                }
                c = self.skip_ws_consume()?;
            }
        }

        match self.opts.array_type {
            ArrayType::Vector => Ok(Value::vector(items)),
            ArrayType::List => Ok(Value::list(items)),
        }
    }

    /// Parse a JSON object: the opening `{` has already been consumed.
    /// Mirrors GNU `json_parse_object`.
    fn parse_object(&mut self) -> Result<Value, Flow> {
        let c = self.skip_ws_consume()?;

        match self.opts.object_type {
            ObjectType::HashTable => self.parse_object_hash_table(c),
            ObjectType::Alist => self.parse_object_alist(c),
            ObjectType::Plist => self.parse_object_plist(c),
        }
    }

    /// Read an object member: KEY string then `:` then value.  `c` is the
    /// already-consumed first char of the key, which must be `"`.  Returns the
    /// decoded key and the parsed value.  Mirrors GNU's per-member handling in
    /// `json_parse_object` plus `json_parse_object_member_value`.
    fn parse_object_member(&mut self, c: u8) -> Result<(String, Value), Flow> {
        if c != b'"' {
            return Err(self.signal_at_pos(JsonError::Parse));
        }
        let key = self.parse_string_body()?;
        // ": value"
        if self.skip_ws_consume()? != b':' {
            return Err(self.signal_at_pos(JsonError::Parse));
        }
        let vc = self.skip_ws_consume()?;
        let val = self.parse_value(vc)?;
        Ok((key, val))
    }

    fn parse_object_hash_table(&mut self, mut c: u8) -> Result<Value, Flow> {
        let ht = Value::hash_table(HashTableTest::Equal);
        if c == b'}' {
            return Ok(ht);
        }
        self.enter_nesting()?;

        loop {
            let (key, val) = self.parse_object_member(c)?;

            {
                let key_val = Value::string(&key);
                let hash_key = HashKey::Text(key.into_boxed_str());
                let _ = ht.with_hash_table_mut(|table| {
                    table.insert(hash_key, key_val, val);
                });
            }

            c = self.skip_ws_consume()?;
            if c == b'}' {
                self.leave_nesting();
                break;
            }
            if c != b',' {
                return Err(self.signal_at_pos(JsonError::Parse));
            }
            c = self.skip_ws_consume()?;
        }

        Ok(ht)
    }

    fn parse_object_alist(&mut self, mut c: u8) -> Result<Value, Flow> {
        let mut pairs: Vec<Value> = Vec::new();
        if c == b'}' {
            return Ok(Value::NIL);
        }
        self.enter_nesting()?;

        loop {
            let (key, val) = self.parse_object_member(c)?;
            pairs.push(Value::cons(Value::symbol(key), val));

            c = self.skip_ws_consume()?;
            if c == b'}' {
                self.leave_nesting();
                break;
            }
            if c != b',' {
                return Err(self.signal_at_pos(JsonError::Parse));
            }
            c = self.skip_ws_consume()?;
        }

        Ok(Value::list(pairs))
    }

    fn parse_object_plist(&mut self, mut c: u8) -> Result<Value, Flow> {
        let mut items: Vec<Value> = Vec::new();
        if c == b'}' {
            return Ok(Value::NIL);
        }
        self.enter_nesting()?;

        loop {
            let (key, val) = self.parse_object_member(c)?;
            // Plist keys are keywords (symbols with leading colon).
            items.push(Value::keyword(format!(":{}", key)));
            items.push(val);

            c = self.skip_ws_consume()?;
            if c == b'}' {
                self.leave_nesting();
                break;
            }
            if c != b',' {
                return Err(self.signal_at_pos(JsonError::Parse));
            }
            c = self.skip_ws_consume()?;
        }

        Ok(Value::list(items))
    }
}

// ===========================================================================
// Public builtin functions
// ===========================================================================

/// `(json-serialize VALUE &rest ARGS)` — serialize a Lisp value to a JSON string.
///
/// ARGS are keyword arguments:
/// - `:null-object VALUE` — Lisp value to serialize as JSON null (default: :null)
/// - `:false-object VALUE` — Lisp value to serialize as JSON false (default: :false)
pub(crate) fn builtin_json_serialize(args: Vec<Value>) -> EvalResult {
    expect_min_args("json-serialize", &args, 1)?;
    let opts = parse_serialize_kwargs(&args, 1)?;
    let json = serialize_to_json(&args[0], &opts, 0)?;
    // GNU `Fjson_serialize` returns `make_unibyte_string (jo.buf, jo.size)`:
    // a UNIBYTE string of raw UTF-8 bytes (multibyte chars are emitted as
    // their raw UTF-8 byte sequences, see `json_out_string`).  `json` already
    // holds exactly those bytes (a Rust UTF-8 String), so wrap them unibyte
    // rather than as a multibyte string.
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_unibyte(json.into_bytes()),
    ))
}

/// `(json-parse-string STRING &rest ARGS)` — parse a JSON string into a Lisp value.
///
/// ARGS are keyword arguments:
/// - `:object-type SYMBOL` — `hash-table` (default), `alist`, or `plist`
/// - `:array-type SYMBOL` — `array` (default, yields vector) or `list`
/// - `:null-object VALUE` — Lisp value for JSON null (default: :null)
/// - `:false-object VALUE` — Lisp value for JSON false (default: :false)
pub(crate) fn builtin_json_parse_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("json-parse-string", &args, 1)?;
    let input = match args[0].kind() {
        ValueKind::String => args[0]
            .as_lisp_string()
            .expect("string object must carry LispString payload"),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };
    let opts = parse_parse_kwargs(&args, 1)?;
    let mut parser = JsonParser::new(input.as_bytes(), input.is_multibyte(), opts);
    let result = parser.parse()?;

    // Ensure there is no trailing non-whitespace, mirroring GNU's
    // `json_skip_whitespace_if_possible (&p) >= 0` check.  The trailing char
    // is consumed before signalling, so the reported position is just after
    // it.
    if parser.skip_ws_if_possible().is_some() {
        return Err(parser.signal_at_pos(JsonError::TrailingContent));
    }

    Ok(result)
}

/// `(json-parse-buffer &rest ARGS)` — parse one JSON value from point.
///
/// Unlike `json-parse-string`, this parses a single JSON value starting at the
/// current point (after leading whitespace), leaves trailing buffer content
/// untouched, and advances point to just after the parsed value.
pub(crate) fn builtin_json_parse_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let opts = parse_parse_kwargs(&args, 0)?;
    let (input, point_base) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let accessible = buf.accessible_emacs_byte_region();
        let point_base = buf.point_emacs_byte_pos();
        let input = buf
            .buffer_substring_lisp_string_range(EmacsByteRange::new(point_base, accessible.end()));
        (input, point_base)
    };

    let mut parser = JsonParser::new(input.as_bytes(), input.is_multibyte(), opts);
    let result = parser.parse()?;
    let new_point = point_base.add_len(EmacsByteLen::new(parser.pos));
    if let Some(current_id) = eval.buffers.current_buffer_id() {
        let _ = eval
            .buffers
            .goto_buffer_emacs_byte_pos(current_id, new_point);
    }
    Ok(result)
}

/// `(json-insert VALUE &rest ARGS)` — insert JSON text at point.
///
/// Keyword arguments mirror `json-serialize` (`:null-object`, `:false-object`).
pub(crate) fn builtin_json_insert(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("json-insert", &args, 1)?;
    let opts = parse_serialize_kwargs(&args, 1)?;
    let json = serialize_to_json(&args[0], &opts, 0)?;
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let (insert_pos, target_multibyte) = eval
        .buffers
        .get(current_id)
        .map(|b| (b.point_emacs_byte_pos(), b.get_multibyte()))
        .unwrap_or((EmacsBytePos::ZERO, true));
    let change = super::editfns::text_change_for_empty_insertion_at_emacs_byte_pos(
        &eval.buffers,
        current_id,
        insert_pos,
        TextExtent::from_emacs_bytes(json.as_bytes(), target_multibyte),
    )?;
    super::editfns::signal_before_text_change(eval, change)?;
    let _ = eval.buffers.insert_into_buffer(current_id, &json);
    super::editfns::signal_after_text_change(eval, change)?;
    Ok(Value::NIL)
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
