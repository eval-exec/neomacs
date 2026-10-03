//! Bounded transactional numeric/Cons allocation recipes with real SSA fields.
//! Threading: compilation-owned IDs/scalars only; no heap/TLS/function lookup.

use super::cfg::Dominance;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    sink_recipes::*,
    types::TypeSet,
    verify::VerifyError,
};
use crate::emacs_core::value::Value as LispValue;
use std::collections::{HashMap, HashSet, VecDeque};
#[path = "sink_front.rs"]
mod front;
#[path = "sink_versions.rs"]
mod versions;

use self::front::{
    append_edge_fields, append_phi_fields, borrow_word, edge_mut, numeric_values, rewrite_source,
};
use self::versions::{
    PassError, Versions, append, append_version, box_field, create_cache_phi, fill_cache_phi,
    guaranteed_word, word_view,
};

const WORK_LIMIT: usize = 1_000_000;

#[derive(Default)]
struct Selection {
    numeric: HashSet<Inst>,
    sqrt: HashSet<Inst>,
    phis: HashSet<Value>,
    views: HashSet<Inst>,
    cons: HashSet<Inst>,
}

pub(crate) fn run(func: &mut Func, feedback: &[NumericFeedback]) -> Result<SinkStats, VerifyError> {
    run_with_sqrt_sites(func, feedback, &HashSet::new())
}

/// Feedback and source sqrt PCs are immutable FRONT-captured admission hints.
/// Their selection never proves an operand's numeric kind or callee identity.
pub(crate) fn run_with_sqrt_sites(
    func: &mut Func,
    feedback: &[NumericFeedback],
    sqrt_sites: &HashSet<u32>,
) -> Result<SinkStats, VerifyError> {
    func.verify()?;
    if !func.sink_recipes.owners.is_empty() || func.osr.is_some() {
        return Ok(SinkStats::default());
    }
    let mut candidate = func.clone();
    match transform(&mut candidate, feedback, sqrt_sites) {
        Ok((table, stats)) => {
            candidate.sink_recipes = table;
            // Ordinary verifier checks all actual physical SSA fields/edges;
            // its child independent verifier reconstructs recipe provenance.
            candidate.verify()?;
            *func = candidate;
            Ok(stats)
        }
        Err(PassError::AnalysisLimit) => Ok(SinkStats {
            analysis_bailed: 1,
            ..Default::default()
        }),
        Err(PassError::Decline) => Ok(SinkStats::default()),
        Err(PassError::Invalid(error)) => Err(error),
    }
}

