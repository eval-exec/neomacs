//! Syntax table system for the Elisp VM.
//!
//! Implements Emacs-compatible syntax tables with character classification,
//! motion functions (forward/backward word, sexp scanning), and the
//! `string-to-syntax` descriptor parser.

use crate::emacs_core::error::LispCondition;
use std::cell::{Cell, RefCell};
use std::ops::Deref;

use num_enum::{IntoPrimitive, TryFromPrimitive};
use strum::{EnumString, IntoStaticStr};

use super::error::{EvalResult, Flow, signal};
use super::symbol::Obarray;
use super::textprop::CharPropertyResolver;
use super::value::{Value, ValueKind, list_to_vec};
use crate::buffer::{
    Buffer, BufferManager, CharLen, CharPos0, EmacsByteLen, EmacsBytePos, LispCharPos1,
    TextPropertyTable,
};
use crate::heap_types::LispString;

/// The `syntax-table` property symbol, interned once.
///
/// GNU refers to this as the static `Qsyntax_table`. neomacs was rebuilding it
/// with `Value::symbol("syntax-table")` at every use, which hashes a 12-byte
/// name and walks the obarray -- on paths that run per property run, and in one
/// case per character. `intern` shows at 2.31% of an org editing profile, where
/// fontification creates many short property runs and so refills the run cache
/// constantly.
///
/// Caches the `SymId` rather than the `Value`, matching `cached_symbol_id!` in
/// eval.rs: an id is a plain index, so it cannot be invalidated by GC the way a
/// cached pointer could.
#[inline(always)]
fn syntax_table_prop_symbol() -> Value {
    use std::sync::OnceLock;
    static SYMBOL: OnceLock<crate::emacs_core::intern::SymId> = OnceLock::new();
    let id = if let Some(id) = SYMBOL.get() {
        *id
    } else {
        *SYMBOL.get_or_init(|| crate::emacs_core::intern::intern("syntax-table"))
    };
    Value::symbol(id)
}

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

/// GNU syntax switches backed by predeclared C variables.  The closed domain
/// gives each hot lookup one stable symbol identity; Lisp-visible evaluation
/// remains responsible for signaling if some unrelated variable is unbound.
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum SyntaxStateVariable {
    CommentEndCanBeEscaped,
    ParseSexpIgnoreComments,
    ParseSexpLookupProperties,
}

impl SyntaxStateVariable {
    #[inline(always)]
    fn symbol_id(self) -> crate::emacs_core::intern::SymId {
        use std::sync::OnceLock;

        static COMMENT_END_CAN_BE_ESCAPED: OnceLock<crate::emacs_core::intern::SymId> =
            OnceLock::new();
        static PARSE_SEXP_IGNORE_COMMENTS: OnceLock<crate::emacs_core::intern::SymId> =
            OnceLock::new();
        static PARSE_SEXP_LOOKUP_PROPERTIES: OnceLock<crate::emacs_core::intern::SymId> =
            OnceLock::new();
        let name: &'static str = self.into();
        match self {
            Self::CommentEndCanBeEscaped => {
                *COMMENT_END_CAN_BE_ESCAPED.get_or_init(|| crate::emacs_core::intern::intern(name))
            }
            Self::ParseSexpIgnoreComments => {
                *PARSE_SEXP_IGNORE_COMMENTS.get_or_init(|| crate::emacs_core::intern::intern(name))
            }
            Self::ParseSexpLookupProperties => *PARSE_SEXP_LOOKUP_PROPERTIES
                .get_or_init(|| crate::emacs_core::intern::intern(name)),
        }
    }

    #[inline(always)]
    fn enabled(self, ctx: &super::eval::Context) -> bool {
        matches!(
            ctx.find_symbol_value_by_id(self.symbol_id()),
            Ok(super::eval::SymbolValueLookup::Bound(value)) if value.is_truthy()
        )
    }
}

/// GNU's buffer-local `comment-end-can-be-escaped` policy.
///
/// Naming both states avoids threading a Boolean whose meaning reverses at
/// call sites: scanners ask whether a quoted ender terminates, rather than
/// remembering what `true` meant in Lisp.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum CommentEndEscapePolicy {
    /// GNU's default: quoting cannot suppress a comment end marker.
    #[default]
    EnderAlwaysTerminates,
    /// A quote/escape character makes the following end marker ordinary text.
    EscapeQuotesEnder,
}

/// Which GNU entry point initiated a backward comment scan.
///
/// `Fforward_comment` lets `comment-end-can-be-escaped` suppress the ender
/// that triggered the scan.  `scan_lists` does not: once sexp motion has
/// classified an ender, it always asks `back_comment` to match it.  Keeping
/// that caller distinction typed prevents one shared scanner from quietly
/// changing either public operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BackwardCommentEntryPolicy {
    ForwardComment,
    SexpMotion,
}

impl BackwardCommentEntryPolicy {
    fn escaped_ender_is_suppressed(
        self,
        escape_policy: CommentEndEscapePolicy,
        quoted: bool,
    ) -> bool {
        self == Self::ForwardComment && escape_policy.quoted_ender_is_escaped(quoted)
    }

    fn accepts_two_char_ender_with_quoted_first(self, first_is_quoted: bool) -> bool {
        self == Self::SexpMotion || !first_is_quoted
    }
}

impl CommentEndEscapePolicy {
    fn for_context(ctx: &super::eval::Context) -> Self {
        if SyntaxStateVariable::CommentEndCanBeEscaped.enabled(ctx) {
            Self::EscapeQuotesEnder
        } else {
            Self::EnderAlwaysTerminates
        }
    }

    fn quoted_ender_is_escaped(self, quoted: bool) -> bool {
        quoted && self == Self::EscapeQuotesEnder
    }
}

/// Lisp-visible policy captured once before an evaluator-owned sexp scan.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SexpScanPolicy {
    ignore_comments: bool,
    comment_end_escape: CommentEndEscapePolicy,
}

impl SexpScanPolicy {
    fn for_context(ctx: &super::eval::Context) -> Self {
        Self {
            ignore_comments: SyntaxStateVariable::ParseSexpIgnoreComments.enabled(ctx),
            comment_end_escape: CommentEndEscapePolicy::for_context(ctx),
        }
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
    obarray.define_int_variable("syntax-propertize--done", -1);
    obarray.set_symbol_value(
        "find-word-boundary-function-table",
        super::chartable::make_char_table_value(Value::NIL, Value::NIL),
    );
    obarray.set_symbol_value("forward-comment-function", Value::NIL);

    for name in &[
        "parse-sexp-ignore-comments",
        "parse-sexp-lookup-properties",
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), *value],
            )
        })
}

/// Whether a semantic [`SyntaxEntry`] may reuse GNU's canonical bare syntax
/// object or must be materialized as a fresh Lisp cons.
///
/// Lisp can observe this choice with `eq`, so object identity is distinct from
/// the entry's syntax semantics.  GNU normally reuses `Vsyntax_code_object`
/// for bare syntax codes, but deliberately allocates fresh objects for the
/// standard table's string-quote and escape entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LispSyntaxObjectReuse {
    CanonicalBare,
    Fresh,
}

