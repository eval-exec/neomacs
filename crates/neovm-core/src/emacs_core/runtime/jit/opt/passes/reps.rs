//! Fixnum representation selection with exact GNU semantic observation views.
//!
//! Threading: analysis and transformation state belongs to one compilation;
//! constant classification reads immediate bits without dereferencing objects.

use std::collections::{HashMap, HashSet, VecDeque};

use super::{range, reps_lift::LiftStats};

use crate::emacs_core::jit::opt::{
    ir::{Block, Func, Inst, InstData, Opcode, Rep, RepsCensus, Term, Value, ValueData, ValueDef},
    mem::{AliasClass, Effects},
    types::{Range, TypeSet},
    verify::VerifyError,
};

/// Immutable selection results; threading: owned by one compiler until reporting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RepsStats {
    /// Canonical raw definitions, including inserted UntagFix views.
    pub(crate) raw_values: usize,
    pub(crate) raw_phis: usize,
    pub(crate) tagged_arithmetic: usize,
    pub(crate) raw_arithmetic: usize,
    pub(crate) tagged_views: usize,
}

/// Verification is transactional; the pipeline invokes this only for `reps`.
pub(crate) fn run(func: &mut Func) -> Result<RepsStats, VerifyError> {
    func.verify()?;
    let canonical = canonical_values(func);
    let analysis = analyze(func, &canonical);
    let mut candidate = func.clone();
    let stats = rewrite(&mut candidate, &canonical, &analysis);
    candidate.verify()?;
    *func = candidate;
    Ok(stats)
}

/// Owned output of Reps's complete input and candidate validation. The graph
/// cannot be borrowed mutably or cloned while this terminal state is retained.
/// Threading: compilation-owned IR and opaque scalar bits; transferable without
/// Lisp dereference, mutator pointers, runtime publication or shared caches.
#[derive(Debug)]
#[must_use = "finish the verified Reps census before publishing the terminal plan"]
pub(crate) struct VerifiedRepsTail {
    func: Func,
    stats: RepsStats,
}

/// Terminal graph with its reporting census attached. Only immutable recording
/// and immediate consuming publication are available to the backend.
/// Threading: owned compiler data, independent of any compiling thread or mutator.
#[derive(Debug)]
#[must_use = "record and publish the completed terminal Reps plan"]
pub(crate) struct CompletedRepsPlan {
    func: Func,
}

static_assertions::assert_impl_all!(VerifiedRepsTail: Send, Sync);
static_assertions::assert_impl_all!(CompletedRepsPlan: Send, Sync);
static_assertions::assert_not_impl_any!(VerifiedRepsTail: Clone, Copy, std::ops::DerefMut);
static_assertions::assert_not_impl_any!(CompletedRepsPlan: Clone, Copy, std::ops::DerefMut);
const _: () = assert!(std::mem::size_of::<CompletedRepsPlan>() == std::mem::size_of::<Func>());

/// Keep the original transactional pass and BOTH complete checks. This is the
/// only constructor of terminal authority, available only after `run` succeeds.
pub(crate) fn run_terminal(mut func: Func) -> Result<VerifiedRepsTail, VerifyError> {
    let stats = run(&mut func)?;
    Ok(VerifiedRepsTail { func, stats })
}

impl VerifiedRepsTail {
    #[must_use]
    pub(crate) fn stats(&self) -> &RepsStats {
        &self.stats
    }

    /// Consume the pending census without changing any verifier premise. The
    /// verifier does not read OptCensus; all graph and child-proof data stay owned.
    #[must_use]
    pub(crate) fn finish(self, lift: LiftStats) -> CompletedRepsPlan {
        let Self { mut func, stats } = self;
        func.census.reps = Some(RepsCensus {
            lift,
            selection: stats,
        });
        CompletedRepsPlan { func }
    }
}

impl CompletedRepsPlan {
    #[must_use]
    pub(crate) fn as_func(&self) -> &Func {
        &self.func
    }

