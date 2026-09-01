//! Regex engine and search primitives for the Elisp VM.
//!
//! Uses a direct translation of GNU Emacs's `regex-emacs.c` as the backend.
//! All pattern compilation, matching, and searching goes through the
//! `regex_emacs` module, ensuring 100% semantic compatibility with GNU.
//! Any pattern originating from Lisp must enter through this module; the
//! separately named `native-regex` dependency is reserved for patterns defined
//! entirely by Rust implementation code.
//!
//! # Audit-tracked boundaries vs GNU
//!
//! The audit at `drafts/regex-search-audit.md` tracks divergences
//! from GNU `src/search.c`. Audit findings 1, 2, 3, 4, 7, 8, 9, 10,
//! 11, 12, 14, 16, and 20 have been addressed. The remaining
//! intentionally-deferred items are:
//!
//! - **#5** (translate table byte-only) — full Unicode case-canon
//!   table refactor; covered by a doc comment in `regex_compile`.
//! - **#13** (replace-match multibyte/unibyte) — still routed
//!   through a storage-string compatibility seam instead of a
//!   direct GNU-style byte conversion loop; documented inline.
//! - **#15** (cache key narrow) — extra cache axes (syntax table
//!   identity, multibyte flag) are placeholders for features
//!   neomacs does not have yet; documented inline.
//! - **#17** (gap-aware `re_match_2`) — perf, not correctness.
//!   Each search currently materializes the buffer text via
//!   `Buffer::buffer_substring_range(Buffer::accessible_emacs_byte_range())`
//!   instead of walking across the gap boundary. GNU `regex-emacs.c:re_match_2` would save
//!   a buffer-sized allocation per search. Audit Phase D Task 4.1.
//! - **#18** (boyer_moore literal search) — perf, not correctness.
//!   `literal_find` uses naive `str::find` instead of GNU's
//!   Boyer-Moore-with-skip-table from `src/search.c:1761+`. Audit
//!   Phase D Task 4.2.
//! - **#19** (internal C helpers) — `save_search_regs`,
//!   `update_search_regs`, `freeze_pattern` / `unfreeze_pattern`,
//!   `find_newline1`, and `wordify` are GNU-internal helpers used
//!   on the `running_asynch_code` path and during async-signal
//!   handling. neomacs does not run user signal handlers in C
//!   code or expose `running_asynch_code`, so these helpers have
//!   no consumers. `wordify` (`\bword\b` from a literal word) is
//!   already implemented in elisp via the `wordify` function
//!   defined in `subr.el`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::buffer::{
    Buffer, BufferId, CharPos0, CharRange, EmacsBytePos, EmacsByteRange, LispCharPos1,
};
use crate::emacs_core::casefiddle::apply_replace_match_case;
use crate::emacs_core::regex_emacs::{
    self, BufferSyntaxLookup, CaseTranslation, CompiledPattern, DefaultSyntaxLookup,
    MatchRegisters, SyntaxCacheKey, SyntaxLookup,
};
use crate::heap_types::LispString;

pub(crate) const REPLACE_MATCH_SUBEXP_MISSING: &str = "replace-match subexpression does not exist";
const GNU_SEARCH_REGS_BASE_CAPACITY: usize = 7;
// Compiled-pattern LRU caches. GNU's `regexp_cache` (search.c) holds 20 entries
// and that suffices there because GNU compiles a pattern into a cheap 256-byte
// fastmap. neomacs compiles into a `regex_automata` Teddy prefilter, which is far
// more expensive to build, so an eviction re-pays a much higher cost. font-lock
// searches with *every* regexp in `font-lock-keywords` per fontified region — a
// typical major mode has well over 20 — so a 20-entry cache thrashes and rebuilds
// prefilters on every redisplay. Size these to comfortably hold a few modes'
// worth of keyword patterns; the lookup is a short linear scan of small byte
// slices, negligible next to a single prefilter rebuild. Each cache has its own
// constant so they can be tuned independently.
const SEARCH_PATTERN_CACHE_SIZE: usize = 128;
const LISP_REGEX_PATTERN_CACHE_SIZE: usize = 128;
// GNU's `\=` assertion compares against the current buffer's PT_BYTE even
// during `string-match` (`regex-emacs.c:5201`). A standalone string has no
// matching buffer byte address, so pass an unreachable point to the translated
// matcher instead of treating the START argument as point.
const STRING_MATCH_AT_DOT_UNREACHABLE: usize = usize::MAX;

/// Failure while applying a Lisp-visible regexp as a boolean predicate.
///
/// Keeping this error at the Emacs-regexp boundary prevents callers from
/// exposing errors from an implementation-specific native regexp dialect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EmacsRegexpError {
    Compile(String),
    MatcherOverflow,
}

impl std::fmt::Display for EmacsRegexpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compile(message) => f.write_str(message),
            Self::MatcherOverflow => f.write_str(regex_emacs::MATCHER_OVERFLOW_MESSAGE),
        }
    }
}

fn buffer_syntax_lookup(buf: &Buffer) -> BufferSyntaxLookup {
    buffer_syntax_lookup_with_word_boundary(
        buf,
        crate::emacs_core::regex_emacs::WordBoundaryLookup::default(),
    )
}

fn buffer_syntax_lookup_with_word_boundary(
    buf: &Buffer,
    word_boundary: crate::emacs_core::regex_emacs::WordBoundaryLookup,
) -> BufferSyntaxLookup {
    let category_table = crate::emacs_core::category::active_category_table_for_buffer(Some(buf))
        .ok()
        .filter(|table| !table.is_nil());
    BufferSyntaxLookup {
        syntax_table: crate::emacs_core::syntax::SyntaxTable::for_buffer(buf),
        category_table,
        word_boundary,
    }
}

/// Position-aware syntax lookup for regexp matching over an accessible buffer.
///
/// `BufferSyntaxLookup` itself remains position-independent: it is the base
/// table both objects share, since GNU's `RE_SETUP_SYNTAX_TABLE_FOR_OBJECT`
/// calls `SETUP_BUFFER_SYNTAX_TABLE` whatever the object is. This wrapper adds
/// the buffer's own positional properties; [`StringRegexpSyntaxLookup`] adds a
/// searched string's.
struct BufferRegexpSyntaxLookup<'a> {
    base: BufferSyntaxLookup,
    buffer: &'a Buffer,
    input_start: EmacsBytePos,
    /// One property-run cache for the whole match. GNU arms the same `gl_state`
    /// runs for a regexp over a buffer (`RE_SETUP_SYNTAX_TABLE_FOR_OBJECT`,
    /// src/syntax.c:277); without it every `\s` test did a fresh interval
    /// lookup and a byte->char conversion.
    property_lookup: crate::emacs_core::syntax::SyntaxPropByteRun<'a>,
    /// The lazy `syntax-propertize` frontier, when the caller propertizes on
    /// demand (see [`PropertizeFrontier`]).
    frontier: Option<PropertizeFrontier<'a>>,
}

/// GNU propertizes lazily: `UPDATE_SYNTAX_TABLE_FORWARD` inside the matcher's
/// syntax-reading ops calls `parse_sexp_propertize` the moment a read reaches
/// `syntax-propertize--done`. The Rust matcher cannot re-enter Lisp mid-match,
/// so a buffer lookup armed with a frontier instead RECORDS the first positional
/// syntax read at or past it; the builtin then propertizes that far and re-runs
/// the match. Only syntax-reading ops consult this (exactly the ops GNU's macro
/// guards), so a pattern that never reads syntax past the frontier never
/// propertizes — the `\s<\s<\s<` of `lisp-indent-line` reads three chars,
/// not the buffer tail.
#[derive(Clone, Copy)]
pub(crate) struct PropertizeFrontier<'a> {
    /// First absolute buffer byte that is NOT yet propertized.
    pub(crate) byte: EmacsBytePos,
    /// Lowest byte at/after `byte` the matcher read syntax for, if any.
    pub(crate) crossed: &'a std::cell::Cell<Option<EmacsBytePos>>,
}

impl PropertizeFrontier<'_> {
    #[inline]
    fn note_read(&self, abs: EmacsBytePos) {
        if abs >= self.byte && self.crossed.get().is_none_or(|seen| abs < seen) {
            self.crossed.set(Some(abs));
        }
    }
}

impl SyntaxLookup for BufferRegexpSyntaxLookup<'_> {
    fn char_syntax(&self, c: char) -> crate::emacs_core::syntax::SyntaxClass {
        self.base.char_syntax(c)
    }

    fn char_syntax_at(&self, c: char, input_pos: usize) -> crate::emacs_core::syntax::SyntaxClass {
        let abs = EmacsBytePos::new(self.input_start.get().saturating_add(input_pos));
        if let Some(frontier) = &self.frontier {
            frontier.note_read(abs);
        }
        crate::emacs_core::syntax::regexp_syntax_class_at_emacs_byte(
            self.buffer,
            &self.base.syntax_table,
            c,
            abs,
            &self.property_lookup,
        )
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        self.base.char_has_category(c, cat)
    }

    fn word_boundary_between(&self, c1: char, c2: char) -> bool {
        self.base.word_boundary_between(c1, c2)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        self.base.cache_key()
    }
}

/// Position-aware syntax lookup for a regexp over a searched STRING.
///
/// GNU sets up syntax state for the object being searched, not only for buffers
/// (`RE_SETUP_SYNTAX_TABLE_FOR_OBJECT`, src/syntax.c:277, armed from
/// `re_match_object` in src/search.c), so `string-match` over a propertized
/// string honours that string's own `syntax-table` property exactly as a buffer
/// search honours the buffer's. The base table still comes from the current
/// buffer, and so does the `parse-sexp-lookup-properties` gate that decided
/// whether `syntax_properties` is a resolver at all.
///
/// The matcher's `input_pos` is already a byte offset from the string's first
/// byte -- the string path passes the whole string to `re_search` -- so no
/// origin shift is needed here, unlike the buffer wrapper above.
pub(crate) struct StringRegexpSyntaxLookup<'a> {
    base: BufferSyntaxLookup,
    property_lookup: crate::emacs_core::syntax::StringSyntaxPropByteRun<'a>,
}

/// The syntax lookup a regexp over a searched STRING runs under.
///
/// The three variants are the three answers GNU's setup can produce, and
/// separating them is what keeps the common case free: only
/// [`Self::Propertized`] has a per-character property test, so a `string-match`
/// over a string with no `syntax-table` properties to read -- very nearly all
/// of them -- runs the identical position-free path it ran before strings
/// carried syntax at all.
pub(crate) enum StringSyntaxLookup<'a> {
    /// No current buffer to take a base table from: GNU's standard
    /// classification, as before.
    Standard(DefaultSyntaxLookup),
    /// The current buffer's table, with no positional property to read --
    /// either the string carries none, or `parse-sexp-lookup-properties` is
    /// nil.
    Table(BufferSyntaxLookup),
    /// The current buffer's table plus the string's own `syntax-table`
    /// properties.
    Propertized(StringRegexpSyntaxLookup<'a>),
}

impl<'a> StringSyntaxLookup<'a> {
    /// GNU `RE_SETUP_SYNTAX_TABLE_FOR_OBJECT` for a string object
    /// (src/syntax.c:277): `SETUP_BUFFER_SYNTAX_TABLE` for the base table
    /// whatever the object, and the object's own intervals for the positional
    /// property.
    pub(crate) fn new(
        base: Option<BufferSyntaxLookup>,
        string: &'a LispString,
        syntax_properties: crate::emacs_core::syntax::SyntaxProperties<'a>,
    ) -> Self {
        let Some(base) = base else {
            return Self::Standard(DefaultSyntaxLookup);
        };
        // Only a honouring scan looks at the intervals at all: the caller has
        // already established that a string with none cannot honour anything,
        // so the ignoring path here touches the string object not at all.
        let crate::emacs_core::syntax::SyntaxProperties::Honor(_) = syntax_properties else {
            return Self::Table(base);
        };
        let intervals = string.intervals();
        if intervals.is_empty() {
            return Self::Table(base);
        }
        Self::Propertized(StringRegexpSyntaxLookup {
            property_lookup: crate::emacs_core::syntax::StringSyntaxPropByteRun::new(
                syntax_properties,
                string,
                Some(intervals),
            ),
            base,
        })
    }

    pub(crate) fn as_lookup(&self) -> &dyn SyntaxLookup {
        match self {
            Self::Standard(lookup) => lookup,
            Self::Table(lookup) => lookup,
            Self::Propertized(lookup) => lookup,
        }
    }
}

impl SyntaxLookup for StringRegexpSyntaxLookup<'_> {
    fn char_syntax(&self, c: char) -> crate::emacs_core::syntax::SyntaxClass {
        self.base.char_syntax(c)
    }

    fn char_syntax_at(&self, c: char, input_pos: usize) -> crate::emacs_core::syntax::SyntaxClass {
        crate::emacs_core::syntax::regexp_syntax_class_at_string_byte(
            &self.base.syntax_table,
            c,
            input_pos,
            &self.property_lookup,
        )
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        self.base.char_has_category(c, cat)
    }

    fn word_boundary_between(&self, c1: char, c2: char) -> bool {
        self.base.word_boundary_between(c1, c2)
    }

    fn cache_key(&self) -> SyntaxCacheKey {
        self.base.cache_key()
    }
}

fn buffer_regexp_syntax_lookup<'a>(
    buf: &'a Buffer,
    input_start: EmacsBytePos,
    context: BufferRegexpMatchContext<'a>,
) -> BufferRegexpSyntaxLookup<'a> {
    BufferRegexpSyntaxLookup {
        base: buffer_syntax_lookup_with_word_boundary(buf, context.word_boundary),
        buffer: buf,
        input_start,
        property_lookup: crate::emacs_core::syntax::SyntaxPropByteRun::new(
            context.syntax_properties,
        ),
        frontier: context.frontier,
    }
}

/// Convert GNU engine registers into private zero-based Emacs-byte ranges.
///
/// This type cannot be stored in evaluator state.  Buffer and string search
/// paths must publish it into source-specific character coordinates first.
fn engine_match_data_from_registers(regs: &MatchRegisters, offset: usize) -> EngineMatchData {
    // Build the byte-range groups directly: routing through
    // `EngineMatchData::new` would re-map the whole Vec a second time, per
    // match, on the hottest search path.
    let num_groups = regs.num_regs();
    let mut groups =
        smallvec::SmallVec::<[Option<EmacsByteRange>; GNU_SEARCH_REGS_BASE_CAPACITY]>::new();
    groups.reserve(gnu_search_regs_capacity(num_groups));
    for i in 0..num_groups {
        if regs.start[i] >= 0 && regs.end[i] >= 0 {
            groups.push(Some(
                MatchGroup::new(
                    regs.start[i] as usize + offset,
                    regs.end[i] as usize + offset,
                )
                .emacs_byte_range(),
            ));
        } else {
            groups.push(None);
        }
    }
    groups.resize(gnu_search_regs_capacity(groups.len()), None);
    EngineMatchData { groups }
}

fn buffer_engine_match_data_from_registers(
    regs: &MatchRegisters,
    base_emacs_byte: usize,
) -> EngineMatchData {
    engine_match_data_from_registers(regs, base_emacs_byte)
}

fn gnu_search_regs_capacity(required: usize) -> usize {
    if required <= GNU_SEARCH_REGS_BASE_CAPACITY {
        GNU_SEARCH_REGS_BASE_CAPACITY
    } else {
        required.max(GNU_SEARCH_REGS_BASE_CAPACITY + (GNU_SEARCH_REGS_BASE_CAPACITY >> 1))
    }
}

/// Inline-capacity group storage sized to GNU's base `search_regs`
/// allocation: patterns with <= 7 groups (the overwhelming majority,
/// and every literal search) publish match data with ZERO heap
/// allocations. GNU reuses a global `search_regs` and never allocates
/// per match; two mallocs per match showed up on single-char
/// literal-search sweeps.
pub(crate) type MatchGroupVec =
    smallvec::SmallVec<[Option<MatchGroup>; GNU_SEARCH_REGS_BASE_CAPACITY]>;

fn extend_to_gnu_search_regs_capacity(groups: &mut MatchGroupVec) {
    groups.resize(gnu_search_regs_capacity(groups.len()), None);
}

fn gnu_single_group_vec(group: Option<MatchGroup>) -> MatchGroupVec {
    let mut groups = MatchGroupVec::new();
    groups.push(group);
    extend_to_gnu_search_regs_capacity(&mut groups);
    groups
}

#[derive(Clone)]
enum CompiledSearchPattern {
    /// GNU-translated engine (primary path for all patterns).
    Emacs(Rc<CompiledPattern>),
    /// Simple literal search. Holds the literal as Emacs internal-encoding bytes
    /// (issue #131: no storage round-trip).
    Literal(Vec<u8>),
}

/// Whether matching a compiled regexp depends on the current buffer's syntax.
///
/// Keep this as a semantic enum instead of leaking `CompiledPattern::uses_syntax`
/// to evaluator code.  A buffer-syntax-dependent pattern may need to call Lisp
/// to prepare `syntax-table` text properties before the matcher takes an
/// immutable buffer borrow; an independent pattern must not trigger that
/// observable work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumIs)]
pub(crate) enum BufferRegexpSyntaxDependency {
    SyntaxIndependent,
    BufferSyntaxDependent,
}

/// Whether a buffer regexp may read positional `syntax-table` properties.
///
/// A semantic enum keeps this evaluator decision distinct from regexp syntax
/// dependency and prevents boolean argument swaps at the search/matcher seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumIs)]
pub(crate) enum BufferRegexpSyntaxProperties {
    Ignore,
    Honor,
}