fn transform(
    func: &mut Func,
    feedback: &[NumericFeedback],
    sqrt_sites: &HashSet<u32>,
) -> Result<(SinkRecipes, SinkStats), PassError> {
    let dom = Dominance::new(func)?;
    let original = func.clone();
    let blocks = instruction_blocks(&original);
    let incoming = incoming_edges(&original);
    // A single-entry reducible graph gives an explicit predecessor-version cut.
    // No ambiguous irreducible/OSR entry is speculated around.
    if incoming[func.entry.index()].len() != 0 {
        return Err(PassError::Decline);
    }
    let selection = discover(&original, feedback, sqrt_sites, &dom)?;
    if selection.numeric.is_empty() && selection.sqrt.is_empty() && selection.cons.is_empty() {
        return Ok((SinkRecipes::default(), SinkStats::default()));
    }
    let mut table = SinkRecipes::default();
    let mut stats = SinkStats::default();
    let mut templates = vec![Vec::new(); func.blocks.len()];

    // Allocate complete original-phi physical tuples BEFORE edge/body rewrite.
    let mut phis = selection.phis.iter().copied().collect::<Vec<_>>();
    phis.sort_unstable();
    for owner in &phis {
        append_phi_fields(func, &mut table, *owner)?;
    }
    stats.numeric_phis = phis.len();
    for &block in dom.reverse_postorder() {
        let mut order = Vec::new();
        for &id in &original.blocks[block.index()].insts {
            let data = &original.insts[id.index()];
            if selection.numeric.contains(&id) {
                rewrite_source(
                    func,
                    &mut table,
                    id,
                    [data.args[0], data.args[1]],
                    &mut order,
                )?;
                stats.numeric_sources += 1;
            } else if selection.sqrt.contains(&id) {
                rewrite_sqrt(func, &mut table, id, &mut order)?;
                table.source_sqrt_sites.insert(data.pc);
                stats.numeric_sources += 1;
            } else if selection.cons.contains(&id) {
                rewrite_cons(func, &mut table, id, &mut order)?;
                stats.cons_sources += 1;
            } else if selection.views.contains(&id) {
                rewrite_view(func, &mut table, id, &mut order)?;
            } else {
                order.push(id);
            }
        }
        templates[block.index()] = order;
    }
    let aliases = alias_groups(&original, &selection, &incoming)?;
    let mut versions = Versions {
        current: HashMap::new(),
        aliases,
        work: 0,
        limit: WORK_LIMIT,
    };
    let mut entries = vec![HashMap::<Value, RecipeVersionId>::new(); func.blocks.len()];
    let mut exits = entries.clone();
    let mut processed = vec![false; func.blocks.len()];
    let mut cache_phis = vec![Vec::<(Value, RecipeVersionId)>::new(); func.blocks.len()];
    let mut owner_list = table.owners.keys().copied().collect::<Vec<_>>();
    owner_list.sort_unstable();
    let future_uses = live_owner_blocks(&original, &selection, &dom)?;

    // Live dominating numeric owners get explicit cache-only params at joins.
    // This is a bounded static SSA algorithm, not a backend mutable Variable.
    for &block in dom.reverse_postorder() {
        if incoming[block.index()].len() < 2 {
            continue;
        }
        for &owner in &owner_list {
            versions.spend(1)?;
            let definition = definition_block(&original, &blocks, owner)?;
            if definition == block
                || !dom.dominates(definition, block)
                || !future_uses[block.index()].contains(&owner)
                || !matches!(table.owners[&owner].kind, RecipeKind::Number(_))
            {
                continue;
            }
            let version = create_cache_phi(func, &mut table, owner, block, &mut stats)?;
            cache_phis[block.index()].push((owner, version));
        }
        cache_phis[block.index()].sort_unstable_by_key(|entry| {
            let field = box_field(table.versions[entry.1.0 as usize].fields);
            match func.values[field.index()].def {
                ValueDef::Param { index, .. } => index,
                _ => unreachable!(),
            }
        });
    }

    for &block in dom.reverse_postorder() {
        versions.current.clear();
        if incoming[block.index()].len() == 1 {
            let predecessor = incoming[block.index()][0].0;
            // Reducible RPO must have its unique predecessor ready. Unsupported
            // irreducible order declines unchanged, never guesses a box cache.
            if !processed[predecessor.index()] {
                return Err(PassError::Decline);
            }
            versions.current = exits[predecessor.index()]
                .iter()
                .filter_map(|(&owner, &version)| {
                    future_uses[block.index()]
                        .contains(&owner)
                        .then_some((owner, version))
                })
                .collect();
        } else if incoming[block.index()].len() >= 2 {
            // Unchanged dominating definitions only; a changed cache requires
            // the preallocated phi above. Independent verifier checks all cuts.
            for &owner in &owner_list {
                let definition = definition_block(&original, &blocks, owner)?;
                if definition != block
                    && dom.dominates(definition, block)
                    && future_uses[block.index()].contains(&owner)
                {
                    versions.define(&table, owner);
                }
            }
            for &(owner, version) in &cache_phis[block.index()] {
                versions.current.insert(owner, version);
            }
        }
        for &param in &original.blocks[block.index()].params {
            if table.owners.contains_key(&param) {
                versions.define(&table, param);
            }
        }
        entries[block.index()] = versions.current.clone();
        versions.observe(func, &mut table, RecipePoint::Entry(block), None)?;
        let mut order = Vec::new();
        let mut borrow_memo = HashMap::<Value, Value>::new();
        let mut pending_definition = None::<(Value, usize)>;
        for &id in &templates[block.index()] {
            let data = func.insts[id.index()].clone();
            // Primitive projections belong to the preceding complete recipe
            // definition and do not materialize their logical owner argument.
            if matches!(data.op, Opcode::Sink(SinkOp::RecipeField(_))) {
                versions.observe(func, &mut table, RecipePoint::Before(id), data.frame)?;
                order.push(id);
                if let Some((owner, remaining)) = &mut pending_definition {
                    *remaining -= 1;
                    if *remaining == 0 {
                        versions.define(&table, *owner);
                        pending_definition = None;
                    }
                } else {
                    return Err(VerifyError::InvalidInst(id).into());
                }
                versions.observe(func, &mut table, RecipePoint::After(id), data.frame)?;
                continue;
            }
            let result_owner = data.result.filter(|owner| table.owners.contains_key(owner));
            match data.op {
                Opcode::Sink(SinkOp::SourceNum(_)) => {
                    let left = ensure_number(
                        func,
                        &mut table,
                        &mut versions,
                        data.args[0],
                        data.pc,
                        &mut order,
                        &mut borrow_memo,
                    )?;
                    let right = ensure_number(
                        func,
                        &mut table,
                        &mut versions,
                        data.args[1],
                        data.pc,
                        &mut order,
                        &mut borrow_memo,
                    )?;
                    func.insts[id.index()].args = vec![left, right];
                }
                Opcode::Sink(SinkOp::SourceSqrt) => {
                    let argument = ensure_number(
                        func,
                        &mut table,
                        &mut versions,
                        data.args[1],
                        data.pc,
                        &mut order,
                        &mut borrow_memo,
                    )?;
                    func.insts[id.index()].args[1] = argument;
                }
                Opcode::Sink(SinkOp::SourceCons(_)) => {}
                Opcode::Refine(_) if selection.views.contains(&id) => {
                    let input = ensure_number(
                        func,
                        &mut table,
                        &mut versions,
                        data.args[0],
                        data.pc,
                        &mut order,
                        &mut borrow_memo,
                    )?;
                    func.insts[id.index()].args = vec![input];
                    let owner = result_owner.ok_or(VerifyError::OperandArity(id))?;
                    table.owners.get_mut(&owner).unwrap().origin =
                        RecipeOrigin::SameIdentityView { inst: id, input };
                    let input_version = versions.version(input)?;
                    let definition = table.owners[&owner].definition_version;
                    table.versions[definition.0 as usize].cause = VersionCause::SameIdentity {
                        input: input_version,
                    };
                }
                _ => {
                    // Folded type guards on a fresh known Cons are pure
                    // identity views, not escapes requiring an allocation.
                    if try_cons_view(func, &mut table, &mut versions, id, &mut order)? {
                        continue;
                    }
                    // Own Cons field reads are removed only while virtual.
                    if try_cons_read(func, &mut table, &mut versions, id, &mut order, &mut stats)? {
                        continue;
                    }
                    for (index, &value) in data.args.iter().enumerate() {
                        let canonical =
                            func.resolve(value).ok_or(VerifyError::AliasCycle(value))?;
                        if table.owners.contains_key(&canonical) {
                            let word = versions.materialize(
                                func, &mut table, canonical, data.frame, data.pc, &mut order,
                                &mut stats,
                            )?;
                            let desired = original.values[value.index()].clone();
                            func.insts[id.index()].args[index] = word_view(
                                func,
                                word,
                                desired.ty,
                                desired.rep,
                                data.pc,
                                &mut order,
                            )?;
                        }
                    }
                }
            }
            versions.observe(func, &mut table, RecipePoint::Before(id), data.frame)?;
            order.push(id);
            if let Some(owner) = result_owner {
                let fields = match table.owners[&owner].kind {
                    RecipeKind::Number(_) => 4,
                    RecipeKind::Cons => 1,
                };
                pending_definition = Some((owner, fields));
            }
            versions.observe(func, &mut table, RecipePoint::After(id), data.frame)?;
        }
        if pending_definition.is_some() {
            return Err(PassError::Decline);
        }
        rewrite_term(
            func,
            &original,
            &selection,
            &mut table,
            &mut versions,
            block,
            &cache_phis,
            &mut order,
            &mut borrow_memo,
            &mut stats,
        )?;
        versions.observe(
            func,
            &mut table,
            RecipePoint::Term(block),
            match original.blocks[block.index()].term {
                Term::Deopt(frame) => Some(frame),
                _ => None,
            },
        )?;
        func.blocks[block.index()].insts = order;
        exits[block.index()] = versions.current.clone();
        processed[block.index()] = true;
    }
    // Fill original logical phi tuples first, then appended cache-only params
    // in EXACT actual parameter order. All arguments are real final SSA IDs.
    for &block in dom.reverse_postorder() {
        for &owner in &phis {
            let ValueDef::Param { block: target, .. } = original.values[owner.index()].def else {
                unreachable!()
            };
            if target != block {
                continue;
            }
            for &(source, edge_index) in &incoming[block.index()] {
                let original_edge = original.blocks[source.index()].term.edges()[edge_index];
                let ValueDef::Param { index, .. } = original.values[owner.index()].def else {
                    unreachable!()
                };
                let incoming_owner =
                    func.blocks[source.index()].term.edges()[edge_index].args[index as usize];
                let _ = original_edge; // independent verifier matches original source lineage
                let version = *exits[source.index()]
                    .get(&incoming_owner)
                    .ok_or(VerifyError::BadDefinition(incoming_owner))?;
                append_edge_fields(
                    func,
                    &mut table,
                    source,
                    edge_index,
                    owner,
                    incoming_owner,
                    version,
                )?;
            }
        }
        for &(owner, version) in &cache_phis[block.index()] {
            let inputs = incoming[block.index()]
                .iter()
                .map(|&(source, edge_index)| {
                    exits[source.index()]
                        .get(&owner)
                        .copied()
                        .map(|version| (source, edge_index, version))
                        .ok_or(VerifyError::BadDefinition(owner))
                })
                .collect::<Result<Vec<_>, _>>()?;
            fill_cache_phi(func, &mut table, version, &inputs)?;
        }
    }
    derive_source_views(func, &mut table, &mut versions)?;
    Ok((table, stats))
}

