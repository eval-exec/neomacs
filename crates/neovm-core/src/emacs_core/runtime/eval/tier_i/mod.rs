//! Tier-I (P4.2 Part A, row U3.9), stages T0 and T4: the census of
//! interpreted closure calls, and the analyzer that compiles a hot lambda
//! body into a tree of [`compile::Node`]s mirroring its source conses.  The
//! executor that runs the tree comes with fallback B.
//!
//! # The census (T0)
//!
//! Every interpreted closure body that starts (through `apply_lambda` or the
//! interpreter's own closure call) is counted by the address of its body
//! cons; all closures cconv makes from one `lambda` share it.  At
//! `kill-emacs` the hottest bodies are reported by *work*, calls times the
//! body's cons forms, with the names of the functions whose cells hold them
//! (F-T1: ranking by calls alone picks the wrong functions).
//!
//! # The analyzer (T4)
//!
//! A body called [`TierI::threshold`] times is compiled: its forms, the
//! special forms the executor will mirror, the calls, the forms left to the
//! tree walker whole (islands: macros, literal heads, malformed special
//! forms, symbols with position), and a slot per binder with each variable
//! reference's candidate binders.  The `analyze` report adds the coverage and
//! the trees of the hottest bodies.
//!
//! # Lifetime
//!
//! The entry of a compiled body roots every heap value its nodes hold (the
//! body, the arglist and every form and constant) through
//! [`TierI::trace_roots`], so a compiled key can never be recycled and a
//! node's identity compare can never match a new object at a reused address.
//! Compiled entries are never dropped; past [`MAX_COMPILED_BODIES`] nothing
//! more is compiled.  Entries that only count heat are not rooted: their key
//! is a number that is never dereferenced, so a recycled address only moves a
//! count.
//!
//! # Knobs (read once per process)
//!
//! `NEOVM_TIER_I`:
//! - unset, `off`: nothing (the call hook is one byte test);
//! - `census`: count calls per body and report the hottest bodies by work
//!   at `kill-emacs` (T0);
//! - `analyze`: `census`, and compile at the threshold, reporting how much of
//!   each body the compiler covers natively (T4).
//!
//! `NEOVM_TIER_I_THRESHOLD` (default 2): the call count at which a body is
//! compiled.  `NEOVM_TIER_I_REPORT=<path>`: also write the report there.
//! The report is logged at `info` under the `neovm::tier_i` target.

use super::*;
use std::rc::Rc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use strum::{EnumCount, IntoEnumIterator};

mod compile;

pub(crate) use compile::CompileSummary;
use compile::{TierCode, compile_body};

/// The most bodies ever compiled in one session.  Past it the registry stops
/// compiling (a compiled entry is rooted and never dropped, see the module
/// docs).
pub(crate) const MAX_COMPILED_BODIES: usize = 16_384;

/// How many of the hottest compiled bodies the report prints as trees
/// (`!FORM` marks a form the tree walker runs whole), and how much of each.
const REPORT_TREES: usize = 10;
const REPORT_TREE_CHARS: usize = 600;

/// The most heat-only entries kept before they are all forgotten (they are
/// not rooted, so dropping them is free).
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
    /// `Census`, and compile at the threshold.
    Analyze = 2,
}

impl TierIMode {
    /// Whether the call hook has anything to do.
    #[inline(always)]
    pub(crate) fn engaged(self) -> bool {
        self != TierIMode::Off
    }

    /// Whether bodies are compiled at the threshold.
    pub(crate) fn compiles(self) -> bool {
        self == TierIMode::Analyze
    }
}

const MODE_UNREAD: u8 = 0xff;
static TIER_I_MODE: AtomicU8 = AtomicU8::new(MODE_UNREAD);
const THRESHOLD_UNREAD: u32 = u32::MAX;
static TIER_I_THRESHOLD: AtomicU32 = AtomicU32::new(THRESHOLD_UNREAD);

/// The mode a value of `NEOVM_TIER_I` selects.
pub(crate) fn parse_tier_i_knob(value: Option<&str>) -> TierIMode {
    let Some(value) = value.map(str::trim) else {
        return TierIMode::Off;
    };
    match value.to_ascii_lowercase().as_str() {
        "" | "0" | "off" | "false" | "no" => TierIMode::Off,
        "census" => TierIMode::Census,
        "analyze" => TierIMode::Analyze,
        other => {
            tracing::warn!(value = other, "NEOVM_TIER_I: unknown mode, using off");
            TierIMode::Off
        }
    }
}

