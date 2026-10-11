//! Sparse conditional type propagation for the independently gated opt front.
//!
//! Threading: this pass owns only invocation-local Rust facts and mutates one
//! compiler-owned Func. It never dereferences Lisp bits or caches mutator state.

use std::collections::HashMap;

use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::opt::{
    ir::{Block, Cmp, Edge, Func, InstData, Opcode, Rep, Term, Value, ValueBits, ValueDef},
    mem::{AliasClass, Effects},
    types::{TypeKind, TypeSet},
    verify::VerifyError,
};
use crate::tagged::value::{FIXNUM_CHECK_VALUE, FIXNUM_SHIFT};

use super::cfg::{Dominance, cleanup};

/// Pass counters; threading: one compilation owns this value until reporting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FoldStats {
    pub(crate) guards_folded: usize,
    pub(crate) guards_narrowed: usize,
    pub(crate) constants_folded: usize,
    pub(crate) branches_folded: usize,
    pub(crate) threaded_edges: usize,
    pub(crate) cons_loads: usize,
    pub(crate) deopts: usize,
    /// No facts are published if the bounded solver fails to converge.
    pub(crate) analysis_bailed: bool,
}

/// Validation work after transactional CFG cleanup. Threading: an immutable
/// compiler-local choice; it contains neither IR nor runtime state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CleanupValidation {
    ReuseCleanup,
    RepeatCleanup,
}
static_assertions::assert_impl_all!(CleanupValidation: Send, Sync);

/// Run only when the owning pipeline selected the fold pass. Calls, stores,
/// allocations and polls stay in their original order. No fact reads a Lisp
/// header, and no patched environment constant supplies an instance's type.
pub(crate) fn run(func: &mut Func) -> Result<FoldStats, VerifyError> {
    let validation = if super::super::pass_fast::enabled() {
        CleanupValidation::ReuseCleanup
    } else {
        CleanupValidation::RepeatCleanup
    };
    run_with_validation(func, validation)
}

