//! JIT runtime shims: the C-ABI functions compiled code calls (GC save/push/restore, cons, rootwin grow, the general call and apply shims, the eq/symbolp slow paths), the status/counter constants, the deferred-flow stash, panic containment, and the shim address anchor.
//!
//! Moved out of `compile.rs` unchanged; a child module so it keeps the
//! parent's view of its private items (`use super::*`).

use super::*;

/// Snapshot the scratch-root depth so it can be restored after a rooted region.
///
/// `#[unsafe(no_mangle)] pub` (audit #3 / R1c call-bearing): an AOT `.so` that
/// makes a runtime call imports this shim by its BARE name and binds it at
/// `dlopen` against the host's dynamic symbol table (the host/test binary is
/// linked `-rdynamic`). The JIT path is unaffected — it binds shims by ADDRESS
/// through [`register_shims`], so the only effect of `no_mangle` is an exported
/// symbol name. All shims the MIR tier can emit ([`MIR_SHIM_NAMES`]) carry it.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_gc_save() -> i64 {
    save_scratch_gc_roots() as i64
}

/// Root one live `Value` (by its raw bits) across an upcoming allocation.
///
/// Only heap objects (cons/string/float/veclike incl. bignum) can be collected
/// and need stack rooting; immediates (fixnums, chars, nil/t) are never on the
/// heap, and symbols are kept live by the obarray (always a GC root), not by the
/// operand stack. Skipping those here is correct — `mark_value` would no-op on
/// them anyway — and avoids the thread-local push for the many symbol/fixnum
/// operands the JIT roots before calls. `gc_restore` truncates to the saved
/// depth, so a variable push count is fine.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_gc_push(bits: i64) {
    let v = Value::from_bits(bits as usize);
    if v.is_heap_object() {
        push_scratch_gc_root(v);
    }
}

/// Root a BATCH of `count` live residual `Value`s (raw bits at `ptr[0..count]`)
/// across an upcoming allocation/call — the same heap-only filter as
/// [`neovm_jit_gc_push`], but ONE shim call for the whole residual set instead of
/// N (lever 2). The generated code stores each not-provably-immediate residual
/// into a stack buffer at a STATIC offset (cheap `stack_store`, no per-value shim
/// call) and calls this once; this tight loop re-tests `is_heap_object` and pushes
/// the heap ones. Byte-compile profiles showed `neovm_jit_gc_push` as the #1 hot
/// symbol (7.35%): heap-dense residuals paid a full function call each.
///
/// SAFETY: `ptr` addresses exactly `count` tagged-bits words the generated code
/// wrote immediately before this call (its residual buffer); `count >= 0`.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_gc_push_many(ptr: *const i64, count: i64) {
    for i in 0..count {
        // SAFETY: the generated code wrote `count` tagged words at `ptr`.
        let v = Value::from_bits(unsafe { *ptr.add(i as usize) } as usize);
        if v.is_heap_object() {
            push_scratch_gc_root(v);
        }
    }
}

/// Pop the scratch roots back to a saved depth.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_gc_restore(saved: i64) {
    restore_scratch_gc_roots(saved as usize);
}

/// Allocate `(cons car cdr)`. Roots car+cdr across the allocation itself; the
/// caller roots any *other* live values first.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_cons(car: i64, cdr: i64) -> i64 {
    // No rooting: `Value::cons` is pure allocation (`alloc_cons` — free-list
    // pop or block bump, allocate-black under a concurrent cycle) and never
    // reaches a GC safe point, so nothing can collect while `car`/`cdr` are
    // in flight. Root snapshots happen only at mutator safe points (the
    // backedge/call shims, which root their live values) — the same invariant
    // that lets jitted code hold values in native slots between polls at all.
    let car = Value::from_bits(car as usize);
    let cdr = Value::from_bits(cdr as usize);
    Value::cons(car, cdr).bits() as i64
}

/// Box an `f64` the compiled code computed in a register into a Lisp float.
///
/// No rooting and no vmctx, for exactly the reason [`neovm_jit_cons`] needs
/// none: `alloc_float` is a pure arena slot write (allocate-black under a
/// concurrent cycle) and never reaches a GC safe point, so nothing can collect
/// while the caller's other values sit in native slots.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_make_float(value: f64) -> i64 {
    crate::tagged::gc::with_tagged_heap(|heap| heap.alloc_float_inline(value)).bits() as i64
}

/// Cold overflow path of the JIT residual-root window (see
/// `emit_root_window_stores`): grow the ctx root stack to hold `need` slots
/// and republish the ptr/cap mirrors baked field-offset loads read.
/// SAFETY: vmctx contract (see `neovm_jit_call`); pure Vec growth — no lisp,
/// no GC, no safe point.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_rootwin_grow(ctx: *mut u8, need: i64) {
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    ctx.jit_root_stack_grow(need as usize);
}

std::thread_local! {
    /// The non-local `Flow` (signal/throw/...) raised inside a runtime call made
    /// by JIT code. The call shim stashes it and returns [`STATUS_SIGNAL`]; the
    /// nearest Rust caller of the compiled function takes it and re-raises.
    /// Thread-local because compiled code and its dispatch run on one thread.
    pub(crate) static PENDING_FLOW: std::cell::RefCell<Option<Flow>> = const { std::cell::RefCell::new(None) };
}

/// Native return code: success, result bits written through `out`.
pub const STATUS_OK: i64 = 1;
/// Native return code: a speculation guard failed before any side effect ran —
/// rerun the body on the Tier-0 interpreter.
pub const STATUS_DEOPT: i64 = 0;
/// Native return code: a runtime call raised a non-local `Flow`; take it with
/// [`take_pending_flow`] and propagate.
pub const STATUS_SIGNAL: i64 = 2;

/// Native return code: a speculation guard failed at a PRECISE bytecode pc —
/// the live operand stack was spilled into the leaf's deopt buffer and the
/// frame's binds/handlers were left REGISTERED (no frame unwind): the caller
/// resumes the Tier-0 interpreter mid-function via `Vm::run_resumed_frame`.
/// Unlike [`STATUS_DEOPT`], this is sound even after side effects ran.
pub const STATUS_DEOPT_AT: i64 = 3;

