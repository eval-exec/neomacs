//! Tier-I (P4.2 Part A, row U3.9), stage T0: a census of interpreted
//! closure calls, the heat the later stages tier on.
//!
//! Every interpreted closure body that starts (through `apply_lambda` or the
//! interpreter's own closure call) is counted by the address of its body
//! cons; all closures cconv makes from one `lambda` share it.  At
//! `kill-emacs` the hottest bodies are reported by *work*, calls times the
//! body's cons forms, with the names of the functions whose cells hold them
//! (F-T1: ranking by calls alone picks the wrong functions).
//!
//! The entries are not rooted: a key is a number that is never dereferenced,
//! so a recycled address only moves a count.
//!
//! # Knobs (read once per process)
//!
//! `NEOVM_TIER_I`: unset or `off` (the call hook is one byte test), or
//! `census`.  `NEOVM_TIER_I_REPORT=<path>`: also write the report there.  The
//! report is logged at `info` under the `neovm::tier_i` target.

use super::*;
use std::sync::atomic::{AtomicU8, Ordering};
use strum::{EnumCount, IntoEnumIterator};

/// The most entries kept before they are all forgotten.
const MAX_HEAT_ENTRIES: usize = 1 << 16;

/// The most cons forms the census counts in one body.
const CENSUS_MAX_FORMS: u32 = 50_000;
/// The longest list the census walks.
const CENSUS_MAX_LIST: usize = 10_000;

/// Cons forms in BODY, counted to a bound (the census's static work).
fn count_cons_forms(body: Value) -> u32 {
    fn walk(value: Value, budget: &mut u32, depth: u32) {
        if depth > 64 || *budget == 0 {
            return;
        }
        let mut cursor = value;
        let mut n = 0;
        while cursor.is_cons() && n < CENSUS_MAX_LIST && *budget > 0 {
            let item = cursor.cons_car();
            if item.is_cons() {
                *budget -= 1;
                walk(item, budget, depth + 1);
            }
            cursor = cursor.cons_cdr();
            n += 1;
        }
    }
    let mut budget = CENSUS_MAX_FORMS;
    walk(body, &mut budget, 0);
    CENSUS_MAX_FORMS - budget
}

// ---------------------------------------------------------------------------
// Knob
// ---------------------------------------------------------------------------

/// What `NEOVM_TIER_I` selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum TierIMode {
    /// The tree walker alone.
    Off = 0,
    /// Count calls per body; report at `kill-emacs`.
    Census = 1,
}

impl TierIMode {
    /// Whether the call hook has anything to do.
    #[inline(always)]
    pub(crate) fn engaged(self) -> bool {
        self != TierIMode::Off
    }
}

const MODE_UNREAD: u8 = 0xff;
static TIER_I_MODE: AtomicU8 = AtomicU8::new(MODE_UNREAD);

/// The mode a value of `NEOVM_TIER_I` selects.
pub(crate) fn parse_tier_i_knob(value: Option<&str>) -> TierIMode {
    let Some(value) = value.map(str::trim) else {
        return TierIMode::Off;
    };
    match value.to_ascii_lowercase().as_str() {
        "" | "0" | "off" | "false" | "no" => TierIMode::Off,
        "census" => TierIMode::Census,
        other => {
            tracing::warn!(value = other, "NEOVM_TIER_I: unknown mode, using off");
            TierIMode::Off
        }
    }
}

fn tier_i_mode_from_env() -> TierIMode {
    match TIER_I_MODE.load(Ordering::Relaxed) {
        MODE_UNREAD => {
            let mode = parse_tier_i_knob(std::env::var("NEOVM_TIER_I").ok().as_deref());
            TIER_I_MODE.store(mode as u8, Ordering::Relaxed);
            mode
        }
        1 => TierIMode::Census,
        _ => TierIMode::Off,
    }
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// One thing that happened on the tiered call path.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::EnumCount, strum::EnumIter, strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum TierIEvent {
    /// An interpreted closure body started (lexical or dynamic).
    Call,
    /// The entries were forgotten at their cap.
    HeatCleared,
}

/// Counts per [`TierIEvent`].
#[derive(Clone, Debug, Default)]
pub(crate) struct TierIStats {
    counts: [u64; TierIEvent::COUNT],
}

impl TierIStats {
    #[inline]
    pub(crate) fn count(&self, event: TierIEvent) -> u64 {
        self.counts[event as usize]
    }

    #[inline(always)]
    fn note(&mut self, event: TierIEvent) {
        self.counts[event as usize] = self.counts[event as usize].wrapping_add(1);
    }

