//! Interpreted-closure creation through `cconv-make-interpreted-closure`
//! (P4.1 Stage 0): call statistics and the effect snapshot.
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
//! This module observes those calls without changing them.  A trimming call
//! is *eligible* when the memo could serve it: a well-formed environment, a
//! source whose [`ClosureShape`] and [`ClosureFacts`] exist, no interactive
//! form, and every head [`HeadVerdict::Plain`] -- no macro, autoloaded macro
//! or compiler macro, as `macroexpand-1` (macroexp.el:225-247) and
//! `function-get` (subr.el:4836-4854) would see it.
//!
//! # Knob
//!
//! `NEOVM_CCONV_MEMO` (read once per process): unset, `0` or `off` leaves
//! closure creation exactly as it was; `stats` counts every call of the
//! filter, how many trim, and what each trimming run did (see
//! [`CconvMemoEvent`]).  The counts are logged at `info` under the
//! `neovm::cconv_memo` target every 4096 trimming calls, and
//! [`Context::cconv_memo_report`] formats them.
//!
//! # Effect snapshot
//!
//! [`EffectSnapshot`] is a cheap, generic "did this Lisp run do anything
//! observable" check: loads, echo-area output, buffer creation and text
//! changes, `gensym-counter`, function-cell writes, newly interned symbols and
//! the match data.  Taken before and after a run, a difference means the run
//! had an effect.

use super::cconv_shape::{ClosureFacts, ClosureShape, EnvSummary, FactsRefusal};
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
    /// Count calls and classify every trimming run; behaviour unchanged.
    Stats = 1,
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
        other => {
            tracing::warn!(value = other, "NEOVM_CCONV_MEMO: unknown mode, using off");
            CconvMemoMode::Off
        }
    }
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

/// Everything the filter hook keeps on a [`Context`].  Holds no Lisp value.
#[derive(Debug)]
pub(crate) struct CconvMemo {
    mode: CconvMemoMode,
    stats: CconvMemoStats,
    /// Files loaded (bumped by the loader), for [`EffectSnapshot`].
    loads: u64,
    /// Echo-area / stderr messages emitted, for [`EffectSnapshot`].
    outputs: u64,
}

impl CconvMemo {
    pub(crate) fn new(mode: CconvMemoMode) -> Self {
        Self {
            mode,
            stats: CconvMemoStats::default(),
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
    pub(crate) fn stats(&self) -> &CconvMemoStats {
        &self.stats
    }

    fn note(&mut self, event: CconvMemoEvent) {
        self.stats.note(event);
        if event == CconvMemoEvent::Trim && self.stats.count(event) % 4096 == 0 {
            tracing::info!(target: "neovm::cconv_memo", "{}", self.stats.report());
        }
    }
}

cached_symbol_id!(
    cconv_make_interpreted_closure_symbol,
    "cconv-make-interpreted-closure"
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

    /// Classify a trimming call's inputs: [`CconvMemoEvent::Eligible`] or
    /// the first refusal.
    fn cconv_eligibility(
        &self,
        params: Value,
        body: Value,
        env: Value,
        iform: Value,
    ) -> CconvMemoEvent {
        if EnvSummary::of(env).is_none() {
            return CconvMemoEvent::RefuseEnv;
        }
        if !iform.is_nil() {
            return CconvMemoEvent::RefuseInteractive;
        }
        let Ok(shape) = ClosureShape::of(params, body) else {
            return CconvMemoEvent::RefuseShape;
        };
        if shape.mentions_interactive {
            return CconvMemoEvent::RefuseInteractive;
        }
        let facts = match ClosureFacts::of(params, body) {
            Ok(facts) => facts,
            Err(FactsRefusal::TooLarge) => return CconvMemoEvent::RefuseShape,
            Err(FactsRefusal::UnderscoreUse) => return CconvMemoEvent::RefuseUnderscore,
        };
        for role in facts.symbols.iter().filter(|role| role.head) {
            match self.cconv_head_verdict(role.id) {
                HeadVerdict::Plain => {}
                HeadVerdict::CompilerMacro => return CconvMemoEvent::RefuseCompilerMacroHead,
                HeadVerdict::Macro | HeadVerdict::AutoloadMacro | HeadVerdict::Unresolvable => {
                    return CconvMemoEvent::RefuseMacroHead;
                }
            }
        }
        CconvMemoEvent::Eligible
    }

    /// Whether this hook call goes through [`Self::cconv_filter_call`]: the
    /// knob is on and the filter is `cconv-make-interpreted-closure` itself.
    #[inline]
    pub(super) fn cconv_filter_call_applies(&self, closure_hook: Value) -> bool {
        self.cconv_memo.engaged()
            && closure_hook.as_symbol_id() == Some(cconv_make_interpreted_closure_symbol())
    }

    /// Run `cconv-make-interpreted-closure` on the parts `Ffunction` built,
    /// counting what happened.  The caller holds every argument as a root.
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
            return self.apply(closure_hook, vec![params, body, env, docstring, iform]);
        }
        self.cconv_memo.note(CconvMemoEvent::Trim);
        let eligibility = self.cconv_eligibility(params, body, env, iform);
        self.cconv_memo.note(eligibility);
        let before = self.cconv_effect_snapshot();
        let result = self.apply(closure_hook, vec![params, body, env, docstring, iform]);
        match &result {
            Err(_) => self.cconv_memo.note(CconvMemoEvent::RunError),
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
            }
        }
        if self.cconv_effect_snapshot() != before {
            self.cconv_memo.note(CconvMemoEvent::RunEffect);
        }
        result
    }
}