/// Native return code (subr spec shims only, never crosses the leaf entry
/// ABI): the speculated-SUBR site could not run its direct fast path (binding
/// changed / compiler function overrides active / bitwise-eq miss on the
/// `equal-including-properties` shim). NO work was done beyond the quit poll:
/// the generated code branches to its per-site FALLBACK block, which performs
/// the ORIGINAL generic `Op::Call` lowering (arg spill + residual rooting +
/// `neovm_jit_call` on the constant symbol). Salted into the AOT `ABI_TAG`
/// like the other STATUS codes (`aot::compute_abi_tag`), although AOT leaves
/// can never contain subr spec sites (speculation requires `Some(obarray)`;
/// AOT compiles at `None`).
pub const STATUS_NEED_GENERIC: i64 = 4;

/// Test/debug-build counter of speculated direct-call shim entries (evidence
/// that `find_spec_sites` + the spec lowering actually engage). Release unit
/// tests need this too; non-test release builds carry no counter.
#[cfg(any(test, debug_assertions))]
pub(crate) static SPEC_CALL_COUNT: AtomicU64 = AtomicU64::new(0);

/// Test/debug-build counter of V3 fast-path engagements: a speculated call that
/// ran the cached callee leaf DIRECTLY (skipping funcall dispatch + the cache
/// hash lookup), as opposed to falling back to `call_for_jit`. Test evidence
/// that the fast path actually fires instead of silently no-op'ing.
#[cfg(any(test, debug_assertions))]
pub(crate) static SPEC_FAST_CALL_COUNT: AtomicU64 = AtomicU64::new(0);

/// Test/debug-build counter of the spec shim's OWN fast path (an armed slot
/// whose leaf takes the call as laid out, entered from the shim's frame):
/// distinct from [`SPEC_FAST_CALL_COUNT`], which the slow half's
/// `call_armed_callee_native` also bumps when it runs a cached leaf.
#[cfg(any(test, debug_assertions))]
pub(crate) static SPEC_SHIM_FAST_COUNT: AtomicU64 = AtomicU64::new(0);

/// Debug-build counter of speculated direct-SUBR shim entries (all three
/// shims: general / predicate / equal-including-properties). Test evidence
/// that the subr-kind sites in `find_spec_sites` + their lowering engage.
#[cfg(debug_assertions)]
pub(crate) static SUBR_SPEC_COUNT: AtomicU64 = AtomicU64::new(0);

/// Debug-build counter of subr-spec FAST completions: the site was armed and
/// the shim finished the call itself (direct subr dispatch, predicate tag
/// test, or the bitwise-eq hit) — as opposed to bouncing to the generic
/// fallback block. Proves the fast path fires instead of silently bouncing.
#[cfg(debug_assertions)]
pub(crate) static SUBR_SPEC_FAST_COUNT: AtomicU64 = AtomicU64::new(0);

/// Debug-build counter of subr-spec [`STATUS_NEED_GENERIC`] bounces (binding
/// changed, overrides active, or an eq-shim bitwise miss). Together with
/// [`SUBR_SPEC_FAST_COUNT`] this lets tests prove a site RE-ARMED after an
/// unrelated epoch bump (fast count grows, generic count doesn't).
#[cfg(debug_assertions)]
pub(crate) static SUBR_SPEC_GENERIC_COUNT: AtomicU64 = AtomicU64::new(0);

/// Debug-build counters for the R2 CallBuiltinSym intrinsic shims (Tier-A read
/// `neovm_jit_cbsym_read` [COMMIT 5] + Tier-B dispatch-skip
/// [`neovm_jit_cbsym_spec`]), mirroring `SUBR_SPEC_{COUNT,FAST,GENERIC}`. COUNT
/// = shim entries; FAST = the
/// shim completed the op itself; GENERIC = a [`STATUS_NEED_GENERIC`] bounce
/// (the fresh static entry is no longer a plain builtin, `current-buffer`'s
/// value was never materialized, or the `NEOVM_JIT_FORCE_CBSYM_GENERIC`
/// harness). Engagement tests read these to prove the fast path fires instead
/// of silently bouncing to the general CBSym lowering.
#[cfg(debug_assertions)]
pub(crate) static CBSYM_SPEC_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(debug_assertions)]
pub(crate) static CBSYM_SPEC_FAST_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(debug_assertions)]
pub(crate) static CBSYM_SPEC_GENERIC_COUNT: AtomicU64 = AtomicU64::new(0);

/// Take the `Flow` stashed by a shim that returned [`STATUS_SIGNAL`].
///
/// A panic contained at a shim boundary ([`jit_shim_contain!`]) wins over a
/// flow the shim body had already stashed before panicking: the panic
/// interrupted that flow's protocol midway, so reporting the panic is the
/// honest outcome. (Contrast the module ABI's first-exit-wins, which follows
/// GNU's pending-exit register semantics; `PENDING_FLOW` is a single-slot
/// handoff consumed by the very next dispatch step, not a module-visible
/// register.) The Lisp-heap allocation for the panic-message string happens
/// HERE — the two consumers run at allocation-safe points (see
/// [`PENDING_SHIM_PANIC`]) — never in a shim's catch handler.
///
/// Panic-wins drops ANY stashed flow, including a `Flow::ThreadBlocked`
/// (the cooperative thread-yield handoff): the panicked extent can no
/// longer complete the yield protocol its shim had begun, so the blocked
/// thread's re-dispatch is abandoned along with the rest of that extent and
/// the panic error is what propagates. Accepted, documented trade-off.
pub fn take_pending_flow() -> Option<Flow> {
    let flow = PENDING_FLOW.with(|p| p.borrow_mut().take());
    match PENDING_SHIM_PANIC.with(|p| p.borrow_mut().take()) {
        Some(message) => Some(signal("error", vec![Value::string(&message)])),
        None => flow,
    }
}

pub(crate) fn stash_pending_flow(flow: Flow) {
    PENDING_FLOW.with(|p| *p.borrow_mut() = Some(flow));
}

