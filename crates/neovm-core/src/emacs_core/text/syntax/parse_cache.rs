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
//! * text: the four measured mutators note the edit's first byte
//!   ([`SyntaxParseCache::note_edit`]); wholesale replacements, multibyte and
//!   backend conversions clear it;
//! * `syntax-table` / `category` properties: every property-table mutation that
//!   moves the table's syntax tick notes the first character it can touch
//!   ([`SyntaxParseCache::note_prop_change`]); a swapped-in table clears it;
//! * everything else a scan reads (the syntax table and its char-table write
//!   tick, BEGV, `parse-sexp-lookup-properties`, `comment-end-can-be-escaped`,
//!   multibyteness) is part of each query's key.
//!
//! Notes are counted: before each use, a text epoch or syntax tick that moved
//! further than the notes account for (a mutation path that bypassed them)
//! clears everything, so a missed hook costs hits, never answers.
//!
//! The cache lives in `BufferTextStorage`, outside the copy-on-write text, and
//! a snapshot clone starts empty (P3.0 §3.9). It holds positions and parse
//! states, and descriptor conses as bits (below): no `Value` it keeps alive,
//! nothing to trace.
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
//! `NEOVM_SYNTAX_PARSE_CACHE`: `0`/`off` (default), `1`/`on`, or `verify`
//! (every cached answer is recomputed by a plain scan and compared; a mismatch
//! is logged, counted, fails debug builds, and the plain answer is returned).

use super::parse_loop::{
    Entry, LoopState, LoopTop, Plain, ScanEnd, ScanFinish, ScanMode, TopAction, run_parse_loop,
};
use super::*;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

/// `NEOVM_SYNTAX_PARSE_CACHE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParseCacheMode {
    /// Every scan runs from FROM, as before (the default until measured).
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

#[cfg(test)]
thread_local! {
    /// Test override of `NEOVM_SYNTAX_PARSE_CACHE`.
    pub(crate) static MODE_OVERRIDE: std::cell::Cell<Option<ParseCacheMode>> =
        const { std::cell::Cell::new(None) };
}

/// The cache mode, read once from `NEOVM_SYNTAX_PARSE_CACHE`.
#[inline]
pub(crate) fn parse_cache_mode() -> ParseCacheMode {
    #[cfg(test)]
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
        Some("1" | "on" | "yes" | "true" | "t") => ParseCacheMode::On,
        Some("verify") => ParseCacheMode::Verify,
        _ => ParseCacheMode::Off,
    }
}

/// What changed since the cache was last used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Invalidation {
    /// Nothing a scan reads.
    Nothing,
    /// Text at or after `byte`, or syntax-relevant properties at or after
    /// `char` (either may be `usize::MAX`): everything a scan read before both
    /// is unchanged, and so are its coordinates.
    From { byte: usize, char: usize },
    /// Anything.
    All,
}

/// Per-storage syntax parse cache. See the module documentation.
#[derive(Debug, Default)]
pub(crate) struct SyntaxParseCache {
    runs: Vec<ParseRun>,
    /// LRU clock.
    clock: u64,
    /// Lowest Emacs byte position whose text changed since the last drain.
    dirty_byte: Option<usize>,
    /// Lowest char position whose syntax-relevant properties changed since
    /// the last drain.
    dirty_char: Option<usize>,
    /// A wholesale change since the last drain.
    cleared: bool,
    /// The text epoch and the property table's syntax tick at the last drain.
    seen_epoch: u64,
    seen_prop_tick: u64,
    /// How far the notes since the last drain account for each of them.
    noted_edits: u64,
    noted_prop_ticks: u64,
}

impl SyntaxParseCache {
    /// A text edit whose first changed byte is `at_byte` (every later byte may
    /// have moved). Called once per content-epoch bump.
    #[inline]
    pub(crate) fn note_edit(&mut self, at_byte: usize) {
        self.dirty_byte = Some(self.dirty_byte.map_or(at_byte, |dirty| dirty.min(at_byte)));
        self.noted_edits = self.noted_edits.wrapping_add(1);
    }

