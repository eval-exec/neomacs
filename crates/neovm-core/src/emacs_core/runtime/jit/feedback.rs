//! Per-source feedback (design `p2-1-feedback-reopt` §3.2): what the
//! interpreter and the call shims observe about one byte-code source, shared
//! by every `make-closure` instance through its
//! [`RuntimeState`](super::RuntimeState).
//!
//! [`SourceFeedback`] is the one owner of a source's observations:
//!
//! - the per-op word ([`FeedbackVec`]): the numeric lattice at arithmetic
//!   pcs (and the legacy symbol-only call lattice at call pcs);
//! - [`CallSites`] (P2.1 C3/C4, `NEOVM_JIT_FEEDBACK`): the call-target
//!   lattice of every call site whose callee the code does not name as a
//!   constant: `funcall` of a variable or of a closure, the function an
//!   `apply` spreads into, the callback a mapping builtin calls.
//!
//! Compiles read it once, through the compile-time snapshot
//! (`compile::snapshot`), never piecemeal.
//!
//! Every structure here holds only integers, `SymId`s and Rust-heap
//! `Weak`s: none holds a Lisp `Value`, so the collector never traces
//! feedback and feedback never dangles. Existing call/numeric accessors keep
//! their contracts; the optional array-kind table explicitly supports several
//! mutators joining immutable physical-kind bits with relaxed atomic ORs.
//!
//! # The call-target lattice
//!
//! One word per site, `Uninit -> Sym | Sources(1) -> Sources(n <= POLY) ->
//! Mega`; a symbol after a source (or the reverse), a second symbol, or any
//! other callee (a subr, an interpreted closure, a non-function) goes to
//! `Mega`, which is final. A source is identified by its `RuntimeState`
//! allocation: the word a byte-code object's `runtime` field holds, which is
//! exactly what a compiled source guard compares
//! (`jit_layout::BYTECODE_RUNTIME_WORD_OFFSET`). The site keeps a `Weak` of
//! every source it saw, so no source address is reused while the site
//! remembers it (no ABA).

#[path = "feedback/arrays.rs"]
pub(crate) mod arrays;

use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, Weak};

use super::{CallFeedback, FeedbackVec, NumericFeedback, Runtime, RuntimeState};
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::Value;

// ---------------------------------------------------------------------------
// The knob.
// ---------------------------------------------------------------------------

/// `NEOVM_JIT_FEEDBACK`: whether call targets are recorded, and whether
/// compiles may read them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackMode {
    /// Nothing recorded (the default: today's code, CLIF-identical).
    Off,
    /// The interpreter records the call targets of its non-constant call
    /// sites, compiled code records them through the recording call shims
    /// for a site's first [`STABLE_WINDOW`] executions (its profiling
    /// window: after it a site pays one count test), and the exit report
    /// carries the census (`[neovm-jit-final-calls]`). No compile reads
    /// them.
    Record,
    /// [`Self::Record`] with no window: compiled code records every call,
    /// so the census sees transitions after the window (F-1's stability
    /// premise). A measurement mode.
    Census,
    /// The interpreter records (as [`Self::Record`]) and compiles read the
    /// targets through their snapshot: the input of
    /// `NEOVM_JIT_SPEC_SOURCES` (which implies it). Compiled sites do not
    /// record except inside a T1 leaf's open `NEOVM_JIT_TIER2` window, where
    /// C7 compares target stability before recompiling. That leaf window
    /// also covers stable/reverted windows past the site's STABLE_WINDOW;
    /// OSR, T2 and knob-off Use sites retain their recording-free emission.
    Use,
}