// ---------------------------------------------------------------------------
// Panic containment at the JIT shim boundary.
//
// Every shim here is `extern "C"` and is called from Cranelift frames with no
// landing pads: a panic that escapes a shim body aborts the process ("panic
// in a function that cannot unwind"). The 21 shims whose return ABI has a
// signal channel — a `STATUS_*` code, `neovm_jit_match_handler`'s `-1`
// rethrow ordinal, `neovm_jit_switch`'s [`JIT_SWITCH_STALE`], or a body the
// generated code follows with an unconditional signal-exit branch
// (`neovm_jit_throw`, `neovm_jit_switch_stale`) — contain panics via
// [`jit_shim_contain!`]: the panic becomes a pending
// `Signal(error, "neomacs internal error: …")` and the shim returns its
// signal sentinel, so the EXISTING plumbing takes over (generated code
// branches to its signal exit or handler match, the leaf returns
// STATUS_SIGNAL, the dispatcher takes the flow and propagates a normal Lisp
// error — condition-case-able, editor keeps running).
//
// The wrapper itself is a bare `catch_unwind`: shims are the JIT's hot path
// (the per-backedge quit poll runs once per 255 loop iterations, `varref` on
// every dynamic read), so the happy path carries ZERO containment cost — no
// snapshot, no root bookkeeping, nothing before the catch besides the body.
// Restoration is instead anchored at LEAF ENTRY: `invoke_native` records the
// evaluator bases once per native call ([`JitLeafBases`]), and a contained
// panic is healed against them downstream, at the two points that see it —
// `neovm_jit_match_handler` entry (leaf-local condition-case) and the
// leaf-exit path in `invoke_native`. Leaf-entry bases are exact there
// because every healed field is balanced across each shim call the leaf
// makes; the residue a panic leaves is precisely "above the bases". See
// `Context::restore_jit_shim_boundary` for the field-by-field story.
//
// The other 19 shims (`gc_save`/`gc_push`/`gc_restore`, `cons`, `list`, the
// pure predicate slow paths, `varbind`/`unbind`, the specpdl/handler-frame
// pushes and `pop_handler`) have NO signal channel: generated code consumes
// their return value or assumes the state transition happened,
// unconditionally. "Containing" a panic there would mean returning garbage
// and letting native code keep executing against half-applied state — e.g. a
// fabricated `gc_save` depth would make the paired `gc_restore` truncate
// LIVE scratch roots (use-after-free under exact GC). Strictly worse than
// the abort, so they stay unwrapped: a panic there keeps today's behavior
// (abort at the `extern "C"` boundary).
// ---------------------------------------------------------------------------

std::thread_local! {
    /// Message of a panic contained at a shim boundary, pending
    /// materialization into a `Flow` by [`take_pending_flow`]. A Rust
    /// `String`, never a Lisp value: several shims are called with the
    /// generated code's residual operand stack UNROOTED
    /// ([`neovm_jit_pred_spec`] & friends), so the catch handler must not
    /// allocate on the Lisp heap — an allocation-triggered GC could sweep
    /// values a leaf-local handler is about to resume with. The two pending-
    /// flow consumers run at allocation-safe points instead:
    /// [`neovm_jit_match_handler`]'s entry take (the generated code rooted
    /// its live values before the call) and the dispatcher's
    /// `finish_native_run` (the leaf has exited; its residual stack is
    /// discarded).
    ///
    /// While set, it doubles as the "panicked-extent residue pending" marker
    /// the two healing points test ([`shim_panic_pending`]): set at
    /// containment, consumed by the take — which both healing points run
    /// strictly before.
    pub(crate) static PENDING_SHIM_PANIC: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };

    /// Evaluator bases of the innermost native leaf currently executing,
    /// recorded by `invoke_native` once per native call and stack-restored
    /// around it (nested dispatch replaces and restores, so this always
    /// names the innermost leaf). Read only by the contained-panic healing
    /// points; `None` outside any native extent.
    ///
    /// A POINTER into `invoke_native`'s stack frame, not the value: the
    /// snapshot is ~100 bytes, and value-swapping it through the Cell twice
    /// per native call was 42% of a recursion benchmark's CPU once the spec
    /// fast path made per-call costs the bottleneck. SAFETY (pointee
    /// liveness): the pointer is published immediately before the native
    /// entry call and restored to the outer value immediately after, so it
    /// only ever names a frame that is live for that whole window; the only
    /// readers (contain_jit_shim_panic, heal_shim_panic_residue_before_match)
    /// run inside shims during that window, on the same single Lisp thread.
    /// Shim panics are contained INSIDE the shim (the leaf exits via its
    /// STATUS_SIGNAL path and `invoke_native` continues), and a re-raised
    /// panic aborts at the extern "C" boundary — no path unwinds through
    /// `invoke_native` leaving a stale pointer published.
    pub(crate) static CURRENT_LEAF_BASES: std::cell::Cell<Option<std::ptr::NonNull<JitLeafBases>>> =
        const { std::cell::Cell::new(None) };

    /// Scratch-GC-root depth a contained panic left dead pushes above (the
    /// panicked extent's skipped pops — including the shim body's own).
    /// Separate from [`PENDING_SHIM_PANIC`] because it must SURVIVE the
    /// flow's consumption: when a leaf-local handler catches the contained
    /// panic, the leaf resumes and only its eventual EXIT (possibly
    /// `STATUS_OK`, much later) can sweep the residue — the match path may
    /// not touch the root stack while the dispatch block's live roots sit
    /// on top. A leaf exit sweeps and clears it only when the floor is at
    /// or above its own entry depth (`floor >= bases.roots`); a floor below
    /// belongs to an outer leaf's residue and is left for that leaf's exit.
    pub(crate) static PENDING_ROOT_SWEEP_FLOOR: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

/// Leaf-entry restoration bases. Recorded once per native call by
/// `invoke_native`; the unit of restoration for a contained shim panic is
/// the whole leaf, so leaf entry is the one coherent base point. The
/// scratch-GC-root depth rides inside the snapshot
/// ([`ModuleBoundarySnapshot::scratch_gc_roots_len`]); here it is only the
/// FLOOR for the deferred pending-root sweep — `restore_jit_shim_boundary`
/// never truncates roots itself (the sweep owns that lifecycle).
#[derive(Clone, Copy)]
pub(crate) struct JitLeafBases {
    pub(crate) snap: ModuleBoundarySnapshot,
}

