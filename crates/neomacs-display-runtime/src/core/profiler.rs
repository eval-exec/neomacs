//! Pure Rust profiler implementation.
//!
//! Implements a hash-based sampling profiler inspired by the Emacs profiler
//! (`profiler.c`). The module is self-contained and provides:
//!
//! - [`ProfilerLog`]: a hash table mapping call stacks (backtraces) to hit
//!   counts, with configurable capacity and overflow handling.
//! - [`CallStack`]: a captured call stack represented as a vector of `u64`
//!   frame addresses, usable as a `HashMap` key.
//! - [`ProfilerState`]: lifecycle management for starting/stopping the
//!   profiler in CPU or Memory mode.
//! - [`ProfilerReport`] and helpers for report generation, top-N extraction,
//!   and log merging.
//!
//! # Overflow handling
//!
//! When the log reaches its `max_entries` capacity and a new backtrace is
//! recorded, the entry is discarded and a discard counter is incremented.
//! This mirrors the eviction strategy in `profiler.c` (simplified: we
//! discard new entries rather than evicting the lower half, since the Rust
//! version is intended for lighter-weight use cases).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

// ---------------------------------------------------------------------------
// CallStack
// ---------------------------------------------------------------------------

/// A captured call stack, represented as a sequence of frame addresses.
///
/// Each frame is a `u64` that may represent a function pointer address,
/// a symbol identifier, or any other opaque tag chosen by the caller.
#[derive(Debug, Clone, Eq)]
pub struct CallStack {
    /// The frames in this call stack, from innermost to outermost.
    pub frames: Vec<u64>,
}

impl CallStack {
    /// Create a new `CallStack` from the given frames.
    pub fn new(frames: Vec<u64>) -> Self {
        Self { frames }
    }

    /// Create an empty `CallStack`.
    pub fn empty() -> Self {
        Self { frames: Vec::new() }
    }

    /// Compute a fast hash of this call stack.
    ///
    /// Uses the standard library's `DefaultHasher` (SipHash) for a good
    /// balance of speed and collision resistance.
    pub fn hash_value(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    /// Return the depth (number of frames) in this call stack.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }
}

impl Hash for CallStack {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash each frame in order.
        for &frame in &self.frames {
            frame.hash(state);
        }
        // Also hash the length to distinguish e.g. [1] from [1, 0].
        self.frames.len().hash(state);
    }
}

impl PartialEq for CallStack {
    fn eq(&self, other: &Self) -> bool {
        self.frames == other.frames
    }
}

impl From<&[u64]> for CallStack {
    fn from(slice: &[u64]) -> Self {
        Self {
            frames: slice.to_vec(),
        }
    }
}

impl From<Vec<u64>> for CallStack {
    fn from(frames: Vec<u64>) -> Self {
        Self { frames }
    }
}

// ---------------------------------------------------------------------------
// ProfilerLog
// ---------------------------------------------------------------------------

/// A hash-based sampling profiler log.
///
/// Maps call stacks to hit counts. Supports a configurable maximum number
/// of distinct entries (`max_entries`). When full, new samples are discarded
/// and the `discarded` counter is incremented.
#[derive(Debug, Clone)]
pub struct ProfilerLog {
    /// Maximum depth of call stacks recorded. Backtraces longer than
    /// this are truncated to `max_stack_depth` frames.
    max_stack_depth: usize,
    /// Maximum number of distinct entries in the log.
    max_entries: usize,
    /// Mapping from call stacks to cumulative hit counts.
    entries: HashMap<CallStack, u64>,
    /// Number of samples discarded because the log was full.
    discarded: u64,
    /// Number of samples attributed to GC (special bucket).
    gc_count: u64,
}

impl ProfilerLog {
    /// Create a new, empty profiler log.
    ///
    /// `max_stack_depth` controls the maximum number of frames kept per
    /// backtrace. Deeper stacks are truncated.
    pub fn new(max_stack_depth: usize) -> Self {
        Self::with_capacity(max_stack_depth, 10_000)
    }

