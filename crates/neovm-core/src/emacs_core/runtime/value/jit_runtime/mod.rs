//! Per-function JIT runtime data, placed in the value layer (T1-pre).
//!
//! These are the layout-identical data types the bytecode storage carries: the
//! per-source [`Runtime`] handle, its [`RuntimeState`] counters and feedback
//! cells, and the feedback/retreat tables they own. All JIT implementation,
//! compilation, cache and dispatch policy stays upper. This module names no
//! Cranelift type and no evaluator `Context`, and stores no Lisp `Value`; the
//! only value-layer references are the G0 `intern`/`value` vocabulary.
//!
//! T1-pre moved the definitions and the layout-identical data methods; the
//! JIT policy impls (dispatch, cache, compile, recording) stay in
//! `emacs_core::jit`, so the generated code of the gated functions is
//! unchanged. Fields are crate-visible because those upper impl blocks still
//! touch them; the extraction API is the peer-reviewed privacy ledger, not
//! these interim in-crate visibilities.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64};

use crate::emacs_core::intern::SymId;

pub(crate) mod feedback;
pub(crate) mod retreat;

/// Type/target feedback observed at one CALL site (the JIT's most important
/// speculation input — it enables direct-call inlining).
///
/// Holds a [`SymId`], NOT a function `Value`: a `SymId` is a stable runtime
/// index, never a heap pointer, so feedback is **GC-safe** — the collector never
/// has to trace it, and it never dangles. The optimizing tier turns
/// `Monomorphic(sym)` into a direct/inlined call guarded by a dependency on that
/// symbol's function cell (Phase 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallFeedback {
    /// This site has not executed yet.
    Uninit,
    /// Every observed call so far went to the same named function.
    Monomorphic(SymId),
    /// Conflicting / non-symbol callees seen — no useful speculation.
    Megamorphic,
}

/// Operand types observed at one ARITHMETIC site — the input the lowering
/// needs to stop emitting a fixnum guard for code that is never fixnum.
///
/// `lowering::stack_as_raw` guards fixnum and branches to a deopt block
/// otherwise, so a float or bignum operand bails the whole compiled body.
/// `nbody` gets **3.6%** from the JIT and `pidigits` gets **-6.5%** — the
/// compiler needs to know which sites are worth lowering that way.
///
/// The default (`FixnumOnly`) is the zero slot, and it means exactly what the
/// lowering already assumes, so a site that never runs or only ever sees
/// fixnums needs no recording at all: the arithmetic opcodes record on their
/// SLOW arm only, which they already branch to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericFeedback {
    /// Never seen a non-fixnum operand pair (or never executed).
    FixnumOnly,
    /// Every non-fixnum pair so far was floats (fixnums alongside are fine —
    /// they promote). Lowerable as `f64`.
    Float,
    /// A bignum, marker or non-number operand: nothing an unboxed lowering
    /// can take. (A fixnum overflow or zero divisor records nothing.)
    Other,
}

/// A per-function feedback vector — one slot per bytecode instruction, lazily
/// allocated on first use (when the instruction count is known). Slots for
/// non-call instructions stay [`CallFeedback::Uninit`]. Lock-free
/// (`AtomicU64`), `Send + Sync` — sound to hold inline on a GC-managed function
/// alongside the concurrent collector (the mutator is the only writer).
#[derive(Debug, Default)]
pub struct FeedbackVec {
    pub(crate) slots: OnceLock<Box<[AtomicU64]>>,
}

