//! Executable-graph metadata and post-folding SSA compaction.
//!
//! Threading: all graphs and maps belong exclusively to one compilation. The
//! derived dominance metadata is immutable after construction, contains only
//! compiler ids, and reads no Lisp objects or mutator-local state.

use super::super::ir::*;
use super::super::verify::VerifyError;

/// Cooper immediate dominators with constant-time dominator-tree intervals.
/// Storage is O(blocks + edges) while constructing and O(blocks) afterwards.
/// Threading: immutable compiler-owned metadata; no runtime state or caches.
pub(crate) struct Dominance {
    enter: Vec<usize>,
    exit: Vec<usize>,
    idom: Vec<Option<Block>>,
    order: Vec<Block>,
}

impl Dominance {
    pub(crate) fn new(func: &Func) -> Result<Self, VerifyError> {
        let count = func.blocks.len();
        if func.entry.index() >= count {
            return Err(VerifyError::InvalidEntry(func.entry));
        }
        let mut preds = vec![Vec::new(); count];
        for (index, block) in func.blocks.iter().enumerate() {
            for edge in block.term.edges() {
                let incoming = preds
                    .get_mut(edge.target.index())
                    .ok_or(VerifyError::InvalidBlock(edge.target))?;
                incoming.push(Block(index as u32));
            }
        }
        let mut visited = vec![false; count];
        let mut order = Vec::new();
        let mut walk = vec![(func.entry, false)];
        while let Some((block, exiting)) = walk.pop() {
            if exiting {
                order.push(block);
                continue;
            }
            if std::mem::replace(&mut visited[block.index()], true) {
                continue;
            }
            walk.push((block, true));
            for edge in func.blocks[block.index()].term.edges().into_iter().rev() {
                if !visited[edge.target.index()] {
                    walk.push((edge.target, false));
                }
            }
        }
        order.reverse();
        let mut rank = vec![usize::MAX; count];
        for (index, &block) in order.iter().enumerate() {
            rank[block.index()] = index;
        }
        let mut idom = vec![None; count];
        idom[func.entry.index()] = Some(func.entry);
        let mut changed = true;
        while changed {
            changed = false;
            for &block in order.iter().skip(1) {
                let mut incoming = preds[block.index()]
                    .iter()
                    .copied()
                    .filter(|p| idom[p.index()].is_some());
                let Some(mut parent) = incoming.next() else {
                    continue;
                };
                for mut other in incoming {
                    while parent != other {
                        while rank[parent.index()] > rank[other.index()] {
                            parent = idom[parent.index()].expect("known dominator");
                        }
                        while rank[other.index()] > rank[parent.index()] {
                            other = idom[other.index()].expect("known dominator");
                        }
                    }
                }
                if idom[block.index()] != Some(parent) {
                    idom[block.index()] = Some(parent);
                    changed = true;
                }
            }
        }
        let mut children = vec![Vec::new(); count];
        for (index, parent) in idom.iter().enumerate() {
            let block = Block(index as u32);
            if let Some(parent) = *parent
                && parent != block
            {
                children[parent.index()].push(block);
            }
        }
        let mut enter = vec![usize::MAX; count];
        let mut exit = vec![usize::MAX; count];
        let mut walk = vec![(func.entry, false)];
        let mut clock = 0;
        while let Some((block, exiting)) = walk.pop() {
            if exiting {
                exit[block.index()] = clock;
            } else {
                enter[block.index()] = clock;
                walk.push((block, true));
                walk.extend(children[block.index()].iter().rev().map(|&b| (b, false)));
            }
            clock += 1;
        }
        Ok(Self {
            enter,
            exit,
            idom,
            order,
        })
    }

    pub(crate) fn is_reachable(&self, block: Block) -> bool {
        self.enter
            .get(block.index())
            .is_some_and(|&n| n != usize::MAX)
    }

