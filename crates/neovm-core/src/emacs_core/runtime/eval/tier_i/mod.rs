//! Tier-I, fallback B (P4.2 Part A, row U3.9): hot interpreted closures run
//! through a *closure-compiled* interpreter instead of the tree walker.
//!
//! # What it is
//!
//! A lambda body that has been called [`TierI::threshold`] times is compiled
//! once into a tree of [`compile::Node`]s that mirrors its source conses.  The
//! executor (`exec.rs`) then evaluates the body by walking the *live* conses
//! exactly as `eval_sub` and the special forms do -- the same frames, depth,
//! polls, specpdl entries, temp roots, error data and `ThreadBlocked`
//! continuations, through the same helper functions -- with three things
//! taken from the compiled tree instead of re-derived per evaluation:
//!
//! 1. **The head's class.**  Each form node caches its head's
//!    [`FormHead`] against the obarray's function epoch (the tree walker
//!    probes the shared [`FormHeadCache`] for it), and knows which special
//!    form it was compiled as.
//! 2. **The subform to evaluate next.**  A special form or call node holds one
//!    child node per argument position.  A child is used only when the live
//!    cons at that position still holds *the very object* the child was
//!    compiled from (`eq`); anything else is evaluated with `eval_sub`, which
//!    is the tree walker itself.  A child re-validates its own conses the same
//!    way, so mutated code (`setcar` on a body cons) behaves exactly as the
//!    tree walker behaves: every cons is read when the tree walker would read
//!    it.
//! 3. **Lexical variables.**  Every binder in the body (the formals, `let`,
//!    `let*`) owns a slot in a per-activation array.  When a binder binds its
//!    symbol lexically it conses the `(SYM . VAL)` cell onto the alist exactly
//!    as the tree walker does and also records that cell in its slot.  A
//!    variable reference or `setq` whose innermost enclosing binders are
//!    known reads the first filled slot instead of running `assq` over the
//!    environment: that cell is the one `assq` would find, because nothing but
//!    those binders pushes `(SYM . VAL)` cells onto an activation's
//!    environment (callees and islands restore `self.lexenv` through the
//!    specpdl; a bare `defvar` pushes a symbol, which `assq` skips).  A binder
//!    whose live shape differs from the compiled one marks the whole
//!    activation *untrusted*, and every later reference in it takes the tree
//!    walker's lookup.
//! 4. **What the captured environment holds.**  Scanned once per activation:
//!    when it holds no `(SYM . VAL)` cell, a symbol no enclosing binder bound
//!    lexically has no lexical cell at all, so its reference and `setq` go
//!    straight to the dynamic value (the tree walker's second stage); when it
//!    holds no special declaration (a bare symbol) and no `defvar` or island
//!    has run at the activation's level, a `let` skips the declaration walk.
//!
//! Everything that is not one of the mirrored special forms or a call of a
//! subr, byte-code object or interpreted closure -- macros, autoloads,
//! aliases, `function`, `defvar`, literal `lambda` heads, symbols with
//! position -- is an *island*: the tree walker's own `eval_sub_cons_dispatch`
//! or `eval_sub` runs it.  Exactness is therefore by construction; there is
//! no deoptimization and no guard that could be stale.  GNU-observable
//! behaviour is the tree walker's.  The only differences are in caches
//! nobody can observe (the form-head cache and the lexical lookup caches are
//! not consulted for compiled forms) and allocation of Rust-side memory.
//!
//! # Heat and lifetime
//!
//! [`TierI`] maps a body's address to a [`TierEntry`].  Calls are counted per
//! body (all closures cconv makes from one `lambda` share the body cons); the
//! entry of a compiled body roots every heap value its nodes hold (the body,
//! the arglist and every form and constant) through [`TierI::trace_roots`], so
//! a compiled key can never be recycled and a node's identity compare can
//! never match a new object at a reused address.  Compiled entries are never
//! dropped; past [`MAX_COMPILED_BODIES`] nothing more is compiled.  Entries
//! that only count heat are not rooted: their key is a number that is never
//! dereferenced, so a recycled address only moves a heat count.
//!
//! No tiered code runs once any thread other than the main one exists: the
//! cooperative-thread continuations (`Flow::ThreadBlocked`) are mirrored, but
//! the design keeps them out of scope.
//!
//! # Knobs (read once per process)
//!
//! `NEOVM_TIER_I`:
//! - unset, `off`: nothing (the call hook is one byte test);
//! - `census`: count calls per body and report the hottest bodies by work
//!   (calls x cons forms) at `kill-emacs` (T0);
//! - `analyze`: `census`, and compile at the threshold, reporting how much of
//!   each body the compiler covers natively; compiled code never runs (T4);
//! - `on`: compile at the threshold and run the compiled code;
//! - `verify`: `on`, plus a check after every compiled form that the
//!   evaluation depth, the specpdl, the operand stack and the lexical
//!   environment are balanced as the tree walker leaves them.
//!
//! `NEOVM_TIER_I_THRESHOLD` (default 2): the call count at which a body is
//! compiled.  `NEOVM_TIER_I_REPORT=<path>`: also write the report there.
//! The report is logged at `info` under the `neovm::tier_i` target.

