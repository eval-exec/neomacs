//! Compiler-owned original-Aref proof metadata and final native admission.
//!
//! Threading: a compiler exclusively owns this table and all ids/ranges in it.
//! It contains no Lisp pointers, heap reads, runtime state or shared cache. A
//! final verified table is immutable and is consumed only during native emit.
//! The existing mutator/backing synchronization contract still applies; this
//! adapter never turns a vector length or element buffer into Immutable memory.

use std::collections::{HashMap, HashSet};

use super::cfg::Dominance;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    types::{Range, TypeSet},
    verify::VerifyError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LayoutVersion(u32);

/// Compiler-owned provenance for a lower bound on CURRENT mutable length.
/// Only a retained, executed CheckBounds is a source. An elided check cannot
/// manufacture a stronger floor, and a scalar old length is never a layout.
#[derive(Clone, Debug)]
pub(crate) struct LengthFloorWitness {
    pub(crate) guard: Inst,
    pub(crate) length_read: Inst,
    pub(crate) index: Value,
    pub(crate) floor: i64,
}

/// These two attached same-rep point views describe actual numeric facts.
/// They are deliberately NOT arguments of the original source instruction or
/// original GNU frame. The final verifier separately checks their provenance.
#[derive(Clone, Debug)]
pub(crate) struct NumericBoundsWitness {
    pub(crate) index_view: Value,
    pub(crate) length_view: Value,
    pub(crate) floor: LengthFloorWitness,
}

#[derive(Clone, Debug)]
pub(crate) enum BoundsWitness {
    Checked,
    Elided(NumericBoundsWitness),
}

/// The original Opaque(Aref) remains attached with the same result, source pc,
/// full FrameState, source_states and entry IDs. The inserted checks execute
/// BEFORE that operation; their failure resumes its original GNU operation.
#[derive(Clone, Debug)]
pub(crate) struct ArrayReadProof {
    pub(crate) guarded_base: Value,
    pub(crate) checked_index: Value,
    pub(crate) length_read: Inst,
    pub(crate) length: Value,
    pub(crate) bounds: Inst,
    pub(crate) bounds_result: Value,
    pub(crate) frame: FrameId,
    pub(crate) pc: u32,
    pub(crate) witness: BoundsWitness,
}

/// The exclusively owned `Func.array_reads` sidecar is initialized empty and
/// cloned/remapped with Func. Producers publish the explicit table and the Func
/// field together, then final verification admits native array operations.
#[derive(Clone, Debug, Default)]
pub(crate) struct ArrayReadProofs {
    pub(crate) reads: HashMap<Inst, ArrayReadProof>,
}

impl ArrayReadProofs {
    /// Remap an exclusively owned compiler sidecar during CFG compaction.
    /// An unreachable read owner drops its proof; every witness for a retained
    /// owner must still map. Missing source/root/proof ids reject atomically.
    pub(crate) fn remap(
        &self,
        mut inst: impl FnMut(Inst) -> Option<Inst>,
        mut value: impl FnMut(Value) -> Option<Value>,
        mut frame: impl FnMut(FrameId) -> Option<FrameId>,
    ) -> Result<Self, VerifyError> {
        let mut next = Self::default();
        for (&owner, proof) in &self.reads {
            let Some(new_owner) = inst(owner) else {
                continue;
            };
            let fail = || VerifyError::InvalidInst(owner);
            let mut mapped = proof.clone();
            mapped.guarded_base = value(proof.guarded_base).ok_or_else(fail)?;
            mapped.checked_index = value(proof.checked_index).ok_or_else(fail)?;
            mapped.length = value(proof.length).ok_or_else(fail)?;
            mapped.bounds_result = value(proof.bounds_result).ok_or_else(fail)?;
            mapped.length_read = inst(proof.length_read).ok_or_else(fail)?;
            mapped.bounds = inst(proof.bounds).ok_or_else(fail)?;
            mapped.frame = frame(proof.frame).ok_or_else(fail)?;
            if let BoundsWitness::Elided(witness) = &mut mapped.witness {
                witness.index_view = value(witness.index_view).ok_or_else(fail)?;
                witness.length_view = value(witness.length_view).ok_or_else(fail)?;
                witness.floor.guard = inst(witness.floor.guard).ok_or_else(fail)?;
                witness.floor.length_read = inst(witness.floor.length_read).ok_or_else(fail)?;
                witness.floor.index = value(witness.floor.index).ok_or_else(fail)?;
            }
            if next.reads.insert(new_owner, mapped).is_some() {
                return Err(fail());
            }
        }
        Ok(next)
    }

