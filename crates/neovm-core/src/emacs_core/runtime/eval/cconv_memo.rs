//! Interpreted-closure creation through `cconv-make-interpreted-closure`
//! (P4.1 Stage 0): an exact memo of its analysis, its statistics, and the
//! effect snapshot.
//!
//! GNU's `Ffunction` (src/eval.c:560-627) hands every `#'(lambda ...)`
//! evaluated in a non-empty lexical environment to
//! `internal-make-interpreted-closure-function`, whose value in a dumped
//! Emacs is `cconv-make-interpreted-closure` (lisp/emacs-lisp/cconv.el:
//! 905-972).  When the environment binds lexical variables that function
//! runs `macroexpand-all` and `cconv-fv` over the lambda to trim the
//! environment: about 113K instructions per closure, against 6.9K for the
//! rest of a typical creation.
//!
//! # What is memoized
//!
//! A trimming call is *eligible* when the memo could serve it: the trusted
//! set stands (`cconv_trust`), a well-formed environment, a source whose
//! [`ClosureShape`] and [`ClosureFacts`] exist, no interactive form, and
//! every head [`HeadVerdict::Plain`] -- no macro, autoloaded macro or
//! compiler macro, as `macroexpand-1` (macroexp.el:225-247) and
//! `function-get` (subr.el:4836-4854) would see it.
//!
//! The first eligible call on a body is *recorded*: the real Lisp runs, and
//! its result is kept only if it signalled nothing, moved no
//! [`EffectSnapshot`] counter, left the trusted set standing and returned an
//! identity closure (args and body `eq` to the inputs) whose environment is
//! `(assq V ENV)` cells for some lexical variables V followed by some bare
//! symbols, or cconv's `(t)` constant.  The entry keeps that *analysis* --
//! the ordered captured variables and dynamic symbols -- never a closure,
//! keyed by the body's address and validated on every use:
//!
//! - V1: the trusted set still stands;
//! - V2: the live `(ARGS . BODY)` has the recorded [`ClosureShape`] (a
//!   recycled address or a mutated body just misses);
//! - V3: the environment's lexical and dynamic entries are the recorded
//!   ones, in order;
//! - V4: the visible `lexical-binding` has the recorded truth value
//!   (`cconv--not-lexical-var-p` reads the caller's value, cconv.el:641);
//! - V5: every head's [`HeadVerdict`] is still `Plain` and every symbol's
//!   `special-variable-p` is unchanged (a later `defvar`, `defmacro`,
//!   compiler macro or autoload misses);
//! - V7: `max-lisp-eval-depth` leaves room for the depth the real run would
//!   use (4 per nesting level plus 64), so the memo never succeeds where GNU
//!   signals `excessive-lisp-nesting`.
//!
//! A hit builds what cconv.el:966-972 builds: a fresh list of the live
//! environment's `(assq V ENV)` cells and the dynamic symbols, or the `(t)`
//! constant, and `make-interpreted-closure` of the live args, body,
//! docstring and interactive form.  The memo does not run while
//! `debug-on-next-call` is armed, a quit is pending, or
//! `macroexpand-all-environment`, `overriding-plist-environment`,
//! `macroexp-inhibit-compiler-macros` or `symbols-with-pos-enabled` is
//! non-nil.
//!
//! Excluded from exactness, as for P0.9's unboxed floats: allocation volume
//! and what depends on it (`cons-cells-consed`, GC timing).
//!
//! # Knob
//!
//! `NEOVM_CCONV_MEMO` (read once per process):
//! - unset, `0` or `off`: closure creation exactly as it was;
//! - `stats`: record and look up, count every outcome ([`CconvMemoEvent`]),
//!   but always run the Lisp;
//! - `on` (or `1`): serve hits;
//! - `verify`: on a hit build the memo closure *and* run the Lisp, compare
//!   them element by element, return the Lisp's closure and count
//!   mismatches; `NEOVM_CCONV_MEMO_STRICT=1` (and test builds) panic on one.
//!
//! The counts are logged at `info` under the `neovm::cconv_memo` target every
//! 4096 trimming calls; [`Context::cconv_memo_report`] formats them.
//!
//! # Effect snapshot
//!
//! [`EffectSnapshot`] is a cheap, generic "did this Lisp run do anything
//! observable" check: loads, echo-area output, buffer creation and text
//! changes, `gensym-counter`, function-cell writes, newly interned symbols and
//! the match data.  Taken before and after a run, a difference means the run
//! had an effect.

use super::cconv_shape::{ClosureFacts, ClosureShape, EnvSummary, FactsRefusal};
use super::cconv_trust::TrustedSet;
use super::*;
use std::sync::atomic::{AtomicU8, Ordering};
use strum::{EnumCount, IntoEnumIterator};

// ---------------------------------------------------------------------------
// Knob
// ---------------------------------------------------------------------------

/// What happens around a call of `cconv-make-interpreted-closure`, as
/// `NEOVM_CCONV_MEMO` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum CconvMemoMode {
    /// Closure creation is untouched.
    Off = 0,
    /// Record and look up, count everything, always run the Lisp.
    Stats = 1,
    /// Serve validated hits.
    On = 2,
    /// Serve nothing; on a hit compare the memo's closure with the Lisp's.
    Verify = 3,
}

