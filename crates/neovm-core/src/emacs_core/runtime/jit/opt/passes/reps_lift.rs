//! Expose fixnum arithmetic to the opt pipeline before folding/selecting reps.
//!
//! Threading: the caller exclusively owns the function and supplies an immutable
//! per-PC feedback snapshot. This pass never reads a mutator, dereferences Lisp
//! heap bits, or stores Lisp state in a process/thread cache.

use std::collections::{HashMap, VecDeque};

use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::value::Value as LispValue;

use super::super::{
    ir::*,
    mem::{AliasClass, Effects},
    types::TypeSet,
    verify::VerifyError,
};

/// Compiler-owned lifting diagnostics; published with the completed plan only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LiftStats {
    pub lifted_arithmetic: usize,
    pub lifted_comparisons: usize,
    pub type_guards: usize,
}

/// The caller invokes this only for `reps`. Verification precedes publication:
/// no failed candidate changes the source function or its observation IDs.
pub(crate) fn run(func: &mut Func, feedback: &[NumericFeedback]) -> Result<LiftStats, VerifyError> {
    func.verify()?;
    if !func
        .insts
        .iter()
        .any(|data| selected(data, feedback).is_some())
    {
        return Ok(LiftStats::default());
    }
    let mut candidate = func.clone();
    let mut canonical = canonical_values(&candidate);
    let mut literals = Literals::new(&candidate);
    let mut stats = LiftStats::default();
    for block in 0..candidate.blocks.len() {
        let old = std::mem::take(&mut candidate.blocks[block].insts);
        let mut ordered = Vec::with_capacity(old.len());
        for inst in old {
            let original = candidate.insts[inst.index()].clone();
            let Some(kind) = selected(&original, feedback) else {
                ordered.push(inst);
                continue;
            };
            if original.args.len() != kind.arity() {
                return Err(VerifyError::OperandArity(inst));
            }
            // A known non-fixnum has no successful TaggedFix view. Leave its
            // original error/slow path intact instead of inventing a bottom
            // representation that could later be mistaken for an untag proof.
            if original.args.iter().any(|value| {
                candidate.values[canonical[value.index()].index()]
                    .ty
                    .meet(TypeSet::FIXNUM)
                    .is_bottom()
            }) {
                ordered.push(inst);
                continue;
            }
            let result = original.result.ok_or(VerifyError::OperandArity(inst))?;
            let mut operands = original.args.clone();
            // Preserve the baseline numeric guard order, RHS before LHS.
            // Both guards replay the same original pre-operation GNU frame.
            for index in (0..operands.len()).rev() {
                operands[index] = checked_view(
                    &mut candidate,
                    &mut canonical,
                    &mut ordered,
                    operands[index],
                    &original,
                    &mut stats,
                );
            }
            if let LiftOp::Add1 | LiftOp::Sub1 | LiftOp::Negate = kind {
                let negate = matches!(kind, LiftOp::Negate);
                let literal = literals.emit(
                    &mut candidate,
                    &mut canonical,
                    &mut ordered,
                    usize::from(!negate),
                    original.pc,
                );
                if negate {
                    operands.insert(0, literal);
                } else {
                    operands.push(literal);
                }
            }
            if let LiftOp::Compare(relation) = kind {
                match candidate.values[result.index()].rep {
                    Rep::Bool => {
                        let data = &mut candidate.insts[inst.index()];
                        data.op = Opcode::FixCmp(relation);
                        data.args = operands;
                        data.eff = Effects::PURE;
                        data.mem = AliasClass::None;
                    }
                    Rep::Tagged => {
                        let flag = emit(
                            &mut candidate,
                            &mut canonical,
                            &mut ordered,
                            InstData {
                                op: Opcode::FixCmp(relation),
                                args: operands,
                                result: None,
                                eff: Effects::PURE,
                                mem: AliasClass::None,
                                frame: original.frame,
                                pc: original.pc,
                            },
                            TypeSet::BOOLEAN,
                            Rep::Bool,
                        );
                        let data = &mut candidate.insts[inst.index()];
                        data.op = Opcode::BoolToLisp;
                        data.args = vec![flag];
                        data.eff = Effects::PURE;
                        data.mem = AliasClass::None;
                    }
                    _ => {
                        return Err(VerifyError::RepMismatch {
                            inst: Some(inst),
                            value: result,
                        });
                    }
                }
                stats.lifted_comparisons += 1;
            } else {
                let data = &mut candidate.insts[inst.index()];
                data.op = match kind {
                    LiftOp::Add | LiftOp::Add1 => Opcode::FixAdd { checked: true },
                    LiftOp::Sub | LiftOp::Sub1 | LiftOp::Negate => Opcode::FixSub { checked: true },
                    LiftOp::Mul => Opcode::FixMul { checked: true },
                    LiftOp::Compare(_) => unreachable!(),
                };
                data.args = operands;
                data.eff = Effects::MAY_DEOPT;
                data.mem = AliasClass::None;
                candidate.values[result.index()].ty = TypeSet::FIXNUM;
                candidate.values[result.index()].rep = Rep::TaggedFix;
                stats.lifted_arithmetic += 1;
            }
            ordered.push(inst);
        }
        candidate.blocks[block].insts = ordered;
    }
    candidate.consts = literals.bits.into_boxed_slice();
    narrow_views(&mut candidate, &canonical);
    publication_views(&mut candidate, &mut canonical);
    // Grounded closed phis need a verifier-visible successful type before
    // folding can replace their numeric guards with pure checked views. This
    // compiler-local proof does not select representations or change any SSA
    // identity, source state, effect, or frame. Unknown entry values and
    // seedless dependency cycles are excluded by the shared proof analysis.
    let grounded = super::reps::grounded_fixnums(&candidate);
    for (data, proved) in candidate.values.iter_mut().zip(grounded) {
        if proved {
            data.ty = data.ty.meet(TypeSet::FIXNUM);
        }
    }
    narrow_views(&mut candidate, &canonical);
    candidate.verify()?;
    *func = candidate;
    Ok(stats)
}