/// Per-SOURCE runtime tiering + profiling state, shared by every
/// `ByteCodeFunction` instance that `make-closure` derives from one prototype
/// (see [`Runtime`], the handle). NOT part of the dumped representation
/// (`DumpByteCodeFunction`) — pure runtime state, started cold each session.
/// Relaxed atomics: the mutator is the only writer today, and being `Sync`
/// keeps the heap object sound alongside the concurrent collector.
#[derive(Debug)]
pub struct RuntimeState {
    /// Coarse invocation hotness (saturating at `u32::MAX`). The feedback that
    /// later phases use to decide when to tier a function up.
    pub(crate) heat: AtomicU32,
    /// The `cache::rejection_epoch()` at which the JIT rejected this body as
    /// `NotCompilable` (0 = never). While it equals the live epoch the
    /// dispatcher answers `Interpret` outright: the cache would answer
    /// `NotCompilable` and fall back to the interpreter anyway, so the seam
    /// trip it saves (argument copy, root save/restore, probe) is pure waste.
    /// `cache::clear` bumps the epoch (the cache would retry, so does this);
    /// a grown `make-closure` prefix resets it with its eviction. Without the
    /// `jit` feature there is no cache to reject anything, so it is only ever
    /// written.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) native_rejected_epoch: AtomicU64,
    /// Heat at which a DEFERRED body is compiled (0 = not deferred): one the
    /// profitability gate refused (`NotProfitable`), or one whose leaf a
    /// deopt invalidated (`jit::reopt`: the end of its re-profile window).
    /// Either way the body already proved hot, so once the deferral expires
    /// `dispatch_sized` answers `Compiled` at once. For the profit gate: a
    /// call-heavy body RUNS faster native but its compile is dear (org editing probe, 2026-09-05: ~38M
    /// instructions per admitted body vs ~830 saved per native entry), so it
    /// pays off only past ~18k calls: the gate was +6.4% on a 5-pass session
    /// and −8.8% on a 50-pass one. Deferring to `profit_defer_factor() ×
    /// hot_threshold()` lets long sessions win without taxing short ones.
    /// The dispatcher answers `Interpret` without a cache probe until then.
    pub(crate) profit_deferred_heat: AtomicU32,
    /// Everything the interpreter observed about this source
    /// ([`feedback::SourceFeedback`]); compiles read it through their
    /// snapshot.
    pub(crate) feedback: feedback::SourceFeedback,
    /// Process-unique identity assigned on first JIT compilation attempt (0 =
    /// unassigned). Keys this function's entry in the per-thread compiled-code
    /// cache (`cache`). Monotonic and never reused, so a freed function's
    /// stale cache entry can never be mis-looked-up after the (non-moving) GC
    /// reuses its address — a new function gets a new id. Reset to 0 on clone.
    ///
    /// Authority MINT (T1 privacy ledger A2): the dispatcher selects executable
    /// code by this id without a stored source-identity check in the cache
    /// entry, so this layer stores an OPAQUE word and never mints or validates
    /// it. The only writer is [`set_compiled_id_word`] (unsafe; the upper layer
    /// vouches that the word is its fresh mint for this source, attached to
    /// exactly one function source).
    compiled_id: AtomicU64,
    /// Set by the AOT preload prepopulate (R2-C3) when this function's leaf was
    /// inserted into the compiled cache at startup: `dispatch` then serves the
    /// prewarmed native leaf FROM CALL 1 instead of interpreting until the heat
    /// threshold — the piece that lets the AOT preload cover ONE-SHOT startup
    /// elisp, which never gets hot. One relaxed load on the dispatch path,
    /// never set in the default (AOT-off) configuration.
    pub(crate) aot_prewarmed: std::sync::atomic::AtomicBool,
    /// Set once this body has been compiled and its numeric feedback read.
    ///
    /// Feedback is an input to COMPILATION; once it has been consumed every
    /// further record is dead weight. That is not a micro-optimization on a
    /// body whose arithmetic is all non-fixnum: there the opcodes' "slow" arm
    /// is the ONLY arm, so `pidigits` was paying the recording on every
    /// arithmetic operation it performs — 1.4% of the row. Gating on `heat`
    /// instead does NOT work: a body entered through the armed or speculated
    /// paths barely bumps it (`pidigits` sat at heat 1332 after 333 calls).
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) numeric_feedback_consumed: std::sync::atomic::AtomicBool,
    /// Slice C1 of the JIT call seam: the compiled leaf the interpreter's
    /// `Bcall` arm enters directly, as a raw `*const CompiledLeaf`. Zero = empty.
    /// Dereferencing requires the arming mutator's TLS cache to remain live,
    /// and `leaf_slot_epoch` to equal `cache::leaf_slot_epoch()`.
    ///
    /// P7.11's shared-source/local-activation split must replace this slot
    /// before several mutators enter the same source concurrently: the epoch
    /// supplies neither cache ownership nor an atomic pointer/epoch pair.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    leaf_slot: AtomicU64,
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    leaf_slot_epoch: AtomicU64,
    /// Widest `make-closure` patch seen for this source: the number of leading
    /// constant slots that hold PER-INSTANCE captured values (the prototype
    /// carries placeholder symbols `V0..Vn` there — `byte-compile-make-closure`).
    /// A shared native leaf must never bake, speculate on, or symbol-tag those
    /// slots; it loads them through the executing callee's constant vector at
    /// run time (`compile.rs` "dynamic prefix"). Monotone; recorded by
    /// `builtin_make_closure`, which also evicts any leaf compiled under a
    /// narrower prefix. GNU keeps no such record because GNU byte-code objects
    /// carry no JIT state at all (native-comp attaches to the subr); this port
    /// hung tiering state on the object `make-closure` copies, so the patch
    /// width must be visible to the code that shares that state.
    pub(crate) patched_prefix: AtomicU32,
    /// Deopt-driven invalidations of this source (saturating): the backoff
    /// input of `jit::reopt`.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) reopt_count: std::sync::atomic::AtomicU8,
    /// Counted T2 invalidations, saturating and independent of P0.4's ladder.
    /// Sources may be shared by mutators; Relaxed RMWs preserve this monotone
    /// observational ban without publishing leaf pointers or Lisp state.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) t2_reopts: std::sync::atomic::AtomicU8,
    /// Permanent AOT exclusion after this source requested a JIT re-tier.
    /// Shared by mutators: monotone Relaxed stores/loads publish no code
    /// pointer or Lisp state; each mutator owns its upgrade/cache.
    /// Independent of heat, whose observational updates can race.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) aot_retiered: std::sync::atomic::AtomicBool,
    /// `ReoptLevel` as `u8`: the ceiling every later compile of this source
    /// respects. Monotone.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) reopt_level: std::sync::atomic::AtomicU8,
    /// What deopts taught this source about each pc, allocated on the first
    /// mark: among others, the `Op::Call` pcs that must not be spliced,
    /// MIR-inlined or intrinsified ([`retreat::RetreatBit::NoInline`]).
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) site_retreat: retreat::SiteRetreatTable,
    /// `HofCallerBit` as `u8`, computed once on the first callback credit:
    /// whether this source has a `mapc`/`mapcar` site the default front's
    /// HOF-only path would select, so callback work may count as its heat.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) hof_caller: std::sync::atomic::AtomicU8,
    /// Test-only: pin this function to the Tier-0 interpreter regardless of
    /// hotness (the benchmark harness measures native vs interpreter in ONE
    /// process — a hot copy and a forced-cold copy — to cancel the
    /// cross-process CPU-frequency variance that wrecks a two-process A/B).
    /// Absent from the production library (only the test binary carries it).
    #[cfg(test)]
    pub(crate) force_interpret: std::sync::atomic::AtomicBool,
}