/// [`CCONV_MEMO_MODE`] before the knob is read.
const MODE_UNREAD: u8 = 0xff;

static CCONV_MEMO_MODE: AtomicU8 = AtomicU8::new(MODE_UNREAD);

/// The mode a value of `NEOVM_CCONV_MEMO` selects.
pub(crate) fn parse_cconv_memo_knob(value: Option<&str>) -> CconvMemoMode {
    let Some(value) = value.map(str::trim) else {
        return CconvMemoMode::Off;
    };
    match value.to_ascii_lowercase().as_str() {
        "" | "0" | "off" | "false" | "no" => CconvMemoMode::Off,
        "stats" => CconvMemoMode::Stats,
        "1" | "on" | "true" | "yes" => CconvMemoMode::On,
        "verify" => CconvMemoMode::Verify,
        other => {
            tracing::warn!(value = other, "NEOVM_CCONV_MEMO: unknown mode, using off");
            CconvMemoMode::Off
        }
    }
}

/// Whether a verify mismatch panics: `NEOVM_CCONV_MEMO_STRICT=1`, and every
/// test build.
fn cconv_memo_strict_from_env() -> bool {
    cfg!(test)
        || matches!(
            std::env::var("NEOVM_CCONV_MEMO_STRICT").ok().as_deref(),
            Some("1" | "on" | "true" | "yes")
        )
}

/// The process-wide mode, read from the environment on first use.
pub(crate) fn cconv_memo_mode_from_env() -> CconvMemoMode {
    match CCONV_MEMO_MODE.load(Ordering::Relaxed) {
        MODE_UNREAD => {
            let mode = parse_cconv_memo_knob(std::env::var("NEOVM_CCONV_MEMO").ok().as_deref());
            CCONV_MEMO_MODE.store(mode as u8, Ordering::Relaxed);
            mode
        }
        1 => CconvMemoMode::Stats,
        2 => CconvMemoMode::On,
        3 => CconvMemoMode::Verify,
        _ => CconvMemoMode::Off,
    }
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// One thing that happened to one call of the filter.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::EnumCount, strum::EnumIter, strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum CconvMemoEvent {
    /// `internal-make-interpreted-closure-function` was
    /// `cconv-make-interpreted-closure` and the environment was non-nil.
    Call,
    /// The environment binds no lexical variable (cconv.el:935-949 returns
    /// the closure untrimmed).
    NoLexvars,
    /// The environment binds lexical variables: the trimming path.
    Trim,
    /// The trusted set does not stand (see `cconv_trust`): a trusted
    /// function was redefined or advised, or the set could not be built.
    Untrusted,
    /// Not run through the memo: `debug-on-next-call` is armed.
    BypassDebug,
    /// Not run through the memo: a quit is pending.
    BypassQuit,
    /// Not run through the memo: a compilation environment
    /// (`macroexpand-all-environment`, `overriding-plist-environment`,
    /// `macroexp-inhibit-compiler-macros`, `symbols-with-pos-enabled`).
    BypassCompileEnv,
    /// A lookup validated an entry (V1-V7).
    Hit,
    /// `on`: the hit was served.
    Served,
    /// `verify`: the Lisp built the same closure the memo did.
    VerifyMatch,
    /// `verify`: it did not (logged at `error`).
    VerifyMismatch,
    /// No entry for this body and environment.
    MissNoEntry,
    /// The body at the recorded address has another shape (V2).
    MissShape,
    /// A head became a macro or compiler macro, or a symbol's specialness
    /// changed (V5).
    MissFacts,
    /// Too close to `max-lisp-eval-depth` (V7); run, not re-recorded.
    MissDepth,
    /// A trimming call whose inputs the memo could serve (S0.3's checks).
    Eligible,
    /// The environment is not a proper list of `(SYMBOL . VALUE)` and bare
    /// symbol entries.
    RefuseEnv,
    /// The source is too large or cyclic, or holds a symbol with position.
    RefuseShape,
    /// An interactive form: `cconv-analyze-form` may write
    /// `cconv--interactive-form-funs` (cconv.el:776).
    RefuseInteractive,
    /// A `_` variable is used (see `FactsRefusal::UnderscoreUse`).
    RefuseUnderscore,
    /// A head is a macro, an autoloaded macro, or an unresolvable alias.
    RefuseMacroHead,
    /// A head has a compiler macro.
    RefuseCompilerMacroHead,
    /// A trimming run signalled or threw.
    RunError,
    /// A trimming run returned a closure whose args and body are `eq` to the
    /// ones it was given (an identity expansion).
    RunIdentity,
    /// A trimming run returned a rewritten body (a macro or compiler macro
    /// expanded), or something that is not an interpreted closure.
    RunRewritten,
    /// The effect snapshot moved across a trimming run.
    RunEffect,
    /// A recording run's analysis was stored.
    Recorded,
    /// A recording run returned an environment the memo cannot rebuild (not
    /// `assq` cells followed by symbols), or the trusted set moved.
    RecordRefused,
    /// The memo reached its entry cap and was cleared.
    Cleared,
}

