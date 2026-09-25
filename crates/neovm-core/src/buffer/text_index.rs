//! The text line index (P3.4 design B, stages B1 and B2).
//!
//! An incrementally maintained summary of a buffer's text, cut into chunks
//! of a few KiB of logical Emacs bytes. Each chunk records its bytes, its
//! characters, its `\n`s and its `\r`s, and a Fenwick tree over the chunks
//! gives the sums before any chunk in O(log n). The newline scans behind
//! `forward-line N`, `line-number-at-pos`, `count-lines`, the mode-line
//! `%l`, the line-number gutter and the tree-sitter row become a descent plus
//! a scan of at most one chunk, instead of a scan from the start.
//!
//! GNU has no such index: `find_newline` (search.c) scans, helped by a
//! region cache. Every answer here is a pure function of the text, so the
//! index cannot change a result; `NEOVM_TEXT_LINE_INDEX=verify` recomputes
//! each answer by scanning and reports any disagreement.
//!
//! Coordinates are logical Emacs byte positions, so moving the gap changes
//! nothing. The index is kept by [`super::buffer_text::BufferText`]: its
//! four measured mutators update it, every wholesale mutator drops it, and a
//! line query on a large enough buffer builds it again.
//!
//! - A character is counted at its lead byte (`(b & 0xC0) != 0x80` in a
//!   multibyte buffer, every byte in a unibyte one), as the chunked position
//!   scans do. A chunk boundary may therefore split a multibyte sequence and
//!   the counts stay additive.
//! - Neither `\n` (0x0A) nor `\r` (0x0D) ever occurs inside a multibyte
//!   sequence, so counting those bytes counts those characters.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use super::buffer_text::LineEnd;
use super::position::{EmacsBytePos, EmacsByteRange};
use super::text::backend::TextBackend;

// ---------------------------------------------------------------------------
// Knobs
// ---------------------------------------------------------------------------

/// What `NEOVM_TEXT_LINE_INDEX` selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextLineIndexMode {
    /// The default: no index is ever built, and line queries scan.
    Off,
    /// Line queries on large buffers use the index.
    On,
    /// Like `On`, but every index answer is recomputed by scanning, every
    /// edit recounts the chunks it touched, and a disagreement is reported
    /// (`tracing::error!`, a panic in debug builds). The scanned answer is
    /// the one returned.
    Verify,
}

impl TextLineIndexMode {
    /// The mode a value of `NEOVM_TEXT_LINE_INDEX` selects: `on`/`1`/`true`/
    /// `yes`, `verify`, anything else (or unset) off.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("verify") => Self::Verify,
            Some("1" | "on" | "true" | "yes") => Self::On,
            _ => Self::Off,
        }
    }
}

/// Every setting of the index, read once per process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextLineIndexConfig {
    pub(crate) mode: TextLineIndexMode,
    /// A buffer with fewer bytes never builds an index: scanning it is
    /// already cheap (`NEOVM_TEXT_LINE_INDEX_MIN_BYTES`).
    pub(crate) min_buffer_bytes: usize,
    /// The chunk size the index aims for. A chunk splits above twice this
    /// and merges into a neighbour below a quarter of it
    /// (`NEOVM_TEXT_LINE_INDEX_CHUNK`).
    pub(crate) chunk_bytes: usize,
    /// A line count over a shorter range scans
    /// (`NEOVM_TEXT_LINE_INDEX_QUERY_BYTES`).
    pub(crate) min_query_bytes: usize,
    /// A `forward-line` of at most this many lines scans
    /// (`NEOVM_TEXT_LINE_INDEX_QUERY_LINES`).
    pub(crate) min_query_lines: usize,
    /// Count builds, drops, copies and served queries, and print them at
    /// exit (`NEOVM_TEXT_LINE_INDEX_STATS=1`).
    pub(crate) stats: bool,
}