impl FeedbackMode {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "off" | "0" | "no" | "false" => Some(FeedbackMode::Off),
            "record" | "on" | "1" => Some(FeedbackMode::Record),
            "census" => Some(FeedbackMode::Census),
            "use" => Some(FeedbackMode::Use),
            _ => None,
        }
    }

    /// Whether call targets are recorded.
    #[inline(always)]
    pub fn records(self) -> bool {
        !matches!(self, FeedbackMode::Off)
    }

    /// Whether compiles read the recorded targets.
    #[inline(always)]
    pub fn uses(self) -> bool {
        matches!(self, FeedbackMode::Use)
    }

    /// Whether compiled sites record independently of a T1 leaf window.
    /// Use remains false here; its TIER2-on T1 exception is selected by the
    /// lowering's actual leaf emission and stops when that leaf closes.
    #[inline(always)]
    pub fn records_compiled(self) -> bool {
        matches!(self, FeedbackMode::Record | FeedbackMode::Census)
    }

    /// Whether compiled code records only inside a site's profiling window.
    #[inline(always)]
    pub fn windowed(self) -> bool {
        !matches!(self, FeedbackMode::Census)
    }

    /// The knob's spelling.
    pub fn name(self) -> &'static str {
        match self {
            FeedbackMode::Off => "off",
            FeedbackMode::Record => "record",
            FeedbackMode::Census => "census",
            FeedbackMode::Use => "use",
        }
    }
}

#[cfg(test)]
std::thread_local! {
    static FEEDBACK_MODE_TEST_OVERRIDE: std::cell::Cell<Option<FeedbackMode>> =
        const { std::cell::Cell::new(None) };
}

/// Force the feedback mode on the current thread (tests only; `None`
/// returns to the process setting).
#[cfg(test)]
pub fn force_feedback_mode_for_test(mode: Option<FeedbackMode>) {
    FEEDBACK_MODE_TEST_OVERRIDE.with(|c| c.set(mode));
}

/// The test override of this thread, if any.
#[cfg(test)]
pub(crate) fn feedback_mode_test_override() -> Option<FeedbackMode> {
    FEEDBACK_MODE_TEST_OVERRIDE.with(std::cell::Cell::get)
}

/// The process's [`FeedbackMode`], read once: `NEOVM_JIT_FEEDBACK=off|record|
/// use`; unset, the former `NEOVM_JIT_CALL_FEEDBACK=on` means `record`, and
/// `NEOVM_JIT_SPEC_SOURCES=on` raises either to `use` (its consumer needs
/// its input).
#[inline]
pub fn feedback_mode() -> FeedbackMode {
    #[cfg(test)]
    if let Some(mode) = feedback_mode_test_override() {
        return mode;
    }
    static MODE: OnceLock<FeedbackMode> = OnceLock::new();
    *MODE.get_or_init(feedback_mode_from_env)
}

#[cold]
fn feedback_mode_from_env() -> FeedbackMode {
    let raw = std::env::var("NEOVM_JIT_FEEDBACK").ok();
    let parsed = raw.as_deref().and_then(FeedbackMode::parse);
    if let (Some(raw), None) = (&raw, parsed) {
        tracing::warn!(target: "neovm_jit", raw, "NEOVM_JIT_FEEDBACK: expected off|record|census|use; off");
    }
    #[allow(unused_mut)]
    let mut mode = parsed.unwrap_or_else(|| {
        if std::env::var("NEOVM_JIT_CALL_FEEDBACK").as_deref() == Ok("on") {
            FeedbackMode::Record
        } else {
            FeedbackMode::Off
        }
    });
    // The source-slot consumer needs its input.
    #[cfg(feature = "jit")]
    if super::compile::jit_spec_sources_on() && !mode.uses() {
        mode = FeedbackMode::Use;
    }
    if mode != FeedbackMode::Off {
        tracing::info!(target: "neovm_jit", mode = mode.name(), "NEOVM_JIT_FEEDBACK");
    }
    mode
}

// ---------------------------------------------------------------------------
// Per-source feedback.
// ---------------------------------------------------------------------------

/// One source's feedback (see the module docs).
#[derive(Debug, Default)]
pub(crate) struct SourceFeedback {
    /// Per-op word: `NumericFeedback` at arithmetic pcs (the encoding the
    /// lowering's one predicate, `arith_site_takes_generic`, reads), the
    /// legacy symbol-only `CallFeedback` at call pcs.
    ops: FeedbackVec,
    /// The call-target lattice of the source's non-constant call sites,
    /// built on the first record (only ever under `NEOVM_JIT_FEEDBACK`).
    calls: OnceLock<CallSites>,
}