/// Counts per [`CconvMemoEvent`].
#[derive(Clone, Debug, Default)]
pub(crate) struct CconvMemoStats {
    counts: [u64; CconvMemoEvent::COUNT],
}

impl CconvMemoStats {
    #[inline]
    pub(crate) fn count(&self, event: CconvMemoEvent) -> u64 {
        self.counts[event as usize]
    }

    #[inline]
    fn note(&mut self, event: CconvMemoEvent) {
        self.counts[event as usize] = self.counts[event as usize].wrapping_add(1);
    }

    /// One line per non-zero counter, in declaration order.
    pub(crate) fn report(&self) -> String {
        let mut out = String::from("cconv-memo:");
        for event in CconvMemoEvent::iter() {
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
// Effect snapshot
// ---------------------------------------------------------------------------

/// The observable state a Lisp run could change without leaving a trace in
/// its return value (see the module docs).  Compared, never interpreted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EffectSnapshot {
    loads: u64,
    outputs: u64,
    next_buffer_id: u64,
    live_buffers: usize,
    buffer_ticks: i64,
    gensym_counter: usize,
    function_epoch: u64,
    obarray_len: usize,
    match_data: Option<String>,
}

// ---------------------------------------------------------------------------
// The per-Context state
// ---------------------------------------------------------------------------

/// Everything the filter hook keeps on a [`Context`].  Its only Lisp values
/// are the trusted set's, which the root walk traces.
#[derive(Debug)]
pub(crate) struct CconvMemo {
    mode: CconvMemoMode,
    /// Panic on a verify mismatch.
    strict: bool,
    stats: CconvMemoStats,
    pub(super) trusted: TrustedSet,
    /// Recorded analyses by body address (see the module docs).
    entries: FxHashMap<usize, SmallVec<[MemoEntry; 2]>>,
    entry_count: usize,
    /// Files loaded (bumped by the loader), for [`EffectSnapshot`].
    loads: u64,
    /// Echo-area / stderr messages emitted, for [`EffectSnapshot`].
    outputs: u64,
}

impl CconvMemo {
    pub(crate) fn new(mode: CconvMemoMode) -> Self {
        Self {
            mode,
            strict: cconv_memo_strict_from_env(),
            stats: CconvMemoStats::default(),
            trusted: TrustedSet::default(),
            entries: FxHashMap::default(),
            entry_count: 0,
            loads: 0,
            outputs: 0,
        }
    }

    /// A memo in the process-wide mode (`NEOVM_CCONV_MEMO`).
    pub(crate) fn from_env() -> Self {
        Self::new(cconv_memo_mode_from_env())
    }

    #[inline(always)]
    pub(crate) fn engaged(&self) -> bool {
        self.mode != CconvMemoMode::Off
    }

    #[cfg(test)]
    pub(crate) fn set_mode(&mut self, mode: CconvMemoMode) {
        self.mode = mode;
    }

    #[cfg(test)]
    pub(crate) fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
    }

    /// Corrupt one recorded analysis (drop its first captured variable), so
    /// `verify` has something to catch (T0.12).
    #[cfg(test)]
    pub(crate) fn inject_wrong_analysis_for_test(&mut self) -> bool {
        for bucket in self.entries.values_mut() {
            for entry in bucket.iter_mut() {
                if !entry.lexv.is_empty() {
                    entry.lexv = entry.lexv[1..].into();
                    return true;
                }
            }
        }
        false
    }

    /// Forget every recorded analysis.
    pub(crate) fn clear_entries(&mut self) {
        self.entries.clear();
        self.entry_count = 0;
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> &CconvMemoStats {
        &self.stats
    }

    #[cfg(test)]
    pub(crate) fn trusted(&self) -> &TrustedSet {
        &self.trusted
    }

    pub(crate) fn trace_roots(&self, visit: &mut dyn FnMut(Value)) {
        self.trusted.trace_roots(visit);
    }

    fn note(&mut self, event: CconvMemoEvent) {
        self.stats.note(event);
        if event == CconvMemoEvent::Trim && self.stats.count(event) % 4096 == 0 {
            tracing::info!(target: "neovm::cconv_memo", "{}", self.stats.report());
        }
    }
}

/// Most analyses the memo keeps; reaching it clears the memo.
const MEMO_ENTRY_CAP: usize = 8192;

/// A symbol fact validated on every hit (V5).
#[derive(Clone, Copy, Debug)]
struct MemoFact {
    id: SymId,
    head: bool,
    special: bool,
}

/// One recorded analysis (see the module docs).
#[derive(Debug)]
struct MemoEntry {
    shape: ClosureShape,
    facts: Box<[MemoFact]>,
    env: EnvSummary,
    lexical_binding: bool,
    /// Captured lexical variables, in the closure environment's order.
    lexv: Box<[SymId]>,
    /// Dynamic symbols appended after them.
    dynv: Box<[SymId]>,
    /// Depth the real run may use beyond the current depth (V7).
    depth_budget: usize,
}

/// A validated entry's analysis, copied out of the map.
struct MemoHit {
    lexv: SmallVec<[SymId; 8]>,
    dynv: SmallVec<[SymId; 4]>,
}

/// What a recording run can be kept as.
struct Analysis {
    shape: ClosureShape,
    facts: Box<[MemoFact]>,
}

cached_symbol_id!(
    cconv_make_interpreted_closure_symbol,
    "cconv-make-interpreted-closure"
);
cached_symbol_id!(
    macroexpand_all_environment_symbol,
    "macroexpand-all-environment"
);
cached_symbol_id!(
    overriding_plist_environment_memo_symbol,
    "overriding-plist-environment"
);
cached_symbol_id!(
    macroexp_inhibit_compiler_macros_symbol,
    "macroexp-inhibit-compiler-macros"
);
cached_symbol_id!(gensym_counter_symbol, "gensym-counter");
cached_symbol_id!(compiler_macro_symbol, "compiler-macro");
cached_symbol_id!(autoload_cell_symbol, "autoload");
cached_symbol_id!(macro_cell_symbol, "macro");

/// Longest alias chain [`Context::cconv_head_verdict`] follows.
const ALIAS_HOP_CAP: usize = 100;

/// What `macroexp--expand-all` would do with a form headed by a symbol, as
/// far as the memo cares (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeadVerdict {
    /// Not a macro, not an autoloaded macro, no compiler macro: the form is
    /// left as it is.
    Plain,
    /// A macro, directly or through an alias.
    Macro,
    /// An autoload of type `macro` or `t`: `macroexpand-1` would load it.
    AutoloadMacro,
    /// `(function-get SYM 'compiler-macro)` is non-nil.
    CompilerMacro,
    /// An alias chain longer than [`ALIAS_HOP_CAP`] (or cyclic).
    Unresolvable,
}

/// Whether DEF is an autoload whose TYPE (element 4) is `macro` or `t`:
/// `autoload-do-load` with MACRO-ONLY `macro` would load it
/// (src/eval.c `Fautoload_do_load`).
fn autoload_of_macro(def: Value) -> bool {
    if !def.is_cons() || def.cons_car().as_symbol_id() != Some(autoload_cell_symbol()) {
        return false;
    }
    let mut tail = def;
    for _ in 0..4 {
        tail = tail.cons_cdr();
        if !tail.is_cons() {
            return false;
        }
    }
    let kind = tail.cons_car();
    kind.is_t() || kind.as_symbol_id() == Some(macro_cell_symbol())
}

fn is_macro_cell(def: Value) -> bool {
    def.is_cons() && def.cons_car().as_symbol_id() == Some(macro_cell_symbol())
}

/// The `(assq VAR ENV)` of cconv.el:965, skipping non-cons entries.
fn env_assq(env: Value, var: SymId) -> Option<Value> {
    let mut tail = env;
    while tail.is_cons() {
        let entry = tail.cons_car();
        if entry.is_cons() && entry.cons_car().as_symbol_id() == Some(var) {
            return Some(entry);
        }
        tail = tail.cons_cdr();
    }
    None
}

/// Whether two closures are the same object element by element, with
/// their environment spines compared by their elements (both are fresh).
fn closures_agree(memo: Value, lisp: Value) -> bool {
    use crate::tagged::header::CLOSURE_CONSTANTS;
    if memo.veclike_type() != lisp.veclike_type() {
        return false;
    }
    let (Some(memo_slots), Some(lisp_slots)) = (memo.closure_slots(), lisp.closure_slots()) else {
        return false;
    };
    if memo_slots.len() != lisp_slots.len() {
        return false;
    }
    for (index, (a, b)) in memo_slots.iter().zip(lisp_slots.iter()).enumerate() {
        if index != CLOSURE_CONSTANTS {
            if a != b {
                return false;
            }
            continue;
        }
        if a == b {
            continue;
        }
        let (mut x, mut y) = (*a, *b);
        while x.is_cons() && y.is_cons() {
            if x.cons_car() != y.cons_car() {
                return false;
            }
            x = x.cons_cdr();
            y = y.cons_cdr();
        }
        if !(x.is_nil() && y.is_nil()) {
            return false;
        }
    }
    true
}

/// Whether ENV binds a lexical variable the way cconv.el:931 sees it:
/// `(delq nil (mapcar #'car-safe env))` is non-empty.
fn env_has_lexvars(env: Value) -> bool {
    let mut tail = env;
    while tail.is_cons() {
        let entry = tail.cons_car();
        if entry.is_cons() && !entry.cons_car().is_nil() {
            return true;
        }
        tail = tail.cons_cdr();
    }
    false
}

impl Context {
    /// A file is being loaded ([`EffectSnapshot`]).
    pub(crate) fn note_load_effect(&mut self) {
        self.cconv_memo.loads = self.cconv_memo.loads.wrapping_add(1);
    }