/// Preserve the exact Tagged publication contract after all numeric producers
/// have been lifted, independent of block-ID order. Views are pure same-word
/// identities; original results and observation metadata stay intact. Threading:
/// caches belong to this compilation and never contain mutator or heap state.
fn publication_views(func: &mut Func, canonical: &mut Vec<Value>) {
    if !func.insts.iter().any(|data| {
        data.op == Opcode::PublishRoot
            && func.values[canonical[data.args[0].index()].index()].rep == Rep::TaggedFix
    }) {
        return;
    }
    for block in 0..func.blocks.len() {
        let old = std::mem::take(&mut func.blocks[block].insts);
        let mut ordered = Vec::with_capacity(old.len());
        let mut views = HashMap::new();
        for inst in old {
            if func.insts[inst.index()].op == Opcode::PublishRoot {
                let input = canonical[func.insts[inst.index()].args[0].index()];
                if func.values[input.index()].rep == Rep::TaggedFix {
                    let view = if let Some(&view) = views.get(&input) {
                        view
                    } else {
                        let ty = func.values[input.index()].ty;
                        let pc = func.insts[inst.index()].pc;
                        let view = emit(
                            func,
                            canonical,
                            &mut ordered,
                            InstData {
                                op: Opcode::Refine(TypeSet::FIXNUM),
                                args: vec![input],
                                result: None,
                                eff: Effects::PURE,
                                mem: AliasClass::None,
                                frame: None,
                                pc,
                            },
                            ty,
                            Rep::Tagged,
                        );
                        views.insert(input, view);
                        view
                    };
                    func.insts[inst.index()].args[0] = view;
                }
            }
            ordered.push(inst);
        }
        func.blocks[block].insts = ordered;
    }
}

#[derive(Clone, Copy)]
enum LiftOp {
    Add,
    Sub,
    Mul,
    Add1,
    Sub1,
    Negate,
    Compare(Cmp),
}
impl LiftOp {
    fn arity(self) -> usize {
        match self {
            Self::Add1 | Self::Sub1 | Self::Negate => 1,
            _ => 2,
        }
    }
}

fn selected(data: &InstData, feedback: &[NumericFeedback]) -> Option<LiftOp> {
    if feedback
        .get(data.pc as usize)
        .copied()
        .unwrap_or(NumericFeedback::FixnumOnly)
        != NumericFeedback::FixnumOnly
    {
        return None;
    }
    let (Opcode::Opaque(op) | Opcode::OpaqueBool(op)) = &data.op else {
        return None;
    };
    Some(match op {
        Op::Add => LiftOp::Add,
        Op::Sub => LiftOp::Sub,
        Op::Mul => LiftOp::Mul,
        Op::Add1 => LiftOp::Add1,
        Op::Sub1 => LiftOp::Sub1,
        Op::Negate => LiftOp::Negate,
        Op::Eqlsign => LiftOp::Compare(Cmp::Eq),
        Op::Lss => LiftOp::Compare(Cmp::Lt),
        Op::Gtr => LiftOp::Compare(Cmp::Gt),
        Op::Leq => LiftOp::Compare(Cmp::Le),
        Op::Geq => LiftOp::Compare(Cmp::Ge),
        _ => return None,
    })
}

fn emit(
    func: &mut Func,
    canonical: &mut Vec<Value>,
    ordered: &mut Vec<Inst>,
    mut data: InstData,
    ty: TypeSet,
    rep: Rep,
) -> Value {
    let inst = Inst(func.insts.len() as u32);
    let value = Value(func.values.len() as u32);
    data.result = Some(value);
    func.insts.push(data);
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Inst(inst),
    });
    canonical.push(value);
    ordered.push(inst);
    value
}

