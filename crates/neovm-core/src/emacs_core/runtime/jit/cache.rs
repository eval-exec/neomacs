//! Per-thread compiled-code cache and the baseline JIT tier-up entry point.
//!
//! The dispatch seam (`eval.rs`) calls [`try_run_compiled`] once a function is
//! hot ([`super::Plan::Compiled`]). Compiled code is cached **per thread**,
//! keyed by the function's stable [`super::Runtime::compiled_id`]:
//!
//! - A [`CompiledLeaf`] owns executable memory and a raw code pointer, so it is
//!   `!Send + !Sync`. Keeping it thread-local means it is never shared across
//!   threads — sound by construction, and a fine fit for elisp's overwhelmingly
//!   single-threaded execution. (Each thread that runs a function hot enough
//!   compiles its own copy; in practice that is just the main thread.)
//! - The id is monotonic and never reused, so a function that is GC'd (freeing
//!   the memory its compiled code baked constant pointers into) can never have
//!   its stale cache entry looked up again — even after the non-moving GC reuses
//!   its heap address, the new function there gets a *new* id. Stale entries for
//!   dead functions linger until thread exit (a bounded leak), never a
//!   use-after-free.

use super::compile::lowering::{RegallocChoice, RegallocPolicy, RegallocScope, forced_regalloc};
use super::compile::{CompileError, CompileRequest, compile_bytecode_function_requested};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::ReoptLevel;
use super::compile::{
    CompiledLeaf, LeafObsSnapshot, LeafTier, LeafTotals, NativeRun, stash_pending_flow,
    take_pending_flow,
};
use super::reopt::{DeoptEvent, LeafOrigin};
use super::stats;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::Value;

/// Why a body is interpreted for now although it is hot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeferReason {
    /// The profitability gate refused the body; its runtime carries the heat
    /// at which the compile is retried with the gate bypassed.
    NotProfitable,
    /// A deopt invalidated this body's leaf (`jit::reopt`): it re-profiles
    /// until its runtime's deferral heat, then recompiles with the tier the
    /// old leaf had earned.
    Reoptimize {
        regalloc: RegallocChoice,
        profit_gate_bypassed: bool,
        call_heavy: bool,
    },
}

/// One thread's knowledge of a function's compiled state.
enum CacheEntry {
    /// Native code, ready to run. `Rc` so execution can happen *outside* the
    /// cache borrow — compiled code can `Call` back into elisp, and a hot callee
    /// re-enters this cache (a nested `borrow_mut` would panic).
    Compiled(Rc<CompiledLeaf>),
    /// The body is outside the baseline JIT's supported subset; never retried.
    NotCompilable,
    /// Interpreted for now (see [`DeferReason`]); the runtime carries the
    /// heat at which the compile is retried (`RuntimeState::
    /// profit_deferred_heat`).
    Deferred(DeferReason),
}

/// Dense per-thread compiled-leaf store indexed by `compiled_id`. Ids are
/// small sequential process-uniques (`Runtime::compiled_id_or_assign`
/// fetch_adds from 1), so the id IS the index: the per-call cache probe on the
/// JIT dispatch seam becomes a bounds-checked vector load instead of a std
/// `HashMap` probe — whose SipHash alone was ~13% of a call-heavy benchmark's
/// CPU (every generic-shim call re-enters the seam and probes this cache).
#[derive(Default)]
struct DenseCache {
    slots: Vec<Option<CacheEntry>>,
    /// Evicted Compiled leaves kept alive (see `remove`).
    retired: Vec<Rc<CompiledLeaf>>,
}

impl DenseCache {
    fn get(&self, id: u64) -> Option<&CacheEntry> {
        self.slots.get(id as usize).and_then(|s| s.as_ref())
    }

    fn remove(&mut self, id: u64) {
        if let Some(slot) = self.slots.get_mut(id as usize)
            && let Some(entry) = slot.take()
        {
            // RETIRE, don't drop: a removed Compiled leaf's raw pointer may
            // still live in some caller's SpecSlot (the disjointness rules in
            // resolve_compiled_leaf_ptr / evict_inline_dependents are believed
            // to prevent that, but the compile-everything soak segfaulted on a
            // freed-leaf read - the JIT-campaign entry crash). Keeping the Rc
            // alive makes any such pointer PERMANENTLY VALID; the spec epoch
            // guard already handles behavioral staleness. Bounded: eviction is
            // rare (inlined re-JITs on redefinition). `clear()` still drops
            // everything - a heap swap invalidates slots and leaves together.
            if let CacheEntry::Compiled(leaf) = entry {
                self.retire(leaf);
            }
            // Every armed leaf slot (RuntimeState::leaf_slot) re-resolves:
            // a retired leaf stays ALLOCATED but is never CALLED again.
            bump_leaf_slot_epoch();
        }
    }

    /// Replace `id`'s entry with `next` (or empty it), RETIRING a compiled
    /// leaf that was there — like [`Self::remove`] but without the global
    /// leaf-slot epoch bump: the caller disarms the one leaf slot that can
    /// hold this source's leaf (`RuntimeState::disarm_leaf_slot`).
    fn retire_and_replace(&mut self, id: u64, next: Option<CacheEntry>) {
        let Some(slot) = self.slots.get_mut(id as usize) else {
            return;
        };
        if let Some(CacheEntry::Compiled(leaf)) = std::mem::replace(slot, next) {
            self.retire(leaf);
        }
    }

    /// Keep `leaf` allocated (and its reloc constants rooted) after it left
    /// its cache entry, marked [`CompiledLeaf::retired`].
    fn retire(&mut self, leaf: Rc<CompiledLeaf>) {
        leaf.retired.set(true);
        self.retired.push(leaf);
    }

    fn insert(&mut self, id: u64, entry: CacheEntry) {
        record_compiled_heap();
        let idx = id as usize;
        if self.slots.len() <= idx {
            self.slots.resize_with(idx + 1, || None);
        }
        if let Some(CacheEntry::Compiled(old)) = self.slots[idx].replace(entry) {
            fold_dropped_leaf(&old);
        }
    }

    /// `entry(id).or_insert_with(f)` equivalent.
    fn get_or_insert_with(&mut self, id: u64, f: impl FnOnce() -> CacheEntry) -> &mut CacheEntry {
        let idx = id as usize;
        if self.slots.len() <= idx {
            self.slots.resize_with(idx + 1, || None);
        }
        let slot = &mut self.slots[idx];
        if slot.is_none() {
            record_compiled_heap();
            *slot = Some(f());
        }
        slot.as_mut().expect("slot just filled")
    }

    /// Occupied entries (id, entry) — skips empty slots.
    fn iter(&self) -> impl Iterator<Item = (u64, &CacheEntry)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|e| (i as u64, e)))
    }

    fn values(&self) -> impl Iterator<Item = &CacheEntry> {
        self.slots.iter().filter_map(|s| s.as_ref())
    }

    /// Occupied-entry count (the HashMap `len()` this replaced).
    fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    fn clear(&mut self) {
        for entry in self.slots.iter().flatten() {
            if let CacheEntry::Compiled(leaf) = entry {
                fold_dropped_leaf(leaf);
            }
        }
        for leaf in &self.retired {
            fold_dropped_leaf(leaf);
        }
        self.slots.clear();
        self.retired.clear();
        bump_leaf_slot_epoch();
    }
}

thread_local! {
    /// Summed observability counters of the leaves this thread's caches
    /// dropped (heap-swap `clear`, OSR eviction), so the exit report's
    /// totals still include them. Cold: touched only when leaves are dropped.
    static DROPPED_LEAF_TOTALS: std::cell::Cell<LeafTotals> =
        const { std::cell::Cell::new(LeafTotals { leaves: 0, entries: 0, deopt_at: 0, deopt_rerun: 0, signals: 0 }) };
}

/// Fold a leaf that is about to leave every cache into the dropped totals.
#[cold]
fn fold_dropped_leaf(leaf: &CompiledLeaf) {
    let snap = leaf.obs.snapshot();
    DROPPED_LEAF_TOTALS.with(|t| {
        let mut totals = t.get();
        totals.add(&snap);
        t.set(totals);
    });
}

/// Where a reported leaf sits in this thread's caches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub(crate) enum LeafState {
    /// The current leaf of its `compiled_id`.
    Live,
    /// Replaced (re-tier, stale inline, patched-prefix widening) but kept
    /// allocated for any spec slot still pointing at it.
    Retired,
    /// An OSR variant, entered only from the interpreter.
    Osr,
}

/// One leaf's line in the exit report (read at exit, on the owning thread).
#[derive(Clone, Debug)]
pub(crate) struct LeafRow {
    pub(crate) id: u64,
    /// The compile-time label (`lisp:<fn>#<id>:<tier>`), when naming was on.
    pub(crate) label: Option<Box<str>>,
    pub(crate) tier: LeafTier,
    pub(crate) state: LeafState,
    pub(crate) regalloc: RegallocChoice,
    pub(crate) clif_insts: u32,
    pub(crate) obs: LeafObsSnapshot,
}

impl LeafRow {
    fn of(id: u64, leaf: &CompiledLeaf, state: LeafState) -> Self {
        LeafRow {
            id,
            label: leaf.obs.label.clone(),
            tier: leaf.tier(),
            state,
            regalloc: leaf.regalloc,
            clif_insts: leaf.clif_insts,
            obs: leaf.obs.snapshot(),
        }
    }
}

/// Every leaf this thread still holds (live, retired, OSR), plus the summed
/// counters of the leaves it dropped. Report-time only.
pub(crate) fn leaf_report_rows() -> (Vec<LeafRow>, LeafTotals) {
    let mut rows = Vec::new();
    COMPILED.with(|c| {
        let cache = c.borrow();
        for (id, entry) in cache.iter() {
            if let CacheEntry::Compiled(leaf) = entry {
                rows.push(LeafRow::of(id, leaf, LeafState::Live));
            }
        }
        for leaf in &cache.retired {
            rows.push(LeafRow::of(leaf.obs.id, leaf, LeafState::Retired));
        }
    });
    OSR_CACHE.with(|c| {
        for (&(id, _), entry) in c.borrow().iter() {
            if let Some(entry) = entry {
                rows.push(LeafRow::of(id, &entry.leaf, LeafState::Osr));
            }
        }
    });
    rows.sort_by_key(|r| (r.id, r.obs.osr_pc));
    (rows, DROPPED_LEAF_TOTALS.with(std::cell::Cell::get))
}

/// Validity epoch of every `RuntimeState::leaf_slot`: bumped whenever a
/// compiled entry leaves a cache (retire or clear), so a slot armed earlier
/// reads as empty and re-resolves through the cache. Starts at 1 so a fresh
/// slot (epoch 0) never matches.
static LEAF_SLOT_EPOCH: AtomicU64 = AtomicU64::new(1);

#[inline]
pub(crate) fn leaf_slot_epoch() -> u64 {
    LEAF_SLOT_EPOCH.load(Ordering::Relaxed)
}

fn bump_leaf_slot_epoch() {
    LEAF_SLOT_EPOCH.fetch_add(1, Ordering::Relaxed);
}

#[derive(Clone)]
struct OsrEntry {
    leaf: Rc<CompiledLeaf>,
    stack_depth: usize,
    bind_depth: usize,
}

