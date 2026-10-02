//! Admission checks for wholly observation-free constant-inline callers.
//!
//! Failed admission resumes the physical bytecode entry/header snapshot.
//! It never raises an inline error before a loop decides to call its callee.
//! Successful polls repeat admission, so serviced Lisp effects cannot bypass
//! depth/debugger checks. The ordinary hot loop carries no protocol flag.
//! Threading: all handles and policy are compilation-local. Emitted checks
//! read the activation's mutator-owned Context; no shared Lisp cache is added.

use cranelift_codegen::ir::Value as ClifValue;
use cranelift_frontend::FunctionBuilder;

use super::inline_frames::Frames;
use super::lowering::{self, PendingDeopt, RtCtx, SlotRep};

/// Optional baseline edge state, never a runtime object or native ABI field.
pub(super) struct PollAdmission<'a> {
    pub(super) pc: usize,
    pub(super) frames: &'a Frames,
    pub(super) pending: &'a mut Vec<PendingDeopt>,
}

pub(super) fn poll_admission<'a>(
    rt: &RtCtx,
    frames: &'a Frames,
    pc: usize,
    pending: &'a mut Vec<PendingDeopt>,
) -> Option<PollAdmission<'a>> {
    rt.inline_entry_cache
        .as_ref()
        .is_some_and(|cache| cache.physical_protocol_admitted())
        .then_some(PollAdmission {
            pc,
            frames,
            pending,
        })
}

/// `values` are the unchanged, tagged physical entry/header operands.
pub(super) fn emit(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    frames: &Frames,
    pc: usize,
    values: &[ClifValue],
    pending: &mut Vec<PendingDeopt>,
) {
    // A physical admission failure has no live virtual frame. In particular,
    // a final-iteration poll may resume a header that exits without a call.
    lowering::set_active_region(None);
    let reps = vec![SlotRep::Tagged; values.len()];
    frames.entry_protocol(fb, pc, rt, values, &reps, pending, 0);
}