impl JitLeafBases {
    /// Leaf-entry scratch-root depth (the pending-sweep floor).
    pub(crate) fn roots(&self) -> usize {
        self.snap.scratch_gc_roots_len()
    }
}

/// Whether a shim panic was contained and its residue not yet healed/taken.
pub(crate) fn shim_panic_pending() -> bool {
    PENDING_SHIM_PANIC.with(|p| p.borrow().is_some())
}

/// Handle a panic caught inside a wrapped shim body: probe, arm, stash.
/// Returns the payload back (`Err`) when the panic must NOT be contained —
/// the caller `resume_unwind`s it, which aborts at the `extern "C"` shim,
/// exactly the pre-containment behavior for that class.
///
/// The unrecoverable probe is the module boundary's
/// (`Context::module_panic_recovery_blocked`: the `gc_driver_active` flag +
/// GC lock poison). The ctx-less wrapped shims
/// (`builtin_slice`/`throw`/`switch_stale`) have no Context to read the
/// driver flag through; they probe the lock-poison half via the
/// thread-local heap — same probe, partially applied.
///
/// Deliberately does NO state restoration (it runs off the recorded bases at
/// the healing points instead — see the section comment): this keeps the
/// wrapper's happy path free of per-call snapshot work. It never runs lisp
/// and never allocates on the Lisp heap.
#[cold]
#[inline(never)]
pub(crate) fn contain_jit_shim_panic(
    ctx: *mut u8,
    payload: Box<dyn std::any::Any + Send>,
) -> Result<(), Box<dyn std::any::Any + Send>> {
    let blocked = if ctx.is_null() {
        crate::tagged::gc::with_tagged_heap(|h| h.gc_locks_poisoned())
    } else {
        // SAFETY: seam-provided dormant Context (the shim's own vmctx
        // contract); the probe only reads two flags.
        unsafe { (*(ctx as *const Context)).module_panic_recovery_blocked() }
    };
    if blocked {
        eprintln!(
            "neomacs: refusing to contain a JIT-shim panic (GC state suspect): {}",
            panic_message(&*payload)
        );
        return Err(payload);
    }
    // Arm the root sweep for the leaf exit: the panicked extent's skipped
    // pops all sit at or above the innermost leaf's entry depth. (No bases
    // means no native extent — direct-call tests; nothing to sweep.)
    if let Some(bases_ptr) = CURRENT_LEAF_BASES.with(|b| b.get()) {
        // SAFETY: published for the dynamic extent of the innermost native
        // call, whose `invoke_native` frame owns the pointee (thread_local doc).
        let bases = unsafe { *bases_ptr.as_ptr() };
        PENDING_ROOT_SWEEP_FLOOR.with(|f| {
            let floor = f.get().map_or(bases.roots(), |cur| cur.min(bases.roots()));
            f.set(Some(floor));
        });
    }
    PENDING_SHIM_PANIC.with(|p| {
        *p.borrow_mut() = Some(format!(
            "neomacs internal error: {}",
            panic_message(&*payload)
        ));
    });
    Ok(())
}

/// Heal the panicked extent's evaluator residue before the match shim
/// matches: truncate against the leaf-entry bases, keeping the leaf's own
/// `ours` condition frames (they sit directly on the entry base — callees
/// pop their own frames on every non-panic exit, so only the panicked
/// extent's leaked frames are above them). Must run BEFORE
/// [`take_pending_flow`] materializes the panic: signal dispatch scans the
/// condition stack for the innermost match, and a leaked dead frame could
/// otherwise be selected. Scratch roots are deliberately untouched — the
/// dispatch block's live roots are on top (see the floor doc).
#[cold]
pub(crate) fn heal_shim_panic_residue_before_match(ctx: &mut Context, ours: usize) {
    let Some(bases_ptr) = CURRENT_LEAF_BASES.with(|b| b.get()) else {
        return;
    };
    // SAFETY: published for the dynamic extent of the innermost native call,
    // whose `invoke_native` frame owns the pointee (thread_local doc); the
    // match shim runs inside that extent.
    let bases = unsafe { *bases_ptr.as_ptr() };
    ctx.restore_jit_shim_boundary(&bases.snap, bases.snap.condition_len() + ours);
}

/// Wrap a signal-channel shim body: `catch_unwind` around `$body`; a caught
/// panic is contained per [`contain_jit_shim_panic`] and the shim returns
/// `$sentinel` (its ABI's signal-path value), or is re-raised (abort at the
/// `extern "C"` boundary) when GC state is suspect. Mid-body `return`s keep
/// their exact semantics — they return from the closure. The `no_ctx` arm is
/// for the shims whose bodies cannot reach the Context.
///
/// The happy path is the bare catch: no snapshot, no root bookkeeping —
/// shims are the JIT's hot path, and all restoration state lives in the
/// per-native-call [`JitLeafBases`] instead.
///
/// `AssertUnwindSafe`: the downstream healing against the leaf-entry bases
/// plus the deferred specpdl unwind are what make the crossing state
/// coherent; a contained panic never resumes the broken computation (the
/// sentinel routes generated code to its signal exit).
macro_rules! jit_shim_contain {
    // Literal-`no_ctx` arm FIRST: `no_ctx` would also parse as an `expr`, so
    // ordering is what keeps it from being captured by the ctx arm below.
    (no_ctx, $sentinel:expr, $body:expr) => {{
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            Ok(v) => v,
            Err(payload) => match contain_jit_shim_panic(core::ptr::null_mut(), payload) {
                Ok(()) => $sentinel,
                Err(payload) => std::panic::resume_unwind(payload),
            },
        }
    }};
    ($ctx:expr, $sentinel:expr, $body:expr) => {{
        let ctx_raw: *mut u8 = $ctx;
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            Ok(v) => v,
            Err(payload) => match contain_jit_shim_panic(ctx_raw, payload) {
                Ok(()) => $sentinel,
                Err(payload) => std::panic::resume_unwind(payload),
            },
        }
    }};
}