impl TextLineIndexConfig {
    pub(crate) const DEFAULT_MIN_BUFFER_BYTES: usize = 64 * 1024;
    pub(crate) const DEFAULT_CHUNK_BYTES: usize = 4 * 1024;
    pub(crate) const DEFAULT_MIN_QUERY_BYTES: usize = 8 * 1024;
    pub(crate) const DEFAULT_MIN_QUERY_LINES: usize = 64;
    /// The smallest chunk target accepted: a quarter of it is the merge
    /// floor, which must stay at least one byte.
    pub(crate) const MIN_CHUNK_BYTES: usize = 8;

    /// The defaults with MODE.
    #[cfg(test)]
    pub(crate) const fn with_mode(mode: TextLineIndexMode) -> Self {
        Self {
            mode,
            min_buffer_bytes: Self::DEFAULT_MIN_BUFFER_BYTES,
            chunk_bytes: Self::DEFAULT_CHUNK_BYTES,
            min_query_bytes: Self::DEFAULT_MIN_QUERY_BYTES,
            min_query_lines: Self::DEFAULT_MIN_QUERY_LINES,
            stats: false,
        }
    }

    /// Settings that make every buffer and every query use the index, with
    /// chunks of CHUNK_BYTES: what tests and verify soaks want.
    #[cfg(test)]
    pub(crate) const fn eager(mode: TextLineIndexMode, chunk_bytes: usize) -> Self {
        Self {
            mode,
            min_buffer_bytes: 0,
            chunk_bytes,
            min_query_bytes: 0,
            min_query_lines: 0,
            stats: false,
        }
    }

    /// The settings the given environment values select.
    pub(crate) fn parse(env: impl Fn(&str) -> Option<String>) -> Self {
        let size = |name: &str, default: usize| {
            env(name)
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(default)
        };
        Self {
            mode: TextLineIndexMode::parse(env("NEOVM_TEXT_LINE_INDEX").as_deref()),
            min_buffer_bytes: size(
                "NEOVM_TEXT_LINE_INDEX_MIN_BYTES",
                Self::DEFAULT_MIN_BUFFER_BYTES,
            ),
            chunk_bytes: size("NEOVM_TEXT_LINE_INDEX_CHUNK", Self::DEFAULT_CHUNK_BYTES)
                .max(Self::MIN_CHUNK_BYTES),
            min_query_bytes: size(
                "NEOVM_TEXT_LINE_INDEX_QUERY_BYTES",
                Self::DEFAULT_MIN_QUERY_BYTES,
            ),
            min_query_lines: size(
                "NEOVM_TEXT_LINE_INDEX_QUERY_LINES",
                Self::DEFAULT_MIN_QUERY_LINES,
            ),
            stats: matches!(
                env("NEOVM_TEXT_LINE_INDEX_STATS").as_deref().map(str::trim),
                Some("1" | "on" | "true" | "yes")
            ),
        }
    }

    #[inline]
    pub(crate) fn enabled(self) -> bool {
        self.mode != TextLineIndexMode::Off
    }

    #[inline]
    pub(crate) fn verifies(self) -> bool {
        self.mode == TextLineIndexMode::Verify
    }
}

#[cfg(test)]
thread_local! {
    static CONFIG_OVERRIDE: std::cell::Cell<Option<TextLineIndexConfig>> =
        const { std::cell::Cell::new(None) };
}

/// Run F with the index settings forced to CONFIG on this thread.
#[cfg(test)]
pub(crate) fn with_text_line_index_config<R>(
    config: TextLineIndexConfig,
    f: impl FnOnce() -> R,
) -> R {
    struct Guard(Option<TextLineIndexConfig>);
    impl Drop for Guard {
        fn drop(&mut self) {
            CONFIG_OVERRIDE.with(|slot| slot.set(self.0));
        }
    }
    let _guard = Guard(CONFIG_OVERRIDE.with(|slot| slot.replace(Some(config))));
    f()
}

