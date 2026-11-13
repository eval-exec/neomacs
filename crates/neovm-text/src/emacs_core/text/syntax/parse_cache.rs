//! The syntax parse cache (P3.4): `parse-partial-sexp` answers from recorded
//! loop states of earlier scans, exactly.
//!
//! `parse-partial-sexp` is a pure function of the text, the syntax table, the
//! `syntax-table` properties, a few variables, FROM, the starting state, the
//! options and TO. Its scan loop is deterministic and resumable at any loop top
//! (`parse_loop`), so an earlier scan's loop state at `p` is the state a new
//! scan with the same inputs reaches at `p`, for any TO after `p`. The cache
//! keeps such states per starting key and never answers from one whose inputs
//! may have changed.
//!
//! This file holds the cache and its invalidation. Every change to what a scan
//! reads reaches it:
//!
//! * text: the four measured mutators note the edit's first byte in the
//!   buffer substrate's [`SyntaxMutationNotes`](neovm_edit::buffer::syntax_cache::SyntaxMutationNotes);
//!   wholesale replacements, multibyte and backend conversions clear it;
//! * `syntax-table` / `category` properties: every property-table mutation that
//!   moves the table's syntax tick notes the first character it can touch in
//!   the same notes; a swapped-in table clears it;
//! * everything else a scan reads (the syntax table and its char-table write
//!   tick, BEGV, `parse-sexp-lookup-properties`, `comment-end-can-be-escaped`,
//!   multibyteness) is part of each query's key.
//!
//! Notes are counted: before each use, a text epoch or syntax tick that moved
//! further than the notes account for (a mutation path that bypassed them)
//! clears everything, so a missed hook costs hits, never answers.
//!
//! The recorded runs live in an opaque slot on the buffer text (stage-3 C7).
//! The substrate owns the invalidation notes and cannot name this cache, so
//! the slot is `Option<Box<dyn Any>>` and this module downcasts it. A snapshot
//! clone gets a fresh id and an empty slot (P3.0 §3.9). The cache holds
//! positions and parse states, and descriptor conses as bits (below): no
//! `Value` it keeps alive, nothing to trace.
//!
//! # L1: runs, snapshots and exact answers
//!
//! A run is keyed by everything a scan from FROM reads besides the text and
//! properties: FROM, the internalized starting state, the options and the
//! environment ([`EnvKey`]). It holds loop-top states strictly after FROM
//! (one per `NEOVM_SYNTAX_PARSE_CACHE_CHUNK` characters, and the last loop top
//! before each query's TO, which chains `syntax-ppss`'s repeated queries from
//! one old position), the last few exact answers, and the descriptor conses
//! the recording scans read. A query with the same key resumes from the
//! greatest state before its TO, or answers an identical TO outright; a scan
//! that finds nothing records a run when it spans
//! `NEOVM_SYNTAX_PARSE_CACHE_MIN_SPAN` characters or more.
//!
//! A descriptor cons can be changed in place (`setcar`) without any tick
//! moving, and GNU would see it on its next scan. A recording scan logs the
//! descriptor conses it decodes -- `syntax-table` property values, and the
//! syntax table's entries for non-ASCII characters -- and every reuse first
//! compares the car and cdr of each one the reused state read with what the
//! recording scan saw. A logged cons is alive whenever it is compared: a
//! property value while its interval still holds it, which the property notes
//! guarantee for every position a surviving state covers; a table entry
//! while the table holds it, which the key's table identity and write tick
//! guarantee. A property value that is itself a syntax table cannot be
//! validated this way and is not reused past its position.
//!
//! The ASCII entries of the syntax table are trusted by the table's identity
//! and `char_table_write_tick`, exactly as the flat ASCII classifiers every
//! scan uses already trust them.
//!
//! Bypassed: a query that may run `syntax-propertize` (it never reaches the
//! cache), and a buffer whose properties resolve through `category` symbols,
//! `char-property-alias-alist` or `default-text-properties` (Lisp structure no
//! note observes).
//!
//! `NEOVM_SYNTAX_PARSE_CACHE`: `1`/`on` (default), `0`/`off`, or `verify`
//! (every cached answer is recomputed by a plain scan and compared; a mismatch
//! is logged, counted, fails debug builds, and the plain answer is returned).
//!
//! # L2: canonical run
//!
//! `NEOVM_SYNTAX_PARSE_CACHE_L2`: `0`/`off` (default), `1`/`on`, or `verify`.
//! With L1 enabled, option-free queries can use the BEGV canonical run
//! (`canon`) after an L1 exact miss. Either layer's verify mode compares a
//! cached answer with the same original query, including all its options.
//!
//! # Mutator ownership
//!
//! Runs live in the buffer text's opaque slot. The slot is taken out for a
//! query and stored back when the query returns, so the storage borrow is not
//! held across the scan. Takes never span Lisp, so a nested take cannot
//! happen. A storage drop drops the slot with it. Indirect buffers share the storage,
//! so they share the runs, and `buffer-swap-text` moves the slot with the
//! text. Future mutators sharing one text storage must serialize text,
//! property, syntax-table and descriptor access across validation, scanning
//! and publication. Process-wide knobs contain only scalar policy, and
//! relaxed publication carries no Lisp state. Existing thread-local counters
//! are diagnostics, not cached answers.

use super::parse_loop::{
    Entry, LoopState, LoopTop, Plain, ScanEnd, ScanFinish, ScanMode, TopAction, run_parse_loop,
};
use super::*;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

#[cfg(not(any(test, feature = "test-support")))]
#[path = "parse_canon.rs"]
mod canon;
#[cfg(any(test, feature = "test-support"))]
#[path = "parse_canon.rs"]
pub mod canon;
pub(crate) use canon::{CanonMode, canon_mode};

#[cfg_attr(feature = "test-support", visibility::make(pub))]
/// `NEOVM_SYNTAX_PARSE_CACHE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParseCacheMode {
    /// Every scan runs from FROM.
    Off,
    /// Cached answers are served.
    On,
    /// Cached answers are computed, recomputed by a plain scan and compared;
    /// a mismatch is reported and the plain answer returned.
    Verify,
}

const MODE_UNREAD: u8 = 0;
const MODE_OFF: u8 = 1;
const MODE_ON: u8 = 2;
const MODE_VERIFY: u8 = 3;

static MODE: AtomicU8 = AtomicU8::new(MODE_UNREAD);

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    /// Test override of `NEOVM_SYNTAX_PARSE_CACHE`.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) static MODE_OVERRIDE: std::cell::Cell<Option<ParseCacheMode>> =
        const { std::cell::Cell::new(None) };
}