    /// Create a new profiler log with explicit capacity.
    ///
    /// `max_entries` is the maximum number of distinct backtraces the log
    /// can hold before it starts discarding new samples.
    pub fn with_capacity(max_stack_depth: usize, max_entries: usize) -> Self {
        Self {
            max_stack_depth,
            max_entries,
            entries: HashMap::new(),
            discarded: 0,
            gc_count: 0,
        }
    }

    /// Record a sample with the given backtrace and a weight of 1.
    ///
    /// If the backtrace is longer than `max_stack_depth`, it is truncated.
    /// If the log is at capacity and this backtrace is not already present,
    /// the sample is discarded.
    pub fn record(&mut self, backtrace: &[u64]) {
        self.record_weighted(backtrace, 1);
    }

    /// Record a sample with the given backtrace and an explicit weight.
    ///
    /// This is useful for memory profiling where the weight is the
    /// allocation size.
    pub fn record_weighted(&mut self, backtrace: &[u64], count: u64) {
        let truncated = if backtrace.len() > self.max_stack_depth {
            &backtrace[..self.max_stack_depth]
        } else {
            backtrace
        };

        let key = CallStack::from(truncated);

        if let Some(existing) = self.entries.get_mut(&key) {
            *existing = existing.saturating_add(count);
        } else if self.entries.len() < self.max_entries {
            self.entries.insert(key, count);
        } else {
            // Log is full — discard this sample.
            self.discarded = self.discarded.saturating_add(count);
        }
    }

    /// Record a sample that occurred during GC.
    ///
    /// These are tracked separately because the hash table cannot be
    /// safely modified during garbage collection (mirrors the C
    /// implementation's `gc_count` field).
    pub fn record_gc(&mut self, count: u64) {
        self.gc_count = self.gc_count.saturating_add(count);
    }

    /// Get the hit count for a specific backtrace, or 0 if not recorded.
    pub fn count(&self, backtrace: &[u64]) -> u64 {
        let key = CallStack::from(backtrace);
        self.entries.get(&key).copied().unwrap_or(0)
    }

    /// Return the total number of samples recorded (including GC samples
    /// but excluding discarded samples).
    pub fn total_samples(&self) -> u64 {
        let entry_sum: u64 = self.entries.values().sum();
        entry_sum.saturating_add(self.gc_count)
    }

    /// Return the number of distinct backtraces in the log.
    pub fn num_entries(&self) -> usize {
        self.entries.len()
    }

    /// Return the number of discarded samples.
    pub fn discarded(&self) -> u64 {
        self.discarded
    }

    /// Return the number of GC samples.
    pub fn gc_count(&self) -> u64 {
        self.gc_count
    }

    /// Return the maximum stack depth for this log.
    pub fn max_stack_depth(&self) -> usize {
        self.max_stack_depth
    }

    /// Return the maximum number of entries this log can hold.
    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Return `true` if the log has no recorded backtraces and no GC
    /// samples.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.gc_count == 0
    }

    /// Clear all recorded data, resetting counts, discarded, and GC
    /// counters to zero.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.discarded = 0;
        self.gc_count = 0;
    }

    /// Iterate over all (backtrace, count) pairs in the log.
    pub fn iter(&self) -> impl Iterator<Item = (&CallStack, &u64)> {
        self.entries.iter()
    }
}

// ---------------------------------------------------------------------------
// ProfilerMode
// ---------------------------------------------------------------------------

/// The profiler mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfilerMode {
    /// CPU sampling profiler: takes call-stack samples at a periodic
    /// interval.
    Cpu,
    /// Memory allocation profiler: takes a sample on every allocation
    /// (weighted by allocation size).
    Memory,
}

// ---------------------------------------------------------------------------
// ProfilerState
// ---------------------------------------------------------------------------

/// Manages the profiler lifecycle (start/stop) and owns the current log.
#[derive(Debug)]
pub struct ProfilerState {
    /// Whether the profiler is currently running.
    running: bool,
    /// The profiling mode (CPU or Memory).
    mode: ProfilerMode,
    /// The sampling interval in nanoseconds (only meaningful for CPU mode).
    sample_interval_ns: u64,
    /// The profiler log accumulating samples.
    log: ProfilerLog,
}