    /// A message is being emitted ([`EffectSnapshot`]).
    pub(crate) fn note_output_effect(&mut self) {
        self.cconv_memo.outputs = self.cconv_memo.outputs.wrapping_add(1);
    }

    /// The current [`EffectSnapshot`].  Walks the live buffers, so it is
    /// taken around a recorded run, never on a hit.
    pub(crate) fn cconv_effect_snapshot(&self) -> EffectSnapshot {
        let live = self.buffers.buffer_list();
        let buffer_ticks = live
            .iter()
            .filter_map(|id| self.buffers.get(*id))
            .fold(0i64, |sum, buffer| sum.wrapping_add(buffer.modified_tick()));
        EffectSnapshot {
            loads: self.cconv_memo.loads,
            outputs: self.cconv_memo.outputs,
            next_buffer_id: self.buffers.dump_next_id(),
            live_buffers: live.len(),
            buffer_ticks,
            gensym_counter: self
                .obarray
                .symbol_value_id(gensym_counter_symbol())
                .map_or(0, |value| value.bits()),
            function_epoch: self.obarray.function_epoch(),
            obarray_len: self.obarray.len(),
            match_data: self.match_data.as_ref().map(|data| format!("{data:?}")),
        }
    }

    /// The statistics line (see the module docs).
    #[cfg(test)]
    pub(crate) fn cconv_memo_report(&self) -> String {
        self.cconv_memo.stats.report()
    }

