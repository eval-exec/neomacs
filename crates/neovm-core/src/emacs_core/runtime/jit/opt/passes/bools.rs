//! Closed Boolean SSA webs, retaining original GNU observation identities.
//!
//! Threading: analysis and rewrites belong to one compilation. Opaque constant
//! bits are inspected only for the immediate T/NIL words; no Lisp state is
//! dereferenced, cached, or transferred to another compiler thread.

use std::collections::{HashMap, VecDeque};

use crate::emacs_core::value::Value as LispValue;

use crate::emacs_core::jit::opt::{
    ir::{Func, Inst, InstData, Opcode, Rep, Term, Value, ValueData, ValueDef, opaque_bool_arity},
    mem::{AliasClass, Effects},
    types::TypeSet,
    verify::VerifyError,
};

/// Immutable pass results; threading: owned by one compiler until reporting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BoolStats {
    pub(crate) opaque_producers: usize,
    pub(crate) constant_producers: usize,
    pub(crate) phi_params: usize,
    pub(crate) refinements: usize,
    pub(crate) selects: usize,
    pub(crate) nil_tests: usize,
    pub(crate) tagged_views: usize,
}

/// The pipeline invokes this only when Boolean representations are selected.
/// Verification is transactional: a rejected candidate never changes the
/// original plan. No instruction, observation identity or pinned effect is
/// removed; semantic Lisp views are inserted at actual tagged consumers.
pub(crate) fn run(func: &mut Func) -> Result<BoolStats, VerifyError> {
    func.verify()?;
    let canonical = canonical_values(func);
    let analysis = analyze(func, &canonical);
    let mut candidate = func.clone();
    let stats = rewrite(&mut candidate, &canonical, &analysis);
    candidate.verify()?;
    *func = candidate;
    Ok(stats)
}

/// Only proved producers ground dependent webs. Threading: invocation-local
/// classification and edge lists, containing SSA handles rather than objects.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Node {
    Unknown,
    Existing,
    Constant(bool),
    Opaque,
    Phi,
    Refine,
    Select,
    Projection,
}

impl Node {
    fn seed(self) -> bool {
        matches!(self, Self::Existing | Self::Constant(_) | Self::Opaque)
    }
}

/// Invocation-local dependency proof; threading: exclusively compiler-owned
/// Boolean selections and opaque SSA handles, with no mutator state.
struct Analysis {
    nodes: Vec<Node>,
    selected: Vec<bool>,
}

/// Memoize each alias chain once. The input verifier already excludes cycles
/// and invalid handles; every visited alias receives its canonical definition.
fn canonical_values(func: &Func) -> Vec<Value> {
    let mut resolved = vec![None; func.values.len()];
    let mut path = Vec::new();
    for index in 0..func.values.len() {
        let mut current = Value(index as u32);
        let canonical = loop {
            if let Some(canonical) = resolved[current.index()] {
                break canonical;
            }
            path.push(current);
            match func.values[current.index()].def {
                ValueDef::Alias(next) => current = next,
                _ => break current,
            }
        };
        for value in path.drain(..) {
            resolved[value.index()] = Some(canonical);
        }
    }
    resolved.into_iter().map(Option::unwrap).collect()
}