    pub(crate) fn dominates(&self, definition: Block, block: Block) -> bool {
        self.is_reachable(definition)
            && self.is_reachable(block)
            && self.enter[definition.index()] <= self.enter[block.index()]
            && self.exit[block.index()] <= self.exit[definition.index()]
    }

    pub(crate) fn immediate_dominator(&self, block: Block) -> Option<Block> {
        self.idom
            .get(block.index())
            .copied()
            .flatten()
            .filter(|&p| p != block)
    }

    pub(crate) fn reverse_postorder(&self) -> &[Block] {
        &self.order
    }
}

/// Compact a graph after an explicitly selected optimization rewrites control
/// flow. Every attached instruction and parameter in an executable block is
/// retained, including unused pure results: this function performs no DCE.
/// Source states are removed only with their block; the transforming pass must
/// explicitly clear later states when it truncates a block before a guard.
/// A live stack/frame reference to a removed definition is an error.
///
/// Threading: the candidate and all remapping scratch are exclusively owned by
/// this compiler invocation. Only a verified candidate replaces `func`, so a
/// failure leaves the caller's input unchanged. No Lisp bits are dereferenced.
pub(crate) fn cleanup(func: &mut Func) -> Result<(), VerifyError> {
    let dom = Dominance::new(func)?;
    let mut block_map = vec![None; func.blocks.len()];
    let mut count = 0;
    for (index, mapped) in block_map.iter_mut().enumerate() {
        if dom.is_reachable(Block(index as u32)) {
            *mapped = Some(Block(count));
            count += 1;
        }
    }

    let mut live_insts = vec![false; func.insts.len()];
    let mut live_values = vec![false; func.values.len()];
    for (index, block) in func.blocks.iter().enumerate() {
        if block_map[index].is_none() {
            continue;
        }
        let owner = Block(index as u32);
        for (index, &value) in block.params.iter().enumerate() {
            retain_definition(
                func,
                value,
                ValueDef::Param {
                    block: owner,
                    index: index as u32,
                },
                &mut live_values,
            )?;
        }
        for &inst in &block.insts {
            let data = func
                .insts
                .get(inst.index())
                .ok_or(VerifyError::InvalidInst(inst))?;
            if std::mem::replace(&mut live_insts[inst.index()], true) {
                return Err(VerifyError::DuplicateInst(inst));
            }
            if let Some(value) = data.result {
                retain_definition(func, value, ValueDef::Inst(inst), &mut live_values)?;
            }
        }
    }
    let mut inst_map = vec![None; func.insts.len()];
    let mut count = 0;
    for (index, &live) in live_insts.iter().enumerate() {
        if live {
            inst_map[index] = Some(Inst(count));
            count += 1;
        }
    }
    let mut value_map = vec![None; func.values.len()];
    let mut count = 0;
    for (index, &live) in live_values.iter().enumerate() {
        if live {
            value_map[index] = Some(Value(count));
            count += 1;
        }
    }
    let canonical = resolve_aliases(func)?;
    let mut candidate = Func::new(func.consts.clone(), func.arity, func.dynamic_prefix);
    candidate.entry = mapped_block(func.entry, &block_map)?;
    candidate.osr = func
        .osr
        .as_ref()
        .map(|osr| -> Result<OsrEntry, VerifyError> {
            let header = block_map
                .get(osr.header.index())
                .copied()
                .flatten()
                .ok_or(VerifyError::OsrShape)?;
            Ok(OsrEntry {
                header,
                entry_pc: osr.entry_pc,
                depth: osr.depth,
            })
        })
        .transpose()?;

    for (index, value) in func.values.iter().enumerate() {
        if !live_values[index] {
            continue;
        }
        let def = match value.def {
            ValueDef::Param { block, index } => ValueDef::Param {
                block: mapped_block(block, &block_map)?,
                index,
            },
            ValueDef::Inst(inst) => ValueDef::Inst(mapped_inst(inst, &inst_map)?),
            ValueDef::Alias(_) => return Err(VerifyError::BadDefinition(Value(index as u32))),
        };
        let rep = match value.rep {
            Rep::RawPtr { base } => Rep::RawPtr {
                base: mapped_value(base, &canonical, &value_map)?,
            },
            // Allocation ids are semantic identities, not indexes into a
            // compacted table. Keeping them preserves virtual-object aliasing.
            rep => rep,
        };
        candidate.values.push(ValueData {
            def,
            rep,
            ty: value.ty,
        });
    }

    let mut frame_roots = Vec::new();
    for (index, data) in func.insts.iter().enumerate() {
        if live_insts[index]
            && let Some(frame) = data.frame
        {
            frame_roots.push(frame);
        }
    }
    for (index, block) in func.blocks.iter().enumerate() {
        if block_map[index].is_some()
            && let Term::Deopt(frame) = block.term
        {
            frame_roots.push(frame);
        }
    }
    let mut source_owners = vec![false; func.blocks.len()];
    for (pc, source) in func.source_states.iter().enumerate() {
        if let Some(source) = source {
            let owner = source_owners
                .get_mut(source.block.index())
                .ok_or(VerifyError::SourceState(pc as u32))?;
            *owner = true;
            if block_map[source.block.index()].is_some() {
                frame_roots.push(source.frame);
            }
        }
    }
    let mut frame_map = vec![None; func.frames.len()];
    for frame in ordered_frames(func, &frame_roots)? {
        let data = &func.frames[frame.index()];
        let parent = data
            .parent
            .map(|p| mapped_frame(p, &frame_map))
            .transpose()?;
        let stack = mapped_values(&data.stack, &canonical, &value_map)?.into_boxed_slice();
        frame_map[frame.index()] = Some(candidate.intern_frame(FrameState {
            pc: data.pc,
            stack,
            handlers: data.handlers,
            binds: data.binds,
            parent,
            site: data.site,
        }));
    }
    for (index, data) in func.insts.iter().enumerate() {
        if live_insts[index] {
            candidate.insts.push(InstData {
                op: data.op.clone(),
                args: mapped_values(&data.args, &canonical, &value_map)?,
                result: data
                    .result
                    .map(|v| mapped_value(v, &canonical, &value_map))
                    .transpose()?,
                eff: data.eff,
                mem: data.mem,
                frame: data
                    .frame
                    .map(|f| mapped_frame(f, &frame_map))
                    .transpose()?,
                pc: data.pc,
            });
        }
    }
    for (index, data) in func.blocks.iter().enumerate() {
        if let Some(owner) = block_map[index] {
            candidate.blocks.push(BlockData {
                params: mapped_values(&data.params, &canonical, &value_map)?,
                insts: data
                    .insts
                    .iter()
                    .map(|&i| mapped_inst(i, &inst_map))
                    .collect::<Result<_, _>>()?,
                term: mapped_term(&data.term, &block_map, &canonical, &value_map, &frame_map)?,
                preds: Vec::new(),
                pc: data.pc,
                // Preserve surviving loop-header hints. They are not a proof
                // that a cycle remains after folding; loop passes use edges.
                loop_header: data.loop_header.map(|_| LoopId(owner.0)),
                cold: data.cold,
            });
        }
    }
    if !func.entry_stacks.is_empty() {
        if func.entry_stacks.len() != func.blocks.len() {
            return Err(VerifyError::SourceState(0));
        }
        for (index, stack) in func.entry_stacks.iter().enumerate() {
            if block_map[index].is_some() {
                candidate
                    .entry_stacks
                    .push(mapped_values(stack, &canonical, &value_map)?.into_boxed_slice());
            }
        }
    }
    for source in &func.source_states {
        candidate.source_states.push(match source {
            Some(source) if block_map[source.block.index()].is_some() => Some(SourceState {
                pre: mapped_values(&source.pre, &canonical, &value_map)?.into_boxed_slice(),
                post: mapped_values(&source.post, &canonical, &value_map)?.into_boxed_slice(),
                frame: mapped_frame(source.frame, &frame_map)?,
                block: mapped_block(source.block, &block_map)?,
            }),
            _ => None,
        });
    }
    let mut preds = vec![Vec::new(); candidate.blocks.len()];
    for (index, block) in candidate.blocks.iter().enumerate() {
        for edge in block.term.edges() {
            preds[edge.target.index()].push(Block(index as u32));
        }
    }
    for (block, mut incoming) in candidate.blocks.iter_mut().zip(preds) {
        incoming.sort_unstable();
        incoming.dedup();
        block.preds = incoming;
    }
    // Builder census records split sites. With the pass enabled this field
    // records actual remaining graph critical edges, with switch duplicates
    // counted as one directed edge. The default-none builder is unchanged.
    let critical_edges = candidate
        .blocks
        .iter()
        .map(|block| {
            let mut targets = block
                .term
                .edges()
                .iter()
                .map(|e| e.target)
                .collect::<Vec<_>>();
            targets.sort_unstable();
            targets.dedup();
            if targets.len() <= 1 {
                return 0;
            }
            targets
                .iter()
                .filter(|b| candidate.blocks[b.index()].preds.len() > 1)
                .count()
        })
        .sum();
    // dead_leaders remains a source-block count: add removed blocks that owned
    // source states, excluding removed synthetic/edge-only compiler blocks.
    let dead_leaders = func.census.dead_leaders
        + source_owners
            .iter()
            .zip(&block_map)
            .filter(|(source, mapped)| **source && mapped.is_none())
            .count();
    candidate.census = OptCensus {
        fold: func.census.fold.clone(),
        blocks: candidate.blocks.len(),
        insts: candidate.insts.len(),
        phis: candidate.blocks.iter().map(|b| b.params.len()).sum(),
        frames: candidate.frames.len(),
        refinements: candidate
            .insts
            .iter()
            .filter(|i| matches!(i.op, Opcode::Refine(_)))
            .count(),
        critical_edges,
        dead_leaders,
    };
    candidate.rebuild_frame_intern();
    candidate.verify()?;
    *func = candidate;
    Ok(())
}