/// The index settings, read once per process.
#[inline]
pub(crate) fn text_line_index_config() -> TextLineIndexConfig {
    #[cfg(test)]
    if let Some(config) = CONFIG_OVERRIDE.with(|slot| slot.get()) {
        return config;
    }
    static CONFIG: OnceLock<TextLineIndexConfig> = OnceLock::new();
    *CONFIG.get_or_init(read_text_line_index_config)
}

#[cold]
fn read_text_line_index_config() -> TextLineIndexConfig {
    let config = TextLineIndexConfig::parse(|name| std::env::var(name).ok());
    tracing::debug!(target: "neovm::text_line_index", ?config, "NEOVM_TEXT_LINE_INDEX");
    if config.stats {
        register_stats_report();
    }
    config
}

// ---------------------------------------------------------------------------
// Counters
// ---------------------------------------------------------------------------

/// Index events, counted process-wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum TextLineIndexEvent {
    /// An index was built from the whole text.
    Build,
    /// An index was dropped by a wholesale mutation or an edit too large to
    /// maintain.
    Drop,
    /// An edit copied an index a snapshot still shared.
    CowCopy,
    /// A chunk split or merge rebuilt the Fenwick tree.
    Restructure,
    /// A line count answered from the index.
    CountServed,
    /// A forward `forward-line` answered from the index.
    ForwardServed,
    /// A backward `forward-line` answered from the index.
    BackwardServed,
}

const EVENT_COUNT: usize = 7;

static EVENTS: [AtomicU64; EVENT_COUNT] = [const { AtomicU64::new(0) }; EVENT_COUNT];

/// Verify-mode disagreements, counted whatever `stats` says.
static MISMATCHES: AtomicU64 = AtomicU64::new(0);

/// Count EVENT when the stats knob is on.
#[inline]
pub(crate) fn note_event(config: TextLineIndexConfig, event: TextLineIndexEvent) {
    if config.stats {
        EVENTS[event as usize].fetch_add(1, Ordering::Relaxed);
    }
}

/// How often EVENT happened since the process started (stats knob on).
pub(crate) fn event_count(event: TextLineIndexEvent) -> u64 {
    EVENTS[event as usize].load(Ordering::Relaxed)
}

/// Verify-mode disagreements since the process started.
pub fn text_line_index_mismatches() -> u64 {
    MISMATCHES.load(Ordering::Relaxed)
}

/// Report a verify-mode disagreement: an error log, the counter, and in a
/// debug build a panic, so a test suite run in verify mode fails on the
/// first one.
#[cold]
#[inline(never)]
pub(crate) fn report_mismatch(what: &str, detail: &str) {
    MISMATCHES.fetch_add(1, Ordering::Relaxed);
    tracing::error!(
        target: "neovm::text_line_index",
        what,
        detail,
        "text line index disagrees with a scan"
    );
    if cfg!(debug_assertions) {
        panic!("text line index disagrees with a scan: {what}: {detail}");
    }
}

fn register_stats_report() {
    extern "C" fn report() {
        let line = format!(
            "[neovm-text-line-index] builds={} drops={} cow_copies={} restructures={} \
             counts={} forward={} backward={} mismatches={}\n",
            event_count(TextLineIndexEvent::Build),
            event_count(TextLineIndexEvent::Drop),
            event_count(TextLineIndexEvent::CowCopy),
            event_count(TextLineIndexEvent::Restructure),
            event_count(TextLineIndexEvent::CountServed),
            event_count(TextLineIndexEvent::ForwardServed),
            event_count(TextLineIndexEvent::BackwardServed),
            text_line_index_mismatches(),
        );
        let _ = std::io::Write::write_all(&mut std::io::stderr().lock(), line.as_bytes());
    }
    // SAFETY: `report` is an `extern "C" fn()` that only reads atomics and
    // writes one line to stderr.
    unsafe {
        libc::atexit(report);
    }
}

// ---------------------------------------------------------------------------
// The text the index describes
// ---------------------------------------------------------------------------