/// Re-export the textual macro on a path so `compile.rs` (the parent) and the
/// sibling child modules can call it; `macro_rules!` is otherwise visible only
/// below its definition and to descendants.
pub(crate) use jit_shim_contain;

/// Call a function from JIT code with the interpreter's `Op::Call` semantics
/// (quit poll, writeback, depth guard — see `Vm::call_for_jit`). Reads `nargs`
/// argument words from `args_ptr`; on success writes the result bits through
/// `out` and returns [`STATUS_OK`]; on a non-local exit stashes the `Flow` and
/// returns [`STATUS_SIGNAL`].
///
/// SAFETY contract with the generated code and the dispatch seam:
/// - `ctx` is the `*mut Context` the seam passed into this invocation of the
///   compiled function. The seam's `&mut Context` is dormant for the entire
///   native call (it is not touched until the compiled function returns), the
///   elisp mutator is single-threaded, and the pointer round-trips through
///   native code — so reconstructing `&mut Context` here does not create a
///   *used* aliasing `&mut`.
/// - `args_ptr` points at `nargs` valid argument words (a JIT stack slot).
/// - The generated code rooted every *other* live `Value` of its frame before
///   this call; the callee + args are rooted here, so a GC inside the callee
///   traces everything that survives the call.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_call(
    ctx: *mut u8,
    func_bits: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let func_val = Value::from_bits(func_bits as usize);
        let nargs = nargs as usize;
        // NATIVE-TO-NATIVE: the callee is already a bytecode object — what
        // the compiler emits for a `cl-flet` local, a `lambda` literal or any
        // closure held in a variable, where the object itself sits in the
        // constants vector. Pass the caller's call-args slot straight to its
        // leaf: no bc_buf staging, no premarshal, no scratch-root scope, no
        // `Vm` (the callee's backtrace frame roots it, and the poll returned
        // Ok without collecting). `None` = not fast-pathable (not yet tiered
        // up, wrong arity, debugger armed) and the general path runs it — and
        // tiers it up.
        //
        // Only this path lives in the shim's own frame: everything else is
        // `neovm_jit_call_general`. The apply fast path, added here as one
        // test with this one and then as its own, cost every call on this
        // path 4-6 instructions (elb flet +1.0%, then +1.7%).
        {
            // SAFETY: seam-provided dormant Context (fn-level contract).
            let ctx_ref = unsafe { &mut *(ctx as *mut Context) };
            if func_val.is_bytecode()
                && ctx_ref.maybe_quit_hot_ok()
                // SAFETY: `args_ptr` is the generated code's call-args slot,
                // holding `nargs` words for the whole call.
                && let Some(outcome) =
                    Vm::call_armed_bytecode_value_native(ctx_ref, func_val, args_ptr, nargs)
            {
                return native_call_status(outcome, out);
            }
        }
        neovm_jit_call_general(ctx, func_val, args_ptr, nargs, out)
    })
}

/// Deliver a native-to-native call's outcome to the generated code.
///
/// SAFETY: `out` is the generated code's result stack slot (the call-shim
/// contract).
#[inline(always)]
fn native_call_status(
    outcome: crate::emacs_core::jit::cache::NativeCallOutcome,
    out: *mut i64,
) -> i64 {
    use crate::emacs_core::jit::cache::NativeCallOutcome;
    match outcome {
        NativeCallOutcome::Value(value) => {
            // SAFETY: `out` is the generated code's result slot.
            unsafe { *out = value.bits() as i64 };
            STATUS_OK
        }
        NativeCallOutcome::FlowStashed => STATUS_SIGNAL,
        // `run_leaf_native_to_native` resolves its own Fallback through the
        // interpreter rerun.
        NativeCallOutcome::Fallback => {
            unreachable!("Fallback outcome at the generic call shim")
        }
    }
}

/// [`neovm_jit_call`] for every call its native-to-native path declines,
/// inside the shim's panic containment. Same contract.
#[inline(never)]
fn neovm_jit_call_general(
    ctx: *mut u8,
    func_val: Value,
    args_ptr: *const i64,
    nargs: usize,
    out: *mut i64,
) -> i64 {
    {
        // Polled path: no quit is due, so the arguments are staged on the
        // GC-traced `bc_buf` ONCE and both dispatches below read that same
        // span. The builtin probe is tried first (no Vm, no scratch roots
        // — interned symbol callees are obarray-rooted); anything else
        // falls through to the full path WITHOUT re-staging the arguments,
        // which is what the slow path below had to do when this block
        // owned its own push/truncate pair.
        // SAFETY: seam-provided dormant Context (fn-level contract).
        let ctx_ref = unsafe { &mut *(ctx as *mut Context) };
        if ctx_ref.maybe_quit_hot_ok() {
            // `(apply F ...)` into a compiled F, also without leaving native
            // code.
            if nargs >= 2
                && func_val.as_symbol_id() == Some(Vm::apply_builtin_id())
                && let Some(outcome) = Vm::call_apply_native(ctx_ref, func_val, args_ptr, nargs)
            {
                return native_call_status(outcome, out);
            }
            let args_start = ctx_ref.bc_buf.len();
            // SAFETY: generated code stored `nargs` words at args_ptr, and
            // `Value` is that word.
            ctx_ref.bc_buf.extend_from_slice(unsafe {
                core::slice::from_raw_parts(args_ptr as *const Value, nargs)
            });
            if func_val.is_symbol()
                && let Some(res) =
                    Vm::call_builtin_symbol_for_jit(ctx_ref, func_val, args_start, nargs)
            {
                ctx_ref.bc_buf.truncate(args_start);
                return jit_call_status(res, out);
            }
            // `maybe_quit_hot_ok` above tested exactly `maybe_quit`'s
            // fast-path condition (the same four loads plus the profiler
            // tick) and nothing between here and there can set any of
            // them, so the generic path does not poll a second time.
            let saved = save_scratch_gc_roots();
            // The callee is not on bc_buf, so it needs an explicit scratch
            // root across the call (which may GC); the arguments are
            // already rooted on bc_buf.
            push_scratch_gc_root(func_val);
            let mut vm = Vm::from_context(ctx_ref);
            let res = vm.call_for_jit_stack(func_val, args_start, nargs);
            vm.bc_buf_truncate(args_start);
            restore_scratch_gc_roots(saved);
            return jit_call_status(res, out);
        }
    }
    let saved = save_scratch_gc_roots();
    // The callee is not on bc_buf, so it needs an explicit scratch root across
    // the call (which may GC); the arguments go straight onto the GC-traced
    // bc_buf below, so they are rooted there — no LispArgVec, no per-arg root.
    push_scratch_gc_root(func_val);
    // SAFETY: see the function-level contract — seam-provided, dormant, single
    // mutator thread.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    let status = match ctx.maybe_quit() {
        Err(flow) => {
            stash_pending_flow(flow);
            STATUS_SIGNAL
        }
        Ok(()) => {
            // Push the native call-args slot straight onto bc_buf (GC-traced,
            // so the args are rooted across the call); the fast subr path reads
            // them in place. Truncate back afterwards.
            let args_start = ctx.bc_buf.len();
            // SAFETY: the generated code stored exactly `nargs` argument
            // words at `args_ptr` (its call-args slot) immediately before
            // this call, and `Value` is that word.
            ctx.bc_buf.extend_from_slice(unsafe {
                core::slice::from_raw_parts(args_ptr as *const Value, nargs)
            });
            let mut vm = Vm::from_context(ctx);
            let res = vm.call_for_jit_stack(func_val, args_start, nargs);
            vm.bc_buf_truncate(args_start);
            jit_call_status(res, out)
        }
    };
    restore_scratch_gc_roots(saved);
    status
}

