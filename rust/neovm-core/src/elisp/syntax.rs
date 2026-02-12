//! Syntax table system for the Elisp VM.
//!
//! Implements Emacs-compatible syntax tables with character classification,
//! motion functions (forward/backward word, sexp scanning), and the
//! `string-to-syntax` descriptor parser.

use std::collections::HashMap;

use super::error::{signal, EvalResult};
use super::value::Value;
use crate::buffer::Buffer;

// ===========================================================================
// Syntax classes
// ===========================================================================

/// Emacs syntax classes, matching the designator characters used in
/// `string-to-syntax` and `modify-syntax-entry`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SyntaxClass {
    /// ' ' — Whitespace
    Whitespace,
    /// 'w' — Word constituent
    Word,
    /// '_' — Symbol constituent
    Symbol,
    /// '.' — Punctuation
    Punctuation,
    /// '(' — Open parenthesis/bracket
    Open,
    /// ')' — Close parenthesis/bracket
    Close,
    /// '"' — String delimiter
    StringDelim,
    /// '$' — Math delimiter (paired)
    MathDelim,
    /// '\\' — Escape character
    Escape,
    /// '/' — Character quote (only quotes the next character)
    CharQuote,
    /// '<' — Comment starter
    Comment,
    /// '>' — Comment ender
    EndComment,
    /// '@' — Inherit from standard syntax table
    InheritStandard,
    /// '!' — Generic comment delimiter
    Generic,
}

impl SyntaxClass {
    /// Parse a syntax class from its single-character designator.
    pub fn from_char(ch: char) -> Option<SyntaxClass> {
        match ch {
            ' ' | '-' => Some(SyntaxClass::Whitespace),
            'w' => Some(SyntaxClass::Word),
            '_' => Some(SyntaxClass::Symbol),
            '.' => Some(SyntaxClass::Punctuation),
            '(' => Some(SyntaxClass::Open),
            ')' => Some(SyntaxClass::Close),
            '"' => Some(SyntaxClass::StringDelim),
            '$' => Some(SyntaxClass::MathDelim),
            '\\' => Some(SyntaxClass::Escape),
            '/' => Some(SyntaxClass::CharQuote),
            '<' => Some(SyntaxClass::Comment),
            '>' => Some(SyntaxClass::EndComment),
            '@' => Some(SyntaxClass::InheritStandard),
            '!' => Some(SyntaxClass::Generic),
            _ => None,
        }
    }

    /// Return the canonical single-character designator for this class.
    pub fn to_char(self) -> char {
        match self {
            SyntaxClass::Whitespace => ' ',
            SyntaxClass::Word => 'w',
            SyntaxClass::Symbol => '_',
            SyntaxClass::Punctuation => '.',
            SyntaxClass::Open => '(',
            SyntaxClass::Close => ')',
            SyntaxClass::StringDelim => '"',
            SyntaxClass::MathDelim => '$',
            SyntaxClass::Escape => '\\',
            SyntaxClass::CharQuote => '/',
            SyntaxClass::Comment => '<',
            SyntaxClass::EndComment => '>',
            SyntaxClass::InheritStandard => '@',
            SyntaxClass::Generic => '!',
        }
    }

    /// Return the integer code Emacs uses for this syntax class
    /// (used in the cons cell returned by `string-to-syntax`).
    pub fn code(self) -> i64 {
        match self {
            SyntaxClass::Whitespace => 0,
            SyntaxClass::Punctuation => 1,
            SyntaxClass::Word => 2,
            SyntaxClass::Symbol => 3,
            SyntaxClass::Open => 4,
            SyntaxClass::Close => 5,
            SyntaxClass::MathDelim => 6,
            SyntaxClass::StringDelim => 7,
            SyntaxClass::Escape => 9,
            SyntaxClass::CharQuote => 10,
            SyntaxClass::Comment => 11,
            SyntaxClass::EndComment => 12,
            SyntaxClass::InheritStandard => 13,
            SyntaxClass::Generic => 14,
        }
    }
}

// ===========================================================================
// Syntax flags
// ===========================================================================

/// Flags for comment style and prefix behavior, mirroring Emacs syntax flags.
///
/// Uses a raw `u8` bitmask to avoid external dependencies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SyntaxFlags(u8);

impl SyntaxFlags {
    /// '1' — first char of a two-char comment start sequence
    pub const COMMENT_START_FIRST: SyntaxFlags  = SyntaxFlags(0b0000_0001);
    /// '2' — second char of a two-char comment start sequence
    pub const COMMENT_START_SECOND: SyntaxFlags = SyntaxFlags(0b0000_0010);
    /// '3' — first char of a two-char comment end sequence
    pub const COMMENT_END_FIRST: SyntaxFlags    = SyntaxFlags(0b0000_0100);
    /// '4' — second char of a two-char comment end sequence
    pub const COMMENT_END_SECOND: SyntaxFlags   = SyntaxFlags(0b0000_1000);
    /// 'p' — prefix character (e.g., quote, backquote)
    pub const PREFIX: SyntaxFlags               = SyntaxFlags(0b0001_0000);
    /// 'b' — belongs to alternative "b" comment style
    pub const COMMENT_STYLE_B: SyntaxFlags      = SyntaxFlags(0b0010_0000);
    /// 'n' — nestable comment
    pub const COMMENT_NESTABLE: SyntaxFlags     = SyntaxFlags(0b0100_0000);

    /// Empty flags (no bits set).
    pub const fn empty() -> Self {
        SyntaxFlags(0)
    }

    /// Whether no flags are set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether `self` contains all the bits of `other`.
    pub const fn contains(self, other: SyntaxFlags) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Return the raw bits.
    pub const fn bits(self) -> u8 {
        self.0
    }
}