/// Convert a semantic syntax entry into its Lisp representation:
/// `(CODE . MATCHING-CHAR-OR-NIL)`.
///
/// The CODE is computed as: `(class_code) | (flags << 16)`.  Keeping object
/// reuse as a typed input prevents callers that construct identity-sensitive
/// tables from accidentally conflating semantic equality with Lisp identity.
fn materialize_syntax_entry(entry: &SyntaxEntry, object_reuse: LispSyntaxObjectReuse) -> Value {
    let code = entry.class.code() | ((entry.flags.bits() as i64) << 16);
    if object_reuse == LispSyntaxObjectReuse::CanonicalBare
        && entry.matching_char.is_none()
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

/// Convert a `SyntaxEntry` using GNU's ordinary `string-to-syntax` policy:
/// reuse the canonical object for a bare, unflagged syntax code.
pub fn syntax_entry_to_value(entry: &SyntaxEntry) -> Value {
    materialize_syntax_entry(entry, LispSyntaxObjectReuse::CanonicalBare)
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

/// Global syntax-table content-mutation epoch.
///
/// The neomacs analog of GNU `search.c:clear_regexp_cache`, which
/// `Fmodify_syntax_entry` calls to drop every compiled-regexp cache entry
/// keyed by a syntax table ("It's tempting to compare with the
/// syntax-table we've actually changed, but it's not sufficient because
/// char-table inheritance means that modifying one syntax-table can
/// change others at the same time", search.c:160-170).  Instead of
/// clearing, neomacs's regexp caches key table-dependent entries by
/// `(table identity, epoch)`; bumping the epoch on any syntax-table
/// content mutation makes every such entry unreachable, which is the
/// same conservative invalidation.
///
/// Process-global on purpose: caches are thread-local, so a bump from
/// another thread can only over-invalidate, never serve a stale entry.
///
/// ★ DO NOT key a general syntax-value cache on this epoch. ★
///
/// It is bumped from the `modify-syntax-entry` chokepoints ONLY
/// (`SyntaxTable::modify_syntax_entry` and `modify_syntax_entry_in_buffers`).
/// A syntax table is an ordinary char-table, so Lisp can mutate one straight
/// through `aset`, `set-char-table-range`, or `set-char-table-parent` without
/// ever reaching those, and the epoch will NOT move. That is deliberate and
/// GNU-faithful -- GNU calls `clear_regexp_cache` from `Fmodify_syntax_entry`
/// alone, so its regexp cache has exactly the same blind spot -- and it is
/// sound for the one contract above, where a missed invalidation costs a stale
/// COMPILED REGEXP.
///
/// It is NOT sound for a cache of syntax classes or entries, where the same
/// miss serves a WRONG SYNTAX CLASS and silently misparses the buffer. The
/// ASCII memo on `SyntaxPropRange` avoids this by living for a single scan --
/// the table-immutability window the property-run cache already assumes --
/// rather than trusting this epoch across calls. Anything longer-lived needs
/// its own invalidation, or these bump sites need widening first.
static SYNTAX_TABLE_MUTATION_EPOCH: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Current syntax-table mutation epoch (see
/// [`SYNTAX_TABLE_MUTATION_EPOCH`]).
pub(crate) fn syntax_table_mutation_epoch() -> u64 {
    SYNTAX_TABLE_MUTATION_EPOCH.load(std::sync::atomic::Ordering::Relaxed)
}

/// Record a syntax-table content mutation (GNU `clear_regexp_cache`
/// analog).  Called from the `modify-syntax-entry` chokepoints.
pub(crate) fn note_syntax_table_mutation() {
    SYNTAX_TABLE_MUTATION_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

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
/// > handling.
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
            return *self;
        }
        match builtin_copy_syntax_table(vec![self.chartable]) {
            Ok(copy) => Self { chartable: copy },
            Err(_) => *self,
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
        note_syntax_table_mutation();
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
    forward_word_with_options(buf, table, count, SyntaxProperties::Ignore).0
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
    cursor: Option<(usize, EmacsBytePos, EmacsByteLen)>,
    /// Borrow-free storage window: `(logical_start, base_ptr, len)` for the
    /// contiguous physical segment (gap half) last read from. Every
    /// `char_at` used to re-enter the text storage -- a RefCell borrow, a
    /// backend dispatch, and a gap-half recomputation PER CHARACTER; GNU's
    /// scanners read through a raw `BYTE_POS_ADDR` pointer instead
    /// (syntax.c FETCH_CHAR_AS_MULTIBYTE).
    ///
    /// SOUNDNESS: the pointer is valid until the next text mutation. A
    /// `BufferChars` lives inside one syntax scan that holds `&Buffer`
    /// throughout and runs no Lisp; its property/table arms only READ. The
    /// window therefore never outlives the text layout it points into.
    window: Option<(usize, *const u8, usize)>,
}

impl<'a> BufferChars<'a> {
    fn new(buf: &'a Buffer, base_char: CharPos0) -> Self {
        Self {
            buf,
            base_char,
            multibyte: buf.get_multibyte(),
            cursor: None,
            window: None,
        }
    }

    /// Decode the char code at `byte_pos` through the cached storage
    /// window, refreshing the window when the position leaves it.
    #[inline]
    fn code_at_byte(&mut self, byte_pos: EmacsBytePos) -> u32 {
        let pos = byte_pos.get();
        let window = match self.window {
            Some(w @ (start, _, len)) if pos >= start && pos < start + len => w,
            _ => match self.buf.contiguous_window_at(pos) {
                Some(w) => {
                    self.window = Some(w);
                    w
                }
                // Chunked backend (or out of range): per-call accessor.
                None => return self.buf.char_code_at_emacs_byte_pos(byte_pos).unwrap_or(0),
            },
        };
        let (start, base, len) = window;
        // SAFETY: pos is inside [start, start+len) per the check above, and
        // the window invariant (struct doc) guarantees base..base+len is the
        // live physical segment for those logical bytes.
        let slice =
            unsafe { std::slice::from_raw_parts(base.add(pos - start), len - (pos - start)) };
        let code = if !self.multibyte || slice[0] < 0x80 {
            slice[0] as u32
        } else {
            crate::emacs_core::emacs_char::string_char(slice).0
        };
        debug_assert_eq!(
            Some(code),
            self.buf.char_code_at_emacs_byte_pos(byte_pos),
            "window decode diverged from the storage accessor at byte {pos}"
        );
        code
    }

    /// Hot path only: a sequential (or repeated) read whose byte lands in
    /// the cached storage window on an ASCII byte — ~a dozen instructions,
    /// small enough to inline into every scan loop (the full function
    /// measured 686 bytes of code, which nothing inlines, leaving 10 percent
    /// of the editing profile in call overhead). Everything else outlines.
    #[inline(always)]
    fn char_at(&mut self, idx: usize) -> char {
        if let Some((c_idx, c_byte, c_width)) = self.cursor {
            let byte_pos = if idx == c_idx {
                c_byte
            } else if idx == c_idx + 1 {
                c_byte.add_len(c_width)
            } else {
                return self.char_at_outlined(idx);
            };
            if let Some((start, base, len)) = self.window {
                let pos = byte_pos.get();
                if pos >= start && pos < start + len {
                    // SAFETY: pos is inside the window per the check; window
                    // validity is the struct invariant (see `window`).
                    let b = unsafe { *base.add(pos - start) };
                    if b < 0x80 {
                        self.cursor = Some((idx, byte_pos, EmacsByteLen::new(1)));
                        return b as char;
                    }
                }
            }
        }
        self.char_at_outlined(idx)
    }

    #[inline(never)]
    fn char_at_outlined(&mut self, idx: usize) -> char {
        // For a forward step (or re-read of the same char) advance the byte
        // cursor directly; otherwise pay for one (cached) char->byte
        // conversion.  GNU's syntax scanners never convert per char -- they
        // carry the byte position and bump it by the char width.
        let byte_pos = match self.cursor {
            Some((c_idx, c_byte, _)) if idx == c_idx => c_byte,
            Some((c_idx, c_byte, c_width)) if idx == c_idx + 1 => c_byte.add_len(c_width),
            // Backward step (`char-before`-style peeks — the parser does
            // these constantly): back over continuation bytes (at most 4 —
            // the 5-byte internal encoding's worst case) instead of a full
            // char->byte conversion per peek.
            Some((c_idx, c_byte, _)) if idx + 1 == c_idx && c_byte.get() > 0 => {
                let mut pos = c_byte.get() - 1;
                if self.multibyte {
                    let mut steps = 0;
                    while pos > 0
                        && steps < 4
                        && self
                            .buf
                            .emacs_byte_at_pos(EmacsBytePos::new(pos))
                            .is_some_and(|b| (b & 0xC0) == 0x80)
                    {
                        pos -= 1;
                        steps += 1;
                    }
                }
                EmacsBytePos::new(pos)
            }
            _ => buffer_char_to_emacs_byte_pos(self.buf, self.base_char.add_len(CharLen::new(idx))),
        };
        let code = self.code_at_byte(byte_pos);
        // A unibyte buffer stores one byte per char; a multibyte buffer stores
        // the char's internal multibyte length (raw bytes included -- see
        // `emacs_char::char_bytes`).
        //
        // The `code < 0x80` arm is not redundant with `char_bytes`, which
        // returns 1 for it anyway: the compiler lowers that function's
        // comparison chain BRANCHLESSLY (cmov), so every character paid all of
        // its bounds checks. Annotating this function showed the 5-byte-char
        // bound alone at 9.31% of its samples and the raw-byte check at 4.52%,
        // on a scan whose characters are overwhelmingly ASCII. Testing the
        // dominant case first turns that into one predictable compare.
        let width = if !self.multibyte || code < 0x80 {
            1
        } else {
            crate::emacs_core::emacs_char::char_bytes(code)
        };
        self.cursor = Some((idx, byte_pos, EmacsByteLen::new(width)));
        syntax_char_from_code(code)
    }
}

fn forward_word_with_options(
    buf: &Buffer,
    table: &SyntaxTable,
    count: i64,
    props: SyntaxProperties<'_>,
) -> (EmacsBytePos, bool) {
    // Per-scan syntax-table property cache (GNU gl_state); one
    // interval lookup per property RUN instead of per character.
    let prop_cache = SyntaxPropRange::new(props);
    let prop_cache = &prop_cache;

    if count < 0 {
        return backward_word_with_options(buf, table, -count, props);
    }

    let accessible_bytes = buf.accessible_emacs_byte_region();
    let accessible_chars = buf.accessible_char_region();
    let mut chars = BufferChars::new(buf, accessible_chars.start());
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
                    prop_cache
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
                    prop_cache
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
    backward_word_with_options(buf, table, count, SyntaxProperties::Ignore).0
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
/// `honor` rather than a snapshot: the boundary callback below is arbitrary
/// Lisp, so each probe takes its own [`SyntaxProperties::for_scan`] snapshot,
/// just as it already builds its own property-run cache.
fn word_motion_with_table(
    eval: &mut super::eval::Context,
    count: i64,
    honor: bool,
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
        // accessible-region limit, all from the current point.  Keep the
        // syntax-property cache inside this mutation-free probe: the boundary
        // callback below is arbitrary Lisp and may edit the buffer.
        let probe = {
            let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
            let prop_cache = SyntaxPropRange::new(props);
            let prop_cache = &prop_cache;
            let buf = match eval.buffers.get(current_id) {
                Some(b) => b,
                None => return (EmacsBytePos::new(0), false),
            };
            let table = SyntaxTable::for_buffer(buf);
            let acc_chars = buf.accessible_char_region();
            let acc_start = acc_chars.start().get();
            let acc_len = acc_chars.len().get();
            let mut chars = BufferChars::new(buf, acc_chars.start());
            let point_char = buffer_byte_to_char_pos(buf, buf.point_emacs_byte_pos());
            let mut idx = point_char.saturating_sub(acc_start);
            let mut is_word = |i: usize| {
                matches!(
                    effective_syntax_entry_for_abs_char(
                        buf,
                        &table,
                        chars.char_at(i),
                        acc_start + i,
                        prop_cache
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
            // GNU Fforward_word: when scan_words finds no further word, point
            // still moves to the accessible limit (ZV forward, BEGV backward)
            // and the motion reports incomplete. Leaving point in place
            // instead turns the ubiquitous
            // (while (< (point) (point-max)) (forward-word)) idiom into an
            // infinite loop whenever trailing non-word text remains.
            if let Some(buf) = eval.buffers.get(current_id) {
                let region = buf.accessible_emacs_byte_region();
                let limit = if forward {
                    region.range().end()
                } else {
                    region.range().start()
                };
                let _ = eval.buffers.goto_buffer_emacs_byte_pos(current_id, limit);
            }
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
        if callable
            && let Ok(result) =
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

        if !handled {
            // Plain syntax scan of a single word from the current point.
            let (byte, ok) = {
                let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
                let buf = eval.buffers.get(current_id).expect("buffer");
                let table = SyntaxTable::for_buffer(buf);
                forward_word_with_options(buf, &table, if forward { 1 } else { -1 }, props)
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
    honor: bool,
) -> EmacsBytePos {
    let wbtable = eval.visible_variable_value_or_nil("find-word-boundary-function-table");
    if word_boundary_table_active(&wbtable) {
        let current_id = eval.buffers.current_buffer_id();
        let saved =
            current_id.and_then(|id| eval.buffers.get(id).map(|b| b.point_emacs_byte_pos()));
        let (dest, _) = word_motion_with_table(eval, count, honor, wbtable);
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
        let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
        let table = SyntaxTable::for_buffer(buf);
        forward_word_with_options(buf, &table, count, props).0
    }
}

fn backward_word_with_options(
    buf: &Buffer,
    table: &SyntaxTable,
    count: i64,
    props: SyntaxProperties<'_>,
) -> (EmacsBytePos, bool) {
    // Per-scan syntax-table property cache (GNU gl_state); one
    // interval lookup per property RUN instead of per character.
    let prop_cache = SyntaxPropRange::new(props);
    let prop_cache = &prop_cache;

    if count < 0 {
        return forward_word_with_options(buf, table, -count, props);
    }

    let accessible_bytes = buf.accessible_emacs_byte_region();
    let accessible_chars = buf.accessible_char_region();
    let mut chars = BufferChars::new(buf, accessible_chars.start());
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
                    prop_cache
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
                    prop_cache
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
    skip_syntax_forward_with_options(buf, table, syntax_chars, limit, SyntaxProperties::Ignore)
}

fn skip_syntax_forward_with_options(
    buf: &Buffer,
    table: &SyntaxTable,
    syntax_chars: &str,
    limit: Option<usize>,
    props: SyntaxProperties<'_>,
) -> usize {
    // Per-scan syntax-table property cache (GNU gl_state); one
    // interval lookup per property RUN instead of per character.
    let prop_cache = SyntaxPropRange::new(props);
    let prop_cache = &prop_cache;

    let (classes, negate) = parse_skip_syntax_classes(syntax_chars);

    let accessible_bytes = buf.accessible_emacs_byte_region();
    let accessible_chars = buf.accessible_char_region();
    let mut chars = BufferChars::new(buf, accessible_chars.start());
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
            prop_cache,
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
    skip_syntax_backward_with_options(buf, table, syntax_chars, limit, SyntaxProperties::Ignore)
}

fn skip_syntax_backward_with_options(
    buf: &Buffer,
    table: &SyntaxTable,
    syntax_chars: &str,
    limit: Option<usize>,
    props: SyntaxProperties<'_>,
) -> usize {
    // Per-scan syntax-table property cache (GNU gl_state); one
    // interval lookup per property RUN instead of per character.
    let prop_cache = SyntaxPropRange::new(props);
    let prop_cache = &prop_cache;

    let (classes, negate) = parse_skip_syntax_classes(syntax_chars);

    let accessible_bytes = buf.accessible_emacs_byte_region();
    let accessible_chars = buf.accessible_char_region();
    let mut chars = BufferChars::new(buf, accessible_chars.start());
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
            prop_cache,
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
    match scan_sexps_with_options(
        buf,
        table,
        from,
        count,
        SyntaxProperties::Ignore,
        SexpScanPolicy::default(),
    )
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
    props: SyntaxProperties<'_>,
    policy: SexpScanPolicy,
) -> Result<Option<usize>, ScanListError> {
    // Per-scan syntax-table property cache (GNU gl_state); one
    // interval lookup per property RUN instead of per character.
    let prop_cache = SyntaxPropRange::new(props);
    let prop_cache = &prop_cache;

    if count == 0 {
        return Ok(Some(from));
    }

    let mut chars = BufferChars::new(buf, CharPos0::ZERO);
    let accessible_chars = buf.accessible_char_region();
    let start_bound = accessible_chars.start().get();
    let stop_bound = accessible_chars.end().get();

    // Convert byte position to char index.
    let mut idx =
        buffer_byte_to_char_pos(buf, EmacsBytePos::new(from)).clamp(start_bound, stop_bound);

    if count > 0 {
        for _ in 0..count {
            let skipped = skip_sexp_ignored_forward(
                buf, &mut chars, idx, stop_bound, table, policy, prop_cache,
            );
            idx = skipped.position();
            if matches!(skipped, IgnoredSkip::UnterminatedComment(_)) {
                continue;
            }
            if idx >= stop_bound {
                return Ok(None);
            }
            idx = scan_sexp_forward(buf, &mut chars, stop_bound, idx, table, policy, prop_cache)?;
        }
    } else {
        for _ in 0..(-count) {
            idx = skip_sexp_ignored_backward(
                buf,
                &mut chars,
                idx,
                start_bound,
                table,
                policy,
                prop_cache,
            );
            if idx <= start_bound {
                return Ok(None);
            }
            idx = scan_sexp_backward(buf, &mut chars, idx, start_bound, table, policy, prop_cache)?;
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
    props: SyntaxProperties<'_>,
    class: SyntaxClass,
    flags: SyntaxFlags,
    escape_policy: CommentEndEscapePolicy,
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
    let complete = forward_comment_forward(
        &mut scanner,
        1,
        &SyntaxPropByteRun::new(props),
        escape_policy,
    );
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
    props: SyntaxProperties<'_>,
    class: SyntaxClass,
    flags: SyntaxFlags,
    escape_policy: CommentEndEscapePolicy,
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
    if forward_comment_backward(
        &mut scanner,
        1,
        &SyntaxPropByteRun::new(props),
        escape_policy,
        BackwardCommentEntryPolicy::SexpMotion,
    ) {
        let next = buffer_byte_to_char_pos(buf, scanner.point_emacs_byte_pos());
        (next < idx).then_some(next)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)] // scanner bounds and syntax policies remain explicit
fn skip_sexp_ignored_forward(
    buf: &Buffer,
    chars: &mut BufferChars,
    mut idx: usize,
    stop: usize,
    table: &SyntaxTable,
    policy: SexpScanPolicy,
    prop_cache: &SyntaxPropRange<'_>,
) -> IgnoredSkip {
    let mut skipped_unterminated_comment = false;
    while idx < stop {
        let c = chars.char_at(idx);
        let entry = effective_syntax_entry_for_abs_char(buf, table, c, idx, prop_cache);
        let class = entry.class;
        if policy.ignore_comments
            && let Some(skip) = maybe_skip_comment_forward(
                buf,
                idx,
                prop_cache.props(),
                class,
                entry.flags,
                policy.comment_end_escape,
            )
        {
            skipped_unterminated_comment |= matches!(skip, CommentSkip::Unterminated(_));
            idx = skip.next();
            continue;
        }
        if is_sexp_ignored_syntax(class, policy.ignore_comments) {
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

#[allow(clippy::too_many_arguments)] // scanner bounds and syntax policies remain explicit
fn skip_sexp_ignored_backward(
    buf: &Buffer,
    chars: &mut BufferChars,
    mut idx: usize,
    start: usize,
    table: &SyntaxTable,
    policy: SexpScanPolicy,
    prop_cache: &SyntaxPropRange<'_>,
) -> usize {
    while idx > start {
        let prev = idx - 1;
        let c = chars.char_at(prev);
        let entry = effective_syntax_entry_for_abs_char(buf, table, c, prev, prop_cache);
        let class = entry.class;
        if policy.ignore_comments
            && let Some(next) = maybe_skip_comment_backward(
                buf,
                idx,
                prop_cache.props(),
                class,
                entry.flags,
                policy.comment_end_escape,
            )
        {
            idx = next;
            continue;
        }
        if is_sexp_ignored_syntax(class, policy.ignore_comments) {
            idx -= 1;
            continue;
        }
        break;
    }
    idx
}

#[allow(clippy::too_many_arguments)] // scanner state mirrors GNU syntax traversal
fn skip_string_forward(
    buf: &Buffer,
    chars: &mut BufferChars,
    mut idx: usize,
    stop: usize,
    table: &SyntaxTable,
    delimiter: char,
    delimiter_class: SyntaxClass,
    prop_cache: &SyntaxPropRange<'_>,
) -> Result<usize, String> {
    while idx < stop {
        let c = chars.char_at(idx);
        let class = effective_syntax_entry_for_abs_char(buf, table, c, idx, prop_cache).class;
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

#[allow(clippy::too_many_arguments)] // scanner state mirrors GNU syntax traversal
fn skip_string_backward(
    buf: &Buffer,
    chars: &mut BufferChars,
    mut idx: usize,
    stop: usize,
    table: &SyntaxTable,
    delimiter: char,
    delimiter_class: SyntaxClass,
    prop_cache: &SyntaxPropRange<'_>,
) -> Result<usize, String> {
    while idx > stop {
        idx -= 1;
        let c = chars.char_at(idx);
        let class = effective_syntax_entry_for_abs_char(buf, table, c, idx, prop_cache).class;
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
    props: SyntaxProperties<'_>,
    policy: SexpScanPolicy,
) -> Result<Option<usize>, ScanListError> {
    // Per-scan syntax-table property cache (GNU gl_state); one
    // interval lookup per property RUN instead of per character.
    let prop_cache = SyntaxPropRange::new(props);
    let prop_cache = &prop_cache;

    let mut chars = BufferChars::new(buf, CharPos0::ZERO);
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
                let entry = effective_syntax_entry_for_abs_char(buf, table, ch, idx, prop_cache);
                let class = entry.class;
                if depth == min_depth {
                    last_good = idx;
                }
                if policy.ignore_comments
                    && let Some(skip) = maybe_skip_comment_forward(
                        buf,
                        idx,
                        prop_cache.props(),
                        class,
                        entry.flags,
                        policy.comment_end_escape,
                    )
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
                            buf, &mut chars, idx, stop, table, ch, class, prop_cache,
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
                let entry = effective_syntax_entry_for_abs_char(buf, table, ch, idx, prop_cache);
                let class = entry.class;
                if depth == min_depth {
                    last_good = idx;
                }
                if policy.ignore_comments
                    && let Some(next) = maybe_skip_comment_backward(
                        buf,
                        idx + 1,
                        props,
                        class,
                        entry.flags,
                        policy.comment_end_escape,
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
                            buf, &mut chars, idx, start, table, ch, class, prop_cache,
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

/// Returns true if the character at char index `idx` is quoted, i.e. it is
/// preceded by an odd number of escape/char-quote characters.  Mirrors GNU
/// `char_quoted` in syntax.c.
fn char_quoted_at(
    buf: &Buffer,
    chars: &mut BufferChars,
    idx: usize,
    start_bound: usize,
    table: &SyntaxTable,
    prop_cache: &SyntaxPropRange<'_>,
) -> bool {
    let mut pos = idx;
    let mut quoted = false;
    while pos > start_bound {
        pos -= 1;
        let c = chars.char_at(pos);
        let class = effective_syntax_entry_for_abs_char(buf, table, c, pos, prop_cache).class;
        if !matches!(class, SyntaxClass::Escape | SyntaxClass::CharQuote) {
            break;
        }
        quoted = !quoted;
    }
    quoted
}

/// Scan one sexp forward from char index `start`.
#[allow(clippy::too_many_arguments)] // scanner state mirrors GNU syntax traversal
fn scan_sexp_forward(
    buf: &Buffer,
    chars: &mut BufferChars,
    len: usize,
    start: usize,
    table: &SyntaxTable,
    policy: SexpScanPolicy,
    prop_cache: &SyntaxPropRange<'_>,
) -> Result<usize, ScanListError> {
    let skipped = skip_sexp_ignored_forward(buf, chars, start, len, table, policy, prop_cache);
    let mut idx = skipped.position();

    if matches!(skipped, IgnoredSkip::UnterminatedComment(_)) {
        return Ok(idx);
    }

    if idx >= len {
        return Err(ScanListError::unbalanced(start, idx));
    }

    let ch = chars.char_at(idx);
    let syn_entry = effective_syntax_entry_for_abs_char(buf, table, ch, idx, prop_cache);
    let syn = syn_entry.class;

    match syn {
        SyntaxClass::Open => {
            // Find matching close, respecting nesting.
            let mut depth = 1i32;
            idx += 1;
            while idx < len && depth > 0 {
                let c = chars.char_at(idx);
                let entry = effective_syntax_entry_for_abs_char(buf, table, c, idx, prop_cache);
                let s = entry.class;
                if policy.ignore_comments
                    && let Some(skip) = maybe_skip_comment_forward(
                        buf,
                        idx,
                        prop_cache.props(),
                        s,
                        entry.flags,
                        policy.comment_end_escape,
                    )
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
                                prop_cache,
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
                let s = effective_syntax_entry_for_abs_char(buf, table, c, idx, prop_cache).class;
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
        SyntaxClass::Word | SyntaxClass::Symbol | SyntaxClass::Escape | SyntaxClass::CharQuote => {
            // Scan over a symbol/word sexp.  An escape/char-quote at the start
            // (e.g. `\(`, or `\` joining the next char into the word) consumes
            // the following character and continues into the symbol body, just
            // like GNU's scan_lists Sescape/Scharquote fallthrough.
            if matches!(syn, SyntaxClass::Escape | SyntaxClass::CharQuote) {
                // The escape itself is at `idx`; advance past it.
                idx += 1;
                if idx >= len {
                    // Trailing escape with no char to quote: unbalanced.
                    return Err(ScanListError::unbalanced(start, idx));
                }
                // Consume the quoted character.
                idx += 1;
            }
            // Continue absorbing the rest of the word/symbol, honoring escapes.
            while idx < len {
                let c = chars.char_at(idx);
                let s = effective_syntax_entry_for_abs_char(buf, table, c, idx, prop_cache).class;
                match s {
                    SyntaxClass::Escape | SyntaxClass::CharQuote => {
                        // Skip the escape char, then the quoted char below.
                        idx += 1;
                        if idx >= len {
                            return Err(ScanListError::unbalanced(start, idx));
                        }
                    }
                    SyntaxClass::Word | SyntaxClass::Symbol | SyntaxClass::Quote => {}
                    _ => break,
                }
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
#[allow(clippy::too_many_arguments)] // scanner state mirrors GNU syntax traversal
fn scan_sexp_backward(
    buf: &Buffer,
    chars: &mut BufferChars,
    start: usize,
    start_bound: usize,
    table: &SyntaxTable,
    policy: SexpScanPolicy,
    prop_cache: &SyntaxPropRange<'_>,
) -> Result<usize, ScanListError> {
    let mut idx =
        skip_sexp_ignored_backward(buf, chars, start, start_bound, table, policy, prop_cache);

    if idx == start_bound {
        return Err(ScanListError::unbalanced(idx, start));
    }

    idx -= 1; // move to the character we're examining
    let ch = chars.char_at(idx);
    let syn_entry = effective_syntax_entry_for_abs_char(buf, table, ch, idx, prop_cache);
    let mut syn = syn_entry.class;

    // Quoting turns anything except a comment-ender into a word character.
    // Mirrors GNU scan_lists: if the char we landed on is quoted (preceded by
    // an escape), step back past the escape and treat the pair as a word.
    if syn != SyntaxClass::EndComment
        && char_quoted_at(buf, chars, idx, start_bound, table, prop_cache)
    {
        idx -= 1;
        syn = SyntaxClass::Word;
    }

    match syn {
        SyntaxClass::Close => {
            // Find matching open, respecting nesting.
            let mut depth = 1i32;
            while idx > start_bound && depth > 0 {
                idx -= 1;
                let c = chars.char_at(idx);
                let entry = effective_syntax_entry_for_abs_char(buf, table, c, idx, prop_cache);
                let s = entry.class;
                if policy.ignore_comments
                    && let Some(next) = maybe_skip_comment_backward(
                        buf,
                        idx + 1,
                        prop_cache.props(),
                        s,
                        entry.flags,
                        policy.comment_end_escape,
                    )
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
                                    prop_cache,
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
                let s = effective_syntax_entry_for_abs_char(buf, table, c, idx, prop_cache).class;
                if s == delim_class && (syn == SyntaxClass::StringFence || c == ch) {
                    break;
                }
                idx -= 1;
            }
            let c = chars.char_at(idx);
            let s = effective_syntax_entry_for_abs_char(buf, table, c, idx, prop_cache).class;
            if !(s == delim_class && (syn == SyntaxClass::StringFence || c == ch)) {
                return Err(ScanListError::unbalanced(idx, start));
            }
            Ok(idx)
        }
        SyntaxClass::Word | SyntaxClass::Symbol | SyntaxClass::Escape | SyntaxClass::CharQuote => {
            // Scan backward over a word/symbol sexp, honoring escapes/char-quotes
            // that join the following char into the symbol.  Mirrors GNU
            // scan_lists' backward Sword/Ssymbol/Sescape/Scharquote loop.
            while idx > start_bound {
                let prev = idx - 1;
                let c1 = chars.char_at(prev);
                let c1_class =
                    effective_syntax_entry_for_abs_char(buf, table, c1, prev, prop_cache).class;
                // Don't allow a comment-end to be quoted.
                if c1_class == SyntaxClass::EndComment {
                    break;
                }
                let quoted = char_quoted_at(buf, chars, prev, start_bound, table, prop_cache);
                if quoted {
                    // The previous char is escaped: step back past it now, so the
                    // following `idx -= 1` lands on the escape character.
                    idx -= 1;
                } else if !matches!(
                    c1_class,
                    SyntaxClass::Word | SyntaxClass::Symbol | SyntaxClass::Quote
                ) {
                    break;
                }
                idx -= 1;
            }
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
            LispCondition::WrongNumberOfArguments,
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
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_make_syntax_table(args: Vec<Value>) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
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
            LispCondition::WrongNumberOfArguments,
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
                LispCondition::WrongTypeArgument,
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
        let set_with_reuse =
            |ch: char, e: SyntaxEntry, object_reuse: LispSyntaxObjectReuse| -> Result<(), Flow> {
                super::chartable::builtin_set_char_table_range(
                    vec![
                        table,
                        Value::fixnum(ch as i64),
                        materialize_syntax_entry(&e, object_reuse),
                    ],
                    None,
                )
                .map(|_| ())
            };
        use LispSyntaxObjectReuse::{CanonicalBare, Fresh};
        let set = |ch, entry| set_with_reuse(ch, entry, CanonicalBare);
        let set_fresh = |ch, entry| set_with_reuse(ch, entry, Fresh);
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
        // GNU `init_syntax_once` bypasses `Vsyntax_code_object` for these
        // standard-table entries even though their bare forms are otherwise
        // canonicalizable.  Preserve that observable object ownership.
        set_fresh('"', SyntaxEntry::simple(SyntaxClass::StringDelim))?;
        set_fresh('\\', SyntaxEntry::simple(SyntaxClass::Escape))?;
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
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = buffers
        .get_mut(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    // Mirrors GNU `Fsyntax_table` (`syntax.c:987-993`):
    //     return BVAR (current_buffer, syntax_table);
    let value = buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()];
    if !value.is_nil() && super::chartable::char_table_has_subtype_named(&value, "syntax-table") {
        return Ok(value);
    }
    // Slot invalid: only now is the standard-table fallback needed (its
    // eager computation sat on every per-scan table fetch).
    let fallback = ensure_standard_syntax_table_object()?;

    // Slot is unset (fresh buffer or never assigned). Seed it
    // from the standard syntax table — matches GNU's
    // `reset_buffer` (`buffer.c:1149-1157`) which copies the
    // standard tables into a fresh buffer.
    buf.slots[BUFFER_SLOT_SYNTAX_TABLE.index()] = fallback;
    Ok(fallback)
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
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn syntax_entry_at_char(table: &Value, c: char) -> Option<SyntaxEntry> {
    syntax_entry_at_char_code(table, c as u32)
}

thread_local! {
    /// Per-syntax-table flat ASCII entry cache, keyed on (table identity,
    /// global char-table write tick). GNU's SYNTAX(c) is 2-3 loads because
    /// the char-table's ascii subtable IS the flat array; this port's parse
    /// previously rebuilt a per-CALL memo, so font-lock's thousands of short
    /// `parse-partial-sexp` deltas re-derived every ASCII entry through the
    /// layered ct_lookup chain (~917K lookups / full fontify). The tick makes
    /// any char-table write anywhere invalidate the cache (see
    /// `char_table_write_tick`).
    static SYNTAX_FLAT_ASCII_CACHE: std::cell::Cell<Option<(usize, u64)>> =
        const { std::cell::Cell::new(None) };
    static SYNTAX_FLAT_ASCII_ENTRIES: std::cell::RefCell<[SyntaxEntry; 128]> =
        std::cell::RefCell::new([SyntaxEntry::simple(SyntaxClass::Whitespace); 128]);
}

fn flat_ascii_entries_for_table(table: &SyntaxTable) -> [SyntaxEntry; 128] {
    let tick = crate::emacs_core::chartable::char_table_write_tick();
    let key = (table.chartable.bits(), tick);
    if SYNTAX_FLAT_ASCII_CACHE.with(|c| c.get()) == Some(key) {
        return SYNTAX_FLAT_ASCII_ENTRIES.with(|e| *e.borrow());
    }
    let entries: [SyntaxEntry; 128] =
        std::array::from_fn(|cp| syntax_entry_from_table(table, cp as u8 as char));
    SYNTAX_FLAT_ASCII_ENTRIES.with(|e| *e.borrow_mut() = entries);
    SYNTAX_FLAT_ASCII_CACHE.with(|c| c.set(Some(key)));
    entries
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
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
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

// Count of syntax entries actually DECODED from the char-table, so a test can
// assert the ASCII memo holds: a scan over N ASCII characters must decode a
// bounded number of entries, not one per character stepped.
//
// Counts every decode, not just memo misses -- counting misses alone would
// read zero both when the memo works perfectly and when it never materializes.
// Mirrors the position-conversion scan counter in `emacs_char`.
#[cfg(test)]
thread_local! {
    static SYNTAX_TABLE_DECODES: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
fn record_syntax_table_decode() {
    SYNTAX_TABLE_DECODES.with(|n| n.set(n.get() + 1));
}

#[cfg(not(test))]
fn record_syntax_table_decode() {}

#[cfg(test)]
pub(crate) fn reset_syntax_table_decodes_for_test() {
    SYNTAX_TABLE_DECODES.with(|n| n.set(0));
}

#[cfg(test)]
pub(crate) fn syntax_table_decodes_for_test() -> usize {
    SYNTAX_TABLE_DECODES.with(Cell::get)
}

/// Where a scan gets each character's `syntax-table` property, or that it gets
/// none at all -- GNU `SETUP_SYNTAX_TABLE`'s `parse_sexp_lookup_properties`
/// test, carrying the resolver instead of a bare flag.
///
/// A scanner that honours properties can only do so through the resolver the
/// property builtins use, so "honours properties but reads them raw" -- the
/// state in which `forward-sexp` and `syntax-after` disagreed about a
/// `category`-supplied syntax -- is not a value this type can hold.
/// [`Self::for_scan`] is the one place the Lisp flag becomes a resolver.
#[derive(Clone, Copy)]
pub(crate) enum SyntaxProperties<'a> {
    /// `parse-sexp-lookup-properties` is nil: buffer table syntax only.
    Ignore,
    /// Resolve `syntax-table` per character exactly as `get-char-property`
    /// does, through GNU `textget`'s category/alias/default fallbacks.
    Honor(CharPropertyResolver<'a>),
}

impl<'a> SyntaxProperties<'a> {
    /// Snapshot the property source for one scan. `honor` is the value of
    /// `parse-sexp-lookup-properties`, read (and any `syntax-propertize` run)
    /// by the caller before this point, since Lisp must not run afterwards.
    pub(crate) fn for_scan(honor: bool, obarray: &'a Obarray, buffers: &BufferManager) -> Self {
        if honor {
            Self::Honor(CharPropertyResolver::snapshot(
                obarray,
                buffers,
                syntax_table_prop_symbol(),
            ))
        } else {
            Self::Ignore
        }
    }

    /// The resolved `syntax-table` property at a buffer byte, with no run
    /// cache. Used by the scanners that address text by byte (regexp matching,
    /// `forward-comment`, `backward-prefix-chars`); the char-addressed scanners
    /// go through [`SyntaxPropRange`], which caches the same resolution.
    fn syntax_table_prop_at_emacs_byte(
        self,
        buf: &Buffer,
        byte_pos: EmacsBytePos,
    ) -> Option<Value> {
        let Self::Honor(resolver) = self else {
            return None;
        };
        resolver.resolve_interval_plist(buf.interval_plist_at_emacs_byte_pos(byte_pos)?)
    }
}

/// The `[start, end)` run and its resolved value, in whatever coordinate the
/// scan addresses text by.
///
/// Held as separate `Cell`s rather than one `RefCell<Option<..>>`: this is read
/// PER CHARACTER, and a `RefCell` charged a borrow-flag load, increment and
/// decrement on top of the comparison. `Value` and `Option<Value>` are `Copy`,
/// so plain `Cell`s make an in-run read two integer compares and one load --
/// GNU's inlined `charpos >= gl_state.e_property` test (`syntax.h`).
///
/// `start == end == 0` is the natural empty state (no position satisfies
/// `start <= pos && pos < end`), so no reserved sentinel is needed.
#[derive(Default)]
struct PropRunCells {
    start: Cell<usize>,
    end: Cell<usize>,
    value: Cell<Option<Value>>,
}

impl PropRunCells {
    /// A run covering every position, for a scan that ignores properties: its
    /// per-character path then answers from the same range check a honouring
    /// scan uses, with no test of the property source at all.
    fn covering_everything() -> Self {
        Self {
            start: Cell::new(0),
            end: Cell::new(usize::MAX),
            value: Cell::new(None),
        }
    }

    #[inline]
    fn get(&self, pos: usize) -> Option<Option<Value>> {
        (pos >= self.start.get() && pos < self.end.get()).then(|| self.value.get())
    }

    fn set(&self, start: usize, end: usize, value: Option<Value>) {
        self.start.set(start);
        self.end.set(end);
        self.value.set(value);
    }
}

/// Per-scan `syntax-table` property cache for the scanners that address text by
/// EMACS BYTE position: the regexp matcher, `forward-comment`, and
/// `backward-prefix-chars`.
///
/// GNU's `gl_state` serves these scanners too -- `RE_SETUP_SYNTAX_TABLE_FOR_OBJECT`
/// (src/syntax.c:277) arms the same machinery the sexp scanner uses -- so until
/// this existed they were the only scanners doing a fresh interval lookup, and a
/// byte->char conversion, for every character examined. Caching the run in BYTE
/// coordinates is what removes the conversion: the byte<->char mapping is
/// monotonic, so an interval's char run `[s, e)` is exactly the byte run
/// `[byte(s), byte(e))`, and a hit becomes the same two integer compares the
/// char-addressed cache does.
///
/// Same single-scan lifetime, and so the same invariant: no Lisp runs during a
/// scan, so neither the intervals nor the resolver's snapshot can move under it.
pub(crate) struct SyntaxPropByteRun<'a> {
    run: PropRunCells,
    /// Lazily-filled ASCII syntax-entry memo, same contract as
    /// [`SyntaxPropRange::ascii`]: the byte-addressed scanners (regexp
    /// syntax classes, `forward-comment`) previously paid the full
    /// chartable walk for EVERY character examined.
    ascii: [Cell<Option<SyntaxEntry>>; 128],
    ascii_table: Cell<usize>,
    props: SyntaxProperties<'a>,
}

impl<'a> SyntaxPropByteRun<'a> {
    pub(crate) fn new(props: SyntaxProperties<'a>) -> Self {
        let run = match props {
            SyntaxProperties::Ignore => PropRunCells::covering_everything(),
            SyntaxProperties::Honor(_) => PropRunCells::default(),
        };
        Self {
            run,
            ascii: std::array::from_fn(|_| Cell::new(None)),
            ascii_table: Cell::new(0),
            props,
        }
    }

    /// The property source this cache reads from, for scanners that have to
    /// hand it on to a char-addressed parse (see `back_comment_reparse`).
    fn props(&self) -> SyntaxProperties<'a> {
        self.props
    }

    /// See [`SyntaxPropRange::ascii_entry`] — identical memo, byte-side.
    #[inline]
    fn ascii_entry(&self, table: &SyntaxTable, ch: char) -> Option<SyntaxEntry> {
        let cp = ch as u32;
        if cp >= 128 {
            return None;
        }
        let id = table.chartable.bits();
        if self.ascii_table.get() != id {
            for slot in &self.ascii {
                slot.set(None);
            }
            self.ascii_table.set(id);
        }
        let slot = &self.ascii[cp as usize];
        if let Some(entry) = slot.get() {
            return Some(entry);
        }
        let entry = syntax_entry_from_table(table, ch);
        slot.set(Some(entry));
        Some(entry)
    }

    /// The resolved `syntax-table` property at an Emacs byte position, served
    /// from the cached run when possible.
    #[inline]
    fn syntax_table_prop_at_emacs_byte(
        &self,
        buf: &Buffer,
        byte_pos: EmacsBytePos,
    ) -> Option<Value> {
        if let Some(value) = self.run.get(byte_pos.get()) {
            return value;
        }
        let SyntaxProperties::Honor(resolver) = self.props else {
            // Unreachable: an ignoring cache covers every position.
            return None;
        };
        self.refill_run(buf, byte_pos, resolver)
    }

    /// Locate the interval containing `byte_pos`, resolve its property once,
    /// and cache both with the run converted to byte coordinates.
    ///
    /// Outlined so the per-character check above stays inlinable into the
    /// scan loops.
    #[inline(never)]
    fn refill_run(
        &self,
        buf: &Buffer,
        byte_pos: EmacsBytePos,
        resolver: CharPropertyResolver<'_>,
    ) -> Option<Value> {
        // Byte-run memo: a hit answers with zero conversions and no interval
        // lookup. Only sound when the resolver's coalescing preconditions
        // hold (no aliases / no default fallback) — the same guard the
        // char-side coalescing uses.
        let coalesce = resolver.supports_presence_coalescing();
        if coalesce && let Some((start, end, value)) = buf.syntax_byte_run_memo_lookup(byte_pos) {
            self.run.set(start as usize, end as usize, value);
            return value;
        }
        let char_pos = buf.emacs_byte_pos_to_char_pos_clamped(byte_pos);
        let (plist, _start, _end) = buf.interval_plist_run_at_char_pos(char_pos);
        let value = plist.and_then(|plist| resolver.resolve_interval_plist(plist));
        // See `coalesced_syntax_run_end`: extend over face-only splits.
        let end = coalesced_syntax_run_end(buf, char_pos, &resolver);
        let end_byte = buffer_char_to_emacs_byte_pos(buf, end).get();
        self.run.set(byte_pos.get(), end_byte, value);
        if coalesce {
            buf.syntax_byte_run_memo_store(byte_pos.get() as u64, end_byte as u64, value);
        }
        value
    }
}

/// The string a regexp match reads `syntax-table` properties from: GNU's
/// `gl_state.object` when that object is a string.
///
/// The matcher addresses a string by byte offset from its first byte, the
/// intervals address it by character, so the string itself is needed for the
/// conversion GNU does with `string_byte_to_char`
/// (`RE_SYNTAX_TABLE_BYTE_TO_CHAR`, src/syntax.h).
#[derive(Clone, Copy)]
struct StringPropSource<'a> {
    resolver: CharPropertyResolver<'a>,
    string: &'a LispString,
    intervals: &'a TextPropertyTable,
}

/// Per-match `syntax-table` property cache for a regexp over a STRING object.
///
/// GNU arms the same `gl_state` for a string as for a buffer
/// (`RE_SETUP_SYNTAX_TABLE_FOR_OBJECT`, src/syntax.c:277), so `string-match`
/// over a propertized string sees each character's own syntax. This is the
/// string half of [`SyntaxPropByteRun`]: the same [`PropRunCells`] run check
/// per character, and the same [`SyntaxProperties`] vocabulary at the seam, over
/// the string's own intervals instead of a buffer's.
///
/// A scan that ignores properties -- and a string carrying none, which is the
/// overwhelmingly common case and the reason the run is left `covering_everything`
/// there -- keeps no source at all, so "honours properties but has nothing to
/// read them from" is not a state this type can hold.
pub(crate) struct StringSyntaxPropByteRun<'a> {
    run: PropRunCells,
    source: Option<StringPropSource<'a>>,
}

impl<'a> StringSyntaxPropByteRun<'a> {
    /// `intervals` is the string's own interval table, absent when the string
    /// carries no properties at all. GNU's `update_syntax_table` returns early
    /// when `interval_of` finds no interval, giving such a position no property
    /// -- not even the `default-text-properties` fallback -- so dropping the
    /// resolver here is the same answer, reached without any per-character work.
    pub(crate) fn new(
        props: SyntaxProperties<'a>,
        string: &'a LispString,
        intervals: Option<&'a TextPropertyTable>,
    ) -> Self {
        let source = match (props, intervals) {
            (SyntaxProperties::Honor(resolver), Some(intervals)) => Some(StringPropSource {
                resolver,
                string,
                intervals,
            }),
            _ => None,
        };
        let run = if source.is_some() {
            PropRunCells::default()
        } else {
            PropRunCells::covering_everything()
        };
        Self { run, source }
    }

    /// The resolved `syntax-table` property at a byte offset into the string,
    /// served from the cached run when possible.
    #[inline]
    fn syntax_table_prop_at_string_byte(&self, byte_pos: usize) -> Option<Value> {
        if let Some(value) = self.run.get(byte_pos) {
            return value;
        }
        let Some(source) = self.source else {
            // Unreachable: a cache with no source covers every position.
            return None;
        };
        self.refill_run(source, byte_pos)
    }

    /// Locate the interval containing `byte_pos`, resolve its property once, and
    /// cache both with the run converted to byte coordinates.
    ///
    /// Outlined so the per-character check above stays inlinable into the match
    /// loop.
    #[inline(never)]
    fn refill_run(&self, source: StringPropSource<'a>, byte_pos: usize) -> Option<Value> {
        let char_pos = source.string.byte_to_char_pos(byte_pos);
        let (plist, start, end) = source
            .intervals
            .interval_plist_run_at_char_pos(CharPos0::new(char_pos), source.string.schars());
        let value = plist.and_then(|plist| source.resolver.resolve_interval_plist(plist));
        self.run.set(
            source.string.char_to_byte_pos(start.get()),
            source.string.char_to_byte_pos(end.get()),
            value,
        );
        value
    }
}

/// Syntax class seen by GNU's regexp `SYNTAX` macro at a byte offset into a
/// searched STRING, the string counterpart of
/// [`regexp_syntax_class_at_emacs_byte`].
///
/// `table` is the CURRENT BUFFER's syntax table: GNU's
/// `RE_SETUP_SYNTAX_TABLE_FOR_OBJECT` calls `SETUP_BUFFER_SYNTAX_TABLE` for a
/// string object too, so only the positional property comes from the string.
pub(crate) fn regexp_syntax_class_at_string_byte(
    table: &SyntaxTable,
    ch: char,
    byte_pos: usize,
    prop_cache: &StringSyntaxPropByteRun<'_>,
) -> SyntaxClass {
    if let Some(prop) = prop_cache.syntax_table_prop_at_string_byte(byte_pos)
        && let Some(entry) = syntax_entry_from_syntax_property(prop, ch)
    {
        return entry.class;
    }
    syntax_entry_from_table(table, ch).class
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
struct SyntaxPropRange<'a> {
    /// Run over which the `syntax-table` property is known constant, held as
    /// separate `Cell`s rather than one `RefCell<Option<..>>`.
    ///
    /// This is consulted PER CHARACTER whenever `parse-sexp-lookup-properties`
    /// is on, which elisp-mode sets, so it sits under
    /// `effective_syntax_entry_for_abs_char` -- 11.11% of self time on a
    /// per-keystroke profile. GNU's equivalent is two compares on globals,
    /// inlined (`syntax.h`):
    ///
    ///     if (parse_sexp_lookup_properties && charpos >= gl_state.e_property)
    ///       update_syntax_table_forward (charpos, gl_state.object);
    ///
    /// A `RefCell` cannot match that: every hit paid a borrow-flag load,
    /// increment and decrement on top of the comparison. Plain `Cell`s make the
    /// in-run case two integer compares plus one `Copy` load, with no borrow
    /// bookkeeping -- `Value` and `Option<Value>` are both `Copy`.
    ///
    /// `run_start == run_end == 0` is the natural empty state (no `pos`
    /// satisfies `start <= pos && pos < end`), so no reserved sentinel value is
    /// needed. Mirrors GNU's `gl_state.b_property` / `e_property`.
    run: PropRunCells,
    /// Lazily-filled memo of the 128 ASCII syntax entries for the table this
    /// scan runs under (GNU `SYNTAX_ENTRY` on the char-table's `ascii` slot,
    /// minus the per-char cons decode).
    ///
    /// Filled ON MISS rather than precomputed: font-lock drives
    /// `parse-partial-sexp`/`syntax-ppss` with many SHORT ranges, and eagerly
    /// building all 128 entries charged 128 `ct_lookup`s to a scan that might
    /// step over three characters. Per-char fill is strictly less work than the
    /// uncached path for every scan length -- a scan touching k distinct ASCII
    /// chars pays k decodes instead of one per character stepped.
    ///
    /// Lifetime is a single scan, which is exactly the table-immutability
    /// invariant the property-run cache above already relies on, so no
    /// mutation-epoch check is needed (and none would be sound: `aset` on a
    /// syntax char-table bypasses `note_syntax_table_mutation`).
    ///
    /// Memo of the ASCII syntax entries, filled ON MISS.
    ///
    /// Lazy rather than precomputed: font-lock drives `parse-partial-sexp` /
    /// `syntax-ppss` over many SHORT ranges, and eagerly building all 128
    /// entries charged 128 `ct_lookup`s to a scan that might step over three
    /// characters. Per-char fill is strictly less work at every scan length --
    /// a scan touching k distinct ASCII chars pays k decodes, not one per
    /// character stepped, and not 128 up front.
    ///
    /// Available to EVERY scanner, which is what the measurements favour even
    /// though a font-lock profile puts `parse_state_from_range` at 6.01% of
    /// self time and each other scanner near 0.1%. Three narrower variants
    /// were built and measured against this one on the same corpus:
    ///
    /// | variant                                   | elisp edit |
    /// |-------------------------------------------|-----------:|
    /// | this one                                  |   -5.95%   |
    /// | thread-local generation-stamped scratch   |   -4.96%   |
    /// | boxed, after a 64-lookup threshold        |   -3.82%   |
    /// | boxed, `parse_state_from_range` only      |   -3.08%   |
    ///
    /// The narrower ones gave up editing latency for nothing: all four showed
    /// the same ~+0.35% on byte-compile despite differing structurally, and a
    /// byte-compile profile contains none of this path (it is interpreter
    /// bound, 27.5% VM run_loop), so that delta is build-to-build variation
    /// rather than a memo cost. Do not "optimize" this into one of them
    /// without re-measuring all four workloads.
    ///
    /// Lifetime is a single scan, which is exactly the table-immutability
    /// invariant the property-run cache above already relies on, so no
    /// mutation-epoch check is needed -- and none would be sound, since `aset`
    /// on a syntax char-table bypasses `note_syntax_table_mutation`.
    ascii: [Cell<Option<SyntaxEntry>>; 128],
    /// Identity of the chartable `ascii` was filled against, so a cache reused
    /// across tables cannot serve entries from the wrong one.
    ascii_table: Cell<usize>,
    /// Where the property comes from, so a cached run and the resolution that
    /// produced it cannot come from different sources. Last so the run fields
    /// above, which every scanned character reads, keep the front of the
    /// struct.
    props: SyntaxProperties<'a>,
}

impl<'a> SyntaxPropRange<'a> {
    fn new(props: SyntaxProperties<'a>) -> Self {
        let run = match props {
            SyntaxProperties::Ignore => PropRunCells::covering_everything(),
            SyntaxProperties::Honor(_) => PropRunCells::default(),
        };
        Self {
            run,
            ascii: std::array::from_fn(|_| Cell::new(None)),
            ascii_table: Cell::new(0),
            props,
        }
    }

    /// The property source this cache resolves through, for the byte-addressed
    /// helpers a char-indexed scan calls into (comment skipping).
    fn props(&self) -> SyntaxProperties<'a> {
        self.props
    }

    /// The syntax entry for ASCII `ch` under `table`, served from the per-scan
    /// memo. Returns `None` for non-ASCII, which the caller resolves directly.
    #[inline]
    fn ascii_entry(&self, table: &SyntaxTable, ch: char) -> Option<SyntaxEntry> {
        let cp = ch as u32;
        if cp >= 128 {
            return None;
        }

        let id = table.chartable.bits();
        if self.ascii_table.get() != id {
            // First use, or a different table than the memo was filled
            // against: drop what is there rather than serve a foreign entry.
            for slot in &self.ascii {
                slot.set(None);
            }
            self.ascii_table.set(id);
        }

        let slot = &self.ascii[cp as usize];
        if let Some(entry) = slot.get() {
            return Some(entry);
        }
        let entry = syntax_entry_from_table(table, ch);
        slot.set(Some(entry));
        Some(entry)
    }

    /// The resolved `syntax-table` property at char position `pos`, served from
    /// the cached run when possible.  In debug builds every cache hit is
    /// validated against a fresh interval lookup, the same safety net the
    /// byte<->char cache uses.
    ///
    /// The run is an interval, and a character's resolution depends only on its
    /// interval's plist plus the snapshotted variables, so caching the RESOLVED
    /// value over the run is exactly as sound as caching the raw one was -- and
    /// keeps the category indirection off the per-character path.
    #[inline]
    /// True when the cached run POSITIVELY covers `pos` with no
    /// `syntax-table` property — the precondition for the parse loop's
    /// flat-ASCII batch path. Outside the cached run this answers false
    /// and the caller takes the full per-char path (which refills).
    #[inline]
    fn covered_prop_free(&self, pos: usize) -> bool {
        matches!(self.run.get(pos), Some(None))
    }

    fn syntax_table_prop_at_char(&self, buf: &Buffer, pos: usize) -> Option<Value> {
        // In-run fast path: two integer compares, as in GNU's
        // UPDATE_SYNTAX_TABLE_FORWARD -- and free of any test of `props`, which
        // an ignoring scan folds into the range check by caching a run of
        // `None` over the whole position space. Touching the resolver here
        // instead measured +10.6% on a forward-sexp sweep.
        if let Some(value) = self.run.get(pos) {
            #[cfg(debug_assertions)]
            if let SyntaxProperties::Honor(resolver) = self.props {
                let plist = buf.interval_plist_at_char_pos(offset_char_pos(CharPos0::ZERO, pos));
                let fresh = plist.and_then(|plist| resolver.resolve_interval_plist(plist));
                debug_assert!(
                    value == fresh,
                    "SyntaxPropRange stale syntax-table at char {pos} in [{}, {})",
                    self.run.start.get(),
                    self.run.end.get()
                );
            }
            return value;
        }
        let SyntaxProperties::Honor(resolver) = self.props else {
            // Unreachable: an ignoring cache covers every position. Keep the
            // total function rather than an unwrap.
            return None;
        };
        self.refill_run(buf, pos, resolver)
    }

    /// Locate the run containing `pos`, resolve its property once, and cache
    /// both.
    ///
    /// Outlined so the per-character fast path above stays small enough to
    /// inline into the scan loop.
    #[inline(never)]
    fn refill_run(
        &self,
        buf: &Buffer,
        pos: usize,
        resolver: CharPropertyResolver<'_>,
    ) -> Option<Value> {
        // Cross-scan memo first: font-lock and indentation drive thousands of
        // SHORT parses per command over the same region, and every scan's
        // per-scan cache starts cold. A hit hands back the resolved run with
        // no interval descent and no coalescing walk — the same amortization
        // the byte-addressed refill already has. Guarded by the resolver's
        // coalescing preconditions like the byte side.
        let coalesce = resolver.supports_presence_coalescing();
        if coalesce && let Some((start, end, value)) = buf.syntax_char_run_memo_lookup(pos) {
            self.run.set(start as usize, end as usize, value);
            return value;
        }
        let char_pos = offset_char_pos(CharPos0::ZERO, pos);
        let (plist, start, _end) = buf.interval_plist_run_at_char_pos(char_pos);
        let value = plist.and_then(|plist| resolver.resolve_interval_plist(plist));
        let end = coalesced_syntax_run_end(buf, char_pos, &resolver);
        self.run.set(start.get(), end.get(), value);
        if coalesce {
            buf.syntax_char_run_memo_store(start.get() as u64, end.get() as u64, value);
        }
        value
    }
}

/// End of the run over which the resolved `syntax-table` property is provably
/// constant from `char_pos`: walk interval boundaries comparing only the keys
/// resolution reads (the property, `category`, and its aliases), bounded to a
/// fixed lookahead. Font-lock splits buffers into dense `face`-only intervals,
/// so the RESOLVED run is typically far longer than the raw interval — this
/// turns a per-interval root descent into a per-lookahead cursor walk.
fn coalesced_syntax_run_end(
    buf: &Buffer,
    char_pos: CharPos0,
    resolver: &CharPropertyResolver<'_>,
) -> CharPos0 {
    const COALESCE_LOOKAHEAD_CHARS: usize = 4096;
    let total = buf.accessible_char_region().end();
    let cap = CharPos0::new(
        char_pos
            .get()
            .saturating_add(COALESCE_LOOKAHEAD_CHARS)
            .min(total.get()),
    );
    let end = if resolver.supports_presence_coalescing() {
        // Common case: race through `face`-only intervals on the cached
        // presence bit; prop-bearing intervals are never merged.
        buf.syntax_prop_free_run_end_at_char_pos(char_pos, cap)
    } else {
        // Aliases widen the key set and `default-text-properties` makes a
        // key-free interval resolve differently from a gap: fall back to the
        // raw single-interval run.
        let (_, _, end) = buf.interval_plist_run_at_char_pos(char_pos);
        end
    };
    // Degenerate cap (scan sitting at the region end): keep the run non-empty
    // so the per-char fast path cannot loop on refills.
    if end <= char_pos {
        CharPos0::new(char_pos.get() + 1)
    } else {
        end
    }
}

#[inline]
fn syntax_entry_from_table(table: &SyntaxTable, ch: char) -> SyntaxEntry {
    record_syntax_table_decode();
    table
        .get_entry(ch)
        .unwrap_or_else(|| SyntaxEntry::simple(table.char_syntax(ch)))
}

#[inline]
fn effective_syntax_entry_for_char_at_byte(
    buf: &Buffer,
    table: &SyntaxTable,
    ch: char,
    byte_pos: EmacsBytePos,
    prop_cache: &SyntaxPropByteRun<'_>,
) -> SyntaxEntry {
    if let Some(prop) = prop_cache.syntax_table_prop_at_emacs_byte(buf, byte_pos)
        && let Some(entry) = syntax_entry_from_syntax_property(prop, ch)
    {
        return entry;
    }

    if let Some(entry) = prop_cache.ascii_entry(table, ch) {
        return entry;
    }

    syntax_entry_from_table(table, ch)
}

/// Syntax class seen by GNU's regexp `SYNTAX` macro at a buffer byte.
///
/// Regexp matching differs from plain `char-syntax`: when
/// `parse-sexp-lookup-properties` is active it consults the positional
/// `syntax-table` property before falling back to the buffer's table.
pub(crate) fn regexp_syntax_class_at_emacs_byte(
    buf: &Buffer,
    table: &SyntaxTable,
    ch: char,
    byte_pos: EmacsBytePos,
    prop_cache: &SyntaxPropByteRun<'_>,
) -> SyntaxClass {
    effective_syntax_entry_for_char_at_byte(buf, table, ch, byte_pos, prop_cache).class
}

/// The syntax entry governing the character at `abs_char`.
///
/// Reads the `syntax-table` property through a per-scan run cache (GNU
/// `gl_state`), avoiding an interval lookup (and a char->byte->char round trip)
/// on every char, and serves the table lookup itself from the same cache's
/// lazily-filled ASCII memo.
///
/// Beats GNU on the dominant path: source text is overwhelmingly ASCII, so
/// `ch < 128` costs one array index plus a `Copy` here -- strictly less work
/// than GNU's per-char `CHAR_TABLE_REF_ASCII` + `XCAR` decode, and than
/// neomacs's own ~5 type-tag derefs + cons decode. The memo caches the
/// identical `syntax_entry_from_table` computation used for non-ASCII below, so
/// it is behavior-preserving by construction.
// Inlined into the parse/scan loops: this runs once per character
// stepped (8.7M calls on a fontify pass) and the un-inlined five-arg
// call cost more than the fast path it guards.
#[inline]
fn effective_syntax_entry_for_abs_char(
    buf: &Buffer,
    table: &SyntaxTable,
    ch: char,
    abs_char: usize,
    prop_cache: &SyntaxPropRange<'_>,
) -> SyntaxEntry {
    if let Some(prop) = prop_cache.syntax_table_prop_at_char(buf, abs_char)
        && let Some(entry) = syntax_entry_from_syntax_property(prop, ch)
    {
        return entry;
    }

    if let Some(entry) = prop_cache.ascii_entry(table, ch) {
        return entry;
    }

    syntax_entry_from_table(table, ch)
}

pub(crate) fn parse_sexp_lookup_properties_enabled(ctx: &super::eval::Context) -> bool {
    SyntaxStateVariable::ParseSexpLookupProperties.enabled(ctx)
}

/// Interned-once ids for the propertize-for-scan gate — it runs before
/// every syntax-dependent search/scan and re-hashed both names per call.
#[inline(always)]
fn internal_syntax_propertize_sym() -> crate::emacs_core::intern::SymId {
    static SYMBOL: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
        std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| crate::emacs_core::intern::intern("internal--syntax-propertize"))
}

/// `syntax-propertize--done` id, shared with the search prep's warm
/// precheck.
#[inline(always)]
pub(crate) fn syntax_propertize_done_sym() -> crate::emacs_core::intern::SymId {
    static SYMBOL: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
        std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| crate::emacs_core::intern::intern("syntax-propertize--done"))
}

pub(crate) fn maybe_syntax_propertize_for_scan(
    eval: &mut super::eval::Context,
    target_char_pos: usize,
) -> EvalResult {
    if !parse_sexp_lookup_properties_enabled(eval)
        || eval
            .obarray
            .symbol_function_id(internal_syntax_propertize_sym())
            .is_none()
    {
        return Ok(Value::NIL);
    }

    let done = eval
        .special_variable_value_by_id(syntax_propertize_done_sym())
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
        Value::from_sym_id(internal_syntax_propertize_sym()),
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

/// The (exclusive, 0-based) character position up to which syntax-table
/// properties are known to be set -- GNU `gl_state.e_property` after
/// `parse_sexp_propertize`, read back from `syntax-propertize--done` (a
/// 1-based position: text before it is propertized).  Clamped to
/// `[from + 1, end]` so a scan window always makes progress; when nothing
/// tracks the frontier (no propertize function, unbound variable) the whole
/// accessible range is usable.
fn syntax_propertize_frontier_for_scan(
    eval: &mut super::eval::Context,
    from: usize,
    end: usize,
) -> usize {
    let done = eval
        .special_variable_value_by_id(syntax_propertize_done_sym())
        .unwrap_or(Value::NIL);
    match done.kind() {
        ValueKind::Fixnum(done) if done > 0 && (done as usize) <= end => {
            (done as usize - 1).max(from.saturating_add(1)).min(end)
        }
        _ => end,
    }
}

/// `(syntax-class-to-char CLASS)` — map syntax class code to descriptor char.
pub(crate) fn builtin_syntax_class_to_char(args: Vec<Value>) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("syntax-class-to-char"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let class = match args[0].kind() {
        ValueKind::Fixnum(n) => n,
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("fixnump"), args[0]],
            ));
        }
    };

    let Some(class) = SyntaxClass::from_plain_code(class) else {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
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
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("matching-paren"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let ch = match args[0].kind() {
        ValueKind::Fixnum(n) => char::from_u32(n as u32).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), args[0]],
            )
        })?,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), args[0]],
            ));
        }
    };

    if let Some(buf) = buffers.current_buffer() {
        let entry = SyntaxTable::for_buffer(buf).get_entry(ch);
        if let Some(e) = entry
            && matches!(e.class, SyntaxClass::Open | SyntaxClass::Close)
            && let Some(m) = e.matching_char
        {
            return Ok(Value::char(m));
        }
    }
    Ok(Value::NIL)
}

/// `(standard-syntax-table)` — return the standard syntax table.
pub(crate) fn builtin_standard_syntax_table(args: Vec<Value>) -> EvalResult {
    if !args.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
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
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("syntax-table-p"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    Ok(
        if super::chartable::char_table_has_subtype_named(&args[0], "syntax-table") {
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
            LispCondition::WrongNumberOfArguments,
            vec![syntax_table_prop_symbol(), Value::fixnum(args.len() as i64)],
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
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("set-syntax-table"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    if builtin_syntax_table_p(vec![args[0]])?.is_nil() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
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
            LispCondition::WrongNumberOfArguments,
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
                LispCondition::WrongTypeArgument,
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
    // GNU `Fmodify_syntax_entry` ends with `clear_regexp_cache ()` —
    // compiled regexps whose fastmap/bitmap baked syntax-table content
    // must not survive a table mutation.  Our caches key those entries
    // by (table identity, epoch); bump the epoch.
    note_syntax_table_mutation();

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
            LispCondition::WrongNumberOfArguments,
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
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), args[0]],
            ));
        }
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
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

/// Word-boundary predicate for the casing operations (capitalize /
/// upcase-initials / *-word / *-region). GNU decides word constituency in
/// `case_ch_is_word` (`src/casefiddle.c`) purely from the buffer syntax table:
/// `SYNTAX(ch) == Sword`, or `== Ssymbol` when `case-symbols-as-words` is set.
/// This is what lets a `set-case-syntax-pair` char (which installs *word*
/// syntax via `modify-syntax-entry`) participate in a word -- Unicode
/// letterness is irrelevant. Falls back to the standard syntax table when there
/// is no current buffer (e.g. casing a plain string outside any buffer).
pub(crate) fn casing_word_predicate(
    eval: &super::eval::Context,
) -> impl Fn(u32) -> bool + Copy + 'static {
    let symbols_as_words = eval
        .eval_symbol("case-symbols-as-words")
        .unwrap_or(Value::NIL)
        .is_truthy();
    let chartable = eval
        .buffers
        .current_buffer()
        .map(|buf| SyntaxTable::for_buffer(buf).chartable);
    move |code: u32| {
        let class = match chartable {
            Some(table) => syntax_class_at_char_code(&table, code),
            None => standard_syntax_class_for_code(code),
        };
        class == SyntaxClass::Word || (symbols_as_words && class == SyntaxClass::Symbol)
    }
}

/// `(syntax-after POS)` — return syntax descriptor for char at POS.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_syntax_after(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_syntax_after_in_buffers(&eval.obarray, &eval.buffers, args)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_syntax_after_in_buffers(
    obarray: &Obarray,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
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
                LispCondition::WrongTypeArgument,
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
        &SyntaxPropByteRun::new(SyntaxProperties::for_scan(true, obarray, buffers)),
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
    let delegated = eval
        .eval_symbol("forward-comment-function")
        .unwrap_or(Value::NIL);
    if !delegated.is_nil() {
        return eval.apply(delegated, args);
    }

    let count = expect_forward_comment_count(&args)?;
    let honor = parse_sexp_lookup_properties_enabled(eval);

    if honor && count > 0 {
        // GNU propertizes lazily, only as far as the scan actually reaches.
        // Propertizing the whole accessible tail here made every post-edit
        // `forward-comment` re-run syntax-propertize from the edit point to
        // point-max, which is quadratic under newcomment's per-line loops.
        // Instead propertize a bounded window; the scan below is side-effect
        // free (point commits only at the end) and moves strictly forward, so
        // if it stopped near the propertized frontier we widen and re-run.
        let (current_id, point_char, end_char) = {
            let buf = eval
                .buffers
                .current_buffer()
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            (
                buf.id(),
                buf.point_char_pos().get(),
                buf.accessible_char_region().end().get(),
            )
        };
        // GNU `parse_sexp_propertize (charpos)` asks Lisp for
        // `min (zv, charpos + 1)` and lets `syntax-propertize` decide how far
        // it actually goes (its own `syntax-propertize-chunk-size`, 500);
        // the scan then runs up to that frontier (`gl_state.e_property`)
        // and asks again only when it crosses it.  A fixed 1024-char window
        // here re-propertized 2x GNU's text after EVERY flush -- newcomment's
        // per-line insert flushes `syntax-propertize--done` back to the
        // edit, so a comment-region over 100 lines paid ~176 Lisp calls of
        // ~1K chars each (41% of the window; GNU's whole window was smaller).
        let mut target = point_char.saturating_add(2);
        let mut last_window_end = 0usize;
        loop {
            maybe_syntax_propertize_for_scan(eval, target)?;
            let mut window_end = syntax_propertize_frontier_for_scan(eval, point_char, end_char);
            if window_end <= last_window_end {
                // The frontier did not advance (GNU: "internal--syntax-propertize
                // did not move syntax-propertize--done"); scan the rest as is
                // rather than spin.
                window_end = end_char;
            }
            last_window_end = window_end;
            // This is one semantic snapshot boundary: propertization above
            // ran arbitrary Lisp and may have changed either the property
            // resolver inputs or buffer-local comment escape policy.
            let policy = SexpScanPolicy::for_context(eval);
            let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
            let (ok, final_pos, final_char) = {
                let buf = eval
                    .buffers
                    .current_buffer()
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                let (ok, final_pos) =
                    forward_comment_scan_in_buffer(buf, count, props, policy.comment_end_escape);
                let final_char = buf.emacs_byte_pos_to_char_pos_clamped(final_pos).get();
                (ok, final_pos, final_char)
            };
            // Comment start/end matching peeks at most one char past the
            // cursor, so a stop 2+ chars inside the window read only
            // propertized text.
            if window_end >= end_char || final_char.saturating_add(2) <= window_end {
                let _ = eval
                    .buffers
                    .goto_buffer_emacs_byte_pos(current_id, final_pos);
                return Ok(if ok { Value::T } else { Value::NIL });
            }
            // Crossed the frontier: GNU's `UPDATE_SYNTAX_TABLE_FORWARD`
            // would call `parse_sexp_propertize (window_end)` here.
            target = window_end.saturating_add(2);
        }
    }

    if honor {
        // count <= 0: the scan only reads at or before point.
        let target = eval
            .buffers
            .current_buffer()
            .map(|buf| buf.point_char_pos().get())
            .unwrap_or(0);
        if target > 0 {
            maybe_syntax_propertize_for_scan(eval, target)?;
        }
    }

    // As above, snapshot all Lisp-visible scan controls only after the last
    // possible syntax-propertize callback.
    let policy = SexpScanPolicy::for_context(eval);
    let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
    forward_comment_in_buffers(&mut eval.buffers, count, props, policy.comment_end_escape)
}

fn expect_forward_comment_count(args: &[Value]) -> Result<i64, Flow> {
    if args.len() != 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
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
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integerp"), args[0]],
            ));
        }
    };
    Ok(count)
}

fn forward_comment_in_buffers(
    buffers: &mut BufferManager,
    count: i64,
    props: SyntaxProperties<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> EvalResult {
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
        forward_comment_scan_in_buffer(buf, count, props, escape_policy)
    };
    let _ = buffers.goto_buffer_emacs_byte_pos(current_id, final_pos);
    Ok(if ok { Value::T } else { Value::NIL })
}

/// Run the comment scan without moving point; returns (all-skipped, stop pos).
/// The cursor only ever advances toward the scan direction, so for forward
/// scans the stop position is also the maximum position examined.
fn forward_comment_scan_in_buffer(
    buf: &Buffer,
    count: i64,
    props: SyntaxProperties<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> (bool, EmacsBytePos) {
    // One cache for the whole call: `forward-comment` examines every
    // character it steps over, and until this existed each of those did a
    // fresh interval lookup and a byte->char conversion.
    let prop_cache = SyntaxPropByteRun::new(props);
    let mut scanner = ForwardCommentCursor::new(buf);
    let ok = if count > 0 {
        forward_comment_forward(&mut scanner, count as u64, &prop_cache, escape_policy)
    } else {
        forward_comment_backward(
            &mut scanner,
            (-count) as u64,
            &prop_cache,
            escape_policy,
            BackwardCommentEntryPolicy::ForwardComment,
        )
    };
    (ok, scanner.point_emacs_byte_pos())
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
    prop_cache: &SyntaxPropByteRun<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> bool {
    let mut remaining = count;
    let max = buf.accessible_emacs_byte_region().end();

    'comments: while remaining > 0 {
        // GNU classifies one complete token before deciding whether it is
        // ignorable whitespace, a comment opener, or a stopping character.
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
                prop_cache,
            );
            let class = entry.class;
            let flags = entry.flags;

            // GNU forms a two-character opener before dispatching on either
            // character's base syntax.  The pair therefore overrides `<`,
            // `!`, whitespace, newline end syntax, and every other base class.
            if flags.contains(SyntaxFlags::COMMENT_START_FIRST) {
                let next_pos = unit.end;
                if next_pos < max
                    && let Some(unit2) = buffer_syntax_char_after(buf, next_pos)
                {
                    let entry2 = effective_syntax_entry_for_char_at_byte(
                        buf,
                        &SyntaxTable::for_buffer(buf),
                        unit2.ch,
                        next_pos,
                        prop_cache,
                    );
                    let capabilities = CommentMarkerCapabilities::between(flags, entry2.flags);
                    if let Some(flavor) = capabilities.opener {
                        buf.goto_emacs_byte_pos(unit2.end);
                        if !scan_forward_comment_body(buf, flavor, prop_cache, escape_policy) {
                            return false;
                        }
                        remaining -= 1;
                        continue 'comments;
                    }
                }
            }

            if class == SyntaxClass::Whitespace
                || (class == SyntaxClass::EndComment && unit.ch == '\n')
            {
                buf.goto_emacs_byte_pos(unit.end);
                continue;
            }

            if class == SyntaxClass::Comment {
                let flavor = CommentFlavor::single(flags);
                buf.goto_emacs_byte_pos(unit.end);
                if !scan_forward_comment_body(buf, flavor, prop_cache, escape_policy) {
                    return false;
                }
                remaining -= 1;
                continue 'comments;
            }

            if class == SyntaxClass::CommentFence {
                buf.goto_emacs_byte_pos(unit.end);
                if !scan_forward_comment_fence(buf, prop_cache, escape_policy) {
                    return false;
                }
                remaining -= 1;
                continue 'comments;
            }

            return false;
        }
    }

    true
}

/// Scan forward through comment body until matching comment end.
/// Point should be positioned right after the comment start.
/// Returns true if comment end was found.
fn scan_forward_comment_body(
    buf: &mut ForwardCommentCursor<'_>,
    flavor: CommentFlavor,
    prop_cache: &SyntaxPropByteRun<'_>,
    escape_policy: CommentEndEscapePolicy,
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
            prop_cache,
        );
        let class = entry.class;
        let flags = entry.flags;

        // GNU checks a complete single-character ender first.
        if class == SyntaxClass::EndComment && CommentFlavor::single(flags) == flavor {
            buf.goto_emacs_byte_pos(unit.end);
            nesting -= 1;
            if nesting <= 0 {
                return true;
            }
            continue;
        }

        // A nested single-character opener is applied before the same
        // character participates in a two-character marker.  This apparently
        // odd ordering is observable for combined base-class/flag entries and
        // is exactly GNU `forw_comment`'s order.
        if flavor.nesting.is_nested()
            && class == SyntaxClass::Comment
            && CommentFlavor::single(flags) == flavor
        {
            nesting += 1;
        }

        // GNU applies the configurable escape skip after single-character
        // delimiters, but before testing two-character enders/openers.
        if escape_policy == CommentEndEscapePolicy::EscapeQuotesEnder
            && matches!(class, SyntaxClass::Escape | SyntaxClass::CharQuote)
        {
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

        let mut pair: Option<(BufferSyntaxChar, CommentMarkerCapabilities)> = None;
        if flags.contains(SyntaxFlags::COMMENT_END_FIRST)
            || flags.contains(SyntaxFlags::COMMENT_START_FIRST)
        {
            let next_pos = unit.end;
            if next_pos < max
                && let Some(unit2) = buffer_syntax_char_after(buf, next_pos)
            {
                let entry2 = effective_syntax_entry_for_char_at_byte(
                    buf,
                    &SyntaxTable::for_buffer(buf),
                    unit2.ch,
                    next_pos,
                    prop_cache,
                );
                pair = Some((
                    unit2,
                    CommentMarkerCapabilities::between(flags, entry2.flags),
                ));
            }
        }

        // Two-character enders precede two-character nested openers.  A
        // single nested opener above has already affected `nesting`.
        if flags.contains(SyntaxFlags::COMMENT_END_FIRST) {
            if let Some((unit2, capabilities)) = pair
                && capabilities.ender == Some(flavor)
            {
                buf.goto_emacs_byte_pos(unit2.end);
                nesting -= 1;
                if nesting <= 0 {
                    return true;
                }
                continue;
            }
        }

        // Two-character nested comment start.
        if flavor.nesting.is_nested() {
            if flags.contains(SyntaxFlags::COMMENT_START_FIRST)
                && let Some((unit2, capabilities)) = pair
                && capabilities.opener == Some(flavor)
            {
                buf.goto_emacs_byte_pos(unit2.end);
                nesting += 1;
                continue;
            }
        }

        buf.goto_emacs_byte_pos(unit.end);
    }
}

/// Scan forward for matching comment fence character.
fn scan_forward_comment_fence(
    buf: &mut ForwardCommentCursor<'_>,
    prop_cache: &SyntaxPropByteRun<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> bool {
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
            prop_cache,
        );
        let class = entry.class;

        if escape_policy == CommentEndEscapePolicy::EscapeQuotesEnder
            && matches!(class, SyntaxClass::Escape | SyntaxClass::CharQuote)
        {
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
    prop_cache: &SyntaxPropByteRun<'_>,
    escape_policy: CommentEndEscapePolicy,
    entry_policy: BackwardCommentEntryPolicy,
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
                prop_cache,
            );
            let class = entry.class;
            let flags = entry.flags;
            // GNU computes this at the character initially encountered by
            // the backward walk (the second character of a two-character
            // ender).  Forming such an ender separately requires its first
            // character to be unquoted; `comment-end-can-be-escaped` then
            // decides whether this original quote suppresses the ender.
            let ender_quoted = char_quoted_at_byte(buf, unit.start, min, prop_cache);

            let mut code = class;
            let mut comment_flavor = CommentFlavor::single(flags);
            let mut marker_start = unit.start;

            // Check for two-char comment end: current char has
            // COMMENT_END_SECOND, prev char has COMMENT_END_FIRST.
            if flags.contains(SyntaxFlags::COMMENT_END_SECOND) {
                let prev_pos = unit.start;
                if prev_pos > min
                    && let Some(unit2) = buffer_syntax_char_before(buf, prev_pos)
                {
                    let ch2_pos = unit2.start;
                    let entry2 = effective_syntax_entry_for_char_at_byte(
                        buf,
                        &SyntaxTable::for_buffer(buf),
                        unit2.ch,
                        ch2_pos,
                        prop_cache,
                    );
                    let flags2 = entry2.flags;
                    let capabilities = CommentMarkerCapabilities::between(flags2, flags);
                    if let Some(flavor) = capabilities.ender
                        && entry_policy.accepts_two_char_ender_with_quoted_first(
                            char_quoted_at_byte(buf, unit2.start, min, prop_cache),
                        )
                    {
                        code = SyntaxClass::EndComment;
                        comment_flavor = flavor;
                        marker_start = unit2.start;
                        // Move past both chars of the two-char end.
                        buf.goto_emacs_byte_pos(unit2.start);
                    }
                }
            }

            // Comment fence backward.
            if code == SyntaxClass::CommentFence {
                buf.goto_emacs_byte_pos(unit.start);
                if !scan_backward_comment_fence(buf, prop_cache) {
                    buf.goto_emacs_byte_pos(pt);
                    return false;
                }
                // Successfully skipped one comment via fence.
                break;
            }

            if code == SyntaxClass::EndComment {
                if entry_policy.escaped_ender_is_suppressed(escape_policy, ender_quoted) {
                    if unit.ch == '\n' {
                        buf.goto_emacs_byte_pos(marker_start);
                        continue;
                    }
                    buf.goto_emacs_byte_pos(pt);
                    return false;
                }
                // If we didn't already move point for a two-char end,
                // move past the single-char end now.
                if buf.point_emacs_byte_pos() == pt {
                    buf.goto_emacs_byte_pos(unit.start);
                }
                if scan_backward_comment_body(buf, comment_flavor, prop_cache, escape_policy) {
                    // Successfully scanned back through the comment body.
                    break;
                }
                // scan_backward_comment_body failed.
                if unit.ch == '\n' {
                    // GNU: "This end-of-line is not an end-of-comment.
                    // Treat it like a whitespace."
                    // Restore to just before the newline and continue
                    // the inner loop.
                    buf.goto_emacs_byte_pos(marker_start);
                    continue;
                }
                // Non-newline EndComment that failed to find a matching
                // comment start — failure.
                // GNU's two-character path advances once to undo the extra
                // delimiter decrement and once more at `leave`; both single-
                // and two-character failures therefore restore original point.
                buf.goto_emacs_byte_pos(pt);
                return false;
            }

            if class == SyntaxClass::Whitespace && !ender_quoted {
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

/// The delimiter of a string the backward comment walk is currently inside.
///
/// GNU keeps this as a single `int` (`string_style`), overloading it with the
/// `ST_STRING_STYLE` / `ST_COMMENT_STYLE` sentinels for the two fence classes;
/// naming the three cases keeps the sentinels from having to be reserved out of
/// the character range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BackCommentStringStyle {
    /// An ordinary string quote (`Sstring`), identified by its character:
    /// two *different* quote characters cannot be told apart scanning backward.
    Delim(char),
    /// A generic string fence (`Sstring_fence`).
    StringFence,
    /// A generic comment fence (`Scomment_fence`), which GNU counts as a
    /// string delimiter for parity purposes.
    CommentFence,
}

/// GNU `char_quoted`, addressed by Emacs byte position: is the character
/// starting at `pos` preceded by an odd number of escape / character-quote
/// characters?
fn char_quoted_at_byte(
    buf: &Buffer,
    pos: EmacsBytePos,
    min: EmacsBytePos,
    prop_cache: &SyntaxPropByteRun<'_>,
) -> bool {
    let table = SyntaxTable::for_buffer(buf);
    let mut cursor = pos;
    let mut quoted = false;
    while cursor > min {
        let Some(unit) = buffer_syntax_char_before(buf, cursor) else {
            break;
        };
        let class =
            effective_syntax_entry_for_char_at_byte(buf, &table, unit.ch, unit.start, prop_cache)
                .class;
        if !matches!(class, SyntaxClass::Escape | SyntaxClass::CharQuote) {
            break;
        }
        quoted = !quoted;
        cursor = unit.start;
    }
    quoted
}

/// GNU `back_comment`'s `lossage` fallback: re-parse *forward* and take the
/// comment start from the resulting parse state.
///
/// Backward scanning cannot resolve which of two readings of a mixed
/// string/comment run is the real one -- GNU's own examples are `" { " a { " }`
/// and `{ a (* " *)` -- so once the walk knows it is guessing it throws the
/// guess away and parses forward from a position that is known not to be inside
/// a string or a comment.
///
/// GNU picks that position with `find_defun_start`, which by default (with
/// `comment-use-syntax-ppss` non-nil) asks `syntax-ppss` for the start of the
/// construct containing `comment_end` -- itself a `parse-partial-sexp` from
/// `BEGV`.  Parsing straight from `BEGV` reaches the same answer without a Lisp
/// call and without GNU's retry loop, whose only purpose is to recover when the
/// heuristic start point turns out to be inside another comment.  `BEGV` is
/// always outside every string and comment, so one parse settles it.
///
/// Returns the comment's start position, or `None` when the forward parse says
/// `comment_end` does not end a comment of this style after all.
///
/// Cost: one forward parse of everything before `comment_end`, with no cache
/// across calls.  GNU pays the same parse but gets it from `syntax-ppss`, whose
/// cache is incremental and shared with the rest of the editor.  Reaching that
/// cache from here needs the evaluator, which the byte-addressed comment
/// scanners do not carry; until they do, this is the slow-but-correct half of
/// the trade, and it only runs on the rare buffers that reach lossage at all.
fn back_comment_reparse(
    buf: &ForwardCommentCursor<'_>,
    comment_end: EmacsBytePos,
    flavor: CommentFlavor,
    prop_cache: &SyntaxPropByteRun<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> Option<EmacsBytePos> {
    let begv = buf.accessible_char_region().start();
    let from = char_pos_to_lisp_i64(begv.get());
    let to = buffer_byte_to_lisp_pos(buf, comment_end);
    if to <= from {
        return None;
    }

    let table = SyntaxTable::for_buffer(buf);
    let (state, _) = parse_state_from_range_core(
        buf,
        &table,
        from,
        to,
        None,
        false,
        None,
        CommentStopMode::None,
        prop_cache.props(),
        escape_policy,
    );

    // GNU's acceptance test is `state.incomment == (comnested ? 1 : -1) &&
    // state.comstyle == comstyle`: the parse has to end inside a comment of
    // exactly this style, at exactly this nesting depth.
    let ParseCommentState::Syntax {
        depth,
        flavor: parsed_flavor,
    } = state.in_comment?
    else {
        return None;
    };
    if parsed_flavor != flavor || (flavor.nesting.is_nested() && depth != 1) {
        return None;
    }

    let start = lisp_pos_to_byte(buf, LispCharPos1::new(state.comment_or_string_start?));
    (start != comment_end).then_some(start)
}

/// Scan backward through comment body to find matching comment start.
///
/// This is GNU Emacs's `back_comment()`.  Point should be positioned right
/// after the comment-end delimiter has been consumed (i.e. just before the
/// comment body).
///
/// For **nested** comments the function returns as soon as the nesting
/// count drops to zero.
///
/// For **non-nested** comments the function scans all the way backward,
/// recording the *earliest* comment-starter of the matching style it
/// finds.  A same-style comment-ender encountered during the scan means
/// "anything before this belongs to a different comment" and stops the
/// search.  At the end, point is set to the recorded position.
///
/// Scanning backward cannot tell whether a comment starter it walks over is
/// real or merely sitting inside a string, so the walk also tracks
/// string-quote parity.  A comment starter reached while inside a string --
/// or after any other sign that the walk is guessing -- is *not* accepted;
/// the whole question is handed to [`back_comment_reparse`] instead.
///
/// The walk keeps the syntax immediately to the right of the current
/// character, just like GNU's `prev_syntax`.  That makes a two-character
/// delimiter one classified token when its first character is reached: the
/// first character is the quote-check position, both start/end capabilities
/// remain available, and a base string/fence class cannot hide the marker.
fn scan_backward_comment_body(
    buf: &mut ForwardCommentCursor<'_>,
    flavor: CommentFlavor,
    prop_cache: &SyntaxPropByteRun<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> bool {
    let comment_end = buf.point_emacs_byte_pos();
    let mut nesting = 1i32;
    let min = buf.accessible_emacs_byte_region().start();

    // For non-nested comments: record the earliest matching comment-start
    // seen so far.
    let mut comstart_pos: Option<EmacsBytePos> = None;

    // GNU `string_style`: the string the walk is currently inside, presumed
    // to be none at the point the walk starts.
    let mut string_style: Option<BackCommentStringStyle> = None;
    // GNU `string_lossage`: two different kinds of string delimiter were seen,
    // which backward scanning cannot untangle.
    let mut string_lossage = false;
    // GNU `comment_lossage`: a comment-ender of *another* style was crossed, so
    // a comment starter found further back may belong to that other comment.
    let mut comment_lossage = false;
    // GNU's `goto lossage`: the walk knows its answer would be a guess.
    let mut lossage = false;
    // GNU's `prev_syntax`: the character immediately to the right.  Keeping
    // this observation across loop iterations is what lets classification
    // happen at the first character of a two-character delimiter.
    let mut syntax_to_right: Option<(BufferSyntaxChar, SyntaxEntry)> = None;

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
            prop_cache,
        );
        let class = entry.class;
        let flags = entry.flags;
        let right = syntax_to_right;
        syntax_to_right = Some((unit, entry));

        let marker = right
            .filter(|(right_unit, _)| right_unit.start == unit.end)
            .map(|(_, right_entry)| CommentMarkerCapabilities::between(flags, right_entry.flags))
            .unwrap_or_default();
        let matching_opener = marker.opener == Some(flavor);

        // GNU punts overlapping two-character markers to its forward parser;
        // a local backward choice is not trustworthy for `|*|`, `---`, etc.
        let enters_overlap_check =
            marker.ender.is_some() || matching_opener || class == SyntaxClass::Comment;
        if enters_overlap_check
            && unit.start > min
            && let Some(left_unit) = buffer_syntax_char_before(buf, unit.start)
        {
            let left_entry = effective_syntax_entry_for_char_at_byte(
                buf,
                &SyntaxTable::for_buffer(buf),
                left_unit.ch,
                left_unit.start,
                prop_cache,
            );
            let right_flags = right.map_or(SyntaxFlags::empty(), |(_, entry)| entry.flags);
            let overlaps_ender =
                (matching_opener || class == SyntaxClass::Comment || flavor.nesting.is_nested())
                    && flags.contains(SyntaxFlags::COMMENT_END_SECOND)
                    && left_entry.flags.contains(SyntaxFlags::COMMENT_END_FIRST);
            let overlaps_opener = (marker.ender.is_some() || flavor.nesting.is_nested())
                && flags.contains(SyntaxFlags::COMMENT_START_SECOND)
                && CommentStyle::from_start_flags(flags, right_flags) == flavor.style
                && left_entry.flags.contains(SyntaxFlags::COMMENT_START_FIRST);
            if overlaps_ender || overlaps_opener {
                lossage = true;
                break;
            }
        }

        // A token that can be both an opener and an ender is the nearest
        // opener the first time GNU sees it; later occurrences are enders.
        let marker_is_opener =
            matching_opener && (marker.ender.is_none() || comstart_pos.is_none());
        let effective_ender = (!marker_is_opener).then_some(marker.ender).flatten();

        // GNU: "Ignore escaped characters, except comment-enders which cannot
        // be escaped."  For a two-character marker this asks about its first
        // character, after the complete token has been classified.
        let quoted = char_quoted_at_byte(buf, unit.start, min, prop_cache);
        let effective_is_ender = if marker_is_opener {
            false
        } else if effective_ender.is_some() {
            true
        } else {
            class == SyntaxClass::EndComment
        };
        if quoted && (!effective_is_ender || escape_policy.quoted_ender_is_escaped(true)) {
            buf.goto_emacs_byte_pos(unit.start);
            continue;
        }

        if marker_is_opener {
            if string_style.is_some() || comment_lossage || string_lossage {
                lossage = true;
                break;
            }
            let new_pos = unit.start;
            if flavor.nesting.is_nested() {
                buf.goto_emacs_byte_pos(new_pos);
                nesting -= 1;
                if nesting <= 0 {
                    return true;
                }
            } else {
                comstart_pos = Some(new_pos);
                buf.goto_emacs_byte_pos(new_pos);
            }
            continue;
        }

        if let Some(ender_flavor) = effective_ender {
            if ender_flavor == flavor {
                if flavor.nesting.is_nested() {
                    nesting += 1;
                    buf.goto_emacs_byte_pos(unit.start);
                    continue;
                }
                break;
            }
            if comstart_pos.is_some() || unit.ch != '\n' {
                comment_lossage = true;
            }
            buf.goto_emacs_byte_pos(unit.start);
            continue;
        }

        // ── Comment-end (same style) ──────────────────────────────
        // For nested: increases nesting.
        // For non-nested: means our comment can't extend past this,
        //   so stop scanning.
        if class == SyntaxClass::EndComment && CommentFlavor::single(flags) == flavor {
            if flavor.nesting.is_nested() {
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

        // GNU `case Sendcomment`, else branch: an ender of a *different* style.
        // We are mixing comment styles, so any comment starter found further
        // back might belong to that other comment rather than to ours.  GNU
        // exempts a bare newline before the first comment starter, because
        // otherwise every multi-line C comment would take the slow path.
        if class == SyntaxClass::EndComment && (comstart_pos.is_some() || unit.ch != '\n') {
            comment_lossage = true;
        }

        // ── String quotes and fences ─────────────────────────────
        // GNU tracks the parity of string delimiters across the whole walk so
        // that a comment starter *inside* a string is never mistaken for a real
        // one.  Both fence classes count as delimiters here; a generic comment
        // fence is opened and closed by `scan_backward_comment_fence`, never by
        // this function, so it only contributes parity.
        let quote_style = match class {
            SyntaxClass::StringDelim => Some(BackCommentStringStyle::Delim(unit.ch)),
            SyntaxClass::StringFence => Some(BackCommentStringStyle::StringFence),
            SyntaxClass::CommentFence => Some(BackCommentStringStyle::CommentFence),
            _ => None,
        };
        if let Some(quote_style) = quote_style {
            match string_style {
                // Entering a string, walking backward out of it.
                None => string_style = Some(quote_style),
                // Leaving it again.
                Some(open) if open == quote_style => string_style = None,
                // Two kinds of string delimiter: there is no way to grok this
                // scanning backward.
                Some(_) => string_lossage = true,
            }
            buf.goto_emacs_byte_pos(unit.start);
            continue;
        }

        // ── Single-char comment start (class `<`) ────────────────
        if class == SyntaxClass::Comment && CommentFlavor::single(flags) == flavor {
            if string_style.is_some() || comment_lossage || string_lossage {
                // GNU: "There are odd string quotes involved, so let's be
                // careful.  Test case in Pascal: " { " a { " }"
                lossage = true;
                break;
            }
            let new_pos = unit.start;
            if flavor.nesting.is_nested() {
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

        // Default: skip this character and continue scanning.
        buf.goto_emacs_byte_pos(unit.start);
    }

    if lossage {
        // The backward walk cannot be trusted; decide it going forwards.
        if let Some(start) =
            back_comment_reparse(buf, comment_end, flavor, prop_cache, escape_policy)
        {
            buf.goto_emacs_byte_pos(start);
            return true;
        }
        buf.goto_emacs_byte_pos(comment_end);
        return false;
    }

    // For non-nested comments, check if we recorded any comment-start.
    if !flavor.nesting.is_nested()
        && let Some(pos) = comstart_pos
    {
        buf.goto_emacs_byte_pos(pos);
        return true;
    }

    false
}

/// Scan backward for matching comment fence character.
fn scan_backward_comment_fence(
    buf: &mut ForwardCommentCursor<'_>,
    prop_cache: &SyntaxPropByteRun<'_>,
) -> bool {
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
            prop_cache,
        );
        let class = entry.class;

        buf.goto_emacs_byte_pos(unit.start);

        if class == SyntaxClass::CommentFence
            && !char_quoted_at_byte(buf, unit.start, min, prop_cache)
        {
            return true;
        }
    }
}

/// `(backward-prefix-chars)` — move point backward over prefix-syntax chars.
pub(crate) fn builtin_backward_prefix_chars(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let honor = parse_sexp_lookup_properties_enabled(eval);
    let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
    builtin_backward_prefix_chars_in_buffers(&mut eval.buffers, args, props)
}

pub(crate) fn builtin_backward_prefix_chars_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
    props: SyntaxProperties<'_>,
) -> EvalResult {
    if !args.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
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
    let prop_cache = SyntaxPropByteRun::new(props);
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
            &prop_cache,
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
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), args[0]],
                ));
            }
        }
    };

    let honor = parse_sexp_lookup_properties_enabled(eval);
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
        word_motion_with_table(eval, count, honor, wbtable)
    } else {
        let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let table = SyntaxTable::for_buffer(buf);
        forward_word_with_options(buf, &table, count, props)
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

/// `(forward-sexp &optional COUNT)` — move point forward over COUNT balanced
/// expressions.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_forward_sexp(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1i64
    } else {
        match args[0].kind() {
            ValueKind::Fixnum(n) => n,
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), args[0]],
                ));
            }
        }
    };

    let honor = parse_sexp_lookup_properties_enabled(eval);
    if honor {
        let target = eval
            .buffers
            .current_buffer()
            .map(|buf| buf.accessible_char_region().end().get().saturating_add(1))
            .unwrap_or(1);
        maybe_syntax_propertize_for_scan(eval, target)?;
    }
    let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);
    let from = buf.point_emacs_byte_pos();
    let policy = SexpScanPolicy::for_context(eval);
    let new_pos = match scan_sexps_with_options(buf, &table, from.get(), count, props, policy)
        .map_err(|err| signal(LispCondition::ScanError, err.signal_data()))?
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
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_backward_sexp(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let count = if args.is_empty() || args[0].is_nil() {
        1i64
    } else {
        match args[0].kind() {
            ValueKind::Fixnum(n) => n,
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), args[0]],
                ));
            }
        }
    };

    let honor = parse_sexp_lookup_properties_enabled(eval);
    if honor {
        let target = eval
            .buffers
            .current_buffer()
            .map(|buf| buf.accessible_char_region().end().get().saturating_add(1))
            .unwrap_or(1);
        maybe_syntax_propertize_for_scan(eval, target)?;
    }
    let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);
    let from = buf.point_emacs_byte_pos();
    // backward-sexp with positive count => scan_sexps with negative count
    let policy = SexpScanPolicy::for_context(eval);
    let new_pos = match scan_sexps_with_options(buf, &table, from.get(), -count, props, policy)
        .map_err(|err| signal(LispCondition::ScanError, err.signal_data()))?
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
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("scan-lists"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let from = match args[0].kind() {
        ValueKind::Fixnum(n) => n,
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integer-or-marker-p"), args[0]],
            ));
        }
    };
    let count = match args[1].kind() {
        ValueKind::Fixnum(n) => n,
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integerp"), args[1]],
            ));
        }
    };
    let depth = match args[2].kind() {
        ValueKind::Fixnum(n) => n,
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integerp"), args[2]],
            ));
        }
    };

    let honor = parse_sexp_lookup_properties_enabled(ctx);
    if honor {
        // A backward scan (COUNT < 0) never examines positions past FROM, so
        // propertizing through FROM suffices (GNU parse_sexp_propertize is
        // lazy and would stop there); only forward scans need the
        // conservative whole-accessible target.
        let target = ctx
            .buffers
            .current_buffer()
            .map(|buf| {
                if count < 0 {
                    (from.max(1) as usize).saturating_add(1)
                } else {
                    buf.accessible_char_region().end().get().saturating_add(1)
                }
            })
            .unwrap_or(1);
        maybe_syntax_propertize_for_scan(ctx, target)?;
    }

    let props = SyntaxProperties::for_scan(honor, &ctx.obarray, &ctx.buffers);
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

    let policy = SexpScanPolicy::for_context(ctx);
    match scan_lists_with_options(buf, &table, from_char, count, depth, props, policy) {
        Ok(Some(new_char)) => Ok(Value::fixnum(char_pos_to_lisp_i64(new_char))),
        Ok(None) => Ok(Value::NIL),
        Err(err) => Err(signal(LispCondition::ScanError, err.signal_data())),
    }
}