/// Read access to the text an index describes: the buffer's backend, or a
/// plain byte slice in tests.
pub(crate) trait IndexedText {
    fn is_multibyte(&self) -> bool;
    fn byte_len(&self) -> usize;
    /// Call F on the physical pieces of the logical byte range
    /// `[start, end)`, in order, until F returns false.
    fn for_each_piece(&self, start: usize, end: usize, f: &mut dyn FnMut(&[u8]) -> bool);
}

impl IndexedText for TextBackend {
    fn is_multibyte(&self) -> bool {
        TextBackend::is_multibyte(self)
    }

    fn byte_len(&self) -> usize {
        self.metrics().emacs_byte_len().get()
    }

    fn for_each_piece(&self, start: usize, end: usize, f: &mut dyn FnMut(&[u8]) -> bool) {
        if start >= end {
            return;
        }
        let range = EmacsByteRange::new(EmacsBytePos::new(start), EmacsBytePos::new(end));
        let _ =
            self.for_each_emacs_byte_range_chunk(
                range,
                |piece| {
                    if f(piece) { Ok(()) } else { Err(()) }
                },
            );
    }
}

/// A unibyte or multibyte byte string, for tests.
#[cfg(test)]
pub(crate) struct PlainText<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) multibyte: bool,
    /// Pieces are cut at multiples of this, like gap segments.
    pub(crate) piece: usize,
}

#[cfg(test)]
impl IndexedText for PlainText<'_> {
    fn is_multibyte(&self) -> bool {
        self.multibyte
    }

    fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    fn for_each_piece(&self, start: usize, end: usize, f: &mut dyn FnMut(&[u8]) -> bool) {
        let mut at = start;
        while at < end {
            let next = ((at / self.piece + 1) * self.piece).min(end);
            if !f(&self.bytes[at..next]) {
                return;
            }
            at = next;
        }
    }
}

// ---------------------------------------------------------------------------
// Sums
// ---------------------------------------------------------------------------

/// The sums of one span of text: what a chunk, a Fenwick node and a prefix
/// hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineSums {
    pub(crate) bytes: u64,
    pub(crate) chars: u64,
    pub(crate) newlines: u64,
    pub(crate) crs: u64,
}

impl LineSums {
    /// The sums of BYTES, a span of a unibyte or multibyte text.
    pub(crate) fn of_bytes(bytes: &[u8], multibyte: bool) -> Self {
        let chars = if multibyte {
            bytes.iter().filter(|&&b| (b & 0xC0) != 0x80).count()
        } else {
            bytes.len()
        };
        Self {
            bytes: bytes.len() as u64,
            chars: chars as u64,
            newlines: bytes.iter().filter(|&&b| b == b'\n').count() as u64,
            crs: bytes.iter().filter(|&&b| b == b'\r').count() as u64,
        }
    }

    /// The lines this span ends under LINE_END.
    #[inline]
    pub(crate) fn line_ends(self, line_end: LineEnd) -> u64 {
        match line_end {
            LineEnd::Newline => self.newlines,
            LineEnd::NewlineOrCarriageReturn => self.newlines + self.crs,
        }
    }
}

impl std::ops::Add for LineSums {
    type Output = Self;
    #[inline]
    fn add(self, other: Self) -> Self {
        Self {
            bytes: self.bytes + other.bytes,
            chars: self.chars + other.chars,
            newlines: self.newlines + other.newlines,
            crs: self.crs + other.crs,
        }
    }
}