impl std::ops::BitOr for SyntaxFlags {
    type Output = SyntaxFlags;
    fn bitor(self, rhs: SyntaxFlags) -> SyntaxFlags {
        SyntaxFlags(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for SyntaxFlags {
    fn bitor_assign(&mut self, rhs: SyntaxFlags) {
        self.0 |= rhs.0;
    }
}

// ===========================================================================
// SyntaxEntry
// ===========================================================================

/// A single entry in a syntax table: the class, an optional matching
/// character (for parens/string delimiters), and comment/prefix flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxEntry {
    pub class: SyntaxClass,
    pub matching_char: Option<char>,
    pub flags: SyntaxFlags,
}

impl SyntaxEntry {
    /// Create a simple entry with no matching char or flags.
    pub fn simple(class: SyntaxClass) -> Self {
        Self {
            class,
            matching_char: None,
            flags: SyntaxFlags::empty(),
        }
    }

    /// Create an entry with a matching character (for open/close parens).
    pub fn with_match(class: SyntaxClass, matching: char) -> Self {
        Self {
            class,
            matching_char: Some(matching),
            flags: SyntaxFlags::empty(),
        }
    }
}

// ===========================================================================
// string-to-syntax parser
// ===========================================================================

/// Parse an Emacs syntax descriptor string (e.g., `" "`, `"w"`, `"()"`,
/// `". 12"`) into a `SyntaxEntry`.
pub fn string_to_syntax(s: &str) -> Result<SyntaxEntry, String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return Err("Empty syntax descriptor".to_string());
    }

    let class = SyntaxClass::from_char(chars[0])
        .ok_or_else(|| format!("Invalid syntax class character: '{}'", chars[0]))?;

    let matching_char = if chars.len() > 1 && chars[1] != ' ' {
        Some(chars[1])
    } else {
        None
    };

    let mut flags = SyntaxFlags::empty();
    // Flags start at position 2 (after class + matching char).
    let flag_start = if chars.len() > 1 { 2 } else { 1 };
    for &ch in chars.get(flag_start..).unwrap_or(&[]) {
        match ch {
            '1' => flags |= SyntaxFlags::COMMENT_START_FIRST,
            '2' => flags |= SyntaxFlags::COMMENT_START_SECOND,
            '3' => flags |= SyntaxFlags::COMMENT_END_FIRST,
            '4' => flags |= SyntaxFlags::COMMENT_END_SECOND,
            'p' => flags |= SyntaxFlags::PREFIX,
            'b' => flags |= SyntaxFlags::COMMENT_STYLE_B,
            'n' => flags |= SyntaxFlags::COMMENT_NESTABLE,
            ' ' => {} // whitespace in flag area is ignored
            _ => {} // Emacs silently ignores unknown flags
        }
    }

    Ok(SyntaxEntry {
        class,
        matching_char,
        flags,
    })
}

/// Convert a `SyntaxEntry` into the Emacs cons-cell representation
/// returned by `string-to-syntax`: `(CODE . MATCHING-CHAR-OR-NIL)`.
///
/// The CODE is computed as: `(class_code) | (flags << 16)`.
pub fn syntax_entry_to_value(entry: &SyntaxEntry) -> Value {
    let code = entry.class.code() | ((entry.flags.bits() as i64) << 16);
    let matching = match entry.matching_char {
        Some(ch) => Value::Int(ch as i64),
        None => Value::Nil,
    };
    Value::cons(Value::Int(code), matching)
}

// ===========================================================================
// SyntaxTable
// ===========================================================================

/// An Emacs-style syntax table mapping characters to syntax entries.
///
/// Characters not explicitly set fall back to a parent table (if present)
/// or to the built-in standard defaults.
#[derive(Clone, Debug)]
pub struct SyntaxTable {
    /// Per-character overrides.
    entries: HashMap<char, SyntaxEntry>,
    /// Optional parent table for inheritance.
    parent: Option<Box<SyntaxTable>>,
}

impl SyntaxTable {
    // -- Construction --------------------------------------------------------

    /// Create the standard Emacs syntax table with ASCII defaults.
    pub fn new_standard() -> Self {
        let mut entries = HashMap::new();

        // Whitespace
        for ch in [' ', '\t', '\n', '\r', '\x0c'] {
            entries.insert(ch, SyntaxEntry::simple(SyntaxClass::Whitespace));
        }

        // Word constituents: a-z, A-Z, 0-9
        for ch in 'a'..='z' {
            entries.insert(ch, SyntaxEntry::simple(SyntaxClass::Word));
        }
        for ch in 'A'..='Z' {
            entries.insert(ch, SyntaxEntry::simple(SyntaxClass::Word));
        }
        for ch in '0'..='9' {
            entries.insert(ch, SyntaxEntry::simple(SyntaxClass::Word));
        }

        // Parentheses (with matching chars)
        entries.insert('(', SyntaxEntry::with_match(SyntaxClass::Open, ')'));
        entries.insert(')', SyntaxEntry::with_match(SyntaxClass::Close, '('));
        entries.insert('[', SyntaxEntry::with_match(SyntaxClass::Open, ']'));
        entries.insert(']', SyntaxEntry::with_match(SyntaxClass::Close, '['));
        entries.insert('{', SyntaxEntry::with_match(SyntaxClass::Open, '}'));
        entries.insert('}', SyntaxEntry::with_match(SyntaxClass::Close, '{'));

        // String delimiter
        entries.insert('"', SyntaxEntry::simple(SyntaxClass::StringDelim));

        // Escape
        entries.insert('\\', SyntaxEntry::simple(SyntaxClass::Escape));

        // Symbol constituents (common Emacs defaults for Lisp)
        for ch in ['_', '-'] {
            entries.insert(ch, SyntaxEntry::simple(SyntaxClass::Symbol));
        }

        // Punctuation: everything else in printable ASCII that we haven't
        // covered.  In Emacs the standard table marks most punctuation as
        // punctuation; we enumerate the important ones.
        for ch in [
            '!', '#', '%', '&', '*', '+', ',', '.', '/', ':', ';', '<', '=',
            '>', '?', '@', '^', '|', '~',
        ] {
            entries.insert(ch, SyntaxEntry::simple(SyntaxClass::Punctuation));
        }

        // Prefix characters
        entries.insert(
            '\'',
            SyntaxEntry {
                class: SyntaxClass::Punctuation,
                matching_char: None,
                flags: SyntaxFlags::PREFIX,
            },
        );
        // Backtick
        entries.insert(
            '`',
            SyntaxEntry {
                class: SyntaxClass::Punctuation,
                matching_char: None,
                flags: SyntaxFlags::PREFIX,
            },
        );
        // Dollar for TeX math
        entries.insert('$', SyntaxEntry::simple(SyntaxClass::MathDelim));

        Self {
            entries,
            parent: None,
        }
    }

