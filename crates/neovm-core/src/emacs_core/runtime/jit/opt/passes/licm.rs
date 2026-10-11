//! Bounded immutable/scalar LICM and anticipated loop-entry guards.
//!
//! Threading: CFG facts, candidate and census belong to this compilation. No
//! runtime cache, Lisp heap inspection or mutator assumption is introduced.

use std::collections::{HashMap, HashSet};

use super::{cfg::Dominance, range, static_fix};
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    types::{Range, TypeSet},
    verify::VerifyError,
};

/// Actual movement only; scalar compiler-owned counts stay immutable after
/// publication and do not establish native capability by themselves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LicmStats {
    pub(crate) pure_hoisted: usize,
    pub(crate) immutable_loads_hoisted: usize,
    pub(crate) guards_hoisted: usize,
}

/// Original definitions/observers remain attached in place as identity views.
/// A newly hoisted guard resumes the original header, with its actual initial
/// GNU stack mapped from the one preheader edge; it never uses body phi values
/// in the preheader frame. No block splitting, value/frame compaction or Poll
/// motion occurs. Unsupported/multi-entry/OSR/overlapping loops are unchanged.
pub(crate) fn run(func: &mut Func) -> Result<LicmStats, VerifyError> {
    run_with_fast(func, super::super::pass_fast::enabled())
}

/// Invocation-owned discovery proves the empty-loop case before any candidate
/// clone. The original input verifier and selected publication verifier remain.
fn run_with_fast(func: &mut Func, fast: bool) -> Result<LicmStats, VerifyError> {
    func.verify()?;
    if func.osr.is_some() {
        return Ok(LicmStats::default());
    }
    let dom = Dominance::new(func)?;
    let Some(loops) = natural_loops(func, &dom) else {
        return Ok(LicmStats::default());
    };
    if fast && loops.is_empty() {
        return Ok(LicmStats::default());
    }
    let mut candidate = func.clone();
    let mut stats = LicmStats::default();
    for natural in loops {
        move_loop(&mut candidate, &dom, &natural, &mut stats);
    }
    candidate.verify()?;
    *func = candidate;
    Ok(stats)
}

/// One closed single-entry natural loop and existing unconditional preheader.
/// Threading: compiler IDs only, not a runtime loop-state object.
struct NaturalLoop {
    header: Block,
    preheader: Block,
    members: HashSet<Block>,
}

fn natural_loops(func: &Func, dom: &Dominance) -> Option<Vec<NaturalLoop>> {
    let mut latches: HashMap<Block, Vec<Block>> = HashMap::new();
    for &block in dom.reverse_postorder() {
        for edge in func.blocks[block.index()].term.edges() {
            if dom.dominates(edge.target, block) {
                latches.entry(edge.target).or_default().push(block);
            }
        }
    }
    let mut headers: Vec<_> = latches.into_iter().collect();
    headers.sort_by_key(|(header, _)| *header);
    let mut loops = Vec::new();
    let mut budget = 1_000_000usize;
    for (header, backedges) in headers {
        let mut members = HashSet::from([header]);
        let mut pending = backedges;
        let mut closed = true;
        while let Some(block) = pending.pop() {
            budget = budget.checked_sub(1)?;
            if !members.insert(block) || block == header {
                continue;
            }
            if !dom.dominates(header, block) {
                closed = false;
                break;
            }
            pending.extend(
                func.blocks[block.index()]
                    .preds
                    .iter()
                    .copied()
                    .filter(|&pred| dom.is_reachable(pred)),
            );
        }
        if !closed {
            continue;
        }
        let mut outside: Vec<_> = func.blocks[header.index()]
            .preds
            .iter()
            .copied()
            .filter(|pred| dom.is_reachable(*pred) && !members.contains(pred))
            .collect();
        outside.sort_unstable();
        outside.dedup();
        if outside.len() != 1 {
            continue;
        }
        let preheader = outside[0];
        if !matches!(&func.blocks[preheader.index()].term,
            Term::Jump(edge) if edge.target == header)
        {
            continue;
        }
        if members.iter().any(|&block| {
            block != header
                && func.blocks[block.index()]
                    .preds
                    .iter()
                    .any(|pred| dom.is_reachable(*pred) && !members.contains(pred))
        }) {
            continue;
        }
        loops.push(NaturalLoop {
            header,
            preheader,
            members,
        });
    }
    // Process disjoint innermost loops only in this first bounded revision.
    // No speculative claim about nested-loop gains or duplicate movement.
    loops.sort_by_key(|natural| (natural.members.len(), natural.header));
    let mut claimed = HashSet::new();
    loops.retain(|natural| {
        if natural.members.iter().any(|block| claimed.contains(block)) {
            return false;
        }
        claimed.extend(natural.members.iter().copied());
        true
    });
    Some(loops)
}