/// Match-time Lisp state consulted by a regexp over buffer text.
///
/// This state is deliberately separate from the compiled pattern: GNU reads
/// both positional syntax properties and word-boundary tables when matching,
/// so cached bytecode must remain reusable when either one changes.
///
/// The syntax properties are a resolver snapshot, not a flag: the matcher reads
/// each character's `syntax-table` property through the same GNU `textget`
/// fallbacks the sexp scanner and `get-char-property` use. It is taken AFTER
/// [`prepare_current_buffer_regexp_syntax_to`] has run `syntax-propertize`,
/// which is the last Lisp to run before the match.
#[derive(Clone, Copy)]
pub(crate) struct BufferRegexpMatchContext<'a> {
    syntax_properties: crate::emacs_core::syntax::SyntaxProperties<'a>,
    word_boundary: crate::emacs_core::regex_emacs::WordBoundaryLookup,
    frontier: Option<PropertizeFrontier<'a>>,
}

impl<'a> BufferRegexpMatchContext<'a> {
    pub(crate) fn new(
        syntax_properties: crate::emacs_core::syntax::SyntaxProperties<'a>,
        word_boundary: crate::emacs_core::regex_emacs::WordBoundaryLookup,
    ) -> Self {
        Self {
            syntax_properties,
            word_boundary,
            frontier: None,
        }
    }

    /// Arm the lazy-propertize frontier (see [`PropertizeFrontier`]).
    pub(crate) fn with_frontier(mut self, frontier: PropertizeFrontier<'a>) -> Self {
        self.frontier = Some(frontier);
        self
    }
}

/// Entry of [`SEARCH_PATTERN_CACHE`]:
/// `(posix, case_fold, pattern_multibyte, pattern_bytes, syntax_key, compiled)`.
type SearchPatternCacheEntry = (
    bool,
    bool,
    bool,
    Vec<u8>,
    Option<SyntaxCacheKey>,
    CompiledSearchPattern,
);

/// Entry of [`LISP_REGEX_PATTERN_CACHE`]:
/// `(posix, case_fold, translation_key, pattern_multibyte, target_multibyte,
///   pattern_bytes, syntax_key, compiled)`.
struct LispRegexPatternCacheEntry {
    posix: bool,
    case_fold: bool,
    translation_key: Option<usize>,
    pattern_multibyte: bool,
    target_multibyte: bool,
    pattern: Vec<u8>,
    syntax_key: Option<SyntaxCacheKey>,
    compiled: Rc<CompiledPattern>,
}

/// Does a cached entry compiled under `stored` serve a request running
/// under `current`?
///
/// Mirrors GNU `compile_pattern`'s probe (search.c:222-224):
/// `EQ (cp->syntax_table, Qt) || EQ (cp->syntax_table, BVAR
/// (current_buffer, syntax_table))` — `None` is GNU's `Qt`
/// (table-independent pattern), and the epoch inside
/// [`SyntaxCacheKey::Table`] stands in for GNU's `clear_regexp_cache`
/// on `modify-syntax-entry`.
fn syntax_key_matches(stored: Option<SyntaxCacheKey>, current: SyntaxCacheKey) -> bool {
    match stored {
        None => true,
        Some(key) => key == current,
    }
}

thread_local! {
    // GNU `src/search.c:61` (`searchbuf_head`) uses a `regexp_cache`
    // record keyed on:
    //
    //   - the pattern Lisp string,
    //   - the buffer's syntax table for `used_syntax` patterns (the
    //     `[[:word:]]` / `[[:space:]]` classes bake table content into
    //     the compiled artifacts — for neomacs, into the fastmap),
    //   - the `whitespace-regexp` transform flag,
    //   - the `posix` flag,
    //   - the `multibyte` flag,
    //   - the translate table identity.
    //
    // Neomacs tracks (posix, case_fold[/translate identity], pattern
    // multibyteness, pattern bytes, syntax-table key). Remaining
    // intentional gaps vs GNU:
    //
    //   - We don't expose `whitespace-regexp`.
    //   - `charset_unibyte` has no neomacs analog (UTF-8 internal).
    static SEARCH_PATTERN_CACHE: RefCell<Vec<SearchPatternCacheEntry>> =
        const { RefCell::new(Vec::new()) };

    // Boxed so the move-to-front on a hit (GNU `compile_pattern` relinks a
    // list pointer) rotates 8-byte pointers, not ~80-byte entries; the hot
    // ~20 font-lock patterns then stay in the first slots and a scan is short.
    static LISP_REGEX_PATTERN_CACHE: RefCell<Vec<Box<LispRegexPatternCacheEntry>>> =
        const { RefCell::new(Vec::new()) };
}

// ---------------------------------------------------------------------------
// MatchData
// ---------------------------------------------------------------------------

/// Match data from the last successful search.
#[derive(Clone, Debug)]
pub struct MatchData {
    kind: MatchDataKind,
    /// Debug-only stats: bit N set once group N returned Some to a reader.
    /// Sizes the DISTINCT conversion demand for the lazy-match-data
    /// question (see `match_stats`); absent from optimized builds.
    #[cfg(debug_assertions)]
    read_mask: std::cell::Cell<u64>,
}

/// Lisp-visible provenance of published match positions.
///
/// Keep buffer identity attached to the buffer variant so callers cannot
/// represent the impossible combination "buffer match with no buffer ID" by
/// pairing a boolean with an `Option<BufferId>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumIs)]
pub(crate) enum MatchDataSource {
    String,
    Buffer(BufferId),
}

/// Published match provenance and coordinates.
///
/// Raw regexp-engine byte registers deliberately have no variant here: every
/// value that can enter evaluator state is already in Lisp-visible character
/// coordinates for its source.
#[derive(Clone, Debug)]
enum MatchDataKind {
    /// GNU uses `last_thing_searched = Qt` for string match data.  A searched
    /// string object is available after `string-match`, but not after
    /// `set-match-data` restores an integer list saved by `match-data`.
    StringChars {
        groups: smallvec::SmallVec<[Option<CharRange>; GNU_SEARCH_REGS_BASE_CAPACITY]>,
        searched: Option<SearchedString>,
    },
    /// Buffer identity and published Lisp-coordinate payload travel together.
    Buffer {
        id: BufferId,
        groups: smallvec::SmallVec<[Option<LispCharMatchRange>; GNU_SEARCH_REGS_BASE_CAPACITY]>,
    },
}

/// Private, zero-based Emacs-byte ranges returned by the regexp engine.
///
/// Keeping this outside `MatchDataKind` makes it impossible to publish raw
/// registers accidentally.  It is consumed by a source-specific conversion
/// before a search success crosses the regex module's interface.
#[derive(Clone, Debug)]
struct EngineMatchData {
    groups: smallvec::SmallVec<[Option<EmacsByteRange>; GNU_SEARCH_REGS_BASE_CAPACITY]>,
}

/// A successful buffer search ready to commit to evaluator state.
///
/// Construction consumes private engine registers and publishes them against
/// the still-unchanged buffer, so callers can move point and store match data
/// without knowing any coordinate-conversion rules.
#[derive(Clone, Debug)]
pub(crate) struct BufferSearchSuccess {
    point: EmacsBytePos,
    match_data: MatchData,
}

/// A successful string search ready for optional publication to evaluator
/// state. Predicate callers can consume only the start position and discard
/// the already-published match data without constructing a throwaway slot.
#[derive(Clone, Debug)]
pub(crate) struct StringSearchSuccess {
    start: CharPos0,
    match_data: MatchData,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LispCharMatchRange {
    start: LispMatchPosition,
    end: LispMatchPosition,
}

/// A character position as stored in Lisp-visible match registers.
///
/// Successful buffer searches produce ordinary 1-based buffer positions, but
/// `match-data--translate` may subsequently move them to zero.  Keeping this
/// distinct from both `LispCharPos1` and zero-based string `CharPos0` prevents
/// either coordinate convention from silently clamping or reinterpreting it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LispMatchPosition(usize);

impl LispMatchPosition {
    const fn new(position: usize) -> Self {
        Self(position)
    }

    const fn get(self) -> usize {
        self.0
    }

    fn from_buffer_position(position: LispCharPos1) -> Self {
        Self(
            usize::try_from(position.as_i64())
                .expect("a buffer match position is nonnegative and fits usize"),
        )
    }

    const fn zero_based(self) -> CharPos0 {
        CharPos0::new(self.0.saturating_sub(1))
    }
}

impl LispCharMatchRange {
    fn from_match_group(group: MatchGroup) -> Self {
        Self {
            start: LispMatchPosition::new(group.start()),
            end: LispMatchPosition::new(group.end()),
        }
    }

    fn into_match_group(self) -> MatchGroup {
        MatchGroup::new(self.start.get(), self.end.get())
    }

