//! L2 of the syntax parse cache (P3.4 S5): the canonical run.
//!
//! L1 serves a query only from scans with its own key (FROM, starting state,
//! options). `syntax-ppss` rarely repeats a key: it chains its queries --
//! each starts where an earlier one stopped, with that one's answer as its
//! OLDSTATE -- and flushes its own cache after every `syntax-propertize` pass,
//! so the same text is parsed from BEGV again, then chained again, on every
//! pass. What those queries share is the one scan they all sample: the scan
//! from BEGV with the fresh state and no options, the "canonical" scan. This
//! file keeps its loop-top states per buffer text and answers these kinds of
//! query from them.
//!
//! * **Absolute** queries -- FROM = BEGV, the fresh state, no options -- are
//!   the canonical scan itself: they resume from the canonical state just
//!   below TO, and a query past the last one extends the run as it scans.
//! * **Adopted** queries -- any other FROM, no options, and an OLDSTATE that
//!   agrees with the canonical scan at FROM (below) -- are answered by the
//!   canonical scan from FROM to TO, corrected for the few fields a scan
//!   started at FROM computes differently. Their scans extend the run too.
//! * **Live synchronized** queries -- when FROM cannot be adopted -- retain
//!   their L1 recorder and compare the actual loop state at common canonical
//!   tops, ignoring only minimum depth. Full equality permits a jump. A warm
//!   frontier can supply a temporary continuation bounded by four chunks and
//!   the query span; it is published only after a full match.
//!
//! # Why an adopted answer is exact
//!
//! A query from FROM with an internalized OLDSTATE `Q` and the canonical scan
//! at a loop top FROM in state `C` read the same text with the same table and
//! properties. When `Q` and `C` agree on every field the loop reads for its
//! control flow -- depth, the stack height, string and comment state, the
//! comment or string start, `quoted`, a pending atom (none), a two-character
//! construct pending across FROM (none: `Q`'s element 10 is nil), and a string
//! entered in OLDSTATE (none: `Q` is not in a string) -- the two scans take
//! the same step at every later character. They differ only in what they
//! *report*:
//!
//! * element 6, the minimum depth: `Q`'s starts at FROM's depth and mins in
//!   the depth after every close paren in [FROM, TO); the canonical's started
//!   at BEGV. The canonical scan reports its closes to the scan mode, which
//!   keeps their minimum since FROM.
//! * the per-level `last` and `prev` positions (elements 1, 2 and 9): `Q`
//!   comes with the open-paren positions of element 9 and nothing else, the
//!   canonical with every position it saw. A level that survives from FROM to
//!   TO keeps `Q`'s value for each of the two fields the scan never set
//!   between them, and takes the canonical's for a field it set -- every
//!   setting writes the same value into both scans. A field is set in
//!   [FROM, TO) exactly when its canonical value changed between FROM and TO:
//!   a setting writes a position past FROM (a sexp start after FROM), or, for
//!   `prev` when a child list closes, the level's `last` -- which differs from
//!   `prev` while that child is open. Levels above the lowest stack height of
//!   [FROM, TO] were created after FROM in both scans and agree. That height
//!   follows from the minimum depth: the stack shrinks with the depth until
//!   it is down to its last level.
//!
//! Everything else the result holds (depth, the string and comment state,
//! `quoted`, element 10) is the same in both scans at TO.
//!
//! # What is kept
//!
//! Loop-top states of the canonical scan, one per `NEOVM_SYNTAX_PARSE_CACHE_CHUNK`
//! characters plus the last loop top before each extending query's TO (which
//! lets the next query of a `syntax-ppss` chain start from its FROM), each
//! with the lowest depth a close paren left the scan at since the previous
//! state, so an adopted query can skip whole stretches of the run and still
//! know its minimum depth. The run is invalidated like an L1 run (text and
//! property notes, descriptor validation), keyed by the environment alone,
//! and dropped with its environment.
//!
//! `NEOVM_SYNTAX_PARSE_CACHE_L2`: `0` (default), `1`, or `verify` (on, with
//! every cached answer recomputed by a plain scan as under
//! `NEOVM_SYNTAX_PARSE_CACHE=verify`). L2 is consulted only with the cache on.
//!
//! # Mutator ownership
//!
//! Canonical runs belong to `BufferTextStorage`, under its existing mutator-
//! confined ownership and cache borrow, rather than to a thread. Separate
//! mutators own independent buffer storage and caches. Future mutators sharing
//! one text storage must serialize text/property access across validation,
//! scanning and publication, including mutable syntax tables and descriptor
//! conses; a RefCell borrow alone does not synchronize those accesses.
//! Scans and temporary plans belong to the mutator performing the query.
//! The process-wide knob holds only a scalar mode: relaxed loads/stores publish
//! no Lisp state or other data. Thread-local overrides exist only in tests.

use super::*;
#[cfg(test)]
use std::cell::Cell;

/// `NEOVM_SYNTAX_PARSE_CACHE_L2`.
/// Process-wide scalar policy, published without associated data; this enum
/// holds no mutator state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CanonMode {
    Off,
    On,
    /// On, and every cached answer is verified.
    Verify,
}

const CANON_UNREAD: u8 = 0;
const CANON_OFF: u8 = 1;
const CANON_ON: u8 = 2;
const CANON_VERIFY: u8 = 3;

static CANON_MODE: AtomicU8 = AtomicU8::new(CANON_UNREAD);

#[cfg(test)]
thread_local! {
    /// Test override of `NEOVM_SYNTAX_PARSE_CACHE_L2`.
    pub(crate) static CANON_MODE_OVERRIDE: Cell<Option<CanonMode>> = const { Cell::new(None) };
}

