//! The compiled-leaf data model: CompileError, the leaf tiers and backings (LeafSidecar, LoadedUnit, LeafBacking, LeafTier), the CompiledLeaf handle and its methods, and NativeRun.
//!
//! Moved out of `compile.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

/// Why a bytecode body could not be compiled by this baseline tier.
///
/// Every variant means "stay on the Tier-0 interpreter"; none is fatal.
#[derive(Debug)]
pub enum CompileError {
    /// The function's parameter list is unsupported: `&optional`/`&rest`, or
    /// required params that are dynamically bound (not on the operand stack).
    TakesArguments,
    /// An opcode outside the supported leaf subset (coarse category for logs).
    UnsupportedOp(&'static str),
    /// The body did not end in `Return` (open block / fell off the end).
    NoReturn,
    /// A stack op referenced below the modelled operand stack.
    StackUnderflow,
    /// A `Constant`/`StackRef` operand was out of range for the pool/stack.
    BadOperand,
    /// The body is call-dominated, so native codegen would only add overhead
    /// (per-call operand GC-rooting + a runtime call shim) without an offsetting
    /// win — the baseline tier removes per-op dispatch, not call cost. Measured
    /// net-negative on real workloads; keep it on the interpreter.
    NotProfitable,
    /// The Cranelift backend failed to build or finalize the code.
    Backend(BackendError),
}

impl core::fmt::Display for CompileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CompileError::TakesArguments => write!(f, "function takes arguments"),
            CompileError::UnsupportedOp(k) => write!(f, "unsupported opcode: {k}"),
            CompileError::NoReturn => write!(f, "body does not end in Return"),
            CompileError::StackUnderflow => write!(f, "operand stack underflow"),
            CompileError::BadOperand => write!(f, "operand out of range"),
            CompileError::NotProfitable => write!(f, "call-dominated body, not JIT-profitable"),
            CompileError::Backend(e) => write!(f, "backend: {e}"),
        }
    }
}

impl std::error::Error for CompileError {}

/// Per-(thread,leaf) base-pointer block an AOT leaf's code reads to reach its
/// session-specific buffers (R1c-sidecar). Passed as the 4th entry argument.
///
/// ## Why a per-leaf sidecar
/// AOT code lives in a process-shared `.so`, but the buffers it addresses are
/// per-(thread,leaf): each thread rebuilds its OWN reloc `Vec` (against its OWN
/// thread-local heap) and allocates its OWN deopt buffers, all pointing at the
/// SAME code. The JIT bakes those addresses as `iconst` immediates — impossible
/// across sessions/threads in a shared `.so`. Instead AOT code loads each base
/// from THIS struct, whose pointer arrives as a call argument. Because the
/// pointer is a per-frame argument (passed by [`CompiledLeaf::invoke_native`]
/// from `&self`), reentrancy is automatic: leaf A's frame carries A's sidecar,
/// and A calling B does not perturb it.
///
/// ## Move-stability (load-bearing invariant)
/// The pointers address the leaf's SIBLING `Box` fields (`reloc_data`,
/// `deopt_spill`, `deopt_meta`). A `Box`'s heap pointee is move-stable, so they
/// stay valid when the owning `CompiledLeaf` moves into `Rc::new`. They MUST be
/// captured from the FINAL boxes (after the buffers are in place) and NOTHING
/// may reallocate those boxes after the sidecar is filled — the boxes are
/// immutable for the leaf's life (the `Cell`s inside are mutated in place, which
/// does not move the allocation). The sidecar is itself a `Box` (address-stable)
/// so the baked GlobalValue/arg sees one fixed `*const LeafSidecar`.
#[repr(C)]
pub(crate) struct LeafSidecar {
    /// Base of the per-thread reloc `Vec` (`reloc_data.as_ptr()`); heap-`Const`
    /// loads index off it. Null/unused when the leaf has no heap constants.
    pub(crate) reloc_base: *const Value,
    /// Base of the per-thread precise-deopt spill buffer (`deopt_spill.as_ptr()`).
    pub(crate) spill_base: *const core::cell::Cell<i64>,
    /// Address of the `pc` deopt cell (`&deopt_meta.pc`).
    pub(crate) meta_pc: *const core::cell::Cell<i64>,
    /// Address of the `depth` deopt cell.
    pub(crate) meta_depth: *const core::cell::Cell<i64>,
    /// Address of the `handlers` deopt cell.
    pub(crate) meta_handlers: *const core::cell::Cell<i64>,
    /// R2 increment B2: base of the per-(thread,leaf) `SpecSlot` array the AOT
    /// `Op::Call` spec sites index (`spec_slot_base[slot_idx]`). Null when the leaf
    /// has no armed spec site. Built + armed by [`CompiledLeaf::from_aot`].
    pub(crate) spec_slot_base: *const SpecSlot,
    /// R2 increment B2: base of the per-(thread,leaf) `expected` (subr/bytecode
    /// VALUE bits) array, parallel to `spec_slot_base`. The AOT spec sites load
    /// `spec_expected_base[slot_idx]` instead of baking the session-specific bits.
    /// Null when the leaf has no armed spec site.
    pub(crate) spec_expected_base: *const u64,
}

impl LeafSidecar {
    /// Byte offsets of each field, for the AOT lowering's `load(sidecar, off)`.
    /// `#[repr(C)]` fixes the layout so these match the generated loads exactly.
    pub(crate) const OFF_RELOC_BASE: i32 = 0;
    pub(crate) const OFF_SPILL_BASE: i32 = 8;
    pub(crate) const OFF_META_PC: i32 = 16;
    pub(crate) const OFF_META_DEPTH: i32 = 24;
    pub(crate) const OFF_META_HANDLERS: i32 = 32;
    pub(crate) const OFF_SPEC_SLOT_BASE: i32 = 40;
    pub(crate) const OFF_SPEC_EXPECTED_BASE: i32 = 48;
}

// Compile-time assertion that the hand-written offsets match the actual layout.
pub(crate) const _: () = {
    assert!(core::mem::size_of::<LeafSidecar>() == 56);
    assert!(core::mem::offset_of!(LeafSidecar, reloc_base) == LeafSidecar::OFF_RELOC_BASE as usize);
    assert!(core::mem::offset_of!(LeafSidecar, spill_base) == LeafSidecar::OFF_SPILL_BASE as usize);
    assert!(core::mem::offset_of!(LeafSidecar, meta_pc) == LeafSidecar::OFF_META_PC as usize);
    assert!(core::mem::offset_of!(LeafSidecar, meta_depth) == LeafSidecar::OFF_META_DEPTH as usize);
    assert!(
        core::mem::offset_of!(LeafSidecar, meta_handlers)
            == LeafSidecar::OFF_META_HANDLERS as usize
    );
    assert!(
        core::mem::offset_of!(LeafSidecar, spec_slot_base)
            == LeafSidecar::OFF_SPEC_SLOT_BASE as usize
    );
    assert!(
        core::mem::offset_of!(LeafSidecar, spec_expected_base)
            == LeafSidecar::OFF_SPEC_EXPECTED_BASE as usize
    );
};

