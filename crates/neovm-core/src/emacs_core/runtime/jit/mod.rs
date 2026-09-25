//! Tiered execution subsystem for the Emacs-Lisp VM — the foundation of the
//! modern JIT path. See `bytecode/ELISP_VM_MODERNIZATION.md` for the full
//! design + phased roadmap.
//!
//! Gated behind the `jit` cargo feature, which is **default-ON** (`default =
//! ["jit"]`): both the baseline (Tier-1) Cranelift JIT and the optimizing
//! typed-MIR Tier-2 above it are qualified and shipping. The bytecode
//! interpreter (`bytecode::Vm`) is always the **Tier 0** engine — the
//! correctness oracle that mirrors GNU Emacs 31.0.90 and the deoptimization
//! landing pad. It is never removed.
//!
//! Design rule (carried over from the GC work): every dispatch over an
//! execution tier is an **exhaustive `match`** with no catch-all arm, so adding
//! a tier fails to compile until every site handles it. That is the same
//! compiler-enforced completeness that caught the GC `trace_veclike`
//! use-after-free (an incomplete duplicate with a `_ => {}` arm).
//!
//! # Environment-knob inventory (the authoritative list)
//!
//! Every runtime toggle this subsystem reads, with default and status. Grep
//! anchor: `env::var("NEOVM_`. Keep this table in sync when adding a knob, and
//! give every OPT-IN knob a graduation plan — soak → default-on, or delete —
//! so the surface doesn't accumulate permanently-dead branches.
//!
//! ## Runtime switches (shipping, default-on)
//! | Knob | Default | Meaning |
//! |---|---|---|
//! | `NEOVM_JIT` | on | Kill switch: `0`/`off`/`false`/`no` forces the pure interpreter (the A/B baseline). |
//! | `NEOVM_JIT_THRESHOLD` | 1000 | Tier-up heat threshold ([`Runtime::HOT_THRESHOLD`]); `=1` compiles every compilable function — the differential soak and the strictest oracle configuration. |
//! | `NEOVM_JIT_LOOP_HEAT` | 8 | Heat credited per 256-iteration back-edge wrap (32 iterations ≈ one call; a hot loop tiers up near 32k iterations); `=0` disables loop heat — the pre-loop-heat baseline. |
//! | `NEOVM_JIT_LEVER1` | on | Residual-rooting non-heap skip; `=off` reverts to an unconditional gc_push per residual (single-build A/B). |
//! | `NEOVM_JIT_OSR` | on | Mid-loop interpreter→native transfer (on-stack replacement); `=off` disables. |
//! | `NEOVM_JIT_PROFIT` | on | Profitability gate (calls ≤ arith); `=off` also compiles call-heavy bodies. |
//! | `NEOVM_JIT_INLINE` | on | Bytecode fuser: splice a constant-bytecode callee into its caller before lowering (`jit/inline.rs`); `=off` disables. |
//! | `NEOVM_JIT_MIR_OPAQUE` | on | The MIR tier lowers shim-using ops (variable ops, builtins, `eq`, list ops) through the baseline's emitters; `=0`/`off` makes every such op bail the body to the baseline — an A/B of the adapter alone: the tier gate (`gate:loop-opaque`/`generic-call`/`inline-opaque`) applies either way, so it is not the pre-adapter gate. |
//!
//! ## Opt-in features (default-OFF, pending a graduation decision)
//! | Knob | Enable | Meaning / graduation blocker |
//! |---|---|---|
//! | `NEOVM_JIT_INLINE_ARITH` | `=on` | Level-B native bit-ops (logand/logior/logxor/lognot) with fixnum-guard deopt. Blocker: skips the compiler-macro bounce; a mixed-type loop falls back to the interpreter ungracefully. |
//! | `NEOVM_JIT_INLINE_TYPE_OF` | on | Answer a record's `type-of`/`cl-type-of` inline at an armed JIT site; `=off` calls `neovm_jit_pred_spec` everywhere (single-build A/B). |
//! | `NEOVM_JIT_INLINE_AREF` | on | Inline slot reads at JIT `aref` sites on plain vectors and records; `=off` calls `neovm_jit_aref` everywhere (single-build A/B). |
//! | `NEOVM_JIT_FLONUM` | `off` | Unboxed float results at `Float`-feedback arithmetic sites (`compile::FlonumMode`): `off` boxes every result at its site (the prior lowering, CLIF-identical); `local` keeps a result unboxed while float arithmetic/compares, stack shuffles and variable reads consume it; `resident` also keeps it across audited ops (calls, `aref`, `aset`, ...) that box only their own operands. Single-build A/B of all three. |
//! | `NEOVM_JIT_GATE_RELAX` | `=on` | Relax the calls ≤ arith profit gate. Default-on was tried and REVERTED (regressed byte-compile 21%) — measure byte-compile before ever re-flipping. |
//!
//! ## Measurement / bisection
//! | Knob | Meaning |
//! |---|---|
//! | `NEOVM_JIT_MAX_ID` | Compile only functions with id ≤ N (ids assigned in first-hot order) — clean prefix bisection of a misbehaving workload. |
//! | `NEOVM_JIT_DEBUG_ID` | Dump the bytecode body of the one compiled function with this id. |
//! | `NEOVM_JIT_PROFILE` | Append per-function workload-characterization records to this file path: one CSV row per compile attempt (its last three columns are `compiled_id,tier,name`), and at exit one `#leaf,compiled_id,name,tier,osr_pc,entries,deopt_at,deopt_rerun,signals,top_deopt_pc` row per compiled leaf, joinable on `compiled_id`. Also turns on per-leaf entry counting and per-function names (see `NEOVM_JIT_COMPILE_STATS`). |
//! | `NEOVM_JIT_COMPILE_STATS` | `=1`: print a running compile-stall summary line every 64 compiles (and every 50,000 dispatch consultations), plus a final `[neovm-jit-final*]` report when the command loop returns (`kill-emacs`, end of `--batch`, batch-error exit 255; not on death by signal). The final report separates the session from startup (`since_command_loop`), counts `function_epoch` bumps by reason, and prints per-leaf deopt/signal/entry counts. Under this knob (or `NEOVM_JIT_STATS_FILE`/`NEOVM_JIT_PROFILE`) every JIT leaf compiled from then on also carries a native-entry counter in its prologue (a load, an add and a store: expect call-heavy code to run slightly slower, so do not time runs with a report knob on) and a per-function entry name. With every report knob off the generated code has no counter. |
//! | `NEOVM_JIT_STATS_FILE` | `=<path>`: append every `[neovm-jit-*]` report line to this file instead of stderr (an unopenable path falls back to stderr). Setting it implies `NEOVM_JIT_COMPILE_STATS=1`. |
//! | `NEOVM_JIT_SIZE_UNIT` | Override [`RuntimeState::SIZE_UNIT`] (512): the ops-per-unit divisor scaling the tier-up threshold by body size. |
//! | `NEOVM_JIT_MAX_OPS` | Override [`RuntimeState::MAX_TIER_OPS`] (4096): largest body that tiers at all; `0` = uncapped (the mid-end campaign's acceptance configuration). |
//! | `NEOVM_JIT_REGALLOC` | Force one Cranelift register allocator for every JIT compile: `backtracking` (regalloc2 ion) or `single_pass` (fastalloc). Unset = the policy in `lowering::choose_regalloc` (fast for straight-line bodies, full for loops/OSR, re-tier when hot). |
//! | `NEOVM_JIT_PROFIT_DEFER` | Override [`RuntimeState::PROFIT_DEFER_FACTOR`] (4): a body the profitability gate refuses tiers up anyway at `factor × hot_threshold()` calls (`0` = never, the former veto). |
//! | `NEOVM_JIT_RETIER_FACTOR` | Override [`RuntimeState::RETIER_FACTOR`] (16): a fast-allocator leaf is rebuilt with the full allocator at `factor × hot_threshold()` heat; `0` = never. |
//! | `NEOVM_JIT_REGALLOC_CHECKER=1` | Run regalloc2's checker after every allocation (verification harness for the allocator choice). |
//! | `NEOVM_JIT_DUMP_CLIF` | `=<path>`: append every lowered function's CLIF to this file, each under a `;; baseline\|mir ... entry=<declared name>` header (the IR-composition census). Turns per-function entry names on, so `entry=` names the Lisp function. |
//! | `NEOVM_JIT_DUMP_ASM` | `=<path>`: append the final machine code of every JIT leaf (baseline, MIR, OSR) to this file: a `;; ==== <label> name= id= tier= addr= size= regalloc= clif_insts=` header, Cranelift's post-register-allocation disassembly (physical registers, spill/reload moves; `Context::set_disasm`, requested only under this knob), and the finalized code bytes in hex (`xxd -r -p \| objdump -D -b binary -mi386:x86-64 --adjust-vma=<addr>` recovers exact offsets for a perf `sym+offset`). Turns per-function entry names on. Compile-time only; unset = no disassembly work at all. |
//! | `PERF_BUILDID_DIR` | Not ours: `perf record -- cmd` sets it, and cranelift-jit then appends every defined function to `/tmp/perf-<pid>.map`. It also turns on per-function entry names: each JIT leaf is declared as `lisp:<fn>#<id>:baseline\|mir\|osr@<pc>` instead of the shared `__neovm_jit_leaf`/`__neovm_mir_leaf`, so perf attributes JIT samples per Lisp function (`<fn>` is the called symbol, else `anon[<first symbol constants>]`). For `perf record -p` on a running editor, start neomacs with `PERF_BUILDID_DIR=/tmp`. Names are also on under `NEOVM_JIT_DUMP_CLIF` and any report knob. Only the declared name changes; the code is identical. |
//!
//! ## Verification harnesses (force the cold path everywhere; run the suite with each ON)
//! | Knob | Forces |
//! |---|---|
//! | `NEOVM_JIT_FORCE_DEOPT=1` | Every speculation guard fails → every deopt path executes. |
//! | `NEOVM_JIT_FORCE_SLOW_SPEC=1` | Every spec-call shim takes its stale-epoch re-validate branch on every call. |
//! | `NEOVM_JIT_FORCE_CBSYM_GENERIC=1` | Every CallBuiltinSym intrinsic bounces to its generic fallback. |
//!
//! ## AOT (`jit/aot.rs`)
//! | Knob | Meaning |
//! |---|---|
//! | `NEOVM_AOT` | `1`/`on`/`force` enables the AOT preload; `force` additionally warns when no usable preload loaded. |
//! | `NEOVM_AOT_PGO` | `1`/`on`/`force` enables PGO collection for the AOT function set. |

