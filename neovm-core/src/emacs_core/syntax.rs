//! Syntax table system for the Elisp VM.
//!
//! Implements Emacs-compatible syntax tables with character classification,
//! motion functions (forward/backward word, sexp scanning), and the
//! `string-to-syntax` descriptor parser.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Deref;

use num_enum::{IntoPrimitive, TryFromPrimitive};
use strum::{EnumString, IntoStaticStr};

use super::error::{EvalResult, Flow, signal};
use super::intern::resolve_sym;
use super::value::{RuntimeBindingValue, Value, ValueKind, list_to_vec};
use crate::buffer::{
    Buffer, BufferManager, CharLen, CharPos0, EmacsByteLen, EmacsBytePos, EmacsByteRange,
    LispCharPos1,
};

#[inline]
fn buffer_byte_to_char_pos(buf: &Buffer, byte_pos: EmacsBytePos) -> usize {
    buf.emacs_byte_pos_to_char_pos_clamped(byte_pos).get()
}

#[inline]
fn buffer_char_to_emacs_byte_pos(buf: &Buffer, char_pos: CharPos0) -> EmacsBytePos {
    buf.char_pos_to_emacs_byte_pos_clamped(char_pos)
}

#[inline]
fn offset_char_pos(base: CharPos0, idx: usize) -> CharPos0 {
    base.add_len(CharLen::new(idx))
}

#[inline]
fn char_pos_to_lisp_i64(char_pos: usize) -> i64 {
    CharPos0::new(char_pos).to_lisp().as_i64()
}

#[derive(Clone, Copy)]
struct BufferSyntaxChar {
    ch: char,
    start: EmacsBytePos,
    end: EmacsBytePos,
}

impl BufferSyntaxChar {
    #[inline]
    fn byte_len(self) -> EmacsByteLen {
        self.end
            .saturating_offset_from(self.start)
            .max(EmacsByteLen::new(1))
    }
}

#[inline]
fn buffer_syntax_char_after(buf: &Buffer, byte_pos: EmacsBytePos) -> Option<BufferSyntaxChar> {
    let ch = buf.char_after_emacs_byte_pos(byte_pos)?;
    let len = buf
        .char_after_emacs_byte_len(byte_pos)
        .map(|len| len.max(EmacsByteLen::new(1)))
        .unwrap_or_else(|| EmacsByteLen::new(ch.len_utf8().max(1)));
    Some(BufferSyntaxChar {
        ch,
        start: byte_pos,
        end: byte_pos.add_len(len),
    })
}

#[inline]
fn buffer_syntax_char_before(buf: &Buffer, byte_pos: EmacsBytePos) -> Option<BufferSyntaxChar> {
    let ch = buf.char_before_emacs_byte_pos(byte_pos)?;
    let len = buf
        .char_before_emacs_byte_len(byte_pos)
        .map(|len| len.max(EmacsByteLen::new(1)))
        .unwrap_or_else(|| EmacsByteLen::new(ch.len_utf8().max(1)));
    Some(BufferSyntaxChar {
        ch,
        start: byte_pos.saturating_sub_len(len),
        end: byte_pos,
    })
}

#[inline]
fn buffer_byte_to_lisp_pos(buf: &Buffer, byte_pos: EmacsBytePos) -> i64 {
    buf.emacs_byte_pos_to_char_pos_clamped(byte_pos)
        .to_lisp()
        .as_i64()
}

#[inline]
fn buffer_byte_char_delta(buf: &Buffer, from: usize, to: usize) -> i64 {
    buffer_byte_to_char_pos(buf, EmacsBytePos::new(to)) as i64
        - buffer_byte_to_char_pos(buf, EmacsBytePos::new(from)) as i64
}

thread_local! {
    static STANDARD_SYNTAX_TABLE_OBJECT: RefCell<Option<Value>> = const { RefCell::new(None) };
    static SYNTAX_CODE_OBJECTS: RefCell<Option<Value>> = const { RefCell::new(None) };
}

/// Clear cached thread-local syntax table (must be called when heap changes).
pub fn reset_syntax_thread_locals() {
    STANDARD_SYNTAX_TABLE_OBJECT.with(|slot| *slot.borrow_mut() = None);
    SYNTAX_CODE_OBJECTS.with(|slot| *slot.borrow_mut() = None);
}

/// Restore the canonical standard syntax-table object for the current thread.
///
/// GNU Emacs keeps the standard syntax table as a single canonical Lisp object.
/// NeoVM exposes it through a thread-local cache because `standard-syntax-table`
/// is currently a no-evaluator builtin; callers that reconstruct or move an
/// `Context` between threads must restore that identity explicitly.
pub(crate) fn restore_standard_syntax_table_object(table: Value) {
    STANDARD_SYNTAX_TABLE_OBJECT.with(|slot| *slot.borrow_mut() = Some(table));
}

/// Restore GNU's canonical vector of bare syntax descriptor objects.
pub(crate) fn restore_syntax_code_objects(objects: Value) {
    SYNTAX_CODE_OBJECTS.with(|slot| *slot.borrow_mut() = Some(objects));
}

/// Snapshot the current thread's canonical standard syntax-table object.
pub(crate) fn snapshot_standard_syntax_table_object() -> Option<Value> {
    STANDARD_SYNTAX_TABLE_OBJECT.with(|slot| *slot.borrow())
}

/// Snapshot GNU's canonical vector of bare syntax descriptor objects.
pub(crate) fn snapshot_syntax_code_objects() -> Option<Value> {
    SYNTAX_CODE_OBJECTS.with(|slot| *slot.borrow())
}

/// Collect GC roots from the cached syntax table.
pub fn collect_syntax_gc_roots(roots: &mut Vec<Value>) {
    STANDARD_SYNTAX_TABLE_OBJECT.with(|slot| {
        if let Some(v) = *slot.borrow() {
            roots.push(v);
        }
    });
    SYNTAX_CODE_OBJECTS.with(|slot| {
        if let Some(v) = *slot.borrow() {
            roots.push(v);
        }
    });
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum SyntaxPurposeSymbol {
    SyntaxTable,
}

impl SyntaxPurposeSymbol {
    fn from_lisp_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

// Phase 10D holdout 3: the per-buffer syntax table char-table now lives in
// `Buffer::slots[BUFFER_SLOT_SYNTAX_TABLE.index()]`, mirroring GNU's
// `BVAR(buf, syntax_table)` storage. Reads go through `slots[offset]`,
// writes go through `slots[offset]` plus `set_slot_local_flag` (matching
// `Fset_syntax_table`'s `SET_PER_BUFFER_VALUE_P`). The slot itself is
// non-Lisp-visible (`install_as_forwarder: false`), so the symbol
// `syntax-table` continues to signal void-variable as in GNU.

/// Pre-populate GNU Emacs syntax variables that are defined from C.
pub fn init_syntax_vars(
    obarray: &mut super::symbol::Obarray,
    _custom: &mut super::custom::CustomManager,
) {
    obarray.set_symbol_value("parse-sexp-ignore-comments", Value::NIL);
    obarray.set_symbol_value("parse-sexp-lookup-properties", Value::NIL);
    obarray.set_symbol_value("syntax-propertize--done", Value::fixnum(-1));
    obarray.set_symbol_value("words-include-escapes", Value::NIL);
    obarray.set_symbol_value("multibyte-syntax-as-symbol", Value::NIL);
    obarray.set_symbol_value("open-paren-in-column-0-is-defun-start", Value::T);
    obarray.set_symbol_value(
        "find-word-boundary-function-table",
        super::chartable::make_char_table_value(Value::NIL, Value::NIL),
    );
    obarray.set_symbol_value("comment-end-can-be-escaped", Value::NIL);
    obarray.set_symbol_value("forward-comment-function", Value::NIL);

    for name in &[
        "parse-sexp-ignore-comments",
        "parse-sexp-lookup-properties",
        "syntax-propertize--done",
        "words-include-escapes",
        "multibyte-syntax-as-symbol",
        "open-paren-in-column-0-is-defun-start",
        "find-word-boundary-function-table",
        "comment-end-can-be-escaped",
        "forward-comment-function",
    ] {
        obarray.make_special(name);
    }

    // Mirrors GNU `Fmake_variable_buffer_local` (`data.c:2142-2207`):
    // flip the redirect tag to LOCALIZED, allocate a BLV, set
    // local_if_set = 1. The legacy `obarray.make_buffer_local`
    // helper used to be called here too but it overwrites the
    // freshly-set LOCALIZED redirect back to PLAINVAL and
    // orphans the BLV.
    for name in ["syntax-propertize--done", "comment-end-can-be-escaped"] {
        let id = crate::emacs_core::intern::intern(name);
        let default = obarray
            .find_symbol_value(id)
            .unwrap_or(crate::emacs_core::value::Value::NIL);
        obarray.make_symbol_localized(id, default);
        obarray.set_blv_local_if_set(id, true);
    }
}

// ===========================================================================
// Syntax classes
// ===========================================================================

/// Emacs syntax classes, matching GNU's `enum syntaxcode` from `syntax.h`.
///
/// Discriminant values match the GNU numbering (0–15) so the enum can be
/// cast to `u8` and used directly in bytecode (e.g. the regex engine's
/// `SyntaxSpec` / `NotSyntaxSpec` opcodes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum SyntaxClass {
    /// ' ' — Whitespace (Swhitespace = 0)
    Whitespace = 0,
    /// '.' — Punctuation (Spunct = 1)
    Punctuation = 1,
    /// 'w' — Word constituent (Sword = 2)
    Word = 2,
    /// '_' — Symbol constituent (Ssymbol = 3)
    Symbol = 3,
    /// '(' — Open parenthesis/bracket (Sopen = 4)
    Open = 4,
    /// ')' — Close parenthesis/bracket (Sclose = 5)
    Close = 5,
    /// '\'' — Expression prefix (Squote = 6)
    Quote = 6,
    /// '"' — String delimiter (Sstring = 7)
    StringDelim = 7,
    /// '$' — Math delimiter, paired (Smath = 8)
    Math = 8,
    /// '\\' — Escape character (Sescape = 9)
    Escape = 9,
    /// '/' — Character quote, only quotes the next character (Scharquote = 10)
    CharQuote = 10,
    /// '<' — Comment starter (Scomment = 11)
    Comment = 11,
    /// '>' — Comment ender (Sendcomment = 12)
    EndComment = 12,
    /// '@' — Inherit from standard syntax table (Sinherit = 13)
    InheritStd = 13,
    /// '!' — Generic comment delimiter / comment fence (Scomment_fence = 14)
    CommentFence = 14,
    /// '|' — Generic string fence (Sstring_fence = 15)
    StringFence = 15,
}

const SYNTAX_CLASS_COUNT: usize = 16;
const SYNTAX_CLASS_DESIGNATORS: [char; SYNTAX_CLASS_COUNT] = [
    ' ', '.', 'w', '_', '(', ')', '\'', '"', '$', '\\', '/', '<', '>', '@', '!', '|',
];

impl SyntaxClass {
    /// Parse a GNU syntax descriptor byte, matching
    /// `src/syntax.c:syntax_spec_code`.
    pub fn from_syntax_spec_byte(byte: u8) -> Option<SyntaxClass> {
        match byte {
            b' ' | b'-' => Some(SyntaxClass::Whitespace),
            b'w' => Some(SyntaxClass::Word),
            b'_' => Some(SyntaxClass::Symbol),
            b'.' => Some(SyntaxClass::Punctuation),
            b'(' => Some(SyntaxClass::Open),
            b')' => Some(SyntaxClass::Close),
            b'\'' => Some(SyntaxClass::Quote),
            b'"' => Some(SyntaxClass::StringDelim),
            b'$' => Some(SyntaxClass::Math),
            b'\\' => Some(SyntaxClass::Escape),
            b'/' => Some(SyntaxClass::CharQuote),
            b'<' => Some(SyntaxClass::Comment),
            b'>' => Some(SyntaxClass::EndComment),
            b'@' => Some(SyntaxClass::InheritStd),
            b'!' => Some(SyntaxClass::CommentFence),
            b'|' => Some(SyntaxClass::StringFence),
            _ => None,
        }
    }

    /// Parse a syntax class from its single-character designator.
    pub fn from_char(ch: char) -> Option<SyntaxClass> {
        let byte = u8::try_from(u32::from(ch)).ok()?;
        SyntaxClass::from_syntax_spec_byte(byte)
    }

    /// Return the canonical single-character designator for this class.
    #[inline]
    pub fn to_char(self) -> char {
        SYNTAX_CLASS_DESIGNATORS[usize::from(u8::from(self))]
    }

    /// Return the integer code Emacs uses for this syntax class
    /// (used in the cons cell returned by `string-to-syntax`).
    #[inline]
    pub fn code(self) -> i64 {
        i64::from(u8::from(self))
    }

    #[inline]
    fn from_gnu_discriminant(code: u8) -> Option<SyntaxClass> {
        SyntaxClass::try_from(code).ok()
    }

    /// Parse a syntax class from a syntax table entry code.
    ///
    /// GNU syntax table entries store flags above the low 8 bits; the class is
    /// extracted with `code & 0377`.
    pub fn from_code(n: i64) -> Option<SyntaxClass> {
        SyntaxClass::from_gnu_discriminant((n & 0xFF) as u8)
    }

    /// Parse a public syntax class integer, as accepted by
    /// `syntax-class-to-char`.  Unlike syntax table entries, GNU rejects raw
    /// integers outside 0..Smax here instead of masking flag bits.
    fn from_plain_code(n: i64) -> Option<SyntaxClass> {
        let code = u8::try_from(n).ok()?;
        SyntaxClass::from_gnu_discriminant(code)
    }
}

/// Return the GNU standard syntax-table class for an Emacs character
/// code, mirroring GNU `src/syntax.c:init_syntax_once`.
pub(crate) fn standard_syntax_class_for_code(code: u32) -> SyntaxClass {
    match code {
        0x00..=0x08 | 0x0b | 0x0e..=0x1f | 0x7f => SyntaxClass::Punctuation,
        0x09 | 0x0a | 0x0c | 0x0d | 0x20 => SyntaxClass::Whitespace,
        0x30..=0x39 | 0x41..=0x5a | 0x61..=0x7a | 0x24 | 0x25 => SyntaxClass::Word,
        0x28 | 0x5b | 0x7b => SyntaxClass::Open,
        0x29 | 0x5d | 0x7d => SyntaxClass::Close,
        0x22 => SyntaxClass::StringDelim,
        0x5c => SyntaxClass::Escape,
        0x26 | 0x2a | 0x2b | 0x2d | 0x2f | 0x3c | 0x3d | 0x3e | 0x5f | 0x7c => SyntaxClass::Symbol,
        0x21 | 0x23 | 0x27 | 0x2c | 0x2e | 0x3a | 0x3b | 0x3f | 0x40 | 0x5e | 0x60 | 0x7e => {
            SyntaxClass::Punctuation
        }
        0x80..=0x3F_FFFF => SyntaxClass::Word,
        _ => SyntaxClass::Whitespace,
    }
}

#[inline]
pub(crate) fn standard_syntax_class_for_char(ch: char) -> SyntaxClass {
    standard_syntax_class_for_code(ch as u32)
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
    pub const COMMENT_START_FIRST: SyntaxFlags = SyntaxFlags(0b0000_0001);
    /// '2' — second char of a two-char comment start sequence
    pub const COMMENT_START_SECOND: SyntaxFlags = SyntaxFlags(0b0000_0010);
    /// '3' — first char of a two-char comment end sequence
    pub const COMMENT_END_FIRST: SyntaxFlags = SyntaxFlags(0b0000_0100);
    /// '4' — second char of a two-char comment end sequence
    pub const COMMENT_END_SECOND: SyntaxFlags = SyntaxFlags(0b0000_1000);
    /// 'p' — prefix character (e.g., quote, backquote)
    pub const PREFIX: SyntaxFlags = SyntaxFlags(0b0001_0000);
    /// 'b' — belongs to alternative "b" comment style
    pub const COMMENT_STYLE_B: SyntaxFlags = SyntaxFlags(0b0010_0000);
    /// 'n' — nestable comment
    pub const COMMENT_NESTABLE: SyntaxFlags = SyntaxFlags(0b0100_0000);
    /// 'c' — belongs to alternative "c" comment style
    pub const COMMENT_STYLE_C: SyntaxFlags = SyntaxFlags(0b1000_0000);

    /// Construct from raw bits.
    pub const fn new(bits: u8) -> Self {
        SyntaxFlags(bits)
    }

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
    let descriptor = chars.first().copied().unwrap_or('\0');
    let class = SyntaxClass::from_char(descriptor)
        .ok_or_else(|| format!("Invalid syntax description letter: {descriptor}"))?;

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
            'c' => flags |= SyntaxFlags::COMMENT_STYLE_C,
            ' ' => {} // whitespace in flag area is ignored
            _ => {}   // Emacs silently ignores unknown flags
        }
    }

    Ok(SyntaxEntry {
        class,
        matching_char,
        flags,
    })
}

fn syntax_runtime_string(value: &Value) -> Result<String, Flow> {
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .ok_or_else(|| {
            signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), *value],
            )
        })
}

/// Convert a `SyntaxEntry` into the Emacs cons-cell representation
/// returned by `string-to-syntax`: `(CODE . MATCHING-CHAR-OR-NIL)`.
///
/// The CODE is computed as: `(class_code) | (flags << 16)`.
pub fn syntax_entry_to_value(entry: &SyntaxEntry) -> Value {
    let code = entry.class.code() | ((entry.flags.bits() as i64) << 16);
    if entry.matching_char.is_none()
        && (0..SYNTAX_CLASS_COUNT as i64).contains(&code)
        && let Some(cached) = syntax_code_object(code as usize)
    {
        return cached;
    }
    let matching = match entry.matching_char {
        Some(ch) => Value::fixnum(ch as i64),
        None => Value::NIL,
    };
    Value::cons(Value::fixnum(code), matching)
}

fn make_syntax_code_objects() -> Value {
    Value::vector(
        (0..SYNTAX_CLASS_COUNT)
            .map(|code| Value::cons(Value::fixnum(code as i64), Value::NIL))
            .collect(),
    )
}