fn instruction_blocks(func: &Func) -> Vec<Block> {
    let mut result = vec![func.entry; func.insts.len()];
    for (block, data) in func.blocks.iter().enumerate() {
        for &inst in &data.insts {
            result[inst.index()] = Block(block as u32);
        }
    }
    result
}

fn incoming_edges(func: &Func) -> Vec<Vec<(Block, usize)>> {
    let mut result = vec![Vec::new(); func.blocks.len()];
    for (index, data) in func.blocks.iter().enumerate() {
        for (edge_index, edge) in data.term.edges().iter().enumerate() {
            result[edge.target.index()].push((Block(index as u32), edge_index));
        }
    }
    result
}

fn definition_block(func: &Func, blocks: &[Block], owner: Value) -> Result<Block, VerifyError> {
    let owner = func.resolve(owner).ok_or(VerifyError::AliasCycle(owner))?;
    match func.values[owner.index()].def {
        ValueDef::Inst(inst) => Ok(blocks[inst.index()]),
        ValueDef::Param { block, .. } => Ok(block),
        ValueDef::Alias(_) => Err(VerifyError::AliasCycle(owner)),
    }
}

fn discover(
    func: &Func,
    feedback: &[NumericFeedback],
    sqrt_sites: &HashSet<u32>,
    dom: &Dominance,
) -> Result<Selection, PassError> {
    let mut selected = Selection::default();
    let mut numeric_owners = HashSet::new();
    let mut work = 0usize;
    let incoming = incoming_edges(func);
    for &block in dom.reverse_postorder() {
        for &id in &func.blocks[block.index()].insts {
            let inst = &func.insts[id.index()];
            let Some(result) = inst.result else {
                continue;
            };
            let data = &func.values[result.index()];
            let numeric = matches!(
                inst.op,
                Opcode::Opaque(Op::Add | Op::Sub | Op::Mul | Op::Div)
            ) && inst.args.len() == 2
                && inst.frame.is_some()
                && inst.eff == Effects::ALLOCATES.with(Effects::MAY_DEOPT)
                && inst.mem == AliasClass::None
                && data.rep.is_tagged()
                && !data.ty.meet(TypeSet::FLOAT).is_bottom()
                && feedback.get(inst.pc as usize) == Some(&NumericFeedback::Float);
            let sqrt = matches!(inst.op, Opcode::Call { .. } | Opcode::Opaque(Op::Call(1)))
                && inst.args.len() == 2
                && inst.frame.is_some()
                && data.rep.is_tagged()
                && !data.ty.meet(TypeSet::FLOAT).is_bottom()
                && sqrt_sites.contains(&inst.pc);
            if numeric {
                selected.numeric.insert(id);
                numeric_owners.insert(result);
            }
            if sqrt {
                selected.sqrt.insert(id);
                numeric_owners.insert(result);
            }
            let cons = matches!(
                inst.op,
                Opcode::Opaque(Op::Cons | Op::List(1)) | Opcode::AllocCons
            ) && inst.eff == Effects::ALLOCATES
                && inst.mem == AliasClass::None
                && inst.frame.is_some()
                && data.rep == Rep::Tagged
                && !data.ty.is_bottom()
                && data.ty.is_subset(TypeSet::CONS)
                && inst
                    .args
                    .iter()
                    .all(|value| func.values[value.index()].rep.is_tagged());
            if cons {
                selected.cons.insert(id);
            }
        }
    }
    let source_operands = selected
        .numeric
        .iter()
        .chain(&selected.sqrt)
        .flat_map(|id| func.insts[id.index()].args.iter())
        .filter_map(|&value| func.resolve(value))
        .collect::<HashSet<_>>();
    // Bidirectional closure through ordinary Tagged params and pure exact views.
    // Unknown Tagged leaves are Borrow seeds, not Float proofs. Raw/unpaired
    // leaves cause bounded declination rather than an invented carrier cast.
    loop {
        let before = (selected.phis.len(), selected.views.len());
        for &block in dom.reverse_postorder() {
            for &param in &func.blocks[block.index()].params {
                work += 1;
                if work > WORK_LIMIT {
                    return Err(PassError::AnalysisLimit);
                }
                if !func.values[param.index()].rep.is_tagged() {
                    continue;
                }
                let ValueDef::Param { index, .. } = func.values[param.index()].def else {
                    unreachable!()
                };
                let incoming = incoming[block.index()]
                    .iter()
                    .map(|&(from, edge_index)| {
                        func.blocks[from.index()].term.edges()[edge_index].args[index as usize]
                    })
                    .collect::<Vec<_>>();
                let used_by_source = source_operands.contains(&param);
                if used_by_source
                    || incoming.iter().any(|value| {
                        func.resolve(*value)
                            .is_some_and(|value| numeric_owners.contains(&value))
                    })
                {
                    selected.phis.insert(param);
                    numeric_owners.insert(param);
                    // Backward phi dependencies must also carry full tuples.
                    for input in incoming {
                        let input = func.resolve(input).ok_or(VerifyError::AliasCycle(input))?;
                        if let ValueDef::Param { .. } = func.values[input.index()].def {
                            if !func.values[input.index()].rep.is_tagged() {
                                return Err(PassError::Decline);
                            }
                            selected.phis.insert(input);
                            numeric_owners.insert(input);
                        }
                    }
                }
            }
            for &id in &func.blocks[block.index()].insts {
                let data = &func.insts[id.index()];
                let Opcode::Refine(_) = data.op else {
                    continue;
                };
                let Some(output) = data.result else {
                    continue;
                };
                if data.eff != Effects::PURE
                    || data.mem != AliasClass::None
                    || data.args.len() != 1
                    || !func.values[output.index()].rep.is_tagged()
                {
                    continue;
                }
                let input = func
                    .resolve(data.args[0])
                    .ok_or(VerifyError::AliasCycle(data.args[0]))?;
                if numeric_owners.contains(&input) || source_operands.contains(&output) {
                    selected.views.insert(id);
                    numeric_owners.insert(output);
                    if let ValueDef::Param { .. } = func.values[input.index()].def {
                        if !func.values[input.index()].rep.is_tagged() {
                            return Err(PassError::Decline);
                        }
                        selected.phis.insert(input);
                        numeric_owners.insert(input);
                    }
                }
            }
        }
        if before == (selected.phis.len(), selected.views.len()) {
            break;
        }
    }
    // Seed reachability independently rejects ungrounded phi/view SCCs. Every
    // outside actual Tagged word is a Borrow seed, including unknown heap words.
    let incoming = incoming_edges(func);
    let mut grounded = numeric_owners
        .iter()
        .filter(|owner| {
            matches!(func.values[owner.index()].def,
        ValueDef::Inst(id) if selected.numeric.contains(&id) || selected.sqrt.contains(&id))
        })
        .copied()
        .collect::<HashSet<_>>();
    loop {
        let old = grounded.len();
        for &owner in &numeric_owners {
            let dependencies = match func.values[owner.index()].def {
                ValueDef::Param { block, index } => incoming[block.index()]
                    .iter()
                    .map(|&(source, edge)| {
                        func.blocks[source.index()].term.edges()[edge].args[index as usize]
                    })
                    .collect::<Vec<_>>(),
                ValueDef::Inst(id) if selected.views.contains(&id) => {
                    func.insts[id.index()].args.clone()
                }
                _ => continue,
            };
            for value in dependencies {
                work += 1;
                if work > WORK_LIMIT {
                    return Err(PassError::AnalysisLimit);
                }
                let value = func.resolve(value).ok_or(VerifyError::AliasCycle(value))?;
                if grounded.contains(&value)
                    || (!numeric_owners.contains(&value)
                        && func.values[value.index()].rep.is_tagged())
                {
                    grounded.insert(owner);
                }
                if !numeric_owners.contains(&value) && !func.values[value.index()].rep.is_tagged() {
                    return Err(PassError::Decline);
                }
            }
        }
        if old == grounded.len() {
            break;
        }
    }
    if numeric_owners.iter().any(|owner| !grounded.contains(owner)) {
        return Err(PassError::Decline);
    }
    // Cons initial scope has no cross-block logical lifetime. Exact original
    // source/frame/entry uses count; merely inspecting projected edges is wrong.
    let blocks = instruction_blocks(func);
    // Bound the repeated exact full-frame/outside-use audit before visiting it.
    let cells = func
        .insts
        .iter()
        .map(|inst| inst.args.len())
        .sum::<usize>()
        .saturating_add(
            func.source_states
                .iter()
                .flatten()
                .map(|state| state.pre.len() + state.post.len())
                .sum::<usize>(),
        )
        .saturating_add(
            func.entry_stacks
                .iter()
                .map(|stack| stack.len())
                .sum::<usize>(),
        )
        .saturating_add(
            func.frames
                .iter()
                .map(|frame| frame.stack.len() + 1)
                .sum::<usize>()
                .saturating_mul(func.insts.len() + func.blocks.len()),
        );
    if cells.saturating_mul(selected.cons.len()) > WORK_LIMIT {
        return Err(PassError::AnalysisLimit);
    }
    selected.cons.retain(|&id| {
        let owner = func.insts[id.index()].result.unwrap();
        !observed_elsewhere(func, owner, blocks[id.index()])
    });
    Ok(selected)
}