fn analyze(func: &Func, canonical: &[Value]) -> Analysis {
    let count = func.values.len();
    let mut nodes = vec![Node::Unknown; count];
    let mut dependencies = vec![Vec::new(); count];
    let mut incoming = vec![Vec::new(); count];
    let nil = crate::emacs_core::jit::opt::ir::ValueBits::from_value(LispValue::NIL);
    let yes = crate::emacs_core::jit::opt::ir::ValueBits::from_value(LispValue::T);
    for block in &func.blocks {
        for edge in block.term.edges() {
            for (&param, &argument) in func.blocks[edge.target.index()]
                .params
                .iter()
                .zip(&edge.args)
            {
                incoming[param.index()].push(canonical[argument.index()]);
            }
        }
    }
    for (index, data) in func.values.iter().enumerate() {
        if canonical[index].index() != index {
            continue;
        }
        if data.rep == Rep::Bool {
            nodes[index] = Node::Existing;
            continue;
        }
        if data.rep != Rep::Tagged || data.ty.is_bottom() {
            continue;
        }
        // The builder may leave a join at TOP even when every incoming value
        // is a proved producer. Closure supplies that missing type fact;
        // narrower annotations excluding all Boolean values remain ineligible.
        let boolean_type = !data.ty.meet(TypeSet::BOOLEAN).is_bottom();
        let ValueDef::Inst(id) = data.def else {
            if boolean_type && !incoming[index].is_empty() {
                nodes[index] = Node::Phi;
                dependencies[index] = std::mem::take(&mut incoming[index]);
            }
            continue;
        };
        let inst = &func.insts[id.index()];
        let node = match &inst.op {
            Opcode::Const(pool_index) if *pool_index as usize >= func.dynamic_prefix => {
                let bits = func.consts[*pool_index as usize];
                if (bits == nil || bits == yes) && boolean_type {
                    Node::Constant(bits == yes)
                } else {
                    Node::Unknown
                }
            }
            Opcode::Opaque(op)
                if opaque_bool_arity(op).is_some()
                    && !data.ty.meet(TypeSet::BOOLEAN).is_bottom() =>
            {
                Node::Opaque
            }
            Opcode::Refine(_) if boolean_type => Node::Refine,
            Opcode::Select if boolean_type => Node::Select,
            Opcode::BoolToLisp if boolean_type => Node::Projection,
            _ => Node::Unknown,
        };
        match node {
            Node::Refine | Node::Projection => {
                dependencies[index].push(canonical[inst.args[0].index()])
            }
            Node::Select => dependencies[index].extend(
                inst.args[1..]
                    .iter()
                    .map(|argument| canonical[argument.index()]),
            ),
            _ => {}
        }
        nodes[index] = node;
    }
    let mut dependents = vec![Vec::new(); count];
    for (index, inputs) in dependencies.iter().enumerate() {
        for input in inputs {
            dependents[input.index()].push(index);
        }
    }
    let mut closed: Vec<_> = nodes.iter().map(|node| *node != Node::Unknown).collect();
    prune(&mut closed, &dependencies, &dependents);
    // Propagate grounding through the closed graph. This handles cycles without
    // recursively visiting them or repeatedly scanning a fixed-point lattice.
    let mut selected = vec![false; count];
    let mut queue = VecDeque::new();
    for index in 0..count {
        if closed[index] && nodes[index].seed() {
            selected[index] = true;
            queue.push_back(index);
        }
    }
    while let Some(index) = queue.pop_front() {
        for &dependent in &dependents[index] {
            if closed[dependent] && !selected[dependent] {
                selected[dependent] = true;
                queue.push_back(dependent);
            }
        }
    }
    // A grounded phi can still have a separate seedless SCC as another input.
    // Removing that SCC must also remove every dependent that needs its value.
    prune(&mut selected, &dependencies, &dependents);
    Analysis { nodes, selected }
}

fn prune(selected: &mut [bool], dependencies: &[Vec<Value>], dependents: &[Vec<usize>]) {
    let mut queue = VecDeque::new();
    for (index, inputs) in dependencies.iter().enumerate() {
        if selected[index] && inputs.iter().any(|input| !selected[input.index()]) {
            selected[index] = false;
            queue.push_back(index);
        }
    }
    while let Some(index) = queue.pop_front() {
        for &dependent in &dependents[index] {
            if selected[dependent] {
                selected[dependent] = false;
                queue.push_back(dependent);
            }
        }
    }
}