#![cfg_attr(not(feature = "jit"), allow(dead_code))]

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::emacs_core::intern::SymId;

/// Cranelift codegen backend (Phase 3+). Only compiled with the `jit` feature,
/// since it links Cranelift. Today it exposes a self-contained smoke path that
/// proves the codegen toolchain works inside neovm-core's own build before any
/// bytecode is lowered onto it — the same "prove the tool, then build on it"
/// discipline used to validate TSan before trusting the concurrent GC.
#[cfg(feature = "jit")]
pub mod backend;

/// Baseline bytecode → native lowering (Phase 3b+). Compiles the leaf,
/// straight-line opcode subset to machine code and bails to the interpreter on
/// anything else. Only built with the `jit` feature. See `jit/compile.rs`.
#[cfg(feature = "jit")]
pub mod compile;

/// Per-thread compiled-code cache + the tier-up entry point the dispatch seam
/// calls ([`cache::try_run_compiled`]). Only built with the `jit` feature.
#[cfg(feature = "jit")]
pub mod cache;

/// Bytecode-level inlining (see the module docs).
#[cfg(feature = "jit")]
pub mod inline;

/// MIR: typed SSA IR for the optimizing Tier-2 (above the baseline `compile`).
/// Live: `compile::compile_bytecode_function_inner` builds the MIR for pure
/// required-only bodies, runs the pure inliner + type/unboxing/guard-elision and
/// cons-escape passes, and lowers it via `compile::lower_mir_pure`, falling back
/// to the baseline tier otherwise. Only built with the `jit` feature. See
/// `jit/mir.rs`.
#[cfg(feature = "jit")]
pub mod mir;

/// AOT (ahead-of-time) object emission (Phase R1c): emit the same CLIF the JIT
/// does, but through Cranelift's `ObjectModule`, producing a relocatable `.o`
/// that is linked to a `.so`, `dlopen`'d, and inserted as a pre-warmed
/// `CompiledLeaf`. Only built with the `jit` feature. See `jit/aot.rs`.
#[cfg(feature = "jit")]
pub mod aot;

/// Always-on metering of the synchronous compile stalls the cache-miss path
/// pays on the eval thread — the evidence base for background compilation.
/// Only built with the `jit` feature. See `jit/stats.rs`.
#[cfg(feature = "jit")]
pub mod stats;

#[cfg(feature = "jit")]
pub use cache::{note_seam_interp_fallback, try_run_compiled};

/// Which execution tier currently backs a compiled function.
///
/// This enum models only the interpreter tier; the compiled tiers — the
/// baseline Cranelift JIT and the optimizing typed-MIR Tier-2 — live in
/// `compile.rs` and are selected there, reached via [`Plan::Compiled`]. Do NOT
/// add a catch-all when matching on this — let the compiler enforce that each
/// new tier is handled everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tier {
    /// Tier 0 — interpret the function's bytecode `ops` via `bytecode::Vm`.
    #[default]
    Bytecode,
}

/// The action the dispatcher takes for one invocation of a compiled function.
/// Exhaustive by design (mirrors [`Tier`]).
#[derive(Debug)]
pub enum Plan {
    /// Run the Tier-0 bytecode interpreter.
    Interpret,
    /// The function is hot — consult the JIT (the baseline tier or the
    /// optimizing typed-MIR Tier-2, selected in `compile.rs`): compile-on-first-
    /// use and run native, or fall back to the interpreter on a deopt /
    /// non-compilable body. See [`cache::try_run_compiled`].
    Compiled,
}

