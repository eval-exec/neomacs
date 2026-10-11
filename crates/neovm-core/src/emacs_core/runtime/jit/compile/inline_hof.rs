//! List-only mapc/mapcar emission with an observation-free virtual callback.
//! Threading: this is one compilation's SSA state. The emitted activation
//! owns its Context-local roots and backtrace entry; no mutator state is
//! cached or published by this module.

use super::inline_frames::{Frames, HofChainState};
use super::lowering::{self, RegionDeopt};
use super::*;
use crate::emacs_core::jit::inline::{HofKind, HofSite};
use cranelift_codegen::ir::StackSlotKind;

#[path = "inline_hof/heap_policy.rs"]
mod heap_policy;
pub(crate) use heap_policy::hoist_callback_heap_ptr;

#[path = "inline_hof/captures.rs"]
mod captures;

#[cfg(test)]
#[path = "../tests/inline_hof_headers_test.rs"]
mod tests;

/// Emit one admitted map call, including its internal callback CFG. All
/// exits before Start resume the caller's original Call. Later guards spill
/// the caller, mapping state and the callback's exact next instruction.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit(
    frames: &Frames,
    fb: &mut FunctionBuilder,
    pc: usize,
    site: HofSite,
    rt: &RtCtx,
    binds: usize,
    stack: &mut Vec<ClifValue>,
    reps: &mut Vec<SlotRep>,
    pending: &mut Vec<PendingDeopt>,
    signal_exit: &mut Option<Block>,
    reloc_base: Option<ClifValue>,
    reloc_index: &HashMap<usize, u32>,
) -> Result<(), CompileError> {
    use crate::emacs_core::jit::reopt::DeoptCause;
    let bad = || CompileError::UnsupportedOp("inline-hof-state");
    if stack.len() < 3 {
        return Err(bad());
    }
    lowering::set_active_region(None);
    lowering::materialize_model_stack(fb, Some(rt), stack, reps);
    let parent = stack.clone();
    let parent_reps = reps.clone();
    let count = parent.len();
    let map = parent[count - 3];
    let function = parent[count - 2];
    let sequence = parent[count - 1];
    let flags = MemFlagsData::trusted();
    let ctx = fb.use_var(rt.vmctx_var);
    let identity = deopt_site(fb, pc, 0, stack, reps, pending);
    frames.mark(pending, DeoptCause::InlineIdentity);
    let sym = crate::emacs_core::intern::intern(match site.kind {
        HofKind::Mapc => "mapc",
        HofKind::Mapcar => "mapcar",
    });
    let designator = fb
        .ins()
        .iconst(types::I64, Value::from_sym_id(sym).bits() as i64);
    let same = fb.ins().icmp(IntCC::Equal, map, designator);
    emit_guard(fb, identity, same);
    // Read the cell from this mutator's obarray. Obarray publication and
    // symbol mutation use the existing runtime's synchronization protocol;
    // this introduces no new shared cache or publication mechanism.
    use super::jit_layout::{
        CONTEXT_OBARRAY_OFFSET, LISP_SYMBOL_FUNCTION_OFFSET, LISP_SYMBOL_SIZE, OBARRAY_CHUNK_BITS,
        OBARRAY_CHUNK_SLOTS, OBARRAY_JIT_LEN_OFFSET, OBARRAY_JIT_SPINE_OFFSET,
    };
    let len = fb.ins().load(
        types::I64,
        flags,
        ctx,
        (CONTEXT_OBARRAY_OFFSET + OBARRAY_JIT_LEN_OFFSET) as i32,
    );
    let exists = fb
        .ins()
        .icmp_imm_u(IntCC::UnsignedGreaterThan, len, i64::from(sym.0));
    emit_guard(fb, identity, exists);
    let spine = fb.ins().load(
        types::I64,
        flags,
        ctx,
        (CONTEXT_OBARRAY_OFFSET + OBARRAY_JIT_SPINE_OFFSET) as i32,
    );
    let chunk = fb.ins().load(
        types::I64,
        flags,
        spine,
        ((sym.0 as usize >> OBARRAY_CHUNK_BITS) * 8) as i32,
    );
    let actual = fb.ins().load(
        types::I64,
        flags,
        chunk,
        (((sym.0 as usize & (OBARRAY_CHUNK_SLOTS - 1)) * LISP_SYMBOL_SIZE)
            + LISP_SYMBOL_FUNCTION_OFFSET) as i32,
    );
    let expected = fb
        .ins()
        .iconst(types::I64, Value::subr_from_sym_id(sym).bits() as i64);
    let same = fb.ins().icmp(IntCC::Equal, actual, expected);
    emit_guard(fb, identity, same);
    if site.closure {
        let code = site.callback.get_bytecode_data().ok_or_else(bad)?;
        let word = jit_layout::runtime_identity_word(&code.jit_runtime());
        let miss = source_slots::emit_source_guard(fb, function, word as u64);
        let hit = fb.current_block().expect("source hit");
        fb.switch_to_block(miss);
        fb.seal_block(miss);
        fb.ins().jump(identity, &[]);
        fb.switch_to_block(hit);
        source_slots::emit_closure_prefix_guard(fb, &code.jit_runtime(), site.prefix, identity);
    } else {
        let expected = fb.ins().iconst(types::I64, site.callback.bits() as i64);
        let same = fb.ins().icmp(IntCC::Equal, function, expected);
        emit_guard(fb, identity, same);
    }
    frames.entry_protocol(fb, pc, rt, stack, reps, pending, 0);
    let length_ref = rt.refs.get(fb.func, Shim::HofLength);
    let call = fb.ins().call(length_ref, &[sequence]);
    let length = fb.inst_results(call)[0];
    let proper = fb
        .ins()
        .icmp_imm_s(IntCC::SignedGreaterThanOrEqual, length, 0);
    let bad_list = deopt_site(fb, pc, 0, stack, reps, pending);
    // A runtime preflight refusal (including an active read capture) is a
    // semantic fallback, not evidence against the callback speculation.
    frames.mark(pending, DeoptCause::ColdFlagged);
    emit_guard(fb, bad_list, proper);
    let parent_slot = fb.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        (count * 8) as u32,
        3,
    ));
    for (index, &value) in parent.iter().enumerate() {
        fb.ins()
            .stack_store(types::I64, value, parent_slot, (index * 8) as i32);
    }
    let parent_ptr = fb.ins().stack_addr(types::I64, parent_slot, 0);
    let count_value = fb.ins().iconst(types::I64, count as i64);
    let sink_base = fb.ins().iconst(types::I64, (count + 1) as i64);
    let kind = fb.ins().iconst(types::I64, site.kind as i64);
    let start_ref = rt.refs.get(fb.func, Shim::HofStart);
    let call = fb.ins().call(
        start_ref,
        &[ctx, map, parent_ptr, count_value, length, kind],
    );
    let bt = fb.inst_results(call)[0];
    let started = fb.ins().icmp_imm_s(IntCC::SignedGreaterThanOrEqual, bt, 0);
    let signal = signal_exit.unwrap_or_else(|| {
        let block = fb.create_block();
        *signal_exit = Some(block);
        block
    });
    let initialized = fb.create_block();
    fb.ins().brif(started, initialized, &[], signal, &[]);
    fb.switch_to_block(initialized);
    fb.seal_block(initialized);
    let tail_var = fb.declare_var(types::I64);
    let index_var = fb.declare_var(types::I64);
    let poll_var = fb.declare_var(types::I8);
    // Admitted callback bodies cannot run Lisp or service GC. The complete
    // entry protocol is invariant until a slow physical poll; the first real
    // callback and the first one after such a poll retain their exact guards.
    // Separate checked and steady headers avoid carrying a cache flag through
    // every callback. Forced-deopt emission uses the checked header throughout.
    fb.def_var(tail_var, sequence);
    let zero = fb.ins().iconst(types::I64, 0);
    fb.def_var(index_var, zero);
    let one = fb.ins().iconst(types::I8, 1);
    fb.def_var(poll_var, one);
    let checked_header = fb.create_block();
    let steady_header = (!super::jit_force_deopt()).then(|| fb.create_block());
    let callback = fb.create_block();
    for _ in 0..3 {
        fb.append_block_param(callback, types::I64);
    }
    let after = fb.create_block();
    let done = fb.create_block();
    let result = fb.declare_var(types::I64);
    let consts_var = (site.prefix > 0).then(|| fb.declare_var(types::I64));
    let captures = captures::Captures::build(fb, site)?;
    let tagged_len = retag_fixnum(fb, length);
    let tagged_bt = retag_fixnum(fb, bt);
    let tagged_base = retag_fixnum(fb, sink_base);
    let physical = RegionDeopt {
        call_site_pc: site.call_site_pc,
        stack: parent.clone(),
        reps: parent_reps.clone(),
        chain: None,
    };
    fb.ins().jump(checked_header, &[]);
    fb.switch_to_block(checked_header);
    let (tail, index, item) = emit_candidate(fb, tail_var, index_var, length, done);
    let tagged_index = retag_fixnum(fb, index);
    frames.enter_hof(
        physical.clone(),
        HofChainState {
            kind: site.kind,
            call_pc: site.call_site_pc,
            state: vec![
                function,
                sequence,
                tagged_len,
                tail,
                tagged_index,
                tagged_bt,
                tagged_base,
                item,
            ],
            callback_entered: false,
        },
        binds,
    );
    let callback_stack = vec![item];
    let callback_reps = vec![SlotRep::Tagged];
    frames.entry_protocol(fb, 0, rt, &callback_stack, &callback_reps, pending, 0);
    if let Some(var) = consts_var {
        // The executing closure owns its constant storage, which is immutable
        // after publication (gc_thread's bytecode claim uses the same proof).
        // Admitted native ops cannot enter Lisp or ensure_owned that storage;
        // concurrent GC only reads it. The activation's map roots retain the
        // closure. Conservatively end this SSA cache at every serviced poll,
        // where Lisp may run, and reload on the next real callback. Capture
        // values may get a bounded identity/tag witness; mutable cons cars
        // remain fresh at every body use;
        // shared-source Runtime prefix atomics and guards are unchanged.
        let object = lowering::band_imm_p(fb, function, !(TAG_MASK as i64));
        let (offset, _) = jit_layout::bytecode_constants_offsets().ok_or_else(bad)?;
        let base = fb.ins().load(types::I64, flags, object, offset as i32);
        fb.def_var(var, base);
        captures.enter(frames, fb, base, &callback_stack, &callback_reps, pending)?;
    }
    fb.ins()
        .jump(callback, &[tail.into(), index.into(), item.into()]);
    if let Some(header) = steady_header {
        fb.switch_to_block(header);
        let (tail, index, item) = emit_candidate(fb, tail_var, index_var, length, done);
        fb.ins()
            .jump(callback, &[tail.into(), index.into(), item.into()]);
    }
    fb.switch_to_block(callback);
    fb.seal_block(callback);
    let tail = fb.block_params(callback)[0];
    let index = fb.block_params(callback)[1];
    let item = fb.block_params(callback)[2];
    let ptr = lowering::iadd_imm_p(fb, tail, -(crate::tagged::value::TAG_CONS as i64));
    let tagged_index = retag_fixnum(fb, index);
    frames.enter_hof(
        physical,
        HofChainState {
            kind: site.kind,
            call_pc: site.call_site_pc,
            state: vec![
                function,
                sequence,
                tagged_len,
                tail,
                tagged_index,
                tagged_bt,
                tagged_base,
                item,
            ],
            callback_entered: true,
        },
        binds,
    );
    let consts_base = consts_var.map(|var| fb.use_var(var));
    let mut callback_signal = None;
    emit_callback(
        fb,
        site,
        rt,
        item,
        consts_base,
        &captures,
        pending,
        &mut callback_signal,
        reloc_base,
        reloc_index,
        result,
        after,
    )?;
    if let Some(exit) = callback_signal {
        fb.switch_to_block(exit);
        fb.seal_block(exit);
        fb.set_cold_block(exit);
        let abort_ref = rt.refs.get(fb.func, Shim::HofAbort);
        fb.ins().call(abort_ref, &[ctx, bt]);
        fb.ins().jump(signal, &[]);
    }
    fb.switch_to_block(after);
    fb.seal_block(after);
    let value = fb.use_var(result);
    if site.kind == HofKind::Mapcar {
        let slot = fb.ins().iadd(sink_base, index);
        let store_ref = rt.refs.get(fb.func, Shim::HofStore);
        fb.ins().call(store_ref, &[ctx, slot, value]);
    }
    lowering::set_active_region(None);
    // GNU mapcar1 reads the cdr after the callback (a setcdr in the body
    // must affect the next cursor).
    let next_tail = fb
        .ins()
        .load(types::I64, flags, ptr, jit_layout::CONS_CDR_OFFSET as i32);
    let next_index = lowering::iadd_imm_p(fb, index, 1);
    fb.def_var(tail_var, next_tail);
    fb.def_var(index_var, next_index);
    let more = fb.ins().icmp(IntCC::UnsignedLessThan, next_index, length);
    let edge = fb.create_block();
    fb.ins().brif(more, edge, &[], done, &[]);
    fb.switch_to_block(edge);
    fb.seal_block(edge);
    let old = fb.use_var(poll_var);
    let bump = fb.ins().iadd_imm_u(old, 1);
    fb.def_var(poll_var, bump);
    let wrapped = fb.ins().icmp_imm_u(IntCC::Equal, bump, 0);
    let poll = fb.create_block();
    fb.ins().brif(
        wrapped,
        poll,
        &[],
        steady_header.unwrap_or(checked_header),
        &[],
    );
    fb.switch_to_block(poll);
    fb.seal_block(poll);
    fb.set_cold_block(poll);
    let cursor_slot = fb.ins().iconst(types::I64, count as i64);
    let cursor_ref = rt.refs.get(fb.func, Shim::HofCursor);
    fb.ins().call(cursor_ref, &[ctx, cursor_slot, next_tail]);
    let backedge = rt.refs.get(fb.func, Shim::Backedge);
    let call = fb.ins().call(backedge, &[ctx]);
    let status = fb.inst_results(call)[0];
    let ok = lowering::icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
    let polled = fb.create_block();
    let aborted = fb.create_block();
    fb.ins().brif(ok, polled, &[], aborted, &[]);
    fb.switch_to_block(aborted);
    fb.seal_block(aborted);
    let abort_ref = rt.refs.get(fb.func, Shim::HofAbort);
    fb.ins().call(abort_ref, &[ctx, bt]);
    fb.ins().jump(signal, &[]);
    fb.switch_to_block(polled);
    fb.seal_block(polled);
    let one = fb.ins().iconst(types::I8, 1);
    fb.def_var(poll_var, one);
    fb.ins().jump(checked_header, &[]);
    fb.switch_to_block(done);
    fb.seal_block(done);
    fb.seal_block(checked_header);
    if let Some(header) = steady_header {
        fb.seal_block(header);
    }
    let mapped = fb.use_var(index_var);
    let finish_ref = rt.refs.get(fb.func, Shim::HofFinish);
    let call = fb
        .ins()
        .call(finish_ref, &[ctx, sequence, mapped, bt, sink_base, kind]);
    let value = fb.inst_results(call)[0];
    let ok = lowering::icmp_imm_p(fb, IntCC::NotEqual, value, VALUE_SHIM_SIGNAL);
    let finished = fb.create_block();
    fb.ins().brif(ok, finished, &[], signal, &[]);
    fb.switch_to_block(finished);
    fb.seal_block(finished);
    stack.truncate(count - 3);
    reps.truncate(count - 3);
    stack.push(value);
    reps.push(SlotRep::Tagged);
    Ok(())
}