thread_local! {
    /// `compiled_id` -> compiled state, owned by and private to this thread.
    static COMPILED: RefCell<DenseCache> = RefCell::new(DenseCache::default());

    /// Precise inline-dependency REVERSE map: callee `SymId` -> the set of caller
    /// `compiled_id`s that INLINED it. Populated at compile-miss (the `or_insert_with`
    /// closures register `leaf.inline_deps()`); consulted by `evict_inline_dependents`
    /// when a function is redefined, to evict exactly the affected callers. It is
    /// the only invalidation of inlined callees: every function-cell write reaches
    /// `evict_inline_dependents`.
    /// Same thread/scope as COMPILED (its values are only meaningful as COMPILED keys).
    static INLINE_DEPS: RefCell<HashMap<SymId, HashSet<u64>>> = RefCell::new(HashMap::default());

    /// How many times redefining a symbol evicted callers that had inlined it.
    /// A callee redefined over and over (a `cl-letf' in a loop, a mock
    /// reinstalled per test) would otherwise recompile its hot callers on
    /// every round; past [`UNSTABLE_INLINE_EVICTIONS`] it is not inlined again.
    static INLINE_EVICTIONS: RefCell<HashMap<SymId, u32>> = RefCell::new(HashMap::default());

    /// The tagged-heap identity the cached leaves were compiled against. The JIT
    /// cache is thread-local, but every leaf's reloc vector + baked addresses
    /// reference the heap live at compile time. If the thread's heap is replaced
    /// (a pdump load / in-process image reload / cache-replay test), the whole
    /// cache is stale — detected lazily by identity in `sync_cache_to_current_heap`
    /// and cleared before any stale reloc value is traced or run. Pinned by the
    /// first insert (`record_compiled_heap`), so `None` means "no leaf cached"
    /// and is adopted, never cleared on.
    static COMPILED_HEAP: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };

    /// The [`Obarray::generation`] the cached leaves were compiled against,
    /// pinned by the first compile that had an obarray (like `COMPILED_HEAP`,
    /// `None` means no leaf was built against one yet). A leaf may keep
    /// obarray-internal addresses; `sync_cache_to_obarray` drops the whole
    /// cache if the obarray it runs against is not this one.
    static COMPILED_OBARRAY: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };

    /// OSR (on-stack replacement) leaves, keyed by `(compiled_id, osr_pc)`. The
    /// value is `Some(OsrEntry)` when the function is OSR-eligible +
    /// compiled at that loop header, `None` when it is ineligible/uncompilable (a
    /// negative cache, so a hot loop that can't OSR is probed only once). Same
    /// thread/scope as COMPILED; cleared alongside it (heap-identity + `clear`).
    // The nested shape is private and directly documents positive/negative OSR
    // cache entries plus their operand and binding depths.
    #[allow(clippy::type_complexity)]
    static OSR_CACHE: RefCell<HashMap<(u64, usize), Option<OsrEntry>>> =
        RefCell::new(HashMap::default());
}

#[cfg(debug_assertions)]
thread_local! {
    /// Nesting depth of native leaf executions on this thread (debug builds
    /// only). `clear()` asserts it is 0: the soundness of every spec-slot leaf
    /// pointer and every baked box address rests on "no clear() fires while a
    /// native frame is live" (`resolve_compiled_leaf_ptr`), so a violation must
    /// be loud in debug/test builds instead of a silent use-after-free.
    static NATIVE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Current native nesting depth (0 outside any leaf). Always 0 in release
/// builds, where the counter does not exist.
#[inline]
pub(crate) fn native_depth() -> u32 {
    #[cfg(debug_assertions)]
    {
        NATIVE_DEPTH.with(|d| d.get())
    }
    #[cfg(not(debug_assertions))]
    {
        0
    }
}

/// RAII marker for one native leaf execution (see `NATIVE_DEPTH`). Zero-sized
/// and a no-op in release builds; unwind-safe (the decrement is in `Drop`).
pub(crate) struct NativeDepthGuard(());

impl NativeDepthGuard {
    #[inline]
    pub(crate) fn enter() -> Self {
        #[cfg(debug_assertions)]
        NATIVE_DEPTH.with(|d| d.set(d.get() + 1));
        Self(())
    }
}

impl Drop for NativeDepthGuard {
    #[inline]
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        NATIVE_DEPTH.with(|d| d.set(d.get() - 1));
    }
}

/// Save records, unwind-protect and handler frames are not transferred by OSR.
/// Lexical bodies may use VarBind/Unbind: the entry validates and borrows the VM's
/// live binding stack, and a precise deopt returns its evolved state.
fn osr_body_has_unsupported_state(func: &ByteCodeFunction) -> bool {
    use crate::emacs_core::bytecode::Op;
    func.executable_ops().iter().any(|op| {
        matches!(
            op,
            Op::SaveExcursion
                | Op::SaveRestriction
                | Op::SaveCurrentBuffer
                | Op::SaveWindowExcursion
                | Op::UnwindProtectPop
                | Op::PushCatch(_)
                | Op::PushConditionCase(_)
                | Op::PushConditionCaseRaw(_)
        )
    })
}

/// Compile (once) the OSR variant of `func` entered at `osr_pc`, or `None` when
/// `func` is not OSR-eligible or the body doesn't compile. Eligibility: lexical
/// (params on the operand stack, so the seeded snapshot carries them), no
/// handler/save ops, and the loop header has well-defined operand and binding
/// depths with no active handlers. Both depths are checked against the live VM.
fn compile_osr_leaf(
    obarray: &Obarray,
    func: &ByteCodeFunction,
    osr_pc: usize,
    id: u64,
    name_hint: Option<SymId>,
) -> Option<OsrEntry> {
    record_compiled_obarray(Some(obarray));
    let clock = stats::CompileClock::start(stats::CompileOrigin::Osr);
    let entry = compile_osr_leaf_timed(obarray, func, osr_pc, id, name_hint);
    let elapsed = clock.finish(entry.is_some());
    if let Some(entry) = &entry {
        entry.leaf.obs.note_compile_time(elapsed);
    }
    entry
}

/// [`compile_osr_leaf`]'s body, inside its compile clock.
fn compile_osr_leaf_timed(
    obarray: &Obarray,
    func: &ByteCodeFunction,
    osr_pc: usize,
    id: u64,
    name_hint: Option<SymId>,
) -> Option<OsrEntry> {
    let gate_phase = stats::enter_phase(stats::CompilePhase::Gate);
    let dbg = std::env::var_os("NEOMACS_OSR_DEBUG").is_some();
    // Deopt reoptimization gave up on this source: no native code at all.
    if func.jit_runtime().reopt_level() == ReoptLevel::Interpreter {
        if dbg {
            eprintln!("OSR_DEBUG reject: reopt level interpreter");
        }
        return None;
    }
    if !func.lexical || osr_body_has_unsupported_state(func) {
        if dbg {
            eprintln!(
                "OSR_DEBUG reject pre: lexical={} dynstate={}",
                func.lexical,
                osr_body_has_unsupported_state(func)
            );
        }
        return None;
    }
    let ops = func.executable_ops();
    let native_arity = func.params.required.len()
        + func.params.optional.len()
        + usize::from(func.params.rest.is_some());
    let offset_map = func.executable_gnu_byte_offset_map();
    let cfg = match super::compile::analyze_cfg(ops, &func.constants, offset_map, native_arity) {
        Ok(cfg) => cfg,
        Err(e) => {
            if dbg {
                eprintln!("OSR_DEBUG reject analyze_cfg: {e:?} ops={}", ops.len());
            }
            return None;
        }
    };
    // The loop header must be a real block boundary with a known entry depth and
    // no active handler frames (belt-and-suspenders with the op scan).
    let Some(&entry_depth) = cfg.entry_depth.get(&osr_pc) else {
        if dbg {
            eprintln!("OSR_DEBUG reject: no entry depth at pc {osr_pc}");
        }
        return None;
    };
    let &bind_depth = cfg.entry_binds.get(&osr_pc)?;
    if !cfg.entry_handlers.get(&osr_pc).is_none_or(|h| h.is_empty()) {
        if dbg {
            eprintln!("OSR_DEBUG reject: handlers live at pc {osr_pc}");
        }
        return None;
    }
    // A loop that is already running gets the full allocator: its work per
    // entry is unbounded, so the code quality is worth the compile.
    let _regalloc = RegallocScope::enter(super::compile::regalloc_for(
        RegallocPolicy::Full,
        ops,
        &func.constants,
    ));
    // Same feedback the tier-up compile sees: without it every Float site
    // read FixnumOnly and an OSR'd float loop deopted straight back.
    let _numeric = super::compile::publish_numeric_feedback(func);
    let _label = stats::naming_enabled()
        .then(|| stats::perf_map::LeafLabelScope::enter(id, name_hint, func));
    drop(gate_phase);
    let lower_phase = stats::enter_phase(stats::CompilePhase::Lower);
    let mut leaf = match super::compile::lower_leaf_full_osr(
        ops,
        &func.constants,
        native_arity,
        offset_map,
        Some(obarray),
        Some(osr_pc),
        func.jit_runtime().patched_prefix(),
    ) {
        Ok(leaf) => leaf,
        Err(e) => {
            if dbg {
                eprintln!("OSR_DEBUG reject lower: {e:?}");
            }
            return None;
        }
    };
    drop(lower_phase);
    if dbg {
        eprintln!("OSR_DEBUG compiled: pc={osr_pc} depth={entry_depth} binds={bind_depth}");
    }
    leaf.obs.id = id;
    leaf.obs.osr_pc = u32::try_from(osr_pc).ok();
    leaf.compiled_level = func.jit_runtime().reopt_level();
    Some(OsrEntry {
        leaf: Rc::new(leaf),
        stack_depth: entry_depth,
        bind_depth,
    })
}

/// OSR (on-stack replacement) dispatch: transfer a hot loop in `func` into native
/// code at loop-header `osr_pc`, given the live operand-stack snapshot `stack`
/// (bottom = the frame base). Returns `None` when OSR does not apply — ineligible
/// function, uncompilable body, or operand/binding depths that differ from the
/// compiled entry (a non-balanced back-edge; never transfer then). Otherwise the
/// [`NativeRun`] from the native OSR entry: `Ok(bits)` = the function completed
/// (its result); `Signal`/`Deopt*` = the interpreter handles the outcome. The OSR
/// variant is compiled once and cached (positively or negatively) per
/// `(func, osr_pc)`.
///
/// # Safety
/// `ctx` follows the same dormant-Context contract as [`try_run_compiled`].
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub(crate) fn try_run_osr(
    ctx: *mut Context,
    func: &ByteCodeFunction,
    osr_pc: usize,
    stack: &[Value],
    binds: &[usize],
) -> Option<NativeRun> {
    if ctx.is_null() {
        return None;
    }
    sync_cache_to_current_heap();
    let id = func.jit_runtime().compiled_id_or_assign();
    // SAFETY: dormant seam-provided Context (as try_run_compiled); shared obarray read.
    let obarray = unsafe { &(*ctx).obarray };
    let cached = OSR_CACHE.with(|c| {
        c.borrow_mut()
            .entry((id, osr_pc))
            .or_insert_with(|| {
                // The running function's own frame is the innermost one.
                let name_hint = stats::naming_enabled()
                    .then(|| callee_name_hint(ctx, id))
                    .flatten();
                compile_osr_leaf(obarray, func, osr_pc, id, name_hint)
            })
            .clone()
    });
    let OsrEntry {
        leaf,
        stack_depth: entry_depth,
        bind_depth,
    } = cached?;
    // Only transfer when the live snapshot is exactly the header's entry stack.
    if stack.len() != entry_depth || binds.len() != bind_depth {
        if std::env::var_os("NEOMACS_OSR_DEBUG").is_some() {
            eprintln!(
                "OSR_DEBUG no-transfer: pc={osr_pc} snapshot depth {} / binds {} != entry {entry_depth} / {bind_depth}",
                stack.len(),
                binds.len()
            );
        }
        return None;
    }
    let arg_bits: Vec<i64> = stack.iter().map(|v| v.bits() as i64).collect();
    // Lend a COPY of the live interpreter binding stack. Its outer JIT prefix
    // belongs to suspended native callers and must survive this entire transfer.
    // No Lisp or GC can run between this seed and the native entry.
    let bind_frame = if leaf.has_binds {
        // SAFETY: dormant seam-provided Context; length reads and Vec writes only.
        let ctx = unsafe { &mut *ctx };
        let spec_base = ctx.specpdl.len();
        if binds.last().is_some_and(|&base| base >= spec_base)
            || binds.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return None;
        }
        let stack_base = ctx.jit_bind_stack.len();
        ctx.jit_bind_stack.extend_from_slice(binds);
        Some((spec_base, stack_base))
    } else {
        debug_assert!(binds.is_empty());
        None
    };
    OSR_TRANSFER_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let run = leaf.invoke_osr(
        ctx as *mut u8,
        arg_bits.as_ptr(),
        func.constants.as_ptr(),
        bind_frame,
    );
    if let Some((_, stack_base)) = bind_frame {
        // SAFETY: native execution has returned; only a dormant Context length read.
        debug_assert_eq!(unsafe { (*ctx).jit_bind_stack.len() }, stack_base);
    }
    // The deopt hook runs before the interpreter installs the resume state
    // (the local `leaf` keeps the OSR leaf alive across it).
    let _verdict = match &run {
        NativeRun::Deopt => super::reopt::note_deopt(
            ctx,
            func,
            &leaf,
            LeafOrigin::Osr {
                header_pc: osr_pc,
                snapshot: stack,
            },
            DeoptEvent::Rerun,
        ),
        NativeRun::DeoptAt(resume) => super::reopt::note_deopt(
            ctx,
            func,
            &leaf,
            LeafOrigin::Osr {
                header_pc: osr_pc,
                snapshot: stack,
            },
            DeoptEvent::Precise {
                pc: resume.pc,
                stack: &resume.stack,
            },
        ),
        NativeRun::Ok(_) | NativeRun::Signal => super::reopt::ReoptVerdict::Kept,
    };
    if std::env::var_os("NEOMACS_OSR_DEBUG").is_some() {
        let tag = match &run {
            NativeRun::Ok(_) => "ok",
            NativeRun::Deopt => "deopt",
            NativeRun::DeoptAt(_) => "deopt_at",
            NativeRun::Signal => "signal",
        };
        eprintln!("OSR_DEBUG transfer outcome: pc={osr_pc} {tag}");
    }
    Some(run)
}