// ---------------------------------------------------------------------------
// Phase 1 — feedback. The runtime-observed information later tiers speculate on.
// ---------------------------------------------------------------------------

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

impl CallFeedback {
    /// Pack into one `u64` for lock-free atomic storage. Low 2 bits tag the
    /// variant; a `SymId`'s `u32` rides in the upper bits.
    #[inline]
    const fn pack(self) -> u64 {
        match self {
            CallFeedback::Uninit => 0b00,
            CallFeedback::Monomorphic(SymId(n)) => ((n as u64) << 2) | 0b01,
            CallFeedback::Megamorphic => 0b10,
        }
    }

    #[inline]
    fn unpack(bits: u64) -> Self {
        match bits & 0b11 {
            0b00 => CallFeedback::Uninit,
            0b01 => CallFeedback::Monomorphic(SymId((bits >> 2) as u32)),
            0b10 => CallFeedback::Megamorphic,
            // The mask yields only 0..=3 and 0b11 is a reserved (unused) tag;
            // treat it as the safe over-approximation rather than panicking.
            _ => CallFeedback::Megamorphic,
        }
    }
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

impl NumericFeedback {
    /// Packed into the tag `CallFeedback` leaves reserved. A bytecode
    /// instruction is either a call or an arithmetic op, never both, so the
    /// two lattices never share a slot — and the encodings are chosen so that
    /// reading one as the other still yields the SAFE answer either way
    /// (`Megamorphic` / `Other`), rather than relying on that disjointness.
    #[inline]
    const fn pack(self) -> u64 {
        match self {
            NumericFeedback::FixnumOnly => 0b00,
            NumericFeedback::Float => 0b0111,
            NumericFeedback::Other => 0b1011,
        }
    }

    #[inline]
    fn unpack(bits: u64) -> Self {
        match bits {
            0b00 => NumericFeedback::FixnumOnly,
            0b0111 => NumericFeedback::Float,
            // Anything else in the slot is a call lattice value or an unknown
            // encoding: the safe over-approximation, never a float guess.
            _ => NumericFeedback::Other,
        }
    }
}

/// A per-function feedback vector — one slot per bytecode instruction, lazily
/// allocated on first use (when the instruction count is known). Slots for
/// non-call instructions stay [`CallFeedback::Uninit`]. Lock-free
/// (`AtomicU64`), `Send + Sync` — sound to hold inline on a GC-managed function
/// alongside the concurrent collector (the mutator is the only writer).
#[derive(Debug, Default)]
pub struct FeedbackVec {
    slots: OnceLock<Box<[AtomicU64]>>,
}

impl FeedbackVec {
    #[inline]
    pub const fn new() -> Self {
        Self {
            slots: OnceLock::new(),
        }
    }

    /// Allocate (once) `len` zeroed slots. Idempotent; a benign race just keeps
    /// whichever allocation wins.
    #[inline]
    fn slots(&self, len: usize) -> &[AtomicU64] {
        self.slots
            .get_or_init(|| (0..len).map(|_| AtomicU64::new(0)).collect())
    }

    /// Record an observed callee `sym` at call-site `pc` (instruction index);
    /// `ops_len` is the function's instruction count, for lazy sizing. Drives
    /// the `Uninit -> Monomorphic -> Megamorphic` lattice.
    #[inline]
    pub fn record_call(&self, pc: usize, ops_len: usize, sym: SymId) {
        let slots = self.slots(ops_len);
        let Some(slot) = slots.get(pc) else { return };
        let next = match CallFeedback::unpack(slot.load(Ordering::Relaxed)) {
            CallFeedback::Uninit => CallFeedback::Monomorphic(sym),
            // Unchanged target — no store needed (stays monomorphic).
            CallFeedback::Monomorphic(seen) if seen == sym => return,
            CallFeedback::Monomorphic(_) => CallFeedback::Megamorphic,
            CallFeedback::Megamorphic => return,
        };
        slot.store(next.pack(), Ordering::Relaxed);
    }

    /// Record the operand types observed at arithmetic site `pc`. Called only
    /// from the opcodes' non-fixnum arm, so the fixnum fast path pays nothing.
    /// Drives `FixnumOnly -> Float -> Other`, with `Other` sticky.
    #[inline]
    pub fn record_numeric(&self, pc: usize, ops_len: usize, seen: NumericFeedback) {
        let slots = self.slots(ops_len);
        let Some(slot) = slots.get(pc) else { return };
        let next = match (NumericFeedback::unpack(slot.load(Ordering::Relaxed)), seen) {
            (NumericFeedback::Other, _) => return,
            (NumericFeedback::Float, NumericFeedback::Float) => return,
            (_, seen) => seen,
        };
        slot.store(next.pack(), Ordering::Relaxed);
    }

    /// Operand-type feedback at arithmetic site `pc`.
    #[inline]
    pub fn numeric_at(&self, pc: usize) -> NumericFeedback {
        match self.slots.get() {
            None => NumericFeedback::FixnumOnly,
            Some(slots) => slots.get(pc).map_or(NumericFeedback::FixnumOnly, |s| {
                NumericFeedback::unpack(s.load(Ordering::Relaxed))
            }),
        }
    }