pub(crate) fn ensure_syntax_code_objects() -> Value {
    SYNTAX_CODE_OBJECTS.with(|slot| {
        if let Some(objects) = *slot.borrow() {
            return objects;
        }
        let objects = make_syntax_code_objects();
        *slot.borrow_mut() = Some(objects);
        objects
    })
}

fn syntax_code_object(code: usize) -> Option<Value> {
    if code >= SYNTAX_CLASS_COUNT {
        return None;
    }
    ensure_syntax_code_objects()
        .as_vector_data()
        .and_then(|values| values.get(code).copied())
}

// ===========================================================================
// SyntaxTable
// ===========================================================================

/// An Emacs-style syntax table mapping characters to syntax entries.
///
/// Characters not explicitly set fall back to a parent table (if present)
/// or to the built-in standard defaults.
/// A Lisp-level syntax table: a thin wrapper around the chartable `Value`
/// stored in `buffer->syntax_table` / `buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()]`.
///
/// Mirrors GNU Emacs design: the chartable IS the runtime form. All
/// queries go through `CHAR_TABLE_REF(table, c)` (→ our
/// `syntax_{class,entry}_at_char`) on demand; no eagerly-compiled HashMap
/// shadow form is maintained.
///
/// The inner `Value` is `Value::NIL` in two situations:
/// (1) a freshly-constructed `SyntaxTable::new_standard()` before the
///     standard chartable is materialized by the evaluator, and
/// (2) pdump's placeholder before `sync_current_buffer_syntax_table_state`
///     re-attaches the live chartable from the buffer slot.
/// In both cases `char_syntax()` falls back to GNU's default (Word for
/// >= U+0080, Whitespace for < U+0080), matching `SYNTAX_ENTRY`'s nil
/// handling.
#[derive(Clone, Copy, Debug)]
pub struct SyntaxTable {
    chartable: Value,
}

impl SyntaxTable {
    // -- Construction --------------------------------------------------------

    /// Return a `SyntaxTable` backed by the standard chartable Value.
    /// Materializes the chartable on first call via
    /// `ensure_standard_syntax_table_object()` — the same one installed
    /// on new buffers by `current_buffer_syntax_table_object_in_buffers`.
    pub fn new_standard() -> Self {
        match ensure_standard_syntax_table_object() {
            Ok(table) => Self { chartable: table },
            // If we can't build the chartable (no thread-local state),
            // return a nil-backed placeholder — callers fall back to
            // GNU defaults via `char_syntax` / `get_entry`.
            Err(_) => Self {
                chartable: Value::NIL,
            },
        }
    }

    /// Same as `new_standard` — GNU's `make-syntax-table` with nil parent
    /// creates a fresh, empty chartable whose parent is the standard
    /// table. The distinction is handled at the chartable level by
    /// `builtin_make_syntax_table`.
    pub fn make_syntax_table() -> Self {
        Self::new_standard()
    }

    /// Build a `SyntaxTable` that reads from the given chartable `Value`.
    pub(crate) fn from_chartable(chartable: Value) -> Self {
        Self { chartable }
    }

    /// Build a `SyntaxTable` that reads directly from `buf`'s
    /// syntax-table slot. Mirrors GNU `BVAR (buf, syntax_table)`.
    /// Falls back to a nil-backed placeholder (GNU defaults) if the
    /// slot hasn't been seeded yet.
    pub fn for_buffer(buf: &crate::buffer::buffer::Buffer) -> Self {
        Self {
            chartable: buf.syntax_chartable(),
        }
    }

    /// Install an isolated copy of the standard chartable on `buf` so
    /// subsequent `modify_syntax_entry` calls don't leak into the
    /// shared standard. Returns the new `SyntaxTable`. Mirrors the
    /// GNU idiom `(set-syntax-table (copy-syntax-table))`.
    pub fn isolate_for_buffer(buf: &mut crate::buffer::buffer::Buffer) -> Self {
        use crate::buffer::buffer::BUFFER_SLOT_SYNTAX_TABLE;
        let slot = buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()];
        let source = if slot.is_nil() {
            ensure_standard_syntax_table_object().unwrap_or(Value::NIL)
        } else {
            slot
        };
        let own = if source.is_nil() {
            Value::NIL
        } else {
            builtin_copy_syntax_table(vec![source]).unwrap_or(source)
        };
        buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()] = own;
        Self { chartable: own }
    }

    /// Deep-copy the backing chartable, matching GNU `copy-syntax-table`
    /// (`syntax.c:265-282`). The copy is independent: mutations to
    /// either table do not affect the other.
    pub fn copy_syntax_table(&self) -> Self {
        if self.chartable.is_nil() {
            return self.clone();
        }
        match builtin_copy_syntax_table(vec![self.chartable]) {
            Ok(copy) => Self { chartable: copy },
            Err(_) => self.clone(),
        }
    }

    /// Return the chartable Value backing this table (may be `NIL` for
    /// a placeholder table — see type-level docs).
    pub(crate) fn chartable(&self) -> Value {
        self.chartable
    }

    // -- Queries -------------------------------------------------------------

    /// Return the syntax entry for `ch`, matching GNU
    /// `SYNTAX_ENTRY(c)`. Falls back to the standard chartable when
    /// the wrapper is nil-backed (handled by `syntax_entry_at_char`).
    pub fn get_entry(&self, ch: char) -> Option<SyntaxEntry> {
        self.get_entry_code(ch as u32)
    }

    /// Return the syntax entry for an Emacs character code.  GNU Emacs
    /// syntax tables are indexed by `CHAR_VALID_P` integer codes
    /// (`0..=MAX_CHAR`), not by Unicode scalar values; keep this path
    /// available for callers such as `char-syntax`.
    pub fn get_entry_code(&self, code: u32) -> Option<SyntaxEntry> {
        syntax_entry_at_char_code(&self.chartable, code)
    }

    /// Return the syntax class for `ch` — GNU `SYNTAX(c)`.
    pub fn char_syntax(&self, ch: char) -> SyntaxClass {
        self.char_syntax_code(ch as u32)
    }

    /// Return the syntax class for an Emacs character code — GNU
    /// `SYNTAX(c)`.
    pub fn char_syntax_code(&self, code: u32) -> SyntaxClass {
        syntax_class_at_char_code(&self.chartable, code)
    }

    // -- Mutation -------------------------------------------------------------

    /// Install `entry` for `ch` in the backing chartable. No-op when
    /// the table is a `NIL` placeholder — the evaluator's
    /// `modify-syntax-entry` builtin routes through the chartable
    /// directly for that case.
    pub fn modify_syntax_entry(&mut self, ch: char, entry: SyntaxEntry) {
        if self.chartable.is_nil() {
            return;
        }
        let _ = super::chartable::builtin_set_char_table_range(
            vec![
                self.chartable,
                Value::fixnum(ch as i64),
                syntax_entry_to_value(&entry),
            ],
            None,
        );
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

/// Move forward over `count` words.  Returns the resulting Emacs byte position.
///
/// A "word" is a maximal run of characters with syntax class `Word`.
/// Between words, non-word characters are skipped.
pub fn forward_word(buf: &Buffer, table: &SyntaxTable, count: i64) -> EmacsBytePos {
    forward_word_with_options(buf, table, count, false).0
}

fn syntax_char_from_code(code: u32) -> char {
    super::builtins::character_code_to_rust_char(code as i64).unwrap_or('\u{FFFD}')
}

/// On-demand syntax-char accessor over a buffer.
///
/// Replaces decoding the whole (accessible) buffer into a `Vec<char>` on every
/// syntax call -- which was O(buffer) per call and O(n^2) across a font-lock
/// pass.  Reading a char on demand is now cheap because byte<->char conversion
/// is cached (`gap_buffer`).  `char_at(idx)` returns the syntax char at logical
/// char position `base_char + idx`, so each caller keeps its existing
/// (range-relative or absolute) index convention; the base absorbs the
/// difference.
struct BufferChars<'a> {
    buf: &'a Buffer,
    base_char: CharPos0,
    multibyte: bool,
    /// Byte cursor for sequential reads: `(char_idx, emacs_byte_pos of that
    /// char, byte width of that char)`.  Lets a forward scan advance
    /// `byte += width` with ONE decode per char -- like GNU `syntax.c`
    /// `FETCH_CHAR`, which walks byte positions directly -- instead of a
    /// char->byte conversion per char.  Random access falls back to the
    /// (cached) conversion, so this only ever helps.
    cursor: Cell<Option<(usize, EmacsBytePos, EmacsByteLen)>>,
}

impl<'a> BufferChars<'a> {
    fn new(buf: &'a Buffer, base_char: CharPos0) -> Self {
        Self {
            buf,
            base_char,
            multibyte: buf.get_multibyte(),
            cursor: Cell::new(None),
        }
    }

    #[inline]
    fn char_at(&self, idx: usize) -> char {
        // For a forward step (or re-read of the same char) advance the byte
        // cursor directly; otherwise pay for one (cached) char->byte
        // conversion.  GNU's syntax scanners never convert per char -- they
        // carry the byte position and bump it by the char width.
        let byte_pos = match self.cursor.get() {
            Some((c_idx, c_byte, _)) if idx == c_idx => c_byte,
            Some((c_idx, c_byte, c_width)) if idx == c_idx + 1 => c_byte.add_len(c_width),
            _ => buffer_char_to_emacs_byte_pos(self.buf, self.base_char.add_len(CharLen::new(idx))),
        };
        let code = self.buf.char_code_at_emacs_byte_pos(byte_pos).unwrap_or(0);
        // A unibyte buffer stores one byte per char; a multibyte buffer stores
        // the char's internal multibyte length (raw bytes included -- see
        // `emacs_char::char_bytes`).
        let width = if self.multibyte {
            crate::emacs_core::emacs_char::char_bytes(code)
        } else {
            1
        };
        self.cursor
            .set(Some((idx, byte_pos, EmacsByteLen::new(width))));
        syntax_char_from_code(code)
    }
}

fn forward_word_with_options(
    buf: &Buffer,
    table: &SyntaxTable,
    count: i64,
    honor_properties: bool,
) -> (EmacsBytePos, bool) {
    if count < 0 {
        return backward_word_with_options(buf, table, -count, honor_properties);
    }

    let accessible_bytes = buf.accessible_emacs_byte_region();
    let accessible_chars = buf.accessible_char_region();
    let chars = BufferChars::new(buf, accessible_chars.start());
    let accessible_char_start = accessible_chars.start().get();
    let accessible_len = accessible_chars.len().get();
    let mut idx = buffer_byte_to_char_pos(buf, accessible_bytes.clamp(buf.point_emacs_byte_pos()))
        .saturating_sub(accessible_char_start);

    for _ in 0..count {
        // Skip non-word characters
        while idx < accessible_len
            && !matches!(
                effective_syntax_entry_for_abs_char(
                    buf,
                    table,
                    chars.char_at(idx),
                    accessible_char_start + idx,
                    honor_properties,
                )
                .class,
                SyntaxClass::Word
            )
        {
            idx += 1;
        }
        if idx == accessible_len {
            let abs_char = offset_char_pos(accessible_chars.start(), idx);
            return (buffer_char_to_emacs_byte_pos(buf, abs_char), false);
        }
        // Skip word characters
        while idx < accessible_len
            && matches!(
                effective_syntax_entry_for_abs_char(
                    buf,
                    table,
                    chars.char_at(idx),
                    accessible_char_start + idx,
                    honor_properties,
                )
                .class,
                SyntaxClass::Word
            )
        {
            idx += 1;
        }
    }

    // Convert char index back to byte position (absolute).
    let abs_char = offset_char_pos(accessible_chars.start(), idx);
    (buffer_char_to_emacs_byte_pos(buf, abs_char), true)
}

/// Move backward over `count` words.  Returns the resulting Emacs byte position.
pub fn backward_word(buf: &Buffer, table: &SyntaxTable, count: i64) -> EmacsBytePos {
    backward_word_with_options(buf, table, count, false).0
}

/// Whether `find-word-boundary-function-table` has any binding (i.e. some mode
/// like subword/superword installed boundary functions). When empty (the
/// common case) word motion uses the plain syntax scan unchanged.
fn word_boundary_table_active(table: &Value) -> bool {
    if !super::chartable::is_char_table(table) {
        return false;
    }
    // Probe a few representative word constituents; subword/superword install
    // the boundary function across word characters.
    [b'a' as i64, b'A' as i64, b'0' as i64, b'_' as i64]
        .into_iter()
        .any(|ch| {
            super::chartable::char_table_ref_and_range(table, ch)
                .map(|(v, _, _)| !v.is_nil())
                .unwrap_or(false)
        })
}

/// Move over `count` words honoring `find-word-boundary-function-table`
/// (GNU `scan_words`, syntax.c): after locating the character that begins (or,
/// going backward, ends) the next word, if that character has a bound boundary
/// function call it with (pos, limit) and jump to the returned boundary;
/// otherwise fall back to a one-word syntax scan. Returns the destination byte
/// position and whether all `count` motions completed.
fn word_motion_with_table(
    eval: &mut super::eval::Context,
    count: i64,
    honor_properties: bool,
    wbtable: Value,
) -> (EmacsBytePos, bool) {
    let forward = count > 0;
    let n = count.unsigned_abs();
    let mut completed = true;
    let current_id = match eval.buffers.current_buffer_id() {
        Some(id) => id,
        None => return (EmacsBytePos::new(0), false),
    };

    for _ in 0..n {
        // Locate the boundary character and its 1-based char position, plus the
        // accessible-region limit, all from the current point.
        let probe = {
            let buf = match eval.buffers.get(current_id) {
                Some(b) => b,
                None => return (EmacsBytePos::new(0), false),
            };
            let table = SyntaxTable::for_buffer(buf);
            let acc_chars = buf.accessible_char_region();
            let acc_start = acc_chars.start().get();
            let acc_len = acc_chars.len().get();
            let chars = BufferChars::new(buf, acc_chars.start());
            let point_char = buffer_byte_to_char_pos(buf, buf.point_emacs_byte_pos());
            let mut idx = point_char.saturating_sub(acc_start);
            let is_word = |i: usize| {
                matches!(
                    effective_syntax_entry_for_abs_char(
                        buf,
                        &table,
                        chars.char_at(i),
                        acc_start + i,
                        honor_properties,
                    )
                    .class,
                    SyntaxClass::Word
                )
            };
            if forward {
                while idx < acc_len && !is_word(idx) {
                    idx += 1;
                }
                if idx >= acc_len {
                    None
                } else {
                    // ch0 begins a word; its 1-based char position.
                    let ch0 = chars.char_at(idx);
                    let pos1 = (acc_start + idx + 1) as i64;
                    let limit1 = (acc_start + acc_len + 1) as i64; // ZV (1-based)
                    Some((ch0, pos1, limit1))
                }
            } else {
                while idx > 0 && !is_word(idx - 1) {
                    idx -= 1;
                }
                if idx == 0 {
                    None
                } else {
                    // ch1 ends a word; GNU passes its 1-based position.
                    let ch1 = chars.char_at(idx - 1);
                    let pos1 = (acc_start + idx) as i64;
                    let limit1 = (acc_start + 1) as i64; // BEGV (1-based)
                    Some((ch1, pos1, limit1))
                }
            }
        };

        let Some((ch, pos1, limit1)) = probe else {
            completed = false;
            break;
        };

        // Look the character up in the boundary-function table.
        let func = super::chartable::char_table_ref_and_range(&wbtable, i64::from(ch as u32))
            .map(|(v, _, _)| v)
            .unwrap_or(Value::NIL);
        let mut handled = false;
        let callable = match func.as_symbol_id() {
            Some(id) => eval.obarray.fboundp_id(id),
            None => super::subr_info::subr_is_callable_function_value(&func),
        };
        if callable {
            if let Ok(result) =
                eval.funcall_general(func, vec![Value::fixnum(pos1), Value::fixnum(limit1)])
                && let ValueKind::Fixnum(new_pos1) = result.kind()
            {
                let valid = if forward {
                    new_pos1 > pos1 && new_pos1 <= limit1
                } else {
                    new_pos1 < pos1 && new_pos1 >= limit1
                };
                if valid {
                    let zero_based = (new_pos1 - 1).max(0) as usize;
                    let byte = {
                        let buf = eval.buffers.get(current_id).expect("buffer");
                        buffer_char_to_emacs_byte_pos(buf, CharPos0::new(zero_based))
                    };
                    let _ = eval.buffers.goto_buffer_emacs_byte_pos(current_id, byte);
                    handled = true;
                }
            }
        }

        if !handled {
            // Plain syntax scan of a single word from the current point.
            let (byte, ok) = {
                let buf = eval.buffers.get(current_id).expect("buffer");
                let table = SyntaxTable::for_buffer(buf);
                forward_word_with_options(
                    buf,
                    &table,
                    if forward { 1 } else { -1 },
                    honor_properties,
                )
            };
            let _ = eval.buffers.goto_buffer_emacs_byte_pos(current_id, byte);
            if !ok {
                completed = false;
                break;
            }
        }
    }

    let byte = eval
        .buffers
        .get(current_id)
        .map(|b| b.point_emacs_byte_pos())
        .unwrap_or(EmacsBytePos::new(0));
    (byte, completed)
}

/// Compute where `forward-word`/`backward-word` over `count` words would land,
/// honoring `find-word-boundary-function-table`, WITHOUT moving point. Used by
/// the word-casing commands that operate on the [point, destination] region.
pub(crate) fn forward_word_destination(
    eval: &mut super::eval::Context,
    count: i64,
    honor_properties: bool,
) -> EmacsBytePos {
    let wbtable = eval.visible_variable_value_or_nil("find-word-boundary-function-table");
    if word_boundary_table_active(&wbtable) {
        let current_id = eval.buffers.current_buffer_id();
        let saved =
            current_id.and_then(|id| eval.buffers.get(id).map(|b| b.point_emacs_byte_pos()));
        let (dest, _) = word_motion_with_table(eval, count, honor_properties, wbtable);
        // word_motion_with_table moves point as it scans; restore it.
        if let (Some(id), Some(saved)) = (current_id, saved) {
            let _ = eval.buffers.goto_buffer_emacs_byte_pos(id, saved);
        }
        dest
    } else {
        let buf = match eval.buffers.current_buffer() {
            Some(b) => b,
            None => return EmacsBytePos::new(0),
        };
        let table = SyntaxTable::for_buffer(buf);
        forward_word_with_options(buf, &table, count, honor_properties).0
    }
}