/// Deliver a call shim's result to the generated code: the value bits through
/// its result slot, or the `Flow` stashed for the seam to re-raise.
///
/// SAFETY: `out` is the generated code's result stack slot, valid for a write
/// for the duration of the shim call (the call-shim contract).
#[inline]
fn jit_call_status(res: Result<Value, Flow>, out: *mut i64) -> i64 {
    match res {
        Ok(value) => {
            unsafe { *out = value.bits() as i64 };
            STATUS_OK
        }
        Err(flow) => {
            stash_pending_flow(flow);
            STATUS_SIGNAL
        }
    }
}

/// `apply` a function from JIT code with the interpreter's `Op::Apply`
/// semantics (quit poll first, last argument spread as a list, writeback, NO
/// nesting-depth guard — see `Vm::apply_for_jit`). Same SAFETY contract as
/// [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_apply(
    ctx: *mut u8,
    func_bits: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let func_val = Value::from_bits(func_bits as usize);
        let nargs = nargs as usize;
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(func_val);
        let mut args = LispArgVec::new();
        for i in 0..nargs {
            // SAFETY: the generated code stored exactly `nargs` argument words at
            // `args_ptr` (its call-args stack slot) immediately before this call.
            let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
            push_scratch_gc_root(v);
            args.push(v);
        }
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let status = match ctx.maybe_quit() {
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
            Ok(()) => {
                let mut vm = Vm::from_context(ctx);
                match vm.apply_for_jit(func_val, args) {
                    Ok(value) => {
                        // SAFETY: `out` is the generated code's result stack slot.
                        unsafe { *out = value.bits() as i64 };
                        STATUS_OK
                    }
                    Err(flow) => {
                        stash_pending_flow(flow);
                        STATUS_SIGNAL
                    }
                }
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// THE runtime-shim table: every `#[unsafe(no_mangle)] extern "C"` shim
/// generated code may call, paired with its address. It serves two jobs that
/// used to be three drifting copies:
///
/// 1. `#[used]` DCE anchor. These functions are referenced only by ADDRESS
///    (`builder.symbol`) from the JIT and by NAME (undefined import) from an
///    AOT `.so` — neither is a static call the linker sees, so
///    `--gc-sections` could drop them and `-rdynamic` would then have nothing
///    to export. Holding their addresses here pins them.
/// 2. The JIT module's symbol table, via [`register_shims`]. Before this
///    table the JIT builder hand-listed 27 of the 44 shims; the other 17
///    resolved only through cranelift-jit's `dlsym` fallback — which works in
///    the exported `neomacs` binary and FAILS in a unit-test binary
///    ("can't resolve symbol neovm_jit_make_float"). That is why the float
///    lowering had never been exercised by a unit test.
///
/// The NAME list an AOT `.so` may import stays in `shim_names.rs` (it is
/// `include!`d by two build scripts, which cannot see this table);
/// `shim_table_tests` pins the two sets equal.
///
/// Fn-pointer addresses live in a `Sync` newtype so they can sit in a
/// `static` (raw pointers aren't `Sync`).
#[repr(transparent)]
pub(crate) struct ShimAddr(*const ());
// SAFETY: `ShimAddr` holds a code address that is never dereferenced or mutated
// through this static — it is link-time anchoring metadata only, sound to share.
unsafe impl Sync for ShimAddr {}

#[used]
pub(crate) static JIT_SHIM_TABLE: [(&str, ShimAddr); 51] = [
    ("neovm_jit_apply", ShimAddr(neovm_jit_apply as *const ())),
    ("neovm_jit_aref", ShimAddr(neovm_jit_aref as *const ())),
    ("neovm_jit_aset", ShimAddr(neovm_jit_aset as *const ())),
    ("neovm_jit_assq", ShimAddr(neovm_jit_assq as *const ())),
    ("neovm_jit_memq", ShimAddr(neovm_jit_memq as *const ())),
    ("neovm_jit_setcar", ShimAddr(neovm_jit_setcar as *const ())),
    ("neovm_jit_setcdr", ShimAddr(neovm_jit_setcdr as *const ())),
    (
        "neovm_jit_arith_generic",
        ShimAddr(neovm_jit_arith_generic as *const ()),
    ),
    (
        "neovm_jit_arith_spec",
        ShimAddr(neovm_jit_arith_spec as *const ()),
    ),
    (
        "neovm_jit_backedge",
        ShimAddr(neovm_jit_backedge as *const ()),
    ),
    (
        "neovm_jit_builtin1",
        ShimAddr(neovm_jit_builtin1 as *const ()),
    ),
    (
        "neovm_jit_builtin2",
        ShimAddr(neovm_jit_builtin2 as *const ()),
    ),
    (
        "neovm_jit_builtin3",
        ShimAddr(neovm_jit_builtin3 as *const ()),
    ),
    (
        "neovm_jit_builtin_slice",
        ShimAddr(neovm_jit_builtin_slice as *const ()),
    ),
    ("neovm_jit_call", ShimAddr(neovm_jit_call as *const ())),
    (
        "neovm_jit_call_spec",
        ShimAddr(neovm_jit_call_spec as *const ()),
    ),
    (
        "neovm_jit_call_subr_spec",
        ShimAddr(neovm_jit_call_subr_spec as *const ()),
    ),
    (
        "neovm_jit_cbsym_read",
        ShimAddr(neovm_jit_cbsym_read as *const ()),
    ),
    (
        "neovm_jit_cbsym_spec",
        ShimAddr(neovm_jit_cbsym_spec as *const ()),
    ),
    ("neovm_jit_cons", ShimAddr(neovm_jit_cons as *const ())),
    (
        "neovm_jit_make_float",
        ShimAddr(neovm_jit_make_float as *const ()),
    ),
    (
        "neovm_jit_eq_incl_props_spec",
        ShimAddr(neovm_jit_eq_incl_props_spec as *const ()),
    ),
    (
        "neovm_jit_eq_slow",
        ShimAddr(neovm_jit_eq_slow as *const ()),
    ),
    (
        "neovm_jit_gc_push",
        ShimAddr(neovm_jit_gc_push as *const ()),
    ),
    (
        "neovm_jit_gc_push_many",
        ShimAddr(neovm_jit_gc_push_many as *const ()),
    ),
    (
        "neovm_jit_gc_restore",
        ShimAddr(neovm_jit_gc_restore as *const ()),
    ),
    (
        "neovm_jit_gc_save",
        ShimAddr(neovm_jit_gc_save as *const ()),
    ),
    (
        "neovm_jit_rootwin_grow",
        ShimAddr(neovm_jit_rootwin_grow as *const ()),
    ),
    (
        "neovm_jit_integerp_slow",
        ShimAddr(neovm_jit_integerp_slow as *const ()),
    ),
    ("neovm_jit_list", ShimAddr(neovm_jit_list as *const ())),
    (
        "neovm_jit_match_handler",
        ShimAddr(neovm_jit_match_handler as *const ()),
    ),
    (
        "neovm_jit_named_builtin",
        ShimAddr(neovm_jit_named_builtin as *const ()),
    ),
    (
        "neovm_jit_numberp_slow",
        ShimAddr(neovm_jit_numberp_slow as *const ()),
    ),
    (
        "neovm_jit_pop_handler",
        ShimAddr(neovm_jit_pop_handler as *const ()),
    ),
    (
        "neovm_jit_pred_spec",
        ShimAddr(neovm_jit_pred_spec as *const ()),
    ),
    (
        "neovm_jit_push_catch",
        ShimAddr(neovm_jit_push_catch as *const ()),
    ),
    (
        "neovm_jit_push_cc",
        ShimAddr(neovm_jit_push_cc as *const ()),
    ),
    (
        "neovm_jit_push_cc_raw",
        ShimAddr(neovm_jit_push_cc_raw as *const ()),
    ),
    (
        "neovm_jit_save_current_buffer",
        ShimAddr(neovm_jit_save_current_buffer as *const ()),
    ),
    (
        "neovm_jit_save_excursion",
        ShimAddr(neovm_jit_save_excursion as *const ()),
    ),
    (
        "neovm_jit_save_restriction",
        ShimAddr(neovm_jit_save_restriction as *const ()),
    ),
    (
        "neovm_jit_save_window_excursion",
        ShimAddr(neovm_jit_save_window_excursion as *const ()),
    ),
    ("neovm_jit_switch", ShimAddr(neovm_jit_switch as *const ())),
    (
        "neovm_jit_switch_stale",
        ShimAddr(neovm_jit_switch_stale as *const ()),
    ),
    (
        "neovm_jit_symbolp_slow",
        ShimAddr(neovm_jit_symbolp_slow as *const ()),
    ),
    ("neovm_jit_throw", ShimAddr(neovm_jit_throw as *const ())),
    ("neovm_jit_unbind", ShimAddr(neovm_jit_unbind as *const ())),
    (
        "neovm_jit_unwind_protect",
        ShimAddr(neovm_jit_unwind_protect as *const ()),
    ),
    (
        "neovm_jit_varbind",
        ShimAddr(neovm_jit_varbind as *const ()),
    ),
    ("neovm_jit_varref", ShimAddr(neovm_jit_varref as *const ())),
    ("neovm_jit_varset", ShimAddr(neovm_jit_varset as *const ())),
];

/// Register every shim in [`JIT_SHIM_TABLE`] with a JIT module builder. Both
/// tiers' builders call this, so a shim that exists is a shim the JIT can
/// resolve — in the production binary and in a test binary alike.
pub(crate) fn register_shims(builder: &mut cranelift_jit::JITBuilder) {
    for (name, addr) in &JIT_SHIM_TABLE {
        builder.symbol(*name, addr.0 as *const u8);
    }
}

#[cfg(test)]
#[path = "shims/tests/shim_table_test.rs"]
mod shim_table_tests;

/// Slow path for `eq` when the raw bits differ: only `symbols-with-pos` can
/// still make two differing values `eq`. Read-only on the Context; never
/// allocates, GCs, or signals — a plain value-returning helper.
///
/// SAFETY: same vmctx contract as [`neovm_jit_call`], but only a shared read.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_eq_slow(ctx: *mut u8, a: i64, b: i64) -> i64 {
    let a = Value::from_bits(a as usize);
    let b = Value::from_bits(b as usize);
    // SAFETY: seam-provided dormant Context; read-only access.
    let ctx = unsafe { &*(ctx as *const Context) };
    let eq = ctx.symbols_with_pos_enabled && crate::emacs_core::value::eq_value_swp(&a, &b, true);
    (if eq {
        Value::T.bits()
    } else {
        Value::NIL.bits()
    }) as i64
}

/// Slow path for `symbolp` when the value's tag is not Symbol: only a
/// symbol-with-pos (a veclike) can still count, and only while
/// `symbols-with-pos-enabled`. Read-only; never allocates, GCs, or signals.
///
/// SAFETY: same read-only vmctx contract as [`neovm_jit_eq_slow`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_symbolp_slow(ctx: *mut u8, v: i64) -> i64 {
    let v = Value::from_bits(v as usize);
    // SAFETY: seam-provided dormant Context; read-only access.
    let ctx = unsafe { &*(ctx as *const Context) };
    let is_sym = ctx.symbols_with_pos_enabled && v.is_symbol_with_pos();
    (if is_sym {
        Value::T.bits()
    } else {
        Value::NIL.bits()
    }) as i64
}