fn definitions(func: &Func) -> Vec<Option<Block>> {
    let mut owners = vec![None; func.values.len()];
    for (index, block) in func.blocks.iter().enumerate() {
        let owner = Block(index as u32);
        for &param in &block.params {
            owners[param.index()] = Some(owner);
        }
        for &id in &block.insts {
            if let Some(result) = func.insts[id.index()].result {
                owners[result.index()] = Some(owner);
            }
        }
    }
    owners
}

fn available(
    func: &Func,
    dom: &Dominance,
    owners: &[Option<Block>],
    preheader: Block,
    hoisted: &HashMap<Value, Value>,
    value: Value,
) -> Option<Value> {
    let actual = func.resolve(value)?;
    if let Some(&moved) = hoisted.get(&actual) {
        return Some(moved);
    }
    let owner = owners.get(actual.index()).copied().flatten()?;
    dom.dominates(owner, preheader).then_some(actual)
}

fn fixed(ty: TypeSet) -> Option<Range> {
    (!ty.is_bottom() && ty.is_subset(TypeSet::FIXNUM))
        .then(|| ty.range())
        .flatten()
}

/// This is semantic classification, not trust in a PURE/Immutable hint alone.
/// No fresh allocation, dynamic constant, opaque bytecode, checked arithmetic,
/// arbitrary narrowing Refine, mutable length/backing/slot-0, Eq/SWP or floating
/// arithmetic is made speculative. Only a proved live FLOAT payload is an
/// immutable memory read; its native adapter is a separate implementation seam.
#[deny(clippy::wildcard_enum_match_arm)]
fn movable(func: &Func, inst: &InstData, args: &[Value]) -> Option<bool> {
    let result = inst.result?;
    let output = &func.values[result.index()];
    let actual = |position: usize| -> Option<&ValueData> {
        Some(&func.values[func.resolve(*args.get(position)?)?.index()])
    };
    if inst.op == Opcode::LoadF64 {
        let base = actual(0)?;
        return (inst.eff == Effects::READ_HEAP
            && inst.mem == AliasClass::Immutable
            && base.rep == Rep::Tagged
            && !base.ty.is_bottom()
            && base.ty.is_subset(TypeSet::FLOAT)
            && output.rep == Rep::RawF64)
            .then_some(true);
    }
    if inst.eff != Effects::PURE || inst.mem != AliasClass::None {
        return None;
    }
    let yes = match &inst.op {
        Opcode::Const(index) => {
            // Native unframed literal admission and later Untag cost must use
            // the same actual immutable word proof. Tagged/TOP declarations,
            // dynamic prefix templates and incompatible singleton views are
            // not made into unframed clones by this first native LICM stage.
            static_fix::static_fix_origin(func, result).is_some_and(|(pool, _)| pool == *index)
        }
        Opcode::BoolConst(_) | Opcode::BoolToLisp | Opcode::IsNonNil => true,
        Opcode::TypeTest(_) => actual(0)?.rep.is_tagged(),
        Opcode::TagFix | Opcode::UntagFix => {
            fixed(actual(0)?.ty).is_some() && fixed(output.ty).is_some()
        }
        Opcode::Refine(_) => {
            // Point-local Range views deliberately fail this exact-declaration
            // test. Moving their narrowing would speculate an edge fact.
            actual(0)?.ty == output.ty
                && (actual(0)?.rep == output.rep
                    || actual(0)?.rep.is_tagged() && output.rep.is_tagged())
        }
        Opcode::FixCmp(_) => fixed(actual(0)?.ty).is_some() && fixed(actual(1)?.ty).is_some(),
        Opcode::FixAdd { checked: false }
        | Opcode::FixSub { checked: false }
        | Opcode::FixMul { checked: false } => range::arithmetic_result_fits(
            &inst.op,
            fixed(actual(0)?.ty)?,
            fixed(actual(1)?.ty)?,
            output.ty,
        ),
        Opcode::Select => {
            !output.ty.is_bottom()
                && output.ty.is_subset(TypeSet::FIXNUM.join(TypeSet::BOOLEAN))
                && matches!(
                    output.rep,
                    Rep::Tagged | Rep::TaggedFix | Rep::RawInt | Rep::Bool
                )
        }
        Opcode::Sink(..)
        | Opcode::EnvConst(..)
        | Opcode::Arg(..)
        | Opcode::OsrSlot(..)
        | Opcode::UnboxF64
        | Opcode::FixDiv
        | Opcode::FixRem
        | Opcode::FixMinMax(..)
        | Opcode::F64Add
        | Opcode::F64Sub
        | Opcode::F64Mul
        | Opcode::F64Div
        | Opcode::F64Cmp(..)
        | Opcode::F64FromFix
        | Opcode::F64Neg
        | Opcode::F64Sqrt
        | Opcode::Eq
        | Opcode::CheckType(..)
        | Opcode::CheckNonZero
        | Opcode::CheckBounds
        | Opcode::CheckEq(..)
        | Opcode::CheckNoOverflow
        | Opcode::LoadCar
        | Opcode::LoadCdr
        | Opcode::StoreCar
        | Opcode::StoreCdr
        | Opcode::LoadVecLen
        | Opcode::LoadVecSlots
        | Opcode::LoadVecElem
        | Opcode::StoreVecElem
        | Opcode::LoadRecTag
        | Opcode::LoadSymValue(..)
        | Opcode::StoreSymValue(..)
        | Opcode::LoadF64
        | Opcode::AllocCons
        | Opcode::AllocFloat
        | Opcode::Call { .. }
        | Opcode::Builtin(..)
        | Opcode::Opaque(..)
        | Opcode::OpaqueBool(..)
        | Opcode::InlineEntry(..)
        | Opcode::Poll
        | Opcode::PublishRoot
        | Opcode::FixAdd { checked: true }
        | Opcode::FixSub { checked: true }
        | Opcode::FixMul { checked: true } => false,
    };
    yes.then_some(false)
}

