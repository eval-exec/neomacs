//! GNU Emacs regex engine translated to Rust.
//!
//! This is a direct translation of GNU Emacs's `regex-emacs.c` — the same
//! algorithm, same bytecode format, same semantics.  The engine compiles
//! Emacs regex patterns to bytecode and executes them with syntax-table
//! awareness, backreference support, and POSIX backtracking.
//!
//! ## Architecture
//!
//! ```text
//! Pattern string
//!     ↓
//! regex_compile()     →  CompiledPattern (bytecode + fastmap)
//!     ↓
//! re_search()         →  Find match position (uses fastmap for skipping)
//!     ↓
//! re_match_internal() →  Execute bytecode against text (backtracking)
//!     ↓
//! MatchRegisters      →  Group start/end positions
//! ```
//!
//! ## Reference
//!
//! - GNU source: `src/regex-emacs.c` (5355 lines)
//! - GNU header: `src/regex-emacs.h`
//! - GNU search: `src/search.c` (3514 lines)

use rustc_hash::FxHashMap;
use std::collections::HashSet;

use crate::emacs_core::{emacs_char, syntax::SyntaxClass, value::Value};
use regex_automata::util::prefilter::Prefilter as RaPrefilter;
use regex_automata::{MatchKind, Span};
use smallvec::SmallVec;

const INLINE_REGEX_REGISTERS: usize = 8;
type RegisterScratch = SmallVec<[Option<usize>; INLINE_REGEX_REGISTERS]>;

// ---------------------------------------------------------------------------
// Phase 1: Opcodes and Data Structures
// ---------------------------------------------------------------------------

/// Bytecode opcodes for the compiled regex pattern.
///
/// Translated from `re_opcode_t` enum in regex-emacs.c (lines 202-337).
/// Each opcode may be followed by argument bytes in the bytecode buffer.
/// Bytecode opcodes for the compiled regex pattern.
///
/// **Strict GNU parity**: the numeric values here mirror
/// `enum re_opcode_t` in GNU `src/regex-emacs.c:202-337` exactly.
/// A compiled pattern emitted by our compiler is byte-compatible with
/// the same pattern emitted by GNU's compiler — every opcode occupies
/// the same numeric slot, so bytecode dumps can be compared directly
/// during debugging and future external tools can read either
/// without a translation layer.
///
/// The one-byte form we emit via `<op> as u8` is the same as GNU's
/// `BUF_COMPILED[pc++]` byte. **Do not reorder without updating the
/// GNU reference at the top of this file.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum RegexOp {
    /// No operation (padding/alignment). GNU `no_op` = 0.
    NoOp = 0,

    /// Succeed immediately — no more backtracking. GNU `succeed` = 1.
    Succeed = 1,

    /// Match N exact bytes.  Followed by one byte N, then N literal
    /// bytes. GNU `exactn` = 2.
    Exactn = 2,

    /// Match any character (except newline in some modes).
    /// GNU `anychar` = 3.
    AnyChar = 3,

    /// Match character in bitmap set. Same byte layout as GNU
    /// `charset` = 4:
    /// - 1 byte: bitmap length (low 7 bits), high bit = has range table
    /// - N bytes: bitmap (bit per character, low-bit-first)
    /// - Optional range table for multibyte characters
    Charset = 4,

    /// Match character NOT in bitmap set.  Same format as `Charset`.
    /// GNU `charset_not` = 5.
    CharsetNot = 5,

    /// Start remembering text for group N.  Followed by 1 byte: group
    /// number. GNU `start_memory` = 6.
    StartMemory = 6,

    /// Stop remembering text for group N.  Followed by 1 byte: group
    /// number. GNU `stop_memory` = 7.
    StopMemory = 7,

    /// Match duplicate of group N (backreference \N).  Followed by
    /// 1 byte: group number. GNU `duplicate` = 8.
    Duplicate = 8,

    /// Fail unless at beginning of line (^). GNU `begline` = 9.
    BegLine = 9,

    /// Fail unless at end of line ($). GNU `endline` = 10.
    EndLine = 10,

    /// Succeed at beginning of buffer/string. `` \` ``.
    /// GNU `begbuf` = 11.
    BegBuf = 11,

    /// Succeed at end of buffer/string. `\'`.
    /// GNU `endbuf` = 12.
    EndBuf = 12,

    /// Unconditional jump.  Followed by 2-byte signed offset.
    /// GNU `jump` = 13.
    Jump = 13,

    /// Push failure point, then continue.  Followed by 2-byte signed
    /// offset. GNU `on_failure_jump` = 14.
    OnFailureJump = 14,

    /// Like `OnFailureJump` but doesn't restore string position on
    /// failure. GNU `on_failure_keep_string_jump` = 15.
    OnFailureKeepStringJump = 15,

    /// Like `OnFailureJump` but detects infinite empty-match loops.
    /// GNU `on_failure_jump_loop` = 16.
    OnFailureJumpLoop = 16,

    /// Like `OnFailureJumpLoop` but for non-greedy operators.
    /// GNU `on_failure_jump_nastyloop` = 17.
    OnFailureJumpNastyloop = 17,

    /// Smart jump for greedy `*` and `+`.  Analyzes loop to optimize.
    /// GNU `on_failure_jump_smart` = 18.
    OnFailureJumpSmart = 18,

    /// Match N times then jump on failure.  Followed by 2-byte offset
    /// + 2-byte count. GNU `succeed_n` = 19.
    SucceedN = 19,

    /// Jump N times then fail.  Followed by 2-byte offset + 2-byte
    /// count. GNU `jump_n` = 20.
    JumpN = 20,

    /// Set counter at offset.  Followed by 2-byte offset + 2-byte
    /// value. GNU `set_number_at` = 21.
    SetNumberAt = 21,

    /// Succeed at word beginning (syntax-table aware).  `\<`.
    /// GNU `wordbeg` = 22.
    WordBeg = 22,

    /// Succeed at word end (syntax-table aware).  `\>`.
    /// GNU `wordend` = 23.
    WordEnd = 23,

    /// Succeed at word boundary (syntax-table aware).  `\b`.
    /// GNU `wordbound` = 24.
    WordBound = 24,

    /// Succeed at non-word boundary (syntax-table aware).  `\B`.
    /// GNU `notwordbound` = 25.
    NotWordBound = 25,

    /// Succeed at symbol beginning (syntax-table aware).  `\_<`.
    /// GNU `symbeg` = 26.
    SymBeg = 26,

    /// Succeed at symbol end (syntax-table aware).  `\_>`.
    /// GNU `symend` = 27.
    SymEnd = 27,

    /// Match character with syntax class C.  Followed by 1 byte:
    /// syntax code.  `\sC`. GNU `syntaxspec` = 28.
    SyntaxSpec = 28,

    /// Match character without syntax class C.  Followed by 1 byte.
    /// `\SC`. GNU `notsyntaxspec` = 29.
    NotSyntaxSpec = 29,

    /// Succeed if at point.  `\=`. GNU `at_dot` = 30.
    AtDot = 30,

    /// Match character with category C.  Followed by 1 byte: category
    /// code.  `\cC`. GNU `categoryspec` = 31.
    CategorySpec = 31,

    /// Match character without category C.  Followed by 1 byte.
    /// `\CC`. GNU `notcategoryspec` = 32.
    NotCategorySpec = 32,

    /// Match one character whose syntax class is in a set.  Followed by a
    /// 2-byte little-endian bitmask (bit `1 << class`).  This is a
    /// neomacs-only fusion op with NO GNU counterpart: the post-compile
    /// peephole `fuse_syntaxspec_alternations` collapses an alternation of
    /// single positive `\sC` / `\w` branches (e.g. `\w\|\s_`) into one op,
    /// replacing per-branch `on_failure_jump` backtracking and repeated
    /// `char_syntax` lookups with a single lookup + mask test.  Because a
    /// character has exactly one syntax class, the fused branches are
    /// mutually exclusive, so this is a semantics-preserving rewrite.
    SyntaxSpecSet = 33,

    /// Terminal op of a POSIX pattern: runs the longest-match bookkeeping
    /// that non-POSIX patterns skip via their trailing `Succeed`
    /// (regex-emacs.c:4272-4344 — the "fell off the end" path, reified as
    /// an opcode so a sealed matcher never needs the per-op end-of-buffer
    /// check).  neomacs-only, like `SyntaxSpecSet`.
    PosixEnd = 34,
}

impl RegexOp {
    /// Convert a byte to an opcode.  Returns None for invalid bytes.
    fn from_byte(b: u8) -> Option<Self> {
        if b <= 34 {
            // SAFETY: all values 0-33 are valid enum variants
            Some(unsafe { std::mem::transmute::<u8, RegexOp>(b) })
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Compiled Pattern
// ---------------------------------------------------------------------------

/// A compiled regex pattern — the output of `regex_compile()`.
///
/// Mirrors GNU's `struct re_pattern_buffer` from regex-emacs.h.
#[derive(Clone)]
pub(crate) struct CompiledPattern {
    /// Bytecode buffer.
    pub buffer: Vec<u8>,

    /// Number of subexpressions (groups).
    pub re_nsub: usize,

    /// Fast rejection map: fastmap[c] is true if the pattern can start
    /// with byte c.  Used by `re_search` to skip non-matching positions.
    pub fastmap: [bool; 256],

    /// Whether the fastmap is valid (needs recomputation after compile).
    pub fastmap_accurate: bool,

    /// True if the pattern was compiled for POSIX backtracking.
    pub posix: bool,

    /// True if the source regexp string was multibyte.
    pub multibyte: bool,

    /// True if the current search target is multibyte.
    pub target_multibyte: bool,

    /// True if the pattern can match the empty string.
    pub can_be_null: bool,

    /// True if matching this pattern consults the syntax table at MATCH
    /// time (`\b \B \< \> \_< \_> \w \W \sC \SC` and POSIX classes).
    ///
    /// This is informational only: the matcher resolves per-character
    /// syntax through the `SyntaxLookup` passed to each `re_match` call,
    /// so the compiled bytecode itself stays table-independent.  In
    /// particular it does NOT disable the search fastmap — GNU gates the
    /// fastmap skip solely on `fastmap_accurate && !can_be_null`
    /// (regex-emacs.c:3483 `if (fastmap && startpos < total_size &&
    /// !bufp->can_be_null)`), and syntax-dependent *leading* atoms
    /// (`\w`, `\sC`) set `can_be_null` via `analyze_first` instead.
    pub uses_syntax: bool,

    /// True if the compiled artifacts hardcode syntax-table CONTENT, so
    /// the pattern is only valid for the syntax table it was compiled
    /// against and the compile cache must key such entries by table.
    ///
    /// Mirrors GNU `re_pattern_buffer.used_syntax` (regex-emacs.h:112),
    /// which is set only for `[[:space:]]` / `[[:word:]]` inside a
    /// charset (regex-emacs.c:2096-2101: "In most cases the matching
    /// rule for char classes only uses the syntax table for multibyte
    /// chars ... SPACE and WORD are the two exceptions") and makes
    /// `search.c:compile_pattern` key the cache entry by
    /// `BVAR (current_buffer, syntax_table)`.
    ///
    /// In neomacs the charset MATCH path re-derives ASCII class
    /// membership per call from the active table, so only the FASTMAP
    /// bakes table content (see `compile_fastmap`); the caching rule is
    /// the same.
    pub used_syntax: bool,

    /// Character translation table for case-folding.
    ///
    /// GNU stores the active case-canon char-table in `re_pattern_buffer.translate`.
    /// Keep the same shape at the matcher boundary: literal compilation and input
    /// matching both call through this translator, with a 256-entry byte fast path
    /// only for fastmap/bitmap operations.
    pub translate: Option<CaseTranslation>,

    /// Multibyte (non-ASCII) character ranges for Charset/CharsetNot opcodes.
    /// Key = bytecode position of the Charset/CharsetNot opcode.
    /// Value = list of inclusive (start_char, end_char) character ranges.
    pub multibyte_charsets: FxHashMap<usize, Vec<(char, char)>>,

    /// Per-charset POSIX character class flags.
    ///
    /// GNU stores POSIX classes in the charset range-table bits
    /// (`regex-emacs.c:re_wctype_to_bit`) and checks `re_iswctype`
    /// while executing the charset.  The ASCII bitmap remains the fast path,
    /// but these bits preserve the runtime predicate for multibyte characters
    /// and for syntax-table-sensitive classes such as `word` and `space`.
    /// Fx-hashed: probed per charset op while matching (57K probes per
    /// five org font-lock operations); SipHash on a `usize` key was 1.7M Ir/op.
    pub charset_class_bits: FxHashMap<usize, u32>,

    /// True if the pattern used a non-greedy optional `??`.  Its
    /// `OnFailureKeepStringJump` does NOT restore the string position on
    /// backtrack, so the fallback resumes at the failure position rather
    /// than the split position — semantics a Pike VM (which explores the
    /// jump target from the CURRENT position) cannot reproduce.  The
    /// smart-loop `OnFailureKeepStringJump` is safe (no-rewind ≡ rewind
    /// under its mutual-exclusivity precondition), so only this `??`-sourced
    /// form forces Pike ineligibility.  Set at the sole `??` emit site.
    pub has_nongreedy_optional: bool,

    /// True if this pattern is eligible for the non-backtracking Pike VM
    /// fast path (see [`pike_match`]).  Computed once at compile time by
    /// [`compute_pike_eligible`]: the bytecode must contain no
    /// backreference (`Duplicate`), no interval-counter op
    /// (`SucceedN`/`JumpN`/`SetNumberAt` from `\{n,m\}`), and the pattern
    /// must not be POSIX-longest — every remaining op is simulated
    /// byte-exactly by the Pike VM.  Ineligible patterns fall back to the
    /// backtracker with identical output.
    pub pike_eligible: bool,

    /// True once [`validate_sealed_buffer`] proved every opcode byte valid,
    /// every operand span in-bounds, and every jump target on an op
    /// boundary — the backtracker then dispatches with unchecked fetches
    /// (the seal-then-trust protocol the bytecode VM uses for `seal_ops`).
    /// False (e.g. a hand-assembled test buffer) keeps the checked loop.
    pub buffer_sealed: bool,

    /// Pike-only view of the bytecode: identical to `buffer` except that
    /// smart keep-string loops (`OnFailureKeepStringJump` whose jump-back
    /// targets the loop body) are de-optimized back to the equivalent
    /// REWIND loop (`OnFailureJump` with the jump-back targeting the split).
    ///
    /// Keep-string jumps resume the loop EXIT at the position where the body
    /// fails, which a Pike VM (each thread pinned to the current position)
    /// cannot represent.  The rewind form re-evaluates the exit at every
    /// position, which the Pike VM handles correctly and which is
    /// semantically identical (the compiler only applies the keep-string
    /// optimization when body and continuation are mutually exclusive).
    /// The transform preserves byte LENGTH and all opcode positions, so the
    /// position-keyed charset side tables stay valid.  `None` when the
    /// pattern is ineligible or has no keep-string loop that needs the
    /// rewrite (in which case the Pike VM reads `buffer` directly).
    pub pike_buffer: Option<Vec<u8>>,

    /// Multi-literal SIMD prefilter that AUGMENTS (never replaces) the
    /// backtracker's search skip loop.  When `Some`, `re_search`'s forward
    /// path uses `regex-automata`'s `Prefilter` (memchr / Teddy /
    /// Aho-Corasick, auto-selected) to jump straight to the next position
    /// whose text provably contains a required match literal, instead of the
    /// single-byte fastmap scan.  The backtracker still verifies every
    /// candidate, so the ONLY correctness requirement is that the literals be
    /// SOUND: every match must contain one of them at `LiteralPrefilter.offset`
    /// bytes from the match start (see `build_literal_prefilter`).  `None` when
    /// no such literal set could be proven (fall back to the fastmap — always
    /// correct).  Never set for case-fold patterns or patterns whose only
    /// required literals are single bytes (the fastmap's memchr already handles
    /// those).
    pub prefilter: Option<LiteralPrefilter>,
}

/// A sound multi-literal prefilter for a compiled pattern.  See
/// [`CompiledPattern::prefilter`] and [`build_literal_prefilter`].
#[derive(Clone)]
pub(crate) struct LiteralPrefilter {
    /// `regex-automata`'s SIMD literal scanner over the needle set.
    pf: RaPrefilter,
    /// Byte offset of the found literal within the match: a literal found at
    /// text index `j` means a candidate match may start at `j - offset`.
    /// Currently always `0` (see `build_literal_prefilter`).
    offset: usize,
}

#[derive(Clone, Debug)]
pub struct CaseTranslation {
    /// Translations for codes 0..256, filled ON DEMAND when backed by a
    /// char-table (see [`CaseTranslation::from_char_table`]).
    /// [`CASE_TRANSLATION_UNFILLED`] marks a slot not yet computed.
    byte: [std::cell::Cell<u32>; 256],
    table: Option<crate::emacs_core::value::Value>,
}

/// Sentinel for a `CaseTranslation::byte` slot that has not been computed.
///
/// Safe as a sentinel because a translation is always a valid character code
/// (`<= MAX_CHAR`), which `u32::MAX` is not. Named rather than a bare literal
/// because the array is otherwise indistinguishable from a filled one -- the
/// same class of mistake as the derived `Default` that let `IntervalTree`
/// report an empty memo as valid.
const CASE_TRANSLATION_UNFILLED: u32 = u32::MAX;

impl CaseTranslation {
    pub(crate) fn standard() -> Self {
        // The canonicalization of bytes 0..256 is a constant
        // (`downcase_char_code_emacs_compat`), but a case-insensitive regex is
        // recompiled on every `re-search-forward`, and recomputing this table
        // via Unicode case folding each time was ~15% of a search.  GNU keeps
        // its case-canon table precomputed; build it once per thread and copy.
        thread_local! {
            static STANDARD_BYTE: [u32; 256] = {
                let mut byte = [0u32; 256];
                for i in 0..=255u32 {
                    byte[i as usize] = CaseTranslation::canonicalize_char(i);
                }
                byte
            };
        }
        let byte = STANDARD_BYTE.with(|b| std::array::from_fn(|i| std::cell::Cell::new(b[i])));
        Self { byte, table: None }
    }

    /// A translation backed by a buffer's case-canon char-table.
    ///
    /// Slots are filled ON DEMAND. Eagerly translating all 256 codes here cost
    /// 256 `ct_lookup` calls per construction, and `buffer_search_translation`
    /// builds one of these on EVERY case-folded `looking_at` /
    /// `re-search-forward` -- which font-lock issues thousands of times, each
    /// typically examining a handful of characters. A profile put
    /// `ct_lookup` at 4.29% of a font-lock scroll with this its largest
    /// single consumer.
    ///
    /// Filling on demand is strictly less work at every match length (k
    /// distinct codes examined costs k lookups, never 256) and is exactly as
    /// sound as the eager fill: the memo lives and dies with this instance, so
    /// it cannot outlive the table the way an identity-keyed cache could.
    /// `standard()` above already applied the same lesson to the constant
    /// table; this path was simply missed.
    pub(crate) fn from_char_table(table: crate::emacs_core::value::Value) -> Self {
        Self {
            byte: std::array::from_fn(|_| std::cell::Cell::new(CASE_TRANSLATION_UNFILLED)),
            table: Some(table),
        }
    }

    /// Translation for a code below 256, computing and memoizing it on a miss.
    #[inline]
    fn byte_slot(&self, c: usize) -> Option<u32> {
        let slot = self.byte.get(c)?;
        let cached = slot.get();
        if cached != CASE_TRANSLATION_UNFILLED {
            return Some(cached);
        }
        let translated = match self.table {
            Some(table) => crate::emacs_core::chartable::translate_char(&table, c as i64) as u32,
            None => Self::canonicalize_char(c as u32),
        };
        slot.set(translated);
        Some(translated)
    }

    pub(crate) fn cache_key(&self) -> usize {
        self.table.map_or(0, |table| table.bits())
    }

    pub(crate) fn translate(&self, c: u32) -> u32 {
        if let Some(translated) = self.byte_slot(c as usize) {
            return translated;
        }
        if let Some(table) = self.table {
            return crate::emacs_core::chartable::translate_char(&table, c as i64) as u32;
        }
        Self::canonicalize_char(c)
    }

    fn translate_byte(&self, c: u8) -> u8 {
        self.byte_slot(c as usize).unwrap_or(c as u32) as u8
    }

    fn canonicalize_char(c: u32) -> u32 {
        crate::emacs_core::builtins::downcase_char_code_emacs_compat(c as i64) as u32
    }
}

const CHARSET_CLASS_BIT_ALNUM: u32 = 1 << 0;
const CHARSET_CLASS_BIT_ALPHA: u32 = 1 << 1;
const CHARSET_CLASS_BIT_BLANK: u32 = 1 << 2;
const CHARSET_CLASS_BIT_CNTRL: u32 = 1 << 3;
const CHARSET_CLASS_BIT_DIGIT: u32 = 1 << 4;
const CHARSET_CLASS_BIT_GRAPH: u32 = 1 << 5;
const CHARSET_CLASS_BIT_LOWER: u32 = 1 << 6;
const CHARSET_CLASS_BIT_PRINT: u32 = 1 << 7;
const CHARSET_CLASS_BIT_PUNCT: u32 = 1 << 8;
const CHARSET_CLASS_BIT_SPACE: u32 = 1 << 9;
const CHARSET_CLASS_BIT_UPPER: u32 = 1 << 10;
const CHARSET_CLASS_BIT_XDIGIT: u32 = 1 << 11;
const CHARSET_CLASS_BIT_ASCII: u32 = 1 << 12;
const CHARSET_CLASS_BIT_WORD: u32 = 1 << 13;
const CHARSET_CLASS_BIT_NONASCII: u32 = 1 << 14;
const CHARSET_CLASS_BIT_UNIBYTE: u32 = 1 << 15;
const CHARSET_CLASS_BIT_MULTIBYTE: u32 = 1 << 16;

impl CompiledPattern {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(256),
            re_nsub: 0,
            fastmap: [false; 256],
            fastmap_accurate: false,
            posix: false,
            multibyte: true,
            target_multibyte: true,
            can_be_null: false,
            uses_syntax: false,
            used_syntax: false,
            translate: None,
            multibyte_charsets: FxHashMap::default(),
            charset_class_bits: FxHashMap::default(),
            has_nongreedy_optional: false,
            pike_eligible: false,
            buffer_sealed: false,
            pike_buffer: None,
            prefilter: None,
        }
    }

    /// Splice `bytes` into the bytecode at `at`, keeping the
    /// opcode-position-keyed side tables consistent.
    ///
    /// `multibyte_charsets` and `charset_class_bits` are keyed by the byte
    /// position of each `Charset`/`CharsetNot` opcode.  GNU stores the
    /// charset range table inline in the bytecode, so moving the opcode also
    /// moves its range table; neomacs keeps the range table in a side map, so
    /// every byte inserted *before* an already-emitted charset opcode must
    /// re-key that opcode's entry by the inserted byte count.  Failing to do
    /// so orphans the range table and the charset silently matches no
    /// non-ASCII character (the alternation/quantifier splices that insert an
    /// `on_failure_jump` ahead of a `[é]` are exactly this case).
    fn splice_bytecode(&mut self, at: usize, bytes: &[u8]) {
        let count = bytes.len();
        self.buffer.splice(at..at, bytes.iter().copied());
        if count != 0 {
            shift_charset_keys(&mut self.multibyte_charsets, at, count as isize);
            shift_charset_keys(&mut self.charset_class_bits, at, count as isize);
        }
    }

    /// Move the charset side-table keys for opcodes that lived in
    /// `[from..from_end)` to start at `to` instead.  Used by the non-greedy
    /// `*?`/`+?` quantifier compilers, which `truncate` the body bytes and
    /// re-`extend` them at a new offset.
    fn relocate_charset_keys(&mut self, from: usize, from_end: usize, to: usize) {
        relocate_charset_keys(&mut self.multibyte_charsets, from, from_end, to);
        relocate_charset_keys(&mut self.charset_class_bits, from, from_end, to);
    }

    /// Drop charset side-table keys for opcodes at or beyond `at`, mirroring a
    /// `buffer.truncate(at)`.
    fn truncate_charset_keys(&mut self, at: usize) {
        self.multibyte_charsets.retain(|&pos, _| pos < at);
        self.charset_class_bits.retain(|&pos, _| pos < at);
    }

    /// Copy the charset side-table entries for opcodes in `[from..from_end)`
    /// to the same relative positions starting at `to`.  Used by the greedy
    /// `P+` → `PP*` body duplication, which copies already-emitted body
    /// bytes to the end of the buffer.
    fn clone_charset_keys(&mut self, from: usize, from_end: usize, to: usize) {
        clone_charset_keys(&mut self.multibyte_charsets, from, from_end, to);
        clone_charset_keys(&mut self.charset_class_bits, from, from_end, to);
    }
}

/// Copy entries keyed in `[from..from_end)` to `pos - from + to`, keeping the
/// originals.
fn clone_charset_keys<V: Clone>(
    map: &mut FxHashMap<usize, V>,
    from: usize,
    from_end: usize,
    to: usize,
) {
    if map.is_empty() {
        return;
    }
    let cloned: Vec<(usize, V)> = map
        .iter()
        .filter(|&(&pos, _)| pos >= from && pos < from_end)
        .map(|(&pos, v)| (pos - from + to, v.clone()))
        .collect();
    for (pos, v) in cloned {
        map.insert(pos, v);
    }
}

/// Shift every key `>= at` in an opcode-position-keyed map by `delta` bytes.
fn shift_charset_keys<V>(map: &mut FxHashMap<usize, V>, at: usize, delta: isize) {
    if map.is_empty() || delta == 0 {
        return;
    }
    let moved: Vec<(usize, V)> = map
        .extract_if(|&pos, _| pos >= at)
        .map(|(pos, v)| ((pos as isize + delta) as usize, v))
        .collect();
    for (pos, v) in moved {
        map.insert(pos, v);
    }
}

/// Re-key opcode entries that lived in `[from..from_end)` so they start at `to`.
fn relocate_charset_keys<V>(
    map: &mut FxHashMap<usize, V>,
    from: usize,
    from_end: usize,
    to: usize,
) {
    if map.is_empty() {
        return;
    }
    let moved: Vec<(usize, V)> = map
        .extract_if(|&pos, _| pos >= from && pos < from_end)
        .map(|(pos, v)| (pos - from + to, v))
        .collect();
    // Drop any remaining keys at or beyond `from` that were not relocated
    // (they belonged to bytes that the caller truncated away).
    map.retain(|&pos, _| pos < from);
    for (pos, v) in moved {
        map.insert(pos, v);
    }
}

// ---------------------------------------------------------------------------
// Match Registers
// ---------------------------------------------------------------------------

/// Match result — stores group start/end positions.
///
/// Mirrors GNU's `struct re_registers` from regex-emacs.h.
#[derive(Clone, Debug)]
pub(crate) struct MatchRegisters {
    /// Start positions for each group (group 0 = full match).
    /// -1 means group did not participate in match.
    /// Inline up to 8 groups: GNU reuses one global `search_regs`, so a heap
    /// Vec pair per match would be allocator churn GNU never pays.
    pub start: SmallVec<[i64; 8]>,

    /// End positions for each group.
    pub end: SmallVec<[i64; 8]>,
}

impl MatchRegisters {
    pub fn new(num_groups: usize) -> Self {
        Self {
            start: SmallVec::from_elem(-1, num_groups),
            end: SmallVec::from_elem(-1, num_groups),
        }
    }

    pub fn num_regs(&self) -> usize {
        self.start.len()
    }
}

// ---------------------------------------------------------------------------
// Failure Stack (for backtracking)
// ---------------------------------------------------------------------------

/// A failure point (choice point) on the backtracking stack.
///
/// Mirrors GNU `PUSH_FAILURE_POINT` (regex-emacs.c:1080): a frame is just
/// the resume positions plus a mark into the shared undo log.  Register
/// and counter state is NOT snapshotted here — GNU's protocol delta-saves
/// a register when `start_memory` modifies it (`PUSH_FAILURE_REG`) and a
/// counter when it is written (`PUSH_NUMBER`); `POP_FAILURE_POINT`
/// replays the undo log down to the frame's mark.  The previous
/// implementation rebuilt a full register snapshot and cloned the counter
/// table on EVERY choice point, which was the dominant cost of
/// backtracking-heavy searches.
#[derive(Clone, Copy, Debug)]
struct FailFrame {
    /// `undo.len()` at push time — POP replays undo entries above this.
    undo_mark: usize,

    /// Opcode that created this choice point.  GNU stores this as
    /// `FAILURE_PAT`; empty-loop detection compares this identity, not the
    /// address at which matching will resume.
    origin: FailureOrigin,

    /// Position in the bytecode to resume at when this choice is popped.
    resume: FailureResume,

    /// What popping this choice does to the input cursor.
    input: FailureInput,
}

/// Bytecode identity used only for GNU's empty-loop cycle detection.
///
/// This is deliberately a different type from [`FailureResume`]: two distinct
/// choice-point opcodes may jump to the same continuation, which must not make
/// them the same loop for `CHECK_INFINITE_LOOP`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FailureOrigin(usize);

/// Bytecode continuation restored by a backtrack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FailureResume(usize);

/// Input-cursor policy stored in a GNU regexp failure point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FailureInput {
    Restore(usize),
    KeepCurrent,
}

/// One delta-undo entry (GNU `PUSH_FAILURE_REG` / `PUSH_NUMBER`).
#[derive(Clone, Copy, Debug)]
enum FailUndo {
    /// Register `idx`'s (start, end) before a `start_memory` overwrote it.
    Reg {
        idx: usize,
        start: Option<usize>,
        end: Option<usize>,
    },
    /// Counter at bytecode position `pos` before it was rewritten.
    Counter { pos: usize, val: u16 },
}

/// GNU `emacs_re_max_failures` (regex-emacs.c:893).
const EMACS_RE_MAX_FAILURES: usize = 40_000;
/// GNU `TYPICAL_FAILURE_SIZE` (regex-emacs.c:1110): estimated stack slots
/// per failure point; the fail stack refuses to grow beyond
/// `emacs_re_max_failures * TYPICAL_FAILURE_SIZE` slots.
const TYPICAL_FAILURE_SIZE: usize = 20;
/// Our frames and undo entries each stand for one 3-slot GNU stack item
/// (frame = [prev_frame, str, pat]; reg = [start, end, num]; counter =
/// [val, ptr, -1]), so the GNU slot budget divides by 3.
const FAIL_STACK_ENTRY_LIMIT: usize = EMACS_RE_MAX_FAILURES * TYPICAL_FAILURE_SIZE / 3;

/// Reusable per-thread matcher state.  `re_search` runs `re_match` once
/// per fastmap candidate; allocating the fail stack, register vectors and
/// counter table afresh for every candidate was measured as a major share
/// of search time.  Cleared (not deallocated) on each entry.
#[derive(Default)]
struct MatchScratch {
    frames: Vec<FailFrame>,
    undo: Vec<FailUndo>,
    regstart: RegisterScratch,
    regend: RegisterScratch,
    best_regstart: RegisterScratch,
    best_regend: RegisterScratch,
    /// Live interval-counter overrides, keyed by bytecode position of the
    /// 2-byte counter field.  An association list: patterns rarely have
    /// more than a couple of `\{n,m\}` counters.
    counters: SmallVec<[(usize, u16); 4]>,
}

