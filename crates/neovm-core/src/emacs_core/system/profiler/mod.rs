use neomacs_host_runtime::time::Instant;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use super::eval::{Context, SpecBinding};
use super::intern::resolve_sym;
use super::value::{
    FunctionSourceIdentity, HashKey, HashTableTest, Value, build_hash_table_literal_value,
};

const DEFAULT_MAX_STACK_DEPTH: usize = 16;
const DEFAULT_LOG_SIZE: usize = 10_000;
const MAX_STACK_DEPTH: usize = 4_096;
const MAX_LOG_SIZE: usize = 1_000_000;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum FrameKey {
    Identity(usize),
    FunctionSource(FunctionSourceIdentity),
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct StackKey(SmallVec<[FrameKey; DEFAULT_MAX_STACK_DEPTH]>);

struct Sample {
    frames: SmallVec<[Value; DEFAULT_MAX_STACK_DEPTH]>,
    count: u64,
}

struct ProfilerLog {
    depth: usize,
    capacity: usize,
    entries: FxHashMap<StackKey, Sample>,
    gc_count: u64,
    discarded: u64,
}

/// Resolve one profiler frame `Value` to a folded-stack label.
///
/// Symbols and subrs resolve to their name; anonymous compiled closures
/// collapse by bytecode source id; anything else falls back to its tagged bits.
/// The folded grammar uses `;` and space as separators, so those (and newlines)
/// are stripped from names — a single unsanitized name would corrupt the whole
/// folded output and break inferno / the report parser.
fn frame_label(frame: Value) -> String {
    let raw = if let Some(name) = frame.as_symbol_name() {
        name.to_string()
    } else if let Some(subr_id) = frame.as_subr_id() {
        resolve_sym(subr_id).to_string()
    } else if let Some(FunctionSourceIdentity::ByteCode(id)) = frame.function_source_identity() {
        format!("<bytecode:{id:#x}>")
    } else {
        format!("<closure:{:#x}>", frame.bits())
    };
    raw.replace([' ', ';', '\n', '\r', '\t'], "_")
}

impl ProfilerLog {
    fn new(depth: usize, capacity: usize) -> Self {
        Self {
            depth,
            capacity,
            entries: FxHashMap::default(),
            gc_count: 0,
            discarded: 0,
        }
    }

    fn record(&mut self, active_frames: &[Value], count: u64) {
        if count == 0 {
            return;
        }
        if self.capacity == 0 {
            self.discarded = self.discarded.saturating_add(count);
            return;
        }

        let mut frames: SmallVec<[Value; DEFAULT_MAX_STACK_DEPTH]> =
            SmallVec::with_capacity(self.depth);
        frames.extend(active_frames.iter().copied().take(self.depth));
        frames.resize(self.depth, Value::NIL);
        let key = StackKey(
            frames
                .iter()
                .map(|value| {
                    value
                        .function_source_identity()
                        .map_or(FrameKey::Identity(value.bits()), FrameKey::FunctionSource)
                })
                .collect(),
        );

        if let Some(sample) = self.entries.get_mut(&key) {
            sample.count = sample.count.saturating_add(count);
            return;
        }

        if self.entries.len() >= self.capacity {
            self.evict_cold_half();
        }
        self.entries.insert(key, Sample { frames, count });
    }

    fn evict_cold_half(&mut self) {
        let remove_count = (self.entries.len() / 2).max(1);
        let mut counts: Vec<_> = self.entries.values().map(|sample| sample.count).collect();
        counts.select_nth_unstable(remove_count - 1);
        let cutoff = counts[remove_count - 1];
        let below_cutoff = self
            .entries
            .values()
            .filter(|sample| sample.count < cutoff)
            .count();
        let mut cutoff_entries_to_remove = remove_count - below_cutoff;
        self.entries.retain(|_, sample| {
            let remove =
                sample.count < cutoff || (sample.count == cutoff && cutoff_entries_to_remove > 0);
            if remove {
                self.discarded = self.discarded.saturating_add(sample.count);
                if sample.count == cutoff {
                    cutoff_entries_to_remove -= 1;
                }
            }
            !remove
        });
    }

    fn record_gc(&mut self, count: u64) {
        self.gc_count = self.gc_count.saturating_add(count);
    }

    fn trace_roots(&self, visit: &mut dyn FnMut(Value)) {
        for sample in self.entries.values() {
            for frame in sample.frames.iter().copied() {
                visit(frame);
            }
        }
    }

    fn to_value(&self) -> Value {
        let mut normal_entries: FxHashMap<HashKey, (Value, u64)> = FxHashMap::default();
        for sample in self.entries.values() {
            let frames = Value::vector(sample.frames.iter().copied().collect());
            let key = frames.to_hash_key(&HashTableTest::Equal);
            normal_entries
                .entry(key)
                .and_modify(|(_, count)| *count = count.saturating_add(sample.count))
                .or_insert((frames, sample.count));
        }
        let mut entries = Vec::with_capacity(
            normal_entries.len() + usize::from(self.gc_count > 0) + usize::from(self.discarded > 0),
        );
        entries.extend(
            normal_entries
                .into_values()
                .map(|(frames, count)| (frames, Value::fixnum(fixnum_count(count)))),
        );
        if self.gc_count > 0 {
            entries.push((
                Value::vector(vec![Value::symbol("Automatic GC"), Value::NIL]),
                Value::fixnum(fixnum_count(self.gc_count)),
            ));
        }
        if self.discarded > 0 {
            entries.push((
                Value::vector(vec![Value::symbol("Discarded Samples"), Value::NIL]),
                Value::fixnum(fixnum_count(self.discarded)),
            ));
        }
        build_hash_table_literal_value(
            HashTableTest::Equal,
            None,
            entries.len().max(1) as i64,
            None,
            1.5,
            0.8125,
            entries,
        )
    }
}

fn fixnum_count(count: u64) -> i64 {
    count.min(Value::MOST_POSITIVE_FIXNUM as u64) as i64
}

#[derive(Default)]
pub(crate) struct ProfilerState {
    cpu_log: Option<ProfilerLog>,
    cpu_running: bool,
    /// True while the running CPU session was started by the diagnostics server
    /// (not an interactive `profiler-start`). Lets a later diagnostics start
    /// reclaim an orphaned diagnostics session while still never hijacking a
    /// user session.
    cpu_diagnostics_owned: bool,
    cpu_interval_ns: u64,
    cpu_last_ns: u64,
    cpu_remainder_ns: u64,
    memory_log: Option<ProfilerLog>,
    memory_running: bool,
    memory_last_allocated: u64,
}

impl ProfilerState {
    #[inline]
    pub(crate) fn is_active(&self) -> bool {
        self.cpu_running || self.memory_running
    }

    fn max_active_depth(&self) -> usize {
        let cpu = if self.cpu_running {
            self.cpu_log.as_ref().map_or(0, |log| log.depth)
        } else {
            0
        };
        let memory = if self.memory_running {
            self.memory_log.as_ref().map_or(0, |log| log.depth)
        } else {
            0
        };
        cpu.max(memory)
    }

    fn poll(&mut self, active_frames: &[Value], allocated_bytes: u64) {
        if self.cpu_running {
            let now = thread_cpu_time_ns();
            let elapsed = now.saturating_sub(self.cpu_last_ns);
            self.cpu_last_ns = now;
            let accumulated = elapsed.saturating_add(self.cpu_remainder_ns);
            let samples = accumulated / self.cpu_interval_ns;
            self.cpu_remainder_ns = accumulated % self.cpu_interval_ns;
            if let Some(log) = self.cpu_log.as_mut() {
                log.record(active_frames, samples);
            }
        }

        if self.memory_running {
            let allocated = allocated_bytes.saturating_sub(self.memory_last_allocated);
            self.memory_last_allocated = allocated_bytes;
            if let Some(log) = self.memory_log.as_mut() {
                log.record(active_frames, allocated);
            }
        }
    }

    fn finish_gc(&mut self, allocated_bytes: u64) {
        if self.cpu_running {
            let now = thread_cpu_time_ns();
            let elapsed = now.saturating_sub(self.cpu_last_ns);
            self.cpu_last_ns = now;
            let accumulated = elapsed.saturating_add(self.cpu_remainder_ns);
            let samples = accumulated / self.cpu_interval_ns;
            self.cpu_remainder_ns = accumulated % self.cpu_interval_ns;
            if let Some(log) = self.cpu_log.as_mut() {
                log.record_gc(samples);
            }
        }
        if self.memory_running {
            self.memory_last_allocated = allocated_bytes;
        }
    }

    fn trace_roots(&self, visit: &mut dyn FnMut(Value)) {
        if let Some(log) = &self.cpu_log {
            log.trace_roots(visit);
        }
        if let Some(log) = &self.memory_log {
            log.trace_roots(visit);
        }
    }
}

/// Wall-clock sample-due flag, set by the watchdog timer armed while a
/// profiler runs and consumed on the Lisp thread at the `maybe_quit` safe
/// point ([`Context::profiler_sample_tick`]). GNU's CPU profiler is a SIGPROF
/// handler with ZERO cost when off; SIGPROF is taken here by the native
/// profiler (pprof-rs), so the Lisp profiler samples via this flag instead —
/// one `'static` relaxed load on the quit-poll fast path replaces the two to
/// three `profiler_poll` calls every Lisp call used to pay (the poll walked
/// `is_active()` per push/pop even with the profiler off). Sample COUNTS stay
/// exact regardless of tick timing: `Profiler::poll` self-corrects on thread
/// CPU time (samples = elapsed / interval since the last poll), so the tick
/// only decides which backtrace the accumulated samples attribute to — the
/// same quantization GNU gets from signal delivery.
static PROFILER_SAMPLE_DUE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Generation counter for the watchdog: arming bumps it and spawns a thread
/// pinned to the new generation; disarming (or re-arming) bumps it again and
/// the stale thread exits at its next tick. At most one live watchdog, no
/// join handles to plumb, and a stale `SAMPLE_DUE` is harmless (the consumer
/// re-checks `is_active`).
static PROFILER_TIMER_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[inline]
pub(crate) fn profiler_sample_due() -> bool {
    PROFILER_SAMPLE_DUE.load(std::sync::atomic::Ordering::Relaxed)
}

fn arm_profiler_sample_timer(interval_ns: u64) {
    use std::sync::atomic::Ordering;
    let generation = PROFILER_TIMER_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    // Floor the tick at 100µs: the flag is a wall-clock attribution pulse,
    // not the sample clock (poll() meters samples on thread CPU time).
    let tick = std::time::Duration::from_nanos(interval_ns.max(100_000));
    std::thread::Builder::new()
        .name("neovm-profiler-tick".into())
        .spawn(move || {
            while PROFILER_TIMER_GEN.load(Ordering::Relaxed) == generation {
                std::thread::sleep(tick);
                PROFILER_SAMPLE_DUE.store(true, Ordering::Relaxed);
            }
        })
        .expect("profiler tick thread should spawn");
}

fn disarm_profiler_sample_timer() {
    use std::sync::atomic::Ordering;
    PROFILER_TIMER_GEN.fetch_add(1, Ordering::Relaxed);
    PROFILER_SAMPLE_DUE.store(false, Ordering::Relaxed);
}

impl Context {
    /// Consume a pending sample tick: swap the flag off and take one poll
    /// (which attributes every CPU sample / allocated byte accumulated since
    /// the previous poll to the current backtrace). Called from the
    /// `maybe_quit` fast path; a race that loses the swap just defers the
    /// attribution to the next tick, and a stale flag with no profiler
    /// running falls into `profiler_poll`'s `is_active` early-return.
    #[inline]
    pub(crate) fn profiler_sample_tick(&mut self) {
        if PROFILER_SAMPLE_DUE.swap(false, std::sync::atomic::Ordering::Relaxed) {
            self.profiler_poll();
        }
    }

    #[inline]
    pub(crate) fn profiler_poll(&mut self) {
        if !self.profiler.is_active() {
            return;
        }

        let depth = self.profiler.max_active_depth();
        let frames: SmallVec<[Value; DEFAULT_MAX_STACK_DEPTH]> = self
            .specpdl
            .iter()
            .rev()
            .filter_map(|binding| match binding {
                SpecBinding::Backtrace { function, .. } => Some(*function),
                _ => None,
            })
            .take(depth)
            .collect();
        let allocated_bytes = self.tagged_heap.total_allocated_bytes();
        self.profiler.poll(&frames, allocated_bytes);
    }

    pub(crate) fn profiler_cpu_start(&mut self, interval_ns: u64) -> bool {
        if self.profiler.cpu_running || interval_ns == 0 {
            return false;
        }
        if self.profiler.cpu_log.is_none() {
            let (depth, capacity) = self.profiler_settings();
            self.profiler.cpu_log = Some(ProfilerLog::new(depth, capacity));
        }
        self.profiler.cpu_interval_ns = interval_ns;
        self.profiler.cpu_last_ns = thread_cpu_time_ns();
        self.profiler.cpu_remainder_ns = 0;
        self.profiler.cpu_running = true;
        arm_profiler_sample_timer(interval_ns);
        true
    }

    pub(crate) fn profiler_gc_start(&mut self) {
        self.profiler_poll();
    }

    pub(crate) fn profiler_gc_finish(&mut self) {
        if self.profiler.is_active() {
            let allocated_bytes = self.tagged_heap.total_allocated_bytes();
            self.profiler.finish_gc(allocated_bytes);
        }
    }

    pub(crate) fn profiler_cpu_stop(&mut self) -> bool {
        if !self.profiler.cpu_running {
            return false;
        }
        self.profiler_poll();
        self.profiler.cpu_running = false;
        if !self.profiler.memory_running {
            disarm_profiler_sample_timer();
        }
        true
    }

    pub(crate) fn profiler_cpu_running(&self) -> bool {
        self.profiler.cpu_running
    }

    pub(crate) fn profiler_cpu_log(&mut self) -> Option<Value> {
        self.profiler_poll();
        let was_running = std::mem::replace(&mut self.profiler.cpu_running, false);
        let Some(log) = self.profiler.cpu_log.as_ref() else {
            self.profiler.cpu_running = was_running;
            return None;
        };
        let value = log.to_value();
        let log = self
            .profiler
            .cpu_log
            .take()
            .expect("profiler log checked above");
        if was_running {
            self.profiler.cpu_log = Some(ProfilerLog::new(log.depth, log.capacity));
            self.profiler.cpu_last_ns = thread_cpu_time_ns();
            self.profiler.cpu_remainder_ns = 0;
            self.profiler.cpu_running = true;
        }
        Some(value)
    }

    /// Non-destructively fold the CPU profiler log into Brendan-Gregg folded
    /// stacks (`root;...;leaf <count>` per line), resolving function names on
    /// the Lisp thread where the interner is live. Unlike `profiler_cpu_log`,
    /// this does NOT take or clear the log — the caller stops the profiler
    /// separately. Returns an empty string when nothing was sampled.
    pub(crate) fn profiler_cpu_folded(&mut self) -> String {
        self.profiler_poll();
        let Some(log) = self.profiler.cpu_log.as_ref() else {
            return String::new();
        };
        let mut folded: FxHashMap<String, u64> = FxHashMap::default();
        for sample in log.entries.values() {
            // Frames are innermost-first and NIL-padded to `depth`; drop the
            // padding and reverse to root;...;leaf for Brendan-Gregg order.
            let mut labels: Vec<String> = sample
                .frames
                .iter()
                .copied()
                .filter(|frame| !frame.is_nil())
                .map(frame_label)
                .collect();
            labels.reverse();
            if labels.is_empty() {
                continue;
            }
            *folded.entry(labels.join(";")).or_insert(0) += sample.count;
        }
        if log.gc_count > 0 {
            *folded.entry("Automatic_GC".to_string()).or_insert(0) += log.gc_count;
        }
        if log.discarded > 0 {
            *folded.entry("Discarded_Samples".to_string()).or_insert(0) += log.discarded;
        }
        let mut lines: Vec<(String, u64)> = folded.into_iter().collect();
        lines.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut out = String::new();
        for (stack, count) in lines {
            out.push_str(&stack);
            out.push(' ');
            out.push_str(&count.to_string());
            out.push('\n');
        }
        out
    }

    /// Public diagnostics entry point: begin a CPU capture at `interval_ns`.
    ///
    /// Returns false WITHOUT touching the profiler only when an INTERACTIVE CPU
    /// session (`M-x profiler-start`) is already running, so a diagnostics probe
    /// never hijacks a user's session. If a previous *diagnostics* session is
    /// still running (e.g. an orphan whose stop task never drained), it is
    /// reclaimed — stopped and restarted — so captures can't get wedged. On a
    /// fresh start it resets the log so the capture reflects only this window.
    pub fn diagnostics_cpu_profile_start(&mut self, interval_ns: u64) -> bool {
        if self.profiler_cpu_running() {
            if self.profiler.cpu_diagnostics_owned {
                // Reclaim an orphaned diagnostics session.
                self.profiler_cpu_stop();
            } else {
                return false;
            }
        }
        self.profiler.cpu_log = None;
        let started = self.profiler_cpu_start(interval_ns);
        self.profiler.cpu_diagnostics_owned = started;
        started
    }

    /// Public diagnostics entry point: fold the accrued CPU samples into
    /// Brendan-Gregg folded stacks, then stop the profiler and clear the log so
    /// the next capture starts clean. Must run on the Lisp thread (invoked from
    /// an `EvalThreadTask`) so name resolution via the interner is valid.
    pub fn diagnostics_cpu_profile_stop_fold(&mut self) -> String {
        let folded = self.profiler_cpu_folded();
        self.profiler_cpu_stop();
        self.profiler.cpu_log = None;
        self.profiler.cpu_diagnostics_owned = false;
        folded
    }

    /// Public diagnostics entry point: stop the profiler and discard the log
    /// without folding. Cleans up a capture whose HTTP request was cancelled,
    /// so an orphaned session can't contaminate the next capture.
    pub fn diagnostics_cpu_profile_abort(&mut self) {
        self.profiler_cpu_stop();
        self.profiler.cpu_log = None;
        self.profiler.cpu_diagnostics_owned = false;
    }

    pub(crate) fn profiler_memory_start(&mut self) -> bool {
        if self.profiler.memory_running {
            return false;
        }
        if self.profiler.memory_log.is_none() {
            let (depth, capacity) = self.profiler_settings();
            self.profiler.memory_log = Some(ProfilerLog::new(depth, capacity));
        }
        self.profiler.memory_last_allocated = self.tagged_heap.total_allocated_bytes();
        self.profiler.memory_running = true;
        if !self.profiler.cpu_running {
            // Allocation attribution rides the same tick; 1ms default when no
            // CPU interval is armed.
            arm_profiler_sample_timer(1_000_000);
        }
        true
    }

    pub(crate) fn profiler_memory_stop(&mut self) -> bool {
        if !self.profiler.memory_running {
            return false;
        }
        self.profiler_poll();
        self.profiler.memory_running = false;
        if !self.profiler.cpu_running {
            disarm_profiler_sample_timer();
        }
        true
    }

    pub(crate) fn profiler_memory_running(&self) -> bool {
        self.profiler.memory_running
    }

    pub(crate) fn profiler_memory_log(&mut self) -> Option<Value> {
        self.profiler_poll();
        let was_running = std::mem::replace(&mut self.profiler.memory_running, false);
        let Some(log) = self.profiler.memory_log.as_ref() else {
            self.profiler.memory_running = was_running;
            return None;
        };
        let value = log.to_value();
        let log = self
            .profiler
            .memory_log
            .take()
            .expect("profiler log checked above");
        if was_running {
            self.profiler.memory_log = Some(ProfilerLog::new(log.depth, log.capacity));
            self.profiler.memory_last_allocated = self.tagged_heap.total_allocated_bytes();
            self.profiler.memory_running = true;
        }
        Some(value)
    }

    pub(crate) fn trace_profiler_roots(&self, visit: &mut dyn FnMut(Value)) {
        self.profiler.trace_roots(visit);
    }

    fn profiler_settings(&self) -> (usize, usize) {
        let bounded_setting = |name: &str, default: usize, maximum: usize| {
            self.obarray
                .symbol_value(name)
                .and_then(|value| value.as_fixnum())
                .map(|value| value.clamp(0, maximum as i64) as usize)
                .unwrap_or(default)
        };
        (
            bounded_setting(
                "profiler-max-stack-depth",
                DEFAULT_MAX_STACK_DEPTH,
                MAX_STACK_DEPTH,
            ),
            bounded_setting("profiler-log-size", DEFAULT_LOG_SIZE, MAX_LOG_SIZE),
        )
    }
}

#[cfg(unix)]
fn thread_cpu_time_ns() -> u64 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let result = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
    if result == 0 {
        (time.tv_sec as u64)
            .saturating_mul(1_000_000_000)
            .saturating_add(time.tv_nsec as u64)
    } else {
        monotonic_fallback_ns()
    }
}