/// The L2 mode, read once from `NEOVM_SYNTAX_PARSE_CACHE_L2`.
#[inline]
pub(crate) fn canon_mode() -> CanonMode {
    #[cfg(test)]
    if let Some(mode) = CANON_MODE_OVERRIDE.with(Cell::get) {
        return mode;
    }
    match CANON_MODE.load(Ordering::Relaxed) {
        CANON_OFF => CanonMode::Off,
        CANON_ON => CanonMode::On,
        CANON_VERIFY => CanonMode::Verify,
        _ => read_canon_knob(),
    }
}

#[cold]
#[inline(never)]
fn read_canon_knob() -> CanonMode {
    let mode = parse_canon_knob(std::env::var("NEOVM_SYNTAX_PARSE_CACHE_L2").ok().as_deref());
    CANON_MODE.store(
        match mode {
            CanonMode::Off => CANON_OFF,
            CanonMode::On => CANON_ON,
            CanonMode::Verify => CANON_VERIFY,
        },
        Ordering::Relaxed,
    );
    tracing::debug!(?mode, "NEOVM_SYNTAX_PARSE_CACHE_L2 read");
    mode
}

pub(crate) fn parse_canon_knob(value: Option<&str>) -> CanonMode {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("1" | "on" | "yes" | "true" | "t") => CanonMode::On,
        Some("verify") => CanonMode::Verify,
        _ => CanonMode::Off,
    }
}

/// Canonical states kept per buffer text: 32 Mi characters at the default
/// chunk, plus one per extending query.
const MAX_CANON_SNAPSHOTS: usize = 16 * 1024;

/// A loop-top state of the canonical scan.
/// Owned by the buffer's canonical run and cloned under its exclusive cache
/// access; it contains positions and Rust parse data, with no rooted Lisp state.
#[derive(Clone, Debug)]
pub(super) struct CanonSnap {
    /// The canonical scan's state here; its minimum depth is the scan's since
    /// BEGV.
    pub(super) state: LoopState,
    /// The lowest depth a close paren left the scan at between the previous
    /// state (BEGV for the first) and this one; `i64::MAX` when none closed.
    pub(super) closes_min: i64,
}

impl CanonSnap {
    fn pos(&self) -> usize {
        self.state.char_pos
    }
}

/// The canonical run of one environment. See the module documentation.
/// Owned by the current mutator-confined `BufferTextStorage`. Cache borrows
/// serialize access within that owner; future shared storage/table/descriptor
/// writers require outer synchronization as described in the module docs.
#[derive(Debug)]
pub(super) struct CanonicalRun {
    pub(super) env: EnvKey,
    /// Ascending, strictly after BEGV.
    pub(super) snaps: Vec<CanonSnap>,
    /// The last state taken on the chunk grid (BEGV when none).
    grid_frontier: usize,
    pub(super) descriptors: Vec<Descriptor>,
}

impl CanonicalRun {
    pub(super) fn new(env: EnvKey) -> Self {
        Self {
            env,
            snaps: Vec::new(),
            grid_frontier: env.begv,
            descriptors: Vec::new(),
        }
    }

    /// Where the run's knowledge ends: its last state, or BEGV.
    pub(super) fn frontier(&self) -> usize {
        self.snaps.last().map_or(self.env.begv, CanonSnap::pos)
    }

    /// Forget everything that read text at or after `byte` or properties at
    /// or after `char` (a state at loop top `c` read characters up to `c`).
    pub(super) fn truncate(&mut self, byte: usize, char: usize) {
        let keep = self
            .snaps
            .partition_point(|snap| snap.state.byte_pos.get() < byte && snap.pos() < char);
        self.snaps.truncate(keep);
        let frontier = self.frontier();
        self.grid_frontier = self.grid_frontier.min(frontier);
        self.descriptors
            .retain(|descriptor| descriptor.first_char <= frontier && descriptor.first_char < char);
    }

    /// Whether every descriptor read at or before `upto` is unchanged; a
    /// changed one truncates the run where it was first read.
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

    /// The index of the greatest validated state strictly before `pos`
    /// (`before`) or at or before it.
    fn validated_below(&mut self, pos: usize, inclusive: bool) -> Option<usize> {
        loop {
            let below = if inclusive {
                self.snaps.partition_point(|snap| snap.pos() <= pos)
            } else {
                self.snaps.partition_point(|snap| snap.pos() < pos)
            };
            if below == 0 {
                return None;
            }
            if self.validate(self.snaps[below - 1].pos()) {
                return Some(below - 1);
            }
        }
    }

    /// Take in what a canonical scan saw: its descriptors and the states it
    /// took past the run's end. Returns the first position the scan could not
    /// validate, including overflow of the run's existing descriptor capacity.
    /// Exact answers depending on this position must not be memoized in L1.
    fn merge(&mut self, scan: CanonScan) -> usize {
        let CanonScan {
            taken,
            near_end,
            grid_taken,
            log,
            ..
        } = scan;
        if let Some(at) = stale_descriptor(&self.descriptors, &log) {
            count(|stats| stats.descriptor_changes += 1);
            self.truncate(usize::MAX, at);
        }
        let limit = merge_log(&mut self.descriptors, log);
        for snap in taken.into_iter().chain(near_end) {
            if self.snaps.len() >= MAX_CANON_SNAPSHOTS || snap.pos() >= limit {
                break;
            }
            if snap.pos() > self.frontier() {
                self.snaps.push(snap);
            }
        }
        if grid_taken < limit {
            self.grid_frontier = self.grid_frontier.max(grid_taken).min(self.frontier());
        }
        limit
    }
}

/// The first position of a descriptor in `known` that `log` read again and
/// that no longer holds what was recorded: everything read from there on is
/// stale. Checked before the log is merged, so that the truncation it calls
/// for cannot drop what the new scan read.
pub(super) fn stale_descriptor(known: &[Descriptor], log: &DescriptorLog) -> Option<usize> {
    log.entries
        .iter()
        .filter_map(|(_, value)| {
            known
                .iter()
                .find(|entry| entry.bits == value.bits() && !entry.unchanged())
                .map(|entry| entry.first_char)
        })
        .min()
}

