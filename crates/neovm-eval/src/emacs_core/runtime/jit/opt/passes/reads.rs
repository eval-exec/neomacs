//! Read reuse: a repeated car/cdr read of one cons word becomes a view of
//! the first read.
//!
//! `(when (< (cadr b) (car b)) (setcar b (cadr b)))` reads `(cadr b)` twice.
//! When nothing between the two reads can write a cons field, call Lisp,
//! poll or allocate, the second read returns the first one's word, so it
//! becomes a pure `Refine` of it. The first read's type facts (the
//! `CheckType` its comparison made) then cover the second use too: a store
//! of that word proves an immediate and drops the GEN1 barrier's tag test
//! (`compile::opt_store_facts`).
//!
//! Unlike GVN (`passes::gvn`, opt-in), availability ends at every
//! instruction that is not a guard, a pure operation or another such read,
//! and at every join and loop header. A reused word therefore never stays
//! live across a call, a store or a slow path, where a reload is cheaper
//! than keeping it in a register.
//!
//! Threading: pass state and results belong to one compilation; no runtime
//! cache, Lisp object or mutator state is read or retained.

use std::collections::HashMap;

use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    verify::VerifyError,
};

use super::cfg::Dominance;

/// Reads that became views, published as immutable compile metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReadStats {
    pub(crate) reuses: usize,
}

/// Transform a verified function, keeping every value, block, frame and
/// source position; a reused read keeps its id as a pure `Refine`.
pub(crate) fn run(func: &mut Func) -> Result<ReadStats, VerifyError> {
    func.verify()?;
    let dom = Dominance::new(func)?;
    let mut candidate = func.clone();
    let stats = transform(&mut candidate, &dom);
    candidate.verify()?;
    *func = candidate;
    Ok(stats)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Field {
    Car,
    Cdr,
}

impl Field {
    #[doc(hidden)]
    pub fn alias(self) -> AliasClass {
        match self {
            Self::Car => AliasClass::ConsCar,
            Self::Cdr => AliasClass::ConsCdr,
        }
    }
}

/// What a successful read proves about its base: a strict read (`car`)
/// signals on a non-list, so after it the base is a list and a lenient
/// read (`car-safe`) of it returns the same word; the converse fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Strictness {
    Strict,
    Lenient,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    base: Value,
    field: Field,
    strictness: Strictness,
}

/// Availability at a block's end, keyed by the base word's number.
type Available = HashMap<Key, Value>;

fn transform(func: &mut Func, dom: &Dominance) -> ReadStats {
    let mut numbers = alias_numbers(func);
    let mut at_end: Vec<Option<Available>> = vec![None; func.blocks.len()];
    let mut stats = ReadStats::default();
    for &block in dom.reverse_postorder() {
        let data = &func.blocks[block.index()];
        let mut preds = data.preds.clone();
        preds.retain(|&pred| dom.is_reachable(pred));
        preds.sort_unstable();
        preds.dedup();
        // Only a lone, already visited, non-back-edge predecessor hands its
        // reads down; joins and loop headers start empty.
        let inherited = match preds[..] {
            [pred] if data.loop_header.is_none() && !dom.dominates(block, pred) => {
                at_end[pred.index()].clone()
            }
            _ => None,
        };
        let mut available = inherited.unwrap_or_default();
        for position in 0..func.blocks[block.index()].insts.len() {
            let id = func.blocks[block.index()].insts[position];
            visit(func, id, &mut numbers, &mut available, &mut stats);
        }
        at_end[block.index()] = Some(available);
    }
    stats
}

fn visit(
    func: &mut Func,
    id: Inst,
    numbers: &mut [Value],
    available: &mut Available,
    stats: &mut ReadStats,
) {
    let inst = &func.insts[id.index()];
    if let Some((field, strictness)) = field_read(func, inst) {
        let result = inst.result.expect("a read has a result");
        let base = number(numbers, inst.args[0]);
        let key = Key {
            base,
            field,
            strictness,
        };
        if let Some(&leader) = available.get(&key) {
            replace(func, id, leader);
            numbers[result.index()] = number(numbers, leader);
            stats.reuses += 1;
            return;
        }
        available.insert(key, result);
        if strictness == Strictness::Strict {
            available
                .entry(Key {
                    strictness: Strictness::Lenient,
                    ..key
                })
                .or_insert(result);
        }
        return;
    }
    if !quiet(inst) {
        available.clear();
        return;
    }
    // A guard's or refinement's output is the same runtime word as its input.
    if let (Opcode::CheckType(_) | Opcode::Refine(_), &[input], Some(result)) =
        (&inst.op, &inst.args[..], inst.result)
    {
        let resolved = func.resolve(input).unwrap_or(input);
        let from = func.values[resolved.index()].rep;
        let to = func.values[result.index()].rep;
        if from == to || from.is_tagged() && to.is_tagged() {
            numbers[result.index()] = number(numbers, input);
        }
    }
}

/// A car or cdr read whose word and effects are exactly its bytecode's.
fn field_read(func: &Func, inst: &InstData) -> Option<(Field, Strictness)> {
    let (field, strictness, expected) = match &inst.op {
        Opcode::Opaque(op @ Op::Car) => (Field::Car, Strictness::Strict, op_effects(op)),
        Opcode::Opaque(op @ Op::CarSafe) => (Field::Car, Strictness::Lenient, op_effects(op)),
        Opcode::Opaque(op @ Op::Cdr) => (Field::Cdr, Strictness::Strict, op_effects(op)),
        Opcode::Opaque(op @ Op::CdrSafe) => (Field::Cdr, Strictness::Lenient, op_effects(op)),
        // The builder emits these only on a proven list or cons.
        Opcode::LoadCar => (Field::Car, Strictness::Strict, Effects::READ_HEAP),
        Opcode::LoadCdr => (Field::Cdr, Strictness::Strict, Effects::READ_HEAP),
        _ => return None,
    };
    let load_with_deopt = matches!(inst.op, Opcode::LoadCar | Opcode::LoadCdr)
        && inst.eff == Effects::READ_HEAP.with(Effects::MAY_DEOPT);
    let result = inst.result?;
    (inst.args.len() == 1
        && inst.mem == field.alias()
        && (inst.eff == expected || load_with_deopt)
        && func.values[result.index()].rep == Rep::Tagged)
        .then_some((field, strictness))
}

fn op_effects(op: &Op) -> Effects {
    super::super::build::op_effects(op).0
}

/// An instruction that writes no memory, calls nothing, allocates nothing
/// and never polls: a pure operation or a guard. Anything else ends every
/// availability.
fn quiet(inst: &InstData) -> bool {
    !matches!(inst.op, Opcode::Poll | Opcode::Call { .. }) && Effects::MAY_DEOPT.contains(inst.eff)
}

fn replace(func: &mut Func, id: Inst, leader: Value) {
    let inst = &mut func.insts[id.index()];
    let result = inst.result.expect("a reused read has a result");
    inst.op = Opcode::Refine(func.values[result.index()].ty);
    inst.args = vec![leader];
    inst.eff = Effects::PURE;
    inst.mem = AliasClass::None;
}

/// Each value's alias-chain leader, resolved once.
fn alias_numbers(func: &Func) -> Vec<Value> {
    (0..func.values.len())
        .map(|index| {
            let value = Value(index as u32);
            func.resolve(value).unwrap_or(value)
        })
        .collect()
}

fn number(numbers: &[Value], mut value: Value) -> Value {
    while numbers[value.index()] != value {
        value = numbers[value.index()];
    }
    value
}

#[cfg(test)]
#[path = "tests/reads_test.rs"]
mod tests;