    /// The symbol's function cell as `fboundp` sees it.
    fn cconv_fbound_cell(&self, id: SymId) -> Option<Value> {
        self.obarray
            .symbol_function_id(id)
            .filter(|cell| !cell.is_nil())
    }

    /// GNU `macrop` (subr.el:4793-4799) of a function cell that is a symbol:
    /// follow the aliases, then a `(macro ...)` or an autoloaded macro.
    fn cconv_symbol_macrop(&self, mut id: SymId) -> Option<bool> {
        for _ in 0..ALIAS_HOP_CAP {
            let Some(def) = self.cconv_fbound_cell(id) else {
                return Some(false);
            };
            match def.as_symbol_id() {
                Some(next) if !def.is_nil() => id = next,
                _ => return Some(is_macro_cell(def) || autoload_of_macro(def)),
            }
        }
        None
    }

    /// [`HeadVerdict`] for a form headed by ID, mirroring `macroexpand-1`
    /// with a nil environment and then `function-get` of `compiler-macro`
    /// with AUTOLOAD nil.  Reads only the function cells and plists; the
    /// memo refuses to run when `overriding-plist-environment` could make
    /// `get` answer otherwise.
    pub(crate) fn cconv_head_verdict(&self, id: SymId) -> HeadVerdict {
        if let Some(def) = self.cconv_fbound_cell(id) {
            if autoload_of_macro(def) {
                return HeadVerdict::AutoloadMacro;
            }
            if is_macro_cell(def) {
                return HeadVerdict::Macro;
            }
            if let Some(alias) = def.as_symbol_id() {
                match self.cconv_symbol_macrop(alias) {
                    Some(true) => return HeadVerdict::Macro,
                    Some(false) => {}
                    None => return HeadVerdict::Unresolvable,
                }
            }
        }
        let mut f = id;
        for _ in 0..ALIAS_HOP_CAP {
            if self
                .obarray
                .get_property_id(f, compiler_macro_symbol())
                .is_some_and(|handler| !handler.is_nil())
            {
                return HeadVerdict::CompilerMacro;
            }
            let Some(cell) = self.cconv_fbound_cell(f) else {
                return HeadVerdict::Plain;
            };
            match cell.as_symbol_id() {
                Some(next) => f = next,
                None => return HeadVerdict::Plain,
            }
        }
        HeadVerdict::Unresolvable
    }

    /// The memo's view of a trimming call's source: its shape and facts,
    /// or the first refusal.
    fn cconv_analyze_source(&self, params: Value, body: Value) -> Result<Analysis, CconvMemoEvent> {
        let shape = ClosureShape::of(params, body).map_err(|_| CconvMemoEvent::RefuseShape)?;
        if shape.mentions_interactive {
            return Err(CconvMemoEvent::RefuseInteractive);
        }
        let roles = match ClosureFacts::of(params, body) {
            Ok(facts) => facts.symbols,
            Err(FactsRefusal::TooLarge) => return Err(CconvMemoEvent::RefuseShape),
            Err(FactsRefusal::UnderscoreUse) => return Err(CconvMemoEvent::RefuseUnderscore),
        };
        let mut facts = Vec::with_capacity(roles.len());
        for role in roles {
            if role.head {
                match self.cconv_head_verdict(role.id) {
                    HeadVerdict::Plain => {}
                    HeadVerdict::CompilerMacro => {
                        return Err(CconvMemoEvent::RefuseCompilerMacroHead);
                    }
                    HeadVerdict::Macro | HeadVerdict::AutoloadMacro | HeadVerdict::Unresolvable => {
                        return Err(CconvMemoEvent::RefuseMacroHead);
                    }
                }
            }
            facts.push(MemoFact {
                id: role.id,
                head: role.head,
                special: self.obarray.is_special_id(role.id),
            });
        }
        Ok(Analysis {
            shape,
            facts: facts.into_boxed_slice(),
        })
    }