/// The call count a value of `NEOVM_TIER_I_THRESHOLD` selects (at least 1).
pub(crate) fn parse_tier_i_threshold(value: Option<&str>) -> u32 {
    const DEFAULT: u32 = 2;
    match value.map(str::trim) {
        None | Some("") => DEFAULT,
        Some(text) => match text.parse::<u32>() {
            Ok(n) => n.max(1),
            Err(_) => {
                tracing::warn!(value = text, "NEOVM_TIER_I_THRESHOLD: not a count, using 2");
                DEFAULT
            }
        },
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
        2 => TierIMode::Analyze,
        _ => TierIMode::Off,
    }
}

fn tier_i_threshold_from_env() -> u32 {
    match TIER_I_THRESHOLD.load(Ordering::Relaxed) {
        THRESHOLD_UNREAD => {
            let n = parse_tier_i_threshold(std::env::var("NEOVM_TIER_I_THRESHOLD").ok().as_deref());
            TIER_I_THRESHOLD.store(n, Ordering::Relaxed);
            n
        }
        n => n,
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
    /// A body was compiled.
    Compiled,
    /// A body could not be compiled (not a proper list, too large, cyclic).
    CompileRefused,
    /// The compiled-body cap was reached; the body stays interpreted.
    CompileCapped,
    /// The heat-only entries were forgotten at their cap.
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

/// What the registry knows about one body.
pub(super) struct TierEntry {
    /// Calls counted (every mode but `Off`).
    pub(super) calls: u64,
    /// Cons forms in the body, counted once on first sight (`census`), the
    /// static work of one call.
    pub(super) forms: u32,
    /// The compiled code, once the threshold was reached.
    pub(super) code: Option<Rc<TierCode>>,
    /// Compilation was refused; never try again.
    pub(super) refused: bool,
}

/// The per-Context Tier-I state (see the module docs).
pub(crate) struct TierI {
    mode: TierIMode,
    threshold: u32,
    stats: TierIStats,
    /// Body address -> entry.
    entries: FxHashMap<usize, TierEntry>,
    compiled: usize,
}

impl TierI {
    pub(crate) fn new(mode: TierIMode, threshold: u32) -> Self {
        Self {
            mode,
            threshold: threshold.max(1),
            stats: TierIStats::default(),
            entries: FxHashMap::default(),
            compiled: 0,
        }
    }

    /// The process-wide knobs.
    pub(crate) fn from_env() -> Self {
        Self::new(tier_i_mode_from_env(), tier_i_threshold_from_env())
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

    #[cfg(test)]
    pub(crate) fn set_threshold(&mut self, threshold: u32) {
        self.threshold = threshold.max(1);
    }

    /// Forget everything (a test resetting its context).
    #[cfg(test)]
    pub(crate) fn clear_for_test(&mut self) {
        self.entries.clear();
        self.compiled = 0;
        self.stats = TierIStats::default();
    }

    /// Every heap value a compiled body's nodes hold (module docs).
    pub(crate) fn trace_roots(&self, visit: &mut dyn FnMut(Value)) {
        for entry in self.entries.values() {
            if let Some(code) = &entry.code {
                for value in code.roots() {
                    visit(*value);
                }
            }
        }
    }

    /// Count a call of BODY, and compile it at the threshold.
    fn enter(&mut self, obarray: &Obarray, arglist: Value, body: Value) {
        self.stats.note(TierIEvent::Call);
        let key = body.bits();
        if self.entries.len() >= MAX_HEAT_ENTRIES && !self.entries.contains_key(&key) {
            self.entries.retain(|_, entry| entry.code.is_some());
            self.stats.note(TierIEvent::HeatCleared);
        }
        let mode = self.mode;
        let entry = self.entries.entry(key).or_insert_with(|| TierEntry {
            calls: 0,
            forms: if mode == TierIMode::Census {
                count_cons_forms(body)
            } else {
                0
            },
            code: None,
            refused: false,
        });
        entry.calls = entry.calls.saturating_add(1);
        if entry.code.is_some()
            || entry.refused
            || !mode.compiles()
            || entry.calls < u64::from(self.threshold)
        {
            return;
        }
        if self.compiled >= MAX_COMPILED_BODIES {
            entry.refused = true;
            self.stats.note(TierIEvent::CompileCapped);
            return;
        }
        match compile_body(obarray, arglist, body) {
            Some(code) => {
                entry.forms = code.summary().forms;
                entry.code = Some(Rc::new(code));
                self.compiled += 1;
                self.stats.note(TierIEvent::Compiled);
            }
            None => {
                entry.refused = true;
                self.stats.note(TierIEvent::CompileRefused);
            }
        }
    }

    /// The census and coverage tables: the hottest bodies by work, with the
    /// names of the functions whose cells hold them.
    fn report_lines(&self, names: &FxHashMap<usize, String>, limit: usize) -> Vec<String> {
        let mut rows: Vec<(&usize, &TierEntry)> = self.entries.iter().collect();
        let work = |entry: &TierEntry| entry.calls.saturating_mul(u64::from(entry.forms.max(1)));
        rows.sort_by(|a, b| work(b.1).cmp(&work(a.1)).then(b.1.calls.cmp(&a.1.calls)));
        let total_calls: u64 = self.entries.values().map(|e| e.calls).sum();
        let total_work: u64 = self.entries.values().map(work).sum();
        let mut lines = vec![format!(
            "tier-i census: bodies={} calls={} work={} compiled={}",
            self.entries.len(),
            total_calls,
            total_work,
            self.compiled
        )];
        let mut covered = CompileSummary::default();
        for entry in self.entries.values() {
            if let Some(code) = &entry.code {
                covered.add(code.summary());
            }
        }
        if covered.forms > 0 {
            lines.push(format!("tier-i coverage (all compiled bodies): {covered}"));
        }
        let mut trees = Vec::new();
        for (key, entry) in rows.into_iter().take(limit) {
            let name = names.get(key).map(String::as_str).unwrap_or("<anonymous>");
            let coverage = entry
                .code
                .as_ref()
                .map(|code| format!(" {}", code.summary()))
                .unwrap_or_default();
            lines.push(format!(
                "{:>12} work {:>9} calls {:>5} forms  {name}{coverage}",
                work(entry),
                entry.calls,
                entry.forms
            ));
            if let Some(code) = &entry.code
                && trees.len() < REPORT_TREES
            {
                let mut tree = code.describe();
                if tree.len() > REPORT_TREE_CHARS {
                    let mut end = REPORT_TREE_CHARS;
                    while !tree.is_char_boundary(end) {
                        end -= 1;
                    }
                    tree.truncate(end);
                    tree.push_str(" ...");
                }
                trees.push(format!("tier-i tree {name}: {tree}"));
            }
        }
        lines.extend(trees);
        lines
    }
}

impl Context {
    /// A lexical closure's BODY, whose formals are already consed onto
    /// NEW_ENV: [`Self::run_lexical_closure_body`] with the Tier-I hook.
    #[inline(never)]
    pub(super) fn tier_i_run_lexical_body(
        &mut self,
        arglist: Value,
        new_env: Value,
        body: Value,
    ) -> EvalResult {
        self.tier_i.enter(&self.obarray, arglist, body);
        self.run_lexical_closure_body(new_env, body)
    }

    /// A dynamic closure's BODY after `begin_lambda_call` bound its formals:
    /// [`Self::eval_lambda_body_value`] with the Tier-I hook.
    #[inline(never)]
    pub(super) fn tier_i_run_dynamic_body(&mut self, arglist: Value, body: Value) -> EvalResult {
        self.tier_i.enter(&self.obarray, arglist, body);
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

    /// The compiled tree of the function NAME's body, when it is compiled.
    #[cfg(test)]
    pub(crate) fn tier_i_describe_function(&self, name: &str) -> Option<String> {
        let cell = self.obarray.symbol_function_id(intern(name))?;
        let body = cell.closure_body_value()?;
        let entry = self.tier_i.entries.get(&body.bits())?;
        entry.code.as_ref().map(|code| code.describe())
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