#[cfg(test)]
thread_local! {
    /// Test hook: calls that reached `neovm_jit_varref` instead of the inline
    /// plain-cell read.
    pub(crate) static VARREF_SHIM_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Read a variable from JIT code (`Op::VarRef` semantics via
/// `Vm::varref_for_jit`). Writes the value through `out` and returns
/// [`STATUS_OK`], or stashes the `Flow` (e.g. `void-variable`) and returns
/// [`STATUS_SIGNAL`]. SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_varref(ctx: *mut u8, sym: i64, out: *mut i64) -> i64 {
    #[cfg(test)]
    VARREF_SHIM_CALLS.with(|c| c.set(c.get() + 1));
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        use crate::emacs_core::intern::SymId;
        use crate::emacs_core::symbol::SymbolRedirect;
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let sym_id = SymId(sym as u32);
        // The interpreter's `fast_path_var_ref` first branch, without a Vm:
        // a plain, bound, non-nil value is the answer (28K reads per org
        // font-lock op paid a Vm construction and two frames for it). nil
        // (a possible dedicated buffer-local), unbound, forwarded and
        // buffer-local symbols keep the full path below.
        if let Some(symbol) = ctx.obarray.get_by_id(sym_id)
            && symbol.redirect() == SymbolRedirect::Plainval
        {
            let val = unsafe { symbol.val.plain };
            if !val.is_unbound() && !val.is_nil() {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = val.bits() as i64 };
                return STATUS_OK;
            }
        }
        let mut vm = Vm::from_context(ctx);
        match vm.varref_for_jit(sym_id) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        }
    })
}