/// Merge a scan's descriptor log into `known`. Returns the first position
/// the scan read something that cannot be validated (a syntax table, or a
/// descriptor past the cap): states at or after it must not be kept.
pub(super) fn merge_log(known: &mut Vec<Descriptor>, log: DescriptorLog) -> usize {
    let mut limit = log.unvalidatable_from.unwrap_or(usize::MAX);
    for (pos, value) in log.entries {
        if let Some(entry) = known.iter_mut().find(|entry| entry.bits == value.bits()) {
            entry.first_char = entry.first_char.min(pos);
            continue;
        }
        if known.len() < DESCRIPTOR_LOG_CAP {
            known.push(Descriptor::read(pos, value));
        } else {
            limit = limit.min(pos);
        }
    }
    limit
}

/// The scan mode of the canonical scan: states on the chunk grid and the last
/// loop top before TO, past the run's end; the minimum post-close depth per
/// state and since an adoption point; and, optionally, a pause at a loop top.
/// Temporary data owned by the querying mutator; it is never shared between
/// active scans or retained as a thread-local cache.
pub(super) struct CanonScan {
    /// Loop tops at or below this are not taken (the resume point).
    floor: usize,
    /// Loop tops at or below this are already in the run.
    frontier: usize,
    grid_next: usize,
    chunk: usize,
    /// The last loop top before TO is at or after this.
    near_from: usize,
    /// Pause at the first loop top at or after this.
    pause_at: usize,
    /// Closes since the last state taken (or the scan's start at a state).
    seg_min: i64,
    /// Closes since [`Self::start_query_min`].
    pub(super) query_min: i64,
    taken: Vec<CanonSnap>,
    near_end: Option<CanonSnap>,
    /// The last grid state taken (`floor` when none).
    grid_taken: usize,
    log: DescriptorLog,
}

impl CanonScan {
    fn new(floor: usize, run: &CanonicalRun, to_char: usize, pause_at: usize) -> Self {
        let chunk = chunk_chars();
        let frontier = run.frontier();
        Self {
            floor,
            frontier,
            grid_next: run.grid_frontier.max(floor).saturating_add(chunk),
            chunk,
            near_from: to_char.saturating_sub(2),
            pause_at,
            seg_min: i64::MAX,
            query_min: i64::MAX,
            taken: Vec::new(),
            near_end: None,
            grid_taken: floor,
            log: DescriptorLog::default(),
        }
    }

    /// Start counting the adopted query's closes here.
    fn start_query_min(&mut self) {
        self.query_min = i64::MAX;
    }

    fn next_target(&self, at: usize) -> usize {
        let record_from = self.floor.max(self.frontier) + 1;
        let next = if at >= self.near_from {
            at + 1
        } else {
            self.grid_next.min(self.near_from)
        };
        next.max(record_from).min(self.pause_at)
    }
}

impl ScanMode for CanonScan {
    const ACTIVE: bool = true;
    const RECORD_DESCRIPTORS: bool = true;
    const TRACK_CLOSES: bool = true;

    fn first_target(&self) -> usize {
        let record_from = self.floor.max(self.frontier) + 1;
        self.grid_next
            .min(self.near_from)
            .max(record_from)
            .min(self.pause_at)
    }

    fn at_loop_top(&mut self, top: LoopTop<'_>) -> TopAction {
        let at = top.char_pos;
        if at >= self.pause_at {
            return TopAction::Pause;
        }
        if at > self.floor && at > self.frontier {
            if at >= self.near_from {
                // The last loop top before TO, as far as this one knows:
                // it replaces an earlier candidate, whose closes it covers.
                let closes_min = self
                    .near_end
                    .take()
                    .map_or(self.seg_min, |earlier| earlier.closes_min.min(self.seg_min));
                self.near_end = Some(CanonSnap {
                    state: top.to_state(),
                    closes_min,
                });
                self.seg_min = i64::MAX;
            } else if at >= self.grid_next {
                self.taken.push(CanonSnap {
                    state: top.to_state(),
                    closes_min: self.seg_min,
                });
                self.seg_min = i64::MAX;
                self.grid_taken = at;
                self.grid_next = at.saturating_add(self.chunk);
            }
        }
        TopAction::Continue(self.next_target(at))
    }

    fn descriptors_read(&mut self, log: DescriptorLog) {
        // A paused scan resumes under a fresh property cache: keep both logs.
        for (pos, value) in log.entries {
            self.log.note(pos, value);
        }
        if let Some(at) = log.unvalidatable_from {
            self.log.unvalidatable_from = Some(
                self.log
                    .unvalidatable_from
                    .map_or(at, |known| known.min(at)),
            );
        }
    }

    #[inline]
    fn closed(&mut self, depth: i64) {
        self.seg_min = self.seg_min.min(depth);
        self.query_min = self.query_min.min(depth);
    }
}

/// A canonical scan from `entry` to `to_char` under `mode`.
#[allow(clippy::too_many_arguments)]
fn canon_scan(
    buf: &Buffer,
    table: &SyntaxTable,
    entry: Entry,
    to_char: usize,
    props: SyntaxProperties<'_>,
    escape_policy: CommentEndEscapePolicy,
    mode: &mut CanonScan,
) -> ScanEnd {
    run_parse_loop(
        buf,
        table,
        entry,
        to_char,
        None,
        false,
        CommentStopMode::None,
        props,
        escape_policy,
        mode,
    )
}

/// The canonical run for `env`, made (or remade) empty when the buffer text
/// has none for it.
fn canonical_for(cache: &mut SyntaxParseCache, env: EnvKey) -> &mut CanonicalRun {
    if cache.canonical.as_ref().is_none_or(|run| run.env != env) {
        if cache.canonical.is_some() {
            count(|stats| stats.canon_resets += 1);
        }
        cache.canonical = Some(CanonicalRun::new(env));
    }
    cache.canonical.as_mut().expect("just made")
}