impl SourceFeedback {
    /// No feedback yet (nothing allocated).
    #[inline]
    pub(crate) const fn new() -> Self {
        Self {
            ops: FeedbackVec::new(),
            calls: OnceLock::new(),
        }
    }

    /// Record callee `sym` at call site `pc` (the legacy lattice).
    #[inline]
    pub(crate) fn record_call(&self, pc: usize, ops_len: usize, sym: SymId) {
        self.ops.record_call(pc, ops_len, sym);
    }

    /// The legacy call lattice at `pc`.
    #[inline]
    pub(crate) fn call_at(&self, pc: usize) -> CallFeedback {
        self.ops.call_at(pc)
    }

    /// Join `seen` into arithmetic site `pc`; whether the slot moved.
    #[inline]
    pub(crate) fn record_numeric(&self, pc: usize, ops_len: usize, seen: NumericFeedback) -> bool {
        self.ops.record_numeric(pc, ops_len, seen)
    }

    /// The numeric lattice at arithmetic site `pc`.
    #[inline]
    pub(crate) fn numeric_at(&self, pc: usize) -> NumericFeedback {
        self.ops.numeric_at(pc)
    }

    /// The call-site table, if one was built.
    #[inline(always)]
    pub(crate) fn call_sites(&self) -> Option<&CallSites> {
        self.calls.get()
    }
}

impl RuntimeState {
    /// The source's call-site table, if one was built.
    #[inline(always)]
    pub(crate) fn call_sites(&self) -> Option<&CallSites> {
        self.feedback.call_sites()
    }
}

impl Runtime {
    /// Record the target of the call at `pc` of `func` (this runtime's
    /// function) that the interpreter is about to make: `callee` is the
    /// called value, `arg0` its first argument (nil when it has none). A
    /// no-op at a site whose callee is a constant (the compile knows it);
    /// at `apply` and mapping-builtin sites the target is `arg0`.
    ///
    /// The fast path is the table lookup and one compare; only a lattice
    /// transition leaves it.
    #[inline(always)]
    pub(crate) fn record_call_target(
        &self,
        pc: usize,
        func: &crate::emacs_core::bytecode::ByteCodeFunction,
        callee: Value,
        arg0: Value,
    ) {
        let Some(sites) = self.feedback.calls.get() else {
            return self.record_call_target_first(pc, func, callee, arg0);
        };
        if let Some(site) = sites.site_at(pc) {
            site.observe(site.target_of(callee, arg0));
        }
    }

    /// [`Self::record_call_target`] on a source without a table: build it,
    /// register the source with the census, then record.
    #[cold]
    #[inline(never)]
    fn record_call_target_first(
        &self,
        pc: usize,
        func: &crate::emacs_core::bytecode::ByteCodeFunction,
        callee: Value,
        arg0: Value,
    ) {
        let sites = self.call_sites_for(func.executable_ops(), &func.constants);
        if let Some(site) = sites.site_at(pc) {
            site.observe(site.target_of(callee, arg0));
        }
    }

    /// This source's call-site table, built from `ops` (its executable
    /// instructions) on first use and registered with the exit census.
    pub(crate) fn call_sites_for(&self, ops: &[Op], constants: &[Value]) -> &CallSites {
        let mut built = false;
        let sites = self.feedback.calls.get_or_init(|| {
            built = true;
            CallSites::build(ops, constants)
        });
        if built {
            census::register(&self.shared);
        }
        sites
    }

    /// The source's `RuntimeState` address (its identity among live
    /// sources, as `Arc::as_ptr`).
    #[inline(always)]
    pub(crate) fn state_ptr(&self) -> *const RuntimeState {
        Arc::as_ptr(&self.shared)
    }

    /// The source's `RuntimeState`, shared (a compiled leaf that bakes an
    /// address inside it keeps it alive).
    pub(crate) fn share_state(&self) -> Arc<RuntimeState> {
        Arc::clone(&self.shared)
    }
}

/// The identity word of the source `state` (what a byte-code object of it
/// holds in its `runtime` field, and what a source guard compares), read
/// through `jit_layout::runtime_identity_word`, the golden-tested reader.
#[cfg(feature = "jit")]
pub(crate) fn identity_word_of(state: &Arc<RuntimeState>) -> usize {
    super::compile::jit_layout::runtime_identity_word(&Runtime {
        shared: Arc::clone(state),
    })
}