/// `(scan-sexps FROM COUNT)` — scan over COUNT sexps from FROM.
pub(crate) fn builtin_scan_sexps(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    if args.len() != 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("scan-sexps"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let from = match args[0].kind() {
        ValueKind::Fixnum(n) => n,
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("number-or-marker-p"), args[0]],
            ));
        }
    };
    let count = match args[1].kind() {
        ValueKind::Fixnum(n) => n,
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integerp"), args[1]],
            ));
        }
    };

    let honor = parse_sexp_lookup_properties_enabled(ctx);
    if honor {
        // A backward scan (COUNT < 0) never examines positions past FROM, so
        // propertizing through FROM suffices (GNU parse_sexp_propertize is
        // lazy and would stop there); only forward scans need the
        // conservative whole-accessible target.
        let target = ctx
            .buffers
            .current_buffer()
            .map(|buf| {
                if count < 0 {
                    (from.max(1) as usize).saturating_add(1)
                } else {
                    buf.accessible_char_region().end().get().saturating_add(1)
                }
            })
            .unwrap_or(1);
        maybe_syntax_propertize_for_scan(ctx, target)?;
    }

    let props = SyntaxProperties::for_scan(honor, &ctx.obarray, &ctx.buffers);
    let buf = ctx
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);

    let from_char = LispCharPos1::new(from)
        .to_char_pos()
        .min(buf.total_char_end_pos());
    let from_byte = buffer_char_to_emacs_byte_pos(buf, from_char);

    let policy = SexpScanPolicy::for_context(ctx);
    match scan_sexps_with_options(buf, &table, from_byte.get(), count, props, policy) {
        Ok(Some(new_byte)) => Ok(Value::fixnum(buffer_byte_to_lisp_pos(
            buf,
            EmacsBytePos::new(new_byte),
        ))),
        Ok(None) => Ok(Value::NIL),
        Err(err) => Err(signal(LispCondition::ScanError, err.signal_data())),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParseStringState {
    Delim(char),
    Fence,
}

/// The syntax-table comment style identity carried by GNU's parse state.
///
/// The b/c delimiter flags form identities a (0), b (1), c (2), or bc (3);
/// the separate n flag controls nesting.  Keep the identity as an opaque
/// numeric value because `parse-partial-sexp` accepts an externally supplied
/// old state and GNU preserves numeric style values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CommentStyle(i64);

impl CommentStyle {
    const A: Self = Self(0);
    const GENERIC_FENCE_SENTINEL: i64 = 257;

    fn from_main_flags(flags: SyntaxFlags) -> Self {
        Self(
            i64::from(flags.contains(SyntaxFlags::COMMENT_STYLE_B))
                | (i64::from(flags.contains(SyntaxFlags::COMMENT_STYLE_C)) << 1),
        )
    }

    /// GNU uses the second character as the main character of a two-character
    /// comment opener.  Style c, unlike style b, may be present on either.
    fn from_start_flags(first: SyntaxFlags, second: SyntaxFlags) -> Self {
        Self(
            i64::from(second.contains(SyntaxFlags::COMMENT_STYLE_B))
                | (i64::from(
                    first.contains(SyntaxFlags::COMMENT_STYLE_C)
                        || second.contains(SyntaxFlags::COMMENT_STYLE_C),
                ) << 1),
        )
    }

    /// GNU uses the first character as the main character of a two-character
    /// comment ender.  Style c, unlike style b, may be present on either.
    fn from_end_flags(first: SyntaxFlags, second: SyntaxFlags) -> Self {
        Self(
            i64::from(first.contains(SyntaxFlags::COMMENT_STYLE_B))
                | (i64::from(
                    first.contains(SyntaxFlags::COMMENT_STYLE_C)
                        || second.contains(SyntaxFlags::COMMENT_STYLE_C),
                ) << 1),
        )
    }

    fn to_parse_state_value(self) -> Value {
        if self == Self::A {
            Value::NIL
        } else {
            Value::fixnum(self.0)
        }
    }
}

/// Whether a syntax-table comment delimiter participates in nesting.
///
/// GNU stores this as the `n` flag beside the style bits.  It is part of the
/// delimiter identity, not an independent scanning option: a flat delimiter
/// must never match a nested one merely because their b/c style agrees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum CommentNesting {
    #[default]
    Flat,
    Nested,
}