    /// A property mutation that moved the syntax tick by `ticks` and can
    /// change syntax-relevant properties only at or after `at_char`.
    #[inline]
    pub(crate) fn note_prop_change(&mut self, at_char: usize, ticks: u64) {
        self.dirty_char = Some(self.dirty_char.map_or(at_char, |dirty| dirty.min(at_char)));
        self.noted_prop_ticks = self.noted_prop_ticks.wrapping_add(ticks);
    }

    /// Everything goes: a change with no known extent.
    #[inline]
    pub(crate) fn clear(&mut self) {
        self.cleared = true;
    }

    /// Apply what changed since the last drain, given the storage's current
    /// text epoch and syntax tick, and reset the notes.
    pub(crate) fn drain(&mut self, epoch: u64, prop_tick: u64) -> Invalidation {
        let outcome = self.take_invalidation(epoch, prop_tick);
        match outcome {
            Invalidation::Nothing => {}
            Invalidation::All => self.runs.clear(),
            Invalidation::From { byte, char } => {
                for run in &mut self.runs {
                    run.truncate(byte, char);
                }
                self.runs.retain(|run| !run.is_empty());
            }
        }
        outcome
    }

    fn take_invalidation(&mut self, epoch: u64, prop_tick: u64) -> Invalidation {
        let unaccounted_text = epoch.wrapping_sub(self.seen_epoch) != self.noted_edits;
        let unaccounted_props =
            prop_tick.wrapping_sub(self.seen_prop_tick) != self.noted_prop_ticks;
        let outcome = if self.cleared || unaccounted_text || unaccounted_props {
            if !self.cleared && (unaccounted_text || unaccounted_props) {
                tracing::trace!(
                    unaccounted_text,
                    unaccounted_props,
                    "syntax parse cache: a change bypassed the notes; clearing"
                );
            }
            Invalidation::All
        } else if self.dirty_byte.is_none() && self.dirty_char.is_none() {
            Invalidation::Nothing
        } else {
            Invalidation::From {
                byte: self.dirty_byte.unwrap_or(usize::MAX),
                char: self.dirty_char.unwrap_or(usize::MAX),
            }
        };
        self.dirty_byte = None;
        self.dirty_char = None;
        self.cleared = false;
        self.seen_epoch = epoch;
        self.seen_prop_tick = prop_tick;
        self.noted_edits = 0;
        self.noted_prop_ticks = 0;
        outcome
    }
}

// ---------------------------------------------------------------------------
// Knobs and limits
// ---------------------------------------------------------------------------

/// A knob's value + 1 (0 = not read yet).
static CHUNK: AtomicUsize = AtomicUsize::new(0);
static MIN_SPAN: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
thread_local! {
    /// Test override of the chunk and the minimum span.
    pub(crate) static GEOMETRY_OVERRIDE: std::cell::Cell<Option<(usize, usize)>> =
        const { std::cell::Cell::new(None) };
}

/// Characters between recorded loop-top states
/// (`NEOVM_SYNTAX_PARSE_CACHE_CHUNK`, default 2048, at least 16).
fn chunk_chars() -> usize {
    #[cfg(test)]
    if let Some((chunk, _)) = GEOMETRY_OVERRIDE.with(std::cell::Cell::get) {
        return chunk;
    }
    knob_usize(&CHUNK, "NEOVM_SYNTAX_PARSE_CACHE_CHUNK", 2048, 16)
}

/// The shortest scan that starts a run (`NEOVM_SYNTAX_PARSE_CACHE_MIN_SPAN`,
/// default 128): a shorter one costs less than recording it.
fn min_span_chars() -> usize {
    #[cfg(test)]
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
const MAX_RUNS: usize = 8;
/// Loop-top states kept per run.
const MAX_SNAPSHOTS: usize = 1024;
/// Exact answers kept per run.
const MAX_RESULTS: usize = 4;

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

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
        })
    };
}