fn run_with_validation(
    func: &mut Func,
    validation: CleanupValidation,
) -> Result<FoldStats, VerifyError> {
    func.verify()?;
    let Some(analysis) = analyze(func)? else {
        return Ok(FoldStats {
            analysis_bailed: true,
            ..FoldStats::default()
        });
    };
    let mut stats = FoldStats::default();
    // Appended constants are immediate words only. Prefix slots must never be
    // reused for Const, even when their template bits happen to match.
    let mut constants = func.consts.to_vec();
    let mut constant_indices = HashMap::new();
    for (index, &bits) in constants.iter().enumerate().skip(func.dynamic_prefix) {
        constant_indices.entry(bits).or_insert(index as u32);
    }
    for index in 0..func.blocks.len() {
        if !analysis.reachable[index] {
            continue;
        }
        let block = Block(index as u32);
        let mut proofs = HashMap::new();
        // A point proof is not a global type annotation on the original Arg.
        // Typed loads consume the actual dominating checked/refined identity.
        let mut proof_values = HashMap::new();
        let ids = func.blocks[index].insts.clone();
        for (position, id) in ids.into_iter().enumerate() {
            let old = func.insts[id.index()].clone();
            let argument = |n: usize| point_fact(&analysis, old.args[n], &proofs);
            if let Opcode::CheckType(target) = old.op {
                let input = argument(0);
                if !input.is_bottom() && input.meet(target).is_bottom() {
                    let frame = old.frame.ok_or(VerifyError::MissingFrame(id))?;
                    func.blocks[index].insts.truncate(position);
                    func.blocks[index].term = Term::Deopt(frame);
                    for (pc, state) in func.source_states.iter_mut().enumerate() {
                        if state.as_ref().is_some_and(|state| state.block == block) {
                            if pc > old.pc as usize {
                                *state = None;
                            } else if pc == old.pc as usize {
                                let state = state.as_mut().expect("matched state");
                                state.post = state.pre.clone();
                            }
                        }
                    }
                    stats.deopts += 1;
                    break;
                }
                let proven = input.meet(target);
                if !input.is_bottom() && input.is_subset(target) {
                    // A type fact at this program point does not globally
                    // narrow an original GNU Arg/phi identity. A numeric
                    // representation bridge needs an actual checked SSA view
                    // before its runtime guard may become a pure refinement.
                    let input_value = func.resolve(old.args[0]).expect("verified guard input");
                    let output_rep = old.result.map(|v| func.values[v.index()].rep);
                    let needs_fix_view = output_rep == Some(Rep::TaggedFix)
                        && func.values[input_value.index()].rep == Rep::Tagged
                        && (!func.values[input_value.index()]
                            .ty
                            .is_subset(TypeSet::FIXNUM)
                            || func.values[input_value.index()].ty.is_bottom());
                    let checked_view = needs_fix_view
                        .then(|| {
                            proof_values
                                .get(&analysis.origins[old.args[0].index()])
                                .copied()
                        })
                        .flatten()
                        .filter(|&v| {
                            let v = func.resolve(v).expect("verified earlier guard view");
                            let data = &func.values[v.index()];
                            data.rep.is_tagged()
                                && !data.ty.is_bottom()
                                && data.ty.is_subset(TypeSet::FIXNUM)
                        });
                    if !needs_fix_view || checked_view.is_some() {
                        let inst = &mut func.insts[id.index()];
                        if let Some(view) = checked_view {
                            inst.args[0] = view;
                        }
                        inst.op = Opcode::Refine(proven);
                        inst.eff = Effects::PURE;
                        inst.mem = AliasClass::None;
                        stats.guards_folded += 1;
                    }
                } else if target == TypeSet::LIST
                    && !input.contains(TypeKind::Nil)
                    && !input.is_bottom()
                {
                    func.insts[id.index()].op = Opcode::CheckType(TypeSet::CONS);
                    stats.guards_narrowed += 1;
                }
                if !proven.is_bottom() {
                    proofs.insert(analysis.origins[old.args[0].index()], proven);
                    if let Some(result) = old.result {
                        proof_values.insert(analysis.origins[old.args[0].index()], result);
                    }
                }
                continue;
            }
            if let Opcode::Refine(target) = old.op {
                let proven = argument(0).meet(target);
                if !proven.is_bottom() {
                    proofs.insert(analysis.origins[old.args[0].index()], proven);
                    if let Some(result) = old.result {
                        proof_values.insert(analysis.origins[old.args[0].index()], result);
                    }
                }
                continue;
            }
            if let Opcode::Opaque(ref op) = old.op {
                let load = match op {
                    Op::Car | Op::CarSafe => Some((Opcode::LoadCar, AliasClass::ConsCar)),
                    Op::Cdr | Op::CdrSafe => Some((Opcode::LoadCdr, AliasClass::ConsCdr)),
                    _ => None,
                };
                if let Some((op, alias)) = load {
                    let input = argument(0);
                    if !input.is_bottom() && input.is_subset(TypeSet::CONS) {
                        let globally_cons = |value: Value| {
                            let fact = analysis.facts[value.index()];
                            !fact.is_bottom() && fact.is_subset(TypeSet::CONS)
                        };
                        let checked = if globally_cons(old.args[0]) {
                            Some(old.args[0])
                        } else {
                            proof_values
                                .get(&analysis.origins[old.args[0].index()])
                                .copied()
                                .filter(|&value| globally_cons(value))
                        };
                        if let Some(checked) = checked {
                            let inst = &mut func.insts[id.index()];
                            inst.op = op;
                            inst.args[0] = checked;
                            inst.eff = Effects::READ_HEAP;
                            inst.mem = alias;
                            stats.cons_loads += 1;
                            continue;
                        }
                    }
                }
            }
            let fact = transfer(func, &old, |value| point_fact(&analysis, value, &proofs));
            let Some(result) = old.result else {
                continue;
            };
            let rep = func.values[result.index()].rep;
            let constant = safe_constant(fact);
            let eligible = match &old.op {
                Opcode::IsNonNil
                | Opcode::TypeTest(_)
                | Opcode::Eq
                | Opcode::BoolToLisp
                | Opcode::FixCmp(_) => true,
                Opcode::Opaque(op) => {
                    matches!(
                        op,
                        Op::Null
                            | Op::Not
                            | Op::Consp
                            | Op::Stringp
                            | Op::Listp
                            | Op::Eq
                            | Op::Eqlsign
                            | Op::Lss
                            | Op::Gtr
                            | Op::Leq
                            | Op::Geq
                    ) || matches!(op, Op::Car | Op::Cdr | Op::CarSafe | Op::CdrSafe)
                        && argument(0).is_subset(TypeSet::NIL)
                        && !argument(0).is_bottom()
                }
                _ => false,
            };
            if eligible && let Some(bits) = constant {
                let op = if rep == Rep::Bool {
                    Some(Opcode::BoolConst(bits.0 != 0))
                } else if rep.is_tagged() {
                    let next = constants.len() as u32;
                    let pool_index = *constant_indices.entry(bits).or_insert_with(|| {
                        constants.push(bits);
                        next
                    });
                    Some(Opcode::Const(pool_index))
                } else {
                    None
                };
                if let Some(op) = op {
                    let inst = &mut func.insts[id.index()];
                    inst.op = op;
                    inst.args.clear();
                    inst.eff = Effects::PURE;
                    inst.mem = AliasClass::None;
                    stats.constants_folded += 1;
                }
            } else if matches!(old.op, Opcode::Select)
                && let Some(flag) = truth(argument(0))
            {
                let selected = old.args[if flag { 1 } else { 2 }];
                let inst = &mut func.insts[id.index()];
                inst.op = Opcode::Refine(fact);
                inst.args = vec![selected];
                inst.eff = Effects::PURE;
                inst.mem = AliasClass::None;
                stats.constants_folded += 1;
            }
        }
        if let Term::Branch {
            flag,
            if_true,
            if_false,
        } = &func.blocks[index].term
        {
            if let Some(flag) = truth(point_fact(&analysis, *flag, &proofs)) {
                func.blocks[index].term = Term::Jump(if flag {
                    if_true.clone()
                } else {
                    if_false.clone()
                });
                stats.branches_folded += 1;
            }
        }
    }
    func.consts = constants.into_boxed_slice();
    // Global facts never include a same-block proof about an earlier operand.
    // Only the guarded result carries that narrower globally valid type.
    for (index, value) in func.values.iter_mut().enumerate() {
        let fact = analysis.facts[index];
        if !fact.is_bottom() {
            value.ty = value.ty.meet(fact);
        }
    }
    thread_edges(func, &analysis, &mut stats)?;
    cleanup(func)?;
    // Cleanup validates its complete remapped candidate before publication;
    // its metadata-preserving sink path also runs the full verifier. No IR
    // mutation occurs after that successful return. FAST keeps that result;
    // the original work schedule remains available with FAST=off.
    match validation {
        CleanupValidation::ReuseCleanup => {}
        CleanupValidation::RepeatCleanup => func.verify()?,
    }
    Ok(stats)
}