/// The cache mode, read once from `NEOVM_SYNTAX_PARSE_CACHE`.
#[inline]
pub(crate) fn parse_cache_mode() -> ParseCacheMode {
    // Same rule as the override's own gate: moved text tests set it through
    // the test-support surface, so the read must exist there too.
    #[cfg(any(test, feature = "test-support"))]
    if let Some(mode) = MODE_OVERRIDE.with(std::cell::Cell::get) {
        return mode;
    }
    match MODE.load(Ordering::Relaxed) {
        MODE_OFF => ParseCacheMode::Off,
        MODE_ON => ParseCacheMode::On,
        MODE_VERIFY => ParseCacheMode::Verify,
        _ => read_parse_cache_knob(),
    }
}

#[cold]
#[inline(never)]
fn read_parse_cache_knob() -> ParseCacheMode {
    let mode = parse_parse_cache_knob(std::env::var("NEOVM_SYNTAX_PARSE_CACHE").ok().as_deref());
    MODE.store(
        match mode {
            ParseCacheMode::Off => MODE_OFF,
            ParseCacheMode::On => MODE_ON,
            ParseCacheMode::Verify => MODE_VERIFY,
        },
        Ordering::Relaxed,
    );
    tracing::debug!(?mode, "NEOVM_SYNTAX_PARSE_CACHE read");
    mode
}

pub(crate) fn parse_parse_cache_knob(value: Option<&str>) -> ParseCacheMode {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("0" | "off" | "no" | "false" | "nil") => ParseCacheMode::Off,
        Some("verify") => ParseCacheMode::Verify,
        _ => ParseCacheMode::On,
    }
}

pub(crate) use neovm_edit::buffer::syntax_cache::Invalidation;

#[cfg(not(any(test, feature = "test-support")))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
/// Per-storage syntax parse cache. See the module documentation.
#[derive(Debug, Default)]
pub(crate) struct SyntaxParseCache {
    runs: Vec<ParseRun>,
    /// The BEGV canonical run, under the same storage-owned exclusive cache
    /// access as L1 runs; it is never stored in a mutator thread-local cache.
    canonical: Option<canon::CanonicalRun>,
    /// A conservative membership filter for numeric FROM positions. A clear
    /// bit proves a miss; collisions still require an exact run search.
    from_filter: u64,
    /// LRU clock.
    clock: u64,
}
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
/// Per-storage syntax parse cache. See the module documentation.
#[derive(Debug, Default)]
pub struct SyntaxParseCache {
    pub(crate) runs: Vec<ParseRun>,
    /// The BEGV canonical run, under the same storage-owned exclusive cache
    /// access as L1 runs; it is never stored in a mutator thread-local cache.
    pub(crate) canonical: Option<canon::CanonicalRun>,
    /// A conservative membership filter for numeric FROM positions. A clear
    /// bit proves a miss; collisions still require an exact run search.
    from_filter: u64,
    /// LRU clock.
    clock: u64,
}