fn retain_definition(
    func: &Func,
    value: Value,
    expected: ValueDef,
    live: &mut [bool],
) -> Result<(), VerifyError> {
    let data = func
        .values
        .get(value.index())
        .ok_or(VerifyError::InvalidValue(value))?;
    if data.def != expected || std::mem::replace(&mut live[value.index()], true) {
        return Err(VerifyError::BadDefinition(value));
    }
    Ok(())
}

/// Invocation-local graph colors; no Lisp state or shared caches.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    Unseen,
    Visiting,
    Done,
}

fn resolve_aliases(func: &Func) -> Result<Vec<Value>, VerifyError> {
    let mut resolved = vec![None; func.values.len()];
    let mut visits = vec![Visit::Unseen; func.values.len()];
    let mut path = Vec::new();
    for index in 0..func.values.len() {
        let mut current = Value(index as u32);
        let root = loop {
            let data = func
                .values
                .get(current.index())
                .ok_or(VerifyError::InvalidValue(current))?;
            if let Some(root) = resolved[current.index()] {
                break root;
            }
            if visits[current.index()] == Visit::Visiting {
                return Err(VerifyError::AliasCycle(current));
            }
            visits[current.index()] = Visit::Visiting;
            path.push(current);
            match data.def {
                ValueDef::Alias(next) => current = next,
                _ => break current,
            }
        };
        for value in path.drain(..) {
            visits[value.index()] = Visit::Done;
            resolved[value.index()] = Some(root);
        }
    }
    Ok(resolved
        .into_iter()
        .map(|v| v.expect("resolved value"))
        .collect())
}