impl ProfilerState {
    /// Create a new profiler state with the given log configuration.
    ///
    /// The profiler starts in the stopped state.
    pub fn new(max_stack_depth: usize, max_entries: usize) -> Self {
        Self {
            running: false,
            mode: ProfilerMode::Cpu,
            sample_interval_ns: 0,
            log: ProfilerLog::with_capacity(max_stack_depth, max_entries),
        }
    }

    /// Start the profiler in the given mode with the specified sampling
    /// interval (nanoseconds, meaningful only for CPU mode).
    ///
    /// If the profiler is already running, this is a no-op and returns
    /// `false`. Otherwise it resets the log and returns `true`.
    pub fn start(&mut self, mode: ProfilerMode, sample_interval_ns: u64) -> bool {
        if self.running {
            return false;
        }
        self.running = true;
        self.mode = mode;
        self.sample_interval_ns = sample_interval_ns;
        self.log.clear();
        true
    }

    /// Stop the profiler and return the accumulated log.
    ///
    /// The internal log is replaced with a fresh, empty log (preserving
    /// the same capacity settings). If the profiler was not running, an
    /// empty log is returned.
    pub fn stop(&mut self) -> ProfilerLog {
        self.running = false;
        let max_stack_depth = self.log.max_stack_depth;
        let max_entries = self.log.max_entries;
        std::mem::replace(
            &mut self.log,
            ProfilerLog::with_capacity(max_stack_depth, max_entries),
        )
    }

    /// Return `true` if the profiler is currently running.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Return the current profiling mode.
    pub fn mode(&self) -> ProfilerMode {
        self.mode
    }

    /// Return the current sampling interval in nanoseconds.
    pub fn sample_interval_ns(&self) -> u64 {
        self.sample_interval_ns
    }

    /// Return a reference to the current (live) log.
    pub fn log(&self) -> &ProfilerLog {
        &self.log
    }

    /// Return a mutable reference to the current (live) log.
    ///
    /// This is used by the sampling signal handler (or equivalent) to
    /// record samples while the profiler is running.
    pub fn log_mut(&mut self) -> &mut ProfilerLog {
        &mut self.log
    }
}

// ---------------------------------------------------------------------------
// ProfilerReport
// ---------------------------------------------------------------------------

/// A summary report generated from a profiler log.
#[derive(Debug, Clone)]
pub struct ProfilerReport {
    /// Total number of samples (including GC, excluding discarded).
    pub total_samples: u64,
    /// Number of distinct backtraces.
    pub num_entries: usize,
    /// Number of samples discarded due to log overflow.
    pub discarded: u64,
    /// Number of samples attributed to GC.
    pub gc_count: u64,
    /// The top entries sorted by count (descending). Each element is
    /// `(backtrace_frames, count)`.
    pub top: Vec<(Vec<u64>, u64)>,
}

/// Generate a report from a profiler log.
///
/// The report includes the top entries (all of them, sorted by count
/// descending) and aggregate statistics.
pub fn generate_report(log: &ProfilerLog) -> ProfilerReport {
    let mut top: Vec<(Vec<u64>, u64)> = log
        .entries
        .iter()
        .map(|(k, &v)| (k.frames.clone(), v))
        .collect();
    top.sort_by_key(|e| std::cmp::Reverse(e.1));

    ProfilerReport {
        total_samples: log.total_samples(),
        num_entries: log.num_entries(),
        discarded: log.discarded(),
        gc_count: log.gc_count(),
        top,
    }
}

/// Return the top `n` entries from the log, sorted by count descending.
///
/// Each entry is `(backtrace_frames, count)`.
pub fn top_entries(log: &ProfilerLog, n: usize) -> Vec<(Vec<u64>, u64)> {
    let mut entries: Vec<(Vec<u64>, u64)> = log
        .entries
        .iter()
        .map(|(k, &v)| (k.frames.clone(), v))
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.1));
    entries.truncate(n);
    entries
}