/// A loaded AOT shared object (`.so`), owning its `libloading::Library`.
///
/// The dynamic library is kept mapped (`r-x` by the OS loader) for as long as any
/// cached [`CompiledLeaf`] backed by it is alive — it is NEVER unloaded while
/// cached, since the leaf's `entry` points into the library's code. Shared via
/// `Arc` so several leaves emitted into one unit can share a single `dlopen`.
///
pub(crate) struct LoadedUnit {
    /// The open dynamic library. Dropping it `dlclose`s the `.so` and unmaps its
    /// code, so it must outlive every leaf whose `entry` points into it. Read via
    /// [`LoadedUnit::library`] at load (dlsym); held purely to keep the mapping
    /// alive afterwards (the leaf calls `entry` directly).
    pub(crate) lib: libloading::Library,
}

impl LoadedUnit {
    /// Wrap an already-`dlopen`'d library so leaves can hold it alive.
    pub(crate) fn new(lib: libloading::Library) -> Self {
        Self { lib }
    }

    /// The open library, for `dlsym`'ing the entry + descriptor (R1c-5). The
    /// returned symbols borrow the library, which the `Arc<LoadedUnit>` keeps
    /// alive for the leaf's lifetime.
    pub(crate) fn library(&self) -> &libloading::Library {
        &self.lib
    }
}

impl core::fmt::Debug for LoadedUnit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LoadedUnit").finish_non_exhaustive()
    }
}

/// The lambda-list + frame-shape metadata an AOT leaf needs to rebuild a
/// [`CompiledLeaf`] at load (R1c). Recovered from the unit's exported descriptor
/// (`aot::AotDescriptor`) — it is the AOT analogue of the values the JIT lowering
/// computes inline (arity/required/has_rest from the lambda list; has_binds /
/// has_handlers / has_side_effects from the body; max_depth + has_precise_deopt
/// for the deopt-buffer sizing). Carried separately from the reloc recipe so the
/// loader can size the (per-thread) deopt buffers without touching the heap.
#[derive(Debug, Clone)]
pub(crate) struct AotLeafMeta {
    pub arity: usize,
    pub required: usize,
    pub has_rest: bool,
    pub has_binds: bool,
    pub has_handlers: bool,
    pub has_side_effects: bool,
    /// Deepest pre-op operand stack (the framestate a precise guard spills);
    /// sizes `deopt_spill` when `has_precise_deopt`.
    pub max_depth: usize,
    /// Whether the body has precise (post-call) deopt sites — i.e. it bakes the
    /// deopt-buffer addresses, so those buffers must be sized + their bases
    /// load-resolved. The R1c-5 pure subset is always `false` (rerun-from-start
    /// deopt only); call-bearing AOT is a later increment.
    pub has_precise_deopt: bool,
}

/// Which code producer a [`CompiledLeaf`]'s `entry` pointer lives in — and what
/// must be kept alive for the lifetime of the leaf so the code stays mapped.
///
/// AOT is a fourth code producer, NOT a deopt target: a `CompiledLeaf` is the
/// same handle, the same `entry` ABI, the same `NativeRun` outcomes whether its
/// code came from the JIT (`JITModule`-owned executable memory) or AOT (a loaded
/// `.so`). This enum just records the backing so drop releases the right thing.
/// No catch-all when matching on it — the compiler enforces that a new backing
/// is handled everywhere (the GC-tested completeness rule).
// Both variants own their backing directly so the compiled entry cannot outlive
// its mapped code; another allocation would only narrow this private enum.
#[allow(clippy::large_enum_variant)]
pub(crate) enum LeafBacking {
    /// JIT: the `JITModule` owns the executable memory `entry` points into.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    Jit(JITModule),
    /// AOT: a loaded shared object owns the code `entry` points into. `Arc` so
    /// several leaves from one unit share the single mapping; never unloaded
    /// while any backed leaf is cached.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    Aot(std::sync::Arc<LoadedUnit>),
}

impl core::fmt::Debug for LeafBacking {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LeafBacking::Jit(_) => f.write_str("LeafBacking::Jit"),
            LeafBacking::Aot(_) => f.write_str("LeafBacking::Aot"),
        }
    }
}

/// A compiled leaf function taking a fixed number of arguments.
///
/// Owns its [`LeafBacking`] (a JIT `JITModule` or a loaded AOT `.so`), which keeps
/// the code `entry` points into mapped for the lifetime of this handle. The raw
/// entry pointer makes this neither `Send` nor `Sync`, which is correct — the
/// code is tied to its owning backing.
/// Which compilation tier produced a leaf. Phase-0 observability for the
/// mid-end campaign: tier selection happens inside
/// `compile_bytecode_function_inner` and was previously recorded nowhere —
/// residency was visible only through the entry symbol name in external
/// profilers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeafTier {
    Baseline,
    Mir,
    Aot,
}

impl LeafTier {
    pub(crate) fn name(self) -> &'static str {
        match self {
            LeafTier::Baseline => "baseline",
            LeafTier::Mir => "mir",
            LeafTier::Aot => "aot",
        }
    }
}