// ---------------------------------------------------------------------------
// Call sites.
// ---------------------------------------------------------------------------

/// Most sources a site tells apart before it goes megamorphic.
pub(crate) const POLY: usize = 4;

/// Executions after which a site's lattice transition counts as LATE in the
/// census: the window a later tier would have compiled against
/// (`T2_WINDOW`, design §3.4). F-1's stability premise (R3) is that few
/// counted sites transition after it.
pub(crate) const STABLE_WINDOW: u32 = 15_000;

/// What a call site records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum SiteShape {
    /// The called value itself (the callee is not a constant: a variable,
    /// a closure, an argument).
    Callee = 0,
    /// The function `apply` spreads into: argument 0 of a call whose
    /// callee is the constant `apply`.
    ApplyArg = 1,
    /// The callback of a mapping builtin (`mapc`, `mapcar`, `mapcan`,
    /// `mapconcat`): argument 0.
    CallbackArg = 2,
}

impl SiteShape {
    /// The census name.
    pub(crate) fn name(self) -> &'static str {
        match self {
            SiteShape::Callee => "callee",
            SiteShape::ApplyArg => "apply",
            SiteShape::CallbackArg => "callback",
        }
    }
}

/// A site's recorded target, as a compile reads it (the sources upgraded;
/// a source that died drops out, and a site whose sources all died reads
/// `Mega`).
#[derive(Clone, Debug)]
pub(crate) enum CallTarget {
    /// Never executed (or recorded).
    Uninit,
    /// Always this symbol.
    Sym(SymId),
    /// Always a byte-code object of one of these sources (1..=POLY).
    Sources(Vec<Arc<RuntimeState>>),
    /// Anything else.
    Mega,
}

/// The lattice word's tag bits (see [`target_key`]).
const TAG_BITS: u64 = 0b111;
const STATE_UNINIT: u64 = 0;
const TAG_SYM: u64 = 0b001;
const TAG_SOURCE: u64 = 0b010;
const TAG_POLY: u64 = 0b011;
const STATE_MEGA: u64 = 0b100;

/// The lattice word a site holds when `target` is the only callee it saw:
/// a symbol's id, or a source's identity word (8-aligned, so the tag fits),
/// or `Mega` for anything else (a pdump stub has identity 0).
#[inline(always)]
fn target_key(target: Value) -> u64 {
    if let Some(id) = target.as_symbol_id() {
        return (u64::from(id.0) << 3) | TAG_SYM;
    }
    #[cfg(not(feature = "jit"))]
    return STATE_MEGA;
    #[cfg(feature = "jit")]
    match target.bytecode_runtime_word() {
        Some(word) if word != 0 => {
            debug_assert_eq!(word as u64 & TAG_BITS, 0, "an Arc is 8-aligned");
            word as u64 | TAG_SOURCE
        }
        _ => STATE_MEGA,
    }
}

/// One call site's feedback.
#[derive(Debug)]
pub(crate) struct CallSiteFeedback {
    /// The lattice word: [`STATE_UNINIT`], a symbol (`id << 3 | TAG_SYM`),
    /// one source (its identity word `| TAG_SOURCE`), `n` sources
    /// (`n << 3 | TAG_POLY`) or [`STATE_MEGA`].
    state: AtomicU64,
    /// The sources seen, as `Weak::into_raw` (null = none): each keeps its
    /// allocation reserved while the site lives. Written before the state
    /// that names them.
    sources: [AtomicPtr<RuntimeState>; POLY],
    /// Executions counted by compiled code's recording call shims
    /// (saturating; the interpreter counts none).
    count: AtomicU32,
    /// Lattice transitions made at or after [`STABLE_WINDOW`] counted
    /// executions.
    late: AtomicU32,
    /// The site's pc in its body.
    pc: u32,
    /// What it records.
    shape: SiteShape,
}

impl CallSiteFeedback {
    fn new(pc: u32, shape: SiteShape) -> Self {
        Self {
            state: AtomicU64::new(STATE_UNINIT),
            sources: std::array::from_fn(|_| AtomicPtr::new(std::ptr::null_mut())),
            count: AtomicU32::new(0),
            late: AtomicU32::new(0),
            pc,
            shape,
        }
    }