    /// Create a new syntax table that inherits from the standard table.
    pub fn make_syntax_table() -> Self {
        Self {
            entries: HashMap::new(),
            parent: Some(Box::new(Self::new_standard())),
        }
    }

    /// Create a copy of this syntax table (deep clone).
    pub fn copy_syntax_table(&self) -> Self {
        self.clone()
    }

    // -- Queries -------------------------------------------------------------

    /// Look up the syntax entry for `ch`.
    pub fn get_entry(&self, ch: char) -> Option<&SyntaxEntry> {
        self.entries.get(&ch).or_else(|| {
            self.parent.as_ref().and_then(|p| p.get_entry(ch))
        })
    }

    /// Return the syntax class for `ch`, defaulting to `Symbol` for
    /// characters not present in the table (matching Emacs behavior for
    /// multi-byte characters).
    pub fn char_syntax(&self, ch: char) -> SyntaxClass {
        self.get_entry(ch)
            .map(|e| e.class)
            .unwrap_or(SyntaxClass::Symbol)
    }

    // -- Mutation -------------------------------------------------------------

    /// Set the syntax entry for `ch`.
    pub fn modify_syntax_entry(&mut self, ch: char, entry: SyntaxEntry) {
        self.entries.insert(ch, entry);
    }
}

impl Default for SyntaxTable {
    fn default() -> Self {
        Self::new_standard()
    }
}

// ===========================================================================
// Motion functions (operate on a Buffer + SyntaxTable)
// ===========================================================================

/// Move forward over `count` words.  Returns the resulting byte position.
///
/// A "word" is a maximal run of characters with syntax class `Word`.
/// Between words, non-word characters are skipped.
pub fn forward_word(buf: &Buffer, table: &SyntaxTable, count: i64) -> usize {
    if count < 0 {
        return backward_word(buf, table, -count);
    }

    let text = buf.buffer_string();
    let chars: Vec<char> = text.chars().collect();
    // Convert byte pos to char index within accessible region.
    let base = buf.point_min();
    let rel_byte = buf.point().saturating_sub(base);
    let mut idx = buf.text.byte_to_char(base + rel_byte) - buf.text.byte_to_char(base);

    let accessible_char_start = buf.text.byte_to_char(base);
    let accessible_char_end = buf.text.byte_to_char(buf.point_max());
    let accessible_len = accessible_char_end - accessible_char_start;

    for _ in 0..count {
        // Skip non-word characters
        while idx < accessible_len && !matches!(table.char_syntax(chars[idx]), SyntaxClass::Word) {
            idx += 1;
        }
        // Skip word characters
        while idx < accessible_len && matches!(table.char_syntax(chars[idx]), SyntaxClass::Word) {
            idx += 1;
        }
    }

    // Convert char index back to byte position (absolute).
    let abs_char = accessible_char_start + idx;
    buf.text.char_to_byte(abs_char)
}

/// Move backward over `count` words.  Returns the resulting byte position.
pub fn backward_word(buf: &Buffer, table: &SyntaxTable, count: i64) -> usize {
    if count < 0 {
        return forward_word(buf, table, -count);
    }

    let text = buf.buffer_string();
    let chars: Vec<char> = text.chars().collect();
    let base = buf.point_min();
    let rel_byte = buf.point().saturating_sub(base);
    let mut idx = buf.text.byte_to_char(base + rel_byte) - buf.text.byte_to_char(base);

    for _ in 0..count {
        // Skip non-word characters backward
        while idx > 0 && !matches!(table.char_syntax(chars[idx - 1]), SyntaxClass::Word) {
            idx -= 1;
        }
        // Skip word characters backward
        while idx > 0 && matches!(table.char_syntax(chars[idx - 1]), SyntaxClass::Word) {
            idx -= 1;
        }
    }

    let accessible_char_start = buf.text.byte_to_char(base);
    let abs_char = accessible_char_start + idx;
    buf.text.char_to_byte(abs_char)
}

/// Skip forward over characters whose syntax class matches any character in
/// `syntax_chars` (each character in the string names a syntax class,
/// e.g., `"w_"` matches Word and Symbol).  Returns the resulting byte position.
pub fn skip_syntax_forward(
    buf: &Buffer,
    table: &SyntaxTable,
    syntax_chars: &str,
    limit: Option<usize>,
) -> usize {
    let classes: Vec<SyntaxClass> = syntax_chars
        .chars()
        .filter_map(SyntaxClass::from_char)
        .collect();

    let text = buf.buffer_string();
    let chars: Vec<char> = text.chars().collect();
    let base = buf.point_min();
    let rel_byte = buf.point().saturating_sub(base);
    let mut idx = buf.text.byte_to_char(base + rel_byte) - buf.text.byte_to_char(base);

    let accessible_char_start = buf.text.byte_to_char(base);
    let accessible_char_end = buf.text.byte_to_char(buf.point_max());
    let accessible_len = accessible_char_end - accessible_char_start;

    let char_limit = limit
        .map(|lim| {
            let lim_clamped = lim.min(buf.point_max());
            buf.text.byte_to_char(lim_clamped) - accessible_char_start
        })
        .unwrap_or(accessible_len);

    while idx < char_limit {
        let syn = table.char_syntax(chars[idx]);
        if !classes.contains(&syn) {
            break;
        }
        idx += 1;
    }

    let abs_char = accessible_char_start + idx;
    buf.text.char_to_byte(abs_char)
}