fn backward_word_with_options(
    buf: &Buffer,
    table: &SyntaxTable,
    count: i64,
    honor_properties: bool,
) -> (EmacsBytePos, bool) {
    if count < 0 {
        return forward_word_with_options(buf, table, -count, honor_properties);
    }

    let accessible_bytes = buf.accessible_emacs_byte_region();
    let accessible_chars = buf.accessible_char_region();
    let chars = BufferChars::new(buf, accessible_chars.start());
    let accessible_char_start = accessible_chars.start().get();
    let mut idx = buffer_byte_to_char_pos(buf, accessible_bytes.clamp(buf.point_emacs_byte_pos()))
        .saturating_sub(accessible_char_start);

    for _ in 0..count {
        // Skip non-word characters backward
        while idx > 0
            && !matches!(
                effective_syntax_entry_for_abs_char(
                    buf,
                    table,
                    chars.char_at(idx - 1),
                    accessible_char_start + idx - 1,
                    honor_properties,
                )
                .class,
                SyntaxClass::Word
            )
        {
            idx -= 1;
        }
        if idx == 0 {
            let abs_char = offset_char_pos(accessible_chars.start(), idx);
            return (buffer_char_to_emacs_byte_pos(buf, abs_char), false);
        }
        // Skip word characters backward
        while idx > 0
            && matches!(
                effective_syntax_entry_for_abs_char(
                    buf,
                    table,
                    chars.char_at(idx - 1),
                    accessible_char_start + idx - 1,
                    honor_properties,
                )
                .class,
                SyntaxClass::Word
            )
        {
            idx -= 1;
        }
    }

    let abs_char = offset_char_pos(accessible_chars.start(), idx);
    (buffer_char_to_emacs_byte_pos(buf, abs_char), true)
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
    skip_syntax_forward_with_options(buf, table, syntax_chars, limit, false)
}

fn skip_syntax_forward_with_options(
    buf: &Buffer,
    table: &SyntaxTable,
    syntax_chars: &str,
    limit: Option<usize>,
    honor_properties: bool,
) -> usize {
    let (classes, negate) = parse_skip_syntax_classes(syntax_chars);

    let accessible_bytes = buf.accessible_emacs_byte_region();
    let accessible_chars = buf.accessible_char_region();
    let chars = BufferChars::new(buf, accessible_chars.start());
    let accessible_char_start = accessible_chars.start().get();
    let accessible_len = accessible_chars.len().get();
    let mut idx = buffer_byte_to_char_pos(buf, accessible_bytes.clamp(buf.point_emacs_byte_pos()))
        .saturating_sub(accessible_char_start);

    let char_limit = limit
        .map(|lim| {
            let lim_clamped = accessible_bytes.clamp(EmacsBytePos::new(lim));
            buffer_byte_to_char_pos(buf, lim_clamped) - accessible_char_start
        })
        .unwrap_or(accessible_len);

    while idx < char_limit {
        let syn = effective_syntax_entry_for_abs_char(
            buf,
            table,
            chars.char_at(idx),
            accessible_char_start + idx,
            honor_properties,
        )
        .class;
        if classes.contains(&syn) == negate {
            break;
        }
        idx += 1;
    }

    let abs_char = offset_char_pos(accessible_chars.start(), idx);
    buffer_char_to_emacs_byte_pos(buf, abs_char).get()
}

/// Skip backward over characters whose syntax class matches any character in
/// `syntax_chars`.  Returns the resulting byte position.
pub fn skip_syntax_backward(
    buf: &Buffer,
    table: &SyntaxTable,
    syntax_chars: &str,
    limit: Option<usize>,
) -> usize {
    skip_syntax_backward_with_options(buf, table, syntax_chars, limit, false)
}

fn skip_syntax_backward_with_options(
    buf: &Buffer,
    table: &SyntaxTable,
    syntax_chars: &str,
    limit: Option<usize>,
    honor_properties: bool,
) -> usize {
    let (classes, negate) = parse_skip_syntax_classes(syntax_chars);

    let accessible_bytes = buf.accessible_emacs_byte_region();
    let accessible_chars = buf.accessible_char_region();
    let chars = BufferChars::new(buf, accessible_chars.start());
    let accessible_char_start = accessible_chars.start().get();
    let mut idx = buffer_byte_to_char_pos(buf, accessible_bytes.clamp(buf.point_emacs_byte_pos()))
        .saturating_sub(accessible_char_start);

    let char_limit = limit
        .map(|lim| {
            let lim_clamped = accessible_bytes.clamp(EmacsBytePos::new(lim));
            buffer_byte_to_char_pos(buf, lim_clamped) - accessible_char_start
        })
        .unwrap_or(0);

    while idx > char_limit {
        let syn = effective_syntax_entry_for_abs_char(
            buf,
            table,
            chars.char_at(idx - 1),
            accessible_char_start + idx - 1,
            honor_properties,
        )
        .class;
        if classes.contains(&syn) == negate {
            break;
        }
        idx -= 1;
    }

    let abs_char = offset_char_pos(accessible_chars.start(), idx);
    buffer_char_to_emacs_byte_pos(buf, abs_char).get()
}

fn parse_skip_syntax_classes(syntax_chars: &str) -> (Vec<SyntaxClass>, bool) {
    let mut chars = syntax_chars.chars();
    let negate = matches!(chars.clone().next(), Some('^'));
    if negate {
        chars.next();
    }
    (chars.filter_map(SyntaxClass::from_char).collect(), negate)
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
    match scan_sexps_with_options(buf, table, from, count, false, false)
        .map_err(|err| err.message)?
    {
        Some(pos) => Ok(pos),
        None if count < 0 => Ok(buf.accessible_emacs_byte_region().start().get()),
        None => Ok(buf.accessible_emacs_byte_region().end().get()),
    }
}

fn scan_sexps_with_options(
    buf: &Buffer,
    table: &SyntaxTable,
    from: usize,
    count: i64,
    honor_properties: bool,
    ignore_comments: bool,
) -> Result<Option<usize>, ScanListError> {
    if count == 0 {
        return Ok(Some(from));
    }

    let chars = BufferChars::new(buf, CharPos0::ZERO);
    let accessible_chars = buf.accessible_char_region();
    let start_bound = accessible_chars.start().get();
    let stop_bound = accessible_chars.end().get();

    // Convert byte position to char index.
    let mut idx =
        buffer_byte_to_char_pos(buf, EmacsBytePos::new(from)).clamp(start_bound, stop_bound);

    if count > 0 {
        for _ in 0..count {
            let skipped = skip_sexp_ignored_forward(
                buf,
                &chars,
                idx,
                stop_bound,
                table,
                honor_properties,
                ignore_comments,
            );
            idx = skipped.position();
            if matches!(skipped, IgnoredSkip::UnterminatedComment(_)) {
                continue;
            }
            if idx >= stop_bound {
                return Ok(None);
            }
            idx = scan_sexp_forward(
                buf,
                &chars,
                stop_bound,
                idx,
                table,
                honor_properties,
                ignore_comments,
            )?;
        }
    } else {
        for _ in 0..(-count) {
            idx = skip_sexp_ignored_backward(
                buf,
                &chars,
                idx,
                start_bound,
                table,
                honor_properties,
                ignore_comments,
            );
            if idx <= start_bound {
                return Ok(None);
            }
            idx = scan_sexp_backward(
                buf,
                &chars,
                idx,
                start_bound,
                table,
                honor_properties,
                ignore_comments,
            )?;
        }
    }

    Ok(Some(
        buffer_char_to_emacs_byte_pos(buf, CharPos0::ZERO.add_len(CharLen::new(idx))).get(),
    ))
}