/// Mismatches `verify` found in this process, on any thread.
static MISMATCHES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// This thread's cache statistics.
pub(crate) fn parse_cache_stats() -> ParseCacheStats {
    STATS.with(std::cell::Cell::get)
}

/// `verify` mismatches in this process.
#[cfg(test)]
pub(crate) fn parse_cache_mismatches() -> u64 {
    MISMATCHES.load(Ordering::Relaxed)
}

#[cfg(test)]
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
fn maybe_write_stats_file(stats: &ParseCacheStats) {
    static PATH: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let Some(path) = PATH
        .get_or_init(|| std::env::var("NEOVM_SYNTAX_PARSE_CACHE_STATS").ok())
        .as_deref()
    else {
        return;
    };
    if path.is_empty() || !stats.queries.is_multiple_of(256) {
        return;
    }
    let _ = std::fs::write(
        path,
        format!(
            "queries={} exact={} resumes={} skipped_chars={} recorded={} short={} \
             bypassed={} verified={} mismatches={} descriptor_changes={}\n",
            stats.queries,
            stats.exact,
            stats.resumes,
            stats.skipped_chars,
            stats.recorded,
            stats.short,
            stats.bypassed,
            stats.verified,
            stats.mismatches,
            stats.descriptor_changes
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
            honor_props: matches!(props, SyntaxProperties::Honor(_)),
            escape_policy,
            multibyte: buf.get_multibyte(),
        }
    }
}

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

/// A finished answer for one TO.
#[derive(Debug)]
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
                finish.cursor_byte.get() + crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH,
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
        let value = Value::from_bits(self.bits);
        value.cons_car().bits() == self.car && value.cons_cdr().bits() == self.cdr
    }
}

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

    fn store(&mut self, key: RunKey, record: Record, result: Option<ExactResult>) {
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
                }
                self.runs.push(ParseRun::new(key));
                self.runs.len() - 1
            }
        };
        let run = &mut self.runs[index];
        run.last_used = clock;

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
        }
    }
}

/// The scan mode of a query the cache records: loop-top states on the chunk
/// grid past what the run has, the last loop top before TO, and the property
/// values read.
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
    static SYMBOL: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
        std::sync::OnceLock::new();
    Value::from_sym_id(*SYMBOL.get_or_init(|| crate::emacs_core::intern::intern("category")))
}

/// Whether the buffer's `syntax-table` properties resolve through Lisp
/// structure no note observes: `category` symbols' plists,
/// `char-property-alias-alist`, `default-text-properties`.
fn resolves_through_lisp(buf: &Buffer, props: SyntaxProperties<'_>) -> bool {
    match props {
        SyntaxProperties::Ignore => false,
        SyntaxProperties::Honor(resolver) => {
            !resolver.supports_presence_coalescing()
                || buf.text_props_property_name_presence(category_symbol())
                    != crate::buffer::text_props::PropertyNamePresence::DefinitelyAbsent
        }
    }
}

fn finished(end: ScanEnd) -> ScanFinish {
    match end {
        ScanEnd::Finished(finish) => finish,
        ScanEnd::Paused(_) => unreachable!("plain and recording scans never pause"),
    }
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
    count(|stats| stats.queries += 1);
    if resolves_through_lisp(buf, props) {
        count(|stats| stats.bypassed += 1);
        let finish = plain(start);
        return (finish.state, finish.stop);
    }
    let verify_start = (mode == ParseCacheMode::Verify).then(|| start.clone());
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
                let stats = parse_cache_stats();
                maybe_write_stats_file(&stats);
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
            let stats = parse_cache_stats();
            maybe_write_stats_file(&stats);
            return (finish.state, finish.stop);
        }
    };
    let (state, stop) = answer.expect("a cached answer");
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
    let stats = parse_cache_stats();
    maybe_write_stats_file(&stats);
    answer
}

#[cfg(test)]
#[path = "tests/parse_cache_invalidation.rs"]
mod invalidation_tests;

#[cfg(test)]
#[path = "tests/parse_cache.rs"]
mod memo_tests;