fn observed_elsewhere(func: &Func, owner: Value, definition: Block) -> bool {
    let is_owner = |value| func.resolve(value) == Some(owner);
    let frame_has = |mut next: Option<FrameId>| {
        while let Some(id) = next {
            let frame = &func.frames[id.index()];
            if frame.stack.iter().copied().any(is_owner) {
                return true;
            }
            next = frame.parent;
        }
        false
    };
    for (block, data) in func.blocks.iter().enumerate() {
        if Block(block as u32) == definition {
            continue;
        }
        if data.insts.iter().any(|id| {
            let inst = &func.insts[id.index()];
            inst.args.iter().copied().any(is_owner) || frame_has(inst.frame)
        }) || term_values(&data.term).into_iter().any(is_owner)
            || func
                .entry_stacks
                .get(block)
                .is_some_and(|values| values.iter().copied().any(is_owner))
            || matches!(data.term, Term::Deopt(frame) if frame_has(Some(frame)))
        {
            return true;
        }
    }
    func.source_states.iter().flatten().any(|state| {
        state.block != definition
            && state
                .pre
                .iter()
                .chain(state.post.iter())
                .copied()
                .any(is_owner)
    })
}

fn project(
    func: &mut Func,
    owner: Value,
    component: RecipeField,
    ty: TypeSet,
    rep: Rep,
    pc: u32,
    order: &mut Vec<Inst>,
) -> Value {
    append(
        func,
        Opcode::Sink(SinkOp::RecipeField(component)),
        vec![owner],
        ty,
        rep,
        Effects::PURE,
        None,
        pc,
        order,
    )
}

