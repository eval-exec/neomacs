//! List HOF activation shims and the cold mapping completion seam.
//!
//! Threading: every root frame, backtrace index and counter is owned by the
//! running mutator's exclusive Context. Nothing is cached globally or retained
//! by another mutator; the shared HofKind is immutable ABI metadata.

use super::*;
use crate::emacs_core::builtins::{MapCallee, MapSink, mapcar1_eval_from};
use crate::emacs_core::jit::vframe::{HofKind, HofResume, InlinedFrameResume};

/// Bound this pure preflight at one less than GNU's largest list-length
/// quit window. Longer lists use the builtin.
const HOF_LIST_PREFLIGHT_MAX: i64 = 65_535;

/// Decline non-list, dotted, circular and long sequences without signaling
/// or allocating Lisp objects. The ordinary builtin then performs GNU's Flength
/// validation, including its exact error payload, before any callback.
///
/// GNU `src/fns.c:list_length` walks one cdr per node using the teleporting
/// tortoise in `src/lisp.h:FOR_EACH_TAIL_STEP_CYCLEP`; Neo's
/// `proper_list_length_or_signal` follows the same schedule. This bounded
/// preflight needs neither GNU's quit machinery nor its longer-window counter.
/// Threading: reads the caller-rooted sequence under the current mutator's
/// existing cons-access contract, with no Lisp call or safepoint during the
/// walk. It publishes no state; a concurrent collector may read the cells.
#[unsafe(no_mangle)]
pub(crate) extern "C" fn neovm_jit_hof_length(sequence: i64) -> i64 {
    jit_shim_contain!(no_ctx, -1, {
        // The native loop reads list cells directly. Decline before mapping
        // entry whenever this mutator must record reads, so the shared builtin
        // traversal and its callbacks keep the existing capture protocol.
        if crate::tagged::collection_reads::reads_need_observation() {
            return -1;
        }
        let mut tail = Value::from_bits(sequence as usize);
        let mut tortoise = tail;
        let mut span = 2u32;
        let mut remaining = span;
        let mut len = 0i64;
        while tail.is_cons() {
            if len == HOF_LIST_PREFLIGHT_MAX {
                return -1;
            }
            tail = tail.cons_cdr();
            len += 1;
            remaining -= 1;
            if remaining == 0 {
                // Teleport after 2, 6, 14, ... traversed cells, as GNU does.
                // The accepted-length bound limits span to 65,536 and len
                // to 65,535, so neither increment nor doubling can overflow.
                span *= 2;
                remaining = span;
                tortoise = tail;
            } else if tail.bits() == tortoise.bits() {
                // GNU BASE_EQ detects the same cons, never structural Lisp equal.
                return -1;
            }
        }
        if tail.is_nil() { len } else { -1 }
    })
}

/// Begin an eager GNU Bcall mapping activation. Returns its raw backtrace
/// index, or -1 with pending flow. The parent stack and list cursor precede
/// the result slots in one dedicated VM root frame. Only the cold depth-limit
/// signal can run callback Lisp; it keeps those parent roots live.
/// SAFETY: generated code supplies the active mutator and count live tagged
/// parent words, ending in [mapping designator, callback, sequence].
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn neovm_jit_hof_start(
    ctx: *mut u8,
    map: i64,
    parent: *const i64,
    count: i64,
    len: i64,
    kind: i64,
) -> i64 {
    jit_shim_contain!(ctx, -1, {
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let count = usize::try_from(count).expect("HOF parent count");
        let len = usize::try_from(len).expect("HOF validated length");
        let kind = HofKind::from_word(kind).expect("HOF kind");
        assert!(count >= 3);
        let parent = unsafe { core::slice::from_raw_parts(parent, count) };
        ctx.push_vm_root_frame();
        for &word in parent {
            ctx.push_vm_frame_root(Value::from_bits(word as usize));
        }
        let callback = Value::from_bits(parent[count - 2] as usize);
        let sequence = Value::from_bits(parent[count - 1] as usize);
        assert_eq!(ctx.push_vm_frame_root_slot(sequence), count);
        if kind == HofKind::Mapcar {
            assert_eq!(ctx.reserve_vm_frame_root_slots(len), count + 1);
        }
        ctx.depth += 1;
        if ctx.depth > ctx.max_depth {
            let check = Vm::from_context(ctx).bytecode_depth_exceeded();
            if let Err(flow) = check {
                let flow = ctx.finish_lisp_depth_overflow(flow);
                ctx.pop_vm_root_frame();
                stash_pending_flow(flow);
                return -1;
            }
        }
        let bt = ctx.specpdl.len();
        ctx.push_backtrace_frame(Value::from_bits(map as usize), &[callback, sequence]);
        bt as i64
    })
}

/// Store a tagged callback result in an absolute slot of the active HOF
/// root frame. This is GC-free and cannot enter Lisp.
/// SAFETY: generated code supplies the mutator and a reserved result slot.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn neovm_jit_hof_store(ctx: *mut u8, index: i64, value: i64) -> i64 {
    jit_shim_contain!(ctx, VALUE_SHIM_SIGNAL, {
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        ctx.set_vm_frame_root_slot(
            usize::try_from(index).expect("HOF result slot"),
            Value::from_bits(value as usize),
        );
        value
    })
}

/// Keep the current tail rooted even after a callback disconnects it from
/// the original list. SAFETY: SLOT names this activation's cursor slot.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn neovm_jit_hof_cursor(ctx: *mut u8, slot: i64, tail: i64) -> i64 {
    jit_shim_contain!(ctx, VALUE_SHIM_SIGNAL, {
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        ctx.set_vm_frame_root_slot(
            usize::try_from(slot).expect("HOF cursor slot"),
            Value::from_bits(tail as usize),
        );
        Value::NIL.bits() as i64
    })
}