/// Inspect the next GNU mapcar1 iteration before any callback-entry guard.
/// Both headers preserve the empty/shortened-tail exit and original car.
fn emit_candidate(
    fb: &mut FunctionBuilder,
    tail_var: Variable,
    index_var: Variable,
    length: ClifValue,
    done: Block,
) -> (ClifValue, ClifValue, ClifValue) {
    let tail = fb.use_var(tail_var);
    let index = fb.use_var(index_var);
    let in_range = fb.ins().icmp(IntCC::UnsignedLessThan, index, length);
    let tag = lowering::band_imm_p(fb, tail, TAG_MASK as i64);
    let cons = lowering::icmp_imm_p(fb, IntCC::Equal, tag, crate::tagged::value::TAG_CONS as i64);
    let more = fb.ins().band(in_range, cons);
    let candidate = fb.create_block();
    fb.ins().brif(more, candidate, &[], done, &[]);
    fb.switch_to_block(candidate);
    fb.seal_block(candidate);
    let ptr = lowering::iadd_imm_p(fb, tail, -(crate::tagged::value::TAG_CONS as i64));
    let item = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        ptr,
        jit_layout::CONS_CAR_OFFSET as i32,
    );
    (tail, index, item)
}

/// Lower the already-admitted callback with the ordinary arithmetic and heap
/// guards. Its local stack is independent of the parent's SSA snapshots.
#[allow(clippy::too_many_arguments)]
fn emit_callback(
    fb: &mut FunctionBuilder,
    site: HofSite,
    rt: &RtCtx,
    item: ClifValue,
    consts_base: Option<ClifValue>,
    captures: &captures::Captures,
    pending: &mut Vec<PendingDeopt>,
    signal_exit: &mut Option<Block>,
    reloc_base: Option<ClifValue>,
    reloc_index: &HashMap<usize, u32>,
    result: Variable,
    after: Block,
) -> Result<(), CompileError> {
    let code = site
        .callback
        .get_bytecode_data()
        .ok_or(CompileError::BadOperand)?;
    let ops = code.executable_ops();
    let constants = mask_dynamic_prefix(&code.constants, site.prefix);
    let cfg = analyze_cfg(ops, &constants, code.executable_gnu_byte_offset_map(), 1)?;
    let feedback: Vec<_> = (0..ops.len())
        .map(|pc| code.jit_runtime().numeric_feedback(pc))
        .collect();
    let _feedback = publish_numeric_feedback_vec(feedback);
    let blocks: HashMap<_, _> = cfg
        .leaders
        .iter()
        .map(|&leader| (leader, fb.create_block()))
        .collect();
    let vars: Vec<_> = (0..cfg.max_depth)
        .map(|_| fb.declare_var(types::I64))
        .collect();
    fb.def_var(vars[0], item);
    fb.ins().jump(blocks[&0], &[]);
    for (leader_index, &leader) in cfg.leaders.iter().enumerate() {
        fb.switch_to_block(blocks[&leader]);
        let Some(&depth) = cfg.entry_depth.get(&leader) else {
            fb.ins()
                .trap(cranelift_codegen::ir::TrapCode::unwrap_user(1));
            continue;
        };
        let mut stack: Vec<_> = vars[..depth].iter().map(|&var| fb.use_var(var)).collect();
        let mut reps = vec![SlotRep::Tagged; depth];
        let end = cfg
            .leaders
            .get(leader_index + 1)
            .copied()
            .unwrap_or(ops.len());
        let mut terminated = false;
        for (off, op) in ops[leader..end].iter().enumerate() {
            let pc = leader + off;
            match op {
                Op::Constant(_) if captures.constant(pc).is_some() => {
                    let cell = fb.use_var(captures.constant(pc).expect("matched capture"));
                    stack.push(cell);
                    reps.push(SlotRep::Tagged);
                }
                Op::Car | Op::CarSafe if captures.car(pc) => {
                    lowering::materialize_model_stack(fb, Some(rt), &mut stack, &mut reps);
                    let cell = stack.pop().ok_or(CompileError::StackUnderflow)?;
                    reps.pop();
                    // The guarded identity is the preceding Constant/Dup;
                    // no internal CFG entry can substitute another operand.
                    let ptr =
                        lowering::iadd_imm_p(fb, cell, -(crate::tagged::value::TAG_CONS as i64));
                    let car = fb.ins().load(
                        types::I64,
                        MemFlagsData::trusted(),
                        ptr,
                        jit_layout::CONS_CAR_OFFSET as i32,
                    );
                    stack.push(car);
                    reps.push(SlotRep::Tagged);
                }
                Op::Return => {
                    lowering::materialize_model_stack(fb, Some(rt), &mut stack, &mut reps);
                    fb.def_var(result, stack.pop().ok_or(CompileError::StackUnderflow)?);
                    fb.ins().jump(after, &[]);
                    terminated = true;
                    break;
                }
                Op::Goto(target) => {
                    write_edge_stack_to_vars(
                        fb,
                        Some(rt),
                        &vars,
                        &mut stack,
                        &mut reps,
                        &vec![false; cfg.max_depth],
                    );
                    fb.ins().jump(blocks[&(*target as usize)], &[]);
                    terminated = true;
                    break;
                }
                Op::GotoIfNil(target)
                | Op::GotoIfNotNil(target)
                | Op::GotoIfNilElsePop(target)
                | Op::GotoIfNotNilElsePop(target) => {
                    lowering::materialize_model_stack(fb, Some(rt), &mut stack, &mut reps);
                    let condition = *stack.last().ok_or(CompileError::StackUnderflow)?;
                    let preserve =
                        matches!(op, Op::GotoIfNilElsePop(_) | Op::GotoIfNotNilElsePop(_));
                    if !preserve {
                        stack.pop();
                        reps.pop();
                    }
                    write_edge_stack_to_vars(
                        fb,
                        Some(rt),
                        &vars,
                        &mut stack,
                        &mut reps,
                        &vec![false; cfg.max_depth],
                    );
                    let nil =
                        lowering::icmp_imm_p(fb, IntCC::Equal, condition, Value::NIL.bits() as i64);
                    let (yes, no) = if matches!(op, Op::GotoIfNil(_) | Op::GotoIfNilElsePop(_)) {
                        (blocks[&(*target as usize)], blocks[&(pc + 1)])
                    } else {
                        (blocks[&(pc + 1)], blocks[&(*target as usize)])
                    };
                    fb.ins().brif(nil, yes, &[], no, &[]);
                    terminated = true;
                    break;
                }
                Op::Setcar | Op::Setcdr => {
                    lowering::materialize_model_stack(fb, Some(rt), &mut stack, &mut reps);
                    let deopt = deopt_site(fb, pc, 0, &stack, &reps, pending);
                    let value = stack.pop().ok_or(CompileError::StackUnderflow)?;
                    let cell = stack.pop().ok_or(CompileError::StackUnderflow)?;
                    reps.truncate(stack.len());
                    let output = fb.declare_var(types::I64);
                    let merge = fb.create_block();
                    if captures.store(pc) {
                        heap_inline::emit_inline_cons_store_known_cons(
                            fb, rt, cell, value, false, deopt, output, merge,
                        );
                    } else {
                        heap_inline::emit_inline_cons_store(
                            fb,
                            rt,
                            cell,
                            value,
                            matches!(op, Op::Setcdr),
                            deopt,
                            output,
                            merge,
                        );
                    }
                    fb.switch_to_block(merge);
                    fb.seal_block(merge);
                    stack.push(fb.use_var(output));
                    reps.push(SlotRep::Tagged);
                }
                other => {
                    let mut dispatch = Vec::new();
                    // The constants' reloc indices were appended to the
                    // physical leaf by the front, keyed by value bits.
                    lowering::lower_simple_op(
                        fb,
                        pc,
                        pending,
                        signal_exit,
                        &constants,
                        &mut stack,
                        &mut reps,
                        Some(rt),
                        &[],
                        &mut dispatch,
                        None,
                        other,
                        &HashSet::new(),
                        reloc_base,
                        reloc_index,
                        false,
                        None,
                        None,
                        site.prefix,
                        consts_base,
                    )?;
                    if !dispatch.is_empty() {
                        return Err(CompileError::UnsupportedOp("inline-hof-observer"));
                    }
                }
            }
        }
        if !terminated {
            write_edge_stack_to_vars(
                fb,
                Some(rt),
                &vars,
                &mut stack,
                &mut reps,
                &vec![false; cfg.max_depth],
            );
            fb.ins().jump(blocks[&end], &[]);
        }
    }
    Ok(())
}