/// Speculation before an anticipated header guard crosses only these total
/// pure operations. A failed guard, an opaque success path, a mutable read,
/// call, write, allocation, safepoint or Poll closes the anticipation window.
fn transparent(func: &Func, inst: &InstData) -> bool {
    movable(func, inst, &inst.args) == Some(false)
}

fn move_loop(func: &mut Func, dom: &Dominance, natural: &NaturalLoop, stats: &mut LicmStats) {
    let owners = definitions(func);
    let mut hoisted = HashMap::new();
    let mut ordinal = vec![usize::MAX; func.blocks.len()];
    for (index, &block) in dom.reverse_postorder().iter().enumerate() {
        ordinal[block.index()] = index;
    }
    let mut order: Vec<_> = natural.members.iter().copied().collect();
    order.sort_by_key(|block| ordinal[block.index()]);
    // Header-first RPO ensures the anticipated guard can provide the actual
    // dominating successful view for later invariant scalar operations.
    let mut anticipation = true;
    for block in order {
        for id in func.blocks[block.index()].insts.clone() {
            let old = func.insts[id.index()].clone();
            let args: Option<Vec<_>> = old
                .args
                .iter()
                .map(|&value| available(func, dom, &owners, natural.preheader, &hoisted, value))
                .collect();
            let guard = block == natural.header
                && anticipation
                && matches!(old.op, Opcode::CheckType(_))
                && old.eff == Effects::MAY_DEOPT
                && old.mem == AliasClass::None;
            if guard {
                if let (Some(args), Some(result), Some(old_frame)) =
                    (args.as_ref(), old.result, old.frame)
                {
                    let frame = replay_frame(func, dom, &owners, natural, old_frame);
                    let Some(frame) = frame else {
                        anticipation = false;
                        continue;
                    };
                    let header_pc = func.blocks[natural.header.index()].pc;
                    let moved = clone_to_preheader(
                        func,
                        natural.preheader,
                        &old,
                        args.clone(),
                        Some(frame),
                        header_pc,
                    );
                    replace(func, id, moved);
                    hoisted.insert(result, moved);
                    stats.guards_hoisted += 1;
                    continue;
                }
            }
            if block == natural.header && !transparent(func, &old) {
                anticipation = false;
            }
            let (Some(args), Some(result)) = (args, old.result) else {
                continue;
            };
            let Some(immutable) = movable(func, &old, &args) else {
                continue;
            };
            let moved = clone_to_preheader(func, natural.preheader, &old, args, None, old.pc);
            replace(func, id, moved);
            hoisted.insert(result, moved);
            if immutable {
                stats.immutable_loads_hoisted += 1;
            } else {
                stats.pure_hoisted += 1;
            }
        }
    }
}