/// Skip backward over characters whose syntax class matches any character in
/// `syntax_chars`.  Returns the resulting byte position.
pub fn skip_syntax_backward(
    buf: &Buffer,
    table: &SyntaxTable,
    syntax_chars: &str,
    limit: Option<usize>,
) -> usize {
    let classes: Vec<SyntaxClass> = syntax_chars
        .chars()
        .filter_map(SyntaxClass::from_char)
        .collect();

    let text = buf.buffer_string();
    let chars: Vec<char> = text.chars().collect();
    let base = buf.point_min();
    let rel_byte = buf.point().saturating_sub(base);
    let mut idx = buf.text.byte_to_char(base + rel_byte) - buf.text.byte_to_char(base);

    let accessible_char_start = buf.text.byte_to_char(base);

    let char_limit = limit
        .map(|lim| {
            let lim_clamped = lim.max(base);
            buf.text.byte_to_char(lim_clamped) - accessible_char_start
        })
        .unwrap_or(0);

    while idx > char_limit {
        let syn = table.char_syntax(chars[idx - 1]);
        if !classes.contains(&syn) {
            break;
        }
        idx -= 1;
    }

    let abs_char = accessible_char_start + idx;
    buf.text.char_to_byte(abs_char)
}

/// Scan for balanced expressions (sexps).
///
/// Starting from byte position `from`, scan `count` sexps forward (positive
/// count) or backward (negative count).  Returns the byte position after the
/// last sexp, or an error if unbalanced.
pub fn scan_sexps(
    buf: &Buffer,
    table: &SyntaxTable,
    from: usize,
    count: i64,
) -> Result<usize, String> {
    if count == 0 {
        return Ok(from);
    }

    let text = buf.text.to_string();
    let chars: Vec<char> = text.chars().collect();
    let total_chars = chars.len();

    // Convert byte position to char index.
    let mut idx = buf.text.byte_to_char(from);

    if count > 0 {
        for _ in 0..count {
            idx = scan_sexp_forward(&chars, total_chars, idx, table)?;
        }
    } else {
        for _ in 0..(-count) {
            idx = scan_sexp_backward(&chars, idx, table)?;
        }
    }

    Ok(buf.text.char_to_byte(idx))
}

/// Scan one sexp forward from char index `start`.
fn scan_sexp_forward(
    chars: &[char],
    len: usize,
    start: usize,
    table: &SyntaxTable,
) -> Result<usize, String> {
    let mut idx = start;

    // Skip whitespace and comments
    while idx < len && matches!(table.char_syntax(chars[idx]), SyntaxClass::Whitespace | SyntaxClass::Comment | SyntaxClass::EndComment) {
        idx += 1;
    }

    if idx >= len {
        return Err("Scan error: unbalanced parentheses".to_string());
    }

    let ch = chars[idx];
    let syn = table.char_syntax(ch);

    match syn {
        SyntaxClass::Open => {
            // Find matching close, respecting nesting.
            let open_char = ch;
            let close_char = table
                .get_entry(open_char)
                .and_then(|e| e.matching_char)
                .unwrap_or(')');
            let mut depth = 1i32;
            idx += 1;
            while idx < len && depth > 0 {
                let c = chars[idx];
                let s = table.char_syntax(c);
                match s {
                    SyntaxClass::Open => {
                        depth += 1;
                    }
                    SyntaxClass::Close => {
                        depth -= 1;
                    }
                    SyntaxClass::StringDelim => {
                        // Skip over string contents
                        let delim = c;
                        idx += 1;
                        while idx < len && chars[idx] != delim {
                            if matches!(table.char_syntax(chars[idx]), SyntaxClass::Escape) {
                                idx += 1; // skip escaped char
                            }
                            idx += 1;
                        }
                        // idx now points at closing delim (or past end)
                    }
                    SyntaxClass::Escape => {
                        idx += 1; // skip next char
                    }
                    _ => {}
                }
                idx += 1;
            }
            if depth != 0 {
                return Err(format!(
                    "Scan error: unbalanced parentheses (looking for '{}')",
                    close_char
                ));
            }
            Ok(idx)
        }
        SyntaxClass::Close => {
            Err("Scan error: unbalanced parentheses (unexpected close)".to_string())
        }
        SyntaxClass::StringDelim => {
            // Scan to matching string delimiter.
            let delim = ch;
            idx += 1;
            while idx < len && chars[idx] != delim {
                if matches!(table.char_syntax(chars[idx]), SyntaxClass::Escape) {
                    idx += 1;
                }
                idx += 1;
            }
            if idx >= len {
                return Err("Scan error: unterminated string".to_string());
            }
            Ok(idx + 1) // past closing delim
        }
        SyntaxClass::Word | SyntaxClass::Symbol => {
            // Scan over a symbol/word sexp.
            while idx < len
                && matches!(
                    table.char_syntax(chars[idx]),
                    SyntaxClass::Word | SyntaxClass::Symbol
                )
            {
                idx += 1;
            }
            Ok(idx)
        }
        SyntaxClass::Escape | SyntaxClass::CharQuote => {
            // Escape + next char form one sexp.
            idx += 1;
            if idx < len {
                idx += 1;
            }
            Ok(idx)
        }
        SyntaxClass::MathDelim => {
            // Scan to matching math delimiter.
            let delim = ch;
            idx += 1;
            while idx < len && chars[idx] != delim {
                idx += 1;
            }
            if idx >= len {
                return Err("Scan error: unterminated math delimiter".to_string());
            }
            Ok(idx + 1)
        }
        _ => {
            // Single punctuation or other character is its own sexp.
            Ok(idx + 1)
        }
    }
}