#[cfg(test)]
#[path = "tests/fold_validation.rs"]
mod validation_tests;

/// Invocation-local fixed-point facts. An edge slot distinguishes even two
/// switch edges with the same destination and different phi arguments.
/// Threading: exclusively owned scratch; no Lisp references or global cache.
struct Analysis {
    facts: Vec<TypeSet>,
    reachable: Vec<bool>,
    executable: Vec<Vec<bool>>,
    origins: Vec<Value>,
}

fn edge_at(term: &Term, index: usize) -> &Edge {
    match term {
        Term::Jump(edge) => edge,
        Term::Branch {
            if_true, if_false, ..
        } => {
            if index == 0 {
                if_true
            } else {
                if_false
            }
        }
        Term::Switch { cases, default, .. } => cases.get(index).map_or(default, |case| &case.edge),
        _ => unreachable!("recorded edge belongs to an edge terminator"),
    }
}

fn origins(func: &Func) -> Vec<Value> {
    let mut origins = vec![None; func.values.len()];
    let mut path = Vec::new();
    for index in 0..func.values.len() {
        path.clear();
        let mut value = Value(index as u32);
        let origin = loop {
            if let Some(origin) = origins[value.index()] {
                break origin;
            }
            path.push(value);
            value = match func.values[value.index()].def {
                ValueDef::Alias(next) => next,
                ValueDef::Inst(inst)
                    if matches!(
                        func.insts[inst.index()].op,
                        Opcode::Refine(_) | Opcode::CheckType(_)
                    ) =>
                {
                    func.insts[inst.index()].args[0]
                }
                _ => break value,
            };
        };
        for &value in &path {
            origins[value.index()] = Some(origin);
        }
    }
    origins
        .into_iter()
        .map(|origin| origin.expect("completed origin"))
        .collect()
}