impl CommentNesting {
    fn from_flags(first: SyntaxFlags, second: SyntaxFlags) -> Self {
        if first.contains(SyntaxFlags::COMMENT_NESTABLE)
            || second.contains(SyntaxFlags::COMMENT_NESTABLE)
        {
            Self::Nested
        } else {
            Self::Flat
        }
    }

    fn is_nested(self) -> bool {
        self == Self::Nested
    }
}

/// Complete GNU comment-delimiter identity.
///
/// Keeping style and nestability together makes an incomplete comparison
/// impossible at the comment scanner's matching boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CommentFlavor {
    style: CommentStyle,
    nesting: CommentNesting,
}

impl CommentFlavor {
    fn single(flags: SyntaxFlags) -> Self {
        Self {
            style: CommentStyle::from_main_flags(flags),
            nesting: CommentNesting::from_flags(flags, SyntaxFlags::empty()),
        }
    }

    fn two_char_start(first: SyntaxFlags, second: SyntaxFlags) -> Self {
        Self {
            style: CommentStyle::from_start_flags(first, second),
            nesting: CommentNesting::from_flags(first, second),
        }
    }

    fn two_char_end(first: SyntaxFlags, second: SyntaxFlags) -> Self {
        Self {
            style: CommentStyle::from_end_flags(first, second),
            nesting: CommentNesting::from_flags(first, second),
        }
    }
}