fn is_sexp_ignored_syntax(class: SyntaxClass, ignore_comments: bool) -> bool {
    matches!(
        class,
        SyntaxClass::Whitespace
            | SyntaxClass::EndComment
            | SyntaxClass::Punctuation
            | SyntaxClass::Quote
    ) || (!ignore_comments && class == SyntaxClass::Comment)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommentSkip {
    Complete(usize),
    Unterminated(usize),
}

impl CommentSkip {
    fn next(self) -> usize {
        match self {
            CommentSkip::Complete(next) | CommentSkip::Unterminated(next) => next,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IgnoredSkip {
    At(usize),
    UnterminatedComment(usize),
}

impl IgnoredSkip {
    fn position(self) -> usize {
        match self {
            IgnoredSkip::At(pos) | IgnoredSkip::UnterminatedComment(pos) => pos,
        }
    }
}

fn maybe_skip_comment_forward(
    buf: &Buffer,
    idx: usize,
    honor_properties: bool,
    class: SyntaxClass,
    flags: SyntaxFlags,
) -> Option<CommentSkip> {
    if !(class == SyntaxClass::Comment
        || class == SyntaxClass::CommentFence
        || flags.contains(SyntaxFlags::COMMENT_START_FIRST))
    {
        return None;
    }

    let start_byte = buffer_char_to_emacs_byte_pos(buf, CharPos0::new(idx));
    let mut scanner = ForwardCommentCursor {
        buffer: buf,
        point: start_byte,
    };
    let complete = forward_comment_forward(&mut scanner, 1, honor_properties);
    let next = buffer_byte_to_char_pos(buf, scanner.point_emacs_byte_pos());
    if next <= idx {
        None
    } else if complete {
        Some(CommentSkip::Complete(next))
    } else {
        Some(CommentSkip::Unterminated(next))
    }
}

fn maybe_skip_comment_backward(
    buf: &Buffer,
    idx: usize,
    honor_properties: bool,
    class: SyntaxClass,
    flags: SyntaxFlags,
) -> Option<usize> {
    if !(class == SyntaxClass::EndComment
        || class == SyntaxClass::CommentFence
        || flags.contains(SyntaxFlags::COMMENT_END_SECOND))
    {
        return None;
    }

    let start_byte = buffer_char_to_emacs_byte_pos(buf, CharPos0::new(idx));
    let mut scanner = ForwardCommentCursor {
        buffer: buf,
        point: start_byte,
    };
    if forward_comment_backward(&mut scanner, 1, honor_properties) {
        let next = buffer_byte_to_char_pos(buf, scanner.point_emacs_byte_pos());
        (next < idx).then_some(next)
    } else {
        None
    }
}

fn skip_sexp_ignored_forward(
    buf: &Buffer,
    chars: &BufferChars,
    mut idx: usize,
    stop: usize,
    table: &SyntaxTable,
    honor_properties: bool,
    ignore_comments: bool,
) -> IgnoredSkip {
    let mut skipped_unterminated_comment = false;
    while idx < stop {
        let c = chars.char_at(idx);
        let entry = effective_syntax_entry_for_abs_char(buf, table, c, idx, honor_properties);
        let class = entry.class;
        if ignore_comments
            && let Some(skip) =
                maybe_skip_comment_forward(buf, idx, honor_properties, class, entry.flags)
        {
            skipped_unterminated_comment |= matches!(skip, CommentSkip::Unterminated(_));
            idx = skip.next();
            continue;
        }
        if is_sexp_ignored_syntax(class, ignore_comments) {
            idx += 1;
            continue;
        }
        break;
    }
    if skipped_unterminated_comment {
        IgnoredSkip::UnterminatedComment(idx)
    } else {
        IgnoredSkip::At(idx)
    }
}

fn skip_sexp_ignored_backward(
    buf: &Buffer,
    chars: &BufferChars,
    mut idx: usize,
    start: usize,
    table: &SyntaxTable,
    honor_properties: bool,
    ignore_comments: bool,
) -> usize {
    while idx > start {
        let prev = idx - 1;
        let c = chars.char_at(prev);
        let entry = effective_syntax_entry_for_abs_char(buf, table, c, prev, honor_properties);
        let class = entry.class;
        if ignore_comments
            && let Some(next) =
                maybe_skip_comment_backward(buf, idx, honor_properties, class, entry.flags)
        {
            idx = next;
            continue;
        }
        if is_sexp_ignored_syntax(class, ignore_comments) {
            idx -= 1;
            continue;
        }
        break;
    }
    idx
}

fn skip_string_forward(
    buf: &Buffer,
    chars: &BufferChars,
    mut idx: usize,
    stop: usize,
    table: &SyntaxTable,
    honor_properties: bool,
    delimiter: char,
    delimiter_class: SyntaxClass,
) -> Result<usize, String> {
    while idx < stop {
        let c = chars.char_at(idx);
        let class = effective_syntax_entry_for_abs_char(buf, table, c, idx, honor_properties).class;
        if class == delimiter_class
            && (delimiter_class == SyntaxClass::StringFence || c == delimiter)
        {
            return Ok(idx + 1);
        }
        if matches!(class, SyntaxClass::Escape | SyntaxClass::CharQuote) {
            idx += 1;
        }
        idx += 1;
    }

    Err("Scan error: unbalanced parentheses".to_string())
}

fn skip_string_backward(
    buf: &Buffer,
    chars: &BufferChars,
    mut idx: usize,
    stop: usize,
    table: &SyntaxTable,
    honor_properties: bool,
    delimiter: char,
    delimiter_class: SyntaxClass,
) -> Result<usize, String> {
    while idx > stop {
        idx -= 1;
        let c = chars.char_at(idx);
        let class = effective_syntax_entry_for_abs_char(buf, table, c, idx, honor_properties).class;
        if class == delimiter_class
            && (delimiter_class == SyntaxClass::StringFence || c == delimiter)
        {
            return Ok(idx);
        }
    }

    Err("Scan error: unbalanced parentheses".to_string())
}

fn scan_lists_with_options(
    buf: &Buffer,
    table: &SyntaxTable,
    from: usize,
    count: i64,
    initial_depth: i64,
    honor_properties: bool,
    ignore_comments: bool,
) -> Result<Option<usize>, ScanListError> {
    let chars = BufferChars::new(buf, CharPos0::ZERO);
    let mut idx = from;
    let accessible_chars = buf.accessible_char_region();
    let start = accessible_chars.start().get();
    let stop = accessible_chars.end().get();
    let mut depth = initial_depth;
    let min_depth = if depth > 0 { 0 } else { depth };
    let mut last_good = from;

    if count > 0 {
        let mut remaining = count;
        while remaining > 0 {
            let mut found = false;
            while idx < stop {
                let ch = chars.char_at(idx);
                let entry =
                    effective_syntax_entry_for_abs_char(buf, table, ch, idx, honor_properties);
                let class = entry.class;
                if depth == min_depth {
                    last_good = idx;
                }
                if ignore_comments
                    && let Some(skip) =
                        maybe_skip_comment_forward(buf, idx, honor_properties, class, entry.flags)
                {
                    idx = skip.next();
                    if matches!(skip, CommentSkip::Unterminated(_)) && depth == 0 {
                        found = true;
                        break;
                    }
                    continue;
                }
                idx += 1;

                match class {
                    SyntaxClass::Open => {
                        depth += 1;
                        if depth == 0 {
                            found = true;
                            break;
                        }
                    }
                    SyntaxClass::Close => {
                        depth -= 1;
                        if depth == 0 {
                            found = true;
                            break;
                        }
                        if depth < min_depth {
                            return Err(ScanListError::containing_ends_prematurely(last_good, idx));
                        }
                    }
                    SyntaxClass::StringDelim | SyntaxClass::StringFence => {
                        idx = skip_string_forward(
                            buf,
                            &chars,
                            idx,
                            stop,
                            table,
                            honor_properties,
                            ch,
                            class,
                        )
                        .map_err(|_| ScanListError::unbalanced(last_good, stop))?;
                    }
                    SyntaxClass::Escape | SyntaxClass::CharQuote => {
                        if idx >= stop {
                            return Err(ScanListError::unbalanced(last_good, stop));
                        }
                        idx += 1;
                    }
                    _ => {}
                }
            }

            if depth != 0 {
                return Err(ScanListError::unbalanced(last_good, idx));
            }
            if !found {
                return Ok(None);
            }
            remaining -= 1;
        }
        Ok(Some(idx))
    } else if count < 0 {
        let mut remaining = count;
        while remaining < 0 {
            let mut found = false;
            while idx > start {
                idx -= 1;
                let ch = chars.char_at(idx);
                let entry =
                    effective_syntax_entry_for_abs_char(buf, table, ch, idx, honor_properties);
                let class = entry.class;
                if depth == min_depth {
                    last_good = idx;
                }
                if ignore_comments
                    && let Some(next) = maybe_skip_comment_backward(
                        buf,
                        idx + 1,
                        honor_properties,
                        class,
                        entry.flags,
                    )
                {
                    idx = next;
                    continue;
                }

                match class {
                    SyntaxClass::Close => {
                        depth += 1;
                        if depth == 0 {
                            found = true;
                            break;
                        }
                    }
                    SyntaxClass::Open => {
                        depth -= 1;
                        if depth == 0 {
                            found = true;
                            break;
                        }
                        if depth < min_depth {
                            return Err(ScanListError::containing_ends_prematurely(last_good, idx));
                        }
                    }
                    SyntaxClass::StringDelim | SyntaxClass::StringFence => {
                        idx = skip_string_backward(
                            buf,
                            &chars,
                            idx,
                            start,
                            table,
                            honor_properties,
                            ch,
                            class,
                        )
                        .map_err(|_| ScanListError::unbalanced(last_good, start))?;
                    }
                    _ => {}
                }
            }

            if depth != 0 {
                return Err(ScanListError::unbalanced(last_good, idx));
            }
            if !found {
                return Ok(None);
            }
            remaining += 1;
        }
        Ok(Some(idx))
    } else {
        Ok(Some(idx))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScanListError {
    message: String,
    last_good: usize,
    at: usize,
}

impl ScanListError {
    fn new(message: impl Into<String>, last_good: usize, at: usize) -> Self {
        Self {
            message: message.into(),
            last_good,
            at,
        }
    }

    fn unbalanced(last_good: usize, at: usize) -> Self {
        Self::new("Unbalanced parentheses", last_good, at)
    }

    fn containing_ends_prematurely(last_good: usize, at: usize) -> Self {
        Self::new("Containing expression ends prematurely", last_good, at)
    }

    fn signal_data(&self) -> Vec<Value> {
        vec![
            Value::string(&self.message),
            Value::fixnum(char_pos_to_lisp_i64(self.last_good)),
            Value::fixnum(char_pos_to_lisp_i64(self.at)),
        ]
    }
}

/// Scan one sexp forward from char index `start`.
fn scan_sexp_forward(
    buf: &Buffer,
    chars: &BufferChars,
    len: usize,
    start: usize,
    table: &SyntaxTable,
    honor_properties: bool,
    ignore_comments: bool,
) -> Result<usize, ScanListError> {
    let skipped = skip_sexp_ignored_forward(
        buf,
        chars,
        start,
        len,
        table,
        honor_properties,
        ignore_comments,
    );
    let mut idx = skipped.position();

    if matches!(skipped, IgnoredSkip::UnterminatedComment(_)) {
        return Ok(idx);
    }

    if idx >= len {
        return Err(ScanListError::unbalanced(start, idx));
    }

    let ch = chars.char_at(idx);
    let syn_entry = effective_syntax_entry_for_abs_char(buf, table, ch, idx, honor_properties);
    let syn = syn_entry.class;

    match syn {
        SyntaxClass::Open => {
            // Find matching close, respecting nesting.
            let mut depth = 1i32;
            idx += 1;
            while idx < len && depth > 0 {
                let c = chars.char_at(idx);
                let entry =
                    effective_syntax_entry_for_abs_char(buf, table, c, idx, honor_properties);
                let s = entry.class;
                if ignore_comments
                    && let Some(skip) =
                        maybe_skip_comment_forward(buf, idx, honor_properties, s, entry.flags)
                {
                    idx = skip.next();
                    continue;
                }
                match s {
                    SyntaxClass::Open => {
                        depth += 1;
                    }
                    SyntaxClass::Close => {
                        depth -= 1;
                    }
                    SyntaxClass::StringDelim | SyntaxClass::StringFence => {
                        // Skip over string contents
                        let delim_class = s;
                        idx += 1;
                        while idx < len {
                            let sc = effective_syntax_entry_for_abs_char(
                                buf,
                                table,
                                chars.char_at(idx),
                                idx,
                                honor_properties,
                            )
                            .class;
                            if sc == delim_class
                                && (s == SyntaxClass::StringFence || chars.char_at(idx) == c)
                            {
                                break;
                            }
                            if matches!(sc, SyntaxClass::Escape) {
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
                return Err(ScanListError::unbalanced(start, idx));
            }
            Ok(idx)
        }
        SyntaxClass::Close => Err(ScanListError::containing_ends_prematurely(start, idx + 1)),
        SyntaxClass::StringDelim | SyntaxClass::StringFence => {
            // Scan to matching string delimiter.
            // StringFence always pairs with itself (like `"` but independent).
            let delim_class = syn;
            idx += 1;
            while idx < len {
                let c = chars.char_at(idx);
                let s =
                    effective_syntax_entry_for_abs_char(buf, table, c, idx, honor_properties).class;
                if s == delim_class && (syn == SyntaxClass::StringFence || c == ch) {
                    break;
                }
                if matches!(s, SyntaxClass::Escape) {
                    idx += 1; // skip escaped char
                }
                idx += 1;
            }
            if idx >= len {
                return Err(ScanListError::unbalanced(start, idx));
            }
            Ok(idx + 1) // past closing delim
        }
        SyntaxClass::Word | SyntaxClass::Symbol => {
            // Scan over a symbol/word sexp.
            while idx < len
                && matches!(
                    effective_syntax_entry_for_abs_char(
                        buf,
                        table,
                        chars.char_at(idx),
                        idx,
                        honor_properties,
                    )
                    .class,
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
        SyntaxClass::Math => {
            // Scan to matching math delimiter.
            let delim = ch;
            idx += 1;
            while idx < len && chars.char_at(idx) != delim {
                idx += 1;
            }
            if idx >= len {
                return Err(ScanListError::unbalanced(start, idx));
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
    buf: &Buffer,
    chars: &BufferChars,
    start: usize,
    start_bound: usize,
    table: &SyntaxTable,
    honor_properties: bool,
    ignore_comments: bool,
) -> Result<usize, ScanListError> {
    let mut idx = skip_sexp_ignored_backward(
        buf,
        chars,
        start,
        start_bound,
        table,
        honor_properties,
        ignore_comments,
    );

    if idx == start_bound {
        return Err(ScanListError::unbalanced(idx, start));
    }

    idx -= 1; // move to the character we're examining
    let ch = chars.char_at(idx);
    let syn_entry = effective_syntax_entry_for_abs_char(buf, table, ch, idx, honor_properties);
    let syn = syn_entry.class;

    match syn {
        SyntaxClass::Close => {
            // Find matching open, respecting nesting.
            let mut depth = 1i32;
            while idx > start_bound && depth > 0 {
                idx -= 1;
                let c = chars.char_at(idx);
                let entry =
                    effective_syntax_entry_for_abs_char(buf, table, c, idx, honor_properties);
                let s = entry.class;
                if ignore_comments
                    && let Some(next) =
                        maybe_skip_comment_backward(buf, idx + 1, honor_properties, s, entry.flags)
                {
                    idx = next;
                    continue;
                }
                match s {
                    SyntaxClass::Close => {
                        depth += 1;
                    }
                    SyntaxClass::Open => {
                        depth -= 1;
                    }
                    SyntaxClass::StringDelim | SyntaxClass::StringFence => {
                        // Skip over string contents backward
                        let delim_class = s;
                        if idx > start_bound {
                            idx -= 1;
                            while idx > start_bound {
                                let sc = effective_syntax_entry_for_abs_char(
                                    buf,
                                    table,
                                    chars.char_at(idx),
                                    idx,
                                    honor_properties,
                                )
                                .class;
                                if sc == delim_class
                                    && (s == SyntaxClass::StringFence || chars.char_at(idx) == c)
                                {
                                    break;
                                }
                                idx -= 1;
                            }
                            // idx now points at the opening delim
                        }
                    }
                    _ => {}
                }
            }
            if depth != 0 {
                return Err(ScanListError::unbalanced(idx, start));
            }
            Ok(idx)
        }
        SyntaxClass::Open => Err(ScanListError::containing_ends_prematurely(idx, start)),
        SyntaxClass::StringDelim | SyntaxClass::StringFence => {
            // Scan backward to matching string delimiter.
            let delim_class = syn;
            if idx == start_bound {
                return Err(ScanListError::unbalanced(idx, start));
            }
            idx -= 1;
            while idx > start_bound {
                let c = chars.char_at(idx);
                let s =
                    effective_syntax_entry_for_abs_char(buf, table, c, idx, honor_properties).class;
                if s == delim_class && (syn == SyntaxClass::StringFence || c == ch) {
                    break;
                }
                idx -= 1;
            }
            let c = chars.char_at(idx);
            let s = effective_syntax_entry_for_abs_char(buf, table, c, idx, honor_properties).class;
            if !(s == delim_class && (syn == SyntaxClass::StringFence || c == ch)) {
                return Err(ScanListError::unbalanced(idx, start));
            }
            Ok(idx)
        }
        SyntaxClass::Word | SyntaxClass::Symbol => {
            // Scan backward over word/symbol chars.
            while idx > start_bound
                && matches!(
                    effective_syntax_entry_for_abs_char(
                        buf,
                        table,
                        chars.char_at(idx - 1),
                        idx - 1,
                        honor_properties,
                    )
                    .class,
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
        SyntaxClass::Math => {
            let delim = ch;
            if idx == start_bound {
                return Err(ScanListError::unbalanced(idx, start));
            }
            idx -= 1;
            while idx > start_bound && chars.char_at(idx) != delim {
                idx -= 1;
            }
            if chars.char_at(idx) != delim {
                return Err(ScanListError::unbalanced(idx, start));
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
            vec![
                Value::symbol("string-to-syntax"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let s = syntax_runtime_string(&args[0])?;
    let entry = string_to_syntax(&s).map_err(|msg| signal("error", vec![Value::string(&msg)]))?;
    if matches!(entry.class, SyntaxClass::InheritStd) {
        return Ok(Value::NIL);
    }
    Ok(syntax_entry_to_value(&entry))
}

/// `(make-syntax-table &optional PARENT)` — create a new syntax table.
pub(crate) fn builtin_make_syntax_table(args: Vec<Value>) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("make-syntax-table"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let table = super::chartable::make_char_table_value(Value::symbol("syntax-table"), Value::NIL);
    let parent = if args.is_empty() || args[0].is_nil() {
        ensure_standard_syntax_table_object()?
    } else {
        args[0]
    };
    if !parent.is_nil() {
        super::chartable::builtin_set_char_table_parent(vec![table, parent])?;
    }
    Ok(table)
}

/// `(copy-syntax-table &optional TABLE)` — return a fresh copy of TABLE.
pub(crate) fn builtin_copy_syntax_table(args: Vec<Value>) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("copy-syntax-table"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let source = if args.is_empty() || args[0].is_nil() {
        builtin_standard_syntax_table(vec![])?
    } else {
        let table = args[0];
        if builtin_syntax_table_p(vec![table])?.is_nil() {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("syntax-table-p"), table],
            ));
        }
        table
    };

    let copy = super::builtins::builtin_copy_sequence(vec![source])?;
    super::chartable::builtin_set_char_table_range(vec![copy, Value::NIL, Value::NIL], None)?;
    if super::chartable::builtin_char_table_parent(vec![copy])?.is_nil() {
        super::chartable::builtin_set_char_table_parent(vec![
            copy,
            ensure_standard_syntax_table_object()?,
        ])?;
    }
    Ok(copy)
}

fn ensure_standard_syntax_table_object() -> EvalResult {
    STANDARD_SYNTAX_TABLE_OBJECT.with(|slot| {
        if let Some(table) = slot.borrow().as_ref() {
            return Ok(*table);
        }
        let whitespace = syntax_entry_to_value(&SyntaxEntry::simple(SyntaxClass::Whitespace));
        let punctuation = syntax_entry_to_value(&SyntaxEntry::simple(SyntaxClass::Punctuation));
        let word = syntax_entry_to_value(&SyntaxEntry::simple(SyntaxClass::Word));
        let table =
            super::chartable::make_char_table_value(Value::symbol("syntax-table"), whitespace);

        for cp in 0..=(' ' as i64 - 1) {
            super::chartable::builtin_set_char_table_range(
                vec![table, Value::fixnum(cp), punctuation],
                None,
            )?;
        }
        super::chartable::builtin_set_char_table_range(
            vec![table, Value::fixnum(0x7f), punctuation],
            None,
        )?;

        // Standard ASCII defaults — matches GNU `Fset_standard_syntax_table`
        // in `syntax.c:3476-3557`. Word: letters, digits, $ %;
        // Open/Close: paren/bracket/brace pairs with matching chars;
        // StringDelim: "; Escape: \; Symbol: _ - + * / & | < > =;
        // Punctuation: . , ; : ? ! # @ ~ ^ ' `.
        let set = |ch: char, e: SyntaxEntry| -> Result<(), Flow> {
            super::chartable::builtin_set_char_table_range(
                vec![table, Value::fixnum(ch as i64), syntax_entry_to_value(&e)],
                None,
            )
            .map(|_| ())
        };
        for ch in [' ', '\t', '\n', '\r', '\u{000c}'] {
            set(ch, SyntaxEntry::simple(SyntaxClass::Whitespace))?;
        }
        for ch in 'a'..='z' {
            set(ch, SyntaxEntry::simple(SyntaxClass::Word))?;
        }
        for ch in 'A'..='Z' {
            set(ch, SyntaxEntry::simple(SyntaxClass::Word))?;
        }
        for ch in '0'..='9' {
            set(ch, SyntaxEntry::simple(SyntaxClass::Word))?;
        }
        set('$', SyntaxEntry::simple(SyntaxClass::Word))?;
        set('%', SyntaxEntry::simple(SyntaxClass::Word))?;
        set('(', SyntaxEntry::with_match(SyntaxClass::Open, ')'))?;
        set(')', SyntaxEntry::with_match(SyntaxClass::Close, '('))?;
        set('[', SyntaxEntry::with_match(SyntaxClass::Open, ']'))?;
        set(']', SyntaxEntry::with_match(SyntaxClass::Close, '['))?;
        set('{', SyntaxEntry::with_match(SyntaxClass::Open, '}'))?;
        set('}', SyntaxEntry::with_match(SyntaxClass::Close, '{'))?;
        set('"', SyntaxEntry::simple(SyntaxClass::StringDelim))?;
        set('\\', SyntaxEntry::simple(SyntaxClass::Escape))?;
        for ch in ['_', '-', '+', '*', '/', '&', '|', '<', '>', '='] {
            set(ch, SyntaxEntry::simple(SyntaxClass::Symbol))?;
        }
        for ch in ['.', ',', ';', ':', '?', '!', '#', '@', '~', '^', '\'', '`'] {
            set(ch, SyntaxEntry::simple(SyntaxClass::Punctuation))?;
        }
        super::chartable::builtin_set_char_table_range(
            vec![
                table,
                Value::cons(Value::fixnum(0x80), Value::fixnum(0x3F_FFFF)),
                word,
            ],
            None,
        )?;
        *slot.borrow_mut() = Some(table);
        Ok(table)
    })
}

fn current_buffer_syntax_table_object_in_buffers(
    buffers: &mut BufferManager,
) -> Result<Value, Flow> {
    use crate::buffer::buffer::BUFFER_SLOT_SYNTAX_TABLE;
    let fallback = ensure_standard_syntax_table_object()?;
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = buffers
        .get_mut(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    // Mirrors GNU `Fsyntax_table` (`syntax.c:987-993`):
    //     return BVAR (current_buffer, syntax_table);
    let value = buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()];
    if !value.is_nil() && builtin_syntax_table_p(vec![value])?.is_truthy() {
        return Ok(value);
    }

    // Slot is unset (fresh buffer or never assigned). Seed it
    // from the standard syntax table — matches GNU's
    // `reset_buffer` (`buffer.c:1149-1157`) which copies the
    // standard tables into a fresh buffer.
    buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()] = fallback;
    Ok(fallback)
}

fn current_buffer_syntax_table_object(eval: &mut super::eval::Context) -> Result<Value, Flow> {
    current_buffer_syntax_table_object_in_buffers(&mut eval.buffers)
}

pub(crate) fn sync_current_buffer_syntax_table_state(
    ctx: &mut super::eval::Context,
) -> Result<(), Flow> {
    // Just ensure the slot is seeded with the standard chartable if
    // it was left `Value::NIL`. No compilation, no cache rebuild —
    // motion/parse code reads `buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()]`
    // directly via `SyntaxTable::for_buffer`. Matches GNU
    // `set_buffer_internal`.
    let _ = current_buffer_syntax_table_object_in_buffers(&mut ctx.buffers)?;
    Ok(())
}

fn set_current_buffer_syntax_table_object_in_buffers(
    buffers: &mut BufferManager,
    table: Value,
) -> Result<(), Flow> {
    use crate::buffer::buffer::BUFFER_SLOT_SYNTAX_TABLE;
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = buffers
        .get_mut(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    // Mirrors GNU `Fset_syntax_table` (`syntax.c:1030-1042`):
    //     bset_syntax_table (current_buffer, table);
    //     SET_PER_BUFFER_VALUE_P (current_buffer,
    //                             PER_BUFFER_VAR_IDX (syntax_table), 1);
    buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()] = table;
    buf.set_slot_local_flag(BUFFER_SLOT_SYNTAX_TABLE, true);
    Ok(())
}

fn set_current_buffer_syntax_table_object(
    eval: &mut super::eval::Context,
    table: Value,
) -> Result<(), Flow> {
    set_current_buffer_syntax_table_object_in_buffers(&mut eval.buffers, table)
}

/// Read the `SyntaxEntry` for character `c` from the chartable `table`.
///
/// Mirrors GNU Emacs `SYNTAX_ENTRY(c)` in `src/syntax.h`:
///
/// ```c
/// #define SYNTAX_ENTRY(c) \
///   char_table_ref (BVAR (current_buffer, syntax_table), c)
/// ```
///
/// When `table` is `Value::NIL` (an un-seeded buffer slot or a
/// placeholder wrapper), falls back to the evaluator's
/// standard-syntax-table chartable. This mirrors GNU `reset_buffer`,
/// which copies `Vstandard_syntax_table` into every fresh
/// `buffer->syntax_table` — so from the reader's point of view a
/// "never-set" slot always behaves like the standard.
pub(crate) fn syntax_entry_at_char(table: &Value, c: char) -> Option<SyntaxEntry> {
    syntax_entry_at_char_code(table, c as u32)
}

pub(crate) fn syntax_entry_at_char_code(table: &Value, code: u32) -> Option<SyntaxEntry> {
    let effective = if table.is_nil() {
        ensure_standard_syntax_table_object().unwrap_or(Value::NIL)
    } else {
        *table
    };
    if effective.is_nil() {
        return None;
    }
    let entry = super::chartable::ct_lookup(&effective, code as i64).ok()?;
    syntax_entry_from_chartable_entry(&entry)
}

/// Return the `SyntaxClass` for `c` under `table`, mirroring GNU
/// `SYNTAX(c)` in `src/syntax.h`. Uses the same fallback as
/// `SyntaxTable::char_syntax` on the old compiled form: codepoints
/// >= 0x80 default to Word; below 0x80 default to Whitespace.
pub(crate) fn syntax_class_at_char(table: &Value, c: char) -> SyntaxClass {
    syntax_class_at_char_code(table, c as u32)
}

pub(crate) fn syntax_class_at_char_code(table: &Value, code: u32) -> SyntaxClass {
    match syntax_entry_at_char_code(table, code) {
        Some(entry) => entry.class,
        None => {
            if code >= 0x80 {
                SyntaxClass::Word
            } else {
                SyntaxClass::Whitespace
            }
        }
    }
}

fn syntax_entry_from_chartable_entry(entry: &Value) -> Option<SyntaxEntry> {
    match entry.kind() {
        ValueKind::Nil => None,
        ValueKind::Cons => {
            let pair_car = entry.cons_car();
            let pair_cdr = entry.cons_cdr();
            let code = match pair_car.kind() {
                ValueKind::Fixnum(code) => code,
                _ => return None,
            };
            let class = SyntaxClass::from_code(code)?;
            let matching_char = match pair_cdr.kind() {
                ValueKind::Fixnum(n) => char::from_u32(n as u32),
                ValueKind::Nil => None,
                _ => None,
            };
            Some(SyntaxEntry {
                class,
                matching_char,
                flags: SyntaxFlags::new(((code >> 16) & 0xFF) as u8),
            })
        }
        ValueKind::Fixnum(code) => Some(SyntaxEntry {
            class: SyntaxClass::from_code(code)?,
            matching_char: None,
            flags: SyntaxFlags::new(((code >> 16) & 0xFF) as u8),
        }),
        _ => None,
    }
}

fn syntax_table_from_chartable(table: Value) -> Result<SyntaxTable, Flow> {
    if builtin_syntax_table_p(vec![table])?.is_nil() {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("syntax-table-p"), table],
        ));
    }
    // GNU parity: the chartable IS the runtime form. Just wrap it.
    Ok(SyntaxTable::from_chartable(table))
}

fn syntax_entry_from_syntax_property(prop: Value, ch: char) -> Option<SyntaxEntry> {
    if builtin_syntax_table_p(vec![prop]).ok()?.is_truthy() {
        let raw =
            super::chartable::builtin_char_table_range(vec![prop, Value::fixnum(ch as i64)], None)
                .ok()?;
        syntax_entry_from_chartable_entry(&raw)
    } else {
        syntax_entry_from_chartable_entry(&prop)
    }
}

/// Per-scan cache of the `syntax-table` text-property run, mirroring GNU
/// `syntax.c` `gl_state` (`b_property`/`e_property` plus the current value).
/// A scan reads the property once per char, but it is almost always nil over
/// long runs, so caching the `[start, end)` char range turns a per-char
/// interval lookup (and its byte->char) into a plain range check, refetching
/// only when the scan leaves the run.  Indexed by char position so a
/// char-indexed scan needs no conversion at all on a hit.  Created fresh per
/// scan, so it never observes a mid-scan property edit (syntax-propertize runs
/// before the scan).
struct SyntaxPropRange {
    cache: RefCell<Option<(usize, usize, Option<Value>)>>,
}

impl SyntaxPropRange {
    fn new() -> Self {
        Self {
            cache: RefCell::new(None),
        }
    }

    /// The `syntax-table` property at char position `pos`, served from the
    /// cached run when possible.  In debug builds every cache hit is validated
    /// against a fresh interval lookup, the same safety net the byte<->char
    /// cache uses.
    fn syntax_table_prop_at_char(&self, buf: &Buffer, pos: usize) -> Option<Value> {
        let char_pos = offset_char_pos(CharPos0::ZERO, pos);
        {
            let cache = self.cache.borrow();
            if let Some((start, end, ref value)) = *cache
                && start <= pos
                && pos < end
            {
                #[cfg(debug_assertions)]
                {
                    let (fresh, _, _) =
                        buf.get_property_run_at_char_pos(char_pos, Value::symbol("syntax-table"));
                    debug_assert!(
                        *value == fresh,
                        "SyntaxPropRange stale syntax-table at char {pos} in [{start}, {end})"
                    );
                }
                return value.clone();
            }
        }
        let (value, start, end) =
            buf.get_property_run_at_char_pos(char_pos, Value::symbol("syntax-table"));
        self.cache
            .replace(Some((start.get(), end.get(), value.clone())));
        value
    }
}

#[inline]
fn syntax_entry_from_table(table: &SyntaxTable, ch: char) -> SyntaxEntry {
    table
        .get_entry(ch)
        .unwrap_or_else(|| SyntaxEntry::simple(table.char_syntax(ch)))
}

fn effective_syntax_entry_for_char_at_byte(
    buf: &Buffer,
    table: &SyntaxTable,
    ch: char,
    byte_pos: EmacsBytePos,
    honor_properties: bool,
) -> SyntaxEntry {
    if honor_properties
        && let Some(prop) =
            buf.text_props_get_property_at_emacs_byte_pos(byte_pos, Value::symbol("syntax-table"))
        && let Some(entry) = syntax_entry_from_syntax_property(prop, ch)
    {
        return entry;
    }

    syntax_entry_from_table(table, ch)
}

/// Like [`effective_syntax_entry_for_abs_char`] but reads the `syntax-table`
/// property through a per-scan run cache (GNU `gl_state`), avoiding an
/// interval lookup (and a char->byte->char round trip) on every char.
fn effective_syntax_entry_for_abs_char_cached(
    buf: &Buffer,
    table: &SyntaxTable,
    ch: char,
    abs_char: usize,
    honor_properties: bool,
    prop_cache: &SyntaxPropRange,
) -> SyntaxEntry {
    if honor_properties
        && let Some(prop) = prop_cache.syntax_table_prop_at_char(buf, abs_char)
        && let Some(entry) = syntax_entry_from_syntax_property(prop, ch)
    {
        return entry;
    }

    syntax_entry_from_table(table, ch)
}

fn effective_syntax_entry_for_abs_char(
    buf: &Buffer,
    table: &SyntaxTable,
    ch: char,
    abs_char: usize,
    honor_properties: bool,
) -> SyntaxEntry {
    let byte_pos = buffer_char_to_emacs_byte_pos(buf, offset_char_pos(CharPos0::ZERO, abs_char));
    effective_syntax_entry_for_char_at_byte(buf, table, ch, byte_pos, honor_properties)
}

pub(crate) fn parse_sexp_lookup_properties_enabled(ctx: &super::eval::Context) -> bool {
    ctx.eval_symbol("parse-sexp-lookup-properties")
        .unwrap_or(Value::NIL)
        .is_truthy()
}

fn parse_sexp_ignore_comments_enabled(ctx: &super::eval::Context) -> bool {
    ctx.eval_symbol("parse-sexp-ignore-comments")
        .unwrap_or(Value::NIL)
        .is_truthy()
}

fn maybe_syntax_propertize_for_scan(
    eval: &mut super::eval::Context,
    target_char_pos: usize,
) -> EvalResult {
    if !parse_sexp_lookup_properties_enabled(eval)
        || eval
            .obarray
            .symbol_function("internal--syntax-propertize")
            .is_none()
    {
        return Ok(Value::NIL);
    }

    let done = eval
        .eval_symbol("syntax-propertize--done")
        .unwrap_or(Value::fixnum(-1));
    if let ValueKind::Fixnum(done) = done.kind()
        && done >= target_char_pos as i64
    {
        return Ok(Value::NIL);
    }

    let before_modiff = eval
        .buffers
        .current_buffer()
        .map(|buf| buf.chars_modified_tick())
        .unwrap_or_default();
    eval.apply(
        Value::symbol("internal--syntax-propertize"),
        vec![Value::fixnum(target_char_pos as i64)],
    )?;
    let after_modiff = eval
        .buffers
        .current_buffer()
        .map(|buf| buf.chars_modified_tick())
        .unwrap_or_default();
    if after_modiff != before_modiff {
        return Err(signal(
            "error",
            vec![Value::string(
                "internal--syntax-propertize modified the buffer!",
            )],
        ));
    }
    Ok(Value::NIL)
}

/// `(syntax-class-to-char CLASS)` — map syntax class code to descriptor char.
pub(crate) fn builtin_syntax_class_to_char(args: Vec<Value>) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("syntax-class-to-char"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let class = match args[0].kind() {
        ValueKind::Fixnum(n) => n,
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("fixnump"), args[0]],
            ));
        }
    };

    let Some(class) = SyntaxClass::from_plain_code(class) else {
        return Err(signal(
            "args-out-of-range",
            vec![Value::fixnum(15), Value::fixnum(class)],
        ));
    };

    Ok(Value::char(class.to_char()))
}

/// `(matching-paren CHAR)` — return matching paren for bracket chars.
///
/// This is an evaluator-dependent version that uses the current buffer's
/// syntax table, matching GNU `Fmatching_paren`: return a match only when the
/// effective syntax class is open or close.
pub(crate) fn builtin_matching_paren(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_matching_paren_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_matching_paren_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("matching-paren"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let ch = match args[0].kind() {
        ValueKind::Fixnum(n) => char::from_u32(n as u32).ok_or_else(|| {
            signal(
                "wrong-type-argument",
                vec![Value::symbol("characterp"), args[0]],
            )
        })?,
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("characterp"), args[0]],
            ));
        }
    };

    if let Some(buf) = buffers.current_buffer() {
        let entry = SyntaxTable::for_buffer(buf).get_entry(ch);
        if let Some(e) = entry {
            if matches!(e.class, SyntaxClass::Open | SyntaxClass::Close) {
                if let Some(m) = e.matching_char {
                    return Ok(Value::char(m));
                }
            }
        }
    }
    Ok(Value::NIL)
}

/// `(standard-syntax-table)` — return the standard syntax table.
pub(crate) fn builtin_standard_syntax_table(args: Vec<Value>) -> EvalResult {
    if !args.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("standard-syntax-table"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    ensure_standard_syntax_table_object()
}

/// `(syntax-table-p OBJECT)` — return t if OBJECT is a syntax table.
pub(crate) fn builtin_syntax_table_p(args: Vec<Value>) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("syntax-table-p"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let is_char_table = super::chartable::builtin_char_table_p(vec![args[0]])?;
    if !is_char_table.is_truthy() {
        return Ok(Value::NIL);
    }

    let subtype = super::chartable::builtin_char_table_subtype(vec![args[0]])?;
    Ok(
        if SyntaxPurposeSymbol::from_lisp_value(&subtype) == Some(SyntaxPurposeSymbol::SyntaxTable)
        {
            Value::T
        } else {
            Value::NIL
        },
    )
}

/// `(syntax-table)` — return the current buffer syntax table.
///
/// Returns the buffer-local syntax-table object, defaulting to the standard
/// syntax-table object.
pub(crate) fn builtin_syntax_table(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_syntax_table_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_syntax_table_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if !args.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("syntax-table"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    current_buffer_syntax_table_object_in_buffers(buffers)
}

/// `(set-syntax-table TABLE)` — install TABLE for current buffer and return it.
///
/// NeoVM currently stores syntax behavior on `Buffer.syntax_table` internals;
/// this installs the exposed syntax-table object for compatibility and returns it.
pub(crate) fn builtin_set_syntax_table(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_syntax_table_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_set_syntax_table_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("set-syntax-table"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    if builtin_syntax_table_p(vec![args[0]])?.is_nil() {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("syntax-table-p"), args[0]],
        ));
    }
    let table = args[0];
    // Matches GNU `Fset_syntax_table` — just bset_syntax_table on the
    // slot. Motion code reads it live via `SyntaxTable::for_buffer`.
    set_current_buffer_syntax_table_object_in_buffers(buffers, table)?;
    Ok(table)
}

// ===========================================================================
// Builtin functions (evaluator-dependent — operate on current buffer)
// ===========================================================================

/// `(modify-syntax-entry CHAR NEWENTRY &optional SYNTAX-TABLE)`
pub(crate) fn builtin_modify_syntax_entry(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    modify_syntax_entry_in_buffers(&mut eval.buffers, &args)
}

pub(crate) fn modify_syntax_entry_in_buffers(
    buffers: &mut BufferManager,
    args: &[Value],
) -> EvalResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("modify-syntax-entry"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let descriptor = syntax_runtime_string(&args[1])?;
    let entry =
        string_to_syntax(&descriptor).map_err(|msg| signal("error", vec![Value::string(&msg)]))?;
    let target_table = if let Some(table) = args.get(2) {
        if builtin_syntax_table_p(vec![*table])?.is_nil() {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("syntax-table-p"), *table],
            ));
        }
        *table
    } else {
        current_buffer_syntax_table_object_in_buffers(buffers)?
    };
    let current_table = current_buffer_syntax_table_object_in_buffers(buffers)?;
    let update_current_buffer_table = target_table == current_table;

    // Update the exposed syntax-table object.
    let chartable_entry = if matches!(entry.class, SyntaxClass::InheritStd) {
        Value::NIL
    } else {
        syntax_entry_to_value(&entry)
    };
    super::chartable::builtin_set_char_table_range(
        vec![target_table, args[0], chartable_entry],
        None,
    )?;

    if !update_current_buffer_table {
        return Ok(Value::NIL);
    }
    // Current buffer's slot already points at `target_table` (it's the
    // same chartable we just mutated above via set-char-table-range).
    // No compiled form to refresh — motion reads the chartable live.
    let _ = target_table;
    Ok(Value::NIL)
}

/// `(char-syntax CHAR)` — return the syntax class designator char.
pub(crate) fn builtin_char_syntax(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_char_syntax_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_char_syntax_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("char-syntax"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let code = match args[0].kind() {
        ValueKind::Fixnum(c)
            if (0..=crate::emacs_core::emacs_char::MAX_CHAR as i64).contains(&c) =>
        {
            c as u32
        }
        ValueKind::Fixnum(_) => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("characterp"), args[0]],
            ));
        }
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("characterp"), args[0]],
            ));
        }
    };

    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    // GNU `Fchar_syntax` (syntax.c): in a unibyte buffer the character is mapped
    // through `make_char_multibyte` before the syntax lookup. A byte 0x80-0xFF
    // becomes its eight-bit character (word syntax); a code >= 0x100 maps past
    // MAX_CHAR to an invalid character, whose syntax is the default whitespace.
    let class = if !buf.get_multibyte() {
        if code < 0x100 {
            SyntaxTable::for_buffer(buf)
                .char_syntax_code(crate::emacs_core::emacs_char::unibyte_to_char(code as u8))
        } else {
            SyntaxClass::Whitespace
        }
    } else {
        SyntaxTable::for_buffer(buf).char_syntax_code(code)
    };
    Ok(Value::char(class.to_char()))
}