/// Count of OSR transfers actually taken (a native OSR entry was invoked). Lets
/// tests prove the transfer fired rather than the interpreter finishing the loop.
pub(crate) static OSR_TRANSFER_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Register a freshly-compiled leaf's inlined-callee deps into the reverse map.
/// Called ONLY from the cache compile-miss path ([`compile_cache_entry`]), so
/// it runs once per compile, never on the hot dispatch path.
fn register_inline_deps(id: u64, leaf: &CompiledLeaf) {
    // A recompiled id (re-tier, stale inline, deferred re-attempt) may no
    // longer inline what its previous leaf did: forget the old membership
    // first, or a later redefinition finds a non-inlined leaf in a dep set
    // (the `evict_inline_dependents` invariant; tripped by the 2026-09-05
    // gate-off census run).
    forget_inline_deps(id);
    for &sym in leaf.inline_deps() {
        INLINE_DEPS.with(|m| m.borrow_mut().entry(sym).or_default().insert(id));
    }
}

/// Drop `id` from every inline-dependency set (empty sets are removed).
fn forget_inline_deps(id: u64) {
    INLINE_DEPS.with(|m| {
        let mut m = m.borrow_mut();
        m.retain(|_, ids| {
            ids.remove(&id);
            !ids.is_empty()
        });
    });
}

/// Generation of NotCompilable verdicts. `RuntimeState::mark_native_rejected`
/// stamps a rejected function with the current value and the tier dispatcher
/// answers `Interpret` for it without probing this cache while the stamp is
/// current; [`clear`] bumps it, so a verdict is forgotten exactly when the
/// cache itself forgets (a heap change) and the next call re-consults the
/// cache as before. Process-global on purpose: a clear on any thread makes
/// every thread re-probe, which only ever re-finds `NotCompilable`.
static REJECTION_EPOCH: AtomicU64 = AtomicU64::new(1);

/// The current NotCompilable generation (see [`REJECTION_EPOCH`]).
pub(crate) fn rejection_epoch() -> u64 {
    REJECTION_EPOCH.load(Ordering::Relaxed)
}

/// The cache-miss JIT compile — the synchronous eval-thread stall this tier
/// pays once per function. Meters the stall into [`stats`] (timing ONLY the
/// compile call; an AOT-served miss never reaches here), records the precise
/// inline deps on success (so a later redefinition of an inlined callee evicts
/// this leaf), and maps the outcome to a [`CacheEntry`]. Runs solely from the
/// `or_insert_with` closures, never on the hot dispatch path.
/// `NEOVM_JIT_NUMERIC_STATS=1`: what the arithmetic sites of a body being
/// compiled have actually seen. The lowering guards fixnum and deopts on
/// anything else, so a body whose sites are all `Float` or `Other` is being
/// compiled into a guaranteed bail — this is how to tell which is which.
#[cold]
fn numeric_feedback_trace(id: u64, func: &ByteCodeFunction) {
    use crate::emacs_core::jit::NumericFeedback;
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    if !*ON.get_or_init(|| std::env::var("NEOVM_JIT_NUMERIC_STATS").as_deref() == Ok("1")) {
        return;
    }
    let rt = func.jit_runtime();
    let ops_len = func.executable_ops().len();
    let (mut fixnum, mut float, mut other) = (0u32, 0u32, 0u32);
    for pc in 0..ops_len {
        match rt.numeric_feedback(pc) {
            NumericFeedback::FixnumOnly => fixnum += 1,
            NumericFeedback::Float => float += 1,
            NumericFeedback::Other => other += 1,
        }
    }
    eprintln!(
        "[neovm-jit-numeric] compiling id={id} ops={ops_len} fixnum-or-unrun={fixnum} float={float} other={other}"
    );
}

fn compile_cache_entry(
    id: u64,
    func: &ByteCodeFunction,
    obarray: Option<&Obarray>,
    request: CompileRequest,
    name_hint: Option<SymId>,
) -> CacheEntry {
    // Deopt reoptimization gave up on this source (`ReoptLevel::Interpreter`):
    // remember the verdict like any rejected body. Also re-applies the level
    // after a `clear()` forgot the entry.
    let rt = func.jit_runtime();
    if rt.reopt_level() == ReoptLevel::Interpreter {
        rt.mark_native_rejected(rejection_epoch());
        return CacheEntry::NotCompilable;
    }
    numeric_feedback_trace(id, func);
    record_compiled_obarray(obarray);
    // Per-function entry names (perf map, CLIF/asm dumps), only when asked.
    let _label = stats::naming_enabled()
        .then(|| stats::perf_map::LeafLabelScope::enter(id, name_hint, func));
    let clock = stats::CompileClock::start(request.origin);
    let result = compile_bytecode_function_requested(func, obarray, request);
    let elapsed = clock.finish(result.is_ok());
    stats::record_compile(elapsed, func.executable_ops().len(), &result);
    match result {
        Ok(mut leaf) => {
            leaf.obs.id = id;
            leaf.compiled_level = rt.reopt_level();
            leaf.obs.note_compile_time(elapsed);
            register_inline_deps(id, &leaf);
            CacheEntry::Compiled(Rc::new(leaf))
        }
        Err(CompileError::NotProfitable) if super::profit_defer_factor() != 0 => {
            // Not a verdict, a deferral: come back once the body has the
            // calls to amortize its compile (see `profit_deferred_heat`).
            let rt = func.jit_runtime();
            let at = super::hot_threshold()
                .saturating_mul(super::profit_defer_factor())
                .max(rt.heat().saturating_add(1));
            rt.defer_tier_up(at);
            CacheEntry::Deferred(DeferReason::NotProfitable)
        }
        Err(_) => {
            // Remember the verdict where the dispatcher can see it. Measured
            // on the org editing probe (callgrind, 2026-09-05): 18 of the 21
            // functions that reached tier-up were rejected, and their ~80,000
            // later hot calls each still took the whole seam trip — argument
            // copy, scratch-root save/restore, cache probe, fallback — to be
            // told `NotCompilable` again; that trip was the bulk of the JIT's
            // +1.0% instruction tax on that workload.
            func.jit_runtime().mark_native_rejected(rejection_epoch());
            CacheEntry::NotCompilable
        }
    }
}

/// Precise invalidation: function `sym` was just redefined — evict the JIT cache
/// entries of every caller that INLINED it, so each re-JITs against the new
/// definition on its next call, while unrelated callers stay cached.
///
/// MUST be called OUTSIDE any `COMPILED`/`INLINE_DEPS` borrow (the redefinition path
/// in symbol.rs is) — it takes the two thread_local borrows itself, separately and
/// briefly. Idempotent: an absent/already-evicted id is a no-op. Compiled-id never
/// reuses, so a stale id in a dep set just removes nothing.
/// Evict `id`'s compiled state (its leaf is RETIRED, not dropped — see
/// `DenseCache::remove` — and its OSR entries are dropped, which is sound: an OSR
/// leaf is only ever entered from the interpreter, never cached in a spec slot).
/// Used when a `make-closure` widened the source's patched prefix after a leaf
/// was compiled under the narrower one (`RuntimeState::note_patched_prefix`).
/// The symbol naming the function whose leaf `id` is about to be compiled,
/// for its perf-map label: the speculated callee a spec site is resolving,
/// else the function of the innermost backtrace frame (both tier-up seams
/// push the callee's frame before dispatching). Accepted only when that
/// symbol's function is the bytecode with this very `compiled_id`, so a
/// wrong frame falls back to an anonymous label. Read-only: no interning,
/// no allocation on the Lisp heap. Cold, and only called under naming.
#[cold]
#[inline(never)]
fn callee_name_hint(ctx: *const Context, id: u64) -> Option<SymId> {
    let pending = stats::perf_map::take_pending_callee();
    if ctx.is_null() {
        return None;
    }
    // SAFETY: the dormant seam-provided Context (the same contract as
    // try_run_compiled); shared reads of the specpdl and obarray only.
    let ctx = unsafe { &*ctx };
    let names_this_leaf = |sym: SymId| {
        ctx.obarray
            .indirect_function_id(sym)
            .and_then(Value::bytecode_data_if_materialized)
            .is_some_and(|bc| bc.jit_runtime().compiled_id() == Some(id))
    };
    if let Some(sym) = pending
        && names_this_leaf(sym)
    {
        return Some(sym);
    }
    let sym = ctx.innermost_backtrace_function(64)?.as_symbol_id()?;
    names_this_leaf(sym).then_some(sym)
}

pub(crate) fn evict_compiled(id: u64) {
    COMPILED.with(|c| c.borrow_mut().remove(id));
    OSR_CACHE.with(|c| {
        c.borrow_mut().retain(|(fid, _), entry| {
            let keep = *fid != id;
            if !keep && let Some(entry) = entry {
                fold_dropped_leaf(&entry.leaf);
            }
            keep
        })
    });
}

/// Point every caller on this thread that caches `dead` in a spec slot back
/// at the cache: clear each BYTECODE-kind spec slot of every live, retired
/// and OSR leaf whose cached callee leaf is `dead` (see
/// [`CompiledLeaf::unlink_spec_slots_to`]). Returns how many slots it
/// cleared. The next call through such a slot re-resolves its callee
/// (`call_spec_slow` -> `call_armed_callee_native` ->
/// `resolve_compiled_leaf_ptr`), which finds whatever the cache now holds.
///
/// O(spec slots on the thread): about 10K relaxed loads for 2,000 leaves of
/// 5 slots each. Used where a leaf stops being current: the deopt
/// invalidation (`jit::reopt`), the re-tier (`try_run_compiled`) and, later,
/// a per-symbol resync. Must run outside any `COMPILED`/`OSR_CACHE` borrow.
pub(crate) fn unlink_spec_slots(dead: *const CompiledLeaf) -> usize {
    let mut cleared = 0;
    COMPILED.with(|c| {
        let cache = c.borrow();
        for entry in cache.values() {
            if let CacheEntry::Compiled(leaf) = entry {
                cleared += leaf.unlink_spec_slots_to(dead);
            }
        }
        for leaf in &cache.retired {
            cleared += leaf.unlink_spec_slots_to(dead);
        }
    });
    OSR_CACHE.with(|c| {
        for entry in c.borrow().values().flatten() {
            cleared += entry.leaf.unlink_spec_slots_to(dead);
        }
    });
    cleared
}

thread_local! {
    /// Function objects a redefinition took out of a symbol's function cell,
    /// with the symbol and the identity of the heap they live in (see
    /// [`pin_redefined_function`]).
    static REDEFINED_PINS: RefCell<Vec<RedefinedPin>> = const { RefCell::new(Vec::new()) };
}

/// One pinned function: see [`pin_redefined_function`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct RedefinedPin {
    pub(crate) heap: usize,
    pub(crate) sym: SymId,
    pub(crate) function: Value,
}

