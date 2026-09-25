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
//! This module observes those calls without changing them.
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