    /// Metadata values are compiler users even when absent from normal args.
    /// Preserve their definitions through compiler DCE; this adds no runtime
    /// rooting state or metadata-only extension of a heap value's lifetime.
    pub(crate) fn visit_values(&self, mut visit: impl FnMut(Value)) {
        for proof in self.reads.values() {
            for value in [
                proof.guarded_base,
                proof.checked_index,
                proof.length,
                proof.bounds_result,
            ] {
                visit(value);
            }
            if let BoundsWitness::Elided(witness) = &proof.witness {
                visit(witness.index_view);
                visit(witness.length_view);
                visit(witness.floor.index);
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ArrayLiftStats {
    pub(crate) reads_lifted: usize,
    pub(crate) bounds_inserted: usize,
}

/// Compiler-owned scalar admission hints, captured by the owning mutator and
/// mapped to FINAL lowered pcs before this pass. These are profitability hints,
/// not heap/type/length proofs: every selected read still executes a real shape
/// guard. No hint can authorize pointer access or certify mutable layout.
///
/// `site_types` contains only stable plain-vector/record masks recorded by T1's
/// live tier window. Unknown/Other and unsupported fused callee sites are bottom.
/// `constant_types` contains actual non-prefix source constants classified while
/// rooted on the mutator; the opaque bits prevent a hint for a replaced pool item
/// being reused. A worker never dereferences those bits.
#[derive(Clone, Debug, Default)]
pub(crate) struct ArrayAdmission {
    pub(crate) site_types: Vec<TypeSet>,
    pub(crate) constant_types: HashMap<u32, (ValueBits, TypeSet)>,
}

fn admitted_type(func: &Func, value: Value, pc: usize, hints: &ArrayAdmission) -> Option<TypeSet> {
    let plain = TypeSet::VECTOR.join(TypeSet::RECORD);
    if shape_guarded(func, value) {
        return Some(func.values[value.index()].ty);
    }
    // An actual checked identity is authority; a bare declared Arg type is not.
    let origin = word_origin(func, value)?;
    if let ValueDef::Inst(id) = func.values.get(origin.index())?.def {
        if let Opcode::Const(index) = func.insts.get(id.index())?.op {
            if index as usize >= func.dynamic_prefix {
                if let Some(&(bits, ty)) = hints.constant_types.get(&index) {
                    if func.consts.get(index as usize) == Some(&bits)
                        && !ty.is_bottom()
                        && ty.is_subset(plain)
                    {
                        return Some(ty);
                    }
                }
            }
        }
    }
    let ty = *hints.site_types.get(pc)?;
    (!ty.is_bottom() && ty.is_subset(plain)).then_some(ty)
}

fn append(
    func: &mut Func,
    op: Opcode,
    args: Vec<Value>,
    ty: TypeSet,
    rep: Rep,
    eff: Effects,
    mem: AliasClass,
    frame: FrameId,
    pc: u32,
) -> (Inst, Value) {
    let id = Inst(func.insts.len() as u32);
    let value = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Inst(id),
    });
    func.insts.push(InstData {
        op,
        args,
        result: Some(value),
        eff,
        mem,
        frame: Some(frame),
        pc,
    });
    (id, value)
}

/// Exact macro lift, selected only by Range and compatible native-Aref knobs.
/// Index CHECK_FIXNUM precedes the array-tag/header check, which precedes the
/// bounds check, matching GNU data.c Faref and bytecode.c Baref. Failure is a
/// speculation deopt, NOT a new wrong-type/error implementation.
///
/// The small guard cache is block-local and cleared at every unmodeled event.
/// It reuses exact checked scalar/object identities, not object contents or a
/// slot-0 convention. Layout length itself is loaded freshly for each Aref.
pub(crate) fn lift(
    func: &mut Func,
    proofs: &mut ArrayReadProofs,
    hints: &ArrayAdmission,
) -> Result<ArrayLiftStats, VerifyError> {
    func.verify()?;
    verify_reads(func, proofs).map_err(|error| VerifyError::InvalidInst(error.inst))?;
    let mut candidate = func.clone();
    let mut table = proofs.clone();
    let mut stats = ArrayLiftStats::default();
    for block in 0..candidate.blocks.len() {
        let original = candidate.blocks[block].insts.clone();
        let mut attached = Vec::with_capacity(original.len());
        let mut fixnums = HashMap::<Value, Value>::new();
        let mut arrays = HashMap::<Value, Value>::new();
        for id in original {
            let old = candidate.insts[id.index()].clone();
            let eligible = !table.reads.contains_key(&id)
                && old.op == Opcode::Opaque(Op::Aref)
                && old.args.len() == 2
                && old.result.is_some()
                && old.frame.is_some()
                && old.eff == Effects::READ_HEAP.with(Effects::MAY_DEOPT)
                && old.mem == AliasClass::VecElem
                && old
                    .args
                    .iter()
                    .all(|&v| candidate.values[v.index()].rep.is_tagged());
            if !eligible {
                if layout_barrier(&old, false) {
                    fixnums.clear();
                    arrays.clear();
                }
                attached.push(id);
                continue;
            }
            let frame = old.frame.expect("eligible original frame");
            let index_type = candidate.values[old.args[1].index()]
                .ty
                .meet(TypeSet::FIXNUM);
            let Some(admitted) = admitted_type(&candidate, old.args[0], old.pc as usize, hints)
            else {
                // Legitimate string/bool-vector/char-table Arefs retain their
                // shared path rather than deopt on each normal T2 invocation.
                fixnums.clear();
                arrays.clear();
                attached.push(id);
                continue;
            };
            let base_type = candidate.values[old.args[0].index()].ty.meet(admitted);
            if index_type.is_bottom() || base_type.is_bottom() {
                fixnums.clear();
                arrays.clear();
                attached.push(id);
                continue;
            }
            let base_origin = word_origin(&candidate, old.args[0]).expect("verified view chain");
            let index_origin = word_origin(&candidate, old.args[1]).expect("verified view chain");
            // The original unknown Arg is never globally stamped FIXNUM/ARRAY.
            let index = if let Some(&view) = fixnums.get(&index_origin) {
                view
            } else {
                let (guard, view) = append(
                    &mut candidate,
                    Opcode::CheckType(TypeSet::FIXNUM),
                    vec![old.args[1]],
                    index_type,
                    Rep::TaggedFix,
                    Effects::MAY_DEOPT,
                    AliasClass::None,
                    frame,
                    old.pc,
                );
                attached.push(guard);
                fixnums.insert(index_origin, view);
                view
            };
            let base = if let Some(&view) = arrays.get(&base_origin) {
                view
            } else {
                let (guard, view) = append(
                    &mut candidate,
                    Opcode::CheckType(admitted),
                    vec![old.args[0]],
                    base_type,
                    Rep::Tagged,
                    Effects::MAY_DEOPT,
                    AliasClass::None,
                    frame,
                    old.pc,
                );
                attached.push(guard);
                arrays.insert(base_origin, view);
                view
            };
            let (length_read, length) = append(
                &mut candidate,
                Opcode::LoadVecLen,
                vec![base],
                TypeSet::fixnum_range(Range {
                    lo: 0,
                    hi: Range::FULL.hi,
                }),
                Rep::RawInt,
                Effects::READ_HEAP,
                AliasClass::Unknown,
                frame,
                old.pc,
            );
            let index_ty = candidate.values[index.index()].ty;
            let (bounds, bounds_result) = append(
                &mut candidate,
                Opcode::CheckBounds,
                vec![index, length],
                index_ty,
                Rep::TaggedFix,
                Effects::MAY_DEOPT,
                AliasClass::None,
                frame,
                old.pc,
            );
            attached.extend([length_read, bounds]);
            candidate.insts[id.index()].args = vec![base, bounds_result];
            table.reads.insert(
                id,
                ArrayReadProof {
                    guarded_base: base,
                    checked_index: index,
                    length_read,
                    length,
                    bounds,
                    bounds_result,
                    frame,
                    pc: old.pc,
                    witness: BoundsWitness::Checked,
                },
            );
            attached.push(id);
            stats.reads_lifted += 1;
            stats.bounds_inserted += 1;
        }
        candidate.blocks[block].insts = attached;
    }
    candidate.array_reads = table.clone(); // Atomic sidecar handoff before Func::verify.
    candidate.verify()?; // Requires the strengthened root-owned array verifier.
    verify_reads(&candidate, &table).map_err(|e| VerifyError::InvalidInst(e.inst))?;
    *func = candidate;
    *proofs = table;
    Ok(stats)
}

/// Pure mathematical/word identities only; a phi, EnvConst, dereference or
/// arithmetic producer is an origin, not a transparent mutable identity.
fn word_origin(func: &Func, mut value: Value) -> Option<Value> {
    for _ in 0..=func.values.len() {
        let data = func.values.get(value.index())?;
        match data.def {
            ValueDef::Alias(next) => value = next,
            ValueDef::Inst(id) => {
                let inst = func.insts.get(id.index())?;
                if matches!(
                    inst.op,
                    Opcode::CheckType(_) | Opcode::Refine(_) | Opcode::TagFix | Opcode::UntagFix
                ) && inst.args.len() == 1
                {
                    value = inst.args[0];
                } else if inst.op == Opcode::CheckBounds
                    && inst.args.len() == 2
                    && inst.result == Some(value)
                {
                    value = inst.args[0];
                } else {
                    return Some(value);
                }
            }
            ValueDef::Param { .. } => return Some(value),
        }
    }
    None
}

/// Declared intervals alone are NOT an array-BCE proof. The bounded first
/// adapter accepts actual non-prefix fixnum literals and real executed
/// CheckType interval guards through exact Tag/Untag/Refine identities. Wider
/// branch/induction/arithmetic interval proofs need their own audited witness
/// kind before this adapter may certify them; Range must decline those array
/// eliminations meanwhile, rather than emit a native-refusal-only pass.
struct CheckedInterval {
    range: Range,
    guards: Vec<Inst>,
}

/// A retained real Bounds needs tag grounding, independently of whether a
/// narrower declared interval has a native-supported numeric witness. Follow
/// exact scalar identity views to an actual FIX guard/literal; a bare declared
/// Arg or pure narrowed Refine supplies neither. No range claim is returned.
fn checked_fix_tag(func: &Func, mut value: Value) -> Option<Vec<Inst>> {
    for _ in 0..=func.values.len() {
        let data = func.values.get(value.index())?;
        match data.def {
            ValueDef::Alias(next) => value = next,
            ValueDef::Inst(id) => {
                let inst = func.insts.get(id.index())?;
                match inst.op {
                    Opcode::Const(index) if index as usize >= func.dynamic_prefix => {
                        let ty = TypeSet::for_constant(*func.consts.get(index as usize)?);
                        return (!ty.is_bottom() && ty.is_subset(TypeSet::FIXNUM)).then(Vec::new);
                    }
                    Opcode::CheckType(target)
                        if !target.is_bottom()
                            && target.is_subset(TypeSet::FIXNUM)
                            && inst.args.len() == 1
                            && inst.eff == Effects::MAY_DEOPT
                            && inst.frame.is_some() =>
                    {
                        return Some(vec![id]);
                    }
                    Opcode::Refine(_) | Opcode::TagFix | Opcode::UntagFix
                        if inst.args.len() == 1 =>
                    {
                        value = inst.args[0]
                    }
                    _ => return None,
                }
            }
            ValueDef::Param { .. } => return None,
        }
    }
    None
}

fn checked_interval(func: &Func, mut value: Value) -> Option<CheckedInterval> {
    let wanted = func.values.get(value.index())?.ty.range()?;
    let mut allowed = Range::FULL;
    let mut guarded = false;
    let mut guards = Vec::new();
    for _ in 0..=func.values.len() {
        let data = func.values.get(value.index())?;
        match data.def {
            ValueDef::Alias(next) => value = next,
            ValueDef::Inst(id) => {
                let inst = func.insts.get(id.index())?;
                let source = match inst.op {
                    Opcode::Const(index) if index as usize >= func.dynamic_prefix => {
                        let ty = TypeSet::for_constant(*func.consts.get(index as usize)?);
                        if ty.is_bottom() || !ty.is_subset(TypeSet::FIXNUM) {
                            return None;
                        }
                        allowed.meet(ty.range()?)?
                    }
                    Opcode::CheckType(target)
                        if !target.is_bottom()
                            && target.is_subset(TypeSet::FIXNUM)
                            && inst.eff == Effects::MAY_DEOPT
                            && inst.frame.is_some() =>
                    {
                        // The native guard itself checks this complete
                        // interval, so its successful value is grounded.
                        allowed = allowed.meet(target.range()?)?;
                        guarded = true;
                        guards.push(id);
                        value = *inst.args.first()?;
                        continue;
                    }
                    Opcode::Refine(_) | Opcode::TagFix | Opcode::UntagFix
                        if inst.args.len() == 1 =>
                    {
                        value = inst.args[0];
                        continue;
                    }
                    _ if guarded => allowed,
                    _ => return None,
                };
                // A point view may be no stronger than its real source. A
                // narrower arbitrary Refine cannot act as a missing guard.
                return (wanted.lo <= source.lo && source.hi <= wanted.hi).then_some(
                    CheckedInterval {
                        range: wanted,
                        guards,
                    },
                );
            }
            ValueDef::Param { .. } => {
                return guarded
                    .then_some(allowed)
                    .filter(|source| wanted.lo <= source.lo && source.hi <= wanted.hi)
                    .map(|_| CheckedInterval {
                        range: wanted,
                        guards,
                    });
            }
        }
    }
    None
}

fn shape_guarded(func: &Func, mut value: Value) -> bool {
    let array = TypeSet::VECTOR.join(TypeSet::RECORD);
    for _ in 0..=func.values.len() {
        let Some(data) = func.values.get(value.index()) else {
            return false;
        };
        if data.rep != Rep::Tagged || data.ty.is_bottom() || !data.ty.is_subset(array) {
            return false;
        }
        match data.def {
            ValueDef::Alias(next) => value = next,
            ValueDef::Inst(id) => {
                let Some(inst) = func.insts.get(id.index()) else {
                    return false;
                };
                match inst.op {
                    Opcode::CheckType(target) => {
                        return !target.is_bottom()
                            && target.is_subset(array)
                            && inst.eff == Effects::MAY_DEOPT
                            && inst.frame.is_some();
                    }
                    Opcode::Refine(_) if inst.args.len() == 1 => value = inst.args[0],
                    _ => return false,
                }
            }
            _ => return false,
        }
    }
    false
}

/// Exact native successful macro is read-only and cannot invoke Lisp/GC. Every
/// other opaque op, Poll, Call, allocation and write kills current layout facts.
/// This exception is valid ONLY if verify_reads succeeds for the entire table.
pub(crate) fn layout_barrier(inst: &InstData, has_array_proof: bool) -> bool {
    if has_array_proof
        && inst.op == Opcode::Opaque(Op::Aref)
        && inst.eff == Effects::READ_HEAP.with(Effects::MAY_DEOPT)
        && inst.mem == AliasClass::VecElem
    {
        return false;
    }
    matches!(
        inst.op,
        Opcode::Poll
            | Opcode::Call { .. }
            | Opcode::Opaque(_)
            | Opcode::OpaqueBool(_)
            | Opcode::Builtin(_)
            | Opcode::AllocCons
            | Opcode::AllocFloat
            | Opcode::StoreCar
            | Opcode::StoreCdr
            | Opcode::StoreVecElem
            | Opcode::StoreSymValue(_)
    ) || inst.eff.intersects(
        Effects::WRITE_HEAP
            .with(Effects::MAY_GC)
            .with(Effects::MAY_REENTER)
            .with(Effects::MAY_SIGNAL)
            .with(Effects::ALLOCATES),
    )
}

#[derive(Clone, Copy, Debug)]
struct Position {
    block: Block,
    ordinal: usize,
    epoch: LayoutVersion,
}

fn inst_before(
    positions: &[Option<Position>],
    dom: &Dominance,
    definition: Inst,
    usage: Inst,
) -> bool {
    let Some(a) = positions.get(definition.index()).copied().flatten() else {
        return false;
    };
    let Some(b) = positions.get(usage.index()).copied().flatten() else {
        return false;
    };
    if a.block == b.block {
        a.ordinal < b.ordinal
    } else {
        dom.dominates(a.block, b.block)
    }
}

fn value_before(
    func: &Func,
    positions: &[Option<Position>],
    dom: &Dominance,
    value: Value,
    usage: Inst,
) -> bool {
    let Some(value) = func.resolve(value) else {
        return false;
    };
    let Some(data) = func.values.get(value.index()) else {
        return false;
    };
    let Some(usage) = positions.get(usage.index()).copied().flatten() else {
        return false;
    };
    match data.def {
        ValueDef::Param { block, .. } => dom.dominates(block, usage.block),
        ValueDef::Inst(id) => {
            positions
                .get(id.index())
                .copied()
                .flatten()
                .is_some_and(|definition| {
                    if definition.block == usage.block {
                        definition.ordinal < usage.ordinal
                    } else {
                        dom.dominates(definition.block, usage.block)
                    }
                })
        }
        ValueDef::Alias(_) => false,
    }
}

fn fix_scalar(func: &Func, value: Value) -> bool {
    let Some(data) = func.values.get(value.index()) else {
        return false;
    };
    !data.ty.is_bottom()
        && data.ty.is_subset(TypeSet::FIXNUM)
        && matches!(data.rep, Rep::Tagged | Rep::TaggedFix | Rep::RawInt)
}

fn bounds_result_matches_index(func: &Func, check: &InstData) -> bool {
    let Some(&index) = check.args.first() else {
        return false;
    };
    let Some(result) = check.result else {
        return false;
    };
    if !fix_scalar(func, index) || !fix_scalar(func, result) {
        return false;
    }
    let input = &func.values[index.index()];
    let output = &func.values[result.index()];
    input.rep == output.rep && output.ty.is_subset(input.ty)
}

/// Compiler verification error, containing ids only. Native admission must
/// refuse the candidate transactionally if an *elided* proof cannot be rebuilt.
#[derive(Clone, Debug)]
pub(crate) struct ArrayProofError {
    pub(crate) inst: Inst,
    pub(crate) reason: &'static str,
}

/// The native adapter constructs these only after final metadata verification.
/// Shared native Aref receives the verified base/index and current load epoch;
/// it reloads backing and performs no signalable operation before the read.
#[derive(Debug)]
pub(crate) struct VerifiedPlainRead {
    pub(crate) base: Value,
    pub(crate) index: Value,
    pub(crate) bounds_elided: bool,
    pub(crate) epoch: LayoutVersion,
}
#[derive(Debug)]
pub(crate) struct VerifiedArrayReads {
    pub(crate) reads: HashMap<Inst, VerifiedPlainRead>,
    pub(crate) length_reads: HashSet<Inst>,
    pub(crate) bounds: HashSet<Inst>,
}

/// Per-compilation verifier scratch. Built once for a producer query or final
/// native admission; no per-check graph scans or clones and no runtime state.
struct VerificationContext {
    dom: Dominance,
    positions: Vec<Option<Position>>,
}

fn verification_context(
    func: &Func,
    proofs: &ArrayReadProofs,
) -> Result<VerificationContext, ArrayProofError> {
    let dom = Dominance::new(func).map_err(|_| ArrayProofError {
        inst: Inst(0),
        reason: "invalid-cfg",
    })?;
    let mut positions = vec![None; func.insts.len()];
    let mut next = 0u32;
    for (block, data) in func.blocks.iter().enumerate() {
        next = next.checked_add(1).ok_or(ArrayProofError {
            inst: Inst(0),
            reason: "epoch-limit",
        })?;
        for (ordinal, &id) in data.insts.iter().enumerate() {
            let inst = func.insts.get(id.index()).ok_or(ArrayProofError {
                inst: id,
                reason: "invalid-attached-instruction",
            })?;
            if layout_barrier(inst, proofs.reads.contains_key(&id)) {
                next = next.checked_add(1).ok_or(ArrayProofError {
                    inst: id,
                    reason: "epoch-limit",
                })?;
            }
            positions[id.index()] = Some(Position {
                block: Block(block as u32),
                ordinal,
                epoch: LayoutVersion(next),
            });
        }
    }
    Ok(VerificationContext { dom, positions })
}

/// Supported source intervals, keyed by each ORIGINAL resultful Bounds Inst.
/// The input table is verified once using the same position/dominance scratch.
/// Unsupported branch/induction/unchecked arithmetic interval witnesses are
/// simply absent, so Range can decline BCE before counting an elimination.
/// Every accepted value and real interval guard must already execute before
/// that Bounds; a future narrowed view cannot fabricate a prior length floor.
/// This query contains no mutation, heap dereference or worker-visible Lisp.
pub(crate) fn checked_index_ranges(
    func: &Func,
    proofs: &ArrayReadProofs,
) -> Result<HashMap<Inst, Range>, ArrayProofError> {
    let context = verification_context(func, proofs)?;
    verify_reads_with_context(func, proofs, &context)?;
    let mut ranges = HashMap::with_capacity(proofs.reads.len());
    for proof in proofs.reads.values() {
        let Some(interval) = checked_interval(func, proof.checked_index) else {
            continue;
        };
        if value_before(
            func,
            &context.positions,
            &context.dom,
            proof.checked_index,
            proof.bounds,
        ) && interval
            .guards
            .iter()
            .all(|&guard| inst_before(&context.positions, &context.dom, guard, proof.bounds))
        {
            ranges.insert(proof.bounds, interval.range);
        }
    }
    Ok(ranges)
}

/// Rebuild from FINAL verified IR after Range, LICM and Reps. Layout versions
/// are newly assigned from the actual final instruction order, not trusted
/// epoch numbers copied before CFG/layout changes. The initial implementation
/// proves within one block only; every block boundary starts a fresh version.
/// This is conservative at all joins/backedges and avoids a partial path proof.
pub(crate) fn verify_reads(
    func: &Func,
    proofs: &ArrayReadProofs,
) -> Result<VerifiedArrayReads, ArrayProofError> {
    // The caller must validate ordinary IR first. This entry may itself run
    // from Func::verify, so it NEVER calls Func::verify recursively. Metadata
    // definitions and source guards are independently checked below.
    let context = verification_context(func, proofs)?;
    verify_reads_with_context(func, proofs, &context)
}

fn verify_reads_with_context(
    func: &Func,
    proofs: &ArrayReadProofs,
    context: &VerificationContext,
) -> Result<VerifiedArrayReads, ArrayProofError> {
    let positions = &context.positions;
    let dom = &context.dom;
    let mut verified = HashMap::with_capacity(proofs.reads.len());
    let mut length_reads = HashSet::with_capacity(proofs.reads.len());
    let mut bounds = HashSet::with_capacity(proofs.reads.len());
    let mut owners_by_bounds = HashMap::with_capacity(proofs.reads.len());
    for (&owner, proof) in &proofs.reads {
        if owners_by_bounds.insert(proof.bounds, proof).is_some() {
            return Err(ArrayProofError {
                inst: owner,
                reason: "duplicate-bounds-owner",
            });
        }
    }
    for (&id, proof) in &proofs.reads {
        let fail = |reason| ArrayProofError { inst: id, reason };
        if [
            proof.guarded_base,
            proof.checked_index,
            proof.length,
            proof.bounds_result,
        ]
        .iter()
        .any(|value| func.values.get(value.index()).is_none())
        {
            return Err(fail("metadata-value"));
        }
        let inst = func.insts.get(id.index()).ok_or_else(|| fail("read-id"))?;
        let read = positions
            .get(id.index())
            .copied()
            .flatten()
            .ok_or_else(|| fail("detached-read"))?;
        let length_inst = func
            .insts
            .get(proof.length_read.index())
            .ok_or_else(|| fail("length-id"))?;
        let length_pos = positions
            .get(proof.length_read.index())
            .copied()
            .flatten()
            .ok_or_else(|| fail("detached-length"))?;
        let check = func
            .insts
            .get(proof.bounds.index())
            .ok_or_else(|| fail("check-id"))?;
        let check_pos = positions
            .get(proof.bounds.index())
            .copied()
            .flatten()
            .ok_or_else(|| fail("detached-check"))?;
        if inst.op != Opcode::Opaque(Op::Aref)
            || inst.eff != Effects::READ_HEAP.with(Effects::MAY_DEOPT)
            || inst.mem != AliasClass::VecElem
            || inst.frame != Some(proof.frame)
            || inst.pc != proof.pc
            || inst.args.len() != 2
            || !shape_guarded(func, proof.guarded_base)
            || word_origin(func, inst.args[0]) != word_origin(func, proof.guarded_base)
            || word_origin(func, inst.args[1]) != word_origin(func, proof.checked_index)
        {
            return Err(fail("original-read-or-shape"));
        }
        if length_inst.op != Opcode::LoadVecLen
            || length_inst.args != [proof.guarded_base]
            || length_inst.result != Some(proof.length)
            || length_inst.eff != Effects::READ_HEAP
            || length_inst.mem != AliasClass::Unknown
            || length_inst.frame != Some(proof.frame)
            || func.values[proof.length.index()].rep != Rep::RawInt
            || func.values[proof.length.index()].ty
                != TypeSet::fixnum_range(Range {
                    lo: 0,
                    hi: Range::FULL.hi,
                })
            || length_inst.pc != proof.pc
            || check.frame != Some(proof.frame)
            || check.pc != proof.pc
            || check.result != Some(proof.bounds_result)
            || length_pos.block != read.block
            || check_pos.block != read.block
            || length_pos.epoch != read.epoch
            || check_pos.epoch != read.epoch
            || !(length_pos.ordinal < check_pos.ordinal && check_pos.ordinal < read.ordinal)
        {
            return Err(fail("fresh-length-or-check"));
        }
        let elided = match &proof.witness {
            BoundsWitness::Checked => {
                // Bounds checks compare integer payloads; success cannot
                // establish that an unknown original Lisp word was a fixnum.
                // Require an independently grounded literal/real FIX guard,
                // already executed before the retained original Bounds.
                let guards = checked_fix_tag(func, proof.checked_index)
                    .ok_or_else(|| fail("checked-index-source"))?;
                if !fix_scalar(func, proof.checked_index)
                    || !value_before(func, positions, dom, proof.checked_index, proof.bounds)
                    || !guards
                        .iter()
                        .all(|&guard| inst_before(positions, dom, guard, proof.bounds))
                {
                    return Err(fail("checked-index-source"));
                }
                if check.op != Opcode::CheckBounds || check.args.len() != 2
                    || word_origin(func,check.args[0]) != word_origin(func,proof.checked_index)
                    // The current actual raw load result is the owner-length
                    // authority. Reps preserves arg1; no heap identity hint
                    // or old scalar length may stand in for this fresh load.
                    || func.resolve(check.args[1]) != func.resolve(proof.length)
                    || !bounds_result_matches_index(func,check) || !fix_scalar(func,check.args[1])
                    || check.eff != Effects::MAY_DEOPT || check.mem != AliasClass::None
                {
                    return Err(fail("executed-bounds"));
                }
                false
            }
            BoundsWitness::Elided(witness) => {
                let index = checked_interval(func, witness.index_view)
                    .ok_or_else(|| fail("index-witness"))?;
                let length = func
                    .values
                    .get(witness.length_view.index())
                    .and_then(|d| d.ty.range())
                    .ok_or_else(|| fail("length-view"))?;
                let previous = func
                    .insts
                    .get(witness.floor.guard.index())
                    .ok_or_else(|| fail("floor-guard"))?;
                let previous_load = func
                    .insts
                    .get(witness.floor.length_read.index())
                    .ok_or_else(|| fail("floor-load"))?;
                let previous_pos = positions
                    .get(witness.floor.guard.index())
                    .copied()
                    .flatten()
                    .ok_or_else(|| fail("floor-detached"))?;
                let previous_load_pos = positions
                    .get(witness.floor.length_read.index())
                    .copied()
                    .flatten()
                    .ok_or_else(|| fail("floor-load-detached"))?;
                let previous_index = checked_interval(func, witness.floor.index)
                    .ok_or_else(|| fail("floor-index"))?;
                let prior_owner = owners_by_bounds
                    .get(&witness.floor.guard)
                    .ok_or_else(|| fail("floor-owner"))?;
                if prior_owner.length_read != witness.floor.length_read
                    || !matches!(prior_owner.witness, BoundsWitness::Checked)
                {
                    return Err(fail("floor-owner"));
                }
                let floor = previous_index
                    .range
                    .lo
                    .max(0)
                    .checked_add(1)
                    .ok_or_else(|| fail("floor-overflow"))?;
                if !matches!(check.op, Opcode::Refine(_))
                    || check.args.len() != 1
                    || word_origin(func, check.args[0]) != word_origin(func, proof.checked_index)
                    || !bounds_result_matches_index(func, check)
                    || check.eff != Effects::PURE
                    || check.mem != AliasClass::None
                    || word_origin(func, witness.index_view)
                        != word_origin(func, proof.checked_index)
                    || word_origin(func, witness.length_view) != Some(proof.length)
                    || previous.op != Opcode::CheckBounds
                    || previous.eff != Effects::MAY_DEOPT
                    || previous.frame.is_none()
                    || previous.args.len() != 2
                    || word_origin(func, previous.args[0]) != word_origin(func, witness.floor.index)
                    || previous_load.op != Opcode::LoadVecLen
                    || previous_load.eff != Effects::READ_HEAP
                    || previous_load.mem != AliasClass::Unknown
                    || previous_load.args.len() != 1
                    || previous_load.result.and_then(|v| word_origin(func, v))
                        != word_origin(func, previous.args[1])
                    || word_origin(func, previous_load.args[0])
                        != word_origin(func, proof.guarded_base)
                    || previous_pos.block != read.block
                    || previous_load_pos.block != read.block
                    || previous_pos.epoch != read.epoch
                    || previous_load_pos.epoch != read.epoch
                    || !(previous_load_pos.ordinal < previous_pos.ordinal
                        && previous_pos.ordinal < length_pos.ordinal)
                    || !value_before(func, positions, dom, witness.index_view, proof.bounds)
                    || !value_before(func, positions, dom, witness.length_view, proof.bounds)
                    || !value_before(
                        func,
                        positions,
                        dom,
                        witness.floor.index,
                        witness.floor.guard,
                    )
                    || !index
                        .guards
                        .iter()
                        .all(|&guard| inst_before(positions, dom, guard, proof.bounds))
                    || !previous_index
                        .guards
                        .iter()
                        .all(|&guard| inst_before(positions, dom, guard, witness.floor.guard))
                    || witness.floor.floor > floor
                    || length.lo > witness.floor.floor
                    || length.hi != Range::FULL.hi
                    || index.range.lo < 0
                    || index.range.hi >= length.lo
                {
                    return Err(fail("current-owner-numeric-proof"));
                }
                true
            }
        };
        verified.insert(
            id,
            VerifiedPlainRead {
                base: proof.guarded_base,
                index: inst.args[1],
                bounds_elided: elided,
                epoch: read.epoch,
            },
        );
        length_reads.insert(proof.length_read);
        bounds.insert(proof.bounds);
    }
    Ok(VerifiedArrayReads {
        reads: verified,
        length_reads,
        bounds,
    })
}

#[cfg(test)]
#[path = "tests/array_reads.rs"]
mod tests;