/// Scan one sexp backward from char index `start`.
fn scan_sexp_backward(
    chars: &[char],
    start: usize,
    table: &SyntaxTable,
) -> Result<usize, String> {
    let mut idx = start;

    // Skip whitespace and comments backward
    while idx > 0 && matches!(table.char_syntax(chars[idx - 1]), SyntaxClass::Whitespace | SyntaxClass::Comment | SyntaxClass::EndComment) {
        idx -= 1;
    }

    if idx == 0 {
        return Err("Scan error: beginning of buffer".to_string());
    }

    idx -= 1; // move to the character we're examining
    let ch = chars[idx];
    let syn = table.char_syntax(ch);

    match syn {
        SyntaxClass::Close => {
            // Find matching open, respecting nesting.
            let close_char = ch;
            let open_char = table
                .get_entry(close_char)
                .and_then(|e| e.matching_char)
                .unwrap_or('(');
            let mut depth = 1i32;
            while idx > 0 && depth > 0 {
                idx -= 1;
                let c = chars[idx];
                let s = table.char_syntax(c);
                match s {
                    SyntaxClass::Close => {
                        depth += 1;
                    }
                    SyntaxClass::Open => {
                        depth -= 1;
                    }
                    SyntaxClass::StringDelim => {
                        // Skip over string contents backward
                        let delim = c;
                        if idx > 0 {
                            idx -= 1;
                            while idx > 0 && chars[idx] != delim {
                                idx -= 1;
                            }
                            // idx now points at the opening delim
                        }
                    }
                    _ => {}
                }
            }
            if depth != 0 {
                return Err(format!(
                    "Scan error: unbalanced parentheses (looking for '{}')",
                    open_char
                ));
            }
            Ok(idx)
        }
        SyntaxClass::Open => {
            Err("Scan error: unbalanced parentheses (unexpected open)".to_string())
        }
        SyntaxClass::StringDelim => {
            // Scan backward to matching string delimiter.
            let delim = ch;
            if idx == 0 {
                return Err("Scan error: unterminated string".to_string());
            }
            idx -= 1;
            while idx > 0 && chars[idx] != delim {
                idx -= 1;
            }
            if chars[idx] != delim {
                return Err("Scan error: unterminated string".to_string());
            }
            Ok(idx)
        }
        SyntaxClass::Word | SyntaxClass::Symbol => {
            // Scan backward over word/symbol chars.
            while idx > 0
                && matches!(
                    table.char_syntax(chars[idx - 1]),
                    SyntaxClass::Word | SyntaxClass::Symbol
                )
            {
                idx -= 1;
            }
            Ok(idx)
        }
        SyntaxClass::Escape | SyntaxClass::CharQuote => {
            // The escape char itself is a sexp.
            Ok(idx)
        }
        SyntaxClass::MathDelim => {
            let delim = ch;
            if idx == 0 {
                return Err("Scan error: unterminated math delimiter".to_string());
            }
            idx -= 1;
            while idx > 0 && chars[idx] != delim {
                idx -= 1;
            }
            if chars[idx] != delim {
                return Err("Scan error: unterminated math delimiter".to_string());
            }
            Ok(idx)
        }
        _ => {
            // Single char sexp.
            Ok(idx)
        }
    }
}

// ===========================================================================
// Builtin functions (pure — no evaluator needed)
// ===========================================================================

/// `(string-to-syntax S)` — parse a syntax descriptor string.
pub(crate) fn builtin_string_to_syntax(args: Vec<Value>) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("string-to-syntax"), Value::Int(args.len() as i64)],
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ));
        }
    };
    let entry = string_to_syntax(&s).map_err(|msg| {
        signal("error", vec![Value::string(&msg)])
    })?;
    Ok(syntax_entry_to_value(&entry))
}

/// `(make-syntax-table &optional PARENT)` — create a new syntax table.
/// Returns `t` (the table is attached via side effects in Emacs; here we
/// return a symbol placeholder since we store it on the buffer).
pub(crate) fn builtin_make_syntax_table(_args: Vec<Value>) -> EvalResult {
    // In this VM, syntax tables live on buffers.  `make-syntax-table`
    // is provided for compatibility but returns t.
    Ok(Value::True)
}

// ===========================================================================
// Builtin functions (evaluator-dependent — operate on current buffer)
// ===========================================================================

/// `(modify-syntax-entry CHAR NEWENTRY &optional SYNTAX-TABLE)`
pub(crate) fn builtin_modify_syntax_entry(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() < 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("modify-syntax-entry"), Value::Int(args.len() as i64)],
        ));
    }
    let ch = match &args[0] {
        Value::Char(c) => *c,
        Value::Int(n) => {
            char::from_u32(*n as u32).ok_or_else(|| {
                signal("error", vec![Value::string(&format!("Invalid character code: {}", n))])
            })?
        }
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("characterp"), other.clone()],
            ));
        }
    };
    let descriptor = match &args[1] {
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ));
        }
    };
    let entry = string_to_syntax(&descriptor).map_err(|msg| {
        signal("error", vec![Value::string(&msg)])
    })?;

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.syntax_table.modify_syntax_entry(ch, entry);
    Ok(Value::Nil)
}

/// `(char-syntax CHAR)` — return the syntax class designator char.
pub(crate) fn builtin_char_syntax(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("char-syntax"), Value::Int(args.len() as i64)],
        ));
    }
    let ch = match &args[0] {
        Value::Char(c) => *c,
        Value::Int(n) => {
            char::from_u32(*n as u32).ok_or_else(|| {
                signal("error", vec![Value::string(&format!("Invalid character code: {}", n))])
            })?
        }
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("characterp"), other.clone()],
            ));
        }
    };

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let class = buf.syntax_table.char_syntax(ch);
    Ok(Value::Char(class.to_char()))
}

/// `(forward-word &optional COUNT)` — move point forward COUNT words.
pub(crate) fn builtin_forward_word(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        match &args[0] {
            Value::Int(n) => *n,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), other.clone()],
                ));
            }
        }
    };

    // We need to read the syntax table first, then call forward_word, then write point.
    // To satisfy the borrow checker, clone the syntax table.
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = buf.syntax_table.clone();
    let new_pos = forward_word(buf, &table, count);

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.goto_char(new_pos);
    Ok(Value::Nil)
}

/// `(backward-word &optional COUNT)` — move point backward COUNT words.
pub(crate) fn builtin_backward_word(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        match &args[0] {
            Value::Int(n) => *n,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), other.clone()],
                ));
            }
        }
    };

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = buf.syntax_table.clone();
    let new_pos = backward_word(buf, &table, count);

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.goto_char(new_pos);
    Ok(Value::Nil)
}

/// `(forward-sexp &optional COUNT)` — move point forward over COUNT balanced
/// expressions.
pub(crate) fn builtin_forward_sexp(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1i64
    } else {
        match &args[0] {
            Value::Int(n) => *n,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), other.clone()],
                ));
            }
        }
    };

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = buf.syntax_table.clone();
    let from = buf.point();
    let new_pos = scan_sexps(buf, &table, from, count).map_err(|msg| {
        signal("scan-error", vec![Value::string(&msg)])
    })?;

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.goto_char(new_pos);
    Ok(Value::Nil)
}