impl std::ops::AddAssign for LineSums {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl std::ops::Sub for LineSums {
    type Output = Self;
    #[inline]
    fn sub(self, other: Self) -> Self {
        Self {
            bytes: self.bytes - other.bytes,
            chars: self.chars - other.chars,
            newlines: self.newlines - other.newlines,
            crs: self.crs - other.crs,
        }
    }
}

impl std::ops::SubAssign for LineSums {
    #[inline]
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

/// The sums of the logical byte range `[start, end)` of TEXT.
pub(crate) fn sums_in(text: &(impl IndexedText + ?Sized), start: usize, end: usize) -> LineSums {
    let multibyte = text.is_multibyte();
    let mut sums = LineSums::default();
    text.for_each_piece(start, end, &mut |piece| {
        sums += LineSums::of_bytes(piece, multibyte);
        true
    });
    sums
}

/// The line ends under LINE_END in the logical byte range `[start, end)`.
fn line_ends_in(
    text: &(impl IndexedText + ?Sized),
    start: usize,
    end: usize,
    line_end: LineEnd,
) -> u64 {
    let mut count = 0usize;
    text.for_each_piece(start, end, &mut |piece| {
        count += match line_end {
            LineEnd::Newline => piece.iter().filter(|&&b| b == b'\n').count(),
            LineEnd::NewlineOrCarriageReturn => {
                piece.iter().filter(|&&b| b == b'\n' || b == b'\r').count()
            }
        };
        true
    });
    count as u64
}

/// The position of the NTH (1-based) `\n` in the logical byte range
/// `[start, end)`, if there are that many.
fn nth_newline_in(
    text: &(impl IndexedText + ?Sized),
    start: usize,
    end: usize,
    nth: u64,
) -> Option<usize> {
    debug_assert!(nth >= 1);
    let mut remaining = nth;
    let mut base = start;
    let mut found = None;
    text.for_each_piece(start, end, &mut |piece| {
        let in_piece = piece.iter().filter(|&&b| b == b'\n').count() as u64;
        if in_piece < remaining {
            remaining -= in_piece;
            base += piece.len();
            return true;
        }
        let at = memchr::memchr_iter(b'\n', piece)
            .nth((remaining - 1) as usize)
            .expect("the piece holds the remaining newlines");
        found = Some(base + at);
        false
    });
    found
}

// ---------------------------------------------------------------------------
// The index
// ---------------------------------------------------------------------------

/// Chunk sizes: the target a build and a split cut to, the size above which
/// a chunk splits, and the size below which it merges into a neighbour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChunkGeometry {
    target: usize,
    max: usize,
    min: usize,
}

impl ChunkGeometry {
    fn new(target: usize) -> Self {
        let target = target.max(TextLineIndexConfig::MIN_CHUNK_BYTES);
        Self {
            target,
            max: target * 2,
            min: target / 4,
        }
    }
}

/// See the module documentation.
#[derive(Clone, Debug)]
pub(crate) struct TextLineIndex {
    /// The chunks in text order. None is empty.
    chunks: Vec<LineSums>,
    /// A Fenwick tree over `chunks`, 1-based: `tree[0]` is unused.
    tree: Vec<LineSums>,
    total: LineSums,
    multibyte: bool,
    geometry: ChunkGeometry,
}

#[inline]
fn lowest_bit(i: usize) -> usize {
    i & i.wrapping_neg()
}

impl TextLineIndex {
    /// Index all of TEXT in chunks of about CHUNK_BYTES.
    pub(crate) fn build(text: &(impl IndexedText + ?Sized), chunk_bytes: usize) -> Self {
        let geometry = ChunkGeometry::new(chunk_bytes);
        let multibyte = text.is_multibyte();
        let chunks = cut_chunks(text, 0, text.byte_len(), geometry.target, multibyte);
        let mut index = Self {
            chunks,
            tree: Vec::new(),
            total: LineSums::default(),
            multibyte,
            geometry,
        };
        index.rebuild_tree();
        index
    }

    pub(crate) fn total(&self) -> LineSums {
        self.total
    }

    pub(crate) fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub(crate) fn is_multibyte(&self) -> bool {
        self.multibyte
    }

    /// Recompute the tree and the total from `chunks`: O(chunks).
    fn rebuild_tree(&mut self) {
        let n = self.chunks.len();
        self.tree.clear();
        self.tree.reserve(n + 1);
        self.tree.push(LineSums::default());
        self.tree.extend_from_slice(&self.chunks);
        for i in 1..=n {
            let parent = i + lowest_bit(i);
            if parent <= n {
                let child = self.tree[i];
                self.tree[parent] += child;
            }
        }
        self.total = self
            .chunks
            .iter()
            .fold(LineSums::default(), |sum, &chunk| sum + chunk);
    }

