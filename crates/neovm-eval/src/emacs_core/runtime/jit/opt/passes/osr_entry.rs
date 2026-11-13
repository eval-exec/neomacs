//! Check the fixnum loop state of an OSR entry once, at the entry.
//!
//! An OSR leaf enters its loop header with the interpreter's stack slots,
//! whose types the compiler cannot know. Every header phi that also takes
//! those slots is therefore any-typed, and each fixnum use inside the loop
//! guards it again on every iteration -- where the baseline OSR emitter
//! checks the slots once on entry and keeps the loop untagged. This pass
//! makes the same speculation explicit: a header phi whose every other input
//! is a fixnum, and which the loop itself checks for one, gets a fixnum
//! guard on its OSR slot in the entry block. A failing guard deoptimizes
//! before the first iteration, to the header with the slots as they came.
//!
//! Threading: per-compilation IR only; no Lisp heap, mutator or cache state.

pub(crate) use super::super::{
    ir::*,
    mem::{AliasClass, Effects},
    types::TypeSet,
    verify::VerifyError,
};

/// Compiler-owned diagnostics; published with the completed plan only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OsrEntryStats {
    /// OSR slots guarded as fixnums at the entry.
    pub(crate) speculated: usize,
    /// Block parameters every incoming value of which is now a fixnum,
    /// declared as such so the loop's own guards on them fold.
    pub(crate) narrowed: usize,
}

/// The caller invokes this after integer lifting and before folding; a
/// function without an OSR entry, or without fixnum loop state to guard,
/// is returned untouched and unverified (its producer verified it). A
/// guarded candidate is verified before it replaces the function.
pub(crate) fn run(func: &mut Func) -> Result<OsrEntryStats, VerifyError> {
    let Some(osr) = func.osr.clone() else {
        return Ok(OsrEntryStats::default());
    };
    let entry = func.entry;
    let Term::Jump(edge) = &func.blocks[entry.index()].term else {
        return Ok(OsrEntryStats::default());
    };
    let header = edge.target;
    let entry_args = edge.args.clone();
    let types = loop_state_types(func, entry);
    let checked = fixnum_checked(func);
    let Some(frame) = entry_frame(func, &osr, entry) else {
        return Ok(OsrEntryStats::default());
    };
    let mut candidate = func.clone();
    let mut stats = OsrEntryStats::default();
    for (index, &param) in func.blocks[header.index()].params.iter().enumerate() {
        let ty = types[param.index()];
        let slot = entry_args[index];
        let slot_ty = func.values[slot.index()].ty;
        if ty.is_bottom()
            || !ty.is_subset(TypeSet::FIXNUM)
            || (!slot_ty.is_bottom() && slot_ty.is_subset(TypeSet::FIXNUM))
            || !checked[param.index()]
        {
            continue;
        }
        let guarded = guard(&mut candidate, entry, slot, frame, osr.entry_pc);
        let Term::Jump(edge) = &mut candidate.blocks[entry.index()].term else {
            unreachable!("the entry still jumps to its header");
        };
        edge.args[index] = guarded;
        stats.speculated += 1;
    }
    if stats.speculated == 0 {
        return Ok(stats);
    }
    stats.narrowed = narrow_fixnum_params(&mut candidate, &types);
    candidate.verify()?;
    *func = candidate;
    Ok(stats)
}