/// `sym`'s function cell stopped holding `previous` (`fset`, `defalias`,
/// `fmakunbound`, advice, unintern). A compiled activation called through
/// `sym` may still be running `previous`, and its backtrace frame records
/// the symbol, as GNU's `Bcall` does -- not the object, which GNU's bytecode
/// frame keeps alive (`fp->fun`, src/bytecode.c:518) and which a deopt
/// resume or an interpreter rerun of that activation reads. So a
/// byte-code `previous` stays a GC root while any backtrace frame records
/// `sym` ([`trace_redefined_pins`]); with none, nothing can be running it
/// through `sym`, and the next collection drops the pin. Cold: one push per
/// redefinition of a byte-code function, none for a pair already pinned.
pub(crate) fn pin_redefined_function(sym: SymId, previous: Value) {
    if !previous.is_bytecode() {
        return;
    }
    let Some(heap) = crate::tagged::gc::current_tagged_heap_identity() else {
        return;
    };
    REDEFINED_PINS.with(|pins| {
        let mut pins = pins.borrow_mut();
        if !pins
            .iter()
            .any(|pin| pin.heap == heap && pin.sym == sym && pin.function.bits() == previous.bits())
        {
            pins.push(RedefinedPin {
                heap,
                sym,
                function: previous,
            });
        }
    });
}

/// Whether any redefined function is pinned (the root walk's fast exit).
pub(crate) fn has_redefined_pins() -> bool {
    REDEFINED_PINS.with(|pins| !pins.borrow().is_empty())
}

/// Root walk of the heap `heap`: visit every function pinned in it whose
/// symbol a live backtrace frame records (`frame_calls`), and drop the
/// others -- another heap's pins too, which no frame of this one can need.
pub(crate) fn trace_redefined_pins(
    heap: usize,
    frame_calls: &dyn Fn(SymId) -> bool,
    visit: &mut dyn FnMut(Value),
) {
    REDEFINED_PINS.with(|pins| {
        pins.borrow_mut().retain(|pin| {
            let live = pin.heap == heap && frame_calls(pin.sym);
            if live {
                visit(pin.function);
            }
            live
        });
    });
}

/// The pinned functions (tests).
#[cfg(test)]
pub(crate) fn redefined_pins_for_test() -> Vec<RedefinedPin> {
    REDEFINED_PINS.with(|pins| pins.borrow().clone())
}

/// Whether `leaf` is still the leaf the caches hold for `func` at `origin`:
/// the entry leaf of its `compiled_id`, or the OSR leaf at its header. A
/// retired leaf never is.
pub(crate) fn leaf_is_current(
    func: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    origin: LeafOrigin<'_>,
) -> bool {
    if leaf.retired.get() {
        return false;
    }
    let Some(id) = func.jit_runtime().compiled_id() else {
        return false;
    };
    match origin {
        LeafOrigin::Entry => COMPILED.with(|c| {
            c.try_borrow().is_ok_and(|c| {
                matches!(c.get(id), Some(CacheEntry::Compiled(l)) if std::ptr::eq(Rc::as_ptr(l), leaf))
            })
        }),
        LeafOrigin::Osr { header_pc, .. } => OSR_CACHE.with(|c| {
            c.try_borrow().is_ok_and(|c| {
                matches!(c.get(&(id, header_pc)), Some(Some(e)) if std::ptr::eq(Rc::as_ptr(&e.leaf), leaf))
            })
        }),
    }
}

/// Whether `(func, osr_pc)` has an OSR cache entry (positive or negative) on
/// this thread. `Vm::osr_transfer` asks after a transfer whose precise deopt
/// it installed in place: an entry the deopt's invalidation dropped means
/// the next hot back-edge may transfer again, into a leaf compiled from the
/// widened feedback, instead of latching the loop onto the interpreter.
pub(crate) fn osr_entry_cached(func: &ByteCodeFunction, osr_pc: usize) -> bool {
    let Some(id) = func.jit_runtime().compiled_id() else {
        return false;
    };
    OSR_CACHE.with(|c| c.borrow().contains_key(&(id, osr_pc)))
}

/// Deopt-driven invalidation of `func`'s compiled code on this thread
/// (`jit::reopt`), after its feedback was widened. Counts the invalidation
/// toward the source's backoff, makes the interpreter record feedback
/// again, and:
///
/// 1. retires the entry leaf, leaving `Deferred(Reoptimize)` (a re-profile
///    window of `heat` interpreted calls), nothing (`Reprofile::Immediate`:
///    the next hot dispatch recompiles) or `NotCompilable` (the level
///    reached `Interpreter`);
/// 2. retires the source's OSR leaves (compiled from the same feedback; kept
///    allocated and rooted, a running OSR frame also holds its own `Rc`);
/// 3. clears every caller's spec slot that caches the retired entry leaf,
///    and the source's own leaf slot, so nothing calls it again;
/// 4. forgets the source's inline dependencies (its entry is no longer a
///    compiled leaf that inlined anything).
///
/// Runs on a deopt exit before the resume state is seeded: no Lisp-heap
/// allocation, no safepoint. Cold.
#[cold]
#[inline(never)]
pub(crate) fn invalidate_for_reopt(
    func: &ByteCodeFunction,
    origin: LeafOrigin<'_>,
    floor: ReoptLevel,
    reprofile: super::reopt::Reprofile,
) {
    use super::reopt::Reprofile;
    let rt = func.jit_runtime();
    let id = rt.compiled_id_or_assign();
    let knobs = super::reopt::knobs();
    let level = rt.note_reopt(floor, knobs.max_reopts);
    rt.reopen_numeric_feedback();
    rt.clear_aot_prewarmed();
    rt.disarm_leaf_slot();
    let retired_entry = COMPILED.with(|c| {
        let Ok(mut c) = c.try_borrow_mut() else {
            tracing::warn!(target: "neovm_jit::reopt", id, "cache borrowed; entry kept");
            return None;
        };
        let old = match c.get(id) {
            Some(CacheEntry::Compiled(l)) => Rc::clone(l),
            _ => return None,
        };
        let next = match (level, reprofile) {
            (ReoptLevel::Interpreter, _) => Some(CacheEntry::NotCompilable),
            (_, Reprofile::Immediate) => None,
            (_, Reprofile::Window) => Some(CacheEntry::Deferred(DeferReason::Reoptimize {
                regalloc: old.regalloc,
                profit_gate_bypassed: old.profit_gate_bypassed,
                call_heavy: old.call_heavy,
            })),
        };
        c.retire_and_replace(id, next);
        Some(old)
    });
    let osr: Vec<Rc<CompiledLeaf>> = OSR_CACHE.with(|c| {
        let Ok(mut c) = c.try_borrow_mut() else {
            return Vec::new();
        };
        let keys: Vec<(u64, usize)> = c
            .iter()
            .filter(|((fid, _), e)| *fid == id && e.is_some())
            .map(|(k, _)| *k)
            .collect();
        keys.iter()
            .filter_map(|k| c.remove(k).flatten().map(|e| e.leaf))
            .collect()
    });
    if !osr.is_empty() {
        let retired = COMPILED.with(|c| match c.try_borrow_mut() {
            Ok(mut c) => {
                for leaf in &osr {
                    c.retire(Rc::clone(leaf));
                }
                true
            }
            Err(_) => false,
        });
        if !retired {
            // Dropped instead, as `evict_compiled` drops OSR leaves (only
            // the interpreter enters them; a running one holds its own Rc).
            for leaf in &osr {
                fold_dropped_leaf(leaf);
            }
        }
    }
    let mut unlinked = 0;
    if let Some(old) = &retired_entry {
        // Its entry no longer names a compiled leaf that inlined anything,
        // so a redefinition of an inlined callee has nothing to evict.
        forget_inline_deps(id);
        unlinked = unlink_spec_slots(Rc::as_ptr(old));
        if reprofile == Reprofile::Window && level != ReoptLevel::Interpreter {
            rt.defer_tier_up(rt.heat().saturating_add(knobs.heat).max(1));
        }
    }
    if level == ReoptLevel::Interpreter {
        rt.mark_native_rejected(rejection_epoch());
    }
    stats::record_reopt(level);
    tracing::debug!(
        target: "neovm_jit::reopt",
        id,
        osr = matches!(origin, LeafOrigin::Osr { .. }),
        floor = floor.name(),
        level = level.name(),
        count = rt.reopt_count(),
        entry = retired_entry.is_some(),
        osr_leaves = osr.len(),
        unlinked,
        "invalidated"
    );
}

/// Evictions after which a callee counts as unstable (see `INLINE_EVICTIONS`).
const UNSTABLE_INLINE_EVICTIONS: u32 = 2;

/// Whether `sym` has been redefined out from under inlining callers often
/// enough that inlining it again would just buy another recompile.
pub(crate) fn inline_callee_is_unstable(sym: SymId) -> bool {
    INLINE_EVICTIONS.with(|m| {
        m.borrow()
            .get(&sym)
            .is_some_and(|&n| n >= UNSTABLE_INLINE_EVICTIONS)
    })
}

pub(crate) fn evict_inline_dependents(sym: SymId) {
    let Some(dependents) = INLINE_DEPS.with(|m| m.borrow_mut().remove(&sym)) else {
        return;
    };
    stats::epoch::note_inline_evictions(dependents.len());
    INLINE_EVICTIONS.with(|m| *m.borrow_mut().entry(sym).or_default() += 1);
    COMPILED.with(|cache| {
        let mut cache = cache.borrow_mut();
        for id in dependents {
            // Only leaves that RECORDED inline deps are ever in a dep set. A
            // baseline bit-op leaf among them may sit in a spec or leaf slot
            // (see resolve_compiled_leaf_ptr): `remove` retires it, so the
            // pointer stays valid, and disarms the leaf slots. The predicate is
            // the dep list, NOT `inline_epoch`: a baseline leaf with an inlined
            // bit-op (LEVEL-B, `inline_arith_callee_syms`) records deps and
            // deliberately no epoch, and the 2026-09-05 gate-off census tripped
            // the epoch-based version of this assertion on exactly that.
            debug_assert!(
                !matches!(cache.get(id), Some(CacheEntry::Compiled(l)) if l.inline_deps().is_empty()),
                "precise eviction must only touch leaves that recorded inline deps"
            );
            cache.remove(id);
        }
    });
}

/// Test-only: what the cache holds for `id`.
#[cfg(test)]
pub(crate) fn cache_entry_kind_for_test(id: u64) -> &'static str {
    COMPILED.with(|c| match c.borrow().get(id) {
        Some(CacheEntry::Compiled(_)) => "compiled",
        Some(CacheEntry::NotCompilable) => "not-compilable",
        Some(CacheEntry::Deferred(DeferReason::NotProfitable)) => "deferred",
        Some(CacheEntry::Deferred(DeferReason::Reoptimize { .. })) => "deferred-reopt",
        None => "none",
    })
}

/// Test-only: the register allocator of the leaf cached for `id`, if any.
#[cfg(test)]
pub(crate) fn compiled_regalloc_for_test(id: u64) -> Option<RegallocChoice> {
    COMPILED.with(|c| match c.borrow().get(id) {
        Some(CacheEntry::Compiled(l)) => Some(l.regalloc),
        _ => None,
    })
}

/// Test-only: the identity of the compiled leaf cached for `id`.
#[cfg(test)]
pub(crate) fn compiled_leaf_ptr_for_test(id: u64) -> Option<*const CompiledLeaf> {
    COMPILED.with(|c| match c.borrow().get(id) {
        Some(CacheEntry::Compiled(leaf)) => Some(Rc::as_ptr(leaf)),
        _ => None,
    })
}

/// Test-only: is a compiled leaf currently cached for `id` on this thread?
#[cfg(test)]
pub(crate) fn is_compiled_for_test(id: u64) -> bool {
    COMPILED.with(|c| matches!(c.borrow().get(id), Some(CacheEntry::Compiled(_))))
}