fn checked_view(
    func: &mut Func,
    canonical: &mut Vec<Value>,
    ordered: &mut Vec<Inst>,
    input: Value,
    original: &InstData,
    stats: &mut LiftStats,
) -> Value {
    let data = &func.values[canonical[input.index()].index()];
    if data.rep == Rep::TaggedFix {
        return input;
    }
    let ty = data.ty.meet(TypeSet::FIXNUM);
    let proved = !data.ty.is_bottom() && data.ty.is_subset(TypeSet::FIXNUM);
    if !proved {
        stats.type_guards += 1;
    }
    emit(
        func,
        canonical,
        ordered,
        InstData {
            op: if proved {
                Opcode::Refine(TypeSet::FIXNUM)
            } else {
                Opcode::CheckType(TypeSet::FIXNUM)
            },
            args: vec![input],
            result: None,
            eff: if proved {
                Effects::PURE
            } else {
                Effects::MAY_DEOPT
            },
            mem: AliasClass::None,
            frame: original.frame,
            pc: original.pc,
        },
        ty,
        Rep::TaggedFix,
    )
}

/// Only immediate zero/one pool indices are cached. Threading: compiler-local
/// scratch; dynamic-prefix slots are never treated as immutable literal seeds.
struct Literals {
    bits: Vec<ValueBits>,
    indices: [Option<u32>; 2],
}
impl Literals {
    fn new(func: &Func) -> Self {
        let mut indices = [None; 2];
        let words = [
            ValueBits::from_value(LispValue::fixnum(0)),
            ValueBits::from_value(LispValue::fixnum(1)),
        ];
        for (index, bits) in func.consts.iter().enumerate().skip(func.dynamic_prefix) {
            for number in 0..2 {
                if *bits == words[number] && indices[number].is_none() {
                    indices[number] = Some(index as u32);
                }
            }
            if indices.iter().all(Option::is_some) {
                break;
            }
        }
        Self {
            bits: func.consts.to_vec(),
            indices,
        }
    }

    fn emit(
        &mut self,
        func: &mut Func,
        canonical: &mut Vec<Value>,
        ordered: &mut Vec<Inst>,
        number: usize,
        pc: u32,
    ) -> Value {
        let bits = ValueBits::from_value(LispValue::fixnum(number as i64));
        let index = *self.indices[number].get_or_insert_with(|| {
            let index = self.bits.len() as u32;
            self.bits.push(bits);
            index
        });
        // A fresh definition at the use is dominance-safe across sibling
        // blocks; sharing the opaque pool word needs no shared SSA definition.
        emit(
            func,
            canonical,
            ordered,
            InstData {
                op: Opcode::Const(index),
                args: Vec::new(),
                result: None,
                eff: Effects::PURE,
                mem: AliasClass::None,
                frame: None,
                pc,
            },
            TypeSet::for_constant(bits),
            Rep::TaggedFix,
        )
    }
}

/// Path compression computes the immutable alias map in O(values) storage and
/// O(values + alias links) work. Threading: invocation-local compiler scratch.
fn canonical_values(func: &Func) -> Vec<Value> {
    let mut canonical = vec![None; func.values.len()];
    let mut path = Vec::new();
    for index in 0..func.values.len() {
        let mut value = Value(index as u32);
        path.clear();
        let resolved = loop {
            if let Some(resolved) = canonical[value.index()] {
                break resolved;
            }
            path.push(value);
            if let ValueDef::Alias(next) = func.values[value.index()].def {
                value = next;
            } else {
                break value;
            }
        };
        for value in &path {
            canonical[value.index()] = Some(resolved);
        }
    }
    canonical.into_iter().map(Option::unwrap).collect()
}

/// Reannotate only dependent checked/refined declarations; their original SSA
/// handles, reps, instructions, effects and frames stay intact. Each shrinking
/// fact schedules its dependent views instead of rescanning the whole Func.
fn narrow_views(func: &mut Func, canonical: &[Value]) {
    let mut users = vec![Vec::new(); func.values.len()];
    let mut work = VecDeque::new();
    let mut queued = vec![false; func.insts.len()];
    for (index, data) in func.insts.iter().enumerate() {
        if matches!(data.op, Opcode::CheckType(_) | Opcode::Refine(_)) {
            users[canonical[data.args[0].index()].index()].push(index);
            work.push_back(index);
            queued[index] = true;
        }
    }
    while let Some(index) = work.pop_front() {
        queued[index] = false;
        let data = &func.insts[index];
        let (Opcode::CheckType(target) | Opcode::Refine(target)) = data.op else {
            unreachable!()
        };
        let input = canonical[data.args[0].index()];
        let result = data.result.expect("verified checked/refined result");
        let narrowed = func.values[result.index()]
            .ty
            .meet(func.values[input.index()].ty.meet(target));
        if narrowed != func.values[result.index()].ty {
            func.values[result.index()].ty = narrowed;
            for &user in &users[result.index()] {
                if !std::mem::replace(&mut queued[user], true) {
                    work.push_back(user);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/reps_lift.rs"]
mod tests;