/// The types each block parameter takes from every edge but the entry's:
/// the loop's own state, assuming the entry brings what the loop makes.
/// Kinds only (ranges widened away), so the ascent is finite.
fn loop_state_types(func: &Func, entry: Block) -> Vec<TypeSet> {
    let mut types = vec![TypeSet::BOTTOM; func.values.len()];
    let type_of = |types: &[TypeSet], value: Value| -> TypeSet {
        let Some(value) = func.resolve(value) else {
            return TypeSet::TOP;
        };
        match func.values[value.index()].def {
            ValueDef::Param { .. } => types[value.index()],
            _ => func.values[value.index()].ty,
        }
    };
    for _ in 0..=func.blocks.len() {
        let mut changed = false;
        for (index, block) in func.blocks.iter().enumerate() {
            if Block(index as u32) == entry {
                continue;
            }
            for edge in block.term.edges() {
                let params = &func.blocks[edge.target.index()].params;
                for (&param, &arg) in params.iter().zip(&edge.args) {
                    let previous = types[param.index()];
                    let next = previous.widen(previous.join(type_of(&types, arg)));
                    if next != previous {
                        types[param.index()] = next;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            return types;
        }
    }
    vec![TypeSet::TOP; func.values.len()]
}

/// Declare as fixnums the block parameters whose every incoming value is
/// one, now that the entry's slots are guarded: the fold pass turns a guard
/// into a refinement only for a value declared a fixnum. A greatest fixed
/// point: start from every parameter the loop types as a fixnum and drop
/// those with an incoming value that is not.
fn narrow_fixnum_params(func: &mut Func, types: &[TypeSet]) -> usize {
    let mut fixnum = vec![false; func.values.len()];
    for block in &func.blocks {
        for &param in &block.params {
            let ty = types[param.index()];
            fixnum[param.index()] = !ty.is_bottom() && ty.is_subset(TypeSet::FIXNUM);
        }
    }
    let is_fixnum = |fixnum: &[bool], value: Value| -> bool {
        let Some(value) = func.resolve(value) else {
            return false;
        };
        let data = &func.values[value.index()];
        match data.def {
            ValueDef::Param { .. } => fixnum[value.index()],
            _ => data.rep.is_tagged() && !data.ty.is_bottom() && data.ty.is_subset(TypeSet::FIXNUM),
        }
    };
    loop {
        let mut changed = false;
        for block in &func.blocks {
            for edge in block.term.edges() {
                let params = &func.blocks[edge.target.index()].params;
                for (&param, &arg) in params.iter().zip(&edge.args) {
                    if fixnum[param.index()] && !is_fixnum(&fixnum, arg) {
                        fixnum[param.index()] = false;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut narrowed = 0;
    for (index, data) in func.values.iter_mut().enumerate() {
        if fixnum[index] && !(data.rep == Rep::TaggedFix && data.ty.is_subset(TypeSet::FIXNUM)) {
            data.ty = data.ty.meet(TypeSet::FIXNUM);
            data.rep = Rep::TaggedFix;
            narrowed += 1;
        }
    }
    narrowed
}

/// Values the function itself guards as fixnums somewhere: speculating on
/// any other one could deoptimize an entry the loop would have survived.
fn fixnum_checked(func: &Func) -> Vec<bool> {
    let mut checked = vec![false; func.values.len()];
    for inst in &func.insts {
        if let Opcode::CheckType(target) = inst.op
            && !target.is_bottom()
            && target.is_subset(TypeSet::FIXNUM)
            && let Some(&input) = inst.args.first()
            && let Some(input) = func.resolve(input)
        {
            checked[input.index()] = true;
        }
    }
    checked
}

/// The interpreter state at the OSR entry: the header's pc with the slots
/// as they came, and the header frame's handler and binding depths.
fn entry_frame(func: &mut Func, osr: &OsrEntry, entry: Block) -> Option<FrameId> {
    let header = func
        .frames
        .iter()
        .find(|frame| frame.pc == osr.entry_pc && frame.parent.is_none() && frame.site.is_none())?;
    let state = FrameState {
        pc: osr.entry_pc,
        stack: func.entry_stacks.get(entry.index())?.clone(),
        handlers: header.handlers,
        binds: header.binds,
        parent: None,
        site: None,
    };
    Some(func.intern_frame(state))
}

/// Append `CheckType(FIXNUM)` of `slot` to the entry block.
fn guard(func: &mut Func, entry: Block, slot: Value, frame: FrameId, pc: u32) -> Value {
    let id = Inst(func.insts.len() as u32);
    let value = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty: TypeSet::FIXNUM,
        rep: Rep::TaggedFix,
        def: ValueDef::Inst(id),
    });
    func.insts.push(InstData {
        op: Opcode::CheckType(TypeSet::FIXNUM),
        args: vec![slot],
        result: Some(value),
        eff: Effects::MAY_DEOPT,
        mem: AliasClass::None,
        frame: Some(frame),
        pc,
    });
    func.blocks[entry.index()].insts.push(id);
    value
}

#[cfg(test)]
#[path = "tests/osr_entry_test.rs"]
mod tests;