#[cfg(windows)]
fn thread_cpu_time_ns() -> u64 {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};

    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok = unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    if ok == 0 {
        return monotonic_fallback_ns();
    }
    let ticks = filetime_ticks(kernel).saturating_add(filetime_ticks(user));
    ticks.saturating_mul(100)
}

#[cfg(windows)]
fn filetime_ticks(time: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64
}

#[cfg(not(any(unix, windows)))]
fn thread_cpu_time_ns() -> u64 {
    monotonic_fallback_ns()
}

fn monotonic_fallback_ns() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(Instant::now)
        .elapsed()
        .as_nanos()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_folded_renders_recorded_stacks_with_names_and_sorts_by_count() {
        let mut ctx = Context::new();
        // Not running, so the profiler_poll() inside profiler_cpu_folded is a
        // no-op and the log holds exactly what we record.
        assert!(!ctx.profiler.is_active());
        ctx.profiler.cpu_log = Some(ProfilerLog::new(16, 100));
        {
            let log = ctx.profiler.cpu_log.as_mut().unwrap();
            // Innermost-first, matching profiler_poll's rev() collection.
            log.record(&[Value::symbol("neo-leaf"), Value::symbol("neo-root")], 7);
            log.record(&[Value::symbol("neo-solo")], 3);
        }
        let folded = ctx.profiler_cpu_folded();
        // Root-first Brendan-Gregg order after the reverse; highest count first.
        assert!(
            folded.starts_with("neo-root;neo-leaf 7"),
            "folded was:\n{folded}"
        );
        assert!(folded.contains("neo-solo 3"), "folded was:\n{folded}");
    }

    #[test]
    fn diagnostics_capture_resets_stale_log_and_does_not_hijack() {
        let mut ctx = Context::new();
        assert!(!ctx.profiler.is_active());

        // Stale samples from a hypothetical prior capture.
        ctx.profiler.cpu_log = Some(ProfilerLog::new(16, 100));
        ctx.profiler
            .cpu_log
            .as_mut()
            .unwrap()
            .record(&[Value::symbol("stale-sym")], 99);

        // A fresh capture resets the log, so the stale sample cannot survive.
        assert!(ctx.diagnostics_cpu_profile_start(1_000_000));
        assert!(ctx.profiler_cpu_running());
        let folded = ctx.diagnostics_cpu_profile_stop_fold();
        assert!(
            !folded.contains("stale-sym"),
            "stale sample leaked: {folded}"
        );
        // Stopping clears the log so the next capture starts clean.
        assert!(!ctx.profiler_cpu_running());

        // A diagnostics capture reclaims its OWN orphaned session (a prior
        // diagnostics start whose stop never ran) instead of wedging.
        assert!(ctx.diagnostics_cpu_profile_start(1_000_000));
        assert!(
            ctx.diagnostics_cpu_profile_start(1_000_000),
            "should reclaim an orphaned diagnostics session"
        );
        ctx.diagnostics_cpu_profile_abort();
        assert!(!ctx.profiler_cpu_running());

        // But it must NOT hijack an interactive (non-diagnostics) session.
        assert!(ctx.profiler_cpu_start(1_000_000));
        assert!(
            !ctx.diagnostics_cpu_profile_start(1_000_000),
            "must not hijack an interactive profiler-start session"
        );
        assert!(ctx.profiler_cpu_running());
        ctx.profiler_cpu_stop();
    }

    #[test]
    fn bounded_log_evicts_cold_samples_and_reports_discarded_weight() {
        let mut log = ProfilerLog::new(2, 2);
        log.record(&[Value::symbol("hot")], 10);
        log.record(&[Value::symbol("cold")], 1);
        log.record(&[Value::symbol("new")], 2);

        assert_eq!(log.entries.len(), 2);
        assert_eq!(log.discarded, 1);
    }

    #[test]
    fn same_source_interpreted_closure_instances_do_not_merge_like_gnu_function_equal() {
        // GNU 31: two interpreted-function instances of the same lambda are NOT
        // `function-equal` (they merge EQ-only). Only compiled closures merge by
        // shared bytecode -- see the compiled-closure test below.
        let mut ctx = Context::new();
        let closures = ctx
            .eval_str(
                "(let ((make-closure (lambda () (lambda (value) value))))
                   (list (funcall make-closure) (funcall make-closure)))",
            )
            .unwrap();
        let first = closures.cons_car();
        let second = closures.cons_cdr().cons_car();
        assert_ne!(first.bits(), second.bits());
        assert!(!first.function_equal(second));

        let mut log = ProfilerLog::new(1, 10);
        log.record(&[first], 2);
        log.record(&[second], 3);
        assert_eq!(log.entries.len(), 2);
    }

    #[test]
    fn same_source_compiled_closure_instances_merge_like_gnu_function_equal() {
        let mut ctx = Context::new();
        let closures = ctx
            .eval_str(
                r#"(let* ((prototype (make-byte-code 0 "\300\207" [nil] 1))
                          (first (make-closure prototype 1))
                          (second (make-closure prototype 2)))
                     (list first second))"#,
            )
            .unwrap();
        let first = closures.cons_car();
        let second = closures.cons_cdr().cons_car();
        assert_ne!(first.bits(), second.bits());
        assert!(first.function_equal(second));

        let mut log = ProfilerLog::new(1, 10);
        log.record(&[first], 2);
        log.record(&[second], 3);
        assert_eq!(log.entries.len(), 1);
        assert_eq!(log.entries.values().next().unwrap().count, 5);
    }

    #[test]
    fn structurally_equal_export_keys_preserve_all_counts() {
        let mut ctx = Context::new();
        let first = ctx.eval_str("(lambda (value) value)").unwrap();
        let second = ctx.eval_str("(lambda (value) value)").unwrap();
        assert_ne!(first.bits(), second.bits());
        assert!(!first.function_equal(second));

        let mut log = ProfilerLog::new(1, 10);
        log.record(&[first], 2);
        log.record(&[second], 3);
        assert_eq!(log.entries.len(), 2);

        let table = log.to_value();
        let table = table.as_hash_table().unwrap();
        assert_eq!(table.data.len(), 1);
        assert_eq!(table.data.values().next().unwrap().as_fixnum(), Some(5));
    }

    #[test]
    fn zero_capacity_discards_samples_without_panicking() {
        let mut log = ProfilerLog::new(0, 0);
        log.record(&[Value::symbol("ignored")], 7);
        assert!(log.entries.is_empty());
        assert_eq!(log.discarded, 7);
    }

    #[test]
    fn automatic_gc_samples_use_the_gnu_special_bucket() {
        let mut log = ProfilerLog::new(4, 10);
        log.record_gc(9);
        let table = log.to_value();
        let table = table.as_hash_table().unwrap();
        let key = *table.key_snapshots().next().unwrap();
        let frames = key.as_vector_data().unwrap();
        assert_eq!(frames.len(), 2);
        assert!(frames[0].is_symbol_named("Automatic GC"));
        assert_eq!(table.data.values().next().unwrap().as_fixnum(), Some(9));
    }

    #[test]
    fn profiler_frames_remain_gc_roots_after_the_call_returns() {
        let mut ctx = Context::new();
        let closure = ctx.eval_str("(lambda (value) value)").unwrap();
        let mut log = ProfilerLog::new(1, 10);
        log.record(&[closure], 1);
        ctx.profiler.memory_log = Some(log);

        ctx.gc_collect_exact();

        let table = ctx.profiler_memory_log().unwrap();
        let key = *table
            .as_hash_table()
            .unwrap()
            .key_snapshots()
            .next()
            .unwrap();
        let frame = key.as_vector_data().unwrap()[0];
        assert!(frame.is_lambda());
    }

    #[test]
    fn profiler_el_public_memory_workflow_builds_and_renders_a_report() {
        let mut ctx = crate::emacs_core::load::create_bootstrap_evaluator_cached().unwrap();
        let lisp_root = std::path::Path::new(env!("CARGO_WORKSPACE_DIR"))
            .join("lisp")
            .canonicalize()
            .unwrap();
        let profiler_el = lisp_root.join("profiler.el");
        ctx.obarray.set_symbol_value(
            "load-path",
            Value::list(vec![
                Value::string(lisp_root.to_string_lossy()),
                Value::string(lisp_root.join("emacs-lisp").to_string_lossy()),
            ]),
        );
        let load_result = ctx.eval_str(&format!(
            r#"(load {:?} nil t t)"#,
            profiler_el.to_string_lossy()
        ));
        assert_eq!(crate::emacs_core::format_eval_result(&load_result), "OK t");

        ctx.eval_str("(profiler-start 'mem)").unwrap();
        ctx.eval_str("(make-list 256 'profiled-value)").unwrap();
        ctx.eval_str("(profiler-stop)").unwrap();
        assert!(
            ctx.eval_str("(and (hash-table-p profiler-memory-log) (> (hash-table-count profiler-memory-log) 0))")
                .unwrap()
                .is_truthy()
        );
        assert!(
            ctx.eval_str("(profiler-calltree-p (profiler-calltree-build profiler-memory-log))")
                .unwrap()
                .is_truthy()
        );
        ctx.eval_str("(profiler-report)").unwrap();
    }
}