/// Assign a variable from JIT code (`Op::VarSet` semantics via
/// `Vm::varset_for_jit`; may run variable watchers — arbitrary lisp). Roots the
/// value across the assignment. SAFETY: same vmctx contract as
/// [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_varset(ctx: *mut u8, sym: i64, val: i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        use crate::emacs_core::intern::SymId;
        let value = Value::from_bits(val as usize);
        {
            // SAFETY: see neovm_jit_call's function-level contract.
            let ctx = unsafe { &mut *(ctx as *mut Context) };
            // A plain cell: one store, no safe point, so no scratch roots and
            // no `Vm` (see `Context::try_set_plain_variable`).
            if ctx.try_set_plain_variable(SymId(sym as u32), value) {
                return STATUS_OK;
            }
        }
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(value);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let mut vm = Vm::from_context(ctx);
        let status = match vm.varset_for_jit(SymId(sym as u32), value) {
            Ok(()) => STATUS_OK,
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// Dynamically bind a variable (`Op::VarBind` semantics: GNU `Bvarbind`,
/// `specbind(sym, POP)`). Records the pre-bind specpdl depth for the matching
/// `unbind`, or stashes the predicate signal and returns [`STATUS_SIGNAL`].
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_varbind(ctx: *mut u8, sym: i64, val: i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        use crate::emacs_core::intern::SymId;
        let value = Value::from_bits(val as usize);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let bind_depth = ctx.specpdl.len();
        // GNU `specbind`'s plain arm first: one swap and a specpdl push, no
        // Lisp and no safe point, so `value` needs no scratch root (the cell
        // holds it from the swap on). Rooting it and going through the
        // general `try_specbind` cost more than the bind itself on a source
        // load, where nearly every `let` is of a plain global.
        if ctx.specbind_plain_untrapped_fast(SymId(sym as u32), value) {
            ctx.jit_bind_stack.push(bind_depth);
            return STATUS_OK;
        }
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(value);
        let status = match ctx.try_specbind(SymId(sym as u32), value) {
            Ok(()) => {
                ctx.jit_bind_stack.push(bind_depth);
                STATUS_OK
            }
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// Unbind the `n` most recent JIT-made dynamic bindings (`Op::Unbind`
/// semantics). The static bind-depth analysis guarantees `n` never exceeds this
/// frame's outstanding binds; the `min` is defensive only.
/// Returns `STATUS_SIGNAL` with the cleanup flow stashed when unwinding exits
/// nonlocally. SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_unbind(ctx: *mut u8, n: i64) -> i64 {
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    let target = {
        let s = &mut ctx.jit_bind_stack;
        let take = (n as usize).min(s.len());
        if take == 0 {
            None
        } else {
            let target = s[s.len() - take];
            let new_len = s.len() - take;
            s.truncate(new_len);
            Some(target)
        }
    };
    let Some(target) = target else {
        return STATUS_OK;
    };
    // GNU `unbind_to`'s common case, top-down: a `let` of a plain untrapped
    // cell is one store, the lexical environment a `let` saved is one
    // store. Pop those first; only what is left -- a watched or buffer-local
    // `let`, an `unwind-protect`, a frame with `debug-on-exit` -- takes the
    // general unwinder with its quit-flag bracket and debugger check. The
    // general path did this same pop, after ~80 instructions of its own on
    // every unbind of a source load's `let`s.
    ctx.pop_simple_specpdl_suffix(target);
    if ctx.specpdl.len() <= target {
        return STATUS_OK;
    }
    match ctx.unbind_to_with_result(target, Ok(Value::NIL)) {
        Ok(_) => STATUS_OK,
        Err(flow) => {
            stash_pending_flow(flow);
            STATUS_SIGNAL
        }
    }
}