pub struct CompiledLeaf {
    /// The tier that produced this leaf (see [`LeafTier`]).
    pub(crate) tier: LeafTier,
    /// The register allocator that built it: a `Fast` leaf re-tiers to `Full`
    /// once hot (`cache::try_run_compiled`); AOT leaves are always `Full`.
    pub(crate) regalloc: super::lowering::RegallocChoice,
    /// Compiled with the profitability gate bypassed (a deferred call-heavy
    /// body that proved hot): a recompile of this id keeps the bypass, or a
    /// stale-inline rebuild would meet the gate again and be deferred anew.
    pub(crate) profit_gate_bypassed: bool,
    /// The body has more calls than arithmetic (`compile::body_is_call_heavy`):
    /// compiled with the fast allocator and never re-tiered (see
    /// `lowering::choose_regalloc`).
    pub(crate) call_heavy: bool,
    /// Cranelift IR instructions the body lowered to (diagnostics: the IR
    /// expansion per bytecode op is what the compile cost tracks).
    pub(crate) clif_insts: u32,
    /// Cranelift IR blocks the body lowered to.
    pub(crate) clif_blocks: u32,
    /// Number of fixed slots the native code reads from the args pointer at
    /// entry: `nonrest` parameters (required + optional, nil-padded) plus one
    /// slot for the `&rest` list when present. [`call`](Self::call) normalizes
    /// an incoming argument list to exactly this many slots, mirroring the
    /// interpreter's `run_frame` frame seeding.
    pub(crate) arity: usize,
    /// Number of required parameters (lower bound of an acceptable call).
    pub(crate) required: usize,
    /// Whether the last native slot is a `&rest` list.
    pub(crate) has_rest: bool,
    /// Whether the body makes dynamic bindings (`varbind`/`unbind`). When set,
    /// [`call`](Self::call) restores the entry specpdl depth on every exit —
    /// the `cleanup_bytecode_frame` parity unwind — and requires a non-null
    /// vmctx.
    pub(crate) has_binds: bool,
    /// Precise-deopt spill buffer: a failing guard writes the live operand
    /// stack here (raw tagged bits) before returning [`STATUS_DEOPT_AT`].
    /// Untraced by design — consumed immediately after the native call
    /// returns, with no allocation in between.
    pub(crate) deopt_spill: Box<[core::cell::Cell<i64>]>,
    /// Precise-deopt pc/depth/handler-count cells (see [`DeoptCells`]).
    pub(crate) deopt_meta: Box<DeoptCells>,
    /// Per-site direct-call speculation state ([`SpecSlot`]): armed epoch +
    /// lazily-cached callee leaf pointer. Generated code holds raw pointers
    /// into this Box (stable: boxed slice, owned here, code only runs under a
    /// live Rc of this leaf).
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) spec_slots: Box<[SpecSlot]>,
    /// R2 increment B2 (AOT only): the per-site `expected` (subr/bytecode VALUE
    /// bits) array parallel to `spec_slots`, one entry per `Op::Call` spec site in
    /// slot order. AOT code loads `spec_expected_base[slot_idx]` from the sidecar
    /// instead of baking the session-specific bits; the loader ([`from_aot`]) fills
    /// it from the LIVE cell at load. Empty for JIT leaves (they bake `expected` as
    /// an `iconst`) and for AOT leaves with no armed spec site. Address-stable (a
    /// boxed slice, the sidecar's `spec_expected_base` points into it).
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) spec_expected: Box<[u64]>,
    /// Whether the body registers handler frames (`condition-case`/`catch`).
    /// When set, [`call`](Self::call) truncates `ctx.condition_stack` back to
    /// the entry depth on every exit (before the specpdl unwind, exactly like
    /// `cleanup_bytecode_frame` — no stale frame may be matchable while unbind
    /// cleanups run lisp) and requires a non-null vmctx.
    pub(crate) has_handlers: bool,
    /// If this leaf INLINED a callee, the obarray `function_epoch` armed at compile
    /// time. The dispatch (try_run_compiled / resolve_compiled_leaf_ptr) recompiles
    /// the leaf when the epoch moves — so redefining any inlined callee re-JITs and
    /// no stale inline ever runs. `None` = no inlining (never epoch-checked).
    pub(crate) inline_epoch: Option<u64>,
    /// Whether this leaf executes a SIDE EFFECT (a call) that may precede a deopt.
    /// Such a body must NEVER rerun-from-start (STATUS_DEOPT) — only precise
    /// STATUS_DEOPT_AT resume is sound, because a rerun would re-execute the side
    /// effect. Set only by the MIR tier's calls-slice (the baseline is all-precise:
    /// every guard is STATUS_DEOPT_AT, so it never reruns after a call). Guards the
    /// null-vmctx degradation in `invoke_native` (the HOLE-3 refuse-to-rerun).
    pub(crate) has_side_effects: bool,
    /// SymIds of the callees this leaf INLINED — its precise dependency set. If any
    /// is redefined, this leaf must re-JIT; the dispatch evicts it eagerly via the
    /// INLINE_DEPS reverse map (cache.rs), and the coarse inline_epoch backstop
    /// catches it lazily regardless. Empty unless the leaf inlined something.
    pub(crate) inline_deps: Box<[crate::emacs_core::intern::SymId]>,
    /// R1a: per-leaf heap-constant relocation vector. Generated code loads each
    /// heap-object constant from `reloc_data[idx]` through a baked base pointer
    /// instead of baking the tagged heap pointer as an immediate — so the code
    /// holds NO heap pointer (GC-traceable here, AOT-portable). Fixnums + non-heap
    /// immediates (nil/t) stay baked. Traced as a GC root while the leaf is cached.
    pub(crate) reloc_data: Box<[Value]>,
    /// R1c-sidecar: per-(thread,leaf) base-pointer block the AOT code reads to
    /// reach `reloc_data`/`deopt_spill`/`deopt_meta` (its pointer is passed as the
    /// 4th entry arg). `Some` for AOT leaves (built by [`from_aot`]); `None` for
    /// JIT leaves (their code bakes the bases as `iconst`, ignoring the 4th arg).
    /// A `Box` so its address is stable and its raw-pointer fields stay valid
    /// after the `CompiledLeaf` moves into the cache `Rc` (see [`LeafSidecar`]).
    pub(crate) sidecar: Option<Box<LeafSidecar>>,
    /// Number of leading constant slots this leaf loads THROUGH THE EXECUTING
    /// CALLEE (the 4th entry param carries `callee.constants.as_ptr()`) instead
    /// of baking: the source's `make-closure` patched prefix at compile time
    /// (`RuntimeState::patched_prefix`). 0 for a plain function, whose 4th
    /// param the JIT ignores. AOT leaves are never built for a patched source.
    pub(crate) dynamic_prefix: u32,
    // Field order matters for drop: `entry` points into `_backing`'s memory (the
    // JITModule's executable pages or the loaded `.so`'s code); keep `_backing`
    // alive — and dropped AFTER `entry` — as long as the handle exists.
    pub(crate) entry: *const u8,
    pub(crate) _backing: LeafBacking,
}

impl core::fmt::Debug for CompiledLeaf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `JITModule` is not `Debug`; show only the entry pointer + arity.
        f.debug_struct("CompiledLeaf")
            .field("arity", &self.arity)
            .field("entry", &self.entry)
            .finish_non_exhaustive()
    }
}