/// Merge multiple profiler logs into a single combined log.
///
/// Counts for identical backtraces are summed (with saturation).
/// The resulting log uses the maximum `max_stack_depth` and
/// `max_entries` from the input logs (so nothing is lost due to
/// capacity limits during the merge itself). GC counts and discarded
/// counts are also summed.
pub fn merge_logs(logs: &[&ProfilerLog]) -> ProfilerLog {
    if logs.is_empty() {
        return ProfilerLog::new(16);
    }

    let max_depth = logs.iter().map(|l| l.max_stack_depth).max().unwrap_or(16);
    // Use a generous capacity: sum of all entries across logs.
    let total_entries: usize = logs.iter().map(|l| l.num_entries()).sum();
    let max_cap = logs.iter().map(|l| l.max_entries).max().unwrap_or(10_000);
    let capacity = total_entries.max(max_cap);

    let mut merged = ProfilerLog::with_capacity(max_depth, capacity);

    for log in logs {
        for (stack, &count) in log.entries.iter() {
            if let Some(existing) = merged.entries.get_mut(stack) {
                *existing = existing.saturating_add(count);
            } else {
                merged.entries.insert(stack.clone(), count);
            }
        }
        merged.gc_count = merged.gc_count.saturating_add(log.gc_count);
        merged.discarded = merged.discarded.saturating_add(log.discarded);
    }

    merged
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- CallStack tests --

    #[test]
    fn call_stack_new_and_depth() {
        let cs = CallStack::new(vec![1, 2, 3]);
        assert_eq!(cs.depth(), 3);
        assert_eq!(cs.frames, vec![1, 2, 3]);
    }

    #[test]
    fn call_stack_empty() {
        let cs = CallStack::empty();
        assert_eq!(cs.depth(), 0);
        assert!(cs.frames.is_empty());
    }

    #[test]
    fn call_stack_equality() {
        let a = CallStack::new(vec![10, 20, 30]);
        let b = CallStack::new(vec![10, 20, 30]);
        let c = CallStack::new(vec![10, 20, 31]);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn call_stack_hash_consistency() {
        let a = CallStack::new(vec![100, 200]);
        let b = CallStack::new(vec![100, 200]);
        assert_eq!(a.hash_value(), b.hash_value());
    }

    #[test]
    fn call_stack_hash_differs_for_different_stacks() {
        let a = CallStack::new(vec![1, 2, 3]);
        let b = CallStack::new(vec![3, 2, 1]);
        // Not strictly guaranteed, but overwhelmingly likely.
        assert_ne!(a.hash_value(), b.hash_value());
    }

    #[test]
    fn call_stack_from_slice() {
        let slice: &[u64] = &[7, 8, 9];
        let cs = CallStack::from(slice);
        assert_eq!(cs.frames, vec![7, 8, 9]);
    }

    #[test]
    fn call_stack_from_vec() {
        let v = vec![42u64, 43];
        let cs = CallStack::from(v);
        assert_eq!(cs.frames, vec![42, 43]);
    }

    // -- ProfilerLog basic tests --

    #[test]
    fn profiler_log_new_is_empty() {
        let log = ProfilerLog::new(16);
        assert!(log.is_empty());
        assert_eq!(log.total_samples(), 0);
        assert_eq!(log.num_entries(), 0);
        assert_eq!(log.discarded(), 0);
        assert_eq!(log.gc_count(), 0);
        assert_eq!(log.max_stack_depth(), 16);
    }

    #[test]
    fn profiler_log_record_and_count() {
        let mut log = ProfilerLog::new(16);
        let bt = [1u64, 2, 3];
        log.record(&bt);
        log.record(&bt);
        log.record(&bt);
        assert_eq!(log.count(&bt), 3);
        assert_eq!(log.total_samples(), 3);
        assert_eq!(log.num_entries(), 1);
    }

    #[test]
    fn profiler_log_record_weighted() {
        let mut log = ProfilerLog::new(16);
        let bt = [10u64, 20];
        log.record_weighted(&bt, 100);
        log.record_weighted(&bt, 50);
        assert_eq!(log.count(&bt), 150);
        assert_eq!(log.total_samples(), 150);
    }

    #[test]
    fn profiler_log_multiple_backtraces() {
        let mut log = ProfilerLog::new(16);
        let bt1 = [1u64, 2];
        let bt2 = [3u64, 4];
        log.record(&bt1);
        log.record(&bt1);
        log.record(&bt2);
        assert_eq!(log.count(&bt1), 2);
        assert_eq!(log.count(&bt2), 1);
        assert_eq!(log.num_entries(), 2);
        assert_eq!(log.total_samples(), 3);
    }

    #[test]
    fn profiler_log_truncates_deep_backtrace() {
        let mut log = ProfilerLog::new(3);
        let deep_bt = [1u64, 2, 3, 4, 5, 6];
        log.record(&deep_bt);
        // Should be stored truncated to depth 3.
        assert_eq!(log.count(&[1, 2, 3]), 1);
        // The full backtrace should not match.
        assert_eq!(log.count(&deep_bt), 0);
    }

    #[test]
    fn profiler_log_capacity_overflow_discards() {
        let mut log = ProfilerLog::with_capacity(16, 2);
        log.record(&[1u64, 2]);
        log.record(&[3u64, 4]);
        // Log is now full (2 distinct entries).
        log.record(&[5u64, 6]); // Should be discarded.
        assert_eq!(log.num_entries(), 2);
        assert_eq!(log.discarded(), 1);
        assert_eq!(log.count(&[5, 6]), 0);
    }

    #[test]
    fn profiler_log_overflow_existing_entry_still_works() {
        let mut log = ProfilerLog::with_capacity(16, 2);
        log.record(&[1u64, 2]);
        log.record(&[3u64, 4]);
        // Log is full, but recording an existing backtrace should still work.
        log.record(&[1u64, 2]);
        assert_eq!(log.count(&[1, 2]), 2);
        assert_eq!(log.discarded(), 0);
    }

    #[test]
    fn profiler_log_gc_count() {
        let mut log = ProfilerLog::new(16);
        log.record_gc(5);
        log.record_gc(3);
        assert_eq!(log.gc_count(), 8);
        assert_eq!(log.total_samples(), 8);
        // GC samples don't create entries.
        assert_eq!(log.num_entries(), 0);
        assert!(!log.is_empty());
    }

    #[test]
    fn profiler_log_clear() {
        let mut log = ProfilerLog::new(16);
        log.record(&[1u64, 2]);
        log.record_gc(10);
        assert!(!log.is_empty());
        log.clear();
        assert!(log.is_empty());
        assert_eq!(log.total_samples(), 0);
        assert_eq!(log.gc_count(), 0);
        assert_eq!(log.discarded(), 0);
    }

    #[test]
    fn profiler_log_count_nonexistent() {
        let log = ProfilerLog::new(16);
        assert_eq!(log.count(&[99, 100]), 0);
    }

    #[test]
    fn profiler_log_saturating_add() {
        let mut log = ProfilerLog::new(16);
        let bt = [1u64];
        log.record_weighted(&bt, u64::MAX - 1);
        log.record_weighted(&bt, 10);
        // Should saturate to u64::MAX.
        assert_eq!(log.count(&bt), u64::MAX);
    }

    #[test]
    fn profiler_log_iter() {
        let mut log = ProfilerLog::new(16);
        log.record(&[1u64]);
        log.record(&[2u64]);
        log.record(&[1u64]);
        let collected: HashMap<Vec<u64>, u64> =
            log.iter().map(|(k, &v)| (k.frames.clone(), v)).collect();
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[&vec![1u64]], 2);
        assert_eq!(collected[&vec![2u64]], 1);
    }

    // -- ProfilerState tests --

    #[test]
    fn profiler_state_lifecycle() {
        let mut state = ProfilerState::new(16, 1000);
        assert!(!state.is_running());

        // Start.
        assert!(state.start(ProfilerMode::Cpu, 1_000_000));
        assert!(state.is_running());
        assert_eq!(state.mode(), ProfilerMode::Cpu);
        assert_eq!(state.sample_interval_ns(), 1_000_000);

        // Record some samples.
        state.log_mut().record(&[1u64, 2, 3]);
        state.log_mut().record(&[1u64, 2, 3]);
        assert_eq!(state.log().total_samples(), 2);

        // Stop and get log.
        let log = state.stop();
        assert!(!state.is_running());
        assert_eq!(log.total_samples(), 2);
        assert_eq!(log.count(&[1, 2, 3]), 2);

        // Internal log should be fresh.
        assert!(state.log().is_empty());
    }

    #[test]
    fn profiler_state_double_start_returns_false() {
        let mut state = ProfilerState::new(16, 1000);
        assert!(state.start(ProfilerMode::Cpu, 1_000_000));
        // Second start should fail.
        assert!(!state.start(ProfilerMode::Memory, 0));
        // Mode should remain CPU.
        assert_eq!(state.mode(), ProfilerMode::Cpu);
    }

    #[test]
    fn profiler_state_stop_when_not_running() {
        let mut state = ProfilerState::new(16, 1000);
        let log = state.stop();
        assert!(log.is_empty());
    }

    #[test]
    fn profiler_state_memory_mode() {
        let mut state = ProfilerState::new(16, 1000);
        assert!(state.start(ProfilerMode::Memory, 0));
        assert_eq!(state.mode(), ProfilerMode::Memory);
        state.log_mut().record_weighted(&[10u64, 20], 4096);
        let log = state.stop();
        assert_eq!(log.count(&[10, 20]), 4096);
    }

    // -- Report and merge tests --

    #[test]
    fn generate_report_basic() {
        let mut log = ProfilerLog::new(16);
        log.record_weighted(&[1u64, 2], 100);
        log.record_weighted(&[3u64, 4], 50);
        log.record_weighted(&[5u64, 6], 200);
        log.record_gc(10);

        let report = generate_report(&log);
        assert_eq!(report.total_samples, 360);
        assert_eq!(report.num_entries, 3);
        assert_eq!(report.gc_count, 10);
        assert_eq!(report.discarded, 0);
        // Top should be sorted descending.
        assert_eq!(report.top[0].1, 200);
        assert_eq!(report.top[1].1, 100);
        assert_eq!(report.top[2].1, 50);
    }

    #[test]
    fn top_entries_limits_count() {
        let mut log = ProfilerLog::new(16);
        for i in 0..20u64 {
            log.record_weighted(&[i], i + 1);
        }
        let top = top_entries(&log, 5);
        assert_eq!(top.len(), 5);
        // Highest count should be 20 (from backtrace [19]).
        assert_eq!(top[0].1, 20);
        assert_eq!(top[4].1, 16);
    }

    #[test]
    fn top_entries_more_than_available() {
        let mut log = ProfilerLog::new(16);
        log.record(&[1u64]);
        log.record(&[2u64]);
        let top = top_entries(&log, 100);
        assert_eq!(top.len(), 2);
    }

    #[test]
    fn merge_logs_basic() {
        let mut log1 = ProfilerLog::new(16);
        log1.record_weighted(&[1u64, 2], 10);
        log1.record_weighted(&[3u64, 4], 5);
        log1.record_gc(2);

        let mut log2 = ProfilerLog::new(16);
        log2.record_weighted(&[1u64, 2], 20);
        log2.record_weighted(&[5u64, 6], 15);
        log2.record_gc(3);

        let merged = merge_logs(&[&log1, &log2]);
        assert_eq!(merged.count(&[1, 2]), 30);
        assert_eq!(merged.count(&[3, 4]), 5);
        assert_eq!(merged.count(&[5, 6]), 15);
        assert_eq!(merged.gc_count(), 5);
        assert_eq!(merged.num_entries(), 3);
    }

    #[test]
    fn merge_logs_empty_input() {
        let merged = merge_logs(&[]);
        assert!(merged.is_empty());
    }

    #[test]
    fn merge_logs_discarded_sums() {
        let mut log1 = ProfilerLog::with_capacity(16, 1);
        log1.record(&[1u64]);
        log1.record(&[2u64]); // Discarded.
        assert_eq!(log1.discarded(), 1);

        let mut log2 = ProfilerLog::with_capacity(16, 1);
        log2.record(&[3u64]);
        log2.record(&[4u64]); // Discarded.
        log2.record(&[5u64]); // Discarded.
        assert_eq!(log2.discarded(), 2);

        let merged = merge_logs(&[&log1, &log2]);
        assert_eq!(merged.discarded(), 3);
    }

    #[test]
    fn profiler_log_with_capacity_accessors() {
        let log = ProfilerLog::with_capacity(8, 500);
        assert_eq!(log.max_stack_depth(), 8);
        assert_eq!(log.max_entries(), 500);
    }
}
