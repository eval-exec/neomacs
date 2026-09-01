//! Bytecode → native lowering.
//!
//! Real compilation of neovm-core bytecode to machine code. Coverage is now
//! broad rather than a narrow leaf subset: **87 of the 88 bytecode opcodes
//! lower**, `MakeClosure` being the sole exception — and that op is a legacy
//! NeoVM-compiler form no runtime path emits any more: every closure created
//! at run time is an ordinary `(make-closure PROTO ...)` `Call`, which lowers.
//! The closure story that DOES matter is on the callee side: `make-closure`
//! instances share one tiering state and one leaf per source, with the patched
//! constant prefix loaded through the executing callee (`dynamic_prefix`;
//! `jit::Runtime`). `&optional`/`&rest`,
//! `catch`/`throw`, `condition-case`, `unwind-protect`, dynamic binding
//! (`varbind`/`unbind`), `save-excursion`/`save-restriction`/
//! `save-current-buffer`, static `switch` tables, `eq`/`symbolp` and
//! `Div`/`Rem` are all supported — the pre-2026-07 header claiming otherwise
//! long outlived the code and kept re-seeding a false "JIT coverage is narrow"
//! backlog item.
//!
//! Non-fixnum arithmetic does **not** refuse compilation: floats and bignums
//! **deopt at runtime** through fixnum tag guards, precisely and mid-function
//! ([`NativeRun::DeoptAt`], resumed by `Vm::run_resumed_frame`).
//!
//! Whole-function bails ([`CompileError`]) are correspondingly narrow:
//! dynamically-bound parameters ([`CompileError::TakesArguments`]), malformed
//! or unbalanced bytecode, dynamic `switch` jump tables, and Cranelift backend
//! failures.
//!
//! The remaining refusal is deliberate and is the real constraint on JIT
//! reach: [`CompileError::NotProfitable`], decided by `body_is_jit_profitable`
//! (`calls <= arith`). The baseline tier removes per-op interpreter *dispatch*,
//! but a native call costs more than a VM call — it GC-roots its live operands
//! and trampolines through a runtime shim. Call-dominated bodies therefore
//! measured net-negative, so widening opcode coverage cannot reach them;
//! lowering per-call overhead is what would.
//!
//! Two tiers live here. The optimizing Tier-2 path builds MIR (`jit/mir.rs`)
//! for pure required-only bodies and lowers it via `lower_mir_pure`; anything
//! else falls back to the baseline single-pass lowering. Baseline control flow
//! builds a CLIF basic-block CFG (`analyze_cfg` + `lower_leaf`); the operand
//! stack flows across edges through per-slot SSA variables, so Cranelift
//! inserts the phi nodes and branches carry no explicit block arguments.
//!
//! ## Speculation + deopt
//!
//! The arithmetic ops are *speculative*: native code assumes the operands are
//! fixnums and the result stays in fixnum range — exactly the interpreter's
//! fast path (`vm.rs` `Op::Add`). Each assumption is a **guard**; if a guard
//! fails at run time the function **deoptimizes**: it returns a 0 flag and the
//! caller re-runs the body on the Tier-0 interpreter, which handles the slow
//! cases (non-numbers signal, out-of-range promotes to a bignum). Because every
//! op in the supported subset is pure (no heap writes, no calls, no side
//! effects), re-running from the start after a deopt is always correct.
//!
//! ABI: `extern "C" fn(args: *const i64, out: *mut i64) -> i64`. Reads the
//! function's fixed arguments from `args` (seeding the operand stack), returns 1
//! and writes the result's raw tagged bits through `out` on success; returns 0
//! (deopt) otherwise, leaving `out` untouched.
//!
//! Allocation (`cons`) calls a C-ABI runtime shim. Because that may trigger GC,
//! live `Value`s held across it are kept alive by pushing them onto the
//! GC-traced scratch-root stack (see the `neovm_jit_*` shims); the GC is
//! non-moving, so the JIT's SSA registers stay valid afterward without a reload.
//! No vmctx is needed yet (`cons` uses the thread-local heap directly); that
//! arrives with `Call`/`Apply`.
//!
//! The bytecode operand stack is modelled at *compile time* as a `Vec` of
//! Cranelift SSA values (abstract interpretation). A `Value` is opaque to native
//! code: it flows as its `usize` bit pattern (`i64` in CLIF), exactly as the
//! interpreter stores it.

use crate::emacs_core::error::LispCondition;
use cranelift_codegen::ir::Value as ClifValue;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{
    AbiParam, Block, BlockArg, FuncRef, Function, InstBuilder, MemFlagsData, Signature, StackSlot,
    StackSlotData, StackSlotKind, Type, UserFuncName, types,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use smallvec::SmallVec;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use super::backend::BackendError;
use super::mir;
use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::vm::condition_frame_resume;
use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::dynamic_module::panic_message;
use crate::emacs_core::error::{Flow, make_signal_binding_value, signal};
use crate::emacs_core::eval::{
    ConditionFrame, Context, LispArgVec, ModuleBoundarySnapshot, ResumeTarget,
    lookup_global_subr_entry, push_scratch_gc_root, restore_scratch_gc_roots,
    save_scratch_gc_roots, subr_entry_from_value,
};
use crate::emacs_core::intern::{SymId, intern, resolve_sym};
use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::{Value, ValueKind};
use crate::tagged::header::{ConsCell, SubrDispatchKind, SubrFn};
use crate::tagged::value::{
    FIXNUM_CHECK_MASK, FIXNUM_CHECK_VALUE, FIXNUM_SHIFT, TAG_BITS, TAG_CONS, TAG_MASK, TAG_STRING,
    TAG_SYMBOL,
};

// ---------------------------------------------------------------------------
// Runtime shims — C-ABI functions the JIT calls for operations that allocate
// (and so may trigger GC). Live `Value`s held across such a call are kept alive
// by pushing them onto the GC-traced scratch-root stack; the GC is non-moving,
// so the JIT's SSA registers stay valid afterward (no reload). These are the
// foundation the eventual `Call`/`Apply` reuse.
// ---------------------------------------------------------------------------

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
    static PENDING_FLOW: std::cell::RefCell<Option<Flow>> = const { std::cell::RefCell::new(None) };
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
    static PENDING_SHIM_PANIC: std::cell::RefCell<Option<String>> =
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
    static CURRENT_LEAF_BASES: std::cell::Cell<Option<std::ptr::NonNull<JitLeafBases>>> =
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
    static PENDING_ROOT_SWEEP_FLOOR: std::cell::Cell<Option<usize>> =
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
struct JitLeafBases {
    snap: ModuleBoundarySnapshot,
}

impl JitLeafBases {
    /// Leaf-entry scratch-root depth (the pending-sweep floor).
    fn roots(&self) -> usize {
        self.snap.scratch_gc_roots_len()
    }
}

/// Whether a shim panic was contained and its residue not yet healed/taken.
fn shim_panic_pending() -> bool {
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
fn contain_jit_shim_panic(
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
fn heal_shim_panic_residue_before_match(ctx: &mut Context, ours: usize) {
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
/// `#[unsafe(no_mangle)]` shims the MIR tier emits ([`MIR_SHIM_NAMES`](super::aot::MIR_SHIM_NAMES))
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
struct ShimAddr(*const ());
// SAFETY: `ShimAddr` holds a code address that is never dereferenced or mutated
// through this static — it is link-time anchoring metadata only, sound to share.
unsafe impl Sync for ShimAddr {}

#[used]
static JIT_SHIM_ANCHOR: [ShimAddr; 43] = [
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
    static JIT_BIND_STACK: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
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

// ---------------------------------------------------------------------------
// Direct-builtin tables: the SAME typed `builtins::builtin_*` functions the
// interpreter opcode arms call, exposed to generated code through three
// arity-shaped generic shims. Single source of truth — the JIT cannot drift
// from the interpreter's semantics for these ops.
// ---------------------------------------------------------------------------

type JitBuiltin1 = fn(&mut Context, Value) -> Result<Value, Flow>;
type JitBuiltin2 = fn(&mut Context, Value, Value) -> Result<Value, Flow>;
type JitBuiltin3 = fn(&mut Context, Value, Value, Value) -> Result<Value, Flow>;

use crate::emacs_core::builtins as b;

static JIT_BUILTIN1: [JitBuiltin1; 4] = [
    b::builtin_length_1,          // 0
    b::builtin_symbol_value_1,    // 1
    b::builtin_symbol_function_1, // 2
    b::builtin_nreverse_1,        // 3
];

static JIT_BUILTIN2: [JitBuiltin2; 15] = [
    b::builtin_nth_2,          // 0
    b::builtin_nthcdr_2,       // 1
    b::builtin_elt_2,          // 2
    b::builtin_member_2,       // 3
    b::builtin_memq_2,         // 4
    b::builtin_assq_2,         // 5
    b::builtin_equal_2,        // 6
    b::builtin_setcar_2,       // 7
    b::builtin_setcdr_2,       // 8
    b::builtin_aref_2,         // 9
    b::builtin_set_2,          // 10
    b::builtin_fset_2,         // 11
    b::builtin_get_2,          // 12
    b::builtin_string_equal_2, // 13
    b::builtin_string_lessp_2, // 14
];

static JIT_BUILTIN3: [JitBuiltin3; 1] = [
    b::builtin_put_3, // 0
];

/// Slice-shaped builtins (`fn(&[Value]) -> EvalResult`, no Context) — the
/// exact functions the interpreter's `Nconc`/`Concat`/`Substring` arms call.
type JitBuiltinSlice = fn(&[Value]) -> Result<Value, Flow>;

static JIT_BUILTIN_SLICE: [JitBuiltinSlice; 3] = [
    b::builtin_nconc_slice_values, // 0
    b::builtin_concat_slice,       // 1
    b::builtin_substring_slice,    // 2
];

/// `(nargs, table_index)` for ops lowered through the slice-builtin shim.
/// `Concat`'s arity rides in the opcode; `Nconc`/`Substring` are fixed.
fn slice_builtin_spec(op: &Op) -> Option<(usize, usize)> {
    Some(match op {
        Op::Nconc => (2, 0),
        Op::Concat(n) => (*n as usize, 1),
        Op::Substring => (3, 2),
        _ => return None,
    })
}

/// `(table_arity, table_index)` for ops lowered through the generic
/// direct-builtin shims. (There is no longer a per-op "mutates" flag: every op
/// that needs runtime re-entry already sets `needs_rt`, and these ops always
/// route through the precise-deopt path, so there is nothing to poison.)
fn direct_builtin_spec(op: &Op) -> Option<(u8, usize)> {
    Some(match op {
        Op::Length => (1, 0),
        Op::SymbolValue => (1, 1),
        Op::SymbolFunction => (1, 2),
        Op::Nreverse => (1, 3),
        Op::Nth => (2, 0),
        Op::Nthcdr => (2, 1),
        Op::Elt => (2, 2),
        Op::Member => (2, 3),
        Op::Memq => (2, 4),
        Op::Assq => (2, 5),
        Op::Equal => (2, 6),
        Op::Setcar => (2, 7),
        Op::Setcdr => (2, 8),
        Op::Aref => (2, 9),
        Op::Set => (2, 10),
        Op::Fset => (2, 11),
        Op::Get => (2, 12),
        Op::StringEqual => (2, 13),
        Op::StringLessp => (2, 14),
        Op::Put => (3, 0),
        _ => return None,
    })
}

/// Call a unary direct builtin (`JIT_BUILTIN1[idx]`) — the identical function
/// the interpreter arm calls. Roots the argument across the call (builtins may
/// GC); the generated code rooted the rest of its frame.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_builtin1(ctx: *mut u8, idx: i64, a: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let a = Value::from_bits(a as usize);
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(a);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let status = match JIT_BUILTIN1[idx as usize](ctx, a) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
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

/// Binary variant of [`neovm_jit_builtin1`].
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_builtin2(ctx: *mut u8, idx: i64, a: i64, b: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let a = Value::from_bits(a as usize);
        let b = Value::from_bits(b as usize);
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(a);
        push_scratch_gc_root(b);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let status = match JIT_BUILTIN2[idx as usize](ctx, a, b) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
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

/// Ternary variant of [`neovm_jit_builtin1`].
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_builtin3(
    ctx: *mut u8,
    idx: i64,
    a: i64,
    b: i64,
    c: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let a = Value::from_bits(a as usize);
        let b = Value::from_bits(b as usize);
        let c = Value::from_bits(c as usize);
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(a);
        push_scratch_gc_root(b);
        push_scratch_gc_root(c);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let status = match JIT_BUILTIN3[idx as usize](ctx, a, b, c) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
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

/// `Op::List`: build a list from `n` operand words (the interpreter's
/// `Value::list_from_slice` on the live stack slice). The values are rooted
/// here across the per-cell allocations; the generated code rooted the rest of
/// its frame. Infallible, context-free.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_list(args_ptr: *const i64, nargs: i64) -> i64 {
    let nargs = nargs as usize;
    let saved = save_scratch_gc_roots();
    let mut args: SmallVec<[Value; 8]> = SmallVec::with_capacity(nargs);
    for i in 0..nargs {
        // SAFETY: the generated code stored exactly `nargs` words at
        // `args_ptr` (its call-args stack slot) immediately before this call.
        let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
        push_scratch_gc_root(v);
        args.push(v);
    }
    let result = Value::list_from_slice(&args).bits() as i64;
    restore_scratch_gc_roots(saved);
    result
}

/// Call a slice-shaped direct builtin (`JIT_BUILTIN_SLICE[idx]`) — the
/// identical function the interpreter arm calls (`nconc`/`concat`/
/// `substring`). Roots the operands across the call (they may allocate);
/// context-free like the interpreter's slice calls.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_builtin_slice(
    idx: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(no_ctx, STATUS_SIGNAL, {
        let nargs = nargs as usize;
        let saved = save_scratch_gc_roots();
        let mut args: SmallVec<[Value; 8]> = SmallVec::with_capacity(nargs);
        for i in 0..nargs {
            // SAFETY: see neovm_jit_list — the same spill-slot contract.
            let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
            push_scratch_gc_root(v);
            args.push(v);
        }
        let status = match JIT_BUILTIN_SLICE[idx as usize](&args) {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
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

/// Named-builtin dispatch for `Op::CallBuiltin`/`Op::CallBuiltinSym`/
/// `Op::Aset` — re-enters the runtime through the dedicated `Vm::*_for_jit`
/// helpers, which mirror the interpreter arms exactly (override-aware named
/// dispatch for CallBuiltin/Aset, advice-bypassing direct dispatch for
/// CallBuiltinSym, mutating-first-arg string writeback, trailing quit poll).
/// `variant`: 0 = CallBuiltin, 1 = CallBuiltinSym, 2 = Aset.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_named_builtin(
    ctx: *mut u8,
    variant: i64,
    sym: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let nargs = nargs as usize;
        let saved = save_scratch_gc_roots();
        let mut args = LispArgVec::new();
        for i in 0..nargs {
            // SAFETY: the generated code stored exactly `nargs` words at
            // `args_ptr` (its call-args stack slot) immediately before this call.
            let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
            push_scratch_gc_root(v);
            args.push(v);
        }
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let mut vm = Vm::from_context(ctx);
        let result = match variant {
            0 => vm.callbuiltin_for_jit(SymId(sym as u32), args),
            1 => vm.callbuiltinsym_for_jit(SymId(sym as u32), args),
            _ => vm.aset_for_jit(args[0], args[1], args[2]),
        };
        let status = match result {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
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

/// `Op::SaveWindowExcursion` (GNU bytecode.c Bsave_window_excursion): pop the
/// body form list, evaluate `(progn . body)` inside a real
/// window-configuration save/restore — the interpreter arm 1:1, including
/// error precedence (a failed restore wins over the body's flow). The body
/// runs arbitrary lisp: everything live is rooted here, the generated code
/// rooted the rest of its frame.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_save_window_excursion(ctx: *mut u8, body: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        let body = Value::from_bits(body as usize);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let root_scope = save_scratch_gc_roots();
        push_scratch_gc_root(body);
        let progn_form = Value::cons(Value::symbol("progn"), body);
        push_scratch_gc_root(progn_form);
        let status = (|| {
            let saved = match crate::emacs_core::builtins::SavedWindowConfiguration::capture(
                ctx,
                Value::NIL,
            ) {
                Ok(saved) => saved,
                Err(flow) => {
                    stash_pending_flow(flow);
                    return STATUS_SIGNAL;
                }
            };
            let body_result = ctx.with_unwind_scope(|ctx| {
                ctx.record_native_unwind(
                    crate::emacs_core::eval::NativeUnwindAction::RestoreWindowConfiguration {
                        configuration: saved,
                        options:
                            crate::emacs_core::builtins::WindowConfigurationRestoreOptions::default(
                            ),
                    },
                );
                ctx.eval_sub(progn_form)
            });
            match body_result {
                Ok(result) => {
                    // SAFETY: `out` is the generated code's result stack slot.
                    unsafe { *out = result.bits() as i64 };
                    STATUS_OK
                }
                Err(flow) => {
                    stash_pending_flow(flow);
                    STATUS_SIGNAL
                }
            }
        })();
        restore_scratch_gc_roots(root_scope);
        status
    })
}

/// Sentinel `SpecSlot::epoch` value marking a site the AOT LOADER left DISARMED
/// because its live-obarray re-classification disagreed with the kind baked into
/// the generated code (see `CompiledLeaf::from_aot`'s arming). A disarmed site
/// must NEVER re-arm: the shims' fast paths are keyed on the BAKED kind (e.g.
/// `neovm_jit_pred_spec`'s hardcoded `is_record` tag test), so re-arming against a
/// now-different binding would run the WRONG op. Both `neovm_jit_call_spec` and
/// `subr_spec_armed` short-circuit to their not-armed (generic / strict-symbol)
/// path on this value BEFORE any re-validate/re-arm. `u64::MAX` is RESERVED: the
/// live obarray `function_epoch` skips it on wrap (`advance_function_epoch`), so a
/// legitimately-armed slot never collides with it. JIT leaves never store it, so
/// the extra compare is a perfectly-predicted never-taken branch there (~0 tax).
pub(crate) const SPEC_EPOCH_DISARMED: u64 = u64::MAX;

/// Speculated direct call (`Op::Call` whose callee slot provably holds a
/// constant symbol that was fbound to a bytecode object at compile time).
/// Quit poll FIRST (the interpreter's Op::Call order — quit processing can run
/// lisp, including fset), then the validity check: if `ctx.obarray`'s
/// function_epoch still equals this site's armed epoch, NO function binding
/// anywhere has changed since the binding was observed equal to `expected`,
/// so the callee object is still reachable through the obarray and calling it
/// directly is exactly equivalent to resolving the symbol — minus the
/// resolution. On an epoch move, re-validate THIS binding: unchanged -> re-arm
/// the slot and proceed direct; changed -> strict symbol call (fset/advice
/// take effect immediately, GNU default-settings parity).
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; `slot` points into the
/// owning CompiledLeaf's spec_slots (alive whenever its code runs).
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_call_spec(
    ctx: *mut u8,
    sym: i64,
    expected: i64,
    slot: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        // Debug-build evidence that speculation actually engages (tests assert on
        // it; release builds carry no counter).
        #[cfg(debug_assertions)]
        SPEC_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
        let nargs = nargs as usize;
        // Build a rooted LispArgVec from the caller's call-args slot — used only by
        // the strict-call fallback paths (call_for_jit), inside their own
        // scratch-root scope. The native-to-native fast path passes `args_ptr`
        // straight through and touches no scratch-root state at all.
        let read_rooted_args = || {
            let mut args = LispArgVec::new();
            for i in 0..nargs {
                // SAFETY: the generated code stored exactly `nargs` argument words
                // at `args_ptr` (its call-args slot) immediately before this call.
                let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
                push_scratch_gc_root(v);
                args.push(v);
            }
            args
        };
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        // Same split as the interpreter's Op::Call poll: the loads-only fast
        // condition runs first; the full poll only when it has work.
        let quit = if ctx.maybe_quit_hot_ok() {
            Ok(())
        } else {
            ctx.maybe_quit()
        };
        let status = match quit {
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
            Ok(()) => {
                // SAFETY: slot points into the executing leaf's spec_slots.
                let slot = unsafe { &*(slot as *const SpecSlot) };
                let slot_epoch = slot.epoch.load(Ordering::Relaxed);
                // A loader-DISARMED site (epoch == SPEC_EPOCH_DISARMED) is
                // permanently not-armed: its baked callee/kind disagreed with the
                // live obarray at load, so re-validating could re-arm the WRONG
                // binding. Check it FIRST (before the epoch load / re-validate) and
                // fall to the strict-symbol-call path. JIT never sets DISARMED, so
                // this compare is perfectly-predicted never-taken there.
                let armed = if slot_epoch == SPEC_EPOCH_DISARMED {
                    false
                } else {
                    let epoch = ctx.obarray.function_epoch();
                    // NEOVM_JIT_FORCE_SLOW_SPEC pretends the armed epoch is stale so
                    // every call exercises the re-validate/re-arm branch.
                    (!jit_force_slow_spec() && slot_epoch == epoch) || {
                        let cur = ctx.obarray.symbol_function_id(SymId(sym as u32));
                        if cur.is_some_and(|v| v.bits() as i64 == expected) {
                            slot.epoch.store(epoch, Ordering::Relaxed);
                            true
                        } else {
                            // The binding changed: drop any cached callee leaf so
                            // a later re-arm can't reuse a stale callee.
                            slot.leaf.store(0, Ordering::Relaxed);
                            false
                        }
                    }
                };
                // Armed: the symbol still names the compile-time bytecode object.
                // Try the fast path (cached leaf, native-to-native pass-through when
                // the callee is a pure fixed-arity match — no arg marshaling at
                // all). Fall back to the strict call on the VALUE if it can't be
                // fast-pathed (arity / not compilable). Not armed: strict call on
                // the SYMBOL (resolves the new binding — fset/advice take effect
                // immediately).
                use crate::emacs_core::jit::cache::NativeCallOutcome;
                let outcome = if armed {
                    let target = Value::from_bits(expected as usize);
                    // No scratch rooting and no Vm construction on the armed
                    // fast path: nothing between the epoch proof and the
                    // callee's backtrace push (which roots the target for the
                    // whole native run) can reach a GC safe point, and
                    // `Vm::from_context`'s eager cache zero-fill was a
                    // measured per-call tax.
                    match Vm::call_armed_callee_native(ctx, target, &slot.leaf, args_ptr, nargs) {
                        Some(o) => o,
                        None => {
                            let saved = save_scratch_gc_roots();
                            push_scratch_gc_root(target);
                            let mut vm = Vm::from_context(ctx);
                            let res = vm.call_for_jit(target, read_rooted_args());
                            restore_scratch_gc_roots(saved);
                            NativeCallOutcome::from_result(res)
                        }
                    }
                } else {
                    let target = Value::from_sym_id(SymId(sym as u32));
                    let saved = save_scratch_gc_roots();
                    push_scratch_gc_root(target);
                    let mut vm = Vm::from_context(ctx);
                    let res = vm.call_for_jit(target, read_rooted_args());
                    restore_scratch_gc_roots(saved);
                    NativeCallOutcome::from_result(res)
                };
                match outcome {
                    NativeCallOutcome::Value(value) => {
                        // SAFETY: `out` is the generated code's result stack slot.
                        unsafe { *out = value.bits() as i64 };
                        STATUS_OK
                    }
                    NativeCallOutcome::FlowStashed => STATUS_SIGNAL,
                    // from_result never produces Fallback, and
                    // call_armed_callee_native resolves its Fallbacks itself.
                    NativeCallOutcome::Fallback => {
                        unreachable!("Fallback outcome at the spec-shim boundary")
                    }
                }
            }
        };
        status
    })
}

/// Predicate discriminator for [`neovm_jit_pred_spec`] (baked as an iconst by
/// the lowering): `recordp`.
pub(crate) const PRED_KIND_RECORDP: i64 = 0;
/// Predicate discriminator for [`neovm_jit_pred_spec`]: `symbol-with-pos-p`.
pub(crate) const PRED_KIND_SYMBOL_WITH_POS_P: i64 = 1;

/// Op discriminators for [`neovm_jit_arith_spec`] (baked as an iconst by the
/// lowering, and — offset by 5 — the [`SpecCalleeKind::to_spec_disc`] value):
/// `logand`.
pub(crate) const ARITH_KIND_LOGAND: i64 = 0;
/// Op discriminator for [`neovm_jit_arith_spec`]: `logior`.
pub(crate) const ARITH_KIND_LOGIOR: i64 = 1;
/// Op discriminator for [`neovm_jit_arith_spec`]: `logxor`.
pub(crate) const ARITH_KIND_LOGXOR: i64 = 2;
/// Op discriminator for [`neovm_jit_arith_spec`]: `ash` (2-arg arithmetic shift).
/// LEFT shifts can overflow fixnum range → the shim bounces to generic (bignum).
pub(crate) const ARITH_KIND_ASH: i64 = 3;
/// Op discriminator for [`neovm_jit_arith_spec`]: `lognot` (1-arg bitwise NOT).
pub(crate) const ARITH_KIND_LOGNOT: i64 = 4;
/// Op discriminator for [`neovm_jit_arith_spec`]: `mod` (2-arg floor-modulo).
/// The fixnum fast path is GC-free: `|a % b| < |b|` and the sign-fixup add
/// stays within the divisor's magnitude, so the result is always a fixnum; a
/// zero divisor bounces to generic (arith-error), non-fixnums (floats,
/// bignums, markers) bounce to generic.
pub(crate) const ARITH_KIND_MOD: i64 = 5;

/// Classify a callee SYMBOL name + arity as a bitwise-arithmetic intrinsic op,
/// or `None`. Shared by [`subr_spec_kind`] (which additionally proves the live
/// binding is the real subr) and the profitability gate's arith-intrinsic scan
/// (name-only). The intrinsifiable forms are the ones whose fixnum fast path is
/// GC-free (or bounces to generic when it would allocate):
///
/// * `logand`/`logior`/`logxor` (2 args) — bitwise of two fixnums is always a
///   fixnum, never overflows (these are `ManySlice` → full generic dispatch
///   today, the biggest win).
/// * `ash` (2 args) — the interpreter has NO fixnum fast path (it always
///   materializes a bignum), so a fixnum shift is a real win; LEFT-shift overflow
///   bounces to generic.
/// * `lognot` (1 arg) — `!n` of a fixnum is always a fixnum.
/// * `mod` (2 args) — floor-modulo of two fixnums is always a fixnum
///   (`|a % b| < |b|`, and the sign-fixup add stays below the divisor's
///   magnitude); a zero divisor bounces to generic (arith-error).
///
/// Other arities stay on the generic path (0 → identity const, ≥3 → reduction).
/// `lsh` is deliberately absent: it is an elisp `defun` (subr.el), not a subr,
/// so it takes the Bytecode spec path and its `ash` call intrinsifies transitively.
fn arith_intrinsic_op_by_name(name: &str, nargs: usize) -> Option<u8> {
    let op = match (name, nargs) {
        ("logand", 2) => ARITH_KIND_LOGAND,
        ("logior", 2) => ARITH_KIND_LOGIOR,
        ("logxor", 2) => ARITH_KIND_LOGXOR,
        ("ash", 2) => ARITH_KIND_ASH,
        ("lognot", 1) => ARITH_KIND_LOGNOT,
        ("mod", 2) => ARITH_KIND_MOD,
        _ => return None,
    };
    Some(op as u8)
}

/// Shared arming check for the three subr spec shims: TRUE iff the site's
/// direct fast path is still valid — the symbol's function cell (validated via
/// the per-site epoch, re-validated on any epoch move exactly like
/// [`neovm_jit_call_spec`]) still holds the compile-time subr VALUE, and no
/// compiler function overrides are active. The overrides check has no
/// bytecode-spec counterpart but is REQUIRED here for interpreter parity: the
/// generic path's `direct_subr_call_target` refuses direct subr dispatch for a
/// SYMBOL callee while overrides are active (they shadow function cells in
/// `resolve_named_call_target_by_id` and live in a VARIABLE, invisible to
/// `function_epoch`), so an armed site must bounce to the generic block then
/// too. Never allocates, never runs lisp — callers rely on this being GC-free.
#[inline]
fn subr_spec_armed(ctx: &Context, sym: i64, expected: i64, slot: &SpecSlot) -> bool {
    let slot_epoch = slot.epoch.load(Ordering::Relaxed);
    // A loader-DISARMED site never re-arms (see SPEC_EPOCH_DISARMED /
    // neovm_jit_call_spec): the pred/eq shims' fast paths are keyed on the BAKED
    // kind, so re-arming a now-different binding would run the WRONG op. Check it
    // FIRST; JIT never sets DISARMED (perfectly-predicted here).
    if slot_epoch == SPEC_EPOCH_DISARMED {
        return false;
    }
    if ctx.compiler_function_overrides_active() {
        return false;
    }
    let epoch = ctx.obarray.function_epoch();
    // NEOVM_JIT_FORCE_SLOW_SPEC pretends the armed epoch is stale so every
    // call exercises the re-validate/re-arm branch.
    (!jit_force_slow_spec() && slot_epoch == epoch) || {
        let cur = ctx.obarray.symbol_function_id(SymId(sym as u32));
        if cur.is_some_and(|v| v.bits() as i64 == expected) {
            slot.epoch.store(epoch, Ordering::Relaxed);
            true
        } else {
            // The subr shims never populate `slot.leaf`; nothing to clear.
            false
        }
    }
}

/// Speculated direct SUBR call (`Op::Call` whose callee slot provably holds a
/// constant symbol fbound at compile time to a fixed-arity builtin subr — see
/// `find_spec_sites`' subr classification). Quit poll FIRST (interpreter
/// `Op::Call` order), then the validity check ([`subr_spec_armed`]):
///
/// * ARMED — the cell still holds the compile-time `#<subr>` VALUE. Dispatch
///   it directly via `Vm::call_spec_subr_stack`, which replicates the generic
///   path's subr protocol exactly (depth guard, backtrace frame recording the
///   SYMBOL, arity signal against the FRESH entry, A0..A8 stack-args dispatch,
///   debugger hook) minus the symbol resolution. The `SubrEntry` is re-read
///   from the subr object on EVERY call: `update_static_subr_object_entry`
///   rewrites entries IN PLACE keeping the value bits identical, so the fn
///   pointer must never be cached across calls (only the stable VALUE bits are
///   baked).
/// * NOT ARMED — return [`STATUS_NEED_GENERIC`] with no side effects; the
///   generated fallback block re-does this site as a plain generic call on the
///   SYMBOL (fset/advice/overrides take effect immediately, GNU parity).
///
/// GC rooting: the args are pushed onto the GC-traced `bc_buf` (rooted across
/// the subr call, which can allocate/GC/signal) — the same rooting scheme as
/// [`neovm_jit_call`]; the generated code rooted its residual operand stack
/// before this call. The recursion-depth guard is applied inside
/// `call_spec_subr_stack` via `with_bytecode_call_depth`, exactly where
/// `call_for_jit_stack` applies it on the generic path.
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; `slot` points into the
/// owning CompiledLeaf's spec_slots (alive whenever its code runs).
///
/// AOT-importable (increment B2): the `Op::Call` subr spec sites are now emitted
/// by the AOT baseline tier too (`build_baseline_leaf_object` with `Some(obarray)`),
/// so an AOT `.so` may import this symbol. It is therefore host-exported
/// (`#[unsafe(no_mangle)] pub`) + in `shim_names.rs`/`MIR_SHIM_NAMES` (salted into
/// `ABI_TAG`) + anchored by `JIT_SHIM_ANCHOR`. Cross-session soundness rides on the
/// per-site epoch guard + the loader's DISARM sentinel (`SPEC_EPOCH_DISARMED`), not
/// on any baked address — the shim re-reads the live entry every call.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_call_subr_spec(
    ctx: *mut u8,
    sym: i64,
    expected: i64,
    slot: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        let nargs = nargs as usize;
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if let Err(flow) = ctx.maybe_quit() {
            stash_pending_flow(flow);
            return STATUS_SIGNAL;
        }
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        if !subr_spec_armed(ctx, sym, expected, slot) {
            #[cfg(debug_assertions)]
            SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
            return STATUS_NEED_GENERIC;
        }
        #[cfg(debug_assertions)]
        SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        let target = Value::from_bits(expected as usize);
        let saved = save_scratch_gc_roots();
        // Push the args straight onto bc_buf (GC-traced → rooted across the subr,
        // and the stack-args dispatcher reads them in place — no LispArgVec). The
        // callee needs no root: static subr objects are Box::leak'd, never freed.
        let args_start = ctx.bc_buf.len();
        for i in 0..nargs {
            // SAFETY: the generated code stored exactly `nargs` argument words at
            // `args_ptr` (its call-args slot) immediately before this call.
            let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
            ctx.bc_buf.push(v);
        }
        let mut vm = Vm::from_context(ctx);
        let res = vm.call_spec_subr_stack(SymId(sym as u32), target, args_start, nargs);
        vm.bc_buf_truncate(args_start);
        let status = match res {
            Ok(value) => {
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = value.bits() as i64 };
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

/// Speculated PREDICATE call: `recordp` / `symbol-with-pos-p` sites collapse
/// to a hardcoded tag test when armed. Both predicates are pure single-tag
/// checks, verified exact against their builtins (`builtin_recordp_1` =
/// `Value::is_record`, `builtin_symbol_with_pos_p_1` =
/// `Value::is_symbol_with_pos`; both independent of
/// `symbols-with-pos-enabled` — which is why `keywordp`/`symbolp` are General
/// only). NOT ARMED → [`STATUS_NEED_GENERIC`]; the generated fallback block
/// runs the plain generic call (full rooting, override/redefinition parity).
///
/// GC-FREE INVARIANT (load-bearing): the generated code does NOT root its
/// residual operand stack around this call, so no path here may allocate on
/// the lisp heap or run lisp code. That holds: `maybe_quit`'s quit/throw Flow
/// construction is pure Rust-heap (`signal(LispCondition::Quit, vec![])` — no lisp
/// allocation), `subr_spec_armed` only reads, and the armed test is a tag
/// check on an immediate/heap header. The skipped backtrace frame is
/// unobservable: the armed path cannot signal, GC, or run lisp between frame
/// push and pop.
/// SAFETY + AOT-importable status: same as [`neovm_jit_call_subr_spec`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_pred_spec(
    ctx: *mut u8,
    kind: i64,
    sym: i64,
    expected: i64,
    slot: i64,
    a: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if let Err(flow) = ctx.maybe_quit() {
            stash_pending_flow(flow);
            return STATUS_SIGNAL;
        }
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        if !subr_spec_armed(ctx, sym, expected, slot) {
            #[cfg(debug_assertions)]
            SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
            return STATUS_NEED_GENERIC;
        }
        #[cfg(debug_assertions)]
        SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        let v = Value::from_bits(a as usize);
        // `is_record`/`is_symbol_with_pos` = tag check BEFORE any header deref
        // (`veclike_type` guards on `is_veclike` first) — safe on immediates.
        let truth = if kind == PRED_KIND_RECORDP {
            v.is_record()
        } else {
            debug_assert_eq!(kind, PRED_KIND_SYMBOL_WITH_POS_P);
            v.is_symbol_with_pos()
        };
        let result = if truth { Value::T } else { Value::NIL };
        // SAFETY: `out` is the generated code's result stack slot.
        unsafe { *out = result.bits() as i64 };
        STATUS_OK
    })
}

/// Speculated `equal-including-properties` call (2 args): when armed and the
/// arguments are BITWISE equal, the answer is `t` with zero dispatch —
/// bit-equal ⟹ same object ⟹ equal-including-properties, which is literally
/// the builtin's own first check (`try_equal_value_inner`'s
/// `left.bits() == right.bits()`), so this covers same-object strings/conses,
/// identical NaN boxes, fixnums, and interned symbols alike.
///
/// On a bitwise MISS the shim returns [`STATUS_NEED_GENERIC`] instead of
/// invoking the builtin here: the deep comparison can allocate (its
/// depth-overflow arm signals with a freshly allocated lisp string), and this
/// site's direct path deliberately skips the residual-stack rooting — calling
/// the builtin from here could GC with the caller's live registers unrooted.
/// The generic fallback block does the fully-rooted call instead; the miss
/// cost is one epoch check + bit compare on top of a deep structural
/// comparison, i.e. noise. This keeps the same GC-FREE INVARIANT as
/// [`neovm_jit_pred_spec`] (no allocation / no lisp on ANY path here).
/// SAFETY + AOT-importable status: same as [`neovm_jit_call_subr_spec`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_eq_incl_props_spec(
    ctx: *mut u8,
    sym: i64,
    expected: i64,
    slot: i64,
    a: i64,
    b: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if let Err(flow) = ctx.maybe_quit() {
            stash_pending_flow(flow);
            return STATUS_SIGNAL;
        }
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        if subr_spec_armed(ctx, sym, expected, slot) && a == b {
            #[cfg(debug_assertions)]
            SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
            // SAFETY: `out` is the generated code's result stack slot.
            unsafe { *out = Value::T.bits() as i64 };
            return STATUS_OK;
        }
        #[cfg(debug_assertions)]
        SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
        STATUS_NEED_GENERIC
    })
}

/// Fixnum fast path for `(ash value count)` — mirrors GNU `Fash` /
/// `builtin_ash_slice` for the both-fixnum case, or `None` when the result would
/// leave fixnum range (so the shim bounces to the allocating generic path):
///
/// * `count == 0` → `value` unchanged.
/// * `count > 0` (left shift) → `value << count`, but ONLY when it is exactly
///   reversible (`(sh >> count) == value` rejects any bit lost to i64 overflow)
///   AND lands in fixnum range; otherwise `None` (the real result is a bignum).
///   `count >= 64` is rejected up front (an i64 shift by ≥64 is undefined).
/// * `count < 0` (arithmetic right shift, floor toward −∞ = `mpz_fdiv_q_2exp`) →
///   always a fixnum (magnitude only shrinks); huge shifts clamp to a 63-bit
///   sign fill (−1 for negative `value`, 0 otherwise), matching GNU.
fn ash_fixnum_fast(value: i64, count: i64) -> Option<i64> {
    if count == 0 {
        Some(value)
    } else if count > 0 {
        if count >= 64 {
            return None;
        }
        let sh = value.checked_shl(count as u32)?;
        if (sh >> count) == value
            && (Value::MOST_NEGATIVE_FIXNUM..=Value::MOST_POSITIVE_FIXNUM).contains(&sh)
        {
            Some(sh)
        } else {
            None
        }
    } else {
        // count < 0. `-count` is safe (count ≥ MOST_NEGATIVE_FIXNUM > i64::MIN).
        let bits = if count <= -63 { 63 } else { (-count) as u32 };
        Some(value >> bits)
    }
}

/// Speculated bitwise-arithmetic call: `logand`/`logior`/`logxor`/`ash` (2 args)
/// and `lognot` (1 arg). When armed AND the argument(s) are fixnums, computes the
/// native op with zero dispatch — EXACTLY the interpreter's fixnum semantics:
/// `&`/`|`/`^` (`builtin_logand_slice` et al., always a fixnum, never overflows),
/// `!n` (`builtin_lognot`, always a fixnum), and the fixnum `ash` fast path
/// ([`ash_fixnum_fast`], which the interpreter LACKS — it always materializes a
/// bignum). `Value::fixnum` never overflows on these results and the shim never
/// allocates.
///
/// Anything else — NOT armed (callee redefined), an arg not a fixnum (marker /
/// bignum / wrong-type), or an `ash` LEFT-shift whose result leaves fixnum range
/// — returns [`STATUS_NEED_GENERIC`], and the generated fallback runs the plain
/// generic call reaching the SAME builtin body (bignum via GMP, marker→position,
/// wrong-type signal, `ash` overflow→bignum/overflow-error). The
/// deep/allocating cases therefore only run on the fully-rooted fallback path,
/// keeping the GC-FREE INVARIANT of [`neovm_jit_eq_incl_props_spec`] (no
/// allocation / no lisp on ANY path here; `maybe_quit` is Rust-heap only).
/// `op` is baked as an iconst (`ARITH_KIND_*`); each op also has a distinct
/// [`SpecCalleeKind::to_spec_disc`], so an AOT site whose baked op no longer
/// matches the live callee is disarmed by the loader (never run with a wrong op).
/// For `lognot` (1 arg) the generated code passes a dummy `b`; the shim ignores it.
/// SAFETY + AOT-importable status: same as [`neovm_jit_call_subr_spec`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_arith_spec(
    ctx: *mut u8,
    kind: i64,
    sym: i64,
    expected: i64,
    slot: i64,
    a: i64,
    b: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        SUBR_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if let Err(flow) = ctx.maybe_quit() {
            stash_pending_flow(flow);
            return STATUS_SIGNAL;
        }
        // SAFETY: slot points into the executing leaf's spec_slots.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        if subr_spec_armed(ctx, sym, expected, slot) {
            // Single `match kind` (jump table) so the hot and/or/xor path has no
            // extra dispatch. `None` on any non-fixnum arg or an `ash` overflow ->
            // the generic bounce below. lognot is 1-arg (dummy `b`, ignored); the
            // 2-arg ops read both operands only inside their own arm.
            let fix2 = || match (
                Value::from_bits(a as usize).as_fixnum(),
                Value::from_bits(b as usize).as_fixnum(),
            ) {
                (Some(l), Some(r)) => Some((l, r)),
                _ => None,
            };
            let res: Option<i64> = match kind {
                ARITH_KIND_LOGAND => fix2().map(|(l, r)| l & r),
                ARITH_KIND_LOGIOR => fix2().map(|(l, r)| l | r),
                ARITH_KIND_LOGXOR => fix2().map(|(l, r)| l ^ r),
                ARITH_KIND_ASH => fix2().and_then(|(l, r)| ash_fixnum_fast(l, r)),
                // GNU Fmod integer branch: truncated rem, then pull the
                // result onto the divisor's side of zero.
                ARITH_KIND_MOD => fix2().and_then(|(l, r)| {
                    if r == 0 {
                        return None;
                    }
                    let m = l % r;
                    Some(if m != 0 && ((m < 0) != (r < 0)) {
                        m + r
                    } else {
                        m
                    })
                }),
                _ => {
                    debug_assert_eq!(kind, ARITH_KIND_LOGNOT);
                    Value::from_bits(a as usize).as_fixnum().map(|n| !n)
                }
            };
            if let Some(res) = res {
                #[cfg(debug_assertions)]
                SUBR_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = Value::fixnum(res).bits() as i64 };
                return STATUS_OK;
            }
        }
        #[cfg(debug_assertions)]
        SUBR_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
        STATUS_NEED_GENERIC
    })
}

/// R2 Tier-B CallBuiltinSym intrinsic (dispatch-skip): reproduce the
/// `Op::CallBuiltinSym` interpreter arm EXACTLY, skipping only the
/// `resolve_sym` → `builtin_name_id` → `dispatch_vm_builtin_unrooted`
/// special-name string-switch round trip. CallBuiltinSym name-dispatches the
/// STATIC subr table (`subr_from_sym_id(builtin_name_id(resolve_sym(sym)))`,
/// vm.rs) and is advice/fset/override IMMUNE, so there is NO epoch guard and NO
/// `compiler_function_overrides_active` gate: the shim RE-READS the live static
/// entry each call (`lookup_global_subr_entry`, safe against in-place rewrites)
/// and either
///
/// * `Some` + `dispatch_kind == Builtin` — dispatches via `funcall_general` on
///   the name-canonical SUBR value (`subr_from_sym_id(sym)`; canonicalizes a
///   name-alias, `Box::leak` process-stable). `funcall_general` IS the arm's
///   own dispatch (vm.rs → `dispatch_vm_builtin_unrooted` → `funcall_general`),
///   so this is byte-identical: a SUBR-value backtrace frame, the arity check
///   vs the FRESH entry signalling `wrong-number-of-arguments` with the SUBR
///   payload, exact-slice args (`Many` gets `into_vec`; fixed arity nil-pads
///   after the arity gate), the debugger `dispatch_signal_result_if_needed`,
///   and `unbind_to_with_result` — WITHOUT `with_bytecode_call_depth` (open-
///   coded ops add no lisp-eval-depth level). The `maybe_quit` poll runs AFTER
///   the op (GNU `Op::CallBuiltinSym` order), only on the Ok path. The R2 ship
///   set excludes `aset`/`fillarray`, so the arm's mutating-first-arg writeback
///   never applies and is correctly absent here.
/// * anything else — [`STATUS_NEED_GENERIC`]. The per-site generated fallback
///   re-runs the ORIGINAL general CBSym lowering (`neovm_jit_named_builtin`
///   variant 1 → `Vm::callbuiltinsym_for_jit`), which reproduces
///   void-function / invalid-function / the special-name arm exactly. Deferring
///   every edge case there keeps THIS shim's correctness obligation to the
///   clean `Some`+`Builtin` case only.
///
/// GC rooting: args are pushed onto the GC-traced `bc_buf` (rooted across the
/// subr, which can allocate/GC/signal) — the [`neovm_jit_call_subr_spec`]
/// scheme; the generated code rooted its residual operand stack via
/// gc_save/gc_push before this call, and `funcall_general` additionally roots
/// the args in its backtrace frame. `subr_from_sym_id`'s canonical subr object
/// is `Box::leak`'d, so the callee value needs no root.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
///
/// R2 increment A (CBSym-in-AOT): EXPORTED (`#[unsafe(no_mangle)] pub` + listed
/// in `shim_names.rs`/`MIR_SHIM_NAMES` + anchored by `JIT_SHIM_ANCHOR`). The
/// CBSym classification (`cbsym_spec_kind`) is name-canonical + obarray-free
/// (static subr table + name resolution only), so the AOT baseline emit
/// (`build_baseline_leaf_object`, `obarray=None`) now classifies CBSym sites too —
/// an AOT `.so` may import this shim and binds it against the host at `dlopen`.
/// (The round-1 `Op::Call` subr-spec shims stay JIT-only: their `find_spec_sites`
/// pass still requires `Some(obarray)` — that's increment B.)
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_cbsym_spec(
    ctx: *mut u8,
    sym: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        CBSYM_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        let nargs = nargs as usize;
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let sym_id = SymId(sym as u32);
        // Advice/fset/override-immune: NO epoch guard. Re-read the live static entry
        // (in-place-rewrite safe). Not a plain builtin now → bounce so the general
        // shim reproduces void/invalid-function exactly. FORCE_CBSYM_GENERIC forces
        // every classified site down this bounce (harness).
        let armed = !force_cbsym_generic()
            && lookup_global_subr_entry(sym_id)
                .is_some_and(|e| e.dispatch_kind == SubrDispatchKind::Builtin);
        if !armed {
            #[cfg(debug_assertions)]
            CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
            return STATUS_NEED_GENERIC;
        }
        #[cfg(debug_assertions)]
        CBSYM_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        // The name-canonical SUBR value the arm's funcall_general dispatches on.
        let subr_value = Value::subr_from_sym_id(sym_id);
        let saved = save_scratch_gc_roots();
        // Root the args on the GC-traced bc_buf (like neovm_jit_call_subr_spec),
        // then build the exact-length arg vector funcall_general copies into its
        // backtrace frame.
        let args_start = ctx.bc_buf.len();
        for i in 0..nargs {
            // SAFETY: the generated code stored exactly `nargs` argument words at
            // `args_ptr` (its call-args slot) immediately before this call.
            let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
            ctx.bc_buf.push(v);
        }
        let args: LispArgVec = ctx.bc_buf[args_start..args_start + nargs]
            .iter()
            .copied()
            .collect();
        let res = ctx.funcall_general(subr_value, args);
        ctx.bc_buf.truncate(args_start);
        let status = match res {
            Ok(value) => {
                // Quit poll AFTER the op (GNU Op::CallBuiltinSym order; the arm polls
                // maybe_quit only once the result is computed, and never on the
                // error path).
                match ctx.maybe_quit() {
                    Ok(()) => {
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
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        };
        restore_scratch_gc_roots(saved);
        status
    })
}

/// The fixed arity of a Tier-A CallBuiltinSym read (all 0-arg except
/// match-beginning/match-end, which take the group number). The shim bounces to
/// STATUS_NEED_GENERIC on an arity mismatch so the general path signals
/// wrong-number-of-arguments identically to the interpreter.
#[inline]
fn cbsym_read_expected_nargs(which: u8) -> usize {
    match which {
        CBSYM_A_MATCH_BEGINNING | CBSYM_A_MATCH_END => 1,
        _ => 0,
    }
}

/// R2 Tier-A CallBuiltinSym intrinsic (GC-free read): DELEGATE to the REAL
/// builtin body for `which`, never reimplement — a byte→char conversion
/// (match-beginning) or a three-case boundary (bolp) can then never drift. Every
/// OK path returns an IMMEDIATE (fixnum / t / nil / a pre-materialized buffer
/// value), so the shim needs NO residual-stack rooting (the generated code does
/// not root it — like the round-1 predicate shims); an Err returns STATUS_SIGNAL,
/// which DISCARDS the unrooted residual stack (no UAF even if the body's error
/// path allocates). NO epoch guard (CallBuiltinSym name-dispatches the static
/// table; advice/fset/override immune): re-read the live entry and bounce to
/// STATUS_NEED_GENERIC when it is no longer a plain builtin, when the arity is
/// unexpected, or under the FORCE_CBSYM_GENERIC harness — the general shim
/// reproduces void/invalid-function / wrong-number-of-arguments exactly.
///
/// `current-buffer` is the MISSED-HAZARD case: `builtin_current_buffer` calls
/// `make_buffer` (ALLOCATES → would corrupt the unrooted residual stack), so this
/// shim reads the ALREADY-materialized buffer value from the heap's buffer
/// registry (non-allocating) and bounces to STATUS_NEED_GENERIC when it was never
/// materialized (the general path materializes it, with a rooted residual stack).
///
/// ANTI-REQUIREMENT (upheld by the lowering): the JIT must NEVER value-number or
/// cache buffer state (current buffer / point / BEGV / ZV) across a CallBuiltinSym
/// op — `set-buffer`/`insert`/`goto-char`/`widen` change it — so every CBSym op
/// stays an opaque shim call, re-reading state each time.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
///
/// R2 increment A (CBSym-in-AOT): EXPORTED (`#[unsafe(no_mangle)] pub` + listed
/// in `shim_names.rs`/`MIR_SHIM_NAMES` + anchored by `JIT_SHIM_ANCHOR`). Tier-A
/// classification is name-canonical + obarray-free, so the AOT baseline emit
/// (`obarray=None`) now emits Tier-A sites too — an AOT `.so` may import this shim
/// and binds it against the host at `dlopen`.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_cbsym_read(
    ctx: *mut u8,
    which: i64,
    sym: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        #[cfg(debug_assertions)]
        CBSYM_SPEC_COUNT.fetch_add(1, Ordering::Relaxed);
        let which = which as u8;
        let nargs = nargs as usize;
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let sym_id = SymId(sym as u32);
        // Advice/fset/override-immune: NO epoch guard. Bounce (general path) when the
        // static entry is no longer a plain builtin, the arity is unexpected, or the
        // harness forces it.
        let armed = !force_cbsym_generic()
            && nargs == cbsym_read_expected_nargs(which)
            && lookup_global_subr_entry(sym_id)
                .is_some_and(|e| e.dispatch_kind == SubrDispatchKind::Builtin);
        if !armed {
            #[cfg(debug_assertions)]
            CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
            return STATUS_NEED_GENERIC;
        }
        // current-buffer: NEVER allocate. Read the already-materialized buffer value;
        // bounce when it was never made (general path -> make_buffer, rooted).
        if which == CBSYM_A_CURRENT_BUFFER {
            let Some(buf_id) = ctx.buffers.current_buffer().map(|b| b.id) else {
                #[cfg(debug_assertions)]
                CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
                return STATUS_NEED_GENERIC;
            };
            return match crate::tagged::gc::with_tagged_heap(|h| h.buffer_value(buf_id)) {
                Some(v) => {
                    #[cfg(debug_assertions)]
                    CBSYM_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
                    // SAFETY: `out` is the generated code's result stack slot.
                    unsafe { *out = v.bits() as i64 };
                    STATUS_OK
                }
                None => {
                    #[cfg(debug_assertions)]
                    CBSYM_SPEC_GENERIC_COUNT.fetch_add(1, Ordering::Relaxed);
                    STATUS_NEED_GENERIC
                }
            };
        }
        #[cfg(debug_assertions)]
        CBSYM_SPEC_FAST_COUNT.fetch_add(1, Ordering::Relaxed);
        // DELEGATE to the builtin body (GC-free OK path). match-beginning/end read
        // ctx.match_data through the same published-register interface, so a
        // separate register-read reimplementation could drift from Lisp.
        use crate::emacs_core::builtins::search;
        use crate::emacs_core::{editfns, navigation};
        let res = match which {
            CBSYM_A_POINT => crate::emacs_core::buffer::builtin_point_0(ctx),
            CBSYM_A_POINT_MIN => crate::emacs_core::buffer::builtin_point_min_0(ctx),
            CBSYM_A_POINT_MAX => crate::emacs_core::buffer::builtin_point_max_0(ctx),
            CBSYM_A_BOLP => navigation::builtin_bolp(ctx, Vec::new()),
            CBSYM_A_EOLP => navigation::builtin_eolp(ctx, Vec::new()),
            CBSYM_A_BOBP => navigation::builtin_bobp(ctx, Vec::new()),
            CBSYM_A_EOBP => navigation::builtin_eobp(ctx, Vec::new()),
            CBSYM_A_FOLLOWING_CHAR => editfns::builtin_following_char_0(ctx),
            CBSYM_A_PRECEDING_CHAR => editfns::builtin_preceding_char(ctx, Vec::new()),
            // Tier-A char-after is 0-arg (nargs gated above): reads at point.
            CBSYM_A_CHAR_AFTER => crate::emacs_core::buffer::builtin_char_after(ctx, Vec::new()),
            CBSYM_A_MATCH_BEGINNING => {
                // SAFETY: the generated code stored exactly nargs==1 word at args_ptr.
                let group = Value::from_bits(unsafe { *args_ptr } as usize);
                search::builtin_match_beginning_with_state(&ctx.match_data, &[group])
            }
            CBSYM_A_MATCH_END => {
                // SAFETY: the generated code stored exactly nargs==1 word at args_ptr.
                let group = Value::from_bits(unsafe { *args_ptr } as usize);
                search::builtin_match_end_with_state(&ctx.match_data, &[group])
            }
            // Unknown discriminant (unreachable for a classified site): bounce.
            _ => return STATUS_NEED_GENERIC,
        };
        // Run the SAME signal post-processing the interpreter arm gets for free via
        // funcall_general (`dispatch_signal_result_if_needed`): a no-op on Ok (stays
        // GC-free), and on a signal it runs the debugger dispatch + sets
        // `search_complete` — so the Err Flow is byte-identical to the interpreter's
        // (which routes match-beginning/end through funcall_general). The residual
        // stack is discarded on STATUS_SIGNAL, so any allocation here is UAF-safe.
        let res = ctx.dispatch_signal_result_if_needed(res);
        match res {
            Ok(value) => {
                // Quit poll AFTER (GNU CallBuiltinSym order). maybe_quit's Flow is
                // Rust-heap, so this stays GC-free.
                match ctx.maybe_quit() {
                    Ok(()) => {
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
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        }
    })
}

/// `Op::Throw`: stash `Flow::Throw{tag, value}` for the signal-exit path.
/// Compiled bodies have no local handlers (handler opcodes bail), so a throw
/// always propagates out — exactly the interpreter's `resume_nonlocal` once no
/// local handler matches. Context-free.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_throw(tag: i64, value: i64) {
    jit_shim_contain!(no_ctx, (), {
        stash_pending_flow(Flow::throw(
            Value::from_bits(tag as usize),
            Value::from_bits(value as usize),
        ));
    })
}

/// Slow path for `integerp` when the value isn't a fixnum: bignums are
/// veclikes, so delegate to the value layer's own predicate. Context-free,
/// pure, never allocates or signals.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_integerp_slow(v: i64) -> i64 {
    let v = Value::from_bits(v as usize);
    (if v.is_integer() {
        Value::T.bits()
    } else {
        Value::NIL.bits()
    }) as i64
}

/// Slow path for `numberp` when the value isn't a fixnum (floats, bignums).
/// Context-free, pure, never allocates or signals.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_numberp_slow(v: i64) -> i64 {
    let v = Value::from_bits(v as usize);
    (if v.is_number() {
        Value::T.bits()
    } else {
        Value::NIL.bits()
    }) as i64
}

/// `Op::SaveCurrentBuffer`: record the current buffer on the specpdl + the
/// bind stack, exactly like the interpreter arm (conditional + infallible).
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_save_current_buffer(ctx: *mut u8) {
    use crate::emacs_core::eval::SpecBinding;
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    if let Some(buffer_id) = ctx.buffers.current_buffer().map(|buffer| buffer.id) {
        JIT_BIND_STACK.with(|s| s.borrow_mut().push(ctx.specpdl.len()));
        ctx.specpdl
            .push(SpecBinding::SaveCurrentBuffer { buffer_id });
    }
}

/// `Op::SaveExcursion`: record point/mark/buffer via the same Context helper
/// the interpreter uses (`record_save_excursion` pushes the specpdl record and
/// returns the pre-push depth for the bind stack).
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_save_excursion(ctx: *mut u8) {
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    if let Some(count) = ctx.record_save_excursion() {
        JIT_BIND_STACK.with(|s| s.borrow_mut().push(count));
    }
}

/// `Op::SaveRestriction`: record the narrowing state, exactly like the
/// interpreter arm (conditional + infallible).
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_save_restriction(ctx: *mut u8) {
    use crate::emacs_core::eval::SpecBinding;
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    if let Some(saved) = ctx.buffers.save_current_restriction_state() {
        JIT_BIND_STACK.with(|s| s.borrow_mut().push(ctx.specpdl.len()));
        ctx.specpdl.push(SpecBinding::save_restriction(saved));
    }
}

/// `Op::UnwindProtectPop`: register an unwind-protect cleanup form as a
/// specpdl record (the interpreter arm mirrored 1:1 — same `SpecBinding`
/// entry, same captured lexenv). The cleanup runs whenever `unbind_to` crosses
/// it: the matching `Unbind`, or the frame unwind on any exit — shared
/// machinery with the interpreter, including the signal path.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_unwind_protect(ctx: *mut u8, forms: i64) {
    use crate::emacs_core::eval::SpecBinding;
    let forms = Value::from_bits(forms as usize);
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    JIT_BIND_STACK.with(|s| s.borrow_mut().push(ctx.specpdl.len()));
    let lexenv = ctx.lexenv;
    ctx.specpdl
        .push(SpecBinding::UnwindProtect { forms, lexenv });
}

/// `Op::PushConditionCase`: register a `condition-case` handler frame on the
/// ctx-level condition stack, mirroring the interpreter arm exactly — implicit
/// `error` conditions, a `VmConditionCase` resume carrying the bytecode target,
/// the static operand-stack depth at the push, the current specpdl depth, and
/// the current JIT bind-stack length (this frame's analogue of the
/// interpreter's frame-local `bind_stack`). Infallible.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_push_cc(ctx: *mut u8, target: i64, stack_len: i64) {
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    let resume_id = ctx.allocate_resume_id();
    let bind_stack_len = JIT_BIND_STACK.with(|s| s.borrow().len());
    let spec_depth = ctx.specpdl.len();
    ctx.push_condition_frame(ConditionFrame::ConditionCase {
        conditions: Value::symbol("error"),
        resume: ResumeTarget::VmConditionCase {
            resume_id,
            target: target as u32,
            stack_len: stack_len as usize,
            spec_depth,
            bind_stack_len,
        },
    });
}

/// `Op::PushConditionCaseRaw`: like [`neovm_jit_push_cc`] but the handler
/// pattern (conditions) was popped from the operand stack by the generated
/// code and is passed in. Infallible.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_push_cc_raw(
    ctx: *mut u8,
    target: i64,
    stack_len: i64,
    conditions: i64,
) {
    let conditions = Value::from_bits(conditions as usize);
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    let resume_id = ctx.allocate_resume_id();
    let bind_stack_len = JIT_BIND_STACK.with(|s| s.borrow().len());
    let spec_depth = ctx.specpdl.len();
    ctx.push_condition_frame(ConditionFrame::ConditionCase {
        conditions,
        resume: ResumeTarget::VmConditionCase {
            resume_id,
            target: target as u32,
            stack_len: stack_len as usize,
            spec_depth,
            bind_stack_len,
        },
    });
}

/// `Op::PushCatch`: register a `catch` frame (tag popped by the generated
/// code), mirroring the interpreter arm. Infallible.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_push_catch(ctx: *mut u8, target: i64, stack_len: i64, tag: i64) {
    let tag = Value::from_bits(tag as usize);
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    let resume_id = ctx.allocate_resume_id();
    let bind_stack_len = JIT_BIND_STACK.with(|s| s.borrow().len());
    let spec_depth = ctx.specpdl.len();
    ctx.push_condition_frame(ConditionFrame::Catch {
        tag,
        resume: ResumeTarget::VmCatch {
            resume_id,
            target: target as u32,
            stack_len: stack_len as usize,
            spec_depth,
            bind_stack_len,
        },
    });
}

/// `Op::PopHandler`: drop the innermost handler frame (normal exit from a
/// protected extent). The static handler-depth analysis guarantees balance.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_pop_handler(ctx: *mut u8) {
    // SAFETY: see neovm_jit_call's function-level contract.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    ctx.pop_condition_frame();
}

/// Handler-match dispatch: called on the cold path after a runtime call inside
/// a protected extent returned [`STATUS_SIGNAL`], with `ours` = the number of
/// condition frames this *native frame* has active at the site (static). The
/// per-frame invariant "callees pop their own frames on every exit" means the
/// top `ours` frames of `ctx.condition_stack` are exactly ours.
///
/// Mirrors `Vm::resume_nonlocal` 1:1:
/// - `Throw`: select via `matching_catch_resume` (whole-stack scan, like the
///   interpreter); pop our frames innermost-first looking for the selected
///   resume. Found -> unwind (`unbind_to` to the frame's spec depth, truncate
///   the JIT bind stack), write the thrown value through `out`, and return the
///   0-based miss count `m` (0 = innermost handler matched). Selected-but-outer
///   -> all ours popped, rethrow (-1). No catch anywhere -> `no-catch` signal.
/// - `Signal`: `kill-emacs` propagates untouched (frames left for the frame
///   unwind, like the interpreter's early return). Otherwise run
///   `dispatch_signal_if_needed` (signal hooks + handler-bind — may run lisp,
///   GC, or itself raise: loop on the new flow, the interpreter's recursion),
///   then unwind to `selected_resume` among our frames; on a match the error
///   object (`make_signal_binding_value`) goes through `out`.
///
/// The generated code keeps its live operand-stack values rooted across this
/// call (the lisp run by cleanups/hooks can collect) and maps the returned
/// ordinal back to the statically known handler target.
/// SAFETY: same vmctx contract as [`neovm_jit_call`]; `out` is the generated
/// code's result stack slot.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_match_handler(ctx: *mut u8, ours: i64, out: *mut i64) -> i64 {
    jit_shim_contain!(ctx, -1, {
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let ours = ours as usize;
        // A contained panic skipped the panicked extent's cleanup; heal its
        // residue against the leaf-entry bases BEFORE taking/matching, so
        // the count-based pops below and the innermost-match scan operate on
        // exactly this leaf's own frames.
        if shim_panic_pending() {
            heal_shim_panic_residue_before_match(ctx, ours);
        }
        let mut flow = take_pending_flow().expect("match shim runs only after STATUS_SIGNAL");
        let mut remaining = ours;
        let mut popped_ordinal_base = 0usize;
        'resume: loop {
            match flow {
                Flow::ThreadBlocked(_) | Flow::Shutdown(_) => {
                    stash_pending_flow(flow);
                    return -1;
                }
                Flow::Throw(thrown) => {
                    let (tag, value) = (thrown.tag, thrown.value);
                    let Some(selected) = ctx.matching_catch_resume(&tag) else {
                        // No matching catch anywhere: unwind all our frames and
                        // propagate `no-catch` (resume_nonlocal parity).
                        for _ in 0..remaining {
                            ctx.pop_condition_frame();
                        }
                        stash_pending_flow(signal(LispCondition::NoCatch, vec![tag, value]));
                        return -1;
                    };
                    for m in 0..remaining {
                        let frame = ctx
                            .pop_condition_frame()
                            .expect("JIT handler frames missing from condition stack");
                        let resume = condition_frame_resume(frame);
                        if resume == selected {
                            let ResumeTarget::VmCatch {
                                spec_depth,
                                bind_stack_len,
                                ..
                            } = resume
                            else {
                                unreachable!("JIT catch frame carries a VmCatch resume");
                            };
                            // unbind_to may run unwind-protect cleanups (lisp ->
                            // GC); keep the carried values alive across it.
                            let saved = save_scratch_gc_roots();
                            push_scratch_gc_root(tag);
                            push_scratch_gc_root(value);
                            let unwind = ctx.unbind_to_with_result(spec_depth, Ok(Value::NIL));
                            JIT_BIND_STACK.with(|s| s.borrow_mut().truncate(bind_stack_len));
                            restore_scratch_gc_roots(saved);
                            if let Err(next) = unwind {
                                let popped = m + 1;
                                remaining -= popped;
                                popped_ordinal_base += popped;
                                flow = next;
                                continue 'resume;
                            }
                            // SAFETY: `out` is the generated code's result slot.
                            unsafe { *out = value.bits() as i64 };
                            return (popped_ordinal_base + m) as i64;
                        }
                    }
                    // The selected catch belongs to an outer frame: ours are all
                    // popped; rethrow for the frame unwind + outer handlers.
                    stash_pending_flow(Flow::throw(tag, value));
                    return -1;
                }
                Flow::Signal(sig) => {
                    if sig.symbol == intern("kill-emacs") {
                        // Interpreter parity: propagate immediately, frames left
                        // to the frame-exit truncation.
                        stash_pending_flow(Flow::Signal(sig));
                        return -1;
                    }
                    // Signal hooks / handler-bind handlers may run lisp and GC;
                    // root the signal payload across the dispatch.
                    let saved = save_scratch_gc_roots();
                    push_scratch_gc_root(Value::from_sym_id(sig.symbol));
                    for v in sig.data.iter().copied() {
                        push_scratch_gc_root(v);
                    }
                    if let Some(raw) = sig.raw_data {
                        push_scratch_gc_root(raw);
                    }
                    let dispatched = ctx.dispatch_signal_if_needed(sig);
                    restore_scratch_gc_roots(saved);
                    let sig = match dispatched {
                        Ok(sig) => sig,
                        // A hook/handler raised: restart matching on the new flow
                        // (resume_nonlocal recurses here).
                        Err(next) => {
                            flow = next;
                            continue;
                        }
                    };
                    let Some(selected) = sig.selected_resume.clone() else {
                        for _ in 0..remaining {
                            ctx.pop_condition_frame();
                        }
                        stash_pending_flow(Flow::Signal(sig));
                        return -1;
                    };
                    for m in 0..remaining {
                        let frame = ctx
                            .pop_condition_frame()
                            .expect("JIT handler frames missing from condition stack");
                        let resume = condition_frame_resume(frame);
                        if resume == selected {
                            let ResumeTarget::VmConditionCase {
                                spec_depth,
                                bind_stack_len,
                                ..
                            } = resume
                            else {
                                unreachable!(
                                    "JIT condition-case frame carries a VmConditionCase resume"
                                );
                            };
                            // unbind_to runs cleanups and the error object below
                            // allocates: root the signal payload throughout.
                            let saved = save_scratch_gc_roots();
                            push_scratch_gc_root(Value::from_sym_id(sig.symbol));
                            for v in sig.data.iter().copied() {
                                push_scratch_gc_root(v);
                            }
                            if let Some(raw) = sig.raw_data {
                                push_scratch_gc_root(raw);
                            }
                            let unwind = ctx.unbind_to_with_result(spec_depth, Ok(Value::NIL));
                            JIT_BIND_STACK.with(|s| s.borrow_mut().truncate(bind_stack_len));
                            if let Err(next) = unwind {
                                restore_scratch_gc_roots(saved);
                                let popped = m + 1;
                                remaining -= popped;
                                popped_ordinal_base += popped;
                                flow = next;
                                continue 'resume;
                            }
                            let binding = make_signal_binding_value(&sig);
                            restore_scratch_gc_roots(saved);
                            // SAFETY: `out` is the generated code's result slot.
                            unsafe { *out = binding.bits() as i64 };
                            return (popped_ordinal_base + m) as i64;
                        }
                    }
                    stash_pending_flow(Flow::Signal(sig));
                    return -1;
                }
            }
        }
    })
}

/// `Op::Switch` lookup result: the dispatch value is not in the jump table —
/// fall through (interpreter parity).
const JIT_SWITCH_MISS: i64 = -1;
/// `Op::Switch` lookup result: the table no longer matches what was compiled
/// (a value mutated to a non-fixnum); the shim stashed a signal.
const JIT_SWITCH_STALE: i64 = -2;

/// `Op::Switch`: look the dispatch value up in the (statically verified
/// compile-time constant) hash-table jump table, with the interpreter's exact
/// key semantics (`to_hash_key_swp` under the table's own test). Returns the
/// raw fixnum target address on a hit ([`JIT_SWITCH_MISS`]/[`JIT_SWITCH_STALE`]
/// otherwise); the generated code maps raw addresses onto the statically
/// resolved target blocks. Pure lookup — no allocation, no lisp.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_switch(ctx: *mut u8, dispatch: i64, table: i64) -> i64 {
    jit_shim_contain!(ctx, JIT_SWITCH_STALE, {
        let table = Value::from_bits(table as usize);
        let dispatch = Value::from_bits(dispatch as usize);
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let Some(ht) = table.as_hash_table() else {
            // Statically verified a hash table; only runtime mutation of the
            // constant pool itself could change that.
            stash_pending_flow(signal(
                "error",
                vec![Value::string("jit: switch jump table mutated at runtime")],
            ));
            return JIT_SWITCH_STALE;
        };
        let key = dispatch.to_hash_key_swp(&ht.test, ctx.symbols_with_pos_enabled);
        match ht.data.get(&key).copied() {
            Some(v) => match v.kind() {
                ValueKind::Fixnum(addr) if addr >= 0 => addr,
                _ => {
                    stash_pending_flow(signal(
                        "error",
                        vec![Value::string("jit: switch jump table mutated at runtime")],
                    ));
                    JIT_SWITCH_STALE
                }
            },
            None => JIT_SWITCH_MISS,
        }
    })
}

/// Cold path for a switch hit whose raw address is not in the statically
/// compiled target set (the jump table was mutated after compilation — code
/// the byte-compiler never produces). Stash a loud signal; the generated code
/// routes to its signal path. Context-free.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_switch_stale() {
    jit_shim_contain!(no_ctx, (), {
        stash_pending_flow(signal(
            "error",
            vec![Value::string("jit: switch jump table mutated at runtime")],
        ));
    })
}

/// Back-edge service poll: GC safepoint + `maybe_quit`, via the same shared
/// Context helper the interpreter's `branch_to!` wrap path uses
/// (`bytecode_branch_maybe_gc_and_quit`). Generated code calls this every 255
/// backward jumps (the interpreter's u8 `quitcounter` cadence), with its live
/// operand-stack values rooted by the caller — the poll may collect.
/// SAFETY: same vmctx contract as [`neovm_jit_call`].
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_backedge(ctx: *mut u8) -> i64 {
    jit_shim_contain!(ctx, STATUS_SIGNAL, {
        // SAFETY: see neovm_jit_call's function-level contract.
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        match ctx.bytecode_branch_maybe_gc_and_quit() {
            Ok(()) => STATUS_OK,
            Err(flow) => {
                stash_pending_flow(flow);
                STATUS_SIGNAL
            }
        }
    })
}

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
    reloc_base: *const Value,
    /// Base of the per-thread precise-deopt spill buffer (`deopt_spill.as_ptr()`).
    spill_base: *const core::cell::Cell<i64>,
    /// Address of the `pc` deopt cell (`&deopt_meta.pc`).
    meta_pc: *const core::cell::Cell<i64>,
    /// Address of the `depth` deopt cell.
    meta_depth: *const core::cell::Cell<i64>,
    /// Address of the `handlers` deopt cell.
    meta_handlers: *const core::cell::Cell<i64>,
    /// R2 increment B2: base of the per-(thread,leaf) `SpecSlot` array the AOT
    /// `Op::Call` spec sites index (`spec_slot_base[slot_idx]`). Null when the leaf
    /// has no armed spec site. Built + armed by [`CompiledLeaf::from_aot`].
    spec_slot_base: *const SpecSlot,
    /// R2 increment B2: base of the per-(thread,leaf) `expected` (subr/bytecode
    /// VALUE bits) array, parallel to `spec_slot_base`. The AOT spec sites load
    /// `spec_expected_base[slot_idx]` instead of baking the session-specific bits.
    /// Null when the leaf has no armed spec site.
    spec_expected_base: *const u64,
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
const _: () = {
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
    lib: libloading::Library,
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
    tier: LeafTier,
    /// Number of fixed slots the native code reads from the args pointer at
    /// entry: `nonrest` parameters (required + optional, nil-padded) plus one
    /// slot for the `&rest` list when present. [`call`](Self::call) normalizes
    /// an incoming argument list to exactly this many slots, mirroring the
    /// interpreter's `run_frame` frame seeding.
    arity: usize,
    /// Number of required parameters (lower bound of an acceptable call).
    required: usize,
    /// Whether the last native slot is a `&rest` list.
    has_rest: bool,
    /// Whether the body makes dynamic bindings (`varbind`/`unbind`). When set,
    /// [`call`](Self::call) restores the entry specpdl depth on every exit —
    /// the `cleanup_bytecode_frame` parity unwind — and requires a non-null
    /// vmctx.
    has_binds: bool,
    /// Precise-deopt spill buffer: a failing guard writes the live operand
    /// stack here (raw tagged bits) before returning [`STATUS_DEOPT_AT`].
    /// Untraced by design — consumed immediately after the native call
    /// returns, with no allocation in between.
    deopt_spill: Box<[core::cell::Cell<i64>]>,
    /// Precise-deopt pc/depth/handler-count cells (see [`DeoptCells`]).
    deopt_meta: Box<DeoptCells>,
    /// Per-site direct-call speculation state ([`SpecSlot`]): armed epoch +
    /// lazily-cached callee leaf pointer. Generated code holds raw pointers
    /// into this Box (stable: boxed slice, owned here, code only runs under a
    /// live Rc of this leaf).
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    spec_slots: Box<[SpecSlot]>,
    /// R2 increment B2 (AOT only): the per-site `expected` (subr/bytecode VALUE
    /// bits) array parallel to `spec_slots`, one entry per `Op::Call` spec site in
    /// slot order. AOT code loads `spec_expected_base[slot_idx]` from the sidecar
    /// instead of baking the session-specific bits; the loader ([`from_aot`]) fills
    /// it from the LIVE cell at load. Empty for JIT leaves (they bake `expected` as
    /// an `iconst`) and for AOT leaves with no armed spec site. Address-stable (a
    /// boxed slice, the sidecar's `spec_expected_base` points into it).
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    spec_expected: Box<[u64]>,
    /// Whether the body registers handler frames (`condition-case`/`catch`).
    /// When set, [`call`](Self::call) truncates `ctx.condition_stack` back to
    /// the entry depth on every exit (before the specpdl unwind, exactly like
    /// `cleanup_bytecode_frame` — no stale frame may be matchable while unbind
    /// cleanups run lisp) and requires a non-null vmctx.
    has_handlers: bool,
    /// If this leaf INLINED a callee, the obarray `function_epoch` armed at compile
    /// time. The dispatch (try_run_compiled / resolve_compiled_leaf_ptr) recompiles
    /// the leaf when the epoch moves — so redefining any inlined callee re-JITs and
    /// no stale inline ever runs. `None` = no inlining (never epoch-checked).
    inline_epoch: Option<u64>,
    /// Whether this leaf executes a SIDE EFFECT (a call) that may precede a deopt.
    /// Such a body must NEVER rerun-from-start (STATUS_DEOPT) — only precise
    /// STATUS_DEOPT_AT resume is sound, because a rerun would re-execute the side
    /// effect. Set only by the MIR tier's calls-slice (the baseline is all-precise:
    /// every guard is STATUS_DEOPT_AT, so it never reruns after a call). Guards the
    /// null-vmctx degradation in `invoke_native` (the HOLE-3 refuse-to-rerun).
    has_side_effects: bool,
    /// SymIds of the callees this leaf INLINED — its precise dependency set. If any
    /// is redefined, this leaf must re-JIT; the dispatch evicts it eagerly via the
    /// INLINE_DEPS reverse map (cache.rs), and the coarse inline_epoch backstop
    /// catches it lazily regardless. Empty unless the leaf inlined something.
    inline_deps: Box<[crate::emacs_core::intern::SymId]>,
    /// R1a: per-leaf heap-constant relocation vector. Generated code loads each
    /// heap-object constant from `reloc_data[idx]` through a baked base pointer
    /// instead of baking the tagged heap pointer as an immediate — so the code
    /// holds NO heap pointer (GC-traceable here, AOT-portable). Fixnums + non-heap
    /// immediates (nil/t) stay baked. Traced as a GC root while the leaf is cached.
    reloc_data: Box<[Value]>,
    /// R1c-sidecar: per-(thread,leaf) base-pointer block the AOT code reads to
    /// reach `reloc_data`/`deopt_spill`/`deopt_meta` (its pointer is passed as the
    /// 4th entry arg). `Some` for AOT leaves (built by [`from_aot`]); `None` for
    /// JIT leaves (their code bakes the bases as `iconst`, ignoring the 4th arg).
    /// A `Box` so its address is stable and its raw-pointer fields stay valid
    /// after the `CompiledLeaf` moves into the cache `Rc` (see [`LeafSidecar`]).
    sidecar: Option<Box<LeafSidecar>>,
    /// Number of leading constant slots this leaf loads THROUGH THE EXECUTING
    /// CALLEE (the 4th entry param carries `callee.constants.as_ptr()`) instead
    /// of baking: the source's `make-closure` patched prefix at compile time
    /// (`RuntimeState::patched_prefix`). 0 for a plain function, whose 4th
    /// param the JIT ignores. AOT leaves are never built for a patched source.
    dynamic_prefix: u32,
    // Field order matters for drop: `entry` points into `_backing`'s memory (the
    // JITModule's executable pages or the loaded `.so`'s code); keep `_backing`
    // alive — and dropped AFTER `entry` — as long as the handle exists.
    entry: *const u8,
    _backing: LeafBacking,
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

const _: () = {
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
        spec_sites: &[super::aot::AotSpecSite],
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
    fn invoke_native(
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
        let _native_depth = super::cache::NativeDepthGuard::enter();
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
    fn cold_frame_exit(
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

/// Coarse opcode category for [`CompileError::UnsupportedOp`] diagnostics.
fn op_category(op: &Op) -> &'static str {
    match op {
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Rem | Op::Add1 | Op::Sub1 | Op::Negate => {
            "arithmetic"
        }
        Op::Call(_) | Op::Apply(_) => "call",
        Op::VarRef(_) | Op::VarSet(_) | Op::VarBind(_) | Op::Unbind(_) => "variable",
        Op::Goto(_)
        | Op::GotoIfNil(_)
        | Op::GotoIfNotNil(_)
        | Op::GotoIfNilElsePop(_)
        | Op::GotoIfNotNilElsePop(_)
        | Op::Switch => "control-flow",
        Op::StackSet(_) | Op::DiscardN(_) => "stack-mutate",
        _ => "other",
    }
}

/// Resolve the `SymId` a `VarRef`/`VarSet` operand names, at compile time —
/// mirrors the interpreter's `sym_id_at` (symbol or symbol-with-pos), except
/// that exotic constants bail to the interpreter instead of falling back to
/// `nil`.
fn const_sym_id(constants: &[Value], idx: u16) -> Result<u32, CompileError> {
    let v = constants
        .get(idx as usize)
        .ok_or(CompileError::BadOperand)?;
    v.as_symbol_id()
        .or_else(|| v.as_symbol_with_pos_sym().and_then(|s| s.as_symbol_id()))
        .map(|id| id.0)
        .ok_or(CompileError::BadOperand)
}

/// Materialize a session-specific op-operand SymId as a Cranelift `i64`.
///
/// The JIT bakes `iconst(sym)` (valid same-session). AOT must RELOC it BY NAME:
/// the normalized `Value::symbol(sym)` is collected into the per-leaf reloc
/// vector (`collect_baseline_aot_relocs`), so its tagged bits are loaded from
/// `reloc_base[idx]` and the SymId recovered via `bits >> TAG_BITS`
/// (TAG_SYMBOL == 0b000). Keyed on `reloc_index` PRESENCE: the JIT reloc set
/// never holds op-symbols, so JIT always bakes → byte-identical. Mirrors the
/// CallBuiltinSym callee site (audit CRITICAL #2: VarRef/VarSet/VarBind baked a
/// session SymId here).
fn materialize_op_sym_id(
    fb: &mut FunctionBuilder,
    reloc_base: Option<ClifValue>,
    reloc_index: &std::collections::HashMap<usize, u32>,
    sym: u32,
) -> ClifValue {
    let key = (sym as usize) << TAG_BITS | TAG_SYMBOL;
    match reloc_index.get(&key) {
        Some(&idx) => {
            let base = reloc_base.expect("reloc_base set when an op-symbol is reloc'd");
            let sym_bits =
                fb.ins()
                    .load(types::I64, MemFlagsData::trusted(), base, (idx * 8) as i32);
            fb.ins().ushr_imm_u(sym_bits, TAG_BITS as i64)
        }
        None => fb.ins().iconst(types::I64, sym as i64),
    }
}

/// Materialize an `Op::Call` spec site's `expected` (subr/bytecode VALUE bits) for
/// the shim call. JIT (`aot=false`) bakes it as an `iconst` (valid same-session);
/// AOT (`aot=true`) loads it from `spec_expected_base[slot_idx]` — the per-thread
/// array the loader (`from_aot`) fills from the LIVE cell (the bits are
/// session-specific, so they must never be baked). Keyed on `aot`, NOT presence, so
/// the JIT stays byte-identical to before B2.
fn materialize_spec_expected(
    fb: &mut FunctionBuilder,
    aot: bool,
    spec_expected_base: Option<ClifValue>,
    expected: u64,
    slot_idx: usize,
) -> ClifValue {
    if aot {
        let base =
            spec_expected_base.expect("AOT sets spec_expected_base at an Op::Call spec site");
        fb.ins().load(
            types::I64,
            MemFlagsData::trusted(),
            base,
            (slot_idx * 8) as i32,
        )
    } else {
        fb.ins().iconst(types::I64, expected as i64)
    }
}

/// Materialize an `Op::Call` spec site's `SpecSlot` pointer for the shim call. JIT
/// (`aot=false`) bakes the slot address as an `iconst`; AOT (`aot=true`) computes
/// `spec_slot_base + slot_idx * size_of::<SpecSlot>()` off the per-thread base the
/// loader put in the sidecar (the address is session-specific). Byte-identical to
/// before B2 for the JIT.
fn materialize_spec_slot(
    fb: &mut FunctionBuilder,
    aot: bool,
    spec_slot_base: Option<ClifValue>,
    slot_ptr: i64,
    slot_idx: usize,
) -> ClifValue {
    if aot {
        let base = spec_slot_base.expect("AOT sets spec_slot_base at an Op::Call spec site");
        fb.ins()
            .iadd_imm_u(base, (slot_idx * core::mem::size_of::<SpecSlot>()) as i64)
    } else {
        fb.ins().iconst(types::I64, slot_ptr)
    }
}

/// True iff this function's parameters are pushed onto the operand stack at
/// entry (so the body's `StackRef` opcodes reach them) — mirrors the
/// interpreter's `params_on_stack` in `vm.rs` `run_frame`. Dynamic-binding
/// bytecode binds params via `varref` instead and is not supported here.
fn params_on_stack(f: &ByteCodeFunction) -> bool {
    f.lexical
        || f.env.is_some()
        || matches!(
            f.arglist.kind(),
            crate::emacs_core::value::ValueKind::Fixnum(_)
        )
}

/// Compile a [`ByteCodeFunction`] whose parameters live on the operand stack
/// (lexical bytecode); otherwise bail.
///
/// `&optional` and `&rest` are supported: the native frame has one slot per
/// non-rest parameter plus one for the rest list, and [`CompiledLeaf::call`]
/// normalizes each incoming argument list to that frame (nil-padding, rest-list
/// construction) exactly as the interpreter's `run_frame` seeds it.
/// Dynamic-binding bytecode (params bound via `varbind`, not on the stack)
/// still bails.
pub fn compile_bytecode_function(f: &ByteCodeFunction) -> Result<CompiledLeaf, CompileError> {
    compile_bytecode_function_with(f, None)
}

/// [`compile_bytecode_function`] with the compiling thread's obarray for
/// direct-call speculation.
/// Profiling chokepoint (env-gated, zero cost when off): record the op-mix of
/// every distinct bytecode function the JIT attempts to compile, so a real
/// workload can be characterized — is hot elisp arithmetic-heavy (unboxing
/// helps), call-heavy (inlining helps), or dispatch/alloc-bound (an MIR tier
/// helps little)? Set `NEOVM_JIT_PROFILE=<path>` to append one CSV row per
/// function. Used to justify (or not) the optimizing Tier-2 investment.
fn jit_profile_path() -> Option<&'static str> {
    use std::sync::OnceLock;
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    PATH.get_or_init(|| std::env::var("NEOVM_JIT_PROFILE").ok())
        .as_deref()
}

/// Verification harness (J0): when `NEOVM_JIT_FORCE_DEOPT=1`, EVERY speculation
/// guard (`emit_guard`) is forced to fail, so every guarded native fast path
/// takes its deopt path instead. Running the full suite with this on (ideally
/// with `NEOVM_JIT_THRESHOLD=1` so every function compiles) exercises every deopt
/// site and must produce results identical to the interpreter — the JIT analogue
/// of `NEOVM_GC_STRESS`/`gc_stress`. Catches deopt-frame-reconstruction bugs (the
/// riskiest part of speculation) before the optimizing Tier-2 adds more guards.
fn jit_force_deopt() -> bool {
    use std::sync::OnceLock;
    static FORCE: OnceLock<bool> = OnceLock::new();
    *FORCE.get_or_init(|| std::env::var("NEOVM_JIT_FORCE_DEOPT").as_deref() == Ok("1"))
}

/// Verification harness (Gap 1): when `NEOVM_JIT_FORCE_SLOW_SPEC=1`, EVERY
/// speculated-call shim (the bytecode `neovm_jit_call_spec` + the three subr
/// spec shims) treats its per-site armed epoch as stale on every call, forcing
/// the epoch-mismatch/re-validate branch each time: the binding is re-read
/// from the obarray and compared against the baked expectation before any
/// direct dispatch. A suite run with this on (ideally with
/// `NEOVM_JIT_THRESHOLD=1`) stress-tests the slow/re-arm paths everywhere the
/// armed fast path would normally short-circuit — the spec-machinery analogue
/// of [`jit_force_deopt`].
fn jit_force_slow_spec() -> bool {
    use std::sync::OnceLock;
    static FORCE: OnceLock<bool> = OnceLock::new();
    *FORCE.get_or_init(|| std::env::var("NEOVM_JIT_FORCE_SLOW_SPEC").as_deref() == Ok("1"))
}

/// Verification harness (R2): when `NEOVM_JIT_FORCE_CBSYM_GENERIC=1`, EVERY
/// CallBuiltinSym intrinsic shim ([`neovm_jit_cbsym_spec`] +
/// `neovm_jit_cbsym_read`) bounces to [`STATUS_NEED_GENERIC`] on every call,
/// forcing the per-site generated fallback (the general CBSym lowering →
/// `Vm::callbuiltinsym_for_jit`). A differential run with this on
/// (`NEOVM_JIT_THRESHOLD=1`) proves the fallback path is byte-identical to the
/// fast path — the CBSym analogue of [`jit_force_slow_spec`] / `jit_force_deopt`.
fn force_cbsym_generic() -> bool {
    use std::sync::OnceLock;
    static FORCE: OnceLock<bool> = OnceLock::new();
    *FORCE.get_or_init(|| std::env::var("NEOVM_JIT_FORCE_CBSYM_GENERIC").as_deref() == Ok("1"))
}

fn jit_profile_emit(f: &ByteCodeFunction, obarray: Option<&Obarray>, compiled: bool) {
    let Some(path) = jit_profile_path() else {
        return;
    };
    let ops = f.executable_ops();
    let mut arith = 0u32;
    let mut calls = 0u32;
    let mut alloc = 0u32;
    let mut listops = 0u32;
    let mut varops = 0u32;
    let mut preds = 0u32;
    let mut backedges = 0u32;
    for (i, op) in ops.iter().enumerate() {
        match op {
            Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Rem
            | Op::Add1
            | Op::Sub1
            | Op::Negate
            | Op::Max
            | Op::Min
            | Op::Eqlsign
            | Op::Lss
            | Op::Gtr
            | Op::Leq
            | Op::Geq => arith += 1,
            Op::Call(_) | Op::Apply(_) | Op::CallBuiltin(..) | Op::CallBuiltinSym(..) => calls += 1,
            Op::Cons | Op::List(_) | Op::Concat(_) | Op::Nconc => alloc += 1,
            Op::Car | Op::Cdr | Op::CarSafe | Op::CdrSafe => listops += 1,
            Op::VarRef(_) | Op::VarSet(_) | Op::VarBind(_) | Op::Unbind(_) => varops += 1,
            Op::Null
            | Op::Not
            | Op::Consp
            | Op::Stringp
            | Op::Listp
            | Op::Symbolp
            | Op::Integerp
            | Op::Numberp => preds += 1,
            Op::Goto(t)
            | Op::GotoIfNil(t)
            | Op::GotoIfNotNil(t)
            | Op::GotoIfNilElsePop(t)
            | Op::GotoIfNotNilElsePop(t)
                if (*t as usize) <= i =>
            {
                backedges += 1;
            }
            _ => {}
        }
    }
    // Inlinable call sites: those whose callee is a constant symbol currently
    // fbound to a BYTECODE object (the only directly-inlinable target) — the
    // Bytecode-kind subset of what `find_spec_sites` detects (Gap 1 added
    // subr-kind sites, which are NOT inlinable — keep this metric's meaning).
    // `calls - inlinable` are subr / dynamic / non-bytecode callees inlining
    // can't directly take. This sizes inlining's TRUE surface (vs the
    // call-bearing upper bound).
    let arity =
        f.params.required.len() + f.params.optional.len() + usize::from(f.params.rest.is_some());
    let inlinable = match obarray {
        Some(ob) => analyze_cfg(ops, &f.constants, f.executable_gnu_byte_offset_map(), arity)
            .map(|cfg| {
                find_spec_sites(ops, &f.constants, &cfg.leaders, ob)
                    .values()
                    .filter(|site| site.kind == SpecCalleeKind::Bytecode)
                    .count()
            })
            .unwrap_or(0),
        None => 0,
    };
    let line = format!(
        "{},{},{},{},{},{},{},{},{},{},{}\n",
        ops.len(),
        arith,
        calls,
        alloc,
        listops,
        varops,
        preds,
        backedges,
        u8::from(backedges > 0),
        u8::from(compiled),
        inlinable,
    );
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
thread_local! {
    /// Per-thread override for the profitability gate, set by tests that need to
    /// compile a deliberately call-dominated body to exercise the call/spec
    /// machinery (which production would correctly decline to compile).
    static PROFIT_GATE_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force the profitability gate on/off on the current thread (tests only).
#[cfg(test)]
pub(crate) fn force_profit_gate_for_test(on: bool) {
    PROFIT_GATE_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

/// Is the JIT profitability gate enabled? Default yes; `NEOVM_JIT_PROFIT=off`
/// disables it, so the gate can be A/B-measured against the old behavior in a
/// single build.
fn jit_profit_gate_on() -> bool {
    #[cfg(test)]
    if let Some(o) = PROFIT_GATE_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NEOVM_JIT_PROFIT").as_deref() != Ok("off"))
}

#[cfg(test)]
std::thread_local! {
    static GATE_RELAX_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force the call-heavy gate-relaxation on/off on the current thread (tests only).
#[cfg(test)]
pub(crate) fn force_gate_relax_for_test(on: bool) {
    GATE_RELAX_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

/// Is the call-heavy gate relaxation enabled? **Default NO** — reverted to the
/// conservative `calls <= arith` on 2026-07-21 after it regressed byte-compilation
/// (see below). `NEOVM_JIT_GATE_RELAX=on` opts in (stops counting user-function
/// `Op::Call`/`Op::Apply` against profitability, so user-call-heavy bodies tier).
///
/// HISTORY: briefly default-ON (commit 20cb6190a). Motivating measurements looked
/// good on SYNTHETICS — a hot user-fn call loop 2.31x, trivial-builtin 1.21x, real
/// font-lock 1.013x (neutral) — and the oracle suite showed zero new failures. But
/// those synthetics were unrepresentative: they run the SAME hot body enough to
/// amortize the JIT compile cost. The reverted-to conservative gate exists
/// precisely to protect BYTE-COMPILATION (call-heavy, builtin-heavy, ~one-shot),
/// and a proper `perf stat instructions:u` A/B (byte-compile cl-macs.el x8,
/// release) measured the flip **21% SLOWER** (22.56B on vs 18.58B off, ratio 1.214
/// x3 runs): with the gate on, ~9% goes to runtime regalloc2+cranelift compilation
/// that never amortizes because native ≈ interp for builtin-heavy code (the
/// font-lock 1.013x), so the compile tax is pure loss. LESSON: measure the
/// workload the gate was DESIGNED for (byte-compile), not a favorable synthetic.
/// The right long-term path is AOT (compile the standard library at build time so
/// it never runtime-compiles) + a gate that distinguishes amortizing from
/// one-shot hot bodies — not this blanket relaxation. Knob kept for hot long-
/// running user-call-heavy loops that genuinely amortize.
fn jit_gate_relax_on() -> bool {
    #[cfg(test)]
    if let Some(o) = GATE_RELAX_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NEOVM_JIT_GATE_RELAX").as_deref() == Ok("on"))
}

#[cfg(test)]
std::thread_local! {
    static INLINE_ARITH_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force Level-B inline-arith on/off on the current thread (tests only).
#[cfg(test)]
pub(crate) fn force_inline_arith_for_test(on: bool) {
    INLINE_ARITH_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

/// LEVEL-B: inline `logand`/`logior`/`logxor`/`lognot` (JIT only) as native
/// `band`/`bor`/`bxor`/`ineg` on the TAGGED fixnum bits (the tag `2` survives
/// `&`/`|`, is restored after `^`, and `ineg` maps a tagged fixnum to its
/// `lognot`), guarded by a fixnum check that deopts, instead of the armed
/// `neovm_jit_arith_spec` shim (which marshals 8 args). `mod` inlines its
/// floor-modulo on the untagged values (srem + branchless sign-fixup,
/// zero-divisor deopt). Redefinition is caught by the leaf's `inline_epoch`
/// eviction. `ash` never inlines (overflow/bignum).
/// Default ON since the shim-vs-inline A/B (list workload −11.5% wall, no
/// change elsewhere); `NEOVM_JIT_INLINE_ARITH=off` is the kill switch. AOT
/// always keeps the shim (its loader owns arm/disarm — an inline op has no
/// per-site epoch).
fn jit_inline_arith_on() -> bool {
    #[cfg(test)]
    if let Some(o) = INLINE_ARITH_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_INLINE_ARITH").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

/// Which `ArithIntrinsic` ops Level-B lowers inline (everything but `ash`, whose
/// overflow/bignum path must stay on the shim).
fn arith_op_inlines(op: u8) -> bool {
    op != ARITH_KIND_ASH as u8
}

/// Every `Op::Call` site whose callee provably holds an intrinsifiable bit-op
/// SYMBOL constant (`logand`/`logior`/`logxor`/`ash`/`lognot` at its intrinsic
/// arity) — the sites `subr_spec_kind` classifies as
/// [`SpecCalleeKind::ArithIntrinsic`] — as `(op_index, callee_sym, arith_op)`.
/// The profitability gate uses the indices to NOT count an intrinsifiable bit-op
/// as a call (it lowers to a native op ≈ arith, not a call+dispatch, so a
/// bit-op-heavy loop is not vetoed as "call-dominated"); Level-B uses the callee
/// syms of the inlinable ops for the leaf's redefinition-eviction dep set.
///
/// A name-only, obarray-free mirror of [`find_spec_sites`]' abstract-tag scan
/// (the callee sits `nargs` below the top; only the stack-shuffling ops carry a
/// tag). It omits `find_spec_sites`' block-leader clearing (which needs the
/// CFG): the byte-compiler always emits a callee push and its `Op::Call` inside
/// one basic block with no jump target between, so the two agree on real
/// bytecode; on hand-crafted bytecode the gate may merely over-/under-count (a
/// heuristic mis-estimate, never a miscompile — `find_spec_sites` independently
/// decides, off the LIVE binding, what actually intrinsifies). Ascending order.
fn arith_intrinsic_call_sites(
    ops: &[Op],
    constants: &[Value],
) -> Vec<(usize, crate::emacs_core::intern::SymId, u8)> {
    let mut out = Vec::new();
    let mut tags: Vec<Option<u16>> = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        match op {
            Op::Constant(cidx) => {
                let tag = constants
                    .get(*cidx as usize)
                    .and_then(|v| v.as_symbol_id())
                    .map(|_| *cidx);
                tags.push(tag);
            }
            Op::Nil | Op::True => tags.push(None),
            Op::Dup => {
                let t = tags.last().copied().flatten();
                tags.push(t);
            }
            Op::StackRef(n) => {
                let n = *n as usize;
                let t = if tags.len() > n {
                    tags[tags.len() - 1 - n]
                } else {
                    None
                };
                tags.push(t);
            }
            Op::StackSet(n) => {
                let n = *n as usize;
                let t = tags.pop().flatten();
                if n > 0 && tags.len() >= n {
                    let d = tags.len() - n;
                    tags[d] = t;
                }
            }
            Op::DiscardN(raw) => {
                let preserve_tos = (raw & 0x80) != 0;
                let n = (raw & 0x7F) as usize;
                if preserve_tos && n > 0 {
                    let t = tags.pop().flatten();
                    for _ in 0..n.min(tags.len()) {
                        tags.pop();
                    }
                    tags.push(t);
                } else {
                    for _ in 0..n.min(tags.len()) {
                        tags.pop();
                    }
                }
            }
            Op::Call(n) => {
                let nargs = *n as usize;
                if tags.len() > nargs
                    && let Some(cidx) = tags[tags.len() - 1 - nargs]
                    && let Some(sym_id) =
                        constants.get(cidx as usize).and_then(|v| v.as_symbol_id())
                    && let Some(arith_op) = arith_intrinsic_op_by_name(resolve_sym(sym_id), nargs)
                {
                    out.push((i, sym_id, arith_op));
                }
                for _ in 0..(nargs + 1).min(tags.len()) {
                    tags.pop();
                }
                tags.push(None);
            }
            Op::Apply(n) => {
                let nargs = *n as usize;
                for _ in 0..(nargs + 1).min(tags.len()) {
                    tags.pop();
                }
                tags.push(None);
            }
            other => match simple_effect(other) {
                Ok((needs, delta)) => {
                    let consumed = needs;
                    let produced = (needs as i64 + delta).max(0) as usize;
                    for _ in 0..consumed.min(tags.len()) {
                        tags.pop();
                    }
                    for _ in 0..produced {
                        tags.push(None);
                    }
                }
                Err(_) => tags.clear(),
            },
        }
    }
    out
}

/// The distinct callee symbols of the body's INLINABLE (`arith_op_inlines`) bit-op
/// sites — the redefinition-eviction dep set for a Level-B leaf (an inlined op
/// bakes the native instruction with no per-call arming, so `fset`ing the callee
/// must evict the leaf). `ash` is excluded (it stays on the self-arming shim).
fn inline_arith_callee_syms(
    ops: &[Op],
    constants: &[Value],
) -> Vec<crate::emacs_core::intern::SymId> {
    let mut syms: Vec<crate::emacs_core::intern::SymId> =
        arith_intrinsic_call_sites(ops, constants)
            .into_iter()
            .filter(|&(_, _, op)| arith_op_inlines(op))
            .map(|(_, sym, _)| sym)
            .collect();
    syms.sort_by_key(|s| s.0);
    syms.dedup();
    syms
}

/// Decide whether a bytecode body is worth compiling.
///
/// The baseline tier only removes per-op interpreter *dispatch*. A function call
/// costs MORE in native code than in the VM — each call GC-roots its live
/// operands and trampolines through a runtime shim (`neovm_jit_gc_push` +
/// `neovm_jit_call`). So a call-dominated body pays that overhead with nothing
/// to offset it: measured ~32% SLOWER on real workloads (byte-compilation,
/// font-lock), where ~36 of 48 tiered bodies had zero arithmetic and ~10 calls
/// each — pure call/control code the native frame can only shuffle, not speed
/// up. Compile only when arithmetic is not outnumbered by calls; the genuine win
/// shape (hot arithmetic/control loops — the 7x microbenchmark) clears this, and
/// call-free bodies always pass (`0 <= 0`).
fn body_is_jit_profitable(ops: &[Op], constants: &[Value]) -> bool {
    if !jit_profit_gate_on() {
        return true;
    }
    let relax = jit_gate_relax_on();
    // The intrinsifiable bit-op `Op::Call` sites (logand/logior/logxor/ash/lognot),
    // which lower to native ops — counted as arith below, not calls. Ascending
    // op-index order (linear scan), so `binary_search` is valid.
    let intrinsic: Vec<usize> = arith_intrinsic_call_sites(ops, constants)
        .into_iter()
        .map(|(idx, _, _)| idx)
        .collect();
    let mut arith = 0u32;
    let mut calls = 0u32;
    for (i, op) in ops.iter().enumerate() {
        match op {
            Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Rem
            | Op::Add1
            | Op::Sub1
            | Op::Negate
            | Op::Max
            | Op::Min
            | Op::Eqlsign
            | Op::Lss
            | Op::Gtr
            | Op::Leq
            | Op::Geq => arith += 1,
            // Gate-relaxation: USER-function calls (`Op::Call`/`Op::Apply`) are
            // net-POSITIVE tiered (measured 2.31x native vs interp — V3
            // native-to-native + lever-1), so they must not veto compilation. A
            // body dominated by them (font-lock's re-search-forward calls aside —
            // those are subrs, but still Op::Call) tiers to a net win/neutral.
            // Builtin calls stay counted (real builtin-heavy = neutral, not a win).
            Op::Call(_) | Op::Apply(_) if relax => {}
            // A `logand`/`logior`/`logxor` bit-op `Op::Call` that WILL intrinsify
            // (`ArithIntrinsic` → `neovm_jit_arith_spec`) lowers to a GC-free
            // native op, not a call+dispatch — so, exactly like the CBSym
            // intrinsics below, it must NOT veto a bit-op-heavy loop (a pure
            // `(logand (logxor ...))` loop is `calls > arith` → NotProfitable →
            // the intrinsic could never engage). A generic call still vetoes.
            Op::Call(_) if intrinsic.binary_search(&i).is_ok() => {}
            Op::Call(_) | Op::Apply(_) | Op::CallBuiltin(..) => calls += 1,
            // R2: a CallBuiltinSym that WILL be intrinsified (Tier-A GC-free read
            // or Tier-B dispatch-skip) costs ~an arith op, not a full
            // call+dispatch — so it must NOT veto a buffer-op-heavy loop (point/
            // insert/goto-char/... loops were `calls > arith` -> NotProfitable ->
            // the intrinsic could never engage). Drop those from the call count
            // via the classifier's OWN predicate, so the gate agrees exactly with
            // what tiers. Non-intrinsifiable CBSym ops still count as calls, so a
            // genuinely call-dominated non-spec body stays protected.
            Op::CallBuiltinSym(sym, n) if cbsym_spec_kind(*sym, *n as usize).is_none() => {
                calls += 1;
            }
            _ => {}
        }
    }
    calls <= arith
}

pub fn compile_bytecode_function_with(
    f: &ByteCodeFunction,
    obarray: Option<&Obarray>,
) -> Result<CompiledLeaf, CompileError> {
    let result = compile_bytecode_function_inner(f, obarray);
    if jit_profile_path().is_some() {
        jit_profile_emit(f, obarray, result.is_ok());
    }
    result
}

/// Max instruction budget (excluding `Arg`s) for an inlined callee body. Small
/// pure helpers (`sq`, `1+`-wrappers, accessors) are the target; larger callees
/// stay calls and the baseline handles them.
const MAX_INLINE_INSTS: usize = 8;

/// Resolve a constant call-target symbol to its callee MIR for inlining, or `None`
/// unless it is a required-only lexical bytecode function (so `build_mir`'s
/// argument seeding matches the arity).
fn resolve_inline_callee(ob: &Obarray, sym: Value) -> Option<mir::MirFunction> {
    let sym_id = sym.as_symbol_id()?;
    let binding = ob.symbol_function_id(sym_id)?;
    let bc = binding.get_bytecode_data()?;
    // Required-only lexical, and no captured lexenv: inlining drops the lexenv
    // install, which is only otherwise safe because lexenv-reading ops lower to
    // Opaque and `callee_inlinable` rejects them — keep the safety local here too.
    if !bc.lexical || bc.env.is_some() || !bc.params.optional.is_empty() || bc.params.rest.is_some()
    {
        return None;
    }
    // A patched source's leading constants are per-instance; inlining would
    // bake this instance's captured values into the caller.
    if bc.jit_runtime().patched_prefix() > 0 {
        return None;
    }
    mir::build_mir(bc.executable_ops(), &bc.constants, bc.params.required.len()).ok()
}

fn compile_bytecode_function_inner(
    f: &ByteCodeFunction,
    obarray: Option<&Obarray>,
) -> Result<CompiledLeaf, CompileError> {
    let ops = f.executable_ops();
    let required = f.params.required.len();
    let nonrest = required + f.params.optional.len();
    let has_rest = f.params.rest.is_some();
    let native_arity = nonrest + usize::from(has_rest);
    if native_arity > 0 && !params_on_stack(f) {
        // Params are dynamically bound, not on the stack — `StackRef` would not
        // find them.
        return Err(CompileError::TakesArguments);
    }
    // Typed-MIR Tier-2: for pure required-only functions, build the SSA MIR and
    // lower it with fixnum UNBOXING (raw arithmetic, retag only at boundaries) —
    // faster than the baseline's per-op untag/retag. Fall back to the baseline on
    // any bail (calls, cons, optional/&rest args, ...). Restricted to no
    // optional/&rest so the MIR's argument seeding matches `native_arity`.
    // A `make-closure`-patched source: leading constant slots are per-instance.
    // The MIR tier bakes every constant, so it is skipped; the baseline gets
    // the masked view for its analyses and loads the prefix through the callee.
    let dynamic_prefix = f.jit_runtime().patched_prefix();
    let masked;
    let constants: &[Value] = if dynamic_prefix > 0 {
        masked = mask_dynamic_prefix(&f.constants, dynamic_prefix);
        &masked
    } else {
        &f.constants
    };
    if !has_rest
        && f.params.optional.is_empty()
        && dynamic_prefix == 0
        && let Ok(mut mir) = mir::build_mir(ops, constants, native_arity)
    {
        // Inline pure single-block callees (resolved through the obarray). When
        // a call is inlined the body can become pure (no Opaque), so
        // lower_mir_pure handles it and unboxing/guard-elision flow ACROSS the
        // former call boundary. Record the armed function_epoch so the dispatch
        // re-JITs if any inlined callee is later redefined (see CompiledLeaf
        // ::inline_epoch).
        let mut inlined_syms: Vec<crate::emacs_core::intern::SymId> = Vec::new();
        let inline_epoch = obarray.and_then(|ob| {
            let armed = ob.function_epoch();
            let n = mir::inline_pure_single_block_callees(
                &mut mir,
                &|sym| resolve_inline_callee(ob, sym),
                MAX_INLINE_INSTS,
                &mut inlined_syms,
            );
            (n > 0).then_some(armed)
        });
        if let Ok(mut leaf) = lower_mir_pure(&mir) {
            // Tier gate: a call-bearing MIR leaf (has_side_effects) only earns
            // the MIR tier when it INLINED something — that's the one case the
            // MIR tier beats the baseline (cross-boundary unboxing/elision).
            // For a plain non-inlined call the baseline is strictly better
            // (spec-call native-to-native speculation + battle-tested), so let
            // it fall through. Pure (call-free) leaves always take the MIR tier.
            if !leaf.has_side_effects || inline_epoch.is_some() {
                leaf.required = required;
                leaf.has_rest = has_rest;
                leaf.inline_epoch = inline_epoch;
                // The precise dependency set (registered into INLINE_DEPS at the
                // cache compile-miss site so a redefinition of any inlined callee
                // evicts exactly this leaf).
                leaf.inline_deps = inlined_syms.into();
                return Ok(leaf);
            }
        }
    }
    // The MIR tier above already claimed any body its inlining/unboxing makes
    // worthwhile. What's left goes to the baseline, whose per-op call shims aren't
    // worth it for a call-dominated body — keep those on the interpreter.
    if !body_is_jit_profitable(ops, constants) {
        return Err(CompileError::NotProfitable);
    }
    let mut leaf = lower_leaf_full(
        ops,
        constants,
        native_arity,
        f.executable_gnu_byte_offset_map(),
        obarray,
        dynamic_prefix,
    )?;
    leaf.required = required;
    leaf.has_rest = has_rest;
    // LEVEL-B redefinition guard: an inlined bit-op (logand/logior/logxor/lognot)
    // bakes the native op with NO per-call arming, so the leaf must be evicted if
    // its callee is ever redefined. Use PRECISE inline_deps only (NOT the coarse
    // inline_epoch backstop): `set_symbol_function_id` → `note_function_redefined`
    // → `evict_inline_dependents(sym)` fires on every fset/fmakunbound, so a leaf
    // registered under `logand` is evicted exactly when `logand` changes. The
    // coarse `inline_epoch` backstop is DELIBERATELY omitted — it evicts on ANY
    // function redefinition, which thrash-recompiles bit-op leaves during
    // byte-compile/loadup (measured +13.7%); a bare epoch bump without a cell write
    // means the bit-op is unchanged, so precise eviction loses no correctness.
    if jit_inline_arith_on() && obarray.is_some() {
        let inline_syms = inline_arith_callee_syms(ops, constants);
        if !inline_syms.is_empty() {
            leaf.inline_deps = inline_syms.into();
        }
    }
    Ok(leaf)
}

/// Emit a speculation guard.
///
/// If `cond` (an `i8` boolean from `icmp`) is false, branch to the shared deopt
/// block — created lazily on first use; otherwise fall through into a fresh,
/// sealed continuation block. On return, the builder is positioned in the
/// continuation so lowering continues on the success path.
fn emit_guard(fb: &mut FunctionBuilder, deopt: Block, cond: ClifValue) {
    // J0 verification harness: force every guard to fail so the deopt path is
    // always taken (see `jit_force_deopt`). A constant-false condition makes
    // `brif` unconditionally branch to `deopt`.
    let cond = if jit_force_deopt() {
        let ty = fb.func.dfg.value_type(cond);
        fb.ins().iconst(ty, 0)
    } else {
        cond
    };
    let cont = fb.create_block();
    fb.ins().brif(cond, cont, &[], deopt, &[]);
    fb.switch_to_block(cont);
    // `cont`'s only predecessor is the guard branch just emitted.
    fb.seal_block(cont);
}

/// Return the bits of `v` when it is a Cranelift integer constant.
///
/// Since Cranelift 0.134, immediate convenience builders materialize an
/// `iconst` operand instead of preserving a distinct immediate instruction
/// shape. Keep that representation knowledge at this single inspection seam.
fn iconst_bits(fb: &FunctionBuilder, v: ClifValue) -> Option<i64> {
    use cranelift_codegen::ir::{InstructionData, Opcode, ValueDef};
    let ValueDef::Result(inst, _) = fb.func.dfg.value_def(v) else {
        return None;
    };
    match fb.func.dfg.insts[inst] {
        InstructionData::UnaryImm {
            opcode: Opcode::Iconst,
            imm,
        } => Some(imm.bits()),
        _ => None,
    }
}

/// Return `(value, immediate)` for a binary instruction whose right operand is
/// an `iconst`. This is the 0.134 IR shape produced by helpers such as
/// `bor_imm_u` and `ishl_imm_u`.
fn binary_value_and_iconst(
    fb: &FunctionBuilder,
    v: ClifValue,
    expected_opcode: cranelift_codegen::ir::Opcode,
) -> Option<(ClifValue, i64)> {
    use cranelift_codegen::ir::{InstructionData, ValueDef};
    let ValueDef::Result(inst, _) = fb.func.dfg.value_def(v) else {
        return None;
    };
    let InstructionData::Binary { opcode, args } = fb.func.dfg.insts[inst] else {
        return None;
    };
    if opcode != expected_opcode {
        return None;
    }
    Some((args[0], iconst_bits(fb, args[1])?))
}

/// True if `v` is a compile-time fixnum constant — an `iconst` whose immediate
/// already carries the fixnum tag bits. A runtime fixnum guard on such a value
/// is provably unnecessary (it is the same fixnum on every path), so
/// [`guard_fixnum`] can skip it. This is the safe, dataflow-free subset of
/// redundant-guard elimination: constant operands of arithmetic/comparison are
/// pervasive (`(+ i 1)`, `(< i n)`, `(1+ i)`), and a fixnum `iconst` dominates
/// every use, so eliding its guard cannot change any result or deopt.
fn is_fixnum_const(fb: &FunctionBuilder, v: ClifValue) -> bool {
    iconst_bits(fb, v)
        .is_some_and(|bits| (bits & FIXNUM_CHECK_MASK as i64) == FIXNUM_CHECK_VALUE as i64)
}

/// True if `v` is a compile-time constant (`iconst`) whose bits are a NON-HEAP
/// immediate — a fixnum, or a symbol (`nil`/`t`/keywords/interned names are all
/// symbol-tagged). Such a value provably never needs operand-stack GC rooting, so
/// a residual push can be skipped ENTIRELY at compile time — the baseline (no-MIR)
/// analogue of [`LispType::never_needs_gc_root`]. The predicate is exactly the one
/// [`emit_conditional_gc_push`] inlines at run time, so it can never mis-skip a
/// heap value; and heap constants are `iconst`-immune in the baseline anyway (R1a
/// routes them through the reloc load, never a baked pointer). Because the
/// baseline runs Cranelift at `opt_level="none"` (no constant folding), skipping
/// here removes a dead tag-test the optimizer would otherwise leave in.
fn is_nonheap_const(fb: &FunctionBuilder, v: ClifValue) -> bool {
    if let Some(bits) = iconst_bits(fb, v) {
        let bits = bits as usize;
        return (bits & FIXNUM_CHECK_MASK) == FIXNUM_CHECK_VALUE || (bits & TAG_MASK) == TAG_SYMBOL;
    }
    false
}

/// True if `v` PROVABLY holds the tagged bits of symbol `sym` at this point:
/// an `iconst` of exactly those bits (the JIT bake of a symbol constant, see
/// the `Op::Constant` lowering) or a load of the symbol's slot from the
/// per-leaf reloc vector (the AOT shape — same emission, reloc'd). This is
/// the SSA soundness gate for `Op::Call` speculation: `find_spec_sites`'
/// abstract stack tracking SELECTS the sites, but the spec shim call the
/// lowering emits IGNORES the runtime callee slot in favor of the baked
/// symbol — so the lowering only takes the spec path when this independent
/// proof holds, and any divergence in the tracking degrades to the generic
/// call instead of a wrong-callee mis-speculation. Copies made by
/// `Dup`/`StackRef`/`StackSet` reuse the same SSA value, so straight-line
/// propagation keeps the proof; values that crossed a block boundary are
/// variables (not an iconst/load result) and correctly fail it.
fn callee_is_symbol_const(
    fb: &FunctionBuilder,
    v: ClifValue,
    sym: u32,
    reloc_base: Option<ClifValue>,
    reloc_index: &std::collections::HashMap<usize, u32>,
) -> bool {
    use cranelift_codegen::ir::immediates::Offset32;
    use cranelift_codegen::ir::{InstructionData, Opcode, ValueDef};
    let expected_bits = Value::from_sym_id(crate::emacs_core::intern::SymId(sym)).bits();
    let ValueDef::Result(inst, _) = fb.func.dfg.value_def(v) else {
        return false;
    };
    match fb.func.dfg.insts[inst] {
        InstructionData::UnaryImm {
            opcode: Opcode::Iconst,
            imm,
        } => imm.bits() == expected_bits as i64,
        InstructionData::Load {
            opcode: Opcode::Load,
            arg,
            offset,
            ..
        } => {
            let Some(base) = reloc_base else {
                return false;
            };
            let Some(&idx) = reloc_index.get(&expected_bits) else {
                return false;
            };
            arg == base && offset == Offset32::new((idx * 8) as i32)
        }
        _ => false,
    }
}

/// True if `v` is provably a fixnum at this point — a fixnum constant
/// ([`is_fixnum_const`]) OR the output of [`retag_fixnum`], i.e.
/// `bor_imm(ishl_imm(_, k>=FIXNUM_SHIFT), FIXNUM_CHECK_VALUE)`, whose low tag
/// bits are exactly `0b10`. In either case a fixnum guard on `v` would always
/// pass, so it can be elided. The retag case extends redundant-guard elimination
/// to chained arithmetic WITHIN a block: the range-checked, retagged inner result
/// of `(+ (+ a b) c)` / `(< (1+ i) n)` is re-guarded for nothing. (Sound even if
/// some non-retag op produced the same bit pattern — any value with low bits
/// `0b10` passes the guard. opt_level=none keeps the instruction sequence stable.)
fn is_known_fixnum(fb: &FunctionBuilder, v: ClifValue) -> bool {
    use cranelift_codegen::ir::Opcode;
    if is_fixnum_const(fb, v) {
        return true;
    }
    let Some((shifted, tag)) = binary_value_and_iconst(fb, v, Opcode::Bor) else {
        return false;
    };
    if tag != FIXNUM_CHECK_VALUE as i64 {
        return false;
    }
    // The bor operand must clear the low FIXNUM_SHIFT bits (a left shift by at
    // least FIXNUM_SHIFT), so `v`'s low two bits are exactly the fixnum tag.
    binary_value_and_iconst(fb, shifted, Opcode::Ishl)
        .is_some_and(|(_, shift)| shift >= FIXNUM_SHIFT as i64)
}

/// Guard that `v` is a fixnum (`(v & 0b11) == 0b10`), deopting otherwise.
fn guard_fixnum(fb: &mut FunctionBuilder, deopt: Block, v: ClifValue, known: &HashSet<ClifValue>) {
    // Redundant-guard elimination: a value provably a fixnum needs no runtime
    // guard. Within-block: a fixnum constant or range-checked+retagged arithmetic
    // result ([`is_known_fixnum`]). Cross-block: an operand the dataflow analysis
    // proved fixnum at this block's entry ([`compute_known_fixnum_slots`], seeded
    // into `known` by `lower_leaf_full`).
    if is_known_fixnum(fb, v) || known.contains(&v) {
        return;
    }
    let tag = fb.ins().band_imm_u(v, FIXNUM_CHECK_MASK as i64);
    let is_fix = fb
        .ins()
        .icmp_imm_u(IntCC::Equal, tag, FIXNUM_CHECK_VALUE as i64);
    emit_guard(fb, deopt, is_fix);
}

/// Retag an untagged i64 `n` as a fixnum `Value`: `(n << 2) | 2`.
fn retag_fixnum(fb: &mut FunctionBuilder, n: ClifValue) -> ClifValue {
    let shifted = fb.ins().ishl_imm_u(n, FIXNUM_SHIFT as i64);
    fb.ins().bor_imm_u(shifted, FIXNUM_CHECK_VALUE as i64)
}

/// Lower a fixnum-fast-path binary op (`Add`/`Sub`) with the exact parity the
/// interpreter uses (`vm.rs` `Op::Add`): require both operands be fixnums and
/// the result be in fixnum range, else deopt. Returns the tagged-fixnum result.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn lower_fixnum_binop(
    fb: &mut FunctionBuilder,
    deopt: Block,
    is_sub: bool,
    a: ClifValue,
    b: ClifValue,
    known: &HashSet<ClifValue>,
) -> ClifValue {
    guard_fixnum(fb, deopt, a, known);
    guard_fixnum(fb, deopt, b, known);

    // Untag (arithmetic shift right by 2 == GNU XFIXNUM), compute, range-check.
    let av = fb.ins().sshr_imm_u(a, FIXNUM_SHIFT as i64);
    let bv = fb.ins().sshr_imm_u(b, FIXNUM_SHIFT as i64);
    // Operands are <= 61-bit, so the i64 result cannot overflow; a fixnum-range
    // check is sufficient and matches the interpreter exactly.
    let res = if is_sub {
        fb.ins().isub(av, bv)
    } else {
        fb.ins().iadd(av, bv)
    };

    // Guard: MOST_NEGATIVE_FIXNUM <= res <= MOST_POSITIVE_FIXNUM.
    let ge_lo = fb.ins().icmp_imm_u(
        IntCC::SignedGreaterThanOrEqual,
        res,
        Value::MOST_NEGATIVE_FIXNUM,
    );
    let le_hi = fb.ins().icmp_imm_u(
        IntCC::SignedLessThanOrEqual,
        res,
        Value::MOST_POSITIVE_FIXNUM,
    );
    let in_range = fb.ins().band(ge_lo, le_hi);
    emit_guard(fb, deopt, in_range);

    retag_fixnum(fb, res)
}

/// A fixnum-fast-path unary opcode.
#[derive(Clone, Copy)]
enum UnaryKind {
    /// `1+`: n -> n + 1.
    Add1,
    /// `1-`: n -> n - 1.
    Sub1,
    /// unary `-`: n -> -n.
    Negate,
}

/// Lower a fixnum-fast-path unary op with exact interpreter parity (`vm.rs`
/// `Op::Add1`/`Op::Sub1`/`Op::Negate`): require a fixnum operand whose result
/// stays in range, else deopt. The single out-of-range input per op is the
/// boundary fixnum, so the interpreter's `n != BOUND` guard is reproduced
/// exactly rather than a post-compute range check.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn lower_fixnum_unop(
    fb: &mut FunctionBuilder,
    deopt: Block,
    kind: UnaryKind,
    a: ClifValue,
    known: &HashSet<ClifValue>,
) -> ClifValue {
    guard_fixnum(fb, deopt, a, known);
    let n = fb.ins().sshr_imm_u(a, FIXNUM_SHIFT as i64);

    // The only input that leaves fixnum range is the op's boundary value.
    let bound = match kind {
        UnaryKind::Add1 => Value::MOST_POSITIVE_FIXNUM,
        UnaryKind::Sub1 | UnaryKind::Negate => Value::MOST_NEGATIVE_FIXNUM,
    };
    let in_range = fb.ins().icmp_imm_u(IntCC::NotEqual, n, bound);
    emit_guard(fb, deopt, in_range);

    let res = match kind {
        UnaryKind::Add1 => fb.ins().iadd_imm_u(n, 1),
        UnaryKind::Sub1 => fb.ins().iadd_imm_u(n, -1),
        UnaryKind::Negate => fb.ins().ineg(n),
    };
    retag_fixnum(fb, res)
}

/// Lower a fixnum multiply with exact interpreter parity (`vm.rs` `Op::Mul`):
/// both operands fixnums and the exact product in fixnum range, else deopt.
///
/// Operands are <= 61-bit so the product is <= 122-bit; widening to `i128` makes
/// it exact, then a single range check covers both i64 overflow and
/// fixnum-range overflow at once.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn lower_fixnum_mul(
    fb: &mut FunctionBuilder,
    deopt: Block,
    a: ClifValue,
    b: ClifValue,
    known: &HashSet<ClifValue>,
) -> ClifValue {
    guard_fixnum(fb, deopt, a, known);
    guard_fixnum(fb, deopt, b, known);
    let av = fb.ins().sshr_imm_u(a, FIXNUM_SHIFT as i64);
    let bv = fb.ins().sshr_imm_u(b, FIXNUM_SHIFT as i64);

    let a128 = fb.ins().sextend(types::I128, av);
    let b128 = fb.ins().sextend(types::I128, bv);
    let prod = fb.ins().imul(a128, b128);

    let lo = fb.ins().iconst(types::I64, Value::MOST_NEGATIVE_FIXNUM);
    let hi = fb.ins().iconst(types::I64, Value::MOST_POSITIVE_FIXNUM);
    let lo128 = fb.ins().sextend(types::I128, lo);
    let hi128 = fb.ins().sextend(types::I128, hi);
    let ge = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, prod, lo128);
    let le = fb.ins().icmp(IntCC::SignedLessThanOrEqual, prod, hi128);
    let in_range = fb.ins().band(ge, le);
    emit_guard(fb, deopt, in_range);

    let res = fb.ins().ireduce(types::I64, prod);
    retag_fixnum(fb, res)
}

/// Lower fixnum `/` or `%` with exact interpreter parity (`vm.rs`
/// `Op::Div`/`Op::Rem`): both operands fixnums and the divisor nonzero, else
/// deopt (the interpreter's `/` builtin signals arith-error on zero). Rust and
/// CLIF `sdiv`/`srem` both truncate toward zero, matching the interpreter; the
/// operands are <= 61-bit so the i64 ops cannot trap.
///
/// STALE PARITY (dead code): the interpreter's `Op::Div` fast path now
/// range-checks and routes `MOST_NEGATIVE_FIXNUM / -1` to the `/` builtin
/// (bignum promotion, like GNU), so wiring this lowering up would require the
/// same range guard the unboxed MIR analogue `raw_fixnum_divrem` already has
/// (it deopts on the overflow rather than wrapping).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn lower_fixnum_divrem(
    fb: &mut FunctionBuilder,
    deopt: Block,
    is_rem: bool,
    a: ClifValue,
    b: ClifValue,
    known: &HashSet<ClifValue>,
) -> ClifValue {
    guard_fixnum(fb, deopt, a, known);
    guard_fixnum(fb, deopt, b, known);
    let bv = fb.ins().sshr_imm_u(b, FIXNUM_SHIFT as i64);
    let nonzero = fb.ins().icmp_imm_u(IntCC::NotEqual, bv, 0);
    emit_guard(fb, deopt, nonzero);
    let av = fb.ins().sshr_imm_u(a, FIXNUM_SHIFT as i64);
    let res = if is_rem {
        fb.ins().srem(av, bv)
    } else {
        fb.ins().sdiv(av, bv)
    };
    retag_fixnum(fb, res)
}

/// A non-allocating unary type/nil predicate. Inspects only the tagged bits;
/// never dereferences the value, allocates, or deopts.
#[derive(Clone, Copy)]
enum PredKind {
    /// `null`/`not`: value is nil.
    Null,
    /// `consp`: value is a cons.
    Consp,
    /// `stringp`: value is a string.
    Stringp,
    /// `listp`: value is nil or a cons.
    Listp,
}

/// Lower a type/nil predicate to `t`/`nil` via `select` (no branch, no deopt —
/// it matches the interpreter for any value by inspecting the tag bits).
fn lower_predicate(fb: &mut FunctionBuilder, kind: PredKind, a: ClifValue) -> ClifValue {
    let cond = match kind {
        PredKind::Null => fb
            .ins()
            .icmp_imm_u(IntCC::Equal, a, Value::NIL.bits() as i64),
        PredKind::Consp => {
            let tag = fb.ins().band_imm_u(a, TAG_MASK as i64);
            fb.ins().icmp_imm_u(IntCC::Equal, tag, TAG_CONS as i64)
        }
        PredKind::Stringp => {
            let tag = fb.ins().band_imm_u(a, TAG_MASK as i64);
            fb.ins().icmp_imm_u(IntCC::Equal, tag, TAG_STRING as i64)
        }
        PredKind::Listp => {
            let is_nil = fb
                .ins()
                .icmp_imm_u(IntCC::Equal, a, Value::NIL.bits() as i64);
            let tag = fb.ins().band_imm_u(a, TAG_MASK as i64);
            let is_cons = fb.ins().icmp_imm_u(IntCC::Equal, tag, TAG_CONS as i64);
            fb.ins().bor(is_nil, is_cons)
        }
    };
    let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
    let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
    fb.ins().select(cond, t, nil)
}

/// Lower `car`/`cdr` (and the `-safe` variants) with exact interpreter parity:
/// a cons yields the loaded field; otherwise plain car/cdr yields nil for nil
/// and deopts for anything else (the interpreter signals
/// `wrong-type-argument`), while car-safe/cdr-safe yield nil for ANY non-cons
/// (total, no deopt). Non-allocating; reading a cons field needs no SATB
/// barrier (the barrier is on writes), and there is no GC safepoint here.
fn lower_car_cdr(
    fb: &mut FunctionBuilder,
    deopt: Option<Block>,
    is_cdr: bool,
    safe: bool,
    a: ClifValue,
) -> ClifValue {
    let tag = fb.ins().band_imm_u(a, TAG_MASK as i64);
    let is_cons = fb.ins().icmp_imm_u(IntCC::Equal, tag, TAG_CONS as i64);
    if !safe {
        let is_nil = fb
            .ins()
            .icmp_imm_u(IntCC::Equal, a, Value::NIL.bits() as i64);
        let valid = fb.ins().bor(is_cons, is_nil);
        emit_guard(
            fb,
            deopt.expect("guarded car/cdr lowers with a deopt site"),
            valid,
        );
    }

    // Branch: cons -> load the field; nil -> nil. The result flows through a
    // fresh SSA variable (Cranelift inserts the phi at the merge).
    let res = fb.declare_var(types::I64);
    let cons_blk = fb.create_block();
    let nil_blk = fb.create_block();
    let merge = fb.create_block();
    fb.ins().brif(is_cons, cons_blk, &[], nil_blk, &[]);

    fb.switch_to_block(cons_blk);
    let ptr = fb.ins().band_imm_u(a, !(TAG_MASK as i64));
    let offset = if is_cdr {
        core::mem::offset_of!(ConsCell, cdr_or_next)
    } else {
        core::mem::offset_of!(ConsCell, car)
    };
    let field = fb
        .ins()
        .load(types::I64, MemFlagsData::trusted(), ptr, offset as i32);
    fb.def_var(res, field);
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(nil_blk);
    if safe {
        // -safe variants: ANY non-cons yields nil.
        let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
        fb.def_var(res, nil);
    } else {
        fb.def_var(res, a); // nil -> nil (a already holds nil, guarded above)
    }
    fb.ins().jump(merge, &[]);

    fb.switch_to_block(merge);
    fb.use_var(res)
}

/// Lower a no-argument straight-line leaf body. Thin wrapper over [`lower_leaf`]
/// kept for the existing call sites/tests.
pub fn lower_nullary_leaf(ops: &[Op], constants: &[Value]) -> Result<CompiledLeaf, CompileError> {
    lower_leaf(ops, constants, 0)
}

/// Get MIR value `v` as a RAW (untagged) fixnum i64 for arithmetic. If `cval_raw`
/// marks it already raw (a prior fixnum arithmetic result or fixnum constant in
/// this block), use it directly — no re-guard, no re-untag (the unboxing fast
/// path: chained fixnum arithmetic stays raw). Otherwise guard it is a fixnum
/// (deopt else) and untag.
fn mir_as_raw(
    fb: &mut FunctionBuilder,
    cval: &[Option<ClifValue>],
    cval_raw: &[bool],
    v: mir::MirValue,
    deopt: Block,
) -> Result<ClifValue, CompileError> {
    let i = v.0 as usize;
    let cv = cval[i].ok_or(CompileError::BadOperand)?;
    if cval_raw[i] {
        Ok(cv)
    } else {
        guard_fixnum(fb, deopt, cv, &HashSet::new());
        Ok(fb.ins().sshr_imm_u(cv, FIXNUM_SHIFT as i64))
    }
}

/// Get MIR value `v` as a TAGGED `Value` (for boundaries: returns, predicates,
/// car/cdr, cross-block block args). Retags a raw fixnum; passes a tagged value
/// through unchanged.
fn mir_as_tagged(
    fb: &mut FunctionBuilder,
    cval: &[Option<ClifValue>],
    cval_raw: &[bool],
    v: mir::MirValue,
) -> Result<ClifValue, CompileError> {
    let i = v.0 as usize;
    let cv = cval[i].ok_or(CompileError::BadOperand)?;
    if cval_raw[i] {
        Ok(retag_fixnum(fb, cv))
    } else {
        Ok(cv)
    }
}

/// Force MIR value `v` to its TAGGED form IN PLACE (mutating `cval`/`cval_raw`),
/// returning the tagged value. Wired by the calls-slice (next increment); kept
/// separate so the soundness-critical force-tag/deopt-routing logic lands and is
/// reviewable on its own. Use before a call (a GC SAFEPOINT): a raw
/// (untagged) fixnum must not be live across a call — the concurrent GC would
/// trace the bare i64 as a tagged pointer (a raw `3` has bits `0b011` == TAG_CONS
/// -> a bogus rooted cons -> UAF). Unlike [`mir_as_tagged`] (which retags WITHOUT
/// writing back), this clears the raw mask so every LATER use and every
/// deopt-framestate snapshot sees the tagged form — no stale raw alias survives
/// the safepoint. The MIR analogue of the baseline's `stack_force_tagged`.
fn mir_force_tagged(
    fb: &mut FunctionBuilder,
    cval: &mut [Option<ClifValue>],
    cval_raw: &mut [bool],
    v: mir::MirValue,
) -> Result<ClifValue, CompileError> {
    let i = v.0 as usize;
    let cv = cval[i].ok_or(CompileError::BadOperand)?;
    if cval_raw[i] {
        let tagged = retag_fixnum(fb, cv);
        cval[i] = Some(tagged);
        cval_raw[i] = false;
        Ok(tagged)
    } else {
        Ok(cv)
    }
}

/// Root one live residual `v` (already tagged) across a GC safepoint, inlining
/// the `is_heap_object` tag test so a non-heap value (fixnum or symbol) skips
/// the `neovm_jit_gc_push` shim CALL entirely at run time.
///
/// Used for residuals whose MIR type is `Unknown`/`Any` — not provably
/// immediate (those skip the push at *compile* time, [`LispType::never_needs_gc_root`])
/// and not provably heap (those get an unconditional push,
/// [`LispType::provably_heap`]). Empirically most such residuals resolve to
/// fixnum accumulators or symbol arguments at run time, so the branch is
/// overwhelmingly not-taken and predicts well.
///
/// CORRECTNESS: the emitted test skips the push ONLY for values the tag layout
/// guarantees are non-heap. Every non-heap `Value` is either a fixnum
/// (`bits & FIXNUM_CHECK_MASK == FIXNUM_CHECK_VALUE`) or a symbol
/// (`bits & TAG_MASK == TAG_SYMBOL`) — nil/t are symbols, chars are fixnums —
/// while every heap tag (cons/string/veclike/float) satisfies NEITHER predicate.
/// So `!(is_fixnum | is_symbol)` is an exact, layout-anchored `is_heap_object`
/// and can never drop a live heap root (which under GC would be a
/// use-after-free). The shim additionally re-checks `is_heap_object`, so even
/// the unused tag `0b001` (never produced) would be handled safely if pushed.
/// Is lever 1 (the inlined `is_heap_object` residual-rooting tag test) enabled?
/// Default yes; `NEOVM_JIT_LEVER1=off` reverts residual rooting to the pre-lever-1
/// behavior — an UNCONDITIONAL `neovm_jit_gc_push` per residual, with no
/// compile-time non-heap-constant skip — so lever 1's per-call effect can be
/// A/B-measured against the old code in a SINGLE build (pair with a call-heavy
/// bench like `jit_bench_fib`, whose recursive-call residual is a fixnum that
/// lever 1 skips at run time). Cached once per process.
fn jit_lever1_on() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NEOVM_JIT_LEVER1").as_deref() != Ok("off"))
}

/// Bracketing state for residual-root windows: `base == None` means the
/// site emitted no rooting at all (statically empty to-root set); otherwise
/// the frame-base `jit_root_stack_top` value loaded before the stores, which
/// the post-call helper writes back.
#[derive(Clone, Copy)]
struct CondRoots {
    base: Option<ClifValue>,
}

impl CondRoots {
    const NONE: Self = Self { base: None };
}

/// Compile-time byte offsets of the [`Context`] JIT root-window mirror fields
/// generated code reads/writes ((ptr, top, cap)).
fn ctx_rootwin_offsets() -> (i32, i32, i32) {
    (
        core::mem::offset_of!(Context, jit_root_stack_ptr) as i32,
        core::mem::offset_of!(Context, jit_root_stack_top) as i32,
        core::mem::offset_of!(Context, jit_root_stack_cap) as i32,
    )
}

/// Store `to_root` into the ctx residual-root window at `[top..top+N)` and
/// bump `top`, returning the saved frame base for the post-call restore.
///
/// This replaces the gc_save / gc_push_many / gc_restore shim trio (~3 calls
/// plus per-value pushes, measured at ~123 Ir/call with heap residuals): on
/// the non-grow path it is two field loads, one compare, N+1 stores and no
/// calls. `top` is invariant between sites (every site restores it), so the
/// fresh load here always sees the frame base; nested calls stack naturally.
/// Slots below the stack's length always hold valid tagged Values
/// (NIL-initialized, only ever overwritten by these tagged stores), so the
/// tracer's `0..top` walk never sees garbage and stale slots merely
/// over-retain, exactly like interpreter operand-stack residue.
fn emit_root_window_stores(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    to_root: &[ClifValue],
) -> ClifValue {
    let (off_ptr, off_top, off_cap) = ctx_rootwin_offsets();
    let vmctx = fb.use_var(rt.vmctx_var);
    let base = fb
        .ins()
        .load(types::I64, MemFlagsData::trusted(), vmctx, off_top);
    let need = fb.ins().iadd_imm_u(base, to_root.len() as i64);
    let cap = fb
        .ins()
        .load(types::I64, MemFlagsData::trusted(), vmctx, off_cap);
    let fits = fb.ins().icmp(IntCC::UnsignedLessThanOrEqual, need, cap);
    let grow_blk = fb.create_block();
    let store_blk = fb.create_block();
    fb.ins().brif(fits, store_blk, &[], grow_blk, &[]);
    fb.switch_to_block(grow_blk);
    fb.seal_block(grow_blk);
    fb.ins().call(rt.refs.rootwin_grow, &[vmctx, need]);
    fb.ins().jump(store_blk, &[]);
    fb.switch_to_block(store_blk);
    fb.seal_block(store_blk);
    // Re-load the (possibly regrown) buffer pointer AFTER the capacity gate.
    let ptr = fb
        .ins()
        .load(rt.ptr_ty, MemFlagsData::trusted(), vmctx, off_ptr);
    let byte_off = fb.ins().ishl_imm_u(base, 3);
    let slot0 = fb.ins().iadd(ptr, byte_off);
    for (i, &v) in to_root.iter().enumerate() {
        fb.ins()
            .store(MemFlagsData::trusted(), v, slot0, (i * 8) as i32);
    }
    fb.ins()
        .store(MemFlagsData::trusted(), need, vmctx, off_top);
    base
}

fn emit_cond_residual_roots_pre(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    values: &[ClifValue],
) -> CondRoots {
    let on = jit_lever1_on();
    let mut to_root: Vec<ClifValue> = Vec::with_capacity(values.len());
    for &v in values {
        if on && is_nonheap_const(fb, v) {
            continue; // provably non-heap immediate: nothing to root.
        }
        to_root.push(v);
    }
    if to_root.is_empty() {
        return CondRoots::NONE;
    }
    CondRoots {
        base: Some(emit_root_window_stores(fb, rt, &to_root)),
    }
}

fn emit_cond_residual_roots_post(fb: &mut FunctionBuilder, rt: &RtCtx, cr: CondRoots) {
    let Some(base) = cr.base else {
        return;
    };
    // Pop the site's residual window: top back to the frame base.
    let (_, off_top, _) = ctx_rootwin_offsets();
    let vmctx = fb.use_var(rt.vmctx_var);
    fb.ins()
        .store(MemFlagsData::trusted(), base, vmctx, off_top);
}

/// The deopt landing block for a guard-emitting MIR inst. In a CALL-BEARING body
/// (`precise`), every guard gets a fresh PER-SITE STATUS_DEOPT_AT block capturing
/// the inst's pre-op operand stack from `inst.pre_stack` (snapshotted EAGERLY
/// through `cval`/`cval_raw`, because a later call force-tags residual slots and
/// would otherwise corrupt a pre-call guard's raw mask) — NEVER rerun-from-start,
/// which would re-execute a call's side effect (the loop-back-edge hole the
/// adversarial critique caught). In a pure body it is the shared rerun-from-start
/// block (STATUS_DEOPT), created lazily.
fn mir_deopt_block(
    fb: &mut FunctionBuilder,
    precise: bool,
    inst: &mir::MirInst,
    cval: &[Option<ClifValue>],
    cval_raw: &[bool],
    shared: &mut Option<Block>,
    pending: &mut Vec<PendingDeopt>,
) -> Result<Block, CompileError> {
    if precise {
        let mut stack = Vec::with_capacity(inst.pre_stack.len());
        let mut raw = Vec::with_capacity(inst.pre_stack.len());
        for v in &inst.pre_stack {
            stack.push(cval[v.0 as usize].ok_or(CompileError::BadOperand)?);
            raw.push(cval_raw[v.0 as usize]);
        }
        // handlers_len = 0: build_mir bails on handler/bind opcodes, so a MIR leaf
        // never has condition-case/catch frames to transfer on resume.
        Ok(deopt_site(fb, inst.pc, 0, &stack, &raw, pending))
    } else {
        Ok(*shared.get_or_insert_with(|| fb.create_block()))
    }
}

/// Raw fixnum add/sub: operands and result are untagged i64 (no untag/retag), with
/// the interpreter's fixnum-range check (deopt on overflow). The unboxed analogue
/// of [`lower_fixnum_binop`].
fn raw_fixnum_addsub(
    fb: &mut FunctionBuilder,
    deopt: Block,
    is_sub: bool,
    av: ClifValue,
    bv: ClifValue,
) -> ClifValue {
    let res = if is_sub {
        fb.ins().isub(av, bv)
    } else {
        fb.ins().iadd(av, bv)
    };
    let ge_lo = fb.ins().icmp_imm_u(
        IntCC::SignedGreaterThanOrEqual,
        res,
        Value::MOST_NEGATIVE_FIXNUM,
    );
    let le_hi = fb.ins().icmp_imm_u(
        IntCC::SignedLessThanOrEqual,
        res,
        Value::MOST_POSITIVE_FIXNUM,
    );
    let in_range = fb.ins().band(ge_lo, le_hi);
    emit_guard(fb, deopt, in_range);
    res
}

/// Raw fixnum 1+/1-/negate: untagged in, untagged out, with the interpreter's
/// boundary check (deopt on the single out-of-range input). Unboxed analogue of
/// [`lower_fixnum_unop`].
fn raw_fixnum_unop(
    fb: &mut FunctionBuilder,
    deopt: Block,
    kind: UnaryKind,
    av: ClifValue,
) -> ClifValue {
    let bound = match kind {
        UnaryKind::Add1 => Value::MOST_POSITIVE_FIXNUM,
        UnaryKind::Sub1 | UnaryKind::Negate => Value::MOST_NEGATIVE_FIXNUM,
    };
    let in_range = fb.ins().icmp_imm_u(IntCC::NotEqual, av, bound);
    emit_guard(fb, deopt, in_range);
    match kind {
        UnaryKind::Add1 => fb.ins().iadd_imm_u(av, 1),
        UnaryKind::Sub1 => fb.ins().iadd_imm_u(av, -1),
        UnaryKind::Negate => fb.ins().ineg(av),
    }
}

/// Raw fixnum `*`: untagged in/out, widen to i128 for the product + the
/// interpreter's fixnum-range check (deopt on overflow). Unboxed analogue of
/// [`lower_fixnum_mul`].
fn raw_fixnum_mul(
    fb: &mut FunctionBuilder,
    deopt: Block,
    av: ClifValue,
    bv: ClifValue,
) -> ClifValue {
    let a128 = fb.ins().sextend(types::I128, av);
    let b128 = fb.ins().sextend(types::I128, bv);
    let prod = fb.ins().imul(a128, b128);
    let lo = fb.ins().iconst(types::I64, Value::MOST_NEGATIVE_FIXNUM);
    let hi = fb.ins().iconst(types::I64, Value::MOST_POSITIVE_FIXNUM);
    let lo128 = fb.ins().sextend(types::I128, lo);
    let hi128 = fb.ins().sextend(types::I128, hi);
    let ge = fb.ins().icmp(IntCC::SignedGreaterThanOrEqual, prod, lo128);
    let le = fb.ins().icmp(IntCC::SignedLessThanOrEqual, prod, hi128);
    let in_range = fb.ins().band(ge, le);
    emit_guard(fb, deopt, in_range);
    fb.ins().ireduce(types::I64, prod)
}

/// Raw fixnum `/`/`%`: untagged in/out. Deopts on a zero divisor (interpreter
/// signals arith-error). Operands are <= 61-bit so `sdiv`/`srem` cannot trap.
/// For `/`, the only out-of-fixnum-range result is MOST_NEGATIVE_FIXNUM / -1 (a
/// wrap in the interpreter); deopt on it for parity rather than keep an
/// out-of-range raw value (`%` is always in range). Unboxed analogue of
/// [`lower_fixnum_divrem`].
fn raw_fixnum_divrem(
    fb: &mut FunctionBuilder,
    deopt: Block,
    is_rem: bool,
    av: ClifValue,
    bv: ClifValue,
) -> ClifValue {
    let nonzero = fb.ins().icmp_imm_u(IntCC::NotEqual, bv, 0);
    emit_guard(fb, deopt, nonzero);
    if is_rem {
        fb.ins().srem(av, bv)
    } else {
        let res = fb.ins().sdiv(av, bv);
        let ge = fb.ins().icmp_imm_u(
            IntCC::SignedGreaterThanOrEqual,
            res,
            Value::MOST_NEGATIVE_FIXNUM,
        );
        let le = fb.ins().icmp_imm_u(
            IntCC::SignedLessThanOrEqual,
            res,
            Value::MOST_POSITIVE_FIXNUM,
        );
        let in_range = fb.ins().band(ge, le);
        emit_guard(fb, deopt, in_range);
        res
    }
}

/// Raw fixnum `max`/`min`: untagged in/out, a branchless `select` of the two
/// already-range-valid operands (no overflow, no deopt of its own).
fn raw_fixnum_maxmin(
    fb: &mut FunctionBuilder,
    is_min: bool,
    av: ClifValue,
    bv: ClifValue,
) -> ClifValue {
    let cc = if is_min {
        IntCC::SignedLessThan
    } else {
        IntCC::SignedGreaterThan
    };
    let cond = fb.ins().icmp(cc, av, bv);
    fb.ins().select(cond, av, bv)
}

/// **MIR Tier-2 lowering.** Lower a [`mir::MirFunction`] to a [`CompiledLeaf`] by
/// driving CLIF emission from the MIR instead of a bytecode walk. Wired into
/// `compile_bytecode_function_inner` as the live optimizing tier. A *pure* body
/// (arithmetic / comparisons / type predicates / car-cdr / stack — no shim-using
/// ops) needs no vmctx and reruns the interpreter from the start on a failing
/// guard (sound: no side effect precedes any guard). A call-bearing body threads
/// vmctx + the runtime shims and routes every guard to a per-site precise deopt
/// (see below).
///
/// Uses CLIF **block parameters** as the SSA phis — each MIR block becomes a
/// CLIF block whose params are its entry operand stack, and terminator edges
/// pass the live stack as block arguments. Validated by differential tests
/// against the interpreter and the force-deopt gate.
pub(crate) fn lower_mir_pure(m: &mir::MirFunction) -> Result<CompiledLeaf, CompileError> {
    use mir::MirOp;

    // The MIR tier handles a CALL (MirOp::Opaque{Call/Apply}) via PRECISE deopt:
    // such a body threads vmctx + the runtime shims and routes EVERY guard to a
    // per-site STATUS_DEOPT_AT (all-precise — a call-bearing body must never
    // rerun-from-start, which would re-execute the call's side effect).
    let has_call = m.blocks.iter().any(|b| {
        b.insts.iter().any(|i| {
            matches!(
                &i.op,
                MirOp::Opaque {
                    op: Op::Call(_) | Op::Apply(_),
                    ..
                }
            )
        })
    });

    // Escape analysis (hoisted — depends only on `m`). A NON-escaping cons is elided
    // (scalar-replaced, no allocation); an ESCAPING cons is heap-allocated via the
    // neovm_jit_cons shim so the body stays in the MIR tier. Both the calls-slice and
    // cons allocation need the runtime scaffolding (needs_rt: vmctx + shims), but a
    // cons allocation is a GC SAFEPOINT, NOT an observable side effect — so it does
    // NOT force precise deopt. precise (+ has_side_effects) stay = has_call:
    // rerun-from-start re-allocates a fresh (never-escaped) cons, which is sound.
    let cons_repl: Vec<Option<(mir::MirValue, mir::MirValue)>> = if has_call {
        vec![None; m.value_types.len()]
    } else {
        mir::cons_scalar_repl_targets(m)
    };
    let has_escaping_cons = m
        .blocks
        .iter()
        .flat_map(|b| b.insts.iter())
        .any(|i| matches!(&i.op, MirOp::Cons(..)) && cons_repl[i.result.0 as usize].is_none());
    let needs_rt = has_call || has_escaping_cons;

    // --- JIT-only module prologue (the wrapper). ----------------------------
    // The three ObjectModule-incompatible seams that stay here (and out of the
    // generic build fn `build_mir_leaf_fn`): `builder.symbol(...)` bakes the shim
    // host addresses (AOT replaces this with `Linkage::Import` + dlopen);
    // `JITModule::new` (AOT: `ObjectModule::new`); `finalize_definitions` +
    // `get_finalized_function` below (AOT: `ObjectModule::finish()` + `dlsym`).
    let mut builder = JITBuilder::with_isa(jit_isa()?, default_libcall_names());
    if needs_rt {
        // The shims the calls-slice + cons allocation reference; declare_rt_refs
        // declares the full import set but Cranelift resolves only referenced ones.
        builder.symbol("neovm_jit_gc_save", neovm_jit_gc_save as *const u8);
        builder.symbol("neovm_jit_gc_push", neovm_jit_gc_push as *const u8);
        builder.symbol(
            "neovm_jit_gc_push_many",
            neovm_jit_gc_push_many as *const u8,
        );
        builder.symbol("neovm_jit_gc_restore", neovm_jit_gc_restore as *const u8);
        builder.symbol(
            "neovm_jit_rootwin_grow",
            neovm_jit_rootwin_grow as *const u8,
        );
        builder.symbol("neovm_jit_call", neovm_jit_call as *const u8);
        builder.symbol("neovm_jit_apply", neovm_jit_apply as *const u8);
        builder.symbol("neovm_jit_cons", neovm_jit_cons as *const u8);
    }
    let mut module = JITModule::new(builder);

    // Precise-deopt spill buffer + cells, sized to the deepest pre-op operand stack
    // (the framestate a post-call guard spills). Empty/inert for pure bodies (which
    // keep the rerun-from-start STATUS_DEOPT path).
    let max_depth = m
        .blocks
        .iter()
        .flat_map(|b| b.insts.iter())
        .map(|i| i.pre_stack.len())
        .max()
        .unwrap_or(0);
    let deopt_spill: Box<[core::cell::Cell<i64>]> = if has_call {
        (0..max_depth).map(|_| core::cell::Cell::new(0)).collect()
    } else {
        Box::from([])
    };
    let deopt_meta: Box<DeoptCells> = Box::new(DeoptCells {
        pc: core::cell::Cell::new(0),
        depth: core::cell::Cell::new(0),
        handlers: core::cell::Cell::new(0),
    });

    // R1a: per-leaf heap-constant reloc vector — collect the DISTINCT heap-object
    // constants (deduped by tagged bits) so generated code loads each from
    // reloc_data[idx] instead of baking its heap pointer as an immediate (untraced
    // by the GC, unportable to an AOT .so). Fixnums + non-heap immediates (nil/t)
    // stay baked. Allocated here (before the FunctionBuilder) so reloc_data.as_ptr()
    // is stable when the loads bake its base address (same pattern as deopt_spill).
    let mut reloc_vals: Vec<Value> = Vec::new();
    let mut reloc_index: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    for blk in &m.blocks {
        for inst in &blk.insts {
            if let MirOp::Const(v) = &inst.op {
                let bits = v.bits();
                if (bits & FIXNUM_CHECK_MASK) != FIXNUM_CHECK_VALUE
                    && v.is_heap_object()
                    && !reloc_index.contains_key(&bits)
                {
                    reloc_index.insert(bits, reloc_vals.len() as u32);
                    reloc_vals.push(*v);
                }
            }
        }
    }
    let reloc_data: Box<[Value]> = reloc_vals.into_boxed_slice();

    // Build + define the leaf into the module via the module-generic seam
    // (`build_mir_leaf_fn`). The buffers are owned here and threaded in by
    // reference so their addresses (baked into the generated loads) stay stable
    // and so the wrapper can move them into the returned `CompiledLeaf`.
    let fid = build_mir_leaf_fn(
        &mut module,
        m,
        &deopt_spill,
        &deopt_meta,
        &reloc_data,
        &reloc_index,
        has_call,
        &cons_repl,
        needs_rt,
        "__neovm_mir_leaf",
        Linkage::Local,
        /*aot=*/ false,
    )?;

    // --- JIT-only module epilogue (the wrapper). ----------------------------
    module
        .finalize_definitions()
        .map_err(|e| CompileError::Backend(BackendError::Finalize(e.to_string())))?;
    let entry = module.get_finalized_function(fid);

    Ok(CompiledLeaf {
        tier: LeafTier::Mir,
        arity: m.arity,
        required: m.arity,
        has_rest: false,
        has_binds: false,
        has_handlers: false,
        // Set by compile_bytecode_function_inner after a successful inline pass.
        inline_epoch: None,
        // A call-bearing body runs a side effect ahead of its (precise) deopts, so
        // it must never rerun-from-start (the refuse-to-rerun guard).
        has_side_effects: has_call,
        // Baseline default; compile_bytecode_function_inner overrides with the
        // actual inlined-callee SymIds after the inline pass.
        inline_deps: Box::from([]),
        spec_slots: Box::from([]),
        spec_expected: Box::from([]),
        deopt_spill,
        deopt_meta,
        reloc_data,
        // JIT bakes its bases as iconst; the 4th entry arg is ignored.
        sidecar: None,
        // MIR leaves are only built for unpatched sources (see
        // compile_bytecode_function_inner).
        dynamic_prefix: 0,
        entry,
        _backing: LeafBacking::Jit(module),
    })
}

/// The host ISA for JIT modules, with cranelift-jit's own flag defaults
/// (`use_colocated_libcalls=false`, `is_pic=false` — mirrored by the AOT
/// module builder, which flips only `is_pic`) plus ONE deliberate change:
/// the Cranelift IR **verifier** runs only in debug builds. Cranelift enables
/// it by default and `JITBuilder::new` inherited that, so every production
/// tier-up paid a full IR verification pass (~12% of the compile Ir on the
/// fontify sim's 352-op font-lock body). The verifier exists to catch
/// lowering bugs, which debug/test builds still do; release compiles are
/// trusted the same way a shipped compiler's are.
fn jit_isa() -> Result<std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa>, CompileError> {
    use cranelift_codegen::settings::{self, Configurable};
    let init_err = |e: String| CompileError::Backend(BackendError::ModuleInit(e));
    let mut flags = settings::builder();
    flags
        .set("use_colocated_libcalls", "false")
        .map_err(|e| init_err(e.to_string()))?;
    flags
        .set("is_pic", "false")
        .map_err(|e| init_err(e.to_string()))?;
    flags
        .set(
            "enable_verifier",
            if cfg!(debug_assertions) {
                "true"
            } else {
                "false"
            },
        )
        .map_err(|e| init_err(e.to_string()))?;
    cranelift_native::builder()
        .map_err(|e| init_err(e.to_string()))?
        .finish(settings::Flags::new(flags))
        .map_err(|e| init_err(e.to_string()))
}

/// Whether a constant `Value` must be routed through the per-leaf reloc vector
/// when lowering for **AOT** (`true`) instead of being baked as an `iconst`.
///
/// A baked immediate is only valid in the SESSION that emitted it. Two kinds of
/// constant carry session-specific bits and so cannot be baked into a
/// cross-session `.so`:
///   * HEAP OBJECTS (string/cons/vector/float) — the bits are a heap pointer
///     (already routed through reloc by R1a, via `is_heap_object()`).
///   * SYMBOLS other than `nil`/`t` — the bits encode a `SymId`, which is
///     INTERN-ORDER dependent (`intern.rs`: `SymId(symbols.len())`), so the same
///     name interns to a different id in a different session. `nil`/`t` are
///     pre-seeded at fixed ids 0/1, so they ARE session-stable and stay baked.
///
/// Everything else — fixnums (chars are fixnums in `[0, MAX_CHAR]`), `nil`, `t`
/// — is a universal immediate with session-stable bits and is baked in both
/// tiers. For the JIT (`aot=false`) only heap objects reloc (symbols bake, which
/// is correct same-session and keeps the JIT byte-identical); the broader symbol
/// reloc applies ONLY to AOT. (Audit #16.)
pub(crate) fn const_relocs_for_aot(v: Value) -> bool {
    v.is_heap_object() || (v.is_symbol() && v != Value::NIL && v != Value::T)
}

/// Module-generic build seam for [`lower_mir_pure`]: sets up the leaf ABI
/// signature, lowers the MIR through a `FunctionBuilder`, then declares +
/// defines the function into `module`, returning its `FuncId`. CLIF output is
/// byte-identical to the previous in-line lowering — this is a pure extraction.
///
/// Generic over `M: Module` so the same lowering drives the `JITModule` JIT
/// path today and an `ObjectModule` AOT path later, unchanged. The buffers
/// (`deopt_spill`/`deopt_meta`/`reloc_data`) are borrowed: their stable
/// addresses are baked into the generated code, and the caller retains
/// ownership to move them into the `CompiledLeaf`.
///
/// This fn deliberately contains NONE of the three ObjectModule-incompatible
/// JIT seams, which stay in the [`lower_mir_pure`] wrapper:
///   * `builder.symbol(...)`    — AOT: `Linkage::Import` resolved via dlopen.
///   * `finalize_definitions()` — AOT: `ObjectModule::finish()`.
///   * `get_finalized_function` — AOT: `dlsym` of the exported entry symbol.
///
/// `pub(crate)` so the AOT path (`jit::aot`) can drive it with `M = ObjectModule`.
///
/// `entry_name` / `entry_linkage` parameterize ONLY the entry's symbol-table
/// declaration (not the CLIF body, which stays byte-identical): the JIT wrapper
/// passes `("__neovm_mir_leaf", Linkage::Local)` exactly as before, while the AOT
/// path passes a unique `("__neovm_aot_{hash}_{tag}", Linkage::Export)` so the
/// `.o` exports a symbol the loader can `dlsym`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_mir_leaf_fn<M: Module>(
    module: &mut M,
    m: &mir::MirFunction,
    deopt_spill: &[core::cell::Cell<i64>],
    deopt_meta: &DeoptCells,
    reloc_data: &[Value],
    reloc_index: &std::collections::HashMap<usize, u32>,
    has_call: bool,
    cons_repl: &[Option<(mir::MirValue, mir::MirValue)>],
    needs_rt: bool,
    entry_name: &str,
    entry_linkage: Linkage,
    // R1c-sidecar: false → JIT (bases baked as `iconst` from the passed-in buffer
    // addresses, unchanged/fast); true → AOT (bases loaded from the 4th entry arg,
    // the per-thread `LeafSidecar`, since the addresses are session-specific). The
    // CLIF body is otherwise identical — same RESULTS either way.
    aot: bool,
) -> Result<cranelift_module::FuncId, CompileError> {
    use mir::{BinKind, CmpKind, MirOp, MirTerm, PredKind as MP, UnaryKind as MU};

    // Phase-0 fix: this reset + the post-finalize set below used to exist only
    // in the baseline `build_leaf_fn`, so a Tier-2 compile's trace line
    // reported the PREVIOUS baseline compile's IR stats.
    LAST_IR_STATS.with(|c| c.set((0, 0, 0, 0)));

    let frontend_config = module.target_config();
    let call_conv = frontend_config.default_call_conv;
    let ptr_ty = frontend_config.pointer_type();

    // Unified 4-param entry ABI: fn(vmctx, args, out, sidecar) -> status. The
    // `sidecar` param is the per-(thread,leaf) base block (LeafSidecar). AOT code
    // reads its bases from it (`aot=true`); JIT code declares it but never reads
    // it (`aot=false`, bases stay `iconst`), so the dispatch passes null.
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(ptr_ty)); // vmctx (unused for pure)
    sig.params.push(AbiParam::new(ptr_ty)); // args
    sig.params.push(AbiParam::new(ptr_ty)); // out
    sig.params.push(AbiParam::new(ptr_ty)); // sidecar (*const LeafSidecar)
    sig.returns.push(AbiParam::new(types::I64));

    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig.clone());
    let mut fbctx = FunctionBuilderContext::new();
    {
        let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);

        // One CLIF block per MIR block, params = the MIR block's params.
        let clif_blocks: Vec<Block> = m
            .blocks
            .iter()
            .map(|blk| {
                let cb = fb.create_block();
                for _ in &blk.params {
                    fb.append_block_param(cb, types::I64);
                }
                cb
            })
            .collect();

        // Runtime context for calls (vmctx + shims + arg/result slots), built only
        // when the body has a call. declare_rt_refs declares the full import set;
        // only the referenced shims (call/apply/gc_*) are resolved at finalize.
        let rt = if needs_rt {
            // `module` is already `&mut M`; reborrow it for the call. The MIR
            // tier never emits subr-speculated or CBSym-intrinsic calls
            // (subr_spec=false, cbsym_spec=false).
            let refs = declare_rt_refs(&mut *module, fb.func, call_conv, ptr_ty, false, false)?;
            let vmctx_var = fb.declare_var(ptr_ty);
            let max_call_args = m
                .blocks
                .iter()
                .flat_map(|b| b.insts.iter())
                .filter_map(|i| match &i.op {
                    MirOp::Opaque {
                        op: Op::Call(n) | Op::Apply(n),
                        ..
                    } => Some(*n as usize),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let call_args_slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (max_call_args.max(1) * 8) as u32,
                3,
            ));
            let call_result_slot =
                fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            // Lever 2: residual gather buffer, sized to the max pre-op operand-stack
            // depth (an upper bound on any site's residual count).
            let max_residual = m
                .blocks
                .iter()
                .flat_map(|b| b.insts.iter())
                .map(|i| i.pre_stack.len())
                .max()
                .unwrap_or(0);
            let residual_buf_slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (max_residual.max(1) * 8) as u32,
                3,
            ));
            let gc_saved_slot =
                fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            Some(RtCtx {
                refs,
                vmctx_var,
                ptr_ty,
                call_args_slot,
                call_result_slot,
                residual_buf_slot,
                gc_saved_slot,
            })
        } else {
            None
        };
        // The deopt-buffer base addresses (for the JIT `iconst` path). The CLIF
        // `DeoptRefs` (iconst or sidecar-load) is materialized in the entry block
        // below, once it is populated and the sidecar param is available.
        let spill_base_addr = deopt_spill.as_ptr() as i64;
        let meta_pc_addr = &deopt_meta.pc as *const core::cell::Cell<i64> as i64;
        let meta_depth_addr = &deopt_meta.depth as *const core::cell::Cell<i64> as i64;
        let meta_handlers_addr = &deopt_meta.handlers as *const core::cell::Cell<i64> as i64;
        // ALL-PRECISE deopt for call-bearing bodies (see mir_deopt_block): never
        // rerun-from-start after a call. Pure bodies keep the shared rerun block.
        let precise = has_call;
        // (cons_repl + needs_rt computed by the wrapper, threaded in as params.)
        let mut pending: Vec<PendingDeopt> = Vec::new();
        // Shared signal-propagation block (returns STATUS_SIGNAL), created lazily by
        // the first call lowering.
        let mut signal_exit: Option<Block> = None;

        // Map every MIR value to its CLIF value (filled in dominance order: a
        // single forward pass works because the MIR is SSA and block params
        // carry all cross-block values).
        let mut cval: Vec<Option<ClifValue>> = vec![None; m.value_types.len()];
        // Per-value form: true if `cval` holds an UNTAGGED raw fixnum (unboxing).
        // Fixnum arithmetic results + fixnum constants stay raw WITHIN a block (no
        // intermediate retag/untag/re-guard); boundaries (returns, predicates,
        // car/cdr, cross-block args) retag. Block params/args + non-fixnum values
        // are tagged (false) — no raw phis (the simpler, sound scope).
        let mut cval_raw: Vec<bool> = vec![false; m.value_types.len()];

        // Shared deopt landing block: pure bodies rerun the interpreter from the
        // start (STATUS_DEOPT), created lazily on the first guard.
        let mut deopt: Option<Block> = None;

        // Function-entry block: stash the out pointer + load args, jump into MIR
        // block 0 passing the args as block params.
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        let vmctx_param = fb.block_params(entry)[0];
        if let Some(rt) = &rt {
            fb.def_var(rt.vmctx_var, vmctx_param);
        }
        let args_ptr = fb.block_params(entry)[1];
        let out_ptr = fb.block_params(entry)[2];
        // R1c-sidecar: the 4th entry param (the per-thread `*const LeafSidecar`).
        // Read only in AOT mode; JIT ignores it. The entry block dominates every
        // block, so a base materialized here is valid in any (incl. cold) block.
        let sidecar_param = aot.then(|| fb.block_params(entry)[3]);
        // R1a: base address of the heap-constant reloc vector, materialized once
        // near entry. JIT bakes the Box address as `iconst`; AOT loads it from the
        // sidecar (session-specific). `None` when the body references no heap
        // constants (then nothing loads off it).
        let reloc_base = if reloc_data.is_empty() {
            None
        } else if aot {
            let sc = sidecar_param.expect("AOT sets sidecar_param");
            Some(fb.ins().load(
                ptr_ty,
                MemFlagsData::trusted(),
                sc,
                LeafSidecar::OFF_RELOC_BASE,
            ))
        } else {
            Some(fb.ins().iconst(ptr_ty, reloc_data.as_ptr() as i64))
        };
        // The deopt-buffer bases (iconst or sidecar-load), materialized in the
        // entry block so they dominate the cold precise-deopt blocks.
        let deopt_refs = materialize_deopt_refs(
            &mut fb,
            ptr_ty,
            aot,
            /*has_precise_deopt=*/ precise,
            sidecar_param,
            spill_base_addr,
            meta_pc_addr,
            meta_depth_addr,
            meta_handlers_addr,
        );
        let arg_vals: Vec<BlockArg> = (0..m.arity)
            .map(|i| {
                let v = fb.ins().load(
                    types::I64,
                    MemFlagsData::trusted(),
                    args_ptr,
                    (i * 8) as i32,
                );
                BlockArg::Value(v)
            })
            .collect();
        fb.ins().jump(clif_blocks[0], &arg_vals);

        for (bi, blk) in m.blocks.iter().enumerate() {
            let cb = clif_blocks[bi];
            fb.switch_to_block(cb);
            // Bind this block's params to the CLIF block params.
            let bp = fb.block_params(cb).to_vec();
            for (p, &cv) in blk.params.iter().zip(bp.iter()) {
                cval[p.0 as usize] = Some(cv);
            }

            for inst in &blk.insts {
                let r = inst.result.0 as usize;
                match &inst.op {
                    MirOp::Arg(_) => {
                        // The param already holds the argument (bound above).
                    }
                    MirOp::Const(v) => {
                        // Which non-fixnum consts route through the reloc vector vs
                        // bake: JIT relocs heap objects only (symbols bake — valid
                        // same-session, keeps the JIT byte-identical); AOT also
                        // relocs non-nil/t symbols, whose baked SymId would be
                        // session-specific in a cross-session `.so` (audit #16).
                        let needs_reloc = if aot {
                            const_relocs_for_aot(*v)
                        } else {
                            v.is_heap_object()
                        };
                        if (v.bits() & FIXNUM_CHECK_MASK) == FIXNUM_CHECK_VALUE {
                            // Fixnum constant (incl chars) -> keep raw (untagged integer).
                            cval[r] = Some(
                                fb.ins()
                                    .iconst(types::I64, (v.bits() as i64) >> FIXNUM_SHIFT),
                            );
                            cval_raw[r] = true;
                        } else if !needs_reloc {
                            // Session-stable immediate (nil/t/char/...): no
                            // session-specific bits, so bake the tagged bits directly.
                            cval[r] = Some(fb.ins().iconst(types::I64, v.bits() as i64));
                        } else {
                            // Session-specific const (heap object always; under AOT
                            // also a non-nil/t symbol): load from the per-leaf reloc
                            // vector (R1a) — never bake session-specific bits, so the
                            // code is GC-pointer-free AND cross-session AOT-portable.
                            let idx = reloc_index[&v.bits()];
                            let base = reloc_base.expect("reloc_base set when reloc nonempty");
                            cval[r] = Some(fb.ins().load(
                                types::I64,
                                MemFlagsData::trusted(),
                                base,
                                (idx * 8) as i32,
                            ));
                        }
                    }
                    MirOp::Bin(kind, a, b) => {
                        let d = mir_deopt_block(
                            &mut fb,
                            precise,
                            inst,
                            &cval,
                            &cval_raw,
                            &mut deopt,
                            &mut pending,
                        )?;
                        let av = mir_as_raw(&mut fb, &cval, &cval_raw, *a, d)?;
                        let bv = mir_as_raw(&mut fb, &cval, &cval_raw, *b, d)?;
                        let res = match kind {
                            BinKind::Add => raw_fixnum_addsub(&mut fb, d, false, av, bv),
                            BinKind::Sub => raw_fixnum_addsub(&mut fb, d, true, av, bv),
                            BinKind::Mul => raw_fixnum_mul(&mut fb, d, av, bv),
                            BinKind::Div => raw_fixnum_divrem(&mut fb, d, false, av, bv),
                            BinKind::Rem => raw_fixnum_divrem(&mut fb, d, true, av, bv),
                            BinKind::Max => raw_fixnum_maxmin(&mut fb, false, av, bv),
                            BinKind::Min => raw_fixnum_maxmin(&mut fb, true, av, bv),
                        };
                        cval[r] = Some(res);
                        cval_raw[r] = true;
                    }
                    MirOp::Unary(kind, a) => {
                        let k = match kind {
                            MU::Add1 => UnaryKind::Add1,
                            MU::Sub1 => UnaryKind::Sub1,
                            MU::Negate => UnaryKind::Negate,
                        };
                        let d = mir_deopt_block(
                            &mut fb,
                            precise,
                            inst,
                            &cval,
                            &cval_raw,
                            &mut deopt,
                            &mut pending,
                        )?;
                        let av = mir_as_raw(&mut fb, &cval, &cval_raw, *a, d)?;
                        cval[r] = Some(raw_fixnum_unop(&mut fb, d, k, av));
                        cval_raw[r] = true;
                    }
                    MirOp::Cmp(kind, a, b) => {
                        let cc = match kind {
                            CmpKind::NumEq => IntCC::Equal,
                            CmpKind::Lt => IntCC::SignedLessThan,
                            CmpKind::Gt => IntCC::SignedGreaterThan,
                            CmpKind::Le => IntCC::SignedLessThanOrEqual,
                            CmpKind::Ge => IntCC::SignedGreaterThanOrEqual,
                        };
                        let d = mir_deopt_block(
                            &mut fb,
                            precise,
                            inst,
                            &cval,
                            &cval_raw,
                            &mut deopt,
                            &mut pending,
                        )?;
                        let av = mir_as_raw(&mut fb, &cval, &cval_raw, *a, d)?;
                        let bv = mir_as_raw(&mut fb, &cval, &cval_raw, *b, d)?;
                        let cond = fb.ins().icmp(cc, av, bv);
                        let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
                        let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
                        cval[r] = Some(fb.ins().select(cond, t, nil));
                    }
                    MirOp::Pred(kind, a) => {
                        let k = match kind {
                            MP::Null | MP::Not => PredKind::Null,
                            MP::Consp => PredKind::Consp,
                            MP::Stringp => PredKind::Stringp,
                            MP::Listp => PredKind::Listp,
                            // Symbolp/Integerp/Numberp use shims; deferred.
                            _ => return Err(CompileError::UnsupportedOp("mir-pure-pred")),
                        };
                        let a = mir_as_tagged(&mut fb, &cval, &cval_raw, *a)?;
                        cval[r] = Some(lower_predicate(&mut fb, k, a));
                    }
                    MirOp::CarCdr { cdr, safe, arg } => {
                        // If `arg` is a scalar-replaced (eliminated) cons, forward the
                        // read directly to its car/cdr operand SSA value — no consp
                        // guard, no allocation, no lower_car_cdr. Carry cval_raw so a
                        // raw fixnum stays raw across the elided cons. (Checked BEFORE
                        // mir_deopt_block — a forwarded read never deopts.)
                        if let Some((car_v, cdr_v)) = cons_repl[arg.0 as usize] {
                            let src = if *cdr { cdr_v } else { car_v };
                            cval[r] = cval[src.0 as usize];
                            cval_raw[r] = cval_raw[src.0 as usize];
                        } else {
                            let d = if *safe {
                                None
                            } else {
                                Some(mir_deopt_block(
                                    &mut fb,
                                    precise,
                                    inst,
                                    &cval,
                                    &cval_raw,
                                    &mut deopt,
                                    &mut pending,
                                )?)
                            };
                            let a = mir_as_tagged(&mut fb, &cval, &cval_raw, *arg)?;
                            cval[r] = Some(lower_car_cdr(&mut fb, d, *cdr, *safe, a));
                        }
                    }
                    // A CALL: a GC safepoint + a side effect. Force-tag every value
                    // that survives it (a raw fixnum cannot cross the safepoint —
                    // the GC would trace the untagged i64 as a pointer), root the
                    // live-across-call residual, dispatch the GENERIC shim (no spec
                    // plumbing in the MIR tier), propagate a signal, and on STATUS_OK
                    // push the tagged result. The body's guards are all precise
                    // (`precise == has_call`), so no rerun-from-start re-runs this.
                    MirOp::Opaque { op, args } if matches!(op, Op::Call(_) | Op::Apply(_)) => {
                        let rt = rt
                            .as_ref()
                            .ok_or(CompileError::UnsupportedOp("mir-call-no-rt"))?;
                        let n = match op {
                            Op::Call(n) | Op::Apply(n) => *n as usize,
                            _ => unreachable!("guarded to Call/Apply"),
                        };
                        let is_apply = matches!(op, Op::Apply(_));
                        if args.len() != n + 1 {
                            return Err(CompileError::UnsupportedOp("mir-call-arity"));
                        }
                        // Marshal the n args (args[1..]) tagged into the call buffer.
                        for (i, a) in args[1..].iter().enumerate() {
                            let v = mir_force_tagged(&mut fb, &mut cval, &mut cval_raw, *a)?;
                            fb.ins()
                                .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
                        }
                        let func_val =
                            mir_force_tagged(&mut fb, &mut cval, &mut cval_raw, args[0])?;
                        // Residual = operand-stack values live ACROSS the call (the
                        // pre-op stack below func+args). Root them (force-tagged) so a
                        // GC inside the callee can trace them.
                        let residual_len = inst.pre_stack.len().saturating_sub(n + 1);
                        // Gather the to-root residuals (force-tag ALL so
                        // downstream `cval` state never moves; skip provably-
                        // immediate MIR types when the opt is on).
                        let on = jit_lever1_on();
                        let mut to_root: Vec<ClifValue> = Vec::with_capacity(residual_len);
                        for k in 0..residual_len {
                            let rv = inst.pre_stack[k];
                            let v = mir_force_tagged(&mut fb, &mut cval, &mut cval_raw, rv)?;
                            if on && m.value_type(rv).never_needs_gc_root() {
                                continue;
                            }
                            to_root.push(v);
                        }
                        // CONDITIONAL ROOTING: residuals here are typed
                        // Unknown/Any but empirically resolve to immediates
                        // (fixnum accumulators, symbols) on the hot paths, so
                        // test the tags INLINE and branch around all three
                        // rooting shims (save + push_many + restore) when
                        // nothing is heap. `!(is_fixnum | is_symbol)` is the
                        // exact layout-anchored `is_heap_object` (see the
                        // lever-1 correctness note); the shim's own re-test
                        // keeps any over-approximation harmless. The saved
                        // depth crosses the callee via `gc_saved_slot`, with
                        // -1 marking "nothing rooted".
                        let saved = if to_root.is_empty() {
                            CondRoots::NONE
                        } else {
                            CondRoots {
                                base: Some(emit_root_window_stores(&mut fb, rt, &to_root)),
                            }
                        };
                        let vmctx = fb.use_var(rt.vmctx_var);
                        let args_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0);
                        let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
                        let n_val = fb.ins().iconst(types::I64, n as i64);
                        let shim = if is_apply {
                            rt.refs.apply
                        } else {
                            rt.refs.call
                        };
                        let call = fb
                            .ins()
                            .call(shim, &[vmctx, func_val, args_addr, n_val, out_addr]);
                        let status = fb.inst_results(call)[0];
                        emit_cond_residual_roots_post(&mut fb, rt, saved);
                        // STATUS_OK -> continue; anything else is STATUS_SIGNAL.
                        let se = *signal_exit.get_or_insert_with(|| fb.create_block());
                        let cont = fb.create_block();
                        let ok = fb.ins().icmp_imm_u(IntCC::Equal, status, STATUS_OK);
                        fb.ins().brif(ok, cont, &[], se, &[]);
                        fb.switch_to_block(cont);
                        fb.seal_block(cont);
                        let result =
                            fb.ins()
                                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
                        cval[r] = Some(result);
                        cval_raw[r] = false;
                    }
                    // A non-escaping cons (escape analysis) is ELIDED: emit nothing,
                    // leave cval[r]=None — every use is a CarCdr that forwards to the
                    // operands.
                    MirOp::Cons(..) if cons_repl[r].is_some() => {}
                    // An ESCAPING cons is heap-allocated via the neovm_jit_cons shim —
                    // a GC SAFEPOINT, but NOT an observable side effect (a fresh
                    // unshared object), so it needs NO precise deopt: rerun-from-start
                    // (pure body) re-allocates a fresh cons the caller never saw, and a
                    // call-bearing body spills the allocated cons (a real Value) into
                    // its precise framestate normally. Force-tag car+cdr (no raw fixnum
                    // into the heap pair / across the safepoint; the shim self-roots
                    // them) + gc-root the live-across-allocation residual, like a call.
                    MirOp::Cons(car, cdr) => {
                        let rt = rt
                            .as_ref()
                            .ok_or(CompileError::UnsupportedOp("mir-cons-no-rt"))?;
                        let car_v = mir_force_tagged(&mut fb, &mut cval, &mut cval_raw, *car)?;
                        let cdr_v = mir_force_tagged(&mut fb, &mut cval, &mut cval_raw, *cdr)?;
                        // No residual rooting: the cons shim is pure
                        // allocation and never reaches a GC safe point (see
                        // `neovm_jit_cons`), so nothing live across it can be
                        // collected. Infallible + context-free (no status, no
                        // vmctx) — no STATUS branch / signal exit.
                        let call = fb.ins().call(rt.refs.cons, &[car_v, cdr_v]);
                        let result = fb.inst_results(call)[0];
                        cval[r] = Some(result);
                        cval_raw[r] = false;
                    }
                    // Shim-using ops, deferred: `eq` needs the symbols-with-position
                    // slow-path shim (vmctx) so plain tagged-bits comparison would
                    // diverge when symbols-with-pos-enabled; other `opaque`
                    // (VarRef/builtins/...) not yet ported.
                    MirOp::Eq(..) | MirOp::Opaque { .. } => {
                        return Err(CompileError::UnsupportedOp("mir-pure-shim-op"));
                    }
                }
            }

            // Terminator.
            match &blk.term {
                MirTerm::Return(v) => {
                    let rv = mir_as_tagged(&mut fb, &cval, &cval_raw, *v)?;
                    let out = out_ptr;
                    fb.ins().store(MemFlagsData::trusted(), rv, out, 0);
                    let ok = fb.ins().iconst(types::I64, STATUS_OK);
                    fb.ins().return_(&[ok]);
                }
                MirTerm::Goto { target, args } => {
                    // Cross-block args are tagged (block params are tagged).
                    let mut a: Vec<BlockArg> = Vec::with_capacity(args.len());
                    for v in args {
                        a.push(BlockArg::Value(mir_as_tagged(
                            &mut fb, &cval, &cval_raw, *v,
                        )?));
                    }
                    fb.ins().jump(clif_blocks[target.0 as usize], &a);
                }
                MirTerm::Branch {
                    cond,
                    on_nil,
                    taken,
                    taken_args,
                    fallthrough,
                    fallthrough_args,
                    ..
                } => {
                    let c = mir_as_tagged(&mut fb, &cval, &cval_raw, *cond)?;
                    let is_nil = fb
                        .ins()
                        .icmp_imm_u(IntCC::Equal, c, Value::NIL.bits() as i64);
                    let mut ta: Vec<BlockArg> = Vec::with_capacity(taken_args.len());
                    for v in taken_args {
                        ta.push(BlockArg::Value(mir_as_tagged(
                            &mut fb, &cval, &cval_raw, *v,
                        )?));
                    }
                    let mut fa: Vec<BlockArg> = Vec::with_capacity(fallthrough_args.len());
                    for v in fallthrough_args {
                        fa.push(BlockArg::Value(mir_as_tagged(
                            &mut fb, &cval, &cval_raw, *v,
                        )?));
                    }
                    let tb = clif_blocks[taken.0 as usize];
                    let fbk = clif_blocks[fallthrough.0 as usize];
                    // brif takes the `then` block when the condition is true.
                    if *on_nil {
                        fb.ins().brif(is_nil, tb, &ta, fbk, &fa);
                    } else {
                        fb.ins().brif(is_nil, fbk, &fa, tb, &ta);
                    }
                }
            }
        }

        if let Some(db) = deopt {
            fb.switch_to_block(db);
            let code = fb.ins().iconst(types::I64, STATUS_DEOPT);
            fb.ins().return_(&[code]);
        }

        // Per-site precise-deopt blocks (call-bearing bodies): spill the captured
        // framestate (retagging raw slots in the cold block) + return STATUS_DEOPT_AT.
        // No-op for pure bodies (pending is empty).
        emit_pending_deopts(&mut fb, deopt_refs, &mut pending);

        // Signal propagation from a call: return STATUS_SIGNAL (the Flow is stashed
        // in the Context by the shim). No binds/handlers to unwind — build_mir bails
        // on those, so a MIR leaf never registers any.
        if let Some(se) = signal_exit {
            fb.switch_to_block(se);
            fb.seal_block(se);
            let code = fb.ins().iconst(types::I64, STATUS_SIGNAL);
            fb.ins().return_(&[code]);
        }

        fb.seal_all_blocks();
        fb.finalize(frontend_config);
    }
    LAST_IR_STATS.with(|c| {
        let (_, _, sites, slots) = c.get();
        c.set((
            func.dfg.num_insts() as u32,
            func.layout.blocks().count() as u32,
            sites,
            slots,
        ));
    });

    let fid = module
        .declare_function(entry_name, entry_linkage, &sig)
        .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
    let mut ctx = module.make_context();
    ctx.func = func;
    module
        .define_function(fid, &mut ctx)
        .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
    module.clear_context(&mut ctx);

    Ok(fid)
}

/// Per-function runtime-call machinery: shim references plus the vmctx variable
/// and the scratch stack slots `Call` spills through. Present only when the body
/// re-enters the runtime (`Cons` / `Call`).
struct RtCtx {
    refs: RtRefs,
    /// The `*mut Context` function parameter, carried in an SSA variable so any
    /// block can read it.
    vmctx_var: Variable,
    /// Pointer type of the target (for `stack_addr`).
    ptr_ty: Type,
    /// Spill buffer for outgoing call arguments (max `Call` nargs in the body).
    call_args_slot: StackSlot,
    /// 8-byte result slot the call shim writes through.
    call_result_slot: StackSlot,
    /// Lever 2: gather buffer for BATCHED residual GC rooting. Sized to the body's
    /// max operand-stack depth (an upper bound on any site's residual count).
    /// Reused per residual-rooting site — `neovm_jit_gc_push_many` reads it
    /// synchronously, so the next site's stores may overwrite it.
    residual_buf_slot: StackSlot,
    /// Conditional-rooting saved scratch depth for the current call site:
    /// `-1` when the site's runtime tag tests found no heap residual (all
    /// three rooting shims skipped), else the `gc_save` result the post-call
    /// restore consumes. Written and read within one site — reusable.
    gc_saved_slot: StackSlot,
}

/// Callable references to every runtime shim, declared into one function.
struct RtRefs {
    rootwin_grow: FuncRef,
    gc_save: FuncRef,
    gc_push: FuncRef,
    gc_push_many: FuncRef,
    gc_restore: FuncRef,
    cons: FuncRef,
    call: FuncRef,
    apply: FuncRef,
    eq_slow: FuncRef,
    symbolp_slow: FuncRef,
    varref: FuncRef,
    varset: FuncRef,
    varbind: FuncRef,
    unbind: FuncRef,
    backedge: FuncRef,
    save_current_buffer: FuncRef,
    save_excursion: FuncRef,
    save_restriction: FuncRef,
    unwind_protect: FuncRef,
    throw_flow: FuncRef,
    integerp_slow: FuncRef,
    numberp_slow: FuncRef,
    builtin1: FuncRef,
    builtin2: FuncRef,
    builtin3: FuncRef,
    push_cc: FuncRef,
    push_cc_raw: FuncRef,
    push_catch: FuncRef,
    pop_handler: FuncRef,
    match_handler: FuncRef,
    switch_lookup: FuncRef,
    switch_stale: FuncRef,
    list: FuncRef,
    builtin_slice: FuncRef,
    named_builtin: FuncRef,
    save_window_excursion: FuncRef,
    call_spec: FuncRef,
    /// The three subr-speculation shims (Gap 1), declared ONLY when the body
    /// actually has subr-kind spec sites (`declare_rt_refs`' `subr_spec`
    /// flag). `None` otherwise — in particular for EVERY AOT build
    /// (`build_baseline_leaf_object` compiles with an empty site map), so an
    /// AOT object can never acquire an import of these JIT-only shims (they
    /// are deliberately NOT in `shim_names.rs`, and
    /// `assert_aot_imports_exported` refuses foreign imports at emit time).
    call_subr_spec: Option<FuncRef>,
    pred_spec: Option<FuncRef>,
    eq_incl_props_spec: Option<FuncRef>,
    /// `neovm_jit_arith_spec` (logand/logior/logxor intrinsic). Declared under the
    /// same `subr_spec` flag as the round-1 subr shims (see `is_round1_subr`).
    arith_spec: Option<FuncRef>,
    /// The R2 CallBuiltinSym intrinsic shims — Tier-B dispatch-skip
    /// ([`neovm_jit_cbsym_spec`]) + Tier-A GC-free read
    /// ([`neovm_jit_cbsym_read`]), declared ONLY when the body has a CBSym-kind
    /// spec site (`declare_rt_refs`' `cbsym_spec` flag). UNLIKE the round-1 subr
    /// shims (still `Some(obarray)`-gated), CBSym classification is obarray-free,
    /// so these ARE declared for AOT baseline leaves too (increment A) — both are
    /// exported (`shim_names.rs`) and bind against the host at `dlopen`. `None`
    /// when the body has no CBSym-kind site.
    cbsym_spec: Option<FuncRef>,
    cbsym_read: Option<FuncRef>,
}

/// Declare the runtime-shim imports into `module`/`func` and return the callable
/// refs. The matching addresses are registered on the `JITBuilder` in
/// [`lower_leaf`] via `builder.symbol(...)` (the JIT seam); under AOT the same
/// `Linkage::Import` declarations resolve via the dynamic loader instead.
///
/// Generic over the module type (`M: Module`) so it serves both the `JITModule`
/// JIT path and the future `ObjectModule` AOT path with no change — it only
/// calls `Module::declare_function`, a trait method available on both.
///
/// `subr_spec`: declare the three round-1 subr-speculation shims (Gap 1) — still
/// JIT-only (their `find_spec_sites` pass requires `Some(obarray)`, i.e. never
/// AOT — increment B), so those names (absent from `shim_names.rs`) are never
/// DECLARED into an `ObjectModule`, independent of whether unreferenced
/// declarations would reach the emitted object.
/// `cbsym_spec`: declare the R2 CallBuiltinSym intrinsic shims (Tier-A read +
/// Tier-B dispatch-skip). CBSym classification is obarray-free, so as of increment
/// A this flag is TRUE for AOT baseline leaves too — the shims ARE in
/// `shim_names.rs` (exported + salted) and resolve at `dlopen`.
/// Both flags are set by `build_leaf_fn` from the body's actual spec sites.
fn declare_rt_refs<M: Module>(
    module: &mut M,
    func: &mut Function,
    call_conv: cranelift_codegen::isa::CallConv,
    ptr_ty: Type,
    subr_spec: bool,
    cbsym_spec: bool,
) -> Result<RtRefs, CompileError> {
    let i64t = types::I64;
    let mut sig_ret = Signature::new(call_conv); // () -> i64
    sig_ret.returns.push(AbiParam::new(i64t));
    let mut sig_arg = Signature::new(call_conv); // (i64) -> ()
    sig_arg.params.push(AbiParam::new(i64t));
    let mut sig_push_many = Signature::new(call_conv); // (ptr, i64) -> ()  (lever 2 batch)
    sig_push_many.params.push(AbiParam::new(ptr_ty));
    sig_push_many.params.push(AbiParam::new(i64t));
    let mut sig_cons = Signature::new(call_conv); // (i64, i64) -> i64
    sig_cons.params.push(AbiParam::new(i64t));
    sig_cons.params.push(AbiParam::new(i64t));
    sig_cons.returns.push(AbiParam::new(i64t));
    // (vmctx, func_bits, args_ptr, nargs, out_ptr) -> status
    let mut sig_call = Signature::new(call_conv);
    sig_call.params.push(AbiParam::new(ptr_ty));
    sig_call.params.push(AbiParam::new(i64t));
    sig_call.params.push(AbiParam::new(ptr_ty));
    sig_call.params.push(AbiParam::new(i64t));
    sig_call.params.push(AbiParam::new(ptr_ty));
    sig_call.returns.push(AbiParam::new(i64t));
    // (vmctx, a, b) -> t/nil bits
    let mut sig_eq = Signature::new(call_conv);
    sig_eq.params.push(AbiParam::new(ptr_ty));
    sig_eq.params.push(AbiParam::new(i64t));
    sig_eq.params.push(AbiParam::new(i64t));
    sig_eq.returns.push(AbiParam::new(i64t));
    // (vmctx, v) -> t/nil bits
    let mut sig_symp = Signature::new(call_conv);
    sig_symp.params.push(AbiParam::new(ptr_ty));
    sig_symp.params.push(AbiParam::new(i64t));
    sig_symp.returns.push(AbiParam::new(i64t));

    let declare = |module: &mut M, name: &str, sig: &Signature| {
        module
            .declare_function(name, Linkage::Import, sig)
            .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))
    };

    let save_id = declare(module, "neovm_jit_gc_save", &sig_ret)?;
    let push_id = declare(module, "neovm_jit_gc_push", &sig_arg)?;
    let push_many_id = declare(module, "neovm_jit_gc_push_many", &sig_push_many)?;
    let restore_id = declare(module, "neovm_jit_gc_restore", &sig_arg)?;
    // (vmctx, need) -> (): same param shape as push_many.
    let rootwin_grow_id = declare(module, "neovm_jit_rootwin_grow", &sig_push_many)?;
    let cons_id = declare(module, "neovm_jit_cons", &sig_cons)?;
    let call_id = declare(module, "neovm_jit_call", &sig_call)?;
    let apply_id = declare(module, "neovm_jit_apply", &sig_call)?;
    let eq_id = declare(module, "neovm_jit_eq_slow", &sig_eq)?;
    let symp_id = declare(module, "neovm_jit_symbolp_slow", &sig_symp)?;
    // (vmctx, sym_id, out_ptr) -> status
    let mut sig_varref = Signature::new(call_conv);
    sig_varref.params.push(AbiParam::new(ptr_ty));
    sig_varref.params.push(AbiParam::new(i64t));
    sig_varref.params.push(AbiParam::new(ptr_ty));
    sig_varref.returns.push(AbiParam::new(i64t));
    // (vmctx, sym_id, val) -> status
    let mut sig_varset = Signature::new(call_conv);
    sig_varset.params.push(AbiParam::new(ptr_ty));
    sig_varset.params.push(AbiParam::new(i64t));
    sig_varset.params.push(AbiParam::new(i64t));
    sig_varset.returns.push(AbiParam::new(i64t));
    let varref_id = declare(module, "neovm_jit_varref", &sig_varref)?;
    let varset_id = declare(module, "neovm_jit_varset", &sig_varset)?;
    // (vmctx, sym_id, val) -> status
    let mut sig_varbind = Signature::new(call_conv);
    sig_varbind.params.push(AbiParam::new(ptr_ty));
    sig_varbind.params.push(AbiParam::new(i64t));
    sig_varbind.params.push(AbiParam::new(i64t));
    sig_varbind.returns.push(AbiParam::new(i64t));
    // (vmctx, n) -> status
    let mut sig_unbind = Signature::new(call_conv);
    sig_unbind.params.push(AbiParam::new(ptr_ty));
    sig_unbind.params.push(AbiParam::new(i64t));
    sig_unbind.returns.push(AbiParam::new(i64t));
    let varbind_id = declare(module, "neovm_jit_varbind", &sig_varbind)?;
    let unbind_id = declare(module, "neovm_jit_unbind", &sig_unbind)?;
    // (vmctx) -> status
    let mut sig_backedge = Signature::new(call_conv);
    sig_backedge.params.push(AbiParam::new(ptr_ty));
    sig_backedge.returns.push(AbiParam::new(i64t));
    let backedge_id = declare(module, "neovm_jit_backedge", &sig_backedge)?;
    // (vmctx) -> ()  — the infallible Save* records.
    let mut sig_save = Signature::new(call_conv);
    sig_save.params.push(AbiParam::new(ptr_ty));
    let scb_id = declare(module, "neovm_jit_save_current_buffer", &sig_save)?;
    let sexc_id = declare(module, "neovm_jit_save_excursion", &sig_save)?;
    let sres_id = declare(module, "neovm_jit_save_restriction", &sig_save)?;
    // (vmctx, forms) -> ()  — unwind-protect record (infallible). Keep this
    // distinct from the now-fallible unbind ABI above.
    let mut sig_unwind_protect = Signature::new(call_conv);
    sig_unwind_protect.params.push(AbiParam::new(ptr_ty));
    sig_unwind_protect.params.push(AbiParam::new(i64t));
    let up_id = declare(module, "neovm_jit_unwind_protect", &sig_unwind_protect)?;
    // (tag, value) -> ()  — context-free Flow stash.
    let mut sig_throw = Signature::new(call_conv);
    sig_throw.params.push(AbiParam::new(i64t));
    sig_throw.params.push(AbiParam::new(i64t));
    let throw_id = declare(module, "neovm_jit_throw", &sig_throw)?;
    // (v) -> t/nil bits  — context-free predicates.
    let mut sig_pred1 = Signature::new(call_conv);
    sig_pred1.params.push(AbiParam::new(i64t));
    sig_pred1.returns.push(AbiParam::new(i64t));
    let intp_id = declare(module, "neovm_jit_integerp_slow", &sig_pred1)?;
    let nump_id = declare(module, "neovm_jit_numberp_slow", &sig_pred1)?;
    // (vmctx, idx, a[, b[, c]], out_ptr) -> status — generic direct builtins.
    let mut sig_b1 = Signature::new(call_conv);
    sig_b1.params.push(AbiParam::new(ptr_ty));
    sig_b1.params.push(AbiParam::new(i64t));
    sig_b1.params.push(AbiParam::new(i64t));
    sig_b1.params.push(AbiParam::new(ptr_ty));
    sig_b1.returns.push(AbiParam::new(i64t));
    let mut sig_b2 = sig_b1.clone();
    sig_b2.params.insert(3, AbiParam::new(i64t));
    let mut sig_b3 = sig_b2.clone();
    sig_b3.params.insert(4, AbiParam::new(i64t));
    let b1_id = declare(module, "neovm_jit_builtin1", &sig_b1)?;
    let b2_id = declare(module, "neovm_jit_builtin2", &sig_b2)?;
    let b3_id = declare(module, "neovm_jit_builtin3", &sig_b3)?;
    // (vmctx, target, stack_len) -> ()  — condition-case push (infallible).
    let mut sig_pcc = Signature::new(call_conv);
    sig_pcc.params.push(AbiParam::new(ptr_ty));
    sig_pcc.params.push(AbiParam::new(i64t));
    sig_pcc.params.push(AbiParam::new(i64t));
    // (vmctx, target, stack_len, conditions/tag) -> ()
    let mut sig_pcc_raw = sig_pcc.clone();
    sig_pcc_raw.params.push(AbiParam::new(i64t));
    let pcc_id = declare(module, "neovm_jit_push_cc", &sig_pcc)?;
    let pcc_raw_id = declare(module, "neovm_jit_push_cc_raw", &sig_pcc_raw)?;
    let pcatch_id = declare(module, "neovm_jit_push_catch", &sig_pcc_raw)?;
    let pop_handler_id = declare(module, "neovm_jit_pop_handler", &sig_save)?;
    // (vmctx, ours, out_ptr) -> matched ordinal or -1.
    let match_id = declare(module, "neovm_jit_match_handler", &sig_varref)?;
    // (vmctx, dispatch, table) -> raw target addr / miss / stale.
    let switch_id = declare(module, "neovm_jit_switch", &sig_eq)?;
    // () -> ()  — stash the stale-table signal.
    let sig_void = Signature::new(call_conv);
    let switch_stale_id = declare(module, "neovm_jit_switch_stale", &sig_void)?;
    // (args_ptr, nargs) -> list bits  — infallible n-ary list builder.
    let mut sig_list = Signature::new(call_conv);
    sig_list.params.push(AbiParam::new(ptr_ty));
    sig_list.params.push(AbiParam::new(i64t));
    sig_list.returns.push(AbiParam::new(i64t));
    let list_id = declare(module, "neovm_jit_list", &sig_list)?;
    // (idx, args_ptr, nargs, out_ptr) -> status  — slice-shaped builtins.
    let mut sig_slice = Signature::new(call_conv);
    sig_slice.params.push(AbiParam::new(i64t));
    sig_slice.params.push(AbiParam::new(ptr_ty));
    sig_slice.params.push(AbiParam::new(i64t));
    sig_slice.params.push(AbiParam::new(ptr_ty));
    sig_slice.returns.push(AbiParam::new(i64t));
    let slice_id = declare(module, "neovm_jit_builtin_slice", &sig_slice)?;
    // (vmctx, variant, sym, args_ptr, nargs, out_ptr) -> status.
    let mut sig_named = Signature::new(call_conv);
    sig_named.params.push(AbiParam::new(ptr_ty));
    sig_named.params.push(AbiParam::new(i64t));
    sig_named.params.push(AbiParam::new(i64t));
    sig_named.params.push(AbiParam::new(ptr_ty));
    sig_named.params.push(AbiParam::new(i64t));
    sig_named.params.push(AbiParam::new(ptr_ty));
    sig_named.returns.push(AbiParam::new(i64t));
    let named_id = declare(module, "neovm_jit_named_builtin", &sig_named)?;
    // (vmctx, body, out_ptr) -> status.
    let swe_id = declare(module, "neovm_jit_save_window_excursion", &sig_varref)?;
    // (vmctx, sym, expected, slot_ptr, args_ptr, nargs, out_ptr) -> status.
    let mut sig_spec = Signature::new(call_conv);
    sig_spec.params.push(AbiParam::new(ptr_ty));
    sig_spec.params.push(AbiParam::new(i64t));
    sig_spec.params.push(AbiParam::new(i64t));
    sig_spec.params.push(AbiParam::new(i64t));
    sig_spec.params.push(AbiParam::new(ptr_ty));
    sig_spec.params.push(AbiParam::new(i64t));
    sig_spec.params.push(AbiParam::new(ptr_ty));
    sig_spec.returns.push(AbiParam::new(i64t));
    let call_spec_id = declare(module, "neovm_jit_call_spec", &sig_spec)?;
    // Gap 1: the subr-speculation shims, JIT-only (see the `subr_spec` doc).
    // call_subr_spec shares sig_spec's shape; pred/eq share one 7-param shape:
    // (vmctx, k1, k2, k3, k4, k5, out_ptr) -> status
    //   pred: (vmctx, kind, sym, expected, slot_ptr, a, out_ptr)
    //   eq:   (vmctx, sym, expected, slot_ptr, a, b, out_ptr)
    // arith adds one word for the second arg (kind + 2 args):
    //   arith: (vmctx, kind, sym, expected, slot_ptr, a, b, out_ptr)
    let subr_spec_refs = if subr_spec {
        let mut sig_pred = Signature::new(call_conv);
        sig_pred.params.push(AbiParam::new(ptr_ty));
        for _ in 0..5 {
            sig_pred.params.push(AbiParam::new(i64t));
        }
        sig_pred.params.push(AbiParam::new(ptr_ty));
        sig_pred.returns.push(AbiParam::new(i64t));
        let mut sig_arith = Signature::new(call_conv);
        sig_arith.params.push(AbiParam::new(ptr_ty)); // vmctx
        for _ in 0..6 {
            // kind, sym, expected, slot_ptr, a, b
            sig_arith.params.push(AbiParam::new(i64t));
        }
        sig_arith.params.push(AbiParam::new(ptr_ty)); // out
        sig_arith.returns.push(AbiParam::new(i64t));
        let subr_id = declare(module, "neovm_jit_call_subr_spec", &sig_spec)?;
        let pred_id = declare(module, "neovm_jit_pred_spec", &sig_pred)?;
        let eq_id = declare(module, "neovm_jit_eq_incl_props_spec", &sig_pred)?;
        let arith_id = declare(module, "neovm_jit_arith_spec", &sig_arith)?;
        Some((subr_id, pred_id, eq_id, arith_id))
    } else {
        None
    };
    // R2 CallBuiltinSym intrinsic shims (JIT-only, see doc). Tier-B dispatch-skip
    // shares `sig_call`'s shape (vmctx, sym, args_ptr, nargs, out_ptr) -> status;
    // Tier-A read adds a leading `which` discriminant
    // (vmctx, which, sym, args_ptr, nargs, out_ptr) -> status.
    let (cbsym_spec_id, cbsym_read_id) = if cbsym_spec {
        let mut sig_read = Signature::new(call_conv);
        sig_read.params.push(AbiParam::new(ptr_ty)); // vmctx
        sig_read.params.push(AbiParam::new(i64t)); // which
        sig_read.params.push(AbiParam::new(i64t)); // sym
        sig_read.params.push(AbiParam::new(ptr_ty)); // args_ptr
        sig_read.params.push(AbiParam::new(i64t)); // nargs
        sig_read.params.push(AbiParam::new(ptr_ty)); // out
        sig_read.returns.push(AbiParam::new(i64t));
        (
            Some(declare(module, "neovm_jit_cbsym_spec", &sig_call)?),
            Some(declare(module, "neovm_jit_cbsym_read", &sig_read)?),
        )
    } else {
        (None, None)
    };

    Ok(RtRefs {
        rootwin_grow: module.declare_func_in_func(rootwin_grow_id, func),
        gc_save: module.declare_func_in_func(save_id, func),
        gc_push: module.declare_func_in_func(push_id, func),
        gc_push_many: module.declare_func_in_func(push_many_id, func),
        gc_restore: module.declare_func_in_func(restore_id, func),
        cons: module.declare_func_in_func(cons_id, func),
        call: module.declare_func_in_func(call_id, func),
        apply: module.declare_func_in_func(apply_id, func),
        eq_slow: module.declare_func_in_func(eq_id, func),
        symbolp_slow: module.declare_func_in_func(symp_id, func),
        varref: module.declare_func_in_func(varref_id, func),
        varset: module.declare_func_in_func(varset_id, func),
        varbind: module.declare_func_in_func(varbind_id, func),
        unbind: module.declare_func_in_func(unbind_id, func),
        backedge: module.declare_func_in_func(backedge_id, func),
        save_current_buffer: module.declare_func_in_func(scb_id, func),
        save_excursion: module.declare_func_in_func(sexc_id, func),
        save_restriction: module.declare_func_in_func(sres_id, func),
        unwind_protect: module.declare_func_in_func(up_id, func),
        throw_flow: module.declare_func_in_func(throw_id, func),
        integerp_slow: module.declare_func_in_func(intp_id, func),
        numberp_slow: module.declare_func_in_func(nump_id, func),
        builtin1: module.declare_func_in_func(b1_id, func),
        builtin2: module.declare_func_in_func(b2_id, func),
        builtin3: module.declare_func_in_func(b3_id, func),
        push_cc: module.declare_func_in_func(pcc_id, func),
        push_cc_raw: module.declare_func_in_func(pcc_raw_id, func),
        push_catch: module.declare_func_in_func(pcatch_id, func),
        pop_handler: module.declare_func_in_func(pop_handler_id, func),
        match_handler: module.declare_func_in_func(match_id, func),
        switch_lookup: module.declare_func_in_func(switch_id, func),
        switch_stale: module.declare_func_in_func(switch_stale_id, func),
        list: module.declare_func_in_func(list_id, func),
        builtin_slice: module.declare_func_in_func(slice_id, func),
        named_builtin: module.declare_func_in_func(named_id, func),
        save_window_excursion: module.declare_func_in_func(swe_id, func),
        call_spec: module.declare_func_in_func(call_spec_id, func),
        call_subr_spec: subr_spec_refs.map(|(id, _, _, _)| module.declare_func_in_func(id, func)),
        pred_spec: subr_spec_refs.map(|(_, id, _, _)| module.declare_func_in_func(id, func)),
        eq_incl_props_spec: subr_spec_refs
            .map(|(_, _, id, _)| module.declare_func_in_func(id, func)),
        arith_spec: subr_spec_refs.map(|(_, _, _, id)| module.declare_func_in_func(id, func)),
        cbsym_spec: cbsym_spec_id.map(|id| module.declare_func_in_func(id, func)),
        cbsym_read: cbsym_read_id.map(|id| module.declare_func_in_func(id, func)),
    })
}

/// The per-leaf cells a precise-deopt exit writes through before returning
/// [`STATUS_DEOPT_AT`]: the failing op's bytecode index, the live operand
/// stack depth (the values themselves go to the spill buffer), and the number
/// of condition frames this frame had registered at that point. `Cell` makes
/// the native interior writes legal; the mutator is single-threaded and the
/// values are consumed immediately after the native call returns.
pub(crate) struct DeoptCells {
    pub(crate) pc: core::cell::Cell<i64>,
    pub(crate) depth: core::cell::Cell<i64>,
    pub(crate) handlers: core::cell::Cell<i64>,
}

/// A precise-deopt exit block queued at a guard-emitting op: created (and
/// targeted by that op's guards) during lowering, filled after the bytecode
/// block terminates. Captures the op's index and the operand stack snapshot
/// from BEFORE the op popped its operands — the interpreter reruns the
/// failing op itself.
struct PendingDeopt {
    block: Block,
    pc: usize,
    handlers_len: usize,
    stack: Vec<ClifValue>,
    /// Per-slot raw mask snapshot (cross-op unboxing): `true` slots hold an
    /// untagged i64 and must be retagged in the cold deopt block before the
    /// framestate spill, since `run_resumed_frame` reads them back as tagged
    /// `Value`s.
    stack_raw: Vec<bool>,
}

/// Queue (and return) the precise-deopt block for the guard-emitting op at
/// bytecode index `pc`, capturing the pre-op operand stack + its raw mask.
fn deopt_site(
    fb: &mut FunctionBuilder,
    pc: usize,
    handlers_len: usize,
    stack: &[ClifValue],
    stack_raw: &[bool],
    pending: &mut Vec<PendingDeopt>,
) -> Block {
    let block = fb.create_block();
    pending.push(PendingDeopt {
        block,
        pc,
        handlers_len,
        stack: stack.to_vec(),
        stack_raw: stack_raw.to_vec(),
    });
    block
}

/// How the precise-deopt blocks reach the leaf's deopt cells + spill buffer.
///
/// JIT (`Baked`): the four base addresses are stable Box pointers baked as
/// `iconst` LAZILY inside each cold deopt block (zero hot-path cost — the
/// pre-sidecar behavior). AOT (`Sidecar`): the bases are session-specific, so
/// they are LOADED from the per-thread `LeafSidecar` ONCE in the entry block
/// (which dominates the cold blocks) and shared as CLIF values. Splitting the
/// two keeps the JIT's CLIF unchanged (audit: hoisting the iconsts to the entry
/// block was a minor hot-path regression for JIT leaves with deopt sites).
#[derive(Clone, Copy)]
enum DeoptRefs {
    /// JIT: raw Box addresses, iconst'd lazily in each cold deopt block.
    Baked {
        spill_base: i64,
        meta_pc: i64,
        meta_depth: i64,
        meta_handlers: i64,
    },
    /// AOT: entry-block CLIF values loaded from the sidecar (dominate the cold
    /// blocks, so reused directly).
    Sidecar {
        spill_base: ClifValue,
        meta_pc: ClifValue,
        meta_depth: ClifValue,
        meta_handlers: ClifValue,
    },
}

/// Fill the precise-deopt blocks queued within one bytecode block: spill the
/// captured live stack, record pc/depth/handler-count, and return
/// [`STATUS_DEOPT_AT`]. For `Baked` (JIT) the base addresses are iconst'd HERE in
/// the cold block (off the hot path); for `Sidecar` (AOT) they are the
/// entry-block loaded values.
thread_local! {
    /// IR-size facts of the most recent baseline compile on this thread —
    /// `(clif insts, blocks, deopt sites, deopt snapshot slots)` — read by
    /// `stats::record_compile` for its per-compile trace line. Diagnostic only.
    pub(super) static LAST_IR_STATS: core::cell::Cell<(u32, u32, u32, u32)> =
        const { core::cell::Cell::new((0, 0, 0, 0)) };
}

fn emit_pending_deopts(fb: &mut FunctionBuilder, refs: DeoptRefs, pending: &mut Vec<PendingDeopt>) {
    LAST_IR_STATS.with(|c| {
        let (i, b, sites, slots) = c.get();
        let add_slots: usize = pending.iter().map(|pd| pd.stack.len()).sum();
        c.set((
            i,
            b,
            sites.saturating_add(pending.len() as u32),
            slots.saturating_add(add_slots as u32),
        ));
    });
    for pd in pending.drain(..) {
        fb.switch_to_block(pd.block);
        fb.seal_block(pd.block);
        // Materialize the four bases. For Baked, the iconsts live in THIS cold
        // block (the original JIT placement); for Sidecar they are entry values.
        let (spill_base, meta_pc, meta_depth, meta_handlers) = match refs {
            DeoptRefs::Baked {
                spill_base,
                meta_pc,
                meta_depth,
                meta_handlers,
            } => (
                fb.ins().iconst(types::I64, spill_base),
                fb.ins().iconst(types::I64, meta_pc),
                fb.ins().iconst(types::I64, meta_depth),
                fb.ins().iconst(types::I64, meta_handlers),
            ),
            DeoptRefs::Sidecar {
                spill_base,
                meta_pc,
                meta_depth,
                meta_handlers,
            } => (spill_base, meta_pc, meta_depth, meta_handlers),
        };
        for (j, &v) in pd.stack.iter().enumerate() {
            // Retag raw fixnum slots in the COLD deopt block (zero hot-path cost):
            // the framestate is read back as tagged Values by run_resumed_frame.
            let tagged = if pd.stack_raw[j] {
                retag_fixnum(fb, v)
            } else {
                v
            };
            fb.ins()
                .store(MemFlagsData::trusted(), tagged, spill_base, (j * 8) as i32);
        }
        let pc_v = fb.ins().iconst(types::I64, pd.pc as i64);
        fb.ins().store(MemFlagsData::trusted(), pc_v, meta_pc, 0);
        let depth_v = fb.ins().iconst(types::I64, pd.stack.len() as i64);
        fb.ins()
            .store(MemFlagsData::trusted(), depth_v, meta_depth, 0);
        let h_v = fb.ins().iconst(types::I64, pd.handlers_len as i64);
        fb.ins()
            .store(MemFlagsData::trusted(), h_v, meta_handlers, 0);
        let code = fb.ins().iconst(types::I64, STATUS_DEOPT_AT);
        fb.ins().return_(&[code]);
    }
}

/// Build the [`DeoptRefs`] for this leaf.
///
/// JIT (`aot=false`) → `DeoptRefs::Baked` with the raw Box addresses: NOTHING is
/// emitted in the entry block; `emit_pending_deopts` iconst's them lazily inside
/// each cold deopt block (the original placement — no hot-path cost).
///
/// AOT (`aot=true`) → `DeoptRefs::Sidecar` with the bases LOADED from the
/// per-thread `LeafSidecar` in the ENTRY block (so they dominate the cold blocks
/// and are shared). MUST be called with `fb` in the entry block. Gated on
/// `has_precise_deopt`: a body with no precise-deopt site never reaches
/// `emit_pending_deopts`, and the `sidecar` may even be null (the raw-entry
/// pure-leaf path), so emit nothing/zero placeholders and dereference no sidecar.
#[allow(clippy::too_many_arguments)]
fn materialize_deopt_refs(
    fb: &mut FunctionBuilder,
    ptr_ty: Type,
    aot: bool,
    has_precise_deopt: bool,
    sidecar: Option<ClifValue>,
    spill_base_addr: i64,
    meta_pc_addr: i64,
    meta_depth_addr: i64,
    meta_handlers_addr: i64,
) -> DeoptRefs {
    if !aot {
        // JIT: defer the address iconsts to the cold deopt blocks. No entry-block
        // codegen here at all — keeps the hot path byte-identical to pre-sidecar.
        return DeoptRefs::Baked {
            spill_base: spill_base_addr,
            meta_pc: meta_pc_addr,
            meta_depth: meta_depth_addr,
            meta_handlers: meta_handlers_addr,
        };
    }
    if has_precise_deopt {
        let sc = sidecar.expect("AOT precise-deopt lowering requires the sidecar param");
        let load = |fb: &mut FunctionBuilder, off: i32| {
            fb.ins().load(ptr_ty, MemFlagsData::trusted(), sc, off)
        };
        DeoptRefs::Sidecar {
            spill_base: load(fb, LeafSidecar::OFF_SPILL_BASE),
            meta_pc: load(fb, LeafSidecar::OFF_META_PC),
            meta_depth: load(fb, LeafSidecar::OFF_META_DEPTH),
            meta_handlers: load(fb, LeafSidecar::OFF_META_HANDLERS),
        }
    } else {
        // AOT, no precise deopt: bases unused (pending is empty). Zero
        // placeholders, never a sidecar deref (sidecar may be null here).
        let z = fb.ins().iconst(ptr_ty, 0);
        DeoptRefs::Sidecar {
            spill_base: z,
            meta_pc: z,
            meta_depth: z,
            meta_handlers: z,
        }
    }
}

/// A handler-dispatch block queued at a `STATUS_SIGNAL` site inside a
/// protected extent: created (and branched to) at the site, filled after the
/// current bytecode block terminates by [`emit_pending_dispatches`]. Carries
/// the static handler list active at the site and the live operand-stack
/// snapshot (the site's SSA values dominate the dispatch block — it is their
/// only successor on the signal edge).
struct PendingDispatch {
    block: Block,
    handlers: Vec<HandlerStatic>,
    stack: Vec<ClifValue>,
}

/// Where a `STATUS_SIGNAL` site should branch: with no active handlers, the
/// shared signal-exit block (today's behavior); inside a protected extent, a
/// per-site dispatch block that will call the match shim.
fn signal_target_for_site(
    fb: &mut FunctionBuilder,
    signal_exit: &mut Option<Block>,
    handlers: &[HandlerStatic],
    pending: &mut Vec<PendingDispatch>,
    stack: &[ClifValue],
) -> Block {
    if handlers.is_empty() {
        return *signal_exit.get_or_insert_with(|| fb.create_block());
    }
    let block = fb.create_block();
    pending.push(PendingDispatch {
        block,
        handlers: handlers.to_vec(),
        stack: stack.to_vec(),
    });
    block
}

/// Fill the dispatch blocks queued by [`signal_target_for_site`] within one
/// bytecode block (called after its terminator, when the builder can switch
/// blocks). Each dispatch: root the live operand stack (the match shim can run
/// lisp — unwind-protect cleanups, handler-bind handlers, signal hooks — and
/// GC), call the match shim, and map the returned ordinal (`m` misses from the
/// innermost handler; -1 = propagate) onto the statically known handler
/// targets: re-materialize the handler's entry stack (the current model values
/// below its push depth + the error value the shim wrote through the result
/// slot) and jump to its block.
fn emit_pending_dispatches(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    signal_exit: &mut Option<Block>,
    vars: &[Variable],
    block_for: &HashMap<usize, Block>,
    pending: &mut Vec<PendingDispatch>,
) -> Result<(), CompileError> {
    for pd in pending.drain(..) {
        fb.switch_to_block(pd.block);
        fb.seal_block(pd.block);
        let saved = if pd.stack.is_empty() {
            CondRoots::NONE
        } else {
            emit_cond_residual_roots_pre(fb, rt, &pd.stack)
        };
        let vmctx = fb.use_var(rt.vmctx_var);
        let ours = fb.ins().iconst(types::I64, pd.handlers.len() as i64);
        let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
        let call = fb
            .ins()
            .call(rt.refs.match_handler, &[vmctx, ours, out_addr]);
        let idx = fb.inst_results(call)[0];
        emit_cond_residual_roots_post(fb, rt, saved);
        // Compare chain over the (small) static handler list: shim ordinal
        // m counts misses from the top, so m maps to handlers[len-1-m].
        let k = pd.handlers.len();
        for m in 0..k {
            let (target, push_depth) = pd.handlers[k - 1 - m];
            if push_depth > pd.stack.len() {
                // The byte-compiler keeps the operand stack at or above the
                // protected base inside the extent; anything else is exotic —
                // bail to the interpreter.
                return Err(CompileError::UnsupportedOp("handler-depth"));
            }
            let hit = fb.create_block();
            let next = fb.create_block();
            let is_m = fb.ins().icmp_imm_u(IntCC::Equal, idx, m as i64);
            fb.ins().brif(is_m, hit, &[], next, &[]);
            fb.switch_to_block(hit);
            fb.seal_block(hit);
            for (j, &v) in pd.stack.iter().take(push_depth).enumerate() {
                fb.def_var(vars[j], v);
            }
            let err = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            fb.def_var(vars[push_depth], err);
            fb.ins().jump(block_for[&target], &[]);
            fb.switch_to_block(next);
            fb.seal_block(next);
        }
        let se = *signal_exit.get_or_insert_with(|| fb.create_block());
        fb.ins().jump(se, &[]);
    }
    Ok(())
}

/// Get model-stack slot `k` as a RAW (untagged) fixnum i64 for arithmetic. If the
/// slot is already raw (a prior fixnum arithmetic result in this block), return it
/// directly — the cross-op fast path: no re-guard, no re-untag. Otherwise guard it
/// is a fixnum (deopt else, honoring the cross-block `known` elision) and untag once.
fn stack_as_raw(
    fb: &mut FunctionBuilder,
    deopt: Block,
    stack: &[ClifValue],
    stack_raw: &[bool],
    k: usize,
    known: &HashSet<ClifValue>,
) -> ClifValue {
    if stack_raw[k] {
        stack[k]
    } else {
        guard_fixnum(fb, deopt, stack[k], known);
        fb.ins().sshr_imm_u(stack[k], FIXNUM_SHIFT as i64)
    }
}

/// Retag model-stack slot `k` to a tagged `Value` if it currently holds a raw
/// fixnum, clearing its raw flag. Used at every boundary where a value escapes the
/// in-flight arithmetic (returns, predicates, car/cdr, calls/gc roots, cross-block
/// edges, deopt/signal snapshots).
fn stack_force_tagged(
    fb: &mut FunctionBuilder,
    stack: &mut [ClifValue],
    stack_raw: &mut [bool],
    k: usize,
) {
    if stack_raw[k] {
        stack[k] = retag_fixnum(fb, stack[k]);
        stack_raw[k] = false;
    }
}

/// Force every raw slot in the model stack back to a tagged `Value`. Called before
/// any op/terminator that gc_pushes, calls a shim, snapshots the stack for signal
/// dispatch, or writes the stack to `vars` (cross-block) — so nothing raw ever
/// escapes the block or reaches the tracer.
fn retag_all_raw(fb: &mut FunctionBuilder, stack: &mut [ClifValue], stack_raw: &mut [bool]) {
    for k in 0..stack.len() {
        stack_force_tagged(fb, stack, stack_raw, k);
    }
}

/// Ops that participate in cross-op fixnum unboxing: they maintain `stack_raw`
/// themselves (arithmetic produces raw results, comparisons consume raw operands,
/// stack shuffles move the raw flags). EVERY OTHER op force-tags the stack first
/// (so its gc_push / signal snapshot / shim args never observe a raw slot) and has
/// its mask re-synced by the caller.
fn op_preserves_raw(op: &Op) -> bool {
    matches!(
        op,
        Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Rem
            | Op::Add1
            | Op::Sub1
            | Op::Negate
            | Op::Max
            | Op::Min
            | Op::Eqlsign
            | Op::Lss
            | Op::Gtr
            | Op::Leq
            | Op::Geq
            | Op::Constant(_)
            | Op::Nil
            | Op::True
            | Op::Pop
            | Op::Dup
            | Op::StackRef(_)
            | Op::StackSet(_)
            | Op::DiscardN(_)
    )
}

/// Lower one non-control-flow opcode, updating the compile-time operand `stack`
/// (the live CLIF SSA values within the current basic block). Terminators
/// (`Return`/`Goto`/`GotoIf*`) are handled by the block lowerer before this.
#[allow(clippy::too_many_arguments)]
fn lower_simple_op(
    fb: &mut FunctionBuilder,
    pc: usize,
    deopt_sites: &mut Vec<PendingDeopt>,
    signal_exit: &mut Option<Block>,
    constants: &[Value],
    stack: &mut Vec<ClifValue>,
    // Per-slot raw mask (cross-op unboxing), kept in lockstep with `stack`.
    stack_raw: &mut Vec<bool>,
    rt: Option<&RtCtx>,
    handlers: &[HandlerStatic],
    pending: &mut Vec<PendingDispatch>,
    // R2 increment B2: an `Op::Call` spec site carries `(sym, expected, slot_ptr,
    // slot_idx, kind)`. `slot_ptr` is the baked `SpecSlot*` (JIT); `slot_idx` indexes
    // the AOT sidecar's `spec_slot_base`/`spec_expected_base` arrays.
    spec: Option<(u32, u64, i64, usize, SpecCalleeKind)>,
    op: &Op,
    // Cross-block known-fixnum operand values at this block (seeded by
    // `lower_leaf_full` from `compute_known_fixnum_slots`); `guard_fixnum` elides
    // guards for members.
    known: &HashSet<ClifValue>,
    // R1a: heap-constant reloc vector base (baked in entry) + bits->index map, so
    // `Op::Constant` loads a heap object from reloc_base[idx] instead of baking it.
    reloc_base: Option<ClifValue>,
    reloc_index: &std::collections::HashMap<usize, u32>,
    // R2 increment B2: false → JIT (spec `expected`/`slot` baked as `iconst`,
    // byte-identical); true → AOT (loaded from the sidecar's `spec_expected_base`/
    // `spec_slot_base` at `slot_idx`). The two bases are `Some` only in AOT mode at a
    // body with an `Op::Call` spec site (loaded once in the entry block).
    aot: bool,
    spec_slot_base: Option<ClifValue>,
    spec_expected_base: Option<ClifValue>,
    // `make-closure` patched prefix + the callee constant base bound in the entry
    // block (JIT only, `None` when the prefix is 0): `Op::Constant(idx)` with
    // `idx < dynamic_prefix` loads `consts_base[idx]` instead of baking.
    dynamic_prefix: usize,
    consts_base: Option<ClifValue>,
) -> Result<(), CompileError> {
    // Non-unboxing ops must see only tagged Values: force-tag the whole stack so
    // their gc_push / signal snapshot / shim args never observe a raw slot (closes
    // the GC-root + dispatch-snapshot soundness holes in one place).
    if !op_preserves_raw(op) {
        retag_all_raw(fb, stack, stack_raw);
    }
    match op {
        // A `make-closure`-patched slot: per-instance, so load it through the
        // executing callee's constant vector (live, exactly the interpreter's
        // read) instead of baking the compile-time instance's value.
        Op::Constant(idx) if (*idx as usize) < dynamic_prefix => {
            let base = consts_base.expect("consts_base bound for a dynamic-prefix leaf");
            let off = i32::try_from(*idx as usize * 8).map_err(|_| CompileError::BadOperand)?;
            let cv = fb
                .ins()
                .load(types::I64, MemFlagsData::trusted(), base, off);
            stack.push(cv);
            stack_raw.push(false);
        }
        Op::Constant(idx) => {
            let v = constants
                .get(*idx as usize)
                .ok_or(CompileError::BadOperand)?;
            // Reloc-load when this const's bits are in the per-leaf reloc vector,
            // else bake. Keyed on `reloc_index` PRESENCE (not `is_heap_object`):
            //  - heap objects are ALWAYS collected (both JIT + AOT) → present → load
            //    (never bake a heap pointer; GC-pointer-free + AOT-portable, R1a);
            //  - a non-nil/t SYMBOL const is collected ONLY under AOT
            //    (collect_baseline_aot_relocs, const_relocs_for_aot) → present under
            //    AOT → loads its session-stable reloc; absent under JIT → bakes. This
            //    closes the audit CRITICAL #1: a quoted/arg symbol const took the
            //    iconst else-branch and baked its SESSION SymId (silent cross-session
            //    corruption). JIT stays byte-identical (its reloc_index never holds an
            //    op-symbol), exactly as the CallBuiltinSym site below.
            let cv = if reloc_index.contains_key(&v.bits()) {
                let i = reloc_index[&v.bits()];
                let base = reloc_base.expect("reloc_base set when a const is reloc'd");
                fb.ins()
                    .load(types::I64, MemFlagsData::trusted(), base, (i * 8) as i32)
            } else {
                // Fixnum / nil / t / char (immediate, session-stable): bake the bits.
                fb.ins().iconst(types::I64, v.bits() as i64)
            };
            stack.push(cv);
            stack_raw.push(false);
        }
        Op::Nil => {
            stack.push(fb.ins().iconst(types::I64, Value::NIL.bits() as i64));
            stack_raw.push(false);
        }
        Op::True => {
            stack.push(fb.ins().iconst(types::I64, Value::T.bits() as i64));
            stack_raw.push(false);
        }
        Op::Pop => {
            stack.pop().ok_or(CompileError::StackUnderflow)?;
            stack_raw.pop();
        }
        Op::Dup => {
            let top = *stack.last().ok_or(CompileError::StackUnderflow)?;
            let top_raw = *stack_raw.last().ok_or(CompileError::StackUnderflow)?;
            stack.push(top);
            stack_raw.push(top_raw);
        }
        Op::StackRef(n) => {
            // 0 = top of stack, 1 = one below, ...
            let n = *n as usize;
            let idx = stack
                .len()
                .checked_sub(1 + n)
                .ok_or(CompileError::StackUnderflow)?;
            stack.push(stack[idx]);
            stack_raw.push(stack_raw[idx]);
        }
        Op::StackSet(n) => {
            // Assign TOS into the slot N below TOS, then pop TOS (N = 0 == pop).
            let n = *n as usize;
            let top = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let top_raw = stack_raw.pop().ok_or(CompileError::StackUnderflow)?;
            if n != 0 {
                let idx = stack
                    .len()
                    .checked_sub(n)
                    .ok_or(CompileError::StackUnderflow)?;
                stack[idx] = top;
                stack_raw[idx] = top_raw;
            }
        }
        Op::DiscardN(raw) => {
            // Low 7 bits: count to discard. High bit: keep TOS in the last kept
            // slot before discarding. Pure operand-stack manipulation.
            let preserve_tos = (*raw & 0x80) != 0;
            let n = (*raw & 0x7F) as usize;
            if n != 0 {
                let len = stack.len();
                if preserve_tos {
                    let target = len.checked_sub(1 + n).ok_or(CompileError::StackUnderflow)?;
                    stack[target] = stack[len - 1];
                    stack_raw[target] = stack_raw[len - 1];
                } else if n > len {
                    return Err(CompileError::StackUnderflow);
                }
                stack.truncate(len - n);
                stack_raw.truncate(len - n);
            }
        }
        Op::Add | Op::Sub => {
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let b = stack_as_raw(fb, dsite, stack, stack_raw, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, stack_raw, n - 2, known);
            stack.truncate(n - 2);
            stack_raw.truncate(n - 2);
            let is_sub = matches!(op, Op::Sub);
            stack.push(raw_fixnum_addsub(fb, dsite, is_sub, a, b));
            stack_raw.push(true);
        }
        Op::Mul => {
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let b = stack_as_raw(fb, dsite, stack, stack_raw, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, stack_raw, n - 2, known);
            stack.truncate(n - 2);
            stack_raw.truncate(n - 2);
            stack.push(raw_fixnum_mul(fb, dsite, a, b));
            stack_raw.push(true);
        }
        Op::Div | Op::Rem => {
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let b = stack_as_raw(fb, dsite, stack, stack_raw, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, stack_raw, n - 2, known);
            stack.truncate(n - 2);
            stack_raw.truncate(n - 2);
            let is_rem = matches!(op, Op::Rem);
            stack.push(raw_fixnum_divrem(fb, dsite, is_rem, a, b));
            stack_raw.push(true);
        }
        Op::Eq => {
            // Bit-equal -> t natively; differing bits -> the read-only slow-path
            // shim (only symbols-with-pos can make differing bits eq).
            let rt = rt.ok_or(CompileError::UnsupportedOp("eq"))?;
            let b = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let res = fb.declare_var(types::I64);
            let fast = fb.create_block();
            let slow = fb.create_block();
            let merge = fb.create_block();
            let same = fb.ins().icmp(IntCC::Equal, a, b);
            fb.ins().brif(same, fast, &[], slow, &[]);

            fb.switch_to_block(fast);
            fb.seal_block(fast);
            let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
            fb.def_var(res, t);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(slow);
            fb.seal_block(slow);
            let vmctx = fb.use_var(rt.vmctx_var);
            let call = fb.ins().call(rt.refs.eq_slow, &[vmctx, a, b]);
            let slow_res = fb.inst_results(call)[0];
            fb.def_var(res, slow_res);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(merge);
            fb.seal_block(merge);
            stack.push(fb.use_var(res));
        }
        Op::Symbolp => {
            // Symbol tag -> t natively (nil/t are symbols); otherwise the
            // read-only slow-path shim (symbol-with-pos while enabled).
            let rt = rt.ok_or(CompileError::UnsupportedOp("symbolp"))?;
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let res = fb.declare_var(types::I64);
            let fast = fb.create_block();
            let slow = fb.create_block();
            let merge = fb.create_block();
            let tag = fb.ins().band_imm_u(a, TAG_MASK as i64);
            let is_sym = fb.ins().icmp_imm_u(IntCC::Equal, tag, TAG_SYMBOL as i64);
            fb.ins().brif(is_sym, fast, &[], slow, &[]);

            fb.switch_to_block(fast);
            fb.seal_block(fast);
            let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
            fb.def_var(res, t);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(slow);
            fb.seal_block(slow);
            let vmctx = fb.use_var(rt.vmctx_var);
            let call = fb.ins().call(rt.refs.symbolp_slow, &[vmctx, a]);
            let slow_res = fb.inst_results(call)[0];
            fb.def_var(res, slow_res);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(merge);
            fb.seal_block(merge);
            stack.push(fb.use_var(res));
        }
        Op::Add1 | Op::Sub1 | Op::Negate => {
            let n = stack.len();
            if n < 1 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let a = stack_as_raw(fb, dsite, stack, stack_raw, n - 1, known);
            stack.truncate(n - 1);
            stack_raw.truncate(n - 1);
            let kind = match op {
                Op::Add1 => UnaryKind::Add1,
                Op::Sub1 => UnaryKind::Sub1,
                Op::Negate => UnaryKind::Negate,
                _ => unreachable!("matched Add1/Sub1/Negate above"),
            };
            stack.push(raw_fixnum_unop(fb, dsite, kind, a));
            stack_raw.push(true);
        }
        Op::Eqlsign | Op::Lss | Op::Gtr | Op::Leq | Op::Geq => {
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let b = stack_as_raw(fb, dsite, stack, stack_raw, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, stack_raw, n - 2, known);
            stack.truncate(n - 2);
            stack_raw.truncate(n - 2);
            let cc = match op {
                Op::Eqlsign => IntCC::Equal,
                Op::Lss => IntCC::SignedLessThan,
                Op::Gtr => IntCC::SignedGreaterThan,
                Op::Leq => IntCC::SignedLessThanOrEqual,
                Op::Geq => IntCC::SignedGreaterThanOrEqual,
                _ => unreachable!("matched comparison ops above"),
            };
            // Operands raw, result is a tagged t/nil (a sink, not raw).
            let cond = fb.ins().icmp(cc, a, b);
            let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
            let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
            stack.push(fb.ins().select(cond, t, nil));
            stack_raw.push(false);
        }
        Op::Null | Op::Not | Op::Consp | Op::Stringp | Op::Listp => {
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let kind = match op {
                Op::Null | Op::Not => PredKind::Null,
                Op::Consp => PredKind::Consp,
                Op::Stringp => PredKind::Stringp,
                Op::Listp => PredKind::Listp,
                _ => unreachable!("matched predicate ops above"),
            };
            stack.push(lower_predicate(fb, kind, a));
        }
        Op::Car | Op::Cdr => {
            // Non-raw: the top-of-fn retag_all_raw already tagged the stack; the
            // deopt snapshot's mask is all-false (cold retag is a no-op here).
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let is_cdr = matches!(op, Op::Cdr);
            stack.push(lower_car_cdr(fb, Some(dsite), is_cdr, false, a));
        }
        Op::CarSafe | Op::CdrSafe => {
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let is_cdr = matches!(op, Op::CdrSafe);
            stack.push(lower_car_cdr(fb, None, is_cdr, true, a));
        }
        Op::Max | Op::Min => {
            // Both fixnum -> select the larger/smaller RAW operand (one of the two
            // valid raw inputs, so the result stays raw); otherwise deopt to the
            // interpreter's number-coercing builtin.
            let n = stack.len();
            if n < 2 {
                return Err(CompileError::StackUnderflow);
            }
            let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
            let b = stack_as_raw(fb, dsite, stack, stack_raw, n - 1, known);
            let a = stack_as_raw(fb, dsite, stack, stack_raw, n - 2, known);
            stack.truncate(n - 2);
            stack_raw.truncate(n - 2);
            stack.push(raw_fixnum_maxmin(fb, matches!(op, Op::Min), a, b));
            stack_raw.push(true);
        }
        Op::Integerp | Op::Numberp => {
            // Fixnum tag -> t natively; anything else (bignum/float/non-number)
            // through the context-free slow shim.
            let rt = rt.ok_or(CompileError::UnsupportedOp("predicate"))?;
            let a = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let shim = if matches!(op, Op::Integerp) {
                rt.refs.integerp_slow
            } else {
                rt.refs.numberp_slow
            };
            let res = fb.declare_var(types::I64);
            let fast = fb.create_block();
            let slow = fb.create_block();
            let merge = fb.create_block();
            let tagbits = fb.ins().band_imm_u(a, FIXNUM_CHECK_MASK as i64);
            let is_fix = fb
                .ins()
                .icmp_imm_u(IntCC::Equal, tagbits, FIXNUM_CHECK_VALUE as i64);
            fb.ins().brif(is_fix, fast, &[], slow, &[]);

            fb.switch_to_block(fast);
            fb.seal_block(fast);
            let t = fb.ins().iconst(types::I64, Value::T.bits() as i64);
            fb.def_var(res, t);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(slow);
            fb.seal_block(slow);
            let call = fb.ins().call(shim, &[a]);
            let slow_res = fb.inst_results(call)[0];
            fb.def_var(res, slow_res);
            fb.ins().jump(merge, &[]);

            fb.switch_to_block(merge);
            fb.seal_block(merge);
            stack.push(fb.use_var(res));
        }
        Op::VarRef(idx) => {
            // Read through the runtime's variable machinery (buffer-locals,
            // redirects); can signal void-variable. Reads are idempotent, so
            // this neither poisons nor guards.
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let sym = const_sym_id(constants, *idx)?;
            // Root live stack values: variable access may allocate.
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let sym_v = materialize_op_sym_id(fb, reloc_base, reloc_index, sym);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            let call = fb.ins().call(rt.refs.varref, &[vmctx, sym_v, out_addr]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
            let cont = fb.create_block();
            let ok = fb.ins().icmp_imm_u(IntCC::Equal, status, STATUS_OK);
            fb.ins().brif(ok, cont, &[], se, &[]);
            fb.switch_to_block(cont);
            fb.seal_block(cont);
            let result = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            stack.push(result);
        }
        Op::VarSet(idx) => {
            // Assign through the runtime (may run variable watchers — arbitrary
            // lisp — and signal). A side effect: poisons later guards.
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let sym = const_sym_id(constants, *idx)?;
            let val = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let sym_v = materialize_op_sym_id(fb, reloc_base, reloc_index, sym);
            let call = fb.ins().call(rt.refs.varset, &[vmctx, sym_v, val]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
            let cont = fb.create_block();
            let ok = fb.ins().icmp_imm_u(IntCC::Equal, status, STATUS_OK);
            fb.ins().brif(ok, cont, &[], se, &[]);
            fb.switch_to_block(cont);
            fb.seal_block(cont);
        }
        Op::Call(n) | Op::Apply(n) => {
            // `rt` is always present here (`needs_rt` includes Call/Apply).
            // Stack: [func a1 .. aN] -> [result], mirroring the interpreter's
            // Op::Call / Op::Apply; the two differ only in which shim runs
            // (apply spreads its last argument inside the runtime).
            let rt = rt.ok_or(CompileError::UnsupportedOp("call"))?;
            let shim = if matches!(op, Op::Apply(_)) {
                rt.refs.apply
            } else {
                rt.refs.call
            };
            let n = *n as usize;
            if stack.len() < n + 1 {
                return Err(CompileError::StackUnderflow);
            }
            let args_at = stack.len() - n;
            // Speculated direct call: the callee slot holds a constant symbol
            // whose compile-time binding was a bytecode object or a fixed-arity
            // builtin subr (Apply never speculates).
            let spec = spec.filter(|_| matches!(op, Op::Call(_)));
            // SSA soundness gate: emit the spec shim ONLY when the callee
            // slot's value is provably the site's symbol constant (iconst /
            // reloc-load of its tagged bits). find_spec_sites' abstract stack
            // tracking selected this site; if the lowering cannot re-prove it
            // here, the site silently degrades to the generic call below —
            // never a wrong-callee speculation.
            let spec = spec.filter(|&(sym, ..)| {
                let proven =
                    callee_is_symbol_const(fb, stack[args_at - 1], sym, reloc_base, reloc_index);
                if !proven {
                    tracing::debug!(
                        target: "neovm_jit",
                        sym,
                        "spec site dropped: callee slot not provably the tracked symbol"
                    );
                }
                proven
            });
            // LEVEL-B (JIT only): inline logand/logior/logxor/lognot as native ops
            // on the TAGGED fixnum bits, guarded by a fixnum check that DEOPTS —
            // instead of the armed shim's 8-arg call. The fixnum tag is 2
            // (`retag_fixnum = (n<<2)|2`), so `a & b` / `a | b` keep it (2&2=2,
            // 2|2=2), `a ^ b` clears it (2^2=0 → restore with `| 2`), and negating
            // a tagged fixnum yields `lognot` exactly (-a == retag(~n)). A non-fixnum
            // arg deopts to the precise-deopt block, where the interpreter re-runs
            // the REAL call from `pc` (graceful for the odd bignum; a fully mixed
            // loop stays interpreted). Redefinition is caught by the leaf's
            // inline_epoch eviction (set in compile_bytecode_function_inner). AOT
            // (`aot`) keeps the shim — its loader owns arm/disarm.
            if jit_inline_arith_on()
                && !aot
                && let Some((_, _, _, _, SpecCalleeKind::ArithIntrinsic { op })) = spec
                && arith_op_inlines(op)
            {
                let dsite = deopt_site(fb, pc, handlers.len(), stack, stack_raw, deopt_sites);
                let sp = stack.len();
                let is_lognot = op == ARITH_KIND_LOGNOT as u8;
                let a = stack[sp - if is_lognot { 1 } else { 2 }];
                guard_fixnum(fb, dsite, a, known);
                let res = if is_lognot {
                    // lognot(n) == ~n == -n-1; on the tagged bits: -a == retag(~n).
                    fb.ins().ineg(a)
                } else {
                    let b = stack[sp - 1];
                    guard_fixnum(fb, dsite, b, known);
                    match op {
                        x if x == ARITH_KIND_LOGAND as u8 => fb.ins().band(a, b),
                        x if x == ARITH_KIND_LOGIOR as u8 => fb.ins().bor(a, b),
                        x if x == ARITH_KIND_MOD as u8 => {
                            // GNU Fmod integer branch on the untagged values:
                            // truncated srem, then pull a nonzero result onto
                            // the divisor's side of zero. Zero divisor deopts
                            // (the interpreter re-runs the real call, which
                            // signals arith-error). The result magnitude stays
                            // below |b|, so the retag never overflows.
                            let av = fb.ins().sshr_imm_u(a, FIXNUM_SHIFT as i64);
                            let bv = fb.ins().sshr_imm_u(b, FIXNUM_SHIFT as i64);
                            let nonzero = fb.ins().icmp_imm_u(IntCC::NotEqual, bv, 0);
                            emit_guard(fb, dsite, nonzero);
                            let m = fb.ins().srem(av, bv);
                            let signs = fb.ins().bxor(m, bv);
                            let differ = fb.ins().icmp_imm_u(IntCC::SignedLessThan, signs, 0);
                            let m_nonzero = fb.ins().icmp_imm_u(IntCC::NotEqual, m, 0);
                            let need_fix = fb.ins().band(differ, m_nonzero);
                            let fixed = fb.ins().iadd(m, bv);
                            let floored = fb.ins().select(need_fix, fixed, m);
                            retag_fixnum(fb, floored)
                        }
                        _ => {
                            debug_assert_eq!(op, ARITH_KIND_LOGXOR as u8);
                            // XOR clears the tag bit (2^2=0); restore it.
                            let x = fb.ins().bxor(a, b);
                            fb.ins().bor_imm_u(x, FIXNUM_CHECK_VALUE as i64)
                        }
                    }
                };
                // Drop callee + args, push the tagged fixnum result.
                stack.truncate(args_at - 1);
                stack_raw.truncate(args_at - 1);
                stack.push(res);
                stack_raw.push(false);
                return Ok(());
            }
            // Pred/EqIncl sites pass their 1–2 args in REGISTERS on the direct
            // path (no spill; their fallback block spills for itself). Every
            // other shape spills the args into the call buffer for its shim.
            let reg_args: Option<SmallVec<[ClifValue; 2]>> = match spec {
                Some((_, _, _, _, kind)) if kind.is_reg_args() => {
                    Some(stack[args_at..].iter().copied().collect())
                }
                _ => {
                    for (i, &v) in stack[args_at..].iter().enumerate() {
                        fb.ins()
                            .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
                    }
                    None
                }
            };
            let func_val = stack[args_at - 1];
            stack.truncate(args_at - 1);
            // Root every value that stays live across the call (the callee +
            // args are rooted by the shim; the constants are rooted by the
            // dispatch seam via the executing function). Pred/EqIncl direct
            // paths SKIP this: their shims are GC-free by contract (they bounce
            // to the fallback block rather than run anything that could
            // allocate), and the fallback block roots for itself.
            let saved = if stack.is_empty() || reg_args.is_some() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let args_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            let n_val = fb.ins().iconst(types::I64, n as i64);
            // Subr-kind sites can return STATUS_NEED_GENERIC, which routes to a
            // fallback block that re-does this site as the ORIGINAL generic
            // call; bytecode-kind sites keep their everything-inside-the-shim
            // protocol. `None` = no NEED_GENERIC possible.
            let mut generic_fallback: Option<Block> = None;
            let call = match spec {
                Some((sym, expected, slot_ptr, slot_idx, SpecCalleeKind::Bytecode)) => {
                    let sym_v = materialize_op_sym_id(fb, reloc_base, reloc_index, sym);
                    let exp_v =
                        materialize_spec_expected(fb, aot, spec_expected_base, expected, slot_idx);
                    let slot_v = materialize_spec_slot(fb, aot, spec_slot_base, slot_ptr, slot_idx);
                    fb.ins().call(
                        rt.refs.call_spec,
                        &[vmctx, sym_v, exp_v, slot_v, args_addr, n_val, out_addr],
                    )
                }
                Some((sym, expected, slot_ptr, slot_idx, kind)) => {
                    // PRESERVE emission order: create the generic-fallback block
                    // FIRST (byte-identical to before B2), then the operands.
                    generic_fallback = Some(fb.create_block());
                    let sym_v = materialize_op_sym_id(fb, reloc_base, reloc_index, sym);
                    let exp_v =
                        materialize_spec_expected(fb, aot, spec_expected_base, expected, slot_idx);
                    let slot_v = materialize_spec_slot(fb, aot, spec_slot_base, slot_ptr, slot_idx);
                    // The refs are Some whenever a subr-kind site exists (the
                    // declare is keyed on exactly that condition).
                    match (kind, &reg_args) {
                        (SpecCalleeKind::SubrGeneral, _) => {
                            let f = rt
                                .refs
                                .call_subr_spec
                                .ok_or(CompileError::UnsupportedOp("subr-spec-refs"))?;
                            fb.ins().call(
                                f,
                                &[vmctx, sym_v, exp_v, slot_v, args_addr, n_val, out_addr],
                            )
                        }
                        (
                            SpecCalleeKind::PredRecordp | SpecCalleeKind::PredSymbolWithPos,
                            Some(args),
                        ) => {
                            let f = rt
                                .refs
                                .pred_spec
                                .ok_or(CompileError::UnsupportedOp("subr-spec-refs"))?;
                            let kind_v = fb.ins().iconst(
                                types::I64,
                                if kind == SpecCalleeKind::PredRecordp {
                                    PRED_KIND_RECORDP
                                } else {
                                    PRED_KIND_SYMBOL_WITH_POS_P
                                },
                            );
                            fb.ins()
                                .call(f, &[vmctx, kind_v, sym_v, exp_v, slot_v, args[0], out_addr])
                        }
                        (SpecCalleeKind::EqInclProps, Some(args)) => {
                            let f = rt
                                .refs
                                .eq_incl_props_spec
                                .ok_or(CompileError::UnsupportedOp("subr-spec-refs"))?;
                            fb.ins().call(
                                f,
                                &[vmctx, sym_v, exp_v, slot_v, args[0], args[1], out_addr],
                            )
                        }
                        (SpecCalleeKind::ArithIntrinsic { op }, Some(args)) => {
                            let f = rt
                                .refs
                                .arith_spec
                                .ok_or(CompileError::UnsupportedOp("subr-spec-refs"))?;
                            let kind_v = fb.ins().iconst(types::I64, op as i64);
                            // lognot is 1-arg: pass a dummy `b` (the shim ignores it
                            // for LOGNOT). The 2-arg ops collected both.
                            let b_v = args.get(1).copied().unwrap_or_else(|| {
                                fb.ins().iconst(types::I64, Value::NIL.bits() as i64)
                            });
                            fb.ins().call(
                                f,
                                &[vmctx, kind_v, sym_v, exp_v, slot_v, args[0], b_v, out_addr],
                            )
                        }
                        // Reg-arg kinds always collected their args above.
                        _ => return Err(CompileError::UnsupportedOp("subr-spec-shape")),
                    }
                }
                None => fb
                    .ins()
                    .call(shim, &[vmctx, func_val, args_addr, n_val, out_addr]),
            };
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            // STATUS_OK -> continue with the result; STATUS_NEED_GENERIC (subr
            // spec sites only) -> the generic fallback block; anything else is
            // STATUS_SIGNAL -> propagate via the handler-aware signal target.
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
            let cont = fb.create_block();
            let ok = fb.ins().icmp_imm_u(IntCC::Equal, status, STATUS_OK);
            if let Some(gen_block) = generic_fallback {
                let check = fb.create_block();
                fb.ins().brif(ok, cont, &[], check, &[]);
                fb.switch_to_block(check);
                fb.seal_block(check);
                let need_gen = fb
                    .ins()
                    .icmp_imm_u(IntCC::Equal, status, STATUS_NEED_GENERIC);
                fb.ins().brif(need_gen, gen_block, &[], se, &[]);
                // Fallback: the ORIGINAL generic Op::Call lowering for this
                // site — spill the register args (if any), root the residual
                // stack, call the plain generic shim on the constant SYMBOL
                // (which resolves the live binding: fset/advice/overrides all
                // take effect), same OK/signal branching.
                fb.switch_to_block(gen_block);
                fb.seal_block(gen_block);
                if let Some(args) = &reg_args {
                    for (i, &v) in args.iter().enumerate() {
                        fb.ins()
                            .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
                    }
                }
                let saved_gen = if stack.is_empty() {
                    CondRoots::NONE
                } else {
                    emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
                };
                let vmctx_gen = fb.use_var(rt.vmctx_var);
                let call_gen = fb
                    .ins()
                    .call(shim, &[vmctx_gen, func_val, args_addr, n_val, out_addr]);
                let status_gen = fb.inst_results(call_gen)[0];
                emit_cond_residual_roots_post(fb, rt, saved_gen);
                let se_gen = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
                let ok_gen = fb.ins().icmp_imm_u(IntCC::Equal, status_gen, STATUS_OK);
                fb.ins().brif(ok_gen, cont, &[], se_gen, &[]);
            } else {
                fb.ins().brif(ok, cont, &[], se, &[]);
            }
            fb.switch_to_block(cont);
            fb.seal_block(cont);
            let result = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            stack.push(result);
        }
        Op::Cons => {
            // `rt` is always present here: analyze_cfg accepts Cons only when the
            // function declares the shims (see `needs_rt` in lower_leaf).
            let rt = rt.ok_or(CompileError::UnsupportedOp("cons"))?;
            let cdr = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let car = stack.pop().ok_or(CompileError::StackUnderflow)?;
            // No rooting at all: the cons shim is pure allocation and never
            // reaches a GC safe point (see `neovm_jit_cons`), so neither
            // car/cdr nor the residual operand stack can be collected under it.
            let call = fb.ins().call(rt.refs.cons, &[car, cdr]);
            let result = fb.inst_results(call)[0];
            stack.push(result);
        }
        Op::VarBind(idx) => {
            // GNU Bvarbind: specbind(sym, POP). A typed per-buffer forwarder can
            // signal, so branch through the same handler-aware signal target as
            // VarSet. The shim records a bind depth only after a successful
            // store.
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let sym = const_sym_id(constants, *idx)?;
            let val = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let vmctx = fb.use_var(rt.vmctx_var);
            let sym_v = materialize_op_sym_id(fb, reloc_base, reloc_index, sym);
            // The shim runs variable watchers (arbitrary lisp -> GC). `val` is
            // rooted by `specbind` inside the shim, but the remaining operand
            // stack lives only in Cranelift registers — root it across the call
            // (mirrors VarRef/VarSet). This is an exact-root GC: a live Value
            // unrooted across a GC-capable call is a use-after-free.
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let call = fb.ins().call(rt.refs.varbind, &[vmctx, sym_v, val]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let cont = fb.create_block();
            let signal =
                signal_target_for_site(fb, signal_exit, handlers, pending, stack.as_slice());
            let ok = fb.ins().icmp_imm_u(IntCC::Equal, status, STATUS_OK);
            fb.ins().brif(ok, cont, &[], signal, &[]);
            fb.switch_to_block(cont);
            fb.seal_block(cont);
        }
        Op::Unbind(n) => {
            // Unbind the N most recent dynamic bindings. Static analysis
            // guarantees balance, but cleanup Lisp/watchers can still exit.
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let vmctx = fb.use_var(rt.vmctx_var);
            let n_v = fb.ins().iconst(types::I64, *n as i64);
            // The shim runs unwind-protect cleanups (arbitrary lisp -> GC); root
            // the whole live operand stack across the call.
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let call = fb.ins().call(rt.refs.unbind, &[vmctx, n_v]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let cont = fb.create_block();
            let signal =
                signal_target_for_site(fb, signal_exit, handlers, pending, stack.as_slice());
            let ok = fb.ins().icmp_imm_u(IntCC::Equal, status, STATUS_OK);
            fb.ins().brif(ok, cont, &[], signal, &[]);
            fb.switch_to_block(cont);
            fb.seal_block(cont);
        }
        Op::SaveCurrentBuffer | Op::SaveExcursion | Op::SaveRestriction => {
            // Infallible specpdl records (the interpreter arms mirrored in the
            // shims); restored by the matching Unbind or the frame unwind.
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let shim = match op {
                Op::SaveCurrentBuffer => rt.refs.save_current_buffer,
                Op::SaveExcursion => rt.refs.save_excursion,
                Op::SaveRestriction => rt.refs.save_restriction,
                _ => unreachable!("matched Save* above"),
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            fb.ins().call(shim, &[vmctx]);
        }
        Op::UnwindProtectPop => {
            // Pop the cleanup form and register the unwind-protect record
            // (infallible; the cleanup runs via the shared unbind machinery).
            let rt = rt.ok_or(CompileError::UnsupportedOp("variable"))?;
            let forms = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let vmctx = fb.use_var(rt.vmctx_var);
            fb.ins().call(rt.refs.unwind_protect, &[vmctx, forms]);
        }
        Op::SaveWindowExcursion => {
            // Evaluate the popped body under a window-configuration
            // save/restore via the shim (interpreter arm parity).
            let rt = rt.ok_or(CompileError::UnsupportedOp("builtin"))?;
            let body = stack.pop().ok_or(CompileError::StackUnderflow)?;
            // Root remaining live values: the body runs arbitrary lisp.
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            let call = fb
                .ins()
                .call(rt.refs.save_window_excursion, &[vmctx, body, out_addr]);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
            let cont = fb.create_block();
            let ok = fb.ins().icmp_imm_u(IntCC::Equal, status, STATUS_OK);
            fb.ins().brif(ok, cont, &[], se, &[]);
            fb.switch_to_block(cont);
            fb.seal_block(cont);
            let result = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            stack.push(result);
        }
        Op::CallBuiltin(..) | Op::CallBuiltinSym(..) | Op::Aset => {
            // Named-builtin escape hatch + aset: route through the
            // Vm::*_for_jit helpers mirroring the interpreter arms
            // (override-aware / advice-bypassing / writeback / quit poll).
            let rt = rt.ok_or(CompileError::UnsupportedOp("builtin"))?;
            let (variant, sym, nargs): (i64, u32, usize) = match op {
                Op::CallBuiltin(name_idx, n) => {
                    (0, const_sym_id(constants, *name_idx)?, *n as usize)
                }
                Op::CallBuiltinSym(sym, n) => (1, sym.0, *n as usize),
                Op::Aset => (2, 0, 3),
                _ => unreachable!("matched named-builtin ops above"),
            };
            if stack.len() < nargs {
                return Err(CompileError::StackUnderflow);
            }
            // R2: a Tier-B CallBuiltinSym spec site takes the dispatch-skip fast
            // path (`neovm_jit_cbsym_spec`) with a NEED_GENERIC fallback to THIS
            // op's original general lowering; Tier-A sites (COMMIT 5) take the
            // GC-free read shim; every other named-builtin op keeps the general
            // lowering. As of increment A BOTH JIT and AOT baseline emit take the
            // fast path (CBSym classification is obarray-free) — the `sym`
            // materialize below is AOT-reloc-aware, so the baked shim call reloads
            // the SymId by name under AOT and iconsts it under JIT (byte-identical).
            let cbsym_spec_b = matches!(op, Op::CallBuiltinSym(..))
                && matches!(spec, Some((_, _, _, _, SpecCalleeKind::CbsymTierB)));
            // Tier-A GC-free read (`neovm_jit_cbsym_read`): its OK path returns an
            // IMMEDIATE and never allocates, so the fast path skips residual-stack
            // rooting entirely (like the round-1 predicate shims). `which` is the
            // baked builtin discriminant. NEED_GENERIC still routes to the general
            // fallback (which DOES root, since it can allocate).
            let cbsym_a_which: Option<u8> = if matches!(op, Op::CallBuiltinSym(..)) {
                match spec {
                    Some((_, _, _, _, SpecCalleeKind::CbsymTierA { which })) => Some(which),
                    _ => None,
                }
            } else {
                None
            };
            let at = stack.len() - nargs;
            for (i, &v) in stack[at..].iter().enumerate() {
                fb.ins()
                    .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
            }
            stack.truncate(at);
            // Root remaining live values (arbitrary lisp may run; the shim roots
            // the operands themselves). The Tier-A read shim is GC-free by
            // contract, so its fast path needs NO residual rooting; its
            // NEED_GENERIC fallback block re-roots for the general call.
            let saved = if stack.is_empty() || cbsym_a_which.is_some() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            // R2-E (must-nail #2): the named-builtin callee SymId is session-specific.
            // The JIT bakes it (`iconst(sym)`); AOT must RELOC it BY NAME — the op's
            // symbol Value was collected into the per-leaf reloc vector, so load its
            // bits from reloc_base[idx] and recover the SymId (`bits >> TAG_BITS`,
            // TAG_SYMBOL==0). Keyed on `reloc_index` presence: the JIT reloc set never
            // contains op-symbols (only heap consts), so JIT always bakes → byte-
            // identical. `Aset` (variant 2, sym==0) has no symbol → unchanged iconst.
            // Shared by the fast-shim call, the direct general call, AND the
            // fallback (all JIT-only when a CBSym spec site exists).
            let sym_v = match reloc_index
                .get(&((sym as usize) << TAG_BITS | TAG_SYMBOL))
                .filter(|_| variant != 2)
            {
                Some(&idx) => {
                    let base = reloc_base.expect("reloc_base set when an op-symbol is reloc'd");
                    let sym_bits =
                        fb.ins()
                            .load(types::I64, MemFlagsData::trusted(), base, (idx * 8) as i32);
                    fb.ins().ushr_imm_u(sym_bits, TAG_BITS as i64)
                }
                None => fb.ins().iconst(types::I64, sym as i64),
            };
            let args_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0);
            let n_val = fb.ins().iconst(types::I64, nargs as i64);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            // Fast path: the Tier-A GC-free read shim or the Tier-B dispatch-skip
            // shim; NEED_GENERIC routes to `generic_fallback` (the ORIGINAL
            // named-builtin call). Everything else keeps the general lowering.
            let mut generic_fallback: Option<Block> = None;
            let call = if let Some(which) = cbsym_a_which {
                generic_fallback = Some(fb.create_block());
                let f = rt
                    .refs
                    .cbsym_read
                    .ok_or(CompileError::UnsupportedOp("cbsym-read-refs"))?;
                let which_v = fb.ins().iconst(types::I64, which as i64);
                fb.ins()
                    .call(f, &[vmctx, which_v, sym_v, args_addr, n_val, out_addr])
            } else if cbsym_spec_b {
                generic_fallback = Some(fb.create_block());
                let f = rt
                    .refs
                    .cbsym_spec
                    .ok_or(CompileError::UnsupportedOp("cbsym-spec-refs"))?;
                fb.ins()
                    .call(f, &[vmctx, sym_v, args_addr, n_val, out_addr])
            } else {
                let variant_v = fb.ins().iconst(types::I64, variant);
                fb.ins().call(
                    rt.refs.named_builtin,
                    &[vmctx, variant_v, sym_v, args_addr, n_val, out_addr],
                )
            };
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
            let cont = fb.create_block();
            let ok = fb.ins().icmp_imm_u(IntCC::Equal, status, STATUS_OK);
            if let Some(gen_block) = generic_fallback {
                // STATUS_OK -> cont; STATUS_NEED_GENERIC -> the general CBSym
                // lowering; anything else -> STATUS_SIGNAL via the signal target.
                let check = fb.create_block();
                fb.ins().brif(ok, cont, &[], check, &[]);
                fb.switch_to_block(check);
                fb.seal_block(check);
                let need_gen = fb
                    .ins()
                    .icmp_imm_u(IntCC::Equal, status, STATUS_NEED_GENERIC);
                fb.ins().brif(need_gen, gen_block, &[], se, &[]);
                // Fallback: the ORIGINAL general CBSym lowering (variant 1 ->
                // `Vm::callbuiltinsym_for_jit`). The fast shim left the args in
                // `call_args_slot` untouched, so reuse `args_addr`; the residual
                // stack was restored above, so re-root it around this call.
                fb.switch_to_block(gen_block);
                fb.seal_block(gen_block);
                let saved_gen = if stack.is_empty() {
                    CondRoots::NONE
                } else {
                    emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
                };
                let vmctx_gen = fb.use_var(rt.vmctx_var);
                let variant_gen = fb.ins().iconst(types::I64, variant);
                let call_gen = fb.ins().call(
                    rt.refs.named_builtin,
                    &[vmctx_gen, variant_gen, sym_v, args_addr, n_val, out_addr],
                );
                let status_gen = fb.inst_results(call_gen)[0];
                emit_cond_residual_roots_post(fb, rt, saved_gen);
                let se_gen = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
                let ok_gen = fb.ins().icmp_imm_u(IntCC::Equal, status_gen, STATUS_OK);
                fb.ins().brif(ok_gen, cont, &[], se_gen, &[]);
            } else {
                fb.ins().brif(ok, cont, &[], se, &[]);
            }
            fb.switch_to_block(cont);
            fb.seal_block(cont);
            let result = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            stack.push(result);
        }
        Op::List(n) => {
            // N-ary list builder — infallible allocation through the shim
            // (the interpreter's Value::list_from_slice on the stack slice).
            let rt = rt.ok_or(CompileError::UnsupportedOp("builtin"))?;
            let n = *n as usize;
            if stack.len() < n {
                return Err(CompileError::StackUnderflow);
            }
            let at = stack.len() - n;
            for (i, &v) in stack[at..].iter().enumerate() {
                fb.ins()
                    .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
            }
            stack.truncate(at);
            // Root remaining live values (the allocation may GC; the shim
            // roots the operands themselves).
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let args_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0);
            let n_val = fb.ins().iconst(types::I64, n as i64);
            let call = fb.ins().call(rt.refs.list, &[args_addr, n_val]);
            let result = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            stack.push(result);
        }
        other => {
            // Slice-shaped builtins (nconc/concat/substring): spill the
            // operands and call the generic slice shim with the table index
            // baked in — the SAME builtins::*_slice function the interpreter
            // arm calls.
            if let Some((nargs, idx)) = slice_builtin_spec(other) {
                let rt = rt.ok_or(CompileError::UnsupportedOp("builtin"))?;
                if stack.len() < nargs {
                    return Err(CompileError::StackUnderflow);
                }
                let at = stack.len() - nargs;
                for (i, &v) in stack[at..].iter().enumerate() {
                    fb.ins()
                        .stack_store(rt.ptr_ty, v, rt.call_args_slot, (i * 8) as i32);
                }
                stack.truncate(at);
                let saved = if stack.is_empty() {
                    CondRoots::NONE
                } else {
                    emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
                };
                let idx_v = fb.ins().iconst(types::I64, idx as i64);
                let args_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_args_slot, 0);
                let n_val = fb.ins().iconst(types::I64, nargs as i64);
                let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
                let call = fb
                    .ins()
                    .call(rt.refs.builtin_slice, &[idx_v, args_addr, n_val, out_addr]);
                let status = fb.inst_results(call)[0];
                emit_cond_residual_roots_post(fb, rt, saved);
                let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
                let cont = fb.create_block();
                let ok = fb.ins().icmp_imm_u(IntCC::Equal, status, STATUS_OK);
                fb.ins().brif(ok, cont, &[], se, &[]);
                fb.switch_to_block(cont);
                fb.seal_block(cont);
                let result = fb
                    .ins()
                    .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
                stack.push(result);
                return Ok(());
            }
            // Direct-builtin ops: pop the operands, root the rest of the live
            // frame, and call the arity-shaped generic shim with the table
            // index baked in — the shim invokes the SAME builtins::* function
            // the interpreter arm calls.
            let Some((arity, idx)) = direct_builtin_spec(other) else {
                return Err(CompileError::UnsupportedOp(op_category(other)));
            };
            let rt = rt.ok_or(CompileError::UnsupportedOp("builtin"))?;
            let arity = arity as usize;
            if stack.len() < arity {
                return Err(CompileError::StackUnderflow);
            }
            let at = stack.len() - arity;
            let operands: Vec<ClifValue> = stack[at..].to_vec();
            stack.truncate(at);
            // Root remaining live values (the builtin may allocate/GC; the
            // shim roots the operands themselves).
            let saved = if stack.is_empty() {
                CondRoots::NONE
            } else {
                emit_cond_residual_roots_pre(fb, rt, stack.as_slice())
            };
            let vmctx = fb.use_var(rt.vmctx_var);
            let idx_v = fb.ins().iconst(types::I64, idx as i64);
            let out_addr = fb.ins().stack_addr(rt.ptr_ty, rt.call_result_slot, 0);
            let shim = match arity {
                1 => rt.refs.builtin1,
                2 => rt.refs.builtin2,
                _ => rt.refs.builtin3,
            };
            let mut call_args = vec![vmctx, idx_v];
            call_args.extend(operands);
            call_args.push(out_addr);
            let call = fb.ins().call(shim, &call_args);
            let status = fb.inst_results(call)[0];
            emit_cond_residual_roots_post(fb, rt, saved);
            let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack);
            let cont = fb.create_block();
            let ok = fb.ins().icmp_imm_u(IntCC::Equal, status, STATUS_OK);
            fb.ins().brif(ok, cont, &[], se, &[]);
            fb.switch_to_block(cont);
            fb.seal_block(cont);
            let result = fb
                .ins()
                .stack_load(rt.ptr_ty, types::I64, rt.call_result_slot, 0);
            stack.push(result);
        }
    }
    Ok(())
}

/// Minimum operand-stack depth a simple op requires, and its net depth change.
/// `Err` for anything outside the supported simple subset.
pub(crate) fn simple_effect(op: &Op) -> Result<(usize, i64), CompileError> {
    if let Some((arity, _)) = direct_builtin_spec(op) {
        // N operands -> one result.
        return Ok((arity as usize, 1 - arity as i64));
    }
    if let Some((nargs, _)) = slice_builtin_spec(op) {
        return Ok((nargs, 1 - nargs as i64));
    }
    Ok(match op {
        Op::List(n) => (*n as usize, 1 - *n as i64),
        Op::CallBuiltin(_, n) | Op::CallBuiltinSym(_, n) => (*n as usize, 1 - *n as i64),
        Op::Aset => (3, -2),
        Op::SaveWindowExcursion => (1, 0),
        Op::Constant(_) | Op::Nil | Op::True => (0, 1),
        Op::StackRef(n) => (*n as usize + 1, 1),
        Op::StackSet(n) => (*n as usize + 1, -1),
        Op::DiscardN(raw) => {
            let n = (*raw & 0x7F) as usize;
            let needs = if (*raw & 0x80) != 0 && n > 0 {
                n + 1
            } else {
                n
            };
            (needs, -(n as i64))
        }
        Op::Dup => (1, 1),
        Op::Pop => (1, -1),
        Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Rem
        | Op::Eq
        | Op::Eqlsign
        | Op::Lss
        | Op::Gtr
        | Op::Leq
        | Op::Geq => (2, -1),
        Op::Add1 | Op::Sub1 | Op::Negate => (1, 0),
        Op::Null | Op::Not | Op::Consp | Op::Stringp | Op::Listp | Op::Symbolp => (1, 0),
        Op::Integerp | Op::Numberp => (1, 0),
        Op::Car | Op::Cdr | Op::CarSafe | Op::CdrSafe => (1, 0),
        Op::Max | Op::Min => (2, -1),
        Op::Cons => (2, -1),
        // [func a1 .. aN] -> [result]
        Op::Call(n) | Op::Apply(n) => (*n as usize + 1, -(*n as i64)),
        Op::VarRef(_) => (0, 1),
        Op::VarSet(_) => (1, -1),
        Op::VarBind(_) => (1, -1),
        Op::Unbind(_) => (0, 0),
        Op::SaveCurrentBuffer | Op::SaveExcursion | Op::SaveRestriction => (0, 0),
        Op::UnwindProtectPop => (1, -1),
        other => return Err(CompileError::UnsupportedOp(op_category(other))),
    })
}

/// A statically tracked active handler: `(handler target instruction, operand
/// stack depth at the push)`. The list at any program point is the stack of
/// `PushConditionCase`/`PushCatch` frames not yet popped, outermost first.
type HandlerStatic = (usize, usize);

/// What kind of callee a speculation site validated at compile time — decides
/// which shim the armed site calls and how its lowering roots/falls back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpecCalleeKind {
    /// A BYTECODE object: `neovm_jit_call_spec` (epoch logic inside the shim,
    /// V3 native-to-native fast path, strict-symbol fallback inside the shim).
    Bytecode,
    /// A fixed-arity builtin SUBR: `neovm_jit_call_subr_spec` — armed direct
    /// dispatch with a FRESH per-call entry read; NOT-armed returns
    /// [`STATUS_NEED_GENERIC`] and the site's generated fallback block runs
    /// the plain generic call.
    SubrGeneral,
    /// `recordp` (1 arg): `neovm_jit_pred_spec`, pure tag test when armed.
    PredRecordp,
    /// `symbol-with-pos-p` (1 arg): `neovm_jit_pred_spec`, pure tag test.
    PredSymbolWithPos,
    /// `equal-including-properties` (2 args): `neovm_jit_eq_incl_props_spec`,
    /// bitwise-eq hit → `t`; anything else bounces to the generic block.
    EqInclProps,
    /// A bitwise arithmetic intrinsic — `logand`/`logior`/`logxor` (2 args):
    /// `neovm_jit_arith_spec`. When armed AND both args are fixnums, computes the
    /// native `&`/`|`/`^` in-register (bitwise ops on two fixnums always yield a
    /// fixnum — the interpreter's own 2-arg fast path, `builtin_logand_slice` et
    /// al.); anything else (marker/bignum/wrong-type/not-armed) bounces to the
    /// generic block. `op` is an `ARITH_KIND_*` discriminant, baked as an iconst
    /// into the shim call. Each op gets a DISTINCT [`to_spec_disc`] value so the
    /// AOT loader disarms a site whose baked op no longer matches the live callee.
    ArithIntrinsic { op: u8 },
    /// R2 CallBuiltinSym intrinsic, Tier-A (`which` = a `CBSYM_A_*`
    /// discriminant): a provably-trivial buffer/match-state read whose builtin
    /// body allocates no lisp — `neovm_jit_cbsym_read` calls the body GC-free,
    /// bouncing to [`STATUS_NEED_GENERIC`] when the static entry is no longer a
    /// plain builtin (or, for `current-buffer`, when the buffer value was never
    /// materialized). Classified BY NAME off `Op::CallBuiltinSym` (advice/fset
    /// immune — the op name-dispatches the static subr table).
    CbsymTierA { which: u8 },
    /// R2 CallBuiltinSym intrinsic, Tier-B (dispatch-skip): a plain builtin
    /// reached via `neovm_jit_cbsym_spec`, which reproduces the CBSym
    /// interpreter arm EXACTLY (fresh entry re-read, SUBR-value backtrace frame
    /// via `subr_from_sym_id`, arity vs the fresh entry, exact-slice args, NO
    /// `with_bytecode_call_depth`, quit-AFTER) — skipping only the
    /// `resolve_sym` → `builtin_name_id` → special-name string-switch round
    /// trip. Bounces to [`STATUS_NEED_GENERIC`] (→ the general CBSym lowering)
    /// when the fresh entry is not `Some` + `Builtin`.
    CbsymTierB,
}

/// Tier-A `which` discriminants (baked into generated code as an `iconst` and
/// read by [`neovm_jit_cbsym_read`]). Each maps 1:1 to a builtin whose body the
/// shim calls GC-free; a register-read reimplementation would diverge (e.g.
/// match-beginning does a byte→char conversion), so the shim always DELEGATES.
pub(crate) const CBSYM_A_POINT: u8 = 0;
pub(crate) const CBSYM_A_POINT_MIN: u8 = 1;
pub(crate) const CBSYM_A_POINT_MAX: u8 = 2;
pub(crate) const CBSYM_A_BOLP: u8 = 3;
pub(crate) const CBSYM_A_EOLP: u8 = 4;
pub(crate) const CBSYM_A_BOBP: u8 = 5;
pub(crate) const CBSYM_A_EOBP: u8 = 6;
pub(crate) const CBSYM_A_FOLLOWING_CHAR: u8 = 7;
pub(crate) const CBSYM_A_PRECEDING_CHAR: u8 = 8;
pub(crate) const CBSYM_A_CHAR_AFTER: u8 = 9;
pub(crate) const CBSYM_A_CURRENT_BUFFER: u8 = 10;
pub(crate) const CBSYM_A_MATCH_BEGINNING: u8 = 11;
pub(crate) const CBSYM_A_MATCH_END: u8 = 12;

impl SpecCalleeKind {
    /// Kinds whose direct path passes its 1–2 args in REGISTERS with no
    /// call-args spill and no residual rooting (the shims are GC-free by
    /// contract); their generated fallback block spills + roots itself.
    fn is_reg_args(self) -> bool {
        matches!(
            self,
            SpecCalleeKind::PredRecordp
                | SpecCalleeKind::PredSymbolWithPos
                | SpecCalleeKind::EqInclProps
                | SpecCalleeKind::ArithIntrinsic { .. }
        )
    }

    /// The three round-1 direct-SUBR speculation kinds that make
    /// `declare_rt_refs` pull in the JIT-only `neovm_jit_{call_subr,pred,eq_incl_props}_spec`
    /// shims. CBSym kinds are deliberately excluded — they declare their own
    /// shims off a separate flag so a CBSym-only body never imports the
    /// `Op::Call` spec shims.
    fn is_round1_subr(self) -> bool {
        matches!(
            self,
            SpecCalleeKind::SubrGeneral
                | SpecCalleeKind::PredRecordp
                | SpecCalleeKind::PredSymbolWithPos
                | SpecCalleeKind::EqInclProps
                | SpecCalleeKind::ArithIntrinsic { .. }
        )
    }

    /// The CallBuiltinSym intrinsic kinds (Tier-A read shims / Tier-B
    /// dispatch-skip). Keyed by the op's own SymId, resolved BY NAME.
    fn is_cbsym(self) -> bool {
        matches!(
            self,
            SpecCalleeKind::CbsymTierA { .. } | SpecCalleeKind::CbsymTierB
        )
    }

    /// R2 increment B2: the AOT descriptor discriminant for an `Op::Call` spec
    /// site's baked kind, or `None` for a CBSym kind (name-canonical, epochless —
    /// carries no per-site descriptor entry). The loader RE-CLASSIFIES the live
    /// binding and arms a site ONLY when the fresh `to_spec_disc()` matches the
    /// `kind_disc` baked here — so a re-aliased callee (e.g. `recordp` rebound to a
    /// non-`PredRecordp` subr) DISARMS instead of running the wrong baked op.
    ///
    /// EXHAUSTIVE match (no `_`): a new `SpecCalleeKind` variant is compile-forced
    /// to choose a discriminant (and bump [`DISC_COUNT`](Self::DISC_COUNT), salted
    /// into `ABI_TAG`) rather than silently defaulting.
    pub(crate) fn to_spec_disc(self) -> Option<u8> {
        match self {
            SpecCalleeKind::Bytecode => Some(0),
            SpecCalleeKind::SubrGeneral => Some(1),
            SpecCalleeKind::PredRecordp => Some(2),
            SpecCalleeKind::PredSymbolWithPos => Some(3),
            SpecCalleeKind::EqInclProps => Some(4),
            // A DISTINCT disc per bit-op (5=and, 6=ior, 7=xor). The op is baked as
            // an iconst into the shim call, so the AOT loader — which re-classifies
            // the LIVE callee and arms only on an exact disc match — must disarm a
            // site whose baked op differs from the live binding's op (e.g. a `logand`
            // site whose callee was re-aliased to `logior`). A shared disc would arm
            // it and run the wrong baked op; distinct discs make the mismatch disarm.
            SpecCalleeKind::ArithIntrinsic { op } => {
                debug_assert!(op <= 5, "ArithIntrinsic op discriminant out of range");
                Some(5 + op)
            }
            SpecCalleeKind::CbsymTierA { .. } | SpecCalleeKind::CbsymTierB => None,
        }
    }

    /// Number of distinct `Op::Call` spec discriminants [`to_spec_disc`](Self::to_spec_disc)
    /// assigns (0..DISC_COUNT). Salted into `ABI_TAG` so a renumber/count change
    /// re-tags stale `.so`s.
    pub(crate) const DISC_COUNT: u8 = 11;
}

/// A speculated direct-call site: an `Op::Call` whose callee slot provably
/// holds the constant symbol `sym`, fbound at compile time to the bytecode
/// object or fixed-arity builtin subr `expected_bits` (see `kind`). `slot`
/// indexes the leaf's armed-epoch slots.
#[derive(Clone, Copy)]
struct SpecSite {
    sym: u32,
    expected_bits: u64,
    slot: usize,
    kind: SpecCalleeKind,
}

/// R2 phase 2 — the NAME ALLOWLIST of `SubrFn::Many` builtins that an
/// `Op::Call(n)` site is permitted to speculate on (classified as
/// [`SpecCalleeKind::SubrGeneral`], reusing the round-1 subr shim). Chosen from
/// the real interactive-session profile's `Op::Call` HOT group (re-search-forward
/// 3.4%, parse-partial-sexp 4.65%, looking-at 1.78%, intern-soft 2.57%,
/// put-text-property 1.92%, match-data/set-match-data, scan-sexps, ...) — all
/// registered `SubrFn::Many`, which round-1's fixed-arity gate
/// (`subr_entry_uses_fixed_value_call`) excludes by construction.
///
/// `Op::Call(n)` carries the EXACT nargs, and the armed path passes that exact
/// slice to the Many subr (`call_spec_subr_stack` ->
/// `dispatch_builtin_subr_from_stack_args_unchecked`'s `Many` arm, exact-length
/// `.to_vec()` — NO nil-pad), so it is byte-identical to the generic/interpreter
/// dispatch of the same site.
///
/// Membership is `SubrFn::Many` ONLY, never `ManySlice`: the `ManySlice`
/// variadics (`+`/`logand`/`logior`/`logxor`/`list`/`vector`/`append`/`nconc`/
/// `string-match`, all `max=None`) are rejected by the `SubrFn::Many` match in
/// `subr_spec_kind`, independent of this list (asserted by
/// `subr_spec_kind_rejects_registered_manyslice`).
const SUBR_MANY_ALLOWLIST: &[&str] = &[
    "re-search-forward",
    "looking-at",
    "parse-partial-sexp",
    "match-data",
    "set-match-data",
    "scan-sexps",
    "intern-soft",
    "line-end-position",
    "syntax-table",
    "set-syntax-table",
    "put-text-property",
    // Residual-coverage audit (task A PART 2): the font-lock SUBR-MIX
    // (`vm_subr_mix_fontlock`) ranks `get-text-property` at 11.0% — the 6th
    // hottest builtin and the READ sibling of the already-allowlisted
    // `put-text-property` (11.6%), which #1 shipped WITHOUT its read pair. It is
    // a plain `SubrFn::Many` read (registered `min=2`, `max=Some(3)`, so the
    // Many arm caps `nargs<=3`), no writeback, no SWP-flag — so the exact-slice
    // Many dispatch is byte-identical to generic exactly as `put-text-property`'s
    // is (strictly SIMPLER, being a pure interval-tree read).
    "get-text-property",
];

/// Classify a compile-time function-cell binding for SUBR speculation at an
/// `Op::Call(nargs)` site on `site_sym`, or `None` when the site must stay on
/// the generic path. Every clause is load-bearing:
///
/// * `subr_entry_from_value` + `dispatch_kind == Builtin` — only plain
///   builtins; special forms / context-callables have different call
///   protocols. Checked again at run time (fresh entry read) since entries
///   are rewritten in place.
/// * `aset`/`fillarray` excluded on BOTH the site name and the resolved subr
///   name — their mutating-first-string-arg WRITEBACK protocol
///   (`Vm::mutates_first_arg_name` / `maybe_writeback_mutating_first_arg`)
///   wraps the generic call; the resolved-name check also covers
///   `(fset 'alias (symbol-function 'aset))` aliases, which
///   `writeback_mutating_callable_names` detects through the cell.
/// * `funcall`/`apply`/`eval` excluded (both names) — re-entrant drivers;
///   depth/backtrace conservatism (`eval` IS a fixed-arity A2).
///
/// Then one of two disjoint arms:
///
/// * FIXED-ARITY `A0..A8` (`subr_entry_uses_fixed_value_call`), the round-1
///   surface. `min_args <= nargs <= max_args`, else a call that must signal
///   wrong-number-of-arguments stays generic (byte-identical payload/frame).
///   `vectorp` stays General (correct, just not a tag test): bool-vectors and
///   sentinel char-tables are genuine `VecLikeType::Vector` objects that
///   `builtin_vectorp_1` distinguishes semantically — an inline tag test would
///   return `t` where the builtin returns `nil`. `keywordp`/`symbolp` stay
///   General because their builtins consult `symbols-with-pos-enabled`.
/// * ALLOWLISTED `SubrFn::Many` (R2 phase 2, [`SUBR_MANY_ALLOWLIST`]) — the
///   armed path dispatches the EXACT-length arg slice to the Many subr (no
///   nil-pad, so byte-identical to generic). `nargs >= min_args` always; when
///   `max_args` is `Some`, also `nargs <= max_args` (an over-arity call stays
///   generic for the identical signal). When `max_args` is `None`
///   (`put-text-property`, declared with arity `0..unbounded` and its real 4..5
///   range body-enforced in `textprop.rs`), permit any `nargs >= min_args`: the
///   body self-checks and spec ≡ generic both reach that same body check.
///   The site is CELL-DISPATCHED, so the round-1 guards (per-site epoch,
///   `compiler_function_overrides_active`, fresh-entry re-read in
///   `call_spec_subr_stack`) still deopt on advice/defalias/fset — unlike the
///   name-canonical `Op::CallBuiltinSym` path, which is override-immune.
fn subr_spec_kind(binding: Value, site_sym: SymId, nargs: usize) -> Option<SpecCalleeKind> {
    let (subr_sym, entry) = subr_entry_from_value(binding)?;
    if entry.dispatch_kind != SubrDispatchKind::Builtin {
        return None;
    }
    let site_name = resolve_sym(site_sym);
    let resolved_name = resolve_sym(subr_sym);
    if [site_name, resolved_name]
        .iter()
        .any(|name| matches!(*name, "aset" | "fillarray" | "funcall" | "apply" | "eval"))
    {
        return None;
    }
    // Bitwise-arith intrinsic (`logand`/`logior`/`logxor`, 2 args) — a GC-free
    // native `&`/`|`/`^` via `neovm_jit_arith_spec`. Classified HERE, before the
    // fixed-arity/`SubrFn::Many` branches, because these builtins are `ManySlice`
    // variadics that both branches reject (they otherwise get full generic
    // dispatch — the biggest, easiest win). Match on `resolved_name` (the subr the
    // cell points to) so an alias `(fset 'myand (symbol-function 'logand))` still
    // intrinsifies; the per-site epoch/expected-bits guard deopts on redefinition.
    if let Some(op) = arith_intrinsic_op_by_name(resolved_name, nargs) {
        return Some(SpecCalleeKind::ArithIntrinsic { op });
    }
    if Context::subr_entry_uses_fixed_value_call(entry) {
        if nargs < entry.min_args as usize {
            return None;
        }
        if entry.max_args.is_none_or(|max| nargs > max as usize) {
            return None;
        }
        Some(match (resolved_name, nargs) {
            ("recordp", 1) => SpecCalleeKind::PredRecordp,
            ("symbol-with-pos-p", 1) => SpecCalleeKind::PredSymbolWithPos,
            ("equal-including-properties", 2) => SpecCalleeKind::EqInclProps,
            _ => SpecCalleeKind::SubrGeneral,
        })
    } else if matches!(entry.function, Some(SubrFn::Many(_)))
        && SUBR_MANY_ALLOWLIST.contains(&resolved_name)
    {
        if nargs < entry.min_args as usize {
            return None;
        }
        // Some(max): enforce `nargs <= max`. None (`put-text-property`): the
        // body self-enforces its real range, so permit any `nargs >= min`.
        if entry.max_args.is_some_and(|max| nargs > max as usize) {
            return None;
        }
        Some(SpecCalleeKind::SubrGeneral)
    } else {
        None
    }
}

/// The `dispatch_vm_builtin_unrooted` special names (vm.rs) — VM-internal
/// bytecode operations that are NOT real Elisp subrs (they are handled by an
/// explicit string switch BEFORE `funcall_general`). A CBSym site whose name is
/// one of these must never be intrinsified: the fast shim funnels through
/// `funcall_general`, which would resolve the name-canonical static subr
/// entry — a DIFFERENT dispatch than the special-name arm. None of the R2 ship
/// set collides with these (asserted by `cbsym_shipset_excludes_special_names`
/// in the tests), but the classifier denylists them anyway for defence.
const CBSYM_SPECIAL_NAMES: &[&str] = &[
    "call-interactively",
    "start-kbd-macro",
    "end-kbd-macro",
    "call-last-kbd-macro",
    "execute-kbd-macro",
    "garbage-collect",
    "mapatoms",
    "maphash",
    "store-kbd-macro-event",
    "cancel-kbd-macro-events",
    "%%defvar",
    "%%defconst",
    "%%unimplemented-elc-bytecode",
];

/// Classify an `Op::CallBuiltinSym(sym, nargs)` site for R2 intrinsification, or
/// `None` when it must stay on the general named-builtin lowering. The
/// distinguishing property of CallBuiltinSym (vs the `Op::Call` sites
/// `subr_spec_kind` handles): the op carries the EXACT nargs and name-dispatches
/// the static subr table (`subr_from_sym_id(builtin_name_id(resolve_sym(sym)))`,
/// vm.rs) — it is advice/fset/override-IMMUNE, so there is NO epoch guard and NO
/// `compiler_function_overrides_active` gate; the target is name-canonical and
/// `Box::leak` process-stable. Every clause is load-bearing:
///
/// * `lookup_global_subr_entry(sym)` + `dispatch_kind == Builtin` — the op must
///   currently name a plain builtin. Re-checked FRESH in the shim (entries are
///   rewritten in place); a mismatch there bounces to `STATUS_NEED_GENERIC`.
/// * ALLOWLIST by name (the profiled R2 winners only) — so nothing outside the
///   audited ship set is ever intrinsified. This structurally excludes the
///   `aset`/`fillarray` writeback names, `funcall`/`apply`/`eval`, and every
///   `dispatch_vm_builtin_unrooted` special name (none are in the allowlist);
///   the explicit denylist below is defence-in-depth.
/// * NO fixed-arity gate (unlike `subr_spec_kind`): the Tier-B shim dispatches
///   through `funcall_general` on the exact-length arg vector, so a `Many` subr
///   gets `into_vec()` (byte-identical) and a wrong-arity call signals
///   `wrong-number-of-arguments` with the SUBR payload identically to the
///   interpreter arm — no need to force those to the generic path.
///
/// Obarray-FREE: consults only `lookup_global_subr_entry` (the static subr table)
/// and name resolution, so it classifies identically at JIT emit (`Some(obarray)`,
/// via `find_spec_sites`) AND AOT baseline emit (`obarray=None`, via
/// `find_cbsym_spec_sites` — increment A). The CallBuiltinSym op then takes the
/// Tier-A/B fast shim in BOTH tiers (its op-SymId is reloc'd by name under AOT).
fn cbsym_spec_kind(sym: SymId, _nargs: usize) -> Option<SpecCalleeKind> {
    let entry = lookup_global_subr_entry(sym)?;
    if entry.dispatch_kind != SubrDispatchKind::Builtin {
        return None;
    }
    let name = resolve_sym(sym);
    // Defence-in-depth denylist (the allowlist below already excludes these).
    if CBSYM_SPECIAL_NAMES.contains(&name)
        || matches!(name, "aset" | "fillarray" | "funcall" | "apply" | "eval")
    {
        return None;
    }
    // Tier-A: provably-trivial GC-free reads (COMMIT 5 shims). char-after is
    // Tier-A only in its 0-arg form; a 1-arg / marker call bounces to Tier-B's
    // generic path via the None fall-through here.
    let tier_a = match name {
        "point" => Some(CBSYM_A_POINT),
        "point-min" => Some(CBSYM_A_POINT_MIN),
        "point-max" => Some(CBSYM_A_POINT_MAX),
        "bolp" => Some(CBSYM_A_BOLP),
        "eolp" => Some(CBSYM_A_EOLP),
        "bobp" => Some(CBSYM_A_BOBP),
        "eobp" => Some(CBSYM_A_EOBP),
        "following-char" => Some(CBSYM_A_FOLLOWING_CHAR),
        "preceding-char" => Some(CBSYM_A_PRECEDING_CHAR),
        "char-after" if _nargs == 0 => Some(CBSYM_A_CHAR_AFTER),
        "current-buffer" => Some(CBSYM_A_CURRENT_BUFFER),
        "match-beginning" => Some(CBSYM_A_MATCH_BEGINNING),
        "match-end" => Some(CBSYM_A_MATCH_END),
        _ => None,
    };
    if let Some(which) = tier_a {
        return Some(SpecCalleeKind::CbsymTierA { which });
    }
    // Tier-B: plain builtins reached via `neovm_jit_cbsym_spec` (dispatch skip).
    if matches!(
        name,
        "length"
            | "insert"
            | "set-marker"
            | "goto-char"
            | "delete-region"
            | "forward-line"
            | "forward-char"
            | "buffer-substring"
            | "set-buffer"
            | "skip-chars-forward"
            | "skip-chars-backward"
            | "current-column"
            | "widen"
            | "indent-to"
            // Residual-coverage audit (task A PART 2): `end-of-line` is a
            // dedicated-opcode CBSym motion builtin (GNU op 127) at 2.37% in the
            // font-lock SUBR-MIX — the ONLY member of its own loop's motion set
            // (forward-line/forward-char/current-column, all already Tier-B) left
            // on the generic path. Point-moving side effect (not a pure read ->
            // NOT Tier-A); the Tier-B dispatch-skip reproduces the CBSym
            // interpreter arm exactly, byte-identical, as its siblings do.
            | "end-of-line"
    ) {
        return Some(SpecCalleeKind::CbsymTierB);
    }
    None
}

/// Per-site speculation state, baked into generated code by raw address and
/// read by `neovm_jit_call_spec`. `epoch` is the obarray `function_epoch` at
/// which this site's callee binding was last validated. `leaf` lazily caches a
/// `*const CompiledLeaf` (as `usize` bits; 0 = none) for the armed callee, so
/// repeat calls skip the compiled-cache hash lookup (the V3 fast path). The
/// leaf pointer is cleared whenever revalidation fails (the binding changed),
/// and is sound while set because the tagged-heap identity is stable during
/// native execution (so `cache::clear()` cannot fire mid-call to free the leaf —
/// see `resolve_compiled_leaf_ptr`; NOT "the cache never evicts", audit #1).
/// `repr(C)` pins the field order the baked pointer arithmetic relies on.
#[repr(C)]
pub(crate) struct SpecSlot {
    epoch: AtomicU64,
    leaf: AtomicU64,
}

/// Find direct-call speculation sites: the byte-compiler's standard call
/// shape `Constant(f) arg-push* Call(n)` where every op between the callee
/// push and its call only PUSHES new slots (Constant/Nil/True/Dup/StackRef —
/// the callee slot can't be rewritten), no jump target lands inside the
/// window, and `f` is currently fbound to either
///
/// * a BYTECODE object ([`SpecCalleeKind::Bytecode`]) — an epoch-equal check
///   on a bytecode binding proves it still names the same immutable bytecode
///   object; or
/// * a FIXED-ARITY builtin SUBR passing [`subr_spec_kind`]'s constraints
///   (Gap 1) — subr VALUE bits are stable (`Box::leak`'d, rewritten in place
///   by `update_static_subr_object_entry` keeping bits identical, and
///   `Context::install_subr` path bumps `function_epoch` on every rewrite), so the
///   same epoch/bits validation applies; the armed shims re-read the ENTRY
///   fresh per call precisely because of those in-place rewrites.
///
/// This full pass runs ONLY under `Some(obarray)` (see `lower_leaf_full`), so its
/// `Op::Call` SUBR speculation auto-excludes AOT — no AOT object bakes subr-spec
/// slot state or references the round-1/`Op::Call` spec shims (that is increment
/// B). The CBSym pass it ends with ([`append_cbsym_spec_sites`]) is obarray-FREE,
/// and the AOT baseline emit runs it separately ([`find_cbsym_spec_sites`]), so
/// AOT bodies DO reference the CBSym shims (increment A).
fn find_spec_sites(
    ops: &[Op],
    constants: &[Value],
    leaders: &[usize],
    obarray: &Obarray,
) -> HashMap<usize, SpecSite> {
    let mut sites = HashMap::new();
    let mut next_slot = 0usize;
    // Block-local abstract operand stack: a SUFFIX of the real stack where a
    // tracked slot is `Some(const-idx)` iff it provably holds that SYMBOL
    // constant (pushed by `Op::Constant` in this block, copied only through
    // the stack-shuffling ops modeled below). An `Op::Call(n)` whose callee
    // slot (n below the top) carries a tag becomes a speculation site — this
    // generalizes the old scan, which required every argument between the
    // callee push and the call to be a TRIVIAL push and therefore rejected
    // any call with a computed argument (e.g. a self-recursive
    // `(fib (- n 1))`, whose `Op::Sub` disqualified the site and pinned every
    // recursive call to the generic shim).
    //
    // Soundness split: this pass SELECTS sites; the `Op::Call` lowering only
    // emits the spec shim after independently PROVING (SSA inspection,
    // `callee_is_symbol_const`) that the callee slot's value is the site's
    // symbol constant — so a divergence between this model and the lowering
    // degrades to the generic call, never a wrong-callee speculation.
    //
    // Suffix discipline: cleared at every block leader (unknown entry stack)
    // and on any unmodeled op; pops past the suffix bottom just empty it (the
    // values below were already unknown) and later pushes re-grow it, so
    // top-relative reads inside the suffix stay exact.
    let mut tags: Vec<Option<u16>> = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        if leaders.binary_search(&i).is_ok() {
            tags.clear();
        }
        match op {
            Op::Constant(cidx) => {
                let tag = constants
                    .get(*cidx as usize)
                    .and_then(|v| v.as_symbol_id())
                    .map(|_| *cidx);
                tags.push(tag);
            }
            Op::Nil | Op::True => tags.push(None),
            Op::Dup => {
                let t = tags.last().copied().flatten();
                tags.push(t);
            }
            Op::StackRef(n) => {
                let n = *n as usize;
                let t = if tags.len() > n {
                    tags[tags.len() - 1 - n]
                } else {
                    None
                };
                tags.push(t);
            }
            Op::StackSet(n) => {
                // Interpreter semantics: the top value moves into the slot `n`
                // below it, then the top is dropped (n == 0 is a plain pop).
                let n = *n as usize;
                let t = tags.pop().flatten();
                if n > 0 && tags.len() >= n {
                    let d = tags.len() - n;
                    tags[d] = t;
                }
            }
            Op::DiscardN(raw) => {
                let preserve_tos = (raw & 0x80) != 0;
                let n = (raw & 0x7F) as usize;
                if preserve_tos && n > 0 {
                    // The top survives, landing n slots lower.
                    let t = tags.pop().flatten();
                    for _ in 0..n.min(tags.len()) {
                        tags.pop();
                    }
                    tags.push(t);
                } else {
                    for _ in 0..n.min(tags.len()) {
                        tags.pop();
                    }
                }
            }
            op @ (Op::Call(n) | Op::Apply(n)) => {
                let nargs = *n as usize;
                // Site check BEFORE applying the call's stack effect: the
                // callee sits nargs below the top (Apply never speculates).
                if matches!(op, Op::Call(_))
                    && tags.len() > nargs
                    && let Some(cidx) = tags[tags.len() - 1 - nargs]
                    && let Some(sym_val) = constants.get(cidx as usize)
                    && let Some(sym_id) = sym_val.as_symbol_id()
                    && let Some(binding) = obarray.symbol_function_id(sym_id)
                {
                    let kind = if binding.is_bytecode() {
                        Some(SpecCalleeKind::Bytecode)
                    } else {
                        subr_spec_kind(binding, sym_id, nargs)
                    };
                    if let Some(kind) = kind {
                        sites.insert(
                            i,
                            SpecSite {
                                sym: sym_id.0,
                                expected_bits: binding.bits() as u64,
                                slot: next_slot,
                                kind,
                            },
                        );
                        next_slot += 1;
                    }
                }
                // [func a1 .. aN] -> [result]
                for _ in 0..(nargs + 1).min(tags.len()) {
                    tags.pop();
                }
                tags.push(None);
            }
            other => match simple_effect(other) {
                Ok((needs, delta)) => {
                    // For every op reaching this arm, `needs` IS the consumed
                    // operand count and `needs + delta` the produced count.
                    // The ops where that identity does NOT hold — the
                    // stack-shuffling Dup/StackRef/StackSet/DiscardN (reads
                    // without consuming / writes in place) — are all handled
                    // explicitly above, as are Constant/Nil/True/Call/Apply.
                    let consumed = needs;
                    let produced = (needs as i64 + delta).max(0) as usize;
                    for _ in 0..consumed.min(tags.len()) {
                        tags.pop();
                    }
                    for _ in 0..produced {
                        tags.push(None);
                    }
                }
                // Control flow / anything unmodeled: forget everything.
                Err(_) => tags.clear(),
            },
        }
    }
    // R2: CallBuiltinSym intrinsic sites (self-contained per-op, classified by
    // NAME — obarray-free). Runs for BOTH JIT (here) and AOT baseline emit (via
    // `find_cbsym_spec_sites`, increment A); continues the slot numbering from the
    // `Op::Call` pass above so the JIT map is byte-identical.
    append_cbsym_spec_sites(ops, &mut sites, &mut next_slot);
    sites
}

/// The CallBuiltinSym intrinsic classification pass of [`find_spec_sites`], split
/// out so the AOT baseline emit (`obarray=None`) can run it WITHOUT the `Op::Call`
/// subr pass (increment B). A `CallBuiltinSym` op is self-contained (it carries
/// its callee SymId + the EXACT nargs), so there is no callee-slot-rewrite window
/// to validate and no jump-target hazard — classify in place by NAME.
/// [`cbsym_spec_kind`] consults ONLY the static subr table + name resolution (NO
/// obarray), so it classifies identically at JIT emit (`Some(obarray)`) and AOT
/// emit (`None`). Sites are keyed at the op's own index (disjoint from the
/// `Op::Call` indices: an index is one op) and numbered from `*next_slot`.
///
/// CBSym is SLOTLESS/EPOCHLESS: a site carries a `slot` index for uniformity with
/// the `Op::Call` sites, but its lowering (`Op::CallBuiltinSym` arm) reads ONLY
/// the site's `kind` (Tier-A `which` / Tier-B) — it never bakes the slot pointer
/// or `expected_bits`. So an AOT body needs no per-site descriptor entry.
fn append_cbsym_spec_sites(
    ops: &[Op],
    sites: &mut HashMap<usize, SpecSite>,
    next_slot: &mut usize,
) {
    for (i, op) in ops.iter().enumerate() {
        let Op::CallBuiltinSym(sym, n) = op else {
            continue;
        };
        let Some(kind) = cbsym_spec_kind(*sym, *n as usize) else {
            continue;
        };
        sites.insert(
            i,
            SpecSite {
                sym: sym.0,
                // Unused for CBSym: the target is name-canonical (no epoch
                // guard); the shim recomputes `subr_from_sym_id(sym)` itself.
                expected_bits: 0,
                slot: *next_slot,
                kind,
            },
        );
        *next_slot += 1;
    }
}

/// R2 increment A: the CBSym-only speculation-site map for the AOT baseline emit
/// (`obarray=None`). The `Op::Call` subr speculation stays JIT-only (needs a live
/// obarray binding — increment B), but CBSym is name-canonical + obarray-free, so
/// AOT bodies get the Tier-A/B fast shims too. Slots number from 0; the sites'
/// slot pointers are never baked (CBSym is slotless — see [`append_cbsym_spec_sites`]).
fn find_cbsym_spec_sites(ops: &[Op]) -> HashMap<usize, SpecSite> {
    let mut sites = HashMap::new();
    let mut next_slot = 0usize;
    append_cbsym_spec_sites(ops, &mut sites, &mut next_slot);
    sites
}

/// R2 increment B2 tier-pivot: does this body have ≥1 `Op::Call` subr/bytecode
/// speculation site against the LIVE `obarray`? Used by the AOT EMIT path
/// (`compile_leaf_to_object` / `build_preload_object`) to force a spec-bearing body
/// to the BASELINE tier (only it bakes the `Op::Call` spec fast paths) AND by the
/// LOAD path (`live_reloc_for_emit_tier`) to pick the matching baseline reloc
/// collector — so both agree and the leaf actually serves. Obarray-classified, so a
/// callee redefined away from a spec kind between dump + load simply yields `false`
/// (the recipe-compare / re-classify then keep it sound — bail-to-JIT or DISARM).
pub(crate) fn has_op_call_spec_sites(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    obarray: &Obarray,
) -> bool {
    let Ok(cfg) = analyze_cfg(ops, constants, None, arity) else {
        return false;
    };
    find_spec_sites(ops, constants, &cfg.leaders, obarray)
        .values()
        .any(|s| s.kind.to_spec_disc().is_some())
}

/// R2 increment B2: finalize a baseline leaf's speculation-site map for AOT emit —
/// DROP any `Op::Call` spec site whose callee symbol isn't in the reloc vector
/// (`materialize_op_sym_id` would otherwise bake a session SymId; the site then
/// falls to the generic `None` arm — the leaf still emits, never bails), RENUMBER
/// the surviving slots densely (`Op::Call` spec sites 0..K, then the slotless CBSym
/// sites K..N) so the loader's descriptor position IS the codegen slot index, and
/// return the surviving `Op::Call` sites as [`AotSpecSite`]s in slot order.
///
/// (In practice a callee push is always the `Op::Constant` that
/// `collect_baseline_aot_relocs` collected, so the drop path is defence-in-depth.)
fn finalize_baseline_spec_sites(
    spec_sites: &mut HashMap<usize, SpecSite>,
    ops: &[Op],
    reloc_index: &std::collections::HashMap<usize, u32>,
) -> Vec<super::aot::AotSpecSite> {
    // Snapshot (op_idx, site) in the original slot order (find_spec_sites numbers
    // Op::Call sites first, then CBSym), then rebuild the map with dense slots.
    let mut ordered: Vec<(usize, SpecSite)> = spec_sites.iter().map(|(&op, s)| (op, *s)).collect();
    ordered.sort_by_key(|(_, s)| s.slot);
    spec_sites.clear();
    let mut aot_sites: Vec<super::aot::AotSpecSite> = Vec::new();
    let mut next_slot = 0usize;
    // Pass 1: the Op::Call subr/bytecode spec sites (own the descriptor entries).
    for (op_idx, mut site) in ordered
        .iter()
        .copied()
        .filter(|(_, s)| s.kind.to_spec_disc().is_some())
    {
        let key = (site.sym as usize) << TAG_BITS | TAG_SYMBOL;
        let Some(&callee_reloc_idx) = reloc_index.get(&key) else {
            continue; // DROP — un-reloc'd callee → generic None arm; do NOT bail.
        };
        let nargs = match ops.get(op_idx) {
            Some(Op::Call(n)) => *n,
            _ => continue, // spec sites are keyed at an Op::Call; be defensive.
        };
        let kind_disc = site
            .kind
            .to_spec_disc()
            .expect("filtered to an Op::Call spec disc");
        site.slot = next_slot;
        next_slot += 1;
        spec_sites.insert(op_idx, site);
        aot_sites.push(super::aot::AotSpecSite {
            kind_disc,
            which: 0,
            nargs,
            callee_reloc_idx,
        });
    }
    // Pass 2: the slotless CBSym sites (no descriptor entry), renumbered after.
    for (op_idx, mut site) in ordered
        .iter()
        .copied()
        .filter(|(_, s)| s.kind.to_spec_disc().is_none())
    {
        site.slot = next_slot;
        next_slot += 1;
        spec_sites.insert(op_idx, site);
    }
    aot_sites
}

/// Resolve the static target set of the `Op::Switch` at `i`: the byte
/// compiler always pushes the jump table as a constant immediately before the
/// switch, so require `ops[i-1]` to be that `Constant`, the constant to be a
/// hash table, and every table value to be a fixnum address resolving (through
/// the GNU byte-offset map when present) to an in-range instruction index.
/// Returns deduplicated `(raw address, instruction index)` pairs; anything
/// else bails to the interpreter.
fn switch_static_targets(
    ops: &[Op],
    constants: &[Value],
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    i: usize,
) -> Result<Vec<(i64, usize)>, CompileError> {
    let table = match i.checked_sub(1).map(|p| &ops[p]) {
        Some(Op::Constant(idx)) => constants
            .get(*idx as usize)
            .ok_or(CompileError::BadOperand)?,
        _ => return Err(CompileError::UnsupportedOp("switch-dynamic")),
    };
    let Some(ht) = table.as_hash_table() else {
        return Err(CompileError::UnsupportedOp("switch-dynamic"));
    };
    let mut out: Vec<(i64, usize)> = Vec::with_capacity(ht.data.len());
    for v in ht.data.values() {
        let ValueKind::Fixnum(raw) = v.kind() else {
            return Err(CompileError::UnsupportedOp("switch-dynamic"));
        };
        let raw_addr = usize::try_from(raw).map_err(|_| CompileError::BadOperand)?;
        let target = match offset_map {
            Some(map) => map
                .binary_search_by_key(&raw_addr, |e| e.byte_offset)
                .map(|k| map[k].instruction_index)
                .map_err(|_| CompileError::BadOperand)?,
            None => raw_addr,
        };
        if target >= ops.len() {
            return Err(CompileError::BadOperand);
        }
        if !out.iter().any(|&(r, _)| r == raw) {
            out.push((raw, target));
        }
    }
    Ok(out)
}

/// Basic-block analysis: sorted block leaders, the operand-stack depth at each
/// block's entry, the active-handler stack at each block's entry, the resolved
/// static target sets of every `Op::Switch`, and the max depth seen at any
/// block boundary.
pub(crate) struct Cfg {
    pub(crate) leaders: Vec<usize>,
    pub(crate) entry_depth: HashMap<usize, usize>,
    pub(crate) entry_handlers: HashMap<usize, Vec<HandlerStatic>>,
    pub(crate) switch_targets: HashMap<usize, Vec<(i64, usize)>>,
    pub(crate) max_depth: usize,
}

/// Record that `target` is entered with stack depth `d`, outstanding dynamic
/// bind count `binds`, and active handler stack `handlers`, scheduling it for
/// analysis on first sight. Depth, bind count, and handler stack must be
/// non-negative and consistent across all paths (the byte-compiler guarantees
/// a single static value per program point), so each block is analyzed once.
#[allow(clippy::too_many_arguments)] // dataflow successor state remains split for in-place updates
fn push_succ(
    entry_depth: &mut HashMap<usize, usize>,
    entry_binds: &mut HashMap<usize, usize>,
    entry_handlers: &mut HashMap<usize, Vec<HandlerStatic>>,
    work: &mut Vec<usize>,
    target: usize,
    d: i64,
    binds: usize,
    handlers: &[HandlerStatic],
) -> Result<(), CompileError> {
    if d < 0 {
        return Err(CompileError::StackUnderflow);
    }
    let d = d as usize;
    match entry_depth.get(&target) {
        Some(&existing) if existing != d => {
            Err(CompileError::UnsupportedOp("inconsistent stack depth"))
        }
        Some(_) => {
            if entry_binds.get(&target).copied().unwrap_or(0) != binds {
                return Err(CompileError::UnsupportedOp("inconsistent bind depth"));
            }
            if entry_handlers
                .get(&target)
                .is_none_or(|existing| existing != handlers)
            {
                return Err(CompileError::UnsupportedOp("inconsistent handler stack"));
            }
            Ok(())
        }
        None => {
            entry_depth.insert(target, d);
            entry_binds.insert(target, binds);
            entry_handlers.insert(target, handlers.to_vec());
            work.push(target);
            Ok(())
        }
    }
}

/// Partition `ops` into basic blocks and compute the operand-stack depth at each
/// block boundary, validating that every op is supported, jump targets are in
/// range, depth never underflows, and every path ends in `Return`.
pub(crate) fn analyze_cfg(
    ops: &[Op],
    constants: &[Value],
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    arity: usize,
) -> Result<Cfg, CompileError> {
    let n = ops.len();
    if n == 0 {
        return Err(CompileError::NoReturn);
    }

    // 1. Block leaders: index 0, every jump target, and every index following a
    //    branch/goto/return.
    let mut leader_set: BTreeSet<usize> = BTreeSet::new();
    let mut switch_targets: HashMap<usize, Vec<(i64, usize)>> = HashMap::new();
    leader_set.insert(0);
    for (i, op) in ops.iter().enumerate() {
        match op {
            Op::Switch => {
                // Resolve the static target set now (bails for non-constant
                // tables); every target is a leader, plus the miss
                // fall-through.
                let targets = switch_static_targets(ops, constants, offset_map, i)?;
                for &(_, t) in &targets {
                    leader_set.insert(t);
                }
                if i + 1 < n {
                    leader_set.insert(i + 1);
                }
                switch_targets.insert(i, targets);
            }
            Op::Goto(t)
            | Op::GotoIfNil(t)
            | Op::GotoIfNotNil(t)
            | Op::GotoIfNilElsePop(t)
            | Op::GotoIfNotNilElsePop(t) => {
                let t = *t as usize;
                if t >= n {
                    return Err(CompileError::BadOperand);
                }
                leader_set.insert(t);
                if i + 1 < n {
                    leader_set.insert(i + 1);
                }
            }
            // Handler pushes end their block (the lowering emits an anchor
            // edge to the handler target) and make the target a leader.
            Op::PushConditionCase(t) | Op::PushConditionCaseRaw(t) | Op::PushCatch(t) => {
                let t = *t as usize;
                if t >= n {
                    return Err(CompileError::BadOperand);
                }
                leader_set.insert(t);
                if i + 1 < n {
                    leader_set.insert(i + 1);
                }
            }
            Op::Return | Op::Throw if i + 1 < n => {
                leader_set.insert(i + 1);
            }
            _ => {}
        }
    }
    let leaders: Vec<usize> = leader_set.into_iter().collect();
    let next_leader = |idx: usize| leaders.iter().copied().find(|&l| l > idx).unwrap_or(n);

    // 2. Propagate entry depths over the CFG (worklist). Guards deopt at a
    // PRECISE pc (the interpreter resumes mid-function with the live state),
    // so side effects before a guard are fine — no poisoning dimension.
    let mut entry_depth: HashMap<usize, usize> = HashMap::new();
    let mut entry_binds: HashMap<usize, usize> = HashMap::new();
    let mut entry_handlers: HashMap<usize, Vec<HandlerStatic>> = HashMap::new();
    entry_depth.insert(0, arity);
    entry_binds.insert(0, 0);
    entry_handlers.insert(0, Vec::new());
    let mut work = vec![0usize];
    let mut max_depth = arity;

    while let Some(l) = work.pop() {
        let mut cur = entry_depth[&l] as i64;
        let mut binds = entry_binds.get(&l).copied().unwrap_or(0);
        let mut handlers = entry_handlers.get(&l).cloned().unwrap_or_default();
        let end = next_leader(l);
        let mut terminated = false;
        for op in &ops[l..end] {
            // The terminator (if any) is always the last op of the block.
            match op {
                Op::Return => {
                    if cur < 1 {
                        return Err(CompileError::StackUnderflow);
                    }
                    // Returning with outstanding binds is fine: the frame
                    // unwind in CompiledLeaf::call unbinds to the entry base,
                    // exactly like cleanup_bytecode_frame.
                    terminated = true;
                    break;
                }
                Op::Throw => {
                    // [tag value] -> non-local exit; a terminator for compiled
                    // code (no local handlers exist — handler opcodes bail).
                    if cur < 2 {
                        return Err(CompileError::StackUnderflow);
                    }
                    terminated = true;
                    break;
                }
                Op::Goto(t) => {
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        *t as usize,
                        cur,
                        binds,
                        &handlers,
                    )?;
                    terminated = true;
                    break;
                }
                Op::GotoIfNil(t) | Op::GotoIfNotNil(t) => {
                    if cur < 1 {
                        return Err(CompileError::StackUnderflow);
                    }
                    cur -= 1; // pop the condition
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        *t as usize,
                        cur,
                        binds,
                        &handlers,
                    )?;
                    // fall-through
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        end,
                        cur,
                        binds,
                        &handlers,
                    )?;
                    terminated = true;
                    break;
                }
                Op::GotoIfNilElsePop(t) | Op::GotoIfNotNilElsePop(t) => {
                    if cur < 1 {
                        return Err(CompileError::StackUnderflow);
                    }
                    // The jump preserves TOS (depth cur); the fall-through pops it.
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        *t as usize,
                        cur,
                        binds,
                        &handlers,
                    )?;
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        end,
                        cur - 1,
                        binds,
                        &handlers,
                    )?;
                    terminated = true;
                    break;
                }
                Op::Switch => {
                    // [dispatch table] -> jump to a static target or fall
                    // through on a miss. The target set was resolved in the
                    // leader pass.
                    if end >= n {
                        return Err(CompileError::NoReturn);
                    }
                    if cur < 2 {
                        return Err(CompileError::StackUnderflow);
                    }
                    cur -= 2;
                    // The Switch is the last op of its block (pass 1 made the
                    // following index a leader), so `end` is the fall-through.
                    let i = end - 1;
                    for &(_, t) in switch_targets.get(&i).expect("resolved in pass 1") {
                        push_succ(
                            &mut entry_depth,
                            &mut entry_binds,
                            &mut entry_handlers,
                            &mut work,
                            t,
                            cur,
                            binds,
                            &handlers,
                        )?;
                    }
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        end,
                        cur,
                        binds,
                        &handlers,
                    )?;
                    terminated = true;
                    break;
                }
                Op::PushConditionCase(t) | Op::PushConditionCaseRaw(t) | Op::PushCatch(t) => {
                    if end >= n {
                        // A push as the final op would fall off the end.
                        return Err(CompileError::NoReturn);
                    }
                    // Raw/Catch consume the conditions/tag operand first.
                    if !matches!(op, Op::PushConditionCase(_)) {
                        if cur < 1 {
                            return Err(CompileError::StackUnderflow);
                        }
                        cur -= 1;
                    }
                    // Handler edge: entered with the push-time stack plus the
                    // error value, the handler stack as of BEFORE this push
                    // (the matched frame and everything above it were popped
                    // by the unwind), and the push-time bind count (the catch
                    // restored the specpdl/bind state).
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        *t as usize,
                        cur + 1,
                        binds,
                        &handlers,
                    )?;
                    if cur as usize + 1 > max_depth {
                        max_depth = cur as usize + 1;
                    }
                    handlers.push((*t as usize, cur as usize));
                    // Fall-through edge: same stack, handler now active.
                    push_succ(
                        &mut entry_depth,
                        &mut entry_binds,
                        &mut entry_handlers,
                        &mut work,
                        end,
                        cur,
                        binds,
                        &handlers,
                    )?;
                    terminated = true;
                    break;
                }
                Op::PopHandler => {
                    // Normal exit from a protected extent: drop the innermost
                    // static handler. No stack effect; non-poisoning (the pop
                    // is a silent registration change — a deopt-rerun re-pushes
                    // and re-pops it after the frame unwind truncated ours).
                    if handlers.pop().is_none() {
                        return Err(CompileError::UnsupportedOp("unbalanced-pophandler"));
                    }
                }
                other => {
                    let (needs, delta) = simple_effect(other)?;
                    if cur < needs as i64 {
                        return Err(CompileError::StackUnderflow);
                    }
                    cur += delta;
                    if cur as usize > max_depth {
                        max_depth = cur as usize;
                    }
                    match other {
                        Op::VarBind(_)
                        | Op::SaveCurrentBuffer
                        | Op::SaveExcursion
                        | Op::SaveRestriction
                        | Op::UnwindProtectPop => binds += 1,
                        Op::Unbind(un) => {
                            let un = *un as usize;
                            if un > binds {
                                // Unbinding more than this function bound —
                                // bail to the interpreter (its bind_stack
                                // saturation handles it).
                                return Err(CompileError::UnsupportedOp("unbalanced-unbind"));
                            }
                            binds -= un;
                        }
                        _ => {}
                    }
                }
            }
        }
        if !terminated {
            // Block falls through into the next leader (guaranteed to exist and
            // be < n; a block running off the end with no Return is invalid).
            if end >= n {
                return Err(CompileError::NoReturn);
            }
            push_succ(
                &mut entry_depth,
                &mut entry_binds,
                &mut entry_handlers,
                &mut work,
                end,
                cur,
                binds,
                &handlers,
            )?;
        }
    }

    for &d in entry_depth.values() {
        max_depth = max_depth.max(d);
    }
    Ok(Cfg {
        leaders,
        entry_depth,
        entry_handlers,
        switch_targets,
        max_depth,
    })
}

/// Apply one non-terminator op's effect to the known-fixnum operand-stack model
/// `k` (parallel to the real operand stack: `k[i]` is `true` iff position `i` is
/// PROVABLY a fixnum). Returns `Err(())` for any op this analysis does not model
/// precisely, so the caller bails the whole function (conservative — no guard is
/// elided). Fixnum constants and fixnum arithmetic results are `true`;
/// StackRef/Dup/StackSet/DiscardN move bits; everything else is `false`.
fn apply_known_fixnum_op(op: &Op, constants: &[Value], k: &mut Vec<bool>) -> Result<(), ()> {
    match op {
        Op::Constant(idx) => {
            let is_fix = constants
                .get(*idx as usize)
                .map(|v| (v.bits() & FIXNUM_CHECK_MASK) == FIXNUM_CHECK_VALUE)
                .unwrap_or(false);
            k.push(is_fix);
        }
        Op::Nil | Op::True => k.push(false),
        Op::StackRef(j) => {
            let n = *j as usize;
            let v = *k.get(k.len().checked_sub(1 + n).ok_or(())?).ok_or(())?;
            k.push(v);
        }
        Op::Dup => {
            let v = *k.last().ok_or(())?;
            k.push(v);
        }
        Op::StackSet(j) => {
            let n = *j as usize;
            let v = k.pop().ok_or(())?;
            if n >= 1 {
                let idx = k.len().checked_sub(n).ok_or(())?;
                k[idx] = v;
            }
        }
        Op::Pop => {
            k.pop().ok_or(())?;
        }
        Op::DiscardN(raw) => {
            let n = (*raw & 0x7F) as usize;
            let preserve = (*raw & 0x80) != 0 && n > 0;
            if preserve {
                let tos = k.pop().ok_or(())?;
                let keep = k.len().checked_sub(n).ok_or(())?;
                k.truncate(keep);
                k.push(tos);
            } else {
                let keep = k.len().checked_sub(n).ok_or(())?;
                k.truncate(keep);
            }
        }
        // Fixnum arithmetic: the result is range-checked + retagged -> fixnum.
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Rem | Op::Max | Op::Min => {
            k.pop().ok_or(())?;
            k.pop().ok_or(())?;
            k.push(true);
        }
        Op::Add1 | Op::Sub1 | Op::Negate => {
            k.pop().ok_or(())?;
            k.push(true);
        }
        // Operand-consuming ops whose result is NOT a known fixnum: pop `needs`
        // (== the operands consumed for these) and push the results as unknown.
        // `simple_effect` is authoritative for the depth change.
        Op::Eq
        | Op::Eqlsign
        | Op::Lss
        | Op::Gtr
        | Op::Leq
        | Op::Geq
        | Op::Null
        | Op::Not
        | Op::Consp
        | Op::Stringp
        | Op::Listp
        | Op::Symbolp
        | Op::Integerp
        | Op::Numberp
        | Op::Car
        | Op::Cdr
        | Op::CarSafe
        | Op::CdrSafe
        | Op::Cons
        | Op::Aset
        | Op::List(_)
        | Op::Call(_)
        | Op::Apply(_)
        | Op::CallBuiltin(..)
        | Op::CallBuiltinSym(..)
        | Op::VarRef(_)
        | Op::VarSet(_)
        | Op::VarBind(_)
        | Op::Unbind(_)
        | Op::UnwindProtectPop
        | Op::SaveCurrentBuffer
        | Op::SaveExcursion
        | Op::SaveRestriction => {
            let (needs, delta) = simple_effect(op).map_err(|_| ())?;
            // These ops (unlike StackRef/Dup/StackSet/DiscardN) consume exactly
            // their top `needs` operands, so popping `needs` keeps `k` aligned.
            for _ in 0..needs {
                k.pop().ok_or(())?;
            }
            let pushes = needs as i64 + delta;
            for _ in 0..pushes.max(0) {
                k.push(false);
            }
        }
        // Direct/slice builtin dispatch (e.g. `1+`-as-subr): pop nargs, push one
        // unknown result. Detected the same way `simple_effect` does.
        other if direct_builtin_spec(other).is_some() || slice_builtin_spec(other).is_some() => {
            let (needs, delta) = simple_effect(other).map_err(|_| ())?;
            for _ in 0..needs {
                k.pop().ok_or(())?;
            }
            let pushes = needs as i64 + delta;
            for _ in 0..pushes.max(0) {
                k.push(false);
            }
        }
        // Anything else (Switch/handler ops/unmodeled) -> bail.
        _ => return Err(()),
    }
    Ok(())
}

/// **Cross-block redundant-guard elimination — the analysis.** Wired into
/// `lower_leaf_full` (and disabled under OSR); the "UNWIRED" this doc once
/// carried was stale.
///
/// Forward dataflow fixpoint over the CFG: for each block leader, the operand-
/// stack SLOTS provably fixnum at block entry. A slot is known-fixnum at entry
/// iff it is known-fixnum on EVERY predecessor edge (meet = AND); loops need the
/// fixpoint (the back-edge induction value depends on the slot's own bit). It is
/// a MUST analysis, so non-entry blocks start at TOP (all-`true`) and are
/// narrowed by predecessors; the entry block starts all-`false` (args untyped).
///
/// Conservative: returns an EMPTY map (no elision anywhere) for any function
/// containing an op this analysis does not model precisely (Switch, catch/
/// condition-case handlers, ...). NOT yet wired into `lower_leaf_full`; the
/// integration that consumes this is a follow-up. `cfg` must come from
/// [`analyze_cfg`] on the same `ops`.
fn compute_known_fixnum_slots(
    ops: &[Op],
    constants: &[Value],
    cfg: &Cfg,
) -> HashMap<usize, Vec<bool>> {
    let n = ops.len();
    let next_leader = |idx: usize| cfg.leaders.iter().copied().find(|&l| l > idx).unwrap_or(n);
    let empty = HashMap::new();

    // in[leader] = known-fixnum bits at block entry. Entry (0) is all-false;
    // every other block starts at TOP for the AND fixpoint.
    let mut in_sets: HashMap<usize, Vec<bool>> = HashMap::new();
    for &l in &cfg.leaders {
        let d = cfg.entry_depth.get(&l).copied().unwrap_or(0);
        in_sets.insert(l, vec![l != 0; d]);
    }

    // AND a predecessor contribution into a successor's in-set; report narrowing.
    fn meet(into: &mut [bool], contrib: &[bool]) -> bool {
        let mut changed = false;
        for (slot, &c) in into.iter_mut().zip(contrib.iter()) {
            if *slot && !c {
                *slot = false;
                changed = true;
            }
        }
        changed
    }

    let mut iterate = true;
    while iterate {
        iterate = false;
        for &l in &cfg.leaders {
            let mut k = in_sets[&l].clone();
            let end = next_leader(l);
            let mut edges: Vec<(usize, Vec<bool>)> = Vec::new();
            let mut terminated = false;
            for op in &ops[l..end] {
                match op {
                    Op::Return | Op::Throw => {
                        terminated = true;
                        break;
                    }
                    Op::Goto(t) => {
                        edges.push((*t as usize, k.clone()));
                        terminated = true;
                        break;
                    }
                    Op::GotoIfNil(t) | Op::GotoIfNotNil(t) => {
                        if k.pop().is_none() {
                            return empty;
                        }
                        edges.push((*t as usize, k.clone()));
                        edges.push((end, k.clone()));
                        terminated = true;
                        break;
                    }
                    Op::GotoIfNilElsePop(t) | Op::GotoIfNotNilElsePop(t) => {
                        // The jump preserves TOS; the fall-through pops it.
                        edges.push((*t as usize, k.clone()));
                        let mut ft = k.clone();
                        if ft.pop().is_none() {
                            return empty;
                        }
                        edges.push((end, ft));
                        terminated = true;
                        break;
                    }
                    other => {
                        if apply_known_fixnum_op(other, constants, &mut k).is_err() {
                            // Unmodeled op (Switch / handler / ...): bail entirely.
                            return empty;
                        }
                    }
                }
            }
            if !terminated {
                if end >= n {
                    return empty;
                }
                edges.push((end, k.clone()));
            }
            for (t, contrib) in &edges {
                if let Some(into) = in_sets.get_mut(t)
                    && meet(into, contrib)
                {
                    iterate = true;
                }
            }
        }
    }
    in_sets
}

/// Write the live operand `stack` back into the slot variables so a successor
/// block can read it (the variable/SSA machinery inserts the needed phis).
fn write_stack_to_vars(fb: &mut FunctionBuilder, vars: &[Variable], stack: &[ClifValue]) {
    for (k, &v) in stack.iter().enumerate() {
        fb.def_var(vars[k], v);
    }
}

/// Emit a backward jump with the interpreter's `branch_to!` parity: bump the
/// u8 quit counter; on every wrap (each 255th backward jump — counter resets to
/// 1, exactly like the interpreter) root the live operand stack and call the
/// back-edge service poll (GC safepoint + `maybe_quit`), propagating a signaled
/// `Flow` via the shared signal-exit block. The caller has already written the
/// operand stack to `vars` (the target's entry state).
#[allow(clippy::too_many_arguments)]
fn emit_backedge_jump(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    counter_slot: StackSlot,
    signal_exit: &mut Option<Block>,
    vars: &[Variable],
    target_depth: usize,
    target_block: Block,
    handlers: &[HandlerStatic],
    pending: &mut Vec<PendingDispatch>,
) {
    let c = fb.ins().stack_load(rt.ptr_ty, types::I64, counter_slot, 0);
    let c1 = fb.ins().iadd_imm_u(c, 1);
    let c1m = fb.ins().band_imm_u(c1, 0xFF);
    fb.ins().stack_store(rt.ptr_ty, c1m, counter_slot, 0);
    let wrapped = fb.ins().icmp_imm_u(IntCC::Equal, c1m, 0);
    let poll = fb.create_block();
    fb.ins().brif(wrapped, poll, &[], target_block, &[]);

    fb.switch_to_block(poll);
    fb.seal_block(poll);
    let one = fb.ins().iconst(types::I64, 1);
    fb.ins().stack_store(rt.ptr_ty, one, counter_slot, 0);
    // The live operand stack at the jump (already written to vars): rooted
    // across the poll, and the handler-entry snapshot if a quit signal lands
    // in a protected extent (condition-case catching `quit` around a loop).
    let vals: Vec<ClifValue> = (0..target_depth).map(|k| fb.use_var(vars[k])).collect();
    let saved = if vals.is_empty() {
        CondRoots::NONE
    } else {
        emit_cond_residual_roots_pre(fb, rt, &vals)
    };
    let vmctx = fb.use_var(rt.vmctx_var);
    let call = fb.ins().call(rt.refs.backedge, &[vmctx]);
    let status = fb.inst_results(call)[0];
    emit_cond_residual_roots_post(fb, rt, saved);
    let se = signal_target_for_site(fb, signal_exit, handlers, pending, &vals);
    let ok = fb.ins().icmp_imm_u(IntCC::Equal, status, STATUS_OK);
    fb.ins().brif(ok, target_block, &[], se, &[]);
}

/// Lower a leaf bytecode body taking `arity` fixed arguments to native code.
///
/// Whether the body has a BACKWARD jump (a loop) — needs the back-edge poll
/// (GC safepoint + quit), mirroring the interpreter's `branch_to!` wrap. Switch
/// targets count: a jump-table edge can also close a loop. Single source of truth
/// for both the JIT (`lower_leaf_full`) and the baseline-AOT emit (R2-E).
pub(crate) fn baseline_has_backedge(ops: &[Op], cfg: &Cfg) -> bool {
    ops.iter().enumerate().any(|(i, o)| match o {
        Op::Goto(t)
        | Op::GotoIfNil(t)
        | Op::GotoIfNotNil(t)
        | Op::GotoIfNilElsePop(t)
        | Op::GotoIfNotNilElsePop(t) => (*t as usize) <= i,
        _ => false,
    }) || cfg
        .switch_targets
        .iter()
        .any(|(i, ts)| ts.iter().any(|&(_, t)| t <= *i))
}

/// Whether the body re-enters the runtime (needs vmctx + the `neovm_jit_*` shim
/// scaffolding): a back-edge polls through vmctx; Eq/Symbolp use the
/// symbols-with-pos slow path; VarRef/VarSet/VarBind/Unbind hit the variable
/// machinery; Call/Apply/named-builtins/handlers re-enter elisp. Single source of
/// truth for both the JIT and the baseline-AOT emit (R2-E).
pub(crate) fn baseline_needs_rt(ops: &[Op], has_backedge: bool) -> bool {
    has_backedge
        || ops.iter().any(|o| {
            direct_builtin_spec(o).is_some()
                || slice_builtin_spec(o).is_some()
                || matches!(
                    o,
                    Op::List(_)
                        | Op::CallBuiltin(..)
                        | Op::CallBuiltinSym(..)
                        | Op::Aset
                        | Op::SaveWindowExcursion
                )
                || matches!(
                    o,
                    Op::Cons
                        | Op::Call(_)
                        | Op::Apply(_)
                        | Op::Eq
                        | Op::Symbolp
                        | Op::VarRef(_)
                        | Op::VarSet(_)
                        | Op::VarBind(_)
                        | Op::Unbind(_)
                        | Op::SaveCurrentBuffer
                        | Op::SaveExcursion
                        | Op::SaveRestriction
                        | Op::UnwindProtectPop
                        | Op::Throw
                        | Op::Integerp
                        | Op::Numberp
                        | Op::PushConditionCase(_)
                        | Op::PushConditionCaseRaw(_)
                        | Op::PushCatch(_)
                        | Op::PopHandler
                        | Op::Switch
                )
        })
}

/// Handles arbitrary intra-function control flow (`Goto`/`GotoIf*`) by building a
/// CLIF basic-block CFG: each bytecode basic block becomes a CLIF block, and the
/// operand stack flows across edges through per-slot SSA variables (Cranelift
/// inserts the phis). The `arity` arguments are loaded and seed the bottom of the
/// stack (arg0 deepest), exactly as the interpreter's `run_frame` pushes them.
pub fn lower_leaf(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
) -> Result<CompiledLeaf, CompileError> {
    lower_leaf_with_map(ops, constants, arity, None)
}

/// [`lower_leaf`] with the function's GNU byte-offset map, needed to resolve
/// `Op::Switch` jump-table addresses to instruction indices (GNU bytecode
/// stores byte offsets; natively compiled chunks store indices directly).
pub fn lower_leaf_with_map(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
) -> Result<CompiledLeaf, CompileError> {
    lower_leaf_full(ops, constants, arity, offset_map, None, 0)
}

/// [`lower_leaf_with_map`] plus the compiling thread's obarray, enabling
/// direct-call speculation (constant-symbol callees bound to bytecode get
/// epoch-validated direct calls; see [`find_spec_sites`]).
pub fn lower_leaf_full(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    obarray: Option<&Obarray>,
    dynamic_prefix: usize,
) -> Result<CompiledLeaf, CompileError> {
    lower_leaf_full_osr(
        ops,
        constants,
        arity,
        offset_map,
        obarray,
        None,
        dynamic_prefix,
    )
}

/// The compile-time view of a source with a `make-closure` patched prefix: the
/// per-instance slots read as `nil` so that NO analysis (symbol tags, spec
/// sites, known-fixnum elision, reloc collection, arith intrinsics) treats the
/// triggering instance's captured value — or the prototype's `V0..Vn`
/// placeholder — as a property of the SOURCE. The emitter never consults the
/// masked value for those slots: it loads them through the executing callee.
fn mask_dynamic_prefix(constants: &[Value], dynamic_prefix: usize) -> Vec<Value> {
    constants
        .iter()
        .enumerate()
        .map(|(i, v)| if i < dynamic_prefix { Value::NIL } else { *v })
        .collect()
}

/// [`lower_leaf_full`] plus an optional OSR entry pc (on-stack replacement,
/// JIT-only): when `Some(osr_pc)`, the compiled function's entry seeds the live
/// operand stack (from the `args` pointer) and jumps to the loop-header block at
/// `osr_pc`, letting the interpreter transfer a hot loop into native code
/// mid-execution. Cross-block known-fixnum elision is DISABLED for OSR (the
/// analysis assumes the normal block-0 entry as the sole root; the OSR entry adds
/// a predecessor it never saw, so every fixnum op guards — a non-fixnum simply
/// deopts, always sound).
pub fn lower_leaf_full_osr(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    obarray: Option<&Obarray>,
    osr_pc: Option<usize>,
    dynamic_prefix: usize,
) -> Result<CompiledLeaf, CompileError> {
    // Every analysis and the reloc collection below see the MASKED view; only
    // the emitter's `Op::Constant` arm knows the prefix (it loads those slots
    // through the callee at run time).
    let masked;
    let constants: &[Value] = if dynamic_prefix > 0 {
        masked = mask_dynamic_prefix(constants, dynamic_prefix);
        &masked
    } else {
        constants
    };
    let cfg = analyze_cfg(ops, constants, offset_map, arity)?;
    // Cross-block redundant-guard elimination: per-block-entry known-fixnum slots
    // (empty if the function has an op the analysis doesn't model -> no elision).
    // Disabled under OSR (see the doc): the OSR entry is an unanalyzed predecessor.
    let known_fixnum_slots = if osr_pc.is_some() {
        HashMap::new()
    } else {
        compute_known_fixnum_slots(ops, constants, &cfg)
    };
    let n = ops.len();
    // Direct-call speculation sites + their armed-epoch slots. The Box's heap
    // storage is address-stable: slot pointers are baked into the generated
    // code as immediates and the Box moves into the CompiledLeaf at the end.
    let (spec_sites, spec_slots): (HashMap<usize, SpecSite>, Box<[SpecSlot]>) = match obarray {
        Some(ob) => {
            let sites = find_spec_sites(ops, constants, &cfg.leaders, ob);
            let slots: Box<[SpecSlot]> = (0..sites.len())
                .map(|_| SpecSlot {
                    epoch: AtomicU64::new(0),
                    leaf: AtomicU64::new(0),
                })
                .collect();
            // Arm every slot with the epoch the bindings were observed at; any
            // bump before first execution self-heals via shim re-validation.
            let epoch = ob.function_epoch();
            for site in sites.values() {
                slots[site.slot].epoch.store(epoch, Ordering::Relaxed);
            }
            (sites, slots)
        }
        None => (HashMap::new(), Box::from([])),
    };
    // Precise-deopt buffers: live operand-stack spill (max depth) + the
    // pc/depth/handler-count cells. Address-stable Boxes owned by the leaf;
    // generated code writes through baked raw addresses.
    let deopt_spill: Box<[core::cell::Cell<i64>]> = (0..cfg.max_depth)
        .map(|_| core::cell::Cell::new(0))
        .collect();
    let deopt_meta: Box<DeoptCells> = Box::new(DeoptCells {
        pc: core::cell::Cell::new(0),
        depth: core::cell::Cell::new(0),
        handlers: core::cell::Cell::new(0),
    });

    // R1a: per-leaf heap-constant reloc vector (see lower_mir_pure). The baseline's
    // Op::Constant loads from reloc_data[idx] instead of baking a heap pointer, so
    // the code is GC-pointer-free + AOT-portable. Allocated here (before the
    // FunctionBuilder) so reloc_data.as_ptr() is stable when the load bakes its base.
    let mut reloc_vals: Vec<Value> = Vec::new();
    let mut reloc_index: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    for op in ops {
        if let Op::Constant(idx) = op
            && let Some(v) = constants.get(*idx as usize)
            && v.is_heap_object()
            && !reloc_index.contains_key(&v.bits())
        {
            reloc_index.insert(v.bits(), reloc_vals.len() as u32);
            reloc_vals.push(*v);
        }
    }
    let reloc_data: Box<[Value]> = reloc_vals.into_boxed_slice();

    // Baseline tier runs Cranelift at the default opt_level="none": its job is
    // FAST compilation (low tier-up latency; the soak compiles every function).
    // Measured opt_level="speed" (2026-06-13): no runtime win on fib (call-
    // bound) or the arithmetic loop, because Cranelift sees our tagged Values
    // as opaque i64 — it can't unbox, drop fixnum guards, or reason about lisp
    // effects. The real headroom is semantic (unboxing/inlining), which needs
    // an MIR-level optimizing Tier-2; opt_level="speed" belongs there, not at
    // this tier where it would only cost compile time.
    let mut builder = JITBuilder::with_isa(jit_isa()?, default_libcall_names());
    builder.symbol("neovm_jit_gc_save", neovm_jit_gc_save as *const u8);
    builder.symbol("neovm_jit_gc_push", neovm_jit_gc_push as *const u8);
    builder.symbol(
        "neovm_jit_gc_push_many",
        neovm_jit_gc_push_many as *const u8,
    );
    builder.symbol("neovm_jit_gc_restore", neovm_jit_gc_restore as *const u8);
    builder.symbol(
        "neovm_jit_rootwin_grow",
        neovm_jit_rootwin_grow as *const u8,
    );
    builder.symbol("neovm_jit_cons", neovm_jit_cons as *const u8);
    builder.symbol("neovm_jit_call", neovm_jit_call as *const u8);
    builder.symbol("neovm_jit_apply", neovm_jit_apply as *const u8);
    builder.symbol("neovm_jit_eq_slow", neovm_jit_eq_slow as *const u8);
    builder.symbol(
        "neovm_jit_symbolp_slow",
        neovm_jit_symbolp_slow as *const u8,
    );
    builder.symbol("neovm_jit_varref", neovm_jit_varref as *const u8);
    builder.symbol("neovm_jit_varset", neovm_jit_varset as *const u8);
    builder.symbol("neovm_jit_varbind", neovm_jit_varbind as *const u8);
    builder.symbol("neovm_jit_unbind", neovm_jit_unbind as *const u8);
    builder.symbol("neovm_jit_backedge", neovm_jit_backedge as *const u8);
    builder.symbol(
        "neovm_jit_save_current_buffer",
        neovm_jit_save_current_buffer as *const u8,
    );
    builder.symbol(
        "neovm_jit_save_excursion",
        neovm_jit_save_excursion as *const u8,
    );
    builder.symbol(
        "neovm_jit_save_restriction",
        neovm_jit_save_restriction as *const u8,
    );
    builder.symbol("neovm_jit_throw", neovm_jit_throw as *const u8);
    builder.symbol(
        "neovm_jit_integerp_slow",
        neovm_jit_integerp_slow as *const u8,
    );
    builder.symbol(
        "neovm_jit_numberp_slow",
        neovm_jit_numberp_slow as *const u8,
    );
    builder.symbol(
        "neovm_jit_unwind_protect",
        neovm_jit_unwind_protect as *const u8,
    );
    builder.symbol("neovm_jit_builtin1", neovm_jit_builtin1 as *const u8);
    builder.symbol("neovm_jit_builtin2", neovm_jit_builtin2 as *const u8);
    builder.symbol("neovm_jit_builtin3", neovm_jit_builtin3 as *const u8);
    builder.symbol("neovm_jit_push_cc", neovm_jit_push_cc as *const u8);
    builder.symbol("neovm_jit_push_cc_raw", neovm_jit_push_cc_raw as *const u8);
    builder.symbol("neovm_jit_push_catch", neovm_jit_push_catch as *const u8);
    builder.symbol("neovm_jit_pop_handler", neovm_jit_pop_handler as *const u8);
    builder.symbol(
        "neovm_jit_match_handler",
        neovm_jit_match_handler as *const u8,
    );
    builder.symbol("neovm_jit_switch", neovm_jit_switch as *const u8);
    builder.symbol(
        "neovm_jit_switch_stale",
        neovm_jit_switch_stale as *const u8,
    );
    builder.symbol("neovm_jit_list", neovm_jit_list as *const u8);
    builder.symbol(
        "neovm_jit_builtin_slice",
        neovm_jit_builtin_slice as *const u8,
    );
    builder.symbol(
        "neovm_jit_named_builtin",
        neovm_jit_named_builtin as *const u8,
    );
    builder.symbol(
        "neovm_jit_save_window_excursion",
        neovm_jit_save_window_excursion as *const u8,
    );
    builder.symbol("neovm_jit_call_spec", neovm_jit_call_spec as *const u8);
    // Gap 1 (JIT-only, by address — never in the dynamic symbol table): the
    // subr-speculation shims. Registered unconditionally (harmless when the
    // body has no subr sites; the declares are gated instead).
    builder.symbol(
        "neovm_jit_call_subr_spec",
        neovm_jit_call_subr_spec as *const u8,
    );
    builder.symbol("neovm_jit_pred_spec", neovm_jit_pred_spec as *const u8);
    builder.symbol(
        "neovm_jit_eq_incl_props_spec",
        neovm_jit_eq_incl_props_spec as *const u8,
    );
    builder.symbol("neovm_jit_arith_spec", neovm_jit_arith_spec as *const u8);
    // R2 CallBuiltinSym intrinsic shims (Tier-B dispatch-skip + Tier-A GC-free
    // read). Unconditional registration (declares are gated on a CBSym spec
    // site); JIT-only.
    builder.symbol("neovm_jit_cbsym_spec", neovm_jit_cbsym_spec as *const u8);
    builder.symbol("neovm_jit_cbsym_read", neovm_jit_cbsym_read as *const u8);
    let mut module = JITModule::new(builder);
    // has_backedge + needs_rt via the shared single-source helpers (R2-E) — same
    // logic as before, just factored so the baseline-AOT emit can reuse it.
    let has_backedge = baseline_has_backedge(ops, &cfg);
    let needs_rt = baseline_needs_rt(ops, has_backedge);

    // Build + define the leaf into the module via the module-generic seam
    // (`build_leaf_fn`). Buffers (`spec_slots`/`deopt_*`/`reloc_data`) are owned
    // here, threaded in by reference so their baked addresses stay stable, and
    // moved into the returned `CompiledLeaf` below.
    let fid = build_leaf_fn(
        &mut module,
        ops,
        constants,
        arity,
        &cfg,
        &known_fixnum_slots,
        &spec_sites,
        &spec_slots,
        n,
        &deopt_spill,
        &deopt_meta,
        &reloc_data,
        &reloc_index,
        has_backedge,
        needs_rt,
        /*aot=*/ false,
        "__neovm_jit_leaf",
        Linkage::Local,
        osr_pc,
        dynamic_prefix,
    )?;

    // --- JIT-only module epilogue (the wrapper). ----------------------------
    module
        .finalize_definitions()
        .map_err(|e| CompileError::Backend(BackendError::Finalize(e.to_string())))?;

    let entry = module.get_finalized_function(fid);
    Ok(CompiledLeaf {
        tier: LeafTier::Baseline,
        arity,
        // Plain fixed-arity defaults; compile_bytecode_function overrides for
        // &optional/&rest lambda lists.
        required: arity,
        has_rest: false,
        // The baseline never inlines; only the MIR tier sets this.
        inline_epoch: None,
        // The baseline is all-precise (every guard is STATUS_DEOPT_AT, never a
        // rerun-from-start after a call), so it never needs the refuse-to-rerun.
        has_side_effects: false,
        // The baseline never inlines.
        inline_deps: Box::from([]),
        has_binds: ops.iter().any(|o| {
            matches!(
                o,
                Op::VarBind(_)
                    | Op::Unbind(_)
                    | Op::SaveCurrentBuffer
                    | Op::SaveExcursion
                    | Op::SaveRestriction
                    | Op::UnwindProtectPop
            )
        }),
        has_handlers: ops.iter().any(|o| {
            matches!(
                o,
                Op::PushConditionCase(_) | Op::PushConditionCaseRaw(_) | Op::PushCatch(_)
            )
        }),
        spec_slots,
        // JIT bakes each site's `expected` as an iconst; no sidecar array needed.
        spec_expected: Box::from([]),
        deopt_spill,
        deopt_meta,
        reloc_data,
        // JIT bakes its bases as iconst; the 4th entry arg is the executing
        // callee's constant base, read only when `dynamic_prefix > 0`.
        sidecar: None,
        dynamic_prefix: u32::try_from(dynamic_prefix).expect("patched prefix fits u32"),
        entry,
        _backing: LeafBacking::Jit(module),
    })
}

/// R2-E: the BASELINE-tier metadata an AOT leaf's descriptor needs (the loader
/// sizes the per-thread deopt buffers + records the frame shape from this).
pub(crate) struct BaselineAotMeta {
    pub arity: usize,
    /// `cfg.max_depth` — the deopt operand-stack spill size.
    pub max_depth: usize,
    /// VarBind/Unbind/save-* present (the loader's `has_binds`).
    pub has_binds: bool,
    /// condition-case/catch handlers present.
    pub has_handlers: bool,
    /// R2 increment B2: the `Op::Call` subr/bytecode spec sites baked into this
    /// leaf (in slot order — descriptor position == codegen slot index). Empty when
    /// emitted without a live obarray (CBSym-only, increment A). The descriptor
    /// carries these so the loader can re-classify + arm each runtime `SpecSlot`.
    pub spec_sites: Vec<super::aot::AotSpecSite>,
}

/// R2-E (baseline-tier AOT emit): lower a bytecode body through the BASELINE tier
/// into `module` as an AOT object entry (`build_leaf_fn::<M>(aot=true)`), for
/// bodies the MIR tier rejects (Switch/Throw/handlers/CallBuiltin(Sym)/VarRef...).
///
/// Replicates [`lower_leaf_full`]'s analysis prologue but: (1) `obarray=None`, so
/// the `Op::Call` subr speculation is skipped (increment B), but the CBSym
/// intrinsic sites ARE classified (name-canonical + obarray-free — increment A);
/// CBSym is slotless, so there is still no per-spec-slot baked state and the deopt
/// fits the sidecar's existing bases; (2) the reloc set is the
/// caller's (covering const-relocs + the named-builtin op-symbols, #16/#17), with
/// the index keyed by tagged bits; (3) `aot=true` so reloc/deopt bases load from
/// the sidecar; (4) the entry is `Linkage::Export` under `entry_name` for the
/// loader to `dlsym`. The deopt buffers are sized-but-unbaked (AOT reads bases
/// from the sidecar, not these addresses) — throwaway here.
///
/// Returns the [`BaselineAotMeta`] for the descriptor.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_baseline_leaf_object<M: Module>(
    module: &mut M,
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    reloc_data: &[Value],
    reloc_index: &std::collections::HashMap<usize, u32>,
    entry_name: &str,
    // R2 increment B2: the compiling thread's obarray. `Some` → the `Op::Call`
    // subr/bytecode speculation pass runs (spec fast paths baked, descriptor
    // entries emitted); `None` → CBSym-only (increment A, obarray-free).
    obarray: Option<&Obarray>,
) -> Result<BaselineAotMeta, CompileError> {
    let cfg = analyze_cfg(ops, constants, None, arity)?;
    let known_fixnum_slots = compute_known_fixnum_slots(ops, constants, &cfg);
    let n = ops.len();
    // Classify speculation sites. With a LIVE obarray (increment B2) the full
    // `Op::Call` subr/bytecode pass runs alongside the obarray-free CBSym pass;
    // without one, only CBSym (increment A). CBSym is SLOTLESS/EPOCHLESS (its
    // lowering reads only the `kind`, never the slot/expected), so it needs no
    // descriptor entry; the `Op::Call` spec sites DO (the loader re-classifies +
    // arms each). `finalize_baseline_spec_sites` DROPs an un-reloc'd callee, dense-
    // renumbers the surviving slots, and returns the descriptor sites in slot order.
    let mut spec_sites: HashMap<usize, SpecSite> = match obarray {
        Some(ob) => find_spec_sites(ops, constants, &cfg.leaders, ob),
        None => find_cbsym_spec_sites(ops),
    };
    let aot_spec_sites = finalize_baseline_spec_sites(&mut spec_sites, ops, reloc_index);
    let spec_slots: Box<[SpecSlot]> = (0..spec_sites.len())
        .map(|_| SpecSlot {
            epoch: AtomicU64::new(0),
            leaf: AtomicU64::new(0),
        })
        .collect();
    let has_backedge = baseline_has_backedge(ops, &cfg);
    let needs_rt = baseline_needs_rt(ops, has_backedge);
    // Sized-but-unbaked deopt buffers: in AOT mode build_leaf_fn loads the spill +
    // meta bases from the SIDECAR (not these addresses), so these are throwaway
    // (the per-thread real buffers are allocated by the loader from the descriptor).
    let deopt_spill: Box<[core::cell::Cell<i64>]> = (0..cfg.max_depth)
        .map(|_| core::cell::Cell::new(0))
        .collect();
    let deopt_meta: Box<DeoptCells> = Box::new(DeoptCells {
        pc: core::cell::Cell::new(0),
        depth: core::cell::Cell::new(0),
        handlers: core::cell::Cell::new(0),
    });
    let max_depth = cfg.max_depth;
    build_leaf_fn(
        module,
        ops,
        constants,
        arity,
        &cfg,
        &known_fixnum_slots,
        &spec_sites,
        &spec_slots,
        n,
        &deopt_spill,
        &deopt_meta,
        reloc_data,
        reloc_index,
        has_backedge,
        needs_rt,
        /*aot=*/ true,
        entry_name,
        Linkage::Export,
        /*osr_pc=*/ None, // OSR is JIT-only
        /*dynamic_prefix=*/ 0, // AOT never targets a patched source
    )?;
    Ok(BaselineAotMeta {
        arity,
        max_depth,
        has_binds: ops.iter().any(|o| {
            matches!(
                o,
                Op::VarBind(_)
                    | Op::Unbind(_)
                    | Op::SaveCurrentBuffer
                    | Op::SaveExcursion
                    | Op::SaveRestriction
                    | Op::UnwindProtectPop
            )
        }),
        has_handlers: ops.iter().any(|o| {
            matches!(
                o,
                Op::PushConditionCase(_) | Op::PushConditionCaseRaw(_) | Op::PushCatch(_)
            )
        }),
        spec_sites: aot_spec_sites,
    })
}

/// Module-generic build seam for [`lower_leaf_full`]: sets up the leaf ABI
/// signature, lowers the bytecode `ops` through a `FunctionBuilder`, then
/// declares + defines the function into `module`, returning its `FuncId`. CLIF
/// output is byte-identical to the previous in-line lowering (pure extraction).
///
/// Generic over `M: Module` so the same lowering drives the `JITModule` JIT
/// path today and an `ObjectModule` AOT path later, unchanged. The
/// address-stable buffers (`spec_slots`/`deopt_spill`/`deopt_meta`/`reloc_data`)
/// are borrowed: their addresses are baked into the generated code, and the
/// caller retains ownership to move them into the `CompiledLeaf`.
///
/// This fn deliberately contains NONE of the three ObjectModule-incompatible
/// JIT seams, which stay in the [`lower_leaf_full`] wrapper:
///   * `builder.symbol(...)`    — AOT: `Linkage::Import` resolved via dlopen.
///   * `finalize_definitions()` — AOT: `ObjectModule::finish()`.
///   * `get_finalized_function` — AOT: `dlsym` of the exported entry symbol.
#[allow(clippy::too_many_arguments)]
fn build_leaf_fn<M: Module>(
    module: &mut M,
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    cfg: &Cfg,
    known_fixnum_slots: &HashMap<usize, Vec<bool>>,
    spec_sites: &HashMap<usize, SpecSite>,
    spec_slots: &[SpecSlot],
    n: usize,
    deopt_spill: &[core::cell::Cell<i64>],
    deopt_meta: &DeoptCells,
    reloc_data: &[Value],
    reloc_index: &std::collections::HashMap<usize, u32>,
    has_backedge: bool,
    needs_rt: bool,
    // R2-E (baseline-tier AOT): false → JIT (bases baked as `iconst`, byte-identical
    // to before); true → AOT (reloc-base + deopt bases loaded from the per-thread
    // `LeafSidecar` 4th entry arg, since the addresses are session-specific). Mirrors
    // `build_mir_leaf_fn`'s `aot` flag. Same RESULTS either way.
    aot: bool,
    // The exported entry symbol + its linkage. JIT: `("__neovm_jit_leaf", Local)`.
    // AOT: the content-hash entry name + `Export` so the loader can `dlsym` it.
    entry_name: &str,
    entry_linkage: Linkage,
    // OSR (on-stack replacement, JIT-only): when `Some(osr_pc)`, the function
    // entry does NOT seed args + jump to block 0; instead it seeds the operand
    // stack (`entry_depth[osr_pc]` tagged Values read from the `args` pointer)
    // and jumps STRAIGHT to the loop-header block at `osr_pc`, so the interpreter
    // can transfer a hot loop into native code mid-execution. Blocks unreachable
    // from `osr_pc` (the pre-loop prologue) are pruned so no dangling SSA survives.
    osr_pc: Option<usize>,
    // `make-closure` patched prefix of the source (JIT only): those leading
    // constant slots load through the callee constant base in the 4th entry
    // param instead of baking. 0 = plain function / AOT.
    dynamic_prefix: usize,
) -> Result<cranelift_module::FuncId, CompileError> {
    LAST_IR_STATS.with(|c| c.set((0, 0, 0, 0)));
    let frontend_config = module.target_config();
    let call_conv = frontend_config.default_call_conv;
    let ptr_ty = frontend_config.pointer_type();

    // ABI: fn(vmctx: *mut Context, args: *const i64, out: *mut i64) -> i64.
    // Reads `arity` argument words from `args`; returns STATUS_OK + writes the
    // result bits via `out` on success, STATUS_DEOPT on a failed guard, or
    // STATUS_SIGNAL when a runtime call raised a Flow (stashed for
    // `take_pending_flow`). `vmctx` is only used by runtime-call shims.
    // Unified 4-param entry ABI: fn(vmctx, args, out, sidecar) -> status (see
    // build_mir_leaf_fn). JIT (`aot=false`) declares but ignores `sidecar` (bases
    // stay `iconst`); AOT (`aot=true`) reads its reloc/deopt bases from it.
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(ptr_ty)); // vmctx
    sig.params.push(AbiParam::new(ptr_ty)); // args
    sig.params.push(AbiParam::new(ptr_ty)); // out
    sig.params.push(AbiParam::new(ptr_ty)); // sidecar (*const LeafSidecar)
    sig.returns.push(AbiParam::new(types::I64));

    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig.clone());
    let mut fbctx = FunctionBuilderContext::new();
    {
        let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);

        // Declare the runtime-call machinery into this function if the body
        // re-enters the runtime (`Cons` / `Call`).
        let rt = if needs_rt {
            // Declare the JIT-only round-1 subr-speculation shims iff the body has
            // round-1 subr-kind spec sites. The AOT baseline emit classifies ONLY
            // CBSym sites (`find_cbsym_spec_sites`; the `Op::Call` subr pass is
            // increment B), so this is always false for AOT — an ObjectModule
            // never declares the round-1 subr import names. CBSym-kind sites are
            // deliberately NOT counted here: they get their own R2 shims, so a
            // CBSym-only body never imports the `Op::Call` spec shims.
            let subr_spec = spec_sites.values().any(|site| site.kind.is_round1_subr());
            // The R2 CallBuiltinSym intrinsic shims (Tier-A read / Tier-B
            // dispatch-skip), declared when a CBSym-kind site exists. UNLIKE the
            // round-1 shims these are NOW emitted by AOT too (increment A): CBSym
            // classification is obarray-free, so `find_cbsym_spec_sites` populates
            // this for the baseline `ObjectModule` and the two shims become imports
            // resolved against the host at `dlopen`.
            let cbsym_spec = spec_sites.values().any(|site| site.kind.is_cbsym());
            // `module` is already `&mut M`; reborrow it for the call.
            let refs = declare_rt_refs(
                &mut *module,
                fb.func,
                call_conv,
                ptr_ty,
                subr_spec,
                cbsym_spec,
            )?;
            let vmctx_var = fb.declare_var(ptr_ty);
            let max_call_args = ops
                .iter()
                .filter_map(|o| match o {
                    Op::Call(n) | Op::Apply(n) | Op::List(n) | Op::Concat(n) => Some(*n as usize),
                    Op::CallBuiltin(_, n) | Op::CallBuiltinSym(_, n) => Some(*n as usize),
                    Op::Nconc => Some(2),
                    Op::Substring | Op::Aset => Some(3),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let call_args_slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (max_call_args.max(1) * 8) as u32,
                3,
            ));
            let call_result_slot =
                fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            // Lever 2: residual gather buffer, sized to the operand-stack depth
            // (an upper bound on any site's residual count).
            let residual_buf_slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (cfg.max_depth.max(1) * 8) as u32,
                3,
            ));
            let gc_saved_slot =
                fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            Some(RtCtx {
                refs,
                vmctx_var,
                ptr_ty,
                call_args_slot,
                call_result_slot,
                residual_buf_slot,
                gc_saved_slot,
            })
        } else {
            None
        };

        // SSA variables: one I64 slot per operand-stack position (carries the
        // stack across block edges), plus one for the out pointer (used by
        // `Return` in any block).
        let vars: Vec<Variable> = (0..cfg.max_depth)
            .map(|_| fb.declare_var(types::I64))
            .collect();
        let out_var = fb.declare_var(ptr_ty);

        // One CLIF block per bytecode basic block.
        let block_for: HashMap<usize, Block> = cfg
            .leaders
            .iter()
            .map(|&l| (l, fb.create_block()))
            .collect();
        // Deopt-buffer base addresses (for the JIT `iconst` path). The CLIF
        // `DeoptRefs` is materialized in the entry block below (the baseline tier
        // has no AOT path yet, so always the `iconst` form).
        let spill_base_addr = deopt_spill.as_ptr() as i64;
        let meta_pc_addr = &deopt_meta.pc as *const core::cell::Cell<i64> as i64;
        let meta_depth_addr = &deopt_meta.depth as *const core::cell::Cell<i64> as i64;
        let meta_handlers_addr = &deopt_meta.handlers as *const core::cell::Cell<i64> as i64;
        // Shared signal-propagation block (returns STATUS_SIGNAL), created
        // lazily by the first `Call` lowering.
        let mut signal_exit: Option<Block> = None;
        // Backward-jump quit counter (the interpreter's u8 `quitcounter`), kept
        // in a stack slot so every block can bump it.
        let backedge_counter: Option<StackSlot> = has_backedge.then(|| {
            fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3))
        });

        // Function-entry block: stash vmctx + the out pointer, load args into
        // the slot variables, then jump into bytecode block 0.
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        let vmctx_param = fb.block_params(entry)[0];
        let args_ptr = fb.block_params(entry)[1];
        let out_ptr = fb.block_params(entry)[2];
        // R2-E: the 4th entry param (the per-thread `*const LeafSidecar`). Read only
        // in AOT mode; JIT ignores it. The entry block dominates every block, so a
        // base materialized here is valid in any (incl. cold deopt) block.
        let sidecar_param = aot.then(|| fb.block_params(entry)[3]);
        // JIT leaf of a `make-closure`-patched source: the same 4th entry param is
        // the EXECUTING CALLEE's constant base (`CompiledLeaf::call_consts`); the
        // patched slots load off it (`lower_simple_op` `Op::Constant`). Bound in
        // the entry block so it dominates every block.
        debug_assert!(
            !(aot && dynamic_prefix > 0),
            "AOT never targets a patched source"
        );
        let consts_base = (!aot && dynamic_prefix > 0).then(|| fb.block_params(entry)[3]);
        // R1a: base address of the heap-constant reloc vector, materialized once in
        // entry (dominates all blocks); the baseline Op::Constant loads off it by
        // index. JIT bakes the Box address as `iconst`; AOT loads it from the
        // sidecar (session-specific). `None` when the body references no heap consts.
        let reloc_base = if reloc_data.is_empty() {
            None
        } else if aot {
            let sc = sidecar_param.expect("AOT sets sidecar_param");
            Some(fb.ins().load(
                ptr_ty,
                MemFlagsData::trusted(),
                sc,
                LeafSidecar::OFF_RELOC_BASE,
            ))
        } else {
            Some(fb.ins().iconst(ptr_ty, reloc_data.as_ptr() as i64))
        };
        // R2 increment B2: the AOT sidecar's spec-slot / spec-expected array bases,
        // loaded ONCE here (the entry block dominates every block, incl. cold deopt)
        // iff this is an AOT body with at least one `Op::Call` subr/bytecode spec
        // site (CBSym sites are slotless — they read neither base). JIT
        // (`aot=false`) emits NOTHING here (bases stay `None`; each site bakes its
        // slot/expected as `iconst`), so the JIT lowering is byte-identical to
        // pre-B2. `None` when the leaf has no such site, so a null sidecar base is
        // never loaded/indexed.
        let has_op_call_spec = spec_sites.values().any(|s| s.kind.to_spec_disc().is_some());
        let (spec_slot_base, spec_expected_base) = if aot && has_op_call_spec {
            let sc = sidecar_param.expect("AOT sets sidecar_param");
            (
                Some(fb.ins().load(
                    ptr_ty,
                    MemFlagsData::trusted(),
                    sc,
                    LeafSidecar::OFF_SPEC_SLOT_BASE,
                )),
                Some(fb.ins().load(
                    ptr_ty,
                    MemFlagsData::trusted(),
                    sc,
                    LeafSidecar::OFF_SPEC_EXPECTED_BASE,
                )),
            )
        } else {
            (None, None)
        };
        // Deopt-buffer bases as entry-block values: JIT (`aot=false`) → the `iconst`
        // form (deferred to the cold deopt blocks, byte-identical to pre-R2-E); AOT
        // (`aot=true`) → loaded from the sidecar. The baseline's precise deopt spills
        // operand-stack + pc/depth/handlers — exactly the sidecar's carried bases (at
        // D0 there are no spec slots, so no per-spec-slot state beyond these).
        let deopt_refs = materialize_deopt_refs(
            &mut fb,
            ptr_ty,
            aot,
            /*has_precise_deopt=*/ true,
            sidecar_param,
            spill_base_addr,
            meta_pc_addr,
            meta_depth_addr,
            meta_handlers_addr,
        );
        if let Some(rt) = &rt {
            fb.def_var(rt.vmctx_var, vmctx_param);
        }
        fb.def_var(out_var, out_ptr);
        if let Some(slot) = backedge_counter {
            // The interpreter starts quitcounter at 1.
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().stack_store(ptr_ty, one, slot, 0);
        }
        // Entry seeding + jump target. Normal: seed the `arity` args into the
        // bottom slots and jump to bytecode block 0. OSR: the `args` pointer holds
        // the live OPERAND STACK snapshot (`entry_depth[osr_pc]` tagged Values), so
        // seed those slots and jump STRAIGHT to the loop-header block at `osr_pc`.
        let (seed_count, jump_target) = match osr_pc {
            Some(p) => (cfg.entry_depth[&p], block_for[&p]),
            None => (arity, block_for[&0]),
        };
        for (i, var) in vars.iter().take(seed_count).enumerate() {
            let v = fb.ins().load(
                types::I64,
                MemFlagsData::trusted(),
                args_ptr,
                (i * 8) as i32,
            );
            fb.def_var(*var, v);
        }
        fb.ins().jump(jump_target, &[]);

        let next_leader = |idx: usize| cfg.leaders.iter().copied().find(|&l| l > idx).unwrap_or(n);

        for &l in &cfg.leaders {
            let blk = block_for[&l];
            fb.switch_to_block(blk);
            // Materialize the incoming operand stack from the slot variables.
            let depth = cfg.entry_depth[&l];
            let mut stack: Vec<ClifValue> = (0..depth).map(|k| fb.use_var(vars[k])).collect();
            // Cross-op unboxing: incoming slots are tagged (loaded from vars). Raw
            // (untagged) fixnums live only WITHIN a block; the mask resets to
            // all-tagged at each block entry (no cross-block raw in this increment).
            let mut stack_raw: Vec<bool> = vec![false; depth];
            // Cross-block known-fixnum operands at this block's entry: each slot
            // the dataflow analysis proved fixnum maps to its just-materialized
            // ClifValue. StackRef/Dup keep the same ClifValue, so the set stays
            // valid as the block runs; `guard_fixnum` elides guards for members.
            let known_fixnum: HashSet<ClifValue> = known_fixnum_slots
                .get(&l)
                .map(|slots| {
                    slots
                        .iter()
                        .enumerate()
                        .filter_map(|(k, &is_fix)| {
                            (is_fix).then(|| stack.get(k).copied()).flatten()
                        })
                        .collect()
                })
                .unwrap_or_default();
            // Active handler frames at block entry (static), kept in sync as
            // PopHandler ops run; signal sites inside a protected extent queue
            // a dispatch block here, filled after the block's terminator.
            let mut handlers: Vec<HandlerStatic> =
                cfg.entry_handlers.get(&l).cloned().unwrap_or_default();
            let mut pending: Vec<PendingDispatch> = Vec::new();
            let mut pending_deopt: Vec<PendingDeopt> = Vec::new();

            let end = next_leader(l);
            let mut terminated = false;
            for (off, op) in ops[l..end].iter().enumerate() {
                let i = l + off;
                // Terminators consume / snapshot / spill the operand stack as tagged
                // Values; force-tag any raw slots first (the block's raw state is
                // discarded after the terminator, so no per-pop lockstep is needed
                // past this point).
                if matches!(
                    op,
                    Op::Return
                        | Op::Throw
                        | Op::Goto(_)
                        | Op::GotoIfNil(_)
                        | Op::GotoIfNotNil(_)
                        | Op::GotoIfNilElsePop(_)
                        | Op::GotoIfNotNilElsePop(_)
                        | Op::Switch
                        | Op::PushConditionCase(_)
                        | Op::PushConditionCaseRaw(_)
                        | Op::PushCatch(_)
                ) {
                    retag_all_raw(&mut fb, &mut stack, &mut stack_raw);
                }
                match op {
                    Op::Return => {
                        let result = stack.pop().ok_or(CompileError::StackUnderflow)?;
                        let out = fb.use_var(out_var);
                        fb.ins().store(MemFlagsData::trusted(), result, out, 0);
                        let one = fb.ins().iconst(types::I64, 1);
                        fb.ins().return_(&[one]);
                        terminated = true;
                        break;
                    }
                    Op::Throw => {
                        // Stash Flow::Throw{tag, value} and exit via the signal
                        // path; inside a protected extent that path is the
                        // handler dispatch (a same-function `catch` is caught
                        // natively via the match shim).
                        let value = stack.pop().ok_or(CompileError::StackUnderflow)?;
                        let tag = stack.pop().ok_or(CompileError::StackUnderflow)?;
                        let rt = rt.as_ref().ok_or(CompileError::UnsupportedOp("throw"))?;
                        fb.ins().call(rt.refs.throw_flow, &[tag, value]);
                        let se = signal_target_for_site(
                            &mut fb,
                            &mut signal_exit,
                            &handlers,
                            &mut pending,
                            &stack,
                        );
                        fb.ins().jump(se, &[]);
                        terminated = true;
                        break;
                    }
                    Op::Goto(t) => {
                        write_stack_to_vars(&mut fb, &vars, &stack);
                        let tu = *t as usize;
                        if tu <= i {
                            // Backward jump: bump the quit counter and poll on
                            // wrap, exactly like the interpreter's branch_to!.
                            let (rt, slot) = (
                                rt.as_ref().expect("backedge implies rt"),
                                backedge_counter.expect("backedge implies counter"),
                            );
                            emit_backedge_jump(
                                &mut fb,
                                rt,
                                slot,
                                &mut signal_exit,
                                &vars,
                                cfg.entry_depth[&tu],
                                block_for[&tu],
                                &handlers,
                                &mut pending,
                            );
                        } else {
                            fb.ins().jump(block_for[&tu], &[]);
                        }
                        terminated = true;
                        break;
                    }
                    Op::GotoIfNil(t) | Op::GotoIfNotNil(t) => {
                        let cond = stack.pop().ok_or(CompileError::StackUnderflow)?;
                        write_stack_to_vars(&mut fb, &vars, &stack);
                        let is_nil =
                            fb.ins()
                                .icmp_imm_u(IntCC::Equal, cond, Value::NIL.bits() as i64);
                        let tu = *t as usize;
                        let mut target = block_for[&tu];
                        let fallthrough = block_for[&(i + 1)];
                        let backedge = (tu <= i).then(|| fb.create_block());
                        if let Some(tramp) = backedge {
                            target = tramp;
                        }
                        // brif takes the `then` block when the condition is true.
                        if matches!(op, Op::GotoIfNil(_)) {
                            fb.ins().brif(is_nil, target, &[], fallthrough, &[]);
                        } else {
                            fb.ins().brif(is_nil, fallthrough, &[], target, &[]);
                        }
                        if let Some(tramp) = backedge {
                            // Taken-edge trampoline carrying the back-edge poll.
                            fb.switch_to_block(tramp);
                            fb.seal_block(tramp);
                            let (rt, slot) = (
                                rt.as_ref().expect("backedge implies rt"),
                                backedge_counter.expect("backedge implies counter"),
                            );
                            emit_backedge_jump(
                                &mut fb,
                                rt,
                                slot,
                                &mut signal_exit,
                                &vars,
                                cfg.entry_depth[&tu],
                                block_for[&tu],
                                &handlers,
                                &mut pending,
                            );
                        }
                        terminated = true;
                        break;
                    }
                    Op::GotoIfNilElsePop(t) | Op::GotoIfNotNilElsePop(t) => {
                        // Peek the condition without popping; write the FULL stack
                        // (cond on top) to vars. The jump-taken successor reads it
                        // all (depth D); the fall-through (depth D-1) ignores the
                        // top slot — implementing the "ElsePop".
                        let cond = *stack.last().ok_or(CompileError::StackUnderflow)?;
                        write_stack_to_vars(&mut fb, &vars, &stack);
                        let is_nil =
                            fb.ins()
                                .icmp_imm_u(IntCC::Equal, cond, Value::NIL.bits() as i64);
                        let tu = *t as usize;
                        let mut target = block_for[&tu];
                        let fallthrough = block_for[&(i + 1)];
                        let backedge = (tu <= i).then(|| fb.create_block());
                        if let Some(tramp) = backedge {
                            target = tramp;
                        }
                        if matches!(op, Op::GotoIfNilElsePop(_)) {
                            fb.ins().brif(is_nil, target, &[], fallthrough, &[]);
                        } else {
                            fb.ins().brif(is_nil, fallthrough, &[], target, &[]);
                        }
                        if let Some(tramp) = backedge {
                            fb.switch_to_block(tramp);
                            fb.seal_block(tramp);
                            let (rt, slot) = (
                                rt.as_ref().expect("backedge implies rt"),
                                backedge_counter.expect("backedge implies counter"),
                            );
                            emit_backedge_jump(
                                &mut fb,
                                rt,
                                slot,
                                &mut signal_exit,
                                &vars,
                                cfg.entry_depth[&tu],
                                block_for[&tu],
                                &handlers,
                                &mut pending,
                            );
                        }
                        terminated = true;
                        break;
                    }
                    Op::Switch => {
                        // [dispatch table] -> shim lookup (the interpreter's
                        // exact hash-key semantics) returning the raw fixnum
                        // address; map it onto the statically resolved targets
                        // with a compare chain. Miss -> fall through. A raw
                        // address outside the static set or a mutated table ->
                        // loud signal (out-of-contract self-modification).
                        let rt_ref = rt.as_ref().ok_or(CompileError::UnsupportedOp("switch"))?;
                        let table = stack.pop().ok_or(CompileError::StackUnderflow)?;
                        let dispatch = stack.pop().ok_or(CompileError::StackUnderflow)?;
                        write_stack_to_vars(&mut fb, &vars, &stack);
                        let vmctx = fb.use_var(rt_ref.vmctx_var);
                        let call = fb
                            .ins()
                            .call(rt_ref.refs.switch_lookup, &[vmctx, dispatch, table]);
                        let addr = fb.inst_results(call)[0];
                        let targets = cfg.switch_targets.get(&i).expect("resolved in analyze");
                        let sig = signal_target_for_site(
                            &mut fb,
                            &mut signal_exit,
                            &handlers,
                            &mut pending,
                            &stack,
                        );
                        let fall = block_for[&(i + 1)];
                        // miss -> fall through
                        let miss = fb.ins().icmp_imm_u(IntCC::Equal, addr, JIT_SWITCH_MISS);
                        let chain = fb.create_block();
                        fb.ins().brif(miss, fall, &[], chain, &[]);
                        fb.switch_to_block(chain);
                        fb.seal_block(chain);
                        // stale (-2): the shim stashed the flow already.
                        let stale = fb.ins().icmp_imm_u(IntCC::Equal, addr, JIT_SWITCH_STALE);
                        let mut cur_blk = fb.create_block();
                        fb.ins().brif(stale, sig, &[], cur_blk, &[]);
                        for &(raw, target) in targets {
                            fb.switch_to_block(cur_blk);
                            fb.seal_block(cur_blk);
                            let next = fb.create_block();
                            let hit = fb.ins().icmp_imm_u(IntCC::Equal, addr, raw);
                            if target <= i {
                                // Backward jump-table edge: poll through a
                                // trampoline, exactly like Goto back-edges.
                                let tramp = fb.create_block();
                                fb.ins().brif(hit, tramp, &[], next, &[]);
                                fb.switch_to_block(tramp);
                                fb.seal_block(tramp);
                                let (rt_b, slot) = (
                                    rt.as_ref().expect("backedge implies rt"),
                                    backedge_counter.expect("backedge implies counter"),
                                );
                                emit_backedge_jump(
                                    &mut fb,
                                    rt_b,
                                    slot,
                                    &mut signal_exit,
                                    &vars,
                                    cfg.entry_depth[&target],
                                    block_for[&target],
                                    &handlers,
                                    &mut pending,
                                );
                            } else {
                                fb.ins().brif(hit, block_for[&target], &[], next, &[]);
                            }
                            cur_blk = next;
                        }
                        // Exhausted: a hit whose address is not in the static
                        // set — stash the stale-table signal and propagate.
                        fb.switch_to_block(cur_blk);
                        fb.seal_block(cur_blk);
                        fb.ins().call(rt_ref.refs.switch_stale, &[]);
                        fb.ins().jump(sig, &[]);
                        terminated = true;
                        break;
                    }
                    Op::PushConditionCase(t) | Op::PushConditionCaseRaw(t) | Op::PushCatch(t) => {
                        // Register the handler frame via the shim (interpreter
                        // arm parity), then end the block with an "anchor"
                        // edge: a never-taken branch to the handler target
                        // that (a) guarantees the target block always has a
                        // Cranelift predecessor with every entry var defined
                        // (its real entries are the runtime match dispatches)
                        // and (b) falls through to the protected body.
                        let rt_ref = rt.as_ref().ok_or(CompileError::UnsupportedOp("handler"))?;
                        let tu = *t as usize;
                        let vmctx = fb.use_var(rt_ref.vmctx_var);
                        let t_v = fb.ins().iconst(types::I64, tu as i64);
                        match op {
                            Op::PushConditionCase(_) => {
                                let d_v = fb.ins().iconst(types::I64, stack.len() as i64);
                                fb.ins().call(rt_ref.refs.push_cc, &[vmctx, t_v, d_v]);
                            }
                            Op::PushConditionCaseRaw(_) => {
                                let conditions = stack.pop().ok_or(CompileError::StackUnderflow)?;
                                let d_v = fb.ins().iconst(types::I64, stack.len() as i64);
                                fb.ins()
                                    .call(rt_ref.refs.push_cc_raw, &[vmctx, t_v, d_v, conditions]);
                            }
                            Op::PushCatch(_) => {
                                let tag = stack.pop().ok_or(CompileError::StackUnderflow)?;
                                let d_v = fb.ins().iconst(types::I64, stack.len() as i64);
                                fb.ins()
                                    .call(rt_ref.refs.push_catch, &[vmctx, t_v, d_v, tag]);
                            }
                            _ => unreachable!("matched Push* above"),
                        }
                        write_stack_to_vars(&mut fb, &vars, &stack);
                        // Placeholder error-value slot for the never-taken
                        // anchor edge (real entries define it from the shim).
                        let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
                        fb.def_var(vars[stack.len()], nil);
                        let never = fb.ins().iconst(types::I8, 0);
                        fb.ins()
                            .brif(never, block_for[&tu], &[], block_for[&(i + 1)], &[]);
                        terminated = true;
                        break;
                    }
                    Op::PopHandler => {
                        // Normal exit from the protected extent: drop the
                        // runtime frame and the static tracking entry.
                        let rt_ref = rt.as_ref().ok_or(CompileError::UnsupportedOp("handler"))?;
                        let vmctx = fb.use_var(rt_ref.vmctx_var);
                        fb.ins().call(rt_ref.refs.pop_handler, &[vmctx]);
                        handlers
                            .pop()
                            .ok_or(CompileError::UnsupportedOp("unbalanced-pophandler"))?;
                    }
                    other => {
                        let spec = spec_sites.get(&i).map(|site| {
                            (
                                site.sym,
                                site.expected_bits,
                                &spec_slots[site.slot] as *const SpecSlot as i64,
                                // R2 increment B2: the slot index the AOT sidecar's
                                // spec-slot / spec-expected arrays are keyed by.
                                site.slot,
                                site.kind,
                            )
                        });
                        lower_simple_op(
                            &mut fb,
                            i,
                            &mut pending_deopt,
                            &mut signal_exit,
                            constants,
                            &mut stack,
                            &mut stack_raw,
                            rt.as_ref(),
                            &handlers,
                            &mut pending,
                            spec,
                            other,
                            &known_fixnum,
                            reloc_base,
                            reloc_index,
                            aot,
                            spec_slot_base,
                            spec_expected_base,
                            dynamic_prefix,
                            consts_base,
                        )?;
                        // Re-sync the raw mask after the op: raw-preserving ops keep
                        // it in lockstep (assert); every other op force-tagged the
                        // stack at the top, so reset the mask to all-tagged here.
                        if op_preserves_raw(other) {
                            debug_assert_eq!(
                                stack.len(),
                                stack_raw.len(),
                                "raw-preserving op left stack_raw desynced"
                            );
                        } else {
                            stack_raw.resize(stack.len(), false);
                        }
                    }
                }
            }
            if !terminated {
                // Fall through into the next leader block (analyze guaranteed it
                // exists and is < n). vars carry tagged Values across the edge.
                retag_all_raw(&mut fb, &mut stack, &mut stack_raw);
                write_stack_to_vars(&mut fb, &vars, &stack);
                fb.ins().jump(block_for[&end], &[]);
            }
            // Fill the precise-deopt exit blocks queued by this block's guards.
            emit_pending_deopts(&mut fb, deopt_refs, &mut pending_deopt);
            // Fill the handler-dispatch blocks queued by this block's signal
            // sites (the builder can switch blocks now that it's terminated).
            if !pending.is_empty() {
                let rt_ref = rt.as_ref().expect("pending dispatches imply rt");
                emit_pending_dispatches(
                    &mut fb,
                    rt_ref,
                    &mut signal_exit,
                    &vars,
                    &block_for,
                    &mut pending,
                )?;
            }
        }

        // Terminate the shared signal block (return STATUS_SIGNAL) iff used.
        if let Some(sb) = signal_exit {
            fb.switch_to_block(sb);
            let code = fb.ins().iconst(types::I64, STATUS_SIGNAL);
            fb.ins().return_(&[code]);
        }

        fb.seal_all_blocks();
        fb.finalize(frontend_config);
    }
    LAST_IR_STATS.with(|c| {
        let (_, _, sites, slots) = c.get();
        c.set((
            func.dfg.num_insts() as u32,
            func.layout.blocks().count() as u32,
            sites,
            slots,
        ));
    });

    let fid = module
        .declare_function(entry_name, entry_linkage, &sig)
        .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
    let mut ctx = module.make_context();
    ctx.func = func;
    module
        .define_function(fid, &mut ctx)
        .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
    module.clear_context(&mut ctx);

    Ok(fid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emacs_core::value::LambdaParams;

    fn nullary() -> ByteCodeFunction {
        ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        })
    }

    #[test]
    fn compiles_constant_return() {
        // (lambda () 42)  ==  [Constant(0), Return], constants = [42]
        let c = Value::make_int(42);
        let leaf = lower_nullary_leaf(&[Op::Constant(0), Op::Return], &[c]).unwrap();
        assert_eq!(leaf.call_for_test(&[]), Some(c.bits()));
    }

    #[test]
    fn is_fixnum_const_detects_fixnum_constants_for_guard_elision() {
        // Redundant-guard elimination: a fixnum `iconst` is provably a fixnum, so
        // guard_fixnum elides its runtime guard; a symbol (nil) constant and a
        // computed value are NOT fixnum constants and keep their guards.
        let mut func = Function::with_name_signature(
            UserFuncName::user(0, 0),
            Signature::new(cranelift_codegen::isa::CallConv::SystemV),
        );
        let mut fbctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);
        let block = fb.create_block();
        fb.switch_to_block(block);
        fb.seal_block(block);
        let fixnum = fb
            .ins()
            .iconst(types::I64, Value::make_int(7).bits() as i64);
        let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
        let sum = fb.ins().iadd(fixnum, fixnum);
        assert!(
            is_fixnum_const(&fb, fixnum),
            "a fixnum iconst is a fixnum constant"
        );
        assert!(
            !is_fixnum_const(&fb, nil),
            "nil (symbol tag) is not a fixnum"
        );
        assert!(
            !is_fixnum_const(&fb, sum),
            "an iadd result is not a constant"
        );

        // is_known_fixnum additionally recognizes a retag_fixnum output (a
        // range-checked arithmetic result), eliding the re-guard on chained
        // arithmetic; a bare untagged iadd is not recognized.
        let shifted = fb.ins().ishl_imm_u(sum, FIXNUM_SHIFT as i64);
        let retagged = fb.ins().bor_imm_u(shifted, FIXNUM_CHECK_VALUE as i64);
        assert!(
            is_known_fixnum(&fb, retagged),
            "retag_fixnum output is a known fixnum"
        );
        assert!(
            is_known_fixnum(&fb, fixnum),
            "a fixnum constant is a known fixnum"
        );
        assert!(
            !is_known_fixnum(&fb, sum),
            "a bare iadd is not a known fixnum"
        );
        assert!(!is_known_fixnum(&fb, nil), "nil is not a known fixnum");
    }

    fn known_fixnum_at(ops: &[Op], constants: &[Value], leader: usize) -> Option<Vec<bool>> {
        let cfg = analyze_cfg(ops, constants, None, 0).unwrap();
        compute_known_fixnum_slots(ops, constants, &cfg)
            .get(&leader)
            .cloned()
    }

    #[test]
    fn cross_block_known_fixnum_propagates_meets_and_loops() {
        // Forward: a fixnum constant flows across a Goto into its successor block.
        let ops = [Op::Constant(0), Op::Goto(2), Op::Return];
        assert_eq!(
            known_fixnum_at(&ops, &[Value::make_int(7)], 2),
            Some(vec![true]),
            "fixnum constant is known-fixnum across a Goto"
        );
        // A non-fixnum constant is NOT known-fixnum across the edge.
        assert_eq!(
            known_fixnum_at(&ops, &[Value::NIL], 2),
            Some(vec![false]),
            "nil is not a known fixnum across a Goto"
        );

        // Merge narrows: fixnum on the then-path, non-fixnum on the else-path.
        let diamond = [
            Op::Constant(0),  // 0: condition
            Op::GotoIfNil(4), // 1: pop, branch to else(4) or fall to then(2)
            Op::Constant(1),  // 2: then -> fixnum
            Op::Goto(5),      // 3
            Op::Constant(2),  // 4: else -> nil (leader); falls through to 5
            Op::Return,       // 5: merge (leader)
        ];
        let cs = [Value::make_int(0), Value::make_int(9), Value::NIL];
        assert_eq!(
            known_fixnum_at(&diamond, &cs, 5),
            Some(vec![false]),
            "merge of fixnum and non-fixnum is not known-fixnum"
        );

        // THE TARGET: a loop induction variable (i=0; while i<10: i=1+i) is
        // proven fixnum at the loop head across the back-edge (the fixpoint).
        let loop_ops = [
            Op::Constant(0),  // 0: i = 0
            Op::StackRef(0),  // 1: loop head (back-edge target): push i
            Op::Constant(1),  // 2: push limit 10
            Op::Lss,          // 3: i < 10
            Op::GotoIfNil(9), // 4: pop; exit -> 9
            Op::StackRef(0),  // 5: body: push i
            Op::Add1,         // 6: 1+ i
            Op::StackSet(1),  // 7: i = 1+ i
            Op::Goto(1),      // 8: back-edge
            Op::Return,       // 9: exit
        ];
        let lc = [Value::make_int(0), Value::make_int(10)];
        assert_eq!(
            known_fixnum_at(&loop_ops, &lc, 1),
            Some(vec![true]),
            "loop induction variable is known-fixnum at the loop head"
        );
    }

    #[test]
    fn compiles_nil_and_true() {
        assert_eq!(
            lower_nullary_leaf(&[Op::Nil, Op::Return], &[])
                .unwrap()
                .call_for_test(&[]),
            Some(Value::NIL.bits())
        );
        assert_eq!(
            lower_nullary_leaf(&[Op::True, Op::Return], &[])
                .unwrap()
                .call_for_test(&[]),
            Some(Value::T.bits())
        );
    }

    #[test]
    fn dup_and_pop_select_the_right_value() {
        // [Const(0), Const(1), Dup, Pop, Return] -> top is constants[1]
        let a = Value::make_int(7);
        let b = Value::make_int(9);
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Dup,
                Op::Pop,
                Op::Return,
            ],
            &[a, b],
        )
        .unwrap();
        assert_eq!(leaf.call_for_test(&[]), Some(b.bits()));
    }

    #[test]
    fn stackref_reaches_below_top() {
        // [Const(0), Const(1), StackRef(1), Return] -> pushes a copy of a, returns a
        let a = Value::make_int(100);
        let b = Value::make_int(200);
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::StackRef(1),
                Op::Return,
            ],
            &[a, b],
        )
        .unwrap();
        assert_eq!(leaf.call_for_test(&[]), Some(a.bits()));
    }

    #[test]
    fn compiles_fixnum_add() {
        // (+ 40 2) -> 42, all fixnums in range
        let leaf = lower_nullary_leaf(
            &[Op::Constant(0), Op::Constant(1), Op::Add, Op::Return],
            &[Value::make_int(40), Value::make_int(2)],
        )
        .unwrap();
        assert_eq!(leaf.call_for_test(&[]), Some(Value::make_int(42).bits()));
    }

    #[test]
    fn compiles_fixnum_sub_including_negative() {
        // (- 3 10) -> -7
        let leaf = lower_nullary_leaf(
            &[Op::Constant(0), Op::Constant(1), Op::Sub, Op::Return],
            &[Value::make_int(3), Value::make_int(10)],
        )
        .unwrap();
        assert_eq!(leaf.call_for_test(&[]), Some(Value::make_int(-7).bits()));
    }

    #[test]
    fn add_overflowing_fixnum_range_deopts() {
        // MOST_POSITIVE_FIXNUM + 1 leaves fixnum range -> deopt (None), so the
        // interpreter can promote to a bignum.
        let leaf = lower_nullary_leaf(
            &[Op::Constant(0), Op::Constant(1), Op::Add, Op::Return],
            &[
                Value::make_int(Value::MOST_POSITIVE_FIXNUM),
                Value::make_int(1),
            ],
        )
        .unwrap();
        assert_eq!(leaf.call_for_test(&[]), None);
    }

    #[test]
    fn add_non_fixnum_operand_deopts() {
        // a = fixnum 5, b = nil -> not both fixnums -> deopt.
        let leaf = lower_nullary_leaf(
            &[Op::Constant(0), Op::Nil, Op::Add, Op::Return],
            &[Value::make_int(5)],
        )
        .unwrap();
        assert_eq!(leaf.call_for_test(&[]), None);
    }

    #[test]
    fn add_then_sub_chain() {
        // ((1 + 2) - 4) = -1
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Add,
                Op::Constant(2),
                Op::Sub,
                Op::Return,
            ],
            &[Value::make_int(1), Value::make_int(2), Value::make_int(4)],
        )
        .unwrap();
        assert_eq!(leaf.call_for_test(&[]), Some(Value::make_int(-1).bits()));
    }

    #[test]
    fn compiles_unary_fixnum_ops() {
        // 1+ 41 -> 42
        let add1 = lower_nullary_leaf(
            &[Op::Constant(0), Op::Add1, Op::Return],
            &[Value::make_int(41)],
        )
        .unwrap();
        assert_eq!(add1.call_for_test(&[]), Some(Value::make_int(42).bits()));

        // 1- 43 -> 42
        let sub1 = lower_nullary_leaf(
            &[Op::Constant(0), Op::Sub1, Op::Return],
            &[Value::make_int(43)],
        )
        .unwrap();
        assert_eq!(sub1.call_for_test(&[]), Some(Value::make_int(42).bits()));

        // - 42 -> -42
        let neg = lower_nullary_leaf(
            &[Op::Constant(0), Op::Negate, Op::Return],
            &[Value::make_int(42)],
        )
        .unwrap();
        assert_eq!(neg.call_for_test(&[]), Some(Value::make_int(-42).bits()));
    }

    #[test]
    fn unary_boundary_inputs_deopt() {
        // 1+ MOST_POSITIVE -> overflow -> deopt
        let add1 = lower_nullary_leaf(
            &[Op::Constant(0), Op::Add1, Op::Return],
            &[Value::make_int(Value::MOST_POSITIVE_FIXNUM)],
        )
        .unwrap();
        assert_eq!(add1.call_for_test(&[]), None);

        // 1- MOST_NEGATIVE -> underflow -> deopt
        let sub1 = lower_nullary_leaf(
            &[Op::Constant(0), Op::Sub1, Op::Return],
            &[Value::make_int(Value::MOST_NEGATIVE_FIXNUM)],
        )
        .unwrap();
        assert_eq!(sub1.call_for_test(&[]), None);

        // - MOST_NEGATIVE -> +MOST_POSITIVE+1 out of range -> deopt
        let neg = lower_nullary_leaf(
            &[Op::Constant(0), Op::Negate, Op::Return],
            &[Value::make_int(Value::MOST_NEGATIVE_FIXNUM)],
        )
        .unwrap();
        assert_eq!(neg.call_for_test(&[]), None);
    }

    #[test]
    fn unary_on_non_fixnum_deopts() {
        // 1+ t -> not a fixnum -> deopt
        let leaf = lower_nullary_leaf(&[Op::True, Op::Add1, Op::Return], &[]).unwrap();
        assert_eq!(leaf.call_for_test(&[]), None);
    }

    #[test]
    fn compiles_fixnum_comparisons() {
        fn cmp(ops: &[Op], a: i64, b: i64) -> Option<usize> {
            lower_nullary_leaf(ops, &[Value::make_int(a), Value::make_int(b)])
                .unwrap()
                .call_for_test(&[])
        }
        let t = Some(Value::T.bits());
        let nil = Some(Value::NIL.bits());
        assert_eq!(
            cmp(
                &[Op::Constant(0), Op::Constant(1), Op::Lss, Op::Return],
                3,
                5
            ),
            t
        );
        assert_eq!(
            cmp(
                &[Op::Constant(0), Op::Constant(1), Op::Lss, Op::Return],
                5,
                3
            ),
            nil
        );
        assert_eq!(
            cmp(
                &[Op::Constant(0), Op::Constant(1), Op::Gtr, Op::Return],
                5,
                3
            ),
            t
        );
        assert_eq!(
            cmp(
                &[Op::Constant(0), Op::Constant(1), Op::Leq, Op::Return],
                4,
                4
            ),
            t
        );
        assert_eq!(
            cmp(
                &[Op::Constant(0), Op::Constant(1), Op::Geq, Op::Return],
                4,
                5
            ),
            nil
        );
        assert_eq!(
            cmp(
                &[Op::Constant(0), Op::Constant(1), Op::Eqlsign, Op::Return],
                7,
                7
            ),
            t
        );
        assert_eq!(
            cmp(
                &[Op::Constant(0), Op::Constant(1), Op::Eqlsign, Op::Return],
                7,
                8
            ),
            nil
        );
    }

    #[test]
    fn comparison_on_non_fixnum_deopts() {
        // (< 1 nil) -> nil isn't a fixnum -> deopt.
        let leaf = lower_nullary_leaf(
            &[Op::Constant(0), Op::Nil, Op::Lss, Op::Return],
            &[Value::make_int(1)],
        )
        .unwrap();
        assert_eq!(leaf.call_for_test(&[]), None);
    }

    #[test]
    fn compiles_if_branch() {
        // (lambda (x) (if x 1 2)):
        //  0 StackRef(0); 1 GotoIfNil(4); 2 Constant(0=>1); 3 Return;
        //  4 Constant(1=>2); 5 Return
        let f = lower_leaf(
            &[
                Op::StackRef(0),
                Op::GotoIfNil(4),
                Op::Constant(0),
                Op::Return,
                Op::Constant(1),
                Op::Return,
            ],
            &[Value::make_int(1), Value::make_int(2)],
            1,
        )
        .unwrap();
        assert_eq!(
            f.call_for_test(&[Value::T]),
            Some(Value::make_int(1).bits())
        );
        assert_eq!(
            f.call_for_test(&[Value::make_int(99)]),
            Some(Value::make_int(1).bits())
        );
        assert_eq!(
            f.call_for_test(&[Value::NIL]),
            Some(Value::make_int(2).bits())
        );
    }

    #[test]
    fn compiles_goto_if_not_nil() {
        // jumps to the second arm when the arg is non-nil.
        let f = lower_leaf(
            &[
                Op::StackRef(0),
                Op::GotoIfNotNil(4),
                Op::Constant(0),
                Op::Return,
                Op::Constant(1),
                Op::Return,
            ],
            &[Value::make_int(1), Value::make_int(2)],
            1,
        )
        .unwrap();
        assert_eq!(
            f.call_for_test(&[Value::NIL]),
            Some(Value::make_int(1).bits())
        );
        assert_eq!(
            f.call_for_test(&[Value::T]),
            Some(Value::make_int(2).bits())
        );
    }

    #[test]
    fn compiles_goto_if_nil_else_pop() {
        // (lambda (x) (and x 7)) shape:
        //  0 StackRef(0); 1 GotoIfNilElsePop(3); 2 Constant(0=>7); 3 Return
        // x nil  -> jump keeping x -> return x (nil);
        // x else -> pop x, push 7 -> return 7.  A join with differing stacks (phi).
        let f = lower_leaf(
            &[
                Op::StackRef(0),
                Op::GotoIfNilElsePop(3),
                Op::Constant(0),
                Op::Return,
            ],
            &[Value::make_int(7)],
            1,
        )
        .unwrap();
        assert_eq!(
            f.call_for_test(&[Value::make_int(5)]),
            Some(Value::make_int(7).bits())
        );
        assert_eq!(f.call_for_test(&[Value::NIL]), Some(Value::NIL.bits()));
    }

    #[test]
    fn compiles_unconditional_goto() {
        //  0 Goto(1); 1 Constant(0=>5); 2 Return
        let f = lower_leaf(
            &[Op::Goto(1), Op::Constant(0), Op::Return],
            &[Value::make_int(5)],
            0,
        )
        .unwrap();
        assert_eq!(f.call_for_test(&[]), Some(Value::make_int(5).bits()));
    }

    #[test]
    fn jit_matches_interpreter_on_if_branch() {
        use crate::emacs_core::bytecode::Vm;
        use crate::emacs_core::eval::Context;
        let ops = [
            Op::StackRef(0),
            Op::GotoIfNil(4),
            Op::Constant(0),
            Op::Return,
            Op::Constant(1),
            Op::Return,
        ];
        let constants = [Value::make_int(10), Value::make_int(20)];
        for arg in [Value::T, Value::NIL, Value::make_int(3)] {
            let mut eval = Context::new_minimal_vm_harness();
            let mut f = ByteCodeFunction::new(LambdaParams {
                required: vec![crate::emacs_core::intern::SymId(1)],
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = ops.to_vec();
            f.constants = constants.to_vec().into();
            f.max_stack = 16;
            let want = {
                let mut vm = Vm::from_context(&mut eval);
                vm.execute(&f, vec![arg]).expect("interp runs if").bits()
            };
            let got = lower_leaf(&ops, &constants, 1)
                .unwrap()
                .call_for_test(&[arg]);
            assert_eq!(
                got,
                Some(want),
                "if-branch mismatch for arg bits {}",
                arg.bits()
            );
            // Also via the typed-MIR Tier-2 path (probe lower_mir_pure control flow).
            if let Ok(mir) = mir::build_mir(&ops, &constants, 1) {
                if let Ok(mleaf) = lower_mir_pure(&mir) {
                    let ctx_ptr = &mut eval as *mut Context as *mut u8;
                    if let NativeRun::Ok(bits) = mleaf.call(ctx_ptr, &[arg]) {
                        assert_eq!(
                            bits,
                            want,
                            "MIR if-branch mismatch for arg bits {}",
                            arg.bits()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn compiles_stackset() {
        // (lambda (a) (setq a (1+ a)) a):
        //  0 StackRef(0); 1 Add1; 2 StackSet(1); 3 StackRef(0); 4 Return
        let f = lower_leaf(
            &[
                Op::StackRef(0),
                Op::Add1,
                Op::StackSet(1),
                Op::StackRef(0),
                Op::Return,
            ],
            &[],
            1,
        )
        .unwrap();
        assert_eq!(
            f.call_for_test(&[Value::make_int(41)]),
            Some(Value::make_int(42).bits())
        );
    }

    #[test]
    fn compiles_discardn() {
        let consts = &[
            Value::make_int(10),
            Value::make_int(20),
            Value::make_int(30),
        ];
        // Non-preserve: push 10,20,30; discard top 2 -> 10.
        let np = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Constant(2),
                Op::DiscardN(2),
                Op::Return,
            ],
            consts,
        )
        .unwrap();
        assert_eq!(np.call_for_test(&[]), Some(Value::make_int(10).bits()));
        // Preserve TOS: push 10,20,30; discardN(2 | 0x80) keeps 30 -> 30.
        let pr = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Constant(2),
                Op::DiscardN(0x82),
                Op::Return,
            ],
            consts,
        )
        .unwrap();
        assert_eq!(pr.call_for_test(&[]), Some(Value::make_int(30).bits()));
    }

    #[test]
    fn compiles_countdown_loop_matches_interpreter() {
        use crate::emacs_core::bytecode::Vm;
        use crate::emacs_core::eval::Context;
        // (lambda (n) (while (> n 0) (setq n (1- n))) n) -> 0. A back-edge loop:
        //  0 StackRef(0); 1 Constant(0=>0); 2 Gtr; 3 GotoIfNil(8);
        //  4 StackRef(0); 5 Sub1; 6 StackSet(1); 7 Goto(0);
        //  8 StackRef(0); 9 Return
        let ops = [
            Op::StackRef(0),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(8),
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::Goto(0),
            Op::StackRef(0),
            Op::Return,
        ];
        let constants = [Value::make_int(0)];
        for n in [0i64, 1, 4, 9] {
            let mut eval = Context::new_minimal_vm_harness();
            let mut f = ByteCodeFunction::new(LambdaParams {
                required: vec![crate::emacs_core::intern::SymId(1)],
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = ops.to_vec();
            f.constants = constants.to_vec().into();
            f.max_stack = 16;
            let want = {
                let mut vm = Vm::from_context(&mut eval);
                vm.execute(&f, vec![Value::make_int(n)])
                    .expect("interp loop")
                    .bits()
            };
            let got = lower_leaf(&ops, &constants, 1)
                .unwrap()
                .call_for_test(&[Value::make_int(n)]);
            assert_eq!(got, Some(want), "loop mismatch for n={n}");
            assert_eq!(
                got,
                Some(Value::make_int(0).bits()),
                "countdown should reach 0 (n={n})"
            );
            // Also via the typed-MIR Tier-2 path (probe lower_mir_pure loops/back-edges).
            if let Ok(mir) = mir::build_mir(&ops, &constants, 1) {
                if let Ok(mleaf) = lower_mir_pure(&mir) {
                    let ctx_ptr = &mut eval as *mut Context as *mut u8;
                    if let NativeRun::Ok(bits) = mleaf.call(ctx_ptr, &[Value::make_int(n)]) {
                        assert_eq!(bits, want, "MIR loop mismatch for n={n}");
                    }
                }
            }
        }
    }

    #[test]
    fn mir_merge_phi_matches_interpreter() {
        use crate::emacs_core::bytecode::Vm;
        use crate::emacs_core::eval::Context;
        // (lambda (c) (1+ (if c 10 20))) — a diamond whose then/else values merge
        // at a common block (a phi), consumed by Add1. Tests build_mir's merge-phi.
        let ops = [
            Op::StackRef(0),  // 0: cond
            Op::GotoIfNil(4), // 1: pop; else->4, fall to then->2
            Op::Constant(0),  // 2: then: 10
            Op::Goto(5),      // 3
            Op::Constant(1),  // 4: else: 20 (leader); falls through to 5
            Op::Add1,         // 5: merge: 1+ phi (leader)
            Op::Return,       // 6
        ];
        let constants = [Value::make_int(10), Value::make_int(20)];
        for c in [Value::T, Value::NIL] {
            let mut eval = Context::new_minimal_vm_harness();
            let mut f = ByteCodeFunction::new(LambdaParams {
                required: vec![crate::emacs_core::intern::SymId(1)],
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = ops.to_vec();
            f.constants = constants.to_vec().into();
            f.max_stack = 16;
            let want = {
                let mut vm = Vm::from_context(&mut eval);
                vm.execute(&f, vec![c]).expect("interp diamond").bits()
            };
            if let Ok(mir) = mir::build_mir(&ops, &constants, 1) {
                if let Ok(mleaf) = lower_mir_pure(&mir) {
                    let ctx_ptr = &mut eval as *mut Context as *mut u8;
                    if let NativeRun::Ok(bits) = mleaf.call(ctx_ptr, &[c]) {
                        assert_eq!(
                            bits,
                            want,
                            "MIR merge-phi mismatch for cond bits {}",
                            c.bits()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn mir_multi_phi_merge_matches_interpreter() {
        use crate::emacs_core::bytecode::Vm;
        use crate::emacs_core::eval::Context;
        // A diamond where BOTH branches leave TWO values on the stack, so the
        // merge needs TWO phis; then Sub consumes them. Compares MIR to the
        // interpreter (ground truth) — no manual expected value.
        let ops = [
            Op::StackRef(0),  // 0: cond, depth 2
            Op::GotoIfNil(5), // 1: pop; else->5, fall->2  (depth 1)
            Op::Constant(0),  // 2: then: 10  (depth 2)
            Op::Constant(1),  // 3:        20 (depth 3)
            Op::Goto(7),      // 4: -> merge(7)
            Op::Constant(1),  // 5: else: 20 (depth 2) [leader]
            Op::Constant(0),  // 6:        10 (depth 3)  falls to 7
            Op::Sub,          // 7: merge: two phis -> Sub (depth 2) [leader]
            Op::Return,       // 8
        ];
        let constants = [Value::make_int(10), Value::make_int(20)];
        for c in [Value::T, Value::NIL] {
            let mut eval = Context::new_minimal_vm_harness();
            let mut f = ByteCodeFunction::new(LambdaParams {
                required: vec![crate::emacs_core::intern::SymId(1)],
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = ops.to_vec();
            f.constants = constants.to_vec().into();
            f.max_stack = 16;
            let want = {
                let mut vm = Vm::from_context(&mut eval);
                vm.execute(&f, vec![c]).expect("interp multi-phi").bits()
            };
            if let Ok(mir) = mir::build_mir(&ops, &constants, 1) {
                if let Ok(mleaf) = lower_mir_pure(&mir) {
                    let ctx_ptr = &mut eval as *mut Context as *mut u8;
                    if let NativeRun::Ok(bits) = mleaf.call(ctx_ptr, &[c]) {
                        assert_eq!(
                            bits,
                            want,
                            "MIR multi-phi-merge mismatch for cond bits {}",
                            c.bits()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn inline_pure_callee_lowers_and_runs() {
        // Caller (lambda (a) (sq a)) with sq = (lambda (x) (* x x)). build_mir(caller)
        // has an Opaque{Call} (so lower_mir_pure alone would bail); inlining sq's
        // pure body turns the caller into (* a a) — a pure MIR lower_mir_pure
        // handles — proving cross-call-boundary inlining + unboxing.
        let sq_sym = Value::symbol("jit-inline-sq");
        let sq_ops = [Op::Dup, Op::Mul, Op::Return];
        let caller_ops = [Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
        let caller_consts = [sq_sym];
        let mut m = mir::build_mir(&caller_ops, &caller_consts, 1).expect("caller MIR builds");
        let n = mir::inline_pure_single_block_callees(
            &mut m,
            &|v| {
                (v.bits() == sq_sym.bits())
                    .then(|| mir::build_mir(&sq_ops, &[], 1).expect("sq builds"))
            },
            16,
            &mut Vec::new(),
        );
        assert_eq!(n, 1, "sq must be inlined (the call replaced by its body)");
        let leaf = lower_mir_pure(&m).expect("inlined (now pure) MIR lowers");
        for a in [3i64, 7, -4, 0] {
            let arg = Value::make_int(a);
            match leaf.call(std::ptr::null_mut(), &[arg]) {
                NativeRun::Ok(bits) => assert_eq!(
                    bits,
                    Value::make_int(a * a).bits(),
                    "inlined (* a a) for a={a}"
                ),
                NativeRun::Deopt | NativeRun::DeoptAt(_) => {}
                other => panic!("a={a}: unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn inlined_callee_redefinition_rejits() {
        use crate::emacs_core::eval::Context;
        use crate::emacs_core::intern::SymId;
        // C = (lambda (x) (* x x)); F = (lambda (a) (C a)). Compiling F inlines C
        // -> (* a a). Redefine C = (1+ x) + bump the function epoch (as fset would):
        // F's cache entry is now stale -> re-JIT -> F computes the NEW C, (1+ a).
        // If the inline-epoch invalidation were broken, the stale inline would
        // return 25 instead of 6. Verifies the redefinition soundness.
        let mut ev = Context::new();
        let ctx = &mut ev as *mut Context;
        let c_sym = Value::symbol("jit-inline-redef-c");
        let crate::emacs_core::value::ValueKind::Symbol(c_id) = c_sym.kind() else {
            panic!("symbol");
        };
        let mk = |ops: Vec<Op>| {
            let mut c = ByteCodeFunction::new(LambdaParams {
                required: vec![SymId(1)],
                optional: Vec::new(),
                rest: None,
            });
            c.lexical = true;
            c.ops = ops;
            c.max_stack = 16;
            Value::make_bytecode(c)
        };
        ev.obarray
            .set_symbol_function_id(c_id, mk(vec![Op::Dup, Op::Mul, Op::Return]));
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(2)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
        f.constants = vec![c_sym].into();
        f.max_stack = 16;
        let f_val = Value::make_bytecode(f.clone());
        let r1 = crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(5)]);
        assert!(
            matches!(r1, Ok(Some(b)) if b == Value::make_int(25).bits()),
            "inlined (* 5 5) should be 25"
        );
        // Redefine C and bump the epoch (fset/defalias bump function_epoch).
        ev.obarray
            .set_symbol_function_id(c_id, mk(vec![Op::Add1, Op::Return]));
        ev.obarray.bump_function_epoch();
        let r2 = crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(5)]);
        assert!(
            matches!(r2, Ok(Some(b)) if b == Value::make_int(6).bits()),
            "after redefinition + epoch bump, re-JIT inlines the new C: (1+ 5) = 6"
        );
    }

    #[test]
    fn mir_call_lowering_runs_a_non_inlined_call() {
        use crate::emacs_core::eval::Context;
        use crate::emacs_core::intern::SymId;
        // C = identity; F = (1+ (C a)). The MIR has a NON-inlined Opaque{Call(C)}
        // plus a post-call 1+ guard. lower_mir_pure now lowers the call (generic
        // shim, vmctx-threaded) and routes the 1+ guard to PRECISE deopt. Verify
        // F(5) = 1+(id 5) = 6 end-to-end with a REAL Context (the precise-deopt
        // resume path is exercised by the NEOVM_JIT_FORCE_DEOPT gate).
        let mut ev = Context::new();
        let ctx = &mut ev as *mut Context;
        let c_sym = Value::symbol("jit-mir-call-c");
        let crate::emacs_core::value::ValueKind::Symbol(c_id) = c_sym.kind() else {
            panic!("symbol");
        };
        let mut c = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        c.lexical = true;
        c.ops = vec![Op::StackRef(0), Op::Return];
        c.max_stack = 16;
        ev.obarray
            .set_symbol_function_id(c_id, Value::make_bytecode(c));
        let f_ops = [
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Add1,
            Op::Return,
        ];
        let f_consts = [c_sym];
        let m = mir::build_mir(&f_ops, &f_consts, 1).expect("F builds");
        let leaf = lower_mir_pure(&m).expect("F lowers (non-inlined call + precise deopt)");
        assert!(
            leaf.has_side_effects,
            "a call-bearing MIR leaf is side-effecting (must never rerun-from-start)"
        );
        match leaf.call(ctx as *mut u8, &[Value::make_int(5)]) {
            NativeRun::Ok(bits) => {
                assert_eq!(bits, Value::make_int(6).bits(), "1+(id 5) = 6")
            }
            other => panic!("F(5): expected Ok(6), got {other:?}"),
        }
    }

    #[test]
    fn inline_plus_residual_call_takes_mir_tier() {
        use crate::emacs_core::eval::Context;
        use crate::emacs_core::intern::SymId;
        // F = (g (sq a)): sq = (* x x) [inlinable pure single-block]; g = a 2-block
        // (if y (1+ y) 0) [non-inlinable]. The inliner splices sq, leaving a residual
        // Call(g) + inline_epoch=Some, so the tier gate routes F to the MIR tier's
        // calls-slice (sq's arithmetic unboxed up to the g-call boundary). Verifies
        // the production compile path end-to-end: inlined + a residual call.
        let mut ev = Context::new();
        let ctx = &mut ev as *mut Context;
        let mk_sym = |name: &str| {
            let s = Value::symbol(name);
            let crate::emacs_core::value::ValueKind::Symbol(id) = s.kind() else {
                panic!("symbol");
            };
            (s, id)
        };
        let (sq_sym, sq_id) = mk_sym("jit-ir-sq");
        let (g_sym, g_id) = mk_sym("jit-ir-g");
        let mut sq = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        sq.lexical = true;
        sq.ops = vec![Op::Dup, Op::Mul, Op::Return];
        sq.max_stack = 16;
        ev.obarray
            .set_symbol_function_id(sq_id, Value::make_bytecode(sq));
        let mut g = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        g.lexical = true;
        // (if y (1+ y) 0) — two basic blocks, so callee_inlinable refuses it.
        g.ops = vec![
            Op::StackRef(0),
            Op::GotoIfNil(5),
            Op::StackRef(0),
            Op::Add1,
            Op::Return,
            Op::Constant(0),
            Op::Return,
        ];
        g.constants = vec![Value::make_int(0)].into();
        g.max_stack = 16;
        ev.obarray
            .set_symbol_function_id(g_id, Value::make_bytecode(g));
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(3)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![
            Op::Constant(0), // g
            Op::Constant(1), // sq
            Op::StackRef(2), // a
            Op::Call(1),     // (sq a)
            Op::Call(1),     // (g (sq a))
            Op::Return,
        ];
        f.constants = vec![g_sym, sq_sym].into();
        f.max_stack = 16;
        let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("F compiles");
        assert!(
            leaf.inline_epoch().is_some(),
            "F inlined sq -> took the MIR tier (not the baseline)"
        );
        assert!(
            leaf.has_side_effects,
            "F has a residual non-inlined call (g) lowered in the MIR tier"
        );
        // F(3) = g(sq(3)) = g(9) = 1+9 = 10.
        match leaf.call(ctx as *mut u8, &[Value::make_int(3)]) {
            NativeRun::Ok(bits) => {
                assert_eq!(bits, Value::make_int(10).bits(), "g(sq(3)) = 1+9 = 10")
            }
            other => panic!("F(3): expected Ok(10), got {other:?}"),
        }
    }

    #[test]
    fn precise_eviction_only_evicts_inlined_dependents() {
        use crate::emacs_core::eval::Context;
        use crate::emacs_core::intern::SymId;
        // F = (C a) inlines C = (* x x), so INLINE_DEPS records C -> {F}. Redefining
        // an UNRELATED symbol D must NOT evict F (precision: no churn). Redefining C
        // DOES evict F (so it re-JITs against the new C) and clears the dep entry.
        // The coarse inline_epoch backstop would re-JIT regardless; this asserts the
        // PRECISE eviction (only dependents are evicted, eagerly).
        let mut ev = Context::new();
        let ctx = &mut ev as *mut Context;
        let mk_sym = |name: &str| {
            let s = Value::symbol(name);
            let crate::emacs_core::value::ValueKind::Symbol(id) = s.kind() else {
                panic!("symbol");
            };
            (s, id)
        };
        let mk_fn = |ops: Vec<Op>, consts: Vec<Value>| {
            let mut bf = ByteCodeFunction::new(LambdaParams {
                required: vec![SymId(1)],
                optional: Vec::new(),
                rest: None,
            });
            bf.lexical = true;
            bf.ops = ops;
            bf.constants = consts.into();
            bf.max_stack = 16;
            bf
        };
        let (c_sym, c_id) = mk_sym("jit-pe-c");
        let (_d_sym, d_id) = mk_sym("jit-pe-d");
        ev.obarray.set_symbol_function_id(
            c_id,
            Value::make_bytecode(mk_fn(vec![Op::Dup, Op::Mul, Op::Return], vec![])),
        );
        ev.obarray.set_symbol_function_id(
            d_id,
            Value::make_bytecode(mk_fn(vec![Op::Add1, Op::Return], vec![])),
        );
        let f = mk_fn(
            vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
            vec![c_sym],
        );
        let f_val = Value::make_bytecode(f.clone());
        // Compile F (inlines C).
        let _ = crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(4)]);
        let f_id = f.jit_runtime().compiled_id_or_assign();
        assert!(
            crate::emacs_core::jit::cache::is_compiled_for_test(f_id),
            "F is JIT-cached after compile"
        );
        assert_eq!(
            crate::emacs_core::jit::cache::inline_dependent_count_for_test(c_id),
            1,
            "F recorded as inlining C"
        );
        // Redefine an UNRELATED symbol D -> F must NOT be evicted (precision).
        ev.obarray.set_symbol_function_id(
            d_id,
            Value::make_bytecode(mk_fn(vec![Op::Sub1, Op::Return], vec![])),
        );
        assert!(
            crate::emacs_core::jit::cache::is_compiled_for_test(f_id),
            "unrelated redefinition (D) must NOT evict F"
        );
        // Redefine the inlined callee C -> F evicted + dep entry cleared.
        ev.obarray.set_symbol_function_id(
            c_id,
            Value::make_bytecode(mk_fn(vec![Op::Add1, Op::Return], vec![])),
        );
        assert!(
            !crate::emacs_core::jit::cache::is_compiled_for_test(f_id),
            "redefining the inlined callee C evicts F (precise)"
        );
        assert_eq!(
            crate::emacs_core::jit::cache::inline_dependent_count_for_test(c_id),
            0,
            "C's dep entry cleared on eviction"
        );
    }

    #[test]
    fn mir_scalar_replaces_non_escaping_cons() {
        // F = (car (cons a b)) -> a, with the cons ELIDED (escape analysis, pure
        // body -> MIR tier, zero allocation). Previously the cons bailed the whole
        // body to the baseline. Verify it lowers (no bail) and returns the car.
        let ops = [
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Cons,
            Op::Car,
            Op::Return,
        ];
        let m = mir::build_mir(&ops, &[], 2).expect("builds");
        let leaf = lower_mir_pure(&m).expect("scalar-replaced cons lowers (no bail)");
        assert!(
            !leaf.has_side_effects,
            "a pure scalar-replaced body has no side effects (no allocation/call)"
        );
        match leaf.call_for_test(&[Value::make_int(3), Value::make_int(5)]) {
            Some(bits) => assert_eq!(bits, Value::make_int(3).bits(), "(car (cons 3 5)) = 3"),
            None => panic!("expected Some(3) — no deopt"),
        }
    }

    #[test]
    fn mir_allocates_escaping_cons() {
        use crate::emacs_core::eval::Context;
        // F = (cons a b), returned -> the cons ESCAPES -> heap-allocated in the MIR
        // tier via neovm_jit_cons (previously this body bailed to the baseline).
        // Verify it lowers (no bail) + runs to a cons; the contents (3 . 5) are
        // covered by the differential gate. Real Context (the allocation runs).
        let mut ev = Context::new();
        let ctx = &mut ev as *mut Context;
        let ops = [Op::StackRef(1), Op::StackRef(1), Op::Cons, Op::Return];
        let m = mir::build_mir(&ops, &[], 2).expect("builds");
        let leaf = lower_mir_pure(&m).expect("escaping cons lowers (no bail)");
        assert!(
            !leaf.has_side_effects,
            "a cons allocation is a GC safepoint, not a side effect (no precise deopt)"
        );
        match leaf.call(ctx as *mut u8, &[Value::make_int(3), Value::make_int(5)]) {
            NativeRun::Ok(bits) => assert!(
                matches!(
                    Value::from_bits(bits).kind(),
                    crate::emacs_core::value::ValueKind::Cons
                ),
                "(cons 3 5) allocates a cons"
            ),
            other => panic!("F(3,5): expected Ok(cons), got {other:?}"),
        }
    }

    #[test]
    fn backedge_polls_quit_like_the_interpreter() {
        use crate::emacs_core::bytecode::Vm;
        use crate::emacs_core::eval::Context;
        // Countdown loop with enough iterations (> 255 backward jumps) for the
        // u8 quit counter to wrap and trigger the back-edge service poll.
        let ops = [
            Op::StackRef(0),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(8),
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::Goto(0),
            Op::StackRef(0),
            Op::Return,
        ];
        let constants = [Value::make_int(0)];
        let mut ev = Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut Context as *mut u8;
        let leaf = lower_leaf(&ops, &constants, 1).unwrap();

        // Flag clear: the loop runs to completion natively (polls return OK).
        assert_eq!(
            leaf.call(ctx_ptr, &[Value::make_int(1000)]),
            NativeRun::Ok(Value::make_int(0).bits())
        );

        // Flag set: the wrap poll must signal quit out of native code...
        ev.set_quit_flag_value(Value::T);
        assert_eq!(
            leaf.call(ctx_ptr, &[Value::make_int(1000)]),
            NativeRun::Signal,
            "C-g must interrupt a compiled loop"
        );
        assert!(take_pending_flow().is_some(), "quit Flow stashed");

        // ...exactly like the interpreter on the same body (the poll clears the
        // flag, so re-set it for the oracle run).
        ev.set_quit_flag_value(Value::T);
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.to_vec();
        f.constants = constants.to_vec().into();
        f.max_stack = 16;
        let interp = {
            let mut vm = Vm::from_context(&mut ev);
            vm.execute(&f, vec![Value::make_int(1000)])
        };
        assert!(interp.is_err(), "interpreter quits on the same loop");

        // Flag cleared by the quit: the loop completes again.
        assert_eq!(
            leaf.call(ctx_ptr, &[Value::make_int(1000)]),
            NativeRun::Ok(Value::make_int(0).bits())
        );
    }

    #[test]
    fn compiles_save_excursion_with_unwind_semantics() {
        use crate::emacs_core::eval::Context;
        let mut ev = Context::new();
        let ctx_ptr = &mut ev as *mut Context as *mut u8;
        ev.eval_str(r#"(insert "hello world")"#).expect("insert");
        let specpdl_before = ev.specpdl.len();
        let constants = [
            Value::symbol("goto-char"),
            Value::make_int(1),
            Value::symbol("point"),
        ];

        // Balanced: (save-excursion (goto-char 1)) then (point) — restored.
        let balanced = lower_nullary_leaf(
            &[
                Op::SaveExcursion,
                Op::Constant(0),
                Op::Constant(1),
                Op::Call(1),
                Op::Pop,
                Op::Unbind(1),
                Op::Constant(2),
                Op::Call(0),
                Op::Return,
            ],
            &constants,
        )
        .unwrap();
        assert_eq!(
            balanced.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(12).bits()),
            "point must be restored by the Unbind"
        );
        assert_eq!(ev.specpdl.len(), specpdl_before);

        // Early return with the record dangling: the frame unwind restores it.
        let dangling = lower_nullary_leaf(
            &[
                Op::SaveExcursion,
                Op::Constant(0),
                Op::Constant(1),
                Op::Call(1),
                Op::Return,
            ],
            &constants,
        )
        .unwrap();
        assert_eq!(
            dangling.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(1).bits())
        );
        assert_eq!(ev.specpdl.len(), specpdl_before, "frame unwind pops record");
        let point_now =
            lower_nullary_leaf(&[Op::Constant(2), Op::Call(0), Op::Return], &constants).unwrap();
        assert_eq!(
            point_now.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(12).bits()),
            "point must be restored by the frame unwind too"
        );

        // SaveCurrentBuffer / SaveRestriction: records create + frame-unwind
        // cleanly (same shim/record machinery; arms mirrored 1:1).
        for op in [Op::SaveCurrentBuffer, Op::SaveRestriction] {
            let mech = lower_nullary_leaf(&[op, Op::Nil, Op::Return], &[]).unwrap();
            assert_eq!(mech.call(ctx_ptr, &[]), NativeRun::Ok(Value::NIL.bits()));
            assert_eq!(ev.specpdl.len(), specpdl_before);
        }

        // Precise deopt: a guard after the Save* record compiles and runs
        // (a failing guard would resume the interpreter mid-frame with the
        // record still registered).
        let after = lower_nullary_leaf(
            &[Op::SaveExcursion, Op::Constant(1), Op::Add1, Op::Return],
            &constants,
        )
        .expect("guard after a side effect compiles under precise deopt");
        match after.call(ctx_ptr, &[]) {
            NativeRun::Ok(_) => {}
            other => panic!("guard-after-save must run, got {other:?}"),
        }
        assert_eq!(ev.specpdl.len(), specpdl_before);
    }

    #[test]
    fn compiles_trivial_natives_carsafe_maxmin_throw_numpreds() {
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let t = NativeRun::Ok(Value::T.bits());
        let nil = NativeRun::Ok(Value::NIL.bits());
        let run1 = |op: Op, v: Value, ctx: *mut u8| {
            lower_nullary_leaf(&[Op::Constant(0), op, Op::Return], &[v])
                .unwrap()
                .call(ctx, &[])
        };

        // car-safe / cdr-safe: total — non-cons (incl. fixnums) -> nil, no deopt.
        let cons = Value::cons(Value::make_int(3), Value::make_int(4));
        assert_eq!(
            run1(Op::CarSafe, cons, ctx_ptr),
            NativeRun::Ok(Value::make_int(3).bits())
        );
        assert_eq!(
            run1(Op::CdrSafe, cons, ctx_ptr),
            NativeRun::Ok(Value::make_int(4).bits())
        );
        assert_eq!(run1(Op::CarSafe, Value::make_int(9), ctx_ptr), nil);
        assert_eq!(run1(Op::CdrSafe, Value::T, ctx_ptr), nil);
        assert_eq!(run1(Op::CarSafe, Value::NIL, ctx_ptr), nil);

        // max / min: fixnum fast path keeps the original tagged operand;
        // non-fixnum deopts to the interpreter's coercing builtin.
        let run2 = |op: Op, a: Value, b: Value, ctx: *mut u8| {
            lower_nullary_leaf(&[Op::Constant(0), Op::Constant(1), op, Op::Return], &[a, b])
                .unwrap()
                .call(ctx, &[])
        };
        assert_eq!(
            run2(Op::Max, Value::make_int(3), Value::make_int(7), ctx_ptr),
            NativeRun::Ok(Value::make_int(7).bits())
        );
        assert_eq!(
            run2(Op::Max, Value::make_int(-3), Value::make_int(-7), ctx_ptr),
            NativeRun::Ok(Value::make_int(-3).bits())
        );
        assert_eq!(
            run2(Op::Min, Value::make_int(3), Value::make_int(7), ctx_ptr),
            NativeRun::Ok(Value::make_int(3).bits())
        );
        // Non-fixnum operand: precise deopt at the Max op with the operands
        // still on the captured stack.
        match run2(Op::Max, Value::make_float(1.5), Value::make_int(7), ctx_ptr) {
            NativeRun::DeoptAt(resume) => {
                let DeoptResume { pc, ref stack, .. } = *resume;
                assert_eq!(pc, 2, "deopt at the Max op");
                assert_eq!(stack[1], Value::make_int(7));
            }
            other => panic!("expected a precise deopt, got {other:?}"),
        }

        // integerp / numberp: fixnum natively; float/bignum via the slow shim.
        assert_eq!(run1(Op::Integerp, Value::make_int(5), ctx_ptr), t);
        assert_eq!(run1(Op::Integerp, Value::make_float(1.5), ctx_ptr), nil);
        assert_eq!(run1(Op::Integerp, Value::T, ctx_ptr), nil);
        assert_eq!(run1(Op::Numberp, Value::make_int(5), ctx_ptr), t);
        assert_eq!(run1(Op::Numberp, Value::make_float(1.5), ctx_ptr), t);
        assert_eq!(run1(Op::Numberp, Value::NIL, ctx_ptr), nil);

        // throw: stashes Flow::Throw and exits via the signal path.
        let tag = Value::symbol("jit-throw-tag");
        let thrown = lower_nullary_leaf(
            &[Op::Constant(0), Op::Constant(1), Op::Throw],
            &[tag, Value::make_int(42)],
        )
        .unwrap();
        assert_eq!(thrown.call(ctx_ptr, &[]), NativeRun::Signal);
        match take_pending_flow().expect("throw Flow stashed") {
            Flow::Throw(thrown) => {
                assert_eq!(thrown.tag, tag);
                assert_eq!(thrown.value, Value::make_int(42));
            }
            other => panic!("expected Flow::Throw, got {other:?}"),
        }
    }

    #[test]
    fn compiles_direct_builtin_ops() {
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let ok_int = |n: i64| NativeRun::Ok(Value::make_int(n).bits());
        let run = |ops: &[Op], consts: &[Value], ctx: *mut u8| {
            lower_nullary_leaf(ops, consts).unwrap().call(ctx, &[])
        };

        // length
        let list = Value::cons(
            Value::make_int(1),
            Value::cons(
                Value::make_int(2),
                Value::cons(Value::make_int(3), Value::NIL),
            ),
        );
        assert_eq!(
            run(&[Op::Constant(0), Op::Length, Op::Return], &[list], ctx_ptr),
            ok_int(3)
        );

        // nth: (nth 1 '(1 2 3)) = 2 — operand order matches the arm (n, list).
        assert_eq!(
            run(
                &[Op::Constant(0), Op::Constant(1), Op::Nth, Op::Return],
                &[Value::make_int(1), list],
                ctx_ptr
            ),
            ok_int(2)
        );

        // memq: (memq 'b '(a b c)) -> the tail whose car is 'b.
        let (a, bsym, c) = (
            Value::symbol("jit-memq-a"),
            Value::symbol("jit-memq-b"),
            Value::symbol("jit-memq-c"),
        );
        let abc = Value::cons(a, Value::cons(bsym, Value::cons(c, Value::NIL)));
        let NativeRun::Ok(tail) = run(
            &[Op::Constant(0), Op::Constant(1), Op::Memq, Op::Return],
            &[bsym, abc],
            ctx_ptr,
        ) else {
            panic!("memq must succeed");
        };
        assert_eq!(Value::from_bits(tail).cons_car(), bsym);

        // equal on structurally-equal fresh lists -> t.
        let l1 = Value::cons(
            Value::make_int(1),
            Value::cons(Value::make_int(2), Value::NIL),
        );
        let l2 = Value::cons(
            Value::make_int(1),
            Value::cons(Value::make_int(2), Value::NIL),
        );
        assert_eq!(
            run(
                &[Op::Constant(0), Op::Constant(1), Op::Equal, Op::Return],
                &[l1, l2],
                ctx_ptr
            ),
            NativeRun::Ok(Value::T.bits())
        );

        // setcar mutates through the SATB-barriered builtin; result = new car.
        let cell = Value::cons(Value::make_int(10), Value::make_int(20));
        assert_eq!(
            run(
                &[Op::Constant(0), Op::Constant(1), Op::Setcar, Op::Return],
                &[cell, Value::make_int(99)],
                ctx_ptr
            ),
            ok_int(99)
        );
        assert_eq!(cell.cons_car(), Value::make_int(99), "mutation visible");

        // Precise deopt: a guard after the mutation compiles and runs —
        // (1+ (setcar cell 1)) = 2 with the mutation visible.
        assert_eq!(
            run(
                &[
                    Op::Constant(0),
                    Op::Constant(1),
                    Op::Setcar,
                    Op::Add1,
                    Op::Return,
                ],
                &[cell, Value::make_int(1)],
                ctx_ptr
            ),
            ok_int(2)
        );
        assert_eq!(cell.cons_car(), Value::make_int(1), "mutation visible");

        // symbol-value: live read + void-variable signal.
        let var = Value::symbol("jit-bw-var");
        let crate::emacs_core::value::ValueKind::Symbol(var_id) = var.kind() else {
            panic!("symbol expected");
        };
        ev.obarray.set_symbol_value_id(var_id, Value::make_int(5));
        assert_eq!(
            run(
                &[Op::Constant(0), Op::SymbolValue, Op::Return],
                &[var],
                ctx_ptr
            ),
            ok_int(5)
        );
        let unbound = Value::symbol("jit-bw-unbound");
        assert_eq!(
            run(
                &[Op::Constant(0), Op::SymbolValue, Op::Return],
                &[unbound],
                ctx_ptr
            ),
            NativeRun::Signal
        );
        assert!(take_pending_flow().is_some());

        // put / get round-trip on a plist.
        let psym = Value::symbol("jit-bw-plist");
        let prop = Value::symbol("jit-bw-prop");
        assert_eq!(
            run(
                &[
                    Op::Constant(0),
                    Op::Constant(1),
                    Op::Constant(2),
                    Op::Put,
                    Op::Return,
                ],
                &[psym, prop, Value::make_int(7)],
                ctx_ptr
            ),
            ok_int(7)
        );
        assert_eq!(
            run(
                &[Op::Constant(0), Op::Constant(1), Op::Get, Op::Return],
                &[psym, prop],
                ctx_ptr
            ),
            ok_int(7)
        );

        // aref on a string; string-equal.
        let s = Value::string("abc");
        assert_eq!(
            run(
                &[Op::Constant(0), Op::Constant(1), Op::Aref, Op::Return],
                &[s, Value::make_int(1)],
                ctx_ptr
            ),
            ok_int('b' as i64)
        );
        let s2 = Value::string("abc");
        assert_eq!(
            run(
                &[
                    Op::Constant(0),
                    Op::Constant(1),
                    Op::StringEqual,
                    Op::Return
                ],
                &[s, s2],
                ctx_ptr
            ),
            NativeRun::Ok(Value::T.bits())
        );
    }

    /// The bit-op intrinsics (`logand`/`logior`/`logxor`/`ash`/`lognot`)
    /// JIT-compiled: the armed fast shim computes the native op == the interpreter
    /// (including negative two's-complement fixnums and `ash` shifts), a non-fixnum
    /// arg deopts to the generic fallback (same wrong-type signal), and an `ash`
    /// LEFT-shift that overflows fixnum range deopts to the generic bignum.
    #[test]
    fn arith_intrinsic_bitops_jit_match_interp_and_deopt() {
        use crate::emacs_core::eval::Context;
        // This test targets the armed SHIM path (asserts SUBR_SPEC_FAST_COUNT); pin
        // Level-B inline OFF so and/or/xor/lognot go through the shim deterministically
        // regardless of NEOVM_JIT_INLINE_ARITH (the inline path is covered separately).
        force_inline_arith_for_test(false);
        let mut ev = Context::new(); // binds logand/logior/logxor/ash/lognot
        let ctx = &mut ev as *mut Context as *mut u8;
        // An N-arg body `(OP p0 [p1])`: Constant(OP); StackRef(nargs)*nargs; Call(nargs).
        let mk = |op_name: &str, nargs: usize, ob: &crate::emacs_core::symbol::Obarray| {
            let mut ops = vec![Op::Constant(0)];
            for _ in 0..nargs {
                ops.push(Op::StackRef(nargs as u16));
            }
            ops.push(Op::Call(nargs as u16));
            ops.push(Op::Return);
            let mut f = ByteCodeFunction::new(LambdaParams {
                required: (0..nargs).map(|i| SymId(1 + i as u32)).collect(),
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = ops;
            f.constants = vec![Value::symbol(op_name)].into();
            f.max_stack = 16;
            compile_bytecode_function_with(&f, Some(ob)).expect("bit-op body compiles")
        };
        let int = |n: i64| Value::make_int(n);
        // 2-arg ops with a fixnum result.
        for (name, a, b, want) in [
            ("logand", 12, 10, 8),
            ("logior", 12, 10, 14),
            ("logxor", 12, 10, 6),
            ("logand", -1, 5, 5),  // two's-complement: -1 is all-ones
            ("logior", -8, 3, -5), // sign bit survives
            ("logxor", -1, -1, 0),
            ("ash", 3, 4, 48),    // left shift: 3 << 4
            ("ash", 5, 0, 5),     // no shift
            ("ash", 256, -3, 32), // right shift: 256 >> 3
            ("ash", -7, -1, -4),  // arithmetic right shift: floor(-3.5) = -4
        ] {
            let leaf = mk(name, 2, &ev.obarray);
            #[cfg(debug_assertions)]
            let fast0 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
            let r = leaf.call(ctx, &[int(a), int(b)]);
            assert_eq!(
                r,
                NativeRun::Ok(int(want).bits()),
                "({name} {a} {b}) = {want}"
            );
            #[cfg(debug_assertions)]
            assert!(
                SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed) > fast0,
                "({name} {a} {b}): armed fast shim must fire"
            );
        }
        // lognot (1-arg): !n of a fixnum is always a fixnum.
        for (a, want) in [(5i64, -6i64), (-1, 0), (0, -1)] {
            let leaf = mk("lognot", 1, &ev.obarray);
            #[cfg(debug_assertions)]
            let fast0 = SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed);
            assert_eq!(
                leaf.call(ctx, &[int(a)]),
                NativeRun::Ok(int(want).bits()),
                "(lognot {a}) = {want}"
            );
            #[cfg(debug_assertions)]
            assert!(
                SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed) > fast0,
                "(lognot {a}): armed fast shim must fire"
            );
        }
        // ash LEFT-shift overflowing fixnum range -> NEED_GENERIC -> generic makes
        // the bignum 2^100; result must equal the interpreter's (a bignum, != any
        // fixnum), taking the generic bounce not the fast path.
        {
            let leaf = mk("ash", 2, &ev.obarray);
            #[cfg(debug_assertions)]
            let gen0 = SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed);
            let NativeRun::Ok(bits) = leaf.call(ctx, &[int(1), int(100)]) else {
                panic!("(ash 1 100) must return Ok (a bignum via generic)");
            };
            let got = Value::from_bits(bits);
            let interp = ev.eval_str("(ash 1 100)").expect("interp ash");
            assert!(
                crate::emacs_core::value::equal_value(&got, &interp, 0),
                "(ash 1 100): JIT {got:?} != interp {interp:?} (both should be 2^100)"
            );
            assert!(got.as_bignum().is_some(), "(ash 1 100) is a bignum");
            #[cfg(debug_assertions)]
            assert!(
                SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed) > gen0,
                "(ash 1 100): overflow took the NEED_GENERIC bounce"
            );
        }
        // Non-fixnum arg (a cons): as_fixnum → None → STATUS_NEED_GENERIC → the
        // generic fallback runs the real logand, which signals wrong-type — the
        // SAME as the interpreter. Proves the deopt path is wired, GC-safe.
        let leaf = mk("logand", 2, &ev.obarray);
        let cons = Value::cons(int(1), Value::NIL);
        #[cfg(debug_assertions)]
        let gen0 = SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed);
        assert_eq!(
            leaf.call(ctx, &[cons, int(5)]),
            NativeRun::Signal,
            "(logand '(1) 5) signals wrong-type via the generic fallback"
        );
        assert!(take_pending_flow().is_some(), "the signal was stashed");
        #[cfg(debug_assertions)]
        assert!(
            SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed) > gen0,
            "the non-fixnum arg took the NEED_GENERIC bounce"
        );
    }

    /// LEVEL-B: with inline-arith forced on, logand/logior/logxor/lognot compile
    /// to inline native ops (== interpreter, incl. negatives); the leaf records an
    /// inline_epoch (redefinition eviction); `ash` stays on the shim (no
    /// inline_epoch); and a non-fixnum arg DEOPTS (never wrongly computes inline).
    #[test]
    fn arith_intrinsic_inline_level_b_matches_interp() {
        use crate::emacs_core::eval::Context;
        force_inline_arith_for_test(true);
        let mut ev = Context::new();
        let ctx = &mut ev as *mut Context as *mut u8;
        let mk = |op_name: &str, nargs: usize, ob: &crate::emacs_core::symbol::Obarray| {
            let mut ops = vec![Op::Constant(0)];
            for _ in 0..nargs {
                ops.push(Op::StackRef(nargs as u16));
            }
            ops.push(Op::Call(nargs as u16));
            ops.push(Op::Return);
            let mut f = ByteCodeFunction::new(LambdaParams {
                required: (0..nargs).map(|i| SymId(1 + i as u32)).collect(),
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = ops;
            f.constants = vec![Value::symbol(op_name)].into();
            f.max_stack = 16;
            compile_bytecode_function_with(&f, Some(ob)).expect("inline bit-op body compiles")
        };
        let int = |n: i64| Value::make_int(n);
        for (name, nargs, args, want) in [
            ("logand", 2usize, vec![12i64, 10], 8i64),
            ("logior", 2, vec![12, 10], 14),
            ("logxor", 2, vec![12, 10], 6),
            ("logand", 2, vec![-1, 5], 5),
            ("logior", 2, vec![-8, 3], -5),
            ("logxor", 2, vec![-1, -1], 0),
            ("logxor", 2, vec![-1, 0], -1), // tag-restore on a negative result
            ("lognot", 1, vec![5], -6),
            ("lognot", 1, vec![-1], 0),
            ("lognot", 1, vec![0], -1),
            (
                "lognot",
                1,
                vec![Value::MOST_NEGATIVE_FIXNUM],
                Value::MOST_POSITIVE_FIXNUM,
            ),
        ] {
            let leaf = mk(name, nargs, &ev.obarray);
            assert!(
                !leaf.inline_deps().is_empty(),
                "{name} inline leaf must register a redefinition-eviction dep"
            );
            let argv: Vec<Value> = args.iter().map(|&n| int(n)).collect();
            assert_eq!(
                leaf.call(ctx, &argv),
                NativeRun::Ok(int(want).bits()),
                "({name} {args:?}) inline = {want}"
            );
        }
        // ash is NOT inlined: stays on the self-arming shim, so no inline dep.
        let ash_leaf = mk("ash", 2, &ev.obarray);
        assert!(
            ash_leaf.inline_deps().is_empty(),
            "ash stays on the shim — no inline redefinition dep"
        );
        assert_eq!(
            ash_leaf.call(ctx, &[int(3), int(4)]),
            NativeRun::Ok(int(48).bits())
        );
        // A non-fixnum arg on an inline op DEOPTS (guard fails) — it never runs the
        // inline `&` on a non-fixnum; the caller re-runs the real logand interpreted.
        let leaf = mk("logand", 2, &ev.obarray);
        let cons = Value::cons(int(1), Value::NIL);
        assert!(
            matches!(
                leaf.call(ctx, &[cons, int(5)]),
                NativeRun::Deopt | NativeRun::DeoptAt(_)
            ),
            "(logand '(1) 5) must deopt, not compute inline"
        );
        force_inline_arith_for_test(false);
    }

    /// OSR compile path: a counting loop `(while (< i 5) (setq i (1+ i)))`
    /// compiled with an ALTERNATE ENTRY at the loop-header pc. Called with a
    /// synthetic operand stack (the live `i`), it must resume the loop mid-flight
    /// and return the same result as running from the start — for i seeded at 0
    /// (full loop), 3 (partial), 5 (exit immediately), and 10 (past the bound).
    #[test]
    fn osr_entry_resumes_loop_from_seeded_stack() {
        use crate::emacs_core::eval::Context;
        let mut ev = Context::new();
        let ctx = &mut ev as *mut Context as *mut u8;
        // pcs:  0 Constant0(=0)   -- i = 0  (prologue, UNREACHABLE under OSR)
        //       1 StackRef0       -- loop header / OSR entry, entry_depth = 1
        //       2 Constant1(=5)
        //       3 Lss             -- i < 5
        //       4 GotoIfNil(9)
        //       5 StackRef0
        //       6 Add1            -- i + 1
        //       7 StackSet1       -- i = i+1
        //       8 Goto(1)         -- backward branch (loop)
        //       9 Return          -- return i
        let ops = vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::Constant(1),
            Op::Lss,
            Op::GotoIfNil(9),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(1),
            Op::Return,
        ];
        let constants = vec![Value::make_int(0), Value::make_int(5)];
        const OSR_PC: usize = 1;
        let leaf = lower_leaf_full_osr(
            &ops,
            &constants,
            0,
            None,
            Some(&ev.obarray),
            Some(OSR_PC),
            0,
        )
        .expect("OSR variant compiles (alternate loop-header entry)");
        // Seed the operand stack = [i]; the OSR entry resumes the loop from `i`.
        for (seed, want) in [(0i64, 5i64), (3, 5), (5, 5), (10, 10)] {
            let args = [Value::make_int(seed).bits() as i64];
            match leaf.call_premarshaled(ctx, args.as_ptr()) {
                NativeRun::Ok(bits) => assert_eq!(
                    Value::from_bits(bits),
                    Value::make_int(want),
                    "OSR resume from i={seed} must return {want}"
                ),
                other => panic!("OSR run from i={seed}: expected Ok({want}), got {other:?}"),
            }
        }
    }

    /// OSR end-to-end: a once-called summation loop `(let ((acc 0)(i 0)) (while
    /// (< i n) (setq acc (+ acc i)) (setq i (1+ i))) acc)` run through the
    /// INTERPRETER with OSR forced on + the function pinned hot — the hot back-edge
    /// transfers into native code mid-loop and finishes there. The result must
    /// equal the pure interpreter (OSR off), for n large enough to wrap the
    /// back-edge counter (256) and trigger the transfer.
    #[test]
    fn osr_transfers_hot_loop_and_matches_interpreter() {
        use crate::emacs_core::bytecode::vm::Vm;
        use crate::emacs_core::eval::Context;
        // sum(0..n-1) loop; see osr_entry_resumes_loop_from_seeded_stack for the shape.
        let mk = || {
            let mut f = ByteCodeFunction::new(LambdaParams {
                required: vec![SymId(1)], // n
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = vec![
                Op::Constant(0), // acc = 0
                Op::Constant(0), // i = 0
                Op::StackRef(0), // 2: L_header (OSR entry) — i
                Op::StackRef(3), // n
                Op::Lss,         // i < n
                Op::GotoIfNil(14),
                Op::StackRef(1), // acc
                Op::StackRef(1), // i
                Op::Add,         // acc + i
                Op::StackSet(2), // acc = acc + i
                Op::StackRef(0), // i
                Op::Add1,        // i + 1
                Op::StackSet(1), // i = i + 1
                Op::Goto(2),     // back to L_header
                Op::StackRef(1), // 14: L_end — acc
                Op::Return,
            ];
            f.constants = vec![Value::make_int(0)].into();
            f.max_stack = 16;
            f.seal_hand_assembled_ops();
            f
        };
        let n = 2000i64;
        let want = Value::make_int(n * (n - 1) / 2); // sum 0..n-1

        // OSR OFF: pure interpreter baseline.
        let mut ev = Context::new();
        crate::emacs_core::jit::force_osr_for_test(false);
        let f_off = mk();
        let off = Vm::from_context(&mut ev)
            .execute(&f_off, vec![Value::make_int(n)])
            .expect("interp run");
        assert_eq!(off, want, "interpreter sum(0..{}) baseline", n - 1);

        // OSR ON + pinned hot: the hot back-edge transfers into native mid-loop.
        crate::emacs_core::jit::force_osr_for_test(true);
        let f_on = mk();
        f_on.jit_runtime().set_hot_for_test();
        let before = crate::emacs_core::jit::cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed);
        let on = Vm::from_context(&mut ev)
            .execute(&f_on, vec![Value::make_int(n)])
            .expect("OSR run");
        assert_eq!(on, want, "OSR sum(0..{}) must match the interpreter", n - 1);
        assert!(
            crate::emacs_core::jit::cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed) > before,
            "the OSR transfer must actually fire (not the interpreter finishing the loop)"
        );
        crate::emacs_core::jit::force_osr_for_test(false);
    }

    #[test]
    fn compiles_unwind_protect_pop() {
        use crate::emacs_core::eval::Context;
        let mut ev = Context::new();
        let ctx_ptr = &mut ev as *mut Context as *mut u8;
        // NOTE: the opcode's operand is a LIST of cleanup forms (sf_progn_value),
        // exactly what the byte-compiler pushes for (unwind-protect BODY FORMS..).
        let cleanup = ev
            .eval_str("'((setq jit-up-ran t))")
            .expect("cleanup forms");
        // The cleanup form list lives in a Rust local across the next eval
        // and the native calls below; root it or a stress-GC frees it.
        ev.push_specpdl_root(cleanup);
        ev.eval_str("(setq jit-up-ran nil)").expect("flag init");
        let specpdl_before = ev.specpdl.len();
        let consts = [
            cleanup,
            Value::make_int(7),
            Value::symbol("jit-up-no-such-fn"),
        ];

        // Balanced: the matching Unbind runs the cleanup.
        let balanced = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::UnwindProtectPop,
                Op::Constant(1),
                Op::Unbind(1),
                Op::Return,
            ],
            &consts,
        )
        .unwrap();
        assert_eq!(
            balanced.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(7).bits())
        );
        assert_eq!(
            ev.eval_str("jit-up-ran").unwrap(),
            Value::T,
            "cleanup ran on the balanced path"
        );
        assert_eq!(ev.specpdl.len(), specpdl_before);

        // Signal inside the protected extent: the frame unwind runs the cleanup.
        ev.eval_str("(setq jit-up-ran nil)").expect("flag reset");
        let signaled = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::UnwindProtectPop,
                Op::Constant(2),
                Op::Call(0),
                Op::Return,
            ],
            &consts,
        )
        .unwrap();
        assert_eq!(signaled.call(ctx_ptr, &[]), NativeRun::Signal);
        assert!(take_pending_flow().is_some());
        assert_eq!(
            ev.eval_str("jit-up-ran").unwrap(),
            Value::T,
            "cleanup ran on the signal path"
        );
        assert_eq!(ev.specpdl.len(), specpdl_before);
    }

    /// MIR Tier-2 Phase 4b: a pure body lowered bytecode→MIR→CLIF produces the
    /// SAME native result as the interpreter — the first end-to-end proof of the
    /// MIR pipeline.
    #[test]
    fn mir_pure_lowering_matches_interpreter() {
        use crate::emacs_core::bytecode::ByteCodeFunction;
        use crate::emacs_core::value::LambdaParams;

        let cases: Vec<(Vec<Op>, Vec<Value>, usize, Vec<Value>)> = vec![
            // (lambda (a b) (+ a b)) on (40, 2) -> 42.
            (
                vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return],
                vec![],
                2,
                vec![Value::make_int(40), Value::make_int(2)],
            ),
            // (lambda (n) (if (< n 2) n (1- n))) — branch + arithmetic.
            (
                vec![
                    Op::StackRef(0),
                    Op::Constant(0),
                    Op::Lss,
                    Op::GotoIfNil(6),
                    Op::StackRef(0),
                    Op::Return,
                    Op::StackRef(0),
                    Op::Sub1,
                    Op::Return,
                ],
                vec![Value::make_int(2)],
                1,
                vec![Value::make_int(9)],
            ),
            // Pure countdown loop: (lambda (n) (let ((acc 0)) (while (> n 0)
            // (setq acc (+ acc n)) (setq n (1- n))) acc)).
            (
                vec![
                    Op::Constant(0),   // 0  acc=0      [n 0]
                    Op::StackRef(1),   // 1  [n acc n]   <- head
                    Op::Constant(0),   // 2  0
                    Op::Gtr,           // 3  [n acc c]
                    Op::GotoIfNil(13), // 4  [n acc]
                    Op::StackRef(1),   // 5  n
                    Op::StackRef(1),   // 6  acc
                    Op::Add,           // 7  acc'
                    Op::StackSet(1),   // 8  [n acc']
                    Op::StackRef(1),   // 9  n
                    Op::Sub1,          // 10 n-1
                    Op::StackSet(2),   // 11 [n-1 acc']
                    Op::Goto(1),       // 12 backedge
                    Op::StackRef(0),   // 13 [n acc acc]
                    Op::Return,        // 14
                ],
                vec![Value::make_int(0)],
                1,
                vec![Value::make_int(10)],
            ),
        ];

        for (ops, constants, arity, args) in cases {
            let mir = mir::build_mir(&ops, &constants, arity).expect("MIR builds");
            let leaf = lower_mir_pure(&mir).expect("MIR lowers (pure subset)");

            // Interpreter oracle.
            let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
            let mut f = ByteCodeFunction::new(LambdaParams {
                required: (1..=arity)
                    .map(|i| crate::emacs_core::intern::SymId(i as u32))
                    .collect(),
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = ops.clone();
            f.constants = constants.clone().into();
            f.max_stack = 32;
            let want = {
                let mut vm = Vm::from_context(&mut ev);
                vm.execute(&f, args.clone()).expect("interpreter runs")
            };

            match leaf.call_for_test(&args) {
                Some(bits) => assert_eq!(
                    Value::from_bits(bits),
                    want,
                    "MIR-lowered native result must equal the interpreter for {ops:?}"
                ),
                None => panic!("MIR-lowered pure body deopted unexpectedly for {ops:?}"),
            }
        }
    }

    /// A pure-arithmetic guard deopts cleanly (non-fixnum input) — same as the
    /// baseline tier, since the pure subset reruns the interpreter from start.
    #[test]
    fn mir_pure_lowering_deopts_on_nonfixnum() {
        // (lambda (a b) (+ a b)) called with a string -> the fixnum guard fails.
        let ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
        let mir = mir::build_mir(&ops, &[], 2).expect("builds");
        let leaf = lower_mir_pure(&mir).expect("lowers");
        assert_eq!(
            leaf.call_for_test(&[Value::string("x"), Value::make_int(2)]),
            None,
            "non-fixnum operand deopts (rerun-from-start)"
        );
    }

    /// A CALL now LOWERS in the MIR tier (the calls-slice handles it via precise
    /// deopt + the generic shim) where it previously bailed to the baseline. Other
    /// shim ops (Eq) remain out of scope and still bail.
    #[test]
    fn mir_pure_lowering_handles_a_call() {
        // (lambda () (foo)) — has a Call (opaque) -> now lowered (was a bail).
        let ops = vec![Op::Constant(0), Op::Call(0), Op::Return];
        let mir = mir::build_mir(&ops, &[Value::symbol("foo")], 0).expect("MIR builds");
        let leaf = lower_mir_pure(&mir).expect("a call now lowers via the calls-slice");
        assert!(
            leaf.has_side_effects,
            "a call-bearing leaf is side-effecting (no rerun-from-start)"
        );
        // (lambda (a b) (eq a b)) — Eq still bails (needs the symbols-with-pos shim).
        let eq_ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Eq, Op::Return];
        let eq_mir = mir::build_mir(&eq_ops, &[], 2).expect("eq MIR builds");
        assert!(matches!(
            lower_mir_pure(&eq_mir),
            Err(CompileError::UnsupportedOp("mir-pure-shim-op"))
        ));
    }

    #[test]
    fn bails_on_unsupported_op() {
        // MakeClosure (closure construction) is not in the supported subset ->
        // refuse, do not miscompile.
        let err = lower_nullary_leaf(
            &[Op::Nil, Op::Nil, Op::MakeClosure(0), Op::Nil, Op::Return],
            &[Value::NIL],
        )
        .unwrap_err();
        assert!(matches!(err, CompileError::UnsupportedOp("other")));
        // A Switch whose jump table is not a compile-time constant bails too
        // (the byte compiler always emits Constant(table) right before it).
        let err = lower_nullary_leaf(&[Op::Nil, Op::Nil, Op::Switch, Op::Nil, Op::Return], &[])
            .unwrap_err();
        assert!(matches!(err, CompileError::UnsupportedOp("switch-dynamic")));
    }

    #[test]
    fn list_and_slice_builtins_run_natively() {
        use crate::emacs_core::print::print_value;
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;

        // (list 1 2 3)
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Constant(2),
                Op::List(3),
                Op::Return,
            ],
            &[Value::make_int(1), Value::make_int(2), Value::make_int(3)],
        )
        .expect("list body compiles");
        let NativeRun::Ok(bits) = leaf.call(ctx_ptr, &[]) else {
            panic!("native list failed");
        };
        assert_eq!(print_value(&Value::from_bits(bits)), "(1 2 3)");

        // (concat "foo" "bar")
        let leaf = lower_nullary_leaf(
            &[Op::Constant(0), Op::Constant(1), Op::Concat(2), Op::Return],
            &[Value::string("foo"), Value::string("bar")],
        )
        .expect("concat body compiles");
        let NativeRun::Ok(bits) = leaf.call(ctx_ptr, &[]) else {
            panic!("native concat failed");
        };
        assert_eq!(print_value(&Value::from_bits(bits)), "\"foobar\"");

        // (substring "hello" 1 3)
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Constant(2),
                Op::Substring,
                Op::Return,
            ],
            &[
                Value::string("hello"),
                Value::make_int(1),
                Value::make_int(3),
            ],
        )
        .expect("substring body compiles");
        let NativeRun::Ok(bits) = leaf.call(ctx_ptr, &[]) else {
            panic!("native substring failed");
        };
        assert_eq!(print_value(&Value::from_bits(bits)), "\"el\"");

        // (nconc (list 1 2) (list 3)) — built natively end-to-end.
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::List(2),
                Op::Constant(2),
                Op::List(1),
                Op::Nconc,
                Op::Return,
            ],
            &[Value::make_int(1), Value::make_int(2), Value::make_int(3)],
        )
        .expect("nconc body compiles");
        let NativeRun::Ok(bits) = leaf.call(ctx_ptr, &[]) else {
            panic!("native nconc failed");
        };
        assert_eq!(print_value(&Value::from_bits(bits)), "(1 2 3)");

        // Signal path: (substring 5 0 1) is a wrong-type-argument.
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Constant(2),
                Op::Substring,
                Op::Return,
            ],
            &[Value::make_int(5), Value::make_int(0), Value::make_int(1)],
        )
        .expect("substring body compiles");
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
        let flow = take_pending_flow().expect("signal stashed");
        match flow {
            Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
            other => panic!("expected wrong-type-argument, got {other:?}"),
        }
    }

    #[test]
    fn named_builtin_ops_run_natively() {
        // CallBuiltin/CallBuiltinSym need the full runtime's subr resolution
        // (covered by the eval_test seam differential); Aset's fast path runs
        // against the minimal harness.
        use crate::emacs_core::print::print_value;
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;

        // Aset: mutate a constant vector natively, read back.
        let vec = Value::vector(vec![Value::make_int(0), Value::make_int(0)]);
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0), // v
                Op::Constant(1), // 1
                Op::Constant(2), // 99
                Op::Aset,
                Op::Return,
            ],
            &[vec, Value::make_int(1), Value::make_int(99)],
        )
        .expect("aset body compiles");
        assert_eq!(
            leaf.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(99).bits())
        );
        assert_eq!(print_value(&vec), "[0 99]");

        // Signal path: (aset 5 0 1) is a wrong-type-argument.
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Constant(2),
                Op::Aset,
                Op::Return,
            ],
            &[Value::make_int(5), Value::make_int(0), Value::make_int(1)],
        )
        .expect("aset body compiles");
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
        let _ = take_pending_flow().expect("signal stashed");
    }

    #[test]
    fn cbsym_classifier_selects_shipset_by_name() {
        // R2 COMMIT 1: `find_spec_sites` classifies CallBuiltinSym sites BY NAME
        // (Tier-A read / Tier-B dispatch-skip), allowlist only, keyed at the
        // op's own index. Nothing consumes these kinds yet (the lowering ignores
        // them) — this pins the classifier itself.
        use crate::emacs_core::eval::Context;
        use crate::emacs_core::intern::intern;
        let ev = Context::new();
        let point = Op::CallBuiltinSym(intern("point"), 0);
        let insert = Op::CallBuiltinSym(intern("insert"), 1);
        let car = Op::CallBuiltinSym(intern("car"), 1); // real builtin, NOT shipped
        let gc = Op::CallBuiltinSym(intern("garbage-collect"), 0); // special name
        let goto = Op::CallBuiltinSym(intern("goto-char"), 1);
        let mbeg = Op::CallBuiltinSym(intern("match-beginning"), 1);
        let ops = [point, insert, car, gc, goto, mbeg, Op::Return];
        // The CBSym loop ignores `leaders`; pass the entry leader only.
        let sites = find_spec_sites(&ops, &[], &[0], &ev.obarray);
        assert_eq!(
            sites.get(&0).map(|s| s.kind),
            Some(SpecCalleeKind::CbsymTierA {
                which: CBSYM_A_POINT
            }),
            "point -> Tier-A read"
        );
        assert_eq!(
            sites.get(&1).map(|s| s.kind),
            Some(SpecCalleeKind::CbsymTierB),
            "insert -> Tier-B dispatch-skip"
        );
        assert!(!sites.contains_key(&2), "car is not in the R2 ship set");
        assert!(
            !sites.contains_key(&3),
            "garbage-collect is a dispatch_vm_builtin_unrooted special name"
        );
        assert_eq!(
            sites.get(&4).map(|s| s.kind),
            Some(SpecCalleeKind::CbsymTierB),
            "goto-char -> Tier-B"
        );
        assert_eq!(
            sites.get(&5).map(|s| s.kind),
            Some(SpecCalleeKind::CbsymTierA {
                which: CBSYM_A_MATCH_BEGINNING
            }),
            "match-beginning -> Tier-A (does a byte->char conversion; must delegate)"
        );
        // Every classified CBSym site reports `is_cbsym`; none report `is_round1_subr`.
        for idx in [0u32, 1, 4, 5] {
            let k = sites[&(idx as usize)].kind;
            assert!(k.is_cbsym(), "{idx}: classified kind is CBSym");
            assert!(!k.is_round1_subr(), "{idx}: not an Op::Call subr kind");
        }
    }

    #[test]
    fn cbsym_shipset_excludes_special_and_writeback_names() {
        // The `dispatch_vm_builtin_unrooted` special names + the writeback /
        // re-entrant names must NEVER classify: the fast shim funnels through
        // `funcall_general`, a DIFFERENT dispatch than the special-name arm, and
        // aset/fillarray carry a writeback protocol. Allowlist construction makes
        // this automatic; assert it functionally (a collision would classify one).
        use crate::emacs_core::eval::Context;
        use crate::emacs_core::intern::intern;
        let _ev = Context::new();
        for name in CBSYM_SPECIAL_NAMES.iter().copied().chain([
            "aset",
            "fillarray",
            "funcall",
            "apply",
            "eval",
        ]) {
            assert!(
                cbsym_spec_kind(intern(name), 0).is_none(),
                "excluded name {name:?} must never classify as a CBSym intrinsic"
            );
            assert!(
                cbsym_spec_kind(intern(name), 1).is_none(),
                "excluded name {name:?} (1-arg) must never classify"
            );
        }
    }

    #[test]
    fn spec_sites_track_callee_through_computed_arguments() {
        // The abstract-stack widening: an Op::Call whose ARGUMENTS are computed
        // expressions still speculates, as long as the CALLEE slot provably
        // holds the constant symbol. The old trivial-push scan rejected any
        // intervening arithmetic — pinning fib-style self-recursion
        // (callee (- x 1)) to the generic shim forever.
        let (ev, sym_val) = harness_with_inc_callee("spec-computed-arg-callee");
        let consts = [sym_val, Value::make_int(1)];
        // (callee (- arg0 1)): Constant(sym); StackRef; Constant(1); Sub; Call(1)
        let ops = [
            Op::Constant(0),
            Op::StackRef(0),
            Op::Constant(1),
            Op::Sub,
            Op::Call(1),
            Op::Return,
        ];
        let sites = find_spec_sites(&ops, &consts, &[0], &ev.obarray);
        assert_eq!(
            sites.get(&4).map(|s| s.kind),
            Some(SpecCalleeKind::Bytecode),
            "computed-argument call must speculate on its constant callee"
        );
    }

    #[test]
    fn spec_sites_track_both_calls_of_a_nested_call_argument() {
        // (callee (callee 5)): the inner call is an argument of the outer one;
        // BOTH callee slots hold the tracked constant, so both sites speculate
        // (the inner call's result correctly untags the arg slot, not the
        // outer callee's slot).
        let (ev, sym_val) = harness_with_inc_callee("spec-nested-call-callee");
        let consts = [sym_val, Value::make_int(5)];
        let ops = [
            Op::Constant(0),
            Op::Constant(0),
            Op::Constant(1),
            Op::Call(1),
            Op::Call(1),
            Op::Return,
        ];
        let sites = find_spec_sites(&ops, &consts, &[0], &ev.obarray);
        assert_eq!(
            sites.get(&3).map(|s| s.kind),
            Some(SpecCalleeKind::Bytecode),
            "inner call speculates"
        );
        assert_eq!(
            sites.get(&4).map(|s| s.kind),
            Some(SpecCalleeKind::Bytecode),
            "outer call speculates across the nested call"
        );
    }

    #[test]
    fn spec_sites_reset_at_block_leaders() {
        // A block leader between the callee push and the call means the entry
        // stack is unknown — the tracker must forget the constant (values
        // reaching the call could come from another predecessor).
        let (ev, sym_val) = harness_with_inc_callee("spec-leader-reset-callee");
        let consts = [sym_val, Value::make_int(5)];
        let ops = [Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return];
        let sites = find_spec_sites(&ops, &consts, &[0, 2], &ev.obarray);
        assert!(
            !sites.contains_key(&2),
            "a leader between push and call must clear the tracking"
        );
    }

    #[test]
    fn spec_sites_respect_stackset_clobbering_the_callee_slot() {
        // StackSet overwrites the tracked callee slot with a computed value:
        // speculating here would call the WRONG function. The tracker must
        // model the in-place write.
        let (ev, sym_val) = harness_with_inc_callee("spec-stackset-clobber-callee");
        let consts = [sym_val, Value::make_int(5)];
        // [sym 5 nil] -> StackSet(2) moves nil into the callee slot -> [nil 5]
        let ops = [
            Op::Constant(0),
            Op::Constant(1),
            Op::Nil,
            Op::StackSet(2),
            Op::Call(1),
            Op::Return,
        ];
        let sites = find_spec_sites(&ops, &consts, &[0], &ev.obarray);
        assert!(
            !sites.contains_key(&4),
            "a clobbered callee slot must not speculate"
        );
    }

    #[test]
    fn cbsym_intrinsic_ops_no_longer_veto_profitability() {
        // R2 COMMIT 3: an intrinsifiable CallBuiltinSym op no longer counts as a
        // call in `body_is_jit_profitable`, so a buffer-op-heavy loop that USED
        // to be NotProfitable (calls > arith) now tiers. A genuine call still
        // vetoes; a non-shipped CBSym still counts.
        use crate::emacs_core::eval::Context;
        use crate::emacs_core::intern::intern;
        let _ev = Context::new(); // populate the subr table for cbsym_spec_kind
        force_profit_gate_for_test(true);
        force_gate_relax_for_test(false); // pin default (env-independent); Op::Call vetoes
        let point = Op::CallBuiltinSym(intern("point"), 0); // Tier-A eligible
        let goto = Op::CallBuiltinSym(intern("goto-char"), 1); // Tier-B eligible
        // Before the re-weight: calls=2 arith=0 -> NotProfitable. Now the two
        // intrinsifiable CBSym ops drop out of the call count -> profitable.
        assert!(
            body_is_jit_profitable(&[point, Op::Pop, goto, Op::Return], &[]),
            "an intrinsifiable buffer-op body now tiers"
        );
        // A genuine Op::Call (no arithmetic) still vetoes.
        assert!(
            !body_is_jit_profitable(&[Op::Constant(0), Op::Call(0), Op::Return], &[]),
            "a real call-dominated body still declines"
        );
        // A non-shipped CBSym (`car`) is NOT intrinsified, so it still counts.
        assert!(
            !body_is_jit_profitable(&[Op::CallBuiltinSym(intern("car"), 1), Op::Return], &[]),
            "a non-intrinsifiable CBSym still counts as a call"
        );
    }

    #[test]
    fn gate_relax_lets_user_call_heavy_bodies_tier() {
        // NEOVM_JIT_GATE_RELAX: user-function Op::Call/Apply stop vetoing (measured
        // 2.31x net-positive tiered), while builtin calls stay counted (real
        // builtin-heavy = neutral, e.g. font-lock). Default OFF preserves
        // `calls <= arith`.
        use crate::emacs_core::eval::Context;
        use crate::emacs_core::intern::intern;
        let _ev = Context::new(); // subr table for cbsym_spec_kind
        force_profit_gate_for_test(true);
        // 4 user calls, 0 arith.
        let user_call_heavy = [
            Op::Constant(0),
            Op::Call(0),
            Op::Constant(0),
            Op::Call(0),
            Op::Constant(0),
            Op::Call(0),
            Op::Constant(0),
            Op::Call(0),
            Op::Return,
        ];
        // 4 non-intrinsified builtin calls (car), 0 arith — the font-lock shape.
        let builtin_heavy = [
            Op::CallBuiltinSym(intern("car"), 1),
            Op::CallBuiltinSym(intern("car"), 1),
            Op::CallBuiltinSym(intern("car"), 1),
            Op::CallBuiltinSym(intern("car"), 1),
            Op::Return,
        ];

        // Default (relax OFF): both decline (calls > arith), unchanged behavior.
        force_gate_relax_for_test(false);
        assert!(
            !body_is_jit_profitable(&user_call_heavy, &[]),
            "relax OFF: user-call-heavy still declines (unchanged)"
        );
        assert!(
            !body_is_jit_profitable(&builtin_heavy, &[]),
            "relax OFF: builtin-heavy declines"
        );

        // Relax ON: user calls no longer veto; builtin calls still do.
        force_gate_relax_for_test(true);
        assert!(
            body_is_jit_profitable(&user_call_heavy, &[]),
            "relax ON: user-call-heavy now tiers (measured 2.31x net-positive)"
        );
        assert!(
            !body_is_jit_profitable(&builtin_heavy, &[]),
            "relax ON: builtin-call-heavy still declines (font-lock ~1.0x, correctly declined)"
        );
        force_gate_relax_for_test(false);
    }

    #[test]
    fn switch_jump_table_dispatches_natively() {
        // Mirror vm_switch_branches_using_hash_table_jump_table: a constant
        // eq jump table {foo -> byte offset 8} resolving through the GNU
        // byte-offset map to instruction 5. Hit -> 20, miss -> 10.
        use crate::emacs_core::value::HashTableTest;
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let table = Value::hash_table(HashTableTest::Eq);
        let _ = table.with_hash_table_mut(|ht| {
            let key = Value::symbol("jit-sw-foo").to_hash_key(&ht.test);
            ht.insert(key, Value::symbol("jit-sw-foo"), Value::fixnum(8));
        });
        let map = vec![GnuByteOffsetMapEntry::new(8, 5)];
        let leaf = lower_leaf_with_map(
            &[
                Op::StackRef(0), // [x x]
                Op::Constant(0), // [x x table]
                Op::Switch,      // [x], jump or fall through
                Op::Constant(1), // miss: 10
                Op::Return,
                Op::Constant(2), // 5: hit: 20
                Op::Return,
            ],
            &[table, Value::make_int(10), Value::make_int(20)],
            1,
            Some(&map),
        )
        .expect("switch body compiles");
        let hit = leaf.call(ctx_ptr, &[Value::symbol("jit-sw-foo")]);
        assert_eq!(hit, NativeRun::Ok(Value::make_int(20).bits()));
        let miss = leaf.call(ctx_ptr, &[Value::symbol("jit-sw-bar")]);
        assert_eq!(miss, NativeRun::Ok(Value::make_int(10).bits()));
    }

    #[test]
    fn handler_analysis_bails_on_unbalanced_pophandler() {
        // PopHandler with no statically active handler frame.
        let err = lower_nullary_leaf(&[Op::PopHandler, Op::Nil, Op::Return], &[]).unwrap_err();
        assert!(matches!(
            err,
            CompileError::UnsupportedOp("unbalanced-pophandler")
        ));
    }

    #[test]
    fn handler_body_compiles_and_runs_catch_throw_natively() {
        // (catch 'tag (throw 'tag 42)) — the throw is caught by this same
        // frame's PushCatch via the match shim, natively.
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let tag = Value::symbol("jit-unit-tag");
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0),  // 'tag
                Op::PushCatch(5), // frame, handler target 5
                Op::Constant(0),  // 'tag
                Op::Constant(1),  // 42
                Op::Throw,
                Op::Return, // 5: handler entry [thrown]
            ],
            &[tag, Value::make_int(42)],
        )
        .expect("handler body compiles");
        let base = ev.condition_stack.len();
        match leaf.call(ctx_ptr, &[]) {
            NativeRun::Ok(bits) => {
                assert_eq!(Value::from_bits(bits), Value::make_int(42));
            }
            other => panic!("expected native catch, got {other:?}"),
        }
        assert_eq!(ev.condition_stack.len(), base, "frame popped by the catch");
    }

    #[test]
    fn handler_frames_unwound_on_propagation() {
        // (catch 'a (throw 'b 1)) — no frame matches: the flow propagates as
        // STATUS_SIGNAL (no-catch) and our registered frame is unwound.
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0),  // 'a
                Op::PushCatch(5), // frame, handler target 5
                Op::Constant(1),  // 'b
                Op::Constant(2),  // 1
                Op::Throw,
                Op::Return, // 5: handler (reachable only via the frame)
            ],
            &[
                Value::symbol("jit-unit-a"),
                Value::symbol("jit-unit-b"),
                Value::make_int(1),
            ],
        )
        .expect("handler body compiles");
        let base = ev.condition_stack.len();
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
        let flow = take_pending_flow().expect("no-catch flow stashed");
        match flow {
            Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "no-catch"),
            other => panic!("expected no-catch signal, got {other:?}"),
        }
        assert_eq!(ev.condition_stack.len(), base, "frames unwound");
    }

    #[test]
    fn compiles_varref_and_varset() {
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let var = Value::symbol("jit-test-dynvar");
        let crate::emacs_core::value::ValueKind::Symbol(var_id) = var.kind() else {
            panic!("symbol expected");
        };
        ev.obarray.set_symbol_value_id(var_id, Value::make_int(33));

        // VarRef reads the live value.
        let read = lower_nullary_leaf(&[Op::VarRef(0), Op::Return], &[var]).unwrap();
        assert_eq!(
            read.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(33).bits())
        );

        // VarSet stores; read back through the runtime.
        let write = lower_nullary_leaf(
            &[Op::Constant(1), Op::VarSet(0), Op::Nil, Op::Return],
            &[var, Value::make_int(44)],
        )
        .unwrap();
        assert_eq!(write.call(ctx_ptr, &[]), NativeRun::Ok(Value::NIL.bits()));
        assert_eq!(
            read.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(44).bits()),
            "VarSet must be visible to a subsequent VarRef"
        );

        // Reading an unbound variable signals (void-variable) -> Signal.
        let unbound = Value::symbol("jit-test-unbound-var");
        let bad = lower_nullary_leaf(&[Op::VarRef(0), Op::Return], &[unbound]).unwrap();
        assert_eq!(bad.call(ctx_ptr, &[]), NativeRun::Signal);
        assert!(take_pending_flow().is_some());
    }

    #[test]
    fn compiles_varbind_unbind_with_full_unwind_semantics() {
        use crate::emacs_core::bytecode::Vm;
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let var = Value::symbol("jit-test-bind-var");
        let crate::emacs_core::value::ValueKind::Symbol(var_id) = var.kind() else {
            panic!("symbol expected");
        };
        ev.obarray.set_symbol_value_id(var_id, Value::make_int(99));
        let read = lower_nullary_leaf(&[Op::VarRef(0), Op::Return], &[var]).unwrap();
        let global_now = |ev: &mut crate::emacs_core::eval::Context| {
            let p = ev as *mut crate::emacs_core::eval::Context as *mut u8;
            match read.call(p, &[]) {
                NativeRun::Ok(bits) => Value::from_bits(bits),
                other => panic!("global read failed: {other:?}"),
            }
        };

        // Balanced let: bind 5, read it, unbind, return. Matches the
        // interpreter on the same body.
        let ops = [
            Op::Constant(1), // 5
            Op::VarBind(0),
            Op::VarRef(0),
            Op::Unbind(1),
            Op::Return,
        ];
        let consts = [var, Value::make_int(5)];
        let balanced = lower_nullary_leaf(&ops, &consts).unwrap();
        assert_eq!(
            balanced.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(5).bits())
        );
        assert_eq!(global_now(&mut ev), Value::make_int(99), "binding popped");
        let interp = {
            let mut f = ByteCodeFunction::new(LambdaParams {
                required: Vec::new(),
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = ops.to_vec();
            f.constants = consts.to_vec().into();
            f.max_stack = 16;
            let mut vm = Vm::from_context(&mut ev);
            vm.execute(&f, vec![]).expect("interp runs let")
        };
        assert_eq!(interp, Value::make_int(5), "interpreter agrees");
        assert_eq!(global_now(&mut ev), Value::make_int(99));

        // Early return with the binding still active: the frame unwind must
        // restore the global (cleanup_bytecode_frame parity).
        let early = lower_nullary_leaf(
            &[Op::Constant(1), Op::VarBind(0), Op::True, Op::Return],
            &consts,
        )
        .unwrap();
        assert_eq!(early.call(ctx_ptr, &[]), NativeRun::Ok(Value::T.bits()));
        assert_eq!(
            global_now(&mut ev),
            Value::make_int(99),
            "early return must unwind the dangling binding"
        );

        // Signal inside the dynamic extent: the binding must also unwind.
        let sig = lower_nullary_leaf(
            &[
                Op::Constant(1),
                Op::VarBind(0),
                Op::Constant(2), // undefined function symbol
                Op::Call(0),
                Op::Return,
            ],
            &[
                var,
                Value::make_int(5),
                Value::symbol("jit-bind-no-such-fn"),
            ],
        )
        .unwrap();
        assert_eq!(sig.call(ctx_ptr, &[]), NativeRun::Signal);
        assert!(take_pending_flow().is_some());
        assert_eq!(
            global_now(&mut ev),
            Value::make_int(99),
            "signal must unwind the dangling binding"
        );
    }

    #[test]
    fn compiled_unbind_and_frame_exit_propagate_restore_watcher_signals() {
        fn install_restore_watcher(variable: &str) -> crate::emacs_core::eval::Context {
            let mut eval = crate::emacs_core::eval::Context::new();
            let source = format!(
                r#"(progn
                     (setq {variable} 9)
                     (fset 'jit-unbind-error-watcher
                           (lambda (_symbol _new-value operation _where)
                             (if (eq operation 'unlet)
                                 (signal 'error '("restore"))
                               nil)))
                     (add-variable-watcher '{variable}
                                           'jit-unbind-error-watcher))"#
            );
            eval.eval_str(&source).expect("install restore watcher");
            eval
        }

        let variable = "jit-test-explicit-unbind-error";
        let mut explicit_ctx = install_restore_watcher(variable);
        let explicit_base = explicit_ctx.specpdl.len();
        let explicit_ptr = &mut explicit_ctx as *mut crate::emacs_core::eval::Context as *mut u8;
        let explicit = lower_nullary_leaf(
            &[
                Op::Constant(1),
                Op::VarBind(0),
                Op::True,
                Op::Unbind(1),
                Op::Return,
            ],
            &[Value::symbol(variable), Value::make_int(1)],
        )
        .expect("explicit unbind body compiles");
        assert_eq!(explicit.call(explicit_ptr, &[]), NativeRun::Signal);
        assert!(matches!(take_pending_flow(), Some(Flow::Signal(_))));
        assert_eq!(explicit_ctx.specpdl.len(), explicit_base);

        let variable = "jit-test-frame-unbind-error";
        let mut frame_ctx = install_restore_watcher(variable);
        let frame_base = frame_ctx.specpdl.len();
        let frame_ptr = &mut frame_ctx as *mut crate::emacs_core::eval::Context as *mut u8;
        let dangling = lower_nullary_leaf(
            &[Op::Constant(1), Op::VarBind(0), Op::True, Op::Return],
            &[Value::symbol(variable), Value::make_int(1)],
        )
        .expect("dangling binding body compiles");
        assert_eq!(dangling.call(frame_ptr, &[]), NativeRun::Signal);
        assert!(matches!(take_pending_flow(), Some(Flow::Signal(_))));
        assert_eq!(frame_ctx.specpdl.len(), frame_base);
    }

    #[test]
    fn cleanup_flow_does_not_pop_an_outer_callers_handler() {
        use crate::emacs_core::eval::{ConditionFrame, ResumeTarget, SpecBinding};

        let mut ctx = crate::emacs_core::eval::Context::new();
        let outer_tag = Value::symbol("jit-test-outer-caller-tag");
        let local_tag = Value::symbol("jit-test-unmatched-local-tag");
        let inner_tag = Value::symbol("jit-test-inner-tag");

        // Model a caller-owned catch below two handlers owned by this native
        // leaf. The inner catch is selected by the original throw. Unwinding
        // it runs a cleanup that throws to the caller, so the resumed search
        // must pop only the one remaining leaf-local handler.
        ctx.push_condition_frame(ConditionFrame::Catch {
            tag: outer_tag,
            resume: ResumeTarget::InterpreterCatch,
        });
        ctx.push_condition_frame(ConditionFrame::Catch {
            tag: local_tag,
            resume: ResumeTarget::VmCatch {
                resume_id: 1,
                target: 10,
                stack_len: 0,
                spec_depth: 0,
                bind_stack_len: 0,
            },
        });
        ctx.push_condition_frame(ConditionFrame::Catch {
            tag: inner_tag,
            resume: ResumeTarget::VmCatch {
                resume_id: 2,
                target: 20,
                stack_len: 0,
                spec_depth: 0,
                bind_stack_len: 0,
            },
        });

        let quoted_outer = Value::list(vec![Value::symbol("quote"), outer_tag]);
        let cleanup_form = Value::list(vec![
            Value::symbol("throw"),
            quoted_outer,
            Value::make_int(42),
        ]);
        ctx.specpdl.push(SpecBinding::UnwindProtect {
            forms: Value::list(vec![cleanup_form]),
            lexenv: ctx.lexenv,
        });

        stash_pending_flow(Flow::throw(inner_tag, Value::make_int(1)));
        let mut out = 0i64;
        let ctx_ptr = &mut ctx as *mut crate::emacs_core::eval::Context as *mut u8;
        assert_eq!(neovm_jit_match_handler(ctx_ptr, 2, &mut out), -1);
        assert_eq!(ctx.condition_stack.len(), 1, "caller handler survives");
        assert_eq!(ctx.specpdl.len(), 0, "cleanup extent fully unwound");
        let flow = take_pending_flow().expect("cleanup throw propagated to caller");
        let Flow::Throw(thrown) = flow else {
            panic!("expected cleanup throw, got {flow:?}");
        };
        assert_eq!(thrown.tag, outer_tag);
        assert_eq!(thrown.value, Value::make_int(42));
    }

    #[test]
    fn guard_after_varbind_and_unbalanced_unbind_bail() {
        // Precise deopt: a guard after a binding compiles (a failing guard
        // transfers the bind to the resumed interpreter frame).
        lower_nullary_leaf(
            &[
                Op::Constant(1),
                Op::VarBind(0),
                Op::Constant(1),
                Op::Add1,
                Op::Return,
            ],
            &[Value::symbol("jit-test-bind-poison"), Value::make_int(1)],
        )
        .expect("guard after a binding compiles under precise deopt");

        // Unbinding more than this function bound bails to the interpreter.
        let err = lower_nullary_leaf(&[Op::Unbind(1), Op::Nil, Op::Return], &[]).unwrap_err();
        assert!(matches!(
            err,
            CompileError::UnsupportedOp("unbalanced-unbind")
        ));
    }

    #[test]
    fn guard_after_varset_compiles_and_runs() {
        // Precise deopt: a guard after an assignment compiles and runs; the
        // assignment is NOT replayed on a later deopt (resume is mid-frame).
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(1),
                Op::VarSet(0),
                Op::Constant(1),
                Op::Add1,
                Op::Return,
            ],
            &[Value::symbol("jit-test-poison-var"), Value::make_int(1)],
        )
        .expect("guard after an assignment compiles under precise deopt");
        assert_eq!(
            leaf.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(2).bits())
        );
    }

    #[test]
    fn compiles_fixnum_mul() {
        let mul = |a: i64, b: i64| {
            lower_nullary_leaf(
                &[Op::Constant(0), Op::Constant(1), Op::Mul, Op::Return],
                &[Value::make_int(a), Value::make_int(b)],
            )
            .unwrap()
            .call_for_test(&[])
        };
        assert_eq!(mul(6, 7), Some(Value::make_int(42).bits()));
        assert_eq!(mul(-6, 7), Some(Value::make_int(-42).bits()));
        assert_eq!(mul(0, 12345), Some(Value::make_int(0).bits()));
        // Product overflowing fixnum range -> deopt.
        assert_eq!(mul(Value::MOST_POSITIVE_FIXNUM, 2), None);
        assert_eq!(mul(1 << 40, 1 << 40), None); // 2^80, way out of range
    }

    #[test]
    fn mul_non_fixnum_deopts() {
        let leaf = lower_nullary_leaf(
            &[Op::Constant(0), Op::Nil, Op::Mul, Op::Return],
            &[Value::make_int(5)],
        )
        .unwrap();
        assert_eq!(leaf.call_for_test(&[]), None);
    }

    #[test]
    fn compiles_type_predicates() {
        // Inspects only tag bits; never dereferences, so heap values needn't be
        // kept alive (no GC safepoint in the JIT call).
        fn pred(op: Op, v: Value) -> Option<usize> {
            lower_nullary_leaf(&[Op::Constant(0), op, Op::Return], &[v])
                .unwrap()
                .call_for_test(&[])
        }
        let t = Some(Value::T.bits());
        let nil = Some(Value::NIL.bits());
        let cons = Value::cons(Value::make_int(1), Value::make_int(2));
        let s = Value::string("hi");

        // null / not: only nil is null; fixnum 0 is NOT nil.
        assert_eq!(pred(Op::Null, Value::NIL), t);
        assert_eq!(pred(Op::Null, Value::make_int(0)), nil);
        assert_eq!(pred(Op::Not, Value::T), nil);
        assert_eq!(pred(Op::Not, Value::NIL), t);
        // consp
        assert_eq!(pred(Op::Consp, cons), t);
        assert_eq!(pred(Op::Consp, Value::NIL), nil);
        assert_eq!(pred(Op::Consp, Value::make_int(5)), nil);
        // stringp
        assert_eq!(pred(Op::Stringp, s), t);
        assert_eq!(pred(Op::Stringp, Value::make_int(5)), nil);
        // listp: nil or cons
        assert_eq!(pred(Op::Listp, cons), t);
        assert_eq!(pred(Op::Listp, Value::NIL), t);
        assert_eq!(pred(Op::Listp, Value::make_int(5)), nil);
    }

    #[test]
    fn compiles_car_cdr() {
        // No GC safepoint in the JIT call, so the cons local stays alive across it.
        let cons = Value::cons(Value::make_int(11), Value::make_int(22));
        let car_ops = [Op::Constant(0), Op::Car, Op::Return];
        let cdr_ops = [Op::Constant(0), Op::Cdr, Op::Return];

        // car/cdr of a cons load the fields; differential vs the interpreter.
        // Direct value assertions, not an interp differential: interp_nullary
        // builds a Context whose heap is installed as the thread-local TAGGED_HEAP
        // and left dangling on drop, which would crash the later cons allocation.
        // car/cdr correctness is fully pinned by the expected values here.
        assert_eq!(
            lower_nullary_leaf(&car_ops, &[cons])
                .unwrap()
                .call_for_test(&[]),
            Some(Value::make_int(11).bits())
        );
        assert_eq!(
            lower_nullary_leaf(&cdr_ops, &[cons])
                .unwrap()
                .call_for_test(&[]),
            Some(Value::make_int(22).bits())
        );

        // car/cdr of nil -> nil.
        assert_eq!(
            lower_nullary_leaf(&car_ops, &[Value::NIL])
                .unwrap()
                .call_for_test(&[]),
            Some(Value::NIL.bits())
        );
        assert_eq!(
            lower_nullary_leaf(&cdr_ops, &[Value::NIL])
                .unwrap()
                .call_for_test(&[]),
            Some(Value::NIL.bits())
        );

        // car of a non-list -> deopt (interpreter signals wrong-type-argument).
        assert_eq!(
            lower_nullary_leaf(&car_ops, &[Value::make_int(5)])
                .unwrap()
                .call_for_test(&[]),
            None
        );

        // Chained: (car (cdr (11 22))) = 22.
        let list = Value::cons(
            Value::make_int(11),
            Value::cons(Value::make_int(22), Value::NIL),
        );
        let cadr =
            lower_nullary_leaf(&[Op::Constant(0), Op::Cdr, Op::Car, Op::Return], &[list]).unwrap();
        assert_eq!(cadr.call_for_test(&[]), Some(Value::make_int(22).bits()));
    }

    #[test]
    fn compiles_cons() {
        // (cons 1 2): allocates a cons cell. No GC between the call and the deref
        // (nothing allocates), so the fresh cons stays valid.
        let leaf = lower_nullary_leaf(
            &[Op::Constant(0), Op::Constant(1), Op::Cons, Op::Return],
            &[Value::make_int(1), Value::make_int(2)],
        )
        .unwrap();
        let cell = Value::from_bits(leaf.call_for_test(&[]).expect("cons runs"));
        assert!(cell.is_cons());
        assert_eq!(cell.cons_car(), Value::make_int(1));
        assert_eq!(cell.cons_cdr(), Value::make_int(2));
    }

    #[test]
    fn compiles_nested_cons_list() {
        // (cons 7 (cons 8 nil)) = (7 8). The inner cons leaves 7 live below it on
        // the operand stack, exercising the gc_push rooting path.
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0),
                Op::Constant(1),
                Op::Nil,
                Op::Cons,
                Op::Cons,
                Op::Return,
            ],
            &[Value::make_int(7), Value::make_int(8)],
        )
        .unwrap();
        let result = Value::from_bits(leaf.call_for_test(&[]).expect("nested cons runs"));
        assert_eq!(result.cons_car(), Value::make_int(7));
        let tail = result.cons_cdr();
        assert!(tail.is_cons());
        assert_eq!(tail.cons_car(), Value::make_int(8));
        assert!(tail.cons_cdr().is_nil());
    }

    /// Build a harness Context with `name` bound to a lexical one-arg bytecode
    /// callee `(lambda (y) (1+ y))`, returning (ctx, callee symbol Value).
    fn harness_with_inc_callee(name: &str) -> (crate::emacs_core::eval::Context, Value) {
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let sym_val = Value::symbol(name);
        let crate::emacs_core::value::ValueKind::Symbol(sym_id) = sym_val.kind() else {
            panic!("Value::symbol must produce a symbol");
        };
        let mut callee = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        callee.lexical = true;
        callee.ops = vec![Op::StackRef(0), Op::Add1, Op::Return];
        callee.max_stack = 16;
        ev.obarray
            .set_symbol_function_id(sym_id, Value::make_bytecode(callee));
        (ev, sym_val)
    }

    #[test]
    fn compiles_call_to_bytecode_callee() {
        // (lambda () (callee 41)) where callee = (lambda (y) (1+ y)).
        // The native code re-enters the runtime through the call shim; the
        // callee runs on the interpreter and the result flows back.
        let (mut ev, sym_val) = harness_with_inc_callee("jit-test-inc-callee");
        let leaf = lower_nullary_leaf(
            &[Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
            &[sym_val, Value::make_int(41)],
        )
        .unwrap();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        assert_eq!(
            leaf.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(42).bits())
        );
    }

    #[test]
    fn call_with_live_values_below_roots_and_returns() {
        // (lambda () (let ((keep 7)) (+0-guard-free use of keep after a call)).
        // Body: push keep=7, push sym, push 41, Call(1) -> keep stays live below
        // the call (exercises the gc_save/gc_push rooting path), then combine:
        // [keep, result] -> StackSet(1) folds result into keep slot -> Return.
        let (mut ev, sym_val) = harness_with_inc_callee("jit-test-inc-callee-2");
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(2), // keep = 7
                Op::Constant(0), // sym
                Op::Constant(1), // 41
                Op::Call(1),     // -> [keep, 42]
                Op::StackSet(1), // -> [42]
                Op::Return,
            ],
            &[sym_val, Value::make_int(41), Value::make_int(7)],
        )
        .unwrap();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        assert_eq!(
            leaf.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(42).bits())
        );
    }

    #[test]
    fn call_signal_propagates() {
        // Calling an unbound function must surface as NativeRun::Signal with the
        // Flow stashed for the caller — not a deopt, not a crash.
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let sym_val = Value::symbol("jit-test-no-such-function");
        let leaf =
            lower_nullary_leaf(&[Op::Constant(0), Op::Call(0), Op::Return], &[sym_val]).unwrap();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
        assert!(
            take_pending_flow().is_some(),
            "STATUS_SIGNAL must stash the Flow"
        );
    }

    #[test]
    fn guard_after_call_deopts_without_replaying_the_call() {
        // THE precise-deopt capability test: a guard after a side-effecting
        // call compiles; when it fails, the interpreter resumes AT the guard
        // op — the call's side effect happened exactly once (rerun-from-start
        // would have replayed it). Full Context: the resumed 1+ promotes to a
        // bignum through the real builtin dispatch.
        let mut ev = crate::emacs_core::eval::Context::new();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        // Callee (lambda (x) (setcar CELL (1+ (car CELL))) x): observable
        // side effect (counter cons), returns its argument unchanged.
        let cell = Value::cons(Value::make_int(0), Value::NIL);
        let sym_val = Value::symbol("jit-test-effect-callee");
        let crate::emacs_core::value::ValueKind::Symbol(sym_id) = sym_val.kind() else {
            panic!("symbol expected");
        };
        let mut callee = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        callee.lexical = true;
        callee.ops = vec![
            Op::Constant(0), // CELL
            Op::Constant(0), // CELL
            Op::Car,
            Op::Add1,
            Op::Setcar,
            Op::Pop,
            Op::StackRef(0),
            Op::Return,
        ];
        callee.constants = vec![cell].into();
        callee.max_stack = 16;
        ev.obarray
            .set_symbol_function_id(sym_id, Value::make_bytecode(callee));

        // Caller: (1+ (callee MOST-POSITIVE-FIXNUM)) — the 1+ guard fails
        // AFTER the call ran.
        let ops = vec![
            Op::Constant(0), // 'callee
            Op::Constant(1), // MOST_POSITIVE
            Op::Call(1),
            Op::Add1, // pc 3: deopts (overflow)
            Op::Return,
        ];
        let constants = vec![sym_val, Value::make_int(Value::MOST_POSITIVE_FIXNUM)];
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.clone();
        f.constants = constants.clone().into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        let leaf = lower_nullary_leaf(&ops, &constants).expect("guard after call compiles now");
        let native = match leaf.call(ctx_ptr, &[]) {
            NativeRun::DeoptAt(resume) => {
                let DeoptResume {
                    pc,
                    stack,
                    handlers,
                    binds,
                    spec_base,
                    cond_base,
                } = *resume;
                assert_eq!(pc, 3, "deopt at the 1+ after the call");
                assert_eq!(
                    cell.cons_car(),
                    Value::make_int(1),
                    "the call's side effect ran exactly once before the deopt"
                );
                let mut vm = Vm::from_context(&mut ev);
                vm.run_resumed_frame(
                    &f,
                    Value::NIL,
                    pc,
                    &stack,
                    handlers,
                    &binds,
                    spec_base,
                    cond_base,
                )
                .expect("resume computes the bignum")
            }
            other => panic!("expected a precise deopt after the call, got {other:?}"),
        };
        assert_eq!(
            cell.cons_car(),
            Value::make_int(1),
            "resume must NOT replay the call"
        );
        // Differential: the pure interpreter on the same body (fresh counter
        // state) computes the same bignum and also increments exactly once.
        b::builtin_setcar_2(&mut ev, cell, Value::make_int(0)).expect("reset counter");
        let interp = {
            let mut vm = Vm::from_context(&mut ev);
            vm.execute(&f, vec![]).expect("interpreter computes")
        };
        assert_eq!(
            crate::emacs_core::print::print_value(&native),
            crate::emacs_core::print::print_value(&interp),
            "resume result must equal the interpreter's"
        );
        assert_eq!(cell.cons_car(), Value::make_int(1));
    }

    #[test]
    fn guard_before_call_compiles_and_deopts_cleanly() {
        // Guards strictly before the first call are fine: a deopt there reruns
        // the interpreter with no side effect having happened.
        let (mut ev, sym_val) = harness_with_inc_callee("jit-test-inc-callee-3");
        let ops = [
            Op::Constant(0), // sym
            Op::Constant(1), // n
            Op::Add1,        // guard BEFORE the call
            Op::Call(1),
            Op::Return,
        ];
        // In-range: runs natively end-to-end: (1+ 40) = 41 -> callee -> 42.
        let leaf = lower_nullary_leaf(&ops, &[sym_val, Value::make_int(40)]).unwrap();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        assert_eq!(
            leaf.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(42).bits())
        );
        // Boundary input: the pre-call guard now deopts PRECISELY at the 1+
        // op (pc 2) with the pre-op stack captured — the resume would rerun
        // exactly that op on the interpreter.
        let leaf2 = lower_nullary_leaf(
            &ops,
            &[sym_val, Value::make_int(Value::MOST_POSITIVE_FIXNUM)],
        )
        .unwrap();
        match leaf2.call(ctx_ptr, &[]) {
            NativeRun::DeoptAt(resume) => {
                let DeoptResume {
                    pc,
                    stack,
                    handlers,
                    binds,
                    ..
                } = *resume;
                assert_eq!(pc, 2, "deopt at the Add1 op");
                assert_eq!(stack.len(), 2, "pre-op stack: [callee-sym, arg]");
                assert_eq!(stack[1], Value::make_int(Value::MOST_POSITIVE_FIXNUM));
                assert_eq!(handlers, 0);
                assert!(binds.is_empty());
            }
            other => panic!("expected a precise deopt, got {other:?}"),
        }
    }

    #[test]
    fn compiles_fixnum_div_rem() {
        let run = |op: Op, a: i64, b: i64| {
            lower_nullary_leaf(
                &[Op::Constant(0), Op::Constant(1), op, Op::Return],
                &[Value::make_int(a), Value::make_int(b)],
            )
            .unwrap()
            .call_for_test(&[])
        };
        // Truncation toward zero, matching the interpreter / C.
        assert_eq!(run(Op::Div, 42, 5), Some(Value::make_int(8).bits()));
        assert_eq!(run(Op::Div, -42, 5), Some(Value::make_int(-8).bits()));
        assert_eq!(run(Op::Div, 42, -5), Some(Value::make_int(-8).bits()));
        assert_eq!(run(Op::Rem, 42, 5), Some(Value::make_int(2).bits()));
        assert_eq!(run(Op::Rem, -42, 5), Some(Value::make_int(-2).bits()));
        // Zero divisor -> deopt (interpreter signals arith-error).
        assert_eq!(run(Op::Div, 1, 0), None);
        assert_eq!(run(Op::Rem, 1, 0), None);
        // Non-fixnum operand -> deopt.
        let nf = lower_nullary_leaf(
            &[Op::Constant(0), Op::Nil, Op::Div, Op::Return],
            &[Value::make_int(4)],
        )
        .unwrap();
        assert_eq!(nf.call_for_test(&[]), None);
    }

    #[test]
    fn div_wrap_case_matches_interpreter() {
        // MOST_NEGATIVE_FIXNUM / -1 overflows fixnum range (= 2^60). The interpreter
        // wraps it; the unboxed JIT (raw_fixnum_divrem) range-checks and DEOPTS
        // rather than keep an out-of-range raw value, then a precise-deopt resume
        // reruns Op::Div in the interpreter and wraps to the same bits. Resume-value
        // parity is covered by the THRESHOLD=1 differential gate + the straight-line
        // fuzz (which generates Div over these boundary constants); here we assert
        // the deopt itself (call_for_test returns None on deopt).
        let ops = [Op::Constant(0), Op::Constant(1), Op::Div, Op::Return];
        let consts = [
            Value::make_int(Value::MOST_NEGATIVE_FIXNUM),
            Value::make_int(-1),
        ];
        let leaf = lower_nullary_leaf(&ops, &consts).unwrap();
        assert_eq!(
            leaf.call_for_test(&[]),
            None,
            "fixnum-overflow division must deopt to the interpreter, not native-wrap"
        );
    }

    #[test]
    fn compiles_eq_and_symbolp() {
        // One live Context for the vmctx-reading slow paths (symbols-with-pos
        // is disabled by default, so differing bits -> nil).
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let sym_a = Value::symbol("jit-eq-sym-a");
        let s = Value::string("eq-str");

        let eq2 = |a: Value, b: Value, ctx: *mut u8| {
            lower_nullary_leaf(
                &[Op::Constant(0), Op::Constant(1), Op::Eq, Op::Return],
                &[a, b],
            )
            .unwrap()
            .call(ctx, &[])
        };
        let t = NativeRun::Ok(Value::T.bits());
        let nil = NativeRun::Ok(Value::NIL.bits());
        // Identical bits -> t (fast path, no shim).
        assert_eq!(eq2(Value::make_int(7), Value::make_int(7), ctx_ptr), t);
        assert_eq!(eq2(sym_a, sym_a, ctx_ptr), t);
        assert_eq!(eq2(Value::NIL, Value::NIL, ctx_ptr), t);
        // Differing bits -> slow shim -> nil (swp disabled).
        assert_eq!(eq2(Value::make_int(7), Value::make_int(8), ctx_ptr), nil);
        assert_eq!(eq2(sym_a, Value::make_int(7), ctx_ptr), nil);

        let symp = |v: Value, ctx: *mut u8| {
            lower_nullary_leaf(&[Op::Constant(0), Op::Symbolp, Op::Return], &[v])
                .unwrap()
                .call(ctx, &[])
        };
        // Symbol tag -> t natively (nil and t are symbols).
        assert_eq!(symp(sym_a, ctx_ptr), t);
        assert_eq!(symp(Value::NIL, ctx_ptr), t);
        assert_eq!(symp(Value::T, ctx_ptr), t);
        // Non-symbol -> slow shim -> nil (swp disabled).
        assert_eq!(symp(Value::make_int(5), ctx_ptr), nil);
        assert_eq!(symp(s, ctx_ptr), nil);
    }

    #[test]
    fn compiles_apply_with_spread() {
        // (apply 'inc (list 41)) -> 42: the last argument spreads as the list.
        let (mut ev, sym_val) = harness_with_inc_callee("jit-test-inc-apply");
        let arg_list = Value::cons(Value::make_int(41), Value::NIL);
        let leaf = lower_nullary_leaf(
            &[Op::Constant(0), Op::Constant(1), Op::Apply(1), Op::Return],
            &[sym_val, arg_list],
        )
        .unwrap();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        assert_eq!(
            leaf.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(42).bits())
        );
    }

    #[test]
    fn compiles_apply_with_leading_args() {
        // (apply 'add2 40 (list 2)) -> 42: leading args + spread tail.
        let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let sym_val = Value::symbol("jit-test-add2-apply");
        let crate::emacs_core::value::ValueKind::Symbol(sym_id) = sym_val.kind() else {
            panic!("symbol expected");
        };
        let mut callee = ByteCodeFunction::new(LambdaParams {
            required: vec![
                crate::emacs_core::intern::SymId(1),
                crate::emacs_core::intern::SymId(2),
            ],
            optional: Vec::new(),
            rest: None,
        });
        callee.lexical = true;
        callee.ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
        callee.max_stack = 16;
        ev.obarray
            .set_symbol_function_id(sym_id, Value::make_bytecode(callee));

        let tail = Value::cons(Value::make_int(2), Value::NIL);
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0), // sym
                Op::Constant(1), // 40
                Op::Constant(2), // (2)
                Op::Apply(2),
                Op::Return,
            ],
            &[sym_val, Value::make_int(40), tail],
        )
        .unwrap();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        assert_eq!(
            leaf.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(42).bits())
        );
    }

    #[test]
    fn bails_on_missing_return() {
        let err = lower_nullary_leaf(&[Op::Nil], &[]).unwrap_err();
        assert!(matches!(err, CompileError::NoReturn));
    }

    #[test]
    fn bails_on_argument_taking_function() {
        let mut f = nullary();
        f.params.required.push(crate::emacs_core::intern::SymId(1));
        f.ops = vec![Op::Nil, Op::Return];
        let err = compile_bytecode_function(&f).unwrap_err();
        assert!(matches!(err, CompileError::TakesArguments));
    }

    #[test]
    fn bails_on_stack_underflow() {
        let err = lower_nullary_leaf(&[Op::Return], &[]).unwrap_err();
        assert!(matches!(err, CompileError::StackUnderflow));
    }

    #[test]
    fn compile_bytecode_function_handles_nullary_leaf() {
        let mut f = nullary();
        let c = Value::make_int(123);
        f.constants = vec![c].into();
        f.ops = vec![Op::Constant(0), Op::Return];
        let leaf = compile_bytecode_function(&f).unwrap();
        assert_eq!(leaf.call_for_test(&[]), Some(c.bits()));
    }

    #[test]
    fn one_arg_identity_and_increment() {
        // (lambda (x) x)
        let id = lower_leaf(&[Op::StackRef(0), Op::Return], &[], 1).unwrap();
        assert_eq!(id.arity(), 1);
        assert_eq!(
            id.call_for_test(&[Value::make_int(7)]),
            Some(Value::make_int(7).bits())
        );
        // (lambda (x) (1+ x))
        let inc = lower_leaf(&[Op::StackRef(0), Op::Add1, Op::Return], &[], 1).unwrap();
        assert_eq!(
            inc.call_for_test(&[Value::make_int(41)]),
            Some(Value::make_int(42).bits())
        );
    }

    #[test]
    fn two_arg_addition_preserves_args_via_stackref() {
        // (lambda (a b) (+ a b)); each StackRef(1) reaches an original arg as the
        // model stack grows: seed [a,b] -> push a -> push b -> Add -> a+b.
        let add = lower_leaf(
            &[Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return],
            &[],
            2,
        )
        .unwrap();
        assert_eq!(
            add.call_for_test(&[Value::make_int(40), Value::make_int(2)]),
            Some(Value::make_int(42).bits())
        );
        // A non-fixnum argument makes the speculative Add deopt.
        assert_eq!(add.call_for_test(&[Value::make_int(40), Value::NIL]), None);
    }

    #[test]
    fn compile_bytecode_function_accepts_required_args_when_lexical() {
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
        let leaf = compile_bytecode_function(&f).unwrap();
        assert_eq!(leaf.arity(), 2);
        assert_eq!(
            leaf.call_for_test(&[Value::make_int(1), Value::make_int(41)]),
            Some(Value::make_int(42).bits())
        );
    }

    #[test]
    fn compile_bytecode_function_bails_on_dynamic_params() {
        // Required params but dynamic binding (not lexical, arglist not a
        // fixnum) -> params are not on the stack -> bail.
        let mut dynp = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        dynp.lexical = false;
        dynp.ops = vec![Op::StackRef(0), Op::Return];
        assert!(!params_on_stack(&dynp));
        assert!(matches!(
            compile_bytecode_function(&dynp),
            Err(CompileError::TakesArguments)
        ));
    }

    #[test]
    fn compiles_optional_params_with_nil_padding() {
        // (lambda (a &optional b) b): frame = [a, b]; missing b is nil-padded.
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: vec![crate::emacs_core::intern::SymId(2)],
            rest: None,
        });
        f.lexical = true;
        f.ops = vec![Op::StackRef(0), Op::Return]; // top of frame = b
        f.max_stack = 16;
        let leaf = compile_bytecode_function(&f).unwrap();
        assert!(leaf.accepts(1) && leaf.accepts(2));
        assert!(!leaf.accepts(0) && !leaf.accepts(3));
        // One arg: b is nil.
        assert_eq!(
            leaf.call(core::ptr::null_mut(), &[Value::make_int(5)]),
            NativeRun::Ok(Value::NIL.bits())
        );
        // Two args: b is supplied.
        assert_eq!(
            leaf.call(
                core::ptr::null_mut(),
                &[Value::make_int(5), Value::make_int(6)]
            ),
            NativeRun::Ok(Value::make_int(6).bits())
        );
    }

    #[test]
    fn compiles_rest_param_as_list() {
        // (lambda (&rest xs) xs): frame = [xs]; surplus args become a list.
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: Some(crate::emacs_core::intern::SymId(1)),
        });
        f.lexical = true;
        f.ops = vec![Op::StackRef(0), Op::Return];
        f.max_stack = 16;
        let leaf = compile_bytecode_function(&f).unwrap();
        assert!(leaf.accepts(0) && leaf.accepts(5));
        // No args: xs = nil.
        assert_eq!(
            leaf.call(core::ptr::null_mut(), &[]),
            NativeRun::Ok(Value::NIL.bits())
        );
        // Two args: xs = (10 20).
        let NativeRun::Ok(bits) = leaf.call(
            core::ptr::null_mut(),
            &[Value::make_int(10), Value::make_int(20)],
        ) else {
            panic!("rest call must succeed");
        };
        let xs = Value::from_bits(bits);
        assert_eq!(xs.cons_car(), Value::make_int(10));
        assert_eq!(xs.cons_cdr().cons_car(), Value::make_int(20));
        assert!(xs.cons_cdr().cons_cdr().is_nil());
    }

    /// Run a nullary body through the Tier-0 interpreter (the correctness
    /// oracle) and return its result.
    fn interp_nullary(ops: &[Op], constants: &[Value]) -> Value {
        use crate::emacs_core::bytecode::Vm;
        use crate::emacs_core::eval::Context;
        let mut eval = Context::new_minimal_vm_harness();
        let mut f = nullary();
        f.ops = ops.to_vec();
        f.constants = constants.to_vec().into();
        f.max_stack = 16;
        let mut vm = Vm::from_context(&mut eval);
        vm.execute(&f, vec![]).expect("interpreter runs the body")
    }

    #[test]
    fn jit_matches_interpreter_on_supported_bodies() {
        // The ultimate parity proof: when the JIT compiles a body and does not
        // deopt, its result must be bit-identical to the interpreter's.
        let cases: &[(&[Op], &[Value])] = &[
            (&[Op::Constant(0), Op::Return], &[Value::make_int(42)]),
            (&[Op::Nil, Op::Return], &[]),
            (&[Op::True, Op::Return], &[]),
            (
                &[Op::Constant(0), Op::Constant(1), Op::Add, Op::Return],
                &[Value::make_int(40), Value::make_int(2)],
            ),
            (
                &[Op::Constant(0), Op::Constant(1), Op::Sub, Op::Return],
                &[Value::make_int(3), Value::make_int(10)],
            ),
            (
                &[Op::Constant(0), Op::Constant(1), Op::Mul, Op::Return],
                &[Value::make_int(-6), Value::make_int(7)],
            ),
            (&[Op::Nil, Op::Null, Op::Return], &[]),
            (
                &[Op::Constant(0), Op::Null, Op::Return],
                &[Value::make_int(0)],
            ),
            (
                &[Op::Constant(0), Op::Consp, Op::Return],
                &[Value::make_int(5)],
            ),
            (
                &[Op::Constant(0), Op::Listp, Op::Return],
                &[Value::make_int(5)],
            ),
            (
                &[Op::Constant(0), Op::Add1, Op::Return],
                &[Value::make_int(41)],
            ),
            (
                &[Op::Constant(0), Op::Sub1, Op::Return],
                &[Value::make_int(43)],
            ),
            (
                &[Op::Constant(0), Op::Negate, Op::Return],
                &[Value::make_int(42)],
            ),
            (
                &[
                    Op::Constant(0),
                    Op::Constant(1),
                    Op::Add,
                    Op::Constant(2),
                    Op::Sub,
                    Op::Return,
                ],
                &[Value::make_int(1), Value::make_int(2), Value::make_int(4)],
            ),
            (
                &[Op::Constant(0), Op::Constant(1), Op::Lss, Op::Return],
                &[Value::make_int(3), Value::make_int(5)],
            ),
            (
                &[Op::Constant(0), Op::Constant(1), Op::Gtr, Op::Return],
                &[Value::make_int(3), Value::make_int(5)],
            ),
            (
                &[Op::Constant(0), Op::Constant(1), Op::Eqlsign, Op::Return],
                &[Value::make_int(5), Value::make_int(5)],
            ),
            (
                &[
                    Op::Constant(0),
                    Op::Constant(1),
                    Op::Constant(2),
                    Op::DiscardN(2),
                    Op::Return,
                ],
                &[
                    Value::make_int(10),
                    Value::make_int(20),
                    Value::make_int(30),
                ],
            ),
            (
                &[
                    Op::Constant(0),
                    Op::Constant(1),
                    Op::Constant(2),
                    Op::DiscardN(0x82),
                    Op::Return,
                ],
                &[
                    Value::make_int(10),
                    Value::make_int(20),
                    Value::make_int(30),
                ],
            ),
        ];
        for (i, (ops, consts)) in cases.iter().enumerate() {
            let want = interp_nullary(ops, consts).bits();
            let got = lower_nullary_leaf(ops, consts).unwrap().call_for_test(&[]);
            assert_eq!(got, Some(want), "JIT/interpreter mismatch on case {i}");
        }
    }

    #[test]
    fn jit_matches_interpreter_with_args() {
        use crate::emacs_core::bytecode::Vm;
        use crate::emacs_core::eval::Context;
        // (lambda (a b) (+ a b)), lexical.
        let ops = [Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
        let args = [Value::make_int(40), Value::make_int(2)];

        let mut eval = Context::new_minimal_vm_harness();
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![
                crate::emacs_core::intern::SymId(1),
                crate::emacs_core::intern::SymId(2),
            ],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.to_vec();
        f.max_stack = 16;
        let want = {
            let mut vm = Vm::from_context(&mut eval);
            vm.execute(&f, args.to_vec())
                .expect("interpreter runs")
                .bits()
        };

        let got = lower_leaf(&ops, &[], 2).unwrap().call_for_test(&args);
        assert_eq!(got, Some(want), "JIT must match the interpreter with args");
    }

    // Note: the JIT's deopt *boundary* (out-of-range -> None) is covered by
    // `add_overflowing_fixnum_range_deopts` and `unary_boundary_inputs_deopt`.
    // A differential check against the interpreter's bignum-promotion path is
    // intentionally omitted here because `new_minimal_vm_harness` does not wire
    // the full `+`/bignum builtins (it signals on that fallback), so it cannot
    // serve as the oracle for the slow path.

    /// Phase-8 micro-benchmark: the hot fixnum countdown loop, Tier 0 vs JIT.
    /// `#[ignore]`d (timing does not belong in CI); run explicitly, in release:
    /// `cargo nextest run --cargo-profile release --features jit --run-ignored all jit_bench`
    #[test]
    #[ignore = "manual perf measurement; run in release"]
    fn jit_bench_countdown_loop() {
        use crate::emacs_core::bytecode::Vm;
        use crate::emacs_core::eval::Context;
        use std::time::Instant;

        // (lambda (n) (while (> n 0) (setq n (1- n))) n)
        let ops = [
            Op::StackRef(0),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(8),
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::Goto(0),
            Op::StackRef(0),
            Op::Return,
        ];
        let constants = [Value::make_int(0)];
        let iters: i64 = 3_000_000;
        let calls = 5;

        let mut ev = Context::new_minimal_vm_harness();

        // Tier 0.
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops.to_vec();
        f.constants = constants.to_vec().into();
        f.max_stack = 16;
        let t0 = Instant::now();
        for _ in 0..calls {
            let mut vm = Vm::from_context(&mut ev);
            let r = vm.execute(&f, vec![Value::make_int(iters)]).unwrap();
            assert_eq!(r, Value::make_int(0));
        }
        let interp = t0.elapsed();

        // JIT.
        let leaf = lower_leaf(&ops, &constants, 1).unwrap();
        let ctx_ptr = &mut ev as *mut Context as *mut u8;
        let t1 = Instant::now();
        for _ in 0..calls {
            assert_eq!(
                leaf.call(ctx_ptr, &[Value::make_int(iters)]),
                NativeRun::Ok(Value::make_int(0).bits())
            );
        }
        let jit = t1.elapsed();

        eprintln!(
            "[jit-bench] countdown {iters}x{calls}: interp {interp:?}  jit {jit:?}  speedup {:.1}x",
            interp.as_secs_f64() / jit.as_secs_f64()
        );
    }

    /// Differential fuzzing (the Phase-9 discipline, brought forward): generate
    /// seeded random straight-line bodies over the supported non-allocating op
    /// subset, run each through BOTH tiers, and hold the tiering contract:
    /// - `Ok(bits)`  -> the interpreter must produce exactly those bits;
    /// - `Deopt`     -> the seam reruns the interpreter (sound by the poisoning
    ///                  analysis), so any interpreter outcome is acceptable;
    /// - `Signal`    -> the interpreter must also signal.
    #[test]
    fn fuzz_straightline_bodies_match_interpreter() {
        use crate::emacs_core::bytecode::Vm;
        use crate::emacs_core::eval::Context;

        // Deterministic xorshift64* — no external randomness (reproducible; on
        // failure the seed in the assert message reproduces the body).
        fn next(state: &mut u64) -> u64 {
            let mut x = *state;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            *state = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        let mut ev = Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut Context as *mut u8;

        // Constant pool: small fixnums, the fixnum boundaries, nil and t —
        // enough to hit fast paths, deopt boundaries, and type guards. No heap
        // values, so Ok-results compare exactly by bits.
        let constants: Vec<Value> = vec![
            Value::make_int(0),
            Value::make_int(1),
            Value::make_int(-1),
            Value::make_int(2),
            Value::make_int(3),
            Value::make_int(Value::MOST_POSITIVE_FIXNUM),
            Value::make_int(Value::MOST_NEGATIVE_FIXNUM),
            Value::NIL,
            Value::T,
        ];

        for seed in 1u64..=600 {
            let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            let len = 1 + (next(&mut rng) % 18) as usize;
            let mut ops: Vec<Op> = Vec::with_capacity(len + 2);
            let mut depth: usize = 0;
            for _ in 0..len {
                let r = (next(&mut rng) % 100) as usize;
                let op = if depth == 0 || r < 30 {
                    // Pushes (always valid).
                    match next(&mut rng) % 3 {
                        0 => Op::Nil,
                        1 => Op::True,
                        _ => Op::Constant((next(&mut rng) % constants.len() as u64) as u16),
                    }
                } else if depth >= 2 && r < 60 {
                    // Binary ops.
                    match next(&mut rng) % 11 {
                        0 => Op::Add,
                        1 => Op::Sub,
                        2 => Op::Mul,
                        3 => Op::Div,
                        4 => Op::Rem,
                        5 => Op::Eqlsign,
                        6 => Op::Lss,
                        7 => Op::Gtr,
                        8 => Op::Leq,
                        9 => Op::Geq,
                        _ => Op::Eq,
                    }
                } else if r < 85 {
                    // Unary ops (depth >= 1).
                    match next(&mut rng) % 10 {
                        0 => Op::Add1,
                        1 => Op::Sub1,
                        2 => Op::Negate,
                        3 => Op::Null,
                        4 => Op::Not,
                        5 => Op::Consp,
                        6 => Op::Stringp,
                        7 => Op::Listp,
                        8 => Op::Symbolp,
                        _ => Op::Dup,
                    }
                } else {
                    // Stack shuffles.
                    match next(&mut rng) % 3 {
                        0 => Op::Dup,
                        1 => Op::StackRef((next(&mut rng) % depth as u64) as u16),
                        _ if depth >= 2 => {
                            Op::StackSet(1 + (next(&mut rng) % (depth as u64 - 1)) as u16)
                        }
                        _ => Op::Pop,
                    }
                };
                let (needs, delta) = simple_effect(&op).expect("generator emits supported ops");
                if depth < needs {
                    continue; // skip an op the current depth can't support
                }
                depth = (depth as i64 + delta) as usize;
                ops.push(op);
            }
            if depth == 0 {
                ops.push(Op::Constant(0));
            }
            ops.push(Op::Return);

            // Tier 0 (oracle).
            let mut f = ByteCodeFunction::new(LambdaParams {
                required: Vec::new(),
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = ops.clone();
            f.constants = constants.clone().into();
            f.max_stack = 64;
            let interp = {
                let mut vm = Vm::from_context(&mut ev);
                vm.execute(&f, vec![])
            };

            // JIT.
            let leaf = lower_leaf(&ops, &constants, 0)
                .unwrap_or_else(|e| panic!("seed {seed}: body must compile, got {e}: {ops:?}"));
            match leaf.call(ctx_ptr, &[]) {
                NativeRun::Ok(bits) => {
                    let want = interp.as_ref().unwrap_or_else(|e| {
                        panic!("seed {seed}: JIT Ok but interpreter erred ({e:?}): {ops:?}")
                    });
                    assert_eq!(
                        bits,
                        want.bits(),
                        "seed {seed}: JIT/interpreter mismatch on {ops:?}"
                    );
                }
                NativeRun::Deopt => {
                    // The seam reruns the interpreter; nothing further to hold.
                }
                NativeRun::DeoptAt(resume) => {
                    let DeoptResume {
                        pc,
                        stack,
                        handlers,
                        binds,
                        spec_base,
                        cond_base,
                    } = *resume;
                    // Precise deopt: resume mid-function and the result must
                    // match the pure-interpreter run exactly.
                    let mut vm = crate::emacs_core::bytecode::Vm::from_context(&mut ev);
                    let resumed = vm.run_resumed_frame(
                        &f,
                        Value::NIL,
                        pc,
                        &stack,
                        handlers,
                        &binds,
                        spec_base,
                        cond_base,
                    );
                    match (&resumed, &interp) {
                        (Ok(got), Ok(want)) => assert_eq!(
                            got.bits(),
                            want.bits(),
                            "seed {seed}: resume/interpreter mismatch on {ops:?}"
                        ),
                        (Err(_), Err(_)) => {}
                        other => panic!(
                            "seed {seed}: resume/interpreter outcome mismatch {other:?}: {ops:?}"
                        ),
                    }
                }
                NativeRun::Signal => {
                    let _ = take_pending_flow();
                    assert!(
                        interp.is_err(),
                        "seed {seed}: JIT signaled but interpreter succeeded: {ops:?}"
                    );
                }
            }

            // Also exercise the typed-MIR Tier-2 path (build_mir + lower_mir_pure)
            // on the same body, skipping bodies the pure subset bails on. Localizes
            // lower_mir_pure miscompiles (the module-test failures under MIR wiring).
            if let Ok(mir) = mir::build_mir(&ops, &constants, 0) {
                if let Ok(mleaf) = lower_mir_pure(&mir) {
                    match mleaf.call(ctx_ptr, &[]) {
                        NativeRun::Ok(bits) => {
                            if let Ok(want) = &interp {
                                assert_eq!(
                                    bits,
                                    want.bits(),
                                    "seed {seed}: MIR/interpreter mismatch on {ops:?}"
                                );
                            }
                        }
                        NativeRun::Deopt | NativeRun::DeoptAt(_) => {}
                        NativeRun::Signal => {
                            let _ = take_pending_flow();
                        }
                    }
                }
            }
        }
    }

    /// Differential fuzzing for SIDE EFFECTS — the gap the return-value fuzzer
    /// above leaves open. Bodies mix arithmetic with `VarSet`/`VarRef` on seeded
    /// special variables and run through the REAL tier dispatch
    /// (`compile_bytecode_function`: MIR if it claims the body, else baseline),
    /// comparing the return value AND the FINAL VALUE OF EVERY SEEDED VARIABLE
    /// against the interpreter. Return-value comparison alone missed the
    /// 0-result-opaque-drop bug for a month — a compiled `setq` returned the
    /// right value (via the bytecode `Dup`) while silently skipping the
    /// assignment — so this pins the state contract: a dropped or mis-lowered
    /// side-effecting op in ANY tier fails here, whichever tier serves the body.
    ///
    /// Extra invariant held: a plain `Deopt` (rerun-from-start) may only come
    /// from a guard BEFORE any side effect (the poisoning analysis), so on
    /// `Deopt` every seeded variable must still hold its initial value.
    #[test]
    fn fuzz_varset_bodies_match_interpreter_state() {
        use crate::emacs_core::bytecode::Vm;
        use crate::emacs_core::eval::Context;
        use crate::emacs_core::intern::SymId;

        fn next(state: &mut u64) -> u64 {
            let mut x = *state;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            *state = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        let mut ev = Context::new_minimal_vm_harness();
        let ctx_ptr = &mut ev as *mut Context as *mut u8;

        // Seeded special variables at constant indices 0..VARS; every stored
        // value in the op soup is an immediate, so state compares exactly by bits.
        const VARS: usize = 3;
        let var_vals: Vec<Value> = ["fuzz-jit-var-a", "fuzz-jit-var-b", "fuzz-jit-var-c"]
            .iter()
            .map(|n| Value::symbol(n))
            .collect();
        let var_ids: Vec<SymId> = var_vals
            .iter()
            .map(|v| match v.kind() {
                crate::emacs_core::value::ValueKind::Symbol(id) => id,
                _ => panic!("symbol expected"),
            })
            .collect();
        let init = [Value::make_int(10), Value::make_int(-7), Value::NIL];

        let mut constants: Vec<Value> = var_vals.clone();
        constants.extend([
            Value::make_int(0),
            Value::make_int(1),
            Value::make_int(-1),
            Value::make_int(3),
            Value::make_int(Value::MOST_POSITIVE_FIXNUM),
            Value::NIL,
            Value::T,
        ]);

        fn reset(ev: &mut Context, ids: &[SymId], init: &[Value]) {
            for (id, v) in ids.iter().zip(init.iter()) {
                ev.obarray.set_symbol_value_id(*id, *v);
            }
        }
        fn snap(ev: &Context, ids: &[SymId]) -> Vec<Option<usize>> {
            ids.iter()
                .map(|id| ev.obarray.symbol_value_id(*id).copied().map(|v| v.bits()))
                .collect()
        }

        for seed in 1u64..=300 {
            let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            let len = 2 + (next(&mut rng) % 16) as usize;
            let mut ops: Vec<Op> = Vec::with_capacity(len + 3);
            let mut depth: usize = 0;
            let mut emitted_varset = false;
            for _ in 0..len {
                let r = (next(&mut rng) % 100) as usize;
                let op = if depth == 0 || r < 25 {
                    match next(&mut rng) % 4 {
                        0 => Op::Nil,
                        1 => Op::True,
                        2 => Op::VarRef((next(&mut rng) % VARS as u64) as u16),
                        _ => Op::Constant(
                            (VARS as u64 + next(&mut rng) % (constants.len() - VARS) as u64) as u16,
                        ),
                    }
                } else if r < 45 {
                    // The point of this fuzzer: a side-effecting VarSet.
                    emitted_varset = true;
                    Op::VarSet((next(&mut rng) % VARS as u64) as u16)
                } else if depth >= 2 && r < 70 {
                    match next(&mut rng) % 8 {
                        0 => Op::Add,
                        1 => Op::Sub,
                        2 => Op::Mul,
                        3 => Op::Div,
                        4 => Op::Eqlsign,
                        5 => Op::Lss,
                        6 => Op::Gtr,
                        _ => Op::Eq,
                    }
                } else if r < 90 {
                    match next(&mut rng) % 6 {
                        0 => Op::Add1,
                        1 => Op::Sub1,
                        2 => Op::Negate,
                        3 => Op::Null,
                        4 => Op::Not,
                        _ => Op::Dup,
                    }
                } else {
                    match next(&mut rng) % 2 {
                        0 => Op::Dup,
                        _ => Op::StackRef((next(&mut rng) % depth as u64) as u16),
                    }
                };
                let (needs, delta) = simple_effect(&op).expect("generator emits supported ops");
                if depth < needs {
                    continue;
                }
                depth = (depth as i64 + delta) as usize;
                ops.push(op);
            }
            // Guarantee at least one VarSet per body so no seed degenerates into
            // the pure fuzzer above.
            if !emitted_varset {
                ops.push(Op::Constant(VARS as u16)); // a fixnum
                ops.push(Op::VarSet((seed % VARS as u64) as u16));
            }
            if depth == 0 {
                ops.push(Op::Constant(VARS as u16));
            }
            ops.push(Op::Return);

            let mut f = ByteCodeFunction::new(LambdaParams {
                required: Vec::new(),
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = ops.clone();
            f.constants = constants.clone().into();
            f.max_stack = 64;

            // Tier 0 (oracle): result + final variable state.
            reset(&mut ev, &var_ids, &init);
            let interp = {
                let mut vm = Vm::from_context(&mut ev);
                vm.execute(&f, vec![])
            };
            let want_state = snap(&ev, &var_ids);

            // The REAL tier dispatch (MIR if it claims the body, else baseline);
            // fall back to a direct baseline lowering if the dispatch declines
            // (e.g. profitability) so every seed still gets state coverage.
            let leaf = compile_bytecode_function(&f)
                .or_else(|_| lower_leaf(&ops, &constants, 0))
                .unwrap_or_else(|e| panic!("seed {seed}: body must compile, got {e}: {ops:?}"));
            reset(&mut ev, &var_ids, &init);
            let init_state = snap(&ev, &var_ids);
            match leaf.call(ctx_ptr, &[]) {
                NativeRun::Ok(bits) => {
                    let want = interp.as_ref().unwrap_or_else(|e| {
                        panic!("seed {seed}: JIT Ok but interpreter erred ({e:?}): {ops:?}")
                    });
                    assert_eq!(bits, want.bits(), "seed {seed}: result mismatch on {ops:?}");
                    assert_eq!(
                        snap(&ev, &var_ids),
                        want_state,
                        "seed {seed}: SIDE-EFFECT STATE mismatch (a VarSet was dropped or mis-lowered) on {ops:?}"
                    );
                }
                NativeRun::Deopt => {
                    // Rerun-from-start is only sound if no side effect ran yet
                    // (VarSet poisons later guards) — the vars must be untouched.
                    assert_eq!(
                        snap(&ev, &var_ids),
                        init_state,
                        "seed {seed}: rerun-from-start deopt AFTER a side effect on {ops:?}"
                    );
                    let rerun = {
                        let mut vm = Vm::from_context(&mut ev);
                        vm.execute(&f, vec![])
                    };
                    match (&rerun, &interp) {
                        (Ok(got), Ok(want)) => {
                            assert_eq!(got.bits(), want.bits(), "seed {seed}: {ops:?}");
                            assert_eq!(snap(&ev, &var_ids), want_state, "seed {seed}: {ops:?}");
                        }
                        (Err(_), Err(_)) => {}
                        other => panic!("seed {seed}: deopt-rerun mismatch {other:?}: {ops:?}"),
                    }
                }
                NativeRun::DeoptAt(resume) => {
                    let DeoptResume {
                        pc,
                        stack,
                        handlers,
                        binds,
                        spec_base,
                        cond_base,
                    } = *resume;
                    // Precise deopt: resume mid-function on the MUTATED state.
                    let resumed = {
                        let mut vm = Vm::from_context(&mut ev);
                        vm.run_resumed_frame(
                            &f,
                            Value::NIL,
                            pc,
                            &stack,
                            handlers,
                            &binds,
                            spec_base,
                            cond_base,
                        )
                    };
                    match (&resumed, &interp) {
                        (Ok(got), Ok(want)) => {
                            assert_eq!(got.bits(), want.bits(), "seed {seed}: {ops:?}");
                            assert_eq!(snap(&ev, &var_ids), want_state, "seed {seed}: {ops:?}");
                        }
                        (Err(_), Err(_)) => {}
                        other => panic!("seed {seed}: resume mismatch {other:?}: {ops:?}"),
                    }
                }
                NativeRun::Signal => {
                    let _ = take_pending_flow();
                    assert!(
                        interp.is_err(),
                        "seed {seed}: JIT signaled but interpreter succeeded: {ops:?}"
                    );
                    // Same deterministic prefix ran on both engines before the
                    // signal, so the partial writes must agree too.
                    assert_eq!(
                        snap(&ev, &var_ids),
                        want_state,
                        "seed {seed}: state mismatch after signal on {ops:?}"
                    );
                }
            }

            // Also pin the MIR tier explicitly on the same body — the exact
            // historical bug shape: build_mir once DROPPED the 0-result VarSet, so
            // lower_mir_pure succeeded (nothing to bail on) and returned a leaf
            // whose return value was right and whose side effect was gone. Today
            // lower_mir_pure bails on the Opaque (Err, skipped below); if a future
            // MIR VarSet port lands, this holds it to the same state contract.
            if let Ok(mir) = mir::build_mir(&ops, &constants, 0) {
                if let Ok(mleaf) = lower_mir_pure(&mir) {
                    reset(&mut ev, &var_ids, &init);
                    match mleaf.call(ctx_ptr, &[]) {
                        NativeRun::Ok(bits) => {
                            if let Ok(want) = &interp {
                                assert_eq!(bits, want.bits(), "seed {seed}: MIR result: {ops:?}");
                            }
                            assert_eq!(
                                snap(&ev, &var_ids),
                                want_state,
                                "seed {seed}: MIR SIDE-EFFECT STATE mismatch on {ops:?}"
                            );
                        }
                        NativeRun::Deopt => {
                            assert_eq!(
                                snap(&ev, &var_ids),
                                init_state,
                                "seed {seed}: MIR rerun-deopt after a side effect on {ops:?}"
                            );
                        }
                        NativeRun::DeoptAt(_) => {}
                        NativeRun::Signal => {
                            let _ = take_pending_flow();
                        }
                    }
                }
            }
        }
    }

    /// B1 (C1): a slot the AOT loader DISARMED (`epoch == SPEC_EPOCH_DISARMED`)
    /// reports NOT-armed and NEVER re-arms — even when the live binding would
    /// otherwise re-validate. Proves the shared subr/pred/eq arming helper
    /// short-circuits on the sentinel BEFORE any obarray re-validation, so a
    /// mis-baked kind can never run the wrong op. (JIT never sets DISARMED; this
    /// path is reached only by loader-armed AOT leaves — see the x-session tests.)
    #[test]
    fn disarmed_spec_slot_never_arms_and_does_not_rearm() {
        use crate::emacs_core::eval::Context;
        let ev = Context::new();
        // Control precondition: no compiler function overrides active (else the
        // helper returns false regardless — the assumption the control relies on).
        assert!(
            !ev.compiler_function_overrides_active(),
            "test assumes no active compiler function overrides"
        );
        // `car` is a canonical builtin fbound in every obarray; use its real
        // binding as the (would-be) callee VALUE so the helper COULD re-validate.
        let car = match Value::symbol("car").kind() {
            crate::emacs_core::value::ValueKind::Symbol(id) => id,
            _ => panic!("symbol"),
        };
        let expected = ev
            .obarray
            .symbol_function_id(car)
            .expect("car fbound")
            .bits() as i64;
        let disarmed = SpecSlot {
            epoch: AtomicU64::new(SPEC_EPOCH_DISARMED),
            leaf: AtomicU64::new(0),
        };
        // Even though (sym, expected) MATCHES the live binding, the DISARMED
        // sentinel forces `false` and leaves the epoch untouched (no re-arm).
        assert!(
            !subr_spec_armed(&ev, car.0 as i64, expected, &disarmed),
            "a DISARMED slot must report not-armed"
        );
        assert_eq!(
            disarmed.epoch.load(Ordering::Relaxed),
            SPEC_EPOCH_DISARMED,
            "a DISARMED slot must not re-arm (epoch unchanged)"
        );
        // Control: the SAME (sym, expected) on a fresh slot DOES arm via the
        // re-validate path — so the assertion above proves the guard, not a dead
        // binding. (A fresh epoch of 0 forces the re-validate branch, which stores
        // the live epoch and returns true because `expected` matches the cell.)
        let fresh = SpecSlot {
            epoch: AtomicU64::new(0),
            leaf: AtomicU64::new(0),
        };
        assert!(
            subr_spec_armed(&ev, car.0 as i64, expected, &fresh),
            "control: a matching live binding arms a non-disarmed slot"
        );
    }

    /// B1 (C2): `SPEC_EPOCH_DISARMED` is the reserved `u64::MAX`, and the obarray
    /// never hands out a live `function_epoch` equal to it (the bump skips it).
    #[test]
    fn function_epoch_never_equals_disarmed_sentinel() {
        assert_eq!(SPEC_EPOCH_DISARMED, u64::MAX);
        let mut ev = crate::emacs_core::eval::Context::new();
        for _ in 0..8 {
            ev.obarray.bump_function_epoch();
            assert_ne!(
                ev.obarray.function_epoch(),
                SPEC_EPOCH_DISARMED,
                "a live function_epoch must never equal the DISARMED sentinel"
            );
        }
    }

    /// COMMIT A compile-time assert: not one `SubrFn::Many` allowlist name is a
    /// known `ManySlice` variadic — the two sets are disjoint by construction, so
    /// no arithmetic/list ManySlice builtin can ever leak onto the allowlist.
    #[test]
    fn subr_spec_many_allowlist_disjoint_from_manyslice() {
        const MANYSLICE: &[&str] = &[
            "+",
            "logand",
            "logior",
            "logxor",
            "list",
            "vector",
            "append",
            "nconc",
            "string-match",
        ];
        for name in SUBR_MANY_ALLOWLIST {
            assert!(
                !MANYSLICE.contains(name),
                "{name:?} is a ManySlice variadic and must not be on SUBR_MANY_ALLOWLIST"
            );
        }
    }

    /// COMMIT A ManySlice-rejection: the classifier ACCEPTS every allowlisted
    /// `SubrFn::Many` builtin (as `SubrGeneral`) at a representative in-range
    /// arity, and REJECTS every registered `ManySlice` variadic
    /// (`+`/logand/logior/logxor/list/vector/append/nconc/string-match). The
    /// `SubrFn::Many` match in `subr_spec_kind` — NOT the allowlist — does the
    /// ManySlice exclusion, so no ManySlice subr ever classifies regardless of
    /// what the allowlist names.
    #[test]
    fn subr_spec_kind_rejects_registered_manyslice() {
        use crate::emacs_core::eval::Context;
        let ev = Context::new();
        let sid = |name: &str| match Value::symbol(name).kind() {
            crate::emacs_core::value::ValueKind::Symbol(id) => id,
            _ => panic!("symbol"),
        };
        // ACCEPT: each allowlisted Many builtin classifies as SubrGeneral.
        for (name, nargs) in [
            ("re-search-forward", 1usize),
            ("looking-at", 1),
            ("parse-partial-sexp", 2),
            ("match-data", 0),
            ("set-match-data", 1),
            ("scan-sexps", 2),
            ("intern-soft", 1),
            ("line-end-position", 0),
            ("syntax-table", 0),
            ("set-syntax-table", 1),
            ("put-text-property", 4),
        ] {
            let id = sid(name);
            let binding = ev
                .obarray
                .symbol_function_id(id)
                .unwrap_or_else(|| panic!("{name} fbound"));
            assert_eq!(
                subr_spec_kind(binding, id, nargs),
                Some(SpecCalleeKind::SubrGeneral),
                "{name} (allowlisted Many) must classify as SubrGeneral"
            );
        }
        // REJECT: every registered ManySlice variadic (EXCEPT the bitwise-arith
        // intrinsics logand/logior/logxor, checked separately below) stays generic
        // at any arity.
        for name in ["+", "list", "vector", "append", "nconc", "string-match"] {
            let id = sid(name);
            let binding = ev
                .obarray
                .symbol_function_id(id)
                .unwrap_or_else(|| panic!("{name} fbound"));
            for nargs in [0usize, 2, 4] {
                assert_eq!(
                    subr_spec_kind(binding, id, nargs),
                    None,
                    "{name} is ManySlice and must NEVER classify (nargs={nargs})"
                );
            }
        }
    }

    /// The bitwise-arith intrinsics (logand/logior/logxor) — `ManySlice` variadics
    /// that would otherwise get full generic dispatch — classify as
    /// `ArithIntrinsic` at EXACTLY 2 args (the GC-free fixnum fast path), and stay
    /// generic (`None`) at every other arity (0=const, 1=identity, ≥3=reduction).
    #[test]
    fn subr_spec_kind_classifies_bitwise_arith_at_two_args() {
        use crate::emacs_core::eval::Context;
        let ev = Context::new();
        let sid = |name: &str| match Value::symbol(name).kind() {
            crate::emacs_core::value::ValueKind::Symbol(id) => id,
            _ => panic!("symbol"),
        };
        // (name, op, the ONE arity that intrinsifies) — every other arity stays generic.
        for (name, op, good_arity) in [
            ("logand", ARITH_KIND_LOGAND as u8, 2usize),
            ("logior", ARITH_KIND_LOGIOR as u8, 2),
            ("logxor", ARITH_KIND_LOGXOR as u8, 2),
            ("ash", ARITH_KIND_ASH as u8, 2),
            ("lognot", ARITH_KIND_LOGNOT as u8, 1),
        ] {
            let id = sid(name);
            let binding = ev
                .obarray
                .symbol_function_id(id)
                .unwrap_or_else(|| panic!("{name} fbound"));
            assert_eq!(
                subr_spec_kind(binding, id, good_arity),
                Some(SpecCalleeKind::ArithIntrinsic { op }),
                "{name} at {good_arity} args must intrinsify with op {op}"
            );
            // Each op gets a distinct discriminant (AOT loader disarms on mismatch).
            assert_eq!(
                SpecCalleeKind::ArithIntrinsic { op }.to_spec_disc(),
                Some(5 + op),
                "{name} disc is 5+op"
            );
            // At any OTHER arity the site must not classify as ArithIntrinsic
            // (fixed-arity ash/lognot become None on arity mismatch; the ManySlice
            // and/or/xor become None too — never a bit-op intrinsic).
            for nargs in [0usize, 1, 2, 3, 4] {
                if nargs == good_arity {
                    continue;
                }
                assert!(
                    !matches!(
                        subr_spec_kind(binding, id, nargs),
                        Some(SpecCalleeKind::ArithIntrinsic { .. })
                    ),
                    "{name} must not arith-intrinsify at nargs={nargs}"
                );
            }
        }
        // The five discs are pairwise distinct and within DISC_COUNT.
        let discs: Vec<u8> = [
            ARITH_KIND_LOGAND,
            ARITH_KIND_LOGIOR,
            ARITH_KIND_LOGXOR,
            ARITH_KIND_ASH,
            ARITH_KIND_LOGNOT,
        ]
        .iter()
        .map(|&op| {
            SpecCalleeKind::ArithIntrinsic { op: op as u8 }
                .to_spec_disc()
                .unwrap()
        })
        .collect();
        assert_eq!(discs, vec![5, 6, 7, 8, 9]);
        assert!(discs.iter().all(|&d| d < SpecCalleeKind::DISC_COUNT));
    }

    /// The `ash_fixnum_fast` helper matches GNU `Fash` for the fixnum cases and
    /// returns `None` exactly when the result would leave fixnum range (→ generic
    /// bignum path).
    #[test]
    fn ash_fixnum_fast_matches_gnu_and_defers_overflow() {
        // Left shifts that stay in range.
        assert_eq!(ash_fixnum_fast(1, 4), Some(16));
        assert_eq!(ash_fixnum_fast(-1, 1), Some(-2));
        assert_eq!(ash_fixnum_fast(3, 0), Some(3));
        // Right shifts (arithmetic, floor toward -inf) — always a fixnum.
        assert_eq!(ash_fixnum_fast(16, -2), Some(4));
        assert_eq!(ash_fixnum_fast(-3, -1), Some(-2)); // floor(-1.5) = -2
        assert_eq!(ash_fixnum_fast(1, -100), Some(0)); // shifted away -> 0
        assert_eq!(ash_fixnum_fast(-1, -100), Some(-1)); // negative -> -1
        // Left shift that overflows fixnum range -> None (generic makes a bignum).
        assert_eq!(ash_fixnum_fast(1, 61), None); // 2^61 > MOST_POSITIVE_FIXNUM
        assert_eq!(ash_fixnum_fast(Value::MOST_POSITIVE_FIXNUM, 1), None);
        assert_eq!(ash_fixnum_fast(1, 64), None); // >= 64: undefined shift, defer
        assert_eq!(ash_fixnum_fast(1, 1000), None);
        // Largest in-range left shift boundary.
        assert_eq!(ash_fixnum_fast(1, 60), Some(1i64 << 60));
    }

    // ---- Panic containment at the shim boundary ----

    /// A leaf whose `Op::Call` invokes the always-registered internal panic
    /// subr: the panic originates in host code reached through
    /// `neovm_jit_call`, the same class a buggy builtin would raise.
    fn panicking_call_leaf(msg: &str) -> CompiledLeaf {
        lower_nullary_leaf(
            &[Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
            &[Value::symbol("neovm--internal-panic"), Value::string(msg)],
        )
        .expect("call body compiles")
    }

    #[test]
    fn contained_shim_panic_surfaces_as_error_flow_and_vm_survives() {
        let mut ev = crate::emacs_core::eval::Context::new();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let leaf = panicking_call_leaf("shim-boom");
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
        let flow = take_pending_flow().expect("contained panic stashes a flow");
        let Flow::Signal(sig) = flow else {
            panic!("expected Signal, got {flow:?}");
        };
        assert_eq!(sig.symbol_name(), "error");
        let msg = sig.data[0].as_str_owned().expect("string payload");
        assert!(
            msg.contains("neomacs internal error") && msg.contains("shim-boom"),
            "unexpected message: {msg}"
        );
        // No one-shot state: containment works again.
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
        let _ = take_pending_flow().expect("second containment works");
        // The evaluator survives: a normal compiled call and a full GC run.
        let ok = lower_nullary_leaf(&[Op::Constant(0), Op::Return], &[Value::make_int(7)])
            .expect("trivial body compiles");
        assert_eq!(
            ok.call(ctx_ptr, &[]),
            NativeRun::Ok(Value::make_int(7).bits())
        );
        ev.funcall_general_untraced(Value::symbol("garbage-collect"), vec![])
            .expect("garbage-collect succeeds after containment");
    }

    #[test]
    fn contained_shim_panic_restores_boundary_state() {
        let mut ev = crate::emacs_core::eval::Context::new();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        // Interpreted middle function: dynamically binds, then calls the
        // panicking subr — the panic unwinds through its LIVE interpreter
        // frame, skipping cleanup_bytecode_frame (the bc_frames pop + depth
        // decrement) and its Unbind. Exactly the residue the leaf-exit
        // healing must truncate / the leaf-exit unwind must sweep.
        let var = Value::symbol("jit-t5-dynvar");
        let mid_sym = Value::symbol("jit-t5-middle");
        let crate::emacs_core::value::ValueKind::Symbol(mid_id) = mid_sym.kind() else {
            panic!("symbol expected");
        };
        let mut mid = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        mid.lexical = true;
        mid.ops = vec![
            Op::Constant(1), // 5
            Op::VarBind(0),  // bind jit-t5-dynvar := 5 (leaked by the panic)
            Op::Constant(2), // 'neovm--internal-panic
            Op::Constant(3), // "mid-boom"
            Op::Call(1),
            Op::Unbind(1),
            Op::Return,
        ];
        mid.constants = vec![
            var,
            Value::make_int(5),
            Value::symbol("neovm--internal-panic"),
            Value::string("mid-boom"),
        ]
        .into();
        mid.max_stack = 16;
        ev.obarray
            .set_symbol_function_id(mid_id, Value::make_bytecode(mid));
        // The leaf binds too, so it carries has_binds: its exit parity unwind
        // is the depth-based sweep that must also collect the middle's leaked
        // binding (the deferred-specpdl half of the recovery contract).
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(1), // 5
                Op::VarBind(0),  // leaf's own binding
                Op::Constant(2), // 'jit-t5-middle
                Op::Call(0),
                Op::Unbind(1),
                Op::Return,
            ],
            &[var, Value::make_int(5), mid_sym],
        )
        .expect("binding call body compiles");
        let depth0 = ev.depth;
        let frames0 = ev.bc_frames.len();
        let buf0 = ev.bc_buf.len();
        let cond0 = ev.condition_stack.len();
        let spec0 = ev.specpdl.len();
        let roots0 = crate::emacs_core::eval::save_scratch_gc_roots();
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
        let _ = take_pending_flow().expect("panic flow stashed");
        assert_eq!(ev.depth, depth0, "lisp depth restored");
        assert_eq!(ev.bc_frames.len(), frames0, "bc_frames truncated");
        assert_eq!(ev.bc_buf.len(), buf0, "bc_buf truncated");
        assert_eq!(
            ev.condition_stack.len(),
            cond0,
            "condition frames truncated"
        );
        assert_eq!(
            ev.specpdl.len(),
            spec0,
            "specpdl unwound (leaf bind + leaked middle bind) at leaf exit"
        );
        assert_eq!(
            crate::emacs_core::eval::save_scratch_gc_roots(),
            roots0,
            "scratch roots restored"
        );
    }

    #[test]
    fn contained_shim_panic_is_caught_by_leaf_local_condition_case() {
        let mut ev = crate::emacs_core::eval::Context::new();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let cond0 = ev.condition_stack.len();
        // (condition-case around the panicking call, in the SAME compiled
        // function): the contained panic must flow through the match shim and
        // resume at this leaf's own handler, like any Lisp error.
        let leaf = lower_nullary_leaf(
            &[
                Op::PushConditionCase(6),
                Op::Constant(0), // 'neovm--internal-panic
                Op::Constant(1), // "caught-locally"
                Op::Call(1),
                Op::PopHandler,
                Op::Return,
                Op::Return, // 6: handler entry [err]
            ],
            &[
                Value::symbol("neovm--internal-panic"),
                Value::string("caught-locally"),
            ],
        )
        .expect("handler body compiles");
        let NativeRun::Ok(bits) = leaf.call(ctx_ptr, &[]) else {
            panic!("expected the leaf-local handler to catch the contained panic");
        };
        let err = Value::from_bits(bits);
        assert_eq!(
            err.cons_car().as_symbol_name().as_deref(),
            Some("error"),
            "binding is (error ...)"
        );
        let msg = err
            .cons_cdr()
            .cons_car()
            .as_str_owned()
            .expect("message string");
        assert!(
            msg.contains("neomacs internal error") && msg.contains("caught-locally"),
            "unexpected message: {msg}"
        );
        assert_eq!(ev.condition_stack.len(), cond0, "handler frame consumed");
    }

    #[test]
    fn contained_shim_panic_with_leaked_callee_handler_still_matches_leaf_handler() {
        let mut ev = crate::emacs_core::eval::Context::new();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        // Interpreted middle function whose OWN condition-case protects the
        // panicking call: the Rust panic (not a Lisp signal) unwinds straight
        // through the interpreter, so the middle's handler never runs and its
        // condition frame is LEAKED above the leaf's — exactly the residue
        // that would desynchronize the match shim's count-based pops and let
        // the innermost-match scan select the dead frame. The match-entry
        // healing must truncate it so the LEAF's handler catches.
        let mid_sym = Value::symbol("jit-t5-shielded-middle");
        let crate::emacs_core::value::ValueKind::Symbol(mid_id) = mid_sym.kind() else {
            panic!("symbol expected");
        };
        let mut mid = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        mid.lexical = true;
        mid.ops = vec![
            Op::PushConditionCase(6),
            Op::Constant(0), // 'neovm--internal-panic
            Op::Constant(1), // "resid-boom"
            Op::Call(1),
            Op::PopHandler,
            Op::Return,
            Op::Return, // 6: mid's handler (unreachable — panics skip it)
        ];
        mid.constants = vec![
            Value::symbol("neovm--internal-panic"),
            Value::string("resid-boom"),
        ]
        .into();
        mid.max_stack = 16;
        ev.obarray
            .set_symbol_function_id(mid_id, Value::make_bytecode(mid));
        let leaf = lower_nullary_leaf(
            &[
                Op::PushConditionCase(5),
                Op::Constant(0), // 'jit-t5-shielded-middle
                Op::Call(0),
                Op::PopHandler,
                Op::Return,
                Op::Return, // 5: leaf's handler entry [err]
            ],
            &[mid_sym],
        )
        .expect("handler body compiles");
        // Warm one round (interning, lazies) before taking the bases.
        let NativeRun::Ok(_) = leaf.call(ctx_ptr, &[]) else {
            panic!("leaf handler must catch the contained panic");
        };
        let cond0 = ev.condition_stack.len();
        let depth0 = ev.depth;
        let frames0 = ev.bc_frames.len();
        let roots0 = crate::emacs_core::eval::save_scratch_gc_roots();
        for _ in 0..2 {
            let NativeRun::Ok(bits) = leaf.call(ctx_ptr, &[]) else {
                panic!("leaf handler must catch the contained panic");
            };
            let err = Value::from_bits(bits);
            assert_eq!(
                err.cons_car().as_symbol_name().as_deref(),
                Some("error"),
                "binding is (error ...)"
            );
            let msg = err
                .cons_cdr()
                .cons_car()
                .as_str_owned()
                .expect("message string");
            assert!(
                msg.contains("neomacs internal error") && msg.contains("resid-boom"),
                "unexpected message: {msg}"
            );
        }
        assert_eq!(
            ev.condition_stack.len(),
            cond0,
            "leaked callee frame truncated + leaf frame consumed, every round"
        );
        assert_eq!(ev.depth, depth0, "lisp depth healed at the match shim");
        assert_eq!(
            ev.bc_frames.len(),
            frames0,
            "bc_frames healed at the match shim"
        );
        assert_eq!(
            crate::emacs_core::eval::save_scratch_gc_roots(),
            roots0,
            "root residue of locally-caught panics swept at leaf exit"
        );
    }

    #[test]
    fn contained_shim_panics_leave_no_residue_over_repeats() {
        let mut ev = crate::emacs_core::eval::Context::new();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        // A BINDING leaf: containment defers specpdl to the next depth-based
        // unwind, which for a `has_binds` leaf is its own exit parity unwind
        // (in production a bind-less leaf is always under an enclosing frame
        // whose `cleanup_bytecode_frame`/handler unwind does the same sweep;
        // this raw `leaf.call` harness has no enclosing frame, so the leaf
        // supplies the sweep itself — the production shape, minus the middle
        // man). The callee-dispatch backtrace entry the panic leaks each
        // round must be collected by that unwind.
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(2), // 5
                Op::VarBind(1),  // bind jit-t5-loopvar
                Op::Constant(0), // 'neovm--internal-panic
                Op::Constant(3), // "looped"
                Op::Call(1),
                Op::Unbind(1),
                Op::Return,
            ],
            &[
                Value::symbol("neovm--internal-panic"),
                Value::symbol("jit-t5-loopvar"),
                Value::make_int(5),
                Value::string("looped"),
            ],
        )
        .expect("binding call body compiles");
        // Warm one containment (interning, lazies) before taking the bases.
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
        let _ = take_pending_flow();
        let roots0 = crate::emacs_core::eval::save_scratch_gc_roots();
        let base = (
            ev.depth,
            ev.bc_frames.len(),
            ev.bc_buf.len(),
            ev.condition_stack.len(),
            ev.specpdl.len(),
        );
        for _ in 0..16 {
            assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
            let _ = take_pending_flow().expect("flow each iteration");
        }
        assert_eq!(
            crate::emacs_core::eval::save_scratch_gc_roots(),
            roots0,
            "scratch-root depth stable over repeated containments"
        );
        assert_eq!(
            (
                ev.depth,
                ev.bc_frames.len(),
                ev.bc_buf.len(),
                ev.condition_stack.len(),
                ev.specpdl.len(),
            ),
            base,
            "no per-containment residue"
        );
    }

    #[test]
    fn gc_suspect_shim_panic_is_re_raised_not_contained() {
        let mut ev = crate::emacs_core::eval::Context::new();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        ev.gc_driver_active = true;
        let payload: Box<dyn std::any::Any + Send> = Box::new("must-flee".to_string());
        let back = contain_jit_shim_panic(ctx_ptr, payload)
            .expect_err("GC-suspect panic must be re-raised, not contained");
        assert_eq!(back.downcast_ref::<String>().unwrap(), "must-flee");
        ev.gc_driver_active = false;
        assert!(
            take_pending_flow().is_none(),
            "nothing stashed on the re-raise path"
        );
    }

    #[test]
    fn contained_panic_wins_over_stale_pending_flow() {
        // A shim body that stashed a real flow and THEN panicked before
        // completing its protocol: the panic must win at take time, and both
        // slots must be consumed.
        let mut ev = crate::emacs_core::eval::Context::new();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        stash_pending_flow(signal("arith-error", vec![]));
        let payload: Box<dyn std::any::Any + Send> = Box::new("late-panic");
        contain_jit_shim_panic(ctx_ptr, payload).expect("containable");
        let flow = take_pending_flow().expect("panic flow present");
        let Flow::Signal(sig) = flow else {
            panic!("expected Signal");
        };
        assert_eq!(sig.symbol_name(), "error");
        assert!(
            sig.data[0]
                .as_str_owned()
                .expect("string payload")
                .contains("late-panic")
        );
        assert!(take_pending_flow().is_none(), "both slots consumed");
    }

    #[test]
    fn parked_panic_survives_leaf_exit_cleanup_running_compiled_code() {
        // A contained panic in a has_binds leaf whose LEAKED unwind-protect
        // cleanup signals through COMPILED code: the leaf-exit parity unwind
        // runs the cleanup while the panic is parked, so the inner leaf's
        // stash/take cycle must see ITS arith-error (not the outer panic),
        // and the outer dispatcher's take must still get the panic error
        // afterwards (previously the inner take consumed it and the outer
        // take found nothing — an `.expect` panic inside recovery).
        let mut ev = crate::emacs_core::eval::Context::new();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        ev.set_variable("jit-fx-witness", Value::NIL);
        // Cleanup: compiled condition-case around (signal 'arith-error nil),
        // recording the caught err object in the witness variable.
        let mut cleanup = ByteCodeFunction::new(LambdaParams {
            required: Vec::new(),
            optional: Vec::new(),
            rest: None,
        });
        cleanup.lexical = true;
        cleanup.ops = vec![
            Op::PushConditionCase(7),
            Op::Constant(0), // 'signal
            Op::Constant(1), // 'arith-error
            Op::Constant(2), // nil
            Op::Call(2),
            Op::PopHandler,
            Op::Return,
            Op::VarSet(3),   // 7: handler entry [err] -> jit-fx-witness
            Op::Constant(2), // nil
            Op::Return,
        ];
        cleanup.constants = vec![
            Value::symbol("signal"),
            Value::symbol("arith-error"),
            Value::NIL,
            Value::symbol("jit-fx-witness"),
        ]
        .into();
        cleanup.max_stack = 16;
        // Force the cleanup hot so its application inside the parity unwind
        // dispatches through the JIT (engagement asserted below — an
        // interpreted cleanup never touches the pending slots and would
        // pass this test vacuously). The profitability gate would reject
        // this call-only body (calls > arith); bypass it — the test needs
        // THIS body native, profitability is orthogonal.
        force_profit_gate_for_test(false);
        let cleanup_id = cleanup.jit_runtime().compiled_id_or_assign();
        cleanup.jit_runtime().set_hot_for_test();
        let cleanup_val = Value::make_bytecode(cleanup);
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(0), // cleanup fn value
                Op::UnwindProtectPop,
                Op::Constant(1), // 'neovm--internal-panic
                Op::Constant(2), // "park-boom"
                Op::Call(1),
                Op::Unbind(1),
                Op::Return,
            ],
            &[
                cleanup_val,
                Value::symbol("neovm--internal-panic"),
                Value::string("park-boom"),
            ],
        )
        .expect("unwind-protect body compiles");
        let spec0 = ev.specpdl.len();
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
        // The outer dispatcher's take sees the PANIC error, not the
        // cleanup's arith-error and not an empty slot.
        let flow = take_pending_flow().expect("parked panic re-stashed for the dispatcher take");
        let Flow::Signal(sig) = flow else {
            panic!("expected Signal, got {flow:?}");
        };
        assert_eq!(sig.symbol_name(), "error");
        let msg = sig.data[0].as_str_owned().expect("string payload");
        assert!(
            msg.contains("neomacs internal error") && msg.contains("park-boom"),
            "unexpected message: {msg}"
        );
        // Engagement: the cleanup really tiered up and ran native.
        assert!(
            crate::emacs_core::jit::cache::is_compiled_for_test(cleanup_id),
            "cleanup must have compiled — the contamination scenario needs \
             its stash/take cycle to run through the JIT dispatcher"
        );
        // The inner handler saw ITS signal.
        let witness = ev
            .obarray
            .symbol_value("jit-fx-witness")
            .cloned()
            .unwrap_or(Value::NIL);
        assert_eq!(
            witness.cons_car().as_symbol_name().as_deref(),
            Some("arith-error"),
            "inner handler must catch its own arith-error, not the parked panic"
        );
        assert_eq!(
            ev.specpdl.len(),
            spec0,
            "parity unwind swept the leaf's entries"
        );
        assert!(take_pending_flow().is_none(), "slots clean after the take");
    }

    #[test]
    fn wide_arg_call_panic_releases_backtrace_args_cleanly() {
        // A >= 3-argument call stores its args as `BacktraceArgs::Evaluated`
        // (an index into backtrace_args_stack). A contained panic truncates
        // that stack at the boundary while the callee's Backtrace specpdl
        // entry survives for the deferred parity unwind — whose
        // release_backtrace_args must treat the healed residue as a no-op
        // instead of tripping its LIFO debug_assert (debug builds would
        // otherwise re-panic inside recovery).
        let mut ev = crate::emacs_core::eval::Context::new();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let var = Value::symbol("jit-fx-wide-dynvar");
        let mid_sym = Value::symbol("jit-fx-wide-middle");
        let crate::emacs_core::value::ValueKind::Symbol(mid_id) = mid_sym.kind() else {
            panic!("symbol expected");
        };
        // Interpreted 3-arg middle that panics: its dispatch stores a wide
        // Evaluated args entry, then the panic leaks it.
        let mut mid = ByteCodeFunction::new(LambdaParams {
            required: vec![intern("a"), intern("b"), intern("c")],
            optional: Vec::new(),
            rest: None,
        });
        mid.lexical = true;
        mid.ops = vec![
            Op::Constant(0), // 'neovm--internal-panic
            Op::Constant(1), // "wide-boom"
            Op::Call(1),
            Op::Return,
        ];
        mid.constants = vec![
            Value::symbol("neovm--internal-panic"),
            Value::string("wide-boom"),
        ]
        .into();
        mid.max_stack = 16;
        ev.obarray
            .set_symbol_function_id(mid_id, Value::make_bytecode(mid));
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(1), // 5
                Op::VarBind(0),  // has_binds: the exit parity unwind must run
                Op::Constant(2), // 'jit-fx-wide-middle
                Op::Constant(3), // 1
                Op::Constant(4), // 2
                Op::Constant(5), // 3
                Op::Call(3),
                Op::Unbind(1),
                Op::Return,
            ],
            &[
                var,
                Value::make_int(5),
                mid_sym,
                Value::make_int(1),
                Value::make_int(2),
                Value::make_int(3),
            ],
        )
        .expect("wide call body compiles");
        let spec0 = ev.specpdl.len();
        let args0 = ev.backtrace_args_stack_len_for_test();
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
        let flow = take_pending_flow().expect("panic flow stashed");
        let Flow::Signal(sig) = flow else {
            panic!("expected Signal, got {flow:?}");
        };
        let msg = sig.data[0].as_str_owned().expect("string payload");
        assert!(msg.contains("wide-boom"), "unexpected message: {msg}");
        assert_eq!(
            ev.backtrace_args_stack_len_for_test(),
            args0,
            "backtrace args stack back at base"
        );
        assert_eq!(ev.specpdl.len(), spec0, "specpdl swept at leaf exit");
        // The deferred unwind ran clean: a second containment round behaves
        // identically (no cascading re-containment from the release path).
        assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal);
        let _ = take_pending_flow().expect("second round stashes too");
        assert_eq!(ev.backtrace_args_stack_len_for_test(), args0);
        assert_eq!(ev.specpdl.len(), spec0);
    }

    #[test]
    fn ctxless_shim_panic_re_raised_when_gc_locks_poisoned() {
        // The ctx-less wrapped shims probe the lock-poison half of the
        // unrecoverable check through the thread heap: with a poisoned GC
        // lock the panic must be re-raised (abort at the shim in
        // production), stashing nothing. Poison is permanent for this
        // process — fine under nextest's process-per-test.
        crate::tagged::gc::with_tagged_heap(|h| h.poison_gc_locks_for_test());
        let payload: Box<dyn std::any::Any + Send> = Box::new("poisoned-flee");
        let back = contain_jit_shim_panic(core::ptr::null_mut(), payload)
            .expect_err("poisoned GC locks must re-raise on the ctx-less path");
        assert_eq!(back.downcast_ref::<&str>().unwrap(), &"poisoned-flee");
        assert!(
            !shim_panic_pending(),
            "nothing stashed on the re-raise path"
        );
        assert!(take_pending_flow().is_none());
    }

    #[test]
    fn contained_panic_in_load_unwinds_load_bookkeeping() {
        // `load` bookkeeping rides the specpdl: a panic contained mid-load
        // must leave `load-in-progress` nil and `loads_in_progress` empty
        // once the deferred unwind runs, and repeated containment must not
        // accumulate entries into a spurious "Recursive load".
        let mut ev = crate::emacs_core::eval::Context::new();
        let ctx_ptr = &mut ev as *mut crate::emacs_core::eval::Context as *mut u8;
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = dir.path().join("jit-fx-panic-load.el");
        std::fs::write(&fixture, "(neovm--internal-panic \"load-boom\")\n")
            .expect("write load fixture");
        let path_str = fixture.to_string_lossy().into_owned();
        let var = Value::symbol("jit-fx-load-dynvar");
        let leaf = lower_nullary_leaf(
            &[
                Op::Constant(1), // 5
                Op::VarBind(0),  // has_binds: exit parity unwind sweeps the leak
                Op::Constant(2), // 'load
                Op::Constant(3), // absolute fixture path
                Op::Call(1),
                Op::Unbind(1),
                Op::Return,
            ],
            &[
                var,
                Value::make_int(5),
                Value::symbol("load"),
                Value::string(&path_str),
            ],
        )
        .expect("load call body compiles");
        let spec0 = ev.specpdl.len();
        // GNU signals "Recursive load" once the same file is in flight five
        // times; five leaked entries would previously get there. Every round
        // must instead report the contained panic with clean state.
        for round in 0..5 {
            assert_eq!(leaf.call(ctx_ptr, &[]), NativeRun::Signal, "round {round}");
            let flow = take_pending_flow().expect("panic flow stashed");
            let Flow::Signal(sig) = flow else {
                panic!("round {round}: expected Signal, got {flow:?}");
            };
            let msg = sig.data[0].as_str_owned().expect("string payload");
            assert!(
                msg.contains("load-boom") && !msg.contains("Recursive load"),
                "round {round}: unexpected message: {msg}"
            );
            assert!(
                ev.loads_in_progress.is_empty(),
                "round {round}: loads_in_progress leaked"
            );
            assert_eq!(
                ev.obarray
                    .symbol_value("load-in-progress")
                    .cloned()
                    .unwrap_or(Value::NIL),
                Value::NIL,
                "round {round}: load-in-progress wedged"
            );
            assert_eq!(ev.specpdl.len(), spec0, "round {round}: specpdl swept");
        }
        // And a healthy load of a well-formed file still works afterwards.
        let ok_file = dir.path().join("jit-fx-ok-load.el");
        std::fs::write(&ok_file, "(setq jit-fx-load-ok t)\n").expect("write ok fixture");
        ev.eval_str(&format!("(load {:?} nil t)", ok_file.to_string_lossy()))
            .expect("normal load succeeds after repeated containment");
        assert!(ev.loads_in_progress.is_empty());
    }
}