/// Outcome of executing a compiled function.
#[derive(Debug, PartialEq, Eq)]
pub enum NativeRun {
    /// Native code produced a result (the raw tagged [`Value`] bits).
    Ok(usize),
    /// A speculation guard failed. The poisoning analysis guarantees no side
    /// effect (no runtime call) ran before any guard, so the caller can safely
    /// rerun the body on the Tier-0 interpreter. (Also the null-vmctx mapping
    /// of a precise deopt: shim-free bodies are side-effect-free by
    /// construction, so rerun-from-start stays sound for them.)
    Deopt,
    /// A guard failed at a precise bytecode pc with the live operand stack
    /// and frame state captured — resume the Tier-0 interpreter MID-FUNCTION
    /// via `Vm::run_resumed_frame`. Boxed: the payload is exceptional-path
    /// only, and carrying it inline made every hot `Ok` return move an
    /// ~88-byte enum through three call boundaries.
    DeoptAt(Box<DeoptResume>),
    /// A runtime call inside the body raised a non-local `Flow` (signal/throw);
    /// take it with [`take_pending_flow`] and propagate it.
    Signal,
}

/// Payload of [`NativeRun::DeoptAt`]. The native call performed NO frame
/// unwind: `binds` (pre-push specpdl depths, this frame's JIT bind-stack
/// segment) and the `handlers` condition frames remain registered and
/// their ownership transfers to the resumed frame, which unwinds to
/// `spec_base`/`cond_base` (the native frame's entry bases) on exit.
#[derive(Debug, PartialEq, Eq)]
pub struct DeoptResume {
    pub pc: usize,
    pub stack: Vec<Value>,
    pub handlers: usize,
    pub binds: Vec<usize>,
    pub spec_base: usize,
    pub cond_base: usize,
}

pub(crate) const _: () = {
    // The hot-path contract this Box exists for: a NativeRun return must
    // stay two words.
    assert!(std::mem::size_of::<NativeRun>() <= 16);
};

impl CompiledLeaf {
    /// The number of fixed slots the native code reads (see the field doc).
    pub fn arity(&self) -> usize {
        self.arity
    }

    /// The tier that produced this leaf (see [`LeafTier`]).
    pub(crate) fn tier(&self) -> LeafTier {
        self.tier
    }

    /// The obarray `function_epoch` this leaf's inlining was armed at (`None` if it
    /// inlined nothing). The dispatch recompiles when the live epoch differs.
    pub(crate) fn inline_epoch(&self) -> Option<u64> {
        self.inline_epoch
    }

    /// The heap-object constants this leaf loads through its reloc vector (R1a).
    /// GC-traced as roots while the leaf is cached so the values stay live
    /// independent of the source function (mandatory once an AOT leaf outlives it).
    pub(crate) fn reloc_values(&self) -> &[Value] {
        &self.reloc_data
    }

    /// See the `dynamic_prefix` field: > 0 iff this leaf needs the executing
    /// callee's constant base on entry (`call_consts` & co.).
    pub(crate) fn dynamic_prefix(&self) -> usize {
        self.dynamic_prefix as usize
    }