use super::*;
use std::rc::Rc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use strum::{EnumCount, IntoEnumIterator};

mod compile;
mod exec;

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
    /// `Census`, and compile at the threshold; never run compiled code.
    Analyze = 2,
    /// Compile at the threshold and run compiled code.
    On = 3,
    /// `On`, with balance checks after every compiled form.
    Verify = 4,
}

impl TierIMode {
    /// Whether the call hook has anything to do.
    #[inline(always)]
    pub(crate) fn engaged(self) -> bool {
        self != TierIMode::Off
    }

    /// Whether bodies are compiled at the threshold.
    pub(crate) fn compiles(self) -> bool {
        matches!(self, TierIMode::Analyze | TierIMode::On | TierIMode::Verify)
    }

    /// Whether compiled code runs.
    pub(crate) fn runs(self) -> bool {
        matches!(self, TierIMode::On | TierIMode::Verify)
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
        "1" | "on" | "true" | "yes" => TierIMode::On,
        "verify" => TierIMode::Verify,
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
        3 => TierIMode::On,
        4 => TierIMode::Verify,
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
    /// A compiled body ran.
    Run,
    /// A compiled body was not run: a thread other than the main one exists.
    RefuseThreads,
    /// A compiled body was not run: the closure's arglist is not the one the
    /// body was compiled with.
    RefuseArglist,
    /// A compiled body was not run: the formals' binding cells are not the
    /// compiled formals (a mutated arglist).
    RefuseFormals,
    /// A binder's live shape differed from the compiled one; the rest of the
    /// activation takes the tree walker's variable lookups.
    Untrusted,
    /// A compiled form was dispatched by the tree walker (an island).
    Island,
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
    pub(super) fn note(&mut self, event: TierIEvent) {
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
    /// The per-activation slot stack: each running compiled body owns
    /// `slots[base..base + code.nslots]`.  Holds lexical binding cells only
    /// while they are reachable from the environment (module docs), so it is
    /// not traced.
    pub(super) slots: Vec<Value>,
}

impl TierI {
    pub(crate) fn new(mode: TierIMode, threshold: u32) -> Self {
        Self {
            mode,
            threshold: threshold.max(1),
            stats: TierIStats::default(),
            entries: FxHashMap::default(),
            compiled: 0,
            slots: Vec::new(),
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

    pub(crate) fn mode(&self) -> TierIMode {
        self.mode
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

    /// Count a call of BODY and return its compiled code when it should run.
    /// Compiles at the threshold.
    fn enter(&mut self, obarray: &Obarray, arglist: Value, body: Value) -> Option<Rc<TierCode>> {
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
        if let Some(code) = &entry.code {
            return mode.runs().then(|| Rc::clone(code));
        }
        if entry.refused || !mode.compiles() || entry.calls < u64::from(self.threshold) {
            return None;
        }
        if self.compiled >= MAX_COMPILED_BODIES {
            entry.refused = true;
            self.stats.note(TierIEvent::CompileCapped);
            return None;
        }
        match compile_body(obarray, arglist, body) {
            Some(code) => {
                let code = Rc::new(code);
                entry.forms = code.summary().forms;
                entry.code = Some(Rc::clone(&code));
                self.compiled += 1;
                self.stats.note(TierIEvent::Compiled);
                mode.runs().then_some(code)
            }
            None => {
                entry.refused = true;
                self.stats.note(TierIEvent::CompileRefused);
                None
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
    /// NEW_ENV: GNU `funcall_lambda`'s `specbind` of the environment, then the
    /// body -- [`Self::run_lexical_closure_body`] with the Tier-I hook.
    #[inline(never)]
    pub(super) fn tier_i_run_lexical_body(
        &mut self,
        arglist: Value,
        new_env: Value,
        body: Value,
    ) -> EvalResult {
        let Some(code) = self.tier_i.enter(&self.obarray, arglist, body) else {
            return self.run_lexical_closure_body(new_env, body);
        };
        let count = self.specpdl.len();
        let old_lexenv = std::mem::replace(&mut self.lexenv, new_env);
        self.push_specpdl_with(|| SpecBinding::LexicalEnv { old_lexenv });
        let result = self.tier_i_run_code(&code, arglist, body, new_env);
        let result = self.rewrap_thread_blocked_in_lexenv(result);
        self.unbind_lexenv_frame(count, result)
    }

    /// A dynamic closure's BODY after `begin_lambda_call` bound its formals:
    /// [`Self::eval_lambda_body_value`] with the Tier-I hook.
    #[inline(never)]
    pub(super) fn tier_i_run_dynamic_body(&mut self, arglist: Value, body: Value) -> EvalResult {
        match self.tier_i.enter(&self.obarray, arglist, body) {
            Some(code) => self.tier_i_run_code(&code, arglist, body, Value::NIL),
            None => self.eval_lambda_body_value(body),
        }
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