fn numeric_fields(
    func: &mut Func,
    owner: Value,
    pc: u32,
    box_ty: TypeSet,
    order: &mut Vec<Inst>,
) -> NumericFields {
    NumericFields {
        payload: project(
            func,
            owner,
            RecipeField::Payload,
            TypeSet::FLOAT,
            Rep::RawF64,
            pc,
            order,
        ),
        word: project(
            func,
            owner,
            RecipeField::Word,
            TypeSet::TOP,
            Rep::RawWord,
            pc,
            order,
        ),
        ready: project(
            func,
            owner,
            RecipeField::Ready,
            TypeSet::BOOLEAN,
            Rep::Bool,
            pc,
            order,
        ),
        real_box: project(
            func,
            owner,
            RecipeField::RealBox,
            box_ty,
            Rep::Tagged,
            pc,
            order,
        ),
    }
}

fn record_owner(
    table: &mut SinkRecipes,
    owner: Value,
    kind: RecipeKind,
    ty: TypeSet,
    origin: RecipeOrigin,
    fields: RecipeFields,
    cause: VersionCause,
) {
    let version = append_version(table, owner, fields, cause);
    table.owners.insert(
        owner,
        OwnerRecipe {
            owner,
            kind,
            semantic_type: ty,
            origin,
            definition_version: version,
        },
    );
}

fn rewrite_sqrt(
    func: &mut Func,
    table: &mut SinkRecipes,
    id: Inst,
    order: &mut Vec<Inst>,
) -> Result<(), VerifyError> {
    let original = func.insts[id.index()].clone();
    let owner = original.result.ok_or(VerifyError::OperandArity(id))?;
    let frame = original.frame.ok_or(VerifyError::MissingFrame(id))?;
    let ty = func.values[owner.index()].ty.meet(TypeSet::FLOAT);
    if ty.is_bottom() {
        return Err(VerifyError::TypeMismatch(owner));
    }
    func.insts[id.index()].op = Opcode::Sink(SinkOp::SourceSqrt);
    func.values[owner.index()].ty = ty;
    func.values[owner.index()].rep = Rep::NumPair;
    order.push(id);
    let fields = numeric_fields(func, owner, original.pc, TypeSet::NIL, order);
    record_owner(
        table,
        owner,
        RecipeKind::Number(NumericMode::FloatOnly),
        ty,
        RecipeOrigin::NumericSource {
            inst: id,
            original_op: Op::Call(1),
            frame,
            pc: original.pc,
        },
        RecipeFields::Number(fields),
        VersionCause::Definition,
    );
    Ok(())
}

fn nil_value(func: &mut Func, pc: u32, order: &mut Vec<Inst>) -> Value {
    let bits = ValueBits::from_value(LispValue::NIL);
    let index = func
        .consts
        .iter()
        .enumerate()
        .skip(func.dynamic_prefix)
        .find_map(|(index, value)| (*value == bits).then_some(index))
        .unwrap_or_else(|| {
            let mut values = func.consts.to_vec();
            let index = values.len();
            values.push(bits);
            func.consts = values.into();
            index
        });
    append(
        func,
        Opcode::Const(index as u32),
        vec![],
        TypeSet::NIL,
        Rep::Tagged,
        Effects::PURE,
        None,
        pc,
        order,
    )
}

fn rewrite_cons(
    func: &mut Func,
    table: &mut SinkRecipes,
    id: Inst,
    order: &mut Vec<Inst>,
) -> Result<(), VerifyError> {
    let original = func.insts[id.index()].clone();
    let owner = original.result.ok_or(VerifyError::OperandArity(id))?;
    let frame = original.frame.ok_or(VerifyError::MissingFrame(id))?;
    let (op, car, cdr) = match original.op {
        Opcode::Opaque(Op::Cons) | Opcode::AllocCons if original.args.len() == 2 => {
            (Op::Cons, original.args[0], original.args[1])
        }
        Opcode::Opaque(Op::List(1)) if original.args.len() == 1 => (
            Op::List(1),
            original.args[0],
            nil_value(func, original.pc, order),
        ),
        _ => return Err(VerifyError::InvalidInst(id)),
    };
    func.insts[id.index()].op = Opcode::Sink(SinkOp::SourceCons(op.clone()));
    func.values[owner.index()].rep = Rep::Virtual(AllocId(id.0));
    order.push(id);
    let real_box = project(
        func,
        owner,
        RecipeField::RealBox,
        TypeSet::NIL,
        Rep::Tagged,
        original.pc,
        order,
    );
    record_owner(
        table,
        owner,
        RecipeKind::Cons,
        TypeSet::CONS,
        RecipeOrigin::ConsSource {
            inst: id,
            original_op: op,
            frame,
            pc: original.pc,
        },
        RecipeFields::Cons(ConsFields { car, cdr, real_box }),
        VersionCause::Definition,
    );
    Ok(())
}

fn rewrite_view(
    func: &mut Func,
    table: &mut SinkRecipes,
    id: Inst,
    order: &mut Vec<Inst>,
) -> Result<(), VerifyError> {
    let data = func.insts[id.index()].clone();
    let owner = data.result.ok_or(VerifyError::OperandArity(id))?;
    let input = func
        .resolve(data.args[0])
        .ok_or(VerifyError::AliasCycle(data.args[0]))?;
    let ty = func.values[owner.index()].ty;
    func.values[owner.index()].rep = Rep::NumPair;
    order.push(id);
    let fields = numeric_fields(func, owner, data.pc, ty.join(TypeSet::NIL), order);
    // Exact current input version is filled by the second source-order walk.
    record_owner(
        table,
        owner,
        RecipeKind::Number(NumericMode::Borrowable),
        ty,
        RecipeOrigin::SameIdentityView { inst: id, input },
        RecipeFields::Number(fields),
        VersionCause::Definition,
    );
    Ok(())
}