    const fn zero_based(self) -> CharRange {
        CharRange::new(self.start.zero_based(), self.end.zero_based())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchGroup {
    start: usize,
    end: usize,
}

impl MatchGroup {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn start(self) -> usize {
        self.start
    }

    pub const fn end(self) -> usize {
        self.end
    }

    pub const fn string_char_range(self) -> CharRange {
        CharRange::new(CharPos0::new(self.start), CharPos0::new(self.end))
    }

    pub const fn emacs_byte_range(self) -> EmacsByteRange {
        EmacsByteRange::new(EmacsBytePos::new(self.start), EmacsBytePos::new(self.end))
    }

    pub const fn from_char_range(range: CharRange) -> Self {
        Self::new(range.start().get(), range.end().get())
    }

    pub const fn from_emacs_byte_range(range: EmacsByteRange) -> Self {
        Self::new(range.start().get(), range.end().get())
    }

    pub fn shift(self, delta: usize) -> Self {
        Self::new(self.start + delta, self.end + delta)
    }

    pub fn saturating_sub(self, delta: usize) -> Self {
        Self::new(
            self.start.saturating_sub(delta),
            self.end.saturating_sub(delta),
        )
    }

    pub fn translate_saturating(self, delta: i64) -> Self {
        if delta >= 0 {
            let delta = delta as usize;
            Self::new(
                self.start.saturating_add(delta),
                self.end.saturating_add(delta),
            )
        } else {
            let delta = (-delta) as usize;
            Self::new(
                self.start.saturating_sub(delta),
                self.end.saturating_sub(delta),
            )
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchedString {
    Heap(super::value::Value),
    Owned(LispString),
}

impl SearchedString {
    pub(crate) fn as_lisp_string(&self) -> Option<&LispString> {
        match self {
            Self::Heap(val) => val.as_lisp_string(),
            Self::Owned(text) => Some(text),
        }
    }

    fn byte_to_char_pos(&self, byte_pos: usize) -> usize {
        let Some(string) = self.as_lisp_string() else {
            return 0;
        };
        string.byte_to_char_pos(byte_pos)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn to_owned(&self) -> String {
        let Some(string) = self.as_lisp_string() else {
            return String::new();
        };
        string
            .as_utf8_str()
            .map(str::to_owned)
            .unwrap_or_else(|| String::from_utf8_lossy(string.as_bytes()).into_owned())
    }
}

pub fn char_pos_to_byte_lisp_string(s: &crate::heap_types::LispString, char_pos: usize) -> usize {
    s.char_to_byte_pos(char_pos)
}

impl MatchData {
    pub(crate) fn string(
        groups: Vec<Option<MatchGroup>>,
        searched: Option<SearchedString>,
    ) -> Self {
        Self {
            kind: MatchDataKind::StringChars {
                groups: groups
                    .into_iter()
                    .map(|group| group.map(MatchGroup::string_char_range))
                    .collect(),
                searched,
            },
            #[cfg(debug_assertions)]
            read_mask: Default::default(),
        }
    }

    pub(crate) fn buffer_lisp_chars(groups: Vec<Option<MatchGroup>>, buffer_id: BufferId) -> Self {
        Self {
            kind: MatchDataKind::Buffer {
                id: buffer_id,
                groups: groups
                    .into_iter()
                    .map(|group| group.map(LispCharMatchRange::from_match_group))
                    .collect(),
            },
            #[cfg(debug_assertions)]
            read_mask: Default::default(),
        }
    }

    pub(crate) fn searched_string(&self) -> Option<&SearchedString> {
        match &self.kind {
            MatchDataKind::StringChars { searched, .. } => searched.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn source(&self) -> MatchDataSource {
        match self.kind {
            MatchDataKind::StringChars { .. } => MatchDataSource::String,
            MatchDataKind::Buffer { id, .. } => MatchDataSource::Buffer(id),
        }
    }

    pub(crate) fn group_count(&self) -> usize {
        match &self.kind {
            MatchDataKind::StringChars { groups, .. } => groups.len(),
            MatchDataKind::Buffer { groups, .. } => groups.len(),
        }
    }

    pub(crate) fn group(&self, index: usize) -> Option<MatchGroup> {
        #[cfg(debug_assertions)]
        match_stats::count_group_read(index);
        let result = match &self.kind {
            MatchDataKind::StringChars { groups, .. } => groups
                .get(index)
                .copied()
                .flatten()
                .map(MatchGroup::from_char_range),
            MatchDataKind::Buffer { groups, .. } => groups
                .get(index)
                .copied()
                .flatten()
                .map(LispCharMatchRange::into_match_group),
        };
        #[cfg(debug_assertions)]
        if result.is_some() {
            match_stats::count_first_some_read(&self.read_mask, index);
        }
        result
    }

    /// Return a group's endpoints as zero-based character offsets into its
    /// source text, regardless of the Lisp-visible coordinate convention.
    ///
    /// String registers are already zero-based. Buffer registers are stored as
    /// one-based Lisp positions and are converted here, keeping that convention
    /// out of replacement and extraction code.
    pub(crate) fn group_zero_based_char_range(&self, index: usize) -> Option<CharRange> {
        match &self.kind {
            MatchDataKind::StringChars { groups, .. } => groups.get(index).copied().flatten(),
            MatchDataKind::Buffer { groups, .. } => groups
                .get(index)
                .copied()
                .flatten()
                .map(|range| range.zero_based()),
        }
    }

    pub(crate) fn groups_snapshot(&self) -> Vec<Option<MatchGroup>> {
        (0..self.group_count())
            .map(|index| self.group(index))
            .collect()
    }

    pub(crate) fn translate_positions(&mut self, delta: i64) {
        let map = |group: MatchGroup| group.translate_saturating(delta);
        match &mut self.kind {
            MatchDataKind::StringChars { groups, .. } => {
                for range in groups.iter_mut().flatten() {
                    *range = map(MatchGroup::from_char_range(*range)).string_char_range();
                }
            }
            MatchDataKind::Buffer { groups, .. } => {
                for range in groups.iter_mut().flatten() {
                    *range = LispCharMatchRange::from_match_group(map(range.into_match_group()));
                }
            }
        }
    }

    /// Rewrite every group through MAP, in the same Lisp-visible coordinates
    /// `match-beginning' reports.
    ///
    /// Both origins are mapped.  `replace-match' adjusts the registers after
    /// editing (GNU's `update_search_regs', search.c) no matter what the match
    /// data records as its origin, because with STRING nil the registers name
    /// buffer positions however they were installed -- `set-match-data' from
    /// plain integers records string-sourced data that still describes the
    /// buffer.  Skipping the adjustment there leaves a caller that replaces in
    /// a loop reading the same stale registers forever.
    pub(crate) fn map_lisp_positions(&mut self, mut map: impl FnMut(MatchGroup) -> MatchGroup) {
        match &mut self.kind {
            MatchDataKind::StringChars { groups, .. } => {
                for range in groups.iter_mut().flatten() {
                    *range = map(MatchGroup::from_char_range(*range)).string_char_range();
                }
            }
            MatchDataKind::Buffer { groups, .. } => {
                for range in groups.iter_mut().flatten() {
                    *range = LispCharMatchRange::from_match_group(map(range.into_match_group()));
                }
            }
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn searched_string_text(&self) -> Option<String> {
        self.searched_string().map(SearchedString::to_owned)
    }
}

impl EngineMatchData {
    fn new(groups: MatchGroupVec) -> Self {
        Self {
            groups: groups
                .into_iter()
                .map(|group| group.map(MatchGroup::emacs_byte_range))
                .collect(),
        }
    }

    fn group(&self, index: usize) -> Option<MatchGroup> {
        self.groups
            .get(index)
            .copied()
            .flatten()
            .map(MatchGroup::from_emacs_byte_range)
    }

    fn publish_string(self, searched_string: SearchedString) -> MatchData {
        crate::emacs_core::perf_trace::time_op(
            crate::emacs_core::perf_trace::HotpathOp::RegexMatchDataChars,
            || {
                // Resolve the searched string ONCE: `SearchedString::
                // byte_to_char_pos` re-decodes the Value per endpoint, and for
                // unibyte/all-ASCII strings byte == char so the whole
                // conversion is the identity.
                let string = searched_string.as_lisp_string();
                let identity = string
                    .map(|s| !s.is_multibyte() || s.schars() == s.sbytes())
                    .unwrap_or(true);
                let char_groups = if identity {
                    self.groups
                        .into_iter()
                        .map(|range| range.map(MatchGroup::from_emacs_byte_range))
                        .collect()
                } else {
                    let string = string.expect("non-identity conversion has a string");
                    self.groups
                        .into_iter()
                        .map(|range| {
                            range.map(|range| {
                                MatchGroup::new(
                                    string.byte_to_char_pos(range.start().get()),
                                    string.byte_to_char_pos(range.end().get()),
                                )
                            })
                        })
                        .collect()
                };
                MatchData::string(char_groups, Some(searched_string))
            },
        )
    }

    fn publish_buffer(self, buf: &Buffer) -> MatchData {
        #[cfg(debug_assertions)]
        match_stats::count_publish(&self.groups);
        let groups = self
            .groups
            .into_iter()
            .map(|range| {
                range.map(|range| LispCharMatchRange {
                    start: LispMatchPosition::from_buffer_position(
                        buf.emacs_byte_pos_to_lisp_char_pos(range.start()),
                    ),
                    end: LispMatchPosition::from_buffer_position(
                        buf.emacs_byte_pos_to_lisp_char_pos(range.end()),
                    ),
                })
            })
            .collect();
        MatchData {
            kind: MatchDataKind::Buffer { id: buf.id, groups },
            #[cfg(debug_assertions)]
            read_mask: Default::default(),
        }
    }
}

impl BufferSearchSuccess {
    fn new(buf: &Buffer, point: EmacsBytePos, engine_match: EngineMatchData) -> Self {
        Self {
            point,
            match_data: engine_match.publish_buffer(buf),
        }
    }

    pub(crate) fn into_parts(self) -> (BufferId, EmacsBytePos, MatchData) {
        let MatchDataSource::Buffer(buffer_id) = self.match_data.source() else {
            unreachable!("buffer search success always carries buffer match data");
        };
        (buffer_id, self.point, self.match_data)
    }
}

impl StringSearchSuccess {
    fn new(match_data: MatchData) -> Self {
        let start = match_data
            .group_zero_based_char_range(0)
            .expect("successful string search has group zero")
            .start();
        Self { start, match_data }
    }

    pub(crate) fn into_parts(self) -> (CharPos0, MatchData) {
        (self.start, self.match_data)
    }
}

// ---------------------------------------------------------------------------
// Emacs → Rust regex translation
// ---------------------------------------------------------------------------

/// Translate basic Emacs regex syntax to Rust regex syntax.
///
/// Key differences handled:
/// - Emacs `\(` `\)` for groups  →  Rust `(` `)`
/// - Emacs `\|` for alternation  →  Rust `|`
/// - Emacs `\{` `\}` for repetition  →  Rust `{` `}`
/// - Emacs `\1`..`\9` for back-references  →  not supported by `regex` crate,
///   but we translate the syntax anyway for completeness
/// - Emacs literal `(` `)` `{` `}` `|`  →  Rust `\(` `\)` `\{` `\}` `\|`
/// - Emacs `\w` (word char)  →  Rust `\w`
/// - Emacs `\W` (non-word char)  →  Rust `\W`
/// - Emacs `\b` (word boundary)  →  Rust `\b`
/// - Emacs `\B` (non-word boundary)  →  Rust `\B`
/// - Emacs `\s-` etc. (syntax classes)  →  simplified to `\s` (whitespace)
/// - Emacs `\<` `\>` (word boundaries)  →  Rust `\b`
/// - Emacs character classes inside `[...]` are kept as-is.
pub fn translate_emacs_regex(pattern: &str) -> String {
    fn next_char_at(s: &str, byte_idx: usize) -> Option<(char, usize)> {
        s.get(byte_idx..)
            .and_then(|tail| tail.chars().next().map(|ch| (ch, ch.len_utf8())))
    }

    fn push_rust_class_char(out: &mut String, ch: char) {
        match ch {
            '\\' => out.push_str("\\\\"),
            '[' => out.push_str("\\["),
            _ => out.push(ch),
        }
    }

    let mut out = String::with_capacity(pattern.len() + 8);
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut in_bracket = false;
    let mut bracket_negated = false;
    // Position in `out` where bracket content starts (after `[` / `[^`).
    // Used to detect empty classes after removing reversed ranges.
    let mut bracket_content_start: usize = 0;

    while i < len {
        let (ch, ch_len) = next_char_at(pattern, i).expect("byte index must be char boundary");

        // Non-ASCII literal bytes should be preserved as full UTF-8 scalar values.
        if !ch.is_ascii() {
            out.push(ch);
            i += ch_len;
            continue;
        }

        // Inside a character class [...], handle Emacs/Rust differences:
        //  - `\` is literal in Emacs and can still participate in ranges
        //  - Reversed ranges like `z-a` are empty in Emacs but error in Rust → remove
        //  - `]` at first position is literal in Emacs → escape it for Rust
        if in_bracket {
            if ch == ']' {
                in_bracket = false;
                if out.len() == bracket_content_start {
                    // Bracket has no content (all ranges were reversed/removed).
                    // [^] → matches anything, [] → matches nothing.
                    // Truncate the opening `[` or `[^` and emit a replacement.
                    let open_len = if bracket_negated { 2 } else { 1 };
                    out.truncate(bracket_content_start - open_len);
                    if bracket_negated {
                        out.push_str("[\\s\\S]");
                    } else {
                        // Empty positive class — can never match.
                        // Use a character class that accepts no character to
                        // avoid unsupported look-around constructs.
                        out.push_str("[^\\s\\S]");
                    }
                } else {
                    out.push(']');
                }
                i += 1;
                continue;
            }
            if ch == '\\' {
                if i + 1 < len && bytes[i + 1] == b']' {
                    // GNU Emacs does not treat \] inside [...] as a literal ].
                    // Keep the backslash as a literal class member and let the
                    // following ] close the class on the next iteration.
                    push_rust_class_char(&mut out, ch);
                    i += 1;
                    continue;
                }
                if i + 2 < len && bytes[i + 1] == b'-' && bytes[i + 2] != b']' {
                    let (end_ch, end_len) =
                        next_char_at(pattern, i + 2).expect("byte index must be char boundary");
                    if ch > end_ch {
                        // GNU Emacs treats `\-x` like a range from `\` to `x`.
                        // If the range is reversed, it is empty.
                        i += 1 + 1 + end_len;
                        continue;
                    }
                    push_rust_class_char(&mut out, ch);
                    out.push('-');
                    push_rust_class_char(&mut out, end_ch);
                    i += 1 + 1 + end_len;
                } else {
                    push_rust_class_char(&mut out, ch);
                    i += 1;
                }
                continue;
            }
            if ch == '[' {
                // In Emacs, `[` inside [...] is literal.  In Rust regex
                // it starts a nested character class.  Escape it.
                // Exception: POSIX classes like [:alpha:] — pass through.
                if i + 1 < len && bytes[i + 1] == b':' {
                    // Looks like a POSIX class `[:...:` — pass through.
                    out.push('[');
                } else {
                    out.push_str("\\[");
                }
                i += 1;
                continue;
            }
            // Check for ranges: if next is `-` and then a non-`]` char,
            // validate the range direction.
            if i + 2 < len && bytes[i + 1] == b'-' && bytes[i + 2] != b']' {
                let (end_ch, end_len) =
                    next_char_at(pattern, i + 2).expect("byte index must be char boundary");
                if ch > end_ch {
                    // Reversed range (e.g. z-a): empty in Emacs, skip entirely.
                    i += 1 + 1 + end_len;
                    continue;
                }
            }
            out.push(ch);
            i += ch_len;
            continue;
        }

        match ch {
            '[' => {
                in_bracket = true;
                bracket_negated = false;
                out.push('[');
                i += 1;
                // Handle `[^` — consume the negation prefix.
                if i < len && bytes[i] == b'^' {
                    out.push('^');
                    bracket_negated = true;
                    i += 1;
                }
                bracket_content_start = out.len();
                // `]` as first char (or first after `^`) is literal in Emacs.
                // In Rust regex it would close the class.  Escape it.
                if i < len && bytes[i] == b']' {
                    out.push_str("\\]");
                    i += 1;
                }
            }
            // Emacs uses literal `(`, `)`, `{`, `}`, `|` — escape them for Rust regex.
            '(' => {
                out.push_str("\\(");
                i += 1;
            }
            ')' => {
                out.push_str("\\)");
                i += 1;
            }
            '{' => {
                out.push_str("\\{");
                i += 1;
            }
            '}' => {
                out.push_str("\\}");
                i += 1;
            }
            '|' => {
                out.push_str("\\|");
                i += 1;
            }
            '\\' if i + 1 < len => {
                let (next, next_len) =
                    next_char_at(pattern, i + 1).expect("byte index must be char boundary");
                match next {
                    // Emacs group → Rust group
                    '(' => {
                        let group_idx = i + 1 + next_len;
                        if group_idx < len && bytes[group_idx] == b'?' {
                            if group_idx + 1 < len && bytes[group_idx + 1] == b':' {
                                out.push_str("(?:");
                                i = group_idx + 2;
                                continue;
                            }

                            let digits_start = group_idx + 1;
                            let mut digits_end = digits_start;
                            while digits_end < len && bytes[digits_end].is_ascii_digit() {
                                digits_end += 1;
                            }
                            if digits_end > digits_start
                                && digits_end < len
                                && bytes[digits_end] == b':'
                            {
                                out.push('(');
                                i = digits_end + 1;
                                continue;
                            }
                        }

                        out.push('(');
                        i += 1 + next_len;
                    }
                    ')' => {
                        out.push(')');
                        i += 1 + next_len;
                    }
                    // Emacs alternation → Rust alternation
                    '|' => {
                        out.push('|');
                        i += 1 + next_len;
                    }
                    // Emacs repetition braces → Rust repetition braces
                    '{' => {
                        let interval_start = i + 1 + next_len;
                        let mut scan = interval_start;
                        let mut closed_interval = false;
                        while scan < len {
                            if bytes[scan] == b'\\' && scan + 1 < len && bytes[scan + 1] == b'}' {
                                let interval = &pattern[interval_start..scan];
                                out.push('{');
                                if let Some(rest) = interval.strip_prefix(',') {
                                    out.push('0');
                                    out.push(',');
                                    out.push_str(rest);
                                } else {
                                    out.push_str(interval);
                                }
                                out.push('}');
                                i = scan + 2;
                                closed_interval = true;
                                break;
                            }
                            scan += 1;
                        }
                        if closed_interval {
                            continue;
                        }
                        out.push('{');
                        i += 1 + next_len;
                    }
                    '}' => {
                        out.push('}');
                        i += 1 + next_len;
                    }
                    // GNU regex.c: \` matches beginning of string (like \A in PCRE)
                    '`' => {
                        out.push_str("\\A");
                        i += 1 + next_len;
                    }
                    // GNU regex.c: \' matches end of string (like \z in PCRE)
                    '\'' => {
                        out.push_str("\\z");
                        i += 1 + next_len;
                    }
                    // Word boundaries
                    '<' => {
                        out.push_str("\\b");
                        i += 1 + next_len;
                    }
                    '>' => {
                        out.push_str("\\b");
                        i += 1 + next_len;
                    }
                    '_' => {
                        i += 1 + next_len;
                        if i < len {
                            let (boundary_ch, boundary_len) =
                                next_char_at(pattern, i).expect("byte index must be char boundary");
                            match boundary_ch {
                                '<' | '>' => {
                                    i += boundary_len;
                                    out.push_str("\\b");
                                }
                                _ => {
                                    out.push('_');
                                }
                            }
                        } else {
                            out.push('_');
                        }
                    }
                    // Back-references (1-9) — not supported by `regex` crate,
                    // but translate the syntax for pattern acceptance.
                    '1'..='9' => {
                        // Rust `regex` doesn't support back-refs; drop silently.
                        // In practice, patterns using \1..\9 will fail to compile
                        // which is acceptable for now.
                        out.push('\\');
                        out.push(next);
                        i += 1 + next_len;
                    }
                    // Emacs syntax classes (\s-, \sw, etc.)
                    // Map to the closest Rust regex equivalents.
                    's' => {
                        i += 1 + next_len;
                        // Consume the syntax-class character and map appropriately
                        if i < len {
                            let (class_ch, class_len) =
                                next_char_at(pattern, i).expect("byte index must be char boundary");
                            match class_ch {
                                '-' | ' ' => {
                                    // \s- or \s  → whitespace
                                    i += class_len;
                                    out.push_str("\\s");
                                }
                                'w' => {
                                    // \sw → word constituent
                                    i += class_len;
                                    out.push_str("\\w");
                                }
                                '_' => {
                                    // \s_ → symbol constituent (word + underscore)
                                    i += class_len;
                                    out.push_str("[\\w_]");
                                }
                                '.' => {
                                    // \s. → punctuation
                                    i += class_len;
                                    out.push_str("[[:punct:]]");
                                }
                                '(' => {
                                    // \s( → open delimiter
                                    i += class_len;
                                    out.push_str("[\\[\\(\\{]");
                                }
                                ')' => {
                                    // \s) → close delimiter
                                    i += class_len;
                                    out.push_str("[\\]\\)\\}]");
                                }
                                '"' => {
                                    // \s" → string quote character
                                    i += class_len;
                                    out.push_str("[\"']");
                                }
                                '\'' | '<' | '>' | '!' | '|' | '/' => {
                                    // Other syntax classes — approximate as whitespace
                                    i += class_len;
                                    out.push_str("\\s");
                                }
                                _ => {
                                    // No valid syntax-class char follows; treat as bare \s
                                    out.push_str("\\s");
                                }
                            }
                        } else {
                            out.push_str("\\s");
                        }
                    }
                    'S' => {
                        i += 1 + next_len;
                        // Consume the syntax-class character and map appropriately
                        if i < len {
                            let (class_ch, class_len) =
                                next_char_at(pattern, i).expect("byte index must be char boundary");
                            match class_ch {
                                '-' | ' ' => {
                                    // \S- or \S  → non-whitespace
                                    i += class_len;
                                    out.push_str("\\S");
                                }
                                'w' => {
                                    // \Sw → non-word constituent
                                    i += class_len;
                                    out.push_str("\\W");
                                }
                                '_' => {
                                    // \S_ → non-symbol constituent
                                    i += class_len;
                                    out.push_str("[^\\w_]");
                                }
                                '.' => {
                                    // \S. → non-punctuation
                                    i += class_len;
                                    out.push_str("[^[:punct:]]");
                                }
                                '(' => {
                                    // \S( → non-open-delimiter
                                    i += class_len;
                                    out.push_str("[^\\[\\(\\{]");
                                }
                                ')' => {
                                    // \S) → non-close-delimiter
                                    i += class_len;
                                    out.push_str("[^\\]\\)\\}]");
                                }
                                '"' => {
                                    // \S" → non-string-quote
                                    i += class_len;
                                    out.push_str("[^\"']");
                                }
                                '\'' | '<' | '>' | '!' | '|' | '/' => {
                                    // Other syntax classes — approximate as non-whitespace
                                    i += class_len;
                                    out.push_str("\\S");
                                }
                                _ => {
                                    // No valid syntax-class char follows; treat as bare \S
                                    out.push_str("\\S");
                                }
                            }
                        } else {
                            out.push_str("\\S");
                        }
                    }
                    'c' => {
                        i += 1 + next_len;
                        if i < len {
                            let (_, class_len) =
                                next_char_at(pattern, i).expect("byte index must be char boundary");
                            i += class_len;
                        }
                        // GNU Emacs category regexps are implemented in C and depend on
                        // the active category table. Rust's `regex` backend has no
                        // equivalent dynamic character-category predicate, so approximate
                        // category escapes as non-ASCII until the native engine is ported.
                        out.push_str("[^\\x00-\\x7F]");
                    }
                    // \= (match at point) → \A (match at start of search region)
                    '=' => {
                        out.push_str("\\A");
                        i += 1 + next_len;
                    }
                    // Known escape sequences — pass through
                    'w' | 'W' | 'b' | 'B' | 'd' | 'D' | 'n' | 't' | 'r' => {
                        out.push('\\');
                        out.push(next);
                        i += 1 + next_len;
                    }
                    // Literal backslash
                    '\\' => {
                        out.push_str("\\\\");
                        i += 1 + next_len;
                    }
                    // Anything else after `\` — pass through the escape
                    _ => {
                        if next.is_ascii() {
                            out.push('\\');
                        }
                        out.push(next);
                        i += 1 + next_len;
                    }
                }
            }
            // Lone trailing backslash — pass through
            '\\' => {
                out.push('\\');
                i += 1;
            }
            // All other chars — pass through as-is
            _ => {
                out.push(ch);
                i += 1;
            }
        }
    }

    out
}

fn trivial_regexp_p(pattern: &[u8]) -> bool {
    // Issue #131: pattern is Emacs internal-encoding bytes; every regex
    // metacharacter is ASCII (< 0x80) and UTF-8 / eight-bit bytes are >= 0x80,
    // so scanning raw bytes never false-matches a metacharacter.
    let mut i = 0;
    while i < pattern.len() {
        match pattern[i] {
            b'.' | b'*' | b'+' | b'?' | b'[' | b'^' | b'$' => return false,
            b'\\' => {
                i += 1;
                let Some(&next) = pattern.get(i) else {
                    return false;
                };
                match next {
                    b'|' | b'(' | b')' | b'`' | b'\'' | b'b' | b'B' | b'<' | b'>' | b'w' | b'W'
                    | b's' | b'S' | b'=' | b'{' | b'}' | b'_' | b'c' | b'C' | b'1' | b'2'
                    | b'3' | b'4' | b'5' | b'6' | b'7' | b'8' | b'9' | b'n' | b't' | b'r' => {
                        return false;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        i += 1;
    }
    true
}

fn literal_from_trivial_regexp(pattern: &[u8]) -> Option<Vec<u8>> {
    if !trivial_regexp_p(pattern) {
        return None;
    }

    let mut out = Vec::with_capacity(pattern.len());
    let mut pos = 0;
    while pos < pattern.len() {
        let (code, len) = crate::emacs_core::emacs_char::string_char(&pattern[pos..]);
        if code == '\\' as u32 {
            pos += len;
            if pos >= pattern.len() {
                return None;
            }
            let (_next, next_len) = crate::emacs_core::emacs_char::string_char(&pattern[pos..]);
            out.extend_from_slice(&pattern[pos..pos + next_len]);
            pos += next_len;
        } else {
            out.extend_from_slice(&pattern[pos..pos + len]);
            pos += len;
        }
    }
    Some(out)
}

/// Issue #131: build the multibyte `LispString` to compile a search pattern from
/// (GNU/the old `from_utf8` path always compiled multibyte; a unibyte pattern
/// promotes its raw bytes to eight-bit characters).
#[allow(dead_code)] // crate-private raw search seam exercised by unit tests
fn pattern_for_compile(pattern: &LispString) -> LispString {
    if pattern.is_multibyte() {
        LispString::from_emacs_bytes(pattern.as_bytes().to_vec())
    } else {
        LispString::from_emacs_bytes(crate::emacs_core::emacs_char::str_to_multibyte(
            pattern.as_bytes(),
        ))
    }
}

fn compile_search_pattern(
    pattern: &LispString,
    case_fold: bool,
) -> Result<CompiledSearchPattern, String> {
    compile_search_pattern_with_posix(pattern, case_fold, false, &DefaultSyntaxLookup)
}

/// Compile PATTERN for a `posix-*` search builtin.
///
/// GNU's `posix-looking-at`, `posix-search-forward`, `posix-search-backward`,
/// and `posix-string-match` all pass `posix=1` to the underlying
/// `looking_at_1` / `search_buffer` / `string_match_1` helpers
/// (see GNU `src/search.c:Fposix_looking_at` etc.). That flag is then
/// threaded through `compile_pattern` into `regex_compile` and
/// ultimately into `re_match_2_internal`, where the POSIX longest-
/// match algorithm (regex-emacs.c:4143-4344, 5278) kicks in.
///
/// Neomacs's `compile_search_pattern` used to hardcode `posix=false`
/// on the call to `regex_emacs::regex_compile`, which is audit
/// finding #2 in `drafts/regex-search-audit.md`. Callers that want
/// POSIX semantics must go through this helper. The pattern cache is
/// keyed on `(posix, case_fold, pattern)` so a non-POSIX entry never
/// satisfies a POSIX request or vice versa.
fn compile_search_pattern_with_posix(
    pattern: &LispString,
    case_fold: bool,
    posix: bool,
    syntax: &dyn SyntaxLookup,
) -> Result<CompiledSearchPattern, String> {
    let syntax_key = syntax.cache_key();
    if let Some(cached) = crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::RegexCompileHit,
        || {
            SEARCH_PATTERN_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                let index = cache.iter().position(
                    |(
                        cached_posix,
                        cached_case_fold,
                        cached_multibyte,
                        cached_pattern,
                        cached_syntax_key,
                        _,
                    )| {
                        *cached_posix == posix
                            && *cached_case_fold == case_fold
                            && *cached_multibyte == pattern.is_multibyte()
                            && cached_pattern.as_slice() == pattern.as_bytes()
                            && syntax_key_matches(*cached_syntax_key, syntax_key)
                    },
                )?;
                // Same MRU relink as the Lisp-pattern cache above.
                cache[..=index].rotate_right(1);
                Some(cache[0].5.clone())
            })
        },
    ) {
        return Ok(cached);
    }

    let compiled = crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::RegexCompileMiss,
        || {
            // Use the GNU-translated engine for all patterns.
            // Only optimize plain literals (no regex metacharacters).
            // A trivial literal is unaffected by POSIX vs non-POSIX
            // semantics because there is nothing to backtrack over,
            // so we can keep the Literal fast-path even when posix
            // is requested.
            if let Some(literal) = literal_from_trivial_regexp(pattern.as_bytes())
                && (!case_fold || literal.is_ascii())
            {
                Ok(CompiledSearchPattern::Literal(literal))
            } else {
                regex_emacs::regex_compile_lisp(pattern, posix, case_fold)
                    .map_err(|e| e.message)
                    .map(|mut cp| {
                        // `used_syntax`: the fastmap bakes ASCII
                        // membership of `[[:word:]]`/`[[:space:]]`.
                        // `regex_compile_lisp` baked the standard
                        // mapping; rebake against the active table
                        // (GNU bakes the buffer table directly at
                        // compile time, regex-emacs.c:2081-2092).
                        if cp.used_syntax && syntax_key != SyntaxCacheKey::Standard {
                            regex_emacs::recompute_fastmap(&mut cp, syntax);
                        }
                        CompiledSearchPattern::Emacs(Rc::new(cp))
                    })
            }
        },
    )?;

    // GNU `compile_pattern_1`: `cp->syntax_table = cp->buf.used_syntax
    // ? BVAR (current_buffer, syntax_table) : Qt;` — only patterns whose
    // compiled artifacts hardcode table content get the syntax axis.
    let entry_syntax_key = match &compiled {
        CompiledSearchPattern::Emacs(cp) if cp.used_syntax => Some(syntax_key),
        _ => None,
    };

    SEARCH_PATTERN_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.insert(
            0,
            (
                posix,
                case_fold,
                pattern.is_multibyte(),
                pattern.as_bytes().to_vec(),
                entry_syntax_key,
                compiled.clone(),
            ),
        );
        if cache.len() > SEARCH_PATTERN_CACHE_SIZE {
            cache.truncate(SEARCH_PATTERN_CACHE_SIZE);
        }
    });

    Ok(compiled)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn compile_lisp_pattern_with_posix(
    pattern: &LispString,
    case_fold: bool,
    posix: bool,
    target_multibyte: bool,
) -> Result<Rc<CompiledPattern>, String> {
    compile_lisp_pattern_with_posix_translation(
        pattern,
        case_fold,
        posix,
        target_multibyte,
        None,
        &DefaultSyntaxLookup,
    )
}

fn compile_lisp_pattern_with_posix_translation(
    pattern: &LispString,
    case_fold: bool,
    posix: bool,
    target_multibyte: bool,
    // The case-canon CHAR-TABLE, not a built CaseTranslation: a
    // CaseTranslation is a 1KB memo the cache-hit path never reads, and
    // building + cloning one per search call was ~170 Ir of pure waste.
    // The compiled pattern (cached) owns the built translation instead,
    // so its memo warms across calls.
    translation_table: Option<super::value::Value>,
    syntax: &dyn SyntaxLookup,
) -> Result<Rc<CompiledPattern>, String> {
    // The standard translation's cache key is a constant (its table is
    // None), so the cache probe never needs the table built; materializing
    // it copied a 1KB byte map per call, cache hits included. Build it on
    // the miss path only.
    let translation_key = if case_fold {
        Some(translation_table.map_or(0, |table| table.bits()))
    } else {
        None
    };
    let syntax_key = syntax.cache_key();

    if let Some(cached) = crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::RegexCompileHit,
        || {
            LISP_REGEX_PATTERN_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                let bytes = pattern.as_bytes();
                // Length first: it rejects most other patterns in a couple of
                // instructions before any flag or byte comparison.
                let matches = |entry: &LispRegexPatternCacheEntry| {
                    entry.pattern.len() == bytes.len()
                        && entry.posix == posix
                        && entry.case_fold == case_fold
                        && entry.translation_key == translation_key
                        && entry.pattern_multibyte == pattern.is_multibyte()
                        && entry.target_multibyte == target_multibyte
                        && syntax_key_matches(entry.syntax_key, syntax_key)
                        && entry.pattern.as_slice() == bytes
                };
                let index = cache.iter().position(|entry| matches(entry))?;
                if index != 0 {
                    cache[..=index].rotate_right(1);
                }
                Some(cache[0].compiled.clone())
            })
        },
    ) {
        return Ok(cached);
    }
    let effective_translation = if case_fold {
        translation_table
            .map(CaseTranslation::from_char_table)
            .or_else(|| Some(CaseTranslation::standard()))
    } else {
        None
    };
    let mut compiled = crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::RegexCompileMiss,
        || {
            regex_emacs::regex_compile_lisp_with_translation(pattern, posix, effective_translation)
                .map_err(|e| e.message)
        },
    )?;
    compiled.target_multibyte = target_multibyte;
    // Rebake the fastmap of `[[:word:]]`/`[[:space:]]` patterns against
    // the active syntax table (GNU compiles them with the buffer table
    // directly; see `compile_search_pattern_with_posix`).
    if compiled.used_syntax && syntax_key != SyntaxCacheKey::Standard {
        regex_emacs::recompute_fastmap(&mut compiled, syntax);
    }
    let entry_syntax_key = compiled.used_syntax.then_some(syntax_key);
    let compiled = Rc::new(compiled);

    LISP_REGEX_PATTERN_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.insert(
            0,
            Box::new(LispRegexPatternCacheEntry {
                posix,
                case_fold,
                translation_key,
                pattern_multibyte: pattern.is_multibyte(),
                target_multibyte,
                pattern: pattern.as_bytes().to_vec(),
                syntax_key: entry_syntax_key,
                compiled: compiled.clone(),
            }),
        );
        cache.truncate(LISP_REGEX_PATTERN_CACHE_SIZE);
    });

    Ok(compiled)
}

/// Classify a Lisp regexp using the same compilation and cache path as a
/// subsequent buffer search.
///
/// GNU's matcher can call `internal--syntax-propertize` lazily from
/// `UPDATE_SYNTAX_TABLE_*`.  Neomacs cannot run re-entrant Lisp while its Rust
/// matcher holds a buffer borrow, so evaluator-facing search builtins use this
/// classification to prepare syntax properties before entering the matcher.
pub(crate) fn buffer_regexp_syntax_dependency(
    buf: &Buffer,
    pattern: &LispString,
    case_fold: bool,
    posix: bool,
) -> Result<BufferRegexpSyntaxDependency, String> {
    buffer_regexp_syntax_dependency_compiled(buf, pattern, case_fold, posix)
        .map(|(dependency, _)| dependency)
}

/// The dependency together with the compiled pattern it was read from, so a
/// caller that goes on to match can hand the compiled pattern straight to
/// `looking_at_compiled` instead of probing the pattern cache a second time
/// (~300 Ir per probe; `looking-at` did it twice per call).  GNU's
/// `Flooking_at` compiles once (`compile_pattern`) and matches with that.
pub(crate) fn buffer_regexp_syntax_dependency_compiled(
    buf: &Buffer,
    pattern: &LispString,
    case_fold: bool,
    posix: bool,
) -> Result<(BufferRegexpSyntaxDependency, Rc<CompiledPattern>), String> {
    let syntax = buffer_syntax_lookup(buf);
    let compiled = compile_lisp_pattern_with_posix_translation(
        pattern,
        case_fold,
        posix,
        buf.get_multibyte(),
        buffer_search_translation_table(buf, case_fold),
        &syntax,
    )?;
    let dependency = if compiled.uses_syntax {
        BufferRegexpSyntaxDependency::BufferSyntaxDependent
    } else {
        BufferRegexpSyntaxDependency::SyntaxIndependent
    };
    Ok((dependency, compiled))
}

/// [`buffer_regexp_syntax_dependency`] plus the pattern's finite maximum
/// per-attempt span in characters (`None` when unbounded), for callers
/// that can propertize a bounded window instead of the whole tail.
pub(crate) fn buffer_regexp_syntax_dependency_and_span(
    buf: &Buffer,
    pattern: &LispString,
    case_fold: bool,
    posix: bool,
) -> Result<(BufferRegexpSyntaxDependency, Option<usize>), String> {
    let syntax = buffer_syntax_lookup(buf);
    let compiled = compile_lisp_pattern_with_posix_translation(
        pattern,
        case_fold,
        posix,
        buf.get_multibyte(),
        buffer_search_translation_table(buf, case_fold),
        &syntax,
    )?;
    let dependency = if compiled.uses_syntax {
        BufferRegexpSyntaxDependency::BufferSyntaxDependent
    } else {
        BufferRegexpSyntaxDependency::SyntaxIndependent
    };
    let span = regex_emacs::pattern_max_match_chars(&compiled);
    Ok((dependency, span))
}

fn string_char_match_data(searched_string: SearchedString, byte_md: EngineMatchData) -> MatchData {
    byte_md.publish_string(searched_string)
}

fn single_group_match_data(start: usize, end: usize) -> EngineMatchData {
    EngineMatchData::new(gnu_single_group_vec(Some(MatchGroup::new(start, end))))
}

/// Leftmost ASCII-case-insensitive occurrence of `needle` in `haystack`,
/// in guaranteed O(n+m) via KMP over ASCII-folded bytes (the sliding
/// `windows()` scan this replaces was O(n*m) on adversarial repetitive
/// inputs). An ASCII needle byte can never fold-match a UTF-8
/// continuation byte (>= 0x80), so on valid UTF-8 haystacks every match
/// offset is a char boundary.
fn ascii_fold_find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    // SIMD scan for the first byte's two case variants, then verify the
    // remainder at each candidate. On repetitive inputs where candidates
    // are dense and verification keeps failing late, the spent-work budget
    // drops to the guaranteed-linear KMP instead (no match starts before
    // the first unresolved candidate, so resuming there is exact).
    let fold = |b: u8| b.to_ascii_lowercase();
    let lo = needle[0].to_ascii_lowercase();
    let up = needle[0].to_ascii_uppercase();
    let tail = &needle[1..];
    let mut budget = haystack.len().saturating_mul(2).saturating_add(256);
    let mut at = 0usize;
    while at + needle.len() <= haystack.len() {
        let rel = if lo == up {
            memchr::memchr(lo, &haystack[at..])
        } else {
            memchr::memchr2(lo, up, &haystack[at..])
        };
        let start = match rel {
            Some(rel) => at + rel,
            None => return None,
        };
        if start + needle.len() > haystack.len() {
            return None;
        }
        let mut used = 1usize;
        let mut ok = true;
        for (i, &nb) in tail.iter().enumerate() {
            used += 1;
            if fold(haystack[start + 1 + i]) != fold(nb) {
                ok = false;
                break;
            }
        }
        if ok {
            return Some(start);
        }
        budget = budget.saturating_sub(used);
        if budget == 0 {
            return ascii_fold_find_bytes_kmp(&haystack[start..], needle)
                .map(|found| start + found);
        }
        at = start + 1;
    }
    None
}

/// Guaranteed O(n+m) ASCII-case-insensitive find: KMP over the folded
/// sequences. Backstop for [`ascii_fold_find_bytes`] on adversarial
/// repetitive inputs.
fn ascii_fold_find_bytes_kmp(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    let fold = |b: u8| b.to_ascii_lowercase();
    // KMP failure table over the folded needle.
    let mut table = vec![0usize; needle.len()];
    let mut k = 0;
    for i in 1..needle.len() {
        let b = fold(needle[i]);
        while k > 0 && fold(needle[k]) != b {
            k = table[k - 1];
        }
        if fold(needle[k]) == b {
            k += 1;
        }
        table[i] = k;
    }
    let mut k = 0;
    for (i, &hb) in haystack.iter().enumerate() {
        let hb = fold(hb);
        while k > 0 && fold(needle[k]) != hb {
            k = table[k - 1];
        }
        if fold(needle[k]) == hb {
            k += 1;
            if k == needle.len() {
                return Some(i + 1 - k);
            }
        }
    }
    None
}

/// Rightmost ASCII-case-insensitive occurrence: KMP over the reversed
/// folded sequences, mapping the reversed hit back to the original start.
fn ascii_fold_rfind_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(haystack.len());
    }
    let n = haystack.len();
    let m = needle.len();
    if m > n {
        return None;
    }
    let rn = |idx: usize| needle[m - 1 - idx].to_ascii_lowercase();
    let mut table = vec![0usize; m];
    let mut k = 0;
    for i in 1..m {
        let b = rn(i);
        while k > 0 && rn(k) != b {
            k = table[k - 1];
        }
        if rn(k) == b {
            k += 1;
        }
        table[i] = k;
    }
    let mut k = 0;
    for i in 0..n {
        let hb = haystack[n - 1 - i].to_ascii_lowercase();
        while k > 0 && rn(k) != hb {
            k = table[k - 1];
        }
        if rn(k) == hb {
            k += 1;
            if k == m {
                return Some(n - 1 - i);
            }
        }
    }
    None
}

fn ascii_case_fold_find(haystack: &str, needle: &str) -> Option<usize> {
    ascii_fold_find_bytes(haystack.as_bytes(), needle.as_bytes())
}

fn ascii_case_fold_rfind(haystack: &str, needle: &str) -> Option<usize> {
    ascii_fold_rfind_bytes(haystack.as_bytes(), needle.as_bytes())
}

fn unicode_case_fold_literal_find(text: &str, literal: &str) -> Option<MatchGroup> {
    let needle: Vec<char> = literal.chars().flat_map(|ch| ch.to_lowercase()).collect();
    if needle.is_empty() {
        return Some(MatchGroup::new(0, 0));
    }
    let mut window = std::collections::VecDeque::with_capacity(needle.len());
    let mut ranges = std::collections::VecDeque::with_capacity(needle.len());
    for (byte_start, ch) in text.char_indices() {
        let byte_end = byte_start + ch.len_utf8();
        for folded_ch in ch.to_lowercase() {
            window.push_back(folded_ch);
            ranges.push_back((byte_start, byte_end));
            if window.len() > needle.len() {
                window.pop_front();
                ranges.pop_front();
            }
            if window.len() == needle.len()
                && window
                    .iter()
                    .zip(needle.iter())
                    .all(|(lhs, rhs)| lhs == rhs)
            {
                return Some(MatchGroup::new(ranges.front()?.0, ranges.back()?.1));
            }
        }
    }
    None
}

fn unicode_case_fold_literal_rfind(text: &str, literal: &str) -> Option<MatchGroup> {
    let needle: Vec<char> = literal.chars().flat_map(|ch| ch.to_lowercase()).collect();
    if needle.is_empty() {
        return Some(MatchGroup::new(text.len(), text.len()));
    }
    let mut last_match = None;
    let mut window = std::collections::VecDeque::with_capacity(needle.len());
    let mut ranges = std::collections::VecDeque::with_capacity(needle.len());
    for (byte_start, ch) in text.char_indices() {
        let byte_end = byte_start + ch.len_utf8();
        for folded_ch in ch.to_lowercase() {
            window.push_back(folded_ch);
            ranges.push_back((byte_start, byte_end));
            if window.len() > needle.len() {
                window.pop_front();
                ranges.pop_front();
            }
            if window.len() == needle.len()
                && window
                    .iter()
                    .zip(needle.iter())
                    .all(|(lhs, rhs)| lhs == rhs)
            {
                last_match = Some(MatchGroup::new(ranges.front()?.0, ranges.back()?.1));
            }
        }
    }
    last_match
}

/// Find LITERAL inside TEXT, optionally case-folded.
///
/// GNU `src/search.c:1761+` ports a Boyer-Moore implementation
/// (`boyer_moore`) with case-fold-aware skip table generation. For
/// long literal needles in large buffers Boyer-Moore is roughly
/// O(n/m) instead of O(n). neomacs uses naive substring scanning
/// here (delegating to `str::find` and a tiny ASCII case-fold
/// helper). Audit finding #18 in `drafts/regex-search-audit.md`
/// flags this as a perf gap, not a correctness gap; the audit's
/// Phase D Task 4.2 covers porting boyer_moore (~1 day).
fn literal_find(text: &str, literal: &str, case_fold: bool) -> Option<MatchGroup> {
    crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::RegexLiteralFind,
        || {
            let start = if case_fold {
                if literal.is_ascii() {
                    ascii_case_fold_find(text, literal)?
                } else {
                    return unicode_case_fold_literal_find(text, literal);
                }
            } else {
                text.find(literal)?
            };
            Some(MatchGroup::new(start, start + literal.len()))
        },
    )
}