/// `(backward-sexp &optional COUNT)` — move point backward over COUNT balanced
/// expressions.
pub(crate) fn builtin_backward_sexp(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1i64
    } else {
        match &args[0] {
            Value::Int(n) => *n,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), other.clone()],
                ));
            }
        }
    };

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = buf.syntax_table.clone();
    let from = buf.point();
    // backward-sexp with positive count => scan_sexps with negative count
    let new_pos = scan_sexps(buf, &table, from, -count).map_err(|msg| {
        signal("scan-error", vec![Value::string(&msg)])
    })?;

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.goto_char(new_pos);
    Ok(Value::Nil)
}

/// `(skip-syntax-forward SYNTAX &optional LIMIT)` — skip forward over chars
/// matching the given syntax classes.
pub(crate) fn builtin_skip_syntax_forward(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("skip-syntax-forward"), Value::Int(0)],
        ));
    }
    let syntax_chars = match &args[0] {
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ));
        }
    };
    let limit = if args.len() > 1 && !args[1].is_nil() {
        match &args[1] {
            Value::Int(n) => Some(*n as usize),
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), other.clone()],
                ));
            }
        }
    } else {
        None
    };

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = buf.syntax_table.clone();
    let new_pos = skip_syntax_forward(buf, &table, &syntax_chars, limit);

    let old_pt = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        .point();

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.goto_char(new_pos);

    // Return number of characters skipped (Emacs convention).
    let chars_moved = if new_pos >= old_pt {
        buf.text.byte_to_char(new_pos) as i64 - buf.text.byte_to_char(old_pt) as i64
    } else {
        buf.text.byte_to_char(old_pt) as i64 - buf.text.byte_to_char(new_pos) as i64
    };
    Ok(Value::Int(chars_moved))
}