    /// The value this site records for a call of `callee` with first
    /// argument `arg0`.
    #[inline(always)]
    pub(crate) fn target_of(&self, callee: Value, arg0: Value) -> Value {
        match self.shape {
            SiteShape::Callee => callee,
            SiteShape::ApplyArg | SiteShape::CallbackArg => arg0,
        }
    }

    /// What the site records.
    pub(crate) fn shape(&self) -> SiteShape {
        self.shape
    }

    /// The site's pc.
    pub(crate) fn pc(&self) -> u32 {
        self.pc
    }

    /// Counted executions.
    pub(crate) fn count(&self) -> u32 {
        self.count.load(Ordering::Relaxed)
    }

    /// Late transitions.
    pub(crate) fn late(&self) -> u32 {
        self.late.load(Ordering::Relaxed)
    }

    /// Join `target` into the lattice. One load and a compare unless the
    /// word moves.
    #[inline(always)]
    pub(crate) fn observe(&self, target: Value) {
        let key = target_key(target);
        let state = self.state.load(Ordering::Relaxed);
        if state == key || state == STATE_MEGA {
            return;
        }
        self.transition(state, key, target);
    }

    /// Whether the site is still inside its profiling window (fewer than
    /// [`STABLE_WINDOW`] counted executions).
    #[inline(always)]
    pub(crate) fn window_open(&self) -> bool {
        self.count.load(Ordering::Relaxed) < STABLE_WINDOW
    }

    /// Test-only: set the counted executions outright.
    #[cfg(test)]
    pub(crate) fn set_count_for_test(&self, count: u32) {
        self.count.store(count, Ordering::Relaxed);
    }

    /// Count one execution, then [`Self::observe`] (the recording call
    /// shims').
    #[inline(always)]
    pub(crate) fn observe_counted(&self, target: Value) {
        let count = self.count.load(Ordering::Relaxed);
        self.count.store(count.saturating_add(1), Ordering::Relaxed);
        self.observe(target);
    }

    /// The word moves (or a polymorphic site meets one of its sources).
    #[inline(never)]
    fn transition(&self, state: u64, key: u64, target: Value) {
        let next = if key == STATE_MEGA {
            STATE_MEGA
        } else if state == STATE_UNINIT {
            if key & TAG_BITS == TAG_SOURCE && !self.remember_source(0, target) {
                STATE_MEGA
            } else {
                key
            }
        } else if key & TAG_BITS == TAG_SOURCE && state & TAG_BITS == TAG_SOURCE {
            if self.remember_source(1, target) {
                (2 << 3) | TAG_POLY
            } else {
                STATE_MEGA
            }
        } else if key & TAG_BITS == TAG_SOURCE && state & TAG_BITS == TAG_POLY {
            let n = (state >> 3) as usize;
            if self.holds_source(n, target) {
                return;
            }
            if n < POLY && self.remember_source(n, target) {
                (((n + 1) as u64) << 3) | TAG_POLY
            } else {
                STATE_MEGA
            }
        } else {
            // A second symbol, a symbol after a source or the reverse.
            STATE_MEGA
        };
        if next == state {
            return;
        }
        self.state.store(next, Ordering::Relaxed);
        if self.count.load(Ordering::Relaxed) >= STABLE_WINDOW {
            let late = self.late.load(Ordering::Relaxed);
            self.late.store(late.saturating_add(1), Ordering::Relaxed);
        }
    }

    /// Without the JIT a byte-code object has no source identity
    /// (`ByteCodeFunction::runtime` is JIT-only), so no key names one.
    #[cfg(not(feature = "jit"))]
    fn remember_source(&self, _i: usize, _target: Value) -> bool {
        false
    }