    /// Why the memo must not run now, if it must not.
    fn cconv_bypass(&self) -> Option<CconvMemoEvent> {
        if self.debug_on_next_call_is_armed() {
            return Some(CconvMemoEvent::BypassDebug);
        }
        if !self.quit_flag.is_nil() {
            return Some(CconvMemoEvent::BypassQuit);
        }
        if self.symbols_with_pos_enabled
            || [
                macroexpand_all_environment_symbol(),
                overriding_plist_environment_memo_symbol(),
                macroexp_inhibit_compiler_macros_symbol(),
            ]
            .into_iter()
            .any(|var| self.cconv_variable_truthy(var))
        {
            return Some(CconvMemoEvent::BypassCompileEnv);
        }
        None
    }

    /// A special variable's visible value is non-nil, as `varref` reads it.
    fn cconv_variable_truthy(&self, var: SymId) -> bool {
        self.visible_runtime_variable_value_by_id(var)
            .ok()
            .flatten()
            .is_some_and(|value| !value.is_nil())
    }

    /// The tightest `max-lisp-eval-depth` in force (global, buffer-local,
    /// cached), so V7 errs on the side of running the Lisp.
    fn cconv_eval_depth_limit(&self) -> usize {
        let as_limit = |value: Option<Value>| {
            value
                .and_then(|v| v.as_fixnum())
                .map(|n| n.max(100) as usize)
        };
        let mut limit = self.max_depth;
        if let Some(global) = as_limit(
            self.obarray
                .symbol_value_id(max_lisp_eval_depth_symbol())
                .copied(),
        ) {
            limit = limit.min(global);
        }
        if self.obarray.is_localized(max_lisp_eval_depth_symbol())
            && let Some(local) = as_limit(
                self.obarray
                    .value_in_buffer(self.buffers.current_buffer(), "max-lisp-eval-depth"),
            )
        {
            limit = limit.min(local);
        }
        limit
    }

    /// Find and validate (V2-V7) an entry for this call.
    fn cconv_memo_lookup(
        &self,
        params: Value,
        body: Value,
        env: &EnvSummary,
        lexical_binding: bool,
    ) -> Result<MemoHit, CconvMemoEvent> {
        let Some(bucket) = self.cconv_memo.entries.get(&body.bits()) else {
            return Err(CconvMemoEvent::MissNoEntry);
        };
        let Some(entry) = bucket
            .iter()
            .find(|entry| entry.lexical_binding == lexical_binding && entry.env == *env)
        else {
            return Err(CconvMemoEvent::MissNoEntry);
        };
        if !entry.shape.matches(params, body) {
            return Err(CconvMemoEvent::MissShape);
        }
        for fact in entry.facts.iter() {
            if (fact.head && self.cconv_head_verdict(fact.id) != HeadVerdict::Plain)
                || self.obarray.is_special_id(fact.id) != fact.special
            {
                return Err(CconvMemoEvent::MissFacts);
            }
        }
        if self.depth.saturating_add(entry.depth_budget) > self.cconv_eval_depth_limit() {
            return Err(CconvMemoEvent::MissDepth);
        }
        Ok(MemoHit {
            lexv: entry.lexv.iter().copied().collect(),
            dynv: entry.dynv.iter().copied().collect(),
        })
    }

    /// Build the closure cconv.el:966-972 would return for an analysis.
    fn cconv_memo_build(
        &mut self,
        params: Value,
        body: Value,
        env: Value,
        docstring: Value,
        iform: Value,
        hit: &MemoHit,
    ) -> EvalResult {
        let newenv = if hit.lexv.is_empty() && hit.dynv.is_empty() {
            self.cconv_memo.trusted.empty_env()
        } else {
            let mut items = Vec::with_capacity(hit.lexv.len() + hit.dynv.len());
            for var in &hit.lexv {
                // V3 matched the environment's lexical cars, so the cell
                // exists; a miss here would be a memo bug, so run the Lisp.
                let Some(cell) = env_assq(env, *var) else {
                    return Err(signal(
                        LispCondition::Error,
                        vec![Value::string("cconv memo: lost environment cell")],
                    ));
                };
                items.push(cell);
            }
            items.extend(hit.dynv.iter().map(|var| Value::from_sym_id(*var)));
            Value::list(items)
        };
        let scope = self.save_specpdl_roots();
        self.push_specpdl_root(newenv);
        let closure = builtins::symbols::make_interpreted_closure_from_parts(
            &params,
            &body,
            &newenv,
            Some(&docstring),
            Some(&iform),
        );
        self.restore_specpdl_roots(scope);
        closure
    }