/// Complete the map frame after all callbacks: build the mapcar result from
/// its rooted slots, decrement depth, then run GNU's exit debugger and pop.
#[cold]
#[inline(never)]
pub(crate) fn finish_mapping(
    ctx: &mut Context,
    sequence: Value,
    mapped: usize,
    bt: usize,
    base: usize,
    kind: HofKind,
    outcome: Result<(), Flow>,
) -> Result<Value, Flow> {
    let result = outcome.map(|()| match kind {
        HofKind::Mapc => sequence,
        HofKind::Mapcar => Value::list_from_slice(ctx.vm_frame_root_slots(base, mapped)),
    });
    // An entry quit can precede the callback's own Ffuncall activation;
    // its first signal hook still runs inside the eager mapping call.
    let result = ctx.dispatch_signal_result_if_needed(result);
    ctx.depth -= 1;
    let result = ctx.finish_traced_call(bt, result);
    ctx.pop_vm_root_frame();
    result
}

/// SAFETY: the supplied state belongs to the active HOF frame. A signal is
/// stashed only after that frame and its roots have been completed.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn neovm_jit_hof_finish(
    ctx: *mut u8,
    sequence: i64,
    mapped: i64,
    bt: i64,
    base: i64,
    kind: i64,
) -> i64 {
    jit_shim_contain!(ctx, VALUE_SHIM_SIGNAL, {
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        match finish_mapping(
            ctx,
            Value::from_bits(sequence as usize),
            usize::try_from(mapped).expect("HOF mapped count"),
            usize::try_from(bt).expect("HOF backtrace index"),
            usize::try_from(base).expect("HOF sink base"),
            HofKind::from_word(kind).expect("HOF kind"),
            Ok(()),
        ) {
            Ok(value) => value.bits() as i64,
            Err(flow) => {
                stash_pending_flow(flow);
                VALUE_SHIM_SIGNAL
            }
        }
    })
}

/// Dispatch a native signal with the map backtrace still visible, then
/// unwind its frame and restash the flow. SAFETY: BT identifies this map;
/// contained-panic residue is healed by the existing native leaf boundary.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn neovm_jit_hof_abort(ctx: *mut u8, bt: i64) -> i64 {
    jit_shim_contain!(ctx, VALUE_SHIM_SIGNAL, {
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let flow = take_pending_flow().expect("HOF abort pending flow");
        // A branch poll signals while GNU's mapping call is active. Keep
        // both its backtrace and depth visible to the first signal hook;
        // finish_traced_call then observes the completed signal search.
        let result = ctx.dispatch_signal_result_if_needed(Err(flow));
        ctx.depth -= 1;
        let result =
            ctx.finish_traced_call(usize::try_from(bt).expect("HOF backtrace index"), result);
        ctx.pop_vm_root_frame();
        if let Err(flow) = result {
            stash_pending_flow(flow);
        }
        Value::NIL.bits() as i64
    })
}

/// Finish the suspended callback once and then continue GNU's shared loop.
/// All physical and callback spills are rooted before Tier-0 can poll or GC.
/// The callback entry flag distinguishes a guard BEFORE Ffuncall from a
/// mid-body deopt, so neither a poll nor callback side effects are replayed.
#[cold]
#[inline(never)]
pub(crate) fn resume_mapping(
    ctx: &mut Context,
    state: &HofResume,
    callback: &InlinedFrameResume,
    condition_base: usize,
) -> Result<Value, Flow> {
    for &value in &callback.stack {
        ctx.push_vm_frame_root(value);
    }
    ctx.push_vm_frame_root(callback.function);
    ctx.push_vm_frame_root(state.tail);
    ctx.push_vm_frame_root(state.item);
    let item = state.item;
    let callback_result = if state.callback_entered {
        let bt = ctx.specpdl.len();
        ctx.push_backtrace_frame(state.function, &[item]);
        ctx.depth += 1;
        let code = callback
            .function
            .get_bytecode_data()
            .expect("validated HOF callback");
        let result = Vm::from_context(ctx).run_resumed_frame(
            code,
            callback.function,
            callback.pc,
            &callback.stack,
            0,
            &callback.binds,
            bt + 1,
            condition_base,
        );
        ctx.depth -= 1;
        ctx.finish_traced_call(bt, result)
    } else {
        ctx.apply1(state.function, item)
    };
    let mapped = match callback_result {
        Ok(value) => {
            if state.kind == HofKind::Mapcar {
                ctx.set_vm_frame_root_slot(state.sink_base + state.index, value);
            }
            let next = state.tail.cons_cdr();
            ctx.set_vm_frame_root_slot(state.sink_base - 1, next);
            let callee = MapCallee::resolve(ctx, state.function);
            let sink = match state.kind {
                HofKind::Mapc => MapSink::Discard,
                HofKind::Mapcar => MapSink::RootSlots(state.sink_base),
            };
            mapcar1_eval_from(
                ctx,
                state.len,
                sink,
                state.sequence,
                next,
                state.index + 1,
                |ctx, item| callee.call(ctx, item),
            )
        }
        Err(flow) => Err(flow),
    };
    match mapped {
        Ok(mapped) => finish_mapping(
            ctx,
            state.sequence,
            mapped,
            state.bt,
            state.sink_base,
            state.kind,
            Ok(()),
        ),
        Err(flow) => finish_mapping(
            ctx,
            state.sequence,
            state.index,
            state.bt,
            state.sink_base,
            state.kind,
            Err(flow),
        ),
    }
}

#[cfg(test)]
#[path = "../tests/inline_hof_runtime_test.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/inline_hof_length_test.rs"]
mod hof_length_tests;