    /// Keep a `Weak` of `target`'s source in slot `i`; false when `target`
    /// has no source (never: the key said it has one).
    #[cfg(feature = "jit")]
    fn remember_source(&self, i: usize, target: Value) -> bool {
        let Some(bc) = target.bytecode_data_if_materialized() else {
            return false;
        };
        let Some(runtime) = bc.runtime.as_ref() else {
            return false;
        };
        let weak = Weak::into_raw(Arc::downgrade(&runtime.shared));
        let old = self.sources[i].swap(weak.cast_mut(), Ordering::Relaxed);
        if !old.is_null() {
            // SAFETY: every non-null slot holds a `Weak::into_raw`.
            drop(unsafe { Weak::from_raw(old) });
        }
        true
    }

    /// Without the JIT no site holds a source.
    #[cfg(not(feature = "jit"))]
    fn holds_source(&self, _n: usize, _target: Value) -> bool {
        false
    }

    /// Whether `target`'s source is among the first `n` the site holds.
    #[cfg(feature = "jit")]
    fn holds_source(&self, n: usize, target: Value) -> bool {
        let Some(runtime) = target
            .bytecode_data_if_materialized()
            .and_then(|bc| bc.runtime.as_ref())
        else {
            return false;
        };
        let ptr = runtime.state_ptr();
        self.sources[..n.min(POLY)]
            .iter()
            .any(|s| std::ptr::eq(s.load(Ordering::Relaxed), ptr))
    }

    /// The site's target as a compile reads it (see [`CallTarget`]).
    pub(crate) fn target(&self) -> CallTarget {
        let state = self.state.load(Ordering::Relaxed);
        let n = match state & TAG_BITS {
            _ if state == STATE_UNINIT => return CallTarget::Uninit,
            TAG_SYM => return CallTarget::Sym(SymId((state >> 3) as u32)),
            TAG_SOURCE => 1,
            TAG_POLY => (state >> 3) as usize,
            _ => return CallTarget::Mega,
        };
        let live: Vec<Arc<RuntimeState>> = self.sources[..n.min(POLY)]
            .iter()
            .filter_map(|s| {
                let ptr = s.load(Ordering::Relaxed);
                if ptr.is_null() {
                    return None;
                }
                // SAFETY: a `Weak::into_raw` this site still owns; borrowed,
                // not dropped.
                let weak = std::mem::ManuallyDrop::new(unsafe { Weak::from_raw(ptr) });
                weak.upgrade()
            })
            .collect();
        if live.is_empty() {
            CallTarget::Mega
        } else {
            CallTarget::Sources(live)
        }
    }
}

impl Drop for CallSiteFeedback {
    fn drop(&mut self) {
        for slot in &self.sources {
            let ptr = slot.load(Ordering::Relaxed);
            if !ptr.is_null() {
                // SAFETY: every non-null slot holds a `Weak::into_raw`
                // this site owns.
                drop(unsafe { Weak::from_raw(ptr) });
            }
        }
    }
}

/// A source's call sites: one [`CallSiteFeedback`] per call whose target is
/// not a constant the compile can read (see [`SiteShape`]).
#[derive(Debug)]
pub(crate) struct CallSites {
    /// Per op: 0 = no recorded site at this pc, else the site's index + 1.
    index: Box<[u16]>,
    sites: Box<[CallSiteFeedback]>,
}

impl CallSites {
    /// The table of `ops` (a body's executable instructions): classify each
    /// `Op::Call`/`Op::Apply` by the constant its callee slot provably holds
    /// (the block-local tag scan `find_spec_sites` uses).
    pub(crate) fn build(ops: &[Op], constants: &[Value]) -> Self {
        let shapes = classify_call_sites(ops, constants);
        let mut index = vec![0u16; ops.len()].into_boxed_slice();
        let mut sites = Vec::new();
        for (pc, shape) in shapes.into_iter().enumerate() {
            let Some(shape) = shape else { continue };
            if sites.len() >= usize::from(u16::MAX) - 1 {
                break;
            }
            sites.push(CallSiteFeedback::new(pc as u32, shape));
            index[pc] = sites.len() as u16;
        }
        CallSites {
            index,
            sites: sites.into_boxed_slice(),
        }
    }

    /// The site at `pc`, if the call there records.
    #[inline(always)]
    pub(crate) fn site_at(&self, pc: usize) -> Option<&CallSiteFeedback> {
        let at = *self.index.get(pc)?;
        if at == 0 {
            return None;
        }
        self.sites.get(usize::from(at) - 1)
    }