    /// Derive a recording run's analysis result from the closure it returned
    /// (see the module docs); `None` if the memo could not rebuild it.
    fn cconv_derive_result(
        &self,
        closure: Value,
        params: Value,
        body: Value,
        env: Value,
    ) -> Option<(Box<[SymId]>, Box<[SymId]>)> {
        use crate::tagged::header::{CLOSURE_ARGLIST, CLOSURE_CODE, CLOSURE_CONSTANTS};
        if closure.veclike_type() != Some(VecLikeType::Lambda) {
            return None;
        }
        let slots = closure.closure_slots()?;
        if slots.get(CLOSURE_ARGLIST) != Some(&params) || slots.get(CLOSURE_CODE) != Some(&body) {
            return None;
        }
        let newenv = *slots.get(CLOSURE_CONSTANTS)?;
        if newenv == self.cconv_memo.trusted.empty_env() {
            return Some((Box::default(), Box::default()));
        }
        let mut lexv = Vec::new();
        let mut dynv = Vec::new();
        let mut tail = newenv;
        while tail.is_cons() {
            let item = tail.cons_car();
            if item.is_cons() {
                let var = item.cons_car().as_symbol_id()?;
                if !dynv.is_empty() || env_assq(env, var) != Some(item) {
                    return None;
                }
                lexv.push(var);
            } else if item.is_symbol() && !item.is_nil() && !item.is_t() {
                dynv.push(item.as_symbol_id()?);
            } else {
                return None;
            }
            tail = tail.cons_cdr();
        }
        if !tail.is_nil() || (lexv.is_empty() && dynv.is_empty()) {
            return None;
        }
        Some((lexv.into_boxed_slice(), dynv.into_boxed_slice()))
    }

    /// Run the filter itself.
    fn cconv_run_lisp(
        &mut self,
        closure_hook: Value,
        params: Value,
        body: Value,
        env: Value,
        docstring: Value,
        iform: Value,
    ) -> EvalResult {
        self.apply(closure_hook, vec![params, body, env, docstring, iform])
    }

    /// Run the filter and classify the run ([`CconvMemoEvent::RunError`] ..
    /// [`CconvMemoEvent::RunEffect`]); `true` in the second slot when the
    /// run was clean: no error, an identity closure, no effect.
    fn cconv_run_observed(
        &mut self,
        closure_hook: Value,
        params: Value,
        body: Value,
        env: Value,
        docstring: Value,
        iform: Value,
    ) -> (EvalResult, bool) {
        let before = self.cconv_effect_snapshot();
        let result = self.cconv_run_lisp(closure_hook, params, body, env, docstring, iform);
        let mut clean = match &result {
            Err(_) => {
                self.cconv_memo.note(CconvMemoEvent::RunError);
                false
            }
            Ok(closure) => {
                let identity = closure
                    .closure_slot(crate::tagged::header::CLOSURE_ARGLIST)
                    .is_some_and(|a| a == params)
                    && closure.closure_body_value().is_some_and(|b| b == body);
                self.cconv_memo.note(if identity {
                    CconvMemoEvent::RunIdentity
                } else {
                    CconvMemoEvent::RunRewritten
                });
                identity
            }
        };
        if self.cconv_effect_snapshot() != before {
            self.cconv_memo.note(CconvMemoEvent::RunEffect);
            clean = false;
        }
        (result, clean)
    }

    /// Run a call the memo does not serve: observed (for the statistics)
    /// unless the memo is `on`, where only recording runs pay for that.
    fn cconv_run_unserved(
        &mut self,
        closure_hook: Value,
        params: Value,
        body: Value,
        env: Value,
        docstring: Value,
        iform: Value,
    ) -> EvalResult {
        if self.cconv_memo.mode == CconvMemoMode::On {
            self.cconv_run_lisp(closure_hook, params, body, env, docstring, iform)
        } else {
            self.cconv_run_observed(closure_hook, params, body, env, docstring, iform)
                .0
        }
    }

    /// Whether this hook call goes through [`Self::cconv_filter_call`]: the
    /// knob is on and the filter is `cconv-make-interpreted-closure` itself.
    #[inline]
    pub(super) fn cconv_filter_call_applies(&self, closure_hook: Value) -> bool {
        self.cconv_memo.engaged()
            && closure_hook.as_symbol_id() == Some(cconv_make_interpreted_closure_symbol())
    }