    /// Construct a `CompiledLeaf` from a LOADED AOT unit (R1c-5).
    ///
    /// `entry` is the `dlsym`'d native entry pointing into `backing`'s `.so`
    /// (which the leaf keeps mapped via the `Arc<LoadedUnit>`); `meta` carries
    /// the lambda-list + frame flags + deopt sizing recovered from the unit's
    /// descriptor; `reloc_data` is the FRESH reloc-const vector rebuilt against
    /// the live heap (R1c-3) — already allocated-black + the caller's to root
    /// (R1c-8, via the COMPILED-walking `collect_jit_reloc_gc_roots`). The deopt
    /// buffers are allocated here, per-thread (the spec's per-thread-sidecar
    /// invariant), sized by `meta.max_depth`.
    ///
    /// SAFETY: `entry` must be the real native entry for this leaf's ABI inside
    /// `backing`'s loaded library, and `backing` must outlive every call — both
    /// guaranteed by the loader (`aot::try_load_leaf`), which verifies the
    /// ABI_TAG and dlsym's `entry` out of the same unit it stores in `backing`.
    ///
    /// R2 increment B2 — `spec_sites` (from the descriptor's spec-section, in slot
    /// order) is RE-CLASSIFIED against `obarray` (the LIVE cell at load): a site is
    /// ARMED only when re-running the same classification (`get_bytecode_data` for
    /// Bytecode, else `subr_spec_kind`) on the live binding yields the SAME
    /// discriminant baked into the code (`site.kind_disc`); otherwise it is left
    /// DISARMED (`SPEC_EPOCH_DISARMED`), so the shims' baked-kind fast paths never
    /// run against a re-aliased callee. `expected` comes from the live cell (never
    /// encoded); redefinition is handled at run time by the per-site epoch guard
    /// exactly like a fresh JIT compile. A `None` obarray (test/testkit load) leaves
    /// every site DISARMED.
    pub(crate) unsafe fn from_aot(
        entry: *const u8,
        backing: std::sync::Arc<LoadedUnit>,
        meta: AotLeafMeta,
        reloc_data: Box<[Value]>,
        spec_sites: &[super::super::aot::AotSpecSite],
        obarray: Option<&Obarray>,
    ) -> Self {
        let deopt_spill: Box<[core::cell::Cell<i64>]> = if meta.has_precise_deopt {
            (0..meta.max_depth)
                .map(|_| core::cell::Cell::new(0))
                .collect()
        } else {
            Box::from([])
        };
        let deopt_meta = Box::new(DeoptCells {
            pc: core::cell::Cell::new(0),
            depth: core::cell::Cell::new(0),
            handlers: core::cell::Cell::new(0),
        });
        // RE-CLASSIFY each `Op::Call` spec site against the LIVE obarray cell and
        // ARM (epoch = live function_epoch, expected = live cell bits) ONLY when the
        // live re-classification matches the baked discriminant; else DISARM. This
        // is the cross-session-soundness crux — a callee re-aliased to a different
        // kind (e.g. `recordp` rebound off `PredRecordp`) DISARMS rather than running
        // the wrong baked op. Built BEFORE the sidecar so the bases point at the
        // FINAL boxes (move-stable, like reloc_data / deopt_spill).
        let n_spec = spec_sites.len();
        let mut spec_slots_vec: Vec<SpecSlot> = Vec::with_capacity(n_spec);
        let mut spec_expected_vec: Vec<u64> = Vec::with_capacity(n_spec);
        for site in spec_sites {
            let (epoch, expected) = 'arm: {
                // No live obarray (test/testkit) → can't re-classify → DISARM.
                let Some(ob) = obarray else {
                    break 'arm (SPEC_EPOCH_DISARMED, 0);
                };
                // Recover the callee SymId from the leaf's OWN reloc vector (the same
                // symbol the code loads via `materialize_op_sym_id`).
                let Some(sym_id) = reloc_data
                    .get(site.callee_reloc_idx as usize)
                    .and_then(|v| v.as_symbol_id())
                else {
                    break 'arm (SPEC_EPOCH_DISARMED, 0);
                };
                // The LIVE binding at load. A now-unbound symbol → DISARM.
                let Some(cell) = ob.symbol_function_id(sym_id) else {
                    break 'arm (SPEC_EPOCH_DISARMED, 0);
                };
                // RE-CLASSIFY exactly as `find_spec_sites` did (Bytecode fast-path,
                // else the fixed-arity/Many subr classifier at the SAME nargs).
                let live_disc = if cell.is_bytecode() {
                    SpecCalleeKind::Bytecode.to_spec_disc()
                } else {
                    subr_spec_kind(cell, sym_id, site.nargs as usize).and_then(|k| k.to_spec_disc())
                };
                // ARM only on an EXACT discriminant match (never "builtin+arity").
                if live_disc == Some(site.kind_disc) {
                    (ob.function_epoch(), cell.bits() as u64)
                } else {
                    (SPEC_EPOCH_DISARMED, 0)
                }
            };
            spec_slots_vec.push(SpecSlot {
                epoch: AtomicU64::new(epoch),
                leaf: AtomicU64::new(0),
            });
            spec_expected_vec.push(expected);
        }
        let spec_slots: Box<[SpecSlot]> = spec_slots_vec.into_boxed_slice();
        let spec_expected: Box<[u64]> = spec_expected_vec.into_boxed_slice();
        // Build the sidecar from the FINAL boxes (move-stability: a Box's heap
        // pointee does not move when the owning CompiledLeaf moves into Rc::new,
        // and these boxes are never reallocated for the leaf's life — only the
        // Cells inside are mutated in place). The AOT code reads its bases from
        // here via the 4th entry arg. `reloc_base` is null when there are no heap
        // consts (empty box); the lowering only emits a reloc load when the body
        // has a heap Const, so a null base is never dereferenced. The two spec
        // bases are null when the leaf has no armed spec site (empty box); the
        // lowering only emits a spec-array load at an `Op::Call` spec site.
        let sidecar = Box::new(LeafSidecar {
            reloc_base: if reloc_data.is_empty() {
                core::ptr::null()
            } else {
                reloc_data.as_ptr()
            },
            spill_base: deopt_spill.as_ptr(),
            meta_pc: &deopt_meta.pc as *const core::cell::Cell<i64>,
            meta_depth: &deopt_meta.depth as *const core::cell::Cell<i64>,
            meta_handlers: &deopt_meta.handlers as *const core::cell::Cell<i64>,
            spec_slot_base: if spec_slots.is_empty() {
                core::ptr::null()
            } else {
                spec_slots.as_ptr()
            },
            spec_expected_base: if spec_expected.is_empty() {
                core::ptr::null()
            } else {
                spec_expected.as_ptr()
            },
        });
        CompiledLeaf {
            tier: LeafTier::Aot,
            regalloc: super::lowering::RegallocChoice::Full,
            profit_gate_bypassed: false,
            call_heavy: false,
            clif_insts: 0,
            clif_blocks: 0,
            arity: meta.arity,
            required: meta.required,
            has_rest: meta.has_rest,
            has_binds: meta.has_binds,
            has_handlers: meta.has_handlers,
            // AOT leaves never inline (no epoch staleness; never re-JIT'd).
            inline_epoch: None,
            has_side_effects: meta.has_side_effects,
            inline_deps: Box::from([]),
            spec_slots,
            spec_expected,
            deopt_spill,
            deopt_meta,
            reloc_data,
            sidecar: Some(sidecar),
            dynamic_prefix: 0,
            entry,
            _backing: LeafBacking::Aot(backing),
        }
    }

    /// The SymIds of the callees this leaf inlined (its precise dependency set).
    pub(crate) fn inline_deps(&self) -> &[crate::emacs_core::intern::SymId] {
        &self.inline_deps
    }

    /// Whether this leaf's code is backed by a loaded AOT `.so` (vs the JIT's
    /// `JITModule`). Test/diagnostic aid for proving the AOT cache path engaged.
    pub(crate) fn is_aot_backed(&self) -> bool {
        matches!(self._backing, LeafBacking::Aot(_))
    }

    /// Whether a call with `n` arguments is valid for this function's lambda
    /// list — the same predicate the interpreter's `run_frame` arity check
    /// applies before signaling `wrong-number-of-arguments`.
    pub fn accepts(&self, n: usize) -> bool {
        let nonrest = self.arity - usize::from(self.has_rest);
        self.required <= n && (self.has_rest || n <= nonrest)
    }

    /// Execute the compiled function with `args` (which must satisfy
    /// [`accepts`](Self::accepts)).
    ///
    /// The argument list is normalized to the native frame exactly as the
    /// interpreter's `run_frame` seeds it: missing `&optional` slots are
    /// nil-padded, and with `&rest` the surplus arguments become a fresh list in
    /// the final slot (allocated here, before entering native code; the caller's
    /// rooting of `args` covers the elements, as it does for `run_frame`).
    ///
    /// `vmctx` is the `*mut Context` runtime-call shims re-enter through. It may
    /// be null **only** when the body performs no runtime re-entry (it contains
    /// no `Call`); allocation (`cons`) uses the thread-local heap and tolerates
    /// a null vmctx too.
    pub fn call(&self, vmctx: *mut u8, args: &[Value]) -> NativeRun {
        self.call_consts(vmctx, core::ptr::null(), args)
    }

    /// [`call`](Self::call) with the executing callee's constant base
    /// (`callee.constants.as_ptr()`), REQUIRED when `dynamic_prefix() > 0`:
    /// the leaf loads the `make-closure`-patched slots through it. Null is
    /// accepted only for an unpatched leaf (the JIT ignores the param then).
    pub fn call_consts(&self, vmctx: *mut u8, consts: *const Value, args: &[Value]) -> NativeRun {
        debug_assert!(self.accepts(args.len()), "compiled call arity mismatch");
        // Copy the argument bits into a contiguous i64 buffer for the native
        // ABI (no heap alloc for the common <= 8 args). A `Value` is an opaque
        // tagged word here; its `usize` bits ride unchanged in an `i64` slot.
        let nonrest = self.arity - usize::from(self.has_rest);
        let mut arg_bits: SmallVec<[i64; 8]> =
            args.iter().take(nonrest).map(|v| v.bits() as i64).collect();
        // Nil-pad missing &optional parameters.
        while arg_bits.len() < nonrest {
            arg_bits.push(Value::NIL.bits() as i64);
        }
        if self.has_rest {
            let rest = if args.len() > nonrest {
                Value::list_from_slice(&args[nonrest..])
            } else {
                Value::NIL
            };
            arg_bits.push(rest.bits() as i64);
        }
        self.invoke_native(vmctx, arg_bits.as_ptr(), consts)
    }

    /// Whether a call with `nargs` arguments needs NO argument normalization
    /// (no `&optional` nil-padding, no `&rest` list construction) — the
    /// native-to-native pre-marshaled fast path applies only then.
    pub(crate) fn is_pure_passthrough(&self, nargs: usize) -> bool {
        !self.has_rest && nargs == self.arity
    }

    /// Whether the body may run WITHOUT its own [`invoke_native`] frame,
    /// called directly from the caller leaf's extent: it registers no dynamic
    /// bindings and no handler frames (nothing for the wrapper's parity
    /// unwinds to restore) and reads no sidecar (JIT leaf). Such a body
    /// behaves exactly like any runtime shim the caller invokes: a contained
    /// panic in ITS shims heals against the caller's published leaf bases —
    /// the caller's extent — which is correct because with no handler frames
    /// nothing inside it can catch, so every non-OK exit leaves the caller
    /// too. The pending-root-sweep floor is likewise swept at the caller's
    /// exit (the callee's entry floor is >= the caller's).
    pub(crate) fn direct_call_eligible(&self) -> bool {
        !self.has_binds && !self.has_handlers && self.sidecar.is_none()
    }

    /// Raw native entry invocation for [`Self::direct_call_eligible`] bodies:
    /// no bases snapshot, no `CURRENT_LEAF_BASES` publish, no bind/handler
    /// frame bookkeeping — the caller owns all of that (its own
    /// `invoke_native` published its bases; this callee runs inside that
    /// extent). The caller must route non-OK statuses through the same
    /// machinery `invoke_native` would (see `run_resolved_leaf_native`).
    ///
    /// SAFETY: same contract as [`Self::call_premarshaled`] — `args_ptr`
    /// addresses `self.arity` live tagged words, `vmctx` is the dormant
    /// seam Context.
    pub(crate) unsafe fn entry_call_raw(
        &self,
        vmctx: *mut u8,
        args_ptr: *const i64,
        out: &mut i64,
    ) -> i64 {
        unsafe { self.entry_call_raw_consts(vmctx, core::ptr::null(), args_ptr, out) }
    }

    /// [`entry_call_raw`](Self::entry_call_raw) with the executing callee's
    /// constant base (see [`call_consts`](Self::call_consts)).
    pub(crate) unsafe fn entry_call_raw_consts(
        &self,
        vmctx: *mut u8,
        consts: *const Value,
        args_ptr: *const i64,
        out: &mut i64,
    ) -> i64 {
        debug_assert!(self.direct_call_eligible());
        debug_assert!(
            self.dynamic_prefix == 0 || !consts.is_null(),
            "a dynamic-prefix leaf needs the callee's constant base"
        );
        // SAFETY: `entry` is finalized native code with the 4-param entry ABI
        // (see `invoke_native`); a JIT leaf reads the 4th param only as its
        // callee constant base, and only when it has a dynamic prefix.
        unsafe {
            let f: extern "C" fn(*mut u8, *const i64, *mut i64, *const LeafSidecar) -> i64 =
                core::mem::transmute(self.entry);
            f(
                vmctx,
                args_ptr,
                out as *mut i64,
                consts as *const LeafSidecar,
            )
        }
    }

    /// Rerun-from-start soundness assert for a direct-call STATUS_DEOPT (the
    /// same defensive rule `invoke_native` applies).
    pub(crate) fn assert_rerunnable(&self) {
        assert!(
            !self.has_side_effects,
            "side-effecting JIT leaf must use precise deopt, not rerun-from-start"
        );
    }

    /// Native-to-native fast path: invoke the body with `args_ptr` addressing
    /// EXACTLY `self.arity` pre-marshaled argument words (the caller's native
    /// call-args slot). Valid only when [`is_pure_passthrough`](Self::is_pure_passthrough)
    /// holds for the call's argument count — no nil-pad / rest-list step. Skips
    /// the `LispArgVec` build and the `arg_bits` re-marshal that [`call`](Self::call)
    /// pays, which is the per-call cost that dominates call-heavy compiled code.
    ///
    /// SAFETY: `args_ptr` must address `self.arity` valid tagged words that stay
    /// live until the native entry reads them (its first block). The spec fast
    /// path guarantees no GC safepoint runs in between: `maybe_quit` already
    /// returned `Ok` (which does not collect) and nothing allocates on a lisp
    /// heap before the entry consumes its args.
    pub(crate) fn call_premarshaled(&self, vmctx: *mut u8, args_ptr: *const i64) -> NativeRun {
        self.call_premarshaled_consts(vmctx, core::ptr::null(), args_ptr)
    }

    /// [`call_premarshaled`](Self::call_premarshaled) with the executing
    /// callee's constant base (see [`call_consts`](Self::call_consts)).
    pub(crate) fn call_premarshaled_consts(
        &self,
        vmctx: *mut u8,
        consts: *const Value,
        args_ptr: *const i64,
    ) -> NativeRun {
        debug_assert!(!vmctx.is_null(), "native-to-native requires a Context");
        self.invoke_native(vmctx, args_ptr, consts)
    }

    /// The post-marshaling tail shared by [`call`](Self::call) and
    /// [`call_premarshaled`](Self::call_premarshaled): invoke the native entry
    /// with `args_ptr` (exactly `self.arity` words) and handle the `STATUS_*`
    /// outcome — precise-deopt capture (no frame unwind, ownership transfers to
    /// the resumed interpreter frame) or the `cleanup_bytecode_frame`-parity
    /// frame unwind on a normal/signal exit.
    pub(crate) fn invoke_native(
        &self,
        vmctx: *mut u8,
        args_ptr: *const i64,
        consts: *const Value,
    ) -> NativeRun {
        debug_assert!(
            self.dynamic_prefix == 0 || !consts.is_null(),
            "a dynamic-prefix leaf needs the callee's constant base"
        );
        let mut out: i64 = 0;
        // SAFETY: `entry` is finalized native code with ABI
        // `extern "C" fn(vmctx: *mut u8, args: *const i64, out: *mut i64) -> i64`
        // (built in `lower_leaf`): it reads `self.arity` words from `args`,
        // writes the result bits through `out` and returns STATUS_OK, or returns
        // STATUS_DEOPT/STATUS_SIGNAL without touching `out`. `_module` keeps the
        // code mapped for `&self`; `arg_bits` and `out` outlive the call; for
        // arity 0 `args` is never read; `vmctx` is only dereferenced inside the
        // call shim under its own documented contract.
        // Frame-unwind bookkeeping for dynamic bindings: record the entry
        // specpdl depth and this frame's bind-stack segment base, and restore
        // both on every exit — exactly cleanup_bytecode_frame's unconditional
        // unbind_to(specpdl_base). On a deopt this is a no-op by construction
        // (varbind poisons, so no binding can precede a deopt).
        let bind_frame = if self.has_binds {
            debug_assert!(!vmctx.is_null(), "binding bodies require a Context");
            // SAFETY: the vmctx contract (dormant seam-provided Context); only
            // a length read here.
            let spec_base = unsafe { (*(vmctx as *const Context)).specpdl.len() };
            let stack_base = JIT_BIND_STACK.with(|s| s.borrow().len());
            Some((spec_base, stack_base))
        } else {
            None
        };
        let cond_base = if self.has_handlers {
            debug_assert!(!vmctx.is_null(), "handler bodies require a Context");
            // SAFETY: as above — only a length read.
            Some(unsafe { (*(vmctx as *const Context)).condition_stack_len() })
        } else {
            None
        };
        // Leaf-entry bases for contained-shim-panic healing: recorded once
        // per native call (length/scalar reads) and published for the
        // duration of the call, so the healing points — the match shim and
        // the exit path below — restore against THIS leaf's entry. Nested
        // dispatch (a callee leaf run from inside one of our shims) replaces
        // and restores the slot, and a callee's contained panic never
        // reaches us as a panic (its dispatcher materializes it into an
        // ordinary `Err(Flow)`), so the innermost-leaf value is always the
        // right one. Null vmctx (shim-free test bodies) records nothing.
        let bases = if vmctx.is_null() {
            None
        } else {
            // SAFETY: as above — length/scalar reads only.
            Some(JitLeafBases {
                snap: unsafe { (*(vmctx as *const Context)).module_boundary_snapshot() },
            })
        };
        // Publish a POINTER to the frame-resident bases (see the thread_local
        // doc): `bases` stays alive in this frame across the whole native
        // call, and the Cell is restored below before it dies.
        let bases_ptr = bases.as_ref().map(std::ptr::NonNull::from);
        let outer_bases = CURRENT_LEAF_BASES.with(|b| b.replace(bases_ptr));
        // R1c-sidecar: the unified 4-param entry ABI. AOT code reads its
        // per-thread bases from `sidecar`; JIT code declares the param but never
        // reads it (its bases are baked `iconst`s), so `null` is safe for JIT.
        // Passing the leaf's OWN sidecar from `&self` makes the base resolution
        // per-frame → reentrancy-safe (a nested call carries the callee's sidecar,
        // not this one).
        // 4th entry param: the AOT sidecar, or — for a JIT leaf — the executing
        // callee's constant base (read only by a dynamic-prefix leaf).
        let sidecar = match &self.sidecar {
            Some(b) => &**b as *const LeafSidecar,
            None => consts as *const LeafSidecar,
        };
        // Debug-only: mark this thread as inside native code for the whole
        // execution (including the cold exit tail below, which can run Lisp
        // unwind forms), so a `cache::clear()` under a live leaf asserts.
        let _native_depth = super::super::cache::NativeDepthGuard::enter();
        let mut status = unsafe {
            let f: extern "C" fn(*mut u8, *const i64, *mut i64, *const LeafSidecar) -> i64 =
                core::mem::transmute(self.entry);
            f(vmctx, args_ptr, &mut out as *mut i64, sidecar)
        };
        CURRENT_LEAF_BASES.with(|b| b.set(outer_bases));
        // Sentinel discipline: a contained panic always routes the generated
        // code to its signal path, so the marker can only be live here on a
        // STATUS_SIGNAL exit (on the match path the take already consumed it).
        debug_assert!(
            status == STATUS_SIGNAL || !shim_panic_pending(),
            "contained shim panic must exit its leaf via STATUS_SIGNAL"
        );
        // Sweep the scratch-root residue of a contained panic once the leaf
        // whose extent held it exits (any exit — a leaf-locally caught panic
        // leaves via STATUS_OK much later). A floor below our entry is an
        // outer leaf's residue: leave it set for that leaf's own exit.
        if let Some(b) = &bases
            && let Some(floor) = PENDING_ROOT_SWEEP_FLOOR.with(|f| f.get())
            && floor >= b.roots()
        {
            restore_scratch_gc_roots(b.roots());
            PENDING_ROOT_SWEEP_FLOOR.with(|f| f.set(None));
        }
        if status == STATUS_DEOPT_AT {
            return self.deopt_at_outcome(vmctx, bind_frame, cond_base);
        }
        if status == STATUS_SIGNAL || cond_base.is_some() || bind_frame.is_some() {
            // Everything below the fast path is a no-op unless a signal is
            // pending or this leaf registered frames — outlined so the hot
            // OK-exit stops paying their register spills.
            status =
                self.cold_frame_exit(vmctx, status, out, bind_frame, cond_base, bases.as_ref());
        }
        return match status {
            STATUS_OK => NativeRun::Ok(out as usize),
            STATUS_SIGNAL => NativeRun::Signal,
            _ => {
                // STATUS_DEOPT = rerun-from-start. A side-effecting body must never
                // reach here: the calls-slice routes EVERY guard in a call-bearing
                // body to precise STATUS_DEOPT_AT (the all-precise rule that closes
                // the loop-back-edge double-side-effect hole). Defensive assert.
                assert!(
                    !self.has_side_effects,
                    "side-effecting JIT leaf must use precise deopt, not rerun-from-start"
                );
                NativeRun::Deopt
            }
        };
    }

    /// Precise-deopt outcome construction — cold by definition (a failed
    /// speculation guard), outlined so `invoke_native`'s hot exit carries
    /// none of its Vec/TLS frame weight.
    #[cold]
    #[inline(never)]
    pub(crate) fn deopt_at_outcome(
        &self,
        vmctx: *mut u8,
        bind_frame: Option<(usize, usize)>,
        cond_base: Option<usize>,
    ) -> NativeRun {
        {
            // Precise deopt: NO frame unwind — the resumed interpreter frame
            // takes ownership of the registered binds/handlers and unwinds to
            // the entry bases itself on every exit. With a null vmctx (shim-
            // free test bodies — side-effect-free by construction) fall back
            // to the legacy rerun-from-start mapping.
            if vmctx.is_null() {
                // HOLE-3 refuse-to-rerun: a side-effecting body must NEVER degrade
                // to rerun-from-start — the call's side effect would re-execute.
                // The calls-slice's capability tests use a REAL Context, so this
                // never fires there; it bars any future shim-free (call_for_test)
                // path from silently double-executing a side effect.
                assert!(
                    !self.has_side_effects,
                    "side-effecting JIT leaf cannot rerun from start with a null vmctx"
                );
                return NativeRun::Deopt;
            }
            let pc = self.deopt_meta.pc.get() as usize;
            let depth = self.deopt_meta.depth.get() as usize;
            let handlers = self.deopt_meta.handlers.get() as usize;
            // No allocation happens between the native spill write and this
            // read; the caller seeds the values into the GC-traced bc_buf
            // before any elisp can run.
            let stack: Vec<Value> = (0..depth)
                .map(|j| Value::from_bits(self.deopt_spill[j].get() as usize))
                .collect();
            let binds: Vec<usize> = match bind_frame {
                Some((_, stack_base)) => JIT_BIND_STACK.with(|s| {
                    let mut s = s.borrow_mut();
                    s.split_off(stack_base)
                }),
                None => Vec::new(),
            };
            // SAFETY: dormant seam Context; length reads only.
            let spec_base = match bind_frame {
                Some((spec_base, _)) => spec_base,
                None => unsafe { (*(vmctx as *const Context)).specpdl.len() },
            };
            let cond_base = match cond_base {
                Some(base) => base,
                None => unsafe { (*(vmctx as *const Context)).condition_stack_len() },
            };
            NativeRun::DeoptAt(Box::new(DeoptResume {
                pc,
                stack,
                handlers,
                binds,
                spec_base,
                cond_base,
            }))
        }
    }

    /// Signal / frame-registered exit path of [`Self::invoke_native`],
    /// outlined (see the call site). Contents and ORDER are exactly the old
    /// inline tail: park a contained panic, heal against the leaf bases,
    /// condition-frame truncation, dynamic-binding unwind (result rooted
    /// across cleanups on OK), then un-park.
    #[cold]
    #[inline(never)]
    #[allow(clippy::too_many_arguments)] // mirrors invoke_native's locals
    pub(crate) fn cold_frame_exit(
        &self,
        vmctx: *mut u8,
        status: i64,
        out: i64,
        bind_frame: Option<(usize, usize)>,
        cond_base: Option<usize>,
        bases: Option<&JitLeafBases>,
    ) -> i64 {
        let mut effective_status = status;
        // A contained shim panic exiting this leaf (no leaf-local handler
        // matched it): heal the panicked extent's evaluator residue against
        // the leaf-entry bases BEFORE the parity unwinds below run lisp
        // (unwind-protect cleanups must not see leaked frames/drifted
        // depths). The condition floor is the entry length — the leaf is
        // dead, its own frames go too (subsuming the `cond_base` truncate
        // below for this path).
        //
        // The pending panic is PARKED (taken into a local) across those
        // parity unwinds: the leaked unwind-protect cleanups they run are
        // arbitrary lisp, possibly compiled — with the marker still set, an
        // inner leaf's `take_pending_flow` would deliver THIS panic to an
        // unrelated inner handler, discard that leaf's real flow, and leave
        // the outer dispatcher's take empty (an `.expect` panic inside
        // recovery). Any flow the panicked body stashed before panicking is
        // dropped now — the panic-wins rule applied eagerly, so both slots
        // are clean for the cleanups' own stash/take cycles. Re-stashed
        // after the unwinds; a cleanup's own contained panic is overwritten
        // then, like any second error raised while unwinding (the module
        // restore documents the same policy for cleanup signals).
        let parked_panic = if status == STATUS_SIGNAL {
            PENDING_SHIM_PANIC.with(|p| p.borrow_mut().take())
        } else {
            None
        };
        if parked_panic.is_some() {
            PENDING_FLOW.with(|p| p.borrow_mut().take());
            if let Some(b) = bases {
                // SAFETY: the native call has returned; truncations/scalar
                // writes only.
                unsafe {
                    (*(vmctx as *mut Context))
                        .restore_jit_shim_boundary(&b.snap, b.snap.condition_len());
                }
            }
        }
        // cleanup_bytecode_frame parity, same order: condition frames first
        // (the specpdl unwind below can run unwind-protect cleanups — lisp
        // that must not be able to match a stale frame of this dead body),
        // then the dynamic-binding unwind. On a deopt both are exactly what
        // makes the interpreter rerun sound: the rerun re-registers them.
        if let Some(base) = cond_base {
            // SAFETY: the native call has returned; the seam's &mut Context is
            // still dormant (we are inside its dynamic extent).
            unsafe { (*(vmctx as *mut Context)).truncate_condition_stack(base) };
        }
        if let Some((spec_base, stack_base)) = bind_frame {
            JIT_BIND_STACK.with(|s| s.borrow_mut().truncate(stack_base));
            if parked_panic.is_some() {
                // Panic is already the winning module-boundary outcome. Drain
                // every binding, but do not let cleanup Lisp replace it.
                unsafe { (*(vmctx as *mut Context)).unbind_to(spec_base) };
            } else {
                // Take any pending nonlocal exit out of TLS while cleanup Lisp
                // runs (nested compiled calls use the same slot), carry it—or
                // the successful return value—through the shared unwinder, and
                // publish the final winning cleanup flow back to the dispatcher.
                let result = match status {
                    STATUS_OK => Ok(Value::from_bits(out as usize)),
                    STATUS_SIGNAL => Err(take_pending_flow()
                        .expect("STATUS_SIGNAL frame exit must carry a pending flow")),
                    _ => Ok(Value::NIL),
                };
                let result =
                    unsafe { (*(vmctx as *mut Context)).unbind_to_with_result(spec_base, result) };
                if let Err(flow) = result {
                    stash_pending_flow(flow);
                    effective_status = STATUS_SIGNAL;
                }
            }
        }
        // Un-park the contained panic for the dispatcher's take, now that
        // the parity unwinds (and any nested leaf dispatch they ran) are
        // done with the pending slots.
        if let Some(msg) = parked_panic {
            PENDING_SHIM_PANIC.with(|p| *p.borrow_mut() = Some(msg));
        }
        effective_status
    }

    /// Test-only adapter: run with a null vmctx (valid because the test bodies
    /// using it perform no runtime re-entry through `Call`) and map the outcome
    /// to the legacy Option shape (`Ok -> Some(bits)`, `Deopt -> None`).
    /// A `Signal` panics — no shim-free test body can produce one.
    #[cfg(test)]
    pub(crate) fn call_for_test(&self, args: &[Value]) -> Option<usize> {
        match self.call(core::ptr::null_mut(), args) {
            NativeRun::Ok(bits) => Some(bits),
            NativeRun::Deopt | NativeRun::DeoptAt(_) => None,
            NativeRun::Signal => panic!("unexpected STATUS_SIGNAL from a test body"),
        }
    }
}