/// `(skip-syntax-backward SYNTAX &optional LIMIT)` — skip backward over chars
/// matching the given syntax classes.
pub(crate) fn builtin_skip_syntax_backward(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("skip-syntax-backward"), Value::Int(0)],
        ));
    }
    let syntax_chars = match &args[0] {
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ));
        }
    };
    let limit = if args.len() > 1 && !args[1].is_nil() {
        match &args[1] {
            Value::Int(n) => Some(*n as usize),
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), other.clone()],
                ));
            }
        }
    } else {
        None
    };

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = buf.syntax_table.clone();
    let new_pos = skip_syntax_backward(buf, &table, &syntax_chars, limit);

    let old_pt = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        .point();

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.goto_char(new_pos);

    // Return negative number of characters skipped.
    let chars_moved = if old_pt >= new_pos {
        -(buf.text.byte_to_char(old_pt) as i64 - buf.text.byte_to_char(new_pos) as i64)
    } else {
        buf.text.byte_to_char(new_pos) as i64 - buf.text.byte_to_char(old_pt) as i64
    };
    Ok(Value::Int(chars_moved))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::buffer::{Buffer, BufferId};
    use crate::buffer::gap_buffer::GapBuffer;

    /// Helper: create a buffer with given text, point at start, full accessible range.
    fn buf_with_text(text: &str) -> Buffer {
        let mut buf = Buffer::new(BufferId(99), "test-syntax".into());
        buf.text = GapBuffer::from_str(text);
        buf.zv = buf.text.len();
        buf.pt = 0;
        buf
    }

    // -----------------------------------------------------------------------
    // SyntaxClass parsing
    // -----------------------------------------------------------------------

    #[test]
    fn syntax_class_roundtrip() {
        let classes = [
            (' ', SyntaxClass::Whitespace),
            ('w', SyntaxClass::Word),
            ('_', SyntaxClass::Symbol),
            ('.', SyntaxClass::Punctuation),
            ('(', SyntaxClass::Open),
            (')', SyntaxClass::Close),
            ('"', SyntaxClass::StringDelim),
            ('$', SyntaxClass::MathDelim),
            ('\\', SyntaxClass::Escape),
            ('/', SyntaxClass::CharQuote),
            ('<', SyntaxClass::Comment),
            ('>', SyntaxClass::EndComment),
            ('@', SyntaxClass::InheritStandard),
            ('!', SyntaxClass::Generic),
        ];
        for (ch, class) in &classes {
            assert_eq!(SyntaxClass::from_char(*ch), Some(*class));
            assert_eq!(class.to_char(), *ch);
        }
    }

    #[test]
    fn syntax_class_dash_is_whitespace() {
        assert_eq!(SyntaxClass::from_char('-'), Some(SyntaxClass::Whitespace));
    }

    // -----------------------------------------------------------------------
    // string-to-syntax parser
    // -----------------------------------------------------------------------

    #[test]
    fn string_to_syntax_whitespace() {
        let entry = string_to_syntax(" ").unwrap();
        assert_eq!(entry.class, SyntaxClass::Whitespace);
        assert_eq!(entry.matching_char, None);
        assert!(entry.flags.is_empty());
    }

    #[test]
    fn string_to_syntax_word() {
        let entry = string_to_syntax("w").unwrap();
        assert_eq!(entry.class, SyntaxClass::Word);
    }

    #[test]
    fn string_to_syntax_open_paren() {
        let entry = string_to_syntax("()").unwrap();
        assert_eq!(entry.class, SyntaxClass::Open);
        assert_eq!(entry.matching_char, Some(')'));
    }

    #[test]
    fn string_to_syntax_close_paren() {
        let entry = string_to_syntax(")(").unwrap();
        assert_eq!(entry.class, SyntaxClass::Close);
        assert_eq!(entry.matching_char, Some('('));
    }

    #[test]
    fn string_to_syntax_string_delim() {
        let entry = string_to_syntax("\"").unwrap();
        assert_eq!(entry.class, SyntaxClass::StringDelim);
    }

    #[test]
    fn string_to_syntax_with_flags() {
        let entry = string_to_syntax(". 12").unwrap();
        assert_eq!(entry.class, SyntaxClass::Punctuation);
        assert_eq!(entry.matching_char, None);
        assert!(entry.flags.contains(SyntaxFlags::COMMENT_START_FIRST));
        assert!(entry.flags.contains(SyntaxFlags::COMMENT_START_SECOND));
    }

    #[test]
    fn string_to_syntax_comment_style_b() {
        let entry = string_to_syntax(". 12b").unwrap();
        assert!(entry.flags.contains(SyntaxFlags::COMMENT_STYLE_B));
    }

    #[test]
    fn string_to_syntax_prefix_flag() {
        let entry = string_to_syntax(". p").unwrap();
        assert_eq!(entry.class, SyntaxClass::Punctuation);
        assert!(entry.flags.contains(SyntaxFlags::PREFIX));
    }

    #[test]
    fn string_to_syntax_empty_errors() {
        assert!(string_to_syntax("").is_err());
    }

    #[test]
    fn string_to_syntax_invalid_class() {
        assert!(string_to_syntax("Z").is_err());
    }

    // -----------------------------------------------------------------------
    // SyntaxTable
    // -----------------------------------------------------------------------

    #[test]
    fn standard_table_word_chars() {
        let table = SyntaxTable::new_standard();
        assert_eq!(table.char_syntax('a'), SyntaxClass::Word);
        assert_eq!(table.char_syntax('Z'), SyntaxClass::Word);
        assert_eq!(table.char_syntax('5'), SyntaxClass::Word);
    }

    #[test]
    fn standard_table_whitespace() {
        let table = SyntaxTable::new_standard();
        assert_eq!(table.char_syntax(' '), SyntaxClass::Whitespace);
        assert_eq!(table.char_syntax('\t'), SyntaxClass::Whitespace);
        assert_eq!(table.char_syntax('\n'), SyntaxClass::Whitespace);
    }

    #[test]
    fn standard_table_parens() {
        let table = SyntaxTable::new_standard();
        assert_eq!(table.char_syntax('('), SyntaxClass::Open);
        assert_eq!(table.char_syntax(')'), SyntaxClass::Close);
        assert_eq!(table.char_syntax('['), SyntaxClass::Open);
        assert_eq!(table.char_syntax(']'), SyntaxClass::Close);
    }

    #[test]
    fn standard_table_string_delim() {
        let table = SyntaxTable::new_standard();
        assert_eq!(table.char_syntax('"'), SyntaxClass::StringDelim);
    }

    #[test]
    fn standard_table_escape() {
        let table = SyntaxTable::new_standard();
        assert_eq!(table.char_syntax('\\'), SyntaxClass::Escape);
    }

    #[test]
    fn standard_table_punctuation() {
        let table = SyntaxTable::new_standard();
        assert_eq!(table.char_syntax('+'), SyntaxClass::Punctuation);
        assert_eq!(table.char_syntax('='), SyntaxClass::Punctuation);
    }

    #[test]
    fn standard_table_symbol_constituents() {
        let table = SyntaxTable::new_standard();
        assert_eq!(table.char_syntax('_'), SyntaxClass::Symbol);
        assert_eq!(table.char_syntax('-'), SyntaxClass::Symbol);
    }

    #[test]
    fn modify_syntax_entry_overrides() {
        let mut table = SyntaxTable::new_standard();
        assert_eq!(table.char_syntax('+'), SyntaxClass::Punctuation);
        table.modify_syntax_entry('+', SyntaxEntry::simple(SyntaxClass::Word));
        assert_eq!(table.char_syntax('+'), SyntaxClass::Word);
    }

    #[test]
    fn inherited_table_falls_back() {
        let table = SyntaxTable::make_syntax_table();
        // Should inherit from standard.
        assert_eq!(table.char_syntax('a'), SyntaxClass::Word);
        assert_eq!(table.char_syntax(' '), SyntaxClass::Whitespace);
    }

    #[test]
    fn inherited_table_override() {
        let mut table = SyntaxTable::make_syntax_table();
        table.modify_syntax_entry('a', SyntaxEntry::simple(SyntaxClass::Punctuation));
        assert_eq!(table.char_syntax('a'), SyntaxClass::Punctuation);
        // Other inherited entries still work.
        assert_eq!(table.char_syntax('b'), SyntaxClass::Word);
    }

    #[test]
    fn copy_syntax_table_is_independent() {
        let original = SyntaxTable::new_standard();
        let mut copy = original.copy_syntax_table();
        copy.modify_syntax_entry('a', SyntaxEntry::simple(SyntaxClass::Punctuation));
        assert_eq!(original.char_syntax('a'), SyntaxClass::Word);
        assert_eq!(copy.char_syntax('a'), SyntaxClass::Punctuation);
    }

    #[test]
    fn unknown_char_defaults_to_symbol() {
        let table = SyntaxTable::new_standard();
        // A random Unicode character not in the table.
        assert_eq!(table.char_syntax('\u{1F600}'), SyntaxClass::Symbol);
    }

    // -----------------------------------------------------------------------
    // forward_word / backward_word
    // -----------------------------------------------------------------------

    #[test]
    fn forward_word_basic() {
        let mut buf = buf_with_text("hello world");
        buf.pt = 0;
        let table = SyntaxTable::new_standard();
        let pos = forward_word(&buf, &table, 1);
        // "hello" ends at byte 5.
        assert_eq!(pos, 5);
    }

    #[test]
    fn forward_word_two() {
        let mut buf = buf_with_text("hello world");
        buf.pt = 0;
        let table = SyntaxTable::new_standard();
        let pos = forward_word(&buf, &table, 2);
        // Past "hello world" = byte 11.
        assert_eq!(pos, 11);
    }

    #[test]
    fn forward_word_from_middle() {
        let mut buf = buf_with_text("hello world");
        buf.pt = 3; // inside "hello"
        let table = SyntaxTable::new_standard();
        let pos = forward_word(&buf, &table, 1);
        assert_eq!(pos, 5); // end of "hello"
    }

    #[test]
    fn backward_word_basic() {
        let mut buf = buf_with_text("hello world");
        buf.pt = 11; // end of text
        let table = SyntaxTable::new_standard();
        let pos = backward_word(&buf, &table, 1);
        assert_eq!(pos, 6); // start of "world"
    }

    #[test]
    fn backward_word_two() {
        let mut buf = buf_with_text("hello world");
        buf.pt = 11;
        let table = SyntaxTable::new_standard();
        let pos = backward_word(&buf, &table, 2);
        assert_eq!(pos, 0); // start of "hello"
    }

    #[test]
    fn forward_word_negative_goes_backward() {
        let mut buf = buf_with_text("hello world");
        buf.pt = 11;
        let table = SyntaxTable::new_standard();
        let pos = forward_word(&buf, &table, -1);
        assert_eq!(pos, 6);
    }

    // -----------------------------------------------------------------------
    // skip_syntax_forward / skip_syntax_backward
    // -----------------------------------------------------------------------

    #[test]
    fn skip_syntax_forward_word() {
        let mut buf = buf_with_text("hello world");
        buf.pt = 0;
        let table = SyntaxTable::new_standard();
        let pos = skip_syntax_forward(&buf, &table, "w", None);
        assert_eq!(pos, 5); // end of "hello"
    }

    #[test]
    fn skip_syntax_forward_whitespace_and_word() {
        let mut buf = buf_with_text("  hello");
        buf.pt = 0;
        let table = SyntaxTable::new_standard();
        let pos = skip_syntax_forward(&buf, &table, " w", None);
        assert_eq!(pos, 7); // end of "  hello"
    }

    #[test]
    fn skip_syntax_backward_word() {
        let mut buf = buf_with_text("hello world");
        buf.pt = 11;
        let table = SyntaxTable::new_standard();
        let pos = skip_syntax_backward(&buf, &table, "w", None);
        assert_eq!(pos, 6); // start of "world"
    }

    #[test]
    fn skip_syntax_forward_with_limit() {
        let mut buf = buf_with_text("helloworld");
        buf.pt = 0;
        let table = SyntaxTable::new_standard();
        let pos = skip_syntax_forward(&buf, &table, "w", Some(3));
        assert_eq!(pos, 3);
    }

    // -----------------------------------------------------------------------
    // scan_sexps (balanced expressions)
    // -----------------------------------------------------------------------

    #[test]
    fn scan_sexps_forward_parens() {
        let buf = buf_with_text("(hello world)");
        let table = SyntaxTable::new_standard();
        let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
        assert_eq!(pos, 13); // past closing paren
    }

    #[test]
    fn scan_sexps_forward_nested() {
        let buf = buf_with_text("(a (b c) d)");
        let table = SyntaxTable::new_standard();
        let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
        assert_eq!(pos, 11);
    }

    #[test]
    fn scan_sexps_forward_word() {
        let buf = buf_with_text("hello world");
        let table = SyntaxTable::new_standard();
        let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
        assert_eq!(pos, 5); // end of "hello"
    }

    #[test]
    fn scan_sexps_forward_string() {
        let buf = buf_with_text("\"hello\" world");
        let table = SyntaxTable::new_standard();
        let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
        assert_eq!(pos, 7); // past closing quote
    }

    #[test]
    fn scan_sexps_backward_parens() {
        let buf = buf_with_text("(hello world)");
        let table = SyntaxTable::new_standard();
        // Start after closing paren.
        let pos = scan_sexps(&buf, &table, 13, -1).unwrap();
        assert_eq!(pos, 0); // back to opening paren
    }

    #[test]
    fn scan_sexps_forward_unbalanced() {
        let buf = buf_with_text("(hello");
        let table = SyntaxTable::new_standard();
        assert!(scan_sexps(&buf, &table, 0, 1).is_err());
    }

    #[test]
    fn scan_sexps_backward_unbalanced() {
        let buf = buf_with_text("hello)");
        let table = SyntaxTable::new_standard();
        assert!(scan_sexps(&buf, &table, 6, -1).is_err());
    }

    #[test]
    fn scan_sexps_zero_count() {
        let buf = buf_with_text("(hello)");
        let table = SyntaxTable::new_standard();
        let pos = scan_sexps(&buf, &table, 3, 0).unwrap();
        assert_eq!(pos, 3); // unchanged
    }

    #[test]
    fn scan_sexps_forward_brackets() {
        let buf = buf_with_text("[a b c]");
        let table = SyntaxTable::new_standard();
        let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
        assert_eq!(pos, 7);
    }

    #[test]
    fn scan_sexps_string_with_escape() {
        let buf = buf_with_text("\"he\\\"llo\" world");
        let table = SyntaxTable::new_standard();
        let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
        assert_eq!(pos, 9); // past the closing quote
    }

    // -----------------------------------------------------------------------
    // syntax_entry_to_value
    // -----------------------------------------------------------------------

    #[test]
    fn syntax_entry_to_value_simple() {
        let entry = SyntaxEntry::simple(SyntaxClass::Word);
        let val = syntax_entry_to_value(&entry);
        // Should be (2 . nil) since Word code = 2
        if let Value::Cons(cell) = &val {
            let cell = cell.lock().unwrap();
            assert!(matches!(cell.car, Value::Int(2)));
            assert!(matches!(cell.cdr, Value::Nil));
        } else {
            panic!("Expected cons cell");
        }
    }

    #[test]
    fn syntax_entry_to_value_with_match() {
        let entry = SyntaxEntry::with_match(SyntaxClass::Open, ')');
        let val = syntax_entry_to_value(&entry);
        if let Value::Cons(cell) = &val {
            let cell = cell.lock().unwrap();
            assert!(matches!(cell.car, Value::Int(4))); // Open code = 4
            assert!(matches!(cell.cdr, Value::Int(41))); // ')' = 41
        } else {
            panic!("Expected cons cell");
        }
    }

    #[test]
    fn syntax_entry_to_value_with_flags() {
        let entry = SyntaxEntry {
            class: SyntaxClass::Punctuation,
            matching_char: None,
            flags: SyntaxFlags::COMMENT_START_FIRST | SyntaxFlags::COMMENT_START_SECOND,
        };
        let val = syntax_entry_to_value(&entry);
        if let Value::Cons(cell) = &val {
            let cell = cell.lock().unwrap();
            // code = 1 (punctuation) | (0x03 << 16) = 1 | 196608 = 196609
            assert!(matches!(cell.car, Value::Int(196609)));
        } else {
            panic!("Expected cons cell");
        }
    }
}
