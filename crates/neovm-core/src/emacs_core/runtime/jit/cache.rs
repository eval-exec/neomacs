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
use std::time::Instant;

use super::compile::{
    CompiledLeaf, NativeRun, compile_bytecode_function_with, stash_pending_flow, take_pending_flow,
};
use super::stats;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::Value;

/// One thread's knowledge of a function's compiled state.
enum CacheEntry {
    /// Native code, ready to run. `Rc` so execution can happen *outside* the
    /// cache borrow — compiled code can `Call` back into elisp, and a hot callee
    /// re-enters this cache (a nested `borrow_mut` would panic).
    Compiled(Rc<CompiledLeaf>),
    /// The body is outside the baseline JIT's supported subset; never retried.
    NotCompilable,
    /// The profitability gate refused the body for now; its runtime carries
    /// the heat at which the compile is retried with the gate bypassed
    /// (`RuntimeState::profit_deferred_heat`). Interpreted until then.
    Deferred,
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
                self.retired.push(leaf);
            }
        }
    }

    fn insert(&mut self, id: u64, entry: CacheEntry) {
        record_compiled_heap();
        let idx = id as usize;
        if self.slots.len() <= idx {
            self.slots.resize_with(idx + 1, || None);
        }
        self.slots[idx] = Some(entry);
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
        self.slots.clear();
        self.retired.clear();
    }
}

thread_local! {
    /// `compiled_id` -> compiled state, owned by and private to this thread.
    static COMPILED: RefCell<DenseCache> = RefCell::new(DenseCache::default());

    /// Precise inline-dependency REVERSE map: callee `SymId` -> the set of caller
    /// `compiled_id`s that INLINED it. Populated at compile-miss (the `or_insert_with`
    /// closures register `leaf.inline_deps()`); consulted by `evict_inline_dependents`
    /// when a function is redefined, to evict exactly the affected callers EARLY. The
    /// coarse `inline_epoch`-vs-live-epoch backstop in `try_run_compiled` remains the
    /// correctness floor regardless — this map is a pure churn-reduction optimization.
    /// Same thread/scope as COMPILED (its values are only meaningful as COMPILED keys).
    static INLINE_DEPS: RefCell<HashMap<SymId, HashSet<u64>>> = RefCell::new(HashMap::default());

    /// The tagged-heap identity the cached leaves were compiled against. The JIT
    /// cache is thread-local, but every leaf's reloc vector + baked addresses
    /// reference the heap live at compile time. If the thread's heap is replaced
    /// (a pdump load / in-process image reload / cache-replay test), the whole
    /// cache is stale — detected lazily by identity in `sync_cache_to_current_heap`
    /// and cleared before any stale reloc value is traced or run. Pinned by the
    /// first insert (`record_compiled_heap`), so `None` means "no leaf cached"
    /// and is adopted, never cleared on.
    static COMPILED_HEAP: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };

    /// OSR (on-stack replacement) leaves, keyed by `(compiled_id, osr_pc)`. The
    /// value is `Some((leaf, entry_depth))` when the function is OSR-eligible +
    /// compiled at that loop header, `None` when it is ineligible/uncompilable (a
    /// negative cache, so a hot loop that can't OSR is probed only once). Same
    /// thread/scope as COMPILED; cleared alongside it (heap-identity + `clear`).
    // The nested shape is private and directly documents positive/negative OSR
    // cache entries plus their entry depth.
    #[allow(clippy::type_complexity)]
    static OSR_CACHE: RefCell<HashMap<(u64, usize), Option<(Rc<CompiledLeaf>, usize)>>> =
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