    /// One line, every non-zero counter in declaration order.
    pub(crate) fn report(&self) -> String {
        let mut out = String::from("tier-i:");
        for event in TierIEvent::iter() {
            let n = self.count(event);
            if n != 0 {
                let name: &'static str = event.into();
                out.push_str(&format!(" {name}={n}"));
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

/// What the census knows about one body.
struct TierEntry {
    calls: u64,
    /// Cons forms in the body, counted once on first sight: the static work
    /// of one call.
    forms: u32,
}

/// The per-Context Tier-I state (see the module docs).
pub(crate) struct TierI {
    mode: TierIMode,
    stats: TierIStats,
    /// Body address -> entry.
    entries: FxHashMap<usize, TierEntry>,
}

impl TierI {
    pub(crate) fn new(mode: TierIMode) -> Self {
        Self {
            mode,
            stats: TierIStats::default(),
            entries: FxHashMap::default(),
        }
    }

    /// The process-wide knob.
    pub(crate) fn from_env() -> Self {
        Self::new(tier_i_mode_from_env())
    }

    #[inline(always)]
    pub(crate) fn engaged(&self) -> bool {
        self.mode.engaged()
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> &TierIStats {
        &self.stats
    }

    #[cfg(test)]
    pub(crate) fn set_mode(&mut self, mode: TierIMode) {
        self.mode = mode;
    }

    /// Forget everything (a test resetting its context).
    #[cfg(test)]
    pub(crate) fn clear_for_test(&mut self) {
        self.entries.clear();
        self.stats = TierIStats::default();
    }

    /// Count a call of BODY.
    fn enter(&mut self, body: Value) {
        self.stats.note(TierIEvent::Call);
        let key = body.bits();
        if self.entries.len() >= MAX_HEAT_ENTRIES && !self.entries.contains_key(&key) {
            self.entries.clear();
            self.stats.note(TierIEvent::HeatCleared);
        }
        let entry = self.entries.entry(key).or_insert_with(|| TierEntry {
            calls: 0,
            forms: count_cons_forms(body),
        });
        entry.calls = entry.calls.saturating_add(1);
    }

    /// The census table: the hottest bodies by work, with the names of the
    /// functions whose cells hold them.
    fn report_lines(&self, names: &FxHashMap<usize, String>, limit: usize) -> Vec<String> {
        let mut rows: Vec<(&usize, &TierEntry)> = self.entries.iter().collect();
        let work = |entry: &TierEntry| entry.calls.saturating_mul(u64::from(entry.forms.max(1)));
        rows.sort_by(|a, b| work(b.1).cmp(&work(a.1)).then(b.1.calls.cmp(&a.1.calls)));
        let total_calls: u64 = self.entries.values().map(|e| e.calls).sum();
        let total_work: u64 = self.entries.values().map(work).sum();
        let mut lines = vec![format!(
            "tier-i census: bodies={} calls={} work={}",
            self.entries.len(),
            total_calls,
            total_work,
        )];
        for (key, entry) in rows.into_iter().take(limit) {
            let name = names.get(key).map(String::as_str).unwrap_or("<anonymous>");
            lines.push(format!(
                "{:>12} work {:>9} calls {:>5} forms  {name}",
                work(entry),
                entry.calls,
                entry.forms
            ));
        }
        lines
    }
}

impl Context {
    /// A lexical closure's BODY, whose formals are already consed onto
    /// NEW_ENV: [`Self::run_lexical_closure_body`] with the Tier-I hook.
    #[inline(never)]
    pub(super) fn tier_i_run_lexical_body(
        &mut self,
        _arglist: Value,
        new_env: Value,
        body: Value,
    ) -> EvalResult {
        self.tier_i.enter(body);
        self.run_lexical_closure_body(new_env, body)
    }

    /// A dynamic closure's BODY after `begin_lambda_call` bound its formals:
    /// [`Self::eval_lambda_body_value`] with the Tier-I hook.
    #[inline(never)]
    pub(super) fn tier_i_run_dynamic_body(&mut self, _arglist: Value, body: Value) -> EvalResult {
        self.tier_i.enter(body);
        self.eval_lambda_body_value(body)
    }

    /// The final report lines, with function names resolved (a walk of the
    /// obarray, so only at the end of a session).
    pub(crate) fn tier_i_report_lines(&self, limit: usize) -> Vec<String> {
        let mut names: FxHashMap<usize, String> = FxHashMap::default();
        for (id, _) in self.obarray.iter_symbols() {
            let Some(cell) = self.obarray.symbol_function_id(id) else {
                continue;
            };
            let (closure, macro_suffix) =
                if cell.is_cons() && cell.cons_car().is_symbol_named("macro") {
                    (cell.cons_cdr(), " (macro)")
                } else {
                    (cell, "")
                };
            if closure.veclike_type() == Some(VecLikeType::Lambda)
                && let Some(body) = closure.closure_body_value()
                && self.tier_i.entries.contains_key(&body.bits())
            {
                names
                    .entry(body.bits())
                    .or_insert_with(|| format!("{}{macro_suffix}", resolve_sym(id)));
            }
        }
        let mut lines = vec![self.tier_i.stats.report()];
        lines.extend(self.tier_i.report_lines(&names, limit));
        lines
    }

    /// Log the report at `kill-emacs` when the knob is on (and write it to
    /// `NEOVM_TIER_I_REPORT` when that is set).
    pub(crate) fn log_tier_i_report(&self) {
        if !self.tier_i.engaged() {
            return;
        }
        let lines = self.tier_i_report_lines(60);
        for line in &lines {
            tracing::info!(target: "neovm::tier_i", "final {line}");
        }
        if let Ok(path) = std::env::var("NEOVM_TIER_I_REPORT")
            && !path.is_empty()
            && let Err(error) = std::fs::write(&path, lines.join("\n") + "\n")
        {
            tracing::warn!(target: "neovm::tier_i", %error, path, "cannot write the report");
        }
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