fn point_fact(analysis: &Analysis, value: Value, proofs: &HashMap<Value, TypeSet>) -> TypeSet {
    let fact = analysis.facts[value.index()];
    proofs
        .get(&analysis.origins[value.index()])
        .map_or(fact, |proof| fact.meet(*proof))
}

fn analyze(func: &Func) -> Result<Option<Analysis>, VerifyError> {
    let dom = Dominance::new(func)?;
    let mut incoming = vec![Vec::new(); func.blocks.len()];
    let mut executable = Vec::with_capacity(func.blocks.len());
    for (index, block) in func.blocks.iter().enumerate() {
        let edges = block.term.edges();
        executable.push(vec![false; edges.len()]);
        for (edge_index, edge) in edges.into_iter().enumerate() {
            incoming[edge.target.index()].push((Block(index as u32), edge_index));
        }
    }
    let mut analysis = Analysis {
        facts: vec![TypeSet::BOTTOM; func.values.len()],
        reachable: vec![false; func.blocks.len()],
        executable,
        origins: origins(func),
    };
    analysis.reachable[func.entry.index()] = true;
    // Reverse postorder handles long acyclic bodies in a single scan. Loop
    // intervals widen at headers; a separate scan cap bounds iterations.
    let units = func
        .insts
        .len()
        .saturating_add(func.values.len())
        .saturating_add(func.blocks.len())
        .max(1);
    let scans = (1_000_000 / units).clamp(2, 64);
    for _ in 0..scans {
        let mut changed = false;
        for &block in dom.reverse_postorder() {
            let index = block.index();
            if !analysis.reachable[index] {
                continue;
            }
            let data = &func.blocks[index];
            for (position, &param) in data.params.iter().enumerate() {
                let mut fact = TypeSet::BOTTOM;
                for &(source, edge_index) in &incoming[index] {
                    if analysis.executable[source.index()][edge_index] {
                        let arg =
                            edge_at(&func.blocks[source.index()].term, edge_index).args[position];
                        fact = fact.join(analysis.facts[arg.index()]);
                    }
                }
                fact = fact.meet(func.values[param.index()].ty);
                let previous = analysis.facts[param.index()];
                let next = if data.loop_header.is_some() {
                    previous.widen(fact)
                } else {
                    previous.join(fact)
                };
                if next != previous {
                    analysis.facts[param.index()] = next;
                    changed = true;
                }
            }
            let mut proofs = HashMap::new();
            let mut stopped = false;
            for &id in &data.insts {
                let inst = &func.insts[id.index()];
                let fact = transfer(func, inst, |value| point_fact(&analysis, value, &proofs));
                if let Some(result) = inst.result {
                    let previous = analysis.facts[result.index()];
                    let next = previous.join(fact);
                    if next != previous {
                        analysis.facts[result.index()] = next;
                        changed = true;
                    }
                }
                if let Opcode::CheckType(target) | Opcode::Refine(target) = inst.op {
                    let input = point_fact(&analysis, inst.args[0], &proofs);
                    let proven = input.meet(target);
                    if proven.is_bottom() {
                        if matches!(inst.op, Opcode::CheckType(_)) {
                            stopped = true;
                            break;
                        }
                        continue;
                    }
                    proofs.insert(analysis.origins[inst.args[0].index()], proven);
                }
            }
            if stopped {
                continue;
            }
            let selected = match data.term {
                Term::Branch { flag, .. } => {
                    let fact = point_fact(&analysis, flag, &proofs);
                    if fact.is_bottom() {
                        continue;
                    }
                    truth(fact).map(|flag| if flag { 0 } else { 1 })
                }
                _ => None,
            };
            for (edge_index, edge) in data.term.edges().into_iter().enumerate() {
                if selected.is_some_and(|selected| selected != edge_index) {
                    continue;
                }
                if !analysis.executable[index][edge_index] {
                    analysis.executable[index][edge_index] = true;
                    changed = true;
                }
                if !analysis.reachable[edge.target.index()] {
                    analysis.reachable[edge.target.index()] = true;
                    changed = true;
                }
            }
        }
        // Existing trivial-phi aliases participate in the same monotone facts.
        for (index, value) in func.values.iter().enumerate() {
            if let ValueDef::Alias(next) = value.def {
                let fact = analysis.facts[index].join(analysis.facts[next.index()]);
                if fact != analysis.facts[index] {
                    analysis.facts[index] = fact;
                    changed = true;
                }
            }
        }
        if !changed {
            return Ok(Some(analysis));
        }
    }
    Ok(None)
}

