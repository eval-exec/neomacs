//! Syntax-class vocabulary and the buffer substrate's classification seam
//! (stage-3 C7).
//!
//! GNU's `enum syntaxcode` is plain data, so the enum lives with the buffer
//! substrate where the edit layer can name it after the split.  The
//! classification itself reads the live syntax char-table and stays in the
//! text layer; the substrate asks through one static fn-pointer table
//! installed at evaluator construction.  The table names only value/edit
//! types (host-free), with no dynamic dispatch and no new TLS.

use neovm_value::value::Value;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::sync::OnceLock;

use super::Buffer;

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

#[doc(hidden)]
pub const SYNTAX_CLASS_COUNT: usize = 16;
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
    pub(crate) fn from_gnu_discriminant(code: u8) -> Option<SyntaxClass> {
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
    #[doc(hidden)]
    pub fn from_plain_code(n: i64) -> Option<SyntaxClass> {
        let code = u8::try_from(n).ok()?;
        SyntaxClass::from_gnu_discriminant(code)
    }
}

/// Text-layer syntax queries the buffer substrate asks for.
#[doc(hidden)]
pub struct SyntaxSeams {
    pub(crate) table_for_buffer: fn(&Buffer) -> Value,
    pub(crate) char_syntax_code: fn(Value, u32) -> SyntaxClass,
}

static SEAMS: OnceLock<SyntaxSeams> = OnceLock::new();

#[doc(hidden)]
pub fn install(seams: SyntaxSeams) {
    let _ = SEAMS.set(seams);
}

pub(crate) fn seams() -> &'static SyntaxSeams {
    SEAMS
        .get()
        .expect("syntax substrate seams are installed at evaluator construction")
}

/// A buffer's syntax table as the edit layer sees it.
///
/// The table is an opaque char-table handle owned by the value layer;
/// classification is the text layer's installed query.  `Copy`, so the
/// skip-char loops hold it without a borrow.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct SyntaxTableView {
    chartable: Value,
}

impl SyntaxTableView {
    /// The syntax table `buf` currently uses (GNU `BVAR (buf, syntax_table)`).
    pub(crate) fn for_buffer(buf: &Buffer) -> Self {
        Self {
            chartable: (seams().table_for_buffer)(buf),
        }
    }

    /// The GNU syntax class of the Emacs character code `code`.
    pub(crate) fn char_syntax_code(self, code: u32) -> SyntaxClass {
        (seams().char_syntax_code)(self.chartable, code)
    }
}

#[doc(hidden)]
impl SyntaxSeams {
    pub const fn new(
        table_for_buffer: fn(&Buffer) -> Value,
        char_syntax_code: fn(Value, u32) -> SyntaxClass,
    ) -> Self {
        Self {
            table_for_buffer,
            char_syntax_code,
        }
    }
}