    /// Every site, in pc order.
    pub(crate) fn sites(&self) -> &[CallSiteFeedback] {
        &self.sites
    }
}

/// The symbols of the builtins whose argument 0 a site records.
fn apply_and_mapping_ids() -> &'static (SymId, [SymId; 4]) {
    static IDS: OnceLock<(SymId, [SymId; 4])> = OnceLock::new();
    IDS.get_or_init(|| {
        use crate::emacs_core::intern::intern;
        (
            intern("apply"),
            ["mapc", "mapcar", "mapcan", "mapconcat"].map(intern),
        )
    })
}

/// Per pc of `ops`: the shape of the call there, `None` for a non-call op
/// or a call whose callee is a constant (neither `apply` nor a mapping
/// builtin). A body with a jump table keeps every call (its targets are
/// not modeled here, and recording a constant site only costs a word).
fn classify_call_sites(ops: &[Op], constants: &[Value]) -> Vec<Option<SiteShape>> {
    let mut shapes = vec![None; ops.len()];
    let has_switch = ops.iter().any(|op| matches!(op, Op::Switch));
    let mut leaders = vec![false; ops.len() + 1];
    for (pc, op) in ops.iter().enumerate() {
        match op {
            Op::Goto(t)
            | Op::GotoIfNil(t)
            | Op::GotoIfNotNil(t)
            | Op::GotoIfNilElsePop(t)
            | Op::GotoIfNotNilElsePop(t)
            | Op::PushConditionCase(t)
            | Op::PushConditionCaseRaw(t)
            | Op::PushCatch(t) => {
                if let Some(l) = leaders.get_mut(*t as usize) {
                    *l = true;
                }
                leaders[pc + 1] = true;
            }
            Op::Return | Op::Throw | Op::Switch => leaders[pc + 1] = true,
            _ => {}
        }
    }
    let (apply, mapping) = apply_and_mapping_ids();
    let mut tags: Vec<Option<u16>> = Vec::new();
    for (pc, op) in ops.iter().enumerate() {
        if leaders[pc] {
            tags.clear();
        }
        if let Op::Call(n) | Op::Apply(n) = op {
            let nargs = *n as usize;
            let constant = if has_switch || matches!(op, Op::Apply(_)) {
                None
            } else {
                tags.len()
                    .checked_sub(1 + nargs)
                    .and_then(|at| tags[at])
                    .and_then(|cidx| constants.get(usize::from(cidx)))
            };
            shapes[pc] = match constant {
                None => Some(SiteShape::Callee),
                Some(c) => match c.as_symbol_id() {
                    Some(id) if id == *apply && nargs >= 1 => Some(SiteShape::ApplyArg),
                    Some(id) if mapping.contains(&id) && nargs >= 1 => Some(SiteShape::CallbackArg),
                    // Any other constant callee: the compile reads it.
                    _ => None,
                },
            };
        }
        #[cfg(feature = "jit")]
        super::compile::spec_tag_transfer(op, constants, &mut tags);
        #[cfg(not(feature = "jit"))]
        tags.clear();
    }
    shapes
}

// ---------------------------------------------------------------------------
// The exit census (`[neovm-jit-final-calls]`).
// ---------------------------------------------------------------------------

/// The sources that built a call-site table, for the exit census.
pub(crate) mod census {
    use super::*;

    fn registry() -> &'static std::sync::Mutex<Vec<Weak<RuntimeState>>> {
        static SOURCES: OnceLock<std::sync::Mutex<Vec<Weak<RuntimeState>>>> = OnceLock::new();
        SOURCES.get_or_init(Default::default)
    }

    /// Remember a source that built its table (once per source).
    pub(crate) fn register(state: &Arc<RuntimeState>) {
        registry()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(Arc::downgrade(state));
    }

    /// Every registered source still alive.
    pub(crate) fn live_sources() -> Vec<Arc<RuntimeState>> {
        registry()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .iter()
            .filter_map(Weak::upgrade)
            .collect()
    }
}

#[cfg(test)]
#[path = "feedback/tests/feedback_test.rs"]
mod tests;