fn truth(fact: TypeSet) -> Option<bool> {
    if fact.is_bottom() {
        None
    } else if fact.is_subset(TypeSet::NIL) {
        Some(false)
    } else if !fact.contains(TypeKind::Nil) {
        Some(true)
    } else {
        None
    }
}

fn boolean(value: Option<bool>) -> TypeSet {
    match value {
        Some(true) => TypeSet::T,
        Some(false) => TypeSet::NIL,
        None => TypeSet::BOOLEAN,
    }
}

fn type_test(input: TypeSet, target: TypeSet) -> TypeSet {
    if input.is_bottom() {
        TypeSet::BOTTOM
    } else {
        boolean(if input.is_subset(target) {
            Some(true)
        } else if input.meet(target).is_bottom() {
            Some(false)
        } else {
            None
        })
    }
}

fn safe_constant(fact: TypeSet) -> Option<ValueBits> {
    if fact.is_bottom() {
        return None;
    }
    if fact.is_subset(TypeSet::FIXNUM) {
        let range = fact.range()?;
        return (range.lo == range.hi).then_some(ValueBits(
            ((range.lo as u64) << FIXNUM_SHIFT) | FIXNUM_CHECK_VALUE as u64,
        ));
    }
    if fact.is_subset(TypeSet::NIL) {
        Some(ValueBits(0))
    } else if fact.is_subset(TypeSet::T) {
        Some(ValueBits(8))
    } else {
        None
    }
}

fn fix_constant(fact: TypeSet) -> Option<i64> {
    if !fact.is_bottom() && fact.is_subset(TypeSet::FIXNUM) {
        let range = fact.range()?;
        (range.lo == range.hi).then_some(range.lo)
    } else {
        None
    }
}

fn compare(cmp: Cmp, a: i64, b: i64) -> bool {
    match cmp {
        Cmp::Eq => a == b,
        Cmp::Ne => a != b,
        Cmp::Lt => a < b,
        Cmp::Le => a <= b,
        Cmp::Gt => a > b,
        Cmp::Ge => a >= b,
    }
}

fn transfer(func: &Func, inst: &InstData, fact: impl Fn(Value) -> TypeSet) -> TypeSet {
    // Even a future precise annotation on the template slot cannot describe
    // all instances of a patched constant prefix.
    if matches!(inst.op, Opcode::EnvConst(_)) {
        return TypeSet::TOP;
    }
    let declared = inst
        .result
        .map_or(TypeSet::BOTTOM, |result| func.values[result.index()].ty);
    let arg = |index: usize| fact(inst.args[index]);
    let compared = |cmp| {
        if arg(0).is_bottom() || arg(1).is_bottom() {
            TypeSet::BOTTOM
        } else {
            boolean(
                fix_constant(arg(0))
                    .zip(fix_constant(arg(1)))
                    .map(|(a, b)| compare(cmp, a, b)),
            )
        }
    };
    let identity = || {
        if arg(0).is_bottom() || arg(1).is_bottom() {
            TypeSet::BOTTOM
        } else {
            boolean(
                safe_constant(arg(0))
                    .zip(safe_constant(arg(1)))
                    .map(|(a, b)| a == b),
            )
        }
    };
    let inferred = match &inst.op {
        Opcode::Const(index) => TypeSet::for_constant(func.consts[*index as usize]),
        Opcode::EnvConst(_) => TypeSet::TOP,
        Opcode::BoolConst(value) => boolean(Some(*value)),
        Opcode::CheckType(target) | Opcode::Refine(target) => arg(0).meet(*target),
        Opcode::TagFix | Opcode::UntagFix | Opcode::BoolToLisp => arg(0),
        Opcode::IsNonNil => {
            if arg(0).is_bottom() {
                TypeSet::BOTTOM
            } else {
                boolean(truth(arg(0)))
            }
        }
        Opcode::TypeTest(target) => type_test(arg(0), *target),
        Opcode::Eq => identity(),
        Opcode::FixCmp(cmp) => compared(*cmp),
        Opcode::Select => {
            if arg(0).is_bottom() {
                TypeSet::BOTTOM
            } else if let Some(flag) = truth(arg(0)) {
                arg(if flag { 1 } else { 2 })
            } else {
                arg(1).join(arg(2))
            }
        }
        Opcode::Opaque(op) => match op {
            Op::Null | Op::Not => type_test(arg(0), TypeSet::NIL),
            Op::Consp => type_test(arg(0), TypeSet::CONS),
            Op::Stringp => type_test(arg(0), TypeSet::STRING),
            Op::Listp => type_test(arg(0), TypeSet::LIST),
            Op::Eq => identity(),
            Op::Eqlsign => compared(Cmp::Eq),
            Op::Lss => compared(Cmp::Lt),
            Op::Gtr => compared(Cmp::Gt),
            Op::Leq => compared(Cmp::Le),
            Op::Geq => compared(Cmp::Ge),
            Op::Car | Op::Cdr | Op::CarSafe | Op::CdrSafe
                if !arg(0).is_bottom() && arg(0).is_subset(TypeSet::NIL) =>
            {
                TypeSet::NIL
            }
            // Symbol-with-position predicates and generic arithmetic remain
            // opaque. Neither headers nor dynamic bindings are worker facts.
            _ => declared,
        },
        _ => declared,
    };
    inferred.meet(declared)
}