fn literal_rfind(text: &str, literal: &str, case_fold: bool) -> Option<MatchGroup> {
    crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::RegexLiteralFind,
        || {
            let start = if case_fold {
                if literal.is_ascii() {
                    ascii_case_fold_rfind(text, literal)?
                } else {
                    return unicode_case_fold_literal_rfind(text, literal);
                }
            } else {
                text.rfind(literal)?
            };
            Some(MatchGroup::new(start, start + literal.len()))
        },
    )
}

/// Decode the next character at byte offset `at`. In a unibyte target every
/// byte is its own character (raw bytes must not be decoded as multibyte leads,
/// see the unibyte note in `literal_find_emacs_bytes`); in a multibyte target
/// decode one Emacs char.
fn next_char_at(text: &[u8], at: usize, multibyte: bool) -> (u32, usize) {
    if multibyte {
        let (code, len) = crate::emacs_core::emacs_char::string_char(&text[at..]);
        (code, len.max(1))
    } else {
        (text[at] as u32, 1)
    }
}

/// Canonicalize `literal` char-by-char through the buffer case-canon table.
fn canon_fold_pattern(literal: &[u8], multibyte: bool, trt: &CaseTranslation) -> Vec<u32> {
    let mut pat = Vec::new();
    let mut i = 0;
    while i < literal.len() {
        let (code, len) = next_char_at(literal, i, multibyte);
        pat.push(trt.translate(code));
        i += len;
    }
    pat
}