/// Extra word-constituent predicate for case operations, honoring
/// `case-symbols-as-words` (GNU `casefiddle.c` `case_ch_is_word`): when that
/// variable is non-nil, symbol-constituent characters (`Ssymbol`, e.g. `_` and
/// `-` in prog modes) also count as word constituents for word-boundary
/// detection. The returned closure is `true` only for symbol-constituent
/// characters and only while the variable is set; callers OR it with their base
/// (`Sword`/alphanumeric) word check. Uses the current buffer's syntax table,
/// matching GNU's `SETUP_BUFFER_SYNTAX_TABLE`.
pub(crate) fn case_symbols_as_words_predicate(
    eval: &super::eval::Context,
) -> impl Fn(u32) -> bool + Copy + 'static {
    let chartable = eval
        .eval_symbol("case-symbols-as-words")
        .unwrap_or(Value::NIL)
        .is_truthy()
        .then(|| {
            eval.buffers
                .current_buffer()
                .map(|buf| SyntaxTable::for_buffer(buf).chartable)
        })
        .flatten();
    move |code: u32| {
        chartable
            .is_some_and(|table| syntax_class_at_char_code(&table, code) == SyntaxClass::Symbol)
    }
}

/// `(syntax-after POS)` — return syntax descriptor for char at POS.
pub(crate) fn builtin_syntax_after(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_syntax_after_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_syntax_after_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("syntax-after"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let pos = match args[0].kind() {
        ValueKind::Fixnum(n) => n,
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("number-or-marker-p"), args[0]],
            ));
        }
    };
    if pos <= 0 {
        return Ok(Value::NIL);
    }

    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let char_index = LispCharPos1::new(pos)
        .to_char_pos()
        .min(buf.total_char_end_pos());
    let byte_index = buffer_char_to_emacs_byte_pos(buf, char_index);
    let Some(unit) = buffer_syntax_char_after(buf, byte_index) else {
        return Ok(Value::NIL);
    };

    let entry = effective_syntax_entry_for_char_at_byte(
        buf,
        &SyntaxTable::for_buffer(buf),
        unit.ch,
        byte_index,
        true,
    );
    Ok(syntax_entry_to_value(&entry))
}

/// `(forward-comment COUNT)` — move point over COUNT comment/whitespace
/// constructs. Returns `t` if all COUNT were successfully skipped, `nil`
/// if scanning stopped early (hit non-comment/non-whitespace or buffer
/// boundary).
pub(crate) fn builtin_forward_comment(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let honor_properties = parse_sexp_lookup_properties_enabled(eval);
    builtin_forward_comment_in_buffers(&mut eval.buffers, args, honor_properties)
}

pub(crate) fn builtin_forward_comment_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
    honor_properties: bool,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("forward-comment"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let count = match args[0].kind() {
        ValueKind::Fixnum(n) => n,
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("integerp"), args[0]],
            ));
        }
    };

    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if count == 0 {
        return Ok(Value::T);
    }

    let (ok, final_pos) = {
        let buf = buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let mut scanner = ForwardCommentCursor::new(buf);
        let ok = if count > 0 {
            forward_comment_forward(&mut scanner, count as u64, honor_properties)
        } else {
            forward_comment_backward(&mut scanner, (-count) as u64, honor_properties)
        };
        (ok, scanner.point_emacs_byte_pos())
    };
    let _ = buffers.goto_buffer_emacs_byte_pos(current_id, final_pos);
    Ok(if ok { Value::T } else { Value::NIL })
}

struct ForwardCommentCursor<'a> {
    buffer: &'a Buffer,
    point: EmacsBytePos,
}

impl<'a> ForwardCommentCursor<'a> {
    fn new(buffer: &'a Buffer) -> Self {
        Self {
            buffer,
            point: buffer.point_emacs_byte_pos(),
        }
    }

    fn point_emacs_byte_pos(&self) -> EmacsBytePos {
        self.point
    }

    fn goto_emacs_byte_pos(&mut self, point: EmacsBytePos) {
        self.point = self.buffer.accessible_emacs_byte_region().clamp(point);
    }
}

impl Deref for ForwardCommentCursor<'_> {
    type Target = Buffer;

    fn deref(&self) -> &Self::Target {
        self.buffer
    }
}