/// Thread only acyclic, effect-free branch blocks. Full GNU stacks sometimes
/// forward a local phi directly into a later frame without an outgoing phi;
/// such escaped local definitions prohibit bypassing that block.
/// Threading: all owner/use maps are exclusive invocation-local scratch.
fn thread_edges(
    func: &mut Func,
    analysis: &Analysis,
    stats: &mut FoldStats,
) -> Result<(), VerifyError> {
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
    let mut escaped = vec![false; func.values.len()];
    let mut mark = |owner: Block, value: Value| {
        let value = func.resolve(value).expect("verified alias");
        escaped[value.index()] |= owners[value.index()] != Some(owner);
    };
    let mut frame =
        |owner: Block, mut current: Option<crate::emacs_core::jit::opt::ir::FrameId>| {
            while let Some(id) = current {
                let state = &func.frames[id.index()];
                for &value in &state.stack {
                    mark(owner, value);
                }
                current = state.parent;
            }
        };
    // Frames are marked separately so the two closures do not hold overlapping
    // mutable borrows of the escape map during instruction/term scanning.
    for (index, block) in func.blocks.iter().enumerate() {
        let owner = Block(index as u32);
        for &id in &block.insts {
            frame(owner, func.insts[id.index()].frame);
        }
        if let Term::Deopt(id) = block.term {
            frame(owner, Some(id));
        }
    }
    for source in func.source_states.iter().flatten() {
        frame(source.block, Some(source.frame));
    }
    drop(frame);
    for (index, block) in func.blocks.iter().enumerate() {
        let owner = Block(index as u32);
        for &value in func
            .entry_stacks
            .get(index)
            .map_or(&[][..], |stack| &stack[..])
        {
            mark(owner, value);
        }
        for &id in &block.insts {
            for &value in &func.insts[id.index()].args {
                mark(owner, value);
            }
        }
        for edge in block.term.edges() {
            for &value in &edge.args {
                mark(owner, value);
            }
        }
        match block.term {
            Term::Branch { flag, .. } | Term::Return(flag) => mark(owner, flag),
            Term::Switch { value, table, .. } => {
                mark(owner, value);
                mark(owner, table);
            }
            _ => {}
        }
    }
    for source in func.source_states.iter().flatten() {
        for &value in source.pre.iter().chain(source.post.iter()) {
            mark(source.block, value);
        }
    }
    drop(mark);
    let dom = Dominance::new(func)?;
    let mut candidates = vec![false; func.blocks.len()];
    for (index, block) in func.blocks.iter().enumerate() {
        candidates[index] = Block(index as u32) != func.entry
            && block.loop_header.is_none()
            && matches!(block.term, Term::Branch { .. })
            && block.params.iter().all(|value| !escaped[value.index()])
            && block.insts.iter().all(|id| {
                let inst = &func.insts[id.index()];
                inst.eff == Effects::PURE
                    && inst.mem == AliasClass::None
                    && inst.result.is_none_or(|value| !escaped[value.index()])
                    && matches!(
                        inst.op,
                        Opcode::Const(_)
                            | Opcode::BoolConst(_)
                            | Opcode::Refine(_)
                            | Opcode::IsNonNil
                            | Opcode::BoolToLisp
                            | Opcode::TypeTest(_)
                            | Opcode::Eq
                    )
            });
    }
    // At most one bypass per original edge prevents arbitrarily long chains
    // and never removes an observable loop, poll, guard or effectful operation.
    let mut rewrites = Vec::new();
    for (index, block) in func.blocks.iter().enumerate() {
        let source = Block(index as u32);
        for (edge_index, incoming) in block.term.edges().into_iter().enumerate() {
            let target = incoming.target;
            if target == source || !candidates[target.index()] || !dom.is_reachable(source) {
                continue;
            }
            let body = &func.blocks[target.index()];
            let mut local = HashMap::new();
            let mut mapped = HashMap::new();
            for (&param, &arg) in body.params.iter().zip(&incoming.args) {
                local.insert(param, analysis.facts[arg.index()]);
                mapped.insert(param, arg);
            }
            for &id in &body.insts {
                let inst = &func.insts[id.index()];
                let fact = transfer(func, inst, |value| {
                    let value = func.resolve(value).expect("verified alias");
                    local
                        .get(&value)
                        .copied()
                        .unwrap_or(analysis.facts[value.index()])
                });
                if let Some(result) = inst.result {
                    local.insert(result, fact);
                    if matches!(inst.op, Opcode::Refine(_)) {
                        let arg = func.resolve(inst.args[0]).expect("verified alias");
                        if let Some(&arg) = mapped.get(&arg) {
                            mapped.insert(result, arg);
                        } else if owners[arg.index()] != Some(target) {
                            mapped.insert(result, arg);
                        }
                    }
                }
            }
            let Term::Branch {
                flag,
                if_true,
                if_false,
            } = &body.term
            else {
                unreachable!()
            };
            let fact = local
                .get(flag)
                .copied()
                .unwrap_or(analysis.facts[flag.index()]);
            let Some(flag) = truth(fact) else {
                continue;
            };
            let selected = if flag { if_true } else { if_false };
            if selected.target == target || dom.dominates(selected.target, source) {
                continue;
            }
            let mut args = Vec::with_capacity(selected.args.len());
            let mut valid = true;
            for (&arg, &param) in selected
                .args
                .iter()
                .zip(&func.blocks[selected.target.index()].params)
            {
                let arg = func.resolve(arg).expect("verified alias");
                let replacement = if owners[arg.index()] == Some(target) {
                    mapped.get(&arg).copied()
                } else {
                    Some(arg)
                };
                let Some(replacement) = replacement else {
                    valid = false;
                    break;
                };
                let replacement = func.resolve(replacement).expect("verified alias");
                let Some(owner) = owners[replacement.index()] else {
                    valid = false;
                    break;
                };
                let data = &func.values[replacement.index()];
                let parameter = &func.values[param.index()];
                if !dom.dominates(owner, source)
                    || !data.ty.is_subset(parameter.ty)
                    || !(data.rep == parameter.rep
                        || data.rep.is_tagged() && parameter.rep.is_tagged())
                {
                    valid = false;
                    break;
                }
                args.push(replacement);
            }
            if valid {
                rewrites.push((
                    source,
                    edge_index,
                    Edge {
                        target: selected.target,
                        args,
                    },
                ));
            }
        }
    }
    for (source, index, edge) in rewrites {
        let term = &mut func.blocks[source.index()].term;
        match term {
            Term::Jump(current) => *current = edge,
            Term::Branch {
                if_true, if_false, ..
            } => {
                if index == 0 {
                    *if_true = edge;
                } else {
                    *if_false = edge;
                }
            }
            Term::Switch { cases, default, .. } => {
                if let Some(case) = cases.get_mut(index) {
                    case.edge = edge;
                } else {
                    *default = edge;
                }
            }
            _ => unreachable!("threaded edge owner retains its terminator"),
        }
        stats.threaded_edges += 1;
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/fold_test.rs"]
mod tests;