    fn tree_add(&mut self, chunk: usize, delta: LineSums) {
        let mut i = chunk + 1;
        while i < self.tree.len() {
            self.tree[i] += delta;
            i += lowest_bit(i);
        }
        self.total += delta;
    }

    fn tree_sub(&mut self, chunk: usize, delta: LineSums) {
        let mut i = chunk + 1;
        while i < self.tree.len() {
            self.tree[i] -= delta;
            i += lowest_bit(i);
        }
        self.total -= delta;
    }

    /// The sums of the chunks before chunk K.
    fn sums_before_chunk(&self, k: usize) -> LineSums {
        let mut sums = LineSums::default();
        let mut i = k;
        while i > 0 {
            sums += self.tree[i];
            i -= lowest_bit(i);
        }
        sums
    }

    /// Descend the tree: the first chunk K whose running sums, with it
    /// included, pass FITS no longer, and the sums of the chunks before K.
    /// FITS must be monotone: true for a prefix of chunks, then false.
    #[inline]
    fn descend(&self, fits: impl Fn(LineSums) -> bool) -> (usize, LineSums) {
        let n = self.chunks.len();
        let mut pos = 0usize;
        let mut acc = LineSums::default();
        let mut step = if n == 0 {
            0
        } else {
            1usize << (usize::BITS - 1 - n.leading_zeros())
        };
        while step > 0 {
            let next = pos + step;
            if next <= n {
                let candidate = acc + self.tree[next];
                if fits(candidate) {
                    pos = next;
                    acc = candidate;
                }
            }
            step >>= 1;
        }
        (pos, acc)
    }

    /// The chunk holding logical byte B (`B < total bytes`), and the sums of
    /// the chunks before it.
    fn locate_byte(&self, b: u64) -> (usize, LineSums) {
        debug_assert!(b < self.total.bytes);
        self.descend(|sums| sums.bytes <= b)
    }

    /// The chunk holding the Tth `\n` of the text (`1 <= T <= total`), and
    /// the sums of the chunks before it.
    fn locate_newline(&self, t: u64) -> (usize, LineSums) {
        debug_assert!(t >= 1 && t <= self.total.newlines);
        self.descend(|sums| sums.newlines < t)
    }

    /// The line ends under LINE_END before logical byte B.
    pub(crate) fn line_ends_before(
        &self,
        text: &(impl IndexedText + ?Sized),
        b: usize,
        line_end: LineEnd,
    ) -> u64 {
        let b = b as u64;
        if b >= self.total.bytes {
            return self.total.line_ends(line_end);
        }
        let (k, before) = self.locate_byte(b);
        let offset = b - before.bytes;
        if offset == 0 {
            return before.line_ends(line_end);
        }
        let start = before.bytes as usize;
        let chunk = self.chunks[k];
        let end = start + chunk.bytes as usize;
        let b = b as usize;
        // Scan the shorter side of B within its chunk.
        if offset <= chunk.bytes / 2 {
            before.line_ends(line_end) + line_ends_in(text, start, b, line_end)
        } else {
            before.line_ends(line_end) + chunk.line_ends(line_end)
                - line_ends_in(text, b, end, line_end)
        }
    }

    /// The position of the Tth (1-based) `\n` of the text, if it has that
    /// many.
    pub(crate) fn newline_position(
        &self,
        text: &(impl IndexedText + ?Sized),
        t: u64,
    ) -> Option<usize> {
        if t == 0 || t > self.total.newlines {
            return None;
        }
        let (k, before) = self.locate_newline(t);
        let start = before.bytes as usize;
        let end = start + self.chunks[k].bytes as usize;
        nth_newline_in(text, start, end, t - before.newlines)
    }

    // -- Maintenance --------------------------------------------------------