/// The per-function handle to [`RuntimeState`], living inline on
/// `ByteCodeFunction` (only when the `jit` feature is on). One pointer; derefs
/// to the shared state.
///
/// SHARED ACROSS `make-closure` INSTANCES (cite-and-overturn of the earlier
/// "a cloned function starts cold — profiling is per-instance" rule, which
/// `ByteCodeFunction::clone` enforced by resetting this to `Runtime::new()`):
/// `make-closure` clones the prototype for EVERY closure instantiation, so
/// per-instance heat meant closure-shaped code — font-lock keyword lambdas,
/// jit-lock, hooks, i.e. interactive editing — never accumulated heat and never
/// tiered, while a threshold-1 soak compiled 11.6K distinct instances of ~500
/// sources. The clone IS faithful to GNU (`Fmake_closure` memcpys the whole
/// prototype vector too); the divergence was hanging mutable tiering state on
/// the object being copied. Sharing the state by SOURCE (the same identity
/// `source_id` already preserves through `make-closure`) is the GNU-shaped
/// fix: heat, feedback, compiled id AND the patched-prefix record all ride the
/// same handle, so heat and compiled artifact are shared TOGETHER — sharing
/// heat alone would make every instance tier at once and compile its own copy
/// (the `NEOVM_JIT_GATE_RELAX` 21%-slower byte-compile precedent).
#[derive(Debug)]
pub struct Runtime {
    pub(crate) shared: std::sync::Arc<RuntimeState>,
}