    #[must_use]
    pub(crate) fn into_func(self) -> Func {
        self.func
    }
}

/// Successful fixnum proofs indexed by original value IDs, including aliases.
///
/// The caller supplies verified IR. This invocation-local analysis does not
/// change types, representations, frames, source states, or cost any boundary.
/// Unknown arguments/environment/OSR sources and seedless dependency cycles
/// cannot ground a web; tagged successful guards can establish a real proof.
pub(crate) fn grounded_fixnums(func: &Func) -> Vec<bool> {
    let canonical = canonical_values(func);
    let proof = prove_fixnums(func, &canonical);
    canonical
        .iter()
        .map(|value| proof.grounded[value.index()])
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Node {
    Unknown,
    Seed,
    Phi,
    Refine,
    Select,
    Arithmetic,
    Projection,
}

/// Invocation-local proof and selection. No runtime objects or mutable hints
/// are retained; unknown sources never ground a representation web.
struct Analysis {
    grounded: Vec<bool>,
    raw: Vec<bool>,
}

/// Compiler-local successful type proof; shared by exposure and selection.
struct GroundedProof {
    nodes: Vec<Node>,
    dependencies: Vec<Vec<Value>>,
    dependents: Vec<Vec<usize>>,
    owners: Vec<Block>,
    grounded: Vec<bool>,
}

fn canonical_values(func: &Func) -> Vec<Value> {
    let mut resolved = vec![None; func.values.len()];
    let mut path = Vec::new();
    for index in 0..func.values.len() {
        let mut current = Value(index as u32);
        let target = loop {
            if let Some(target) = resolved[current.index()] {
                break target;
            }
            path.push(current);
            match func.values[current.index()].def {
                ValueDef::Alias(next) => current = next,
                _ => break current,
            }
        };
        for value in path.drain(..) {
            resolved[value.index()] = Some(target);
        }
    }
    resolved.into_iter().map(Option::unwrap).collect()
}

/// An unchecked opcode is not itself a range proof. Accept only the exact
/// independently validated payload domain supplied by Range's point-local
/// semantic operand views. Unknown Arg/Env/OSR dependencies still prune the
/// grounded web below; this does not promote a declared type into a seed.
fn unchecked_arithmetic_proven(func: &Func, inst: &InstData) -> bool {
    if !matches!(
        inst.op,
        Opcode::FixAdd { checked: false }
            | Opcode::FixSub { checked: false }
            | Opcode::FixMul { checked: false }
    ) || inst.eff != Effects::PURE
        || inst.mem != AliasClass::None
        || inst.args.len() != 2
    {
        return false;
    }
    let Some(result) = inst.result else {
        return false;
    };
    let output = &func.values[result.index()];
    if !matches!(output.rep, Rep::TaggedFix | Rep::RawInt)
        || output.ty.is_bottom()
        || !output.ty.is_subset(TypeSet::FIXNUM)
    {
        return false;
    }
    let fixed = |value: Value| -> Option<Range> {
        let data = &func.values[func.resolve(value)?.index()];
        (data.rep == output.rep && !data.ty.is_bottom() && data.ty.is_subset(TypeSet::FIXNUM))
            .then(|| data.ty.range())
            .flatten()
    };
    let (Some(a), Some(b)) = (fixed(inst.args[0]), fixed(inst.args[1])) else {
        return false;
    };
    range::arithmetic_result_fits(&inst.op, a, b, output.ty)
}

fn prove_fixnums(func: &Func, canonical: &[Value]) -> GroundedProof {
    let count = func.values.len();
    let mut nodes = vec![Node::Unknown; count];
    let mut dependencies = vec![Vec::new(); count];
    let mut incoming = vec![Vec::new(); count];
    let mut owners = vec![func.entry; count];
    for (index, block) in func.blocks.iter().enumerate() {
        let owner = Block(index as u32);
        for &value in &block.params {
            owners[value.index()] = owner;
        }
        for &inst in &block.insts {
            if let Some(value) = func.insts[inst.index()].result {
                owners[value.index()] = owner;
            }
        }
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
        if canonical[index].index() != index
            || !matches!(data.rep, Rep::Tagged | Rep::TaggedFix | Rep::RawInt)
            || data.ty.is_bottom()
            || data.ty.meet(TypeSet::FIXNUM).is_bottom()
        {
            continue;
        }
        let ValueDef::Inst(id) = data.def else {
            if !incoming[index].is_empty() {
                nodes[index] = Node::Phi;
                dependencies[index] = std::mem::take(&mut incoming[index]);
            }
            continue;
        };
        let inst = &func.insts[id.index()];
        nodes[index] = match &inst.op {
            Opcode::Const(pool) if *pool as usize >= func.dynamic_prefix => {
                let actual = TypeSet::for_constant(func.consts[*pool as usize]);
                if !actual.is_bottom()
                    && actual.is_subset(TypeSet::FIXNUM)
                    && !actual.meet(data.ty).is_bottom()
                {
                    Node::Seed
                } else {
                    Node::Unknown
                }
            }
            Opcode::CheckType(target)
                if data.rep.is_tagged()
                    && !target.is_bottom()
                    && target.is_subset(TypeSet::FIXNUM)
                    && data.ty.is_subset(TypeSet::FIXNUM) =>
            {
                Node::Seed
            }
            Opcode::FixAdd { checked: true }
            | Opcode::FixSub { checked: true }
            | Opcode::FixMul { checked: true } => Node::Arithmetic,
            Opcode::FixAdd { checked: false }
            | Opcode::FixSub { checked: false }
            | Opcode::FixMul { checked: false }
                if unchecked_arithmetic_proven(func, inst) =>
            {
                Node::Arithmetic
            }
            Opcode::Refine(_) => Node::Refine,
            Opcode::Select => Node::Select,
            Opcode::TagFix | Opcode::UntagFix => Node::Projection,
            _ => Node::Unknown,
        };
        match nodes[index] {
            Node::Arithmetic => {
                dependencies[index].extend(inst.args.iter().map(|value| canonical[value.index()]))
            }
            Node::Select => dependencies[index]
                .extend(inst.args[1..].iter().map(|value| canonical[value.index()])),
            Node::Refine | Node::Projection => {
                dependencies[index].push(canonical[inst.args[0].index()]);
            }
            _ => {}
        }
    }
    let mut dependents = vec![Vec::new(); count];
    for (index, inputs) in dependencies.iter().enumerate() {
        for input in inputs {
            dependents[input.index()].push(index);
        }
    }
    let mut closed: Vec<_> = nodes.iter().map(|node| *node != Node::Unknown).collect();
    prune(&mut closed, &dependencies, &dependents);
    let mut grounded = vec![false; count];
    let mut queue = VecDeque::new();
    for index in 0..count {
        if closed[index] && nodes[index] == Node::Seed {
            grounded[index] = true;
            queue.push_back(index);
        }
    }
    while let Some(index) = queue.pop_front() {
        for &dependent in &dependents[index] {
            if closed[dependent] && !grounded[dependent] {
                grounded[dependent] = true;
                queue.push_back(dependent);
            }
        }
    }
    // One real seed must not legitimize a separate seedless dependency SCC.
    prune(&mut grounded, &dependencies, &dependents);
    GroundedProof {
        nodes,
        dependencies,
        dependents,
        owners,
        grounded,
    }
}

fn analyze(func: &Func, canonical: &[Value]) -> Analysis {
    let count = func.values.len();
    let GroundedProof {
        nodes,
        dependencies,
        dependents,
        owners,
        grounded,
    } = prove_fixnums(func, canonical);
    let mut queue = VecDeque::new();
    let mut components = vec![None; count];
    let mut members: Vec<Vec<usize>> = Vec::new();
    for index in 0..count {
        if !grounded[index] || nodes[index] == Node::Seed || components[index].is_some() {
            continue;
        }
        let component = members.len();
        let mut found = Vec::new();
        components[index] = Some(component);
        queue.push_back(index);
        while let Some(value) = queue.pop_front() {
            found.push(value);
            for next in dependencies[value]
                .iter()
                .map(|value| value.index())
                .chain(dependents[value].iter().copied())
            {
                if grounded[next] && nodes[next] != Node::Seed && components[next].is_none() {
                    components[next] = Some(component);
                    queue.push_back(next);
                }
            }
        }
        members.push(found);
    }
    let weights = block_weights(func);
    let mut costs: Vec<_> = members.iter().map(|_| Cost::default()).collect();
    for (index, component) in components.iter().enumerate() {
        let Some(component) = *component else {
            continue;
        };
        let weight = weights[owners[index].index()];
        if nodes[index] == Node::Phi && weight > 1 {
            costs[component].benefit += weight;
        }
        if let ValueDef::Inst(inst) = func.values[index].def
            && matches!(func.insts[inst.index()].op, Opcode::FixMul { .. })
        {
            // Checked Tagged Mul has three conversions; Raw has two.
            // Independently proved unchecked Tagged Mul still untags A,
            // debiases B and rebiases its product; Raw is one plain imul.
            // These are emitter-shape counts, not measured instruction gains.
            let conversions_saved = if matches!(
                func.insts[inst.index()].op,
                Opcode::FixMul { checked: false }
            ) {
                3
            } else {
                1
            };
            costs[component].benefit += conversions_saved * weight;
        }
        for &input in &dependencies[index] {
            if components[input.index()] != Some(component) {
                costs[component].untag(input, func, &owners, &weights);
            }
        }
    }
    for (block_index, block) in func.blocks.iter().enumerate() {
        for &id in &block.insts {
            let inst = &func.insts[id.index()];
            let result_component = inst.result.and_then(|value| components[value.index()]);
            let cmp = matches!(inst.op, Opcode::FixCmp(_));
            let raw_cmp = cmp
                && inst
                    .args
                    .iter()
                    .all(|value| grounded[canonical[value.index()].index()]);
            for (argument_index, &argument) in inst.args.iter().enumerate() {
                let value = canonical[argument.index()];
                let Some(component) = components[value.index()] else {
                    continue;
                };
                if result_component == Some(component)
                    || matches!(inst.op, Opcode::TagFix | Opcode::F64FromFix)
                {
                    continue;
                }
                if raw_cmp {
                    for &other in &inst.args {
                        let other = canonical[other.index()];
                        if components[other.index()] != Some(component) {
                            costs[component].untag(other, func, &owners, &weights);
                        }
                    }
                } else {
                    let rep = match inst.op {
                        Opcode::FixAdd { .. }
                        | Opcode::FixSub { .. }
                        | Opcode::FixMul { .. }
                        | Opcode::FixDiv
                        | Opcode::FixRem
                        | Opcode::FixMinMax(_)
                        | Opcode::CheckType(_)
                        | Opcode::Refine(_)
                        | Opcode::Select => inst
                            .result
                            .map(|value| func.values[value.index()].rep)
                            .unwrap_or(Rep::TaggedFix),
                        // This component is being costed as hypothetical RawInt.
                        // Bounds accepts an independent raw length and a raw
                        // index when it has no semantic result. Neither needs a
                        // tagged boundary. A resultful index must match its
                        // unchanged output representation.
                        Opcode::CheckBounds if argument_index > 0 || inst.result.is_none() => {
                            Rep::RawInt
                        }
                        Opcode::CheckBounds => inst
                            .result
                            .map(|value| func.values[value.index()].rep)
                            .unwrap_or(Rep::TaggedFix),
                        Opcode::PublishRoot => Rep::Tagged,
                        _ => Rep::TaggedFix,
                    };
                    // Existing raw typed consumers already accept this word;
                    // selection must not invent a tagged boundary for them.
                    if rep.is_tagged() {
                        costs[component].tag(block_index, value, rep, weights[block_index]);
                    }
                }
            }
        }
        for edge in block.term.edges() {
            for (&argument, &param) in edge
                .args
                .iter()
                .zip(&func.blocks[edge.target.index()].params)
            {
                let value = canonical[argument.index()];
                if let Some(component) = components[value.index()]
                    && components[param.index()] != Some(component)
                {
                    costs[component].tag(
                        block_index,
                        value,
                        func.values[param.index()].rep,
                        weights[block_index],
                    );
                }
            }
        }
        let uses: Vec<_> = match &block.term {
            Term::Return(value) | Term::Branch { flag: value, .. } => vec![*value],
            Term::Switch { value, table, .. } => vec![*value, *table],
            _ => vec![],
        };
        for value in uses {
            let value = canonical[value.index()];
            if let Some(component) = components[value.index()] {
                costs[component].tag(block_index, value, Rep::TaggedFix, weights[block_index]);
            }
        }
    }
    let mut raw = vec![false; count];
    for (component, members) in members.iter().enumerate() {
        let forced = members
            .iter()
            .any(|&value| func.values[value].rep == Rep::RawInt);
        if forced || costs[component].benefit > costs[component].boundary {
            for &value in members {
                raw[value] = true;
            }
        }
    }
    Analysis { grounded, raw }
}

fn prune(selected: &mut [bool], dependencies: &[Vec<Value>], dependents: &[Vec<usize>]) {
    let mut queue = VecDeque::new();
    for (index, inputs) in dependencies.iter().enumerate() {
        if selected[index] && inputs.iter().any(|value| !selected[value.index()]) {
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

/// Static heuristic, not measured native instruction counts. SCC membership
/// weights repeated blocks by 10 and others by 1; nested depth is not modeled.
/// A general untag costs one unit, one tag costs two; cached boundaries count
/// once. A direct immutable fixnum Const costs zero untag conversions: native
/// emission creates its raw payload immediate after the successful type proof.
/// Shared seed views across independent components can make this conservative.
/// A loop phi credits repeated raw transport. Checked Mul credits one saved
/// conversion and independently proved unchecked Mul credits three; its native
/// RawInt emitter uses one plain imul instead of a scaled checked product.
/// Frames/source metadata cost nothing here: their reconstruction remains cold.
#[derive(Default)]
struct Cost {
    benefit: u64,
    boundary: u64,
    untags: HashSet<Value>,
    tags: HashSet<(usize, Value, Rep)>,
}

impl Cost {
    fn untag(&mut self, value: Value, func: &Func, owners: &[Block], weights: &[u64]) {
        if func.values[value.index()].rep != Rep::RawInt && self.untags.insert(value) {
            let data = &func.values[value.index()];
            // Rewrite narrows proven literals before native emission. Match
            // its direct immutable Const payload path, not a refined view or
            // any dynamic-prefix/environment template.
            if let ValueDef::Inst(id) = data.def
                && let Opcode::Const(pool) = func.insts[id.index()].op
                && pool as usize >= func.dynamic_prefix
                && !data.ty.meet(TypeSet::FIXNUM).is_bottom()
            {
                let actual = TypeSet::for_constant(func.consts[pool as usize]);
                if !actual.is_bottom() && actual.is_subset(TypeSet::FIXNUM) {
                    return;
                }
            }
            self.boundary += weights[owners[value.index()].index()];
        }
    }

    fn tag(&mut self, block: usize, value: Value, rep: Rep, weight: u64) {
        if self.tags.insert((block, value, rep)) {
            self.boundary += 2 * weight;
        }
    }
}

fn block_weights(func: &Func) -> Vec<u64> {
    let successors: Vec<Vec<_>> = func
        .blocks
        .iter()
        .map(|block| {
            block
                .term
                .edges()
                .into_iter()
                .map(|edge| edge.target.index())
                .collect()
        })
        .collect();
    let mut seen = vec![false; func.blocks.len()];
    let mut order = Vec::new();
    for root in 0..func.blocks.len() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((block, next)) = stack.last_mut() {
            if let Some(&successor) = successors[*block].get(*next) {
                *next += 1;
                if !seen[successor] {
                    seen[successor] = true;
                    stack.push((successor, 0));
                }
            } else {
                order.push(*block);
                stack.pop();
            }
        }
    }
    seen.fill(false);
    let mut weights = vec![1; func.blocks.len()];
    for root in order.into_iter().rev() {
        if seen[root] {
            continue;
        }
        let mut members = Vec::new();
        let mut stack = vec![root];
        seen[root] = true;
        while let Some(block) = stack.pop() {
            members.push(block);
            for predecessor in &func.blocks[block].preds {
                let predecessor = predecessor.index();
                if !seen[predecessor] {
                    seen[predecessor] = true;
                    stack.push(predecessor);
                }
            }
        }
        if members.len() > 1 || successors[root].contains(&root) {
            for block in members {
                weights[block] = 10;
            }
        }
    }
    weights
}

fn raw_cmp(func: &Func, inst: &InstData, canonical: &[Value], analysis: &Analysis) -> bool {
    inst.args
        .iter()
        .all(|value| analysis.grounded[canonical[value.index()].index()])
        && inst
            .args
            .iter()
            .any(|value| func.values[canonical[value.index()].index()].rep == Rep::RawInt)
}

/// Bounds compares payloads; the length is not an index-result view. Preserve
/// an already valid RawInt length rather than emitting TagFix + native Untag.
/// This transport rule creates no numeric/layout proof and cannot ground an
/// unknown web. Final array verification still checks owner/epoch/provenance.
fn bounds_operand_rep(func: &Func, inst: &InstData, index: usize, canonical: &[Value]) -> Rep {
    if index == 0
        && let Some(result) = inst.result
    {
        return func.values[result.index()].rep;
    }
    let data = &func.values[canonical[inst.args[index].index()].index()];
    if data.rep == Rep::RawInt && !data.ty.is_bottom() && data.ty.is_subset(TypeSet::FIXNUM) {
        Rep::RawInt
    } else {
        // Retain the old tagged fallback for unsupported declarations/reps.
        Rep::TaggedFix
    }
}

fn desired_rep(
    func: &Func,
    inst: &InstData,
    index: usize,
    canonical: &[Value],
    analysis: &Analysis,
) -> Option<Rep> {
    match &inst.op {
        Opcode::FixAdd { .. }
        | Opcode::FixSub { .. }
        | Opcode::FixMul { .. }
        | Opcode::FixDiv
        | Opcode::FixRem
        | Opcode::FixMinMax(_)
        | Opcode::CheckType(_)
        | Opcode::Refine(_) => inst.result.map(|value| func.values[value.index()].rep),
        Opcode::Select if index > 0 => inst.result.map(|value| func.values[value.index()].rep),
        Opcode::Select => None,
        Opcode::CheckBounds => Some(bounds_operand_rep(func, inst, index, canonical)),
        Opcode::FixCmp(_) => Some(if raw_cmp(func, inst, canonical, analysis) {
            Rep::RawInt
        } else {
            Rep::TaggedFix
        }),
        Opcode::TagFix | Opcode::F64FromFix => None,
        Opcode::PublishRoot => Some(Rep::Tagged),
        // These require actual Lisp words, not another canonical raw identity.
        _ => Some(Rep::TaggedFix),
    }
}

fn rewrite(func: &mut Func, canonical: &[Value], analysis: &Analysis) -> RepsStats {
    let original_values = func.values.len();
    let mut stats = RepsStats::default();
    for index in 0..original_values {
        let target = canonical[index].index();
        if analysis.grounded[target] {
            func.values[index].ty = func.values[index].ty.meet(TypeSet::FIXNUM);
        }
        if analysis.raw[target] {
            func.values[index].rep = Rep::RawInt;
        }
    }
    for inst in &mut func.insts {
        if matches!(inst.op, Opcode::TagFix | Opcode::UntagFix)
            && inst.result.is_some_and(|value| analysis.raw[value.index()])
            && func.values[canonical[inst.args[0].index()].index()].rep == Rep::RawInt
        {
            inst.op = Opcode::Refine(TypeSet::FIXNUM);
        }
    }
    narrow_refinements(func, canonical);
    let mut needed = HashSet::new();
    for inst in &func.insts {
        for (index, &argument) in inst.args.iter().enumerate() {
            let value = canonical[argument.index()];
            if desired_rep(func, inst, index, canonical, analysis) == Some(Rep::RawInt)
                && func.values[value.index()].rep != Rep::RawInt
            {
                needed.insert(value);
            }
        }
    }
    for block in &func.blocks {
        for edge in block.term.edges() {
            for (&argument, &param) in edge
                .args
                .iter()
                .zip(&func.blocks[edge.target.index()].params)
            {
                let value = canonical[argument.index()];
                if func.values[param.index()].rep == Rep::RawInt
                    && func.values[value.index()].rep != Rep::RawInt
                {
                    needed.insert(value);
                }
            }
        }
    }
    let mut raw_views = HashMap::new();
    let mut starts = vec![Vec::new(); func.blocks.len()];
    let mut after: HashMap<Inst, Vec<Inst>> = HashMap::new();
    for index in 0..original_values {
        let value = Value(index as u32);
        if !needed.contains(&value) {
            continue;
        }
        let (pc, definition) = match func.values[index].def {
            ValueDef::Inst(id) => (func.insts[id.index()].pc, Some(id)),
            ValueDef::Param { block, .. } => (
                func.blocks[block.index()]
                    .insts
                    .first()
                    .map_or(func.blocks[block.index()].pc, |id| {
                        func.insts[id.index()].pc
                    }),
                None,
            ),
            ValueDef::Alias(_) => unreachable!("canonical raw view"),
        };
        let (inst, view) = conversion(func, value, Opcode::UntagFix, Rep::RawInt, pc);
        if let Some(definition) = definition {
            after.entry(definition).or_default().push(inst);
        } else if let ValueDef::Param { block, .. } = func.values[index].def {
            starts[block.index()].push(inst);
        }
        raw_views.insert(value, view);
    }
    for block_index in 0..func.blocks.len() {
        let mut cache = HashMap::new();
        let mut ordered = std::mem::take(&mut starts[block_index]);
        for id in std::mem::take(&mut func.blocks[block_index].insts) {
            let inst = func.insts[id.index()].clone();
            for (index, &argument) in inst.args.iter().enumerate() {
                if let Some(rep) = desired_rep(func, &inst, index, canonical, analysis) {
                    func.insts[id.index()].args[index] = view_for(
                        func,
                        canonical[argument.index()],
                        rep,
                        inst.pc,
                        &raw_views,
                        &mut cache,
                        &mut ordered,
                        &mut stats,
                    );
                }
            }
            ordered.push(id);
            ordered.extend(after.remove(&id).unwrap_or_default());
        }
        let pc = ordered
            .last()
            .map_or(func.blocks[block_index].pc, |id| func.insts[id.index()].pc);
        let mut term = func.blocks[block_index].term.clone();
        match &mut term {
            Term::Return(value) | Term::Branch { flag: value, .. } => {
                *value = view_for(
                    func,
                    canonical[value.index()],
                    Rep::TaggedFix,
                    pc,
                    &raw_views,
                    &mut cache,
                    &mut ordered,
                    &mut stats,
                );
            }
            Term::Switch { value, table, .. } => {
                for value in [value, table] {
                    *value = view_for(
                        func,
                        canonical[value.index()],
                        Rep::TaggedFix,
                        pc,
                        &raw_views,
                        &mut cache,
                        &mut ordered,
                        &mut stats,
                    );
                }
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
            _ => vec![],
        };
        for edge in edges {
            let params = func.blocks[edge.target.index()].params.clone();
            for (argument, param) in edge.args.iter_mut().zip(params) {
                let rep = func.values[param.index()].rep;
                if rep == Rep::RawInt || rep.is_tagged() {
                    *argument = view_for(
                        func,
                        canonical[argument.index()],
                        rep,
                        pc,
                        &raw_views,
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
    stats.raw_values = func
        .values
        .iter()
        .filter(|data| data.rep == Rep::RawInt && !matches!(data.def, ValueDef::Alias(_)))
        .count();
    stats.raw_phis = func
        .values
        .iter()
        .filter(|data| data.rep == Rep::RawInt && matches!(data.def, ValueDef::Param { .. }))
        .count();
    for inst in &func.insts {
        if matches!(
            inst.op,
            Opcode::FixAdd { .. } | Opcode::FixSub { .. } | Opcode::FixMul { .. }
        ) {
            match func.values[inst.result.expect("verified arithmetic result").index()].rep {
                Rep::RawInt => stats.raw_arithmetic += 1,
                Rep::TaggedFix => stats.tagged_arithmetic += 1,
                _ => unreachable!("verified arithmetic representation"),
            }
        }
    }
    func.census.insts = func.insts.len();
    func.census.refinements = func
        .insts
        .iter()
        .filter(|inst| matches!(inst.op, Opcode::Refine(_)))
        .count();
    stats
}

/// Projection removal keeps each original result definition. Its type must
/// satisfy the stricter Refine contract even when the old TagFix annotation
/// was broader than its input's interval or singleton.
fn narrow_refinements(func: &mut Func, canonical: &[Value]) {
    let mut dependents = vec![Vec::new(); canonical.len()];
    let mut queue = VecDeque::new();
    for (index, inst) in func.insts.iter().enumerate() {
        if matches!(inst.op, Opcode::Refine(_)) {
            dependents[canonical[inst.args[0].index()].index()].push(index);
            queue.push_back(index);
        }
    }
    while let Some(index) = queue.pop_front() {
        let inst = &func.insts[index];
        let Opcode::Refine(target) = inst.op else {
            unreachable!("refinement")
        };
        let result = inst.result.expect("verified refinement result");
        let input = canonical[inst.args[0].index()];
        let old = func.values[result.index()].ty;
        let narrowed = old.meet(func.values[input.index()].ty).meet(target);
        if narrowed != old {
            func.values[result.index()].ty = narrowed;
            queue.extend(dependents[result.index()].iter().copied());
        }
    }
}

fn conversion(func: &mut Func, value: Value, op: Opcode, rep: Rep, pc: u32) -> (Inst, Value) {
    let inst = Inst(func.insts.len() as u32);
    let view = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty: func.values[value.index()].ty,
        rep,
        def: ValueDef::Inst(inst),
    });
    func.insts.push(InstData {
        op,
        args: vec![value],
        result: Some(view),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc,
    });
    (inst, view)
}

#[allow(clippy::too_many_arguments)]
fn view_for(
    func: &mut Func,
    value: Value,
    rep: Rep,
    pc: u32,
    raw_views: &HashMap<Value, Value>,
    cache: &mut HashMap<(Value, Rep), Value>,
    ordered: &mut Vec<Inst>,
    stats: &mut RepsStats,
) -> Value {
    let actual = func.values[value.index()].rep;
    if rep == Rep::RawInt && actual != Rep::RawInt {
        return raw_views[&value];
    }
    if !rep.is_tagged() || actual != Rep::RawInt {
        return value;
    }
    if let Some(&view) = cache.get(&(value, rep)) {
        return view;
    }
    let (inst, view) = conversion(func, value, Opcode::TagFix, rep, pc);
    ordered.push(inst);
    cache.insert((value, rep), view);
    stats.tagged_views += 1;
    view
}

#[cfg(test)]
#[path = "tests/reps_test.rs"]
mod tests;