/// The descriptors an answer from the canonical run read (every one the run
/// read before `to_char`), for the exact memo of the query's own L1 run:
/// placed at FROM or later, so that any change the answer could depend on
/// truncates that run.
fn descriptors_for_answer(run: &CanonicalRun, from_char: usize, to_char: usize) -> Vec<Descriptor> {
    run.descriptors
        .iter()
        .filter(|descriptor| descriptor.first_char < to_char)
        .map(|descriptor| Descriptor {
            first_char: descriptor.first_char.max(from_char),
            ..*descriptor
        })
        .collect()
}

/// An answer from the canonical run: the finished state, the stop position
/// and the descriptors it depends on (for the query's exact memo).
/// Owned by one query and transferred to the buffer cache under the storage's
/// exclusive cache access, without retaining a thread-specific Lisp root.
pub(super) struct CanonAnswer {
    pub(super) finish: ScanFinish,
    pub(super) descriptors: Vec<Descriptor>,
    /// Absolute zero-based character position from which a contributing scan
    /// read syntax that cannot be validated. `usize::MAX` means no such read.
    /// Store an exact result only when its `dep_end_char <= dep_limit`, then
    /// check that its descriptors also fit the receiving L1 run's capacity.
    pub(super) dep_limit: usize,
}

/// Whether a query has the options of the canonical scan. L1 also caches
/// optioned queries, so both absolute and adopted L2 paths check this explicitly.
#[inline]
pub(super) fn has_no_options(key: &RunKey) -> bool {
    key.target_depth.is_none() && !key.stop_before && key.commentstop == CommentStopMode::None
}

/// Whether a query's key is the canonical scan itself.
#[inline]
pub(super) fn is_absolute(key: &RunKey) -> bool {
    has_no_options(key) && key.from_char == key.env.begv && key.start == PartialParseState::new()
}