/// The two independent roles a two-character syntax token may have.
///
/// Some modes deliberately use the same token as both an opener and an
/// ender.  Two `Option`s preserve that fact; an enum choosing only one role
/// would throw information away before GNU's opener-precedence rule can run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CommentMarkerCapabilities {
    opener: Option<CommentFlavor>,
    ender: Option<CommentFlavor>,
}

impl CommentMarkerCapabilities {
    fn between(first: SyntaxFlags, second: SyntaxFlags) -> Self {
        Self {
            opener: (first.contains(SyntaxFlags::COMMENT_START_FIRST)
                && second.contains(SyntaxFlags::COMMENT_START_SECOND))
            .then(|| CommentFlavor::two_char_start(first, second)),
            ender: (first.contains(SyntaxFlags::COMMENT_END_FIRST)
                && second.contains(SyntaxFlags::COMMENT_END_SECOND))
            .then(|| CommentFlavor::two_char_end(first, second)),
        }
    }
}

/// The syntax immediately before a resumed `parse-partial-sexp` range.
///
/// GNU passes `state->prev_syntax` into `forw_comment`, allowing the first
/// character of the resumed range to finish a two-character marker begun by
/// the previous call.  Keeping this as a one-shot typed value prevents the
/// main loop from accidentally replacing the boundary syntax before it has
/// been consumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CommentResumeSyntax {
    class: Option<SyntaxClass>,
    flags: SyntaxFlags,
}