/// True if `pat` (already canonicalized) matches `text` at byte offset `at`,
/// canonicalizing each text char through `trt`. Returns the end byte offset.
fn canon_fold_match_at(
    text: &[u8],
    at: usize,
    pat: &[u32],
    multibyte: bool,
    trt: &CaseTranslation,
) -> Option<usize> {
    let mut ti = at;
    for &pc in pat {
        if ti >= text.len() {
            return None;
        }
        let (code, len) = next_char_at(text, ti, multibyte);
        if trt.translate(code) != pc {
            return None;
        }
        ti += len;
    }
    Some(ti)
}

/// Forward literal search that folds through the buffer's case-canon table
/// (used only when a custom `set-case-syntax-pair` table is installed). GNU's
/// `simple_search` canonicalizes each char through the search `trt`.
fn canon_fold_literal_find(
    text: &[u8],
    literal: &[u8],
    multibyte: bool,
    trt: &CaseTranslation,
) -> Option<MatchGroup> {
    let pat = canon_fold_pattern(literal, multibyte, trt);
    if pat.is_empty() {
        return Some(MatchGroup::new(0, 0));
    }
    // Anchor lane instead of verifying at every char: enumerate the ASCII
    // bytes that canonicalize to the pattern's first char (memo lookups),
    // then scan ASCII stretches with SIMD (is_ascii + memchr) and verify
    // only at anchor bytes. A non-ASCII char can also canonicalize into
    // the anchor (e.g. U+212A KELVIN SIGN -> k), so any non-ASCII window
    // falls back to the per-char verify walk for its span.
    if let Some((n_anchors, anchors)) = ascii_canon_anchors(trt, pat[0]) {
        const WINDOW: usize = 512;
        let mut at = 0usize;
        while at < text.len() {
            let wend = at.saturating_add(WINDOW).min(text.len());
            let window = &text[at..wend];
            if window.is_ascii() {
                let hit = match n_anchors {
                    0 => None,
                    1 => memchr::memchr(anchors[0], window),
                    _ => memchr::memchr2(anchors[0], anchors[1], window),
                };
                match hit {
                    Some(rel) => {
                        let cand = at + rel;
                        if let Some(end) = canon_fold_match_at(text, cand, &pat, multibyte, trt) {
                            return Some(MatchGroup::new(cand, end));
                        }
                        at = cand + 1;
                    }
                    None => at = wend,
                }
            } else {
                let mut wat = at;
                while wat < wend {
                    let b = text[wat];
                    if b >= 0x80
                        || (n_anchors >= 1 && b == anchors[0])
                        || (n_anchors == 2 && b == anchors[1])
                    {
                        if let Some(end) = canon_fold_match_at(text, wat, &pat, multibyte, trt) {
                            return Some(MatchGroup::new(wat, end));
                        }
                    }
                    let (_code, len) = next_char_at(text, wat, multibyte);
                    wat += len;
                }
                at = wat;
            }
        }
        return None;
    }
    // Odd tables (3+ ASCII sources for one canonical char): original walk.
    let mut at = 0;
    loop {
        if let Some(end) = canon_fold_match_at(text, at, &pat, multibyte, trt) {
            return Some(MatchGroup::new(at, end));
        }
        if at >= text.len() {
            return None;
        }
        let (_code, len) = next_char_at(text, at, multibyte);
        at += len;
    }
}

/// Backward analogue of `canon_fold_literal_find`: returns the rightmost match.
/// The ASCII bytes that canonicalize to `canon` under `trt` — the anchor
/// set for SIMD candidate scans. `None` when more than two do (odd table);
/// callers fall back to the verify-every-char walk then.
fn ascii_canon_anchors(trt: &CaseTranslation, canon: u32) -> Option<(usize, [u8; 2])> {
    let mut anchors = [0u8; 2];
    let mut n_anchors = 0usize;
    for b in 0..128u8 {
        if trt.translate(b as u32) == canon {
            if n_anchors == 2 {
                return None;
            }
            anchors[n_anchors] = b;
            n_anchors += 1;
        }
    }
    Some((n_anchors, anchors))
}

fn canon_fold_literal_rfind(
    text: &[u8],
    literal: &[u8],
    multibyte: bool,
    trt: &CaseTranslation,
) -> Option<MatchGroup> {
    let pat = canon_fold_pattern(literal, multibyte, trt);
    if pat.is_empty() {
        return Some(MatchGroup::new(text.len(), text.len()));
    }
    let Some((n_anchors, anchors)) = ascii_canon_anchors(trt, pat[0]) else {
        // Odd tables (3+ ASCII sources for one canonical char): the
        // original verify-every-char walk, keeping the last hit.
        let mut last = None;
        let mut at = 0;
        loop {
            if let Some(end) = canon_fold_match_at(text, at, &pat, multibyte, trt) {
                last = Some(MatchGroup::new(at, end));
            }
            if at >= text.len() {
                return last;
            }
            let (_code, len) = next_char_at(text, at, multibyte);
            at += len;
        }
    };
    // Reverse windows, candidates checked right-to-left: the first verified
    // candidate is the rightmost match start, which is what the forward
    // walk's keep-the-last produced. An ASCII window implies its start is a
    // char boundary (a multibyte char reaching in would put a continuation
    // byte >= 0x80 inside); mixed windows walk forward per char and check
    // their collected candidates in reverse — a non-ASCII char can
    // canonicalize into the anchor (e.g. U+212A -> k), so every one is a
    // candidate.
    const WINDOW: usize = 512;
    let mut wend = text.len();
    while wend > 0 {
        let mut wstart = wend.saturating_sub(WINDOW);
        if multibyte {
            let mut steps = 0;
            while wstart > 0 && steps < 4 && text[wstart] & 0xC0 == 0x80 {
                wstart -= 1;
                steps += 1;
            }
        }
        let window = &text[wstart..wend];
        if window.is_ascii() {
            let mut upto = window.len();
            while upto > 0 {
                let rel = match n_anchors {
                    0 => None,
                    1 => memchr::memrchr(anchors[0], &window[..upto]),
                    _ => memchr::memrchr2(anchors[0], anchors[1], &window[..upto]),
                };
                let Some(rel) = rel else { break };
                let cand = wstart + rel;
                if let Some(end) = canon_fold_match_at(text, cand, &pat, multibyte, trt) {
                    return Some(MatchGroup::new(cand, end));
                }
                upto = rel;
            }
        } else {
            let mut cands: smallvec::SmallVec<[usize; 32]> = smallvec::SmallVec::new();
            let mut wat = wstart;
            while wat < wend {
                let b = text[wat];
                if b >= 0x80
                    || (n_anchors >= 1 && b == anchors[0])
                    || (n_anchors == 2 && b == anchors[1])
                {
                    cands.push(wat);
                }
                let (_code, len) = next_char_at(text, wat, multibyte);
                wat += len;
            }
            for &cand in cands.iter().rev() {
                if let Some(end) = canon_fold_match_at(text, cand, &pat, multibyte, trt) {
                    return Some(MatchGroup::new(cand, end));
                }
            }
        }
        wend = wstart;
    }
    None
}

fn literal_find_emacs_bytes(
    text: &[u8],
    literal: &[u8],
    multibyte: bool,
    case_fold: bool,
) -> Option<MatchGroup> {
    crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::RegexLiteralFind,
        || {
            if literal.is_empty() {
                return Some(MatchGroup::new(0, 0));
            }
            if !case_fold {
                // Two-way search: O(n+m) where the previous sliding
                // `windows()` comparison was O(n*m) on repetitive inputs.
                return memchr::memmem::find(text, literal)
                    .map(|start| MatchGroup::new(start, start + literal.len()));
            }
            if literal.is_ascii() {
                return ascii_fold_find_bytes(text, literal)
                    .map(|start| MatchGroup::new(start, start + literal.len()));
            }
            // Unibyte target: GNU `simple_search` (search.c:1622-1633) advances
            // one BYTE per position and case-folds each byte through the case
            // table.  We must NOT decode multibyte sequences here, or a raw byte
            // embedded in what looks like a multibyte lead (e.g. the 0xA9 in a
            // C3 A9 pair) would be skipped — that was the unibyte search bug.
            if !multibyte {
                if text.len() < literal.len() {
                    return None;
                }
                for at in 0..=text.len() - literal.len() {
                    if let Some(end) = crate::emacs_core::emacs_char::unibyte_case_fold_match_len(
                        text, at, literal,
                    ) {
                        return Some(MatchGroup::new(at, end));
                    }
                }
                return None;
            }
            if let (Some(text_utf8), Some(literal_utf8)) = (
                crate::emacs_core::emacs_char::try_as_utf8(text),
                crate::emacs_core::emacs_char::try_as_utf8(literal),
            ) {
                return literal_find(text_utf8, literal_utf8, true);
            }

            // Non-ASCII case-fold over raw Emacs bytes: compare Emacs-downcased
            // char codes in place so offsets stay in the text's own byte space
            // (eight-bit chars are caseless, matching GNU) — no storage round-trip.
            let mut at = 0;
            loop {
                if let Some(end) =
                    crate::emacs_core::emacs_char::case_fold_match_len(text, at, literal)
                {
                    return Some(MatchGroup::new(at, end));
                }
                if at >= text.len() {
                    return None;
                }
                let (_code, len) = crate::emacs_core::emacs_char::string_char(&text[at..]);
                at += len.max(1);
            }
        },
    )
}

fn literal_rfind_emacs_bytes(
    text: &[u8],
    literal: &[u8],
    multibyte: bool,
    case_fold: bool,
) -> Option<MatchGroup> {
    crate::emacs_core::perf_trace::time_op(
        crate::emacs_core::perf_trace::HotpathOp::RegexLiteralFind,
        || {
            if literal.is_empty() {
                return Some(MatchGroup::new(text.len(), text.len()));
            }
            if !case_fold {
                // Reverse two-way search; the previous reversed `windows()`
                // scan was O(n*m) and lost the memcmp specialization too.
                return memchr::memmem::rfind(text, literal)
                    .map(|start| MatchGroup::new(start, start + literal.len()));
            }
            if literal.is_ascii() {
                return ascii_fold_rfind_bytes(text, literal)
                    .map(|start| MatchGroup::new(start, start + literal.len()));
            }
            // Unibyte target: byte-by-byte rightmost case-fold scan, mirroring
            // GNU `simple_search` reverse search.  See `literal_find_emacs_bytes`.
            if !multibyte {
                if text.len() < literal.len() {
                    return None;
                }
                for at in (0..=text.len() - literal.len()).rev() {
                    if let Some(end) = crate::emacs_core::emacs_char::unibyte_case_fold_match_len(
                        text, at, literal,
                    ) {
                        return Some(MatchGroup::new(at, end));
                    }
                }
                return None;
            }
            if let (Some(text_utf8), Some(literal_utf8)) = (
                crate::emacs_core::emacs_char::try_as_utf8(text),
                crate::emacs_core::emacs_char::try_as_utf8(literal),
            ) {
                return literal_rfind(text_utf8, literal_utf8, true);
            }

            // Rightmost non-ASCII case-fold match over raw Emacs bytes, matching
            // the prior rfind: compare Emacs-downcased char codes in place.
            let mut best = None;
            let mut at = 0;
            while at < text.len() {
                if let Some(end) =
                    crate::emacs_core::emacs_char::case_fold_match_len(text, at, literal)
                {
                    best = Some(MatchGroup::new(at, end));
                }
                let (_code, len) = crate::emacs_core::emacs_char::string_char(&text[at..]);
                at += len.max(1);
            }
            best
        },
    )
}

fn next_search_char_boundary(text: &[u8], pos: usize) -> Option<usize> {
    if pos >= text.len() {
        return None;
    }
    let (_code, len) = crate::emacs_core::emacs_char::string_char(&text[pos..]);
    Some(pos + len)
}

// ---------------------------------------------------------------------------
// Buffer search primitives
// ---------------------------------------------------------------------------

fn with_buffer_emacs_bytes<R>(
    buf: &Buffer,
    range: EmacsByteRange,
    f: impl FnOnce(&[u8]) -> R,
) -> R {
    if buf.has_contiguous_emacs_byte_range(range) {
        return buf
            .with_contiguous_emacs_byte_range(range, f)
            .expect("checked contiguous buffer range should borrow");
    }

    let mut text = Vec::new();
    buf.copy_emacs_byte_range_to(range, &mut text);
    f(&text)
}

/// [`with_buffer_emacs_bytes`] for the regex-engine search paths.
///
/// When the gap-buffer gap sits inside the searched range, every search
/// used to copy the whole accessible region (audit #17).  GNU never
/// copies: `re_search_2` walks the two gap segments in place.  The
/// port's engine is single-segment, so the clean equivalent is one gap
/// motion out of the range (amortized: after an edit parks the gap
/// mid-buffer, the first search moves it once and the rest of the
/// font-lock pass borrows zero-copy).  Chunked backends (piece tree,
/// rope) still take the copy fallback inside `with_buffer_emacs_bytes`.
fn with_buffer_emacs_bytes_for_search<R>(
    buf: &Buffer,
    range: EmacsByteRange,
    f: impl FnOnce(&[u8]) -> R,
) -> R {
    buf.try_make_emacs_byte_range_contiguous(range);
    with_buffer_emacs_bytes(buf, range, f)
}

/// Apply a Lisp-originated regexp to UTF-8 text as a boolean predicate.
///
/// This is the compatibility seam for Lisp-facing APIs that receive an Emacs
/// regexp but do not need match data.  It mirrors GNU's `fast_c_string_match`:
/// matching is case-sensitive, uses the standard syntax table, and reuses the
/// shared compiled-pattern cache.
pub(crate) fn predicate_match(pattern: &LispString, text: &str) -> Result<bool, EmacsRegexpError> {
    if pattern.is_multibyte() {
        let unibyte_pattern = LispString::from_unibyte(
            crate::emacs_core::emacs_char::str_to_unibyte(pattern.as_bytes()),
        );
        predicate_match_with_case_fold(&unibyte_pattern, text, false, false)
    } else {
        predicate_match_with_case_fold(pattern, text, false, false)
    }
}

/// Case-insensitive counterpart to [`predicate_match`].
///
/// GNU's font APIs use `fast_string_match_ignore_case` for Lisp-supplied
/// patterns.  Keeping that policy as a named operation avoids exposing engine
/// flags or translation tables to callers.
pub(crate) fn predicate_match_ignore_case(
    pattern: &LispString,
    text: &str,
) -> Result<bool, EmacsRegexpError> {
    predicate_match_with_case_fold(pattern, text, true, true)
}

fn predicate_match_with_case_fold(
    pattern: &LispString,
    text: &str,
    case_fold: bool,
    target_multibyte: bool,
) -> Result<bool, EmacsRegexpError> {
    let syntax = DefaultSyntaxLookup;
    let compiled = compile_lisp_pattern_with_posix_translation(
        pattern,
        case_fold,
        false,
        target_multibyte,
        None,
        &syntax,
    )
    .map_err(EmacsRegexpError::Compile)?;
    let found = regex_emacs::re_search(
        compiled.as_ref(),
        text.as_bytes(),
        0,
        text.len() as isize,
        &syntax,
        STRING_MATCH_AT_DOT_UNREACHABLE,
    );
    if found.is_none() && regex_emacs::take_matcher_overflow() {
        return Err(EmacsRegexpError::MatcherOverflow);
    }
    Ok(found.is_some())
}