/// Skip whitespace and comments forward. Returns true if all `count`
/// comments were skipped successfully.
fn forward_comment_forward(
    buf: &mut ForwardCommentCursor<'_>,
    count: u64,
    honor_properties: bool,
) -> bool {
    let mut remaining = count;
    let accessible = buf.accessible_emacs_byte_region();
    let max = accessible.end();

    while remaining > 0 {
        // Phase 1: skip whitespace (and stray EndComment newlines).
        loop {
            let pt = buf.point_emacs_byte_pos();
            if pt >= max {
                return false;
            }
            let Some(unit) = buffer_syntax_char_after(buf, pt) else {
                return false;
            };
            let entry = effective_syntax_entry_for_char_at_byte(
                buf,
                &SyntaxTable::for_buffer(buf),
                unit.ch,
                pt,
                honor_properties,
            );
            let class = entry.class;

            if class == SyntaxClass::Whitespace {
                buf.goto_emacs_byte_pos(unit.end);
                continue;
            }
            // In GNU Emacs, EndComment newline is treated as whitespace
            // for forward scanning.
            if class == SyntaxClass::EndComment && unit.ch == '\n' {
                buf.goto_emacs_byte_pos(unit.end);
                continue;
            }
            break;
        }

        // Phase 2: detect comment start.
        let pt = buf.point_emacs_byte_pos();
        if pt >= max {
            return false;
        }
        let Some(unit) = buffer_syntax_char_after(buf, pt) else {
            return false;
        };
        let entry = effective_syntax_entry_for_char_at_byte(
            buf,
            &SyntaxTable::for_buffer(buf),
            unit.ch,
            pt,
            honor_properties,
        );
        let class = entry.class;
        let flags = entry.flags;

        // Single-char comment start (class `<`).
        if class == SyntaxClass::Comment {
            let style_b = flags.contains(SyntaxFlags::COMMENT_STYLE_B);
            let nested = flags.contains(SyntaxFlags::COMMENT_NESTABLE);
            buf.goto_emacs_byte_pos(unit.end);
            if !scan_forward_comment_body(buf, style_b, nested, honor_properties) {
                return false;
            }
            remaining -= 1;
            continue;
        }

        // Comment fence (class `!` = Generic).
        if class == SyntaxClass::CommentFence {
            buf.goto_emacs_byte_pos(unit.end);
            // Scan forward for matching comment fence.
            if !scan_forward_comment_fence(buf, honor_properties) {
                return false;
            }
            remaining -= 1;
            continue;
        }

        // Two-char comment start: check COMMENT_START_FIRST on current
        // char and COMMENT_START_SECOND on next char.
        if flags.contains(SyntaxFlags::COMMENT_START_FIRST) {
            let next_pos = unit.end;
            if next_pos < max {
                if let Some(unit2) = buffer_syntax_char_after(buf, next_pos) {
                    let entry2 = effective_syntax_entry_for_char_at_byte(
                        buf,
                        &SyntaxTable::for_buffer(buf),
                        unit2.ch,
                        next_pos,
                        honor_properties,
                    );
                    let flags2 = entry2.flags;
                    if flags2.contains(SyntaxFlags::COMMENT_START_SECOND) {
                        let style_b = flags2.contains(SyntaxFlags::COMMENT_STYLE_B);
                        let nested = flags2.contains(SyntaxFlags::COMMENT_NESTABLE)
                            || flags.contains(SyntaxFlags::COMMENT_NESTABLE);
                        buf.goto_emacs_byte_pos(unit2.end);
                        if !scan_forward_comment_body(buf, style_b, nested, honor_properties) {
                            return false;
                        }
                        remaining -= 1;
                        continue;
                    }
                }
            }
        }

        // Not whitespace or comment — stop.
        return false;
    }

    true
}

/// Scan forward through comment body until matching comment end.
/// Point should be positioned right after the comment start.
/// Returns true if comment end was found.
fn scan_forward_comment_body(
    buf: &mut ForwardCommentCursor<'_>,
    style_b: bool,
    nested: bool,
    honor_properties: bool,
) -> bool {
    let mut nesting = 1i32;
    let max = buf.accessible_emacs_byte_region().end();

    loop {
        let pt = buf.point_emacs_byte_pos();
        if pt >= max {
            return false;
        }
        let Some(unit) = buffer_syntax_char_after(buf, pt) else {
            return false;
        };
        let entry = effective_syntax_entry_for_char_at_byte(
            buf,
            &SyntaxTable::for_buffer(buf),
            unit.ch,
            pt,
            honor_properties,
        );
        let class = entry.class;
        let flags = entry.flags;

        // Handle escape / charquote.
        if class == SyntaxClass::Escape || class == SyntaxClass::CharQuote {
            buf.goto_emacs_byte_pos(unit.end);
            // Skip the next char too.
            let pt2 = buf.point_emacs_byte_pos();
            if pt2 >= max {
                return false;
            }
            if let Some(unit2) = buffer_syntax_char_after(buf, pt2) {
                buf.goto_emacs_byte_pos(unit2.end);
            }
            continue;
        }

        // Nested comment start (only if nested flag is set).
        if nested {
            if class == SyntaxClass::Comment {
                let sf_b = flags.contains(SyntaxFlags::COMMENT_STYLE_B);
                if sf_b == style_b {
                    nesting += 1;
                    buf.goto_emacs_byte_pos(unit.end);
                    continue;
                }
            }

            if flags.contains(SyntaxFlags::COMMENT_START_FIRST) {
                let next_pos = unit.end;
                if next_pos < max {
                    if let Some(unit2) = buffer_syntax_char_after(buf, next_pos) {
                        let entry2 = effective_syntax_entry_for_char_at_byte(
                            buf,
                            &SyntaxTable::for_buffer(buf),
                            unit2.ch,
                            next_pos,
                            honor_properties,
                        );
                        let flags2 = entry2.flags;
                        if flags2.contains(SyntaxFlags::COMMENT_START_SECOND) {
                            let sf_b = flags2.contains(SyntaxFlags::COMMENT_STYLE_B);
                            if sf_b == style_b {
                                nesting += 1;
                                buf.goto_emacs_byte_pos(unit2.end);
                                continue;
                            }
                        }
                    }
                }
            }
        }

        // Single-char comment end (class `>`).
        if class == SyntaxClass::EndComment {
            let se_b = flags.contains(SyntaxFlags::COMMENT_STYLE_B);
            if se_b == style_b {
                buf.goto_emacs_byte_pos(unit.end);
                nesting -= 1;
                if nesting <= 0 {
                    return true;
                }
                continue;
            }
        }

        // Comment fence end.
        if class == SyntaxClass::CommentFence {
            buf.goto_emacs_byte_pos(unit.end);
            nesting -= 1;
            if nesting <= 0 {
                return true;
            }
            continue;
        }

        // Two-char comment end.
        if flags.contains(SyntaxFlags::COMMENT_END_FIRST) {
            let next_pos = unit.end;
            if next_pos < max {
                if let Some(unit2) = buffer_syntax_char_after(buf, next_pos) {
                    let entry2 = effective_syntax_entry_for_char_at_byte(
                        buf,
                        &SyntaxTable::for_buffer(buf),
                        unit2.ch,
                        next_pos,
                        honor_properties,
                    );
                    let flags2 = entry2.flags;
                    if flags2.contains(SyntaxFlags::COMMENT_END_SECOND) {
                        let se_b = flags.contains(SyntaxFlags::COMMENT_STYLE_B);
                        if se_b == style_b {
                            buf.goto_emacs_byte_pos(unit2.end);
                            nesting -= 1;
                            if nesting <= 0 {
                                return true;
                            }
                            continue;
                        }
                    }
                }
            }
        }

        buf.goto_emacs_byte_pos(unit.end);
    }
}

/// Scan forward for matching comment fence character.
fn scan_forward_comment_fence(buf: &mut ForwardCommentCursor<'_>, honor_properties: bool) -> bool {
    let max = buf.accessible_emacs_byte_region().end();
    loop {
        let pt = buf.point_emacs_byte_pos();
        if pt >= max {
            return false;
        }
        let Some(unit) = buffer_syntax_char_after(buf, pt) else {
            return false;
        };
        let entry = effective_syntax_entry_for_char_at_byte(
            buf,
            &SyntaxTable::for_buffer(buf),
            unit.ch,
            pt,
            honor_properties,
        );
        let class = entry.class;

        if class == SyntaxClass::Escape || class == SyntaxClass::CharQuote {
            buf.goto_emacs_byte_pos(unit.end);
            let pt2 = buf.point_emacs_byte_pos();
            if pt2 >= max {
                return false;
            }
            if let Some(unit2) = buffer_syntax_char_after(buf, pt2) {
                buf.goto_emacs_byte_pos(unit2.end);
            }
            continue;
        }

        buf.goto_emacs_byte_pos(unit.end);

        if class == SyntaxClass::CommentFence {
            return true;
        }
    }
}

/// Skip whitespace and comments backward. Returns true if all `count`
/// comments were skipped successfully.
fn forward_comment_backward(
    buf: &mut ForwardCommentCursor<'_>,
    count: u64,
    honor_properties: bool,
) -> bool {
    let mut remaining = count;
    let accessible = buf.accessible_emacs_byte_region();
    let min = accessible.start();

    // Outer loop: skip `remaining` comments backward.
    while remaining > 0 {
        // Inner loop: scan backward character-by-character to find one
        // comment to skip.  This mirrors GNU Emacs's Fforward_comment
        // backward logic: each iteration decrements point, inspects
        // the character, and either (a) skips whitespace, (b) enters
        // backward comment scanning, or (c) gives up on
        // non-comment/non-whitespace.
        loop {
            let pt = buf.point_emacs_byte_pos();
            if pt <= min {
                return false;
            }
            let Some(unit) = buffer_syntax_char_before(buf, pt) else {
                return false;
            };
            let ch_pos = unit.start;
            let entry = effective_syntax_entry_for_char_at_byte(
                buf,
                &SyntaxTable::for_buffer(buf),
                unit.ch,
                ch_pos,
                honor_properties,
            );
            let class = entry.class;
            let flags = entry.flags;

            let mut code = class;
            let mut comstyle_b = false;
            let mut nested = flags.contains(SyntaxFlags::COMMENT_NESTABLE);
            let mut two_char_end_restore_pos = None;

            if class == SyntaxClass::EndComment {
                comstyle_b = flags.contains(SyntaxFlags::COMMENT_STYLE_B);
            }

            // Check for two-char comment end: current char has
            // COMMENT_END_SECOND, prev char has COMMENT_END_FIRST.
            if flags.contains(SyntaxFlags::COMMENT_END_SECOND) {
                let prev_pos = unit.start;
                if prev_pos > min {
                    if let Some(unit2) = buffer_syntax_char_before(buf, prev_pos) {
                        let ch2_pos = unit2.start;
                        let entry2 = effective_syntax_entry_for_char_at_byte(
                            buf,
                            &SyntaxTable::for_buffer(buf),
                            unit2.ch,
                            ch2_pos,
                            honor_properties,
                        );
                        let flags2 = entry2.flags;
                        if flags2.contains(SyntaxFlags::COMMENT_END_FIRST) {
                            code = SyntaxClass::EndComment;
                            comstyle_b = flags2.contains(SyntaxFlags::COMMENT_STYLE_B);
                            nested = nested || flags2.contains(SyntaxFlags::COMMENT_NESTABLE);
                            two_char_end_restore_pos = Some(unit2.end);
                            // Move past both chars of the two-char end.
                            buf.goto_emacs_byte_pos(unit2.start);
                        }
                    }
                }
            }

            // Comment fence backward.
            if code == SyntaxClass::CommentFence {
                buf.goto_emacs_byte_pos(unit.start);
                if !scan_backward_comment_fence(buf, honor_properties) {
                    buf.goto_emacs_byte_pos(pt);
                    return false;
                }
                // Successfully skipped one comment via fence.
                break;
            }

            if code == SyntaxClass::EndComment {
                // If we didn't already move point for a two-char end,
                // move past the single-char end now.
                if buf.point_emacs_byte_pos() == pt {
                    buf.goto_emacs_byte_pos(unit.start);
                }
                let saved = buf.point_emacs_byte_pos();
                if scan_backward_comment_body(buf, comstyle_b, nested, honor_properties) {
                    // Successfully scanned back through the comment body.
                    break;
                }
                // scan_backward_comment_body failed.
                if unit.ch == '\n' {
                    // GNU: "This end-of-line is not an end-of-comment.
                    // Treat it like a whitespace."
                    // Restore to just before the newline and continue
                    // the inner loop.
                    buf.goto_emacs_byte_pos(unit.start);
                    continue;
                }
                // Non-newline EndComment that failed to find a matching
                // comment start — failure.
                if class != SyntaxClass::EndComment {
                    // Was a two-char sequence: restore one char forward.
                    buf.goto_emacs_byte_pos(
                        two_char_end_restore_pos.unwrap_or(saved.add_len(unit.byte_len())),
                    );
                } else {
                    buf.goto_emacs_byte_pos(pt);
                }
                return false;
            }

            if class == SyntaxClass::Whitespace {
                buf.goto_emacs_byte_pos(unit.start);
                continue;
            }

            // Not whitespace, not comment end — stop.
            return false;
        }
        remaining -= 1;
    }

    true
}

/// Scan backward through comment body to find matching comment start.
///
/// This is a simplified version of GNU Emacs's `back_comment()`.  Point
/// should be positioned right after the comment-end delimiter has been
/// consumed (i.e. just before the comment body).
///
/// For **nested** comments the function returns as soon as the nesting
/// count drops to zero.
///
/// For **non-nested** comments the function scans all the way backward,
/// recording the *earliest* comment-starter of the matching style it
/// finds.  A same-style comment-ender encountered during the scan means
/// "anything before this belongs to a different comment" and stops the
/// search.  At the end, point is set to the recorded position.
fn scan_backward_comment_body(
    buf: &mut ForwardCommentCursor<'_>,
    style_b: bool,
    nested: bool,
    honor_properties: bool,
) -> bool {
    let mut nesting = 1i32;
    let min = buf.accessible_emacs_byte_region().start();

    // For non-nested comments: record the earliest matching comment-start
    // seen so far.
    let mut comstart_pos: Option<EmacsBytePos> = None;

    loop {
        let pt = buf.point_emacs_byte_pos();
        if pt <= min {
            // Reached beginning of accessible region.
            break;
        }
        let Some(unit) = buffer_syntax_char_before(buf, pt) else {
            break;
        };
        let ch_pos = unit.start;
        let entry = effective_syntax_entry_for_char_at_byte(
            buf,
            &SyntaxTable::for_buffer(buf),
            unit.ch,
            ch_pos,
            honor_properties,
        );
        let class = entry.class;
        let flags = entry.flags;

        // ── Comment-end (same style) ──────────────────────────────
        // For nested: increases nesting.
        // For non-nested: means our comment can't extend past this,
        //   so stop scanning.
        if class == SyntaxClass::EndComment {
            let se_b = flags.contains(SyntaxFlags::COMMENT_STYLE_B);
            if se_b == style_b {
                if nested {
                    nesting += 1;
                    buf.goto_emacs_byte_pos(unit.start);
                    continue;
                } else {
                    // Non-nested: this is a same-style comment ender.
                    // Anything before this can't be our comment start
                    // because it would match this ender instead.
                    break;
                }
            }
        }

        // Two-char comment end backward.
        if flags.contains(SyntaxFlags::COMMENT_END_SECOND) {
            let prev_pos = unit.start;
            if prev_pos > min {
                if let Some(unit2) = buffer_syntax_char_before(buf, prev_pos) {
                    let ch2_pos = unit2.start;
                    let entry2 = effective_syntax_entry_for_char_at_byte(
                        buf,
                        &SyntaxTable::for_buffer(buf),
                        unit2.ch,
                        ch2_pos,
                        honor_properties,
                    );
                    let flags2 = entry2.flags;
                    if flags2.contains(SyntaxFlags::COMMENT_END_FIRST) {
                        let se_b = flags2.contains(SyntaxFlags::COMMENT_STYLE_B);
                        if se_b == style_b {
                            if nested {
                                nesting += 1;
                                buf.goto_emacs_byte_pos(unit2.start);
                                continue;
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
        }

        // ── Single-char comment start (class `<`) ────────────────
        if class == SyntaxClass::Comment {
            let sc_b = flags.contains(SyntaxFlags::COMMENT_STYLE_B);
            if sc_b == style_b {
                let new_pos = unit.start;
                if nested {
                    buf.goto_emacs_byte_pos(new_pos);
                    nesting -= 1;
                    if nesting <= 0 {
                        return true;
                    }
                    continue;
                } else {
                    // Non-nested: record this as the best (earliest)
                    // comment-start candidate and keep scanning.
                    comstart_pos = Some(new_pos);
                    buf.goto_emacs_byte_pos(new_pos);
                    continue;
                }
            }
        }

        // ── Comment fence ────────────────────────────────────────
        if class == SyntaxClass::CommentFence {
            let new_pos = unit.start;
            buf.goto_emacs_byte_pos(new_pos);
            if nested {
                nesting -= 1;
                if nesting <= 0 {
                    return true;
                }
            } else {
                comstart_pos = Some(new_pos);
            }
            continue;
        }

        // ── Two-char comment start backward ──────────────────────
        // COMMENT_START_SECOND on current char, COMMENT_START_FIRST
        // on the preceding char.
        if flags.contains(SyntaxFlags::COMMENT_START_SECOND) {
            let prev_pos = unit.start;
            if prev_pos > min {
                if let Some(unit2) = buffer_syntax_char_before(buf, prev_pos) {
                    let ch2_pos = unit2.start;
                    let entry2 = effective_syntax_entry_for_char_at_byte(
                        buf,
                        &SyntaxTable::for_buffer(buf),
                        unit2.ch,
                        ch2_pos,
                        honor_properties,
                    );
                    let flags2 = entry2.flags;
                    if flags2.contains(SyntaxFlags::COMMENT_START_FIRST) {
                        let sc_b = flags.contains(SyntaxFlags::COMMENT_STYLE_B);
                        if sc_b == style_b {
                            let new_pos = unit2.start;
                            if nested {
                                buf.goto_emacs_byte_pos(new_pos);
                                nesting -= 1;
                                if nesting <= 0 {
                                    return true;
                                }
                                continue;
                            } else {
                                comstart_pos = Some(new_pos);
                                buf.goto_emacs_byte_pos(new_pos);
                                continue;
                            }
                        }
                    }
                }
            }
        }

        // Default: skip this character and continue scanning.
        buf.goto_emacs_byte_pos(unit.start);
    }

    // For non-nested comments, check if we recorded any comment-start.
    if !nested {
        if let Some(pos) = comstart_pos {
            buf.goto_emacs_byte_pos(pos);
            return true;
        }
    }

    false
}

/// Scan backward for matching comment fence character.
fn scan_backward_comment_fence(buf: &mut ForwardCommentCursor<'_>, honor_properties: bool) -> bool {
    let min = buf.accessible_emacs_byte_region().start();
    loop {
        let pt = buf.point_emacs_byte_pos();
        if pt <= min {
            return false;
        }
        let Some(unit) = buffer_syntax_char_before(buf, pt) else {
            return false;
        };
        let ch_pos = unit.start;
        let entry = effective_syntax_entry_for_char_at_byte(
            buf,
            &SyntaxTable::for_buffer(buf),
            unit.ch,
            ch_pos,
            honor_properties,
        );
        let class = entry.class;

        buf.goto_emacs_byte_pos(unit.start);

        if class == SyntaxClass::CommentFence {
            return true;
        }
    }
}

/// `(backward-prefix-chars)` — move point backward over prefix-syntax chars.
pub(crate) fn builtin_backward_prefix_chars(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let honor_properties = parse_sexp_lookup_properties_enabled(eval);
    builtin_backward_prefix_chars_in_buffers(&mut eval.buffers, args, honor_properties)
}

pub(crate) fn builtin_backward_prefix_chars_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
    honor_properties: bool,
) -> EvalResult {
    if !args.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("backward-prefix-chars"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let min = buf.accessible_emacs_byte_region().start();
    let mut final_pos = buf.point_emacs_byte_pos();
    loop {
        let pt = final_pos;
        if pt <= min {
            break;
        }
        let Some(unit) = buffer_syntax_char_before(buf, pt) else {
            break;
        };
        let ch_pos = unit.start;
        let entry = effective_syntax_entry_for_char_at_byte(
            buf,
            &SyntaxTable::for_buffer(buf),
            unit.ch,
            ch_pos,
            honor_properties,
        );
        let is_prefix =
            entry.class == SyntaxClass::Quote || entry.flags.contains(SyntaxFlags::PREFIX);
        if !is_prefix {
            break;
        }
        final_pos = unit.start;
    }

    let _ = buffers.goto_buffer_emacs_byte_pos(current_id, final_pos);

    Ok(Value::NIL)
}

/// `(forward-word &optional COUNT)` — move point forward COUNT words.
pub(crate) fn builtin_forward_word(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        match args[0].kind() {
            ValueKind::Fixnum(n) => n,
            _ => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), args[0]],
                ));
            }
        }
    };

    let honor_properties = parse_sexp_lookup_properties_enabled(eval);
    let orig_byte = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf.point_emacs_byte_pos()
    };
    // When `find-word-boundary-function-table` is active (subword/superword),
    // GNU's scan_words consults it per word; otherwise the plain syntax scan is
    // used unchanged.
    let wbtable = eval.visible_variable_value_or_nil("find-word-boundary-function-table");
    let (raw_byte, completed) = if word_boundary_table_active(&wbtable) {
        word_motion_with_table(eval, count, honor_properties, wbtable)
    } else {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let table = SyntaxTable::for_buffer(buf);
        forward_word_with_options(buf, &table, count, honor_properties)
    };
    let (orig_char, raw_char) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            buffer_byte_to_lisp_pos(buf, orig_byte),
            buffer_byte_to_lisp_pos(buf, raw_byte),
        )
    };

    // GNU `Fforward_word` (syntax.c:1561) constrains the destination via
    // `Fconstrain_to_field` so that motion does not cross input-field
    // boundaries (e.g. the minibuffer prompt). The full call is
    // (constrain-to-field VAL PT nil nil nil).
    let constrained = crate::emacs_core::builtins::builtin_constrain_to_field(
        eval,
        vec![
            Value::fixnum(raw_char),
            Value::fixnum(orig_char),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )?;
    let constrained_char = match constrained.kind() {
        ValueKind::Fixnum(n) => n,
        _ => raw_char,
    };

    let final_byte = if constrained_char == raw_char {
        raw_byte
    } else {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        // Convert constrained 1-based char position back to a byte offset.
        let zero_based = (constrained_char - 1).max(0) as usize;
        buffer_char_to_emacs_byte_pos(buf, CharPos0::new(zero_based))
    };

    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(current_id, final_byte);

    // GNU returns t when the requested motion fully succeeded, nil when it
    // stopped early at a buffer edge or a field boundary.
    Ok(if completed && constrained_char == raw_char {
        Value::T
    } else {
        Value::NIL
    })
}

