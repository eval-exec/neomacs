use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(crate) enum HotpathOp {
    StringMatch,
    RegexCompileHit,
    RegexCompileMiss,
    RegexLiteralFind,
    RegexMatchDataChars,
    MatchBeginning,
    MatchEnd,
    RegexpQuote,
    Substring,
    Concat,
    Format,
    Assoc,
}

#[derive(Clone, Copy, Default)]
struct HotpathCounter {
    count: u64,
    total: Duration,
    max: Duration,
}

impl HotpathCounter {
    fn record(&mut self, elapsed: Duration) {
        self.count += 1;
        self.total += elapsed;
        self.max = self.max.max(elapsed);
    }
}

#[derive(Clone, Default)]
struct HotpathStats {
    string_match: HotpathCounter,
    regex_compile_hit: HotpathCounter,
    regex_compile_miss: HotpathCounter,
    regex_literal_find: HotpathCounter,
    regex_match_data_chars: HotpathCounter,
    match_beginning: HotpathCounter,
    match_end: HotpathCounter,
    regexp_quote: HotpathCounter,
    substring: HotpathCounter,
    concat: HotpathCounter,
    format: HotpathCounter,
    assoc: HotpathCounter,
}

impl HotpathStats {
    fn counter_mut(&mut self, op: HotpathOp) -> &mut HotpathCounter {
        match op {
            HotpathOp::StringMatch => &mut self.string_match,
            HotpathOp::RegexCompileHit => &mut self.regex_compile_hit,
            HotpathOp::RegexCompileMiss => &mut self.regex_compile_miss,
            HotpathOp::RegexLiteralFind => &mut self.regex_literal_find,
            HotpathOp::RegexMatchDataChars => &mut self.regex_match_data_chars,
            HotpathOp::MatchBeginning => &mut self.match_beginning,
            HotpathOp::MatchEnd => &mut self.match_end,
            HotpathOp::RegexpQuote => &mut self.regexp_quote,
            HotpathOp::Substring => &mut self.substring,
            HotpathOp::Concat => &mut self.concat,
            HotpathOp::Format => &mut self.format,
            HotpathOp::Assoc => &mut self.assoc,
        }
    }

    fn entries(&self) -> [(&'static str, HotpathCounter); 12] {
        [
            ("string-match", self.string_match),
            ("regex-compile-hit", self.regex_compile_hit),
            ("regex-compile-miss", self.regex_compile_miss),
            ("regex-literal-find", self.regex_literal_find),
            ("regex-match-data-chars", self.regex_match_data_chars),
            ("match-beginning", self.match_beginning),
            ("match-end", self.match_end),
            ("regexp-quote", self.regexp_quote),
            ("substring", self.substring),
            ("concat", self.concat),
            ("format", self.format),
            ("assoc", self.assoc),
        ]
    }
}

struct HotpathState {
    stats: HotpathStats,
    op_events: u64,
    last_live_log: Instant,
}

impl Default for HotpathState {
    fn default() -> Self {
        Self {
            stats: HotpathStats::default(),
            op_events: 0,
            last_live_log: Instant::now(),
        }
    }
}

thread_local! {
    static HOTPATH_STATE: RefCell<HotpathState> = RefCell::new(HotpathState::default());
}

const LIVE_LOG_EVERY_OPS: u64 = 2_048;
const LIVE_LOG_EVERY: Duration = Duration::from_secs(5);

pub(crate) fn hotpath_timing_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("NEOVM_ORACLE_TIMING").is_some()
            || std::env::var_os("NEOVM_HOTPATH_TIMING").is_some()
    })
}

pub(crate) fn time_op<R>(op: HotpathOp, f: impl FnOnce() -> R) -> R {
    if !hotpath_timing_enabled() {
        return f();
    }

    let start = Instant::now();
    let result = f();
    HOTPATH_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.stats.counter_mut(op).record(start.elapsed());
        state.op_events += 1;
        if state.op_events % LIVE_LOG_EVERY_OPS == 0
            && state.last_live_log.elapsed() >= LIVE_LOG_EVERY
        {
            log_snapshot("oracle-hotpath-live", &state.stats);
            state.stats = HotpathStats::default();
            state.last_live_log = Instant::now();
        }
    });
    result
}

pub fn reset_hotpath_stats() {
    if !hotpath_timing_enabled() {
        return;
    }
    HOTPATH_STATE.with(|state| {
        *state.borrow_mut() = HotpathState::default();
    });
}

pub fn log_hotpath_stats(label: &str) {
    if !hotpath_timing_enabled() {
        return;
    }

    HOTPATH_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let snapshot = std::mem::take(&mut state.stats);
        log_snapshot(label, &snapshot);
        state.last_live_log = Instant::now();
    });
}

fn log_snapshot(label: &str, snapshot: &HotpathStats) {
    for (name, counter) in snapshot.entries() {
        if counter.count == 0 {
            continue;
        }
        let total_ms = counter.total.as_secs_f64() * 1000.0;
        let avg_ms = total_ms / counter.count as f64;
        let max_ms = counter.max.as_secs_f64() * 1000.0;
        tracing::info!(
            "{label}: builtin={name} count={} total_ms={:.3} avg_ms={:.3} max_ms={:.3}",
            counter.count,
            total_ms,
            avg_ms,
            max_ms
        );
    }
}