fn ordered_frames(func: &Func, roots: &[FrameId]) -> Result<Vec<FrameId>, VerifyError> {
    let mut visits = vec![Visit::Unseen; func.frames.len()];
    let mut path = Vec::new();
    let mut order = Vec::new();
    for &root in roots {
        let mut current = Some(root);
        while let Some(frame) = current {
            let data = func
                .frames
                .get(frame.index())
                .ok_or(VerifyError::InvalidFrame(frame))?;
            match visits[frame.index()] {
                Visit::Done => break,
                Visit::Visiting => return Err(VerifyError::FrameCycle(frame)),
                Visit::Unseen => visits[frame.index()] = Visit::Visiting,
            }
            path.push(frame);
            current = data.parent;
        }
        while let Some(frame) = path.pop() {
            visits[frame.index()] = Visit::Done;
            order.push(frame);
        }
    }
    Ok(order)
}

fn mapped_block(block: Block, map: &[Option<Block>]) -> Result<Block, VerifyError> {
    map.get(block.index())
        .copied()
        .flatten()
        .ok_or(VerifyError::InvalidBlock(block))
}

fn mapped_inst(inst: Inst, map: &[Option<Inst>]) -> Result<Inst, VerifyError> {
    map.get(inst.index())
        .copied()
        .flatten()
        .ok_or(VerifyError::InvalidInst(inst))
}