/// Whether `func`'s body carries any op that establishes dynamic state the OSR
/// entry cannot reconstruct (it skips the prologue): dynamic bindings, save
/// records, unwind-protect, and condition/catch handler frames. OSR is restricted
/// to bodies WITHOUT these — pure lexical compute loops — so the transfer needs
/// only the operand stack, no specpdl/handler state.
fn osr_body_has_dynamic_state(func: &ByteCodeFunction) -> bool {
    use crate::emacs_core::bytecode::Op;
    func.executable_ops().iter().any(|op| {
        matches!(
            op,
            Op::VarBind(_)
                | Op::Unbind(_)
                | Op::SaveExcursion
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
/// (params on the operand stack, so the seeded snapshot carries them), no dynamic
/// bind/handler/save ops, and the loop header has a well-defined entry depth with
/// no active handlers. Returns `(leaf, entry_depth)` — `entry_depth` is the exact
/// operand-stack size the native entry seeds, checked against the live snapshot.
fn compile_osr_leaf(
    obarray: &Obarray,
    func: &ByteCodeFunction,
    osr_pc: usize,
) -> Option<(Rc<CompiledLeaf>, usize)> {
    let dbg = std::env::var_os("NEOMACS_OSR_DEBUG").is_some();
    if !func.lexical || osr_body_has_dynamic_state(func) {
        if dbg {
            eprintln!(
                "OSR_DEBUG reject pre: lexical={} dynstate={}",
                func.lexical,
                osr_body_has_dynamic_state(func)
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
    if !cfg.entry_handlers.get(&osr_pc).is_none_or(|h| h.is_empty()) {
        if dbg {
            eprintln!("OSR_DEBUG reject: handlers live at pc {osr_pc}");
        }
        return None;
    }
    // A loop that is already running gets the full allocator: its work per
    // entry is unbounded, so the code quality is worth the compile.
    let _regalloc = RegallocScope::enter(super::compile::regalloc_for(RegallocPolicy::Full, ops));
    let leaf = match super::compile::lower_leaf_full_osr(
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
    if dbg {
        eprintln!("OSR_DEBUG compiled: pc={osr_pc} depth={entry_depth}");
    }
    Some((Rc::new(leaf), entry_depth))
}

/// OSR (on-stack replacement) dispatch: transfer a hot loop in `func` into native
/// code at loop-header `osr_pc`, given the live operand-stack snapshot `stack`
/// (bottom = the frame base). Returns `None` when OSR does not apply — ineligible
/// function, uncompilable body, or a snapshot whose depth ≠ the compiled entry
/// depth (a non-balanced back-edge; never transfer then). Otherwise the
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
            .or_insert_with(|| compile_osr_leaf(obarray, func, osr_pc))
            .clone()
    });
    let (leaf, entry_depth) = cached?;
    // Only transfer when the live snapshot is exactly the header's entry stack.
    if stack.len() != entry_depth {
        if std::env::var_os("NEOMACS_OSR_DEBUG").is_some() {
            eprintln!(
                "OSR_DEBUG no-transfer: pc={osr_pc} snapshot depth {} != entry {entry_depth}",
                stack.len()
            );
        }
        return None;
    }
    let arg_bits: Vec<i64> = stack.iter().map(|v| v.bits() as i64).collect();
    OSR_TRANSFER_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let run =
        leaf.call_premarshaled_consts(ctx as *mut u8, func.constants.as_ptr(), arg_bits.as_ptr());
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
fn compile_cache_entry(
    id: u64,
    func: &ByteCodeFunction,
    obarray: Option<&Obarray>,
    request: CompileRequest,
) -> CacheEntry {
    let started = Instant::now();
    let result = compile_bytecode_function_requested(func, obarray, request);
    stats::record_compile(started.elapsed(), func.executable_ops().len(), &result);
    match result {
        Ok(leaf) => {
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
            CacheEntry::Deferred
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
/// definition on its next call. The coarse `inline_epoch`-vs-live-epoch backstop in
/// [`try_run_compiled`] ALSO catches them lazily; this removes the affected callers
/// EAGERLY while leaving unrelated callers cached (no per-redefinition re-JIT churn).
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
pub(crate) fn evict_compiled(id: u64) {
    COMPILED.with(|c| c.borrow_mut().remove(id));
    OSR_CACHE.with(|c| c.borrow_mut().retain(|(fid, _), _| *fid != id));
}

pub(crate) fn evict_inline_dependents(sym: SymId) {
    let Some(dependents) = INLINE_DEPS.with(|m| m.borrow_mut().remove(&sym)) else {
        return;
    };
    COMPILED.with(|cache| {
        let mut cache = cache.borrow_mut();
        for id in dependents {
            // Disjointness (spec-slot pointer safety): only INLINED-into caller
            // leaves are ever in a dep set, and resolve_compiled_leaf_ptr refuses to
            // cache an inlined leaf's pointer in a spec slot — so evicting one here
            // can never dangle a baked SpecSlot.leaf raw pointer.
            debug_assert!(
                !matches!(cache.get(id), Some(CacheEntry::Compiled(l)) if l.inline_epoch().is_none()),
                "precise eviction must only touch inlined leaves (spec-slot pointer safety)"
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
        Some(CacheEntry::Deferred) => "deferred",
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
        },
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
/// keeps it precise: an evicted leaf drops out automatically (no stale roots).
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

pub(crate) fn collect_jit_reloc_gc_roots(roots: &mut Vec<Value>) {
    sync_cache_to_current_heap();
    COMPILED.with(|c| {
        for entry in c.borrow().values() {
            if let CacheEntry::Compiled(leaf) = entry {
                roots.extend_from_slice(leaf.reloc_values());
            }
        }
    });
    // OSR leaves also bake heap-constant reloc vectors — root them too, else a GC
    // between an OSR compile and its next run could free the leaf's constants.
    OSR_CACHE.with(|c| {
        for (leaf, _) in c.borrow().values().flatten() {
            roots.extend_from_slice(leaf.reloc_values());
        }
    });
}

/// GC handshake size probe: `(total COMPILED cache entries, total reloc slots
/// the root walk visits)` — the O() inputs of `collect_jit_reloc_gc_roots`.
/// Read-only; called once per handshake OUTSIDE the timed pause window.
pub(crate) fn compiled_cache_probe() -> (usize, usize) {
    COMPILED.with(|c| {
        let cache = c.borrow();
        let slots = cache
            .values()
            .map(|entry| match entry {
                CacheEntry::Compiled(leaf) => leaf.reloc_values().len(),
                _ => 0,
            })
            .sum();
        (cache.len(), slots)
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
        for (id, leaf) in leaves {
            // INSERT-IF-ABSENT: never clobber a pre-existing entry (a JIT leaf the
            // hook compiled may be spec-slot-referenced + INLINE_DEPS-registered).
            // AOT leaves never inline → no inline deps to register; their reloc
            // consts are rooted via the COMPILED walk in collect_jit_reloc_gc_roots.
            if cache.get(id).is_none() {
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
    OSR_CACHE.with(|c| c.borrow_mut().clear());
    // Every remembered NotCompilable verdict is now as stale as the cache.
    REJECTION_EPOCH.fetch_add(1, Ordering::Relaxed);
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
    let leaf: Option<Rc<CompiledLeaf>> = COMPILED.with(|cache| {
        let mut cache = cache.borrow_mut();
        // SAFETY: the seam-provided Context is dormant for the whole native
        // dispatch (see neovm_jit_call's contract); a shared read of its obarray
        // for compile-time speculation. A null ctx (shim-free test bodies) just
        // disables speculation.
        let obarray = (!ctx.is_null()).then(|| unsafe { &(*ctx).obarray });
        // Re-JIT a STALE INLINED leaf: if it inlined a callee and the obarray's
        // function_epoch has since moved, a callee it inlined may have been
        // redefined — drop the entry so it recompiles below (no stale inline runs).
        let stale = matches!(
            cache.get(id),
            Some(CacheEntry::Compiled(l))
                if l.inline_epoch().is_some()
                    && l.inline_epoch() != obarray.map(|ob| ob.function_epoch())
        );
        // Re-tier: a leaf built with the fast register allocator that the
        // interpreter keeps entering has proven hot — rebuild it with the
        // full allocator (`retier_heat`). A leaf that already earned `Full`
        // keeps it when a stale inline makes it recompile: the tier is
        // earned, not re-decided. Never under a forced allocator, which
        // would rebuild `Fast` forever.
        let (prev_regalloc, prev_bypassed) = match cache.get(id) {
            Some(CacheEntry::Compiled(l)) => (Some(l.regalloc), l.profit_gate_bypassed),
            _ => (None, false),
        };
        let retier = prev_regalloc == Some(RegallocChoice::Fast)
            && forced_regalloc().is_none()
            && super::retier_heat().is_some_and(|at| func.jit_runtime().heat() >= at);
        if stale || retier {
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
        // A deferred body whose deferral ran out compiles now, gate bypassed;
        // one whose deferral still holds stays interpreted (the dispatcher
        // does not probe for it, but the funcall seam and tests may).
        let deferred = matches!(cache.get(id), Some(CacheEntry::Deferred));
        if deferred && !func.jit_runtime().profit_deferral_expired() {
            return None;
        }
        if deferred {
            cache.remove(id);
        }
        let request = CompileRequest {
            regalloc: policy,
            bypass_profit_gate: deferred || prev_bypassed,
        };
        match cache.get_or_insert_with(id, || {
            // R1c-6: consult AOT FIRST (additive — a miss/error falls through to
            // the JIT below, leaving JIT behavior unchanged). An AOT hit is a
            // PRE-WARMED leaf: native code already on disk, no JIT compile. Only
            // the required-only subset the AOT emitter supports is eligible
            // (no &optional/&rest — matches the MIR pure path's arity seeding).
            // AOT bodies bake every constant; a source with a make-closure
            // patched prefix needs the JIT's dynamic-prefix lowering.
            if super::aot::aot_enabled()
                && func.params.optional.is_empty()
                && func.params.rest.is_none()
                && func.jit_runtime().patched_prefix() == 0
            {
                let native_arity = func.params.required.len();
                if let Some(leaf) = super::aot::try_load_leaf(
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
                    stats::record_aot_load(func.executable_ops().len());
                    return CacheEntry::Compiled(Rc::new(leaf));
                }
            }
            compile_cache_entry(id, func, obarray, request)
        }) {
            // Only run native for a valid call (lambda-list range); a mismatch
            // is a wrong-arg-count call the interpreter must signal.
            CacheEntry::Compiled(leaf) if leaf.accepts(args.len()) => Some(Rc::clone(leaf)),
            _ => None,
        }
    });
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
            compile_cache_entry(
                id,
                func,
                obarray,
                CompileRequest {
                    regalloc: RegallocPolicy::Auto,
                    bypass_profit_gate: false,
                },
            )
        }) {
            // INLINED leaves must NOT be fast-path-cached in a spec slot: their
            // validity depends on an inlined callee's epoch, which the caller's
            // spec guard doesn't check. Force them through try_run_compiled (which
            // re-JITs on a stale epoch). Non-inlined leaves keep the stable-pointer
            // fast path (they are never epoch-stale, so the per-entry eviction
            // never drops them — only a wholesale clear() can, and that cannot
            // fire mid-native-execution; see resolve_compiled_leaf_ptr).
            CacheEntry::Compiled(leaf) if leaf.inline_epoch().is_none() => Some(Rc::as_ptr(leaf)),
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
    if leaf.direct_call_eligible() {
        let mut out: i64 = 0;
        // SAFETY: args_ptr addresses leaf.arity live words (the caller's
        // call-args slot, pure passthrough — checked by our caller); ctx is
        // the dormant seam Context.
        let status = unsafe {
            leaf.entry_call_raw_consts(ctx as *mut u8, func.constants.as_ptr(), args_ptr, &mut out)
        };
        if status == super::compile::STATUS_OK {
            NATIVE_OK_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return NativeCallOutcome::Value(Value::from_bits(out as usize));
        }
        count_native_status(status);
        return direct_call_cold(ctx, func, func_value, leaf, status);
    }
    let outcome = leaf.call_premarshaled_consts(ctx as *mut u8, func.constants.as_ptr(), args_ptr);
    count_native_outcome(&outcome);
    match outcome {
        NativeRun::Ok(bits) => NativeCallOutcome::Value(Value::from_bits(bits)),
        // call_premarshaled maps null-vmctx deopts to Deopt; defensive only —
        // the caller re-runs the callee on the interpreter.
        NativeRun::Deopt => NativeCallOutcome::Fallback,
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
        NativeRun::DeoptAt(resume) => deopt_resume_outcome(ctx, func, func_value, resume),
    }
}

/// Precise-deopt resume shared by the wrapped and direct native paths: run
/// the Tier-0 interpreter mid-function off the [`DeoptResume`] payload.
fn deopt_resume_outcome(
    ctx: *mut Context,
    func: &ByteCodeFunction,
    func_value: Value,
    resume: Box<crate::emacs_core::jit::compile::DeoptResume>,
) -> NativeCallOutcome {
    let crate::emacs_core::jit::compile::DeoptResume {
        pc,
        stack,
        handlers,
        binds,
        spec_base,
        cond_base,
    } = *resume;
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
fn direct_call_cold(
    ctx: *mut Context,
    func: &ByteCodeFunction,
    func_value: Value,
    leaf: &CompiledLeaf,
    status: i64,
) -> NativeCallOutcome {
    use super::compile::{STATUS_DEOPT_AT, STATUS_SIGNAL};
    if status == STATUS_SIGNAL {
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
            NativeRun::DeoptAt(resume) => deopt_resume_outcome(ctx, func, func_value, resume),
            // deopt_at_outcome only degrades to plain Deopt with a null vmctx.
            _ => NativeCallOutcome::Fallback,
        };
    }
    // STATUS_DEOPT: rerun-from-start — sound only for side-effect-free bodies
    // (the same defensive rule invoke_native applies).
    leaf.assert_rerunnable();
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
fn count_native_status(status: i64) {
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
    outcome: NativeRun,
) -> Result<Option<usize>, Flow> {
    count_native_outcome(&outcome);
    match outcome {
        NativeRun::Ok(bits) => Ok(Some(bits)),
        NativeRun::Deopt => Ok(None),
        NativeRun::DeoptAt(resume) => {
            let crate::emacs_core::jit::compile::DeoptResume {
                pc,
                stack,
                handlers,
                binds,
                spec_base,
                cond_base,
            } = *resume;
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
mod tests {
    use super::*;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::value::{LambdaParams, Value};

    fn nullary_fn(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.ops = ops;
        f.constants = constants.into();
        f.max_stack = 16;
        f
    }

    /// A body the JIT rejects is remembered on its runtime: after the one
    /// rejected compile the tier dispatcher answers `Interpret` for it without
    /// probing this cache again, until the cache forgets its verdicts.
    #[test]
    fn rejected_body_is_remembered_on_its_runtime() {
        use crate::emacs_core::jit::Plan;
        // A body with no `Return` is rejected (`CompileError::NoReturn`) at
        // the pipeline's first scan; the verdict's kind is irrelevant here,
        // only that the cache records `NotCompilable` for it.
        let f = nullary_fn(vec![Op::Nil], vec![]);
        let rt = f.jit_runtime();
        let len = f.executable_ops().len();
        rt.set_hot_for_test();
        assert!(
            matches!(rt.dispatch_sized(len), Plan::Compiled),
            "hot and unjudged: the dispatcher consults the cache"
        );
        assert_eq!(
            try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
            None,
            "the rejected body falls back to the interpreter"
        );
        assert!(!is_compiled_for_test(
            rt.compiled_id().expect("the probe assigned an id")
        ));
        assert!(
            matches!(rt.dispatch_sized(len), Plan::Interpret),
            "the verdict is remembered: no more seam trips"
        );
        clear();
        assert!(
            matches!(rt.dispatch_sized(len), Plan::Compiled),
            "the cache forgot its verdicts, so the dispatcher probes again"
        );
    }

    /// A straight-line body compiles with the fast allocator, and re-tiers to
    /// the full one — in place, still running — once its heat proves it hot.
    #[test]
    fn fast_leaf_retiers_to_full_once_hot() {
        if forced_regalloc().is_some() {
            return; // the A/B knob overrides the policy
        }
        let Some(retier_at) = crate::emacs_core::jit::retier_heat() else {
            return; // NEOVM_JIT_RETIER_FACTOR=0
        };
        let c = Value::make_int(7);
        let f = nullary_fn(vec![Op::Constant(0), Op::Return], vec![c]);
        let run = || try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap();
        assert_eq!(run(), Some(c.bits()));
        let id = f.jit_runtime().compiled_id().expect("compiled once");
        assert_eq!(compiled_regalloc_for_test(id), Some(RegallocChoice::Fast));
        assert_eq!(run(), Some(c.bits()));
        assert_eq!(
            compiled_regalloc_for_test(id),
            Some(RegallocChoice::Fast),
            "cold: no re-tier"
        );
        f.jit_runtime().set_heat_for_test(retier_at);
        assert_eq!(run(), Some(c.bits()));
        assert_eq!(
            compiled_regalloc_for_test(id),
            Some(RegallocChoice::Full),
            "hot: rebuilt full"
        );
        assert_eq!(run(), Some(c.bits()));
        assert_eq!(
            compiled_regalloc_for_test(id),
            Some(RegallocChoice::Full),
            "and stays full"
        );
    }

    /// A body the profitability gate refuses is DEFERRED, not vetoed, when the
    /// factor is set: the dispatcher interprets it without probing until its
    /// heat reaches the deferral point, then the compile runs with the gate
    /// bypassed and the body runs native. With the factor at 0 it is refused
    /// for good, as before.
    #[test]
    fn unprofitable_body_is_deferred_then_compiled_once_hot_enough() {
        use crate::emacs_core::jit::{Plan, force_profit_defer_for_test, hot_threshold};
        if !super::super::compile::jit_profit_gate_on() {
            return; // NEOVM_JIT_PROFIT=off: nothing is ever unprofitable
        }
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let ctx = &mut ev as *mut crate::emacs_core::eval::Context;
        // (lambda () (length "abc")): one builtin call, no arithmetic → the gate refuses it.
        let body = || {
            nullary_fn(
                vec![
                    Op::Constant(0),
                    Op::CallBuiltinSym(crate::emacs_core::intern::intern("length"), 1),
                    Op::Return,
                ],
                vec![Value::string("abc")],
            )
        };
        // Factor 0: the veto — NotCompilable, remembered on the runtime.
        force_profit_defer_for_test(Some(0));
        let f = body();
        let rt = f.jit_runtime();
        rt.set_hot_for_test();
        assert_eq!(try_run_compiled(ctx, &f, Value::NIL, &[]).unwrap(), None);
        let id = rt.compiled_id().expect("id assigned");
        assert_eq!(cache_entry_kind_for_test(id), "not-compilable");
        assert!(matches!(rt.dispatch_sized(3), Plan::Interpret));
        // Factor 4: deferred to 4× the threshold, interpreted meanwhile.
        force_profit_defer_for_test(Some(4));
        let f = body();
        let rt = f.jit_runtime();
        rt.set_hot_for_test();
        assert_eq!(try_run_compiled(ctx, &f, Value::NIL, &[]).unwrap(), None);
        let id = rt.compiled_id().expect("id assigned");
        assert_eq!(cache_entry_kind_for_test(id), "deferred");
        assert!(
            matches!(rt.dispatch_sized(3), Plan::Interpret),
            "deferred: no probe"
        );
        assert_eq!(
            try_run_compiled(ctx, &f, Value::NIL, &[]).unwrap(),
            None,
            "a probe anyway: still deferred"
        );
        assert_eq!(cache_entry_kind_for_test(id), "deferred");
        // The deferral runs out: the compile runs with the gate bypassed and
        // the body is cached as compiled. (Its native run's builtin dispatch is
        // not this test's claim — the minimal harness cannot serve `length`
        // through the generic shim path — so the run's outcome is not asserted.)
        rt.set_heat_for_test(hot_threshold().saturating_mul(4));
        assert!(matches!(rt.dispatch_sized(3), Plan::Compiled));
        let _ = try_run_compiled(ctx, &f, Value::NIL, &[]);
        assert_eq!(cache_entry_kind_for_test(id), "compiled");
        force_profit_defer_for_test(None);
    }

    #[test]
    fn runs_compilable_nullary_leaf() {
        let c = Value::make_int(42);
        let f = nullary_fn(vec![Op::Constant(0), Op::Return], vec![c]);
        // First call compiles + caches; result is the constant's bits.
        assert_eq!(
            try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
            Some(c.bits())
        );
        // Second call hits the cache; same result.
        assert_eq!(
            try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
            Some(c.bits())
        );
    }

    #[test]
    fn returns_none_for_noncompilable_body() {
        // Switch is unsupported -> NotCompilable -> None (interpreter fallback).
        let f = nullary_fn(
            vec![Op::Nil, Op::Nil, Op::Switch, Op::Nil, Op::Return],
            vec![],
        );
        assert_eq!(
            try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
            None
        );
        assert_eq!(
            try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
            None
        );
    }

    #[test]
    fn deopt_returns_none() {
        // MOST_POSITIVE + 1 overflows fixnum range -> native deopts -> None.
        let f = nullary_fn(
            vec![Op::Constant(0), Op::Constant(1), Op::Add, Op::Return],
            vec![
                Value::make_int(Value::MOST_POSITIVE_FIXNUM),
                Value::make_int(1),
            ],
        );
        assert_eq!(
            try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
            None
        );
    }

    #[test]
    fn metering_records_one_compile_per_cache_miss() {
        stats::reset_compile_stats();
        let c = Value::make_int(7);
        let f = nullary_fn(vec![Op::Constant(0), Op::Return], vec![c]);
        assert_eq!(
            try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
            Some(c.bits())
        );
        let s = stats::compile_stats_snapshot();
        assert_eq!(s.total_compiles, 1);
        assert_eq!(s.compiled_ok, 1);
        assert_eq!(s.not_compilable, 0);
        assert_eq!(s.not_profitable, 0);
        assert_eq!(s.aot_loads, 0);
        assert!(s.total_us > 0, "a real compile takes measurable time");
        assert_eq!(s.max_us, s.total_us);
        assert_eq!(s.max_fn_len, 2, "ops.len() of the worst (only) compile");
        assert_eq!(s.histogram_us.iter().sum::<u64>(), 1);
        assert_eq!(s.histogram_us[stats::bucket_index(s.max_us)], 1);
        // Second call is a cache hit: no new compile is recorded.
        assert_eq!(
            try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
            Some(c.bits())
        );
        assert_eq!(stats::compile_stats_snapshot().total_compiles, 1);
    }

    #[test]
    fn metering_aggregates_across_compiles() {
        stats::reset_compile_stats();
        for i in 0..64 {
            let c = Value::make_int(i);
            let f = nullary_fn(vec![Op::Constant(0), Op::Return], vec![c]);
            assert_eq!(
                try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
                Some(c.bits())
            );
        }
        let s = stats::compile_stats_snapshot();
        assert_eq!(s.total_compiles, 64);
        assert_eq!(s.compiled_ok, 64);
        assert_eq!(s.histogram_us.iter().sum::<u64>(), 64);
        assert!(s.max_us <= s.total_us);
        assert_eq!(s.max_fn_len, 2);
    }

    #[test]
    fn metering_counts_noncompilable_outcome() {
        stats::reset_compile_stats();
        let f = nullary_fn(
            vec![Op::Nil, Op::Nil, Op::Switch, Op::Nil, Op::Return],
            vec![],
        );
        assert_eq!(
            try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
            None
        );
        let s = stats::compile_stats_snapshot();
        assert_eq!(s.total_compiles, 1);
        assert_eq!(s.compiled_ok, 0);
        assert_eq!(s.not_compilable, 1);
    }

    #[test]
    fn assigns_stable_unique_ids() {
        let f1 = nullary_fn(vec![Op::Nil, Op::Return], vec![]);
        let f2 = nullary_fn(vec![Op::Nil, Op::Return], vec![]);
        let a = f1.jit_runtime().compiled_id_or_assign();
        let a_again = f1.jit_runtime().compiled_id_or_assign();
        let b = f2.jit_runtime().compiled_id_or_assign();
        assert_eq!(a, a_again, "id is stable per function");
        assert_ne!(a, b, "distinct functions get distinct ids");
        assert_ne!(a, 0, "0 is reserved for unassigned");
    }

    #[test]
    fn runs_with_args_and_rejects_arity_mismatch() {
        // (lambda (a b) (+ a b)), lexical so params are on the stack.
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![
                crate::emacs_core::intern::SymId(1),
                crate::emacs_core::intern::SymId(2),
            ],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
        f.max_stack = 16;
        // Correct arity -> native result.
        assert_eq!(
            try_run_compiled(
                std::ptr::null_mut(),
                &f,
                Value::NIL,
                &[Value::make_int(40), Value::make_int(2)]
            )
            .unwrap(),
            Some(Value::make_int(42).bits())
        );
        // Wrong arity -> None (interpreter will signal wrong-number-of-arguments).
        assert_eq!(
            try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[Value::make_int(40)]).unwrap(),
            None
        );
    }
}