fn ensure_number(
    func: &mut Func,
    table: &mut SinkRecipes,
    versions: &mut Versions,
    value: Value,
    pc: u32,
    order: &mut Vec<Inst>,
    memo: &mut HashMap<Value, Value>,
) -> Result<Value, PassError> {
    let value = func.resolve(value).ok_or(VerifyError::AliasCycle(value))?;
    if table.owners.contains_key(&value) {
        if matches!(table.owners[&value].kind, RecipeKind::Number(_)) {
            return Ok(value);
        }
        return Err(PassError::Decline);
    }
    if let Some(&owner) = memo.get(&value) {
        return Ok(owner);
    }
    if !func.values[value.index()].rep.is_tagged() {
        return Err(PassError::Decline);
    }
    let start = order.len();
    let owner = borrow_word(func, table, value, pc, order)?;
    let created = order[start..].to_vec();
    // Borrow's contiguous complete unit is the only newly available owner.
    for (index, id) in created.iter().copied().enumerate() {
        versions.observe(func, table, RecipePoint::Before(id), None)?;
        if index + 1 == created.len() {
            versions.define(table, owner);
        }
        versions.observe(func, table, RecipePoint::After(id), None)?;
    }
    memo.insert(value, owner);
    Ok(owner)
}

fn try_cons_read(
    func: &mut Func,
    table: &mut SinkRecipes,
    versions: &mut Versions,
    id: Inst,
    order: &mut Vec<Inst>,
    stats: &mut SinkStats,
) -> Result<bool, PassError> {
    let old = func.insts[id.index()].clone();
    let field = match old.op {
        Opcode::LoadCar | Opcode::Opaque(Op::Car | Op::CarSafe) => 0,
        Opcode::LoadCdr | Opcode::Opaque(Op::Cdr | Op::CdrSafe) => 1,
        _ => return Ok(false),
    };
    if old.args.len() != 1
        || old.result.is_none()
        || !(old.eff == Effects::READ_HEAP
            || old.eff == Effects::READ_HEAP.with(Effects::MAY_DEOPT))
        || old.mem
            != if field == 0 {
                AliasClass::ConsCar
            } else {
                AliasClass::ConsCdr
            }
    {
        return Ok(false);
    }
    let owner = func
        .resolve(old.args[0])
        .ok_or(VerifyError::AliasCycle(old.args[0]))?;
    if !matches!(
        table.owners.get(&owner).map(|owner| owner.kind),
        Some(RecipeKind::Cons)
    ) {
        return Ok(false);
    }
    let version = versions.version(owner)?;
    if guaranteed_word(table, version, WORK_LIMIT)? {
        return Ok(false);
    }
    let RecipeFields::Cons(fields) = table.versions[version.0 as usize].fields else {
        unreachable!()
    };
    let value = if field == 0 { fields.car } else { fields.cdr };
    let value = func.resolve(value).ok_or(VerifyError::AliasCycle(value))?;
    let result = old.result.unwrap();
    let ty = func.values[result.index()]
        .ty
        .meet(func.values[value.index()].ty);
    if ty.is_bottom() {
        return Ok(false);
    }
    let rep = func.values[value.index()].rep;
    // Logical child identity remains the original read result ID. Ordinary
    // Tagged fields keep a same-ID typed same-word Refine.
    func.insts[id.index()].op = Opcode::Refine(ty);
    func.insts[id.index()].args = vec![value];
    func.insts[id.index()].eff = Effects::PURE;
    func.insts[id.index()].mem = AliasClass::None;
    func.insts[id.index()].frame = None;
    func.values[result.index()].ty = ty;
    func.values[result.index()].rep = rep;
    versions.observe(func, table, RecipePoint::Before(id), old.frame)?;
    order.push(id);
    if table.owners.contains_key(&value) {
        let child = versions.version(value)?;
        let fields = table.versions[child.0 as usize].fields;
        let kind = table.owners[&value].kind;
        record_owner(
            table,
            result,
            kind,
            ty,
            RecipeOrigin::SameIdentityView {
                inst: id,
                input: value,
            },
            fields,
            VersionCause::SameIdentity { input: child },
        );
        versions.add_alias(result, value);
        versions.define(table, result);
    }
    versions.observe(func, table, RecipePoint::After(id), old.frame)?;
    stats.cons_reads_elided += 1;
    Ok(true)
}

fn try_cons_view(
    func: &mut Func,
    table: &mut SinkRecipes,
    versions: &mut Versions,
    id: Inst,
    order: &mut Vec<Inst>,
) -> Result<bool, PassError> {
    let old = func.insts[id.index()].clone();
    if !matches!(old.op, Opcode::Refine(_))
        || old.eff != Effects::PURE
        || old.mem != AliasClass::None
        || old.args.len() != 1
    {
        return Ok(false);
    }
    let Some(result) = old.result else {
        return Ok(false);
    };
    let input = func
        .resolve(old.args[0])
        .ok_or(VerifyError::AliasCycle(old.args[0]))?;
    let Some(info) = table.owners.get(&input) else {
        return Ok(false);
    };
    if info.kind != RecipeKind::Cons || !func.values[result.index()].rep.is_tagged() {
        return Ok(false);
    }
    let ty = func.values[result.index()].ty.meet(info.semantic_type);
    // Retain an ordinary escape for contradictory or genuinely narrowing
    // views. The virtual Cons declaration itself must prove this identity.
    if ty.is_bottom() || !info.semantic_type.is_subset(ty) {
        return Ok(false);
    }
    let version = versions.version(input)?;
    let fields = table.versions[version.0 as usize].fields;
    versions.observe(func, table, RecipePoint::Before(id), old.frame)?;
    func.insts[id.index()].op = Opcode::Refine(ty);
    func.insts[id.index()].args = vec![input];
    func.insts[id.index()].frame = None;
    func.values[result.index()].ty = ty;
    func.values[result.index()].rep = func.values[input.index()].rep;
    order.push(id);
    record_owner(
        table,
        result,
        RecipeKind::Cons,
        ty,
        RecipeOrigin::SameIdentityView { inst: id, input },
        fields,
        VersionCause::SameIdentity { input: version },
    );
    versions.add_alias(result, input);
    versions.define(table, result);
    versions.observe(func, table, RecipePoint::After(id), old.frame)?;
    Ok(true)
}