/// Full first-entry replay, not a relocated header-body guard frame. Source,
/// entry and header frame must agree exactly. Parent/site chains are declined
/// until an explicit inline-chain mapper is implemented and independently tested.
fn replay_frame(
    func: &mut Func,
    dom: &Dominance,
    owners: &[Option<Block>],
    natural: &NaturalLoop,
    original_guard_frame: FrameId,
) -> Option<FrameId> {
    let header = &func.blocks[natural.header.index()];
    let source = func.source_states.get(header.pc as usize)?.as_ref()?;
    let base = func.frames.get(source.frame.index())?;
    let original_guard = func.frames.get(original_guard_frame.index())?;
    let entry = func.entry_stacks.get(natural.header.index())?;
    if source.block != natural.header
        || source.pre.as_ref() != entry.as_ref()
        || base.stack != source.pre
        || base.pc != header.pc
        || base.parent.is_some()
        || base.site.is_some()
        || original_guard.parent.is_some()
        || original_guard.site.is_some()
        || base.handlers != original_guard.handlers
        || base.binds != original_guard.binds
    {
        return None;
    }
    let Term::Jump(edge) = &func.blocks[natural.preheader.index()].term else {
        return None;
    };
    let mut mapped = Vec::with_capacity(entry.len());
    for &value in entry.iter() {
        let actual = func.resolve(value)?;
        let initial = match func.values[actual.index()].def {
            ValueDef::Param { block, index } if block == natural.header => {
                *edge.args.get(index as usize)?
            }
            _ => actual,
        };
        mapped.push(available(
            func,
            dom,
            owners,
            natural.preheader,
            &HashMap::new(),
            initial,
        )?);
    }
    let frame = FrameState {
        pc: base.pc,
        stack: mapped.into(),
        handlers: base.handlers,
        binds: base.binds,
        parent: None,
        site: None,
    };
    Some(func.intern_frame(frame))
}

fn clone_to_preheader(
    func: &mut Func,
    preheader: Block,
    old: &InstData,
    args: Vec<Value>,
    frame: Option<FrameId>,
    pc: u32,
) -> Value {
    let output = &func.values[old.result.expect("movable result").index()];
    let (ty, rep) = (output.ty, output.rep);
    let id = Inst(func.insts.len() as u32);
    let value = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Inst(id),
    });
    func.insts.push(InstData {
        op: old.op.clone(),
        args,
        result: Some(value),
        eff: old.eff,
        mem: old.mem,
        frame,
        pc,
    });
    // Existing preheader instructions stay in order, including its final Poll.
    func.blocks[preheader.index()].insts.push(id);
    value
}

fn replace(func: &mut Func, id: Inst, moved: Value) {
    let old = &mut func.insts[id.index()];
    old.op = Opcode::Refine(func.values[old.result.expect("movable output").index()].ty);
    old.args = vec![moved];
    old.eff = Effects::PURE;
    old.mem = AliasClass::None;
    // Original frame, result identity and source PC remain attached in place.
}

#[cfg(test)]
#[path = "tests/licm_test.rs"]
mod tests;

#[cfg(test)]
pub(crate) fn run_fast_for_test(func: &mut Func, fast: bool) -> Result<LicmStats, VerifyError> {
    run_with_fast(func, fast)
}