/// Test-only: whether the cached leaf for `id` is AOT-backed (served from a
/// loaded `.so`, NOT JIT-compiled). Proves the AOT cache consult engaged.
#[cfg(test)]
pub(crate) fn cached_leaf_is_aot_for_test(id: u64) -> Option<bool> {
    COMPILED.with(|c| match c.borrow().get(id) {
        Some(CacheEntry::Compiled(leaf)) => Some(leaf.is_aot_backed()),
        _ => None,
    })
}

/// Whether the cached leaf for `func` is AOT-backed. Used by the call-bearing AOT
/// integration self-test (`aot::testkit_call_bearing_selftest`), which runs in
/// the lib (not `cfg(test)`), so this is a non-test accessor. `None` if `func`
/// has no cached `Compiled` leaf.
pub(crate) fn cached_leaf_is_aot_for_func(func: &ByteCodeFunction) -> Option<bool> {
    let id = func.jit_runtime().compiled_id_or_assign();
    COMPILED.with(|c| match c.borrow().get(id) {
        Some(CacheEntry::Compiled(leaf)) => Some(leaf.is_aot_backed()),
        _ => None,
    })
}

/// Test-only: how many callers are recorded as inlining `sym`.
#[cfg(test)]
pub(crate) fn inline_dependent_count_for_test(sym: SymId) -> usize {
    INLINE_DEPS.with(|m| m.borrow().get(&sym).map_or(0, |s| s.len()))
}

/// R2 increment C (AOT PGO persistence): the `compiled_id`s of the PROVEN-HOT JIT
/// leaves the AOT tier did NOT already provide — every `COMPILED` entry that is
/// `Compiled` (not `NotCompilable`) AND NOT [`CompiledLeaf::is_aot_backed`]. This is
/// exactly the set the shutdown drain persists to `NEOVM_AOT_DIR` (∩ the obarray's
/// required-only bytecode fns) so next session serves them native from call 1.
/// Reads the thread-local `COMPILED`, so it MUST run on the eval thread that owns
/// the cache.
pub(crate) fn jit_compiled_ids() -> HashSet<u64> {
    COMPILED.with(|c| {
        c.borrow()
            .iter()
            .filter_map(|(id, e)| match e {
                CacheEntry::Compiled(leaf) if !leaf.is_aot_backed() => Some(id),
                _ => None,
            })
            .collect()
    })
}

/// Selftest seam (AOT-PGO drain): JIT-compile `func` and cache it as a `Compiled`
/// leaf WITHOUT the AOT consult [`try_run_compiled`] performs — so the drain
/// self-test can stage a proven-hot JIT leaf WITHOUT the `try_load_leaf` →
/// `load_unit` call that would freeze the process-wide AOT unit index (a OnceLock)
/// BEFORE the drain writes its `.so`. Returns the leaf's `compiled_id`, or `None`
/// if the body is not JIT-compilable. Non-`cfg(test)` (integration self-tests
/// compile the lib with `cfg(test)` FALSE), mirroring [`cached_leaf_is_aot_for_func`].
pub(crate) fn compile_and_cache_jit_leaf(
    func: &ByteCodeFunction,
    obarray: Option<&Obarray>,
) -> Option<u64> {
    let id = func.jit_runtime().compiled_id_or_assign();
    let entry = compile_cache_entry(
        id,
        func,
        obarray,
        CompileRequest {
            regalloc: RegallocPolicy::Auto,
            bypass_profit_gate: false,
            origin: stats::CompileOrigin::AotDrain,
        },
        None,
    );
    let compiled = matches!(entry, CacheEntry::Compiled(_));
    COMPILED.with(|c| {
        c.borrow_mut().insert(id, entry);
    });
    compiled.then_some(id)
}

/// Collect, as GC roots, the heap-object constants every currently-cached compiled
/// leaf loads through its reloc vector (R1a). Generated code holds NO heap-pointer
/// immediate — only an index into the leaf's `reloc_data` — so without this a
/// constant referenced solely by live native code could be swept. Walking COMPILED
/// (its entries AND its retired leaves, which can still run) keeps it precise: a
/// leaf the cache dropped (`clear`, an OSR eviction) drops out with it.
/// Clear the cache if the thread's tagged heap was replaced since the cache was
/// built (a pdump load / in-process image reload / cache-replay test): the cached
/// leaves' reloc vectors + baked addresses point into the now-gone heap, so they
/// must neither be traced nor run. Detected by heap identity — one thread-local
/// load + compare on the common no-change path; clears only on an actual change.
///
/// `COMPILED_HEAP == None` is ADOPTED, never treated as a change: it means no
/// insert has pinned an identity yet (`record_compiled_heap`), i.e. the cache is
/// empty — nothing can be stale. Treating the first observation as a change was
/// the JIT-campaign entry crash (rr-proven): under the default no-AOT config the
/// first caller of this fn is the first GC's root walk, which then `clear()`ed
/// every leaf compiled so far — including the leaf whose shim call had triggered
/// that GC — freeing its `reloc_data`/`spec_slots` boxes under its own running
/// machine code; the next compile's `declare_function` name string landed in the
/// freed reloc cell and the leaf called `"neovm_jit_varbind"`'s bytes as a
/// function. See `resolve_compiled_leaf_ptr` for the invariant this preserves.
fn sync_cache_to_current_heap() {
    let cur = crate::tagged::gc::current_tagged_heap_identity();
    let changed = COMPILED_HEAP.with(|h| match h.get() {
        None => {
            h.set(cur);
            false
        }
        Some(prev) if Some(prev) != cur => {
            h.set(cur);
            true
        }
        Some(_) => false,
    });
    if changed {
        clear();
    }
}

/// Pin the heap identity the cache's leaves are built against, at INSERT time
/// (first insert wins; later ones are necessarily the same heap, or a genuine
/// change that `sync_cache_to_current_heap` clears before any insert can run
/// under the new heap via the GC-root/OSR paths). This is what makes `None` in
/// `sync_cache_to_current_heap` mean "empty cache" rather than "unknown".
fn record_compiled_heap() {
    COMPILED_HEAP.with(|h| {
        if h.get().is_none() {
            h.set(crate::tagged::gc::current_tagged_heap_identity());
        }
    });
}

/// Pin the obarray the cache's leaves are built against, at compile time
/// (first compile wins, as `record_compiled_heap` pins the heap).
fn record_compiled_obarray(obarray: Option<&Obarray>) {
    if let Some(obarray) = obarray {
        COMPILED_OBARRAY.with(|g| {
            if g.get().is_none() {
                g.set(Some(obarray.generation()));
            }
        });
    }
}

/// Clear the cache if its leaves were compiled against another obarray than
/// the one of GENERATION, the running Context's (`Obarray::generation`).
///
/// `ctx.obarray` is assigned only by the Context constructors, and a new
/// Context comes with a new heap, which `sync_cache_to_current_heap` already
/// catches -- so this is the belt to that brace, for the leaves that keep
/// obarray-internal addresses (symbol cells, BLV records, forwarder
/// descriptors). One thread-local compare per GC root walk; nothing per call.
pub(crate) fn sync_cache_to_obarray(generation: u64) {
    let changed = COMPILED_OBARRAY.with(|g| g.get().is_some_and(|prev| prev != generation));
    if changed {
        clear();
        COMPILED_OBARRAY.with(|g| g.set(Some(generation)));
    }
}

pub(crate) fn collect_jit_reloc_gc_roots(roots: &mut Vec<Value>) {
    sync_cache_to_current_heap();
    COMPILED.with(|c| {
        let cache = c.borrow();
        for entry in cache.values() {
            if let CacheEntry::Compiled(leaf) = entry {
                roots.extend_from_slice(leaf.reloc_values());
            }
        }
        // RETIRED leaves too. A retired leaf stays allocated because an
        // outer native frame (recursion, a callee that redefined its caller)
        // or a caller's spec slot may still run it, and running it loads its
        // constants from `reloc_data`. Its source function keeps the body's
        // own constants alive, but not those of a callee the MIR tier inlined
        // and that has since been redefined, nor any constant once the source
        // itself is gone. Extra roots only mark more (SATB and
        // allocate-black alike), and `reloc_data` is immutable after the
        // compile, so no barrier is involved. Bounded: retiring is rare
        // (re-tier, stale inline, patched prefix, deopt invalidation).
        for leaf in &cache.retired {
            roots.extend_from_slice(leaf.reloc_values());
        }
    });
    // OSR leaves also bake heap-constant reloc vectors — root them too, else a GC
    // between an OSR compile and its next run could free the leaf's constants.
    OSR_CACHE.with(|c| {
        for entry in c.borrow().values().flatten() {
            roots.extend_from_slice(entry.leaf.reloc_values());
        }
    });
}

/// GC handshake size probe: `(total COMPILED cache entries, total reloc slots
/// the root walk visits)` — the O() inputs of `collect_jit_reloc_gc_roots`.
/// The slots include the retired leaves' (the walk roots them too); the
/// entry count stays the cache's own. Read-only; called once per handshake
/// OUTSIDE the timed pause window.
pub(crate) fn compiled_cache_probe() -> (usize, usize) {
    COMPILED.with(|c| {
        let cache = c.borrow();
        let live: usize = cache
            .values()
            .map(|entry| match entry {
                CacheEntry::Compiled(leaf) => leaf.reloc_values().len(),
                _ => 0,
            })
            .sum();
        let retired: usize = cache.retired.iter().map(|l| l.reloc_values().len()).sum();
        (cache.len(), live + retired)
    })
}

/// R2-C3: insert AOT-prepopulated leaves into `COMPILED` so the loadup set serves
/// native FROM CALL 1. `leaves` is `(compiled_id, CompiledLeaf)` built by
/// `aot::prepopulate_aot_from_preload` from the preload `.so`. Returns how many
/// were actually inserted (cold slots filled — see INSERT-IF-ABSENT below).
///
/// TWO load-bearing invariants:
///
/// 1. ESTABLISH `COMPILED_HEAP` WITHOUT CLEARING (R1a + audit w0guiyma9). We set
///    `COMPILED_HEAP = current` directly (NOT via `sync_cache_to_current_heap`,
///    which would CLEAR on the first None→current transition). This stops the
///    first post-prepopulate GC's `sync_cache_to_current_heap` from wiping our
///    inserts (it will see current==current). Crucially it ALSO preserves any
///    VALID same-heap JIT leaf the after-pdump-load-hook compiled before us — a
///    plain sync-first would destroy it (and any spec-slot pointer into it) on
///    that None→current clear. The cache here was built against the current heap,
///    so it is valid; a genuine heap CHANGE still clears via the GC-root path.
///
/// 2. INSERT-IF-ABSENT, NEVER OVERWRITE (audit w0guiyma9). The
///    `after-pdump-load-hook` runs arbitrary elisp on this thread IMMEDIATELY
///    before prepopulate; it can JIT-compile (and GC) a loadup fn, leaving a
///    VALID JIT leaf in `COMPILED` whose `Rc` may already be pointed at by another
///    leaf's speculation slot (`Rc::as_ptr`) and whose `INLINE_DEPS` are
///    registered. Overwriting it with `cache.insert` would (a) drop that `Rc` →
///    free it → leave the spec slot dangling (USE-AFTER-FREE), and (b) replace a
///    JIT leaf without unregistering its inline deps → a later redefinition's
///    `evict_inline_dependents` trips the spec-slot-safety assert. So we only fill
///    COLD (absent) slots; an existing entry (JIT or AOT, Compiled or
///    NotCompilable) is kept untouched. The prewarm is purely additive.
///
/// REDEFINITION (occupancy note): a prepopulated AOT leaf is keyed by the loadup
/// fn's `compiled_id`. If that fn is later REDEFINED, the redefinition is a NEW
/// `ByteCodeFunction` with a NEW `compiled_id`, so the stale AOT leaf for the old
/// id is simply never looked up again (same staleness story as a JIT leaf — see
/// this module's header). And an AOT leaf (inline_epoch=None, no inline deps) is
/// never in any `INLINE_DEPS` set, so `evict_inline_dependents` never targets it —
/// the spec-slot-safety assert above only ever fires on genuinely-inlined leaves,
/// which AOT leaves are not.
pub(crate) fn prepopulate_aot_leaves(leaves: Vec<(u64, CompiledLeaf)>) -> Vec<u64> {
    // Establish COMPILED_HEAP == current WITHOUT clearing (audit w0guiyma9).
    // Historically a plain `sync_cache_to_current_heap` CLEARED on the very first
    // None→current transition; it now adopts (and every insert pins the identity
    // via `record_compiled_heap`), so this direct set is the same operation the
    // inserts below would perform — kept explicit so the prewarm's contract does
    // not depend on the cache being non-empty. The cache, if non-empty here, was
    // built against the CURRENT heap (the hook ran on this thread after the pdump
    // load), so it is valid and must be kept. Only a genuine heap CHANGE (a later
    // pdump reload) clears, via `sync_cache_to_current_heap` from the GC roots.
    COMPILED_HEAP.with(|h| h.set(crate::tagged::gc::current_tagged_heap_identity()));
    let mut inserted = Vec::new();
    COMPILED.with(|c| {
        let mut cache = c.borrow_mut();
        for (id, mut leaf) in leaves {
            // INSERT-IF-ABSENT: never clobber a pre-existing entry (a JIT leaf the
            // hook compiled may be spec-slot-referenced + INLINE_DEPS-registered).
            // AOT leaves never inline → no inline deps to register; their reloc
            // consts are rooted via the COMPILED walk in collect_jit_reloc_gc_roots.
            if cache.get(id).is_none() {
                leaf.obs.id = id;
                cache.insert(id, CacheEntry::Compiled(Rc::new(leaf)));
                inserted.push(id);
            }
        }
    });
    inserted
}