impl RuntimeState {
    /// A fresh source state: every counter cold, no feedback, no retreat, no
    /// armed leaf. Constructing the owned cells directly keeps this a `const
    /// fn` and keeps the lower layer free of upper constructor calls.
    #[inline]
    pub const fn new() -> Self {
        Self {
            heat: AtomicU32::new(0),
            native_rejected_epoch: AtomicU64::new(0),
            profit_deferred_heat: AtomicU32::new(0),
            feedback: feedback::SourceFeedback {
                ops: FeedbackVec {
                    slots: OnceLock::new(),
                },
                calls: OnceLock::new(),
            },
            compiled_id: AtomicU64::new(0),
            aot_prewarmed: std::sync::atomic::AtomicBool::new(false),
            numeric_feedback_consumed: std::sync::atomic::AtomicBool::new(false),
            leaf_slot: AtomicU64::new(0),
            leaf_slot_epoch: AtomicU64::new(0),
            patched_prefix: AtomicU32::new(0),
            reopt_count: std::sync::atomic::AtomicU8::new(0),
            t2_reopts: std::sync::atomic::AtomicU8::new(0),
            aot_retiered: std::sync::atomic::AtomicBool::new(false),
            reopt_level: std::sync::atomic::AtomicU8::new(0),
            site_retreat: retreat::SiteRetreatTable(OnceLock::new()),
            // HofCallerBit::Unknown (0): the cached HOF classification has
            // not been computed yet.
            hof_caller: std::sync::atomic::AtomicU8::new(0),
            #[cfg(test)]
            force_interpret: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// This state's compiled-cache id word (0 = unassigned). The word is
    /// opaque here: the upper layer minted it and owns its meaning; this layer
    /// only stores and returns it.
    #[inline(always)]
    pub(crate) fn compiled_id_word(&self) -> u64 {
        self.compiled_id.load(std::sync::atomic::Ordering::Acquire)
    }

    /// The armed leaf word when `epoch` is the arm epoch. The word is opaque
    /// here: an upper source encoded a live `CompiledLeaf` address; this layer
    /// never dereferences it and never names the compiler type.
    #[cfg(feature = "jit")]
    #[inline]
    pub(crate) fn armed_leaf_word(&self, epoch: u64) -> Option<std::num::NonZeroU64> {
        let word = self.leaf_slot.load(std::sync::atomic::Ordering::Relaxed);
        if word == 0
            || self
                .leaf_slot_epoch
                .load(std::sync::atomic::Ordering::Relaxed)
                != epoch
        {
            return None;
        }
        std::num::NonZeroU64::new(word)
    }

    /// Empty the direct-entry slot. Safe: clearing removes an executable
    /// target and can never install one; the reader also requires the epoch.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) fn disarm_leaf_slot(&self) {
        self.leaf_slot
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Arm the direct-entry slot with an opaque executable-target word.
///
/// # Safety
/// `word` must be the address of a live upper `CompiledLeaf` that stays
/// allocated as long as this arm can be probed (the cache retires leaves
/// without freeing them), armed and probed on the SAME mutator (P7.11
/// confinement: the cache owner and its Cell-based deopt storage are
/// thread-confined), and `epoch` must be the live `cache::leaf_slot_epoch()`
/// at the arm. The epoch is stored before the word, both `Relaxed`; the
/// reader accepts the word only under the same epoch.
#[cfg(feature = "jit")]
#[inline(always)]
pub unsafe fn set_leaf_slot_word(rt: &RuntimeState, word: u64, epoch: u64) {
    rt.leaf_slot_epoch
        .store(epoch, std::sync::atomic::Ordering::Relaxed);
    rt.leaf_slot
        .store(word, std::sync::atomic::Ordering::Relaxed);
}

/// Install a fresh compiled-cache identity word, returning the id that won.
///
/// # Safety
/// `fresh` must be the upper layer's fresh mint for this source
/// (`NEXT_COMPILED_ID::fetch_add(1, Relaxed) + 1`; never 0, never
/// caller-supplied, never reused), and `rt` must be attached to exactly ONE
/// function source (the A2/A3 attach seal: `ByteCodeFunction::runtime` is
/// crate-private and is only constructed for its own source, then shared by
/// that source's clones). This layer neither mints nor validates the word:
/// storing a caller-chosen word would let one function's cache entry select
/// another source's executable code. The orderings are part of the contract
/// (`compare_exchange` is `AcqRel`/`Acquire`, as the identity publication has
/// always been); a 0 word stays reserved for "unassigned".
#[inline(always)]
pub unsafe fn set_compiled_id_word(rt: &RuntimeState, fresh: u64) -> u64 {
    match rt.compiled_id.compare_exchange(
        0,
        fresh,
        std::sync::atomic::Ordering::AcqRel,
        std::sync::atomic::Ordering::Acquire,
    ) {
        Ok(_) => fresh,
        // Another thread won the race; adopt its id, discard ours.
        Err(actual) => actual,
    }
}