pub(crate) fn builtin_forward_word_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        match args[0].kind() {
            ValueKind::Fixnum(n) => n,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), args[0]],
                ));
            }
        }
    };

    // We need to read the syntax table first, then call forward_word, then write point.
    // To satisfy the borrow checker, clone the syntax table.
    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);
    let new_pos = forward_word(buf, &table, count);

    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let _ = buffers.goto_buffer_emacs_byte_pos(current_id, new_pos);
    Ok(Value::NIL)
}

/// `(backward-word &optional COUNT)` — move point backward COUNT words.
pub(crate) fn builtin_backward_word(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        match args[0].kind() {
            ValueKind::Fixnum(n) => n,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), args[0]],
                ));
            }
        }
    };

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);
    let honor_properties = parse_sexp_lookup_properties_enabled(eval);
    let new_pos = backward_word_with_options(buf, &table, count, honor_properties).0;

    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let _ = eval.buffers.goto_buffer_emacs_byte_pos(current_id, new_pos);
    Ok(Value::NIL)
}

/// `(forward-sexp &optional COUNT)` — move point forward over COUNT balanced
/// expressions.
pub(crate) fn builtin_forward_sexp(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1i64
    } else {
        match args[0].kind() {
            ValueKind::Fixnum(n) => n,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), args[0]],
                ));
            }
        }
    };

    let honor_properties = parse_sexp_lookup_properties_enabled(eval);
    if honor_properties {
        let target = eval
            .buffers
            .current_buffer()
            .map(|buf| buf.accessible_char_region().end().get().saturating_add(1))
            .unwrap_or(1);
        maybe_syntax_propertize_for_scan(eval, target)?;
    }
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);
    let from = buf.point_emacs_byte_pos();
    let ignore_comments = parse_sexp_ignore_comments_enabled(eval);
    let new_pos = match scan_sexps_with_options(
        buf,
        &table,
        from.get(),
        count,
        honor_properties,
        ignore_comments,
    )
    .map_err(|err| signal("scan-error", err.signal_data()))?
    {
        Some(pos) => EmacsBytePos::new(pos),
        None if count < 0 => buf.accessible_emacs_byte_region().start(),
        None => buf.accessible_emacs_byte_region().end(),
    };

    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let _ = eval.buffers.goto_buffer_emacs_byte_pos(current_id, new_pos);
    Ok(Value::NIL)
}

/// `(backward-sexp &optional COUNT)` — move point backward over COUNT balanced
/// expressions.
pub(crate) fn builtin_backward_sexp(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1i64
    } else {
        match args[0].kind() {
            ValueKind::Fixnum(n) => n,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), args[0]],
                ));
            }
        }
    };

    let honor_properties = parse_sexp_lookup_properties_enabled(eval);
    if honor_properties {
        let target = eval
            .buffers
            .current_buffer()
            .map(|buf| buf.accessible_char_region().end().get().saturating_add(1))
            .unwrap_or(1);
        maybe_syntax_propertize_for_scan(eval, target)?;
    }
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);
    let from = buf.point_emacs_byte_pos();
    // backward-sexp with positive count => scan_sexps with negative count
    let ignore_comments = parse_sexp_ignore_comments_enabled(eval);
    let new_pos = match scan_sexps_with_options(
        buf,
        &table,
        from.get(),
        -count,
        honor_properties,
        ignore_comments,
    )
    .map_err(|err| signal("scan-error", err.signal_data()))?
    {
        Some(pos) => EmacsBytePos::new(pos),
        None if count < 0 => buf.accessible_emacs_byte_region().end(),
        None => buf.accessible_emacs_byte_region().start(),
    };

    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let _ = eval.buffers.goto_buffer_emacs_byte_pos(current_id, new_pos);
    Ok(Value::NIL)
}

/// `(scan-lists FROM COUNT DEPTH)` — scan across balanced expressions.
///
/// This uses the same core scanner as `forward-sexp`/`backward-sexp`.
pub(crate) fn builtin_scan_lists(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    if args.len() != 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("scan-lists"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let from = match args[0].kind() {
        ValueKind::Fixnum(n) => n,
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("integer-or-marker-p"), args[0]],
            ));
        }
    };
    let count = match args[1].kind() {
        ValueKind::Fixnum(n) => n,
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("integerp"), args[1]],
            ));
        }
    };
    let depth = match args[2].kind() {
        ValueKind::Fixnum(n) => n,
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("integerp"), args[2]],
            ));
        }
    };

    let honor_properties = parse_sexp_lookup_properties_enabled(ctx);
    if honor_properties {
        let target = ctx
            .buffers
            .current_buffer()
            .map(|buf| buf.accessible_char_region().end().get().saturating_add(1))
            .unwrap_or(1);
        maybe_syntax_propertize_for_scan(ctx, target)?;
    }

    let buf = ctx
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);

    let accessible_chars = buf.accessible_char_region();
    let point_min = accessible_chars.start_lisp().as_i64();
    let point_max = accessible_chars.end_lisp().as_i64();
    let clipped_from = from.clamp(point_min, point_max);
    let from_char = LispCharPos1::new(clipped_from).to_char_pos().get();

    let ignore_comments = parse_sexp_ignore_comments_enabled(ctx);
    match scan_lists_with_options(
        buf,
        &table,
        from_char,
        count,
        depth,
        honor_properties,
        ignore_comments,
    ) {
        Ok(Some(new_char)) => Ok(Value::fixnum(char_pos_to_lisp_i64(new_char))),
        Ok(None) => Ok(Value::NIL),
        Err(err) => Err(signal("scan-error", err.signal_data())),
    }
}