    /// `cconv-make-interpreted-closure` on the parts `Ffunction` built,
    /// through the memo (see the module docs).  The caller holds every
    /// argument as a root.
    #[inline(never)]
    pub(super) fn cconv_filter_call(
        &mut self,
        closure_hook: Value,
        params: Value,
        body: Value,
        env: Value,
        docstring: Value,
        iform: Value,
    ) -> EvalResult {
        self.cconv_memo.note(CconvMemoEvent::Call);
        if !env_has_lexvars(env) {
            self.cconv_memo.note(CconvMemoEvent::NoLexvars);
            return self.cconv_run_lisp(closure_hook, params, body, env, docstring, iform);
        }
        self.cconv_memo.note(CconvMemoEvent::Trim);
        let refusal = if !self.cconv_trusted_set_valid() {
            Some(CconvMemoEvent::Untrusted)
        } else if !iform.is_nil() {
            Some(CconvMemoEvent::RefuseInteractive)
        } else {
            self.cconv_bypass()
        };
        let summary = match (refusal, EnvSummary::of(env)) {
            (None, Some(summary)) => summary,
            (refusal, _) => {
                self.cconv_memo
                    .note(refusal.unwrap_or(CconvMemoEvent::RefuseEnv));
                return self.cconv_run_unserved(closure_hook, params, body, env, docstring, iform);
            }
        };
        let lexical_binding = self.cconv_variable_truthy(lexical_binding_symbol());
        match self.cconv_memo_lookup(params, body, &summary, lexical_binding) {
            Ok(hit) => {
                self.cconv_memo.note(CconvMemoEvent::Hit);
                return match self.cconv_memo.mode {
                    CconvMemoMode::On => {
                        self.cconv_memo.note(CconvMemoEvent::Served);
                        self.cconv_memo_build(params, body, env, docstring, iform, &hit)
                    }
                    CconvMemoMode::Verify => self.cconv_memo_verify(
                        closure_hook,
                        params,
                        body,
                        env,
                        docstring,
                        iform,
                        &hit,
                    ),
                    CconvMemoMode::Stats | CconvMemoMode::Off => {
                        self.cconv_run_lisp(closure_hook, params, body, env, docstring, iform)
                    }
                };
            }
            Err(miss) => {
                self.cconv_memo.note(miss);
                if miss == CconvMemoEvent::MissDepth {
                    return self.cconv_run_unserved(
                        closure_hook,
                        params,
                        body,
                        env,
                        docstring,
                        iform,
                    );
                }
            }
        }
        let analysis = match self.cconv_analyze_source(params, body) {
            Ok(analysis) => analysis,
            Err(refusal) => {
                self.cconv_memo.note(refusal);
                return self.cconv_run_unserved(closure_hook, params, body, env, docstring, iform);
            }
        };
        self.cconv_memo.note(CconvMemoEvent::Eligible);
        let (result, clean) =
            self.cconv_run_observed(closure_hook, params, body, env, docstring, iform);
        if !clean {
            return result;
        }
        let derived = match &result {
            Ok(closure) if self.cconv_trusted_set_valid() => {
                self.cconv_derive_result(*closure, params, body, env)
            }
            _ => None,
        };
        let Some((lexv, dynv)) = derived else {
            self.cconv_memo.note(CconvMemoEvent::RecordRefused);
            return result;
        };
        self.cconv_memo_insert(
            body,
            MemoEntry {
                depth_budget: 4 * analysis.shape.car_depth as usize + 64,
                shape: analysis.shape,
                facts: analysis.facts,
                env: summary,
                lexical_binding,
                lexv,
                dynv,
            },
        );
        result
    }

    fn cconv_memo_insert(&mut self, body: Value, entry: MemoEntry) {
        let memo = &mut self.cconv_memo;
        if memo.entry_count >= MEMO_ENTRY_CAP {
            memo.clear_entries();
            memo.note(CconvMemoEvent::Cleared);
        }
        let bucket = memo.entries.entry(body.bits()).or_default();
        // A stale entry for the same environment (its shape or facts no
        // longer held) is replaced.
        if let Some(old) = bucket
            .iter_mut()
            .find(|old| old.lexical_binding == entry.lexical_binding && old.env == entry.env)
        {
            *old = entry;
        } else {
            bucket.push(entry);
            memo.entry_count += 1;
        }
        memo.note(CconvMemoEvent::Recorded);
    }

    /// `verify` on a hit: build the memo's closure, run the Lisp, compare.
    #[allow(clippy::too_many_arguments)]
    fn cconv_memo_verify(
        &mut self,
        closure_hook: Value,
        params: Value,
        body: Value,
        env: Value,
        docstring: Value,
        iform: Value,
        hit: &MemoHit,
    ) -> EvalResult {
        let memo = self.cconv_memo_build(params, body, env, docstring, iform, hit);
        let scope = self.save_specpdl_roots();
        if let Ok(closure) = &memo {
            self.push_specpdl_root(*closure);
        }
        let before = self.cconv_effect_snapshot();
        let lisp = self.cconv_run_lisp(closure_hook, params, body, env, docstring, iform);
        let quiet = self.cconv_effect_snapshot() == before;
        let agree = match (&memo, &lisp) {
            (Ok(m), Ok(l)) => closures_agree(*m, *l),
            _ => false,
        };
        self.restore_specpdl_roots(scope);
        if quiet && agree {
            self.cconv_memo.note(CconvMemoEvent::VerifyMatch);
        } else {
            self.cconv_memo.note(CconvMemoEvent::VerifyMismatch);
            let shown = |r: &EvalResult| match r {
                Ok(v) => super::super::print::print_value(v),
                Err(flow) => format!("{flow:?}"),
            };
            tracing::error!(
                target: "neovm::cconv_memo",
                body = %super::super::print::print_value(&body),
                memo = %shown(&memo),
                lisp = %shown(&lisp),
                quiet,
                "cconv memo verify mismatch"
            );
            if self.cconv_memo.strict {
                panic!(
                    "cconv memo verify mismatch: body {} memo {} lisp {} quiet {quiet}",
                    super::super::print::print_value(&body),
                    shown(&memo),
                    shown(&lisp)
                );
            }
        }
        lisp
    }
}