fn rewrite(func: &mut Func, canonical: &[Value], analysis: &Analysis) -> BoolStats {
    let mut stats = BoolStats::default();
    let original_values = func.values.len();
    let original_reps: Vec<_> = canonical
        .iter()
        .map(|value| func.values[value.index()].rep)
        .collect();
    for index in 0..original_values {
        if !analysis.selected[index] || canonical[index].index() != index {
            continue;
        }
        func.values[index].rep = Rep::Bool;
        func.values[index].ty = func.values[index].ty.meet(TypeSet::BOOLEAN);
        let ValueDef::Inst(id) = func.values[index].def else {
            stats.phi_params += usize::from(analysis.nodes[index] == Node::Phi);
            continue;
        };
        let inst = &mut func.insts[id.index()];
        match analysis.nodes[index] {
            Node::Constant(value) => {
                inst.op = Opcode::BoolConst(value);
                func.values[index].ty = if value { TypeSet::T } else { TypeSet::NIL };
                stats.constant_producers += 1;
            }
            Node::Opaque => {
                let Opcode::Opaque(op) = &inst.op else {
                    unreachable!("classified opaque")
                };
                inst.op = Opcode::OpaqueBool(op.clone());
                stats.opaque_producers += 1;
            }
            Node::Refine => stats.refinements += 1,
            Node::Select => stats.selects += 1,
            Node::Projection => {
                inst.op = Opcode::Refine(TypeSet::BOOLEAN);
                stats.refinements += 1;
            }
            Node::Existing | Node::Unknown | Node::Phi => {}
        }
    }
    for inst in &mut func.insts {
        if inst.op == Opcode::IsNonNil && analysis.selected[canonical[inst.args[0].index()].index()]
        {
            inst.op = Opcode::Refine(TypeSet::BOOLEAN);
            stats.nil_tests += 1;
        }
    }
    narrow_refinements(func, canonical);
    for index in 0..original_values {
        let target = canonical[index].index();
        if index != target && analysis.selected[target] {
            func.values[index].rep = Rep::Bool;
            func.values[index].ty = func.values[index].ty.meet(func.values[target].ty);
        }
    }
    for block_index in 0..func.blocks.len() {
        let mut cache = HashMap::new();
        let mut ordered = Vec::new();
        for id in std::mem::take(&mut func.blocks[block_index].insts) {
            let inst = func.insts[id.index()].clone();
            for (index, &argument) in inst.args.iter().enumerate() {
                let flag = canonical[argument.index()];
                if func.values[flag.index()].rep != Rep::Bool
                    || !original_reps[argument.index()].is_tagged()
                {
                    continue;
                }
                let bool_use = match inst.op {
                    Opcode::Refine(_) => inst
                        .result
                        .is_some_and(|value| func.values[value.index()].rep == Rep::Bool),
                    Opcode::Select => {
                        index == 0
                            || inst
                                .result
                                .is_some_and(|value| func.values[value.index()].rep == Rep::Bool)
                    }
                    Opcode::BoolToLisp => true,
                    _ => false,
                };
                if !bool_use {
                    let view =
                        tagged_view(func, flag, inst.pc, &mut cache, &mut ordered, &mut stats);
                    func.insts[id.index()].args[index] = view;
                }
            }
            ordered.push(id);
        }
        let pc = ordered.last().map_or(func.blocks[block_index].pc, |inst| {
            func.insts[inst.index()].pc
        });
        let mut term = func.blocks[block_index].term.clone();
        match &mut term {
            Term::Return(value) => tag_use(
                func,
                value,
                canonical,
                pc,
                &mut cache,
                &mut ordered,
                &mut stats,
            ),
            Term::Switch { value, table, .. } => {
                tag_use(
                    func,
                    value,
                    canonical,
                    pc,
                    &mut cache,
                    &mut ordered,
                    &mut stats,
                );
                tag_use(
                    func,
                    table,
                    canonical,
                    pc,
                    &mut cache,
                    &mut ordered,
                    &mut stats,
                );
            }
            _ => {}
        }
        let edges: Vec<_> = match &mut term {
            Term::Jump(edge) => vec![edge],
            Term::Branch {
                if_true, if_false, ..
            } => vec![if_true, if_false],
            Term::Switch { cases, default, .. } => cases
                .iter_mut()
                .map(|case| &mut case.edge)
                .chain([default])
                .collect(),
            _ => Vec::new(),
        };
        for edge in edges {
            let params = func.blocks[edge.target.index()].params.clone();
            for (argument, param) in edge.args.iter_mut().zip(params) {
                if func.values[param.index()].rep.is_tagged() {
                    tag_use(
                        func,
                        argument,
                        canonical,
                        pc,
                        &mut cache,
                        &mut ordered,
                        &mut stats,
                    );
                }
            }
        }
        func.blocks[block_index].insts = ordered;
        func.blocks[block_index].term = term;
    }
    func.census.insts = func.insts.len();
    func.census.refinements = func
        .insts
        .iter()
        .filter(|inst| matches!(inst.op, Opcode::Refine(_)))
        .count();
    stats
}

/// A Boolean refinement can shrink at most BOOLEAN -> singleton -> bottom.
/// Queue dependent refinements so singleton propagation is linear in this web.
fn narrow_refinements(func: &mut Func, canonical: &[Value]) {
    let mut dependents = vec![Vec::new(); canonical.len()];
    let mut queue = VecDeque::new();
    for (index, inst) in func.insts.iter().enumerate() {
        if matches!(inst.op, Opcode::Refine(_))
            && inst
                .result
                .is_some_and(|result| func.values[result.index()].rep == Rep::Bool)
        {
            dependents[canonical[inst.args[0].index()].index()].push(index);
            queue.push_back(index);
        }
    }
    while let Some(index) = queue.pop_front() {
        let inst = &func.insts[index];
        let Opcode::Refine(target) = inst.op else {
            unreachable!("refinement work item")
        };
        let result = inst.result.expect("selected refinement result");
        let input = canonical[inst.args[0].index()];
        let old = func.values[result.index()].ty;
        let narrowed = old.meet(func.values[input.index()].ty).meet(target);
        if narrowed != old {
            func.values[result.index()].ty = narrowed;
            queue.extend(dependents[result.index()].iter().copied());
        }
    }
}

fn tag_use(
    func: &mut Func,
    value: &mut Value,
    canonical: &[Value],
    pc: u32,
    cache: &mut HashMap<Value, Value>,
    ordered: &mut Vec<Inst>,
    stats: &mut BoolStats,
) {
    let flag = canonical[value.index()];
    if func.values[flag.index()].rep == Rep::Bool {
        *value = tagged_view(func, flag, pc, cache, ordered, stats);
    }
}

fn tagged_view(
    func: &mut Func,
    flag: Value,
    pc: u32,
    cache: &mut HashMap<Value, Value>,
    ordered: &mut Vec<Inst>,
    stats: &mut BoolStats,
) -> Value {
    if let Some(&view) = cache.get(&flag) {
        return view;
    }
    let inst = Inst(func.insts.len() as u32);
    let view = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty: func.values[flag.index()].ty,
        rep: Rep::Tagged,
        def: ValueDef::Inst(inst),
    });
    func.insts.push(InstData {
        op: Opcode::BoolToLisp,
        args: vec![flag],
        result: Some(view),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc,
    });
    ordered.push(inst);
    cache.insert(flag, view);
    stats.tagged_views += 1;
    view
}

#[cfg(test)]
#[path = "tests/bools_test.rs"]
mod tests;