/// An absolute query (see [`is_absolute`]) from the canonical run.
pub(super) fn absolute_answer(
    buf: &Buffer,
    table: &SyntaxTable,
    env: EnvKey,
    to_char: usize,
    props: SyntaxProperties<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> CanonAnswer {
    let begv = env.begv;
    let (base, mut mode) = buf.with_syntax_parse_cache(|cache, _| {
        let run = canonical_for(cache, env);
        let base = run
            .validated_below(to_char, false)
            .map(|index| run.snaps[index].clone());
        let floor = base.as_ref().map_or(begv, CanonSnap::pos);
        let mode = CanonScan::new(floor, run, to_char, usize::MAX);
        (base, mode)
    });
    let entry = match base {
        Some(snap) => {
            count(|stats| stats.canon_skipped_chars += (snap.pos() - begv) as u64);
            Entry::Resume {
                at: snap.state,
                first_syntax: None,
            }
        }
        None => Entry::Fresh {
            from_char: begv,
            state: PartialParseState::new(),
            from_oldstate: false,
        },
    };
    count(|stats| stats.canon_absolute += 1);
    let finish = finished(canon_scan(
        buf,
        table,
        entry,
        to_char,
        props,
        escape_policy,
        &mut mode,
    ));
    let (descriptors, dep_limit) = buf.with_syntax_parse_cache(|cache, _| {
        let run = canonical_for(cache, env);
        let dep_limit = run.merge(mode);
        (descriptors_for_answer(run, begv, to_char), dep_limit)
    });
    CanonAnswer {
        finish,
        descriptors,
        dep_limit,
    }
}

/// Back-comment lossage from an already warm canonical run, or `None` when
/// the legacy safe-position path should handle this query.
///
/// Like an absolute query, this resumes the complete BEGV parse state, including
/// string/comment nesting and a pending atom, strictly before TO. It neither
/// creates a cold run nor copies descriptors into an L1 memo. A query within
/// the known run scans its tail plainly; a query beyond the frontier records
/// only its necessary tail and extends the canonical run.
///
/// The querying mutator owns the selected state and temporary scan. Cache
/// borrows end before scanning. The current storage is mutator-confined; any
/// future shared-text mutators must hold an outer access guard across validation,
/// scanning and merging, excluding text/property writes and writes to the syntax
/// char-table or descriptor conses, including writers through another buffer.
/// A RefCell cache borrow alone does not synchronize those mutable dependencies.
/// This helper runs no Lisp.
pub(super) fn back_comment_finish(
    buf: &Buffer,
    table: &SyntaxTable,
    env: EnvKey,
    to_char: usize,
    props: SyntaxProperties<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> Option<ScanFinish> {
    let (at, mode) = buf.with_syntax_parse_cache(|cache, _| {
        let run = cache.canonical.as_mut()?;
        if run.env != env {
            return None;
        }
        let index = run.validated_below(to_char, false)?;
        let snap = run.snaps[index].clone();
        let mode = if to_char > run.frontier() {
            // The greatest state below a TO beyond the frontier is the
            // frontier itself. Segment minima must begin there, not at an
            // earlier state, when new canonical snapshots are published.
            debug_assert_eq!(snap.pos(), run.frontier());
            Some(CanonScan::new(snap.pos(), run, to_char, usize::MAX))
        } else {
            None
        };
        Some((snap.state, mode))
    })?;
    let floor = at.char_pos;
    count(|stats| {
        stats.canon_back_comments += 1;
        stats.canon_skipped_chars += (floor - env.begv) as u64;
    });
    let entry = Entry::Resume {
        at,
        first_syntax: None,
    };
    let finish = match mode {
        None => finished(run_parse_loop(
            buf,
            table,
            entry,
            to_char,
            None,
            false,
            CommentStopMode::None,
            props,
            escape_policy,
            &mut Plain,
        )),
        Some(mut mode) => {
            let finish = finished(canon_scan(
                buf,
                table,
                entry,
                to_char,
                props,
                escape_policy,
                &mut mode,
            ));
            buf.with_syntax_parse_cache(|cache, invalidation| {
                // No Lisp runs in this scan. Do not publish a temporary plan
                // if future plumbing nevertheless invalidates it meanwhile.
                if invalidation == Invalidation::Nothing
                    && let Some(run) = cache.canonical.as_mut()
                    && run.env == env
                    && run.frontier() == floor
                {
                    // The segment minima began at the captured frontier. Do
                    // not append them after any intervening canonical growth.
                    // merge applies the descriptor/dependency limit to every
                    // newly retained snapshot. There is no L1 exact result,
                    // so its returned dep_limit has no second consumer here.
                    let _ = run.merge(mode);
                }
            });
            finish
        }
    };
    Some(finish)
}

/// How far below FROM the canonical run may end for a query to extend it up
/// to FROM first (in chunks, and never more than the query's own span).
const ADOPT_REACH_CHUNKS: usize = 4;

/// Whether an internalized OLDSTATE can agree with a canonical state at all
/// (see the module documentation): no string entered in OLDSTATE, nothing
/// quoted, no two-character construct pending across FROM, and levels as
/// `internalize_parse_state` builds them.
fn adoptable_start(start: &PartialParseState) -> bool {
    start.in_string.is_none()
        && !start.quoted
        && !start.in_string_from_oldstate
        && start.prev_syntax == PARSE_PREV_SYNTAX_SMAX
        && start.mindepth == start.depth
        && start
            .levels
            .last()
            .is_some_and(|level| level.last.is_none() && level.prev.is_none())
        && start.levels.iter().all(|level| level.prev.is_none())
}

/// Whether the canonical scan at loop top FROM (`canon`) and a query starting
/// there in `start` take the same step at every later character.
fn agrees(start: &PartialParseState, canon: &LoopState) -> bool {
    let c = &canon.pps;
    canon.atom_start.is_none()
        && canon.comment_resume.is_none()
        && c.depth == start.depth
        && c.in_string.is_none()
        && !c.quoted
        && c.in_comment == start.in_comment
        && c.comment_or_string_start == start.comment_or_string_start
        && c.levels.len() == start.levels.len()
        && c.levels[..c.levels.len() - 1]
            .iter()
            .zip(&start.levels)
            .all(|(canon, start)| canon.last == start.last)
}

/// The query's answer from the canonical scan's (`answer`, finished at TO):
/// its minimum depth over [FROM, TO] is `query_min`, and the level fields the
/// scan never set between FROM (`canon_from`) and TO keep the query's
/// starting values (`start`).
fn correct(
    answer: &mut PartialParseState,
    canon_from: &PartialParseState,
    start: &PartialParseState,
    query_min: i64,
) {
    answer.mindepth = query_min;
    // The stack is lowest where the depth is: it shrinks by one level per
    // close paren until only the top level is left.
    let height_from = canon_from.levels.len() as i64;
    let lowest = (height_from + (query_min - canon_from.depth)).max(1) as usize;
    for index in 0..lowest.min(answer.levels.len()) {
        let from = &canon_from.levels[index];
        let level = &mut answer.levels[index];
        if level.last == from.last {
            level.last = start.levels[index].last;
        }
        if level.prev == from.prev {
            level.prev = start.levels[index].prev;
        }
    }
}

/// A plan for an adopted query, made under the cache borrow.
/// Temporary data owned by the querying mutator; cloned snapshots belong to
/// this plan until consumed and do not hold a cache borrow across the scan.
struct AdoptPlan {
    /// The greatest canonical state at or before FROM (BEGV when none).
    base: Option<CanonSnap>,
    /// For a jump: the first state after FROM, the last before TO, and the
    /// lowest post-close depth between them.
    jump: Option<(CanonSnap, CanonSnap, i64)>,
    mode: CanonScan,
}

/// A query with no options from FROM > BEGV (or a non-fresh state) through
/// the canonical run, or `None` when the run cannot answer it for less than
/// L1 would (`l1_from`: where L1 would start scanning).
#[allow(clippy::too_many_arguments)]
pub(super) fn adopted_answer(
    buf: &Buffer,
    table: &SyntaxTable,
    key: &RunKey,
    to_char: usize,
    props: SyntaxProperties<'_>,
    escape_policy: CommentEndEscapePolicy,
    l1_from: usize,
) -> Option<CanonAnswer> {
    let from_char = key.from_char;
    let start = &key.start;
    if !has_no_options(key) || !adoptable_start(start) || to_char <= from_char {
        return None;
    }
    let env = key.env;
    let begv = env.begv;
    let chunk = chunk_chars();
    let span = to_char - from_char;
    let plan = buf.with_syntax_parse_cache(|cache, _| {
        let run = canonical_for(cache, env);
        let base = run
            .validated_below(from_char, true)
            .map(|index| run.snaps[index].clone());
        let base_pos = base.as_ref().map_or(begv, CanonSnap::pos);
        // Reach FROM only through text the run nearly covers.
        let gap = from_char - base_pos;
        if gap > chunk.saturating_mul(ADOPT_REACH_CHUNKS) || gap > span {
            return None;
        }
        // Skip [FROM, TO) through the run when it holds states in it.
        let jump = run.validated_below(to_char, false).and_then(|last| {
            let first = run.snaps.partition_point(|snap| snap.pos() <= from_char);
            (first <= last).then(|| {
                let between = run.snaps[first + 1..=last]
                    .iter()
                    .map(|snap| snap.closes_min)
                    .min()
                    .unwrap_or(i64::MAX);
                (run.snaps[first].clone(), run.snaps[last].clone(), between)
            })
        });
        let cost = gap
            + jump
                .as_ref()
                .map_or(span, |(_, last, _)| to_char - last.pos());
        if l1_from > from_char && to_char - l1_from <= cost {
            return None;
        }
        let mode = CanonScan::new(base_pos, run, to_char, from_char);
        Some(AdoptPlan { base, jump, mode })
    })?;
    let AdoptPlan {
        base,
        jump,
        mut mode,
    } = plan;

    // The canonical state at FROM.
    let canon_from = match base {
        Some(snap) if snap.pos() == from_char => snap.state,
        base => {
            let entry = match base {
                Some(snap) => Entry::Resume {
                    at: snap.state,
                    first_syntax: None,
                },
                None => Entry::Fresh {
                    from_char: begv,
                    state: PartialParseState::new(),
                    from_oldstate: false,
                },
            };
            match canon_scan(buf, table, entry, to_char, props, escape_policy, &mut mode) {
                ScanEnd::Paused(state) if state.char_pos == from_char => state,
                _ => return decline(buf, env, mode),
            }
        }
    };
    if !agrees(start, &canon_from) {
        return decline(buf, env, mode);
    }
    count(|stats| stats.canon_adopted += 1);
    mode.pause_at = usize::MAX;
    mode.floor = mode.floor.max(from_char);
    mode.start_query_min();
    let canon_pps_from = canon_from.pps.clone();
    let depth_from = canon_pps_from.depth;

    let mut modes = Vec::with_capacity(2);
    let (finish, query_min) = match jump {
        Some((first, last, between)) => {
            // The closes of [FROM, first) matter only when they could go
            // below everything else known; the first state's own minimum
            // (over the whole stretch that holds FROM) bounds them.
            let before_first = if first.closes_min >= depth_from.min(between) {
                i64::MAX
            } else {
                mode.pause_at = first.pos();
                match canon_scan(
                    buf,
                    table,
                    Entry::Resume {
                        at: canon_from,
                        first_syntax: None,
                    },
                    to_char,
                    props,
                    escape_policy,
                    &mut mode,
                ) {
                    ScanEnd::Paused(state) if state.char_pos == first.pos() => {}
                    _ => unreachable!("the canonical scan reaches its own states"),
                }
                mode.query_min
            };
            count(|stats| stats.canon_skipped_chars += (last.pos() - from_char) as u64);
            modes.push(mode);
            let run_snapshot = buf.with_syntax_parse_cache(|cache, _| {
                let run = canonical_for(cache, env);
                CanonScan::new(last.pos(), run, to_char, usize::MAX)
            });
            let mut tail = run_snapshot;
            let finish = finished(canon_scan(
                buf,
                table,
                Entry::Resume {
                    at: last.state,
                    first_syntax: None,
                },
                to_char,
                props,
                escape_policy,
                &mut tail,
            ));
            let query_min = depth_from
                .min(before_first)
                .min(between)
                .min(tail.query_min);
            modes.push(tail);
            (finish, query_min)
        }
        None => {
            let finish = finished(canon_scan(
                buf,
                table,
                Entry::Resume {
                    at: canon_from,
                    first_syntax: None,
                },
                to_char,
                props,
                escape_policy,
                &mut mode,
            ));
            let query_min = depth_from.min(mode.query_min);
            modes.push(mode);
            (finish, query_min)
        }
    };
    let mut finish = finish;
    correct(&mut finish.state, &canon_pps_from, start, query_min);
    let (descriptors, dep_limit) = buf.with_syntax_parse_cache(|cache, _| {
        let run = canonical_for(cache, env);
        let mut dep_limit = usize::MAX;
        for mode in modes {
            dep_limit = dep_limit.min(run.merge(mode));
        }
        (descriptors_for_answer(run, from_char, to_char), dep_limit)
    });
    Some(CanonAnswer {
        finish,
        descriptors,
        dep_limit,
    })
}

/// No adoption: keep what the canonical scan saw on the way.
fn decline(buf: &Buffer, env: EnvKey, mode: CanonScan) -> Option<CanonAnswer> {
    count(|stats| stats.canon_declined += 1);
    buf.with_syntax_parse_cache(|cache, _| canonical_for(cache, env).merge(mode));
    None
}

/// Query-owned result. No cache borrow spans a scan; future shared writers
/// need the outer text/property/table/descriptor guard from the module docs.
pub(super) enum LiveAnswer {
    Finished { finish: ScanFinish, record: Record },
    Synced { answer: CanonAnswer, record: Record },
}

fn append_live_log(known: &mut DescriptorLog, log: DescriptorLog) {
    for (at, value) in log.entries {
        known.note(at, value);
    }
    if let Some(at) = log.unvalidatable_from {
        known.unvalidatable_from = Some(known.unvalidatable_from.map_or(at, |old| old.min(at)));
    }
}

/// An already existing validated canonical top strictly after the position.
/// This neither creates a run nor scans text.
fn live_candidate(buf: &Buffer, env: EnvKey, after: usize, to_char: usize) -> Option<CanonSnap> {
    buf.with_syntax_parse_cache(|cache, _| {
        let run = cache.canonical.as_mut()?;
        if run.env != env {
            return None;
        }
        let last = run.validated_below(to_char, false)?;
        let index = run.snaps.partition_point(|snap| snap.pos() <= after);
        (index <= last).then(|| run.snaps[index].clone())
    })
}

/// Temporary canonical continuation, owned by this query. Its dependency log
/// and segment minima begin at the old frontier. No speculative state enters
/// the canonical run before the actual query agrees at one of these tops.
struct LiveGrowth {
    scan: CanonScan,
    tops: Vec<CanonSnap>,
    old_frontier: usize,
}

#[allow(clippy::too_many_arguments)]
fn live_growth(
    buf: &Buffer,
    table: &SyntaxTable,
    key: &RunKey,
    floor: usize,
    to_char: usize,
    props: SyntaxProperties<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> Option<LiveGrowth> {
    let budget = chunk_chars()
        .saturating_mul(ADOPT_REACH_CHUNKS)
        .min(to_char - key.from_char);
    let (base, mut scan, probe_end) = buf.with_syntax_parse_cache(|cache, _| {
        let run = cache.canonical.as_mut()?;
        if run.env != key.env {
            return None;
        }
        let index = run.validated_below(to_char, false)?;
        let base = run.snaps[index].clone();
        // Existing future states should be probed without an extra scan.
        // Do not build a cold canonical run, nor reach a remote query start.
        if base.pos() > floor || run.frontier() != base.pos() {
            return None;
        }
        let probe_end = base.pos().saturating_add(budget).min(to_char);
        if probe_end <= floor.saturating_add(1) {
            return None;
        }
        let scan = CanonScan::new(base.pos(), run, probe_end, usize::MAX);
        Some((base, scan, probe_end))
    })?;
    let old_frontier = base.pos();
    let _ = finished(canon_scan(
        buf,
        table,
        Entry::Resume {
            at: base.state,
            first_syntax: None,
        },
        probe_end,
        props,
        escape_policy,
        &mut scan,
    ));
    let tops = scan
        .taken
        .iter()
        .chain(scan.near_end.iter())
        .cloned()
        .collect::<Vec<_>>();
    if tops.iter().all(|top| top.pos() <= floor) {
        return None;
    }
    Some(LiveGrowth {
        scan,
        tops,
        old_frontier,
    })
}

/// One actual-query scan with the ordinary L1 recorder. It owns one existing
/// candidate at a time, or a bounded vector of temporary growth candidates.
/// The Buffer reference provides access within the current mutator, not
/// synchronization with other text/table/descriptor writers.
struct LiveSync<'a> {
    buf: &'a Buffer,
    env: EnvKey,
    to_char: usize,
    record: Record,
    record_target: usize,
    candidate: Option<CanonSnap>,
    matched: Option<CanonSnap>,
    growth: Option<LiveGrowth>,
}

impl<'a> LiveSync<'a> {
    fn new(
        buf: &'a Buffer,
        env: EnvKey,
        floor: usize,
        grid_frontier: usize,
        to_char: usize,
        growth: Option<LiveGrowth>,
    ) -> Self {
        let record = Record::new(floor, grid_frontier, to_char);
        let record_target = record.first_target();
        let mut mode = Self {
            buf,
            env,
            to_char,
            record,
            record_target,
            candidate: None,
            matched: None,
            growth,
        };
        // Hooks strictly past entry see neither unconsumed Fresh-entry
        // comment_resume nor OLDSTATE entry-prefix logic.
        mode.candidate = mode.next_candidate(floor);
        mode
    }

    fn next_candidate(&self, after: usize) -> Option<CanonSnap> {
        match &self.growth {
            Some(growth) => {
                let index = growth.tops.partition_point(|snap| snap.pos() <= after);
                growth.tops.get(index).cloned()
            }
            None => live_candidate(self.buf, self.env, after, self.to_char),
        }
    }

    fn target(&self) -> usize {
        self.record_target
            .min(self.candidate.as_ref().map_or(usize::MAX, CanonSnap::pos))
    }

    fn disable_sync(&mut self) {
        self.candidate = None;
        self.matched = None;
        self.growth = None;
    }
}

impl ScanMode for LiveSync<'_> {
    const ACTIVE: bool = true;
    const RECORD_DESCRIPTORS: bool = true;

    fn first_target(&self) -> usize {
        self.target()
    }

    fn at_loop_top(&mut self, top: LoopTop<'_>) -> TopAction {
        let at = top.char_pos;
        // A paired delimiter can jump past a candidate. Only common tops
        // can agree, so find the first candidate at or after the actual top.
        if self.candidate.as_ref().is_some_and(|snap| snap.pos() < at) {
            self.candidate = self.next_candidate(at - 1);
        }
        let mut agrees = false;
        if let Some(candidate) = &self.candidate
            && candidate.pos() == at
        {
            let mut actual = top.to_state();
            // Compare every current and future Eq field, ignoring ONLY the
            // accumulated minimum. The Paused actual state keeps its own min.
            actual.pps.mindepth = candidate.state.pps.mindepth;
            agrees = actual == candidate.state;
        }
        // Extra canonical probes must not inflate L1 recording density.
        if at >= self.record_target {
            self.record_target = match self.record.at_loop_top(top) {
                TopAction::Continue(next) => next,
                TopAction::Pause => unreachable!("the L1 recorder never pauses"),
            };
        }
        if agrees {
            self.matched = self.candidate.take();
            return TopAction::Pause;
        }
        if self.candidate.as_ref().is_some_and(|snap| snap.pos() <= at) {
            self.candidate = self.next_candidate(at);
        }
        TopAction::Continue(self.target())
    }

    fn descriptors_read(&mut self, log: DescriptorLog) {
        // An invalidated plan can resume the actual query under this mode;
        // retain both scans' descriptor observations across that pause.
        append_live_log(&mut self.record.log, log);
    }
}

/// A jump whose endpoints and minima are owned by this query.
struct LiveJump {
    last: CanonSnap,
    between: i64,
    tail: CanonScan,
}

fn live_jump(
    buf: &Buffer,
    env: EnvKey,
    matched: &CanonSnap,
    to_char: usize,
    growth: Option<&LiveGrowth>,
) -> Option<LiveJump> {
    buf.with_syntax_parse_cache(|cache, invalidation| {
        if invalidation != Invalidation::Nothing {
            return None;
        }
        let run = cache.canonical.as_mut()?;
        if run.env != env {
            return None;
        }
        let (last, between) = match growth {
            Some(growth) => {
                // No Lisp runs; a changed frontier means future mutation
                // plumbing invalidated the speculative plan.
                if run.frontier() != growth.old_frontier {
                    return None;
                }
                let first = growth
                    .tops
                    .partition_point(|snap| snap.pos() < matched.pos());
                let last = growth.tops.len().checked_sub(1)?;
                if first > last || growth.tops[first].state != matched.state {
                    return None;
                }
                let between = growth.tops[first + 1..=last]
                    .iter()
                    .map(|snap| snap.closes_min)
                    .min()
                    .unwrap_or(i64::MAX);
                (growth.tops[last].clone(), between)
            }
            None => {
                let last = run.validated_below(to_char, false)?;
                let first = run.snaps.partition_point(|snap| snap.pos() < matched.pos());
                if first > last || run.snaps[first].state != matched.state {
                    return None;
                }
                let between = run.snaps[first + 1..=last]
                    .iter()
                    .map(|snap| snap.closes_min)
                    .min()
                    .unwrap_or(i64::MAX);
                (run.snaps[last].clone(), between)
            }
        };
        let tail = CanonScan::new(last.pos(), run, to_char, usize::MAX);
        Some(LiveJump {
            last,
            between,
            tail,
        })
    })
}

/// A complete actual query, optionally jumping after full live equality.
/// Temporary canonical growth is bounded and published only after equality.
/// All borrows end before scans; no Lisp runs in any of these scans.
#[allow(clippy::too_many_arguments)]
pub(super) fn live_sync_answer(
    buf: &Buffer,
    table: &SyntaxTable,
    key: &RunKey,
    entry: Entry,
    grid_frontier: usize,
    to_char: usize,
    props: SyntaxProperties<'_>,
    escape_policy: CommentEndEscapePolicy,
) -> LiveAnswer {
    debug_assert!(has_no_options(key));
    let floor = match &entry {
        Entry::Fresh { from_char, .. } => *from_char,
        Entry::Resume { at, .. } => at.char_pos,
    };
    let growth = live_growth(buf, table, key, floor, to_char, props, escape_policy);
    let mut mode = LiveSync::new(buf, key.env, floor, grid_frontier, to_char, growth);
    if mode.candidate.is_none() {
        // No Lisp runs and no canonical states are created during a Record
        // scan, so this query cannot acquire a candidate later. Keep its
        // ordinary L1 monomorphized loop rather than the LiveSync loop.
        let mut record = mode.record;
        let finish = finished(run_parse_loop(
            buf,
            table,
            entry,
            to_char,
            None,
            false,
            CommentStopMode::None,
            props,
            escape_policy,
            &mut record,
        ));
        count(|stats| stats.canon_declined += 1);
        return LiveAnswer::Finished { finish, record };
    }
    let scan = run_parse_loop(
        buf,
        table,
        entry,
        to_char,
        None,
        false,
        CommentStopMode::None,
        props,
        escape_policy,
        &mut mode,
    );
    let actual = match scan {
        ScanEnd::Finished(finish) => {
            count(|stats| stats.canon_declined += 1);
            return LiveAnswer::Finished {
                finish,
                record: mode.record,
            };
        }
        ScanEnd::Paused(actual) => actual,
    };
    let matched = mode.matched.take().expect("only live agreement pauses");
    let Some(LiveJump {
        last,
        between,
        mut tail,
    }) = live_jump(buf, key.env, &matched, to_char, mode.growth.as_ref())
    else {
        // Continue the original query; do not restart or lose its prefix log.
        mode.disable_sync();
        let finish = finished(run_parse_loop(
            buf,
            table,
            Entry::Resume {
                at: actual,
                first_syntax: None,
            },
            to_char,
            None,
            false,
            CommentStopMode::None,
            props,
            escape_policy,
            &mut mode,
        ));
        count(|stats| stats.canon_declined += 1);
        return LiveAnswer::Finished {
            finish,
            record: mode.record,
        };
    };
    count(|stats| {
        stats.canon_synced += 1;
        stats.canon_adopted += 1;
        stats.canon_skipped_chars += (last.pos() - actual.char_pos) as u64;
    });
    let prefix_min = actual.pps.mindepth;
    let mut finish = finished(canon_scan(
        buf,
        table,
        Entry::Resume {
            at: last.state,
            first_syntax: None,
        },
        to_char,
        props,
        escape_policy,
        &mut tail,
    ));
    // All other fields were equal at sync and evolve identically afterward.
    finish.state.mindepth = prefix_min.min(between).min(tail.query_min);
    let (mut descriptors, mut dep_limit) = buf.with_syntax_parse_cache(|cache, invalidation| {
        if invalidation != Invalidation::Nothing {
            return (Vec::new(), 0);
        }
        let Some(run) = cache.canonical.as_mut().filter(|run| run.env == key.env) else {
            return (Vec::new(), 0);
        };
        let mut limit = usize::MAX;
        if let Some(growth) = mode.growth.take() {
            if run.frontier() != growth.old_frontier {
                return (Vec::new(), 0);
            }
            limit = run.merge(growth.scan);
            // A rejected speculative prefix cannot be bypassed by publishing
            // later tail snapshots whose own local log does not see it.
            if limit != usize::MAX {
                tail.log.unvalidatable_from = Some(
                    tail.log
                        .unvalidatable_from
                        .map_or(limit, |old| old.min(limit)),
                );
            }
        }
        limit = limit.min(run.merge(tail));
        (descriptors_for_answer(run, key.from_char, to_char), limit)
    });
    // Actual-prefix descriptors can be absent from the canonical dictionary:
    // the actual scan can initially interpret or skip different characters.
    // Union them conservatively and apply the COMBINED capacity/dependency
    // budget before publishing an exact memo. Keep the original Record log.
    let prefix_log = DescriptorLog {
        entries: mode.record.log.entries.clone(),
        unvalidatable_from: mode.record.log.unvalidatable_from,
    };
    dep_limit = dep_limit.min(merge_log(&mut descriptors, prefix_log));
    LiveAnswer::Synced {
        answer: CanonAnswer {
            finish,
            descriptors,
            dep_limit,
        },
        record: mode.record,
    }
}