    /// Account for INSERTED, now at logical byte AT of TEXT (TEXT already
    /// holds it).
    pub(crate) fn note_insert(
        &mut self,
        text: &(impl IndexedText + ?Sized),
        at: usize,
        inserted: &[u8],
    ) {
        if inserted.is_empty() {
            return;
        }
        let added = LineSums::of_bytes(inserted, self.multibyte);
        let k = if self.chunks.is_empty() {
            self.chunks.push(LineSums::default());
            self.rebuild_tree();
            0
        } else if at == 0 {
            0
        } else {
            // The chunk holding the byte before AT, in the text before the
            // insertion: an insertion at a chunk boundary extends the chunk
            // that ends there.
            self.locate_byte((at - 1) as u64).0
        };
        self.chunks[k] += added;
        self.tree_add(k, added);
        if self.chunks[k].bytes as usize > self.geometry.max {
            self.split_chunk(text, k);
        }
    }

    /// Account for the deletion of the logical bytes `[start, end)` of TEXT
    /// BEFORE TEXT loses them: the doomed bytes are read to count what they
    /// held.
    pub(crate) fn note_delete(
        &mut self,
        text: &(impl IndexedText + ?Sized),
        start: usize,
        end: usize,
    ) {
        if start >= end {
            return;
        }
        debug_assert!(end as u64 <= self.total.bytes);
        let (first, before_first) = self.locate_byte(start as u64);
        let (last, before_last) = self.locate_byte((end - 1) as u64);
        if first == last {
            let doomed = sums_in(text, start, end);
            self.chunks[first] -= doomed;
            self.tree_sub(first, doomed);
            if (self.chunks[first].bytes as usize) < self.geometry.min && self.merge_small(first) {
                note_event(text_line_index_config(), TextLineIndexEvent::Restructure);
                self.rebuild_tree();
            }
            return;
        }
        let first_end = (before_first.bytes + self.chunks[first].bytes) as usize;
        let last_start = before_last.bytes as usize;
        let head = self.chunks[first] - sums_in(text, start, first_end);
        let tail = self.chunks[last] - sums_in(text, last_start, end);
        let survivors = [head, tail];
        let kept = survivors.iter().filter(|sums| sums.bytes > 0).count();
        self.chunks.splice(
            first..=last,
            survivors.into_iter().filter(|sums| sums.bytes > 0),
        );
        // The survivors sit at FIRST (and FIRST + 1); either may be small.
        if kept == 2 {
            self.merge_small(first + 1);
        }
        if kept >= 1 {
            self.merge_small(first);
        }
        note_event(text_line_index_config(), TextLineIndexEvent::Restructure);
        self.rebuild_tree();
    }

    /// Split chunk K, which grew past the maximum, into chunks of the
    /// target size, recounted from TEXT.
    fn split_chunk(&mut self, text: &(impl IndexedText + ?Sized), k: usize) {
        note_event(text_line_index_config(), TextLineIndexEvent::Restructure);
        let start = self.sums_before_chunk(k).bytes as usize;
        let end = start + self.chunks[k].bytes as usize;
        let pieces = cut_chunks(text, start, end, self.geometry.target, self.multibyte);
        self.chunks.splice(k..=k, pieces);
        self.rebuild_tree();
    }

    /// Remove chunk K if it is empty, or merge it into its smaller neighbour
    /// if it is below the minimum and the merged chunk stays within the
    /// maximum. Whether the chunks changed; if so, the caller rebuilds the
    /// tree.
    fn merge_small(&mut self, k: usize) -> bool {
        if k >= self.chunks.len() {
            return false;
        }
        let size = self.chunks[k].bytes as usize;
        if size == 0 {
            self.chunks.remove(k);
            return true;
        }
        if size >= self.geometry.min {
            return false;
        }
        let fits = |other: usize| size + self.chunks[other].bytes as usize <= self.geometry.max;
        let left = k.checked_sub(1).filter(|&l| fits(l));
        let right = Some(k + 1).filter(|&r| r < self.chunks.len() && fits(r));
        let into = match (left, right) {
            (Some(l), Some(r)) => {
                if self.chunks[l].bytes <= self.chunks[r].bytes {
                    l
                } else {
                    r
                }
            }
            (Some(l), None) => l,
            (None, Some(r)) => r,
            (None, None) => return false,
        };
        let merged = self.chunks[k];
        self.chunks[into] += merged;
        self.chunks.remove(k);
        true
    }