impl SyntaxParseCache {
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Borrowed recorded runs (VIS read arm, lead #1052).  The field is
    /// `pub(crate)` in every arm; these getters are the only cross-crate
    /// read routes.
    pub(crate) fn runs_for_test(&self) -> &[ParseRun] {
        &self.runs
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Borrowed BEGV canonical run (VIS read arm, lead #1052).
    pub(crate) fn canonical_for_test(&self) -> Option<&canon::CanonicalRun> {
        self.canonical.as_ref()
    }
}

impl SyntaxParseCache {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// A coarse lookup using only numeric FROM positions. A matching run may
    /// still be invalid or have different options; a real lookup must drain
    /// mutation notes and validate the complete key before using any state.
    #[inline]
    pub(crate) fn has_run_from(&self, from_char: usize) -> bool {
        self.from_filter & from_filter_bit(from_char) != 0
            && self.runs.iter().any(|run| run.key.from_char == from_char)
    }

    fn rebuild_from_filter(&mut self) {
        self.from_filter = self
            .runs
            .iter()
            .fold(0, |mask, run| mask | from_filter_bit(run.key.from_char));
    }

    /// Apply the substrate's invalidation notes to the recorded runs. The
    /// notes themselves live in the buffer text storage; what arrives here is
    /// the plain outcome (stage-3 C7).
    pub(crate) fn apply_invalidation(&mut self, invalidation: Invalidation) {
        match invalidation {
            Invalidation::Nothing => {}
            Invalidation::All => {
                self.runs.clear();
                self.canonical = None;
                self.from_filter = 0;
            }
            Invalidation::From { byte, char } => {
                for run in &mut self.runs {
                    run.truncate(byte, char);
                }
                if let Some(canonical) = &mut self.canonical {
                    canonical.truncate(byte, char);
                }
                let before = self.runs.len();
                self.runs.retain(|run| !run.is_empty());
                if self.runs.len() != before {
                    self.rebuild_from_filter();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Per-buffer cache slot (stage-3 C7)
// ---------------------------------------------------------------------------

/// The text layer's handle on a buffer text's syntax parse cache (stage-3 C7).
///
/// The runs sit in an opaque slot on the buffer text. The slot is taken out
/// while a caller holds the cache, so no storage borrow is held across the
/// scan. Takes never span Lisp, so a nested take cannot happen. Losing the
/// body to a panic only costs the next lookups their hits: the cache never
/// answers from a state it did not validate after the substrate's invalidation.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) trait BufferSyntaxParseCache {
    /// Run `f` with the cache and what changed since its last use.
    fn with_syntax_parse_cache<R>(
        &self,
        f: impl FnOnce(&mut SyntaxParseCache, Invalidation) -> R,
    ) -> R;

    /// Whether a syntax-cache run could share this FROM. This reads numeric
    /// keys only and leaves invalidation pending. Invalidation can remove
    /// runs, never create a missing FROM; actual lookup must still drain it.
    fn syntax_parse_cache_has_run_from(&self, from_char: usize) -> bool;
}

fn cache_from_slot(slot: &mut Option<Box<dyn std::any::Any>>) -> &mut SyntaxParseCache {
    if slot.is_none() {
        *slot = Some(Box::new(SyntaxParseCache::default()));
    }
    slot.as_mut()
        .and_then(|body| body.downcast_mut::<SyntaxParseCache>())
        .expect("syntax parse body is a SyntaxParseCache")
}

impl BufferSyntaxParseCache for Buffer {
    fn with_syntax_parse_cache<R>(
        &self,
        f: impl FnOnce(&mut SyntaxParseCache, Invalidation) -> R,
    ) -> R {
        self.with_syntax_parse_body(|slot, invalidation| {
            let cache = cache_from_slot(slot);
            cache.apply_invalidation(invalidation);
            f(cache, invalidation)
        })
    }

    fn syntax_parse_cache_has_run_from(&self, from_char: usize) -> bool {
        self.with_syntax_parse_body_ref(|body| {
            body.and_then(|b| b.downcast_ref::<SyntaxParseCache>())
                .is_some_and(|cache| cache.has_run_from(from_char))
        })
    }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
/// Multiplicative hashing uses the high bits so aligned FROM positions do not
/// all land in the same bucket. The filter never substitutes for key equality.
#[inline]
fn from_filter_bit(from_char: usize) -> u64 {
    1u64 << ((from_char as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 58)
}

// ---------------------------------------------------------------------------
// Knobs and limits
// ---------------------------------------------------------------------------

/// A knob's value + 1 (0 = not read yet).
static CHUNK: AtomicUsize = AtomicUsize::new(0);
static MIN_SPAN: AtomicUsize = AtomicUsize::new(0);

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    /// Test override of the chunk and the minimum span.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) static GEOMETRY_OVERRIDE: std::cell::Cell<Option<(usize, usize)>> =
        const { std::cell::Cell::new(None) };
}

/// Characters between recorded loop-top states
/// (`NEOVM_SYNTAX_PARSE_CACHE_CHUNK`, default 2048, at least 16).
fn chunk_chars() -> usize {
    // See parse_cache_mode: the override is test-support-visible.
    #[cfg(any(test, feature = "test-support"))]
    if let Some((chunk, _)) = GEOMETRY_OVERRIDE.with(std::cell::Cell::get) {
        return chunk;
    }
    knob_usize(&CHUNK, "NEOVM_SYNTAX_PARSE_CACHE_CHUNK", 2048, 16)
}

/// The shortest scan that starts a run (`NEOVM_SYNTAX_PARSE_CACHE_MIN_SPAN`,
/// default 128): a shorter one costs less than recording it.
fn min_span_chars() -> usize {
    // See parse_cache_mode: the override is test-support-visible.
    #[cfg(any(test, feature = "test-support"))]
    if let Some((_, span)) = GEOMETRY_OVERRIDE.with(std::cell::Cell::get) {
        return span;
    }
    knob_usize(&MIN_SPAN, "NEOVM_SYNTAX_PARSE_CACHE_MIN_SPAN", 128, 0)
}

#[inline]
fn knob_usize(cell: &AtomicUsize, name: &'static str, default: usize, min: usize) -> usize {
    match cell.load(Ordering::Relaxed) {
        0 => read_knob_usize(cell, name, default, min),
        value => value - 1,
    }
}

#[cold]
#[inline(never)]
fn read_knob_usize(cell: &AtomicUsize, name: &'static str, default: usize, min: usize) -> usize {
    let value = std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .map_or(default, |value| value.max(min))
        .min(usize::MAX - 1);
    cell.store(value + 1, Ordering::Relaxed);
    tracing::debug!(name, value, "syntax parse cache knob read");
    value
}

/// Runs kept per buffer text (least recently used goes first).
#[cfg(not(any(test, feature = "test-support")))]
const MAX_RUNS: usize = 8;
#[cfg(any(test, feature = "test-support"))]
pub const MAX_RUNS: usize = 8;
/// Loop-top states kept per run.
const MAX_SNAPSHOTS: usize = 1024;
/// Exact answers kept per run.
const MAX_RESULTS: usize = 4;

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

#[cfg(not(any(test, feature = "test-support")))]
/// What the cache did, per thread (the evaluator's).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ParseCacheStats {
    /// Queries that reached the cache (every `parse-partial-sexp` with the
    /// knob on that runs no `syntax-propertize`).
    pub(crate) queries: u64,
    /// Answered from an exact memo.
    pub(crate) exact: u64,
    /// Resumed from a recorded loop state.
    pub(crate) resumes: u64,
    /// Characters the resumes did not scan.
    pub(crate) skipped_chars: u64,
    /// Scanned from FROM and recorded.
    pub(crate) recorded: u64,
    /// Scanned from FROM, too short to record, no run for the key.
    pub(crate) short: u64,
    /// Not cached: properties resolve through Lisp structure no note sees.
    pub(crate) bypassed: u64,
    /// Answers compared with a plain scan (`verify`), and the mismatches.
    pub(crate) verified: u64,
    pub(crate) mismatches: u64,
    /// Reuses refused because a descriptor cons had changed.
    pub(crate) descriptor_changes: u64,
    /// Queries answered by the canonical run, absolute or by state agreement.
    pub(crate) canon_absolute: u64,
    pub(crate) canon_adopted: u64,
    /// Failed FROM agreement or completed attempts without live synchronization.
    pub(crate) canon_declined: u64,
    /// Characters skipped through canonical snapshots.
    pub(crate) canon_skipped_chars: u64,
    /// Existing canonical runs replaced because the environment changed.
    pub(crate) canon_resets: u64,
    /// Queries that matched a later complete canonical loop state.
    pub(crate) canon_synced: u64,
    /// Back-comment lossage queries resumed from a warm canonical state.
    pub(crate) canon_back_comments: u64,
}
#[cfg(any(test, feature = "test-support"))]
/// What the cache did, per thread (the evaluator's).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParseCacheStats {
    /// Queries that reached the cache (every `parse-partial-sexp` with the
    /// knob on that runs no `syntax-propertize`).
    pub(crate) queries: u64,
    /// Answered from an exact memo.
    pub(crate) exact: u64,
    /// Resumed from a recorded loop state.
    pub(crate) resumes: u64,
    /// Characters the resumes did not scan.
    pub(crate) skipped_chars: u64,
    /// Scanned from FROM and recorded.
    pub(crate) recorded: u64,
    /// Scanned from FROM, too short to record, no run for the key.
    pub(crate) short: u64,
    /// Not cached: properties resolve through Lisp structure no note sees.
    pub(crate) bypassed: u64,
    /// Answers compared with a plain scan (`verify`), and the mismatches.
    pub(crate) verified: u64,
    pub(crate) mismatches: u64,
    /// Reuses refused because a descriptor cons had changed.
    pub(crate) descriptor_changes: u64,
    /// Queries answered by the canonical run, absolute or by state agreement.
    pub(crate) canon_absolute: u64,
    pub(crate) canon_adopted: u64,
    /// Failed FROM agreement or completed attempts without live synchronization.
    pub(crate) canon_declined: u64,
    /// Characters skipped through canonical snapshots.
    pub(crate) canon_skipped_chars: u64,
    /// Existing canonical runs replaced because the environment changed.
    pub(crate) canon_resets: u64,
    /// Queries that matched a later complete canonical loop state.
    pub(crate) canon_synced: u64,
    /// Back-comment lossage queries resumed from a warm canonical state.
    pub(crate) canon_back_comments: u64,
}

impl ParseCacheStats {
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `queries` counter (VIS read arm, lead #1052).  The field is
    /// `pub(crate)` in every arm; these getters are the only cross-crate
    /// read routes.
    pub(crate) fn queries_for_test(&self) -> u64 {
        self.queries
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `exact` counter (VIS read arm, lead #1052).
    pub(crate) fn exact_for_test(&self) -> u64 {
        self.exact
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `resumes` counter (VIS read arm, lead #1052).
    pub(crate) fn resumes_for_test(&self) -> u64 {
        self.resumes
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `skipped_chars` counter (VIS read arm, lead #1052).
    pub(crate) fn skipped_chars_for_test(&self) -> u64 {
        self.skipped_chars
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `recorded` counter (VIS read arm, lead #1052).
    pub(crate) fn recorded_for_test(&self) -> u64 {
        self.recorded
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `short` counter (VIS read arm, lead #1052).
    pub(crate) fn short_for_test(&self) -> u64 {
        self.short
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `bypassed` counter (VIS read arm, lead #1052).
    pub(crate) fn bypassed_for_test(&self) -> u64 {
        self.bypassed
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `verified` counter (VIS read arm, lead #1052).
    pub(crate) fn verified_for_test(&self) -> u64 {
        self.verified
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `mismatches` counter (VIS read arm, lead #1052).
    pub(crate) fn mismatches_for_test(&self) -> u64 {
        self.mismatches
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `descriptor_changes` counter (VIS read arm, lead #1052).
    pub(crate) fn descriptor_changes_for_test(&self) -> u64 {
        self.descriptor_changes
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `canon_absolute` counter (VIS read arm, lead #1052).
    pub(crate) fn canon_absolute_for_test(&self) -> u64 {
        self.canon_absolute
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `canon_adopted` counter (VIS read arm, lead #1052).
    pub(crate) fn canon_adopted_for_test(&self) -> u64 {
        self.canon_adopted
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `canon_declined` counter (VIS read arm, lead #1052).
    pub(crate) fn canon_declined_for_test(&self) -> u64 {
        self.canon_declined
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `canon_skipped_chars` counter (VIS read arm, lead #1052).
    pub(crate) fn canon_skipped_chars_for_test(&self) -> u64 {
        self.canon_skipped_chars
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `canon_resets` counter (VIS read arm, lead #1052).
    pub(crate) fn canon_resets_for_test(&self) -> u64 {
        self.canon_resets
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `canon_synced` counter (VIS read arm, lead #1052).
    pub(crate) fn canon_synced_for_test(&self) -> u64 {
        self.canon_synced
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied `canon_back_comments` counter (VIS read arm, lead #1052).
    pub(crate) fn canon_back_comments_for_test(&self) -> u64 {
        self.canon_back_comments
    }
}

thread_local! {
    static STATS: std::cell::Cell<ParseCacheStats> = const {
        std::cell::Cell::new(ParseCacheStats {
            queries: 0,
            exact: 0,
            resumes: 0,
            skipped_chars: 0,
            recorded: 0,
            short: 0,
            bypassed: 0,
            verified: 0,
            mismatches: 0,
            descriptor_changes: 0,
            canon_absolute: 0,
            canon_adopted: 0,
            canon_declined: 0,
            canon_skipped_chars: 0,
            canon_resets: 0,
            canon_synced: 0,
            canon_back_comments: 0,
        })
    };
}

/// Mismatches `verify` found in this process, on any thread.
static MISMATCHES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg_attr(feature = "test-support", visibility::make(pub))]
/// This thread's cache statistics.
pub(crate) fn parse_cache_stats() -> ParseCacheStats {
    STATS.with(std::cell::Cell::get)
}

/// `verify` mismatches in this process.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn parse_cache_mismatches() -> u64 {
    MISMATCHES.load(Ordering::Relaxed)
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn reset_parse_cache_stats() {
    STATS.with(|cell| cell.set(ParseCacheStats::default()));
}

#[inline]
fn count(f: impl FnOnce(&mut ParseCacheStats)) {
    STATS.with(|cell| {
        let mut stats = cell.get();
        f(&mut stats);
        cell.set(stats);
    });
}

/// `NEOVM_SYNTAX_PARSE_CACHE_STATS=PATH`: the counters are rewritten to PATH
/// every 256 queries (engagement checks for measurement runs).
#[inline]
fn maybe_write_stats_file() {
    static PATH: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let Some(path) = PATH
        .get_or_init(|| std::env::var("NEOVM_SYNTAX_PARSE_CACHE_STATS").ok())
        .as_deref()
    else {
        return;
    };
    if path.is_empty() {
        return;
    }
    if !STATS.with(|cell| cell.get().queries.is_multiple_of(256)) {
        return;
    }
    write_stats_file(path);
}

/// Formatting and filesystem work belongs only to an enabled periodic flush.
#[cold]
#[inline(never)]
fn write_stats_file(path: &str) {
    let stats = parse_cache_stats();
    let _ = std::fs::write(
        path,
        format!(
            "queries={} exact={} resumes={} skipped_chars={} recorded={} short={} \
             bypassed={} verified={} mismatches={} descriptor_changes={} \
             canon_absolute={} canon_adopted={} canon_declined={} \
             canon_skipped_chars={} canon_resets={} canon_synced={} canon_back_comments={}\n",
            stats.queries,
            stats.exact,
            stats.resumes,
            stats.skipped_chars,
            stats.recorded,
            stats.short,
            stats.bypassed,
            stats.verified,
            stats.mismatches,
            stats.descriptor_changes,
            stats.canon_absolute,
            stats.canon_adopted,
            stats.canon_declined,
            stats.canon_skipped_chars,
            stats.canon_resets,
            stats.canon_synced,
            stats.canon_back_comments
        ),
    );
}

// ---------------------------------------------------------------------------
// Keys, runs and the memo
// ---------------------------------------------------------------------------

/// Everything a scan reads besides the text, its properties and the query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EnvKey {
    begv: usize,
    /// The syntax table's identity and every char-table write since: the key
    /// the flat ASCII classifiers already trust. A table's allocation moves
    /// the tick too, so a recycled address cannot alias.
    table_bits: usize,
    char_table_tick: u64,
    honor_props: bool,
    escape_policy: CommentEndEscapePolicy,
    multibyte: bool,
}

impl EnvKey {
    fn of(
        buf: &Buffer,
        table: &SyntaxTable,
        props: SyntaxProperties<'_>,
        escape_policy: CommentEndEscapePolicy,
    ) -> Self {
        Self {
            begv: buf.accessible_char_region().start().get(),
            table_bits: table.chartable.bits(),
            char_table_tick: crate::emacs_core::chartable::char_table_write_tick(),
            honor_props: matches!(
                props,
                SyntaxProperties::Honor(_) | SyntaxProperties::PositionedHonor(_)
            ),
            escape_policy,
            multibyte: buf.get_multibyte(),
        }
    }
}

#[cfg(not(any(test, feature = "test-support")))]
/// Everything a scan from FROM reads besides the text and properties.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RunKey {
    from_char: usize,
    /// The internalized OLDSTATE: two OLDSTATEs GNU treats alike share a run.
    start: PartialParseState,
    from_oldstate: bool,
    target_depth: Option<i64>,
    stop_before: bool,
    commentstop: CommentStopMode,
    env: EnvKey,
}
#[cfg(any(test, feature = "test-support"))]
/// Everything a scan from FROM reads besides the text and properties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunKey {
    pub(crate) from_char: usize,
    /// The internalized OLDSTATE: two OLDSTATEs GNU treats alike share a run.
    start: PartialParseState,
    from_oldstate: bool,
    target_depth: Option<i64>,
    stop_before: bool,
    commentstop: CommentStopMode,
    env: EnvKey,
}

impl RunKey {
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Copied FROM position (VIS read arm, lead #1052).  The field is
    /// `pub(crate)` in every arm; this getter is the only cross-crate route.
    pub(crate) fn from_char_for_test(&self) -> usize {
        self.from_char
    }
}

/// A finished answer for one TO.
#[derive(Debug)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
struct ExactResult {
    to_char: usize,
    /// The scan read no character at or after these positions.
    dep_end_char: usize,
    dep_end_byte: usize,
    state: PartialParseState,
    stop: i64,
}

impl ExactResult {
    fn of(finish: &ScanFinish, to_char: usize) -> Self {
        let stop_char = (finish.stop - 1) as usize;
        // A scan that stopped early read at most the character at its cursor
        // beyond the stop (a two-character peek); one that ran to TO read
        // nothing at or after TO.
        let (dep_end_char, dep_end_byte) = if stop_char >= to_char {
            (to_char, finish.cursor_byte.get())
        } else {
            (
                to_char.min(stop_char + 2),
                finish.cursor_byte.get() + neovm_value::emacs_char::MAX_MULTIBYTE_LENGTH,
            )
        };
        Self {
            to_char,
            dep_end_char,
            dep_end_byte,
            state: finish.state.clone(),
            stop: finish.stop,
        }
    }
}

/// A descriptor cons a recording scan read, as it read it.
#[derive(Clone, Copy, Debug)]
struct Descriptor {
    first_char: usize,
    bits: usize,
    car: usize,
    cdr: usize,
}

impl Descriptor {
    fn read(first_char: usize, value: Value) -> Self {
        Self {
            first_char,
            bits: value.bits(),
            car: value.cons_car().bits(),
            cdr: value.cons_cdr().bits(),
        }
    }

    /// Whether the cons still holds what the recording scan read. Called only
    /// while a state at or after `first_char` survives, so the interval that
    /// held the cons there still does (see the module documentation).
    fn unchanged(&self) -> bool {
        let value = unsafe {
            /* SAFETY: real Value word */
            Value::from_bits(self.bits)
        };
        value.cons_car().bits() == self.car && value.cons_cdr().bits() == self.cdr
    }
}

#[cfg(not(any(test, feature = "test-support")))]
/// Recorded scans that share one key.
#[derive(Debug)]
struct ParseRun {
    key: RunKey,
    /// Loop-top states strictly after FROM, ascending by position (and so by
    /// byte).
    snaps: Vec<LoopState>,
    /// The position of the last state recorded on the chunk grid (FROM when
    /// none): the next grid state is a chunk after it.
    grid_frontier: usize,
    /// Oldest first.
    results: Vec<ExactResult>,
    descriptors: Vec<Descriptor>,
    last_used: u64,
}
#[cfg(any(test, feature = "test-support"))]
/// Recorded scans that share one key.
#[derive(Debug)]
pub struct ParseRun {
    pub(crate) key: RunKey,
    /// Loop-top states strictly after FROM, ascending by position (and so by
    /// byte).
    snaps: Vec<LoopState>,
    /// The position of the last state recorded on the chunk grid (FROM when
    /// none): the next grid state is a chunk after it.
    grid_frontier: usize,
    /// Oldest first.
    results: Vec<ExactResult>,
    descriptors: Vec<Descriptor>,
    last_used: u64,
}

impl ParseRun {
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    /// Borrowed run key (VIS read arm, lead #1052).  The field is
    /// `pub(crate)` in every arm; this getter is the only cross-crate route.
    pub(crate) fn key_for_test(&self) -> &RunKey {
        &self.key
    }
}

impl ParseRun {
    fn new(key: RunKey) -> Self {
        let grid_frontier = key.from_char;
        Self {
            key,
            snaps: Vec::new(),
            grid_frontier,
            results: Vec::new(),
            descriptors: Vec::new(),
            last_used: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.snaps.is_empty() && self.results.is_empty()
    }

    /// Forget everything that read text at or after `byte`, or properties at
    /// or after `char`. The state at loop top `c` read characters up to and
    /// including `c`.
    fn truncate(&mut self, byte: usize, char: usize) {
        let keep = self
            .snaps
            .partition_point(|snap| snap.byte_pos.get() < byte && snap.char_pos < char);
        self.snaps.truncate(keep);
        self.results
            .retain(|result| result.dep_end_byte <= byte && result.dep_end_char <= char);
        let last_snap = self
            .snaps
            .last()
            .map_or(self.key.from_char, |snap| snap.char_pos);
        self.grid_frontier = self.grid_frontier.min(last_snap);
        // A descriptor matters only while something that read it survives.
        let reach = self
            .results
            .iter()
            .map(|result| result.dep_end_char.saturating_sub(1))
            .fold(last_snap, usize::max);
        self.descriptors
            .retain(|descriptor| descriptor.first_char <= reach && descriptor.first_char < char);
    }

    /// Whether every descriptor read at or before `upto` is unchanged. A
    /// changed one invalidates everything from where it was first read.
    fn validate(&mut self, upto: usize) -> bool {
        let changed = self
            .descriptors
            .iter()
            .filter(|descriptor| descriptor.first_char <= upto && !descriptor.unchanged())
            .map(|descriptor| descriptor.first_char)
            .min();
        match changed {
            None => true,
            Some(at) => {
                count(|stats| stats.descriptor_changes += 1);
                self.truncate(usize::MAX, at);
                false
            }
        }
    }
}

/// What a lookup found for a query.
enum Found {
    Exact(PartialParseState, i64),
    Resume {
        at: LoopState,
        grid_frontier: usize,
    },
    Miss {
        run_exists: bool,
        grid_frontier: usize,
    },
}

impl SyntaxParseCache {
    fn lookup(&mut self, key: &RunKey, to_char: usize) -> Found {
        self.clock += 1;
        let clock = self.clock;
        let Some(run) = self.runs.iter_mut().find(|run| run.key == *key) else {
            return Found::Miss {
                run_exists: false,
                grid_frontier: key.from_char,
            };
        };
        run.last_used = clock;
        // A changed descriptor truncates the run: look again at what is left.
        loop {
            if let Some(result) = run.results.iter().find(|result| result.to_char == to_char) {
                let upto = result.dep_end_char.saturating_sub(1);
                if !run.validate(upto) {
                    continue;
                }
                let result = run
                    .results
                    .iter()
                    .find(|result| result.to_char == to_char)
                    .expect("validated just now");
                return Found::Exact(result.state.clone(), result.stop);
            }
            let below = run.snaps.partition_point(|snap| snap.char_pos < to_char);
            if below > 0 {
                let at = run.snaps[below - 1].char_pos;
                if !run.validate(at) {
                    continue;
                }
                return Found::Resume {
                    at: run.snaps[below - 1].clone(),
                    grid_frontier: run.grid_frontier,
                };
            }
            return Found::Miss {
                run_exists: true,
                grid_frontier: run.grid_frontier,
            };
        }
    }

    /// Find or create one L1 run, preserving the current selective FROM-filter
    /// updates and least-recently-used eviction policy.
    fn run_for(&mut self, key: RunKey) -> usize {
        self.clock += 1;
        let clock = self.clock;
        let index = match self.runs.iter().position(|run| run.key == key) {
            Some(index) => index,
            None => {
                if self.runs.len() >= MAX_RUNS
                    && let Some(oldest) = self
                        .runs
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, run)| run.last_used)
                        .map(|(index, _)| index)
                {
                    self.runs.swap_remove(oldest);
                    self.rebuild_from_filter();
                }
                self.from_filter |= from_filter_bit(key.from_char);
                self.runs.push(ParseRun::new(key));
                self.runs.len() - 1
            }
        };
        self.runs[index].last_used = clock;
        index
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn store(&mut self, key: RunKey, record: Record, result: Option<ExactResult>) {
        let index = self.run_for(key);
        let run = &mut self.runs[index];

        // The descriptors this scan read. One already known but no longer
        // holding what was recorded invalidates the states that read it --
        // before anything this scan read is merged, so that no truncation
        // can drop what the new states depend on.
        let stale = record
            .log
            .entries
            .iter()
            .filter_map(|(_, value)| {
                run.descriptors
                    .iter()
                    .find(|known| known.bits == value.bits() && !known.unchanged())
                    .map(|known| known.first_char)
            })
            .min();
        if let Some(at) = stale {
            count(|stats| stats.descriptor_changes += 1);
            run.truncate(usize::MAX, at);
        }
        let mut limit = record.log.unvalidatable_from.unwrap_or(usize::MAX);
        for (pos, value) in record.log.entries {
            if let Some(known) = run
                .descriptors
                .iter_mut()
                .find(|known| known.bits == value.bits())
            {
                known.first_char = known.first_char.min(pos);
                continue;
            }
            if run.descriptors.len() < DESCRIPTOR_LOG_CAP {
                run.descriptors.push(Descriptor::read(pos, value));
            } else {
                limit = limit.min(pos);
            }
        }

        // The states it took, in order, below any position it cannot vouch
        // for.
        let near_end = record.near_end;
        for snap in record.taken.into_iter().chain(near_end) {
            if run.snaps.len() >= MAX_SNAPSHOTS {
                break;
            }
            if snap.char_pos >= limit {
                continue;
            }
            let at = run
                .snaps
                .partition_point(|known| known.char_pos < snap.char_pos);
            if run
                .snaps
                .get(at)
                .is_none_or(|known| known.char_pos != snap.char_pos)
            {
                run.snaps.insert(at, snap);
            }
        }
        if record.grid_taken < limit {
            run.grid_frontier = run.grid_frontier.max(record.grid_taken);
        }
        if let Some(result) = result
            && result.dep_end_char <= limit
        {
            run.results.retain(|known| known.to_char != result.to_char);
            if run.results.len() >= MAX_RESULTS {
                run.results.remove(0);
            }
            run.results.push(result);
        }
        if run.is_empty() {
            self.runs.swap_remove(index);
            self.rebuild_from_filter();
        }
    }

    /// Memoize an L2 answer under the query's own L1 key. The canonical scan's
    /// dependency limit must cover the whole exact answer, and every supplied
    /// descriptor must fit alongside the receiving run's surviving descriptors.
    /// This executes only under the buffer storage's exclusive cache access;
    /// the incoming result and descriptor vector belong to the querying mutator.
    fn store_result(
        &mut self,
        key: RunKey,
        result: ExactResult,
        descriptors: Vec<Descriptor>,
        dep_limit: usize,
    ) {
        // A dependency-rejected answer must not create an empty run, advance
        // its LRU clock or evict another run. Snapshot truncation in the canonical
        // cache alone does not establish that this exact answer is reusable.
        if result.dep_end_char > dep_limit || descriptors.len() > DESCRIPTOR_LOG_CAP {
            return;
        }
        let index = self.run_for(key);
        let run = &mut self.runs[index];
        let stale = descriptors
            .iter()
            .filter_map(|descriptor| {
                run.descriptors
                    .iter()
                    .find(|known| known.bits == descriptor.bits && !known.unchanged())
                    .map(|known| known.first_char)
            })
            .min();
        if let Some(at) = stale {
            count(|stats| stats.descriptor_changes += 1);
            run.truncate(usize::MAX, at);
        }
        let additional = descriptors
            .iter()
            .filter(|descriptor| {
                !run.descriptors
                    .iter()
                    .any(|known| known.bits == descriptor.bits)
            })
            .count();
        if run.descriptors.len() + additional > DESCRIPTOR_LOG_CAP {
            if run.is_empty() {
                self.runs.swap_remove(index);
                self.rebuild_from_filter();
            }
            return;
        }
        for descriptor in descriptors {
            if let Some(known) = run
                .descriptors
                .iter_mut()
                .find(|known| known.bits == descriptor.bits)
            {
                known.first_char = known.first_char.min(descriptor.first_char);
            } else {
                run.descriptors.push(descriptor);
            }
        }
        run.results.retain(|known| known.to_char != result.to_char);
        if run.results.len() >= MAX_RESULTS {
            run.results.remove(0);
        }
        run.results.push(result);
    }
}

/// The scan mode of a query the cache records: loop-top states on the chunk
/// grid past what the run has, the last loop top before TO, and the property
/// values read.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
struct Record {
    /// Loop tops at or below this are not taken (FROM, or the resume point).
    floor: usize,
    grid_next: usize,
    chunk: usize,
    /// The last loop top before TO is at or after this.
    near_from: usize,
    taken: Vec<LoopState>,
    /// The last grid state taken (`floor` when none).
    grid_taken: usize,
    near_end: Option<LoopState>,
    log: DescriptorLog,
}

impl Record {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn new(floor: usize, grid_frontier: usize, to_char: usize) -> Self {
        let chunk = chunk_chars();
        Self {
            floor,
            grid_next: grid_frontier.max(floor).saturating_add(chunk),
            chunk,
            near_from: to_char.saturating_sub(2),
            taken: Vec::new(),
            grid_taken: floor,
            near_end: None,
            log: DescriptorLog::default(),
        }
    }
}

impl ScanMode for Record {
    const ACTIVE: bool = true;
    const RECORD_DESCRIPTORS: bool = true;

    fn first_target(&self) -> usize {
        self.grid_next.min(self.near_from).max(self.floor + 1)
    }

    fn at_loop_top(&mut self, top: LoopTop<'_>) -> TopAction {
        let at = top.char_pos;
        if at > self.floor {
            if at >= self.grid_next {
                self.taken.push(top.to_state());
                self.grid_taken = at;
                self.grid_next = at.saturating_add(self.chunk);
            } else if at >= self.near_from {
                self.near_end = Some(top.to_state());
            }
        }
        let next = if at >= self.near_from {
            at + 1
        } else {
            self.grid_next.min(self.near_from)
        };
        TopAction::Continue(next.max(self.floor + 1))
    }

    fn descriptors_read(&mut self, log: DescriptorLog) {
        self.log = log;
    }
}

/// The `category` property name, interned once.
fn category_symbol() -> Value {
    static SYMBOL: std::sync::OnceLock<neovm_value::intern::SymId> = std::sync::OnceLock::new();
    Value::from_sym_id(*SYMBOL.get_or_init(|| neovm_value::intern::intern("category")))
}

/// Whether the buffer's `syntax-table` properties resolve through Lisp
/// structure no note observes: `category` symbols' plists,
/// `char-property-alias-alist`, `default-text-properties`.
fn resolves_through_lisp(buf: &Buffer, props: SyntaxProperties<'_>) -> bool {
    match props {
        SyntaxProperties::Ignore | SyntaxProperties::PositionedIgnore => false,
        SyntaxProperties::Honor(resolver) | SyntaxProperties::PositionedHonor(resolver) => {
            !resolver.supports_presence_coalescing()
                || buf.text_props_property_name_presence(category_symbol())
                    != neovm_edit::buffer::text_props::PropertyNamePresence::DefinitelyAbsent
        }
    }
}

fn finished(end: ScanEnd) -> ScanFinish {
    match end {
        ScanEnd::Finished(finish) => finish,
        ScanEnd::Paused(_) => unreachable!("plain and recording scans never pause"),
    }
}

/// Whether a short query cannot reuse any run, using validated, absolute
/// zero-based character positions. This leaves mutation notes pending: they
/// can remove a run, but cannot create one for a missing FROM.
#[inline(always)]
pub(super) fn short_query_without_run(buf: &Buffer, from_char: usize, to_char: usize) -> bool {
    to_char - from_char < min_span_chars() && !buf.syntax_parse_cache_has_run_from(from_char)
}

/// Account for a short miss after its plain scan, including the periodic
/// diagnostics used to qualify cache engagement.
#[inline]
pub(super) fn note_short_query() {
    count(|stats| {
        stats.queries += 1;
        stats.short += 1;
    });
    maybe_write_stats_file();
}

/// `parse-partial-sexp` of `buf` through the cache: the finished state and
/// the stop position. The caller has validated FROM and TO and found no
/// `syntax-propertize` to run.
#[allow(clippy::too_many_arguments)] // parse-partial-sexp's arguments
pub(super) fn parse_partial_sexp_cached(
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
    mode: ParseCacheMode,
) -> (PartialParseState, i64) {
    let (from_char, to_char) = clamped_parse_range(buf, from, to);
    let start = PartialParseState::from_oldstate(oldstate);
    let from_oldstate = oldstate.is_some();
    let plain = |state: PartialParseState| {
        finished(run_parse_loop(
            buf,
            table,
            Entry::Fresh {
                from_char,
                state,
                from_oldstate,
            },
            to_char,
            target_depth,
            stop_before,
            commentstop,
            props,
            escape_policy,
            &mut Plain,
        ))
    };
    // Short scans never start a run. If no existing run shares FROM, avoid
    // building a full key or draining invalidation just to discover a miss.
    // Keep the normal lookup for matching FROM: a longer recorded scan can
    // still answer a short query by resuming or returning an exact result.
    if short_query_without_run(buf, from_char, to_char) {
        let finish = plain(start);
        note_short_query();
        return (finish.state, finish.stop);
    }
    count(|stats| stats.queries += 1);
    if props.symbol_mode() == neovm_value::tagged::SymbolMode::Positioned
        || resolves_through_lisp(buf, props)
    {
        count(|stats| stats.bypassed += 1);
        let finish = plain(start);
        return (finish.state, finish.stop);
    }
    let l2_mode = canon_mode();
    let verify_start =
        (mode == ParseCacheMode::Verify || l2_mode == CanonMode::Verify).then(|| start.clone());
    let key = RunKey {
        from_char,
        start,
        from_oldstate,
        target_depth,
        stop_before,
        commentstop,
        env: EnvKey::of(buf, table, props, escape_policy),
    };
    let found = buf.with_syntax_parse_cache(|cache, _| cache.lookup(&key, to_char));
    // Exact L1 answers are already the least-work path. Other option-free
    // queries may use the canonical run; optioned queries retain their current
    // L1 recording/resume paths, with their complete keys and original options.
    let l2 = match &found {
        Found::Exact(..) => None,
        _ if l2_mode != CanonMode::Off && canon::has_no_options(&key) && to_char > from_char => {
            if canon::is_absolute(&key) {
                Some(canon::absolute_answer(
                    buf,
                    table,
                    key.env,
                    to_char,
                    props,
                    escape_policy,
                ))
            } else {
                let l1_from = match &found {
                    Found::Resume { at, .. } => at.char_pos,
                    _ => from_char,
                };
                canon::adopted_answer(buf, table, &key, to_char, props, escape_policy, l1_from)
            }
        }
        _ => None,
    };
    if let Some(canon::CanonAnswer {
        finish,
        descriptors,
        dep_limit,
    }) = l2
    {
        let result = ExactResult::of(&finish, to_char);
        buf.with_syntax_parse_cache(|cache, _| {
            cache.store_result(key, result, descriptors, dep_limit)
        });
        return verified(
            finish.state,
            finish.stop,
            verify_start,
            &plain,
            from,
            to,
            target_depth,
            stop_before,
            commentstop,
        );
    }
    // A query may agree with the canonical scan only after the form or
    // string containing FROM ends. Scan the actual query once, retaining
    // its L1 recorder, and reuse only on complete live-state agreement.
    if l2_mode != CanonMode::Off
        && canon::has_no_options(&key)
        && to_char > from_char
        && !matches!(found, Found::Exact(..))
    {
        let (entry, grid_frontier) = match found {
            Found::Resume { at, grid_frontier } => {
                count(|stats| {
                    stats.resumes += 1;
                    stats.skipped_chars += (at.char_pos - from_char) as u64;
                });
                (
                    Entry::Resume {
                        at,
                        first_syntax: None,
                    },
                    grid_frontier,
                )
            }
            Found::Miss {
                run_exists,
                grid_frontier,
            } => {
                if !run_exists && to_char - from_char < min_span_chars() {
                    count(|stats| stats.short += 1);
                    let finish = plain(key.start);
                    maybe_write_stats_file();
                    return (finish.state, finish.stop);
                }
                count(|stats| stats.recorded += 1);
                (
                    Entry::Fresh {
                        from_char,
                        state: key.start.clone(),
                        from_oldstate,
                    },
                    grid_frontier,
                )
            }
            Found::Exact(..) => unreachable!("exact L1 results retain priority"),
        };
        let finish = match canon::live_sync_answer(
            buf,
            table,
            &key,
            entry,
            grid_frontier,
            to_char,
            props,
            escape_policy,
        ) {
            canon::LiveAnswer::Finished { finish, record } => {
                let result = ExactResult::of(&finish, to_char);
                buf.with_syntax_parse_cache(|cache, _| cache.store(key, record, Some(result)));
                finish
            }
            canon::LiveAnswer::Synced { answer, record } => {
                let result = ExactResult::of(&answer.finish, to_char);
                buf.with_syntax_parse_cache(|cache, _| {
                    cache.store(key.clone(), record, None);
                    cache.store_result(key, result, answer.descriptors, answer.dep_limit);
                });
                answer.finish
            }
        };
        return verified(
            finish.state,
            finish.stop,
            verify_start,
            &plain,
            from,
            to,
            target_depth,
            stop_before,
            commentstop,
        );
    }
    let answer = match found {
        Found::Exact(state, stop) => {
            count(|stats| stats.exact += 1);
            Some((state, stop))
        }
        Found::Resume { at, grid_frontier } => {
            count(|stats| {
                stats.resumes += 1;
                stats.skipped_chars += (at.char_pos - from_char) as u64;
            });
            let mut record = Record::new(at.char_pos, grid_frontier, to_char);
            let finish = finished(run_parse_loop(
                buf,
                table,
                Entry::Resume {
                    at,
                    first_syntax: None,
                },
                to_char,
                target_depth,
                stop_before,
                commentstop,
                props,
                escape_policy,
                &mut record,
            ));
            let result = ExactResult::of(&finish, to_char);
            buf.with_syntax_parse_cache(|cache, _| cache.store(key, record, Some(result)));
            Some((finish.state, finish.stop))
        }
        Found::Miss {
            run_exists,
            grid_frontier,
        } => {
            if !run_exists && to_char - from_char < min_span_chars() {
                count(|stats| stats.short += 1);
                let finish = plain(key.start);
                maybe_write_stats_file();
                return (finish.state, finish.stop);
            }
            count(|stats| stats.recorded += 1);
            let mut record = Record::new(from_char, grid_frontier, to_char);
            let finish = finished(run_parse_loop(
                buf,
                table,
                Entry::Fresh {
                    from_char,
                    state: key.start.clone(),
                    from_oldstate,
                },
                to_char,
                target_depth,
                stop_before,
                commentstop,
                props,
                escape_policy,
                &mut record,
            ));
            let result = ExactResult::of(&finish, to_char);
            buf.with_syntax_parse_cache(|cache, _| cache.store(key, record, Some(result)));
            maybe_write_stats_file();
            return (finish.state, finish.stop);
        }
    };
    let (state, stop) = answer.expect("a cached answer");
    verified(
        state,
        stop,
        verify_start,
        &plain,
        from,
        to,
        target_depth,
        stop_before,
        commentstop,
    )
}

/// Canonical BEGV state for a back-comment lossage query. `None` preserves
/// the legacy index when either cache layer is off or no warm matching state
/// survives validation. Unsupported Lisp property resolvers instead return
/// a plain BEGV state: the legacy index has no stamp for those dependencies.
///
/// State is owned by this query; no cache borrow spans the scan and no Lisp
/// runs. Shared-text mutators require the outer text-access synchronization
/// described by `canon::back_comment_finish`.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(super) fn back_comment_canonical_state(
    buf: &Buffer,
    table: &SyntaxTable,
    to: i64,
    props: SyntaxProperties<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> Option<PartialParseState> {
    let l1 = parse_cache_mode();
    if l1 == ParseCacheMode::Off {
        return None;
    }
    let l2 = canon_mode();
    if l2 == CanonMode::Off {
        return None;
    }
    let from = char_pos_to_lisp_i64(buf.accessible_char_region().start().get());
    let (from_char, to_char) = clamped_parse_range(buf, from, to);
    let plain = |state| {
        finished(run_parse_loop(
            buf,
            table,
            Entry::Fresh {
                from_char,
                state,
                from_oldstate: false,
            },
            to_char,
            None,
            false,
            CommentStopMode::None,
            props,
            escape_policy,
            &mut Plain,
        ))
    };
    if props.symbol_mode() == neovm_value::tagged::SymbolMode::Positioned
        || resolves_through_lisp(buf, props)
    {
        count(|stats| stats.bypassed += 1);
        let state = plain(PartialParseState::new()).state;
        maybe_write_stats_file();
        return Some(state);
    }
    let cached = canon::back_comment_finish(
        buf,
        table,
        EnvKey::of(buf, table, props, escape_policy),
        to_char,
        props,
        escape_policy,
    )?;
    let verify_start =
        (l1 == ParseCacheMode::Verify || l2 == CanonMode::Verify).then(PartialParseState::new);
    let (state, _) = verified(
        cached.state,
        cached.stop,
        verify_start,
        &plain,
        from,
        to,
        None,
        false,
        CommentStopMode::None,
    );
    Some(state)
}

/// Verify either layer's cached answer by running the identical original
/// query. Temporary states are owned by this mutator, and no cache borrow is
/// held while the plain verification scan runs.
#[allow(clippy::too_many_arguments)] // preserve every original query option in diagnostics
#[inline]
fn verified<P>(
    state: PartialParseState,
    stop: i64,
    verify_start: Option<PartialParseState>,
    plain: &P,
    from: i64,
    to: i64,
    target_depth: Option<i64>,
    stop_before: bool,
    commentstop: CommentStopMode,
) -> (PartialParseState, i64)
where
    P: Fn(PartialParseState) -> ScanFinish,
{
    let answer = match verify_start {
        None => (state, stop),
        Some(start) => {
            let fresh = plain(start);
            count(|stats| stats.verified += 1);
            if (&state, stop) != (&fresh.state, fresh.stop) {
                count(|stats| stats.mismatches += 1);
                MISMATCHES.fetch_add(1, Ordering::Relaxed);
                tracing::error!(
                    from,
                    to,
                    ?target_depth,
                    stop_before,
                    ?commentstop,
                    cached = ?(&state, stop),
                    fresh = ?(&fresh.state, fresh.stop),
                    "syntax parse cache mismatch"
                );
                debug_assert!(
                    false,
                    "syntax parse cache mismatch: from {from} to {to}: cached {:?}, fresh {:?}",
                    (&state, stop),
                    (&fresh.state, fresh.stop)
                );
            }
            (fresh.state, fresh.stop)
        }
    };
    maybe_write_stats_file();
    answer
}

/// Carrier park: reset the syntax parse-cache counters.
#[doc(hidden)]
pub fn reset_carrier_tls() {
    STATS.with(|stats| stats.set(ParseCacheStats::default()));
}

/// Debug-only carrier TLS fingerprint for this module's classified records.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn carrier_tls_fingerprint(h: &mut u64) {
    neovm_value::tls_fingerprint::mix_debug(h, STATS.with(|slot| slot.get()));
}

#[cfg(test)]
#[path = "tests/parse_cache_invalidation_test.rs"]
mod invalidation_tests;