/// Match a Tree-sitter predicate regexp against a captured buffer region.
///
/// GNU narrows the parser buffer to the captured node and calls its buffer
/// regexp engine.  Supplying only that region to the matcher gives anchors
/// the same narrowed-buffer bounds while retaining the buffer's syntax and
/// current point for assertions such as `\\=`.
pub(crate) fn treesit_predicate_match_lisp(
    buf: &Buffer,
    pattern: &LispString,
    range: EmacsByteRange,
) -> Result<bool, String> {
    let syntax = buffer_syntax_lookup(buf);
    let compiled = compile_lisp_pattern_with_posix_translation(
        pattern,
        false,
        false,
        buf.get_multibyte(),
        None,
        &syntax,
    )?;
    let point = buf.point_emacs_byte_pos();
    let point_relative = if point >= range.start() && point <= range.end() {
        point.get() - range.start().get()
    } else {
        usize::MAX
    };
    let found = with_buffer_emacs_bytes_for_search(buf, range, |text| {
        regex_emacs::re_search(
            compiled.as_ref(),
            text,
            0,
            text.len() as isize,
            &syntax,
            point_relative,
        )
    });
    if found.is_none() && regex_emacs::take_matcher_overflow() {
        return Err(regex_emacs::MATCHER_OVERFLOW_MESSAGE.to_string());
    }
    Ok(found.is_some())
}

/// Search forward from point for a literal string PATTERN.
///
/// If found, returns the end of match as the point position the caller should
/// apply. If not found, behaviour depends on `noerror`:
/// - `noerror` false: signals `search-failed`
/// - `noerror` true: returns `None` without signaling
///
/// `bound` optionally limits the search to positions <= bound.
pub(crate) fn search_forward(
    buf: &mut Buffer,
    pattern: &crate::heap_types::LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
) -> Result<Option<BufferSearchSuccess>, String> {
    let start = buf.point_emacs_byte_pos();
    let accessible = buf.accessible_emacs_byte_region();
    let limit = bound
        .map(EmacsBytePos::new)
        .unwrap_or(accessible.end())
        .min(accessible.end());

    if start > limit {
        if noerror {
            return Ok(None);
        }
        return Err(format!(
            "Search failed: \"{}\"",
            crate::emacs_core::emacs_char::to_utf8_lossy(pattern.as_bytes())
        ));
    }

    let multibyte = buf.get_multibyte();
    let literal = coerce_pattern_to_buffer_bytes(pattern, multibyte);
    // A custom `set-case-syntax-pair` table folds custom pairs (e.g. [/])
    // during search; route through the buffer's case-canon table then, else
    // keep the fast hardwired ASCII/Unicode folding.
    let translation = buffer_search_translation(buf, case_fold);
    let found =
        with_buffer_emacs_bytes(
            buf,
            EmacsByteRange::new(start, limit),
            |text| match &translation {
                Some(trt) => canon_fold_literal_find(text, &literal, multibyte, trt),
                None => literal_find_emacs_bytes(text, &literal, multibyte, case_fold),
            },
        );

    if let Some(found) = found {
        let matched = found.shift(start.get());
        let match_end = matched.end();
        let engine_match = EngineMatchData::new(gnu_single_group_vec(Some(matched)));
        Ok(Some(BufferSearchSuccess::new(
            buf,
            EmacsBytePos::new(match_end),
            engine_match,
        )))
    } else if noerror {
        // When noerror is t, don't move point.
        // When noerror is a value, move point to bound.
        Ok(None)
    } else {
        Err(format!(
            "Search failed: \"{}\"",
            crate::emacs_core::emacs_char::to_utf8_lossy(pattern.as_bytes())
        ))
    }
}

/// Search backward from point for a literal string PATTERN.
///
/// If found, returns the beginning of match as the point position the caller
/// should apply.
pub(crate) fn search_backward(
    buf: &mut Buffer,
    pattern: &crate::heap_types::LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
) -> Result<Option<BufferSearchSuccess>, String> {
    let end = buf.point_emacs_byte_pos();
    let accessible = buf.accessible_emacs_byte_region();
    let limit = bound
        .map(EmacsBytePos::new)
        .unwrap_or(accessible.start())
        .max(accessible.start());

    if end < limit {
        if noerror {
            return Ok(None);
        }
        return Err(format!(
            "Search failed: \"{}\"",
            crate::emacs_core::emacs_char::to_utf8_lossy(pattern.as_bytes())
        ));
    }

    let multibyte = buf.get_multibyte();
    let literal = coerce_pattern_to_buffer_bytes(pattern, multibyte);
    let translation = buffer_search_translation(buf, case_fold);
    let found =
        with_buffer_emacs_bytes(
            buf,
            EmacsByteRange::new(limit, end),
            |text| match &translation {
                Some(trt) => canon_fold_literal_rfind(text, &literal, multibyte, trt),
                None => literal_rfind_emacs_bytes(text, &literal, multibyte, case_fold),
            },
        );

    if let Some(found) = found {
        let matched = found.shift(limit.get());
        let point = matched.start();
        let engine_match = EngineMatchData::new(gnu_single_group_vec(Some(matched)));
        Ok(Some(BufferSearchSuccess::new(
            buf,
            EmacsBytePos::new(point),
            engine_match,
        )))
    } else if noerror {
        Ok(None)
    } else {
        Err(format!(
            "Search failed: \"{}\"",
            crate::emacs_core::emacs_char::to_utf8_lossy(pattern.as_bytes())
        ))
    }
}

/// Issue #131: coerce a literal search pattern to the buffer's multibyteness the
/// way GNU does, producing Emacs internal-encoding bytes directly — no storage
/// round-trip.
///
/// This mirrors GNU `search_buffer_non_re` (search.c:1319-1343), which coerces
/// the *pattern* to the *buffer's* multibyteness via `copy_text` before the
/// byte/char comparison:
///   - same multibyteness  → raw bytes as-is;
///   - unibyte pattern, multibyte buffer → widen via `str_to_multibyte`
///     (each raw byte becomes an eight-bit character);
///   - multibyte pattern, unibyte buffer → narrow via `str_to_unibyte`
///     (each char collapses to its low byte `c & 0xFF`, one byte per char).
///
/// The last case is what makes a genuine multibyte sequence fail to match the
/// equal raw bytes in a unibyte buffer: e.g. searching for the multibyte char
/// é (internal bytes C3 A9) in a unibyte buffer narrows the pattern to the
/// single byte 0xE9, so it cannot spuriously match the C3 A9 byte pair. (GNU
/// uses `copy_text`, NOT `string-as-unibyte`, which would reinterpret the
/// internal bytes and produce the wrong, raw-byte match.)
fn coerce_pattern_to_buffer_bytes(
    pattern: &crate::heap_types::LispString,
    buf_multibyte: bool,
) -> Vec<u8> {
    if pattern.is_multibyte() == buf_multibyte {
        pattern.as_bytes().to_vec()
    } else if buf_multibyte {
        crate::emacs_core::emacs_char::str_to_multibyte(pattern.as_bytes())
    } else {
        crate::emacs_core::emacs_char::str_to_unibyte(pattern.as_bytes())
    }
}

/// Search forward from point for a regex PATTERN.
///
/// If found, returns the end of match as the point position the caller should
/// apply.
/// Updates match data with capture groups.
#[allow(dead_code)] // crate-private raw search seam exercised by unit tests
pub(crate) fn re_search_forward(
    buf: &mut Buffer,
    pattern: &crate::heap_types::LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
) -> Result<Option<BufferSearchSuccess>, String> {
    re_search_forward_with_posix(buf, pattern, bound, noerror, case_fold, false)
}

/// POSIX longest-match variant of [`re_search_forward`] used by
/// `posix-search-forward`. See GNU `src/search.c:Fposix_search_forward`.
#[allow(dead_code)] // crate-private raw search seam exercised by unit tests
pub(crate) fn re_search_forward_with_posix(
    buf: &mut Buffer,
    pattern: &crate::heap_types::LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
    posix: bool,
) -> Result<Option<BufferSearchSuccess>, String> {
    let start = buf.point_emacs_byte_pos();
    let accessible = buf.accessible_emacs_byte_region();
    let limit = bound
        .map(EmacsBytePos::new)
        .unwrap_or(accessible.end())
        .min(accessible.end());

    if start > limit {
        if noerror {
            return Ok(None);
        }
        return Err(format!(
            "Search failed: \"{}\"",
            crate::emacs_core::emacs_char::to_utf8_lossy(pattern.as_bytes())
        ));
    }

    let region_start = accessible.start();
    let start_rel = start.get() - region_start.get();
    let limit_rel = limit.get() - region_start.get();
    let multibyte = buf.get_multibyte();
    let syn = buffer_syntax_lookup(buf);
    let compiled =
        compile_search_pattern_with_posix(&pattern_for_compile(pattern), case_fold, posix, &syn)?;

    let md_opt =
        with_buffer_emacs_bytes_for_search(buf, accessible.range(), |text| match &compiled {
            CompiledSearchPattern::Literal(literal) => {
                literal_find_emacs_bytes(&text[start_rel..limit_rel], literal, multibyte, case_fold)
                    .map(|matched| {
                        EngineMatchData::new(gnu_single_group_vec(Some(matched.shift(start.get()))))
                    })
            }
            CompiledSearchPattern::Emacs(cp) => {
                let range = (limit_rel - start_rel) as isize;
                regex_emacs::re_search(cp.as_ref(), text, start_rel, range, &syn, start_rel).map(
                    |(_pos, regs)| {
                        buffer_engine_match_data_from_registers(&regs, region_start.get())
                    },
                )
            }
        });

    if md_opt.is_none()
        && matches!(compiled, CompiledSearchPattern::Emacs(_))
        && regex_emacs::take_matcher_overflow()
    {
        // GNU search.c:matcher_overflow — a -2 from the matcher is an
        // error, not a search failure.
        return Err(regex_emacs::MATCHER_OVERFLOW_MESSAGE.to_string());
    }
    if let Some(engine_match) = md_opt {
        let point = EmacsBytePos::new(engine_match.group(0).unwrap().end());
        Ok(Some(BufferSearchSuccess::new(buf, point, engine_match)))
    } else if noerror {
        Ok(None)
    } else {
        Err(format!(
            "Search failed: \"{}\"",
            crate::emacs_core::emacs_char::to_utf8_lossy(pattern.as_bytes())
        ))
    }
}

/// Search backward from point for a regex PATTERN.
///
/// If found, returns the beginning of match as the point position the caller
/// should apply.
/// Updates match data with capture groups.
#[allow(dead_code)] // crate-private raw search seam exercised by unit tests
pub(crate) fn re_search_backward(
    buf: &mut Buffer,
    pattern: &crate::heap_types::LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
) -> Result<Option<BufferSearchSuccess>, String> {
    re_search_backward_with_posix(buf, pattern, bound, noerror, case_fold, false)
}

/// POSIX longest-match variant of [`re_search_backward`] used by
/// `posix-search-backward`. See GNU `src/search.c:Fposix_search_backward`.
#[allow(dead_code)] // crate-private raw search seam exercised by unit tests
pub(crate) fn re_search_backward_with_posix(
    buf: &mut Buffer,
    pattern: &crate::heap_types::LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
    posix: bool,
) -> Result<Option<BufferSearchSuccess>, String> {
    let end = buf.point_emacs_byte_pos();
    let accessible = buf.accessible_emacs_byte_region();
    let limit = bound
        .map(EmacsBytePos::new)
        .unwrap_or(accessible.start())
        .max(accessible.start());

    if end < limit {
        if noerror {
            return Ok(None);
        }
        return Err(format!(
            "Search failed: \"{}\"",
            crate::emacs_core::emacs_char::to_utf8_lossy(pattern.as_bytes())
        ));
    }

    let region_start = accessible.start();
    let start_rel = end.get() - region_start.get();
    let limit_rel = limit.get() - region_start.get();
    let multibyte = buf.get_multibyte();
    let syn = buffer_syntax_lookup(buf);
    let compiled =
        compile_search_pattern_with_posix(&pattern_for_compile(pattern), case_fold, posix, &syn)?;

    let md_opt =
        with_buffer_emacs_bytes_for_search(buf, accessible.range(), |text| match &compiled {
            CompiledSearchPattern::Literal(literal) => literal_rfind_emacs_bytes(
                &text[limit_rel..start_rel],
                literal,
                multibyte,
                case_fold,
            )
            .map(|matched| {
                EngineMatchData::new(gnu_single_group_vec(Some(
                    matched.shift(region_start.get() + limit_rel),
                )))
            }),
            CompiledSearchPattern::Emacs(cp) => {
                // Backward search: negative range means search backward.
                let range = -((start_rel - limit_rel) as isize);
                regex_emacs::re_search(cp.as_ref(), text, start_rel, range, &syn, start_rel).map(
                    |(_pos, regs)| {
                        buffer_engine_match_data_from_registers(&regs, region_start.get())
                    },
                )
            }
        });

    if md_opt.is_none()
        && matches!(compiled, CompiledSearchPattern::Emacs(_))
        && regex_emacs::take_matcher_overflow()
    {
        return Err(regex_emacs::MATCHER_OVERFLOW_MESSAGE.to_string());
    }
    if let Some(engine_match) = md_opt {
        let point = EmacsBytePos::new(engine_match.group(0).unwrap().start());
        Ok(Some(BufferSearchSuccess::new(buf, point, engine_match)))
    } else if noerror {
        Ok(None)
    } else {
        Err(format!(
            "Search failed: \"{}\"",
            crate::emacs_core::emacs_char::to_utf8_lossy(pattern.as_bytes())
        ))
    }
}

/// Build the case-fold translate table for a search in `buf`: the buffer's
/// case-canon table (GNU's search `trt`) when a custom `set-case-syntax-pair`
/// table is installed, else `None` so the engine's fast hardwired folding is
/// used. Mirrors GNU search.c installing `BVAR (current_buffer, case_canon_table)`.
fn buffer_search_translation(
    buf: &Buffer,
    case_fold: bool,
) -> Option<std::rc::Rc<CaseTranslation>> {
    let table = buffer_search_translation_table(buf, case_fold)?;
    // One-entry per-thread cache keyed by table IDENTITY: rebuilding the
    // translation per search left its lazy 0..256 memo permanently cold, so
    // every anchor-enumeration and verify lookup walked the char-table
    // (+650M Ir on a literal-search sweep). Identity keying is the
    // established policy — the compiled-pattern cache and GNU's own
    // `compile_pattern` (search.c, `EQ (cp->buf.translate, translate)`)
    // both key cached translations the same way.
    thread_local! {
        static LITERAL_TRT_CACHE: std::cell::RefCell<Option<(usize, std::rc::Rc<CaseTranslation>)>> =
            const { std::cell::RefCell::new(None) };
    }
    Some(LITERAL_TRT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((bits, trt)) = cache.as_ref()
            && *bits == table.bits()
        {
            return trt.clone();
        }
        let trt = std::rc::Rc::new(CaseTranslation::from_char_table(table));
        *cache = Some((table.bits(), trt.clone()));
        trt
    }))
}

/// The case-canon table alone, for the regexp-compile path: the compiled
/// pattern builds (and caches) its own `CaseTranslation`, so handing compile
/// a prebuilt 1KB memo per call would be waste the cache-hit path discards.
fn buffer_search_translation_table(buf: &Buffer, case_fold: bool) -> Option<super::value::Value> {
    if !case_fold {
        return None;
    }
    crate::emacs_core::casetab::buffer_case_canon_table(buf)
}

pub(crate) fn re_search_forward_lisp_with_posix(
    buf: &mut Buffer,
    pattern: &LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
    posix: bool,
    match_context: BufferRegexpMatchContext<'_>,
) -> Result<Option<BufferSearchSuccess>, String> {
    // GNU `re-search-forward` on a buffer drives the matcher with raw buffer
    // byte positions (`PT_BYTE`, `BEGV_BYTE`, `ZV_BYTE`), even when the
    // pattern itself is a Lisp string.
    let start = buf.point_emacs_byte_pos();
    let accessible = buf.accessible_emacs_byte_region();
    let limit = bound
        .map(EmacsBytePos::new)
        .unwrap_or(accessible.end())
        .min(accessible.end());

    if start > limit {
        if noerror {
            return Ok(None);
        }
        return Err("Search failed".to_string());
    }

    let region_start = accessible.start();
    let start_rel = start.get() - region_start.get();
    let limit_rel = limit.get() - region_start.get();
    let syn = buffer_regexp_syntax_lookup(buf, region_start, match_context);
    let compiled = compile_lisp_pattern_with_posix_translation(
        pattern,
        case_fold,
        posix,
        buf.get_multibyte(),
        buffer_search_translation_table(buf, case_fold),
        &syn,
    )?;

    let search_result = with_buffer_emacs_bytes_for_search(buf, accessible.range(), |text| {
        regex_emacs::re_search(
            compiled.as_ref(),
            text,
            start_rel,
            (limit_rel - start_rel) as isize,
            &syn,
            start_rel,
        )
    });
    if search_result.is_none() && regex_emacs::take_matcher_overflow() {
        return Err(regex_emacs::MATCHER_OVERFLOW_MESSAGE.to_string());
    }
    if let Some((_pos, regs)) = search_result {
        let engine_match = buffer_engine_match_data_from_registers(&regs, region_start.get());
        let point = EmacsBytePos::new(engine_match.group(0).unwrap().end());
        Ok(Some(BufferSearchSuccess::new(buf, point, engine_match)))
    } else if noerror {
        Ok(None)
    } else {
        Err("Search failed".to_string())
    }
}