/// `(scan-sexps FROM COUNT)` — scan over COUNT sexps from FROM.
pub(crate) fn builtin_scan_sexps(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    if args.len() != 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("scan-sexps"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let from = match args[0].kind() {
        ValueKind::Fixnum(n) => n,
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("number-or-marker-p"), args[0]],
            ));
        }
    };
    let count = match args[1].kind() {
        ValueKind::Fixnum(n) => n,
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("integerp"), args[1]],
            ));
        }
    };

    let honor_properties = parse_sexp_lookup_properties_enabled(ctx);
    if honor_properties {
        let target = ctx
            .buffers
            .current_buffer()
            .map(|buf| buf.accessible_char_region().end().get().saturating_add(1))
            .unwrap_or(1);
        maybe_syntax_propertize_for_scan(ctx, target)?;
    }

    let buf = ctx
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);

    let from_char = LispCharPos1::new(from)
        .to_char_pos()
        .min(buf.total_char_end_pos());
    let from_byte = buffer_char_to_emacs_byte_pos(buf, from_char);

    let ignore_comments = parse_sexp_ignore_comments_enabled(ctx);
    match scan_sexps_with_options(
        buf,
        &table,
        from_byte.get(),
        count,
        honor_properties,
        ignore_comments,
    ) {
        Ok(Some(new_byte)) => Ok(Value::fixnum(buffer_byte_to_lisp_pos(
            buf,
            EmacsBytePos::new(new_byte),
        ))),
        Ok(None) => Ok(Value::NIL),
        Err(err) => Err(signal("scan-error", err.signal_data())),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParseStringState {
    Delim(char),
    Fence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParseCommentState {
    Syntax {
        depth: i64,
        style_b: bool,
        nestable: bool,
    },
    Fence {
        depth: i64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommentStopMode {
    None,
    Comment,
    SyntaxTable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PartialParseState {
    depth: i64,
    mindepth: i64,
    levels: Vec<PartialParseLevel>,
    in_string: Option<ParseStringState>,
    in_comment: Option<ParseCommentState>,
    comment_or_string_start: Option<i64>,
    quoted: bool,
    in_string_from_oldstate: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PartialParseLevel {
    last: Option<i64>,
    prev: Option<i64>,
}

impl PartialParseState {
    fn new() -> Self {
        Self {
            depth: 0,
            mindepth: 0,
            levels: vec![PartialParseLevel::default()],
            in_string: None,
            in_comment: None,
            comment_or_string_start: None,
            quoted: false,
            in_string_from_oldstate: false,
        }
    }

    fn from_oldstate(oldstate: Option<&Value>) -> Self {
        let mut state = Self::new();
        let Some(oldstate) = oldstate else {
            return state;
        };
        let Some(items) = list_to_vec(oldstate) else {
            return state;
        };

        state.depth = items.first().and_then(|v| v.as_fixnum()).unwrap_or(0);
        // GNU `internalize_parse_state` ignores element 6 of OLDSTATE.
        // `scan_sexps_forward` initializes `mindepth` from the incoming depth.
        state.mindepth = state.depth;

        if let Some(start) = items.get(8).and_then(|v| v.as_fixnum()) {
            state.comment_or_string_start = Some(start);
        }

        if let Some(v) = items.get(5) {
            if v.is_t() {
                state.quoted = true;
            }
        }

        if let Some(item) = items.get(3) {
            state.in_string = match item.kind() {
                ValueKind::Nil => None,
                ValueKind::T => Some(ParseStringState::Fence),
                ValueKind::Fixnum(n) => u32::try_from(n)
                    .ok()
                    .and_then(char::from_u32)
                    .map(ParseStringState::Delim),
                _ => None,
            };
            state.in_string_from_oldstate = state.in_string.is_some();
        }

        if let Some(item) = items.get(4) {
            state.in_comment = match item.kind() {
                ValueKind::Nil => None,
                ValueKind::T => Some(ParseCommentState::Syntax {
                    depth: 1,
                    style_b: false,
                    nestable: false,
                }),
                ValueKind::Fixnum(n) => Some(ParseCommentState::Syntax {
                    depth: n,
                    style_b: false,
                    nestable: true,
                }),
                _ => None,
            };
        }

        if let Some(item) = items.get(9)
            && let Some(stack_items) = list_to_vec(item)
        {
            state.levels.clear();
            state.levels.push(PartialParseLevel::default());
            for start in stack_items.into_iter().filter_map(|v| v.as_fixnum()) {
                if let Some(level) = state.levels.last_mut() {
                    level.last = Some(start);
                }
                state.levels.push(PartialParseLevel::default());
            }
        }

        state
    }

    fn current_level_mut(&mut self) -> &mut PartialParseLevel {
        self.levels
            .last_mut()
            .expect("partial parse state always has a current level")
    }

    fn finish_current_level_sexp(&mut self, start: i64) {
        let level = self.current_level_mut();
        level.last = Some(start);
        level.prev = Some(start);
    }

    fn finish_string(&mut self) {
        if !self.in_string_from_oldstate
            && let Some(start) = self.comment_or_string_start
        {
            self.finish_current_level_sexp(start);
        }
        self.in_string = None;
        self.in_string_from_oldstate = false;
        self.comment_or_string_start = None;
    }

    fn open_level(&mut self, start: i64) {
        if let Some(level) = self.levels.last_mut() {
            level.last = Some(start);
        }
        self.depth += 1;
        self.levels.push(PartialParseLevel::default());
    }

    fn close_level(&mut self) {
        self.depth -= 1;
        self.mindepth = self.mindepth.min(self.depth);
        if self.levels.len() > 1 {
            self.levels.pop();
        }
        if let Some(start) = self.current_level_mut().last {
            self.current_level_mut().prev = Some(start);
        }
    }

    fn containing_sexp_start(&self) -> Option<i64> {
        self.levels
            .len()
            .checked_sub(2)
            .and_then(|idx| self.levels.get(idx))
            .and_then(|level| level.last)
    }

    fn current_level_completed_sexp_start(&self) -> Option<i64> {
        self.levels.last().and_then(|level| level.prev)
    }

    fn level_start_positions(&self) -> Vec<i64> {
        if self.levels.len() <= 1 {
            return Vec::new();
        }

        self.levels
            .iter()
            .take(self.levels.len() - 1)
            .filter_map(|level| level.last)
            .collect()
    }

    fn into_value(self) -> Value {
        let containing_sexp_start = self.containing_sexp_start();
        let completed_sexp_start = self.current_level_completed_sexp_start();
        let level_starts = self.level_start_positions();
        let stack_value = if level_starts.is_empty() {
            Value::NIL
        } else {
            Value::list(level_starts.into_iter().map(Value::fixnum).collect())
        };

        let string_value = match self.in_string {
            Some(ParseStringState::Delim(term)) => Value::fixnum(term as i64),
            Some(ParseStringState::Fence) => Value::T,
            None => Value::NIL,
        };

        let comment_value = match self.in_comment {
            Some(ParseCommentState::Syntax {
                depth: comment_depth,
                nestable: false,
                ..
            }) => {
                debug_assert_eq!(comment_depth, 1);
                Value::T
            }
            Some(ParseCommentState::Syntax {
                depth: comment_depth,
                nestable: true,
                ..
            }) => Value::fixnum(comment_depth),
            Some(ParseCommentState::Fence {
                depth: comment_depth,
            }) => Value::fixnum(comment_depth),
            None => Value::NIL,
        };

        Value::list(vec![
            Value::fixnum(self.depth),
            containing_sexp_start.map_or(Value::NIL, Value::fixnum),
            completed_sexp_start.map_or(Value::NIL, Value::fixnum),
            string_value,
            comment_value,
            if self.quoted { Value::T } else { Value::NIL },
            Value::fixnum(self.mindepth),
            Value::NIL,
            self.comment_or_string_start
                .map_or(Value::NIL, Value::fixnum),
            stack_value,
            Value::NIL,
        ])
    }
}

fn syntax_class_and_flags(
    buf: &Buffer,
    table: &SyntaxTable,
    ch: char,
    abs_char: usize,
    honor_properties: bool,
    prop_cache: &SyntaxPropRange,
) -> (SyntaxClass, SyntaxFlags) {
    let entry = effective_syntax_entry_for_abs_char_cached(
        buf,
        table,
        ch,
        abs_char,
        honor_properties,
        prop_cache,
    );
    (entry.class, entry.flags)
}

fn parse_commentstop_mode(arg: Option<&Value>) -> CommentStopMode {
    match arg {
        None => CommentStopMode::None,
        Some(v) if v.is_nil() => CommentStopMode::None,
        Some(v)
            if SyntaxPurposeSymbol::from_lisp_value(v)
                == Some(SyntaxPurposeSymbol::SyntaxTable) =>
        {
            CommentStopMode::SyntaxTable
        }
        Some(_) => CommentStopMode::Comment,
    }
}

fn parse_state_from_range_with_options(
    buf: &Buffer,
    table: &SyntaxTable,
    from: i64,
    to: i64,
    target_depth: Option<i64>,
    stop_before: bool,
    oldstate: Option<&Value>,
    commentstop: CommentStopMode,
    honor_properties: bool,
) -> (Value, i64) {
    let accessible_chars = buf.accessible_char_region();
    let point_min = accessible_chars.start().get();
    let point_max = accessible_chars.end().get();
    let from_char = LispCharPos1::new(from)
        .to_char_pos()
        .get()
        .clamp(point_min, point_max);
    let to_char = LispCharPos1::new(to)
        .to_char_pos()
        .get()
        .clamp(point_min, point_max);
    let chars = BufferChars::new(buf, offset_char_pos(CharPos0::ZERO, from_char));
    let to_idx = to_char - from_char;
    let prop_cache = SyntaxPropRange::new();

    let mut state = PartialParseState::from_oldstate(oldstate);
    let mut idx = 0;
    let mut atom_start: Option<i64> = None;

    let finish_atom = |state: &mut PartialParseState, atom_start: &mut Option<i64>| {
        if let Some(start) = atom_start.take() {
            state.finish_current_level_sexp(start);
        }
    };

    while idx < to_idx {
        let abs_char = from_char + idx;
        let pos1 = (abs_char + 1) as i64;
        let ch = chars.char_at(idx);
        let (class, flags) =
            syntax_class_and_flags(buf, table, ch, abs_char, honor_properties, &prop_cache);

        if state.quoted {
            state.quoted = false;
            idx += 1;
            continue;
        }

        if let Some(string_state) = state.in_string {
            match class {
                SyntaxClass::Escape | SyntaxClass::CharQuote => {
                    idx += 1;
                    if idx < to_idx {
                        idx += 1;
                    } else {
                        state.quoted = true;
                    }
                    continue;
                }
                SyntaxClass::StringFence if string_state == ParseStringState::Fence => {
                    state.finish_string();
                    idx += 1;
                    if commentstop == CommentStopMode::SyntaxTable {
                        break;
                    }
                    continue;
                }
                SyntaxClass::StringDelim if matches!(string_state, ParseStringState::Delim(term) if ch == term) =>
                {
                    state.finish_string();
                    idx += 1;
                    if commentstop == CommentStopMode::SyntaxTable {
                        break;
                    }
                    continue;
                }
                _ => {
                    idx += 1;
                    continue;
                }
            }
        }

        if let Some(comment_state) = state.in_comment {
            match comment_state {
                ParseCommentState::Fence {
                    depth: comment_depth,
                } => {
                    if class == SyntaxClass::CommentFence {
                        let next_depth = comment_depth - 1;
                        idx += 1;
                        if next_depth <= 0 {
                            state.in_comment = None;
                            state.comment_or_string_start = None;
                        } else {
                            state.in_comment = Some(ParseCommentState::Fence { depth: next_depth });
                        }
                        if commentstop == CommentStopMode::SyntaxTable {
                            break;
                        }
                        continue;
                    }
                    if matches!(class, SyntaxClass::Escape | SyntaxClass::CharQuote) {
                        idx += 1;
                        if idx < to_idx {
                            idx += 1;
                        } else {
                            state.quoted = true;
                        }
                        continue;
                    }
                    idx += 1;
                    continue;
                }
                ParseCommentState::Syntax {
                    depth: comment_depth,
                    style_b,
                    nestable,
                } => {
                    if matches!(class, SyntaxClass::Escape | SyntaxClass::CharQuote) {
                        idx += 1;
                        if idx < to_idx {
                            idx += 1;
                        } else {
                            state.quoted = true;
                        }
                        continue;
                    }

                    if nestable {
                        if class == SyntaxClass::Comment
                            && flags.contains(SyntaxFlags::COMMENT_STYLE_B) == style_b
                        {
                            state.in_comment = Some(ParseCommentState::Syntax {
                                depth: comment_depth + 1,
                                style_b,
                                nestable,
                            });
                            idx += 1;
                            continue;
                        }

                        if flags.contains(SyntaxFlags::COMMENT_START_FIRST) && idx + 1 < to_idx {
                            let (_, next_flags) = syntax_class_and_flags(
                                buf,
                                table,
                                chars.char_at(idx + 1),
                                abs_char + 1,
                                honor_properties,
                                &prop_cache,
                            );
                            if next_flags.contains(SyntaxFlags::COMMENT_START_SECOND)
                                && next_flags.contains(SyntaxFlags::COMMENT_STYLE_B) == style_b
                            {
                                state.in_comment = Some(ParseCommentState::Syntax {
                                    depth: comment_depth + 1,
                                    style_b,
                                    nestable,
                                });
                                idx += 2;
                                continue;
                            }
                        }
                    }

                    if class == SyntaxClass::EndComment
                        && flags.contains(SyntaxFlags::COMMENT_STYLE_B) == style_b
                    {
                        let next_depth = comment_depth - 1;
                        idx += 1;
                        if next_depth <= 0 {
                            state.in_comment = None;
                            state.comment_or_string_start = None;
                        } else {
                            state.in_comment = Some(ParseCommentState::Syntax {
                                depth: next_depth,
                                style_b,
                                nestable,
                            });
                        }
                        if commentstop == CommentStopMode::SyntaxTable {
                            break;
                        }
                        continue;
                    }

                    if flags.contains(SyntaxFlags::COMMENT_END_FIRST) && idx + 1 < to_idx {
                        let (_, next_flags) = syntax_class_and_flags(
                            buf,
                            table,
                            chars.char_at(idx + 1),
                            abs_char + 1,
                            honor_properties,
                            &prop_cache,
                        );
                        if next_flags.contains(SyntaxFlags::COMMENT_END_SECOND)
                            && next_flags.contains(SyntaxFlags::COMMENT_STYLE_B) == style_b
                        {
                            let next_depth = comment_depth - 1;
                            idx += 2;
                            if next_depth <= 0 {
                                state.in_comment = None;
                                state.comment_or_string_start = None;
                            } else {
                                state.in_comment = Some(ParseCommentState::Syntax {
                                    depth: next_depth,
                                    style_b,
                                    nestable,
                                });
                            }
                            if commentstop == CommentStopMode::SyntaxTable {
                                break;
                            }
                            continue;
                        }
                    }

                    idx += 1;
                    continue;
                }
            }
        }

        if stop_before
            && matches!(
                class,
                SyntaxClass::Escape
                    | SyntaxClass::CharQuote
                    | SyntaxClass::Word
                    | SyntaxClass::Symbol
                    | SyntaxClass::Open
                    | SyntaxClass::StringDelim
                    | SyntaxClass::StringFence
            )
        {
            break;
        }

        if !matches!(
            class,
            SyntaxClass::Word | SyntaxClass::Symbol | SyntaxClass::Quote
        ) {
            finish_atom(&mut state, &mut atom_start);
        }

        if flags.contains(SyntaxFlags::COMMENT_START_FIRST) && idx + 1 < to_idx {
            let (_, next_flags) = syntax_class_and_flags(
                buf,
                table,
                chars.char_at(idx + 1),
                abs_char + 1,
                honor_properties,
                &prop_cache,
            );
            if next_flags.contains(SyntaxFlags::COMMENT_START_SECOND) {
                state.in_comment = Some(ParseCommentState::Syntax {
                    depth: 1,
                    style_b: next_flags.contains(SyntaxFlags::COMMENT_STYLE_B),
                    nestable: flags.contains(SyntaxFlags::COMMENT_NESTABLE)
                        || next_flags.contains(SyntaxFlags::COMMENT_NESTABLE),
                });
                state.comment_or_string_start = Some(pos1);
                idx += 2;
                if commentstop != CommentStopMode::None {
                    break;
                }
                continue;
            }
        }

        match class {
            SyntaxClass::Open => {
                state.open_level(pos1);
                idx += 1;
                if target_depth == Some(state.depth) {
                    break;
                }
                continue;
            }
            SyntaxClass::Close => {
                state.close_level();
                idx += 1;
                if target_depth == Some(state.depth) {
                    break;
                }
                continue;
            }
            SyntaxClass::StringDelim => {
                state.in_string = Some(ParseStringState::Delim(ch));
                state.in_string_from_oldstate = false;
                state.comment_or_string_start = Some(pos1);
                idx += 1;
                if commentstop == CommentStopMode::SyntaxTable {
                    break;
                }
                continue;
            }
            SyntaxClass::StringFence => {
                state.in_string = Some(ParseStringState::Fence);
                state.in_string_from_oldstate = false;
                state.comment_or_string_start = Some(pos1);
                idx += 1;
                if commentstop == CommentStopMode::SyntaxTable {
                    break;
                }
                continue;
            }
            SyntaxClass::Comment => {
                state.in_comment = Some(ParseCommentState::Syntax {
                    depth: 1,
                    style_b: flags.contains(SyntaxFlags::COMMENT_STYLE_B),
                    nestable: flags.contains(SyntaxFlags::COMMENT_NESTABLE),
                });
                state.comment_or_string_start = Some(pos1);
                idx += 1;
                if commentstop != CommentStopMode::None {
                    break;
                }
                continue;
            }
            SyntaxClass::CommentFence => {
                state.in_comment = Some(ParseCommentState::Fence { depth: 1 });
                state.comment_or_string_start = Some(pos1);
                idx += 1;
                if commentstop != CommentStopMode::None {
                    break;
                }
                continue;
            }
            SyntaxClass::Escape | SyntaxClass::CharQuote => {
                if idx + 1 < to_idx {
                    idx += 2;
                    continue;
                }
                state.quoted = true;
                idx += 1;
                continue;
            }
            SyntaxClass::Word | SyntaxClass::Symbol | SyntaxClass::Quote => {
                atom_start.get_or_insert(pos1);
            }
            SyntaxClass::Whitespace | SyntaxClass::EndComment => {}
            _ => {}
        }

        idx += 1;
    }

    finish_atom(&mut state, &mut atom_start);

    (state.into_value(), char_pos_to_lisp_i64(from_char + idx))
}

fn parse_state_from_range(buf: &Buffer, table: &SyntaxTable, from: i64, to: i64) -> Value {
    parse_state_from_range_with_options(
        buf,
        table,
        from,
        to,
        None,
        false,
        None,
        CommentStopMode::None,
        false,
    )
    .0
}

/// `(parse-partial-sexp FROM TO &optional TARGETDEPTH STOPBEFORE STATE COMMENTSTOP)`
/// Baseline parser-state implementation for structural Lisp motion/state queries.
pub(crate) fn builtin_parse_partial_sexp(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() < 2 || args.len() > 6 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("parse-partial-sexp"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let from = super::position::fix_position_eval(eval, &args[0])?;
    let to = super::position::fix_position_eval(eval, &args[1])?;

    if to < from {
        return Err(signal(
            "error",
            vec![Value::string("End position is smaller than start position")],
        ));
    }

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let accessible_chars = buf.accessible_char_region();
    let point_min = accessible_chars.start_lisp().as_i64();
    let point_max = accessible_chars.end_lisp().as_i64();
    if from < point_min || from > point_max || to < point_min || to > point_max {
        return Err(signal(
            "args-out-of-range",
            vec![Value::make_buffer(buf.id), args[0], args[1]],
        ));
    }
    let table = SyntaxTable::for_buffer(buf);
    let target_depth = match args.get(2) {
        Some(v) if !v.is_nil() => match v.kind() {
            ValueKind::Fixnum(n) => Some(n),
            _ => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), *v],
                ));
            }
        },
        _ => None,
    };
    let stop_before = args.get(3).is_some_and(|v| v.is_truthy());
    let oldstate = args.get(4).filter(|v| !v.is_nil());
    let commentstop = parse_commentstop_mode(args.get(5));
    let honor_properties = parse_sexp_lookup_properties_enabled(eval);
    let (state, stop_pos) = parse_state_from_range_with_options(
        buf,
        &table,
        from,
        to,
        target_depth,
        stop_before,
        oldstate,
        commentstop,
        honor_properties,
    );
    let current_id = buf.id;
    let stop_byte = lisp_pos_to_byte(buf, LispCharPos1::new(stop_pos));
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(current_id, stop_byte);
    Ok(state)
}

/// `(syntax-ppss &optional POS)` — parser state at POS.
pub(crate) fn builtin_syntax_ppss(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("syntax-ppss"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = eval
        .buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);

    let pos = if args.is_empty() || args[0].is_nil() {
        buf.point_lisp_char_pos().as_i64()
    } else {
        match args[0].kind() {
            ValueKind::Fixnum(n) => n,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("number-or-marker-p"), args[0]],
                ));
            }
        }
    };

    let honor_properties = parse_sexp_lookup_properties_enabled(eval);
    let modified_tick = buf.modified_tick();
    let old = eval.syntax_ppss_last.and_then(|last| {
        (last.buffer_id == current_id
            && last.modified_tick == modified_tick
            && last.pos <= pos
            && last.pos >= buf.accessible_char_region().start_lisp().as_i64())
        .then_some((last.pos, last.state))
    });
    let (from, oldstate) = old
        .map(|(old_pos, state)| (old_pos, Some(state)))
        .unwrap_or((1, None));

    let state = parse_state_from_range_with_options(
        buf,
        &table,
        from,
        pos,
        None,
        false,
        oldstate.as_ref(),
        CommentStopMode::None,
        honor_properties,
    )
    .0;

    eval.syntax_ppss_last = Some(super::eval::SyntaxPpssLast {
        buffer_id: current_id,
        pos,
        modified_tick,
        state,
    });

    Ok(state)
}

/// `(syntax-ppss-flush-cache POS &rest _IGNORED)` — flush parser-state cache.
///
/// NeoVM currently computes parser state directly, so this is a no-op that
/// enforces Emacs-compatible arity/type behavior.
pub(crate) fn builtin_syntax_ppss_flush_cache(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("syntax-ppss-flush-cache"), Value::fixnum(0)],
        ));
    }

    match args[0].kind() {
        ValueKind::Fixnum(_) => {
            eval.syntax_ppss_last = None;
            Ok(Value::NIL)
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("number-or-marker-p"), args[0]],
        )),
    }
}

fn lisp_pos_to_byte(buf: &Buffer, pos: LispCharPos1) -> EmacsBytePos {
    buf.lisp_pos_to_accessible_emacs_byte_pos(pos)
}

/// `(skip-syntax-forward SYNTAX &optional LIMIT)` — skip forward over chars
/// matching the given syntax classes.
pub(crate) fn builtin_skip_syntax_forward(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let honor_properties = parse_sexp_lookup_properties_enabled(eval);
    builtin_skip_syntax_forward_in_buffers(&mut eval.buffers, args, honor_properties)
}

pub(crate) fn builtin_skip_syntax_forward_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
    honor_properties: bool,
) -> EvalResult {
    if args.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("skip-syntax-forward"), Value::fixnum(0)],
        ));
    }
    let syntax_chars = syntax_runtime_string(&args[0])?;
    let limit = if args.len() > 1 && !args[1].is_nil() {
        match args[1].kind() {
            ValueKind::Fixnum(n) => Some(n),
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), args[1]],
                ));
            }
        }
    } else {
        None
    };

    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);
    let limit = limit.map(|raw| lisp_pos_to_byte(buf, LispCharPos1::new(raw)).get());
    let new_pos =
        skip_syntax_forward_with_options(buf, &table, &syntax_chars, limit, honor_properties);

    let old_pt = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        .point_emacs_byte_pos();
    let new_pos = EmacsBytePos::new(new_pos);

    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let _ = buffers.goto_buffer_emacs_byte_pos(current_id, new_pos);

    // Return number of characters skipped (Emacs convention).
    let buf = buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let chars_moved = if new_pos >= old_pt {
        buffer_byte_char_delta(buf, old_pt.get(), new_pos.get())
    } else {
        buffer_byte_char_delta(buf, new_pos.get(), old_pt.get())
    };
    Ok(Value::fixnum(chars_moved))
}

/// `(skip-syntax-backward SYNTAX &optional LIMIT)` — skip backward over chars
/// matching the given syntax classes.
pub(crate) fn builtin_skip_syntax_backward(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let honor_properties = parse_sexp_lookup_properties_enabled(eval);
    builtin_skip_syntax_backward_in_buffers(&mut eval.buffers, args, honor_properties)
}

pub(crate) fn builtin_skip_syntax_backward_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
    honor_properties: bool,
) -> EvalResult {
    if args.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("skip-syntax-backward"), Value::fixnum(0)],
        ));
    }
    let syntax_chars = syntax_runtime_string(&args[0])?;
    let limit = if args.len() > 1 && !args[1].is_nil() {
        match args[1].kind() {
            ValueKind::Fixnum(n) => Some(n),
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), args[1]],
                ));
            }
        }
    } else {
        None
    };

    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);
    let limit = limit.map(|raw| lisp_pos_to_byte(buf, LispCharPos1::new(raw)).get());
    let new_pos =
        skip_syntax_backward_with_options(buf, &table, &syntax_chars, limit, honor_properties);

    let old_pt = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        .point_emacs_byte_pos();
    let new_pos = EmacsBytePos::new(new_pos);

    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let _ = buffers.goto_buffer_emacs_byte_pos(current_id, new_pos);

    // Return negative number of characters skipped.
    let buf = buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let chars_moved = if old_pt >= new_pos {
        -buffer_byte_char_delta(buf, new_pos.get(), old_pt.get())
    } else {
        buffer_byte_char_delta(buf, old_pt.get(), new_pos.get())
    };
    Ok(Value::fixnum(chars_moved))
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "syntax_test.rs"]
mod tests;
