//! Cold precise-deopt consumers shared by the two native entry seams.
//! Threading: the payload and Context belong to the running mutator. There
//! is no safepoint between spill readback and installing all frame stacks.

use super::*;
use crate::emacs_core::bytecode::vm::{ChainFrame, InlinedChainFrame};
use crate::emacs_core::jit::reopt::{DeoptEvent, LeafOrigin};

/// Classify a chain guard against its innermost source while retiring the
/// physical leaf, then transfer every frame's state into Tier-0. Mapping
/// chains complete their eager builtin activation before the caller resumes.
#[cold]
#[inline(never)]
pub(crate) fn resume_deopt(
    ctx: &mut Context,
    func: &ByteCodeFunction,
    func_value: Value,
    leaf: &CompiledLeaf,
    resume: DeoptResume,
) -> Result<Value, Flow> {
    let DeoptResume {
        pc,
        stack,
        handlers,
        binds,
        spec_base,
        cond_base,
        cause,
        chain: _,
        inlined,
    } = resume;
    let (source, guard_pc, guard_stack) = match inlined.as_ref().and_then(|r| r.frames.last()) {
        Some(frame) => (
            frame
                .function
                .get_bytecode_data()
                .expect("validated chain callee"),
            frame.pc,
            frame.stack.as_slice(),
        ),
        None => (func, pc, stack.as_slice()),
    };
    // This hook allocates neither Lisp objects nor reaches a safepoint.
    let event = DeoptEvent::Precise {
        pc: guard_pc,
        stack: guard_stack,
        cause,
    };
    if let Some(inlined) = inlined.as_ref() {
        super::super::reopt::note_deopt_chain(
            ctx as *mut Context,
            func,
            source,
            leaf,
            LeafOrigin::Entry,
            inlined.guard_site_pc,
            event,
        );
    } else {
        super::super::reopt::note_deopt(
            ctx as *mut Context,
            source,
            leaf,
            LeafOrigin::Entry,
            event,
        );
    }
    let Some(inlined) = inlined else {
        return Vm::from_context(ctx).run_resumed_frame(
            func, func_value, pc, &stack, handlers, &binds, spec_base, cond_base,
        );
    };
    if let Some(mapping) = inlined.hof.as_ref() {
        // Cold reconstruction can box or rebuild physical values; keep that
        // exact readback alive, in addition to Start's original parent roots,
        // before a callback's entry protocol can poll or enter the debugger.
        for &value in &stack {
            ctx.push_vm_frame_root(value);
        }
        let callback = &inlined.frames[0];
        return match super::hof_runtime::resume_mapping(ctx, mapping, callback, cond_base) {
            Ok(value) => {
                let mut stack = stack;
                stack.truncate(stack.len() - 3);
                stack.push(value);
                Vm::from_context(ctx).run_resumed_frame(
                    func,
                    func_value,
                    pc + 1,
                    &stack,
                    handlers,
                    &binds,
                    spec_base,
                    cond_base,
                )
            }
            Err(flow) => {
                // Metadata rejects physical handlers, so no local handler can
                // intercept this flow. Match the ordinary bytecode teardown:
                // drop condition entries before unwinding native bindings.
                ctx.truncate_condition_stack(cond_base);
                ctx.unbind_to_with_result(spec_base, Err(flow))
            }
        };
    }
    let mut vm = Vm::from_context(ctx);
    let frames: Vec<InlinedChainFrame<'_>> = inlined
        .frames
        .iter()
        .map(|frame| InlinedChainFrame {
            frame: ChainFrame {
                function: frame.function,
                pc: frame.pc,
                stack: &frame.stack,
                handlers: 0,
                binds: &frame.binds,
            },
            link: frame.link,
            backtrace: frame.backtrace,
        })
        .collect();
    vm.run_resumed_chain(
        func,
        ChainFrame {
            function: func_value,
            pc,
            stack: &stack,
            handlers,
            binds: &binds,
        },
        &frames,
        spec_base,
        cond_base,
    )
}