/// `re_search_forward_lisp_with_posix` for a pattern the caller already
/// compiled: the same search, no second pattern-cache probe.
pub(crate) fn re_search_forward_compiled(
    buf: &mut Buffer,
    compiled: &CompiledPattern,
    bound: Option<usize>,
    noerror: bool,
    match_context: BufferRegexpMatchContext<'_>,
) -> Result<Option<BufferSearchSuccess>, String> {
    // GNU `re-search-forward` on a buffer drives the matcher with raw buffer
    // byte positions (`PT_BYTE`, `BEGV_BYTE`, `ZV_BYTE`), even when the
    // pattern itself is a Lisp string.
    let start = buf.point_emacs_byte_pos();
    let accessible = buf.accessible_emacs_byte_region();
    let limit = bound
        .map(EmacsBytePos::new)
        .unwrap_or(accessible.end())
        .min(accessible.end());

    if start > limit {
        if noerror {
            return Ok(None);
        }
        return Err("Search failed".to_string());
    }

    let region_start = accessible.start();
    let start_rel = start.get() - region_start.get();
    let limit_rel = limit.get() - region_start.get();
    let syn = buffer_regexp_syntax_lookup(buf, region_start, match_context);

    let search_result = with_buffer_emacs_bytes_for_search(buf, accessible.range(), |text| {
        regex_emacs::re_search(
            compiled,
            text,
            start_rel,
            (limit_rel - start_rel) as isize,
            &syn,
            start_rel,
        )
    });
    if search_result.is_none() && regex_emacs::take_matcher_overflow() {
        return Err(regex_emacs::MATCHER_OVERFLOW_MESSAGE.to_string());
    }
    if let Some((_pos, regs)) = search_result {
        let engine_match = buffer_engine_match_data_from_registers(&regs, region_start.get());
        let point = EmacsBytePos::new(engine_match.group(0).unwrap().end());
        Ok(Some(BufferSearchSuccess::new(buf, point, engine_match)))
    } else if noerror {
        Ok(None)
    } else {
        Err("Search failed".to_string())
    }
}

pub(crate) fn re_search_backward_lisp_with_posix(
    buf: &mut Buffer,
    pattern: &LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
    posix: bool,
    match_context: BufferRegexpMatchContext<'_>,
) -> Result<Option<BufferSearchSuccess>, String> {
    // GNU `re-search-backward` likewise uses buffer byte positions
    // throughout, not character positions.
    let end = buf.point_emacs_byte_pos();
    let accessible = buf.accessible_emacs_byte_region();
    let limit = bound
        .map(EmacsBytePos::new)
        .unwrap_or(accessible.start())
        .max(accessible.start());

    if end < limit {
        if noerror {
            return Ok(None);
        }
        return Err("Search failed".to_string());
    }

    let region_start = accessible.start();
    let start_rel = end.get() - region_start.get();
    let limit_rel = limit.get() - region_start.get();
    let syn = buffer_regexp_syntax_lookup(buf, region_start, match_context);
    let compiled = compile_lisp_pattern_with_posix_translation(
        pattern,
        case_fold,
        posix,
        buf.get_multibyte(),
        buffer_search_translation_table(buf, case_fold),
        &syn,
    )?;

    let search_result = with_buffer_emacs_bytes_for_search(buf, accessible.range(), |text| {
        regex_emacs::re_search(
            compiled.as_ref(),
            text,
            start_rel,
            -((start_rel - limit_rel) as isize),
            &syn,
            start_rel,
        )
    });
    if search_result.is_none() && regex_emacs::take_matcher_overflow() {
        return Err(regex_emacs::MATCHER_OVERFLOW_MESSAGE.to_string());
    }
    if let Some((_pos, regs)) = search_result {
        let engine_match = buffer_engine_match_data_from_registers(&regs, region_start.get());
        let point = EmacsBytePos::new(engine_match.group(0).unwrap().start());
        Ok(Some(BufferSearchSuccess::new(buf, point, engine_match)))
    } else if noerror {
        Ok(None)
    } else {
        Err("Search failed".to_string())
    }
}

/// Test if text after point matches PATTERN (without moving point).
///
/// Returns `true` if the regex matches starting exactly at point, and
/// updates match data.
#[allow(dead_code)] // crate-private raw search seam exercised by unit tests
pub(crate) fn looking_at(
    buf: &Buffer,
    pattern: &crate::heap_types::LispString,
    case_fold: bool,
) -> Result<Option<MatchData>, String> {
    looking_at_with_posix(buf, pattern, case_fold, false)
}

/// POSIX longest-match variant of [`looking_at`] used by
/// `posix-looking-at`. See GNU `src/search.c:Fposix_looking_at`.
#[allow(dead_code)] // crate-private raw search seam exercised by unit tests
pub(crate) fn looking_at_with_posix(
    buf: &Buffer,
    pattern: &crate::heap_types::LispString,
    case_fold: bool,
    posix: bool,
) -> Result<Option<MatchData>, String> {
    let start = buf.point_emacs_byte_pos();
    let accessible = buf.accessible_emacs_byte_region();
    if start > accessible.end() {
        return Ok(None);
    }

    let region_start = accessible.start();
    let start_rel = start.get() - region_start.get();
    let multibyte = buf.get_multibyte();
    let syn = buffer_syntax_lookup(buf);
    let compiled =
        compile_search_pattern_with_posix(&pattern_for_compile(pattern), case_fold, posix, &syn)?;

    let engine_match =
        with_buffer_emacs_bytes_for_search(buf, accessible.range(), |text| match &compiled {
            CompiledSearchPattern::Literal(literal) => {
                let tail = &text[start_rel..];
                let matched = literal_find_emacs_bytes(tail, literal, multibyte, case_fold)
                    .is_some_and(|matched| matched.start() == 0);
                if !matched {
                    return None;
                }
                let full_match = MatchGroup::new(start.get(), start.get() + literal.len());
                Some(EngineMatchData::new(gnu_single_group_vec(Some(full_match))))
            }
            CompiledSearchPattern::Emacs(cp) => {
                regex_emacs::re_match(cp.as_ref(), text, start_rel, text.len(), &syn, start_rel)
                    .map(|(_end, regs)| {
                        buffer_engine_match_data_from_registers(&regs, region_start.get())
                    })
            }
        });
    Ok(engine_match.map(|engine_match| engine_match.publish_buffer(buf)))
}

pub(crate) fn looking_at_lisp_with_posix(
    buf: &Buffer,
    pattern: &LispString,
    case_fold: bool,
    posix: bool,
    match_context: BufferRegexpMatchContext<'_>,
) -> Result<Option<MatchData>, String> {
    // GNU `Flooking_at` (`src/search.c:fast_looking_at`) operates on
    // byte offsets throughout: `BEGV_BYTE`, `PT_BYTE`, `ZV_BYTE`, and
    // the matcher's start/limit are all byte positions into the raw
    // gap-buffer text. Neomacs `buf.pt` / `buf.begv` / `buf.zv` are
    // *character* positions, so feeding them straight into the
    // byte-based regex engine breaks on any multibyte buffer — the
    // start position lands mid-UTF-8-sequence and the pattern fails
    // to match even when the char at `buf.pt` would have matched.
    let start = buf.point_emacs_byte_pos();
    let accessible = buf.accessible_emacs_byte_region();
    if start > accessible.end() {
        return Ok(None);
    }

    let region_start = accessible.start();
    let start_rel = start.get() - region_start.get();
    let syn = buffer_regexp_syntax_lookup(buf, region_start, match_context);
    let compiled = compile_lisp_pattern_with_posix_translation(
        pattern,
        case_fold,
        posix,
        buf.get_multibyte(),
        buffer_search_translation_table(buf, case_fold),
        &syn,
    )?;

    let engine_match = with_buffer_emacs_bytes_for_search(buf, accessible.range(), |text| {
        regex_emacs::re_match(
            compiled.as_ref(),
            text,
            start_rel,
            text.len(),
            &syn,
            start_rel,
        )
    })
    .map(|(_end, regs)| buffer_engine_match_data_from_registers(&regs, region_start.get()));
    Ok(engine_match.map(|engine_match| engine_match.publish_buffer(buf)))
}

/// `looking_at_lisp_with_posix` for a pattern the caller already compiled
/// (from `buffer_regexp_syntax_dependency_compiled`): the same match with no
/// second pattern-cache probe.
pub(crate) fn looking_at_compiled(
    buf: &Buffer,
    compiled: &CompiledPattern,
    match_context: BufferRegexpMatchContext<'_>,
) -> Result<Option<MatchData>, String> {
    let start = buf.point_emacs_byte_pos();
    let accessible = buf.accessible_emacs_byte_region();
    if start > accessible.end() {
        return Ok(None);
    }
    let region_start = accessible.start();
    let start_rel = start.get() - region_start.get();
    let syn = buffer_regexp_syntax_lookup(buf, region_start, match_context);
    let engine_match = with_buffer_emacs_bytes_for_search(buf, accessible.range(), |text| {
        regex_emacs::re_match(compiled, text, start_rel, text.len(), &syn, start_rel)
    })
    .map(|(_end, regs)| buffer_engine_match_data_from_registers(&regs, region_start.get()));
    Ok(engine_match.map(|engine_match| engine_match.publish_buffer(buf)))
}