/// Drop all compiled state on this thread. Called when a pdump load replaces the
/// runtime image (and thus the heap that every cached leaf's reloc vector + baked
/// addresses reference) — so every cached leaf is now stale and must neither be run
/// nor GC-traced. No-op at the single startup load (the cache is empty then); it
/// matters when a process reloads an image in-place (e.g. the pdump round-trip
/// tests), where leaving stale leaves cached makes R1a's reloc roots trace
/// freed/reused memory.
pub(crate) fn clear() {
    debug_assert_eq!(
        native_depth(),
        0,
        "jit::cache::clear() under a running native leaf would free its reloc/spec boxes \
         beneath its own machine code (the heap-identity stability invariant, see \
         resolve_compiled_leaf_ptr)"
    );
    COMPILED.with(|c| c.borrow_mut().clear());
    INLINE_DEPS.with(|m| m.borrow_mut().clear());
    INLINE_EVICTIONS.with(|m| m.borrow_mut().clear());
    OSR_CACHE.with(|c| {
        let mut osr = c.borrow_mut();
        for entry in osr.values().flatten() {
            fold_dropped_leaf(&entry.leaf);
        }
        osr.clear();
    });
    // Every remembered NotCompilable verdict is now as stale as the cache.
    REJECTION_EPOCH.fetch_add(1, Ordering::Relaxed);
    // No leaf is left to be bound to an obarray.
    COMPILED_OBARRAY.with(|g| g.set(None));
    // The pinned functions belong to the heap being replaced.
    REDEFINED_PINS.with(|pins| pins.borrow_mut().clear());
}

/// Tier-up entry point: run `func`'s body as native code if possible.
///
/// - `Ok(Some(bits))` — native code produced the result (raw tagged bits).
/// - `Ok(None)` — fall back to the Tier-0 interpreter: the body is not
///   compilable by this tier, the arity didn't match (the interpreter must
///   signal wrong-number-of-arguments), or compiled code **deoptimized**. A
///   deopt can only happen before any side effect (the guard-after-call
///   poisoning analysis rejects everything else), so rerunning is sound.
/// - `Err(flow)` — a runtime call inside native code raised a non-local exit;
///   propagate it.
///
/// `ctx` is the `Context` the dispatch seam is executing in; runtime-call shims
/// re-enter elisp through it. Compiles on first use (per thread) and caches the
/// outcome, so a non-compilable body is only attempted once.
/// Debug aid: when `NEOVM_JIT_MAX_ID` is set, only functions whose
/// `compiled_id` is <= it run natively — bisecting a misbehaving compiled
/// function out of a workload (ids are assigned in first-hot order, so this is
/// a clean prefix bisection).
fn max_compiled_id() -> u64 {
    use std::sync::OnceLock;
    static MAX: OnceLock<u64> = OnceLock::new();
    *MAX.get_or_init(|| {
        std::env::var("NEOVM_JIT_MAX_ID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(u64::MAX)
    })
}

// C-ABI dispatch seam: `ctx` is a raw `*mut Context` from the native-call shim
// path; the documented dormant-Context contract (see the `unsafe` deref inside)
// makes the read sound. The lint fires on the `pub` + raw-ptr-deref shape, same
// as the 35 `neovm_jit_*` shims, which carry the same allow.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn try_run_compiled(
    ctx: *mut Context,
    func: &ByteCodeFunction,
    func_value: Value,
    args: &[Value],
) -> Result<Option<usize>, Flow> {
    let id = func.jit_runtime().compiled_id_or_assign();
    if id > max_compiled_id() {
        return Ok(None);
    }
    // Debug aid: dump the body of one compiled function by id.
    {
        use std::sync::OnceLock;
        static DEBUG_ID: OnceLock<Option<u64>> = OnceLock::new();
        let dbg = *DEBUG_ID.get_or_init(|| {
            std::env::var("NEOVM_JIT_DEBUG_ID")
                .ok()
                .and_then(|s| s.parse().ok())
        });
        if dbg == Some(id) {
            let consts: Vec<String> = func
                .constants
                .iter()
                .map(crate::emacs_core::print::print_value)
                .collect();
            eprintln!(
                "[jit-debug] id={id} args={} ops={:?} constants={consts:?}",
                args.len(),
                func.executable_ops(),
            );
        }
    }
    // The fast-allocator leaf a re-tier retires, to unlink from its callers
    // once the cache borrow ends.
    let mut retiered: Option<Rc<CompiledLeaf>> = None;
    let leaf: Option<Rc<CompiledLeaf>> = COMPILED.with(|cache| {
        let mut cache = cache.borrow_mut();
        // SAFETY: the seam-provided Context is dormant for the whole native
        // dispatch (see neovm_jit_call's contract); a shared read of its obarray
        // for compile-time speculation. A null ctx (shim-free test bodies) just
        // disables speculation.
        let obarray = (!ctx.is_null()).then(|| unsafe { &(*ctx).obarray });
        // A leaf that inlined a callee needs no epoch check here: every write
        // of a function cell (`note_function_redefined`: fset, defalias,
        // fmakunbound, the silent clear) evicts exactly the leaves that
        // inlined that symbol (`evict_inline_dependents`), and only
        // `compile_cache_entry` makes inlining leaves, registering their
        // deps. The old coarse check -- recompile whenever ANY function was
        // redefined -- made each unrelated `fset' (a `cl-letf' of
        // `message', an `advice-add', a `defalias' while a package loads)
        // re-JIT every hot inlining leaf, ~250K instructions apiece.
        //
        // Re-tier: a leaf built with the fast register allocator that the
        // interpreter keeps entering has proven hot — rebuild it with the
        // full allocator (`retier_heat`). A leaf that already earned `Full`
        // keeps it when an evicted inline makes it recompile: the tier is
        // earned, not re-decided. Never under a forced allocator, which
        // would rebuild `Fast` forever.
        let (prev_regalloc, prev_bypassed, prev_call_heavy) = match cache.get(id) {
            Some(CacheEntry::Compiled(l)) => {
                (Some(l.regalloc), l.profit_gate_bypassed, l.call_heavy)
            }
            _ => (None, false, false),
        };
        // A call-heavy body stays on the fast allocator however hot it gets:
        // its time is in its shim calls, not in the code around them.
        let retier = prev_regalloc == Some(RegallocChoice::Fast)
            && !prev_call_heavy
            && forced_regalloc().is_none()
            && super::retier_heat().is_some_and(|at| func.jit_runtime().heat() >= at);
        if retier {
            if let Some(CacheEntry::Compiled(old)) = cache.get(id) {
                retiered = Some(Rc::clone(old));
            }
            cache.remove(id);
        }
        if retier {
            stats::record_retier();
        }
        let policy = if retier || prev_regalloc == Some(RegallocChoice::Full) {
            RegallocPolicy::Full
        } else {
            RegallocPolicy::Auto
        };
        // A deferred body whose deferral ran out compiles now; one whose
        // deferral still holds stays interpreted (the dispatcher does not
        // probe for it, but the funcall seam and tests may).
        let deferred = match cache.get(id) {
            Some(CacheEntry::Deferred(reason)) => Some(*reason),
            _ => None,
        };
        if deferred.is_some() && !func.jit_runtime().profit_deferral_expired() {
            return None;
        }
        if deferred.is_some() {
            cache.remove(id);
        }
        let (policy, bypass_profit_gate) = match deferred {
            // A refused body that proved hot: compile it, gate bypassed.
            Some(DeferReason::NotProfitable) => (policy, true),
            // A deopt-invalidated body re-profiled: recompile with the tier
            // the old leaf had earned. The allocator is not re-decided, and
            // a leaf already past its re-tier heat gets the full allocator
            // now rather than a second compile on the next call.
            Some(DeferReason::Reoptimize {
                regalloc,
                profit_gate_bypassed,
                call_heavy,
            }) => {
                let retier_due = !call_heavy
                    && forced_regalloc().is_none()
                    && super::retier_heat().is_some_and(|at| func.jit_runtime().heat() >= at);
                let policy = if regalloc == RegallocChoice::Full || retier_due {
                    RegallocPolicy::Full
                } else {
                    policy
                };
                (policy, profit_gate_bypassed || prev_bypassed)
            }
            None => (policy, prev_bypassed),
        };
        let request = CompileRequest {
            regalloc: policy,
            bypass_profit_gate,
            origin: stats::CompileOrigin::Dispatch,
        };
        match cache.get_or_insert_with(id, || {
            // R1c-6: consult AOT FIRST (additive — a miss/error falls through to
            // the JIT below, leaving JIT behavior unchanged). An AOT hit is a
            // PRE-WARMED leaf: native code already on disk, no JIT compile. Only
            // the required-only subset the AOT emitter supports is eligible
            // (no &optional/&rest — matches the MIR pure path's arity seeding).
            // AOT bodies bake every constant; a source with a make-closure
            // patched prefix needs the JIT's dynamic-prefix lowering.
            // A source a deopt invalidated never reloads its AOT leaf: that
            // leaf speculates exactly as the one that deopted.
            if super::aot::aot_enabled()
                && func.params.optional.is_empty()
                && func.params.rest.is_none()
                && func.jit_runtime().patched_prefix() == 0
                && func.jit_runtime().reopt_count() == 0
            {
                let native_arity = func.params.required.len();
                if let Some(mut leaf) = super::aot::try_load_leaf(
                    func.executable_ops(),
                    &func.constants,
                    native_arity,
                    func.jit_runtime()
                        .compiled_id()
                        .and_then(super::aot::prewarm_hash_for),
                    obarray,
                ) {
                    // AOT leaves never inline → no inline deps to register. Their
                    // reloc consts are rooted via the COMPILED walk (R1c-8).
                    leaf.obs.id = id;
                    stats::record_aot_load(func.executable_ops().len());
                    return CacheEntry::Compiled(Rc::new(leaf));
                }
            }
            let name_hint = stats::naming_enabled()
                .then(|| callee_name_hint(ctx, id))
                .flatten();
            // The origin is only read on this cold miss path.
            let origin = if retier {
                stats::CompileOrigin::Retier
            } else if deferred.is_some() {
                stats::CompileOrigin::DeferralExpired
            } else {
                stats::CompileOrigin::Dispatch
            };
            compile_cache_entry(
                id,
                func,
                obarray,
                CompileRequest { origin, ..request },
                name_hint,
            )
        }) {
            // Only run native for a valid call (lambda-list range); a mismatch
            // is a wrong-arg-count call the interpreter must signal.
            CacheEntry::Compiled(leaf) if leaf.accepts(args.len()) => Some(Rc::clone(leaf)),
            _ => None,
        }
    });
    // A re-tier retires the fast-allocator leaf without moving the function
    // epoch, so every caller's spec slot armed with it would keep calling
    // it -- valid code, but the full allocator's smaller frame and better
    // code never reached them. Point them back at the cache: their next
    // call resolves the full leaf.
    if let Some(old) = retiered {
        let unlinked = unlink_spec_slots(Rc::as_ptr(&old));
        tracing::debug!(target: "neovm_jit", id, unlinked, "re-tier unlinked the fast leaf");
    }
    // Execute OUTSIDE the cache borrow (see `CacheEntry::Compiled`).
    match leaf {
        None => Ok(None),
        Some(leaf) => run_resolved_leaf(ctx, func, func_value, &leaf, args),
    }
}