thread_local! {
    static MATCH_SCRATCH: std::cell::RefCell<MatchScratch> =
        std::cell::RefCell::new(MatchScratch::default());

    /// Set when a match aborts on the GNU fail-stack limit.  `re_search`
    /// bails out of its candidate loop when it sees the flag; the
    /// front-end (`regex/mod.rs`) promotes a `None`-with-flag result into the
    /// GNU error `"Stack overflow in regexp matcher"` (search.c:78
    /// `matcher_overflow`, reached via `re_match_2_internal` returning
    /// -2).  Same TLS-flag idiom as the quit poll.
    static MATCHER_OVERFLOW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// GNU `matcher_overflow` message (search.c:80).
pub(crate) const MATCHER_OVERFLOW_MESSAGE: &str = "Stack overflow in regexp matcher";

fn set_matcher_overflow() {
    MATCHER_OVERFLOW.with(|flag| flag.set(true));
}

fn matcher_overflow_pending() -> bool {
    MATCHER_OVERFLOW.with(|flag| flag.get())
}

fn clear_matcher_overflow() {
    MATCHER_OVERFLOW.with(|flag| flag.set(false));
}

/// Read-and-clear the overflow flag.  The front-end calls this after a
/// failed search to distinguish "no match" from "aborted on the
/// fail-stack limit" (which GNU signals as an error).
pub(crate) fn take_matcher_overflow() -> bool {
    MATCHER_OVERFLOW.with(|flag| flag.replace(false))
}

// Engine routing (production): the backtracker runs by DEFAULT — it is
// faster on the well-behaved patterns that dominate real workloads
// (font-lock, search).  For a `pike_eligible` pattern the backtracker runs
// under a LINEAR backtracking-step budget; if a single match blows past it
// (catastrophic backtracking, e.g. `a*a*b`), it sets `PIKE_FALLBACK` and the
// caller re-runs the match on the linear, byte-exact Pike VM.  This keeps
// the common-case speed while eliminating catastrophic blow-up.
//
// A test/fuzz-only typed override lets differential checks pin one engine and
// compare it with another (bypassing the budget heuristic). One enum makes
// contradictory "force both engines" states unrepresentable.
thread_local! {
    /// Set by the budgeted backtracker when it gives up so the caller
    /// re-runs the match on the Pike VM.
    static PIKE_FALLBACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
thread_local! {
    pub(crate) static PIKE_FALLBACK_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(crate) fn pike_fallback_count() -> u64 {
    PIKE_FALLBACK_COUNT.with(|c| c.get())
}

fn set_pike_fallback() {
    PIKE_FALLBACK.with(|f| f.set(true));
    #[cfg(test)]
    PIKE_FALLBACK_COUNT.with(|c| c.set(c.get() + 1));
}

/// Read-and-clear the Pike-fallback flag.
fn take_pike_fallback() -> bool {
    PIKE_FALLBACK.with(|f| f.replace(false))
}

#[cfg(any(test, feature = "fuzzing"))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum RegexEngineOverride {
    #[default]
    Production,
    Backtracker,
    PikeVm,
}

#[cfg(any(test, feature = "fuzzing"))]
thread_local! {
    static REGEX_ENGINE_OVERRIDE: std::cell::Cell<RegexEngineOverride> =
        const { std::cell::Cell::new(RegexEngineOverride::Production) };
}

#[cfg(any(test, feature = "fuzzing"))]
#[inline]
fn force_backtrack() -> bool {
    REGEX_ENGINE_OVERRIDE.with(|slot| slot.get() == RegexEngineOverride::Backtracker)
}

#[cfg(not(any(test, feature = "fuzzing")))]
#[inline]
fn force_backtrack() -> bool {
    false
}

#[cfg(any(test, feature = "fuzzing"))]
#[inline]
fn force_pike() -> bool {
    REGEX_ENGINE_OVERRIDE.with(|slot| slot.get() == RegexEngineOverride::PikeVm)
}

#[cfg(not(any(test, feature = "fuzzing")))]
#[inline]
fn force_pike() -> bool {
    false
}

#[cfg(any(test, feature = "fuzzing"))]
struct RegexEngineOverrideGuard(RegexEngineOverride);

#[cfg(any(test, feature = "fuzzing"))]
impl Drop for RegexEngineOverrideGuard {
    fn drop(&mut self) {
        REGEX_ENGINE_OVERRIDE.with(|slot| slot.set(self.0));
    }
}

/// Run `f` with one regex routing policy, restoring the previous policy even
/// when `f` unwinds.
#[cfg(any(test, feature = "fuzzing"))]
pub(crate) fn with_regex_engine_override<R>(
    engine: RegexEngineOverride,
    f: impl FnOnce() -> R,
) -> R {
    let previous = REGEX_ENGINE_OVERRIDE.with(|slot| slot.replace(engine));
    let _guard = RegexEngineOverrideGuard(previous);
    f()
}

/// Run `f` with the pure backtracker forced (no budget/Pike). Test only.
#[cfg(test)]
pub(crate) fn with_backtracker_forced<R>(f: impl FnOnce() -> R) -> R {
    with_regex_engine_override(RegexEngineOverride::Backtracker, f)
}

/// Run `f` with the Pike VM forced for eligible patterns. Test only.
#[cfg(test)]
pub(crate) fn with_pike_forced<R>(f: impl FnOnce() -> R) -> R {
    with_regex_engine_override(RegexEngineOverride::PikeVm, f)
}

// SyntaxClass is imported from crate::emacs_core::syntax.

// ---------------------------------------------------------------------------
// Bytecode helpers
// ---------------------------------------------------------------------------

/// Store a 2-byte signed offset at position in bytecode buffer.
fn store_number(buf: &mut [u8], pos: usize, number: i16) {
    let bytes = number.to_le_bytes();
    buf[pos] = bytes[0];
    buf[pos + 1] = bytes[1];
}

/// Read a 2-byte signed offset from bytecode buffer.
fn extract_number(buf: &[u8], pos: usize) -> i16 {
    i16::from_le_bytes([buf[pos], buf[pos + 1]])
}

/// Store a 2-byte UNSIGNED repeat counter at position in bytecode buffer. GNU
/// counters are `unsigned short` (0..=RE_DUP_MAX), unlike the signed jump
/// offsets stored by `store_number`.
fn store_number_u16(buf: &mut [u8], pos: usize, number: u16) {
    let bytes = number.to_le_bytes();
    buf[pos] = bytes[0];
    buf[pos + 1] = bytes[1];
}

/// Read a 2-byte UNSIGNED repeat counter from bytecode buffer.
fn extract_number_u16(buf: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes([buf[pos], buf[pos + 1]])
}

/// Live counter overrides, keyed by bytecode position of the 2-byte
/// counter field.  GNU mutates the bytecode in place (`STORE_NUMBER`);
/// neomacs keeps compiled patterns immutable and shared, so overrides
/// live in this small association list instead.
type CounterTable = SmallVec<[(usize, u16); 4]>;

/// Read a counter value, falling back to the bytecode if no override has
/// been stored yet.  Used by `succeed_n`, `jump_n`, and `set_number_at`.
fn get_counter(counters: &CounterTable, bytecode: &[u8], pos: usize) -> u16 {
    counters
        .iter()
        .find(|(counter_pos, _)| *counter_pos == pos)
        .map(|(_, val)| *val)
        .unwrap_or_else(|| extract_number_u16(bytecode, pos))
}

/// Store a counter value WITHOUT recording an undo entry.  Only the
/// backtracking pop path uses this directly; everything else goes through
/// the PUSH_NUMBER-style delta save in the matcher.
fn set_counter(counters: &mut CounterTable, pos: usize, val: u16) {
    if let Some(entry) = counters
        .iter_mut()
        .find(|(counter_pos, _)| *counter_pos == pos)
    {
        entry.1 = val;
    } else {
        counters.push((pos, val));
    }
}

// ---------------------------------------------------------------------------
// Phase 2: Compiler (regex_compile)
//
// Translates GNU Emacs regex-emacs.c:1710-3400 (regex_compile function).
// Compiles an Emacs regex pattern string into bytecode.
// ---------------------------------------------------------------------------

/// Error from regex compilation.
#[derive(Debug, Clone)]
pub(crate) struct RegexCompileError {
    pub message: String,
}

impl std::fmt::Display for RegexCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Compile stack entry — tracks open groups during compilation.
/// Mirrors GNU's compile_stack_elt_t.
#[derive(Clone, Debug)]
struct CompileStackEntry {
    /// Bytecode position of the start of the group's alternatives.
    begalt_offset: usize,
    /// Bytecode position of the fixup jump for alternation (or 0).
    fixup_alt_jump: Option<usize>,
    /// Bytecode position of the last expression start (for postfix ops).
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    laststart_offset: Option<usize>,
    /// Group number at the time of \( (before incrementing).
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    regnum: usize,
    /// The actual group number assigned to this \( (None for shy groups).
    assigned_group: Option<usize>,
    /// Bytecode position of the group's StartMemory (or OnFailureJump
    /// for shy groups). Used by postfix ops like ? * + after \).
    group_bytecode_start: usize,
}

/// Compile an Emacs regex pattern into bytecode.
///
/// This is the main entry point, equivalent to GNU's `regex_compile()`.
///
/// # Arguments
/// * `pattern` - The Emacs regex pattern string
/// * `posix` - If true, use POSIX backtracking semantics
/// * `case_fold` - If true, compile for case-insensitive matching
///
/// # Returns
/// A `CompiledPattern` with bytecode ready for the matcher.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn regex_compile(
    pattern: &str,
    posix: bool,
    case_fold: bool,
) -> Result<CompiledPattern, RegexCompileError> {
    let pattern = crate::heap_types::LispString::from_utf8(pattern);
    regex_compile_lisp(&pattern, posix, case_fold)
}

pub(crate) fn regex_compile_lisp(
    pattern: &crate::heap_types::LispString,
    posix: bool,
    case_fold: bool,
) -> Result<CompiledPattern, RegexCompileError> {
    let translation = case_fold.then(CaseTranslation::standard);
    regex_compile_lisp_with_translation(pattern, posix, translation)
}

pub(crate) fn regex_compile_lisp_with_translation(
    pattern: &crate::heap_types::LispString,
    posix: bool,
    translation: Option<CaseTranslation>,
) -> Result<CompiledPattern, RegexCompileError> {
    let mut buf = CompiledPattern::new();
    buf.posix = posix;
    buf.multibyte = pattern.is_multibyte();
    buf.target_multibyte = pattern.is_multibyte();
    buf.translate = translation;
    let _case_fold = buf.translate.is_some();

    let pattern_bytes = pattern.as_bytes();
    let plen = pattern_bytes.len();
    let mut p = 0; // Current position in pattern

    // Compile stack for tracking open groups
    let mut compile_stack: Vec<CompileStackEntry> = Vec::new();
    let mut regnum: usize = 0; // Current group number

    // Track positions in bytecode for fixup
    let mut begalt_offset: usize = 0; // Start of current alternative
    let mut pending_exact: Option<usize> = None; // Position of current exactn being built
    let mut laststart: Option<usize> = None; // Start of last complete expression (for postfix ops)
    let mut laststart_is_group = false; // True when laststart came from a closed \( ... \).
    let mut fixup_alt_jump: Option<usize> = None; // Jump to fixup at end of alternation

    /// Helper: push a byte to the bytecode buffer
    macro_rules! emit {
        ($byte:expr) => {
            buf.buffer.push($byte);
        };
    }

    /// Helper: push an opcode
    macro_rules! emit_op {
        ($op:expr) => {
            buf.buffer.push($op as u8);
        };
    }

    /// Helper: current bytecode position
    macro_rules! bpos {
        () => {
            buf.buffer.len()
        };
    }

    // Macro to fetch next pattern byte, returning error if at end
    #[allow(unused_macros)]
    macro_rules! pat_fetch {
        () => {{
            if p >= plen {
                return Err(RegexCompileError {
                    message: "premature end of pattern".to_string(),
                });
            }
            let c = pattern_bytes[p];
            p += 1;
            c
        }};
    }

    // Main compilation loop
    while p < plen {
        let c = pattern_bytes[p];
        p += 1;

        match c {
            // ----------------------------------------------------------
            // ^ — beginning of line
            // ----------------------------------------------------------
            b'^' => {
                if !(p == 1 || at_begline_loc_p(pattern_bytes, p)) {
                    goto_normal_char(
                        c as u32,
                        &mut buf,
                        &mut pending_exact,
                        &mut laststart,
                        &mut laststart_is_group,
                    );
                    continue;
                }
                laststart = None;
                laststart_is_group = false;
                pending_exact = None;
                emit_op!(RegexOp::BegLine);
            }

            // ----------------------------------------------------------
            // $ — end of line
            // ----------------------------------------------------------
            b'$' => {
                if !(p == plen || at_endline_loc_p(pattern_bytes, p)) {
                    goto_normal_char(
                        c as u32,
                        &mut buf,
                        &mut pending_exact,
                        &mut laststart,
                        &mut laststart_is_group,
                    );
                    continue;
                }
                laststart = Some(bpos!());
                laststart_is_group = false;
                pending_exact = None;
                emit_op!(RegexOp::EndLine);
            }

            // ----------------------------------------------------------
            // . — any character
            // ----------------------------------------------------------
            b'.' => {
                laststart = Some(bpos!());
                pending_exact = None;
                emit_op!(RegexOp::AnyChar);
            }

            // ----------------------------------------------------------
            // * + ? — repetition operators
            // ----------------------------------------------------------
            b'*' | b'+' | b'?' => {
                let Some(mut last) = laststart else {
                    // No previous expression to repeat — treat as literal
                    goto_normal_char(
                        c as u32,
                        &mut buf,
                        &mut pending_exact,
                        &mut laststart,
                        &mut laststart_is_group,
                    );
                    continue;
                };
                let last_is_group = laststart_is_group;

                // GNU regex_compile: if the preceding expression was part
                // of an exactn with count > 1, split off the last character
                // so that the repetition applies only to that character.
                //
                // Do not split when the postfix applies to a just-closed shy
                // group. GNU clears `pending_exact` on \), so `\(?:ab\)?`
                // repeats the whole "ab" group, not only "b".
                if !last_is_group {
                    last = split_trailing_exactn_atom_if_needed(&mut buf, last);
                }

                // GNU regex-emacs.c: if there is a sequence of repetition
                // chars, collapse it down to just one (the right one).  We
                // track zero_times_ok / many_times_ok / greedy exactly as GNU
                // does so that stacked quantifiers like `a**`, `a*?*`, `a++`,
                // `a???` fold onto the preceding atom instead of being treated
                // as literals.  (Interval operators `\{n,m\}` are NOT folded
                // here, matching GNU.)
                let mut cur = c;
                let mut zero_times_ok = false;
                let mut many_times_ok = false;
                let mut greedy = true;
                loop {
                    if cur == b'?' && (zero_times_ok || many_times_ok) {
                        greedy = false;
                    } else {
                        zero_times_ok |= cur != b'+';
                        many_times_ok |= cur != b'?';
                    }

                    if !(p < plen
                        && (pattern_bytes[p] == b'*'
                            || pattern_bytes[p] == b'+'
                            || pattern_bytes[p] == b'?'))
                    {
                        break;
                    }
                    // Found another repeat character — consume and fold it.
                    cur = pattern_bytes[p];
                    p += 1;
                }

                // Map the collapsed flags back to a single effective postfix
                // operator for `compile_repetition`:
                //   (zero, many) = (T,T) -> `*`, (T,F) -> `?`, (F,T) -> `+`.
                let folded_op = match (zero_times_ok, many_times_ok) {
                    (true, true) => b'*',
                    (true, false) => b'?',
                    (false, true) => b'+',
                    // Unreachable: the first iteration always sets at least one
                    // flag, but fall back to a plain match if it somehow isn't.
                    (false, false) => b'+',
                };

                compile_repetition(folded_op, greedy, posix, last, &mut buf)?;

                laststart = None; // Can't apply another postfix op
                laststart_is_group = false;
                pending_exact = None;
            }

            // ----------------------------------------------------------
            // [ — character class
            // ----------------------------------------------------------
            b'[' => {
                laststart = Some(bpos!());
                pending_exact = None;
                let pattern_multibyte = buf.multibyte;
                compile_charset(pattern_bytes, &mut p, &mut buf, pattern_multibyte)?;
            }

            // ----------------------------------------------------------
            // \ — escape sequence
            // ----------------------------------------------------------
            b'\\' => {
                if p >= plen {
                    return Err(RegexCompileError {
                        message: "Trailing backslash".to_string(),
                    });
                }
                let c2 = pattern_bytes[p];
                p += 1;

                match c2 {
                    // \( — start group
                    b'(' => {
                        let mut is_shy = false;
                        let mut explicit_group: Option<usize> = None;
                        if p < plen && pattern_bytes[p] == b'?' {
                            p += 1; // skip ?
                            if p < plen && pattern_bytes[p] == b':' {
                                is_shy = true;
                                p += 1; // skip :
                            } else {
                                let num_start = p;
                                let mut n = 0usize;
                                while p < plen && pattern_bytes[p].is_ascii_digit() {
                                    if p == num_start && pattern_bytes[p] == b'0' {
                                        return Err(RegexCompileError {
                                            message: "Invalid regular expression".to_string(),
                                        });
                                    }
                                    n = n
                                        .checked_mul(10)
                                        .and_then(|value| {
                                            value.checked_add((pattern_bytes[p] - b'0') as usize)
                                        })
                                        .ok_or_else(|| RegexCompileError {
                                            message: "Regular expression too big".to_string(),
                                        })?;
                                    p += 1;
                                }
                                if p == num_start || p >= plen || pattern_bytes[p] != b':' {
                                    return Err(RegexCompileError {
                                        message: "Invalid regular expression".to_string(),
                                    });
                                }
                                explicit_group = Some(n);
                                p += 1; // skip :
                            }
                        }

                        let group_start = bpos!();
                        let assigned = if let Some(n) = explicit_group {
                            Some(n)
                        } else if !is_shy {
                            // GNU `regex-emacs.c:2246`: an auto-numbered group is
                            // `regnum = ++bufp->re_nsub`, i.e. one past the highest
                            // group number seen so far - NOT `regnum + 1`.  After an
                            // explicit `\(?N:...\)` lowered `regnum` below `re_nsub`
                            // (e.g. `\(?2:\(?1:\)\)\(\)`), `regnum + 1` reused an
                            // already-assigned number; `re_nsub + 1` matches GNU.
                            Some(buf.re_nsub + 1)
                        } else {
                            None
                        };

                        // GNU `regex-emacs.c:2250-2259`: an explicitly-numbered
                        // group `\(?N:...\)` whose number collides with a group
                        // that is still OPEN on the compile stack (an enclosing
                        // `\(...\)` that already holds N) is `REG_BADPAT`.  Note
                        // `compile_stack` here holds only the enclosing groups —
                        // the current group has not been pushed yet.  Reusing a
                        // number for a sequential/closed group stays legal (so
                        // `\(?1:\)\(?1:\)` and `\(\)\(?1:\)` remain accepted).
                        if let Some(n) = explicit_group
                            && n <= buf.re_nsub
                            && compile_stack.iter().any(|e| e.assigned_group == Some(n))
                        {
                            return Err(RegexCompileError {
                                message: "Invalid regular expression".to_string(),
                            });
                        }

                        compile_stack.push(CompileStackEntry {
                            begalt_offset,
                            fixup_alt_jump,
                            laststart_offset: laststart,
                            regnum,
                            assigned_group: assigned,
                            group_bytecode_start: group_start,
                        });

                        if let Some(n) = explicit_group {
                            // Explicit numbered group: assign group number n
                            while buf.re_nsub < n {
                                buf.re_nsub += 1;
                            }
                            regnum = n;
                            emit_op!(RegexOp::StartMemory);
                            emit!(n as u8);
                        } else if !is_shy {
                            // Auto group: number = ++re_nsub (GNU
                            // regex-emacs.c:2246), kept in sync with `assigned`.
                            buf.re_nsub += 1;
                            regnum = buf.re_nsub;
                            emit_op!(RegexOp::StartMemory);
                            emit!(regnum as u8);
                        }

                        begalt_offset = bpos!();
                        laststart = None;
                        fixup_alt_jump = None;
                        pending_exact = None;
                    }

                    // \) — end group
                    b')' => {
                        let Some(entry) = compile_stack.pop() else {
                            return Err(RegexCompileError {
                                message: "Unmatched ) or \\)".to_string(),
                            });
                        };

                        // Handle pending alternation fixup
                        if let Some(fixup) = fixup_alt_jump {
                            let target = bpos!() as i16 - fixup as i16 - 2;
                            store_number(&mut buf.buffer, fixup, target);
                        }

                        // Emit StopMemory for non-shy groups.
                        if let Some(group_num) = entry.assigned_group {
                            emit_op!(RegexOp::StopMemory);
                            emit!(group_num as u8);
                        }

                        begalt_offset = entry.begalt_offset;
                        fixup_alt_jump = entry.fixup_alt_jump;
                        // After \), laststart points to the group's start
                        // so postfix operators (?, *, +) apply to the group.
                        laststart = Some(entry.group_bytecode_start);
                        laststart_is_group = true;
                        // Do NOT restore regnum — it keeps incrementing
                        // across sibling groups (GNU behavior).
                        pending_exact = None;
                    }

                    // \| — alternation
                    b'|' => {
                        pending_exact = None;

                        // Emit jump past the next alternative
                        emit_op!(RegexOp::Jump);
                        let jump_pos = bpos!();
                        emit!(0);
                        emit!(0); // placeholder offset

                        // Fixup previous alternative's failure jump
                        if let Some(fixup) = fixup_alt_jump {
                            let target = bpos!() as i16 - fixup as i16 - 2;
                            store_number(&mut buf.buffer, fixup, target);
                        }

                        // Insert on_failure_jump at the start of the current alt
                        let alt_start = begalt_offset;
                        // We need to insert 3 bytes at alt_start
                        buf.splice_bytecode(alt_start, &[RegexOp::OnFailureJump as u8, 0, 0]);
                        // The failure jump target is right after the jump we just emitted
                        let target = (bpos!() - alt_start - 3) as i16;
                        store_number(&mut buf.buffer, alt_start + 1, target);

                        // Adjust jump_pos since we inserted 3 bytes
                        fixup_alt_jump = Some(jump_pos + 3);

                        begalt_offset = bpos!();
                        laststart = None;
                    }

                    // \` — beginning of buffer
                    b'`' => {
                        // GNU regex-emacs.c deliberately makes postfix operators
                        // following \` literal, matching its treatment of ^.
                        laststart = None;
                        laststart_is_group = false;
                        pending_exact = None;
                        emit_op!(RegexOp::BegBuf);
                    }

                    // \' — end of buffer
                    b'\'' => {
                        laststart = Some(bpos!());
                        pending_exact = None;
                        emit_op!(RegexOp::EndBuf);
                    }

                    // \= — at point
                    b'=' => {
                        laststart = Some(bpos!());
                        pending_exact = None;
                        emit_op!(RegexOp::AtDot);
                    }

                    // \b — word boundary
                    b'b' => {
                        laststart = Some(bpos!());
                        pending_exact = None;
                        buf.uses_syntax = true;
                        emit_op!(RegexOp::WordBound);
                    }

                    // \B — not word boundary
                    b'B' => {
                        laststart = Some(bpos!());
                        pending_exact = None;
                        buf.uses_syntax = true;
                        emit_op!(RegexOp::NotWordBound);
                    }

                    // \< — word beginning
                    b'<' => {
                        laststart = Some(bpos!());
                        pending_exact = None;
                        buf.uses_syntax = true;
                        emit_op!(RegexOp::WordBeg);
                    }

                    // \> — word end
                    b'>' => {
                        laststart = Some(bpos!());
                        pending_exact = None;
                        buf.uses_syntax = true;
                        emit_op!(RegexOp::WordEnd);
                    }

                    // \_ — symbol boundary
                    b'_' => {
                        if p >= plen {
                            return Err(RegexCompileError {
                                message: "Premature end of regular expression".to_string(),
                            });
                        }
                        let c3 = pattern_bytes[p];
                        p += 1;
                        match c3 {
                            b'<' => {
                                laststart = Some(bpos!());
                                pending_exact = None;
                                buf.uses_syntax = true;
                                emit_op!(RegexOp::SymBeg);
                            }
                            b'>' => {
                                laststart = Some(bpos!());
                                pending_exact = None;
                                buf.uses_syntax = true;
                                emit_op!(RegexOp::SymEnd);
                            }
                            _ => {
                                return Err(RegexCompileError {
                                    message: "Invalid regular expression".to_string(),
                                });
                            }
                        }
                    }

                    // \w — word constituent (syntax-table aware)
                    b'w' => {
                        laststart = Some(bpos!());
                        pending_exact = None;
                        buf.uses_syntax = true;
                        emit_op!(RegexOp::SyntaxSpec);
                        emit!(u8::from(SyntaxClass::Word));
                    }

                    // \W — not word constituent
                    b'W' => {
                        laststart = Some(bpos!());
                        pending_exact = None;
                        buf.uses_syntax = true;
                        emit_op!(RegexOp::NotSyntaxSpec);
                        emit!(u8::from(SyntaxClass::Word));
                    }

                    // \sC — syntax class C
                    b's' => {
                        if p >= plen {
                            return Err(RegexCompileError {
                                message: "Premature end of regular expression".to_string(),
                            });
                        }
                        let sc = syntax_spec_code(pattern_bytes[p]);
                        p += 1;
                        laststart = Some(bpos!());
                        pending_exact = None;
                        buf.uses_syntax = true;
                        emit_op!(RegexOp::SyntaxSpec);
                        emit!(sc);
                    }

                    // \SC — not syntax class C
                    b'S' => {
                        if p >= plen {
                            return Err(RegexCompileError {
                                message: "Premature end of regular expression".to_string(),
                            });
                        }
                        let sc = syntax_spec_code(pattern_bytes[p]);
                        p += 1;
                        laststart = Some(bpos!());
                        pending_exact = None;
                        buf.uses_syntax = true;
                        emit_op!(RegexOp::NotSyntaxSpec);
                        emit!(sc);
                    }

                    // \cC — category C
                    b'c' => {
                        if p >= plen {
                            return Err(RegexCompileError {
                                message: "\\c requires category character".to_string(),
                            });
                        }
                        let cat = pattern_bytes[p];
                        p += 1;
                        laststart = Some(bpos!());
                        pending_exact = None;
                        emit_op!(RegexOp::CategorySpec);
                        emit!(cat);
                    }

                    // \CC — not category C
                    b'C' => {
                        if p >= plen {
                            return Err(RegexCompileError {
                                message: "\\C requires category character".to_string(),
                            });
                        }
                        let cat = pattern_bytes[p];
                        p += 1;
                        laststart = Some(bpos!());
                        pending_exact = None;
                        emit_op!(RegexOp::NotCategorySpec);
                        emit!(cat);
                    }

                    // \1-\9 — backreference
                    b'1'..=b'9' => {
                        let group = (c2 - b'0') as usize;
                        if group > buf.re_nsub
                            || compile_stack
                                .iter()
                                .any(|entry| entry.assigned_group == Some(group))
                        {
                            return Err(RegexCompileError {
                                message: "Invalid back reference".to_string(),
                            });
                        }
                        laststart = Some(bpos!());
                        pending_exact = None;
                        emit_op!(RegexOp::Duplicate);
                        emit!(group as u8);
                    }

                    // \{ — interval \{n,m\}
                    b'{' => {
                        // Parse interval
                        let interval_start = p;
                        let (min_count, max_count) = parse_interval(pattern_bytes, &mut p)?;

                        let Some(mut last) = laststart else {
                            // GNU regex-emacs.c:2427 `unfetch_interval`: a
                            // syntactically valid interval without a preceding
                            // atom is literal text beginning with `{`.
                            p = interval_start;
                            goto_normal_char(
                                b'{' as u32,
                                &mut buf,
                                &mut pending_exact,
                                &mut laststart,
                                &mut laststart_is_group,
                            );
                            continue;
                        };
                        if !laststart_is_group {
                            last = split_trailing_exactn_atom_if_needed(&mut buf, last);
                        }

                        compile_interval(min_count, max_count, last, &mut buf)?;
                        laststart = Some(last);
                        laststart_is_group = false;
                        pending_exact = None;
                    }

                    // Other escaped characters — treat as literal
                    _ => {
                        if buf.multibyte && c2 >= 0x80 {
                            let char_start = p - 1;
                            let (code, len) = decode_pattern_char(pattern_bytes, char_start, true)
                                .unwrap_or((c2 as u32, 1));
                            p = char_start + len;
                            goto_normal_char(
                                code,
                                &mut buf,
                                &mut pending_exact,
                                &mut laststart,
                                &mut laststart_is_group,
                            );
                        } else {
                            goto_normal_char(
                                c2 as u32,
                                &mut buf,
                                &mut pending_exact,
                                &mut laststart,
                                &mut laststart_is_group,
                            );
                        }
                    }
                }
            }

            // ----------------------------------------------------------
            // Normal character — add to exactn
            // ----------------------------------------------------------
            _ => {
                if buf.multibyte && c >= 0x80 {
                    let char_start = p - 1;
                    let (code, len) = decode_pattern_char(pattern_bytes, char_start, true)
                        .unwrap_or((c as u32, 1));
                    p = char_start + len;
                    goto_normal_char(
                        code,
                        &mut buf,
                        &mut pending_exact,
                        &mut laststart,
                        &mut laststart_is_group,
                    );
                } else {
                    goto_normal_char(
                        c as u32,
                        &mut buf,
                        &mut pending_exact,
                        &mut laststart,
                        &mut laststart_is_group,
                    );
                }
            }
        }
    }

    // Check for unmatched \(
    if !compile_stack.is_empty() {
        return Err(RegexCompileError {
            message: "Unmatched ( or \\(".to_string(),
        });
    }

    // Handle final alternation fixup
    if let Some(fixup) = fixup_alt_jump {
        let target = bpos!() as i16 - fixup as i16 - 2;
        store_number(&mut buf.buffer, fixup, target);
    }

    // Emit final succeed — but only for non-POSIX patterns.
    //
    // GNU regex-emacs.c:2683-2686:
    //
    //     /* If we don't want backtracking, force success
    //        the first time we reach the end of the compiled pattern.  */
    //     if (!posix_backtracking)
    //       BUF_PUSH (succeed);
    //
    // When `posix_backtracking` is true the matcher must see the
    // natural "fell off the end of the bytecode" path so the POSIX
    // longest-match logic at regex-emacs.c:4272-4344 can run. Emitting
    // `succeed` unconditionally (as an earlier version of this file
    // did) made every pattern jump to `succeed_label`, bypassing the
    // longest-match code entirely.
    if !posix {
        emit_op!(RegexOp::Succeed);
    } else {
        // Exit jumps fixed up above target the old buffer end, which is
        // exactly where this terminal op now sits — no target can point
        // past it (the sealed matcher's no-end-check proof relies on
        // every pattern ending in Succeed or PosixEnd).
        emit_op!(RegexOp::PosixEnd);
    }

    // Resolve on_failure_jump_smart loops (GNU does this lazily at first
    // execution; our bytecode is immutable and shared after this point).
    resolve_smart_jumps(&mut buf);

    // Fuse alternations of single positive syntax-class branches
    // (`\w\|\s_` and friends) into one `SyntaxSpecSet` op.  Runs after
    // smart-jump resolution so it sees final `on_failure_jump`s, and before
    // fastmap population so the fastmap walk observes the fused op.
    fuse_syntaxspec_alternations(&mut buf);

    // Populate the fastmap for search-time position skipping.  The
    // standard syntax table stands in for `[[:word:]]`/`[[:space:]]`
    // ASCII baking here; callers searching under a buffer-local table
    // rebake via `recompute_fastmap` and key their cache by table
    // (GNU `search.c:compile_pattern` + `used_syntax`).
    compile_fastmap(&mut buf, &DefaultSyntaxLookup);

    // Build the multi-literal SIMD prefilter (AUGMENTS the fastmap; the
    // backtracker still verifies every candidate).  Depends only on the
    // bytecode's required-literal structure, which is syntax-table
    // independent, so it survives `recompute_fastmap` cloning unchanged.
    buf.prefilter = build_literal_prefilter(&buf);

    // Decide once whether the non-backtracking Pike VM can simulate this
    // bytecode byte-exactly (see `compute_pike_eligible`).
    buf.pike_eligible = compute_pike_eligible(&buf);
    if buf.pike_eligible {
        // Build the Pike-only rewind view of any keep-string loops.  If a
        // keep-string jump has an unexpected shape, fail closed (ineligible).
        match build_pike_buffer(&buf) {
            Some(pike_buf) => buf.pike_buffer = pike_buf,
            None => buf.pike_eligible = false,
        }
    }

    // Seal: prove the buffer safe for unchecked backtracker dispatch. Runs
    // LAST — after every post-compile rewrite (fusion, splices) — so the
    // validated bytes are exactly what the matcher executes. A compiler bug
    // fails closed to the checked loop (and loudly in debug builds).
    buf.buffer_sealed = validate_sealed_buffer(&buf);
    debug_assert!(
        buf.buffer_sealed,
        "regex compiler produced an unsealable buffer for this pattern"
    );

    Ok(buf)
}

/// Prove the compiled buffer safe for the backtracker's SEALED (unchecked)
/// dispatch: a linear decode must consume the buffer exactly, every opcode
/// byte must name a real [`RegexOp`], every operand span stays in-bounds
/// (via [`opcode_len`]), charsets carry no in-buffer range table (neomacs
/// stores multibyte ranges in side maps; the matcher's `pc` advance would
/// walk into an in-buffer table), and every jump target and counter
/// position lands on an op boundary / in-bounds. Runtime `pc` values are
/// closed over {linear advances, validated jump targets, fail-frame
/// resumes derived from those}, so a sealed loop's fetches never leave the
/// buffer.
fn validate_sealed_buffer(pattern: &CompiledPattern) -> bool {
    let bytecode = &pattern.buffer;
    let len = bytecode.len();
    let mut is_boundary = vec![false; len + 1];
    is_boundary[len] = true;

    // Pass 1: linear decode — boundaries, opcode validity, operand spans.
    let mut pc = 0usize;
    while pc < len {
        is_boundary[pc] = true;
        let Some(op) = RegexOp::from_byte(bytecode[pc]) else {
            return false;
        };
        let Some(op_len) = opcode_len(bytecode, pc) else {
            return false;
        };
        if pc + op_len > len {
            return false;
        }
        if matches!(op, RegexOp::Charset | RegexOp::CharsetNot) && bytecode[pc + 1] & 0x80 != 0 {
            return false;
        }
        pc += op_len;
    }
    if pc != len {
        return false;
    }
    // The sealed loop replaces the per-op end-of-bytecode check with the
    // guarantee that execution only ends at a terminal op.
    if len == 0 {
        return false;
    }
    let mut last_op_start = 0usize;
    let mut walk = 0usize;
    while walk < len {
        last_op_start = walk;
        walk += opcode_len(bytecode, walk).expect("pass 1 validated operand spans");
    }
    if !matches!(
        RegexOp::from_byte(bytecode[last_op_start]),
        Some(RegexOp::Succeed | RegexOp::PosixEnd)
    ) {
        return false;
    }

    // Pass 2: every jump target on a boundary; counter positions in-bounds.
    let mut pc = 0usize;
    while pc < len {
        let op = RegexOp::from_byte(bytecode[pc]).expect("pass 1 validated opcodes");
        match op {
            RegexOp::Jump
            | RegexOp::OnFailureJump
            | RegexOp::OnFailureKeepStringJump
            | RegexOp::OnFailureJumpLoop
            | RegexOp::OnFailureJumpNastyloop
            | RegexOp::OnFailureJumpSmart
            | RegexOp::SucceedN
            | RegexOp::JumpN => {
                // Matcher convention: target = op_pc + 3 + offset.
                // Strictly inside the buffer: the sealed loop drops the
                // end-of-bytecode check, so no jump may land at len — the
                // compiler ends every pattern with Succeed/PosixEnd at the
                // old buffer end, which is where end-targeting jumps land.
                let offset = extract_number(bytecode, pc + 1);
                let target = pc as i64 + 3 + offset as i64;
                if target < 0 || target >= len as i64 || !is_boundary[target as usize] {
                    return false;
                }
            }
            RegexOp::SetNumberAt => {
                // Matcher convention: counter position = op_pc + 1 + offset,
                // read/written as a 2-byte number.
                let offset = extract_number(bytecode, pc + 1);
                let target = pc as i64 + 1 + offset as i64;
                if target < 0 || target + 2 > len as i64 {
                    return false;
                }
            }
            _ => {}
        }
        pc += opcode_len(bytecode, pc).expect("pass 1 validated operand spans");
    }
    true
}

/// Build the Pike-only bytecode view: de-optimize every keep-string smart
/// loop (`OnFailureKeepStringJump` whose jump-back targets the loop body)
/// into the equivalent rewind loop (`OnFailureJump` whose jump-back targets
/// the split).  See [`CompiledPattern::pike_buffer`].
///
/// Returns `Some(None)` if no rewrite is needed (Pike reads `buffer`),
/// `Some(Some(buf))` with the rewritten copy, or `None` if a keep-string
/// jump does not match the expected smart-loop shape (the caller then marks
/// the pattern ineligible — fail closed).  The pattern is already known not
/// to contain a non-greedy `??` keep-string (that sets
/// `has_nongreedy_optional`, which makes it ineligible before we get here),
/// so every `OnFailureKeepStringJump` reaching this point must be a loop.
#[allow(clippy::option_option)]
fn build_pike_buffer(buf: &CompiledPattern) -> Option<Option<Vec<u8>>> {
    let orig = &buf.buffer;
    let mut rewritten: Option<Vec<u8>> = None;
    let mut pc = 0usize;
    while pc < orig.len() {
        let op = RegexOp::from_byte(orig[pc])?;
        if op == RegexOp::OnFailureKeepStringJump {
            // Expected smart keep-string loop shape (after `resolve_smart_jumps`):
            //   pc:      OnFailureKeepStringJump  → p2 (LEXIT)
            //   pc+3:    <one-char body>
            //   p2 - 3:  Jump                     → pc + 3 (body start)
            //   p2:      <continuation>
            let offset = extract_number(orig, pc + 1);
            let p2 = (pc as i64 + 3 + offset as i64) as usize;
            if p2 < pc + 6 || p2 > orig.len() {
                return None;
            }
            let jump_at = p2 - 3;
            if orig.get(jump_at).copied() != Some(RegexOp::Jump as u8) {
                return None;
            }
            let jtarget = (jump_at as i64 + 3 + extract_number(orig, jump_at + 1) as i64) as usize;
            if jtarget != pc + 3 {
                return None;
            }
            // Rewrite: OFKSJ → OnFailureJump (same offset to LEXIT), and
            // retarget the jump-back from the body (pc+3) to the split (pc).
            let out = rewritten.get_or_insert_with(|| orig.clone());
            out[pc] = RegexOp::OnFailureJump as u8;
            let new_off = pc as i64 - (jump_at as i64 + 3);
            store_number(out, jump_at + 1, new_off as i16);
        }
        pc += opcode_len(orig, pc)?;
    }
    Some(rewritten)
}

/// Walk the compiled bytecode and decide whether the Pike VM (see
/// [`pike_match`]) can simulate it byte-exactly.
///
/// The Pike VM reproduces Emacs's leftmost-greedy NFA semantics by
/// exploring the SAME bytecode the backtracker does, in the SAME priority
/// order, but linearly.  These force a fall back to the backtracker:
///   * `Duplicate` — a backreference is not a regular-language construct.
///   * `SucceedN` / `JumpN` / `SetNumberAt` — `\{n,m\}` interval counters
///     carry mutable per-position state that would break the Pike VM's
///     `(pc)`-keyed thread dedup; conservatively excluded in Stage 1.
///   * `OnFailureJumpNastyloop` — a non-greedy quantifier over a NULLABLE
///     body (`\(?:a\|\)*?`).  It has no infinite-loop guard and its exact
///     zero-progress ordering can't be validated against the backtracker
///     (which itself does not terminate on some of these), so it is
///     excluded.  (Non-greedy quantifiers over NON-nullable bodies use
///     `OnFailureJump` and stay eligible.)
///   * A non-greedy `??` (`has_nongreedy_optional`) — its
///     `OnFailureKeepStringJump` has genuine keep-string semantics with no
///     loop back-edge, unlike the smart-loop keep-string that
///     [`build_pike_buffer`] de-optimizes.
///   * POSIX-longest mode (`buf.posix`) — the Pike VM produces
///     leftmost-greedy captures, not POSIX-longest, so it must not run
///     for a POSIX pattern.
///   * A **capture group inside a zero-consume (epsilon) cycle**, e.g.
///     `\(a*\)*` or `\(.??\B\)+?`.  When a `*` / `+` body can iterate
///     without consuming a character, GNU's `check_infinite_loop` takes
///     exactly ONE zero-progress iteration and KEEPS that iteration's
///     captures before forcing the loop exit; the Pike VM's `seen`-set
///     prunes the zero-progress path entirely, losing those captures.
///     Reproducing GNU's empty-loop capture semantics byte-exactly is out
///     of scope for Stage 1.  Crucially this is checked PRECISELY (an
///     actual epsilon cycle through a `StartMemory`/`StopMemory`), NOT by
///     excluding `OnFailureJumpLoop` wholesale — that op is emitted
///     conservatively for many NON-nullable bodies (e.g. the fontlock
///     `\(?:\w\|\s_\|\\.\)+`), which the Pike VM handles correctly as a
///     plain split.
///
/// Every other op the compiler can emit (literals, `.`, charsets,
/// `* + ?` and their non-greedy forms, alternation, groups, anchors,
/// boundaries, `\w \W \sC \SC` and the fused `SyntaxSpecSet`, categories,
/// `\=`) is handled by the Pike VM.  Unknown/malformed bytes make the
/// pattern ineligible (fail closed).
fn compute_pike_eligible(buf: &CompiledPattern) -> bool {
    if buf.posix {
        return false;
    }
    // Non-greedy `??` uses a keep-string jump the Pike VM cannot model.
    if buf.has_nongreedy_optional {
        return false;
    }
    let bytecode = &buf.buffer;
    let mut pc = 0usize;
    while pc < bytecode.len() {
        let Some(op) = RegexOp::from_byte(bytecode[pc]) else {
            return false;
        };
        match op {
            RegexOp::Duplicate
            | RegexOp::SucceedN
            | RegexOp::JumpN
            | RegexOp::SetNumberAt
            // A non-greedy quantifier whose body can match empty (`a*?`/`+?`
            // with a nullable body) compiles to `OnFailureJumpNastyloop`,
            // which has no infinite-loop guard.  Its exact zero-progress
            // ordering is subtle and can't be validated against the
            // backtracker (which itself does not terminate on some of these,
            // e.g. `\(?:a\|\)*?b` on ""), so fall back conservatively.
            | RegexOp::OnFailureJumpNastyloop => return false,
            _ => {}
        }
        let Some(len) = opcode_len(bytecode, pc) else {
            return false;
        };
        pc += len;
    }
    // Precise empty-loop-with-capture check (see doc above).
    !has_capturing_epsilon_cycle(bytecode)
}

/// The non-consuming (epsilon) successors of the op at `pc`.  Consuming ops
/// and `Succeed`/end-of-program are epsilon dead-ends (they break any
/// zero-consume cycle).  Used only by the eligibility analysis.
fn epsilon_successors(bytecode: &[u8], pc: usize) -> SmallVec<[usize; 2]> {
    let mut out: SmallVec<[usize; 2]> = SmallVec::new();
    let Some(op) = RegexOp::from_byte(bytecode[pc]) else {
        return out;
    };
    match op {
        RegexOp::NoOp
        | RegexOp::BegLine
        | RegexOp::EndLine
        | RegexOp::BegBuf
        | RegexOp::EndBuf
        | RegexOp::AtDot
        | RegexOp::WordBound
        | RegexOp::NotWordBound
        | RegexOp::WordBeg
        | RegexOp::WordEnd
        | RegexOp::SymBeg
        | RegexOp::SymEnd => out.push(pc + 1),
        RegexOp::StartMemory | RegexOp::StopMemory => out.push(pc + 2),
        RegexOp::Jump => {
            let offset = extract_number(bytecode, pc + 1);
            out.push((pc as i64 + 3 + offset as i64) as usize);
        }
        RegexOp::OnFailureJump
        | RegexOp::OnFailureKeepStringJump
        | RegexOp::OnFailureJumpLoop
        | RegexOp::OnFailureJumpNastyloop
        | RegexOp::OnFailureJumpSmart => {
            let offset = extract_number(bytecode, pc + 1);
            out.push(pc + 3);
            out.push((pc as i64 + 3 + offset as i64) as usize);
        }
        // Consuming ops, Succeed, and the ineligible ops break the cycle.
        _ => {}
    }
    out
}

/// True if some `StartMemory` / `StopMemory` op lies on a zero-consume
/// (epsilon-only) cycle — the precise condition under which GNU's
/// empty-loop capture semantics diverge from the Pike VM.
fn has_capturing_epsilon_cycle(bytecode: &[u8]) -> bool {
    let mut pc = 0usize;
    while pc < bytecode.len() {
        let Some(op) = RegexOp::from_byte(bytecode[pc]) else {
            return false;
        };
        if matches!(op, RegexOp::StartMemory | RegexOp::StopMemory)
            && epsilon_node_on_cycle(bytecode, pc)
        {
            return true;
        }
        let Some(len) = opcode_len(bytecode, pc) else {
            return false;
        };
        pc += len;
    }
    false
}

/// DFS over epsilon edges: can `start` reach itself without consuming?
fn epsilon_node_on_cycle(bytecode: &[u8], start: usize) -> bool {
    let mut stack: Vec<usize> = epsilon_successors(bytecode, start).to_vec();
    let mut visited = vec![false; bytecode.len() + 1];
    while let Some(n) = stack.pop() {
        if n == start {
            return true;
        }
        if n >= bytecode.len() || visited[n] {
            continue;
        }
        visited[n] = true;
        stack.extend(epsilon_successors(bytecode, n));
    }
    false
}

// ---------------------------------------------------------------------------
// Compiler Helpers
// ---------------------------------------------------------------------------

/// GNU `regex-emacs.c:2765` context check for `^`.
///
/// `p` points just after the `^` byte in `pattern`.  GNU treats `^` as a
/// beginning-of-line assertion only at pattern start, after an alternative
/// (`\|`), or after an opening group (`\(` / `\(?:` / `\(?N:`).  Everywhere
/// else it is a literal character.
fn at_begline_loc_p(pattern: &[u8], p: usize) -> bool {
    if p < 2 {
        return false;
    }

    let mut prev = p - 2;
    match pattern[prev] {
        b'(' | b'|' => {}
        b':' => {
            while prev > 0 && pattern[prev - 1].is_ascii_digit() {
                prev -= 1;
            }
            if !(prev > 1 && pattern[prev - 1] == b'?' && pattern[prev - 2] == b'(') {
                return false;
            }
            prev -= 2;
        }
        _ => return false,
    }

    let slash_end = prev;
    while prev > 0 && pattern[prev - 1] == b'\\' {
        prev -= 1;
    }
    ((slash_end - prev) & 1) != 0
}

/// GNU `regex-emacs.c:2801` context check for `$`.
///
/// `p` points just after the `$` byte in `pattern`.  `$` is an end-of-line
/// assertion only at pattern end, before a closing group (`\)`) or before an
/// alternative (`\|`).
fn at_endline_loc_p(pattern: &[u8], p: usize) -> bool {
    p + 1 < pattern.len() && pattern[p] == b'\\' && matches!(pattern[p + 1], b')' | b'|')
}

fn syntax_spec_code(c: u8) -> u8 {
    SyntaxClass::from_syntax_spec_byte(c)
        .map(u8::from)
        .unwrap_or(0o377)
}

/// Emit a literal character as part of an `exactn` sequence.
///
/// GNU `regex-emacs.c` applies `RE_TRANSLATE (translate, c)` before
/// buffering the char, so a pattern like `"C"` compiled with
/// case-fold on is stored in the bytecode as `'c'`. At match time the
/// buffer char is also `tr()`-translated, so both sides are
/// case-folded to the canonical (lowercase) form. Without the
/// translate-on-compile step here, the pattern byte stays as `'C'`
/// while the matched text byte becomes `'c'` and they fail to compare
/// equal.
fn goto_normal_char(
    c: u32,
    buf: &mut CompiledPattern,
    pending_exact: &mut Option<usize>,
    laststart: &mut Option<usize>,
    laststart_is_group: &mut bool,
) {
    let c = if buf.multibyte {
        buf.translate.as_ref().map_or(c, |table| table.translate(c))
    } else {
        // GNU `regex-emacs.c:normal_char` promotes a unibyte pattern byte
        // before consulting the case table.  High bytes become byte8
        // characters and deliberately bypass translation; otherwise values
        // such as 0xDB are folded as Latin-1 letters even though they denote
        // opaque raw bytes in the regexp.
        let mut byte = c as u8;
        let promoted = regex_unibyte_to_char(byte);
        if !emacs_char::char_byte8_p(promoted)
            && let Some(table) = buf.translate.as_ref()
        {
            let translated = table.translate(promoted);
            if promoted != translated
                && let Some(translated_byte) = regex_char_to_unibyte(translated)
            {
                byte = translated_byte;
            }
        }
        byte as u32
    };

    let mut encoded = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
    let encoded_len = if buf.multibyte {
        emacs_char::char_string(c, &mut encoded)
    } else {
        encoded[0] = c as u8;
        1
    };

    // If we have a pending exactn and it hasn't reached max length (255),
    // just append to it
    if let Some(exact_pos) = *pending_exact {
        let count = buf.buffer[exact_pos] as usize;
        if count + encoded_len <= 255 {
            buf.buffer[exact_pos] += encoded_len as u8;
            buf.buffer.extend_from_slice(&encoded[..encoded_len]);
            *laststart_is_group = false;
            return;
        }
    }

    // Start a new exactn
    *laststart = Some(buf.buffer.len());
    *laststart_is_group = false;
    buf.buffer.push(RegexOp::Exactn as u8);
    *pending_exact = Some(buf.buffer.len());
    buf.buffer.push(encoded_len as u8);
    buf.buffer.extend_from_slice(&encoded[..encoded_len]);
}

/// Split the final character out of a multi-character `exactn` atom.
///
/// GNU `regex-emacs.c` avoids building one `exactn` across a character
/// followed by a postfix or interval operator (see the `normal_char`
/// check for `*`, `+`, `?`, and `\{`). Since this Rust compiler may
/// already have coalesced adjacent literal characters, split lazily
/// before compiling the repeat so `ab\{0,1\}` repeats only `b`, not
/// the whole `ab`.
fn split_trailing_exactn_atom_if_needed(buf: &mut CompiledPattern, laststart: usize) -> usize {
    if buf.buffer.get(laststart).copied() != Some(RegexOp::Exactn as u8) {
        return laststart;
    }

    let count_pos = laststart + 1;
    let Some(&count_byte) = buf.buffer.get(count_pos) else {
        return laststart;
    };
    let count = count_byte as usize;
    let exact_start = count_pos + 1;
    let exact_end = exact_start + count;
    if exact_end > buf.buffer.len() {
        return laststart;
    }
    let exact_bytes = &buf.buffer[exact_start..exact_end];
    let last_char_start = if buf.multibyte {
        let mut rel = 0;
        let mut previous = 0;
        let mut chars = 0;
        while rel < exact_bytes.len() {
            previous = rel;
            let (_, len) = emacs_char::string_char(&exact_bytes[rel..]);
            rel += len;
            chars += 1;
        }
        if chars > 1 { Some(previous) } else { None }
    } else if count > 1 {
        Some(count - 1)
    } else {
        None
    };

    let Some(split_start) = last_char_start else {
        return laststart;
    };

    let split_bytes = exact_bytes[split_start..].to_vec();
    buf.buffer.truncate(exact_start + split_start);
    buf.buffer[count_pos] = split_start as u8;
    let split_atom = buf.buffer.len();
    buf.buffer.push(RegexOp::Exactn as u8);
    buf.buffer.push(split_bytes.len() as u8);
    buf.buffer.extend_from_slice(&split_bytes);
    split_atom
}

/// Compile a repetition operator (*, +, ?).
///
/// Inserts jump opcodes around the preceding expression to implement
/// the repetition. Mirrors GNU's handling in regex_compile cases '*', '+', '?'.
fn compile_repetition(
    op: u8,
    greedy: bool,
    _posix: bool,
    laststart: usize,
    buf: &mut CompiledPattern,
) -> Result<(), RegexCompileError> {
    // All offsets are relative to the position right after the 2-byte offset
    // field.  This matches GNU's convention: after EXTRACT_NUMBER_AND_INCR,
    // `p` points past the offset, and the target is `p + mcnt`.

    let after_last = buf.buffer.len();

    match op {
        b'*' => {
            // * = zero or more
            if greedy {
                // Layout:
                //   [laststart] OFJ*  offset(2)  <expr>  Jump  offset(2)
                //   OFJ* fail target → past the Jump instruction
                //   Jump target → back to the OFJ* opcode
                //
                // Opcode choice mirrors GNU regex-emacs.c:1926-1971:
                // a "simple" one-character body gets on_failure_jump_smart
                // (resolved after compilation to a keep-string fast loop
                // or a plain on_failure_jump by `resolve_smart_jumps`);
                // otherwise on_failure_jump when the body cannot match
                // the empty string, else on_failure_jump_loop (which
                // pays the cycle check).
                let loop_op = if simple_one_char_body(buf, laststart, after_last) {
                    RegexOp::OnFailureJumpSmart
                } else if repeated_body_may_match_empty(&buf.buffer[laststart..after_last]) {
                    RegexOp::OnFailureJumpLoop
                } else {
                    RegexOp::OnFailureJump
                };
                buf.splice_bytecode(laststart, &[loop_op as u8, 0, 0]);
                // After splice, expr occupies [laststart+3 .. laststart+3+expr_len)
                let expr_len = after_last - laststart; // original expr length

                // Add Jump back to the OFJL
                buf.buffer.push(RegexOp::Jump as u8);
                let jpos = buf.buffer.len();
                buf.buffer.push(0);
                buf.buffer.push(0);

                // OFJL fail target: from (laststart+3) → past Jump = (jpos+2)
                // offset = (jpos+2) - (laststart+3) = expr_len + 3
                let ofjl_offset = (expr_len + 3) as i16;
                store_number(&mut buf.buffer, laststart + 1, ofjl_offset);

                // Jump target: from (jpos+2) → OFJL opcode at laststart
                // offset = laststart - (jpos + 2)
                let jump_offset = laststart as i16 - (jpos as i16 + 2);
                store_number(&mut buf.buffer, jpos, jump_offset);
            } else {
                // GNU `regex-emacs.c` compiles non-greedy `*?` as:
                //
                //   jump cond
                // loop:
                //   <expr>
                //   [no-op when expr may match empty]
                // cond:
                //   on_failure_jump[_nastyloop] loop
                //
                // This tries zero iterations first and only falls back
                // into the loop body when a later piece fails.
                let expr_bytes = buf.buffer[laststart..after_last].to_vec();
                let body_may_be_empty = repeated_body_may_match_empty(&expr_bytes);

                buf.buffer.truncate(laststart);

                buf.buffer.push(RegexOp::Jump as u8);
                let jump_pos = buf.buffer.len();
                buf.buffer.push(0);
                buf.buffer.push(0);

                let expr_start = buf.buffer.len();
                buf.buffer.extend_from_slice(&expr_bytes);
                // The body bytes moved from [laststart..after_last) to start at
                // expr_start; re-key any charset side tables to follow them.
                buf.relocate_charset_keys(laststart, after_last, expr_start);
                if body_may_be_empty {
                    buf.buffer.push(RegexOp::NoOp as u8);
                }

                let cond_pos = buf.buffer.len();
                buf.buffer.push(if body_may_be_empty {
                    RegexOp::OnFailureJumpNastyloop as u8
                } else {
                    RegexOp::OnFailureJump as u8
                });
                let cond_arg_pos = buf.buffer.len();
                buf.buffer.push(0);
                buf.buffer.push(0);

                // Initial jump skips directly to the conditional branch.
                let jump_offset = cond_pos as i16 - (jump_pos as i16 + 2);
                store_number(&mut buf.buffer, jump_pos, jump_offset);

                // The conditional branch backtracks into the loop body.
                let cond_offset = expr_start as i16 - (cond_arg_pos as i16 + 2);
                store_number(&mut buf.buffer, cond_arg_pos, cond_offset);
            }
        }
        b'+' => {
            // + = one or more
            // Layout: <expr(already emitted)>  OFJL/OFJ  offset(2)  Jump  offset(2)
            if greedy {
                if simple_one_char_body(buf, laststart, after_last) {
                    // GNU regex-emacs.c:1937-1957: "we turn P+ into PP* if P
                    // is simple", i.e. duplicate the one-char body and compile
                    // the copy as a `*` loop whose head is a smart jump —
                    // `P; OFJS exit; P'; Jump OFJS; exit:` — so
                    // `resolve_smart_jumps` can upgrade the loop to a single
                    // keep-string failure frame for the WHOLE loop (when the
                    // body and continuation are mutually exclusive) instead of
                    // one frame per iteration.  A simple body consumes at
                    // least one char, so the smart opcode is unconditional.
                    let plen = after_last - laststart;
                    buf.buffer.extend_from_within(laststart..after_last);
                    buf.clone_charset_keys(laststart, after_last, after_last);
                    let copy_start = after_last;
                    buf.splice_bytecode(copy_start, &[RegexOp::OnFailureJumpSmart as u8, 0, 0]);
                    buf.buffer.push(RegexOp::Jump as u8);
                    let jpos = buf.buffer.len();
                    buf.buffer.push(0);
                    buf.buffer.push(0);
                    // Smart-jump fail target: from (copy_start+3) → past the
                    // Jump = plen + 3 (same arithmetic as the greedy `*`).
                    store_number(&mut buf.buffer, copy_start + 1, (plen + 3) as i16);
                    // Jump target: back to the smart-jump opcode.
                    let jump_offset = copy_start as i16 - (jpos as i16 + 2);
                    store_number(&mut buf.buffer, jpos, jump_offset);
                    return Ok(());
                }
                // GNU uses on_failure_jump for a body that cannot match
                // empty and on_failure_jump_loop otherwise (the `ofj`
                // choice at regex-emacs.c:1929-1934).  Non-simple bodies
                // skip the PP* rewrite, exactly like GNU: the plain
                // per-iteration on_failure_jump is GNU's own fallback shape.
                let loop_op = if repeated_body_may_match_empty(&buf.buffer[laststart..after_last]) {
                    RegexOp::OnFailureJumpLoop
                } else {
                    RegexOp::OnFailureJump
                };
                // Loop-op fail target → past the Jump instruction (continue)
                buf.buffer.push(loop_op as u8);
                let ofjl_pos = buf.buffer.len();
                buf.buffer.push(0);
                buf.buffer.push(0);

                buf.buffer.push(RegexOp::Jump as u8);
                let jpos = buf.buffer.len();
                buf.buffer.push(0);
                buf.buffer.push(0);

                // OFJL fail: from (ofjl_pos+2) → (jpos+2)
                store_number(&mut buf.buffer, ofjl_pos, (jpos + 2 - ofjl_pos - 2) as i16);

                // Jump target: from (jpos+2) → laststart (start of expr)
                let jump_offset = laststart as i16 - (jpos as i16 + 2);
                store_number(&mut buf.buffer, jpos, jump_offset);
            } else {
                // GNU `regex-emacs.c:1987-1999`: non-greedy `+?`
                // matches one body copy, then uses the same
                // "repeat until fail" conditional jump as `*?`.
                let expr_bytes = buf.buffer[laststart..after_last].to_vec();
                let body_may_be_empty = repeated_body_may_match_empty(&expr_bytes);

                buf.buffer.truncate(laststart);
                let expr_start = buf.buffer.len();
                buf.buffer.extend_from_slice(&expr_bytes);
                // The body moved from [laststart..after_last) to expr_start
                // (here unchanged, but re-key defensively to track the move).
                buf.relocate_charset_keys(laststart, after_last, expr_start);
                if body_may_be_empty {
                    buf.buffer.push(RegexOp::NoOp as u8);
                }

                buf.buffer.push(if body_may_be_empty {
                    RegexOp::OnFailureJumpNastyloop as u8
                } else {
                    RegexOp::OnFailureJump as u8
                });
                let cond_arg_pos = buf.buffer.len();
                buf.buffer.push(0);
                buf.buffer.push(0);

                let cond_offset = expr_start as i16 - (cond_arg_pos as i16 + 2);
                store_number(&mut buf.buffer, cond_arg_pos, cond_offset);
            }
        }
        b'?' => {
            // ? = zero or one
            if greedy {
                // Layout: [laststart] OFJ  offset(2)  <expr>
                // OFJ fail target → past expr
                buf.splice_bytecode(laststart, &[RegexOp::OnFailureJump as u8, 0, 0]);
                let expr_len = after_last - laststart;
                // From (laststart+3) → (laststart+3+expr_len), offset = expr_len
                store_number(&mut buf.buffer, laststart + 1, expr_len as i16);
            } else {
                // Non-greedy `??` (GNU `regex-emacs.c:2009-2015`):
                //
                //   L:    on_failure_jump -> expr_start   ; push body as fallback
                //   L+3:  jump            -> END          ; ...but SKIP it first
                //   L+6:  <expr>
                //   END:
                //
                // The primary path skips the body (zero match = the non-greedy
                // preference); the body is only tried when a later piece fails
                // and matching backtracks to the pushed failure point.  The
                // previous single `OnFailureKeepStringJump` fell through into
                // the body first, i.e. it behaved *greedily* (`a??` matched one
                // char instead of zero).
                //
                // Keep `has_nongreedy_optional` set so the pattern stays on the
                // backtracking matcher (the Pike VM does not model this split).
                buf.has_nongreedy_optional = true;
                let expr_bytes = buf.buffer[laststart..after_last].to_vec();

                buf.buffer.truncate(laststart);

                buf.buffer.push(RegexOp::OnFailureJump as u8);
                let ofj_arg = buf.buffer.len();
                buf.buffer.push(0);
                buf.buffer.push(0);

                buf.buffer.push(RegexOp::Jump as u8);
                let jump_arg = buf.buffer.len();
                buf.buffer.push(0);
                buf.buffer.push(0);

                let expr_start = buf.buffer.len();
                buf.buffer.extend_from_slice(&expr_bytes);
                buf.relocate_charset_keys(laststart, after_last, expr_start);
                let end = buf.buffer.len();

                // on_failure_jump target → body start (the fallback path)
                let ofj_offset = expr_start as i16 - (ofj_arg as i16 + 2);
                store_number(&mut buf.buffer, ofj_arg, ofj_offset);

                // jump target → past the body (the preferred skip path)
                let jump_offset = end as i16 - (jump_arg as i16 + 2);
                store_number(&mut buf.buffer, jump_arg, jump_offset);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

/// GNU's non-greedy `*?` / `+?` loops use the "repeat until fail"
/// layout from `src/regex-emacs.c:1980-2006`, not the eager
/// `on_failure_keep_string_jump` prefix form used for some greedy
/// optimizations. We only need to know whether the repeated body
/// could match the empty string in order to pick between
/// `on_failure_jump` and `on_failure_jump_nastyloop`.
///
/// A full `analyze_first` port would be ideal, but a conservative
/// first-opcode check is sufficient here: return `false` only for
/// obviously consuming atoms. Everything else falls back to the
/// nastyloop opcode, which is slower but semantics-safe.
fn repeated_body_may_match_empty(body: &[u8]) -> bool {
    let Some(&op) = body.first() else {
        return true;
    };

    !matches!(
        RegexOp::from_byte(op),
        Some(
            RegexOp::Exactn
                | RegexOp::AnyChar
                | RegexOp::Charset
                | RegexOp::CharsetNot
                | RegexOp::SyntaxSpec
                | RegexOp::NotSyntaxSpec
                | RegexOp::SyntaxSpecSet
                | RegexOp::CategorySpec
                | RegexOp::NotCategorySpec
        )
    )
}

/// Decode one Emacs character from a pattern byte slice starting at `pos`.
///
/// Multibyte patterns use Emacs internal encoding; unibyte patterns map each
/// byte to a single character code directly.
fn decode_pattern_char(bytes: &[u8], pos: usize, multibyte: bool) -> Option<(u32, usize)> {
    if pos >= bytes.len() {
        return None;
    }
    if multibyte {
        Some(emacs_char::string_char(&bytes[pos..]))
    } else {
        Some((bytes[pos] as u32, 1))
    }
}

fn emacs_char_to_rust_char(code: u32) -> char {
    if emacs_char::char_byte8_p(code) {
        char::from(emacs_char::char_to_byte8(code))
    } else {
        char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER)
    }
}

/// Compile a character class `[...]` into charset bytecode.
/// POSIX named character class kind, returned by `parse_posix_char_class`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PosixCharClassKind {
    Word,
    Space,
    Upper,
    Lower,
    Alpha,
    Alnum,
    Digit,
    Xdigit,
    Punct,
    Graph,
    Print,
    Blank,
    Cntrl,
    Ascii,
    Unibyte,
    NonAscii,
    Multibyte,
    /// `[:` was found but the name is not a valid class.
    Error,
}

impl PosixCharClassKind {
    fn name(self) -> Option<&'static str> {
        match self {
            Self::Word => Some("word"),
            Self::Space => Some("space"),
            Self::Upper => Some("upper"),
            Self::Lower => Some("lower"),
            Self::Alpha => Some("alpha"),
            Self::Alnum => Some("alnum"),
            Self::Digit => Some("digit"),
            Self::Xdigit => Some("xdigit"),
            Self::Punct => Some("punct"),
            Self::Graph => Some("graph"),
            Self::Print => Some("print"),
            Self::Blank => Some("blank"),
            Self::Cntrl => Some("cntrl"),
            Self::Ascii => Some("ascii"),
            Self::Unibyte => Some("unibyte"),
            Self::NonAscii => Some("nonascii"),
            Self::Multibyte => Some("multibyte"),
            Self::Error => None,
        }
    }
}

struct PosixCharClass {
    kind: PosixCharClassKind,
    byte_len: usize,
}

/// Parse a POSIX named character class `[:name:]` at `pattern[pos..]`.
/// Returns `None` if `pattern[pos..]` doesn't start with `[:`.
fn parse_posix_char_class(pattern: &[u8], pos: usize, plen: usize) -> Option<PosixCharClass> {
    if pos + 4 > plen {
        return None;
    }
    if pattern[pos] != b'[' || pattern[pos + 1] != b':' {
        return None;
    }
    // Find closing ":]"
    let name_start = pos + 2;
    let mut end = name_start;
    while end + 1 < plen {
        if pattern[end] == b':' && pattern[end + 1] == b']' {
            break;
        }
        end += 1;
    }
    if end + 1 >= plen {
        // No closing ":]" found — treat "[:" as literal characters
        return None;
    }
    let name = &pattern[name_start..end];
    let kind = match name {
        b"word" => PosixCharClassKind::Word,
        b"alnum" => PosixCharClassKind::Alnum,
        b"alpha" => PosixCharClassKind::Alpha,
        b"space" => PosixCharClassKind::Space,
        b"digit" => PosixCharClassKind::Digit,
        b"blank" => PosixCharClassKind::Blank,
        b"upper" => PosixCharClassKind::Upper,
        b"lower" => PosixCharClassKind::Lower,
        b"punct" => PosixCharClassKind::Punct,
        b"ascii" => PosixCharClassKind::Ascii,
        b"graph" => PosixCharClassKind::Graph,
        b"print" => PosixCharClassKind::Print,
        b"cntrl" => PosixCharClassKind::Cntrl,
        b"xdigit" => PosixCharClassKind::Xdigit,
        b"unibyte" => PosixCharClassKind::Unibyte,
        b"nonascii" => PosixCharClassKind::NonAscii,
        b"multibyte" => PosixCharClassKind::Multibyte,
        _ => PosixCharClassKind::Error,
    };
    Some(PosixCharClass {
        kind,
        byte_len: end + 2 - pos, // include ":]" closing
    })
}

fn compile_charset(
    pattern: &[u8],
    p: &mut usize,
    buf: &mut CompiledPattern,
    pattern_multibyte: bool,
) -> Result<(), RegexCompileError> {
    let plen = pattern.len();
    if *p >= plen {
        return Err(RegexCompileError {
            message: "Unmatched [ or [^".to_string(),
        });
    }

    // Check for negation
    let negate = *p < plen && pattern[*p] == b'^';
    if negate {
        *p += 1;
        if *p >= plen {
            return Err(RegexCompileError {
                message: "Unmatched [ or [^".to_string(),
            });
        }
    }

    let op = if negate {
        RegexOp::CharsetNot
    } else {
        RegexOp::Charset
    };

    // Record the bytecode position of this charset opcode for the
    // multibyte_charsets map.
    let charset_opcode_pos = buf.buffer.len();
    buf.buffer.push(op as u8);
    let _bitmap_len_pos = buf.buffer.len();
    buf.buffer.push(32); // 256 bits = 32 bytes bitmap

    // Initialize 32-byte bitmap (256 bits, one per ASCII char)
    let bitmap_start = buf.buffer.len();
    buf.buffer.extend_from_slice(&[0u8; 32]);

    // Collect multibyte (non-ASCII) ranges for this charset.
    let mut mb_ranges: Vec<(char, char)> = Vec::new();

    // Bitmask of class flags for `[[:word:]]` / `[[:space:]]`. The
    // matcher checks these against the buffer syntax table at run
    // time so per-mode word/space definitions take effect.
    let mut class_bits: u32 = 0;

    // Special case: ] at start is literal
    let mut first = true;
    let mut pending_char: Option<u32> = None;
    let mut closed = false;

    while *p < plen {
        // GNU `re_wctype_parse`: before reading a character, check if
        // we're at `[:name:]` — a POSIX named character class inside
        // the bracket expression.  The `]` in the closing `:]` must
        // not close the outer `[...]`.
        if let Some(cc) = parse_posix_char_class(pattern, *p, plen) {
            if let Some(c) = pending_char.take() {
                add_charset_char(
                    &mut buf.buffer,
                    bitmap_start,
                    c,
                    &mut mb_ranges,
                    pattern_multibyte,
                    buf.translate.as_ref(),
                );
            }
            *p += cc.byte_len;
            let Some(class_name) = cc.kind.name() else {
                return Err(RegexCompileError {
                    message: "Invalid character class name".to_string(),
                });
            };
            apply_posix_class(
                class_name,
                &mut buf.buffer,
                bitmap_start,
                &mut mb_ranges,
                &mut class_bits,
                buf.translate.as_ref(),
            )?;
            if *p >= plen {
                return Err(RegexCompileError {
                    message: "Unmatched [ or [^".to_string(),
                });
            }
            // Mark that we've consumed a character (prevents `]` from
            // being treated as literal at position 0).
            first = false;
            pending_char = None;
            continue;
        }

        let b = pattern[*p];

        // Decode a full Emacs character from the pattern.
        let (c, clen) =
            decode_pattern_char(pattern, *p, pattern_multibyte).unwrap_or((b as u32, 1));
        *p += clen;

        if b == b']' && !first {
            closed = true;
            break;
        }
        first = false;

        // GNU `regex-emacs.c`: a `-' that begins a range (a range-start char is
        // pending) with no following range-end character is a premature end of
        // the pattern -- e.g. `[a-` -- signalled as "Premature end of regular
        // expression", not an unmatched-bracket error.
        if b == b'-' && pending_char.is_some() && *p >= plen {
            return Err(RegexCompileError {
                message: "Premature end of regular expression".to_string(),
            });
        }

        if b == b'-' && *p < plen && pattern[*p] != b']' {
            if let Some(range_start) = pending_char.take() {
                // Range: range_start - next_char
                let (range_end, rlen) = decode_pattern_char(pattern, *p, pattern_multibyte)
                    .unwrap_or((pattern[*p] as u32, 1));
                *p += rlen;
                let translate = buf.translate.as_ref();
                add_charset_range(
                    &mut buf.buffer,
                    bitmap_start,
                    &mut mb_ranges,
                    range_start,
                    range_end,
                    pattern_multibyte,
                    translate,
                );
                continue;
            }
            // '-' at start or after a range → literal '-'
            pending_char = Some('-' as u32);
            continue;
        }

        // GNU `regex-emacs.c` treats backslash as a literal character
        // inside a bracket expression: the parser at lines 2055-2140
        // has no escape handling in the `[...]` loop, so `[\w]` is
        // the character class containing `\` and `w`, and `\n` is
        // the class containing `\` and `n`. Users who want a word
        // character class inside a bracket expression must use the
        // POSIX class `[[:word:]]`.
        if b == b'\\' {
            if let Some(c) = pending_char.take() {
                add_charset_char(
                    &mut buf.buffer,
                    bitmap_start,
                    c,
                    &mut mb_ranges,
                    pattern_multibyte,
                    buf.translate.as_ref(),
                );
            }
            pending_char = Some('\\' as u32);
            continue;
        }

        // POSIX named character classes (`[:alpha:]`, etc.) are now
        // handled by `parse_posix_char_class` at the top of the
        // loop, before the per-character decode.  That function
        // does not advance `p` when the `[:` prefix is a false
        // start, so a literal `[` inside a bracket expression
        // (e.g. `[[`, `[a[:c]`) is treated correctly — it falls
        // through to the character-level processing below.
        //
        // The previous inline handler that used to live here
        // unconditionally advanced `p` past the pattern end when
        // `[:` was not followed by a valid class name, causing
        // spurious "Unmatched [" errors.

        if let Some(prev) = pending_char.take() {
            add_charset_char(
                &mut buf.buffer,
                bitmap_start,
                prev,
                &mut mb_ranges,
                pattern_multibyte,
                buf.translate.as_ref(),
            );
        }
        pending_char = Some(c);
    }

    if !closed {
        return Err(RegexCompileError {
            message: "Unmatched [ or [^".to_string(),
        });
    }

    if let Some(c) = pending_char.take() {
        add_charset_char(
            &mut buf.buffer,
            bitmap_start,
            c,
            &mut mb_ranges,
            pattern_multibyte,
            buf.translate.as_ref(),
        );
    }

    // Store multibyte ranges if any were collected.
    if !mb_ranges.is_empty() {
        buf.multibyte_charsets.insert(charset_opcode_pos, mb_ranges);
    }

    // Record class flags so the matcher can consult the buffer
    // syntax table at run time for `[[:word:]]` and `[[:space:]]`.
    if class_bits != 0 {
        buf.uses_syntax = true;
        // GNU regex-emacs.c:2096-2101: only RECC_SPACE and RECC_WORD
        // hardcode syntax-table content into the compiled pattern (for
        // us: into the fastmap), so only they force the compile cache
        // to key this pattern by syntax table (`used_syntax`).
        if class_bits & (CHARSET_CLASS_BIT_WORD | CHARSET_CLASS_BIT_SPACE) != 0 {
            buf.used_syntax = true;
        }
        buf.charset_class_bits
            .insert(charset_opcode_pos, class_bits);
    }

    Ok(())
}

fn add_charset_char(
    buffer: &mut [u8],
    bitmap_start: usize,
    c: u32,
    mb_ranges: &mut Vec<(char, char)>,
    pattern_multibyte: bool,
    translate: Option<&CaseTranslation>,
) {
    if c < 0x80 {
        set_bitmap_bit(buffer, bitmap_start, c as u8, translate);
    } else if emacs_char::char_byte8_p(c) {
        set_bitmap_raw_bit(buffer, bitmap_start, emacs_char::char_to_byte8(c));
    } else if !pattern_multibyte {
        add_charset_range(
            buffer,
            bitmap_start,
            mb_ranges,
            c,
            c,
            pattern_multibyte,
            translate,
        );
    } else {
        let ch = emacs_char_to_rust_char(c);
        add_multibyte_range(buffer, bitmap_start, mb_ranges, ch, ch, translate);
    }
}

fn add_charset_range(
    buffer: &mut [u8],
    bitmap_start: usize,
    mb_ranges: &mut Vec<(char, char)>,
    mut start: u32,
    end: u32,
    pattern_multibyte: bool,
    translate: Option<&CaseTranslation>,
) {
    if emacs_char::char_byte8_p(end) && start >= 0x80 && !emacs_char::char_byte8_p(start) {
        return;
    }

    if start > end {
        return;
    }

    if start < 0x80 {
        let ascii_end = end.min(0x7f);
        for ch in start..=ascii_end {
            set_bitmap_bit(buffer, bitmap_start, ch as u8, translate);
        }
        start = ascii_end + 1;
        if emacs_char::char_byte8_p(end) {
            start = emacs_char::unibyte_to_char(0x80);
        }
    }

    if start > end {
        return;
    }

    if emacs_char::char_byte8_p(start) {
        let start_byte = emacs_char::char_to_byte8(start);
        let end_byte = emacs_char::char_to_byte8(end);
        for byte in start_byte..=end_byte {
            set_bitmap_raw_bit(buffer, bitmap_start, byte);
        }
    } else if pattern_multibyte {
        let start_ch = emacs_char_to_rust_char(start);
        let end_ch = emacs_char_to_rust_char(end);
        add_multibyte_range(buffer, bitmap_start, mb_ranges, start_ch, end_ch, translate);
    } else {
        if start > 0xFF || end > 0xFF {
            return;
        }
        let start_byte = start as u8;
        let end_byte = end as u8;
        for byte in start_byte..=end_byte {
            let c = regex_unibyte_to_char(byte);
            if emacs_char::char_byte8_p(c) {
                set_bitmap_raw_bit(buffer, bitmap_start, byte);
            } else {
                let translated = translate.map_or(c, |table| table.translate(c));
                if let Some(translated_byte) = regex_char_to_unibyte(translated) {
                    set_bitmap_raw_bit(buffer, bitmap_start, translated_byte);
                } else {
                    set_bitmap_raw_bit(buffer, bitmap_start, byte);
                }
                if let Some(ch) = char::from_u32(translated) {
                    push_or_merge_multibyte_range(mb_ranges, ch);
                }
            }
        }
    }
}

fn push_or_merge_multibyte_range(ranges: &mut Vec<(char, char)>, ch: char) {
    for (lo, hi) in ranges.iter_mut().rev() {
        let ch_u = ch as u32;
        let lo_u = *lo as u32;
        let hi_u = *hi as u32;
        if ch_u >= lo_u.saturating_sub(1) && ch_u <= hi_u.saturating_add(1) {
            if ch_u < lo_u {
                *lo = ch;
            } else if ch_u > hi_u {
                *hi = ch;
            }
            return;
        }
    }
    ranges.push((ch, ch));
}

/// Add a multibyte character range and its case-canonical image.
///
/// Mirrors GNU `SETUP_MULTIBYTE_RANGE`: the original `[FROM, TO]` is stored
/// first, then every character in that range is translated through the active
/// case table.  Translated multibyte characters outside the original range are
/// merged into the range table; translated unibyte characters are reflected in
/// the bitmap.
fn add_multibyte_range(
    buffer: &mut [u8],
    bitmap_start: usize,
    ranges: &mut Vec<(char, char)>,
    start: char,
    end: char,
    translate: Option<&CaseTranslation>,
) {
    ranges.push((start, end));
    let Some(translate) = translate else {
        return;
    };

    let start_u = start as u32;
    let end_u = end as u32;
    for code in start_u..=end_u {
        let translated = translate.translate(code);
        if let Some(byte) = regex_char_to_unibyte(translated) {
            set_bitmap_raw_bit(buffer, bitmap_start, byte);
        }
        if translated >= start_u && translated <= end_u {
            continue;
        }
        if let Some(ch) = char::from_u32(translated) {
            push_or_merge_multibyte_range(ranges, ch);
        }
    }
}

/// Set a bit in the charset bitmap, translating through TRANSLATE if
/// supplied.
///
/// GNU `regex-emacs.c:SETUP_ASCII_RANGE` (lines 1397-1412) runs
/// `C1 = TRANSLATE(C0)` and then `SET_LIST_BIT(C1)` — it translates
/// each individual character as the range is walked and only stores
/// the translated bit. The matcher at regex-emacs.c:4553 does the
/// same TRANSLATE on the input character before the bitmap lookup,
/// so matches work out for any case-equivalent input.
///
/// Earlier versions of this function instead set the bit for both
/// the raw character and its Rust-derived upper/lower partners,
/// regardless of what translate table the pattern was compiled with.
/// That was audit finding #9 in `drafts/regex-search-audit.md`:
/// "charset case-fold range translation is eager (not lazy)". The
/// practical difference only shows up when Rust's Unicode case
/// mapping disagrees with Emacs's case canon table, but the GNU-
/// parity fix is to consult the same translate table both sides.
fn set_bitmap_bit(
    buffer: &mut [u8],
    bitmap_start: usize,
    c: u8,
    translate: Option<&CaseTranslation>,
) {
    let target = match translate {
        Some(table) => table.translate_byte(c),
        None => c,
    };
    set_bitmap_raw_bit(buffer, bitmap_start, target);
}

fn set_bitmap_raw_bit(buffer: &mut [u8], bitmap_start: usize, c: u8) {
    let byte_idx = bitmap_start + (c as usize / 8);
    let bit_idx = c as usize % 8;
    if byte_idx < buffer.len() {
        buffer[byte_idx] |= 1 << bit_idx;
    }
}

/// Apply a POSIX character class to the bitmap and multibyte range list.
///
/// Mirrors GNU `regex-emacs.c:re_wctype_parse` (lines 1525-1601) and
/// `re_iswctype` (lines 1603-1630). The full set of 17 classes is:
/// `alnum`, `alpha`, `blank`, `cntrl`, `digit`, `graph`, `lower`,
/// `print`, `punct`, `space`, `upper`, `xdigit`, `ascii`, `word`,
/// `nonascii`, `unibyte`, `multibyte`.
///
/// Semantics are taken from GNU's header macros at `regex-emacs.c:98-153`:
///
/// - `IS_REAL_ASCII(c)` is `c < 0x80`.
/// - `ISBLANK(c)` for ASCII is `c == ' ' || c == '\t'` only
///   (space and tab; NOT newline, formfeed, carriage return).
/// - `ISSPACE(c)` is `BUFFER_SYNTAX(c) == Swhitespace`; GNU's default
///   standard syntax table treats space, tab, newline, formfeed, and
///   carriage return as whitespace.
/// - `ISGRAPH(c)` for single-byte is `c > ' '` AND NOT in
///   `[0x7F..=0xA0]`.
/// - `ISPRINT(c)` for single-byte is `c >= ' '` AND NOT in
///   `[0x7F..=0x9F]`.
/// - `ISWORD(c)` is `BUFFER_SYNTAX(c) == Sword`; GNU's default treats
///   ASCII letters and digits as word constituents.
/// - `IS_REAL_ASCII(c)` covers 0x00..=0x7F for `ascii`.
/// - `nonascii` = `!IS_REAL_ASCII(c)` (>= 0x80).
/// - `unibyte` matches any single-byte character (bytes 0x00..=0xFF
///   in the bitmap, plus 8-bit raw byte chars).
/// - `multibyte` = `!ISUNIBYTE(c)`; matches multibyte characters
///   only (non-ASCII range via the multibyte range list).
///
/// Unknown class names mirror GNU's `RECC_ERROR` (regex-emacs.c:1600,
/// consumed as `REG_ECTYPE` at line 2071). We signal the same error
/// rather than silently ignoring the class as before.
///
/// Note: `word` and `space` semantically depend on the buffer's
/// syntax table (see audit finding #8 in
/// `drafts/regex-search-audit.md`). For now we bake in the standard
/// default; threading the per-buffer syntax table through charset
/// compilation is tracked as audit #8.
fn apply_posix_class(
    name: &str,
    buffer: &mut [u8],
    bitmap_start: usize,
    mb_ranges: &mut Vec<(char, char)>,
    class_bits: &mut u32,
    translate: Option<&CaseTranslation>,
) -> Result<(), RegexCompileError> {
    *class_bits |= posix_class_bit(name)?;
    // --- ASCII bitmap bits ------------------------------------------------
    //
    // GNU `regex_compile` (regex-emacs.c:2081-2092) sets bitmap (list) bits
    // ONLY for ASCII characters `c < 0x80` where `re_iswctype(c, cc)` is true;
    // the non-ASCII / multibyte side is recorded SOLELY as a range-table bit
    // (`re_wctype_to_bit`) and consulted later by `execute_charset` for chars
    // `c >= 256` (our `class_bits` / `mb_ranges`, the multibyte dispatch path).
    //
    // Therefore the bitmap NEVER contains bits for bytes 0x80..=0xFF.  A raw
    // high byte 0x80..=0xFF in a UNIBYTE target hits the bitmap-only branch of
    // `execute_charset` (`unibyte && c < 256`, regex-emacs.c:3773), where these
    // bits are absent, so it matches NO POSIX class — exactly GNU's behavior
    // (e.g. `[[:nonascii:]]`, `[[:print:]]`, even `[[:unibyte:]]` do NOT match
    // a raw high byte in a unibyte string).  Each arm below enumerates only the
    // ASCII bytes for which `re_iswctype` is true.
    let ascii_bytes: Vec<u8> = match name {
        "alpha" => (b'A'..=b'Z').chain(b'a'..=b'z').collect(),
        "digit" => (b'0'..=b'9').collect(),
        "alnum" => (b'A'..=b'Z')
            .chain(b'a'..=b'z')
            .chain(b'0'..=b'9')
            .collect(),
        // GNU `ISSPACE(c)` is `BUFFER_SYNTAX(c) == Swhitespace`
        // (regex-emacs.c:151): the `[[:space:]]` class consults the ACTIVE
        // syntax table's whitespace class, NOT a fixed isspace set.  GNU builds
        // the ASCII bitmap at compile time from that syntax table; neomacs
        // re-derives the syntax-sensitive ASCII membership at MATCH time via
        // `posix_class_matches` (which tests `char_syntax(ch) == Whitespace`),
        // so we contribute NO fixed bitmap bytes here.  Baking in space/tab/
        // LF/CR/FF would make `[[:space:]]` match LF/CR even when the buffer's
        // syntax table classifies them otherwise (e.g. `\n` = comment-end and
        // `\r` = symbol in `emacs-lisp-mode`), which is exactly the GNU
        // divergence this avoids.
        "space" => Vec::new(),
        // GNU ISBLANK is strictly ASCII space and tab.
        "blank" => vec![b' ', b'\t'],
        "upper" => (b'A'..=b'Z').collect(),
        "lower" => (b'a'..=b'z').collect(),
        "punct" => b"!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~".to_vec(),
        // GNU ISPRINT for ASCII (c < 0x80): c >= ' ' and not 0x7F, i.e.
        // 0x20..=0x7E.  The high-byte printable range (0xA0..=0xFF) is NOT a
        // bitmap bit; multibyte printable chars match via `class_bits` in the
        // multibyte path of `execute_charset`.
        "print" => (0x20u8..=0x7E).collect(),
        // GNU ISGRAPH for ASCII (c < 0x80): c > ' ' and not 0x7F, i.e.
        // 0x21..=0x7E.  High bytes go through the multibyte path only.
        "graph" => (0x21u8..=0x7E).collect(),
        // GNU `ISCNTRL(c)` is `((c) < ' ')` (regex-emacs.c:108), i.e. only
        // 0x00..=0x1F.  DEL (0x7F) is NOT a control char for Emacs regexp
        // `[[:cntrl:]]`, unlike the C-locale `iscntrl`.  Including 0x7F here
        // made json.el's `(rx (in cntrl))` escape DEL as `` instead of
        // emitting it literally (matching GNU `json-encode-string`).
        "cntrl" => (0x00u8..=0x1F).collect(),
        "xdigit" => (b'0'..=b'9')
            .chain(b'A'..=b'F')
            .chain(b'a'..=b'f')
            .collect(),
        "ascii" => (0x00u8..=0x7F).collect(),
        // GNU ISWORD(c) = BUFFER_SYNTAX(c) == Sword. Default standard
        // syntax table has ASCII letters and digits as word
        // constituents. Per-buffer syntax tables are audit #8.
        "word" => (b'A'..=b'Z')
            .chain(b'a'..=b'z')
            .chain(b'0'..=b'9')
            .collect(),
        // `re_iswctype(c, RECC_NONASCII)` = `!IS_REAL_ASCII(c)` is FALSE for
        // every ASCII byte, so nonascii sets NO bitmap bits; non-ASCII chars
        // match via the `BIT_MULTIBYTE` range-table bit (the multibyte path).
        "nonascii" => Vec::new(),
        // `re_iswctype(c, RECC_UNIBYTE)` = `ISUNIBYTE(c)` is true for all ASCII
        // bytes (c < 0x80) only at compile time (the loop runs c < 0x80); high
        // bytes 0x80..=0xFF are NOT set, so `[[:unibyte:]]` matches an ASCII
        // byte but NOT a raw high byte in a unibyte string (matching GNU).
        // RECC_UNIBYTE has no range-table bit (`re_wctype_to_bit` returns 0).
        "unibyte" => (0x00u8..=0x7F).collect(),
        // !ISUNIBYTE(c): only multibyte (non-ASCII) characters.
        // Nothing in the bitmap; everything in the multibyte range.
        "multibyte" => Vec::new(),
        // GNU `re_wctype_parse` returns RECC_ERROR (regex-emacs.c:1600)
        // for unknown names; the caller at regex-emacs.c:2071 then
        // signals REG_ECTYPE. We raise the equivalent compile error
        // here rather than silently continuing.
        _ => {
            return Err(RegexCompileError {
                message: format!("Invalid character class name: {}", name),
            });
        }
    };

    // GNU regex-emacs.c:2081-2092 sets the bit for the raw class
    // member AND also the bit for its TRANSLATE-mapped partner when
    // a translate table is in effect. Our set_bitmap_bit always
    // applies the translation (so it already sets the translated
    // bit); we additionally set the raw bit here to cover inputs
    // that match the raw form without going through the translate.
    for c in ascii_bytes {
        // Raw bit (no translation).
        set_bitmap_bit(buffer, bitmap_start, c, None);
        // Translated bit, when a translate table is active. This is
        // a no-op when `translate` is `None` or `translate[c] == c`.
        if translate.is_some() {
            set_bitmap_bit(buffer, bitmap_start, c, translate);
        }
    }

    // --- Multibyte coverage ----------------------------------------------
    //
    // GNU does not expand multibyte POSIX classes into concrete ranges at
    // compile time.  It records class bits and calls `re_iswctype` while
    // executing the charset.  `class_bits` above is the Neomacs equivalent.
    // These explicit ranges remain only for the two classes that are pure
    // broad range tests and were represented this way historically.
    match name {
        // Non-ASCII entirely: 0x80..=max Unicode scalar.
        "nonascii" | "multibyte" => {
            mb_ranges.push(('\u{80}', '\u{10FFFF}'));
        }
        _ => {}
    }

    Ok(())
}

fn posix_class_bit(name: &str) -> Result<u32, RegexCompileError> {
    match name {
        "alnum" => Ok(CHARSET_CLASS_BIT_ALNUM),
        "alpha" => Ok(CHARSET_CLASS_BIT_ALPHA),
        "blank" => Ok(CHARSET_CLASS_BIT_BLANK),
        "cntrl" => Ok(CHARSET_CLASS_BIT_CNTRL),
        "digit" => Ok(CHARSET_CLASS_BIT_DIGIT),
        "graph" => Ok(CHARSET_CLASS_BIT_GRAPH),
        "lower" => Ok(CHARSET_CLASS_BIT_LOWER),
        "print" => Ok(CHARSET_CLASS_BIT_PRINT),
        "punct" => Ok(CHARSET_CLASS_BIT_PUNCT),
        "space" => Ok(CHARSET_CLASS_BIT_SPACE),
        "upper" => Ok(CHARSET_CLASS_BIT_UPPER),
        "xdigit" => Ok(CHARSET_CLASS_BIT_XDIGIT),
        "ascii" => Ok(CHARSET_CLASS_BIT_ASCII),
        "word" => Ok(CHARSET_CLASS_BIT_WORD),
        "nonascii" => Ok(CHARSET_CLASS_BIT_NONASCII),
        "unibyte" => Ok(CHARSET_CLASS_BIT_UNIBYTE),
        "multibyte" => Ok(CHARSET_CLASS_BIT_MULTIBYTE),
        _ => Err(RegexCompileError {
            message: format!("Invalid character class name: {}", name),
        }),
    }
}

/// Parse an interval \{n,m\} from the pattern.
/// Returns (min, max) where max=None means unbounded.
fn parse_interval(
    pattern: &[u8],
    p: &mut usize,
) -> Result<(usize, Option<usize>), RegexCompileError> {
    let plen = pattern.len();

    // Parse min
    let mut min = 0usize;
    while *p < plen && pattern[*p].is_ascii_digit() {
        min = min * 10 + (pattern[*p] - b'0') as usize;
        *p += 1;
    }

    let max = if *p < plen && pattern[*p] == b',' {
        *p += 1; // skip comma
        if *p < plen && pattern[*p] == b'\\' && *p + 1 < plen && pattern[*p + 1] == b'}' {
            // \{n,\} — unbounded
            None
        } else {
            let mut m = 0usize;
            while *p < plen && pattern[*p].is_ascii_digit() {
                m = m * 10 + (pattern[*p] - b'0') as usize;
                *p += 1;
            }
            Some(m)
        }
    } else {
        Some(min) // \{n\} — exact count
    };

    // GNU's GET_UNSIGNED_NUMBER (regex-emacs.c) rejects a repeat count that
    // exceeds RE_DUP_MAX (0xffff) with `(invalid-regexp "Invalid content of
    // \\{\\}")` while reading the number -- not the generic "too big".
    const RE_DUP_MAX: usize = 0xffff;
    if min > RE_DUP_MAX || max.is_some_and(|m| m > RE_DUP_MAX) {
        return Err(RegexCompileError {
            message: "Invalid content of \\{\\}".to_string(),
        });
    }

    // GNU regex-emacs.c:2390 rejects a descending interval where a finite
    // upper bound is smaller than the lower bound (e.g. `a\{2,1\}`), signaling
    // `(invalid-regexp "Invalid content of \\{\\}")`.  An unbounded `\{n,\}`
    // (max == None) is always valid.
    if let Some(m) = max
        && m < min
    {
        return Err(RegexCompileError {
            message: "Invalid content of \\{\\}".to_string(),
        });
    }

    // Expect \}
    if *p + 1 < plen && pattern[*p] == b'\\' && pattern[*p + 1] == b'}' {
        *p += 2;
    } else {
        return Err(RegexCompileError {
            message: "Unmatched \\{".to_string(),
        });
    }

    Ok((min, max))
}

/// Compile an interval \{n,m\} into bytecode.
fn checked_i16_offset(offset: isize) -> Result<i16, RegexCompileError> {
    i16::try_from(offset).map_err(|_| RegexCompileError {
        message: "Regular expression too big".to_string(),
    })
}

fn checked_u16_counter(value: usize) -> Result<u16, RegexCompileError> {
    u16::try_from(value).map_err(|_| RegexCompileError {
        message: "Regular expression too big".to_string(),
    })
}

fn store_jump_at(
    buffer: &mut [u8],
    op_pos: usize,
    op: RegexOp,
    target: usize,
) -> Result<(), RegexCompileError> {
    buffer[op_pos] = op as u8;
    let offset = checked_i16_offset(target as isize - (op_pos + 3) as isize)?;
    store_number(buffer, op_pos + 1, offset);
    Ok(())
}

fn store_jump2_at(
    buffer: &mut [u8],
    op_pos: usize,
    op: RegexOp,
    target: usize,
    count: usize,
) -> Result<(), RegexCompileError> {
    store_jump_at(buffer, op_pos, op, target)?;
    store_number_u16(buffer, op_pos + 3, checked_u16_counter(count)?);
    Ok(())
}

fn insert_jump(
    buf: &mut CompiledPattern,
    at: usize,
    op: RegexOp,
    target: usize,
) -> Result<(), RegexCompileError> {
    buf.splice_bytecode(at, &[op as u8, 0, 0]);
    store_jump_at(&mut buf.buffer, at, op, target)
}

fn insert_jump2(
    buf: &mut CompiledPattern,
    at: usize,
    op: RegexOp,
    target: usize,
    count: usize,
) -> Result<(), RegexCompileError> {
    buf.splice_bytecode(at, &[op as u8, 0, 0, 0, 0]);
    store_jump2_at(&mut buf.buffer, at, op, target, count)
}

fn insert_set_number_at(
    buf: &mut CompiledPattern,
    at: usize,
    target_counter_offset: usize,
    value: usize,
) -> Result<(), RegexCompileError> {
    buf.splice_bytecode(at, &[RegexOp::SetNumberAt as u8, 0, 0, 0, 0]);
    let offset = checked_i16_offset(target_counter_offset as isize)?;
    store_number(&mut buf.buffer, at + 1, offset);
    store_number_u16(&mut buf.buffer, at + 3, checked_u16_counter(value)?);
    Ok(())
}

/// Compile an interval \{n,m\} into GNU's counted interval bytecode.
///
/// This mirrors `src/regex-emacs.c`'s interval layout:
///
/// ```text
/// set_number_at <jump_n count> <upper>
/// set_number_at <succeed_n count> <lower>
/// succeed_n     <after jump_n>   <lower>
/// <body>
/// jump_n        <succeed_n>      <upper - 1>
/// ```
///
/// GNU uses `on_failure_jump_loop` instead of `succeed_n` for a zero
/// lower bound and omits the upper-bound `jump_n` when no finite upper
/// bound exists.  Keeping this counted shape matters for large intervals
/// such as CC Mode's `[[:alnum:]]\\{,1000\\}`: expanding the body into
/// hundreds of optional copies creates backtracking behavior GNU avoids.
fn compile_interval(
    min: usize,
    max: Option<usize>,
    laststart: usize,
    buf: &mut CompiledPattern,
) -> Result<(), RegexCompileError> {
    if let Some(max_val) = max {
        if max_val == 0 {
            buf.buffer.truncate(laststart);
            buf.truncate_charset_keys(laststart);
            return Ok(());
        }
        if min == 1 && max_val == 1 {
            return Ok(());
        }
    }

    let old_end = buf.buffer.len();
    let upper_extra_bytes = match max {
        None => 3,
        Some(max_val) if max_val > 1 => 5,
        Some(_) => 0,
    };
    let mut emitted_end = old_end;
    let mut startoffset = 0usize;

    if min == 0 {
        insert_jump(
            buf,
            laststart,
            RegexOp::OnFailureJumpLoop,
            old_end + 3 + upper_extra_bytes,
        )?;
        emitted_end += 3;
    } else {
        insert_jump2(
            buf,
            laststart,
            RegexOp::SucceedN,
            old_end + 5 + upper_extra_bytes,
            min,
        )?;
        emitted_end += 5;
        insert_set_number_at(buf, laststart, 5, min)?;
        emitted_end += 5;
        startoffset += 5;
    }

    match max {
        None => {
            let op_pos = emitted_end;
            buf.buffer.extend_from_slice(&[RegexOp::Jump as u8, 0, 0]);
            store_jump_at(
                &mut buf.buffer,
                op_pos,
                RegexOp::Jump,
                laststart + startoffset,
            )?;
        }
        Some(max_val) if max_val > 1 => {
            let op_pos = emitted_end;
            buf.buffer
                .extend_from_slice(&[RegexOp::JumpN as u8, 0, 0, 0, 0]);
            store_jump2_at(
                &mut buf.buffer,
                op_pos,
                RegexOp::JumpN,
                laststart + startoffset,
                max_val - 1,
            )?;
            emitted_end += 5;
            insert_set_number_at(buf, laststart, emitted_end - laststart, max_val - 1)?;
        }
        Some(_) => {}
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Phase 3: Matcher (re_match_2_internal)
//
// Translates GNU regex-emacs.c:4072-5340.
// Executes compiled bytecode against input text with backtracking.
// ---------------------------------------------------------------------------

/// Context for syntax-table and category-table queries during matching.
///
/// The matcher queries the syntax table to implement `\w`, `\b`, `\sC`, etc.
/// In GNU Emacs, this is done via the `SYNTAX()` macro which reads from
/// `gl_state.current_syntax_table`.
/// Identity of a syntax lookup for the compiled-pattern caches.
///
/// GNU `search.c` keeps this as `regexp_cache.syntax_table` — the actual
/// syntax-table object for `used_syntax` patterns, `Qt` for
/// table-independent ones — and compares it with `BASE_EQ` against the
/// current buffer's table on every cache probe (search.c:222-224).
/// The `epoch` component stands in for GNU's `clear_regexp_cache` call
/// on `modify-syntax-entry`: bumping it strands every entry baked
/// against the old table contents.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SyntaxCacheKey {
    /// The immutable built-in standard mapping (`DefaultSyntaxLookup`).
    Standard,
    /// A live syntax chartable: object identity bits + the global
    /// syntax-table mutation epoch at key time.
    Table { id: usize, epoch: u64 },
}

/// GNU's category-aware policy for boundaries between two word constituents.
///
/// `regex-emacs.c` delegates this decision to `WORD_BOUNDARY_P`, which reads
/// `char-script-table`, `word-combining-categories`, and
/// `word-separating-categories`.  Keep those Lisp values at the matcher
/// boundary instead of reducing them to Unicode ranges so dynamic bindings and
/// user mutations retain their GNU-visible effect.
#[derive(Clone, Copy)]
pub(crate) struct WordBoundaryLookup {
    char_script_table: Option<Value>,
    word_combining_categories: Value,
    word_separating_categories: Value,
}

impl Default for WordBoundaryLookup {
    fn default() -> Self {
        Self {
            char_script_table: None,
            word_combining_categories: Value::NIL,
            word_separating_categories: Value::NIL,
        }
    }
}

impl WordBoundaryLookup {
    pub(crate) fn new(
        char_script_table: Option<Value>,
        word_combining_categories: Value,
        word_separating_categories: Value,
    ) -> Self {
        Self {
            char_script_table,
            word_combining_categories,
            word_separating_categories,
        }
    }

    fn script_at(&self, c: char) -> Value {
        self.char_script_table
            .and_then(|table| crate::emacs_core::chartable::ct_lookup(&table, c as i64).ok())
            .unwrap_or(Value::NIL)
    }

    fn category_pair_matches(pair: Value, c1: char, c2: char, syntax: &dyn SyntaxLookup) -> bool {
        if !pair.is_cons() {
            return false;
        }

        let first = pair.cons_car();
        let second = pair.cons_cdr();
        let first_matches = first.is_nil()
            || first.as_fixnum().is_some_and(|category| {
                let Ok(category @ 0x20..=0x7e) = u8::try_from(category) else {
                    return false;
                };
                syntax.char_has_category(c1, category) && !syntax.char_has_category(c2, category)
            });
        let second_matches = second.is_nil()
            || second.as_fixnum().is_some_and(|category| {
                let Ok(category @ 0x20..=0x7e) = u8::try_from(category) else {
                    return false;
                };
                !syntax.char_has_category(c1, category) && syntax.char_has_category(c2, category)
            });
        first_matches && second_matches
    }

    fn boundary_between(&self, c1: char, c2: char, syntax: &dyn SyntaxLookup) -> bool {
        // GNU `WORD_BOUNDARY_P` never separates two ASCII/Latin-1 word
        // constituents, even if their script-table entries differ.
        if (c1 as u32) <= 0xff && (c2 as u32) <= 0xff {
            return false;
        }

        let same_script = self.script_at(c1) == self.script_at(c2);
        let mut categories = if same_script {
            self.word_separating_categories
        } else {
            self.word_combining_categories
        };
        let default_result = !same_script;

        while categories.is_cons() {
            if Self::category_pair_matches(categories.cons_car(), c1, c2, syntax) {
                return !default_result;
            }
            categories = categories.cons_cdr();
        }
        default_result
    }
}

pub(crate) trait SyntaxLookup {
    /// Return the syntax class of character `c` in the current syntax table.
    fn char_syntax(&self, c: char) -> SyntaxClass;

    /// Return the syntax class at byte `input_pos` in the matcher input.
    ///
    /// String matching and compile-time fastmaps use the position-independent
    /// default.  Buffer matching overrides this to honor `syntax-table` text
    /// properties at the corresponding absolute buffer byte.
    fn char_syntax_at(&self, c: char, input_pos: usize) -> SyntaxClass {
        let _ = input_pos;
        self.char_syntax(c)
    }

    /// Return true if character `c` belongs to category `cat`.
    fn char_has_category(&self, c: char, cat: u8) -> bool;

    /// Return true when two adjacent word constituents have a GNU word
    /// boundary between them because of their scripts/categories.
    fn word_boundary_between(&self, _c1: char, _c2: char) -> bool {
        false
    }

    /// Cache identity of this lookup (see [`SyntaxCacheKey`]).  Used by
    /// the front-end pattern caches to key `used_syntax` entries by
    /// syntax table, mirroring GNU `compile_pattern`.
    fn cache_key(&self) -> SyntaxCacheKey;
}

/// Default syntax lookup — uses GNU's standard syntax-table definitions.
/// This is used when no buffer-specific syntax table is available
/// (e.g. in unit tests or string-only matching).
pub(crate) struct DefaultSyntaxLookup;

/// Syntax lookup backed by a buffer's actual syntax table.
/// Used when regex searching within a buffer context.
#[derive(Clone, Copy)]
pub(crate) struct BufferSyntaxLookup {
    pub syntax_table: crate::emacs_core::syntax::SyntaxTable,
    pub category_table: Option<crate::emacs_core::value::Value>,
    pub word_boundary: WordBoundaryLookup,
}

impl SyntaxLookup for DefaultSyntaxLookup {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        crate::emacs_core::syntax::standard_syntax_class_for_char(c)
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        default_char_has_category(c, cat)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        // `standard_syntax_class_for_char` is a hardwired mapping — no
        // chartable, no mutation — so entries baked against it stay
        // valid forever (GNU `Qt` for the standard-classification case).
        SyntaxCacheKey::Standard
    }
}

impl SyntaxLookup for BufferSyntaxLookup {
    fn char_syntax(&self, c: char) -> SyntaxClass {
        self.syntax_table.char_syntax(c)
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        self.category_table
            .and_then(|table| {
                crate::emacs_core::category::char_has_category_in_table(table, c, cat).ok()
            })
            .unwrap_or_else(|| default_char_has_category(c, cat))
    }

    fn word_boundary_between(&self, c1: char, c2: char) -> bool {
        self.word_boundary.boundary_between(c1, c2, self)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        SyntaxCacheKey::Table {
            id: self.syntax_table.chartable().bits(),
            epoch: crate::emacs_core::syntax::syntax_table_mutation_epoch(),
        }
    }
}

/// Return whether character `c` belongs to the GNU regex category
/// `cat` (`\cX`).
///
/// GNU's category mechanism (`src/category.c`) gives each character
/// a 128-bit set of category memberships, populated at startup
/// time from `lisp/international/characters.el`. We don't ship the
/// full table; instead we hardcode the most common categories using
/// Unicode block ranges. The category mnemonics here are taken
/// directly from `lisp/international/characters.el` (the GNU
/// `(define-category ?X "...")` lines starting at line 37).
///
/// Audit finding #6 in `drafts/regex-search-audit.md` flagged that
/// only `\c|` worked. This implementation covers the categories the
/// CJK font-lock and bidi paths actually use.
fn default_char_has_category(c: char, cat: u8) -> bool {
    let cp = c as u32;
    match cat {
        // |  -- "line breakable". GNU's `characters.el` adds this
        // for most CJK and fullwidth ranges; we use the practical
        // shortcut of "any non-ASCII char" which is what neomacs
        // historically returned.
        b'|' => !c.is_ascii(),

        // a  -- ASCII. GNU `lisp/international/characters.el` assigns
        // category `a` to codepoints 32..127, not ASCII controls.
        b'a' => (0x20..=0x7f).contains(&cp),

        // A  -- 2-byte alnum. GNU populates this from CJK Latin /
        // fullwidth ASCII ranges. The practical shortcut is the
        // fullwidth ASCII alphanumeric block.
        b'A' => matches!(cp, 0xFF10..=0xFF19 | 0xFF21..=0xFF3A | 0xFF41..=0xFF5A),

        // l  -- Latin (a-z, A-Z and Latin-1/Extended letters).
        // r  -- Roman (Japanese context, same effective range).
        b'l' | b'r' => {
            c.is_ascii_alphabetic()
                || matches!(cp, 0x00C0..=0x00FF | 0x0100..=0x024F | 0x1E00..=0x1EFF)
        }

        // g  -- Greek (Greek and Coptic block).
        b'g' => matches!(cp, 0x0370..=0x03FF | 0x1F00..=0x1FFF),

        // G  -- 2-byte Greek (fullwidth Greek). Rare; use the
        // same practical bounds as `g` for now.
        b'G' => matches!(cp, 0x0370..=0x03FF | 0x1F00..=0x1FFF),

        // y  -- Cyrillic.
        b'y' | b'Y' => matches!(cp, 0x0400..=0x052F),

        // b  -- Arabic.
        b'b' => matches!(cp, 0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF),

        // w  -- Hebrew.
        b'w' => matches!(cp, 0x0590..=0x05FF | 0xFB1D..=0xFB4F),

        // t  -- Thai.
        b't' => matches!(cp, 0x0E00..=0x0E7F),

        // o  -- Lao.
        b'o' => matches!(cp, 0x0E80..=0x0EFF),

        // q  -- Tibetan.
        b'q' => matches!(cp, 0x0F00..=0x0FFF),

        // i  -- Indian (Devanagari + related). GNU's actual table
        // covers more scripts; this is the most common one.
        b'i' => matches!(cp, 0x0900..=0x097F),

        // I  -- Indian glyphs (broader Indic blocks).
        b'I' => matches!(cp, 0x0900..=0x0DFF),

        // e  -- Ethiopic (Ge'ez).
        b'e' => matches!(cp, 0x1200..=0x137F),

        // v  -- Vietnamese (Latin Extended Additional).
        b'v' => matches!(cp, 0x1E00..=0x1EFF),

        // h  -- Korean (Hangul Syllables + Jamo).
        // N  -- 2-byte Korean (same range here).
        b'h' | b'N' => {
            matches!(cp, 0x1100..=0x11FF | 0xAC00..=0xD7A3 | 0xA960..=0xA97F | 0xD7B0..=0xD7FF)
        }

        // c  -- Chinese / Han ideographs (broad).
        // C  -- 2-byte han (slightly narrower set).
        b'c' | b'C' => matches!(
            cp,
            0x3400..=0x4DBF
                | 0x4E00..=0x9FFF
                | 0xF900..=0xFAFF
                | 0x20000..=0x2FFFF
                | 0x30000..=0x323AF
        ),

        // H  -- Hiragana (Japanese).
        b'H' => matches!(cp, 0x3040..=0x309F | 0x1B000..=0x1B16F),

        // K  -- Katakana (Japanese).
        b'K' => matches!(
            cp,
            0x3099..=0x309C | 0x30A0..=0x30FF | 0x31F0..=0x31FF | 0x1AFF0..=0x1B16F
        ),

        // k  -- Katakana (lowercase mnemonic, same coverage).
        b'k' => matches!(cp, 0x30A0..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F),

        // j  -- Japanese (Hiragana + Katakana + half-width Katakana
        // + CJK punctuation + fullwidth ASCII).
        b'j' => matches!(
            cp,
            0x3000..=0x303F
                | 0x3040..=0x309F
                | 0x30A0..=0x30FF
                | 0xFF00..=0xFFEF
        ),

        // .  -- Base (Unicode L,N,P,S,Zs).
        b'.' => match c.is_ascii() {
            true => c.is_ascii_graphic() || c == ' ',
            false => {
                !matches!(cp, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
            }
        },

        // ^  -- Combining diacritic / mark (Unicode M).
        b'^' => {
            matches!(cp, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
        }

        // R  -- Strong R2L (right-to-left). Practical heuristic:
        // Hebrew and Arabic ranges.
        b'R' => matches!(cp, 0x0590..=0x05FF | 0x0600..=0x06FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF),

        // L  -- Strong L2R (everything else).
        b'L' => {
            !matches!(cp, 0x0590..=0x05FF | 0x0600..=0x06FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
        }

        // 6  -- digit (numeric).
        b'6' => c.is_numeric(),

        // Categories we don't recognize fall through as "no
        // membership" — same as GNU's behavior for an unset bit.
        _ => false,
    }
}

fn unicode_blank_char(c: char) -> bool {
    matches!(
        c,
        '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

fn unicode_line_or_paragraph_separator(c: char) -> bool {
    matches!(c, '\u{2028}' | '\u{2029}')
}

fn unicode_graphic_char(c: char) -> bool {
    !unicode_blank_char(c) && !unicode_line_or_paragraph_separator(c) && !c.is_control()
}

fn unicode_printable_char(c: char) -> bool {
    !c.is_control()
}

/// Emacs character code for a unibyte byte (raw bytes 0x80..=0xFF map to
/// the byte8 range). Mirrors GNU `RE_CHAR_TO_MULTIBYTE`.
fn regex_unibyte_to_char(byte: u8) -> u32 {
    if byte < 0x80 {
        byte as u32
    } else {
        emacs_char::unibyte_to_char(byte)
    }
}

/// Rust `char` used for syntax-table lookups of an Emacs character code.
/// Byte8 codes collapse to their raw byte so `char_syntax` sees the same
/// 0x80..=0xFF index GNU's syntax table uses for eight-bit characters.
fn regex_syntax_char(code: u32) -> char {
    if emacs_char::char_byte8_p(code) {
        char::from(emacs_char::char_to_byte8(code))
    } else {
        char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER)
    }
}

/// Unibyte byte for an Emacs character code, if one exists.
/// Mirrors GNU `RE_CHAR_TO_UNIBYTE`.
fn regex_char_to_unibyte(code: u32) -> Option<u8> {
    if code < 0x80 || emacs_char::char_byte8_p(code) {
        Some(emacs_char::char_to_byte8(code))
    } else {
        None
    }
}

fn gnu_alphabetic_char(code: u32) -> bool {
    emacs_char::char_general_category(code).is_some_and(emacs_char::alphabeticp)
}

fn gnu_alphanumeric_char(code: u32) -> bool {
    emacs_char::char_general_category(code).is_some_and(emacs_char::alphanumericp)
}

/// Runtime POSIX character-class membership for charset `class_bits`.
///
/// Mirrors GNU `re_iswctype` (regex-emacs.c) as consulted by
/// `execute_charset` for the range-table class bits.  The syntax-table
/// dependent classes are `word` and `space` (GNU regex-emacs.c:151
/// `ISSPACE` / `ISWORD` route through `SYNTAX (c)`); everything else is
/// a fixed predicate.  Shared by the matcher (per-character class test)
/// and `compile_fastmap` (baking the ASCII members of a leading class
/// into the fastmap).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PosixClassCaseMode {
    Sensitive,
    Folded,
}

impl PosixClassCaseMode {
    fn from_translation(translate: Option<&CaseTranslation>) -> Self {
        match translate {
            Some(_) => Self::Folded,
            None => Self::Sensitive,
        }
    }

    fn matches_lower(self, ch: char) -> bool {
        match self {
            Self::Sensitive => ch.is_lowercase(),
            Self::Folded => ch.is_lowercase() || ch.is_uppercase(),
        }
    }

    fn matches_upper(self, ch: char) -> bool {
        match self {
            Self::Sensitive => ch.is_uppercase(),
            Self::Folded => ch.is_uppercase() || ch.is_lowercase(),
        }
    }
}

fn posix_class_matches(
    code: u32,
    bits: u32,
    input_pos: Option<usize>,
    syntax: &dyn SyntaxLookup,
    case_mode: PosixClassCaseMode,
) -> bool {
    let byte = regex_char_to_unibyte(code);
    let ch = regex_syntax_char(code);
    let char_syntax = || {
        input_pos.map_or_else(
            || syntax.char_syntax(ch),
            |position| syntax.char_syntax_at(ch, position),
        )
    };
    let is_real_ascii = code < 0x80;
    let ascii_alnum = |b: u8| b.is_ascii_alphabetic() || b.is_ascii_digit();
    let ascii_alpha = |b: u8| b.is_ascii_alphabetic();

    (bits & CHARSET_CLASS_BIT_ALNUM != 0
        && if is_real_ascii {
            ascii_alnum(code as u8)
        } else {
            gnu_alphanumeric_char(code)
        })
        || (bits & CHARSET_CLASS_BIT_ALPHA != 0
            && if is_real_ascii {
                ascii_alpha(code as u8)
            } else {
                gnu_alphabetic_char(code)
            })
        || (bits & CHARSET_CLASS_BIT_BLANK != 0
            && if is_real_ascii {
                matches!(code, 0x20 | 0x09)
            } else {
                unicode_blank_char(ch)
            })
        || (bits & CHARSET_CLASS_BIT_CNTRL != 0 && code < 0x20)
        || (bits & CHARSET_CLASS_BIT_DIGIT != 0 && is_real_ascii && (code as u8).is_ascii_digit())
        || (bits & CHARSET_CLASS_BIT_GRAPH != 0
            && byte.map_or_else(
                || unicode_graphic_char(ch),
                |b| b > b' ' && !(0x7f..=0xa0).contains(&b),
            ))
        || (bits & CHARSET_CLASS_BIT_LOWER != 0 && case_mode.matches_lower(ch))
        || (bits & CHARSET_CLASS_BIT_PRINT != 0
            && byte.map_or_else(
                || unicode_printable_char(ch),
                |b| b >= b' ' && !(0x7f..=0x9f).contains(&b),
            ))
        || (bits & CHARSET_CLASS_BIT_PUNCT != 0
            && if is_real_ascii {
                let b = code as u8;
                b > b' ' && b < 0x7f && !ascii_alnum(b)
            } else {
                char_syntax() != SyntaxClass::Word
            })
        || (bits & CHARSET_CLASS_BIT_SPACE != 0 && char_syntax() == SyntaxClass::Whitespace)
        || (bits & CHARSET_CLASS_BIT_UPPER != 0 && case_mode.matches_upper(ch))
        || (bits & CHARSET_CLASS_BIT_XDIGIT != 0
            && is_real_ascii
            && (code as u8).is_ascii_hexdigit())
        || (bits & CHARSET_CLASS_BIT_ASCII != 0 && is_real_ascii)
        || (bits & CHARSET_CLASS_BIT_WORD != 0 && char_syntax() == SyntaxClass::Word)
        || (bits & CHARSET_CLASS_BIT_NONASCII != 0 && !is_real_ascii)
        || (bits & CHARSET_CLASS_BIT_UNIBYTE != 0 && byte.is_some())
        || (bits & CHARSET_CLASS_BIT_MULTIBYTE != 0 && byte.is_none())
}

// ---------------------------------------------------------------------------
// Shared per-op character tests.
//
// These free functions hold the EXACT per-character predicates that both
// the backtracking matcher (`re_match_internal`) and the non-backtracking
// Pike VM (`pike_match`) execute.  Sharing one implementation is what
// guarantees the Pike fast path is byte-exact with the backtracker on
// case-fold, multibyte, charsets, syntax classes, categories and word /
// symbol boundaries — every subtlety lives in one place.
// ---------------------------------------------------------------------------

/// Case-fold translate, mirroring the `tr` closure in `re_match_internal`.
#[inline]
fn re_tr(translate: &Option<CaseTranslation>, c: u32) -> u32 {
    match translate {
        Some(table) => table.translate(c),
        None => c,
    }
}

/// Decode the Emacs character at `pos` (mirrors the `text_char` closure).
#[inline]
fn re_text_char(text: &[u8], pos: usize, target_multibyte: bool) -> Option<(u32, usize)> {
    if pos >= text.len() {
        return None;
    }
    if target_multibyte {
        Some(emacs_char::string_char(&text[pos..]))
    } else {
        Some((regex_unibyte_to_char(text[pos]), 1))
    }
}

/// Start byte of the character before `pos` (mirrors `prev_char_start`).
#[inline]
fn re_prev_char_start(text: &[u8], pos: usize, target_multibyte: bool) -> Option<usize> {
    if pos == 0 {
        return None;
    }
    if !target_multibyte {
        return Some(pos - 1);
    }
    let mut p = pos - 1;
    while p > 0 && (text[p] & 0xC0) == 0x80 {
        p -= 1;
    }
    Some(p)
}

/// `AnyChar` (`.`): match any character except newline.  Returns the byte
/// length of the consumed char on success.  Mirrors the `AnyChar` arm.
#[inline]
fn match_anychar_at(
    text: &[u8],
    d: usize,
    stop: usize,
    target_multibyte: bool,
    translate: &Option<CaseTranslation>,
) -> Option<usize> {
    if d >= stop {
        return None;
    }
    let (buf_ch, buf_len) = re_text_char(text, d, target_multibyte)?;
    if re_tr(translate, buf_ch) == '\n' as u32 {
        return None;
    }
    Some(buf_len)
}

/// Match ONE character of an `Exactn` literal at literal byte offset
/// `lit_off` against the input char at `d`.  Returns `(pattern_advance,
/// text_advance)` on success.  Mirrors one iteration of the `Exactn` arm's
/// inner loop, including the unibyte/multibyte + case-fold subtleties.
#[inline]
#[allow(clippy::too_many_arguments)] // mirrors the backtracker's Exactn arm state
fn match_exactn_char_at(
    lit: &[u8],
    lit_off: usize,
    pattern_multibyte: bool,
    target_multibyte: bool,
    translate: &Option<CaseTranslation>,
    text: &[u8],
    d: usize,
    stop: usize,
) -> Option<(usize, usize)> {
    if d >= stop {
        return None;
    }
    // ASCII fast path — an ASCII literal byte is the same character in
    // every representation mix (pattern/text, unibyte/multibyte), always
    // one byte long on both sides; when the text byte's case translation
    // also stays ASCII the whole test is one translate + one compare.
    // Translations that leave ASCII (exotic case tables) fall through to
    // the full path below, which recomputes from scratch.
    let ascii_pat_byte = lit[lit_off];
    if ascii_pat_byte < 0x80
        && let Some(&text_byte) = text.get(d)
        && text_byte < 0x80
    {
        let translated = re_tr(translate, text_byte as u32);
        if translated < 0x80 {
            return if translated as u8 == ascii_pat_byte {
                Some((1, 1))
            } else {
                None
            };
        }
    }
    let (buf_ch, buf_len) = re_text_char(text, d, target_multibyte)?;
    if target_multibyte {
        let (pat_ch, pat_len) = if pattern_multibyte {
            emacs_char::string_char(&lit[lit_off..])
        } else {
            (regex_unibyte_to_char(lit[lit_off]), 1)
        };
        if re_tr(translate, buf_ch) != pat_ch {
            return None;
        }
        Some((pat_len, buf_len))
    } else {
        let (pat_byte, pat_advance) = if pattern_multibyte {
            let (pat_ch, pat_len) = emacs_char::string_char(&lit[lit_off..]);
            let byte = regex_char_to_unibyte(pat_ch)?;
            (byte, pat_len)
        } else {
            (lit[lit_off], 1)
        };
        let buf_byte = text[d];
        let mut translated = regex_unibyte_to_char(buf_byte);
        if !emacs_char::char_byte8_p(translated) {
            translated = re_tr(translate, translated);
            if let Some(byte) = regex_char_to_unibyte(translated) {
                translated = byte as u32;
            } else {
                translated = buf_byte as u32;
            }
        } else {
            translated = buf_byte as u32;
        }
        if translated as u8 != pat_byte {
            return None;
        }
        Some((pat_advance, 1))
    }
}

/// `Charset` / `CharsetNot`: match the char at `d` against the bitmap +
/// range-table + POSIX class bits of the charset opcode at
/// `charset_op_pos`.  Returns consumed byte length on success.  Mirrors
/// the `Charset | CharsetNot` arm exactly.
/// Charset test at `d`. The ASCII case — GNU's charset op shape: read the
/// byte, case-translate it, bit-test the bitmap — inlines into both matcher
/// call sites; everything else goes through the outlined
/// [`match_charset_at_slow`]. The call boundary itself measured ~40 of the
/// ~67 Ir per test, on millions of tests per workload.
#[allow(clippy::too_many_arguments)] // mirrors GNU execute_charset's inputs
#[inline(always)]
fn match_charset_at(
    pattern: &CompiledPattern,
    charset_op_pos: usize,
    text: &[u8],
    d: usize,
    stop: usize,
    target_multibyte: bool,
    translate: &Option<CaseTranslation>,
    syntax: &dyn SyntaxLookup,
) -> Option<usize> {
    let bytecode = &pattern.buffer;

    if d >= stop {
        return None;
    }

    // ASCII fast path — an ASCII byte decodes to itself in both text
    // representations, always has length 1, and (when its translation
    // stays ASCII) needs none of the slow path's decode /
    // unibyte-conversion / case-mode machinery. The class check mirrors
    // the slow path exactly: classes test the UNTRANSLATED character.
    // (`stop` may exceed `text.len()`; a past-the-end read is a plain
    // no-match, matching `re_text_char`'s bounds behavior.)
    let Some(&first_byte) = text.get(d) else {
        return None;
    };
    if first_byte < 0x80 {
        let translated = re_tr(translate, first_byte as u32);
        if translated < 0x80 {
            let negate = bytecode[charset_op_pos] == RegexOp::CharsetNot as u8;
            let bitmap_len = bytecode[charset_op_pos + 1] as usize & 0x7F;
            let bitmap_start = charset_op_pos + 2;
            let c = translated as usize;
            let bitmap_hit =
                (c / 8) < bitmap_len && (bytecode[bitmap_start + c / 8] >> (c % 8)) & 1 != 0;
            let in_set = bitmap_hit
                || pattern
                    .charset_class_bits
                    .get(&charset_op_pos)
                    .copied()
                    .map(|bits| {
                        let class_case_mode =
                            PosixClassCaseMode::from_translation(translate.as_ref());
                        posix_class_matches(
                            first_byte as u32,
                            bits,
                            Some(d),
                            syntax,
                            class_case_mode,
                        )
                    })
                    .unwrap_or(false);
            return if negate != in_set { Some(1) } else { None };
        }
    }

    match_charset_at_slow(
        pattern,
        charset_op_pos,
        text,
        d,
        stop,
        target_multibyte,
        translate,
        syntax,
    )
}

/// Non-ASCII / translation-escapes-ASCII remainder of the charset test.
#[allow(clippy::too_many_arguments)] // mirrors the wrapper's seam
#[inline(never)]
fn match_charset_at_slow(
    pattern: &CompiledPattern,
    charset_op_pos: usize,
    text: &[u8],
    d: usize,
    stop: usize,
    target_multibyte: bool,
    translate: &Option<CaseTranslation>,
    syntax: &dyn SyntaxLookup,
) -> Option<usize> {
    let bytecode = &pattern.buffer;
    let negate = bytecode[charset_op_pos] == RegexOp::CharsetNot as u8;
    let bitmap_len = bytecode[charset_op_pos + 1] as usize & 0x7F;
    let bitmap_start = charset_op_pos + 2;

    if d >= stop {
        return None;
    }

    let class_case_mode = PosixClassCaseMode::from_translation(translate.as_ref());
    let (orig_ch, ch_len) = re_text_char(text, d, target_multibyte)?;
    let mut ch = orig_ch;
    let mut unibyte_char = false;

    if target_multibyte {
        ch = re_tr(translate, ch);
        if let Some(byte) = regex_char_to_unibyte(ch) {
            unibyte_char = true;
            ch = byte as u32;
        }
    } else {
        let mut converted = regex_unibyte_to_char(text[d]);
        if !emacs_char::char_byte8_p(converted) {
            converted = re_tr(translate, converted);
            if let Some(byte) = regex_char_to_unibyte(converted) {
                unibyte_char = true;
                ch = byte as u32;
            }
        } else {
            unibyte_char = true;
            ch = text[d] as u32;
        }
    }

    let in_set = if unibyte_char {
        let c = ch as usize;
        let bitmap_hit = if (c / 8) < bitmap_len {
            let byte = bytecode[bitmap_start + c / 8];
            (byte >> (c % 8)) & 1 != 0
        } else {
            false
        };
        bitmap_hit
            || (ch < 0x80
                && pattern
                    .charset_class_bits
                    .get(&charset_op_pos)
                    .copied()
                    .map(|bits| {
                        posix_class_matches(orig_ch, bits, Some(d), syntax, class_case_mode)
                    })
                    .unwrap_or(false))
    } else {
        let range_hit = pattern
            .multibyte_charsets
            .get(&charset_op_pos)
            .map(|ranges| {
                let ch = regex_syntax_char(ch);
                ranges.iter().any(|&(lo, hi)| ch >= lo && ch <= hi)
            })
            .unwrap_or(false);
        range_hit
            || pattern
                .charset_class_bits
                .get(&charset_op_pos)
                .copied()
                .map(|bits| posix_class_matches(orig_ch, bits, Some(d), syntax, class_case_mode))
                .unwrap_or(false)
    };

    let matched = if negate { !in_set } else { in_set };
    if matched { Some(ch_len) } else { None }
}

/// `SyntaxSpec` (`negate=false`) / `NotSyntaxSpec` (`negate=true`).
#[inline]
fn match_syntaxspec_at(
    class_byte: u8,
    negate: bool,
    text: &[u8],
    d: usize,
    stop: usize,
    target_multibyte: bool,
    syntax: &dyn SyntaxLookup,
) -> Option<usize> {
    if d >= stop {
        return None;
    }
    let (c, len) = re_text_char(text, d, target_multibyte)?;
    let is = syntax.char_syntax_at(regex_syntax_char(c), d) as u8 == class_byte;
    if is != negate { Some(len) } else { None }
}

/// `SyntaxSpecSet`: char's syntax class is in the bitmask.
#[inline]
fn match_syntaxspecset_at(
    mask: u16,
    text: &[u8],
    d: usize,
    stop: usize,
    target_multibyte: bool,
    syntax: &dyn SyntaxLookup,
) -> Option<usize> {
    if d >= stop {
        return None;
    }
    let (c, len) = re_text_char(text, d, target_multibyte)?;
    let class = syntax.char_syntax_at(regex_syntax_char(c), d) as u16;
    if (mask >> class) & 1 != 0 {
        Some(len)
    } else {
        None
    }
}

/// `CategorySpec` (`negate=false`) / `NotCategorySpec` (`negate=true`).
#[inline]
fn match_categoryspec_at(
    cat: u8,
    negate: bool,
    text: &[u8],
    d: usize,
    stop: usize,
    target_multibyte: bool,
    syntax: &dyn SyntaxLookup,
) -> Option<usize> {
    if d >= stop {
        return None;
    }
    let (c, len) = re_text_char(text, d, target_multibyte)?;
    let has = syntax.char_has_category(regex_syntax_char(c), cat);
    if has != negate { Some(len) } else { None }
}

/// Character and syntax class at `pos`.  Keeping the character lets GNU word
/// boundaries distinguish adjacent word constituents from different scripts.
#[inline]
fn re_char_and_syntax(
    text: &[u8],
    pos: usize,
    target_multibyte: bool,
    syntax: &dyn SyntaxLookup,
) -> Option<(char, SyntaxClass)> {
    re_text_char(text, pos, target_multibyte).map(|(c, _)| {
        let c = regex_syntax_char(c);
        (c, syntax.char_syntax_at(c, pos))
    })
}

/// Is the char at `pos` a word OR symbol constituent?  (`false` past the ends.)
#[inline]
fn re_char_is_symbol(
    text: &[u8],
    pos: usize,
    target_multibyte: bool,
    syntax: &dyn SyntaxLookup,
) -> bool {
    re_text_char(text, pos, target_multibyte)
        .map(|(c, _)| {
            let s = syntax.char_syntax_at(regex_syntax_char(c), pos);
            s == SyntaxClass::Word || s == SyntaxClass::Symbol
        })
        .unwrap_or(false)
}

/// `\b` word boundary (mirrors GNU `regex-emacs.c` `case wordbound`).
#[inline]
fn assert_word_boundary(
    text: &[u8],
    d: usize,
    target_multibyte: bool,
    syntax: &dyn SyntaxLookup,
) -> bool {
    // GNU Case 1 (regex-emacs.c:4972-4974): a position at the beginning or end
    // of the searched region is *unconditionally* a word boundary — `\b`
    // succeeds and `\B` fails there, regardless of the adjacent character's
    // word syntax.  The previous neighbour-only computation treated the missing
    // edge character as non-word, so `\b` wrongly failed (and `\B` matched) at
    // the edges of a string / accessible buffer region whenever the adjacent
    // character was not a word constituent (e.g. `\b` on "." or the empty
    // string).  `d == 0` is AT_STRINGS_BEG and `d == text.len()` is
    // AT_STRINGS_END (the same edges `BegBuf` / `EndBuf` use).
    if d == 0 || d == text.len() {
        return true;
    }
    let previous = re_prev_char_start(text, d, target_multibyte)
        .and_then(|p| re_char_and_syntax(text, p, target_multibyte, syntax));
    let current = re_char_and_syntax(text, d, target_multibyte, syntax);
    match (previous, current) {
        (Some((c1, SyntaxClass::Word)), Some((c2, SyntaxClass::Word))) => {
            syntax.word_boundary_between(c1, c2)
        }
        (Some((_, previous)), Some((_, current))) => {
            (previous == SyntaxClass::Word) != (current == SyntaxClass::Word)
        }
        _ => false,
    }
}

/// `\<` word beginning.
#[inline]
fn assert_word_beg(
    text: &[u8],
    d: usize,
    target_multibyte: bool,
    syntax: &dyn SyntaxLookup,
) -> bool {
    let Some((c2, SyntaxClass::Word)) = re_char_and_syntax(text, d, target_multibyte, syntax)
    else {
        return false;
    };
    let Some(previous) = re_prev_char_start(text, d, target_multibyte)
        .and_then(|p| re_char_and_syntax(text, p, target_multibyte, syntax))
    else {
        return true;
    };
    match previous {
        (_, class) if class != SyntaxClass::Word => true,
        (c1, SyntaxClass::Word) => syntax.word_boundary_between(c1, c2),
        _ => false,
    }
}

/// `\>` word end.
#[inline]
fn assert_word_end(
    text: &[u8],
    d: usize,
    target_multibyte: bool,
    syntax: &dyn SyntaxLookup,
) -> bool {
    let Some(previous) = re_prev_char_start(text, d, target_multibyte)
        .and_then(|p| re_char_and_syntax(text, p, target_multibyte, syntax))
    else {
        return false;
    };
    let (c1, SyntaxClass::Word) = previous else {
        return false;
    };
    let Some(current) = re_char_and_syntax(text, d, target_multibyte, syntax) else {
        return true;
    };
    match current {
        (_, class) if class != SyntaxClass::Word => true,
        (c2, SyntaxClass::Word) => syntax.word_boundary_between(c1, c2),
        _ => false,
    }
}

/// `\_<` symbol beginning.
#[inline]
fn assert_sym_beg(
    text: &[u8],
    d: usize,
    target_multibyte: bool,
    syntax: &dyn SyntaxLookup,
) -> bool {
    let prev_sym = re_prev_char_start(text, d, target_multibyte)
        .map(|p| re_char_is_symbol(text, p, target_multibyte, syntax))
        .unwrap_or(false);
    let curr_sym = re_char_is_symbol(text, d, target_multibyte, syntax);
    !prev_sym && curr_sym
}

/// `\_>` symbol end.
#[inline]
fn assert_sym_end(
    text: &[u8],
    d: usize,
    target_multibyte: bool,
    syntax: &dyn SyntaxLookup,
) -> bool {
    let prev_sym = re_prev_char_start(text, d, target_multibyte)
        .map(|p| re_char_is_symbol(text, p, target_multibyte, syntax))
        .unwrap_or(false);
    let curr_sym = re_char_is_symbol(text, d, target_multibyte, syntax);
    prev_sym && !curr_sym
}

/// Zero-width syntax assertions and their GNU match-limit protocol.
///
/// GNU's `wordbeg` and `symbeg` opcodes use the ordinary `PREFETCH`, so
/// inspecting the current character fails when `d == stop`.  Boundaries and
/// end assertions use `PREFETCH_NOLIMIT` (or only inspect the previous
/// character).  Keeping that distinction in one exhaustive enum evaluator
/// prevents the backtracking and Pike engines from choosing independently.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyntaxAssertion {
    WordBoundary,
    NotWordBoundary,
    WordBeginning,
    WordEnd,
    SymbolBeginning,
    SymbolEnd,
}

impl SyntaxAssertion {
    fn from_regex_op(op: RegexOp) -> Self {
        match op {
            RegexOp::WordBound => Self::WordBoundary,
            RegexOp::NotWordBound => Self::NotWordBoundary,
            RegexOp::WordBeg => Self::WordBeginning,
            RegexOp::WordEnd => Self::WordEnd,
            RegexOp::SymBeg => Self::SymbolBeginning,
            RegexOp::SymEnd => Self::SymbolEnd,
            _ => unreachable!("{op:?} is not a zero-width syntax assertion"),
        }
    }
}

#[inline]
fn evaluate_syntax_assertion(
    assertion: SyntaxAssertion,
    text: &[u8],
    d: usize,
    stop: usize,
    target_multibyte: bool,
    syntax: &dyn SyntaxLookup,
) -> bool {
    match assertion {
        SyntaxAssertion::WordBoundary => assert_word_boundary(text, d, target_multibyte, syntax),
        SyntaxAssertion::NotWordBoundary => {
            !assert_word_boundary(text, d, target_multibyte, syntax)
        }
        SyntaxAssertion::WordBeginning => {
            d < stop && assert_word_beg(text, d, target_multibyte, syntax)
        }
        SyntaxAssertion::WordEnd => assert_word_end(text, d, target_multibyte, syntax),
        SyntaxAssertion::SymbolBeginning => {
            d < stop && assert_sym_beg(text, d, target_multibyte, syntax)
        }
        SyntaxAssertion::SymbolEnd => assert_sym_end(text, d, target_multibyte, syntax),
    }
}

/// Match a compiled pattern against input text.
///
/// This is the core matching function, equivalent to GNU's `re_match_2_internal`.
///
/// # Arguments
/// * `pattern` - Compiled bytecode pattern
/// * `text` - Input text to match against
/// * `pos` - Starting position in text
/// * `stop` - End of matching region
/// * `syntax` - Syntax table for `\w`, `\b`, `\sC` etc.
/// * `point` - Buffer point position (for `\=` / AtDot)
///
/// # Returns
/// * `Some(end_pos)` if matched — end position of the match
/// * `None` if no match
pub(crate) fn re_match(
    pattern: &CompiledPattern,
    text: &[u8],
    pos: usize,
    stop: usize,
    syntax: &dyn SyntaxLookup,
    point: usize,
) -> Option<(usize, MatchRegisters)> {
    clear_matcher_overflow();
    re_match_candidate(pattern, text, pos, stop, syntax, point)
}

/// `re_match` without the overflow-flag reset — `re_search` uses this per
/// fastmap candidate so an overflow set by one candidate survives until
/// the search loop checks it.
fn re_match_candidate(
    pattern: &CompiledPattern,
    text: &[u8],
    pos: usize,
    stop: usize,
    syntax: &dyn SyntaxLookup,
    point: usize,
) -> Option<(usize, MatchRegisters)> {
    // Test hook: pin the Pike VM (fuzzer engine side).
    if pattern.pike_eligible && force_pike() {
        return pike_match(pattern, text, pos, stop, syntax, point);
    }

    // Production default: run the backtracker.  For an eligible pattern it
    // runs under a linear step budget so catastrophic backtracking trips the
    // Pike fallback below.  Ineligible patterns (and the forced-backtracker
    // test hook) run the backtracker with no budget — unchanged behaviour.
    let budgeted = pattern.pike_eligible && !force_backtrack();
    let result = MATCH_SCRATCH.with(|cell| match cell.try_borrow_mut() {
        Ok(mut scratch) => re_match_internal(
            &mut scratch,
            pattern,
            text,
            pos,
            stop,
            syntax,
            point,
            budgeted,
        ),
        // Defensive: if a syntax/category callback ever re-enters the
        // matcher, fall back to fresh (allocating) state for the nested
        // match rather than corrupting the outer one.
        Err(_) => re_match_internal(
            &mut MatchScratch::default(),
            pattern,
            text,
            pos,
            stop,
            syntax,
            point,
            budgeted,
        ),
    });
    // The budgeted backtracker gave up on a catastrophic match: recompute it
    // linearly (and byte-exactly) with the Pike VM.
    if budgeted && take_pike_fallback() {
        return pike_match(pattern, text, pos, stop, syntax, point);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn re_match_internal(
    scratch: &mut MatchScratch,
    pattern: &CompiledPattern,
    text: &[u8],
    pos: usize,
    stop: usize,
    syntax: &dyn SyntaxLookup,
    point: usize,
    enable_pike_fallback: bool,
) -> Option<(usize, MatchRegisters)> {
    // SEALED selects unchecked bytecode fetches inside the loop, justified
    // by `validate_sealed_buffer`. Hand-assembled (unsealed) buffers take
    // the fully checked instantiation.
    if pattern.buffer_sealed {
        re_match_loop::<true>(
            scratch,
            pattern,
            text,
            pos,
            stop,
            syntax,
            point,
            enable_pike_fallback,
        )
    } else {
        re_match_loop::<false>(
            scratch,
            pattern,
            text,
            pos,
            stop,
            syntax,
            point,
            enable_pike_fallback,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn re_match_loop<const SEALED: bool>(
    scratch: &mut MatchScratch,
    pattern: &CompiledPattern,
    text: &[u8],
    pos: usize,
    stop: usize,
    syntax: &dyn SyntaxLookup,
    point: usize,
    // When true, abort with `set_pike_fallback()` if backtracking exceeds a
    // linear budget — the caller then re-runs on the Pike VM.  Only set for
    // `pike_eligible` patterns (the fallback is byte-exact there).
    enable_pike_fallback: bool,
) -> Option<(usize, MatchRegisters)> {
    let bytecode = &pattern.buffer;
    let num_regs = pattern.re_nsub + 1;

    // Catastrophic-backtracking budget: a CONSTANT cap on the number of
    // backtracks (failure-stack pops) in a single anchored match.  A
    // well-behaved pattern backtracks a number of times bounded by its own
    // structure (alternation arms, quantifier boundaries) — independent of
    // input length — so it never approaches the cap.  A pathological pattern
    // (`a*a*b`, `.*x` with no `x`) backtracks a number of times that grows
    // with the input, blows past the cap, and is handed to the linear Pike
    // VM.  Counting only at backtracks (not every op) keeps the well-behaved
    // hot path free of per-op overhead.
    const BACKTRACK_BUDGET: u64 = 8_000;
    let backtrack_budget: u64 = if enable_pike_fallback {
        BACKTRACK_BUDGET
    } else {
        u64::MAX
    };
    let mut backtrack_count: u64 = 0;

    let MatchScratch {
        frames,
        undo,
        regstart,
        regend,
        best_regstart,
        best_regend,
        counters,
    } = scratch;

    // Failure (choice-point) stack — GNU's delta protocol: frames record
    // only resume positions; register/counter deltas accumulate in `undo`
    // and are replayed on pop (see `FailFrame`).
    frames.clear();
    undo.clear();

    // Mutable counter table for interval repetition (succeed_n / jump_n /
    // set_number_at).  GNU modifies bytecode in-place; we use a side
    // table keyed by bytecode position.
    counters.clear();

    // GNU regex-emacs.c:4188-4204 skips all internal register arrays
    // when the pattern has no subexpressions. Register 0 is handled
    // separately on success, so failed no-group candidate checks should
    // not allocate register scratch space.
    let has_subexpressions = pattern.re_nsub > 0;
    let scratch_regs = if has_subexpressions { num_regs } else { 0 };
    // The scratch arrays are reused across every match call; when the
    // length already fits, reset in place — four out-of-line
    // SmallVec::resize calls per match measured ~275 Ir of pure setup.
    if regstart.len() == scratch_regs {
        regstart.fill(None);
        regend.fill(None);
    } else {
        regstart.clear();
        regstart.resize(scratch_regs, None);
        regend.clear();
        regend.resize(scratch_regs, None);
    }

    // Best match tracking for POSIX longest-match (audit #2).
    //
    // Mirrors GNU regex-emacs.c:4143-4154 and the main loop handling
    // at lines 4268-4345. When the pattern reaches its end with the
    // matcher positioned before the end of the searchable region
    // (`d < stop`), we save the current register state as the "best
    // so far" and force a backtrack to explore alternative paths.
    // After all backtracks have been exhausted, the best saved match
    // is restored. See GNU regex-emacs.c:5278-5279 for the
    // equivalent "restore after total failure" path.
    let posix_longest = pattern.posix;
    let mut best_regs_set = false;
    let mut best_match_end: usize = pos;
    if best_regstart.len() == scratch_regs {
        best_regstart.fill(None);
        best_regend.fill(None);
    } else {
        best_regstart.clear();
        best_regstart.resize(scratch_regs, None);
        best_regend.clear();
        best_regend.resize(scratch_regs, None);
    }

    let mut pc = 0usize; // Bytecode program counter
    let mut d = pos; // Data position in text

    let translate = &pattern.translate;
    let pattern_multibyte = pattern.multibyte;
    let target_multibyte = pattern.target_multibyte;

    // Helper: translate a character for case-folding.  All other
    // per-character predicates (decode, charset, syntax/category tests,
    // word/symbol boundaries) live in the shared free functions
    // (`match_*_at`, `assert_*`) so the Pike fast path is byte-exact with
    // this backtracker; `tr` remains local because only the `Duplicate`
    // (backreference) arm — which the Pike VM never runs — still needs it.
    let tr = |c: u32| -> u32 {
        if let Some(table) = translate {
            table.translate(c)
        } else {
            c
        }
    };

    // `try_fail!()` is the in-function replacement for GNU's
    // `goto fail`: pop the failure stack and resume there. If the
    // stack is empty, every backtracking avenue has been exhausted.
    // GNU's re_match_2_internal checks `best_regs_set` at that point
    // (regex-emacs.c:5278) and restores the saved best match instead
    // of returning -1. We do the same by setting `total_failure` and
    // breaking to the outer finalization block, which consults
    // `best_regs_set` to decide between returning None and restoring
    // the best registers.
    let mut total_failure = false;
    // `macro_rules!` labels are hygienic, so the label has to be
    // passed in explicitly as a `lifetime` metavariable.
    macro_rules! try_fail {
        ($label:lifetime) => {
            // Catastrophic-backtracking guard: past the constant backtrack
            // budget, bail so the caller re-runs on the Pike VM.  Cheap —
            // evaluated only at backtracks, not per op.
            backtrack_count += 1;
            if backtrack_count > backtrack_budget {
                set_pike_fallback();
                return None;
            }
            if goto_fail(&mut pc, &mut d, frames, undo, regstart, regend, counters).is_none() {
                total_failure = true;
                break $label;
            }
        };
    }

    // GNU `PUSH_FAILURE_POINT` incl. `ENSURE_FAIL_STACK`: refuse to grow
    // the stack past the `emacs_re_max_failures` budget; GNU returns -2
    // and `search.c:matcher_overflow` signals "Stack overflow in regexp
    // matcher".  We flag the thread-local and return no-match; the
    // front-end promotes the flag to the same error.
    macro_rules! push_failure_point {
        ($origin:expr, $resume:expr, $input:expr) => {
            if frames.len() + undo.len() >= FAIL_STACK_ENTRY_LIMIT {
                set_matcher_overflow();
                return None;
            }
            frames.push(FailFrame {
                undo_mark: undo.len(),
                origin: FailureOrigin($origin),
                resume: FailureResume($resume),
                input: $input,
            });
        };
    }

    // GNU `PUSH_FAILURE_REG` / `PUSH_NUMBER`: delta-save one register or
    // counter onto the shared undo log (subject to the same stack budget).
    macro_rules! push_failure_undo {
        ($entry:expr) => {
            if frames.len() + undo.len() >= FAIL_STACK_ENTRY_LIMIT {
                set_matcher_overflow();
                return None;
            }
            undo.push($entry);
        };
    }

    // Sealed-gated bytecode fetches for the loop below. SAFETY of the
    // SEALED arms: `validate_sealed_buffer` proved every operand span
    // in-bounds at every op boundary, and runtime `pc` values are closed
    // over validated boundaries (linear advances, validated jump targets,
    // fail-frame resumes derived from those).
    macro_rules! bc_num {
        ($pos:expr) => {{
            let pos = $pos;
            if SEALED {
                debug_assert!(pos + 1 < bytecode.len());
                unsafe {
                    i16::from_le_bytes([
                        *bytecode.get_unchecked(pos),
                        *bytecode.get_unchecked(pos + 1),
                    ])
                }
            } else {
                extract_number(bytecode, pos)
            }
        }};
    }
    macro_rules! bc_byte {
        ($pos:expr) => {{
            let pos = $pos;
            if SEALED {
                debug_assert!(pos < bytecode.len());
                unsafe { *bytecode.get_unchecked(pos) }
            } else {
                bytecode[pos]
            }
        }};
    }

    'main_loop: loop {
        // End of pattern = potential match.
        //
        // GNU regex-emacs.c:4272-4345: if we haven't consumed the
        // full match region and POSIX longest-match is requested,
        // save the current registers as the best seen so far and
        // force another backtrack. When no more backtracks remain,
        // restore whichever saved best is better than the final
        // candidate (regex-emacs.c:4323-4344).
        // SEALED buffers cannot reach pc == len: validation proves the last
        // op is Succeed/PosixEnd and every jump target lands strictly
        // inside the buffer, so the sealed loop drops this per-op check
        // (the terminal ops' arms carry the end-of-pattern logic).
        if !SEALED && pc >= bytecode.len() {
            if d > stop {
                try_fail!('main_loop);
                continue 'main_loop;
            }
            if posix_longest && d < stop {
                let better_than_best = !best_regs_set || d > best_match_end;
                if !frames.is_empty() {
                    if better_than_best {
                        best_regs_set = true;
                        best_match_end = d;
                        best_regstart.clone_from_slice(regstart);
                        best_regend.clone_from_slice(regend);
                    }
                    // Force a backtrack to explore alternative paths.
                    // The stack is non-empty so goto_fail cannot fail.
                    try_fail!('main_loop);
                    continue 'main_loop;
                } else if best_regs_set && !better_than_best {
                    // No more backtracks; the previously saved best
                    // beats the current finishing position.  Restore
                    // it before finalizing.
                    d = best_match_end;
                    for i in 1..num_regs {
                        regstart[i] = best_regstart[i];
                        regend[i] = best_regend[i];
                    }
                }
            }
            break 'main_loop;
        }
        if SEALED {
            debug_assert!(pc < bytecode.len());
        }

        let op_pc = pc;
        // SEALED: `pc < len` was just checked, validation proved every
        // in-buffer op boundary carries a valid opcode byte, and runtime
        // `pc` values are closed over validated boundaries.
        let op = if SEALED {
            let op_byte = unsafe { *bytecode.get_unchecked(pc) };
            debug_assert!(RegexOp::from_byte(op_byte).is_some());
            unsafe { std::mem::transmute::<u8, RegexOp>(op_byte) }
        } else {
            let op_byte = bytecode[pc];
            let Some(op) = RegexOp::from_byte(op_byte) else {
                // Invalid opcode — treat as match failure
                return None;
            };
            op
        };
        pc += 1;

        match op {
            RegexOp::NoOp => {
                // Skip
            }

            RegexOp::Succeed => {
                // GNU regex-emacs.c:4429-4431 jumps directly to
                // `succeed_label`, bypassing the POSIX longest-match
                // check. For non-POSIX patterns, neomacs's compiler
                // emits a trailing `Succeed` so the matcher exits as
                // soon as the pattern completes (mirroring GNU's
                // `if (!posix_backtracking) BUF_PUSH(succeed)` at
                // regex-emacs.c:2685). In POSIX mode the trailing
                // `Succeed` is NOT emitted, so the matcher instead
                // falls through to the end-of-bytecode check above.
                if d > stop {
                    try_fail!('main_loop);
                    continue 'main_loop;
                }
                break 'main_loop;
            }

            RegexOp::PosixEnd => {
                // The reified "fell off the end" path (see the opcode doc):
                // identical to the !SEALED end-of-bytecode block above.
                if d > stop {
                    try_fail!('main_loop);
                    continue 'main_loop;
                }
                if posix_longest && d < stop {
                    let better_than_best = !best_regs_set || d > best_match_end;
                    if !frames.is_empty() {
                        if better_than_best {
                            best_regs_set = true;
                            best_match_end = d;
                            best_regstart.clone_from_slice(regstart);
                            best_regend.clone_from_slice(regend);
                        }
                        // Force a backtrack to explore alternative paths.
                        // The stack is non-empty so goto_fail cannot fail.
                        try_fail!('main_loop);
                        continue 'main_loop;
                    } else if best_regs_set && !better_than_best {
                        // No more backtracks; the previously saved best
                        // beats the current finishing position.  Restore
                        // it before finalizing.
                        d = best_match_end;
                        for i in 1..num_regs {
                            regstart[i] = best_regstart[i];
                            regend[i] = best_regend[i];
                        }
                    }
                }
                break 'main_loop;
            }

            RegexOp::Exactn => {
                let count = bc_byte!(pc) as usize;
                pc += 1;
                let literal_start = pc;
                let literal_end = literal_start + count;
                let lit = &bytecode[literal_start..literal_end];
                let mut matched = true;
                let mut lit_off = 0usize;
                while lit_off < count {
                    match match_exactn_char_at(
                        lit,
                        lit_off,
                        pattern_multibyte,
                        target_multibyte,
                        translate,
                        text,
                        d,
                        stop,
                    ) {
                        Some((pat_advance, text_advance)) => {
                            lit_off += pat_advance;
                            d += text_advance;
                        }
                        None => {
                            matched = false;
                            break;
                        }
                    }
                }
                pc = literal_end;
                if !matched {
                    // NOTE: try_fail! FALLS THROUGH after a successful pop
                    // (it only breaks the loop on total failure), so the
                    // fusion below must stay inside this else-less arm's
                    // matched path — hence the explicit continue here.
                    try_fail!('main_loop);
                    continue 'main_loop;
                }
                // Greedy exactn-loop fusion: `a+` / `a*` compiles to
                // `Exactn; OnFailureJump exit; Jump back` — the same
                // triple as the charset loops, with the same protocol:
                // one failure frame per iteration (original count and
                // payload) and the back-edge quit poll preserved. The
                // literal is re-matched whole each iteration, exactly
                // as jumping back to the Exactn op would.
                if SEALED
                    && unsafe { *bytecode.get_unchecked(pc) } == RegexOp::OnFailureJump as u8
                    && unsafe { *bytecode.get_unchecked(pc + 3) } == RegexOp::Jump as u8
                    && ((pc as i64) + 6 + bc_num!(pc + 4) as i64) as usize == op_pc
                {
                    let ofj_pc = pc;
                    let exit = ((ofj_pc as i64) + 3 + bc_num!(ofj_pc + 1) as i64) as usize;
                    'fused_exactn: loop {
                        push_failure_point!(ofj_pc, exit, FailureInput::Restore(d));
                        if crate::emacs_core::eval::tls_quit_pending() {
                            return None;
                        }
                        let mut lit_off = 0usize;
                        let mut dd = d;
                        while lit_off < count {
                            match match_exactn_char_at(
                                lit,
                                lit_off,
                                pattern_multibyte,
                                target_multibyte,
                                translate,
                                text,
                                dd,
                                stop,
                            ) {
                                Some((pat_advance, text_advance)) => {
                                    lit_off += pat_advance;
                                    dd += text_advance;
                                }
                                None => break 'fused_exactn,
                            }
                        }
                        d = dd;
                    }
                    try_fail!('main_loop);
                    continue 'main_loop;
                }
                // Keep-string loop fusion: a simple `P+`/`P*` whose
                // continuation is mutually exclusive with the body resolves
                // (via the PP* rewrite + resolve_smart_jumps) to
                // `OFKSJ exit; P; Jump P` — ONE failure frame for the whole
                // loop, back edge jumping straight to the body.  Run it
                // inline: no frames, no dispatch, just the match and the
                // quit poll; on failure the loop-entry OFKSJ frame resumes
                // at the exit with the string position kept, via try_fail.
                if SEALED
                    && unsafe { *bytecode.get_unchecked(pc) } == RegexOp::Jump as u8
                    && ((pc as i64) + 3 + bc_num!(pc + 1) as i64) as usize == op_pc
                {
                    'ks_exactn: loop {
                        if crate::emacs_core::eval::tls_quit_pending() {
                            return None;
                        }
                        let mut lit_off = 0usize;
                        let mut dd = d;
                        while lit_off < count {
                            match match_exactn_char_at(
                                lit,
                                lit_off,
                                pattern_multibyte,
                                target_multibyte,
                                translate,
                                text,
                                dd,
                                stop,
                            ) {
                                Some((pat_advance, text_advance)) => {
                                    lit_off += pat_advance;
                                    dd += text_advance;
                                }
                                None => break 'ks_exactn,
                            }
                        }
                        d = dd;
                    }
                    try_fail!('main_loop);
                    continue 'main_loop;
                }
                // Backtracking star-loop fusion: a simple loop whose
                // continuation is NOT provably exclusive resolves to
                // `OFJ exit; P; Jump OFJ` — per-iteration frames with the
                // OFJ at the loop head.  Same frame protocol as the
                // body-first `+` fusion above, entered from the body arm:
                // the loop-head OFJ pushed the first frame before this
                // iteration, so push AFTER each success, before the next
                // attempt.  (Sealed pass 2 proves the Jump target is an
                // opcode boundary, so the op_pc-3 peek reads a real op.)
                if SEALED
                    && op_pc >= 3
                    && unsafe { *bytecode.get_unchecked(pc) } == RegexOp::Jump as u8
                    && ((pc as i64) + 3 + bc_num!(pc + 1) as i64) as usize == op_pc - 3
                    && unsafe { *bytecode.get_unchecked(op_pc - 3) } == RegexOp::OnFailureJump as u8
                {
                    let ofj_pc = op_pc - 3;
                    let exit = ((ofj_pc as i64) + 3 + bc_num!(ofj_pc + 1) as i64) as usize;
                    'star_exactn: loop {
                        push_failure_point!(ofj_pc, exit, FailureInput::Restore(d));
                        if crate::emacs_core::eval::tls_quit_pending() {
                            return None;
                        }
                        let mut lit_off = 0usize;
                        let mut dd = d;
                        while lit_off < count {
                            match match_exactn_char_at(
                                lit,
                                lit_off,
                                pattern_multibyte,
                                target_multibyte,
                                translate,
                                text,
                                dd,
                                stop,
                            ) {
                                Some((pat_advance, text_advance)) => {
                                    lit_off += pat_advance;
                                    dd += text_advance;
                                }
                                None => break 'star_exactn,
                            }
                        }
                        d = dd;
                    }
                    try_fail!('main_loop);
                    continue 'main_loop;
                }
            }

            RegexOp::AnyChar => {
                match match_anychar_at(text, d, stop, target_multibyte, translate) {
                    Some(len) => {
                        d += len;
                        // Greedy anychar-loop fusion: `.*` / `.+` compiles to
                        // the same `AnyChar; OnFailureJump exit; Jump back`
                        // triple as the charset loops — identical protocol
                        // (see the Charset arm): per-iteration frames with
                        // the original payload, quit poll on the back edge.
                        if SEALED
                            && unsafe { *bytecode.get_unchecked(pc) }
                                == RegexOp::OnFailureJump as u8
                            && unsafe { *bytecode.get_unchecked(pc + 3) } == RegexOp::Jump as u8
                            && ((pc as i64) + 6 + bc_num!(pc + 4) as i64) as usize == op_pc
                        {
                            let ofj_pc = pc;
                            let exit = ((ofj_pc as i64) + 3 + bc_num!(ofj_pc + 1) as i64) as usize;
                            loop {
                                push_failure_point!(ofj_pc, exit, FailureInput::Restore(d));
                                if crate::emacs_core::eval::tls_quit_pending() {
                                    return None;
                                }
                                match match_anychar_at(text, d, stop, target_multibyte, translate) {
                                    Some(len) => d += len,
                                    None => break,
                                }
                            }
                            try_fail!('main_loop);
                            continue 'main_loop;
                        }
                        // Keep-string loop fusion (see the Exactn arm): the
                        // resolved `OFKSJ exit; AnyChar; Jump AnyChar` loop
                        // runs inline with no frames and no dispatch.
                        if SEALED
                            && unsafe { *bytecode.get_unchecked(pc) } == RegexOp::Jump as u8
                            && ((pc as i64) + 3 + bc_num!(pc + 1) as i64) as usize == op_pc
                        {
                            loop {
                                if crate::emacs_core::eval::tls_quit_pending() {
                                    return None;
                                }
                                match match_anychar_at(text, d, stop, target_multibyte, translate) {
                                    Some(len) => d += len,
                                    None => break,
                                }
                            }
                            try_fail!('main_loop);
                            continue 'main_loop;
                        }
                        // Backtracking star-loop fusion (see the Exactn arm):
                        // `OFJ exit; AnyChar; Jump OFJ` with per-iteration
                        // frames, run inline.
                        if SEALED
                            && op_pc >= 3
                            && unsafe { *bytecode.get_unchecked(pc) } == RegexOp::Jump as u8
                            && ((pc as i64) + 3 + bc_num!(pc + 1) as i64) as usize == op_pc - 3
                            && unsafe { *bytecode.get_unchecked(op_pc - 3) }
                                == RegexOp::OnFailureJump as u8
                        {
                            let ofj_pc = op_pc - 3;
                            let exit = ((ofj_pc as i64) + 3 + bc_num!(ofj_pc + 1) as i64) as usize;
                            loop {
                                push_failure_point!(ofj_pc, exit, FailureInput::Restore(d));
                                if crate::emacs_core::eval::tls_quit_pending() {
                                    return None;
                                }
                                match match_anychar_at(text, d, stop, target_multibyte, translate) {
                                    Some(len) => d += len,
                                    None => break,
                                }
                            }
                            try_fail!('main_loop);
                            continue 'main_loop;
                        }
                    }
                    None => {
                        try_fail!('main_loop);
                    }
                }
            }

            RegexOp::Charset | RegexOp::CharsetNot => {
                let charset_op_pos = pc - 1; // bytecode position of the opcode
                let bitmap_len = (bc_byte!(pc) & 0x7F) as usize;
                pc += 1;
                // The shared helper (see `match_charset_at`) holds the full
                // GNU `execute_charset` logic — the mutually-exclusive
                // unibyte-bitmap vs multibyte-range/class-bit branches — so
                // the Pike fast path stays byte-exact with this arm.
                let result = match_charset_at(
                    pattern,
                    charset_op_pos,
                    text,
                    d,
                    stop,
                    target_multibyte,
                    translate,
                    syntax,
                );
                pc += bitmap_len;
                match result {
                    Some(ch_len) => {
                        d += ch_len;
                        // Greedy charset-loop fusion: `Charset;
                        // OnFailureJump exit; Jump back` is the compiled
                        // shape of `[...]+` / `[...]*`. Run the loop here,
                        // skipping two dispatches per matched character —
                        // with per-iteration failure frames (push AFTER
                        // each success, exactly the original count and
                        // payload) and the back-edge quit poll preserved.
                        // Sealed only: the peeks rely on validated bounds.
                        if SEALED
                            && unsafe { *bytecode.get_unchecked(pc) }
                                == RegexOp::OnFailureJump as u8
                            && unsafe { *bytecode.get_unchecked(pc + 3) } == RegexOp::Jump as u8
                            && ((pc as i64) + 6 + bc_num!(pc + 4) as i64) as usize == charset_op_pos
                        {
                            let ofj_pc = pc;
                            let exit = ((ofj_pc as i64) + 3 + bc_num!(ofj_pc + 1) as i64) as usize;
                            loop {
                                push_failure_point!(ofj_pc, exit, FailureInput::Restore(d));
                                if crate::emacs_core::eval::tls_quit_pending() {
                                    return None;
                                }
                                match match_charset_at(
                                    pattern,
                                    charset_op_pos,
                                    text,
                                    d,
                                    stop,
                                    target_multibyte,
                                    translate,
                                    syntax,
                                ) {
                                    Some(ch_len) => d += ch_len,
                                    None => break,
                                }
                            }
                            try_fail!('main_loop);
                            continue 'main_loop;
                        }
                        // Keep-string loop fusion (see the Exactn arm): the
                        // resolved `OFKSJ exit; Charset; Jump Charset` loop
                        // runs inline with no frames and no dispatch.
                        if SEALED
                            && unsafe { *bytecode.get_unchecked(pc) } == RegexOp::Jump as u8
                            && ((pc as i64) + 3 + bc_num!(pc + 1) as i64) as usize == charset_op_pos
                        {
                            loop {
                                if crate::emacs_core::eval::tls_quit_pending() {
                                    return None;
                                }
                                match match_charset_at(
                                    pattern,
                                    charset_op_pos,
                                    text,
                                    d,
                                    stop,
                                    target_multibyte,
                                    translate,
                                    syntax,
                                ) {
                                    Some(ch_len) => d += ch_len,
                                    None => break,
                                }
                            }
                            try_fail!('main_loop);
                            continue 'main_loop;
                        }
                        // Backtracking star-loop fusion (see the Exactn arm):
                        // `OFJ exit; Charset; Jump OFJ` with per-iteration
                        // frames, run inline.
                        if SEALED
                            && charset_op_pos >= 3
                            && unsafe { *bytecode.get_unchecked(pc) } == RegexOp::Jump as u8
                            && ((pc as i64) + 3 + bc_num!(pc + 1) as i64) as usize
                                == charset_op_pos - 3
                            && unsafe { *bytecode.get_unchecked(charset_op_pos - 3) }
                                == RegexOp::OnFailureJump as u8
                        {
                            let ofj_pc = charset_op_pos - 3;
                            let exit = ((ofj_pc as i64) + 3 + bc_num!(ofj_pc + 1) as i64) as usize;
                            loop {
                                push_failure_point!(ofj_pc, exit, FailureInput::Restore(d));
                                if crate::emacs_core::eval::tls_quit_pending() {
                                    return None;
                                }
                                match match_charset_at(
                                    pattern,
                                    charset_op_pos,
                                    text,
                                    d,
                                    stop,
                                    target_multibyte,
                                    translate,
                                    syntax,
                                ) {
                                    Some(ch_len) => d += ch_len,
                                    None => break,
                                }
                            }
                            try_fail!('main_loop);
                            continue 'main_loop;
                        }
                    }
                    None => {
                        try_fail!('main_loop);
                    }
                }
            }

            RegexOp::StartMemory => {
                let group = bc_byte!(pc) as usize;
                pc += 1;
                if group < num_regs && group < regstart.len() {
                    // GNU start_memory: "In case we need to undo this
                    // operation (via backtracking)" — PUSH_FAILURE_REG
                    // saves the old (start, end) pair on the fail stack.
                    push_failure_undo!(FailUndo::Reg {
                        idx: group,
                        start: regstart[group],
                        end: regend[group],
                    });
                    regstart[group] = Some(d);
                }
            }

            RegexOp::StopMemory => {
                let group = bc_byte!(pc) as usize;
                pc += 1;
                if group < num_regs
                    && let Some(end) = regend.get_mut(group)
                {
                    // GNU stop_memory pushes nothing: undoing this write
                    // is unnecessary because the value is only read again
                    // after the next start_memory (which delta-saves it)
                    // or at match end (only reachable if this
                    // stop_memory was not undone).  regex-emacs.c:4608.
                    *end = Some(d);
                }
            }

            RegexOp::Duplicate => {
                let group = bc_byte!(pc) as usize;
                pc += 1;

                let Some(start) = regstart.get(group).copied().flatten() else {
                    try_fail!('main_loop);
                    continue;
                };
                let Some(end) = regend.get(group).copied().flatten() else {
                    try_fail!('main_loop);
                    continue;
                };

                let ref_len = end - start;
                if d + ref_len > stop {
                    try_fail!('main_loop);
                    continue;
                }

                // Compare the backreference text
                let mut matched = true;
                for i in 0..ref_len {
                    if tr(text[d + i].into()) != tr(text[start + i].into()) {
                        matched = false;
                        break;
                    }
                }
                if !matched {
                    try_fail!('main_loop);
                    continue;
                }
                d += ref_len;
            }

            RegexOp::BegLine => {
                if d == 0 || (d > 0 && text[d - 1] == b'\n') {
                    // At beginning of line — succeed
                } else {
                    try_fail!('main_loop);
                }
            }

            RegexOp::EndLine => {
                if d >= text.len() || text[d] == b'\n' {
                    // At end of line — succeed
                } else {
                    try_fail!('main_loop);
                }
            }

            RegexOp::BegBuf => {
                if d != 0 {
                    try_fail!('main_loop);
                }
            }

            RegexOp::EndBuf => {
                if d != text.len() {
                    try_fail!('main_loop);
                }
            }

            RegexOp::AtDot => {
                if d != point {
                    try_fail!('main_loop);
                }
            }

            RegexOp::WordBound
            | RegexOp::NotWordBound
            | RegexOp::WordBeg
            | RegexOp::WordEnd
            | RegexOp::SymBeg
            | RegexOp::SymEnd => {
                if !evaluate_syntax_assertion(
                    SyntaxAssertion::from_regex_op(op),
                    text,
                    d,
                    stop,
                    target_multibyte,
                    syntax,
                ) {
                    try_fail!('main_loop);
                }
            }

            RegexOp::SyntaxSpec => {
                let class_byte = bytecode[pc];
                pc += 1;
                match match_syntaxspec_at(
                    class_byte,
                    false,
                    text,
                    d,
                    stop,
                    target_multibyte,
                    syntax,
                ) {
                    Some(len) => d += len,
                    None => {
                        try_fail!('main_loop);
                    }
                }
            }

            RegexOp::NotSyntaxSpec => {
                let class_byte = bytecode[pc];
                pc += 1;
                match match_syntaxspec_at(class_byte, true, text, d, stop, target_multibyte, syntax)
                {
                    Some(len) => d += len,
                    None => {
                        try_fail!('main_loop);
                    }
                }
            }

            RegexOp::SyntaxSpecSet => {
                // Fused positive syntax-class set (see `RegexOp::SyntaxSpecSet`
                // and `fuse_syntaxspec_alternations`).  ONE `char_syntax`
                // lookup + a mask test replaces the per-branch
                // `on_failure_jump` chain and repeated lookups of the original
                // `\sA\|\sB\|...` alternation.
                let mask = extract_number_u16(bytecode, pc);
                pc += 2;
                match match_syntaxspecset_at(mask, text, d, stop, target_multibyte, syntax) {
                    Some(len) => d += len,
                    None => {
                        try_fail!('main_loop);
                    }
                }
            }

            RegexOp::CategorySpec => {
                let cat = bytecode[pc];
                pc += 1;
                match match_categoryspec_at(cat, false, text, d, stop, target_multibyte, syntax) {
                    Some(len) => d += len,
                    None => {
                        try_fail!('main_loop);
                    }
                }
            }

            RegexOp::NotCategorySpec => {
                let cat = bytecode[pc];
                pc += 1;
                match match_categoryspec_at(cat, true, text, d, stop, target_multibyte, syntax) {
                    Some(len) => d += len,
                    None => {
                        try_fail!('main_loop);
                    }
                }
            }

            RegexOp::Jump => {
                // Mirrors GNU `regex-emacs.c:4901`: poll quit at the
                // unconditional-jump site inside the matcher bytecode
                // dispatch loop. Gives interactive `C-g` a chance to
                // abort a pathological regex that would otherwise run
                // for many seconds on a large input.
                if crate::emacs_core::eval::tls_quit_pending() {
                    return None;
                }
                let offset = bc_num!(pc);
                pc = ((pc as i64) + 2 + (offset as i64)) as usize;
            }

            RegexOp::OnFailureJump => {
                let offset = bc_num!(pc);
                pc += 2;
                let fail_pc = ((pc as i64) + (offset as i64)) as usize;
                push_failure_point!(op_pc, fail_pc, FailureInput::Restore(d));
            }

            RegexOp::OnFailureKeepStringJump => {
                let offset = bc_num!(pc);
                pc += 2;
                let fail_pc = ((pc as i64) + (offset as i64)) as usize;
                push_failure_point!(op_pc, fail_pc, FailureInput::KeepCurrent);
            }

            RegexOp::OnFailureJumpLoop => {
                let offset = bc_num!(pc);
                pc += 2;
                let fail_pc = ((pc as i64) + (offset as i64)) as usize;
                // Check for infinite loop (empty match detection).
                if check_infinite_loop(frames, FailureOrigin(op_pc), d) {
                    // Would loop forever on empty match — skip the loop
                    pc = fail_pc;
                } else {
                    push_failure_point!(op_pc, fail_pc, FailureInput::Restore(d));
                }
            }

            RegexOp::OnFailureJumpNastyloop => {
                // Same as OnFailureJumpLoop but for non-greedy
                let offset = bc_num!(pc);
                pc += 2;
                let fail_pc = ((pc as i64) + (offset as i64)) as usize;
                push_failure_point!(op_pc, fail_pc, FailureInput::Restore(d));
            }

            RegexOp::OnFailureJumpSmart => {
                // Unreachable in practice: `resolve_smart_jumps` rewrites
                // every on_failure_jump_smart at compile time (GNU does
                // the same rewrite lazily at first execution,
                // regex-emacs.c:4864-4906).  Fall back to the safe plain
                // on_failure_jump semantics if one survives.
                let offset = bc_num!(pc);
                pc += 2;
                let fail_pc = ((pc as i64) + (offset as i64)) as usize;
                push_failure_point!(op_pc, fail_pc, FailureInput::Restore(d));
            }

            RegexOp::SucceedN => {
                // GNU: succeed_n  <jump-offset:2> <counter:2>
                // "Have to succeed matching what follows at least n times."
                // Counter lives at pc+2 (2 bytes).  When counter > 0 we
                // decrement and continue (must still succeed more times).
                // When counter == 0 we fall through to on_failure_jump_loop
                // semantics using the jump offset.
                let counter_pos = pc + 2; // bytecode position of the counter
                let count = get_counter(counters, bytecode, counter_pos);
                if count != 0 {
                    // Still must succeed more times — decrement & continue.
                    // GNU PUSH_NUMBER: the old value is delta-saved so
                    // backtracking restores it.
                    push_failure_undo!(FailUndo::Counter {
                        pos: counter_pos,
                        val: count,
                    });
                    set_counter(counters, counter_pos, count.saturating_sub(1));
                    pc += 4;
                } else {
                    // Counter exhausted — behave like on_failure_jump_loop.
                    // Read the jump offset and push a failure point.
                    let offset = bc_num!(pc);
                    pc += 2; // skip the offset field
                    let fail_pc = ((pc as i64) + (offset as i64)) as usize;
                    pc += 2; // skip the counter field
                    // Infinite-loop detection (same as OnFailureJumpLoop)
                    if check_infinite_loop(frames, FailureOrigin(op_pc), d) {
                        pc = fail_pc;
                    } else {
                        push_failure_point!(op_pc, fail_pc, FailureInput::Restore(d));
                    }
                }
            }

            RegexOp::JumpN => {
                // GNU: jump_n  <jump-offset:2> <counter:2>
                // "Originally, this is how many times we CAN jump."
                // If counter > 0, decrement and jump.
                // If counter == 0, skip past (don't jump).
                let counter_pos = pc + 2;
                let count = get_counter(counters, bytecode, counter_pos);
                if count != 0 {
                    // Decrement counter (delta-saved, GNU PUSH_NUMBER)
                    // and perform the unconditional jump.
                    push_failure_undo!(FailUndo::Counter {
                        pos: counter_pos,
                        val: count,
                    });
                    set_counter(counters, counter_pos, count.saturating_sub(1));
                    let offset = bc_num!(pc);
                    pc = ((pc as i64) + 2 + (offset as i64)) as usize;
                } else {
                    pc += 4; // Skip past offset + counter fields
                }
            }

            RegexOp::SetNumberAt => {
                // GNU: set_number_at  <offset-to-counter:2> <value:2>
                // Sets the counter at the given offset to the given value.
                // Used to reset interval counters at the start of a loop.
                let rel_offset = bc_num!(pc);
                pc += 2; // advance past the offset field
                let value = extract_number_u16(bytecode, pc);
                pc += 2; // advance past the value field
                // Target counter position: relative to position after
                // the offset field (same convention as GNU).
                let target_pos = ((pc as i64) - 2 + (rel_offset as i64)) as usize;
                // GNU set_number_at writes through PUSH_NUMBER too.
                push_failure_undo!(FailUndo::Counter {
                    pos: target_pos,
                    val: get_counter(counters, bytecode, target_pos),
                });
                set_counter(counters, target_pos, value);
            }
        }
    }

    // GNU regex-emacs.c:5278-5279: when the matcher breaks out of
    // the main loop due to total backtracking exhaustion, if a best
    // match was previously saved for POSIX longest-match, restore it
    // and fall through to the success path; otherwise there is no
    // match at all.
    if total_failure {
        if best_regs_set {
            d = best_match_end;
            for i in 1..num_regs {
                regstart[i] = best_regstart[i];
                regend[i] = best_regend[i];
            }
        } else {
            return None;
        }
    }

    // If we got here, we matched!
    // Fill in registers
    let mut regs = MatchRegisters::new(num_regs);
    regs.start[0] = pos as i64;
    regs.end[0] = d as i64;
    for i in 1..num_regs {
        regs.start[i] = regstart
            .get(i)
            .copied()
            .flatten()
            .map(|v| v as i64)
            .unwrap_or(-1);
        regs.end[i] = regend
            .get(i)
            .copied()
            .flatten()
            .map(|v| v as i64)
            .unwrap_or(-1);
    }

    Some((d, regs))
}

// ---------------------------------------------------------------------------
// Pike VM — non-backtracking NFA simulation (Stage 1 fast path).
//
// The Pike VM simulates the SAME compiled bytecode the backtracker runs, but
// linearly: instead of a depth-first backtracking search it advances a set of
// "threads" (each an NFA state = bytecode pc + capture slots) through the text
// one character at a time, in the SAME priority order the backtracker would
// explore.  Because every consuming test calls the SAME shared `match_*_at`
// helpers and the epsilon-closure follows splits fall-through-first (exactly
// mirroring the backtracker's "continue at the instruction, push the jump
// target as a failure point"), the Pike VM reproduces Emacs's leftmost-greedy
// captures BYTE-EXACTLY — while killing catastrophic backtracking (`a*a*b`
// becomes linear).
//
// Only patterns with `pike_eligible == true` reach here (no backreference, no
// `\{n,m\}` interval counters, not POSIX-longest).  See `compute_pike_eligible`.
// ---------------------------------------------------------------------------

/// A thread's capture slots: `start(g)` at index `2*g`, `end(g)` at `2*g+1`.
/// Group 0 (the whole match) is filled in at finalize from the match span.
type PikeCaps = SmallVec<[Option<u32>; 8]>;

/// Sentinel `pc` for a thread that has reached the end of the program
/// (a match).  `usize::MAX` can never be a real bytecode position.
const PIKE_MATCH_PC: usize = usize::MAX;

/// One live NFA thread.
#[derive(Clone)]
struct PikeThread {
    /// Bytecode position of the consuming op this thread is waiting on, or
    /// [`PIKE_MATCH_PC`] for a thread that has matched.
    pc: usize,
    /// Byte offset already consumed within a multi-character `Exactn`
    /// literal; `0` for the first char and for every non-`Exactn` op.  A
    /// literal is walked one input character per step, so a thread can sit
    /// mid-literal across steps.
    lit_off: usize,
    /// Capture slots carried by this exploration path.
    caps: PikeCaps,
}

/// Epsilon-closure: starting from `start_pc`, follow every non-consuming op
/// (jumps, splits, group markers, zero-width assertions) and append the
/// reached CONSUMING ops (and end-of-program matches) to `list` in priority
/// order.  `seen`/`generation` deduplicate by bytecode pc so the closure is linear.
///
/// Splits (`OnFailureJump` and its variants) are explored fall-through
/// FIRST, which is exactly the order the backtracker tries them (it
/// continues at the fall-through and pushes the jump target as a failure
/// point) — this is what preserves greedy / left-alternation priority.
#[allow(clippy::too_many_arguments)]
fn pike_add_thread(
    bytecode: &[u8],
    list: &mut Vec<PikeThread>,
    seen: &mut [u32],
    generation: u32,
    start_pc: usize,
    start_caps: PikeCaps,
    text: &[u8],
    d: usize,
    stop: usize,
    point: usize,
    target_multibyte: bool,
    num_regs: usize,
    syntax: &dyn SyntaxLookup,
) {
    // Explicit DFS stack (avoids unbounded recursion on deeply nested
    // patterns).  Children are pushed in REVERSE priority so the highest
    // priority is popped first — this reproduces the recursive pre-order
    // closure, hence the backtracker's try-order.
    let mut stack: SmallVec<[(usize, PikeCaps); 32]> = SmallVec::new();
    stack.push((start_pc, start_caps));

    while let Some((pc, caps)) = stack.pop() {
        // pc == bytecode.len() is the end-of-program match position.
        if seen[pc] == generation {
            continue;
        }
        seen[pc] = generation;

        if pc >= bytecode.len() {
            list.push(PikeThread {
                pc: PIKE_MATCH_PC,
                lit_off: 0,
                caps,
            });
            continue;
        }

        let Some(op) = RegexOp::from_byte(bytecode[pc]) else {
            // Malformed opcode: drop this thread (eligibility guarantees
            // this is unreachable for compiled patterns).
            continue;
        };

        match op {
            RegexOp::NoOp => stack.push((pc + 1, caps)),

            RegexOp::Succeed | RegexOp::PosixEnd => list.push(PikeThread {
                pc: PIKE_MATCH_PC,
                lit_off: 0,
                caps,
            }),

            RegexOp::StartMemory => {
                let group = bytecode[pc + 1] as usize;
                let mut caps = caps;
                if group < num_regs {
                    caps[2 * group] = Some(d as u32);
                }
                stack.push((pc + 2, caps));
            }

            RegexOp::StopMemory => {
                let group = bytecode[pc + 1] as usize;
                let mut caps = caps;
                if group < num_regs {
                    caps[2 * group + 1] = Some(d as u32);
                }
                stack.push((pc + 2, caps));
            }

            RegexOp::BegLine => {
                if d == 0 || (d > 0 && text[d - 1] == b'\n') {
                    stack.push((pc + 1, caps));
                }
            }
            RegexOp::EndLine => {
                if d >= text.len() || text[d] == b'\n' {
                    stack.push((pc + 1, caps));
                }
            }
            RegexOp::BegBuf => {
                if d == 0 {
                    stack.push((pc + 1, caps));
                }
            }
            RegexOp::EndBuf => {
                if d == text.len() {
                    stack.push((pc + 1, caps));
                }
            }
            RegexOp::AtDot => {
                if d == point {
                    stack.push((pc + 1, caps));
                }
            }

            RegexOp::WordBound
            | RegexOp::NotWordBound
            | RegexOp::WordBeg
            | RegexOp::WordEnd
            | RegexOp::SymBeg
            | RegexOp::SymEnd => {
                if evaluate_syntax_assertion(
                    SyntaxAssertion::from_regex_op(op),
                    text,
                    d,
                    stop,
                    target_multibyte,
                    syntax,
                ) {
                    stack.push((pc + 1, caps));
                }
            }

            RegexOp::Jump => {
                let offset = extract_number(bytecode, pc + 1);
                let target = (pc as i64 + 3 + offset as i64) as usize;
                stack.push((target, caps));
            }

            RegexOp::OnFailureJump
            | RegexOp::OnFailureKeepStringJump
            | RegexOp::OnFailureJumpLoop
            | RegexOp::OnFailureJumpNastyloop
            | RegexOp::OnFailureJumpSmart => {
                // SPLIT.  Fall-through (pc+3) has higher priority than the
                // jump target — same order the backtracker explores.  Push
                // the lower-priority jump target first so the higher-priority
                // fall-through is popped (explored) first.  Empty-loop cycles
                // terminate via the `seen` dedup, so the loop variants need
                // no special infinite-loop check here.
                let offset = extract_number(bytecode, pc + 1);
                let fall_through = pc + 3;
                let jump_target = (pc as i64 + 3 + offset as i64) as usize;
                stack.push((jump_target, caps.clone()));
                stack.push((fall_through, caps));
            }

            // Consuming ops: the thread waits here for the next input char.
            RegexOp::Exactn
            | RegexOp::AnyChar
            | RegexOp::Charset
            | RegexOp::CharsetNot
            | RegexOp::SyntaxSpec
            | RegexOp::NotSyntaxSpec
            | RegexOp::SyntaxSpecSet
            | RegexOp::CategorySpec
            | RegexOp::NotCategorySpec => {
                list.push(PikeThread {
                    pc,
                    lit_off: 0,
                    caps,
                });
            }

            // Ineligible ops never reach the Pike VM (`compute_pike_eligible`).
            RegexOp::Duplicate | RegexOp::SucceedN | RegexOp::JumpN | RegexOp::SetNumberAt => {
                debug_assert!(false, "Pike VM reached an ineligible op {op:?}");
            }
        }
    }
}

/// Reusable per-thread Pike VM state.  `re_search` runs `pike_match` once per
/// candidate start; allocating the `seen` set and the two thread lists afresh
/// for every candidate was the dominant cost.  The `seen` set uses a
/// monotonic generation stamp so it is neither reallocated nor cleared
/// between candidates — a stale entry from an earlier candidate simply
/// carries an older generation.
#[derive(Default)]
struct PikeScratch {
    seen: Vec<u32>,
    generation: u32,
    clist: Vec<PikeThread>,
    nlist: Vec<PikeThread>,
}

thread_local! {
    static PIKE_SCRATCH: std::cell::RefCell<PikeScratch> =
        std::cell::RefCell::new(PikeScratch::default());
}

/// Anchored leftmost-greedy match of an eligible pattern starting exactly at
/// `pos`.  Byte-exact with `re_match_internal` for `pike_eligible` patterns.
pub(crate) fn pike_match(
    pattern: &CompiledPattern,
    text: &[u8],
    pos: usize,
    stop: usize,
    syntax: &dyn SyntaxLookup,
    point: usize,
) -> Option<(usize, MatchRegisters)> {
    PIKE_SCRATCH.with(|cell| match cell.try_borrow_mut() {
        Ok(mut scratch) => pike_match_inner(&mut scratch, pattern, text, pos, stop, syntax, point),
        // Defensive: a syntax/category callback re-entering the matcher gets
        // fresh (allocating) state rather than corrupting the outer scan.
        Err(_) => pike_match_inner(
            &mut PikeScratch::default(),
            pattern,
            text,
            pos,
            stop,
            syntax,
            point,
        ),
    })
}

fn pike_match_inner(
    scratch: &mut PikeScratch,
    pattern: &CompiledPattern,
    text: &[u8],
    pos: usize,
    stop: usize,
    syntax: &dyn SyntaxLookup,
    point: usize,
) -> Option<(usize, MatchRegisters)> {
    debug_assert!(pattern.pike_eligible);
    // Read opcodes from the Pike-only rewind view when present (keep-string
    // loops de-optimized); charset bitmaps are identical in both buffers so
    // `match_charset_at` may keep reading `pattern.buffer`.
    let bytecode = pattern.pike_buffer.as_deref().unwrap_or(&pattern.buffer);
    let num_regs = pattern.re_nsub + 1;
    let translate = &pattern.translate;
    let pattern_multibyte = pattern.multibyte;
    let target_multibyte = pattern.target_multibyte;

    // Reuse the scratch buffers (cheap Vec-header swaps); cleared, not freed.
    let mut clist = std::mem::take(&mut scratch.clist);
    let mut nlist = std::mem::take(&mut scratch.nlist);
    let mut seen = std::mem::take(&mut scratch.seen);
    clist.clear();
    nlist.clear();
    if seen.len() < bytecode.len() + 1 {
        seen.resize(bytecode.len() + 1, 0);
    }
    // Generation-stamped dedup: bump per step (each `nlist` closure dedups at
    // its own position).  Reset the stamps only if a bump would overflow.
    let mut generation = scratch.generation;
    if generation as u64 + text.len() as u64 + 4 >= u32::MAX as u64 {
        seen.iter_mut().for_each(|x| *x = 0);
        generation = 0;
    }

    // (match_end, captures) of the best (highest-priority) match found so far.
    let mut matched: Option<(usize, PikeCaps)> = None;

    let mut d = pos;

    generation += 1;
    let init_caps: PikeCaps = smallvec::smallvec![None; 2 * num_regs];
    pike_add_thread(
        bytecode,
        &mut clist,
        &mut seen,
        generation,
        0,
        init_caps,
        text,
        d,
        stop,
        point,
        target_multibyte,
        num_regs,
        syntax,
    );

    loop {
        if clist.is_empty() {
            break;
        }

        // Byte length of the input character at `d` (used to advance the
        // scan).  Every consuming op that matches consumes exactly this
        // char, so all surviving threads land at `d + cur_len`.
        let cur_len = if d < stop {
            re_text_char(text, d, target_multibyte).map(|(_, l)| l)
        } else {
            None
        };

        generation += 1; // closures added to nlist evaluate assertions at d + cur_len.
        nlist.clear();

        // Index-based walk: threads are processed in strict priority order and
        // we push successors into `nlist` while reading `clist` — and cut the
        // tail on a match — so a by-reference iterator does not fit.
        #[allow(clippy::needless_range_loop)]
        for idx in 0..clist.len() {
            let pc = clist[idx].pc;

            if pc == PIKE_MATCH_PC {
                // A match is only valid if it ends within the search region.
                // A multibyte character can straddle `stop` (consuming ops
                // gate only on `d >= stop`, so a thread may land at `d >
                // stop`); the backtracker rejects such a thread at its
                // trailing `d > stop` check, so we do too — skip this thread
                // and keep exploring lower-priority ones.
                if d <= stop {
                    // Highest-priority match at this position wins;
                    // lower-priority threads (later in the list) are cut.
                    matched = Some((d, clist[idx].caps.clone()));
                    break;
                }
                continue;
            }

            let lit_off = clist[idx].lit_off;
            let caps = clist[idx].caps.clone();
            let next_d_close = |nlist: &mut Vec<PikeThread>,
                                seen: &mut [u32],
                                next_pc: usize,
                                next_d: usize,
                                caps: PikeCaps| {
                pike_add_thread(
                    bytecode,
                    nlist,
                    seen,
                    generation,
                    next_pc,
                    caps,
                    text,
                    next_d,
                    stop,
                    point,
                    target_multibyte,
                    num_regs,
                    syntax,
                );
            };

            match RegexOp::from_byte(bytecode[pc]) {
                Some(RegexOp::Exactn) => {
                    let count = bytecode[pc + 1] as usize;
                    let lit = &bytecode[pc + 2..pc + 2 + count];
                    if let Some((pat_advance, text_advance)) = match_exactn_char_at(
                        lit,
                        lit_off,
                        pattern_multibyte,
                        target_multibyte,
                        translate,
                        text,
                        d,
                        stop,
                    ) {
                        let new_off = lit_off + pat_advance;
                        let next_d = d + text_advance;
                        if new_off >= count {
                            next_d_close(&mut nlist, &mut seen, pc + 2 + count, next_d, caps);
                        } else {
                            // Still mid-literal: keep consuming, no closure.
                            nlist.push(PikeThread {
                                pc,
                                lit_off: new_off,
                                caps,
                            });
                        }
                    }
                }
                Some(RegexOp::AnyChar) => {
                    if let Some(len) = match_anychar_at(text, d, stop, target_multibyte, translate)
                    {
                        next_d_close(&mut nlist, &mut seen, pc + 1, d + len, caps);
                    }
                }
                Some(RegexOp::Charset | RegexOp::CharsetNot) => {
                    if let Some(len) = match_charset_at(
                        pattern,
                        pc,
                        text,
                        d,
                        stop,
                        target_multibyte,
                        translate,
                        syntax,
                    ) {
                        let next_pc = pc + 2 + (bytecode[pc + 1] as usize & 0x7F);
                        next_d_close(&mut nlist, &mut seen, next_pc, d + len, caps);
                    }
                }
                Some(RegexOp::SyntaxSpec) => {
                    if let Some(len) = match_syntaxspec_at(
                        bytecode[pc + 1],
                        false,
                        text,
                        d,
                        stop,
                        target_multibyte,
                        syntax,
                    ) {
                        next_d_close(&mut nlist, &mut seen, pc + 2, d + len, caps);
                    }
                }
                Some(RegexOp::NotSyntaxSpec) => {
                    if let Some(len) = match_syntaxspec_at(
                        bytecode[pc + 1],
                        true,
                        text,
                        d,
                        stop,
                        target_multibyte,
                        syntax,
                    ) {
                        next_d_close(&mut nlist, &mut seen, pc + 2, d + len, caps);
                    }
                }
                Some(RegexOp::SyntaxSpecSet) => {
                    let mask = extract_number_u16(bytecode, pc + 1);
                    if let Some(len) =
                        match_syntaxspecset_at(mask, text, d, stop, target_multibyte, syntax)
                    {
                        next_d_close(&mut nlist, &mut seen, pc + 3, d + len, caps);
                    }
                }
                Some(RegexOp::CategorySpec) => {
                    if let Some(len) = match_categoryspec_at(
                        bytecode[pc + 1],
                        false,
                        text,
                        d,
                        stop,
                        target_multibyte,
                        syntax,
                    ) {
                        next_d_close(&mut nlist, &mut seen, pc + 2, d + len, caps);
                    }
                }
                Some(RegexOp::NotCategorySpec) => {
                    if let Some(len) = match_categoryspec_at(
                        bytecode[pc + 1],
                        true,
                        text,
                        d,
                        stop,
                        target_multibyte,
                        syntax,
                    ) {
                        next_d_close(&mut nlist, &mut seen, pc + 2, d + len, caps);
                    }
                }
                _ => {
                    // Non-consuming op in clist is impossible (closure only
                    // adds consuming ops / matches).
                    debug_assert!(false, "non-consuming op in Pike clist");
                }
            }
        }

        std::mem::swap(&mut clist, &mut nlist);
        match cur_len {
            Some(len) if !clist.is_empty() => d += len,
            _ => break,
        }
    }

    // Return the reusable buffers to the scratch for the next candidate.
    scratch.clist = clist;
    scratch.nlist = nlist;
    scratch.seen = seen;
    scratch.generation = generation;

    let (end, caps) = matched?;
    let mut regs = MatchRegisters::new(num_regs);
    regs.start[0] = pos as i64;
    regs.end[0] = end as i64;
    for g in 1..num_regs {
        regs.start[g] = caps
            .get(2 * g)
            .copied()
            .flatten()
            .map(|v| v as i64)
            .unwrap_or(-1);
        regs.end[g] = caps
            .get(2 * g + 1)
            .copied()
            .flatten()
            .map(|v| v as i64)
            .unwrap_or(-1);
    }
    Some((end, regs))
}

/// GNU `CHECK_INFINITE_LOOP` (regex-emacs.c:1049-1069): walk down the
/// failure stack while the recorded string position equals the current
/// one (or has keep-current input policy); a frame created by the same
/// failure opcode means the loop has made no progress since it was
/// last here — an empty-match cycle.
fn check_infinite_loop(frames: &[FailFrame], origin: FailureOrigin, d: usize) -> bool {
    for frame in frames.iter().rev() {
        match frame.input {
            FailureInput::Restore(sp) if sp != d => return false,
            _ => {
                if frame.origin == origin {
                    return true;
                }
            }
        }
    }
    false
}

/// Handle match failure — pop the failure stack and backtrack.
/// Returns None if the failure stack is empty (complete failure).
///
/// Mirrors GNU `POP_FAILURE_POINT` (regex-emacs.c:1128): replay the undo
/// log down to the popped frame's mark (restoring registers and counters
/// delta-saved since the push), then resume at the frame's positions.
fn goto_fail(
    pc: &mut usize,
    d: &mut usize,
    frames: &mut Vec<FailFrame>,
    undo: &mut Vec<FailUndo>,
    regstart: &mut RegisterScratch,
    regend: &mut RegisterScratch,
    counters: &mut CounterTable,
) -> Option<()> {
    // Mirrors GNU `regex-emacs.c:5236`: poll quit at the failure /
    // backtrack site. Backtracking loops are the worst offenders for
    // pathological-regex responsiveness; a `C-g` arriving mid-backtrack
    // aborts the entire search here so the evaluator can surface the
    // quit signal on its next `maybe_quit` poll.
    if crate::emacs_core::eval::tls_quit_pending() {
        return None;
    }
    let frame = frames.pop()?;
    while undo.len() > frame.undo_mark {
        match undo.pop().expect("undo log at least undo_mark deep") {
            FailUndo::Reg { idx, start, end } => {
                if idx < regstart.len() {
                    regstart[idx] = start;
                }
                if idx < regend.len() {
                    regend[idx] = end;
                }
            }
            FailUndo::Counter { pos, val } => set_counter(counters, pos, val),
        }
    }
    *pc = frame.resume.0;
    if let FailureInput::Restore(position) = frame.input {
        *d = position;
    }
    Some(())
}

// ---------------------------------------------------------------------------
// Phase 4: Searcher (re_search_2)
//
// Translates GNU regex-emacs.c:3408-4070.
// Searches for a match in text, using fastmap for optimization.
// ---------------------------------------------------------------------------

/// Analyze compiled bytecode to populate `pattern.fastmap`.
///
/// For each byte value `c` that could possibly appear as the first byte of a
/// match, sets `pattern.fastmap[c] = true`.  The searcher (`re_search`) uses
/// this to skip positions that cannot start a match, giving a significant
/// speed-up for patterns that begin with a restricted set of characters.
///
/// Byte length of the opcode at `pc`, or `None` for a malformed buffer.
/// Conservative upper bound, in characters, on the buffer text a single
/// match attempt of this pattern can consume, or `None` when no finite
/// bound is known (repetition, backreferences, repeat counters,
/// range-table charsets, or a bound past the cap). Every jump in a
/// loop-free program points forward, so the program is a left-to-right
/// DAG and the sum of every consuming opcode bounds any path through it.
/// `exactn` counts bytes, which only overestimates its character count.
pub(crate) fn pattern_max_match_chars(pattern: &CompiledPattern) -> Option<usize> {
    const CAP: usize = 512;
    let bytecode = &pattern.buffer;
    let mut pc = 0usize;
    let mut total = 0usize;
    while pc < bytecode.len() {
        let op = RegexOp::from_byte(*bytecode.get(pc)?)?;
        match op {
            RegexOp::Exactn => total += *bytecode.get(pc + 1)? as usize,
            RegexOp::AnyChar
            | RegexOp::SyntaxSpec
            | RegexOp::NotSyntaxSpec
            | RegexOp::CategorySpec
            | RegexOp::NotCategorySpec
            | RegexOp::SyntaxSpecSet => total += 1,
            RegexOp::Charset | RegexOp::CharsetNot => {
                // The high bit marks a trailing range table that
                // `opcode_len` does not skip; walking past one would
                // misread its bytes as opcodes.
                if bytecode.get(pc + 1)? & 0x80 != 0 {
                    return None;
                }
                total += 1;
            }
            RegexOp::Duplicate | RegexOp::SucceedN | RegexOp::JumpN | RegexOp::SetNumberAt => {
                return None;
            }
            RegexOp::Jump
            | RegexOp::OnFailureJump
            | RegexOp::OnFailureKeepStringJump
            | RegexOp::OnFailureJumpLoop
            | RegexOp::OnFailureJumpNastyloop
            | RegexOp::OnFailureJumpSmart => {
                if extract_number(bytecode, pc + 1) < 0 {
                    return None;
                }
            }
            _ => {}
        }
        if total > CAP {
            return None;
        }
        match opcode_len(bytecode, pc) {
            Some(len) if len > 0 => pc += len,
            _ => return None,
        }
    }
    Some(total)
}

fn opcode_len(bytecode: &[u8], pc: usize) -> Option<usize> {
    let op = RegexOp::from_byte(*bytecode.get(pc)?)?;
    Some(match op {
        RegexOp::NoOp
        | RegexOp::Succeed
        | RegexOp::PosixEnd
        | RegexOp::AnyChar
        | RegexOp::BegLine
        | RegexOp::EndLine
        | RegexOp::BegBuf
        | RegexOp::EndBuf
        | RegexOp::AtDot
        | RegexOp::WordBound
        | RegexOp::NotWordBound
        | RegexOp::WordBeg
        | RegexOp::WordEnd
        | RegexOp::SymBeg
        | RegexOp::SymEnd => 1,
        RegexOp::Exactn => 2 + *bytecode.get(pc + 1)? as usize,
        RegexOp::Charset | RegexOp::CharsetNot => 2 + (*bytecode.get(pc + 1)? & 0x7F) as usize,
        RegexOp::StartMemory
        | RegexOp::StopMemory
        | RegexOp::Duplicate
        | RegexOp::SyntaxSpec
        | RegexOp::NotSyntaxSpec
        | RegexOp::CategorySpec
        | RegexOp::NotCategorySpec => 2,
        RegexOp::Jump
        | RegexOp::OnFailureJump
        | RegexOp::OnFailureKeepStringJump
        | RegexOp::OnFailureJumpLoop
        | RegexOp::OnFailureJumpNastyloop
        | RegexOp::OnFailureJumpSmart
        | RegexOp::SyntaxSpecSet => 3,
        RegexOp::SucceedN | RegexOp::JumpN | RegexOp::SetNumberAt => 5,
    })
}

/// Is `[start, end)` exactly one character-matching opcode?  GNU
/// `skip_one_char (laststart) == b` (regex-emacs.c:1928): the compiler
/// splits trailing multi-char `exactn`s before postfix operators, so a
/// simple body is a single `anychar` / one-char `exactn` / `charset` /
/// `syntaxspec` / `categoryspec`.
fn simple_one_char_body(buf: &CompiledPattern, start: usize, end: usize) -> bool {
    let bytecode = &buf.buffer;
    let Some(op) = bytecode.get(start).copied().and_then(RegexOp::from_byte) else {
        return false;
    };
    if !matches!(
        op,
        RegexOp::AnyChar
            | RegexOp::Exactn
            | RegexOp::Charset
            | RegexOp::CharsetNot
            | RegexOp::SyntaxSpec
            | RegexOp::NotSyntaxSpec
            | RegexOp::SyntaxSpecSet
            | RegexOp::CategorySpec
            | RegexOp::NotCategorySpec
    ) {
        return false;
    }
    if op == RegexOp::Exactn {
        // One character only (multibyte chars span several bytes).
        let count = match bytecode.get(start + 1) {
            Some(&count) => count as usize,
            None => return false,
        };
        let char_len = if buf.multibyte && count > 0 && start + 2 + count <= bytecode.len() {
            emacs_char::string_char(&bytecode[start + 2..start + 2 + count]).1
        } else {
            1
        };
        if count != char_len {
            return false;
        }
    }
    opcode_len(bytecode, start) == Some(end - start)
}

/// Conservative superset of the characters a single char-matching opcode
/// can accept: an ASCII bitmask plus a "may match something >= 0x80 /
/// multibyte" flag.  `None` = unanalyzable (syntax/category/class-bit
/// dependent — GNU resolves those against the live syntax table at its
/// runtime rewrite; our rewrite happens at compile time, so anything
/// table-dependent must stay unanalyzable to keep bytecode
/// table-independent).
#[derive(Clone, Copy, Default)]
struct CharSuperset {
    ascii: u128,
    high: bool,
}

impl CharSuperset {
    fn add_char(&mut self, ch: u32) {
        if ch < 0x80 {
            self.ascii |= 1u128 << ch;
        } else {
            self.high = true;
        }
    }

    fn union(&mut self, other: CharSuperset) {
        self.ascii |= other.ascii;
        self.high |= other.high;
    }

    fn disjoint(&self, other: &CharSuperset) -> bool {
        (self.ascii & other.ascii) == 0 && !(self.high && other.high)
    }
}

/// Match superset of the one char-matching opcode at `pos`.
fn one_char_match_superset(buf: &CompiledPattern, pos: usize) -> Option<CharSuperset> {
    let bytecode = &buf.buffer;
    let op = RegexOp::from_byte(*bytecode.get(pos)?)?;
    let mut set = CharSuperset::default();
    match op {
        RegexOp::Exactn => {
            let count = *bytecode.get(pos + 1)? as usize;
            if count == 0 || pos + 2 + count > bytecode.len() {
                return None;
            }
            // Pattern literals are stored translated (case-canonical);
            // the same holds for the sets compared against, so both
            // sides of the exclusivity test are in canonical space
            // (GNU compares RE_STRING_CHAR of the stored bytes too).
            if buf.multibyte {
                let (ch, _) = emacs_char::string_char(&bytecode[pos + 2..pos + 2 + count]);
                set.add_char(ch);
            } else {
                set.add_char(bytecode[pos + 2] as u32);
            }
        }
        RegexOp::AnyChar => {
            // Everything except newline (u128 bits 0..=127 = ASCII).
            set.ascii = !(1u128 << b'\n');
            set.high = true;
        }
        RegexOp::Charset => {
            let charset_pos = pos;
            let bitmap_len = (*bytecode.get(pos + 1)? & 0x7F) as usize;
            if pos + 2 + bitmap_len > bytecode.len() {
                return None;
            }
            // POSIX-class bits can match table-dependent sets; give up.
            if buf.charset_class_bits.contains_key(&charset_pos) {
                return None;
            }
            for c in 0..(bitmap_len * 8).min(256) {
                if (bytecode[pos + 2 + c / 8] >> (c % 8)) & 1 != 0 {
                    set.add_char(c as u32);
                }
            }
            if let Some(ranges) = buf.multibyte_charsets.get(&charset_pos) {
                for &(lo, hi) in ranges {
                    if (lo as u32) < 0x80 {
                        for c in (lo as u32)..=(hi as u32).min(0x7F) {
                            set.add_char(c);
                        }
                    }
                    if (hi as u32) >= 0x80 {
                        set.high = true;
                    }
                }
            }
        }
        RegexOp::CharsetNot => {
            let _charset_pos = pos;
            let bitmap_len = (*bytecode.get(pos + 1)? & 0x7F) as usize;
            if pos + 2 + bitmap_len > bytecode.len() {
                return None;
            }
            // Superset of "not in set": the bitmap complement plus all
            // multibyte characters (class bits and ranges only REMOVE
            // characters from a negated set, so ignoring them keeps
            // this a superset).
            for c in 0..128usize {
                let in_bitmap =
                    c / 8 < bitmap_len && (bytecode[pos + 2 + c / 8] >> (c % 8)) & 1 != 0;
                if !in_bitmap {
                    set.add_char(c as u32);
                }
            }
            set.high = true;
        }
        // Syntax and category tests depend on runtime tables.
        _ => return None,
    }
    Some(set)
}

/// Superset of the first characters matchable by the continuation
/// starting at `from` — a worklist walk in the style of GNU
/// `forall_firstchar` as used by `mutually_exclusive_p`
/// (regex-emacs.c:4025-4035).  Returns `None` when any reachable path is
/// unanalyzable or may accept the empty string (reaching `succeed` /
/// end-of-pattern), in which case the loop must stay backtrack-safe.
fn continuation_first_superset(buf: &CompiledPattern, from: usize) -> Option<CharSuperset> {
    let bytecode = &buf.buffer;
    let mut set = CharSuperset::default();
    let mut worklist: Vec<usize> = vec![from];
    let mut visited: HashSet<usize> = HashSet::new();

    while let Some(start_pc) = worklist.pop() {
        let mut pc = start_pc;
        loop {
            if !visited.insert(pc) {
                break;
            }
            if pc >= bytecode.len() {
                // Pattern may end right after the loop: a shorter
                // iteration count could produce an overall match, so the
                // fast loop is not safe (GNU only allows this in its
                // `unconstrained` refinement, which we skip).
                return None;
            }
            let op = RegexOp::from_byte(bytecode[pc])?;
            match op {
                RegexOp::Succeed | RegexOp::PosixEnd => return None,

                // Char-matching terminals: contribute their superset.
                RegexOp::Exactn | RegexOp::AnyChar | RegexOp::Charset | RegexOp::CharsetNot => {
                    set.union(one_char_match_superset(buf, pc)?);
                    break;
                }

                // Table-dependent matchers: unanalyzable.
                RegexOp::SyntaxSpec
                | RegexOp::NotSyntaxSpec
                | RegexOp::SyntaxSpecSet
                | RegexOp::CategorySpec
                | RegexOp::NotCategorySpec
                | RegexOp::Duplicate => return None,

                // endline is effectively `exactn \n` for this analysis
                // (GNU mutually_exclusive_one, `case endline`).
                RegexOp::EndLine => {
                    set.add_char(b'\n' as u32);
                    break;
                }

                // If the body matched a character we are not at
                // end-of-buffer, so `endbuf` fails: exclusive on this
                // path, nothing to add (GNU `case endbuf: return true`).
                RegexOp::EndBuf => break,

                // Other zero-width assertions cannot rescue a shorter
                // iteration count on their own (the character test
                // already fails there); walk through them.
                RegexOp::BegLine
                | RegexOp::BegBuf
                | RegexOp::AtDot
                | RegexOp::WordBound
                | RegexOp::NotWordBound
                | RegexOp::WordBeg
                | RegexOp::WordEnd
                | RegexOp::SymBeg
                | RegexOp::SymEnd
                | RegexOp::NoOp => {
                    pc += 1;
                }

                RegexOp::StartMemory | RegexOp::StopMemory => {
                    pc += 2;
                }

                RegexOp::Jump => {
                    let offset = extract_number(bytecode, pc + 1);
                    pc = ((pc as i64) + 3 + (offset as i64)) as usize;
                }

                RegexOp::OnFailureJump
                | RegexOp::OnFailureKeepStringJump
                | RegexOp::OnFailureJumpLoop
                | RegexOp::OnFailureJumpNastyloop
                | RegexOp::OnFailureJumpSmart => {
                    let offset = extract_number(bytecode, pc + 1);
                    worklist.push(((pc as i64) + 3 + (offset as i64)) as usize);
                    pc += 3;
                }

                RegexOp::SucceedN | RegexOp::JumpN => {
                    let offset = extract_number(bytecode, pc + 1);
                    worklist.push(((pc as i64) + 3 + (offset as i64)) as usize);
                    pc += 5;
                }

                RegexOp::SetNumberAt => {
                    pc += 5;
                }
            }
        }
    }
    Some(set)
}

/// GNU `mutually_exclusive_p` (regex-emacs.c:4025): true if "the loop
/// body at `body_start` matches something" implies "the continuation at
/// `cont` fails".  Conservative: only provable byte-set disjointness
/// counts, so a `false` merely keeps the safe backtracking loop.
fn mutually_exclusive_p(buf: &CompiledPattern, body_start: usize, cont: usize) -> bool {
    let Some(body) = one_char_match_superset(buf, body_start) else {
        return false;
    };
    let Some(cont_set) = continuation_first_superset(buf, cont) else {
        return false;
    };
    body.disjoint(&cont_set)
}

/// Resolve every `on_failure_jump_smart` in freshly-compiled bytecode.
///
/// GNU performs this rewrite lazily on first execution
/// (regex-emacs.c:4864-4906, discarding `const` and making `re_match`
/// non-reentrant); neomacs shares compiled patterns via a cache, so the
/// rewrite happens once here, immediately after compilation — same
/// resulting bytecode, no runtime mutation:
///
/// - exclusive loop (`mutually_exclusive_p`): the opcode becomes
///   `on_failure_keep_string_jump` and the trailing jump is retargeted
///   from the opcode to the loop body (GNU `STORE_NUMBER (p2 - 2,
///   mcnt + 3)`), so ONE failure point is pushed for the whole loop and
///   iterations do not save/restore the string position;
/// - otherwise it becomes a plain `on_failure_jump`.
fn resolve_smart_jumps(buf: &mut CompiledPattern) {
    let mut pc = 0;
    let mut prev_start: Option<usize> = None;
    while pc < buf.buffer.len() {
        let Some(len) = opcode_len(&buf.buffer, pc) else {
            return;
        };
        if buf.buffer[pc] == RegexOp::OnFailureJumpSmart as u8 {
            resolve_one_smart_jump(buf, pc, prev_start);
        }
        prev_start = Some(pc);
        pc += len;
    }
}

fn resolve_one_smart_jump(buf: &mut CompiledPattern, p3: usize, prev_start: Option<usize>) {
    // Expected shape (emitted by `compile_repetition`):
    //   p3: on_failure_jump_smart <offset -> p2>
    //   p3+3: <one-char body>
    //   p2-3: jump <offset -> p3>
    //   p2: continuation
    // Default to the safe plain loop; upgrade if the shape checks out
    // and the body excludes the continuation.
    buf.buffer[p3] = RegexOp::OnFailureJump as u8;
    if p3 + 3 > buf.buffer.len() {
        return;
    }
    let offset = extract_number(&buf.buffer, p3 + 1);
    let p2 = ((p3 + 3) as i64 + offset as i64) as usize;
    if p2 < p3 + 6 || p2 > buf.buffer.len() {
        return;
    }
    let jump_at = p2 - 3;
    if buf.buffer[jump_at] != RegexOp::Jump as u8 {
        return;
    }
    let back = extract_number(&buf.buffer, jump_at + 1);
    if ((jump_at + 3) as i64 + back as i64) as usize != p3 {
        return;
    }
    if mutually_exclusive_p(buf, p3 + 3, p2) {
        buf.buffer[p3] = RegexOp::OnFailureKeepStringJump as u8;
        store_number(&mut buf.buffer, jump_at + 1, back + 3);
        return;
    }
    // Non-exclusive loop: keep-string is off the table, so a PP*-duplicated
    // body only costs an extra dispatch and loop-head execution per loop
    // entry.  When the op right before the loop is a verified identical copy
    // of the body (`P; OFJ exit; P'; Jump OFJ` — PP* by construction, or a
    // literal `PP*` which is semantically the same `P+`), demote it in place
    // to the body-first plus shape the matcher fuses at per-iteration-frame
    // cost with no loop-entry overhead:
    //   P; OFJ exit; Jump P; <dead NoOps>
    // The dead NoOps keep the buffer length and every other offset stable;
    // the OFJ keeps its exit target.  `prev_start` comes from the resolver's
    // linear walk, so `P` is a real opcode boundary.
    let body_start = p3 + 3;
    let plen = jump_at - body_start;
    let Some(first_p) = prev_start else {
        return;
    };
    if first_p + plen != p3
        || buf.buffer[first_p..p3] != buf.buffer[body_start..jump_at]
        || buf.multibyte_charsets.get(&first_p) != buf.multibyte_charsets.get(&body_start)
        || buf.charset_class_bits.get(&first_p) != buf.charset_class_bits.get(&body_start)
    {
        return;
    }
    buf.buffer[body_start] = RegexOp::Jump as u8;
    let back_to_first = first_p as i64 - (body_start as i64 + 3);
    store_number(&mut buf.buffer, body_start + 1, back_to_first as i16);
    for byte in &mut buf.buffer[body_start + 3..p2] {
        *byte = RegexOp::NoOp as u8;
    }
    // The demoted copy's charset side entries are dead with it.
    buf.multibyte_charsets.remove(&body_start);
    buf.charset_class_bits.remove(&body_start);
}

// ---------------------------------------------------------------------------
// Peephole: fuse alternations of single positive syntax-class tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
thread_local! {
    /// Test-only switch to compile WITHOUT the syntax-class fusion peephole,
    /// so an A/B benchmark can time the exact same pattern fused vs unfused
    /// interleaved in one process (immune to whole-machine timing noise).
    static SYNTAX_FUSION_DISABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Run `f` with the syntax-class fusion peephole disabled (test only).
#[cfg(test)]
pub(crate) fn with_syntax_fusion_disabled<R>(f: impl FnOnce() -> R) -> R {
    SYNTAX_FUSION_DISABLED.with(|flag| flag.set(true));
    let r = f();
    SYNTAX_FUSION_DISABLED.with(|flag| flag.set(false));
    r
}

/// A fusible alternation branch of the exact 8-byte "triple" shape
/// `on_failure_jump(+5); syntaxspec(c); jump(->…->END)` that the alternation
/// compiler emits for a branch whose body is a single positive `\sC` / `\w`.
struct SyntaxTriple {
    /// Syntax class byte matched by the branch's `syntaxspec`.
    class: u8,
    /// Ultimate alternation END reached by following the branch's trailing
    /// `jump` chain (each non-final branch's `jump` targets the NEXT
    /// branch's `jump`, so the chain hops forward to the true END).
    end: usize,
}

/// If a maximal run of fusible positive-`syntaxspec` alternation branches
/// begins at `p`, describe it; otherwise `None`.
///
/// A run is a sequence of contiguous `on_failure_jump(+5); syntaxspec;
/// jump` triples (all `syntaxspec` — the positive form only — and all
/// resolving to the same alternation END), optionally extended by a
/// trailing *final* bare `syntaxspec` branch whose fall-through IS that END.
/// The trailing `jump` must point strictly FORWARD (past the triple): that
/// is what distinguishes an alternation branch from the visually identical
/// `on_failure_jump; body; jump(back)` shape of a resolved `*`/`+` loop.
struct FusibleRun {
    /// Union bitmask of all fused branches' syntax classes.
    mask: u16,
    /// One-past-the-end bytecode position of the whole rewritten region.
    region_end: usize,
    layout: FusionLayout,
}

enum FusionLayout {
    /// The run is followed by a further (non-fused) alternation branch at
    /// `p_next`; keep a single `on_failure_jump -> p_next` and a
    /// `jump -> end` around the fused op.
    Chain { p_next: usize, end: usize },
    /// The run reaches the alternation's final bare branch; the fused op
    /// simply falls through to `region_end` on success and fails to the
    /// enclosing failure point otherwise (no `on_failure_jump`/`jump`).
    FinalBare,
}

/// Resolve the absolute target of the offset-op at `pos` (`pc + 2 + offset`
/// with `pc` after the opcode byte).
fn jump_target(bytecode: &[u8], pos: usize) -> Option<usize> {
    if pos + 3 > bytecode.len() {
        return None;
    }
    let t = (pos as i64) + 3 + (extract_number(bytecode, pos + 1) as i64);
    if t < 0 || t as usize > bytecode.len() {
        return None;
    }
    Some(t as usize)
}

/// Follow a forward chain of `jump` ops starting at `pos`, returning the
/// first non-`jump` landing position.  `None` if the chain is malformed or
/// not strictly forward (which would risk a cycle).
fn follow_jump_chain(bytecode: &[u8], mut pos: usize) -> Option<usize> {
    // Each hop moves strictly forward, so the chain length is bounded by the
    // buffer size; no separate cycle guard is needed.
    while pos < bytecode.len() && bytecode[pos] == RegexOp::Jump as u8 {
        let next = jump_target(bytecode, pos)?;
        if next <= pos {
            return None; // must move strictly forward
        }
        pos = next;
    }
    Some(pos)
}

/// Recognize a single fusible triple starting at `q`.
fn parse_syntax_triple(bytecode: &[u8], q: usize) -> Option<SyntaxTriple> {
    if q + 8 > bytecode.len() {
        return None;
    }
    if bytecode[q] != RegexOp::OnFailureJump as u8 {
        return None;
    }
    // `on_failure_jump` must skip exactly the 2-byte syntaxspec + 3-byte
    // jump that follow it (target == q+8 = the next branch).
    if extract_number(bytecode, q + 1) != 5 {
        return None;
    }
    if bytecode[q + 3] != RegexOp::SyntaxSpec as u8 {
        return None;
    }
    let class = bytecode[q + 4];
    if bytecode[q + 5] != RegexOp::Jump as u8 {
        return None;
    }
    // The branch's `jump` target begins a (possibly one-hop) forward chain
    // of `jump`s that lands at the alternation END.  A strictly-forward
    // chain excludes the backward `jump` of a resolved `*`/`+` loop.
    let immediate = jump_target(bytecode, q + 5)?;
    if immediate <= q + 8 {
        return None;
    }
    let end = follow_jump_chain(bytecode, immediate)?;
    Some(SyntaxTriple { class, end })
}

fn detect_fusible_run(bytecode: &[u8], p: usize) -> Option<FusibleRun> {
    let first = parse_syntax_triple(bytecode, p)?;
    let end = first.end;
    let mut mask: u16 = 1u16 << first.class;
    let mut count = 1usize;
    let mut tail = p + 8; // start of the next branch after the run so far

    // Extend the run with further contiguous triples resolving to the same END.
    while let Some(t) = parse_syntax_triple(bytecode, tail) {
        if t.end != end {
            break;
        }
        mask |= 1u16 << t.class;
        count += 1;
        tail += 8;
    }

    // Optionally absorb the alternation's final bare `syntaxspec` branch:
    // it has no `on_failure_jump`/`jump`, and its fall-through position IS
    // the shared END.
    let final_bare = tail + 2 <= bytecode.len()
        && bytecode[tail] == RegexOp::SyntaxSpec as u8
        && tail + 2 == end;

    if final_bare {
        mask |= 1u16 << bytecode[tail + 1];
        // Need at least two fused branches for the rewrite to pay off.
        if count + 1 < 2 {
            return None;
        }
        Some(FusibleRun {
            mask,
            region_end: end, // == tail + 2
            layout: FusionLayout::FinalBare,
        })
    } else {
        // The run is followed by an unrelated branch at `tail`; keep the
        // alternation edge to it.  A single triple gains nothing here.
        if count < 2 {
            return None;
        }
        Some(FusibleRun {
            mask,
            region_end: tail,
            layout: FusionLayout::Chain { p_next: tail, end },
        })
    }
}

/// True if any control-transfer op ORIGINATING OUTSIDE `[p, region_end)`
/// targets a position in the OPEN interval `(p, region_end)`.  The fusion
/// overwrites the region's interior bytes, so a surviving *external*
/// reference into them would corrupt matching; if one exists we
/// conservatively skip the rewrite.  Ops inside the region are ignored:
/// they are the alternation's own branch jumps, which the fusion subsumes.
/// `p` itself stays a valid opcode boundary and is an allowed target.
fn any_target_inside(bytecode: &[u8], p: usize, region_end: usize) -> bool {
    let mut pc = 0usize;
    while pc < bytecode.len() {
        let Some(len) = opcode_len(bytecode, pc) else {
            // Malformed walk: be conservative and assume a hazard.
            return true;
        };
        if pc >= p && pc < region_end {
            // Interior op — overwritten by the fusion; its target is moot.
            pc += len;
            continue;
        }
        let target = match RegexOp::from_byte(bytecode[pc]) {
            Some(
                RegexOp::Jump
                | RegexOp::OnFailureJump
                | RegexOp::OnFailureKeepStringJump
                | RegexOp::OnFailureJumpLoop
                | RegexOp::OnFailureJumpNastyloop
                | RegexOp::OnFailureJumpSmart
                | RegexOp::SucceedN
                | RegexOp::JumpN
                | RegexOp::SetNumberAt,
            ) => {
                // All offset-bearing ops store their 2-byte offset right
                // after the opcode byte and resolve as `pc + 2 + offset`
                // with `pc` positioned after that opcode byte (== `pos+3`).
                Some(((pc + 1) as i64 + 2 + extract_number(bytecode, pc + 1) as i64) as usize)
            }
            _ => None,
        };
        if let Some(t) = target
            && t > p
            && t < region_end
        {
            return true;
        }
        pc += len;
    }
    false
}

/// Post-compile peephole that fuses alternations of single positive
/// syntax-class branches (`\w\|\s_`, `\sw\|\s_\|\s.`, …) into one
/// `SyntaxSpecSet` op.  Length-preserving: the fused op plus `NoOp` padding
/// exactly fills the original region, so every other jump offset,
/// HashMap-keyed position, and the fastmap walk stay valid without
/// relocation.  See `RegexOp::SyntaxSpecSet` for the correctness argument.
fn fuse_syntaxspec_alternations(buf: &mut CompiledPattern) {
    #[cfg(test)]
    if SYNTAX_FUSION_DISABLED.with(std::cell::Cell::get) {
        return;
    }
    let mut p = 0usize;
    while p < buf.buffer.len() {
        if let Some(run) = detect_fusible_run(&buf.buffer, p)
            && !any_target_inside(&buf.buffer, p, run.region_end)
        {
            apply_fusion(&mut buf.buffer, p, &run);
            p = run.region_end;
            continue;
        }
        match opcode_len(&buf.buffer, p) {
            Some(len) if len > 0 => p += len,
            _ => return,
        }
    }
}

fn apply_fusion(bytecode: &mut [u8], p: usize, run: &FusibleRun) {
    match run.layout {
        FusionLayout::Chain { p_next, end } => {
            // on_failure_jump -> p_next  (fail: try the next branch)
            bytecode[p] = RegexOp::OnFailureJump as u8;
            store_number(bytecode, p + 1, (p_next as i64 - (p + 3) as i64) as i16);
            // syntaxspecset <mask>
            bytecode[p + 3] = RegexOp::SyntaxSpecSet as u8;
            store_number_u16(bytecode, p + 4, run.mask);
            // jump -> end  (success: leave the alternation)
            bytecode[p + 6] = RegexOp::Jump as u8;
            store_number(bytecode, p + 7, (end as i64 - (p + 9) as i64) as i16);
            // Pad the remainder of the original region.
            for b in bytecode.iter_mut().take(run.region_end).skip(p + 9) {
                *b = RegexOp::NoOp as u8;
            }
        }
        FusionLayout::FinalBare => {
            // syntaxspecset <mask>, then fall through (padding) to END.
            bytecode[p] = RegexOp::SyntaxSpecSet as u8;
            store_number_u16(bytecode, p + 1, run.mask);
            for b in bytecode.iter_mut().take(run.region_end).skip(p + 3) {
                *b = RegexOp::NoOp as u8;
            }
        }
    }
}

/// Recompute the fastmap of an already-compiled pattern against a
/// (possibly buffer-local) syntax table.
///
/// GNU bakes `[[:word:]]` / `[[:space:]]` ASCII membership into the
/// charset bitmap with the buffer's syntax table at compile time and
/// keys the compiled-pattern cache by that table
/// (`search.c:compile_pattern`, `cp->syntax_table`).  Neomacs defers
/// the class membership test to match time, so only the FASTMAP bakes
/// table content; the front-end cache calls this to rebuild the
/// fastmap for the active table before caching a `used_syntax` entry
/// under that table's key.
pub(crate) fn recompute_fastmap(pattern: &mut CompiledPattern, syntax: &dyn SyntaxLookup) {
    compile_fastmap(pattern, syntax);
}

/// Translated from GNU regex-emacs.c `re_compile_fastmap` /
/// `analyze_first`.
///
/// `syntax` is consulted only to bake the ASCII members of
/// syntax-table-dependent POSIX classes (`[[:word:]]`, `[[:space:]]`)
/// of a *leading* charset into the fastmap — the analog of GNU baking
/// them into the charset bitmap at compile time (regex-emacs.c:2081).
/// Patterns whose fastmap took that path are flagged `used_syntax` at
/// compile and must be cache-keyed by syntax table.
fn compile_fastmap(pattern: &mut CompiledPattern, syntax: &dyn SyntaxLookup) {
    pattern.fastmap = [false; 256];
    pattern.can_be_null = false;

    let bytecode = &pattern.buffer;
    if bytecode.is_empty() {
        pattern.can_be_null = true;
        pattern.fastmap_accurate = true;
        return;
    }

    let case_fold = pattern.translate.is_some();

    // Worklist of bytecode positions still to process.
    let mut worklist: Vec<usize> = vec![0];
    let mut visited: HashSet<usize> = HashSet::new();

    while let Some(pc) = worklist.pop() {
        let mut pc = pc;

        loop {
            if !visited.insert(pc) {
                // Already processed this position — avoid infinite loops.
                break;
            }

            if pc >= bytecode.len() {
                // Fell off the end of bytecode — pattern can match empty string.
                pattern.can_be_null = true;
                break;
            }

            let Some(op) = RegexOp::from_byte(bytecode[pc]) else {
                break;
            };
            pc += 1;

            match op {
                RegexOp::Succeed | RegexOp::PosixEnd => {
                    // Pattern can succeed here (may match empty string).
                    pattern.can_be_null = true;
                    break;
                }

                RegexOp::Exactn => {
                    if pc >= bytecode.len() {
                        break;
                    }
                    let count = bytecode[pc] as usize;
                    pc += 1;
                    if count == 0 || pc >= bytecode.len() {
                        break;
                    }
                    let first = bytecode[pc];
                    pattern.fastmap[first as usize] = true;
                    if case_fold {
                        if first >= 0x80 {
                            // Multibyte character: the case-folded form may have
                            // a different leading byte (e.g. Cyrillic
                            // т = D1 82 vs Т = D0 A2), so byte-level case-folding
                            // of `first` is meaningless and would wrongly exclude
                            // the other case's lead byte. Conservatively allow all
                            // multibyte leading bytes, matching the Charset path.
                            for c in 128..256usize {
                                pattern.fastmap[c] = true;
                            }
                        } else {
                            let upper = (first as char)
                                .to_uppercase()
                                .next()
                                .unwrap_or(first as char)
                                as u8;
                            let lower = (first as char)
                                .to_lowercase()
                                .next()
                                .unwrap_or(first as char)
                                as u8;
                            pattern.fastmap[upper as usize] = true;
                            pattern.fastmap[lower as usize] = true;
                        }
                    }
                    break; // This opcode consumes input — done on this path.
                }

                RegexOp::AnyChar => {
                    // Matches any character except newline.
                    for c in 0..256usize {
                        if c != b'\n' as usize {
                            pattern.fastmap[c] = true;
                        }
                    }
                    break;
                }

                RegexOp::Charset => {
                    if pc >= bytecode.len() {
                        break;
                    }
                    let charset_op_pos = pc - 1;
                    let bitmap_len = bytecode[pc] as usize & 0x7F;
                    pc += 1;
                    for c in 0..256usize {
                        if c / 8 < bitmap_len
                            && pc + c / 8 < bytecode.len()
                            && (bytecode[pc + c / 8] >> (c % 8)) & 1 != 0
                        {
                            pattern.fastmap[c] = true;
                        }
                    }
                    // If this charset has multibyte ranges, conservatively
                    // allow all non-ASCII leading bytes in the fastmap.
                    if pattern.multibyte_charsets.contains_key(&charset_op_pos) {
                        for c in 128..256usize {
                            pattern.fastmap[c] = true;
                        }
                    }
                    // POSIX classes.  The bitmap only holds the fixed-set
                    // classes' ASCII members (`apply_posix_class`); the
                    // syntax-dependent ones (`[[:word:]]`, `[[:space:]]`)
                    // are re-derived per match from the active table, so
                    // bake their current ASCII members here — the analog
                    // of GNU compiling them into the bitmap with
                    // `re_iswctype` (regex-emacs.c:2081-2092).  Also set
                    // every translated partner, mirroring GNU's
                    // `SET_LIST_BIT (TRANSLATE (c))`, since the search
                    // skip loop indexes the fastmap by translated input.
                    // And a class can match multibyte characters, so all
                    // non-ASCII leading bytes become candidates — GNU's
                    // "If we can match a character class, we can match
                    // any multibyte characters" (analyze_first).
                    if let Some(&bits) = pattern.charset_class_bits.get(&charset_op_pos) {
                        for c in 0u8..0x80 {
                            if posix_class_matches(
                                c as u32,
                                bits,
                                None,
                                syntax,
                                PosixClassCaseMode::Sensitive,
                            ) {
                                pattern.fastmap[c as usize] = true;
                                if let Some(table) = &pattern.translate {
                                    pattern.fastmap[table.translate_byte(c) as usize] = true;
                                }
                            }
                        }
                        for c in 128..256usize {
                            pattern.fastmap[c] = true;
                        }
                    }
                    break;
                }

                RegexOp::CharsetNot => {
                    if pc >= bytecode.len() {
                        break;
                    }
                    let bitmap_len = bytecode[pc] as usize & 0x7F;
                    pc += 1;
                    for c in 0..256usize {
                        let in_set = if c / 8 < bitmap_len && pc + c / 8 < bytecode.len() {
                            (bytecode[pc + c / 8] >> (c % 8)) & 1 != 0
                        } else {
                            false
                        };
                        if !in_set {
                            pattern.fastmap[c] = true;
                        }
                    }
                    // GNU `analyze_first_fastmap` treats any `charset_not`
                    // as capable of matching multibyte characters beyond the
                    // bitmap, even when the bitmap covers every raw byte.
                    // The search loop will only try real character
                    // boundaries, so this conservative non-ASCII coverage is
                    // enough to avoid skipping candidates like `é` for
                    // `[^\x00-\xff]`.
                    for c in 128..256usize {
                        pattern.fastmap[c] = true;
                    }
                    break;
                }

                RegexOp::SyntaxSpec | RegexOp::NotSyntaxSpec | RegexOp::SyntaxSpecSet => {
                    // GNU `analyze_first` gives up on `\sX` / `\SX` in
                    // first-character position: "This match depends on
                    // text properties.  These end with aborting
                    // optimizations" (regex-emacs.c:3186-3190, returning
                    // failure so `re_compile_fastmap` sets
                    // `bufp->can_be_null = 1`).  The syntax class of a
                    // byte is a property of the *buffer's* table at match
                    // time, so no compile-time fastmap can restrict the
                    // candidate set without hardcoding a table.  Setting
                    // `can_be_null` disables the search-time fastmap skip
                    // exactly as in GNU `re_search_2`
                    // (regex-emacs.c:3483).
                    pattern.can_be_null = true;
                    pattern.fastmap_accurate = true;
                    return;
                }

                RegexOp::CategorySpec | RegexOp::NotCategorySpec => {
                    // Categories are too dynamic to predict — allow all bytes.
                    pattern.fastmap = [true; 256];
                    break;
                }

                // Zero-width assertions: they don't consume input, so we
                // continue to the next opcode to find what actually starts
                // the match.
                RegexOp::BegLine
                | RegexOp::EndLine
                | RegexOp::BegBuf
                | RegexOp::EndBuf
                | RegexOp::AtDot
                | RegexOp::WordBound
                | RegexOp::NotWordBound
                | RegexOp::WordBeg
                | RegexOp::WordEnd
                | RegexOp::SymBeg
                | RegexOp::SymEnd => {
                    // Continue to the next opcode.
                }

                RegexOp::StartMemory | RegexOp::StopMemory => {
                    // Skip the group number byte, continue.
                    pc += 1;
                }

                RegexOp::Duplicate => {
                    // Backreferences can match anything — set all.
                    pattern.fastmap = [true; 256];
                    break;
                }

                RegexOp::NoOp => {
                    // Continue to next opcode.
                }

                RegexOp::Jump => {
                    if pc + 1 >= bytecode.len() {
                        break;
                    }
                    let offset = extract_number(bytecode, pc);
                    pc = ((pc as i64) + 2 + (offset as i64)) as usize;
                    // Continue walking from the jump target (don't break).
                }

                RegexOp::OnFailureJump
                | RegexOp::OnFailureKeepStringJump
                | RegexOp::OnFailureJumpLoop
                | RegexOp::OnFailureJumpNastyloop
                | RegexOp::OnFailureJumpSmart => {
                    if pc + 1 >= bytecode.len() {
                        break;
                    }
                    let offset = extract_number(bytecode, pc);
                    pc += 2;
                    // Both the fallthrough (next opcode) and the jump target
                    // can start a match.  Push the jump target onto the
                    // worklist and continue with the fallthrough.
                    let target = ((pc as i64) + (offset as i64)) as usize;
                    worklist.push(target);
                    // Continue with the next opcode (fallthrough path).
                }

                RegexOp::SucceedN => {
                    // succeed_n <offset:2> <counter:2>
                    // When counter > 0, acts like a mandatory match of what follows.
                    // When counter == 0, acts like on_failure_jump.
                    // For fastmap purposes, both paths are possible.
                    if pc + 3 >= bytecode.len() {
                        break;
                    }
                    let offset = extract_number(bytecode, pc);
                    let target = ((pc as i64) + 2 + (offset as i64)) as usize;
                    worklist.push(target);
                    pc += 4; // skip offset + counter, continue with fallthrough
                }

                RegexOp::JumpN => {
                    // jump_n <offset:2> <counter:2>
                    // If counter > 0, jumps; if counter == 0, falls through.
                    // For fastmap, both paths are possible.
                    if pc + 3 >= bytecode.len() {
                        break;
                    }
                    let offset = extract_number(bytecode, pc);
                    let target = ((pc as i64) + 2 + (offset as i64)) as usize;
                    worklist.push(target);
                    pc += 4; // fallthrough
                }

                RegexOp::SetNumberAt => {
                    // set_number_at <offset:2> <value:2> — no input consumed.
                    pc += 4;
                }
            }
        }
    }

    pattern.fastmap_accurate = true;
}

// ---------------------------------------------------------------------------
// Multi-literal SIMD prefilter (sound literal extraction)
// ---------------------------------------------------------------------------

/// Cap on the number of distinct required literals handed to the prefilter.
/// Big keyword alternations (`el-defs` has ~40 `def*` heads) must fit; beyond
/// this, bail to the fastmap (the needle set stops being a useful filter).
const PREFILTER_MAX_LITERALS: usize = 128;
/// Cap on a single literal's length.  Reaching it finalizes the required
/// prefix early — a SHORTER required prefix is still a sound required literal
/// (every match on that path still starts with it), so this only trades
/// specificity for a bounded needle, never correctness.
const PREFILTER_MAX_LITERAL_LEN: usize = 64;
/// Total opcode-visit budget for the extraction walk; guarantees termination
/// on pathological bytecode (returns `None`, i.e. no prefilter — always safe).
const PREFILTER_BUDGET: usize = 16_384;

/// Build a SOUND multi-literal prefilter from the compiled bytecode, or
/// `None` when no required-literal set can be proven (then `re_search` keeps
/// the byte fastmap — always correct).
///
/// SOUNDNESS is the only correctness requirement: the backtracker verifies
/// every candidate the prefilter surfaces, so a prefilter is correct as long
/// as **every match contains one of the returned literals at
/// `offset` bytes from the match start**.  A prefilter that skips *past* a
/// real match (an unsound literal) is a bug caught by the differential fuzzer.
///
/// The extraction enumerates the required *offset-0* prefix set of the
/// pattern (`collect_prefix_literals`): a run of leading `Exactn`s (with
/// zero-width assertions / group markers skipped) forms a forced prefix, and
/// a leading alternation whose every arm begins with an `Exactn` splits that
/// prefix across the arms (`(\(catch\|throw\|require\)` → `["(catch",
/// "(throw", "(require"]`).  If ANY reachable path can begin a match without a
/// literal byte at offset 0 (a leading `\w` / charset / `.` / nullable arm),
/// extraction bails — no prefilter.  Offset is always `0`: a required literal
/// at a fixed byte offset k>0 would need a fixed-byte-width non-literal prefix,
/// which is fragile under multibyte, so it is deliberately not attempted
/// (conservative = sound).
///
/// Case-fold patterns are skipped (the fastmap already handles them via
/// `TRANSLATE` indexing; folding every literal into the needle set is left as
/// a future refinement).  Patterns whose only required literals are single
/// bytes are skipped too — the fastmap's `memchr` already covers those, so a
/// prefilter would add cost without narrowing the candidate set.
fn build_literal_prefilter(pattern: &CompiledPattern) -> Option<LiteralPrefilter> {
    // Case-fold: skip (sound + simple).  See doc comment.
    if pattern.translate.is_some() {
        return None;
    }
    // Nullable patterns match the empty string, so no non-empty literal is
    // required.  `re_search` also disables the fastmap skip for these
    // (GNU regex-emacs.c:3483) — keep the same gate.
    if pattern.can_be_null {
        return None;
    }
    if pattern.buffer.is_empty() {
        return None;
    }

    let mut literals: Vec<Vec<u8>> = Vec::new();
    let mut budget = PREFILTER_BUDGET;
    let mut visited: Vec<usize> = Vec::new();
    // A `None` here means some match path has no proven offset-0 literal —
    // extraction is unsound to use, so no prefilter at all.
    collect_prefix_literals(
        pattern,
        0,
        Vec::new(),
        &mut literals,
        &mut budget,
        &mut visited,
    )?;

    if literals.is_empty() {
        return None;
    }
    // Deduplicate (nested alternations can re-derive the same prefix).
    literals.sort();
    literals.dedup();
    // If every required literal is a single byte, the fastmap's memchr scan
    // already skips exactly these positions — a prefilter buys nothing.
    if literals.iter().all(|l| l.len() <= 1) {
        return None;
    }

    // `regex-automata` auto-selects memchr / memmem / Teddy / Aho-Corasick.
    let pf = RaPrefilter::new(MatchKind::LeftmostFirst, &literals)?;
    // Only adopt it when the crate deems the scan actually fast (otherwise the
    // fastmap path is at least as good).
    if !pf.is_fast() {
        return None;
    }
    Some(LiteralPrefilter { pf, offset: 0 })
}

/// Finalize a branch's accumulated required prefix.  An EMPTY prefix means the
/// path can begin a match without any offset-0 literal, so the whole pattern
/// is un-prefilterable (return `None`); a non-empty prefix is a sound required
/// literal for every match on this path.
fn finalize_prefix(prefix: Vec<u8>, out: &mut Vec<Vec<u8>>) -> Option<()> {
    if prefix.is_empty() {
        return None;
    }
    out.push(prefix);
    Some(())
}

/// Walk the bytecode from `pc`, accumulating the forced literal `prefix`
/// (offset 0), branching at alternation / quantifier `on_failure_jump`s.  On
/// every terminal (a non-literal consuming op, end-of-pattern, `succeed`, a
/// jump, or a loop back-edge) the accumulated prefix is finalized as one
/// required literal.  Returns `None` (bail the whole prefilter) if any path
/// finalizes with an empty prefix, if the budget/literal caps are exceeded, or
/// if the bytecode is malformed.
fn collect_prefix_literals(
    buf: &CompiledPattern,
    mut pc: usize,
    mut prefix: Vec<u8>,
    out: &mut Vec<Vec<u8>>,
    budget: &mut usize,
    visited: &mut Vec<usize>,
) -> Option<()> {
    let bytecode = &buf.buffer;
    loop {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        if out.len() > PREFILTER_MAX_LITERALS {
            return None;
        }
        // End of pattern: a match may end here.  The prefix is required iff
        // non-empty (empty => a nullable/empty-arm path => unsound).
        if pc >= bytecode.len() {
            return finalize_prefix(prefix, out);
        }
        // Loop back-edge already on this path: stop and finalize.  The
        // accumulated prefix (>= one body iteration's literal) is still
        // required on the "took the loop" path; the exit branch (enumerated at
        // the loop's on_failure_jump) covers the "skipped it" path.
        if visited.contains(&pc) {
            return finalize_prefix(prefix, out);
        }
        visited.push(pc);

        let op = RegexOp::from_byte(bytecode[pc])?;
        match op {
            RegexOp::Exactn => {
                let count = *bytecode.get(pc + 1)? as usize;
                if count == 0 || pc + 2 + count > bytecode.len() {
                    return None;
                }
                prefix.extend_from_slice(&bytecode[pc + 2..pc + 2 + count]);
                if prefix.len() >= PREFILTER_MAX_LITERAL_LEN {
                    // Sound early cut: a shorter required prefix still holds.
                    return finalize_prefix(prefix, out);
                }
                pc += 2 + count;
            }

            // Zero-width assertions and bookkeeping: do not consume input or
            // shift the offset — walk through them, offset unchanged.
            RegexOp::BegLine
            | RegexOp::EndLine
            | RegexOp::BegBuf
            | RegexOp::EndBuf
            | RegexOp::AtDot
            | RegexOp::WordBound
            | RegexOp::NotWordBound
            | RegexOp::WordBeg
            | RegexOp::WordEnd
            | RegexOp::SymBeg
            | RegexOp::SymEnd
            | RegexOp::NoOp => {
                pc += 1;
            }

            RegexOp::StartMemory | RegexOp::StopMemory => {
                pc += 2;
            }

            // Alternation / quantifier split: BOTH the fallthrough arm and the
            // jump target can begin a match, and every match takes exactly one
            // path, so the UNION of both arms' required literals is sound.
            RegexOp::OnFailureJump
            | RegexOp::OnFailureKeepStringJump
            | RegexOp::OnFailureJumpLoop
            | RegexOp::OnFailureJumpNastyloop
            | RegexOp::OnFailureJumpSmart => {
                if pc + 2 >= bytecode.len() {
                    return None;
                }
                let offset = extract_number(bytecode, pc + 1) as i64;
                let target = ((pc as i64) + 3 + offset) as usize;
                let fallthrough = pc + 3;
                let mut v1 = visited.clone();
                collect_prefix_literals(buf, fallthrough, prefix.clone(), out, budget, &mut v1)?;
                let mut v2 = visited.clone();
                return collect_prefix_literals(buf, target, prefix, out, budget, &mut v2);
            }

            // Every other op either consumes NON-literal input (AnyChar,
            // Charset(Not), Syntax/Category specs, Duplicate) or is a control
            // terminal for this analysis (Succeed, Jump, SucceedN, JumpN,
            // SetNumberAt).  In all cases the required prefix so far is
            // finalized; a non-literal consuming op after a non-empty prefix is
            // fine (the prefix is still required), and reaching one with an
            // empty prefix means no offset-0 literal is required => bail.
            _ => return finalize_prefix(prefix, out),
        }
    }
}

// Test/fuzz-only escape hatch: force `re_search` down the no-fastmap path so
// equivalence checks can prove search optimizations never change match results.
#[cfg(any(test, feature = "fuzzing"))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SearchOptimizationOverride {
    #[default]
    Production,
    Disabled,
}

#[cfg(any(test, feature = "fuzzing"))]
thread_local! {
    static SEARCH_OPTIMIZATION_OVERRIDE: std::cell::Cell<SearchOptimizationOverride> =
        const { std::cell::Cell::new(SearchOptimizationOverride::Production) };
}

#[cfg(any(test, feature = "fuzzing"))]
fn fastmap_force_disabled() -> bool {
    SEARCH_OPTIMIZATION_OVERRIDE.with(|slot| slot.get() == SearchOptimizationOverride::Disabled)
}

/// Run `f` with the fastmap AND prefilter search-skips disabled, so
/// `re_search` scans every position through the pure backtracker/Pike matcher.
/// This is the ORACLE for the prefilter differential fuzzer: comparing it
/// against the normal (prefilter-enabled) search proves the prefilter never
/// changes match results. The previous policy is restored even if `f` unwinds.
#[cfg(any(test, feature = "fuzzing"))]
pub(crate) fn with_fastmap_disabled<R>(f: impl FnOnce() -> R) -> R {
    struct Guard(SearchOptimizationOverride);

    impl Drop for Guard {
        fn drop(&mut self) {
            SEARCH_OPTIMIZATION_OVERRIDE.with(|slot| slot.set(self.0));
        }
    }

    let previous = SEARCH_OPTIMIZATION_OVERRIDE
        .with(|slot| slot.replace(SearchOptimizationOverride::Disabled));
    let _guard = Guard(previous);
    f()
}

#[cfg(not(any(test, feature = "fuzzing")))]
#[inline(always)]
fn fastmap_force_disabled() -> bool {
    false
}

/// Fastmap byte set when it is small (<= 3 bytes) and pure ASCII — the
/// cases where `memchr`/`memchr2`/`memchr3` can drive the forward skip
/// loop.
fn sparse_ascii_fastmap(fastmap: &[bool; 256]) -> Option<SmallVec<[u8; 3]>> {
    let mut bytes: SmallVec<[u8; 3]> = SmallVec::new();
    for (byte, &set) in fastmap.iter().enumerate() {
        if set {
            if byte >= 0x80 || bytes.len() == 3 {
                return None;
            }
            bytes.push(byte as u8);
        }
    }
    if bytes.is_empty() { None } else { Some(bytes) }
}

/// Search for a match of the compiled pattern in text.
///
/// Equivalent to GNU's `re_search_2()` operating on a single
/// contiguous string. GNU also exposes the two-string variant
/// `re_match_2(pattern, string1, size1, string2, size2, ...)` which
/// walks the buffer text across the gap boundary
/// (`BEG..GPT` and `GPT..ZV`) without copying — for a 100MB buffer
/// that saves a 100MB allocation per search. Audit finding #17 in
/// `drafts/regex-search-audit.md` flags this as missing in neomacs.
///
/// We currently allocate the full buffer text via
/// `Buffer::buffer_substring_range(Buffer::accessible_emacs_byte_range())` at
/// the call site in `regex/mod.rs::re_search_forward_with_posix` and friends, which is
/// correctness-equivalent to GNU's `re_match_2_internal` running
/// over a single string but is O(buffer-size) per search instead of
/// O(match-length). Porting the gap-aware path is a separate
/// optimization (audit Phase D Task 4.1, ~1 day).
///
/// # Arguments
/// * `pattern` - Compiled pattern
/// * `text` - Input text
/// * `start` - Starting search position
/// * `range` - How far to search (positive = forward, negative = backward)
/// * `syntax` - Syntax table lookup
/// * `point` - Buffer point (for `\=`)
///
/// # Returns
/// * `Some((match_start, registers))` if found
/// * `None` if no match
pub(crate) fn re_search(
    pattern: &CompiledPattern,
    text: &[u8],
    start: usize,
    range: isize,
    syntax: &dyn SyntaxLookup,
    point: usize,
) -> Option<(usize, MatchRegisters)> {
    let text_len = text.len();
    // GNU regex-emacs.c:3483: `if (fastmap && startpos < total_size &&
    // !bufp->can_be_null)` — the fastmap skip is gated ONLY on having an
    // accurate map and the pattern not matching the null string.  Syntax-
    // dependent constructs do not disable it: leading `\w`/`\sC` set
    // `can_be_null` via `analyze_first`, zero-width `\b \< \_>` etc.
    // contribute the following atom's characters, and `[[:word:]]` /
    // `[[:space:]]` bake the active table into the fastmap with the
    // compile cache keyed by that table (`used_syntax`).
    let use_fastmap = pattern.fastmap_accurate && !pattern.can_be_null && !fastmap_force_disabled();
    let translate = pattern.translate.as_ref();

    // `^`-anchored pattern: a forward candidate can only match at position 0
    // or right after a newline, so reject others with one byte compare
    // instead of a full matcher entry (scratch borrow, register reset,
    // dispatch) per fastmap hit. GNU enters the matcher and fails at
    // `begline`; the candidate set is identical either way.
    let bol_anchored = {
        let mut pc = 0;
        while pc < pattern.buffer.len() && pattern.buffer[pc] == RegexOp::NoOp as u8 {
            pc += 1;
        }
        pc < pattern.buffer.len() && pattern.buffer[pc] == RegexOp::BegLine as u8
    };

    // A fresh search starts with a clean overflow flag; a candidate match
    // that hits the fail-stack limit sets it, aborting the whole scan
    // (GNU re_search_2 propagates re_match_2_internal's -2 immediately).
    clear_matcher_overflow();
    macro_rules! try_candidate {
        ($pos:expr, $stop:expr) => {
            match re_match_candidate(pattern, text, $pos, $stop, syntax, point) {
                Some(result) => Some(result),
                None => {
                    if matcher_overflow_pending() {
                        return None;
                    }
                    None
                }
            }
        };
    }

    if range >= 0 {
        // Forward search
        let end = (start + range as usize).min(text_len);
        let mut pos = start;
        if bol_anchored {
            // `^`-anchored: candidates are position 0 and each byte after a
            // newline — drive the scan with memchr('\n') (SIMD) instead of
            // entering the matcher at every fastmap hit only to fail at
            // `begline`. The fastmap still pre-rejects a line whose first
            // byte can't start a match; BOL positions are always char
            // boundaries, so no continuation-byte check is needed.
            loop {
                if pos > end {
                    return None;
                }
                if pos == 0 || text[pos - 1] == b'\n' {
                    let first_ok = !use_fastmap
                        || pos >= text_len
                        || match translate {
                            Some(table) => {
                                pattern.fastmap[table.translate_byte(text[pos]) as usize]
                            }
                            None => pattern.fastmap[text[pos] as usize],
                        };
                    if first_ok && let Some(result) = try_candidate!(pos, end) {
                        return Some((pos, result.1));
                    }
                }
                let search_from = pos.min(text_len);
                match memchr::memchr(b'\n', &text[search_from..text_len]) {
                    Some(idx) => pos = search_from + idx + 1,
                    None => return None,
                }
            }
        }
        if use_fastmap {
            if let Some(pref) = &pattern.prefilter {
                // SIMD multi-literal skip: jump straight to the next position
                // whose text provably contains a required match literal, far
                // more aggressively than the single-byte fastmap.  The
                // backtracker still verifies every candidate, so correctness
                // rests only on the literal set being SOUND — every match
                // contains one of the needles at `off` bytes from its start
                // (see `build_literal_prefilter`).  The prefilter is only ever
                // built for non-case-fold patterns, so `translate` is None
                // here.
                let off = pref.offset;
                // The literal for a match starting at `m` sits at
                // `text[m + off ..]`; the earliest candidate is `start`, whose
                // literal begins at `start + off`.
                let mut next_lit = start.saturating_add(off).min(text_len);
                while next_lit <= text_len {
                    let Some(span) = pref.pf.find(
                        text,
                        Span {
                            start: next_lit,
                            end: text_len,
                        },
                    ) else {
                        break;
                    };
                    let lit_at = span.start;
                    let cand = lit_at.saturating_sub(off);
                    if cand > end {
                        break;
                    }
                    // Advance past this literal occurrence for the next probe,
                    // regardless of whether this candidate matches — keeps the
                    // scan strictly monotonic (no infinite loop).
                    next_lit = lit_at + 1;
                    if cand < start {
                        // Literal too early to back up to an in-range start.
                        continue;
                    }
                    // A match cannot start inside a multibyte character; the
                    // needle bytes are exact, so a continuation-byte candidate
                    // is never a real match — skip it.
                    if pattern.target_multibyte && cand < text_len && (text[cand] & 0xC0) == 0x80 {
                        continue;
                    }
                    if let Some(result) = try_candidate!(cand, end) {
                        return Some((cand, result.1));
                    }
                }
            } else if let Some(table) = translate {
                while pos <= end {
                    if pos > text_len {
                        break;
                    }
                    // Skip UTF-8 continuation bytes — only try match at character
                    // boundaries to avoid matching in the middle of a multibyte char.
                    if pattern.target_multibyte && pos < text_len && (text[pos] & 0xC0) == 0x80 {
                        pos += 1;
                        continue;
                    }
                    // GNU disables fastmap skipping for nullable patterns so zero-width
                    // matches like `\\(?:...\\)\\=` are still considered at every point.
                    //
                    // GNU regex-emacs.c:3568 applies TRANSLATE to the input
                    // byte before indexing the fastmap. Under case-fold that
                    // is what lets a fastmap built for a bitmap of lowercase
                    // characters still catch uppercase input (audit #9).
                    if pos < text_len {
                        let idx = table.translate_byte(text[pos]) as usize;
                        if !pattern.fastmap[idx] {
                            pos += 1;
                            continue;
                        }
                    }
                    if let Some(result) = try_candidate!(pos, end) {
                        return Some((pos, result.1));
                    }
                    pos += 1;
                }
            } else if let Some(bytes) = sparse_ascii_fastmap(&pattern.fastmap) {
                // The candidate first-byte set is tiny and pure ASCII
                // (e.g. `{'('}` for the font-lock defun matchers):
                // let memchr's SIMD scan find candidates instead of
                // testing the fastmap byte by byte.  ASCII hits are
                // never UTF-8 continuation bytes, so the char-boundary
                // skip is vacuous here.  GNU uses the plain
                // `while (range > lim && !fastmap[*d]) d++` loop; the
                // candidate set and attempt positions are identical.
                let hi = if end < text_len { end + 1 } else { text_len };
                while pos <= end {
                    if pos < text_len {
                        let found = match *bytes.as_slice() {
                            [b0] => memchr::memchr(b0, &text[pos..hi]),
                            [b0, b1] => memchr::memchr2(b0, b1, &text[pos..hi]),
                            [b0, b1, b2] => memchr::memchr3(b0, b1, b2, &text[pos..hi]),
                            _ => unreachable!("sparse_ascii_fastmap yields 1..=3 bytes"),
                        };
                        match found {
                            Some(idx) => pos += idx,
                            None => {
                                // No candidate byte before `hi`; the only
                                // remaining attempt position is text_len
                                // itself (when the range allows it).
                                pos = text_len;
                                if pos > end {
                                    break;
                                }
                            }
                        }
                    }
                    if let Some(result) = try_candidate!(pos, end) {
                        return Some((pos, result.1));
                    }
                    pos += 1;
                }
            } else {
                while pos <= end {
                    if pos > text_len {
                        break;
                    }
                    if pattern.target_multibyte && pos < text_len && (text[pos] & 0xC0) == 0x80 {
                        pos += 1;
                        continue;
                    }
                    if pos < text_len && !pattern.fastmap[text[pos] as usize] {
                        pos += 1;
                        continue;
                    }
                    if let Some(result) = try_candidate!(pos, end) {
                        return Some((pos, result.1));
                    }
                    pos += 1;
                }
            };
        } else {
            while pos <= end {
                if pos > text_len {
                    break;
                }
                if pattern.target_multibyte && pos < text_len && (text[pos] & 0xC0) == 0x80 {
                    pos += 1;
                    continue;
                }
                if let Some(result) = try_candidate!(pos, end) {
                    return Some((pos, result.1));
                }
                pos += 1;
            }
        }
    } else {
        // Backward search
        let end = start.saturating_sub((-range) as usize);
        if use_fastmap {
            if let Some(table) = translate {
                for pos in (end..=start).rev() {
                    // Skip UTF-8 continuation bytes — only try at character boundaries.
                    if pattern.target_multibyte && pos < text_len && (text[pos] & 0xC0) == 0x80 {
                        continue;
                    }
                    // GNU disables fastmap skipping for nullable patterns so zero-width
                    // matches like `\\(?:...\\)\\=` are still considered at every point.
                    if pos < text_len {
                        let idx = table.translate_byte(text[pos]) as usize;
                        if !pattern.fastmap[idx] {
                            continue;
                        }
                    }
                    // GNU `search.c:1195-1201` calls `re_search_2` for backward
                    // searches with STOP set to the point where the search began.
                    // That means a candidate may start before `start`, but it may
                    // not extend past it.  This prevents a repeated backward search
                    // from re-matching the same non-empty match that begins at
                    // point but ends after it.
                    if let Some(result) = try_candidate!(pos, start) {
                        return Some((pos, result.1));
                    }
                }
            } else {
                for pos in (end..=start).rev() {
                    if pattern.target_multibyte && pos < text_len && (text[pos] & 0xC0) == 0x80 {
                        continue;
                    }
                    if pos < text_len && !pattern.fastmap[text[pos] as usize] {
                        continue;
                    }
                    if let Some(result) = try_candidate!(pos, start) {
                        return Some((pos, result.1));
                    }
                }
            }
        } else {
            for pos in (end..=start).rev() {
                // Skip UTF-8 continuation bytes — only try at character boundaries.
                if pattern.target_multibyte && pos < text_len && (text[pos] & 0xC0) == 0x80 {
                    continue;
                }
                // GNU `search.c:1195-1201` calls `re_search_2` for backward
                // searches with STOP set to the point where the search began.
                // That means a candidate may start before `start`, but it may
                // not extend past it.  This prevents a repeated backward search
                // from re-matching the same non-empty match that begins at
                // point but ends after it.
                if let Some(result) = try_candidate!(pos, start) {
                    return Some((pos, result.1));
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Convenience: compile + search in one call
// ---------------------------------------------------------------------------

/// Compile a pattern and search for it in text.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn search_pattern(
    pattern_str: &str,
    text: &str,
    start: usize,
    case_fold: bool,
    syntax: &dyn SyntaxLookup,
    point: usize,
) -> Result<Option<(usize, MatchRegisters)>, RegexCompileError> {
    let compiled = regex_compile(pattern_str, false, case_fold)?;
    Ok(re_search(
        &compiled,
        text.as_bytes(),
        start,
        (text.len() - start) as isize,
        syntax,
        point,
    ))
}

/// Compile a pattern and match at a specific position.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn match_pattern(
    pattern_str: &str,
    text: &str,
    pos: usize,
    case_fold: bool,
    syntax: &dyn SyntaxLookup,
    point: usize,
) -> Result<Option<(usize, MatchRegisters)>, RegexCompileError> {
    let compiled = regex_compile(pattern_str, false, case_fold)?;
    Ok(re_match(
        &compiled,
        text.as_bytes(),
        pos,
        text.len(),
        syntax,
        point,
    ))
}

#[cfg(test)]
#[path = "tests/emacs.rs"]
mod tests;