impl CommentResumeSyntax {
    fn from_parse_state(raw: i64) -> Self {
        Self {
            class: SyntaxClass::from_code(raw & 0xff),
            flags: SyntaxFlags::new(((raw >> 16) & 0xff) as u8),
        }
    }

    fn marker_with(self, current: SyntaxFlags) -> CommentMarkerCapabilities {
        CommentMarkerCapabilities::between(self.flags, current)
    }

    fn quotes_single_ender(self, policy: CommentEndEscapePolicy) -> bool {
        policy == CommentEndEscapePolicy::EscapeQuotesEnder
            && matches!(
                self.class,
                Some(SyntaxClass::Escape | SyntaxClass::CharQuote)
            )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParseCommentState {
    Syntax { depth: i64, flavor: CommentFlavor },
    Fence,
}

impl ParseCommentState {
    /// Decode GNU parse-state elements 4 (comment state) and 7 (style).
    ///
    /// GNU uses non-numeric element 7 values, including `syntax-table`, as
    /// the generic comment-fence sentinel.  Keeping that representation at
    /// this boundary prevents parser callers from confusing it with style a.
    fn from_oldstate(comment: &Value, style: Option<&Value>) -> Option<Self> {
        if comment.is_nil() {
            return None;
        }

        let syntax_style = match style {
            None => Some(CommentStyle::A),
            Some(value) if value.is_nil() => Some(CommentStyle::A),
            Some(value) => match value.as_fixnum() {
                Some(style) if (0..CommentStyle::GENERIC_FENCE_SENTINEL).contains(&style) => {
                    Some(CommentStyle(style))
                }
                _ => None,
            },
        };

        let Some(style) = syntax_style else {
            return Some(Self::Fence);
        };

        match comment.kind() {
            ValueKind::Fixnum(depth) => Some(Self::Syntax {
                depth,
                flavor: CommentFlavor {
                    style,
                    nesting: CommentNesting::Nested,
                },
            }),
            _ => Some(Self::Syntax {
                depth: 1,
                flavor: CommentFlavor {
                    style,
                    nesting: CommentNesting::Flat,
                },
            }),
        }
    }

    /// Encode GNU parse-state elements 4 (comment state) and 7 (style).
    fn to_parse_state_values(self) -> (Value, Value) {
        match self {
            Self::Syntax {
                depth: comment_depth,
                flavor:
                    CommentFlavor {
                        style,
                        nesting: CommentNesting::Flat,
                    },
            } => {
                debug_assert_eq!(comment_depth, 1);
                (Value::T, style.to_parse_state_value())
            }
            Self::Syntax {
                depth: comment_depth,
                flavor:
                    CommentFlavor {
                        style,
                        nesting: CommentNesting::Nested,
                    },
            } => (Value::fixnum(comment_depth), style.to_parse_state_value()),
            Self::Fence => (Value::T, syntax_table_prop_symbol()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommentStopMode {
    None,
    Comment,
    SyntaxTable,
}

/// GNU `Smax` (one past `Sstring_fence`); used as the "no significant syntax"
/// sentinel for `prev_from_syntax` so element 10 of the parse state is nil.
const PARSE_PREV_SYNTAX_SMAX: i64 = 16;

/// GNU `SYNTAX_FLAGS_COMSTARTEND_FIRST`: bit 16 (comment-start-first) or bit 18
/// (comment-end-first) of a `prev_from_syntax` integer.
const PARSE_PREV_SYNTAX_COMSTARTEND_FIRST: i64 = 0x5_0000;

/// Build GNU's `SYNTAX_WITH_FLAGS` integer for element 10 of `parse-partial-sexp`:
/// the low byte is the syntax class code and bits 16..=23 hold the flag byte
/// (matching GNU `src/syntax.h`: comment-start-first at bit 16, etc.).
fn parse_prev_syntax_int(class: SyntaxClass, flags: SyntaxFlags) -> i64 {
    class.code() | ((flags.bits() as i64) << 16)
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
    /// GNU `prev_from_syntax`: the `SYNTAX_WITH_FLAGS` integer of the most
    /// recently scanned position, or `PARSE_PREV_SYNTAX_SMAX` when that
    /// position holds no significant two-char/quote syntax.
    prev_syntax: i64,
}

/// What a comment-ender sequence did to the parse state.
///
/// GNU consumes a whole comment -- every nested level of it -- inside a single
/// `forw_comment` call (`scan_sexps_forward`, src/syntax.c:3352), and only the
/// code *after* that call clears `state->incomment` and honours `boundary_stop`
/// (src/syntax.c:3370-3374).  An ender that merely pops a nesting level is
/// therefore not a parse boundary at all.  Naming the two outcomes keeps that
/// distinction from having to be re-derived at each ender branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommentEnderEffect {
    /// A nested level closed; the comment is still open.
    NestingLevelClosed,
    /// The comment itself ended here.
    CommentClosed,
}

/// Why `parse_state_from_range_core` stopped while a comment remained open.
/// GNU applies `forw_comment`'s EOF publication rules only at the requested
/// range boundary; COMMENTSTOP returns directly from comment entry instead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ParseCommentExit {
    #[default]
    RangeBoundary,
    StoppedAtEntry,
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
            prev_syntax: PARSE_PREV_SYNTAX_SMAX,
        }
    }

    /// Apply one comment-ender sequence to a syntactic comment of `depth`.
    ///
    /// Mirrors GNU `forw_comment`'s nesting counter: an ender pops one level,
    /// and the comment only ends -- clearing `state->incomment` and
    /// `state->comstr_start` -- when the last level pops.  Note that GNU keeps
    /// `comstr_start` at the OUTERMOST comment start throughout, which is what
    /// element 8 of the parse state reports.
    fn close_comment_level(&mut self, depth: i64, flavor: CommentFlavor) -> CommentEnderEffect {
        let next_depth = depth - 1;
        if next_depth <= 0 {
            self.in_comment = None;
            self.comment_or_string_start = None;
            CommentEnderEffect::CommentClosed
        } else {
            self.in_comment = Some(ParseCommentState::Syntax {
                depth: next_depth,
                flavor,
            });
            CommentEnderEffect::NestingLevelClosed
        }
    }

    /// Publish GNU `forw_comment`'s `last_syntax_ptr` result when the scan
    /// reaches its boundary inside a comment.
    ///
    /// This is deliberately comment-specific: GNU preserves a trailing quote
    /// (and marks the parse state quoted), every end-first marker, and a
    /// start-first marker only while a nestable comment remains open.  Other
    /// raw syntax has already been consumed and becomes `Smax`.
    fn finalize_incomplete_comment_syntax(&mut self) {
        let Some(comment) = self.in_comment else {
            return;
        };
        let Some(class) = SyntaxClass::from_code(self.prev_syntax & 0xff) else {
            self.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
            return;
        };
        let flags = SyntaxFlags::new(((self.prev_syntax >> 16) & 0xff) as u8);
        let quote = matches!(class, SyntaxClass::Escape | SyntaxClass::CharQuote);
        let nested = matches!(
            comment,
            ParseCommentState::Syntax { flavor, .. } if flavor.nesting.is_nested()
        );
        let preserve = quote
            || flags.contains(SyntaxFlags::COMMENT_END_FIRST)
            || (nested && flags.contains(SyntaxFlags::COMMENT_START_FIRST));

        self.quoted |= quote;
        if !preserve {
            self.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
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

        if let Some(v) = items.get(5)
            && v.is_t()
        {
            state.quoted = true;
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
            state.in_comment = ParseCommentState::from_oldstate(item, items.get(7));
        }

        // GNU `internalize_parse_state`:
        //   tem = Fcar (external);   /* element 8 */
        //   state->comstr_start =
        //     RANGED_FIXNUMP (...) ? XFIXNUM (tem) : -1;
        // i.e. when element 8 is nil (or not a fixnum) the comment/string
        // start is normalized to -1, not left unknown.  `scan_sexps_forward`
        // only ever *reports* `comstr_start` while inside a string/comment
        // (element 8 of the result is nil otherwise), so this -1 surfaces
        // exactly when resuming from a from-state that is already inside a
        // string (element 3) or comment (element 4) but gave no start.
        if state.comment_or_string_start.is_none()
            && (state.in_string.is_some() || state.in_comment.is_some())
        {
            state.comment_or_string_start = Some(-1);
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

        // GNU `internalize_parse_state`: element 10 seeds `prev_syntax`
        // (defaulting to Smax when nil), so a continued parse can detect a
        // pending escape/two-char construct that straddled the previous TO.
        state.prev_syntax = match items.get(10).and_then(|v| v.as_fixnum()) {
            Some(n) => n,
            None => PARSE_PREV_SYNTAX_SMAX,
        };

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

    /// GNU `scan_sexps_forward` `done`:
    ///   state->prev_syntax =
    ///     (SYNTAX_FLAGS_COMSTARTEND_FIRST (prev_from_syntax) || state->quoted)
    ///       ? prev_from_syntax : Smax;
    /// then element 10 is nil when the result is Smax, else the integer.
    fn prev_syntax_element(&self) -> Value {
        let effective =
            if (self.prev_syntax & PARSE_PREV_SYNTAX_COMSTARTEND_FIRST) != 0 || self.quoted {
                self.prev_syntax
            } else {
                PARSE_PREV_SYNTAX_SMAX
            };
        if effective == PARSE_PREV_SYNTAX_SMAX {
            Value::NIL
        } else {
            Value::fixnum(effective)
        }
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

        let (comment_value, comment_style_value) = match self.in_comment {
            Some(comment_state) => comment_state.to_parse_state_values(),
            None => (Value::NIL, Value::NIL),
        };

        Value::list(vec![
            Value::fixnum(self.depth),
            containing_sexp_start.map_or(Value::NIL, Value::fixnum),
            completed_sexp_start.map_or(Value::NIL, Value::fixnum),
            string_value,
            comment_value,
            if self.quoted { Value::T } else { Value::NIL },
            Value::fixnum(self.mindepth),
            comment_style_value,
            self.comment_or_string_start
                .map_or(Value::NIL, Value::fixnum),
            stack_value,
            self.prev_syntax_element(),
        ])
    }
}

#[inline]
fn syntax_class_and_flags(
    buf: &Buffer,
    table: &SyntaxTable,
    ch: char,
    abs_char: usize,
    prop_cache: &SyntaxPropRange<'_>,
) -> (SyntaxClass, SyntaxFlags) {
    let entry = effective_syntax_entry_for_abs_char(buf, table, ch, abs_char, prop_cache);
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

#[allow(clippy::too_many_arguments)] // parse-partial-sexp options map directly to GNU semantics
fn parse_state_from_range_with_options(
    buf: &Buffer,
    table: &SyntaxTable,
    from: i64,
    to: i64,
    target_depth: Option<i64>,
    stop_before: bool,
    oldstate: Option<&Value>,
    commentstop: CommentStopMode,
    props: SyntaxProperties<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> (Value, i64) {
    let (state, stopped_at) = parse_state_from_range_core(
        buf,
        table,
        from,
        to,
        target_depth,
        stop_before,
        oldstate,
        commentstop,
        props,
        escape_policy,
    );
    (state.into_value(), stopped_at)
}

/// GNU `scan_sexps_forward`.  The parse state is returned unencoded so that
/// in-tree callers -- `parse-partial-sexp` and `back_comment`'s forward
/// re-parse -- can read it without going through the Lisp representation.
#[allow(clippy::too_many_arguments)] // mirrors GNU `scan_sexps_forward`'s parameters
fn parse_state_from_range_core(
    buf: &Buffer,
    table: &SyntaxTable,
    from: i64,
    to: i64,
    target_depth: Option<i64>,
    stop_before: bool,
    oldstate: Option<&Value>,
    commentstop: CommentStopMode,
    props: SyntaxProperties<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> (PartialParseState, i64) {
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
    let mut chars = BufferChars::new(buf, offset_char_pos(CharPos0::ZERO, from_char));
    let to_idx = to_char - from_char;
    // `prop_cache` carries both the `syntax-table` property run cache and the
    // lazily-filled ASCII syntax memo consumed by
    // `effective_syntax_entry_for_abs_char`.
    let prop_cache = SyntaxPropRange::new(props);

    // Long parses classify ASCII chars through a flat local table while the
    // prop cache positively covers the position with no `syntax-table`
    // property: one array index replaces the layered per-char prop-cell
    // check + Option<Cell> memo decode. The eager 128-entry fill loses on
    // short scans (see the ascii-memo variant table above), so it is gated
    // on span length; entries are `syntax_entry_from_table` — the identical
    // computation the memo caches — so this is behavior-preserving by
    // construction.
    let flat_ascii: Option<[SyntaxEntry; 128]> = Some(flat_ascii_entries_for_table(table));

    let mut state = PartialParseState::from_oldstate(oldstate);
    let mut idx = 0;
    let mut atom_start: Option<i64> = None;
    let mut comment_exit = ParseCommentExit::RangeBoundary;
    let mut comment_resume_syntax = (state.in_comment.is_some() && from_char != point_min)
        .then(|| CommentResumeSyntax::from_parse_state(state.prev_syntax));

    let finish_atom = |state: &mut PartialParseState, atom_start: &mut Option<i64>| {
        if let Some(start) = atom_start.take() {
            state.finish_current_level_sexp(start);
        }
    };

    while idx < to_idx {
        let abs_char = from_char + idx;
        let pos1 = (abs_char + 1) as i64;
        let ch = chars.char_at(idx);
        let entry = match &flat_ascii {
            Some(flat) if (ch as u32) < 128 && prop_cache.covered_prop_free(abs_char) => {
                flat[ch as usize]
            }
            _ => effective_syntax_entry_for_abs_char(buf, table, ch, abs_char, &prop_cache),
        };
        let (class, flags) = (entry.class, entry.flags);
        let resumed_after = comment_resume_syntax.take();

        // GNU INC_FROM records `prev_from_syntax` for every position it steps
        // over; element 10 of the result reports it when the final position
        // holds a quote/comment-delimiter-first construct.  Specific arms below
        // reset it to Smax exactly where GNU does (2-char comment start, string
        // and comment terminators).
        state.prev_syntax = parse_prev_syntax_int(class, flags);

        if state.quoted {
            state.quoted = false;
            if state.in_comment.is_none() {
                idx += 1;
                continue;
            }
            // GNU enters `startincomment` before consulting `start_quoted`.
            // In a resumed comment the old quoted bit describes how the
            // previous range ended; it must not consume the first character
            // of this range (which may begin a two-character closer).
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
                ParseCommentState::Fence => {
                    if class == SyntaxClass::CommentFence {
                        idx += 1;
                        state.in_comment = None;
                        state.comment_or_string_start = None;
                        state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                        if commentstop == CommentStopMode::SyntaxTable {
                            break;
                        }
                        continue;
                    }
                    if escape_policy == CommentEndEscapePolicy::EscapeQuotesEnder
                        && matches!(class, SyntaxClass::Escape | SyntaxClass::CharQuote)
                    {
                        idx += 1;
                        if idx < to_idx {
                            idx += 1;
                            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
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
                    flavor,
                } => {
                    // GNU enters `forw_comment` at `forw_incomment` when an
                    // OLDSTATE supplies `prev_syntax`.  Resolve that
                    // boundary-spanning pair before interpreting CURRENT on
                    // its own, with ender-before-nested-opener precedence.
                    if let Some(previous) = resumed_after {
                        let boundary_marker = previous.marker_with(flags);
                        if boundary_marker.ender == Some(flavor) {
                            idx += 1;
                            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                            if state.close_comment_level(comment_depth, flavor)
                                == CommentEnderEffect::CommentClosed
                                && commentstop == CommentStopMode::SyntaxTable
                            {
                                break;
                            }
                            continue;
                        }
                        if flavor.nesting.is_nested() && boundary_marker.opener == Some(flavor) {
                            state.in_comment = Some(ParseCommentState::Syntax {
                                depth: comment_depth + 1,
                                flavor,
                            });
                            idx += 1;
                            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                            continue;
                        }
                    }

                    if class == SyntaxClass::EndComment
                        && CommentFlavor::single(flags) == flavor
                        && !resumed_after
                            .is_some_and(|previous| previous.quotes_single_ender(escape_policy))
                    {
                        idx += 1;
                        let effect = state.close_comment_level(comment_depth, flavor);
                        if effect == CommentEnderEffect::CommentClosed {
                            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                            if commentstop == CommentStopMode::SyntaxTable {
                                break;
                            }
                        }
                        continue;
                    }

                    // GNU applies a nested single-character opener before
                    // considering a two-character marker beginning at the
                    // same character.
                    let mut effective_comment_depth = comment_depth;
                    if flavor.nesting.is_nested()
                        && class == SyntaxClass::Comment
                        && CommentFlavor::single(flags) == flavor
                    {
                        effective_comment_depth += 1;
                        state.in_comment = Some(ParseCommentState::Syntax {
                            depth: effective_comment_depth,
                            flavor,
                        });
                    }

                    if escape_policy == CommentEndEscapePolicy::EscapeQuotesEnder
                        && matches!(class, SyntaxClass::Escape | SyntaxClass::CharQuote)
                    {
                        idx += 1;
                        if idx < to_idx {
                            idx += 1;
                            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                        } else {
                            state.quoted = true;
                        }
                        continue;
                    }

                    let pair = if (flags.contains(SyntaxFlags::COMMENT_END_FIRST)
                        || flags.contains(SyntaxFlags::COMMENT_START_FIRST))
                        && idx + 1 < to_idx
                    {
                        let (_, next_flags) = syntax_class_and_flags(
                            buf,
                            table,
                            chars.char_at(idx + 1),
                            abs_char + 1,
                            &prop_cache,
                        );
                        Some(CommentMarkerCapabilities::between(flags, next_flags))
                    } else {
                        None
                    };

                    if pair.is_some_and(|capabilities| capabilities.ender == Some(flavor)) {
                        idx += 2;
                        state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                        if state.close_comment_level(effective_comment_depth, flavor)
                            == CommentEnderEffect::CommentClosed
                            && commentstop == CommentStopMode::SyntaxTable
                        {
                            break;
                        }
                        continue;
                    }

                    if flavor.nesting.is_nested() {
                        if pair.is_some_and(|capabilities| capabilities.opener == Some(flavor)) {
                            state.in_comment = Some(ParseCommentState::Syntax {
                                depth: effective_comment_depth + 1,
                                flavor,
                            });
                            idx += 2;
                            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                            continue;
                        }
                    }

                    idx += 1;
                    continue;
                }
            }
        }

        // GNU recognizes an atomic two-character comment opener immediately
        // after advancing over its first character.  The pair therefore
        // overrides STOPBEFORE and every raw base class on that character.
        let opening_pair = if flags.contains(SyntaxFlags::COMMENT_START_FIRST) && idx + 1 < to_idx {
            let (_, next_flags) = syntax_class_and_flags(
                buf,
                table,
                chars.char_at(idx + 1),
                abs_char + 1,
                &prop_cache,
            );
            next_flags
                .contains(SyntaxFlags::COMMENT_START_SECOND)
                .then_some(CommentFlavor::two_char_start(flags, next_flags))
        } else {
            None
        };

        if let Some(flavor) = opening_pair {
            // GNU detects the pair inside `symstarted`: word-like raw syntax
            // keeps the preceding atom pending, while every other class first
            // takes the ordinary `symdone` path.
            if !matches!(
                class,
                SyntaxClass::Word
                    | SyntaxClass::Symbol
                    | SyntaxClass::Quote
                    | SyntaxClass::Escape
                    | SyntaxClass::CharQuote
            ) {
                finish_atom(&mut state, &mut atom_start);
            } else {
                // The jump from GNU's `symstarted` into `atcomment` bypasses
                // `symdone`; the interrupted atom is neither pending nor a
                // completed sexp after the comment scan.
                atom_start = None;
            }
            state.in_comment = Some(ParseCommentState::Syntax { depth: 1, flavor });
            state.comment_or_string_start = Some(pos1);
            idx += 2;
            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
            if commentstop != CommentStopMode::None {
                comment_exit = ParseCommentExit::StoppedAtEntry;
                break;
            }
            continue;
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

        // GNU `symstarted` continues a symbol/atom run across word, symbol,
        // quote AND escape/char-quote constituents; only some other syntax
        // class triggers `symdone' (which records the completed sexp).  Escape
        // and char-quote must therefore NOT finish the pending atom here —
        // doing so wrongly promoted an escaped run to a completed sexp (the
        // `\(a\)` / `a\(b\)c` divergence).
        if !matches!(
            class,
            SyntaxClass::Word
                | SyntaxClass::Symbol
                | SyntaxClass::Quote
                | SyntaxClass::Escape
                | SyntaxClass::CharQuote
        ) {
            finish_atom(&mut state, &mut atom_start);
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
                    flavor: CommentFlavor::single(flags),
                });
                state.comment_or_string_start = Some(pos1);
                idx += 1;
                if commentstop != CommentStopMode::None {
                    comment_exit = ParseCommentExit::StoppedAtEntry;
                    break;
                }
                continue;
            }
            SyntaxClass::CommentFence => {
                state.in_comment = Some(ParseCommentState::Fence);
                state.comment_or_string_start = Some(pos1);
                idx += 1;
                if commentstop != CommentStopMode::None {
                    comment_exit = ParseCommentExit::StoppedAtEntry;
                    break;
                }
                continue;
            }
            SyntaxClass::Escape | SyntaxClass::CharQuote => {
                // GNU `scan_sexps_forward` treats an escape/char-quote like the
                // start of a symbol run: it records `curlevel->last = prev_from`
                // (anchoring the atom at the escape) and skips the quoted char,
                // then continues scanning the symbol.  The atom is only promoted
                // to a *completed* sexp (`curlevel->prev`, our element 2) when a
                // non-symbol char ends the run via `symdone'.
                atom_start.get_or_insert(pos1);
                if idx + 1 < to_idx {
                    idx += 2;
                    continue;
                }
                // Escape with no following char: GNU jumps to `endquoted',
                // which sets state->quoted but BYPASSES `symdone', so the
                // pending atom is never recorded as a completed sexp.  Drop it
                // so element 2 stays nil (matching GNU's `\(a\)`).  prev_syntax
                // already holds the escape's syntax for element 10.
                atom_start = None;
                state.quoted = true;
                idx += 1;
                continue;
            }
            SyntaxClass::Word | SyntaxClass::Symbol => {
                atom_start.get_or_insert(pos1);
            }
            SyntaxClass::Quote => {
                // GNU `scan_sexps_forward`: a top-level Squote (expression
                // prefix, e.g. `'`, backquote, `,`, `#`) falls into the
                // `default' arm — "Ignore whitespace, punctuation, quote,
                // endcomment." — so it does NOT begin an atom and never
                // becomes element 2 (last-complete-sexp start).  But within an
                // in-progress symbol run Squote is a constituent (the inner
                // `symstarted` loop's `case Squote: break;`), so an already
                // started atom keeps running across it (the `finish_atom`
                // guard above already keeps Quote in the run).
            }
            SyntaxClass::Whitespace | SyntaxClass::EndComment => {}
            _ => {}
        }

        idx += 1;
    }

    if comment_exit == ParseCommentExit::RangeBoundary {
        state.finalize_incomplete_comment_syntax();
    }
    finish_atom(&mut state, &mut atom_start);

    (state, char_pos_to_lisp_i64(from_char + idx))
}

/// `(parse-partial-sexp FROM TO &optional TARGETDEPTH STOPBEFORE STATE COMMENTSTOP)`
/// Baseline parser-state implementation for structural Lisp motion/state queries.
pub(crate) fn builtin_parse_partial_sexp(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() < 2 || args.len() > 6 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
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
            LispCondition::ArgsOutOfRange,
            vec![Value::make_buffer(buf.id), args[0], args[1]],
        ));
    }
    let table = SyntaxTable::for_buffer(buf);
    let target_depth = match args.get(2) {
        Some(v) if !v.is_nil() => match v.kind() {
            ValueKind::Fixnum(n) => Some(n),
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), *v],
                ));
            }
        },
        _ => None,
    };
    let stop_before = args.get(3).is_some_and(|v| v.is_truthy());
    let oldstate = args.get(4).filter(|v| !v.is_nil());
    let commentstop = parse_commentstop_mode(args.get(5));
    let honor = parse_sexp_lookup_properties_enabled(eval);
    // Parse-span observability: one line per call when NEOMACS_SYNTAX_STATS_FILE
    // names a path. The per-keystroke syntax cost is O(parsed span); this is
    // the only way to see WHO parses from WHERE (syntax-ppss cache misses
    // parse from far back; a healthy cache parses tiny spans).
    // OnceLock, not a per-call env read: glibc getenv walks the environment
    // linearly, and this runs on EVERY parse-partial-sexp — a fontification
    // pass paid 185M Ir (58k calls) just asking for a debug knob.
    static SYNTAX_STATS_FILE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    if let Some(stats_path) = SYNTAX_STATS_FILE
        .get_or_init(|| std::env::var("NEOMACS_SYNTAX_STATS_FILE").ok())
        .as_deref()
        && !stats_path.is_empty()
        && let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&stats_path)
    {
        use std::io::Write as _;
        let _ = writeln!(f, "pps from={from} to={to} span={}", to - from);
        // One-shot Lisp backtrace for the first far-position parse: names the
        // machinery that drives whole-buffer reparse sweeps.
        static FAR_PARSE_TRACED: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if from > 100_000 && !FAR_PARSE_TRACED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            let _ = writeln!(
                f,
                "FAR-PARSE BACKTRACE:\n{}",
                eval.render_lisp_backtrace(40)
            );
        }
    }
    let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
    let escape_policy = CommentEndEscapePolicy::for_context(eval);
    let (state, stop_pos) = parse_state_from_range_with_options(
        buf,
        &table,
        from,
        to,
        target_depth,
        stop_before,
        oldstate,
        commentstop,
        props,
        escape_policy,
    );
    let current_id = buf.id;
    let stop_byte = lisp_pos_to_byte(buf, LispCharPos1::new(stop_pos));
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(current_id, stop_byte);
    Ok(state)
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
    let (syntax_chars, limit) = expect_skip_syntax_args("skip-syntax-forward", &args)?;
    let honor = parse_sexp_lookup_properties_enabled(eval);

    let (old_pt, limit_byte) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            buf.point_emacs_byte_pos(),
            limit.map(|raw| lisp_pos_to_byte(buf, LispCharPos1::new(raw)).get()),
        )
    };

    let new_pos = if honor {
        // GNU's skip_syntax propertizes LAZILY as the scan advances
        // (parse_sexp_propertize stops at charpos + 1). The Rust scanner
        // cannot run re-entrant Lisp mid-scan, so scan in bounded windows:
        // propertize one window ahead, scan within it, and continue only when
        // the whole window was consumed. Propertizing to the (often ZV)
        // limit up front made a no-op `(skip-syntax-forward " ")` after an
        // edit re-propertize the entire buffer tail: O(buffer) per call.
        const SYNTAX_PROPERTIZE_WINDOW_CHARS: usize = 500;
        let current_id = eval
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let mut stop;
        loop {
            let (window_end_char, window_end_byte, final_limit_byte) = {
                let buf = eval
                    .buffers
                    .get(current_id)
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                let from_char = buf.point_char_pos().get();
                let accessible_end_char = buf.accessible_char_region().end().get();
                let window_end_char =
                    (from_char + SYNTAX_PROPERTIZE_WINDOW_CHARS).min(accessible_end_char);
                let window_end_byte = buf
                    .char_pos_to_emacs_byte_pos_clamped(crate::buffer::CharPos0::new(
                        window_end_char,
                    ))
                    .get();
                let final_limit_byte =
                    limit_byte.unwrap_or_else(|| buf.accessible_emacs_byte_region().end().get());
                (window_end_char, window_end_byte, final_limit_byte)
            };
            let window_end_byte = window_end_byte.min(final_limit_byte);
            maybe_syntax_propertize_for_scan(eval, window_end_char.saturating_add(1))?;
            // Re-snapshot per window: propertizing ran Lisp, which may have
            // changed a category symbol's plist or the control variables.
            let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
            let buf = eval
                .buffers
                .get(current_id)
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            let table = SyntaxTable::for_buffer(buf);
            stop = skip_syntax_forward_with_options(
                buf,
                &table,
                &syntax_chars,
                Some(window_end_byte),
                props,
            );
            if stop < window_end_byte || window_end_byte >= final_limit_byte {
                break;
            }
            // The scan consumed the whole window: advance and continue.
            let _ = eval
                .buffers
                .goto_buffer_emacs_byte_pos(current_id, EmacsBytePos::new(window_end_byte));
        }
        stop
    } else {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let table = SyntaxTable::for_buffer(buf);
        skip_syntax_forward_with_options(
            buf,
            &table,
            &syntax_chars,
            limit_byte,
            SyntaxProperties::Ignore,
        )
    };
    let new_pos = EmacsBytePos::new(new_pos);

    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let _ = eval.buffers.goto_buffer_emacs_byte_pos(current_id, new_pos);

    // Return number of characters skipped (Emacs convention).
    let buf = eval
        .buffers
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
    let (syntax_chars, limit) = expect_skip_syntax_args("skip-syntax-backward", &args)?;
    let honor = parse_sexp_lookup_properties_enabled(eval);
    if honor {
        let target = eval
            .buffers
            .current_buffer()
            .map(|buf| buf.point_char_pos().get())
            .unwrap_or(0);
        if target > 0 {
            maybe_syntax_propertize_for_scan(eval, target)?;
        }
    }

    let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let table = SyntaxTable::for_buffer(buf);
    let limit = limit.map(|raw| lisp_pos_to_byte(buf, LispCharPos1::new(raw)).get());
    let new_pos = skip_syntax_backward_with_options(buf, &table, &syntax_chars, limit, props);

    let old_pt = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        .point_emacs_byte_pos();
    let new_pos = EmacsBytePos::new(new_pos);

    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let _ = eval.buffers.goto_buffer_emacs_byte_pos(current_id, new_pos);

    // Return negative number of characters skipped.
    let buf = eval
        .buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let chars_moved = if old_pt >= new_pos {
        -buffer_byte_char_delta(buf, new_pos.get(), old_pt.get())
    } else {
        buffer_byte_char_delta(buf, old_pt.get(), new_pos.get())
    };
    Ok(Value::fixnum(chars_moved))
}

fn expect_skip_syntax_args(caller: &str, args: &[Value]) -> Result<(String, Option<i64>), Flow> {
    if !(1..=2).contains(&args.len()) {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol(caller), Value::fixnum(args.len() as i64)],
        ));
    }
    let syntax_chars = syntax_runtime_string(&args[0])?;
    let limit = match args.get(1) {
        None => None,
        Some(value) if value.is_nil() => None,
        Some(value) => match value.kind() {
            ValueKind::Fixnum(n) => Some(n),
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), *value],
                ));
            }
        },
    };
    Ok((syntax_chars, limit))
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