/// A stable raw pointer to the compiled leaf for `func` (compiling it on first
/// use), or `None` if the body is `NotCompilable`. Used by the V3 speculated-call
/// fast path to cache a callee leaf handle in a spec slot, skipping the cache
/// hash lookup on subsequent calls.
///
/// POINTER VALIDITY (audit #1 — corrected): the pointer is sound NOT because the
/// cache "never evicts" (it CAN — `clear()` drops every leaf on a tagged-heap
/// identity change, reached from `sync_cache_to_current_heap` during GC root
/// collection). It is sound because the heap identity is STABLE during native
/// leaf execution: every `set_tagged_heap` caller is a top-level entry point
/// (Context build / eval entry / pdump load), never nested inside a running
/// native leaf, so `sync_cache_to_current_heap` never observes a change — hence
/// `clear()` never fires — while a spec-slot pointer is live on the native stack.
/// The spec slots are owned by the executing caller leaf, so a `clear()` would
/// drop the caller and its slots together; no stale slot can outlive it.
/// Slice C1 of the JIT call seam: the leaf the interpreter's `Bcall` arm may
/// enter DIRECTLY — no cache probe, no `LispArgVec` — with the premarshaled
/// shape the caller must supply: `(leaf, nonrest, has_rest)` = the first
/// `nonrest` slots are the args nil-padded for omitted `&optional`, plus one
/// consed `&rest` list when `has_rest` (what `call_consts` builds). `None` = take
/// `try_run_compiled`, which alone owns AOT loads, profit deferral, the
/// re-tier at `retier_heat()` (the slot steps aside on exactly that call so
/// the crossing is seen) and the `NEOVM_JIT_DEBUG_ID` trace. Only
/// inline-dep-free leaves are ever armed (`resolve_compiled_leaf_ptr`), so
/// the `inline_epoch` staleness backstop does not apply to them. Checked
/// BEFORE `dispatch_sized`: on a hit it advances the heat in its place.
#[inline]
pub(crate) fn armed_leaf_for_stack_call(
    func: &ByteCodeFunction,
    nargs: usize,
) -> Option<(*const CompiledLeaf, usize, bool)> {
    let rt = func.jit_runtime();
    let leaf = rt.armed_leaf_slot(leaf_slot_epoch())?;
    #[cfg(test)]
    if rt.force_interpret_for_test() {
        return None;
    }
    // SAFETY: armed from a live `COMPILED` entry under the current epoch;
    // every retire/clear bumps the epoch, and retired leaves stay allocated.
    let (has_rest, arity, accepts) =
        unsafe { ((*leaf).has_rest, (*leaf).arity, (*leaf).accepts(nargs)) };
    if !accepts {
        return None;
    }
    // This entry replaces `dispatch_sized` for an armed function (already
    // tiered up: its threshold/deferral/cap math is settled), so it advances
    // the heat itself and steps aside on the re-tier crossing.
    let now = rt.bump_heat();
    super::retier_heat().is_none_or(|at| now != at).then_some((
        leaf,
        arity - usize::from(has_rest),
        has_rest,
    ))
}

/// The armed leaf for a NATIVE-to-native call at `nargs`, or `None` to let the
/// generic seam handle this call.
///
/// Same probe as [`armed_leaf_for_stack_call`] — same epoch check, same arity
/// check, same heat bookkeeping in place of `dispatch_sized` — with one
/// difference that matters: it tests the re-tier crossing BEFORE advancing the
/// heat instead of after. A caller that declines here falls through to the
/// generic seam, which probes the same slot again; if this probe had already
/// bumped, the second bump would carry the heat PAST `retier_heat()` and the
/// re-tier would never fire for that function. Declining without a bump leaves
/// the crossing for the generic seam's own probe to see.
#[inline]
pub(crate) fn armed_leaf_for_native_call(
    func: &ByteCodeFunction,
    nargs: usize,
) -> Option<*const CompiledLeaf> {
    let rt = func.jit_runtime();
    let leaf = rt.armed_leaf_slot(leaf_slot_epoch())?;
    #[cfg(test)]
    if rt.force_interpret_for_test() {
        return None;
    }
    // SAFETY: armed from a live `COMPILED` entry under the current epoch — see
    // `armed_leaf_for_stack_call`.
    if !unsafe { (*leaf).accepts(nargs) } {
        return None;
    }
    if super::retier_heat().is_some_and(|at| rt.peek_heat().saturating_add(1) == at) {
        return None;
    }
    rt.bump_heat();
    Some(leaf)
}

/// Run a leaf returned by `armed_leaf_for_stack_call` on `args_ptr`
/// (its premarshaled `Value` bits, NOT a pointer into the operand stack: a
/// nested call's shim pushes onto it and may reallocate). Same result shape
/// as `try_run_compiled`: `Ok(None)` = interpret instead.
pub(crate) fn run_armed_leaf(
    ctx: *mut Context,
    func: &ByteCodeFunction,
    func_value: Value,
    leaf: *const CompiledLeaf,
    args_ptr: *const i64,
) -> Result<Option<usize>, Flow> {
    // SAFETY: see armed_leaf_for_stack_call.
    let leaf = unsafe { &*leaf };
    super::stats::record_native_entry();
    match run_resolved_leaf_native(ctx, func, func_value, leaf, args_ptr) {
        NativeCallOutcome::Value(v) => Ok(Some(v.bits())),
        NativeCallOutcome::Fallback => Ok(None),
        NativeCallOutcome::FlowStashed => {
            Err(take_pending_flow()
                .expect("FlowStashed from a compiled leaf implies a stashed Flow"))
        }
    }
}

/// Arm `func`'s leaf slot after a tier-up entry ran its leaf, so the next
/// exact-arity call takes the direct entry. Reads the cache only (the entry
/// exists: the leaf just ran); Deferred/NotCompilable/inlining leaves leave
/// the slot empty.
pub(crate) fn arm_leaf_slot(ctx: *mut Context, func: &ByteCodeFunction) {
    let rt = func.jit_runtime();
    let epoch = leaf_slot_epoch();
    if rt.armed_leaf_slot(epoch).is_some() {
        return;
    }
    if let Some(leaf) = resolve_compiled_leaf_ptr(ctx, func) {
        rt.arm_leaf_slot(leaf, epoch);
    }
}

pub(crate) fn resolve_compiled_leaf_ptr(
    ctx: *mut Context,
    func: &ByteCodeFunction,
) -> Option<*const CompiledLeaf> {
    let id = func.jit_runtime().compiled_id_or_assign();
    if id > max_compiled_id() {
        return None;
    }
    COMPILED.with(|cache| {
        let mut cache = cache.borrow_mut();
        match cache.get_or_insert_with(id, || {
            // SAFETY: same dormant-Context contract as try_run_compiled.
            let obarray = (!ctx.is_null()).then(|| unsafe { &(*ctx).obarray });
            let name_hint = stats::naming_enabled()
                .then(|| callee_name_hint(ctx, id))
                .flatten();
            compile_cache_entry(
                id,
                func,
                obarray,
                CompileRequest {
                    regalloc: RegallocPolicy::Auto,
                    bypass_profit_gate: false,
                    origin: stats::CompileOrigin::FirstSight,
                },
                name_hint,
            )
        }) {
            // MIR-INLINED leaves must NOT be fast-path-cached in a spec slot:
            // their validity depends on an inlined callee's epoch
            // (`inline_epoch`), which only try_run_compiled checks (it re-JITs
            // on a stale epoch). A baseline leaf that inlined a bit-op
            // (`inline_deps` with no epoch) is cached: redefining the bit-op
            // evicts it (`evict_inline_dependents`), which disarms every leaf
            // slot, and the same redefinition moves the function epoch, whose
            // re-arm drops a spec slot's cached leaf. Before, every call into
            // such a leaf — each bindat packer that masks with `logand` — took
            // the strict seam. A retired leaf stays allocated (`DenseCache::
            // remove`), so a pointer never dangles.
            CacheEntry::Compiled(leaf)
                if leaf.inline_deps().is_empty() || leaf.inline_epoch().is_none() =>
            {
                Some(Rc::as_ptr(leaf))
            }
            _ => None,
        }
    })
}

/// Run an already-resolved `leaf` (the caller validated arity) with the full
/// `NativeRun` outcome handling — including precise-deopt resume via
/// `run_resumed_frame`. Shared by `try_run_compiled` and the V3 fast path so
/// both have byte-identical deopt/signal semantics. Same return shape as
/// `try_run_compiled`: `Ok(Some(bits))` success, `Ok(None)` fall-back, `Err`
/// on a non-local flow.
pub(crate) fn run_resolved_leaf(
    ctx: *mut Context,
    func: &ByteCodeFunction,
    func_value: Value,
    leaf: &CompiledLeaf,
    args: &[Value],
) -> Result<Option<usize>, Flow> {
    super::stats::record_native_entry();
    finish_native_run(
        ctx,
        func,
        func_value,
        leaf,
        leaf.call_consts(ctx as *mut u8, func.constants.as_ptr(), args),
    )
}

/// Native-to-native variant of [`run_resolved_leaf`]: `args_ptr` addresses
/// exactly `leaf.arity` pre-marshaled argument words (the caller's native
/// call-args slot), and the leaf is a pure pass-through (no nil-pad / rest).
/// Skips the `LispArgVec` build and the `arg_bits` re-marshal entirely — the
/// per-call cost the call-heavy benchmark is dominated by.
///
/// SAFETY: see [`CompiledLeaf::call_premarshaled`] — `args_ptr` must address
/// `leaf.arity` live words with no GC safepoint before the native entry reads
/// them (the spec fast path's `maybe_quit`-returned-Ok window).
#[inline(always)]
pub(crate) fn run_resolved_leaf_native(
    ctx: *mut Context,
    func: &ByteCodeFunction,
    func_value: Value,
    leaf: &CompiledLeaf,
    args_ptr: *const i64,
) -> NativeCallOutcome {
    // Direct handler-free path: a body with no binds, no handler frames and
    // no sidecar runs WITHOUT its own invoke_native frame — no bases
    // snapshot, no CURRENT_LEAF_BASES publish, no NativeRun materialization.
    // It executes inside the CALLER leaf's extent exactly like any runtime
    // shim the caller invokes (see CompiledLeaf::direct_call_eligible for the
    // containment/soundness argument). Non-OK statuses are routed through
    // the same machinery invoke_native would use, out of line.
    //
    // `#[inline(always)]`: this is the eligibility test, the raw entry call
    // and a status compare; as a separate function it cost the call shims a
    // frame of its own (prologue, epilogue and the call) on every native
    // call. The framed half and every cold exit stay out of line.
    if leaf.direct_call_eligible() {
        let mut out: i64 = 0;
        // SAFETY: args_ptr addresses leaf.arity live words (the caller's
        // call-args slot, pure passthrough — checked by our caller); ctx is
        // the dormant seam Context.
        let status = unsafe {
            leaf.entry_call_raw_consts(ctx as *mut u8, func.constants.as_ptr(), args_ptr, &mut out)
        };
        if status == super::compile::STATUS_OK {
            #[cfg(any(test, debug_assertions))]
            NATIVE_OK_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return NativeCallOutcome::Value(Value::from_bits(out as usize));
        }
        #[cfg(any(test, debug_assertions))]
        count_native_status(status);
        return direct_call_cold(ctx, func, func_value, leaf, status);
    }
    run_resolved_leaf_native_framed(ctx, func, func_value, leaf, args_ptr)
}

