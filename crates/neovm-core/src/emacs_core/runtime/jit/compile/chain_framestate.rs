//! Shared frame-state planner for the baseline fuser and future opt lowering.
//!
//! It composes nested frame snapshots into the shared `DeoptChain` format
//! and a single representation-aware spill list. The existing cold spill
//! emitter can then retag/box/rebuild aliases over the entire list once.
//! Current constant-callee regions continue replaying the call, so this
//! stage has no production chain emitter. Threading: only compile-local
//! CLIF values are borrowed; output metadata contains no Lisp pointers.

use cranelift_codegen::ir::Value as ClifValue;

use super::CompileError;
use super::lowering::SlotRep;
use crate::emacs_core::jit::inline::FusedBody;
use crate::emacs_core::jit::vframe::{
    BtState, DeoptChain, Link, RelocIdx, SpillRange, VFrameKind, VFrameMeta,
};

/// State owned by the physical activation. Immutable compiler input;
/// counts refer to this mutator's eventual native activation.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PhysicalFrameState {
    pub(crate) binds: u16,
    pub(crate) handlers: u16,
}

/// An InlineEnter snapshot, captured before the callee can change its
/// parameters. All stacks are in the physical fused stack's coordinates.
/// Threading: this is compiler-local SSA data, never runtime Lisp state.
pub(crate) struct RegionFrameState {
    pub(crate) region: usize,
    pub(crate) function: RelocIdx,
    pub(crate) link: Link,
    pub(crate) bt: BtState,
    pub(crate) binds: u16,
    pub(crate) handlers: u16,
    pub(crate) pre_call: Vec<(ClifValue, SlotRep)>,
}

/// Shared runtime metadata plus the complete cold spill's SSA slots.
/// Repeated values/representations remain identical across frame slices,
/// allowing the existing emitter to box/rebuild each alias once.
/// Threading: this compiler-local result contains SSA handles and immutable,
/// pointer-free metadata; it is never a shared cache of mutator Lisp values.
pub(crate) struct PlannedChain {
    pub(crate) chain: DeoptChain,
    pub(crate) spill: Vec<(ClifValue, SlotRep)>,
}

/// Compose physical-to-innermost frame states at one fused pc. The
/// baseline and future opt lowering use this same representation. V1
/// refuses active handlers; it never silently drops an outer handler.
pub(crate) fn chain_framestate(
    fused: &FusedBody,
    pc: usize,
    model_stack: &[ClifValue],
    reps: &[SlotRep],
    physical: PhysicalFrameState,
    snapshots: &[RegionFrameState],
) -> Result<PlannedChain, CompileError> {
    let bad = || CompileError::UnsupportedOp("inline-chain-state");
    if model_stack.len() != reps.len() || physical.handlers > 0 {
        return Err(bad());
    }
    let side = fused.v2.as_ref().ok_or_else(bad)?;
    let mut ancestors = Vec::new();
    let mut at = *fused.region_of.get(pc).ok_or_else(bad)?;
    while let Some(region_id) = at {
        if ancestors.contains(&region_id) {
            return Err(bad());
        }
        let region = fused.regions.get(region_id).ok_or_else(bad)?;
        if pc < region.start || pc >= region.end {
            return Err(bad());
        }
        ancestors.push(region_id);
        at = region.parent;
    }
    ancestors.reverse();
    let states: Vec<_> = ancestors
        .iter()
        .map(|&region| {
            let state = snapshots
                .iter()
                .find(|state| state.region == region)
                .ok_or_else(bad)?;
            let meta = &fused.regions[region];
            if state.handlers > 0
                || state.bt == BtState::Physical
                || (state.bt == BtState::Virtual && state.binds > 0)
                || state.pre_call.len() != meta.frame_base + meta.nargs
            {
                return Err(bad());
            }
            Ok(state)
        })
        .collect::<Result<_, _>>()?;
    let mut spill = Vec::new();
    let mut frames = Vec::new();
    let mut append = |kind,
                      pc: usize,
                      values: &[(ClifValue, SlotRep)],
                      binds,
                      handlers,
                      bt|
     -> Result<(), CompileError> {
        let stack = SpillRange {
            start: u32::try_from(spill.len()).map_err(|_| bad())?,
            len: u32::try_from(values.len()).map_err(|_| bad())?,
        };
        frames.push(VFrameMeta {
            kind,
            pc: u32::try_from(pc).map_err(|_| bad())?,
            stack,
            binds,
            handlers,
            bt,
        });
        spill.extend_from_slice(values);
        Ok(())
    };
    if let Some(&first) = ancestors.first() {
        append(
            VFrameKind::PhysicalBytecode,
            fused.regions[first].call_site_pc,
            &states[0].pre_call,
            physical.binds,
            physical.handlers,
            BtState::Physical,
        )?;
        let mut virtual_seen = false;
        for (index, &region_id) in ancestors.iter().enumerate() {
            let region = &fused.regions[region_id];
            let state = states[index];
            if state.bt == BtState::Virtual {
                virtual_seen = true;
            } else if virtual_seen {
                return Err(bad());
            }
            let (resume_pc, stack) = if let Some(&next) = ancestors.get(index + 1) {
                (
                    fused.regions[next].call_site_pc,
                    states[index + 1]
                        .pre_call
                        .get(region.frame_base..)
                        .ok_or_else(bad)?
                        .to_vec(),
                )
            } else {
                (
                    *side.callee_pc_of_fused.get(pc).ok_or_else(bad)? as usize,
                    model_stack
                        .get(region.frame_base..)
                        .ok_or_else(bad)?
                        .iter()
                        .copied()
                        .zip(reps[region.frame_base..].iter().copied())
                        .collect(),
                )
            };
            append(
                VFrameKind::Bytecode {
                    func: state.function,
                    link: state.link,
                },
                resume_pc,
                &stack,
                state.binds,
                state.handlers,
                state.bt,
            )?;
        }
    } else {
        let stack: Vec<_> = model_stack
            .iter()
            .copied()
            .zip(reps.iter().copied())
            .collect();
        append(
            VFrameKind::PhysicalBytecode,
            fused.caller_pc(pc).ok_or_else(bad)?,
            &stack,
            physical.binds,
            physical.handlers,
            BtState::Physical,
        )?;
    }
    Ok(PlannedChain {
        chain: DeoptChain {
            frames: frames.into_boxed_slice(),
        },
        spill,
    })
}

#[cfg(test)]
#[path = "tests/chain_framestate.rs"]
mod tests;