    // -- Checking -------------------------------------------------------------

    /// Recount chunk K (and its neighbours) from TEXT, and check the totals
    /// against TEXT's length.
    pub(crate) fn check_around(
        &self,
        text: &(impl IndexedText + ?Sized),
        at: usize,
    ) -> Result<(), String> {
        if self.total.bytes as usize != text.byte_len() {
            return Err(format!(
                "index holds {} bytes, text has {}",
                self.total.bytes,
                text.byte_len()
            ));
        }
        if self.chunks.is_empty() {
            return Ok(());
        }
        let at = (at as u64).min(self.total.bytes.saturating_sub(1));
        let (k, _) = self.locate_byte(at);
        for chunk in k.saturating_sub(1)..=(k + 1).min(self.chunks.len() - 1) {
            self.check_chunk(text, chunk)?;
        }
        Ok(())
    }

    fn check_chunk(&self, text: &(impl IndexedText + ?Sized), k: usize) -> Result<(), String> {
        let start = self.sums_before_chunk(k).bytes as usize;
        let end = start + self.chunks[k].bytes as usize;
        let counted = sums_in(text, start, end);
        if counted != self.chunks[k] {
            return Err(format!(
                "chunk {k} [{start}, {end}) holds {:?}, the text {counted:?}",
                self.chunks[k]
            ));
        }
        Ok(())
    }

    /// Recount everything from TEXT and check the tree: O(text).
    #[cfg(test)]
    pub(crate) fn check_all(&self, text: &(impl IndexedText + ?Sized)) -> Result<(), String> {
        if self.multibyte != text.is_multibyte() {
            return Err("multibyte flag disagrees with the text".to_owned());
        }
        if self.chunks.iter().any(|chunk| chunk.bytes == 0) {
            return Err("an empty chunk".to_owned());
        }
        if let Some(k) = self
            .chunks
            .iter()
            .position(|chunk| chunk.bytes as usize > self.geometry.max)
        {
            return Err(format!(
                "chunk {k} holds {} bytes, above the maximum {}",
                self.chunks[k].bytes, self.geometry.max
            ));
        }
        let mut rebuilt = self.clone();
        rebuilt.rebuild_tree();
        if rebuilt.tree != self.tree || rebuilt.total != self.total {
            return Err("the Fenwick tree disagrees with the chunks".to_owned());
        }
        let whole = sums_in(text, 0, text.byte_len());
        if whole != self.total {
            return Err(format!("index total {:?}, text {whole:?}", self.total));
        }
        for k in 0..self.chunks.len() {
            self.check_chunk(text, k)?;
        }
        Ok(())
    }
}

/// Cut the logical byte range `[start, end)` of TEXT into chunks of TARGET
/// bytes (the last one shorter), counted.
fn cut_chunks(
    text: &(impl IndexedText + ?Sized),
    start: usize,
    end: usize,
    target: usize,
    multibyte: bool,
) -> Vec<LineSums> {
    let mut chunks = Vec::with_capacity((end - start) / target + 1);
    let mut current = LineSums::default();
    text.for_each_piece(start, end, &mut |mut piece| {
        while !piece.is_empty() {
            let room = target - current.bytes as usize;
            let take = room.min(piece.len());
            current += LineSums::of_bytes(&piece[..take], multibyte);
            piece = &piece[take..];
            if current.bytes as usize == target {
                chunks.push(current);
                current = LineSums::default();
            }
        }
        true
    });
    if current.bytes > 0 {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
#[path = "text_index/tests/mod.rs"]
pub(crate) mod tests;