fn mapped_value(
    value: Value,
    canonical: &[Value],
    map: &[Option<Value>],
) -> Result<Value, VerifyError> {
    let root = canonical
        .get(value.index())
        .ok_or(VerifyError::InvalidValue(value))?;
    map.get(root.index())
        .copied()
        .flatten()
        .ok_or(VerifyError::InvalidValue(value))
}

fn mapped_values(
    values: &[Value],
    canonical: &[Value],
    map: &[Option<Value>],
) -> Result<Vec<Value>, VerifyError> {
    values
        .iter()
        .map(|&v| mapped_value(v, canonical, map))
        .collect()
}

fn mapped_frame(frame: FrameId, map: &[Option<FrameId>]) -> Result<FrameId, VerifyError> {
    map.get(frame.index())
        .copied()
        .flatten()
        .ok_or(VerifyError::InvalidFrame(frame))
}

fn mapped_edge(
    edge: &mut Edge,
    blocks: &[Option<Block>],
    canonical: &[Value],
    values: &[Option<Value>],
) -> Result<(), VerifyError> {
    edge.target = mapped_block(edge.target, blocks)?;
    edge.args = mapped_values(&edge.args, canonical, values)?;
    Ok(())
}

fn mapped_term(
    term: &Term,
    blocks: &[Option<Block>],
    canonical: &[Value],
    values: &[Option<Value>],
    frames: &[Option<FrameId>],
) -> Result<Term, VerifyError> {
    let mut term = term.clone();
    match &mut term {
        Term::Jump(edge) => mapped_edge(edge, blocks, canonical, values)?,
        Term::Branch {
            flag,
            if_true,
            if_false,
        } => {
            *flag = mapped_value(*flag, canonical, values)?;
            mapped_edge(if_true, blocks, canonical, values)?;
            mapped_edge(if_false, blocks, canonical, values)?;
        }
        Term::Switch {
            value,
            table,
            cases,
            default,
        } => {
            *value = mapped_value(*value, canonical, values)?;
            *table = mapped_value(*table, canonical, values)?;
            for case in cases {
                mapped_edge(&mut case.edge, blocks, canonical, values)?;
            }
            mapped_edge(default, blocks, canonical, values)?;
        }
        Term::Return(value) => *value = mapped_value(*value, canonical, values)?,
        Term::Deopt(frame) => *frame = mapped_frame(*frame, frames)?,
        Term::Unreachable => {}
    }
    Ok(term)
}

#[cfg(test)]
#[path = "tests/cfg.rs"]
mod tests;