fn semantic_key(func: &Func, selected: &Selection, mut value: Value) -> Result<Value, VerifyError> {
    for _ in 0..=func.values.len() {
        value = func.resolve(value).ok_or(VerifyError::AliasCycle(value))?;
        match func.values[value.index()].def {
            ValueDef::Inst(id) if selected.views.contains(&id) => {
                value = func.insts[id.index()].args[0]
            }
            _ => return Ok(value),
        }
    }
    Err(VerifyError::AliasCycle(value))
}

/// Exact all-edge signature proof. This deliberately declines more involved
/// coinductive alias cases instead of using payload equality/site identity.
fn alias_groups(
    func: &Func,
    selected: &Selection,
    incoming: &[Vec<(Block, usize)>],
) -> Result<HashMap<Value, Vec<Value>>, VerifyError> {
    let mut parent = HashMap::<Value, Value>::new();
    for &id in selected
        .numeric
        .iter()
        .chain(&selected.sqrt)
        .chain(&selected.views)
    {
        let owner = func.insts[id.index()].result.unwrap();
        parent.insert(owner, owner);
    }
    for &owner in &selected.phis {
        parent.insert(owner, owner);
    }
    let mut signatures = HashMap::<(Block, Vec<Value>), Value>::new();
    let mut phis = selected.phis.iter().copied().collect::<Vec<_>>();
    phis.sort_unstable();
    for owner in phis {
        let ValueDef::Param { block, index } = func.values[owner.index()].def else {
            unreachable!()
        };
        let signature = incoming[block.index()]
            .iter()
            .map(|&(source, edge_index)| {
                semantic_key(
                    func,
                    selected,
                    func.blocks[source.index()].term.edges()[edge_index].args[index as usize],
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(&leader) = signatures.get(&(block, signature.clone())) {
            parent.insert(owner, leader);
        } else {
            signatures.insert((block, signature), owner);
        }
    }
    for &id in &selected.views {
        let owner = func.insts[id.index()].result.unwrap();
        parent.insert(owner, semantic_key(func, selected, owner)?);
    }
    let leader = |mut value: Value| {
        for _ in 0..=parent.len() {
            let next = parent.get(&value).copied().unwrap_or(value);
            if next == value {
                return value;
            }
            value = next;
        }
        value
    };
    let mut groups = HashMap::<Value, Vec<Value>>::new();
    for &owner in parent.keys() {
        groups.entry(leader(owner)).or_default().push(owner);
    }
    let mut result = HashMap::new();
    for (_, mut group) in groups {
        group.sort_unstable();
        for &owner in &group {
            result.insert(owner, group.clone());
        }
    }
    Ok(result)
}

/// Actual full-stack/frame/entry/term uses, propagated backwards to definition
/// blocks. This is proof/liveness planning, not heap-root publication.
fn live_owner_blocks(
    func: &Func,
    selected: &Selection,
    dom: &Dominance,
) -> Result<Vec<HashSet<Value>>, PassError> {
    let mut owners = selected.phis.clone();
    owners.extend(
        selected
            .numeric
            .iter()
            .chain(&selected.sqrt)
            .chain(&selected.views)
            .filter_map(|id| func.insts[id.index()].result),
    );
    let blocks = instruction_blocks(func);
    let incoming = incoming_edges(func);
    let aliases = alias_groups(func, selected, &incoming)?;
    let mut live = vec![HashSet::new(); func.blocks.len()];
    let mut work = 0usize;
    let mut uses = Vec::<(Block, Value)>::new();
    for &block in dom.reverse_postorder() {
        for &id in &func.blocks[block.index()].insts {
            let inst = &func.insts[id.index()];
            uses.extend(inst.args.iter().copied().map(|value| (block, value)));
            let mut frame = inst.frame;
            while let Some(id) = frame {
                let data = &func.frames[id.index()];
                uses.extend(data.stack.iter().copied().map(|value| (block, value)));
                frame = data.parent;
                work += data.stack.len() + 1;
                if work > WORK_LIMIT {
                    return Err(PassError::AnalysisLimit);
                }
            }
        }
        uses.extend(
            term_values(&func.blocks[block.index()].term)
                .into_iter()
                .map(|value| (block, value)),
        );
        if let Some(stack) = func.entry_stacks.get(block.index()) {
            uses.extend(stack.iter().copied().map(|value| (block, value)));
        }
    }
    for state in func.source_states.iter().flatten() {
        uses.extend(
            state
                .pre
                .iter()
                .chain(&state.post)
                .copied()
                .map(|value| (state.block, value)),
        );
    }
    let mut pending = VecDeque::new();
    for (block, value) in uses {
        work += 1;
        if work > WORK_LIMIT {
            return Err(PassError::AnalysisLimit);
        }
        let value = func.resolve(value).ok_or(VerifyError::AliasCycle(value))?;
        if owners.contains(&value) {
            for &owner in aliases.get(&value).map(Vec::as_slice).unwrap_or(&[value]) {
                pending.push_back((block, owner));
            }
        }
    }
    while let Some((block, owner)) = pending.pop_front() {
        work += 1;
        if work > WORK_LIMIT {
            return Err(PassError::AnalysisLimit);
        }
        let definition = definition_block(func, &blocks, owner)?;
        if !dom.dominates(definition, block)
            || !live[block.index()].insert(owner)
            || block == definition
        {
            continue;
        }
        pending.extend(
            incoming[block.index()]
                .iter()
                .map(|&(source, _)| (source, owner)),
        );
    }
    Ok(live)
}

fn term_values(term: &Term) -> Vec<Value> {
    let mut values = match term {
        Term::Return(value) | Term::Branch { flag: value, .. } => vec![*value],
        Term::Switch { value, table, .. } => vec![*value, *table],
        _ => vec![],
    };
    for edge in term.edges() {
        values.extend_from_slice(&edge.args);
    }
    values
}

fn rewrite_term(
    func: &mut Func,
    original: &Func,
    selected: &Selection,
    table: &mut SinkRecipes,
    versions: &mut Versions,
    block: Block,
    cache_phis: &[Vec<(Value, RecipeVersionId)>],
    order: &mut Vec<Inst>,
    memo: &mut HashMap<Value, Value>,
    stats: &mut SinkStats,
) -> Result<(), PassError> {
    let source = original
        .source_states
        .iter()
        .enumerate()
        .rev()
        .find_map(|(pc, state)| {
            state
                .as_ref()
                .filter(|state| state.block == block)
                .map(|state| (pc as u32, state.frame))
        });
    let (pc, frame) = source
        .map(|(pc, frame)| (pc, Some(frame)))
        .unwrap_or((original.blocks[block.index()].pc, None));
    let word = |value: Value,
                func: &mut Func,
                table: &mut SinkRecipes,
                versions: &mut Versions,
                order: &mut Vec<Inst>,
                stats: &mut SinkStats|
     -> Result<Value, PassError> {
        let canonical = func.resolve(value).ok_or(VerifyError::AliasCycle(value))?;
        if !table.owners.contains_key(&canonical) {
            return Ok(value);
        }
        let boxed = versions.materialize(func, table, canonical, frame, pc, order, stats)?;
        let target = &original.values[value.index()];
        word_view(func, boxed, target.ty, target.rep, pc, order)
    };
    let mut term = func.blocks[block.index()].term.clone();
    match &mut term {
        Term::Return(value) | Term::Branch { flag: value, .. } => {
            *value = word(*value, func, table, versions, order, stats)?
        }
        Term::Switch {
            value,
            table: dispatch,
            ..
        } => {
            *value = word(*value, func, table, versions, order, stats)?;
            *dispatch = word(*dispatch, func, table, versions, order, stats)?;
        }
        _ => {}
    }
    func.blocks[block.index()].term = term;
    for edge_index in 0..original.blocks[block.index()].term.edges().len() {
        let old_edge = original.blocks[block.index()].term.edges()[edge_index];
        let mut shared = HashMap::<Value, Vec<Value>>::new();
        for (index, &value) in old_edge.args.iter().enumerate() {
            let param = original.blocks[old_edge.target.index()].params[index];
            if selected.phis.contains(&param) {
                let incoming = ensure_number(func, table, versions, value, pc, order, memo)?;
                let edge = edge_mut(&mut func.blocks[block.index()].term, edge_index).unwrap();
                edge.args[index] = incoming;
                shared.entry(incoming).or_default().push(param);
            } else {
                let value = word(value, func, table, versions, order, stats)?;
                edge_mut(&mut func.blocks[block.index()].term, edge_index)
                    .unwrap()
                    .args[index] = value;
            }
        }
        // A dominating logical owner is also carried into this join, through
        // its preallocated cache-only phi. It can alias an original logical
        // phi on only this edge; the two outgoing owners are then distinct
        // globally but must receive one actual cached object on this edge.
        // Only exact identity/SameIdentity groups participate, never payloads.
        for &(dominating, _) in &cache_phis[old_edge.target.index()] {
            let mut identities = versions
                .aliases
                .get(&dominating)
                .cloned()
                .unwrap_or_default();
            versions.spend(identities.len() + 1)?;
            identities.push(dominating);
            identities.sort_unstable();
            identities.dedup();
            for incoming in identities {
                if let Some(owners) = shared.get_mut(&incoming) {
                    if !owners.contains(&dominating) {
                        owners.push(dominating);
                    }
                }
            }
        }
        // One-edge aliases must not become independent unboxed phi identities.
        // Cache once on the actual shared incoming identity. Materializing at
        // predecessor Term is safe: exact incoming owner is available there;
        // no original effects/error/quit observations are moved.
        for (incoming, owners) in shared {
            if owners.len() < 2 {
                continue;
            }
            let all_equivalent = versions
                .aliases
                .get(&owners[0])
                .is_some_and(|group| owners.iter().all(|owner| group.contains(owner)));
            if !all_equivalent {
                versions.materialize(func, table, incoming, frame, pc, order, stats)?;
            }
        }
    }
    Ok(())
}

/// Use the same monotone prefix cuts as ordinary Func source dominance. Source
/// snapshots select the map at the real final boundary, not a pc-only mutable
/// descriptor. Complete source tuples exist before SourcePost is reached.
fn derive_source_views(
    func: &Func,
    table: &mut SinkRecipes,
    versions: &mut Versions,
) -> Result<(), PassError> {
    let mut positions = vec![(0usize, 0usize); func.blocks.len()];
    versions.spend(table.uses.len())?;
    let mut cuts = HashMap::<RecipePoint, HashMap<Value, RecipeVersionId>>::new();
    for (&(point, owner), &version) in &table.uses {
        cuts.entry(point).or_default().insert(owner, version);
    }
    for (pc, source) in func.source_states.iter().enumerate() {
        let Some(source) = source else {
            continue;
        };
        let block = &func.blocks[source.block.index()];
        let (pre, post) = &mut positions[source.block.index()];
        while let Some(&id) = block.insts.get(*pre) {
            let inst = &func.insts[id.index()];
            if inst.pc >= pc as u32 && !matches!(inst.op, Opcode::Arg(_) | Opcode::OsrSlot(_)) {
                break;
            }
            *pre += 1;
        }
        while let Some(&id) = block.insts.get(*post) {
            let inst = &func.insts[id.index()];
            if inst.pc > pc as u32 && !matches!(inst.op, Opcode::Arg(_) | Opcode::OsrSlot(_)) {
                break;
            }
            *post += 1;
        }
        for (position, point) in [
            (*pre, RecipePoint::SourcePre(pc as u32)),
            (*post, RecipePoint::SourcePost(pc as u32)),
        ] {
            let cut = block
                .insts
                .get(position)
                .map(|&inst| RecipePoint::Before(inst))
                .unwrap_or(RecipePoint::Term(source.block));
            versions.current = cuts.get(&cut).cloned().unwrap_or_default();
            versions.observe(func, table, point, Some(source.frame))?;
        }
    }
    Ok(())
}
