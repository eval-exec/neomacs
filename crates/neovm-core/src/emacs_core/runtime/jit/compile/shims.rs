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
/// via `builder.symbol(...)`, so the only effect of `no_mangle` is an exported
/// symbol name. All 41 shims the MIR tier can emit ([`MIR_SHIM_NAMES`]) carry it.
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

/// Debug-build counter of speculated direct-call shim entries (test evidence
/// that `find_spec_sites` + the spec lowering actually engage).
#[cfg(debug_assertions)]
pub(crate) static SPEC_CALL_COUNT: AtomicU64 = AtomicU64::new(0);

/// Debug-build counter of V3 fast-path engagements: a speculated call that
/// ran the cached callee leaf DIRECTLY (skipping funcall dispatch + the cache
/// hash lookup), as opposed to falling back to `call_for_jit`. Test evidence
/// that the fast path actually fires instead of silently no-op'ing.
#[cfg(debug_assertions)]
pub(crate) static SPEC_FAST_CALL_COUNT: AtomicU64 = AtomicU64::new(0);

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
        {
            // Fast path: plain builtin symbol callee — no Vm, no scratch
            // roots (interned symbol callees are obarray-rooted; the args
            // are staged on the GC-traced bc_buf), loads-only quit check.
            // Falls through to the full path on any other callee shape.
            // SAFETY: seam-provided dormant Context (fn-level contract).
            let ctx = unsafe { &mut *(ctx as *mut Context) };
            if func_val.is_symbol() && ctx.maybe_quit_hot_ok() {
                let args_start = ctx.bc_buf.len();
                for i in 0..nargs {
                    // SAFETY: generated code stored `nargs` words at args_ptr.
                    let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
                    ctx.bc_buf.push(v);
                }
                let fast = Vm::call_builtin_symbol_for_jit(ctx, func_val, args_start, nargs);
                ctx.bc_buf.truncate(args_start);
                if let Some(res) = fast {
                    return match res {
                        Ok(value) => {
                            // SAFETY: `out` is the generated code's result slot.
                            unsafe { *out = value.bits() as i64 };
                            STATUS_OK
                        }
                        Err(flow) => {
                            stash_pending_flow(flow);
                            STATUS_SIGNAL
                        }
                    };
                }
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
                for i in 0..nargs {
                    // SAFETY: the generated code stored exactly `nargs` argument
                    // words at `args_ptr` (its call-args slot) immediately before
                    // this call.
                    let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
                    ctx.bc_buf.push(v);
                }
                let mut vm = Vm::from_context(ctx);
                let res = vm.call_for_jit_stack(func_val, args_start, nargs);
                vm.bc_buf_truncate(args_start);
                match res {
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

/// `#[used]` anchor for the AOT shim table (audit #3 / R1c call-bearing). The 6
/// `#[unsafe(no_mangle)]` shims the MIR tier emits ([`MIR_SHIM_NAMES`](super::super::aot::MIR_SHIM_NAMES))
/// are referenced only by ADDRESS through `builder.symbol(...)` in the JIT and by
/// NAME (undefined import) from an AOT `.so` — neither is a static call the
/// linker sees, so `--gc-sections` could drop them and `-rdynamic` would then
/// have nothing to export. This `#[used]` array of their addresses pins all 6 so
/// they survive DCE and are present for `--export-dynamic-symbol` to promote into
/// the host's dynamic symbol table (where `dlopen` binds the `.so`'s imports).
/// Fn-pointer addresses, in a `Sync` newtype so they can live in a `static`
/// (raw pointers aren't `Sync`). Never read at runtime — the array exists only
/// to anchor the symbols against DCE.
#[repr(transparent)]
pub(crate) struct ShimAddr(*const ());
// SAFETY: `ShimAddr` holds a code address that is never dereferenced or mutated
// through this static — it is link-time anchoring metadata only, sound to share.
unsafe impl Sync for ShimAddr {}

#[used]
pub(crate) static JIT_SHIM_ANCHOR: [ShimAddr; 43] = [
    ShimAddr(neovm_jit_apply as *const ()),
    // logand/logior/logxor intrinsic — AOT-importable, so it must survive
    // `--gc-sections` for `--export-dynamic-symbol` to promote it (see below).
    ShimAddr(neovm_jit_arith_spec as *const ()),
    ShimAddr(neovm_jit_backedge as *const ()),
    ShimAddr(neovm_jit_builtin1 as *const ()),
    ShimAddr(neovm_jit_builtin2 as *const ()),
    ShimAddr(neovm_jit_builtin3 as *const ()),
    ShimAddr(neovm_jit_builtin_slice as *const ()),
    ShimAddr(neovm_jit_call as *const ()),
    ShimAddr(neovm_jit_call_spec as *const ()),
    // R2 increment B2 (Op::Call spec-in-AOT): the three round-1 subr-speculation
    // shims are now AOT-importable, so they must survive `--gc-sections` for
    // `--export-dynamic-symbol` to promote them into the host's dynamic table.
    ShimAddr(neovm_jit_call_subr_spec as *const ()),
    // R2 increment A: the two CBSym intrinsic shims are now AOT-importable, so
    // they must survive `--gc-sections` for `--export-dynamic-symbol` to promote.
    ShimAddr(neovm_jit_cbsym_read as *const ()),
    ShimAddr(neovm_jit_cbsym_spec as *const ()),
    ShimAddr(neovm_jit_cons as *const ()),
    ShimAddr(neovm_jit_eq_incl_props_spec as *const ()),
    ShimAddr(neovm_jit_eq_slow as *const ()),
    ShimAddr(neovm_jit_gc_push as *const ()),
    ShimAddr(neovm_jit_gc_push_many as *const ()),
    ShimAddr(neovm_jit_gc_restore as *const ()),
    ShimAddr(neovm_jit_gc_save as *const ()),
    ShimAddr(neovm_jit_rootwin_grow as *const ()),
    ShimAddr(neovm_jit_integerp_slow as *const ()),
    ShimAddr(neovm_jit_list as *const ()),
    ShimAddr(neovm_jit_match_handler as *const ()),
    ShimAddr(neovm_jit_named_builtin as *const ()),
    ShimAddr(neovm_jit_numberp_slow as *const ()),
    ShimAddr(neovm_jit_pop_handler as *const ()),
    ShimAddr(neovm_jit_pred_spec as *const ()),
    ShimAddr(neovm_jit_push_catch as *const ()),
    ShimAddr(neovm_jit_push_cc as *const ()),
    ShimAddr(neovm_jit_push_cc_raw as *const ()),
    ShimAddr(neovm_jit_save_current_buffer as *const ()),
    ShimAddr(neovm_jit_save_excursion as *const ()),
    ShimAddr(neovm_jit_save_restriction as *const ()),
    ShimAddr(neovm_jit_save_window_excursion as *const ()),
    ShimAddr(neovm_jit_switch as *const ()),
    ShimAddr(neovm_jit_switch_stale as *const ()),
    ShimAddr(neovm_jit_symbolp_slow as *const ()),
    ShimAddr(neovm_jit_throw as *const ()),
    ShimAddr(neovm_jit_unbind as *const ()),
    ShimAddr(neovm_jit_unwind_protect as *const ()),
    ShimAddr(neovm_jit_varbind as *const ()),
    ShimAddr(neovm_jit_varref as *const ()),
    ShimAddr(neovm_jit_varset as *const ()),
];

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

/// Read a variable from JIT code (`Op::VarRef` semantics via
/// `Vm::varref_for_jit`). Writes the value through `out` and returns
/// [`STATUS_OK`], or stashes the `Flow` (e.g. `void-variable`) and returns
/// [`STATUS_SIGNAL`]. SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_varref(ctx: *mut u8, sym: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        use crate::emacs_core::intern::SymId;
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let mut vm = Vm::from_context(ctx);
        match vm.varref_for_jit(SymId(sym as u32)) {
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

std::thread_local! {
    /// Per-thread analogue of the interpreter's per-frame `bind_stack`: the
    /// specpdl depth recorded before each JIT-made `varbind`, consumed by the
    /// `unbind` shim. [`CompiledLeaf::call`] truncates a frame's segment on
    /// every exit (the `cleanup_bytecode_frame` parity unwind).
    pub(crate) static JIT_BIND_STACK: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
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
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(value);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let bind_depth = ctx.specpdl.len();
        let status = match ctx.try_specbind(SymId(sym as u32), value) {
            Ok(()) => {
                JIT_BIND_STACK.with(|stack| stack.borrow_mut().push(bind_depth));
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
    let target = JIT_BIND_STACK.with(|s| {
        let mut s = s.borrow_mut();
        let take = (n as usize).min(s.len());
        if take == 0 {
            return None;
        }
        let target = s[s.len() - take];
        let new_len = s.len() - take;
        s.truncate(new_len);
        Some(target)
    });
    let result = match target {
        Some(target) => ctx.unbind_to_with_result(target, Ok(Value::NIL)),
        None => Ok(Value::NIL),
    };
    match result {
        Ok(_) => STATUS_OK,
        Err(flow) => {
            stash_pending_flow(flow);
            STATUS_SIGNAL
        }
    }
}
