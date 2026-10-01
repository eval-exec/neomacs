//! Cold precise-deopt consumers shared by the two native entry seams.
//! Threading: the payload and Context belong to the running mutator. There
//! is no safepoint between spill readback and installing all frame stacks.

use super::*;
use crate::emacs_core::bytecode::vm::{ChainFrame, InlinedChainFrame};
use crate::emacs_core::jit::reopt::{DeoptEvent, LeafOrigin};

/// Classify a chain guard against its innermost source while retiring the
/// physical leaf, then transfer every frame's state into Tier-0. No producer
/// exists yet; an ordinary deopt keeps the existing single-frame behavior.
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
    let mut vm = Vm::from_context(ctx);
    let Some(inlined) = inlined else {
        return vm.run_resumed_frame(
            func, func_value, pc, &stack, handlers, &binds, spec_base, cond_base,
        );
    };
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