/// The framed half of [`run_resolved_leaf_native`]: a body with bindings,
/// handler frames or a sidecar runs under its own `invoke_native` frame.
/// Out of line so the direct path above, inlined into the call shims, stays
/// a raw entry call and a status compare.
#[inline(never)]
fn run_resolved_leaf_native_framed(
    ctx: *mut Context,
    func: &ByteCodeFunction,
    func_value: Value,
    leaf: &CompiledLeaf,
    args_ptr: *const i64,
) -> NativeCallOutcome {
    let outcome = leaf.call_premarshaled_consts(ctx as *mut u8, func.constants.as_ptr(), args_ptr);
    finish_framed_run(ctx, func, func_value, leaf, outcome)
}

/// Map a framed native run's outcome to the shim's compact outcome: the
/// signal fold and the precise-deopt resume. Shared with the spec shim's
/// fast path, which makes the framed call itself and hands a non-OK
/// outcome here. `leaf` is the leaf that ran (the deopt hook's subject).
#[inline(never)]
pub(crate) fn finish_framed_run(
    ctx: *mut Context,
    func: &ByteCodeFunction,
    func_value: Value,
    leaf: &CompiledLeaf,
    outcome: NativeRun,
) -> NativeCallOutcome {
    #[cfg(any(test, debug_assertions))]
    count_native_outcome(&outcome);
    match outcome {
        NativeRun::Ok(bits) => NativeCallOutcome::Value(Value::from_bits(bits)),
        // call_premarshaled maps null-vmctx deopts to Deopt; defensive only —
        // the caller re-runs the callee on the interpreter.
        NativeRun::Deopt => {
            super::reopt::note_deopt(ctx, func, leaf, LeafOrigin::Entry, DeoptEvent::Rerun);
            NativeCallOutcome::Fallback
        }
        // Fold a contained shim panic into its signal flow at exactly the
        // boundary the old Result-shaped path did (take_pending_flow owns the
        // panic-wins conversion), then put the flow straight back — the shim
        // reads STATUS from the compact outcome without re-materializing the
        // Flow. Cold path: signals only.
        NativeRun::Signal => {
            let flow = take_pending_flow()
                .expect("STATUS_SIGNAL from compiled code implies a stashed Flow");
            stash_pending_flow(flow);
            NativeCallOutcome::FlowStashed
        }
        NativeRun::DeoptAt(resume) => deopt_resume_outcome(ctx, func, func_value, leaf, *resume),
    }
}

/// Precise-deopt resume shared by the wrapped and direct native paths: run
/// the Tier-0 interpreter mid-function off the [`DeoptResume`] payload.
/// `leaf` is the entry leaf that deopted (the deopt hook's subject).
fn deopt_resume_outcome(
    ctx: *mut Context,
    func: &ByteCodeFunction,
    func_value: Value,
    leaf: &CompiledLeaf,
    resume: crate::emacs_core::jit::compile::DeoptResume,
) -> NativeCallOutcome {
    let crate::emacs_core::jit::compile::DeoptResume {
        pc,
        stack,
        handlers,
        binds,
        spec_base,
        cond_base,
    } = resume;
    // Before the resumed frame seeds `stack` into the traced bc_buf: the
    // hook neither allocates on the Lisp heap nor reaches a safepoint.
    super::reopt::note_deopt(
        ctx,
        func,
        leaf,
        LeafOrigin::Entry,
        DeoptEvent::Precise { pc, stack: &stack },
    );
    if ctx.is_null() {
        return NativeCallOutcome::Fallback;
    }
    // SAFETY: the seam-provided &mut Context is dormant during the
    // native call — the same contract every runtime shim uses.
    let ctx = unsafe { &mut *ctx };
    let mut vm = crate::emacs_core::bytecode::Vm::from_context(ctx);
    match vm.run_resumed_frame(
        func, func_value, pc, &stack, handlers, &binds, spec_base, cond_base,
    ) {
        Ok(v) => NativeCallOutcome::Value(v),
        Err(flow) => {
            stash_pending_flow(flow);
            NativeCallOutcome::FlowStashed
        }
    }
}

/// Non-OK statuses of a direct handler-free native call, routed through the
/// same machinery `invoke_native` would apply — out of the hot path.
#[cold]
#[inline(never)]
pub(crate) fn direct_call_cold(
    ctx: *mut Context,
    func: &ByteCodeFunction,
    func_value: Value,
    leaf: &CompiledLeaf,
    status: i64,
) -> NativeCallOutcome {
    use super::compile::{STATUS_DEOPT_AT, STATUS_SIGNAL, shim_panic_pending};
    if status == STATUS_SIGNAL {
        leaf.obs.note_signal();
        // A panic one of the callee's shims contained: leave its marker for
        // the caller leaf's healing exit (`cold_frame_exit` / the match
        // shim restore the boundary only while it is set); folding it here
        // would consume the marker with the panicked extent's residue --
        // depth, bytecode and condition frames -- still in place. The
        // direct callee runs in the caller's extent, so the caller is the
        // healing owner.
        if shim_panic_pending() {
            return NativeCallOutcome::FlowStashed;
        }
        // Same panic-fold boundary as the wrapped path: take_pending_flow
        // owns the panic-wins conversion; the flow goes straight back.
        let flow =
            take_pending_flow().expect("STATUS_SIGNAL from compiled code implies a stashed Flow");
        stash_pending_flow(flow);
        return NativeCallOutcome::FlowStashed;
    }
    if status == STATUS_DEOPT_AT {
        // Precise deopt: no bind/cond frames exist on the direct path (the
        // eligibility gate excludes them).
        return match leaf.deopt_at_outcome(ctx as *mut u8, None, None) {
            NativeRun::DeoptAt(resume) => {
                deopt_resume_outcome(ctx, func, func_value, leaf, *resume)
            }
            // deopt_at_outcome only degrades to plain Deopt with a null vmctx.
            _ => NativeCallOutcome::Fallback,
        };
    }
    // STATUS_DEOPT: rerun-from-start — sound only for side-effect-free bodies
    // (the same defensive rule invoke_native applies).
    leaf.obs.note_deopt_rerun();
    leaf.assert_rerunnable();
    super::reopt::note_deopt(ctx, func, leaf, LeafOrigin::Entry, DeoptEvent::Rerun);
    NativeCallOutcome::Fallback
}

/// Register-sized outcome of a native-to-native call: the hot chain never
/// moves a fat `Result<_, Flow>` through sret. A signalling Flow stays in the
/// pending-flow slot ([`NativeCallOutcome::FlowStashed`]); the shim reports
/// STATUS_SIGNAL and the generated code's signal path consumes it as usual.
pub(crate) enum NativeCallOutcome {
    /// The callee returned this value.
    Value(Value),
    /// Could not run natively — the caller takes its strict/interpreter path.
    Fallback,
    /// A Flow is stashed in the pending slot (signal, or a deopt-resume error).
    FlowStashed,
}

impl NativeCallOutcome {
    /// Compact an `EvalResult`: the error flow goes into the pending slot.
    #[inline]
    pub(crate) fn from_result(res: Result<Value, Flow>) -> Self {
        match res {
            Ok(v) => NativeCallOutcome::Value(v),
            Err(flow) => {
                stash_pending_flow(flow);
                NativeCallOutcome::FlowStashed
            }
        }
    }
}

/// Map a [`NativeRun`] outcome to the `try_run_compiled` return shape, resuming
/// the interpreter mid-frame on a precise deopt. Shared by both resolved-leaf
/// runners so marshaled and native-to-native calls have identical semantics.
/// Phase-0 native-run outcome counters (mid-end campaign observability):
/// process-wide relaxed totals of how every native execution ended. Before
/// these, deopt rates were unmeasurable — a mid-end slice that changes guard
/// placement could not prove its effect. Read via
/// [`native_run_counters`]; the pattern follows `compile::SPEC_CALL_COUNT`.

pub(crate) static NATIVE_OK_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static NATIVE_DEOPT_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static NATIVE_DEOPT_AT_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static NATIVE_SIGNAL_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Phase-0: dispatch-seam fallbacks — `Plan::Compiled` was chosen but the
/// call still ran on the interpreter (`try_run_compiled` returned
/// `Ok(None)`: not compilable, stale epoch, or a rerun-from-start deopt).
/// Together with [`NATIVE_OK_COUNT`] this makes tier RESIDENCY measurable:
/// ok / (ok + fallback) is the native-execution rate of hot calls.
pub(crate) static SEAM_INTERP_FALLBACK_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Called by the eval dispatch seam on an `Ok(None)` native attempt.
#[inline]
pub fn note_seam_interp_fallback() {
    SEAM_INTERP_FALLBACK_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Direct-path variant of [`count_native_outcome`]: the handler-free fast
/// path never materializes a `NativeRun`, only a raw status word.
///
/// Test/debug builds only, like the spec-site counters: a relaxed
/// `fetch_add` is still a locked read-modify-write on x86, ~20 cycles on
/// every native call in a release build that reads the total nowhere.
#[cfg(any(test, debug_assertions))]
pub(crate) fn count_native_status(status: i64) {
    use super::compile::{STATUS_DEOPT_AT, STATUS_SIGNAL};
    let counter = if status == STATUS_SIGNAL {
        &NATIVE_SIGNAL_COUNT
    } else if status == STATUS_DEOPT_AT {
        &NATIVE_DEOPT_AT_COUNT
    } else {
        &NATIVE_DEOPT_COUNT
    };
    counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, debug_assertions))]
fn count_native_outcome(outcome: &NativeRun) {
    let counter = match outcome {
        NativeRun::Ok(_) => &NATIVE_OK_COUNT,
        NativeRun::Deopt => &NATIVE_DEOPT_COUNT,
        NativeRun::DeoptAt(_) => &NATIVE_DEOPT_AT_COUNT,
        NativeRun::Signal => &NATIVE_SIGNAL_COUNT,
    };
    counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// `(ok, deopt_rerun, deopt_at, signal)` totals across all native runs.
#[allow(dead_code)] // read by tests + measurement sessions
pub(crate) fn native_run_counters() -> (u64, u64, u64, u64) {
    use std::sync::atomic::Ordering;
    (
        NATIVE_OK_COUNT.load(Ordering::Relaxed),
        NATIVE_DEOPT_COUNT.load(Ordering::Relaxed),
        NATIVE_DEOPT_AT_COUNT.load(Ordering::Relaxed),
        NATIVE_SIGNAL_COUNT.load(Ordering::Relaxed),
    )
}

fn finish_native_run(
    ctx: *mut Context,
    func: &ByteCodeFunction,
    func_value: Value,
    leaf: &CompiledLeaf,
    outcome: NativeRun,
) -> Result<Option<usize>, Flow> {
    #[cfg(any(test, debug_assertions))]
    count_native_outcome(&outcome);
    match outcome {
        NativeRun::Ok(bits) => Ok(Some(bits)),
        NativeRun::Deopt => {
            super::reopt::note_deopt(ctx, func, leaf, LeafOrigin::Entry, DeoptEvent::Rerun);
            Ok(None)
        }
        NativeRun::DeoptAt(resume) => {
            let crate::emacs_core::jit::compile::DeoptResume {
                pc,
                stack,
                handlers,
                binds,
                spec_base,
                cond_base,
            } = *resume;
            // Before the resumed frame seeds `stack` into the traced bc_buf.
            super::reopt::note_deopt(
                ctx,
                func,
                leaf,
                LeafOrigin::Entry,
                DeoptEvent::Precise { pc, stack: &stack },
            );
            if ctx.is_null() {
                // call() maps null-vmctx deopts to Deopt; defensive only.
                return Ok(None);
            }
            // Precise deopt: resume the Tier-0 interpreter mid-function with
            // the live stack and the (still registered) frame state.
            // SAFETY: the seam-provided &mut Context is dormant during the
            // native call — the same contract every runtime shim uses.
            let ctx = unsafe { &mut *ctx };
            let mut vm = crate::emacs_core::bytecode::Vm::from_context(ctx);
            vm.run_resumed_frame(
                func, func_value, pc, &stack, handlers, &binds, spec_base, cond_base,
            )
            .map(|v| Some(v.bits()))
        }
        NativeRun::Signal => {
            Err(take_pending_flow()
                .expect("STATUS_SIGNAL from compiled code implies a stashed Flow"))
        }
    }
}

#[cfg(test)]
#[path = "cache/tests/cache_test.rs"]
mod tests;