/// Test whether STRING matches PATTERN starting at byte offset 0.
///
/// Returns `true` if the regex matches at the beginning of STRING and updates
/// match data using character positions, mirroring `looking-at` semantics on a
/// string-backed source.
pub fn looking_at_string(
    pattern: &str,
    string: &str,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<bool, String> {
    match compile_search_pattern(
        &crate::heap_types::LispString::from_utf8(pattern),
        case_fold,
    )? {
        CompiledSearchPattern::Literal(literal) => {
            let matched = literal_find_emacs_bytes(string.as_bytes(), &literal, true, case_fold)
                .is_some_and(|matched| matched.start() == 0);
            if !matched {
                return Ok(false);
            }
            *match_data = Some(string_char_match_data(
                SearchedString::Owned(LispString::from_utf8(string)),
                single_group_match_data(0, literal.len()),
            ));
            Ok(true)
        }
        CompiledSearchPattern::Emacs(cp) => {
            let syn = DefaultSyntaxLookup;
            let text_bytes = string.as_bytes();
            if let Some((_end, regs)) =
                regex_emacs::re_match(cp.as_ref(), text_bytes, 0, text_bytes.len(), &syn, 0)
            {
                let byte_md = engine_match_data_from_registers(&regs, 0);
                *match_data = Some(string_char_match_data(
                    SearchedString::Owned(LispString::from_utf8(string)),
                    byte_md,
                ));
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }
}

/// Match a regex against a string (not a buffer).
///
/// `start` is the byte offset within `string` to begin matching.
/// Returns the CHARACTER position of the start of the match (relative
/// to the whole string, not `start`), or `None` if no match.
/// Updates match data with capture groups in CHARACTER positions;
/// stores the searched string.
pub fn string_match_full_with_case_fold(
    pattern: &str,
    string: &str,
    start: usize,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    string_match_full_with_case_fold_and_posix(pattern, string, start, case_fold, false, match_data)
}

/// POSIX longest-match variant of [`string_match_full_with_case_fold`]
/// used by `posix-string-match`. See GNU `src/search.c:Fposix_string_match`.
pub fn string_match_full_with_case_fold_and_posix(
    pattern: &str,
    string: &str,
    start: usize,
    case_fold: bool,
    posix: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    string_match_full_with_case_fold_source_posix(
        pattern,
        string,
        SearchedString::Owned(LispString::from_utf8(string)),
        start,
        case_fold,
        posix,
        match_data,
    )
}

/// Engine-test convenience: match under [`DefaultSyntaxLookup`]. Production
/// string matches must carry their syntax state -- the `string-match`
/// builtins thread the current buffer's tables explicitly, and the internal
/// `fast_string_match` analogues go through
/// `crate::emacs_core::builtins::search::FastStringMatchSyntax` -- so this
/// bypass exists only for tests of the matcher itself.
#[cfg(test)]
pub(crate) fn string_match_full_with_case_fold_source_lisp(
    pattern: &str,
    string: &crate::heap_types::LispString,
    searched_string: SearchedString,
    start: usize,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    string_match_full_with_case_fold_source_lisp_posix(
        pattern,
        string,
        searched_string,
        start,
        case_fold,
        false,
        match_data,
    )
}

/// POSIX longest-match variant of
/// [`string_match_full_with_case_fold_source_lisp`]; same test-only bypass.
#[cfg(test)]
pub(crate) fn string_match_full_with_case_fold_source_lisp_posix(
    pattern: &str,
    string: &crate::heap_types::LispString,
    searched_string: SearchedString,
    start: usize,
    case_fold: bool,
    posix: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    let pattern = LispString::from_utf8(pattern);
    string_match_full_with_case_fold_source_lisp_pattern_posix(
        &pattern,
        string,
        searched_string,
        start,
        case_fold,
        posix,
        match_data,
    )
}

/// Engine-test convenience: see
/// [`string_match_full_with_case_fold_source_lisp`] for why this
/// [`DefaultSyntaxLookup`] bypass is test-only.
#[cfg(test)]
pub(crate) fn string_match_full_with_case_fold_source_lisp_pattern_posix(
    pattern: &LispString,
    string: &crate::heap_types::LispString,
    searched_string: SearchedString,
    start: usize,
    case_fold: bool,
    posix: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    string_match_full_with_case_fold_source_lisp_pattern_posix_syntax(
        pattern,
        string,
        searched_string,
        start,
        case_fold,
        posix,
        None,
        &DefaultSyntaxLookup,
        match_data,
    )
}

#[cfg(test)] // production match-data callers commit through StringSearchSuccess
#[allow(clippy::too_many_arguments)] // matching options remain explicit at the GNU-regexp boundary
pub(crate) fn string_match_full_with_case_fold_source_lisp_pattern_posix_syntax(
    pattern: &LispString,
    string: &crate::heap_types::LispString,
    searched_string: SearchedString,
    start: usize,
    case_fold: bool,
    posix: bool,
    translation_table: Option<super::value::Value>,
    syntax: &dyn SyntaxLookup,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    let success = string_search_full_with_case_fold_source_lisp_pattern_posix_syntax(
        pattern,
        string,
        searched_string,
        start,
        case_fold,
        posix,
        translation_table,
        syntax,
    )?;
    Ok(success.map(|success| {
        let (start, published_match_data) = success.into_parts();
        *match_data = Some(published_match_data);
        start.get()
    }))
}

#[allow(clippy::too_many_arguments)] // matching options remain explicit at the GNU-regexp boundary
pub(crate) fn string_search_full_with_case_fold_source_lisp_pattern_posix_syntax(
    pattern: &LispString,
    string: &crate::heap_types::LispString,
    searched_string: SearchedString,
    start: usize,
    case_fold: bool,
    posix: bool,
    translation_table: Option<super::value::Value>,
    syntax: &dyn SyntaxLookup,
) -> Result<Option<StringSearchSuccess>, String> {
    if start > string.byte_len() {
        return Ok(None);
    }

    let compiled = compile_lisp_pattern_with_posix_translation(
        pattern,
        case_fold,
        posix,
        string.is_multibyte(),
        translation_table,
        syntax,
    )?;
    let text_bytes = string.as_bytes();
    let range = (text_bytes.len() - start) as isize;
    if let Some((_pos, regs)) = regex_emacs::re_search(
        compiled.as_ref(),
        text_bytes,
        start,
        range,
        syntax,
        STRING_MATCH_AT_DOT_UNREACHABLE,
    ) {
        let byte_md = engine_match_data_from_registers(&regs, 0);
        let char_md = string_char_match_data(searched_string, byte_md);
        Ok(Some(StringSearchSuccess::new(char_md)))
    } else if regex_emacs::take_matcher_overflow() {
        Err(regex_emacs::MATCHER_OVERFLOW_MESSAGE.to_string())
    } else {
        Ok(None)
    }
}

pub(crate) fn string_match_full_with_case_fold_source_posix(
    pattern: &str,
    string: &str,
    searched_string: SearchedString,
    start: usize,
    case_fold: bool,
    posix: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    let success = string_search_full_with_case_fold_source_posix(
        pattern,
        string,
        searched_string,
        start,
        case_fold,
        posix,
    )?;
    Ok(success.map(|success| {
        let (start, published_match_data) = success.into_parts();
        *match_data = Some(published_match_data);
        start.get()
    }))
}

pub(crate) fn string_search_full_with_case_fold_and_posix(
    pattern: &str,
    string: &str,
    start: usize,
    case_fold: bool,
    posix: bool,
) -> Result<Option<StringSearchSuccess>, String> {
    string_search_full_with_case_fold_source_posix(
        pattern,
        string,
        SearchedString::Owned(LispString::from_utf8(string)),
        start,
        case_fold,
        posix,
    )
}

fn string_search_full_with_case_fold_source_posix(
    pattern: &str,
    string: &str,
    searched_string: SearchedString,
    start: usize,
    case_fold: bool,
    posix: bool,
) -> Result<Option<StringSearchSuccess>, String> {
    if start > string.len() {
        return Ok(None);
    }

    string_search_full_with_case_fold_source_compiled_syntax(
        compile_search_pattern_with_posix(
            &crate::heap_types::LispString::from_utf8(pattern),
            case_fold,
            posix,
            &DefaultSyntaxLookup,
        )?,
        string,
        searched_string,
        start,
        case_fold,
        &DefaultSyntaxLookup,
    )
}

fn string_search_full_with_case_fold_source_compiled_syntax(
    compiled: CompiledSearchPattern,
    string: &str,
    searched_string: SearchedString,
    start: usize,
    _case_fold: bool,
    syntax: &dyn SyntaxLookup,
) -> Result<Option<StringSearchSuccess>, String> {
    match compiled {
        CompiledSearchPattern::Literal(literal) => {
            let byte_match =
                literal_find_emacs_bytes(&string.as_bytes()[start..], &literal, true, _case_fold)
                    .map(|matched| matched.shift(start));
            if let Some(byte_match) = byte_match {
                let char_md = string_char_match_data(
                    searched_string,
                    single_group_match_data(byte_match.start(), byte_match.end()),
                );
                Ok(Some(StringSearchSuccess::new(char_md)))
            } else {
                Ok(None)
            }
        }
        CompiledSearchPattern::Emacs(cp) => {
            let text_bytes = string.as_bytes();
            let range = (text_bytes.len() - start) as isize;
            if let Some((_pos, regs)) = regex_emacs::re_search(
                cp.as_ref(),
                text_bytes,
                start,
                range,
                syntax,
                STRING_MATCH_AT_DOT_UNREACHABLE,
            ) {
                let byte_md = engine_match_data_from_registers(&regs, 0);
                let char_md = string_char_match_data(searched_string, byte_md);
                Ok(Some(StringSearchSuccess::new(char_md)))
            } else if regex_emacs::take_matcher_overflow() {
                Err(regex_emacs::MATCHER_OVERFLOW_MESSAGE.to_string())
            } else {
                Ok(None)
            }
        }
    }
}

/// Match a regex against a string using Emacs default case-fold behavior.
pub fn string_match_full(
    pattern: &str,
    string: &str,
    start: usize,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    string_match_full_with_case_fold(pattern, string, start, true, match_data)
}

/// Replace the last match in a buffer and return `nil`-style success.
#[cfg(test)]
pub fn replace_match_buffer(
    buf: &mut Buffer,
    newtext: &str,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<MatchData>,
) -> Result<(), String> {
    replace_match_buffer_with_syntax(buf, newtext, fixedcase, literal, subexp, match_data, false)
}

/// Variant that also honors `case-symbols-as-words` for the
/// `fixedcase=nil` path. Mirrors GNU `src/search.c:2485,2494,2504`;
/// the buffer's own syntax table is always consulted via `buf`.
/// See audit findings #14/#20 in `drafts/regex-search-audit.md`.
#[cfg(test)]
pub fn replace_match_buffer_with_syntax(
    buf: &mut Buffer,
    newtext: &str,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<MatchData>,
    case_symbols_as_words: bool,
) -> Result<(), String> {
    let (match_start, match_end, replacement) = compute_buffer_replacement_with_syntax(
        buf,
        newtext,
        fixedcase,
        literal,
        subexp,
        match_data,
        case_symbols_as_words,
    )?;

    let match_range =
        EmacsByteRange::new(EmacsBytePos::new(match_start), EmacsBytePos::new(match_end));
    buf.replace_emacs_byte_range_lisp_string(match_range, &replacement);
    Ok(())
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn compute_buffer_replacement_with_syntax(
    buf: &Buffer,
    newtext: &str,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<MatchData>,
    case_symbols_as_words: bool,
) -> Result<(usize, usize, crate::heap_types::LispString), String> {
    let md = match match_data {
        Some(md) => md,
        None => return Err(REPLACE_MATCH_SUBEXP_MISSING.to_string()),
    };

    // Faithful Emacs-bytes view of the whole buffer; the replace core now
    // indexes/slices it by Emacs-byte offsets directly (issue #131).
    let source = buf.buffer_substring_bytes_range(buf.full_emacs_byte_range());
    let buf_syntax = crate::emacs_core::syntax::SyntaxTable::for_buffer(buf);
    if md.group(subexp).is_none() {
        return Err(REPLACE_MATCH_SUBEXP_MISSING.to_string());
    }
    let char_range = md
        .group_zero_based_char_range(subexp)
        .expect("group existence checked above");
    let buffer_start = buf
        .char_pos_to_emacs_byte_pos_clamped(char_range.start())
        .get();
    let buffer_end = buf
        .char_pos_to_emacs_byte_pos_clamped(char_range.end())
        .get();

    let (_byte_start, _byte_end, replacement_bytes) = compute_replacement_with_syntax(
        newtext,
        fixedcase,
        literal,
        subexp,
        match_data,
        &source,
        buf.get_multibyte(),
        Some(&buf_syntax),
        case_symbols_as_words,
    )?;
    let replacement = if buf.get_multibyte() {
        crate::heap_types::LispString::from_emacs_bytes(replacement_bytes)
    } else {
        crate::heap_types::LispString::from_unibyte(replacement_bytes)
    };

    Ok((buffer_start, buffer_end, replacement))
}

/// Replace the last match in SOURCE (Emacs-bytes) and return the resulting
/// Emacs-bytes. `source_multibyte` mirrors the source Lisp string's
/// `STRING_MULTIBYTE` flag and governs how the result is reassembled.
pub fn replace_match_string(
    source: &[u8],
    source_multibyte: bool,
    newtext: &str,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<MatchData>,
) -> Result<Vec<u8>, String> {
    replace_match_string_with_syntax(
        source,
        source_multibyte,
        newtext,
        fixedcase,
        literal,
        subexp,
        match_data,
        None,
        false,
    )
}

/// Variant of [`replace_match_string`] that threads the syntax table
/// and `case-symbols-as-words` into the case-preservation decision.
/// For pure string replacement (no buffer in scope), pass `None` for
/// the table to get GNU's standard-table baseline behavior.
#[allow(clippy::too_many_arguments)] // public replacement options mirror Lisp replace-match semantics
pub fn replace_match_string_with_syntax(
    source: &[u8],
    source_multibyte: bool,
    newtext: &str,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<MatchData>,
    syntax_table: Option<&crate::emacs_core::syntax::SyntaxTable>,
    case_symbols_as_words: bool,
) -> Result<Vec<u8>, String> {
    let (byte_start, byte_end, replacement) = compute_replacement_with_syntax(
        newtext,
        fixedcase,
        literal,
        subexp,
        match_data,
        source,
        source_multibyte,
        syntax_table,
        case_symbols_as_words,
    )?;
    if byte_end > source.len() || byte_start > byte_end {
        return Err(REPLACE_MATCH_SUBEXP_MISSING.to_string());
    }
    let mut out = Vec::with_capacity(byte_start + replacement.len() + (source.len() - byte_end));
    out.extend_from_slice(&source[..byte_start]);
    out.extend_from_slice(&replacement);
    out.extend_from_slice(&source[byte_end..]);
    Ok(out)
}

/// Convert a character position to a byte offset in a string.
pub fn char_pos_to_byte(s: &str, char_pos: usize) -> usize {
    s.char_indices()
        .nth(char_pos)
        .map(|(byte_pos, _)| byte_pos)
        .unwrap_or(s.len())
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn compute_replacement(
    newtext: &str,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<MatchData>,
    source: &[u8],
    source_multibyte: bool,
) -> Result<(usize, usize, Vec<u8>), String> {
    compute_replacement_with_syntax(
        newtext,
        fixedcase,
        literal,
        subexp,
        match_data,
        source,
        source_multibyte,
        None,
        false,
    )
}

/// Variant of [`compute_replacement`] that also threads a syntax
/// table and the `case-symbols-as-words` flag into
/// `apply_replace_match_case`.
///
/// GNU `src/search.c:2485-2505` checks `SYNTAX(prevc) == Sword` (or
/// `Ssymbol` when `case-symbols-as-words` is non-nil) on the buffer
/// syntax table. Audit findings #14 and #20 in
/// `drafts/regex-search-audit.md` track neomacs's divergence; this
/// helper is the threading point callers must hit to keep parity.
///
/// # Multibyte / unibyte handling (audit #13, issue #131)
///
/// GNU `src/search.c:2622-2720` runs an explicit byte conversion
/// loop over the replacement, branching on both the replacement
/// string's representation and the target buffer's
/// `enable-multibyte-characters` flag.
///
/// `source` is now the faithful Emacs-bytes view of the searched
/// text and is indexed/sliced directly by Emacs-byte offsets. The
/// match-group positions are converted to Emacs-byte offsets the same
/// way the matcher decodes characters (`emacs_char::string_char`), so
/// eight-bit raw bytes and Private-Use-Area glyphs survive intact
/// instead of round-tripping through the legacy PUA-sentinel storage
/// form.
#[allow(clippy::too_many_arguments)] // private worker carries the same explicit replacement semantics
fn compute_replacement_with_syntax(
    newtext: &str,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<MatchData>,
    source: &[u8],
    source_multibyte: bool,
    syntax_table: Option<&crate::emacs_core::syntax::SyntaxTable>,
    case_symbols_as_words: bool,
) -> Result<(usize, usize, Vec<u8>), String> {
    use crate::emacs_core::emacs_char::char_to_byte_pos;

    let md = match match_data {
        Some(md) => md,
        None => return Err(REPLACE_MATCH_SUBEXP_MISSING.to_string()),
    };

    if md.group(subexp).is_none() {
        return Err(REPLACE_MATCH_SUBEXP_MISSING.to_string());
    }
    let char_range = md
        .group_zero_based_char_range(subexp)
        .expect("group existence checked above");
    let byte_start = char_to_byte_pos(source, char_range.start().get());
    let byte_end = char_to_byte_pos(source, char_range.end().get());

    if byte_end > source.len() || byte_start > byte_end {
        return Err(REPLACE_MATCH_SUBEXP_MISSING.to_string());
    }

    let mut replacement = if literal {
        newtext.as_bytes().to_vec()
    } else {
        build_replacement(newtext, md, source)?
    };

    if !fixedcase {
        let matched = &source[byte_start..byte_end];
        replacement = apply_match_case_with_syntax(
            replacement,
            matched,
            source_multibyte,
            syntax_table,
            case_symbols_as_words,
        );
    }

    Ok((byte_start, byte_end, replacement))
}

/// Build a replacement string handling `\&` (whole match) and
/// `\N` (group N, 1-9 only).
///
/// Error semantics mirror GNU `src/search.c:2545-2714` exactly:
///
/// - `\&` → the whole match (`md.groups[0]`). See search.c:2560
///   and search.c:2701.
/// - `\1`..`\9` → the Nth subgroup. `\0` is NOT accepted: GNU's
///   `Freplace_match` loop at search.c:2565 explicitly checks
///   `c >= '1' && c <= '9'`, mirrored at search.c:2703. Any `\0`
///   falls into the `"Invalid use of \\ in replacement text"`
///   error branch at search.c:2584 and 2713. This was audit
///   finding #11 in `drafts/regex-search-audit.md`: before this
///   fix, our `'0'..='9'` range accepted `\0` and returned the
///   whole match.
/// - `\\` → a literal backslash (search.c:2581-2582 and 2708-2709).
/// - `\?` → GNU's string path at search.c:2583 has an explicit
///   `else if (c != '?')` exception: when `c == '?'` neither
///   `substart >= 0` nor `delbackslash` is set, so `lastpos`
///   doesn't advance and the `\?` bytes fall through into the
///   next "middle" copy, effectively emitting the literal `\?`.
///   We mirror that here for both code paths (buffer/string).
/// - Any other `\X` → the same "Invalid use of `\\' in replacement
///   text" error. This was audit finding #12: before this fix, our
///   catch-all silently emitted the literal `\X`.
///
/// The caller (`compute_replacement`) propagates the error; the
/// outer search builtin signals a Lisp error with the GNU-shaped
/// message.
fn build_replacement(template: &str, md: &MatchData, source: &[u8]) -> Result<Vec<u8>, String> {
    use crate::emacs_core::emacs_char::char_to_byte_pos;

    const INVALID_BACKSLASH_MSG: &str = "Invalid use of `\\' in replacement text";

    fn next_char_at(s: &str, byte_idx: usize) -> Option<(char, usize)> {
        s.get(byte_idx..)
            .and_then(|tail| tail.chars().next().map(|ch| (ch, ch.len_utf8())))
    }

    /// Extract the matched group's Emacs-bytes from `source`.
    fn extract_group<'a>(source: &'a [u8], md: &MatchData, group: usize) -> Option<&'a [u8]> {
        let range = md.group_zero_based_char_range(group)?;
        let bs = char_to_byte_pos(source, range.start().get());
        let be = char_to_byte_pos(source, range.end().get());
        if be <= source.len() && bs <= be {
            Some(&source[bs..be])
        } else {
            None
        }
    }

    let mut out: Vec<u8> = Vec::with_capacity(template.len());
    let bytes = template.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'\\' && i + 1 < len {
            let (next, next_len) =
                next_char_at(template, i + 1).expect("byte index must be char boundary");
            match next {
                '&' => {
                    // Whole match
                    if let Some(text) = extract_group(source, md, 0) {
                        out.extend_from_slice(text);
                    }
                    i += 1 + next_len;
                }
                '1'..='9' => {
                    // GNU search.c:2549 — explicit `c >= '1' && c <= '9'`.
                    // `\0` intentionally falls through to the error arm.
                    let group = (next as u8 - b'0') as usize;
                    if let Some(text) = extract_group(source, md, group) {
                        out.extend_from_slice(text);
                    }
                    i += 1 + next_len;
                }
                '\\' => {
                    // GNU search.c:2581-2582, 2708-2709.
                    out.push(b'\\');
                    i += 1 + next_len;
                }
                '?' => {
                    // GNU search.c:2583 `else if (c != '?')`.
                    // `\?` is passed through literally in the
                    // string path; we honor that for both paths.
                    out.push(b'\\');
                    out.push(b'?');
                    i += 1 + next_len;
                }
                _ => {
                    // GNU search.c:2584, 2713 — any other backslash
                    // sequence (`\0`, `\n`, `\X`, …) signals an
                    // `error ("Invalid use of `\\' in replacement
                    // text")`.
                    return Err(INVALID_BACKSLASH_MSG.to_string());
                }
            }
        } else {
            // Template bytes are UTF-8 (valid Emacs-bytes); copy verbatim.
            out.push(bytes[i]);
            i += 1;
        }
    }

    Ok(out)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn apply_match_case(replacement: &str, matched: &str) -> String {
    apply_replace_match_case(replacement, matched)
}

/// Byte-faithful case preservation for the replace core (issue #131).
///
/// `replacement` and `matched` are Emacs-bytes; `source_multibyte` mirrors the
/// searched text's `STRING_MULTIBYTE` flag so eight-bit raw bytes and
/// Private-Use-Area glyphs are analyzed/cased through the LispString case
/// primitives instead of the legacy PUA-sentinel storage form.
fn apply_match_case_with_syntax(
    replacement: Vec<u8>,
    matched: &[u8],
    source_multibyte: bool,
    syntax_table: Option<&crate::emacs_core::syntax::SyntaxTable>,
    case_symbols_as_words: bool,
) -> Vec<u8> {
    use crate::emacs_core::casefiddle::{
        apply_replace_match_case_lisp, apply_replace_match_case_lisp_with,
    };
    use crate::emacs_core::syntax::SyntaxClass;
    use crate::heap_types::LispString;

    let make_lisp = |bytes: Vec<u8>| {
        if source_multibyte {
            LispString::from_emacs_bytes(bytes)
        } else {
            LispString::from_unibyte(bytes)
        }
    };
    let replacement_lisp = make_lisp(replacement);
    let matched_lisp = make_lisp(matched.to_vec());

    let result = match syntax_table {
        None => apply_replace_match_case_lisp(&replacement_lisp, &matched_lisp),
        Some(table) => {
            apply_replace_match_case_lisp_with(&replacement_lisp, &matched_lisp, move |ch| {
                let class = table.char_syntax(ch);
                class == SyntaxClass::Word
                    || (case_symbols_as_words && class == SyntaxClass::Symbol)
            })
        }
    };
    result.as_bytes().to_vec()
}

/// Debug-only publish/read counters sizing the lazy-match-data redesign:
/// how many group endpoints are byte->char converted at publish vs how many
/// are ever read back. Read by the ignored probe test
/// `match_publish_read_stats_probe`; absent from optimized builds.
#[cfg(debug_assertions)]
pub(crate) mod match_stats {
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub(crate) static PUBLISHES: AtomicUsize = AtomicUsize::new(0);
    pub(crate) static PUBLISHED_GROUP0: AtomicUsize = AtomicUsize::new(0);
    pub(crate) static PUBLISHED_SUB: AtomicUsize = AtomicUsize::new(0);
    pub(crate) static READ_GROUP0: AtomicUsize = AtomicUsize::new(0);
    pub(crate) static READ_SUB: AtomicUsize = AtomicUsize::new(0);
    pub(crate) static FULL_EXPORTS: AtomicUsize = AtomicUsize::new(0);

    pub(crate) fn count_publish(groups: &[Option<super::EmacsByteRange>]) {
        PUBLISHES.fetch_add(1, Ordering::Relaxed);
        let live = groups.iter().filter(|g| g.is_some()).count();
        if !groups.is_empty() && groups[0].is_some() {
            PUBLISHED_GROUP0.fetch_add(1, Ordering::Relaxed);
            PUBLISHED_SUB.fetch_add(live - 1, Ordering::Relaxed);
        } else {
            PUBLISHED_SUB.fetch_add(live, Ordering::Relaxed);
        }
    }

    pub(crate) fn count_group_read(index: usize) {
        if index == 0 {
            READ_GROUP0.fetch_add(1, Ordering::Relaxed);
        } else {
            READ_SUB.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn count_full_export() {
        FULL_EXPORTS.fetch_add(1, Ordering::Relaxed);
    }

    /// Distinct converted-group demand: bump once per (match-data, group)
    /// pair whose Some value a reader actually saw. Under lazy conversion
    /// exactly these groups would still convert.
    pub(crate) static FIRST_SOME_G0: AtomicUsize = AtomicUsize::new(0);
    pub(crate) static FIRST_SOME_SUB: AtomicUsize = AtomicUsize::new(0);

    pub(crate) fn count_first_some_read(mask: &std::cell::Cell<u64>, index: usize) {
        if index >= 64 {
            return;
        }
        let bits = mask.get();
        let bit = 1u64 << index;
        if bits & bit == 0 {
            mask.set(bits | bit);
            if index == 0 {
                FIRST_SOME_G0.fetch_add(1, Ordering::Relaxed);
            } else {
                FIRST_SOME_SUB.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub(crate) fn reset() {
        for counter in [
            &PUBLISHES,
            &PUBLISHED_GROUP0,
            &PUBLISHED_SUB,
            &READ_GROUP0,
            &READ_SUB,
            &FULL_EXPORTS,
            &FIRST_SOME_G0,
            &FIRST_SOME_SUB,
        ] {
            counter.store(0, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