    /// Feedback at call-site `pc` (or `Uninit` if unallocated / out of range).
    #[inline]
    pub fn call_at(&self, pc: usize) -> CallFeedback {
        match self.slots.get() {
            None => CallFeedback::Uninit,
            Some(slots) => slots.get(pc).map_or(CallFeedback::Uninit, |s| {
                CallFeedback::unpack(s.load(Ordering::Relaxed))
            }),
        }
    }
}

impl Clone for FeedbackVec {
    /// A clone starts with no feedback (per-instance, like the heat counter).
    fn clone(&self) -> Self {
        Self::new()
    }
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
    heat: AtomicU32,
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
    native_rejected_epoch: AtomicU64,
    /// Heat at which a body the profitability gate refused (`NotProfitable`)
    /// is compiled anyway (0 = not deferred). A call-heavy body RUNS faster
    /// native but its compile is dear (org editing probe, 2026-09-05: ~38M
    /// instructions per admitted body vs ~830 saved per native entry), so it
    /// pays off only past ~18k calls: the gate was +6.4% on a 5-pass session
    /// and −8.8% on a 50-pass one. Deferring to `profit_defer_factor() ×
    /// hot_threshold()` lets long sessions win without taxing short ones.
    /// The dispatcher answers `Interpret` without a cache probe until then.
    profit_deferred_heat: AtomicU32,
    /// Per-call-site type/target feedback (Phase 1). The optimizing tier reads
    /// this to speculate direct/inlined calls.
    feedback: FeedbackVec,
    /// Process-unique identity assigned on first JIT compilation attempt (0 =
    /// unassigned). Keys this function's entry in the per-thread compiled-code
    /// cache ([`cache`]). Monotonic and never reused, so a freed function's
    /// stale cache entry can never be mis-looked-up after the (non-moving) GC
    /// reuses its address — a new function gets a new id. Reset to 0 on clone.
    compiled_id: AtomicU64,
    /// Set by the AOT preload prepopulate (R2-C3) when this function's leaf was
    /// inserted into the compiled cache at startup: `dispatch` then serves the
    /// prewarmed native leaf FROM CALL 1 instead of interpreting until the heat
    /// threshold — the piece that lets the AOT preload cover ONE-SHOT startup
    /// elisp, which never gets hot. One relaxed load on the dispatch path,
    /// never set in the default (AOT-off) configuration.
    aot_prewarmed: std::sync::atomic::AtomicBool,
    /// Slice C1 of the JIT call seam: the compiled leaf the interpreter's
    /// `Bcall` arm enters DIRECTLY (no cache probe, no arg marshaling), as a
    /// raw `*const CompiledLeaf`, valid only while `leaf_slot_epoch` equals
    /// `cache::leaf_slot_epoch()` (bumped on every retire/clear). Zero = empty.
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
    numeric_feedback_consumed: std::sync::atomic::AtomicBool,
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
    patched_prefix: AtomicU32,
    /// Test-only: pin this function to the Tier-0 interpreter regardless of
    /// hotness (the benchmark harness measures native vs interpreter in ONE
    /// process — a hot copy and a forced-cold copy — to cancel the
    /// cross-process CPU-frequency variance that wrecks a two-process A/B).
    /// Absent from the production library (only the test binary carries it).
    #[cfg(test)]
    force_interpret: std::sync::atomic::AtomicBool,
}

/// Source of process-unique [`Runtime::compiled_id`] values. Ids are
/// `fetch_add + 1` so 0 stays reserved for "unassigned".
static NEXT_COMPILED_ID: AtomicU64 = AtomicU64::new(0);

/// Invocations before a function tiers up to the JIT. Defaults to
/// [`Runtime::HOT_THRESHOLD`]; the `NEOVM_JIT_THRESHOLD` environment variable
/// overrides it — e.g. `=1` runs every compilable function through the JIT,
/// the every-function differential soak used to qualify default-on (Phase 9).
/// Body-size unit for the tier-up budget (`RuntimeState::dispatch_sized`): a
/// body of `n` ops must be called `hot_threshold() * max(1, n / unit)` times
/// before it tiers, so the (size-proportional) compile cost is amortized over
/// proportionally more interpreted calls before it is paid. Defaults to
/// [`RuntimeState::SIZE_UNIT`]; `NEOVM_JIT_SIZE_UNIT` overrides it (`0`
/// disables the scaling — every body tiers at the flat threshold).
pub fn size_unit() -> u32 {
    static UNIT: OnceLock<u32> = OnceLock::new();
    *UNIT.get_or_init(|| {
        std::env::var("NEOVM_JIT_SIZE_UNIT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(RuntimeState::SIZE_UNIT)
    })
}

/// `NEOVM_JIT_PROFIT_DEFER`: the factor a profitability-refused body's tier-up
/// is deferred by (× [`hot_threshold`]); `0` = refuse forever, as before.
/// Defaults to [`RuntimeState::PROFIT_DEFER_FACTOR`].
pub fn profit_defer_factor() -> u32 {
    #[cfg(test)]
    if let Some(forced) = PROFIT_DEFER_TEST_OVERRIDE.with(|c| c.get()) {
        return forced;
    }
    static FACTOR: OnceLock<u32> = OnceLock::new();
    *FACTOR.get_or_init(|| {
        std::env::var("NEOVM_JIT_PROFIT_DEFER")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(RuntimeState::PROFIT_DEFER_FACTOR)
    })
}

#[cfg(test)]
std::thread_local! {
    static PROFIT_DEFER_TEST_OVERRIDE: std::cell::Cell<Option<u32>> =
        const { std::cell::Cell::new(None) };
}

/// Test-only: pin [`profit_defer_factor`] for this thread.
#[cfg(test)]
pub(crate) fn force_profit_defer_for_test(factor: Option<u32>) {
    PROFIT_DEFER_TEST_OVERRIDE.with(|c| c.set(factor));
}

/// Heat at which a fast-allocator leaf is rebuilt with the full allocator
/// ([`RuntimeState::RETIER_FACTOR`] × [`hot_threshold`]); `None` = never
/// (`NEOVM_JIT_RETIER_FACTOR=0`).
/// `retier_heat` cache: `0` = not read yet, `u64::MAX` = no re-tier crossing,
/// else `1 + heat`. The env var it derives from cannot change under us, so a
/// racing double read resolves to the same value.
static RETIER_HEAT_CACHE: AtomicU64 = AtomicU64::new(0);

#[inline]
pub fn retier_heat() -> Option<u32> {
    // A plain relaxed load rather than a `OnceLock<Option<u32>>` read: the
    // armed-leaf entry consults this on EVERY compiled call, and there a
    // `OnceLock` costs its initialized-flag branch plus an acquire fence.
    match RETIER_HEAT_CACHE.load(Ordering::Relaxed) {
        0 => retier_heat_init(),
        u64::MAX => None,
        at => Some((at - 1) as u32),
    }
}

#[cold]
#[inline(never)]
fn retier_heat_init() -> Option<u32> {
    let factor = std::env::var("NEOVM_JIT_RETIER_FACTOR")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(RuntimeState::RETIER_FACTOR);
    let at = (factor != 0).then(|| hot_threshold().saturating_mul(factor));
    RETIER_HEAT_CACHE.store(
        at.map_or(u64::MAX, |heat| u64::from(heat) + 1),
        Ordering::Relaxed,
    );
    at
}

/// Largest body (in ops) the JIT tiers up at all; bigger bodies stay on the
/// interpreter. Defaults to [`RuntimeState::MAX_TIER_OPS`]; `NEOVM_JIT_MAX_OPS`
/// overrides it (`0` = no cap).
pub fn max_tier_ops() -> u32 {
    static CAP: OnceLock<u32> = OnceLock::new();
    *CAP.get_or_init(|| {
        std::env::var("NEOVM_JIT_MAX_OPS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(RuntimeState::MAX_TIER_OPS)
    })
}

pub fn hot_threshold() -> u32 {
    static THRESHOLD: OnceLock<u32> = OnceLock::new();
    *THRESHOLD.get_or_init(|| {
        std::env::var("NEOVM_JIT_THRESHOLD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(Runtime::HOT_THRESHOLD)
    })
}

/// Heat credited per backward-branch *wrap* — one wrap is
/// [`LOOP_BACKEDGES_PER_WRAP`] (256) loop iterations, so this weights 256 loop
/// iterations as ≈ one function invocation (`dispatch` credits +1 per call).
/// This is the tier-up signal for a body dominated by a long INNER LOOP but
/// called only a handful of times: `dispatch` alone would never make it hot
/// (heat counts calls), so a hot loop in a rarely-called function stayed in the
/// interpreter forever. `NEOVM_JIT_LOOP_HEAT` overrides it; **`=0` disables
/// loop heat** — the pre-loop-heat behavior and the A/B baseline.
pub fn loop_heat_per_wrap() -> u32 {
    static V: OnceLock<u32> = OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("NEOVM_JIT_LOOP_HEAT")
            .ok()
            .and_then(|s| s.parse().ok())
            // 8 ⇒ 32 loop iterations ≈ one invocation ⇒ a loop tiers up (and
            // OSR fires) near 32k total iterations. The old credit of 1
            // needed 256k iterations — a 60k-iteration hot loop in a
            // once-called function (the realworld buffer bench's insert and
            // scan loops) never left the interpreter.
            .unwrap_or(8)
    })
}

/// Whether the JIT is active at runtime. The `jit` cargo feature compiles the
/// JIT *in*; this switch turns tier-up on/off WITHOUT a recompile, so a single
/// binary can run pure-interpreter or JIT-backed. Default on; `NEOVM_JIT=0`
/// (also `off`/`false`/`no`) forces the interpreter — a kill switch and the
/// A/B-measurement knob (no more `NEOVM_JIT_THRESHOLD=<huge>` hack).
pub fn jit_runtime_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

#[cfg(test)]
std::thread_local! {
    static CALL_FEEDBACK_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force per-call feedback collection on/off on the current thread (tests only).
#[cfg(test)]
pub fn force_call_feedback_for_test(on: bool) {
    CALL_FEEDBACK_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

/// Whether the VM records per-call-site target feedback (`record_call`) on the
/// `Op::Call` hot path.
///
/// **Default OFF.** The feedback vector ([`CallFeedback`] / [`FeedbackVec`]) is
/// the optimizing tier's most important input — `Monomorphic(sym)` is what a
/// future MIR tier turns into a direct/inlined call. But NO tier consumes it
/// today: ordinary bytecode-to-bytecode calls never reach the compile seam
/// (`dispatch_sized` sees ~0.15% of calls), so recording a callee at every call
/// is pure overhead — measured +7.4% Ir / +14.2% cycles on the 3M-call
/// microbenchmark and 3–5% Ir on org-editing, feeding a decision nothing reads.
///
/// This gate stops the *collection*, not the mechanism: `record_call`,
/// `call_at`, and the whole `CallFeedback` lattice are retained unchanged. When
/// the consuming tier is wired, flip this default (or gate it on that tier being
/// active) and the feedback flows again. `NEOVM_JIT_CALL_FEEDBACK=on` re-enables
/// it now for A/B measurement and for the feedback tests.
/// ISOLATION KNOB (measurement only). `NEOVM_JIT_BCALL_TIER=off`: on the
/// adaptive policy, `Op::Call` no longer consults the tier dispatcher
/// (`dispatch_bytecode_call_from_stack` -> `dispatch_sized`: one function call
/// plus the heat atomics and threshold arithmetic) -- it interprets directly,
/// exactly as the interpreter-only policy does at that site, while the monomorphic
/// call cache still stays UNPOPULATED. Isolates the dispatcher's per-call cost
/// from the cache-miss cost. Functions called only through Bcall can no longer
/// tier up while this is set.
pub fn jit_bcall_tier_skipped() -> bool {
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| std::env::var("NEOVM_JIT_BCALL_TIER").as_deref() == Ok("off"))
}

/// ISOLATION KNOB (measurement only). `NEOVM_JIT_BCALL_CACHE=on`: on the
/// adaptive policy, the call-target resolver populates the one-entry
/// monomorphic cache (`RecentInterpreterCall`) for iteratively-enterable
/// bytecode callees, as the interpreter-only policy does, so the repeated call
/// takes the cached fast path and reaches neither the resolver nor the tier
/// dispatcher. Isolates the cache-miss + re-resolve cost. Cached callees stop
/// accumulating Bcall heat while this is set.
pub fn jit_bcall_cache_forced() -> bool {
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| std::env::var("NEOVM_JIT_BCALL_CACHE").as_deref() == Ok("on"))
}

#[inline]
pub fn call_feedback_collection_enabled() -> bool {
    #[cfg(test)]
    if let Some(o) = CALL_FEEDBACK_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NEOVM_JIT_CALL_FEEDBACK").as_deref() == Ok("on"))
}

/// OSR (on-stack replacement): transfer a hot loop in a rarely-/once-called
/// function into native code MID-execution (the case loop-heat's next-entry
/// tier-up cannot reach). Default ON since the `mod` arith-intrinsic made the
/// transferred loop a measured win on builtin-call-bearing bodies (list
/// workload −25% wall; the shimmed-builtin overhead previously ate the
/// transfer's gain — the reason this started life opt-in). Kill switch:
/// `NEOVM_JIT_OSR=off` (same spelling family as `NEOVM_JIT`); the interpreter
/// marshals its live operand and binding stacks into a native OSR entry.
/// Handler/save operations and nonlexical functions remain ineligible.
/// Off ⇒ the back-edge stays a pure interpreter loop, zero added cost.
pub fn jit_osr_on() -> bool {
    #[cfg(test)]
    if let Some(o) = OSR_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_OSR").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

#[cfg(test)]
thread_local! {
    static OSR_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force OSR on/off on the current thread (tests only), overriding the env gate.
#[cfg(test)]
pub fn force_osr_for_test(on: bool) {
    OSR_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

impl RuntimeState {
    /// Invocations before a function is "hot" enough to tier up.
    ///
    /// Tuned via `jit_bench_threshold_economics` (eval_test.rs), an
    /// interleaved debug-build A/B of 1_000 vs the previous placeholder
    /// 10_000 across 1.2k/3k/20k-call workloads: 1_000 halves end-to-end
    /// wall time for the 3k-20k call population (the functions a 10_000
    /// threshold strands in the interpreter forever) and only regresses
    /// ~1.2k-call functions by the one-time compile cost — which a debug
    /// build heavily inflates, so the release regression is smaller still.
    /// Compilation is the only cost lowering adds; going far lower (100)
    /// starts compiling barely-warm functions for no amortized win.
    /// `NEOVM_JIT_THRESHOLD` still overrides per process.
    pub const HOT_THRESHOLD: u32 = 1_000;

    #[inline]
    pub const fn new() -> Self {
        Self {
            heat: AtomicU32::new(0),
            native_rejected_epoch: AtomicU64::new(0),
            profit_deferred_heat: AtomicU32::new(0),
            feedback: FeedbackVec::new(),
            compiled_id: AtomicU64::new(0),
            aot_prewarmed: std::sync::atomic::AtomicBool::new(false),
            numeric_feedback_consumed: std::sync::atomic::AtomicBool::new(false),
            leaf_slot: AtomicU64::new(0),
            leaf_slot_epoch: AtomicU64::new(0),
            patched_prefix: AtomicU32::new(0),
            #[cfg(test)]
            force_interpret: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Number of leading constant slots that are per-instance (`make-closure`
    /// patched) for this source; 0 for a plain function.
    #[inline]
    pub fn patched_prefix(&self) -> usize {
        self.patched_prefix.load(Ordering::Relaxed) as usize
    }

    /// Record a `make-closure` patch of width `n`. Returns `Some(compiled_id)`
    /// when the recorded prefix GREW while a compiled id was already assigned:
    /// any leaf compiled for that id assumed the narrower prefix (it may have
    /// baked a slot that is now per-instance) and must be evicted by the
    /// caller before the next dispatch.
    pub fn note_patched_prefix(&self, n: usize) -> Option<u64> {
        let n = u32::try_from(n).unwrap_or(u32::MAX);
        let prev = self.patched_prefix.fetch_max(n, Ordering::Relaxed);
        if n > prev {
            // The caller evicts the cached verdict for this id; forget ours
            // too, so the next dispatch re-consults the cache like before.
            self.native_rejected_epoch.store(0, Ordering::Relaxed);
            self.compiled_id()
        } else {
            None
        }
    }

    /// Defer this body's tier-up to `heat` (see `profit_deferred_heat`).
    pub(crate) fn defer_tier_up(&self, heat: u32) {
        self.profit_deferred_heat.store(heat, Ordering::Relaxed);
    }

    /// Whether a profitability deferral is still holding at heat `now`.
    #[inline]
    fn tier_up_deferred(&self, now: u32) -> bool {
        let at = self.profit_deferred_heat.load(Ordering::Relaxed);
        at != 0 && now < at
    }

    /// Whether this body's deferral has run out: it was deferred and its heat
    /// has reached the deferral point, so the next compile bypasses the gate.
    pub(crate) fn profit_deferral_expired(&self) -> bool {
        let at = self.profit_deferred_heat.load(Ordering::Relaxed);
        at != 0 && self.heat() >= at
    }

    /// Record that the JIT rejected this body (`CacheEntry::NotCompilable`)
    /// under NotCompilable generation `epoch` — see `native_rejected_epoch`.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    pub(crate) fn mark_native_rejected(&self, epoch: u64) {
        self.native_rejected_epoch.store(epoch, Ordering::Relaxed);
    }

    /// Whether a remembered `NotCompilable` verdict is still current, i.e. a
    /// cache probe would only re-find it.
    #[inline]
    fn native_rejected(&self) -> bool {
        #[cfg(feature = "jit")]
        {
            let stamped = self.native_rejected_epoch.load(Ordering::Relaxed);
            stamped != 0 && stamped == super::jit::cache::rejection_epoch()
        }
        #[cfg(not(feature = "jit"))]
        {
            false
        }
    }

    /// Record one invocation and decide how to run it. The caller MUST handle
    /// the returned [`Plan`] exhaustively.
    ///
    /// Counts the invocation and returns [`Plan::Compiled`] once the function
    /// crosses [`hot_threshold`] (default [`Runtime::HOT_THRESHOLD`]), else
    /// [`Plan::Interpret`]. The compiled plan only means "the JIT may run this"
    /// — the cache still falls back to the interpreter on deopt, and a body
    /// the cache has rejected as `NotCompilable` is remembered here
    /// (`native_rejected_epoch`) so it answers `Interpret` without the trip.
    /// Default [`size_unit`]: bodies up to this many ops tier at the flat
    /// `hot_threshold()`; larger ones need proportionally more calls. Tuned on
    /// the fontify gate (font-lock closures now tier since `make-closure`
    /// instances share heat): a 352-op keyword matcher cost ~80 ms / ~135M Ir
    /// to compile and ran break-even natively, so a flat threshold paid the
    /// whole compile inside one fontification for nothing. V8's interrupt
    /// budget is the precedent for scaling tier-up by bytecode length.
    ///
    /// 512 since 2026-09-15 (was 64), with [`Self::MAX_TIER_OPS`] raised to
    /// 4096: the byte compiler's workhorses are big and called tens of times
    /// per file — `byte-optimize-lapcode` is 2,528 ops and ~64 calls, which at
    /// 64 ops a unit needed 39,000 calls, and `byte-compile-out-toplevel` (288
    /// ops) was over the old cap — so they never left the interpreter. elb
    /// bytecomp -11.5% instructions (50 compiles: 19.83G -> 17.54G, GNU
    /// 16.0G), org-editing -3.7%, org-editing-heavy -0.7%; the other ELB rows
    /// within noise; a 3-file compile pays +4.7% in compile time.
    pub const SIZE_UNIT: u32 = 512;

    /// Default [`retier_heat`] factor: a leaf compiled with the fast register
    /// allocator (`lowering::RegallocChoice::Fast`) is rebuilt with the full
    /// one once its heat reaches this many [`hot_threshold`]s. 16 = 16,000
    /// calls at the default threshold: the call-heavy benchmark (3M calls)
    /// spends 0.5% of them on the fast code; an editing session's leaves
    /// (hundreds to a few thousand calls) never pay the second compile.
    pub const RETIER_FACTOR: u32 = 16;

    /// Default [`profit_defer_factor`]: `0` keeps the profitability gate a
    /// veto (a refused body never compiles); `K` defers the compile to
    /// `K × hot_threshold()` calls instead.
    ///
    /// Chosen by same-binary sweeps (2026-09-05, instructions, medians of 3,
    /// every run checked for exit status and output; `tmp/rr/wf2/ab-k.sh`),
    /// each arm vs the veto, with call-heavy bodies on the fast allocator:
    ///
    /// | fixture                      | K=2    | K=4    | K=8    |
    /// |------------------------------|--------|--------|--------|
    /// | org editing, 5 passes        | −0.05% | −0.13% | −0.23% |
    /// | org editing, 25 passes       | −1.71% | −1.58% | −1.58% |
    /// | org editing, 50 passes       | −3.36% | −3.18% | −2.50% |
    /// | byte-compile cc-engine.el    | −0.86% | −1.13% | −0.72% |
    /// | 3M-call benchmark            | +0.00% | +0.00% | −0.00% |
    /// | 200-function compile fixture | +0.61% | +0.48% | +0.31% |
    ///
    /// A call-heavy body runs faster native (~830 instructions per entry on
    /// org) but its compile is dear, so admitting it at the flat threshold
    /// (gate off) wins in long sessions and loses in short ones: org 5 passes
    /// +3.1%, 50 passes −7.0%, the compile fixture +10.6%. 4 takes most of
    /// the long-session win and the best byte-compile point while every
    /// short session stays within half a percent of the veto.
    pub const PROFIT_DEFER_FACTOR: u32 = 4;

    /// Default [`max_tier_ops`].
    ///
    /// Originally (2026-08-28) the same 352-op font-lock matcher cost ~80 ms
    /// to compile for break-even native code, so the cap was a compile-stall
    /// guard. Re-measured 2026-08-31: compile cost is now effectively linear
    /// (~3.5 µs/op on loop-shaped bodies, 20 µs/op on branch/deopt-heavy
    /// matchers; that matcher compiles in 7.15 ms, whole type-sim compile
    /// total 12.6 ms).
    ///
    /// CORRECTED 2026-09-01: the old "break-even with the interpreter"
    /// finding was an artifact — the "interpreted" halves of those A/Bs ran
    /// ~99.99% NATIVE, because OSR ignores the cap (this gate covers only
    /// entry dispatch) and, in benches, ignored `force_interpret` (fixed:
    /// `is_hot` now honors it). With an honest Tier-0 baseline the >256-op
    /// fixture (`jit_bench_big_body_matcher_shape`) runs 12.6x FASTER
    /// native. The cap's practical effect is therefore only to delay the
    /// ENTRY tier for big bodies whose loops OSR anyway; lifting it is
    /// pending a real-workload A/B (byte-compile watch per the GATE_RELAX
    /// precedent) — see the mid-end campaign notes.
    ///
    /// That A/B (2026-09-15, see [`Self::SIZE_UNIT`]) raised it to 4096: a
    /// stall guard, not a profit gate, sized to admit the byte compiler's
    /// 2,528-op `byte-optimize-lapcode`.
    pub const MAX_TIER_OPS: u32 = 4096;

    /// [`dispatch`](Self::dispatch) with the tier-up budget scaled by the body
    /// size (`ops_len`): bodies above [`max_tier_ops`] never tier, and the hot
    /// threshold is multiplied by `max(1, ops_len / size_unit())`. The seam
    /// call sites use this; the unsized `dispatch` is the flat rule (tests,
    /// tiny bodies).
    #[inline]
    pub fn dispatch_sized(&self, ops_len: usize) -> Plan {
        if !jit_runtime_enabled() {
            return Plan::Interpret;
        }
        #[cfg(test)]
        if self.force_interpret.load(Ordering::Relaxed) {
            return Plan::Interpret;
        }
        let prev = self.heat.load(Ordering::Relaxed);
        let now = prev.saturating_add(1);
        self.heat.store(now, Ordering::Relaxed);
        if self.aot_prewarmed.load(Ordering::Relaxed) {
            return Plan::Compiled;
        }
        if self.native_rejected() || self.tier_up_deferred(now) {
            #[cfg(feature = "jit")]
            super::jit::stats::record_dispatch(false);
            return Plan::Interpret;
        }
        // A body the profit gate refused tiers up exactly at its deferral
        // heat. It already proved hot once — usually on first sight from
        // compiled code, where the seam compiles with no threshold at all —
        // so the size-scaled first-sight threshold below must not apply
        // again: for the 200–800-op font-lock bodies it meant 5–12K more
        // calls, i.e. never within a session (org op −7.6% Ir when they run
        // native).
        if self.profit_deferred_heat.load(Ordering::Relaxed) != 0 {
            #[cfg(feature = "jit")]
            super::jit::stats::record_dispatch(true);
            return Plan::Compiled;
        }
        let cap = max_tier_ops();
        if cap != 0 && ops_len > cap as usize {
            return Plan::Interpret;
        }
        let threshold = hot_threshold();
        let unit = size_unit();
        let factor = if unit == 0 {
            1
        } else {
            u32::try_from(ops_len / unit as usize)
                .unwrap_or(u32::MAX)
                .max(1)
        };
        let plan = if now >= threshold.saturating_mul(factor) {
            Plan::Compiled
        } else {
            Plan::Interpret
        };
        #[cfg(feature = "jit")]
        super::jit::stats::record_dispatch(matches!(plan, Plan::Compiled));
        plan
    }

    #[inline]
    pub fn dispatch(&self) -> Plan {
        // Runtime kill switch (NEOVM_JIT=0): never tier up — pure interpreter,
        // no recompile. The early return also skips the heat bump, so a disabled
        // JIT is strictly cheaper than an enabled-but-cold one.
        if !jit_runtime_enabled() {
            return Plan::Interpret;
        }
        // Test-only: a forced-cold function never tiers up (benchmark A/B).
        #[cfg(test)]
        if self.force_interpret.load(Ordering::Relaxed) {
            return Plan::Interpret;
        }
        // Saturating bump — a long-lived hot function must never wrap to cold.
        let prev = self.heat.load(Ordering::Relaxed);
        let now = prev.saturating_add(1);
        self.heat.store(now, Ordering::Relaxed);
        if self.aot_prewarmed.load(Ordering::Relaxed) {
            Plan::Compiled
        } else if self.native_rejected() || self.tier_up_deferred(now) {
            Plan::Interpret
        } else if now >= hot_threshold() {
            Plan::Compiled
        } else {
            Plan::Interpret
        }
    }

    /// This function's compiled-cache id, assigning a fresh process-unique one
    /// on first call (idempotent under races). Used only by [`cache`].
    #[inline]
    pub fn compiled_id_or_assign(&self) -> u64 {
        let cur = self.compiled_id.load(Ordering::Acquire);
        if cur != 0 {
            return cur;
        }
        // `+ 1` keeps 0 reserved for "unassigned".
        let fresh = NEXT_COMPILED_ID.fetch_add(1, Ordering::Relaxed) + 1;
        match self
            .compiled_id
            .compare_exchange(0, fresh, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => fresh,
            // Another thread won the race; adopt its id, discard ours.
            Err(actual) => actual,
        }
    }

    /// This function's compiled-cache id if one was ALREADY assigned (it has been
    /// compiled/hot), else `None` — WITHOUT assigning a fresh one. The AOT-PGO drain
    /// uses this to intersect the obarray walk with the hot set without minting ids
    /// for the many never-compiled bound functions it walks past.
    #[inline]
    pub fn compiled_id(&self) -> Option<u64> {
        let cur = self.compiled_id.load(Ordering::Acquire);
        (cur != 0).then_some(cur)
    }

    /// True once this function has crossed the tier-up threshold.
    #[inline]
    pub fn is_hot(&self) -> bool {
        // A forced-cold function must never read as hot — the OSR gate
        // consults is_hot() directly, and OSR ignoring force_interpret is
        // how a benchmark's "interpreter" half silently ran ~99.99% native
        // (the Phase-0 big-body baseline was really the baseline-OSR leaf).
        #[cfg(test)]
        if self.force_interpret.load(Ordering::Relaxed) {
            return false;
        }
        self.heat.load(Ordering::Relaxed) >= hot_threshold()
    }

    /// Current invocation count.
    /// Advance the call heat by one (what `dispatch_sized` does before its
    /// tier decision) and return the new value — the direct stack entry keeps
    /// the re-tier trigger honest without the rest of the dispatcher.
    #[cfg(feature = "jit")]
    #[inline]
    pub(crate) fn bump_heat(&self) -> u32 {
        let now = self.heat.load(Ordering::Relaxed).saturating_add(1);
        self.heat.store(now, Ordering::Relaxed);
        now
    }

    /// The heat WITHOUT advancing it — for a probe that may decline and let
    /// another probe of the same call do the advancing.
    #[cfg(feature = "jit")]
    #[inline]
    pub(crate) fn peek_heat(&self) -> u32 {
        self.heat.load(Ordering::Relaxed)
    }

    #[cfg(all(feature = "jit", test))]
    pub(crate) fn force_interpret_for_test(&self) -> bool {
        self.force_interpret.load(Ordering::Relaxed)
    }

    /// The leaf armed for the direct stack entry, if it was armed under
    /// `epoch` (the current `cache::leaf_slot_epoch()`); a stale slot reads
    /// as empty and is re-armed through the cache by the tier-up entry.
    #[cfg(feature = "jit")]
    #[inline]
    pub(crate) fn armed_leaf_slot(&self, epoch: u64) -> Option<*const compile::CompiledLeaf> {
        let ptr = self.leaf_slot.load(Ordering::Relaxed);
        (ptr != 0 && self.leaf_slot_epoch.load(Ordering::Relaxed) == epoch)
            .then_some(ptr as *const compile::CompiledLeaf)
    }

    /// Arm the direct stack entry with a leaf resolved from the cache under
    /// `epoch`. Epoch first, pointer second: a reader that sees the pointer
    /// sees an epoch at least as new.
    #[cfg(feature = "jit")]
    pub(crate) fn arm_leaf_slot(&self, leaf: *const compile::CompiledLeaf, epoch: u64) {
        self.leaf_slot_epoch.store(epoch, Ordering::Relaxed);
        self.leaf_slot.store(leaf as u64, Ordering::Relaxed);
    }

    #[inline]
    pub fn heat(&self) -> u32 {
        self.heat.load(Ordering::Relaxed)
    }

    /// Backward branches per loop-heat wrap — the interpreter's `branch_to!`
    /// quit counter is a `u8`, so it wraps (and calls [`note_loop_work`]) once
    /// per 256 backward branches. Documented here so the loop-heat weighting
    /// (256 iterations ≈ one call) is discoverable from the `Runtime` API.
    ///
    /// [`note_loop_work`]: Self::note_loop_work
    pub const LOOP_BACKEDGES_PER_WRAP: u32 = 256;

    /// Credit loop work toward tier-up: called from the interpreter's
    /// backward-branch quit-counter wrap (`bytecode/vm.rs`), i.e. once per
    /// [`LOOP_BACKEDGES_PER_WRAP`](Self::LOOP_BACKEDGES_PER_WRAP) iterations, so
    /// the per-iteration cost is amortized to ~nothing. A function whose body is
    /// a long INNER LOOP but which is CALLED only a few times never crosses
    /// [`hot_threshold`] on `dispatch`'s per-call bump alone; this accumulates
    /// heat from the loop itself so the NEXT entry tiers it up. The CURRENT
    /// interpreted call still runs to completion in Tier 0 — there is no
    /// on-stack replacement, so a body called exactly once sees no benefit
    /// (that is the OSR follow-up). Saturating: a long-lived loop must never
    /// wrap heat back to cold. Respects the [`jit_runtime_enabled`] kill switch
    /// and the `NEOVM_JIT_LOOP_HEAT=0` off knob.
    #[inline]
    pub fn note_loop_work(&self) {
        let credit = loop_heat_per_wrap();
        if credit == 0 || !jit_runtime_enabled() {
            return;
        }
        let prev = self.heat.load(Ordering::Relaxed);
        self.heat
            .store(prev.saturating_add(credit), Ordering::Relaxed);
    }

    /// Test-only: force this function "hot" so the next [`dispatch`](Self::dispatch)
    /// tiers it up, without driving `HOT_THRESHOLD` real invocations.
    /// Mark this function as served by a prepopulated AOT leaf (see the
    /// field doc): `dispatch` returns `Plan::Compiled` from call 1.
    pub(crate) fn mark_aot_prewarmed(&self) {
        self.aot_prewarmed
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn set_hot_for_test(&self) {
        self.heat.store(Self::HOT_THRESHOLD, Ordering::Relaxed);
    }

    /// Test-only: set the heat outright (re-tier tests).
    #[cfg(test)]
    pub(crate) fn set_heat_for_test(&self, heat: u32) {
        self.heat.store(heat, Ordering::Relaxed);
    }

    /// Test-only: pin this function to the Tier-0 interpreter forever (the
    /// forced-cold half of the benchmark A/B; see `force_interpret`).
    #[cfg(test)]
    pub(crate) fn set_cold_for_test(&self) {
        self.force_interpret
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record an observed callee `sym` at the call site at instruction `pc`
    /// (`ops_len` = the function's instruction count, for lazy sizing).
    #[inline]
    pub fn record_call(&self, pc: usize, ops_len: usize, sym: SymId) {
        self.feedback.record_call(pc, ops_len, sym);
    }

    /// Call-site feedback observed at instruction `pc`.
    #[inline]
    pub fn call_feedback(&self, pc: usize) -> CallFeedback {
        self.feedback.call_at(pc)
    }

    /// Record the operand types seen at the arithmetic site at instruction
    /// `pc` — see [`NumericFeedback`].
    #[inline]
    pub fn record_numeric(&self, pc: usize, ops_len: usize, seen: NumericFeedback) {
        self.feedback.record_numeric(pc, ops_len, seen);
    }

    /// Operand-type feedback observed at arithmetic site `pc`.
    #[inline]
    pub fn numeric_feedback(&self, pc: usize) -> NumericFeedback {
        self.feedback.numeric_at(pc)
    }

    /// Whether this body's arithmetic sites still want their operand types
    /// recorded — see `numeric_feedback_consumed`.
    #[inline]
    pub fn wants_numeric_feedback(&self) -> bool {
        !self
            .numeric_feedback_consumed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Mark this body's numeric feedback as read by a compile.
    #[inline]
    pub fn note_numeric_feedback_consumed(&self) {
        self.numeric_feedback_consumed
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self::new()
    }
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
    shared: std::sync::Arc<RuntimeState>,
}

impl Runtime {
    pub const HOT_THRESHOLD: u32 = RuntimeState::HOT_THRESHOLD;

    /// A fresh, cold, unshared state — for a NEW source (reader, decoder,
    /// `make-byte-code`, pdump restore). Clones share instead.
    #[inline]
    pub fn new() -> Self {
        Self {
            shared: std::sync::Arc::new(RuntimeState::new()),
        }
    }
}

impl std::ops::Deref for Runtime {
    type Target = RuntimeState;
    #[inline]
    fn deref(&self) -> &RuntimeState {
        &self.shared
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Runtime {
    /// A clone SHARES the source's state (see the type docs) — a
    /// `make-closure` instance inherits the prototype's heat, feedback and
    /// compiled leaf, and contributes its own calls to them.
    fn clone(&self) -> Self {
        Self {
            shared: std::sync::Arc::clone(&self.shared),
        }
    }
}

#[cfg(test)]
#[path = "tests/jit_test.rs"]
mod tests;
