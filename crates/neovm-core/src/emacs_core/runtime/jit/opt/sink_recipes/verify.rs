//! Independent recipe provenance and exact-point version verification.
//!
//! The compiler validates ordinary SSA before constructing this capability.
//! Ordinary SSA validation must run first; this function never calls Func.verify.
//! No declaration, pass census, caller-provided current-version map, or equal
//! numeric payload is accepted as semantic identity proof.
//!
//! Threading: scratch maps/ids belong to one compilation and read no Lisp heap.
//! The returned capability borrows immutable compiler-owned metadata. There is
//! no runtime/TLS cache and no assumption of one process-wide mutator.

use super::{
    CachePhiEdge, NumericMode, OwnerRecipe, RecipeField, RecipeFields, RecipeKind, RecipeOrigin,
    RecipePoint, RecipeVersion, RecipeVersionId, SinkOp, SinkRecipes, SinkVerifyError,
    SinkVerifyReason as Why, VerifiedSinkRecipes, VersionCause,
};
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    types::{TypeKind, TypeSet},
};
use std::collections::{BTreeMap, HashMap, HashSet};

const WORK_LIMIT: usize = 1_000_000;
type State = BTreeMap<Value, RecipeVersionId>;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Cut {
    block: Block,
    position: usize,
}
struct Work(std::cell::Cell<usize>);
impl Work {
    fn spend(&self, amount: usize, owner: Value) -> Result<(), SinkVerifyError> {
        let next = self
            .0
            .get()
            .checked_add(amount)
            .ok_or_else(|| error(owner, None, Why::AnalysisLimit))?;
        self.0.set(next);
        if next > WORK_LIMIT {
            return Err(error(owner, None, Why::AnalysisLimit));
        }
        Ok(())
    }
}
fn error(owner: Value, point: Option<RecipePoint>, reason: Why) -> SinkVerifyError {
    SinkVerifyError {
        owner,
        point,
        reason,
    }
}
fn ensure(
    ok: bool,
    owner: Value,
    point: Option<RecipePoint>,
    why: Why,
) -> Result<(), SinkVerifyError> {
    if ok {
        Ok(())
    } else {
        Err(error(owner, point, why))
    }
}
fn tuple(fields: RecipeFields) -> Vec<Value> {
    match fields {
        RecipeFields::Number(f) => vec![f.payload, f.word, f.ready, f.real_box],
        RecipeFields::Cons(f) => vec![f.car, f.cdr, f.real_box],
    }
}
fn box_field(fields: RecipeFields) -> Value {
    match fields {
        RecipeFields::Number(f) => f.real_box,
        RecipeFields::Cons(f) => f.real_box,
    }
}
fn nonbox(fields: RecipeFields) -> Vec<Value> {
    let mut values = tuple(fields);
    values.pop();
    values
}
fn replace_box_equal(a: RecipeFields, b: RecipeFields) -> bool {
    std::mem::discriminant(&a) == std::mem::discriminant(&b) && nonbox(a) == nonbox(b)
}

/// Called only after actual producer/tuple provenance validation. An unboxed
/// Cons must descend through exact copied-identity tuples to a fresh source
/// definition. Absence of a guaranteed box is not itself unboxed proof.
fn unboxed_cons_version(
    table: &SinkRecipes,
    mut version: RecipeVersionId,
    work: &Work,
) -> Result<bool, SinkVerifyError> {
    let mut seen = HashSet::new();
    loop {
        let data = table
            .versions
            .get(version.0 as usize)
            .ok_or_else(|| error(Value(0), None, Why::MissingOwner))?;
        work.spend(1, data.owner)?;
        ensure(seen.insert(version), data.owner, None, Why::UngroundedTuple)?;
        let owner = table
            .owners
            .get(&data.owner)
            .ok_or_else(|| error(data.owner, None, Why::MissingOwner))?;
        if owner.kind != RecipeKind::Cons || !matches!(data.fields, RecipeFields::Cons(_)) {
            return Ok(false);
        }
        match data.cause {
            VersionCause::Definition => {
                return Ok(owner.definition_version == version
                    && matches!(owner.origin, RecipeOrigin::ConsSource { .. }));
            }
            VersionCause::SameIdentity { input } => {
                let previous = table
                    .versions
                    .get(input.0 as usize)
                    .ok_or_else(|| error(data.owner, None, Why::MissingOwner))?;
                if data.fields != previous.fields
                    || !matches!(owner.origin, RecipeOrigin::SameIdentityView { input, .. }
                        if input == previous.owner)
                {
                    return Ok(false);
                }
                version = input;
            }
            // Cached, parameter, and maybe-boxed versions never gain old
            // field roots by negating guaranteed_boxed.
            _ => return Ok(false),
        }
    }
}

/// Bounded Cooper dominators and actual instruction/source cuts. Integration
/// can reuse the ordinary verifier's identical scratch without changing rules.
struct Index {
    positions: Vec<Option<Cut>>,
    preds: Vec<Vec<(Block, u32)>>,
    order: Vec<Block>,
    enter: Vec<usize>,
    exit: Vec<usize>,
    sources: Vec<Option<(Cut, Cut)>>,
    resolved: Vec<Value>,
    entry_positions: Vec<usize>,
}
impl Index {
    fn new(func: &Func, work: &mut Work) -> Result<Self, SinkVerifyError> {
        let fail = Value(0);
        work.spend(
            func.blocks.len() + func.values.len() + func.insts.len() + func.source_states.len(),
            fail,
        )?;
        ensure(
            func.entry.index() < func.blocks.len(),
            fail,
            None,
            Why::InvalidPoint,
        )?;
        let count = func.blocks.len();
        // Direct definitions are their own canonical roots. The initial
        // linear work charge covers this seed pass; only actual Alias walks
        // need path storage and cycle detection below.
        let mut resolved = func
            .values
            .iter()
            .enumerate()
            .map(|(index, data)| match data.def {
                ValueDef::Alias(_) => None,
                _ => Some(Value(index as u32)),
            })
            .collect::<Vec<_>>();
        for start in 0..func.values.len() {
            if resolved[start].is_some() {
                continue;
            }
            let mut path = Vec::new();
            let mut seen = HashSet::new();
            let mut value = Value(start as u32);
            let root = loop {
                work.spend(1, value)?;
                if let Some(root) = resolved.get(value.index()).copied().flatten() {
                    break root;
                }
                ensure(seen.insert(value), value, None, Why::MissingOwner)?;
                let data = func
                    .values
                    .get(value.index())
                    .ok_or_else(|| error(value, None, Why::MissingOwner))?;
                path.push(value);
                match data.def {
                    ValueDef::Alias(next) => value = next,
                    _ => break value,
                }
            };
            for value in path {
                resolved[value.index()] = Some(root);
            }
        }
        let resolved = resolved.into_iter().map(Option::unwrap).collect::<Vec<_>>();
        let mut entry_positions = vec![0; count];
        for &inst in &func.blocks[func.entry.index()].insts {
            work.spend(1, fail)?;
            if !matches!(
                func.insts[inst.index()].op,
                Opcode::Arg(_) | Opcode::OsrSlot(_)
            ) {
                break;
            }
            entry_positions[func.entry.index()] += 1;
        }
        let mut positions = vec![None; func.insts.len()];
        let mut preds = vec![Vec::new(); count];
        for (i, block) in func.blocks.iter().enumerate() {
            for (position, &inst) in block.insts.iter().enumerate() {
                work.spend(1, fail)?;
                let slot = positions
                    .get_mut(inst.index())
                    .ok_or_else(|| error(fail, None, Why::InvalidPoint))?;
                ensure(slot.is_none(), fail, None, Why::InvalidPoint)?;
                *slot = Some(Cut {
                    block: Block(i as u32),
                    position,
                });
            }
            for (edge_index, edge) in block.term.edges().into_iter().enumerate() {
                work.spend(1, fail)?;
                preds
                    .get_mut(edge.target.index())
                    .ok_or_else(|| error(fail, None, Why::InvalidPoint))?
                    .push((Block(i as u32), edge_index as u32));
            }
        }
        let mut visited = vec![false; count];
        let mut order = Vec::new();
        let mut stack = vec![(func.entry, false)];
        while let Some((block, leave)) = stack.pop() {
            work.spend(1, fail)?;
            if leave {
                order.push(block);
                continue;
            }
            if std::mem::replace(&mut visited[block.index()], true) {
                continue;
            }
            stack.push((block, true));
            for edge in func.blocks[block.index()].term.edges().into_iter().rev() {
                if !visited[edge.target.index()] {
                    stack.push((edge.target, false));
                }
            }
        }
        order.reverse();
        let mut rank = vec![usize::MAX; count];
        for (i, &block) in order.iter().enumerate() {
            rank[block.index()] = i;
        }
        let mut idom = vec![None; count];
        idom[func.entry.index()] = Some(func.entry);
        loop {
            let mut changed = false;
            for &block in order.iter().skip(1) {
                work.spend(preds[block.index()].len() + 1, fail)?;
                let mut incoming = preds[block.index()]
                    .iter()
                    .map(|&(p, _)| p)
                    .filter(|p| idom[p.index()].is_some());
                let Some(mut parent) = incoming.next() else {
                    continue;
                };
                for mut other in incoming {
                    while parent != other {
                        work.spend(1, fail)?;
                        if rank[parent.index()] > rank[other.index()] {
                            parent = idom[parent.index()].unwrap();
                        } else {
                            other = idom[other.index()].unwrap();
                        }
                    }
                }
                if idom[block.index()] != Some(parent) {
                    idom[block.index()] = Some(parent);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut children = vec![Vec::new(); count];
        for (i, parent) in idom.iter().enumerate() {
            if let Some(parent) = *parent
                && parent.index() != i
            {
                children[parent.index()].push(Block(i as u32));
            }
        }
        let mut enter = vec![usize::MAX; count];
        let mut exit = vec![usize::MAX; count];
        let mut clock = 0;
        let mut stack = vec![(func.entry, false)];
        while let Some((block, leave)) = stack.pop() {
            work.spend(1, fail)?;
            if leave {
                exit[block.index()] = clock;
            } else {
                enter[block.index()] = clock;
                stack.push((block, true));
                stack.extend(children[block.index()].iter().rev().map(|&b| (b, false)));
            }
            clock += 1;
        }
        // Exact prefix semantics of ordinary source-pre/post verification,
        // including seeded Arg/Osr instructions and malformed nonmonotone pcs.
        let mut cursors = vec![(0usize, 0usize); count];
        let mut sources = vec![None; func.source_states.len()];
        for (pc, source) in func.source_states.iter().enumerate() {
            let Some(source) = source else {
                continue;
            };
            let block = func
                .blocks
                .get(source.block.index())
                .ok_or_else(|| error(fail, None, Why::InvalidPoint))?;
            let (pre, post) = &mut cursors[source.block.index()];
            while let Some(&inst) = block.insts.get(*pre) {
                work.spend(1, fail)?;
                let data = &func.insts[inst.index()];
                if data.pc >= pc as u32 && !matches!(data.op, Opcode::Arg(_) | Opcode::OsrSlot(_)) {
                    break;
                }
                *pre += 1;
            }
            while let Some(&inst) = block.insts.get(*post) {
                work.spend(1, fail)?;
                let data = &func.insts[inst.index()];
                if data.pc > pc as u32 && !matches!(data.op, Opcode::Arg(_) | Opcode::OsrSlot(_)) {
                    break;
                }
                *post += 1;
            }
            sources[pc] = Some((
                Cut {
                    block: source.block,
                    position: *pre,
                },
                Cut {
                    block: source.block,
                    position: *post,
                },
            ));
        }
        Ok(Self {
            positions,
            preds,
            order,
            enter,
            exit,
            sources,
            resolved,
            entry_positions,
        })
    }
    fn point(&self, func: &Func, point: RecipePoint, owner: Value) -> Result<Cut, SinkVerifyError> {
        let bad = || error(owner, Some(point), Why::InvalidPoint);
        match point {
            RecipePoint::Before(inst) => self
                .positions
                .get(inst.index())
                .copied()
                .flatten()
                .ok_or_else(bad),
            RecipePoint::After(inst) => {
                let mut cut = self
                    .positions
                    .get(inst.index())
                    .copied()
                    .flatten()
                    .ok_or_else(bad)?;
                cut.position += 1;
                Ok(cut)
            }
            RecipePoint::Entry(block) => {
                func.blocks.get(block.index()).ok_or_else(bad)?;
                Ok(Cut {
                    block,
                    position: self.entry_positions[block.index()],
                })
            }
            RecipePoint::Term(block) => Ok(Cut {
                block,
                position: func.blocks.get(block.index()).ok_or_else(bad)?.insts.len(),
            }),
            RecipePoint::SourcePre(pc) => self
                .sources
                .get(pc as usize)
                .copied()
                .flatten()
                .map(|p| p.0)
                .ok_or_else(bad),
            RecipePoint::SourcePost(pc) => self
                .sources
                .get(pc as usize)
                .copied()
                .flatten()
                .map(|p| p.1)
                .ok_or_else(bad),
        }
    }
    fn dominates(&self, definition: Block, block: Block) -> bool {
        self.enter[definition.index()] != usize::MAX
            && self.enter[block.index()] != usize::MAX
            && self.enter[definition.index()] <= self.enter[block.index()]
            && self.exit[block.index()] <= self.exit[definition.index()]
    }
    fn available(&self, func: &Func, value: Value, cut: Cut) -> bool {
        let Some(&value) = self.resolved.get(value.index()) else {
            return false;
        };
        match func.values[value.index()].def {
            ValueDef::Param { block, .. } => self.dominates(block, cut.block),
            ValueDef::Inst(inst) => self
                .positions
                .get(inst.index())
                .copied()
                .flatten()
                .is_some_and(|def| {
                    if def.block == cut.block {
                        def.position < cut.position
                    } else {
                        self.dominates(def.block, cut.block)
                    }
                }),
            ValueDef::Alias(_) => false,
        }
    }
}

struct Check<'a> {
    func: &'a Func,
    table: &'a SinkRecipes,
    index: Index,
    work: Work,
    owners: Vec<Value>,
    owner_index: HashMap<Value, usize>,
    dependencies: Vec<Vec<usize>>,
    identity: Vec<usize>,
    // Candidate indexes are compilation-local and borrow no mutable Lisp
    // state. Versions are still independently validated before strict flow;
    // identity members are populated only after the grounded identity proof.
    box_versions: HashMap<Value, Vec<RecipeVersionId>>,
    materializer_versions: HashMap<Inst, Vec<RecipeVersionId>>,
    identity_members: Vec<Vec<Value>>,
    boxed: Vec<bool>,
    definitions: HashMap<Inst, (Value, RecipeVersionId)>,
    // A copied tuple becomes available only after its final projection, but
    // its producer copies the exact version current BEFORE that producer.
    same_identity_inputs: HashMap<Inst, (Value, RecipeVersionId)>,
    updates: HashMap<Cut, Vec<RecipeVersionId>>,
    cache_phis: HashMap<Block, Vec<RecipeVersionId>>,
    points: HashMap<Cut, Vec<RecipePoint>>,
    required_frames: HashMap<RecipePoint, Vec<FrameId>>,
    phi_edges: HashMap<Value, Vec<usize>>,
    point_uses: HashMap<RecipePoint, Vec<(Value, RecipeVersionId)>>,
    point_frames: HashMap<RecipePoint, Vec<FrameId>>,
    canonical: HashMap<(RecipePoint, Value), Value>,
}
impl<'a> Check<'a> {
    fn value(&self, value: Value) -> Result<&ValueData, SinkVerifyError> {
        self.func
            .values
            .get(value.index())
            .ok_or_else(|| error(value, None, Why::MissingOwner))
    }
    fn owner(&self, value: Value) -> Result<&OwnerRecipe, SinkVerifyError> {
        self.table
            .owners
            .get(&value)
            .ok_or_else(|| error(value, None, Why::MissingOwner))
    }
    fn version(&self, id: RecipeVersionId) -> Result<&RecipeVersion, SinkVerifyError> {
        self.table
            .versions
            .get(id.0 as usize)
            .ok_or_else(|| error(Value(0), None, Why::MissingOwner))
    }
    fn inst(&self, inst: Inst, owner: Value) -> Result<&InstData, SinkVerifyError> {
        ensure(
            self.index
                .positions
                .get(inst.index())
                .copied()
                .flatten()
                .is_some(),
            owner,
            None,
            Why::InvalidSourceOperation,
        )?;
        self.func
            .insts
            .get(inst.index())
            .ok_or_else(|| error(owner, None, Why::InvalidSourceOperation))
    }
    fn resolved(&self, value: Value) -> Result<Value, SinkVerifyError> {
        self.index
            .resolved
            .get(value.index())
            .copied()
            .ok_or_else(|| error(value, None, Why::MissingOwner))
    }
    /// Tagged same-word views are the only scalar aliases accepted here.
    /// No Eq/type metadata or equal Float payload creates this relation.
    fn word_origin(&self, mut value: Value) -> Result<Value, SinkVerifyError> {
        for _ in 0..=self.func.values.len() {
            self.work.spend(1, value)?;
            value = self.resolved(value)?;
            let data = self.value(value)?;
            if let ValueDef::Inst(inst) = data.def {
                let op = &self.func.insts[inst.index()];
                // The existing Bool pass retains a source's original Bool ID
                // while projecting its exact GNU T/NIL word at a Tagged use.
                // This pure representation adapter preserves semantic value;
                // arbitrary non-nil words and payload equality do not.
                if op.op == Opcode::BoolToLisp
                    && op.args.len() == 1
                    && op.eff == Effects::PURE
                    && op.mem == AliasClass::None
                    && op.frame.is_none()
                    && data.rep == Rep::Tagged
                    && !data.ty.is_bottom()
                    && data.ty.is_subset(TypeSet::BOOLEAN)
                    && self.value(op.args[0])?.rep == Rep::Bool
                    && !self.value(op.args[0])?.ty.is_bottom()
                    && self.value(op.args[0])?.ty.is_subset(TypeSet::BOOLEAN)
                {
                    value = op.args[0];
                    continue;
                }
                if matches!(
                    op.op,
                    Opcode::Refine(_) | Opcode::CheckType(_) | Opcode::CheckEq(_)
                ) && op.args.len() == 1
                    && data.rep.is_tagged()
                    && self.value(op.args[0])?.rep.is_tagged()
                {
                    value = op.args[0];
                    continue;
                }
                if op.op == Opcode::Sink(SinkOp::RecipeField(RecipeField::RealBox))
                    && op.args.len() == 1
                {
                    if let Some(owner) = self.table.owners.get(&op.args[0]) {
                        match owner.origin {
                            RecipeOrigin::Borrow { original, .. } => {
                                value = original;
                                continue;
                            }
                            RecipeOrigin::SameIdentityView { .. } => {
                                if let VersionCause::SameIdentity { input } =
                                    self.version(owner.definition_version)?.cause
                                {
                                    // This exact copied projection preserves
                                    // the input version's original box word.
                                    value = box_field(self.version(input)?.fields);
                                    continue;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if op.op == Opcode::Sink(SinkOp::CacheBoxAfter) && op.args.len() == 3 {
                    value = op.args[2];
                    continue;
                }
                if matches!(
                    op.op,
                    Opcode::Sink(SinkOp::MaterializeNum | SinkOp::MaterializeCons)
                ) {
                    let mut preserves_box = false;
                    for &id in self.materializer_versions.get(&inst).into_iter().flatten() {
                        self.work.spend(1, value)?;
                        let version = self.version(id)?;
                        if matches!(version.cause,VersionCause::CacheAfter {previous,materialize,..}
                            if materialize==inst && self.boxed.get(previous.0 as usize)==Some(&true))
                        {
                            preserves_box = true;
                            break;
                        }
                    }
                    if preserves_box {
                        value = *op
                            .args
                            .last()
                            .ok_or_else(|| error(value, None, Why::InvalidSourceOperation))?;
                        continue;
                    }
                }
            }
            return Ok(value);
        }
        Err(error(value, None, Why::InvalidSourceOperation))
    }
    fn owner_input(&self, mut value: Value) -> Result<Value, SinkVerifyError> {
        for _ in 0..=self.table.owners.len() {
            self.work.spend(1, value)?;
            value = self.resolved(value)?;
            let Some(owner) = self.table.owners.get(&value) else {
                return self.word_origin(value);
            };
            match owner.origin {
                RecipeOrigin::Borrow { original, .. } => return self.word_origin(original),
                RecipeOrigin::SameIdentityView { input, .. } => value = input,
                _ => return Ok(value),
            }
        }
        Err(error(value, None, Why::UngroundedTuple))
    }
    fn pure_projection(
        &self,
        inst: Inst,
        owner: Value,
        field: RecipeField,
        result: Value,
    ) -> Result<(), SinkVerifyError> {
        let data = self.inst(inst, owner)?;
        ensure(
            data.op == Opcode::Sink(SinkOp::RecipeField(field))
                && data.args.as_slice() == [owner]
                && data.result == Some(result)
                && data.eff == Effects::PURE
                && data.mem == AliasClass::None
                && data.frame.is_none(),
            owner,
            None,
            Why::InvalidSourceOperation,
        )
    }
    fn projections(
        &mut self,
        owner: Value,
        producer: Inst,
        version: RecipeVersionId,
    ) -> Result<Inst, SinkVerifyError> {
        let data = self.version(version)?.fields;
        let cut = self.index.positions[producer.index()].unwrap();
        let list = &self.func.blocks[cut.block.index()].insts;
        let wanted: Vec<(RecipeField, Value)> = match data {
            RecipeFields::Number(f) => vec![
                (RecipeField::Payload, f.payload),
                (RecipeField::Word, f.word),
                (RecipeField::Ready, f.ready),
                (RecipeField::RealBox, f.real_box),
            ],
            RecipeFields::Cons(f) => vec![(RecipeField::RealBox, f.real_box)],
        };
        let mut last = producer;
        for (offset, (field, value)) in wanted.into_iter().enumerate() {
            self.work.spend(1, owner)?;
            let inst = *list
                .get(cut.position + 1 + offset)
                .ok_or_else(|| error(owner, None, Why::IncompleteTuple))?;
            self.pure_projection(inst, owner, field, value)?;
            ensure(
                self.func.insts[inst.index()].pc == self.func.insts[producer.index()].pc,
                owner,
                None,
                Why::IncompleteTuple,
            )?;
            last = inst;
        }
        Ok(last)
    }
    fn field_reps(
        &mut self,
        owner: Value,
        version: RecipeVersionId,
    ) -> Result<(), SinkVerifyError> {
        let fields = self.version(version)?.fields;
        self.work.spend(tuple(fields).len(), owner)?;
        let ok = match fields {
            RecipeFields::Number(f) => {
                self.value(f.payload)?.rep == Rep::RawF64
                    && self.value(f.payload)?.ty.is_subset(TypeSet::FLOAT)
                    && self.value(f.word)?.rep == Rep::RawWord
                    && self.value(f.word)?.ty == TypeSet::TOP
                    && self.value(f.ready)?.rep == Rep::Bool
                    && self.value(f.ready)?.ty.is_subset(TypeSet::BOOLEAN)
                    && self.value(f.real_box)?.rep == Rep::Tagged
            }
            RecipeFields::Cons(f) => {
                self.value(f.real_box)?.rep == Rep::Tagged
                    && self.value(f.real_box)?.ty.is_subset(TypeSet::LIST)
                    && (self.value(f.car)?.rep.is_tagged()
                        || self.table.owners.contains_key(&f.car))
                    && (self.value(f.cdr)?.rep.is_tagged()
                        || self.table.owners.contains_key(&f.cdr))
            }
        };
        ensure(
            ok && tuple(fields)
                .iter()
                .all(|&v| !self.func.values[v.index()].ty.is_bottom()),
            owner,
            None,
            Why::WrongFieldRepresentation,
        )
    }
    fn source(
        &self,
        owner: Value,
        inst: Inst,
        original: &Op,
        frame: FrameId,
        pc: u32,
    ) -> Result<(), SinkVerifyError> {
        let data = self.inst(inst, owner)?;
        let source = self
            .func
            .source_states
            .get(pc as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| error(owner, None, Why::InvalidSourceOperation))?;
        let expected = match original {
            Op::Cons | Op::List(1) => (Effects::ALLOCATES, AliasClass::None),
            Op::Call(1) => (Effects::UNKNOWN, AliasClass::Unknown),
            _ => (
                super::super::build::op_effects(&Op::Add).0,
                AliasClass::None,
            ),
        };
        ensure(
            data.result == Some(owner)
                && data.pc == pc
                && data.frame == Some(frame)
                && source.frame == frame
                && source.block == self.index.positions[inst.index()].unwrap().block
                && source.post.contains(&owner)
                && data.eff == expected.0
                && data.mem == expected.1,
            owner,
            None,
            Why::InvalidSourceOperation,
        )?;
        let arity = match original {
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Cons | Op::Call(1) => 2,
            Op::List(1) => 1,
            _ => return Err(error(owner, None, Why::InvalidSourceOperation)),
        };
        ensure(
            data.args.len() == arity && source.pre.len() >= arity,
            owner,
            None,
            Why::InvalidSourceOperation,
        )?;
        for (&arg, &old) in data
            .args
            .iter()
            .zip(&source.pre[source.pre.len() - arity..])
        {
            ensure(
                self.owner_input(arg)? == self.word_origin(old)?,
                owner,
                None,
                Why::InvalidSourceOperation,
            )?;
        }
        Ok(())
    }
    fn owners_and_producers(&mut self) -> Result<(), SinkVerifyError> {
        let owner_ids = self.owners.clone();
        for owner in owner_ids {
            self.work.spend(1, owner)?;
            let info = self.owner(owner)?.clone();
            let value = self.value(owner)?.clone();
            ensure(
                info.owner == owner
                    && self.resolved(owner)? == owner
                    && value.ty == info.semantic_type
                    && !value.ty.is_bottom(),
                owner,
                None,
                Why::MissingOwner,
            )?;
            ensure(
                match info.kind {
                    RecipeKind::Number(_) => value.rep == Rep::NumPair,
                    RecipeKind::Cons => {
                        matches!(value.rep, Rep::Virtual(_)) && value.ty.is_subset(TypeSet::CONS)
                    }
                },
                owner,
                None,
                Why::WrongFieldRepresentation,
            )?;
            let version = self.version(info.definition_version)?.clone();
            ensure(version.owner == owner, owner, None, Why::MissingOwner)?;
            self.field_reps(owner, info.definition_version)?;
            let mut dependencies = Vec::new();
            let last;
            match info.origin {
                RecipeOrigin::Borrow { inst, original } => {
                    let data = self.inst(inst, owner)?;
                    ensure(
                        matches!(info.kind, RecipeKind::Number(NumericMode::Borrowable))
                            && version.cause == VersionCause::Definition
                            && data.op == Opcode::Sink(SinkOp::BorrowNum)
                            && data.args.as_slice() == [original]
                            && data.result == Some(owner)
                            && data.eff == Effects::PURE
                            && data.mem == AliasClass::None
                            && data.frame.is_none()
                            && self.value(original)?.rep.is_tagged()
                            && self.value(original)?.ty == value.ty,
                        owner,
                        None,
                        Why::InvalidBorrow,
                    )?;
                    // Exact BorrowNum projection producer semantics certify
                    // +0/false/original bits/original box, without inspecting it.
                    last = Some(
                        self.projections(owner, inst, info.definition_version)
                            .map_err(|failure| {
                                if failure.reason == Why::AnalysisLimit {
                                    failure
                                } else {
                                    error(owner, None, Why::InvalidBorrow)
                                }
                            })?,
                    );
                    let RecipeFields::Number(f) = version.fields else {
                        return Err(error(owner, None, Why::InvalidBorrow));
                    };
                    ensure(
                        value.ty.is_subset(self.value(f.real_box)?.ty)
                            && TypeSet::NIL.is_subset(self.value(f.ready)?.ty),
                        owner,
                        None,
                        Why::InvalidBorrow,
                    )?;
                }
                RecipeOrigin::NumericSource {
                    inst,
                    original_op,
                    frame,
                    pc,
                } => {
                    let data = self.inst(inst, owner)?.clone();
                    let sqrt = data.op == Opcode::Sink(SinkOp::SourceSqrt);
                    ensure(
                        data.args.len() == 2,
                        owner,
                        None,
                        Why::InvalidSourceOperation,
                    )?;
                    ensure(
                        if sqrt {
                            original_op == Op::Call(1)
                                && self.table.source_sqrt_sites.contains(&pc)
                                && value.ty.is_subset(TypeSet::FLOAT)
                                && self.value(data.args[0])?.rep.is_tagged()
                        } else {
                            matches!(original_op, Op::Add | Op::Sub | Op::Mul | Op::Div)
                                && data.op == Opcode::Sink(SinkOp::SourceNum(original_op.clone()))
                        },
                        owner,
                        None,
                        Why::InvalidSourceOperation,
                    )?;
                    ensure(
                        value.ty.is_subset(TypeSet::FIXNUM.join(TypeSet::FLOAT))
                            && version.cause == VersionCause::Definition,
                        owner,
                        None,
                        Why::InvalidSourceOperation,
                    )?;
                    self.source(owner, inst, &original_op, frame, pc)?;
                    for &input in data.args.iter().skip(usize::from(sqrt)) {
                        dependencies.push(
                            *self
                                .owner_index
                                .get(&input)
                                .ok_or_else(|| error(input, None, Why::MissingOwner))?,
                        );
                    }
                    let mut output = TypeSet::BOTTOM;
                    if sqrt {
                        output = TypeSet::FLOAT;
                    } else {
                        let a = self.owner(data.args[0])?.semantic_type;
                        let b = self.owner(data.args[1])?.semantic_type;
                        if a.contains(TypeKind::Fixnum) && b.contains(TypeKind::Fixnum) {
                            output = output.join(TypeSet::FIXNUM);
                        }
                        let numeric = TypeSet::FIXNUM.join(TypeSet::FLOAT);
                        if !a.meet(numeric).is_bottom()
                            && !b.meet(numeric).is_bottom()
                            && (a.contains(TypeKind::Float) || b.contains(TypeKind::Float))
                        {
                            output = output.join(TypeSet::FLOAT);
                        }
                    }
                    ensure(
                        output.is_subset(info.semantic_type),
                        owner,
                        None,
                        Why::InvalidSourceOperation,
                    )?;
                    if matches!(info.kind, RecipeKind::Number(NumericMode::FloatOnly)) {
                        ensure(
                            !output.is_bottom() && output.is_subset(TypeSet::FLOAT),
                            owner,
                            None,
                            Why::InvalidSourceOperation,
                        )?;
                    }
                    let RecipeFields::Number(f) = version.fields else {
                        return Err(error(owner, None, Why::WrongFieldRepresentation));
                    };
                    let boxes = if output.contains(TypeKind::Fixnum) {
                        TypeSet::NIL.join(TypeSet::FIXNUM)
                    } else {
                        TypeSet::NIL
                    };
                    ensure(
                        boxes.is_subset(self.value(f.real_box)?.ty)
                            && TypeSet::T.is_subset(self.value(f.ready)?.ty),
                        owner,
                        None,
                        Why::WrongFieldRepresentation,
                    )?;
                    last = Some(self.projections(owner, inst, info.definition_version)?);
                }
                RecipeOrigin::ConsSource {
                    inst,
                    original_op,
                    frame,
                    pc,
                } => {
                    ensure(
                        info.kind == RecipeKind::Cons
                            && version.cause == VersionCause::Definition
                            && self.inst(inst, owner)?.op
                                == Opcode::Sink(SinkOp::SourceCons(original_op.clone())),
                        owner,
                        None,
                        Why::InvalidSourceOperation,
                    )?;
                    self.source(owner, inst, &original_op, frame, pc)?;
                    let RecipeFields::Cons(f) = version.fields else {
                        return Err(error(owner, None, Why::WrongFieldRepresentation));
                    };
                    let data = self.inst(inst, owner)?;
                    ensure(
                        f.car == data.args[0]
                            && (if original_op == Op::Cons {
                                f.cdr == data.args[1]
                            } else {
                                self.is_nil(f.cdr)?
                            }),
                        owner,
                        None,
                        Why::InvalidSourceOperation,
                    )?;
                    for input in [f.car, f.cdr] {
                        if let Some(&index) = self.owner_index.get(&input) {
                            dependencies.push(index);
                        }
                    }
                    last = Some(self.projections(owner, inst, info.definition_version)?);
                }
                RecipeOrigin::Phi {
                    block,
                    field_params,
                } => {
                    ensure(
                        matches!(info.kind, RecipeKind::Number(_))
                            && version.cause == VersionCause::Parameter
                            && matches!(value.def,ValueDef::Param { block: b,.. } if b==block)
                            && tuple(version.fields).as_slice() == field_params.as_ref()
                            && field_params.len() == 4
                            && !self.index.preds[block.index()].is_empty(),
                        owner,
                        None,
                        Why::IncompleteTuple,
                    )?;
                    for &field in &field_params {
                        ensure(
                            matches!(self.value(field)?.def,ValueDef::Param { block:b,.. } if b==block),
                            owner,
                            None,
                            Why::WrongEdgeTuple,
                        )?;
                    }
                    let edges = self
                        .phi_edges
                        .get(&owner)
                        .into_iter()
                        .flatten()
                        .map(|&i| self.table.edges[i].clone())
                        .collect::<Vec<_>>();
                    ensure(
                        edges.len() == self.index.preds[block.index()].len(),
                        owner,
                        None,
                        Why::IncompleteTuple,
                    )?;
                    let mut occurrences = HashSet::new();
                    for edge in edges {
                        self.work
                            .spend(self.index.preds[block.index()].len() + 1, owner)?;
                        ensure(
                            occurrences.insert((edge.source, edge.edge_index))
                                && edge.target == block
                                && self.index.preds[block.index()]
                                    .contains(&(edge.source, edge.edge_index)),
                            owner,
                            None,
                            Why::WrongEdgeTuple,
                        )?;
                        ensure(
                            self.owner(edge.incoming_owner)?
                                .semantic_type
                                .is_subset(info.semantic_type),
                            owner,
                            None,
                            Why::WrongEdgeTuple,
                        )?;
                        ensure(
                            self.version(edge.incoming_version)?.owner == edge.incoming_owner,
                            owner,
                            None,
                            Why::WrongEdgeTuple,
                        )?;
                        dependencies.push(
                            *self.owner_index.get(&edge.incoming_owner).ok_or_else(|| {
                                error(edge.incoming_owner, None, Why::MissingOwner)
                            })?,
                        );
                    }
                    last = None;
                }
                RecipeOrigin::SameIdentityView { inst, input } => {
                    let data = self.inst(inst, owner)?;
                    ensure(
                        matches!(data.op, Opcode::Refine(_))
                            && data.args.as_slice() == [input]
                            && data.result == Some(owner)
                            && data.eff == Effects::PURE
                            && data.mem == AliasClass::None
                            && data.frame.is_none(),
                        owner,
                        None,
                        Why::InvalidSourceOperation,
                    )?;
                    let VersionCause::SameIdentity {
                        input: input_version,
                    } = version.cause
                    else {
                        return Err(error(owner, None, Why::InvalidSourceOperation));
                    };
                    ensure(
                        self.version(input_version)?.owner == input
                            && matches!(
                                (self.owner(input)?.kind, info.kind),
                                (RecipeKind::Number(_), RecipeKind::Number(_))
                                    | (RecipeKind::Cons, RecipeKind::Cons)
                            )
                            && self
                                .owner(input)?
                                .semantic_type
                                .is_subset(info.semantic_type),
                        owner,
                        None,
                        Why::InvalidSourceOperation,
                    )?;
                    dependencies.push(
                        *self
                            .owner_index
                            .get(&input)
                            .ok_or_else(|| error(input, None, Why::MissingOwner))?,
                    );
                    self.same_identity_inputs
                        .insert(inst, (owner, input_version));
                    if self.version(input_version)?.fields == version.fields {
                        // A virtual Cons field read copies the semantic owner
                        // while reusing its already-dominating physical tuple.
                        last = Some(inst);
                    } else {
                        // A numeric Refine copies the complete carrier into
                        // four actual contiguous SSA field projections. The
                        // projection opcode, not payload equality, certifies
                        // each field's correspondence to this exact input.
                        let RecipeFields::Number(before) = self.version(input_version)?.fields
                        else {
                            return Err(error(owner, None, Why::IncompleteTuple));
                        };
                        let RecipeFields::Number(after) = version.fields else {
                            return Err(error(owner, None, Why::IncompleteTuple));
                        };
                        ensure(
                            self.value(before.ready)?
                                .ty
                                .is_subset(self.value(after.ready)?.ty)
                                && info
                                    .semantic_type
                                    .join(TypeSet::NIL)
                                    .is_subset(self.value(after.real_box)?.ty),
                            owner,
                            None,
                            Why::WrongFieldRepresentation,
                        )?;
                        last = Some(self.projections(owner, inst, info.definition_version)?);
                    }
                }
            }
            self.dependencies[self.owner_index[&owner]] = dependencies;
            if let Some(last) = last {
                ensure(
                    self.definitions
                        .insert(last, (owner, info.definition_version))
                        .is_none(),
                    owner,
                    None,
                    Why::InvalidSourceOperation,
                )?;
            }
        }
        Ok(())
    }
    fn is_nil(&self, value: Value) -> Result<bool, SinkVerifyError> {
        let value = self.word_origin(value)?;
        Ok(match self.value(value)?.def {
            ValueDef::Inst(inst) => match self.func.insts[inst.index()].op {
                Opcode::Const(pool) if pool as usize >= self.func.dynamic_prefix => self
                    .func
                    .consts
                    .get(pool as usize)
                    .is_some_and(|bits| bits.0 == 0),
                _ => false,
            },
            _ => false,
        })
    }
    /// Kosaraju SCC grounding: all external dependencies must be grounded,
    /// and a component needs a Borrow/ordinary Cons seed or grounded dependency.
    /// A numeric source in an otherwise seedless cycle is NOT itself a seed.
    fn grounding(&mut self) -> Result<(), SinkVerifyError> {
        let n = self.owners.len();
        let mut reverse = vec![Vec::new(); n];
        for (i, deps) in self.dependencies.iter().enumerate() {
            for &dep in deps {
                self.work.spend(1, self.owners[i])?;
                reverse[dep].push(i);
            }
        }
        let mut seen = vec![false; n];
        let mut finish = Vec::new();
        for start in 0..n {
            let mut stack = vec![(start, false)];
            while let Some((i, leave)) = stack.pop() {
                self.work.spend(1, self.owners[i])?;
                if leave {
                    finish.push(i);
                    continue;
                }
                if std::mem::replace(&mut seen[i], true) {
                    continue;
                }
                stack.push((i, true));
                stack.extend(self.dependencies[i].iter().map(|&dep| (dep, false)));
            }
        }
        let mut component = vec![usize::MAX; n];
        let mut members: Vec<Vec<usize>> = Vec::new();
        for &start in finish.iter().rev() {
            if component[start] != usize::MAX {
                continue;
            }
            let id = members.len();
            let mut group = Vec::new();
            let mut stack = vec![start];
            while let Some(i) = stack.pop() {
                self.work.spend(1, self.owners[i])?;
                if component[i] != usize::MAX {
                    continue;
                }
                component[i] = id;
                group.push(i);
                stack.extend_from_slice(&reverse[i]);
            }
            members.push(group);
        }
        let mut grounded = vec![false; members.len()];
        loop {
            let mut changed = false;
            for (id, group) in members.iter().enumerate() {
                if grounded[id] {
                    continue;
                }
                let mut seed = false;
                let mut external = true;
                for &i in group {
                    self.work.spend(1, self.owners[i])?;
                    match self.owner(self.owners[i])?.origin {
                        RecipeOrigin::Borrow { .. } => seed = true,
                        RecipeOrigin::ConsSource { .. } => {
                            ensure(
                                group.len() == 1 && !self.dependencies[i].contains(&i),
                                self.owners[i],
                                None,
                                Why::CyclicCons,
                            )?;
                            seed = true;
                        }
                        _ => {}
                    }
                    for &dep in &self.dependencies[i] {
                        if component[dep] != id {
                            external &= grounded[component[dep]];
                            seed |= grounded[component[dep]];
                        }
                    }
                }
                if seed && external {
                    grounded[id] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for i in 0..n {
            ensure(
                grounded[component[i]],
                self.owners[i],
                None,
                Why::UngroundedTuple,
            )?;
        }
        Ok(())
    }
    /// Grounded bisimulation for simultaneous phis. Producer allocations are
    /// unique atoms; Borrow atoms use exact original SSA word identity. Only
    /// phis in the same block can share a coinductive partition initially.
    fn identities(&mut self) -> Result<(), SinkVerifyError> {
        #[derive(Hash, PartialEq, Eq)]
        enum Key {
            Atom(Value),
            Borrow(Value),
            Phi(Block, Vec<(Block, u32, usize)>),
            InitialPhi(Block),
        }
        let n = self.owners.len();
        let mut base = (0..n).collect::<Vec<_>>();
        for i in 0..n {
            let mut current = self.owners[i];
            for _ in 0..=n {
                if let RecipeOrigin::SameIdentityView { input, .. } = self.owner(current)?.origin {
                    current = input;
                } else {
                    break;
                }
                self.work.spend(1, current)?;
            }
            ensure(
                !matches!(
                    self.owner(current)?.origin,
                    RecipeOrigin::SameIdentityView { .. }
                ),
                current,
                None,
                Why::UngroundedTuple,
            )?;
            base[i] = self.owner_index[&current];
        }
        let mut classes = vec![0; n];
        let mut initial = HashMap::new();
        for i in 0..n {
            let info = self.owner(self.owners[base[i]])?;
            let key = match info.origin {
                RecipeOrigin::Borrow { original, .. } => Key::Borrow(self.word_origin(original)?),
                RecipeOrigin::Phi { block, .. } => Key::InitialPhi(block),
                _ => Key::Atom(info.owner),
            };
            let next = initial.len();
            classes[i] = *initial.entry(key).or_insert(next);
        }
        loop {
            let mut next = vec![0; n];
            let mut keys = HashMap::new();
            for i in 0..n {
                self.work.spend(1, self.owners[i])?;
                let info = self.owner(self.owners[base[i]])?;
                let key = match info.origin {
                    RecipeOrigin::Borrow { original, .. } => {
                        Key::Borrow(self.word_origin(original)?)
                    }
                    RecipeOrigin::Phi { block, .. } => {
                        let mut inputs = self
                            .phi_edges
                            .get(&info.owner)
                            .into_iter()
                            .flatten()
                            .map(|&i| &self.table.edges[i])
                            .map(|edge| {
                                (
                                    edge.source,
                                    edge.edge_index,
                                    classes[self.owner_index[&edge.incoming_owner]],
                                )
                            })
                            .collect::<Vec<_>>();
                        self.work.spend(inputs.len(), info.owner)?;
                        inputs.sort_unstable();
                        Key::Phi(block, inputs)
                    }
                    _ => Key::Atom(info.owner),
                };
                let count = keys.len();
                next[i] = *keys.entry(key).or_insert(count);
            }
            if next == classes {
                break;
            }
            classes = next;
        }
        // Degenerate copy phis may share an external atom when all incoming
        // identities are that atom or the already-grounded phi's own class.
        loop {
            let mut changed = false;
            for i in 0..n {
                let info = self.owner(self.owners[i])?;
                if !matches!(info.origin, RecipeOrigin::Phi { .. }) {
                    continue;
                }
                let inputs = self.dependencies[i]
                    .iter()
                    .map(|&dep| classes[dep])
                    .filter(|&c| c != classes[i])
                    .collect::<HashSet<_>>();
                self.work.spend(self.dependencies[i].len(), info.owner)?;
                if inputs.len() == 1 {
                    let old = classes[i];
                    let new = *inputs.iter().next().unwrap();
                    self.work.spend(classes.len(), info.owner)?;
                    for class in &mut classes {
                        if *class == old {
                            *class = new;
                        }
                    }
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        self.identity = classes;
        self.identity_members = vec![Vec::new(); n];
        for (i, &owner) in self.owners.iter().enumerate() {
            self.work.spend(1, owner)?;
            self.identity_members[self.identity[i]].push(owner);
        }
        Ok(())
    }
    fn identity_equal(&self, a: Value, b: Value) -> bool {
        match (self.owner_index.get(&a), self.owner_index.get(&b)) {
            (Some(&a), Some(&b)) => self.identity[a] == self.identity[b],
            _ => false,
        }
    }
    /// SSA word equality for cached boxes, including simultaneous physical
    /// cache phis. A grounded equal leaf is required; a bare cyclic pair fails.
    fn same_box(&self, a: Value, b: Value) -> Result<bool, SinkVerifyError> {
        let mut pending = vec![(a, b)];
        let mut seen = HashSet::new();
        let mut grounded = false;
        while let Some((a, b)) = pending.pop() {
            self.work.spend(1, a)?;
            let a = self.word_origin(a)?;
            let b = self.word_origin(b)?;
            if a == b {
                grounded = true;
                continue;
            }
            if !seen.insert((a, b)) {
                continue;
            }
            let (
                ValueDef::Param {
                    block: a_block,
                    index: a_index,
                },
                ValueDef::Param {
                    block: b_block,
                    index: b_index,
                },
            ) = (self.value(a)?.def, self.value(b)?.def)
            else {
                return Ok(false);
            };
            if a_block != b_block || self.index.preds[a_block.index()].is_empty() {
                return Ok(false);
            }
            for &(source, edge_index) in &self.index.preds[a_block.index()] {
                self.work.spend(1, a)?;
                let edges = self.func.blocks[source.index()].term.edges();
                let edge = edges[edge_index as usize];
                pending.push((edge.args[a_index as usize], edge.args[b_index as usize]));
            }
        }
        Ok(grounded)
    }
    fn event(&self, id: RecipeVersionId) -> Result<Cut, SinkVerifyError> {
        let mut id = id;
        let mut seen = HashSet::new();
        while seen.insert(id) {
            let version = self.version(id)?;
            self.work.spend(1, version.owner)?;
            match version.cause {
                VersionCause::CacheAfter { box_projection, .. } => {
                    return self.index.point(
                        self.func,
                        RecipePoint::After(box_projection),
                        version.owner,
                    );
                }
                VersionCause::AliasCacheAfter { input, .. } => id = input,
                VersionCause::CachePhi { block, .. } => return Ok(Cut { block, position: 0 }),
                _ => return Err(error(version.owner, None, Why::InvalidSourceOperation)),
            }
        }
        Err(error(self.version(id)?.owner, None, Why::UngroundedTuple))
    }
    fn versions(&mut self) -> Result<(), SinkVerifyError> {
        for i in 0..self.table.versions.len() {
            let id = RecipeVersionId(i as u32);
            let version = self.version(id)?.clone();
            let owner = version.owner;
            self.owner(owner)?;
            self.field_reps(owner, id)?;
            match version.cause {
                VersionCause::Definition
                | VersionCause::Parameter
                | VersionCause::SameIdentity { .. } => {
                    ensure(
                        self.owner(owner)?.definition_version == id,
                        owner,
                        None,
                        Why::InvalidSourceOperation,
                    )?;
                }
                VersionCause::CacheAfter {
                    previous,
                    materialize,
                    box_projection,
                } => {
                    let prev = self.version(previous)?;
                    let mat = self.inst(materialize, owner)?;
                    let cache = self.inst(box_projection, owner)?;
                    let expected = match version.fields {
                        RecipeFields::Number(_) => SinkOp::MaterializeNum,
                        RecipeFields::Cons(_) => SinkOp::MaterializeCons,
                    };
                    let mut args = vec![owner];
                    args.extend(tuple(prev.fields));
                    let result = mat
                        .result
                        .ok_or_else(|| error(owner, None, Why::InvalidSourceOperation))?;
                    let box_value = cache
                        .result
                        .ok_or_else(|| error(owner, None, Why::InvalidSourceOperation))?;
                    let before =
                        self.index
                            .point(self.func, RecipePoint::Before(box_projection), owner)?;
                    let after_mat =
                        self.index
                            .point(self.func, RecipePoint::After(materialize), owner)?;
                    ensure(
                        prev.owner == owner
                            && replace_box_equal(prev.fields, version.fields)
                            && mat.op == Opcode::Sink(expected)
                            && mat.args == args
                            && mat.eff == Effects::ALLOCATES
                            && mat.mem == AliasClass::None
                            && self.value(result)?.rep == Rep::Tagged
                            && !self.value(result)?.ty.is_bottom()
                            && self
                                .value(result)?
                                .ty
                                .is_subset(self.owner(owner)?.semantic_type)
                            && self
                                .owner(owner)?
                                .semantic_type
                                .is_subset(self.value(result)?.ty)
                            && self.value(result)?.ty.is_subset(self.value(box_value)?.ty)
                            && cache.op == Opcode::Sink(SinkOp::CacheBoxAfter)
                            && cache.args.as_slice() == [owner, box_field(prev.fields), result]
                            && box_field(version.fields) == box_value
                            && cache.eff == Effects::PURE
                            && cache.mem == AliasClass::None
                            && cache.frame.is_none()
                            && before == after_mat
                            && cache.pc == mat.pc,
                        owner,
                        None,
                        Why::InvalidSourceOperation,
                    )?;
                    self.updates.entry(self.event(id)?).or_default().push(id);
                }
                VersionCause::AliasCacheAfter { previous, input } => {
                    let prev = self.version(previous)?;
                    let source = self.version(input)?;
                    ensure(
                        prev.owner == owner
                            && self.identity_equal(owner, source.owner)
                            && replace_box_equal(prev.fields, version.fields)
                            && self.word_origin(box_field(version.fields))?
                                == self.word_origin(box_field(source.fields))?
                            && self
                                .value(box_field(source.fields))?
                                .ty
                                .is_subset(self.value(box_field(version.fields))?.ty),
                        owner,
                        None,
                        Why::StaleBoxVersion,
                    )?;
                    self.updates.entry(self.event(id)?).or_default().push(id);
                }
                VersionCause::CachePhi {
                    block,
                    box_param,
                    ref incoming,
                } => {
                    let definition = self.version(self.owner(owner)?.definition_version)?;
                    ensure(
                        matches!(version.fields, RecipeFields::Number(_))
                            && replace_box_equal(definition.fields, version.fields)
                            && box_field(version.fields) == box_param
                            && self.value(box_param)?.rep == Rep::Tagged
                            && matches!(self.value(box_param)?.def,ValueDef::Param {block:b,..} if b==block)
                            && self
                                .index
                                .available(self.func, owner, Cut { block, position: 0 })
                            && incoming.len() == self.index.preds[block.index()].len(),
                        owner,
                        None,
                        Why::WrongCachePhi,
                    )?;
                    let mut seen = HashSet::new();
                    for edge in incoming {
                        self.work
                            .spend(self.index.preds[block.index()].len() + 1, owner)?;
                        ensure(
                            seen.insert((edge.source, edge.edge_index))
                                && self.index.preds[block.index()]
                                    .contains(&(edge.source, edge.edge_index))
                                && self.version(edge.version)?.owner == owner,
                            owner,
                            None,
                            Why::WrongCachePhi,
                        )?;
                        self.actual_arg(
                            edge.source,
                            edge.edge_index,
                            block,
                            box_param,
                            box_field(self.version(edge.version)?.fields),
                            owner,
                        )?;
                    }
                    ensure(
                        !self.cache_phis.get(&block).is_some_and(|ids| {
                            ids.iter()
                                .any(|id| self.table.versions[id.0 as usize].owner == owner)
                        }),
                        owner,
                        None,
                        Why::WrongCachePhi,
                    )?;
                    self.cache_phis.entry(block).or_default().push(id);
                }
            }
        }
        // Box certainty follows explicit materialization, never Tagged type
        // alone. Borrow is an actual existing Lisp object even when it is NIL.
        self.boxed = vec![false; self.table.versions.len()];
        loop {
            let mut changed = false;
            for i in 0..self.table.versions.len() {
                self.work.spend(1, self.table.versions[i].owner)?;
                if self.boxed[i] {
                    continue;
                }
                let version = &self.table.versions[i];
                let yes = match &version.cause {
                    VersionCause::CacheAfter { .. } => true,
                    VersionCause::Definition => matches!(
                        self.owner(version.owner)?.origin,
                        RecipeOrigin::Borrow { .. }
                    ),
                    VersionCause::SameIdentity { input }
                    | VersionCause::AliasCacheAfter { input, .. } => self.boxed[input.0 as usize],
                    VersionCause::Parameter => {
                        let inputs = self
                            .phi_edges
                            .get(&version.owner)
                            .into_iter()
                            .flatten()
                            .map(|&i| &self.table.edges[i])
                            .collect::<Vec<_>>();
                        self.work.spend(inputs.len(), version.owner)?;
                        !inputs.is_empty()
                            && inputs
                                .iter()
                                .all(|edge| self.boxed[edge.incoming_version.0 as usize])
                    }
                    VersionCause::CachePhi { incoming, .. } => {
                        self.work.spend(incoming.len(), version.owner)?;
                        !incoming.is_empty()
                            && incoming
                                .iter()
                                .all(|edge| self.boxed[edge.version.0 as usize])
                    }
                };
                if yes {
                    self.boxed[i] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        Ok(())
    }
    fn actual_arg(
        &self,
        source: Block,
        index: u32,
        target: Block,
        param: Value,
        value: Value,
        owner: Value,
    ) -> Result<(), SinkVerifyError> {
        self.work.spend(
            self.func
                .blocks
                .get(source.index())
                .map_or(0, |b| b.term.edges().len()),
            owner,
        )?;
        let edge = self
            .func
            .blocks
            .get(source.index())
            .and_then(|b| b.term.edges().get(index as usize).copied())
            .ok_or_else(|| error(owner, None, Why::WrongEdgeTuple))?;
        let ValueDef::Param { block, index } = self.value(param)?.def else {
            return Err(error(owner, None, Why::WrongEdgeTuple));
        };
        ensure(
            block == target
                && edge.target == target
                && edge.args.get(index as usize) == Some(&value),
            owner,
            None,
            Why::WrongEdgeTuple,
        )
    }
    fn edges(&mut self) -> Result<(), SinkVerifyError> {
        for edge in &self.table.edges {
            self.work
                .spend(edge.field_args.len() + 1, edge.owner_param)?;
            let info = self.owner(edge.owner_param)?;
            let RecipeOrigin::Phi {
                block,
                ref field_params,
            } = info.origin
            else {
                return Err(error(edge.owner_param, None, Why::WrongEdgeTuple));
            };
            let version = self.version(edge.incoming_version)?;
            ensure(
                edge.field_args.len() == 4,
                edge.owner_param,
                None,
                Why::IncompleteTuple,
            )?;
            ensure(
                block == edge.target
                    && version.owner == edge.incoming_owner
                    && tuple(version.fields).as_slice() == edge.field_args.as_ref(),
                edge.owner_param,
                None,
                Why::WrongEdgeTuple,
            )?;
            self.actual_arg(
                edge.source,
                edge.edge_index,
                edge.target,
                edge.owner_param,
                edge.incoming_owner,
                edge.owner_param,
            )?;
            for (&param, &value) in field_params.iter().zip(edge.field_args.iter()) {
                self.actual_arg(
                    edge.source,
                    edge.edge_index,
                    edge.target,
                    param,
                    value,
                    edge.owner_param,
                )?;
            }
        }
        // Every pair of distinct logical phis that aliases on this edge must
        // carry one actual shared box on that edge. Shared NIL is not proof.
        let mut groups: HashMap<(Block, u32, Block, usize), Vec<&super::RecipeEdge>> =
            HashMap::new();
        for edge in &self.table.edges {
            groups
                .entry((
                    edge.source,
                    edge.edge_index,
                    edge.target,
                    self.identity[self.owner_index[&edge.incoming_owner]],
                ))
                .or_default()
                .push(edge);
        }
        for group in groups.values() {
            for i in 0..group.len() {
                for j in i + 1..group.len() {
                    self.work.spend(1, group[i].owner_param)?;
                    let (a, b) = (group[i], group[j]);
                    if self.identity_equal(a.owner_param, b.owner_param) {
                        continue;
                    }
                    ensure(
                        self.boxed[a.incoming_version.0 as usize]
                            && self.boxed[b.incoming_version.0 as usize]
                            && self.word_origin(a.field_args[3])?
                                == self.word_origin(b.field_args[3])?,
                        a.owner_param,
                        None,
                        Why::LostPartialAlias,
                    )?;
                }
            }
        }
        self.dominating_partial_edges()
    }
    fn live_entry_numbers(&self) -> Result<Vec<Vec<Value>>, SinkVerifyError> {
        let fail = Value(0);
        let mut uses = Vec::<(Block, Value)>::new();
        let mut frames = Vec::<(Block, FrameId)>::new();
        for (index, data) in self.func.blocks.iter().enumerate() {
            let block = Block(index as u32);
            if let Some(stack) = self.func.entry_stacks.get(index) {
                self.work.spend(stack.len(), fail)?;
                uses.extend(stack.iter().copied().map(|value| (block, value)));
            }
            for &id in &data.insts {
                let inst = &self.func.insts[id.index()];
                self.work.spend(inst.args.len() + 1, fail)?;
                uses.extend(inst.args.iter().copied().map(|value| (block, value)));
                if let Some(frame) = inst.frame {
                    frames.push((block, frame));
                }
            }
            match &data.term {
                Term::Return(value) | Term::Branch { flag: value, .. } => {
                    uses.push((block, *value));
                }
                Term::Switch { value, table, .. } => {
                    uses.extend([(block, *value), (block, *table)]);
                }
                Term::Deopt(frame) => frames.push((block, *frame)),
                _ => {}
            }
            for edge in data.term.edges() {
                self.work.spend(edge.args.len(), fail)?;
                uses.extend(edge.args.iter().copied().map(|value| (block, value)));
            }
        }
        for source in self.func.source_states.iter().flatten() {
            self.work
                .spend(source.pre.len() + source.post.len() + 1, fail)?;
            uses.extend(
                source
                    .pre
                    .iter()
                    .chain(&source.post)
                    .copied()
                    .map(|value| (source.block, value)),
            );
            frames.push((source.block, source.frame));
        }
        let mut seen_frames = HashSet::new();
        while let Some((block, frame)) = frames.pop() {
            self.work.spend(1, fail)?;
            if !seen_frames.insert((block, frame)) {
                continue;
            }
            let data = &self.func.frames[frame.index()];
            self.work.spend(data.stack.len(), fail)?;
            uses.extend(data.stack.iter().copied().map(|value| (block, value)));
            if let Some(parent) = data.parent {
                frames.push((block, parent));
            }
        }
        let mut live = vec![HashSet::<Value>::new(); self.func.blocks.len()];
        // Cache-only transports are already independently shape/edge certified.
        // Include these actual participants as well as whole-Func semantic uses.
        // Whole-Func uses prevent omitting a cache-phi sidecar to evade this check.
        for (&block, ids) in &self.cache_phis {
            for &id in ids {
                self.work.spend(1, fail)?;
                uses.push((block, self.version(id)?.owner));
            }
        }
        for (block, value) in uses {
            self.work.spend(1, value)?;
            let owner = self.resolved(value)?;
            let Some(info) = self.table.owners.get(&owner) else {
                continue;
            };
            if !matches!(info.kind, RecipeKind::Number(_))
                || !self
                    .index
                    .available(self.func, owner, Cut { block, position: 0 })
                || matches!(self.value(owner)?.def,
                    ValueDef::Param { block: definition, .. } if definition == block)
            {
                continue;
            }
            live[block.index()].insert(owner);
        }
        Ok(live
            .into_iter()
            .map(|owners| {
                let mut owners = owners.into_iter().collect::<Vec<_>>();
                owners.sort_unstable();
                owners
            })
            .collect())
    }

    fn dominating_partial_edges(&self) -> Result<(), SinkVerifyError> {
        // With no logical phi there is no outgoing partial-identity split. Do not
        // rescan whole-Func semantic uses for ordinary straight-line numeric code.
        if self.table.edges.is_empty() {
            return Ok(());
        }
        let live = self.live_entry_numbers()?;
        for edge in &self.table.edges {
            for &dominating in &live[edge.target.index()] {
                self.work.spend(1, edge.owner_param)?;
                if !self.identity_equal(edge.incoming_owner, dominating)
                    || self.identity_equal(edge.owner_param, dominating)
                {
                    continue;
                }
                let point = RecipePoint::Term(edge.source);
                let id = *self
                    .table
                    .uses
                    .get(&(point, dominating))
                    .ok_or_else(|| error(dominating, Some(point), Why::LostPartialAlias))?;
                let current = self.version(id)?;
                // The exact source Term view is subsequently checked against the
                // independently reconstructed flow state. An advertised box alone
                // cannot certify the current version or its availability.
                ensure(
                    current.owner == dominating
                        && self.index.available(
                            self.func,
                            dominating,
                            Cut {
                                block: edge.source,
                                position: self.func.blocks[edge.source.index()].insts.len(),
                            },
                        )
                        && self.boxed[edge.incoming_version.0 as usize]
                        && self.boxed[id.0 as usize]
                        && self.word_origin(edge.field_args[3])?
                            == self.word_origin(box_field(current.fields))?,
                    edge.owner_param,
                    Some(point),
                    Why::LostPartialAlias,
                )?;
            }
        }
        Ok(())
    }
    fn add_point(&mut self, point: RecipePoint, owner: Value) -> Result<(), SinkVerifyError> {
        let cut = self.index.point(self.func, point, owner)?;
        let group = self.points.entry(cut).or_default();
        if !group.contains(&point) {
            group.push(point);
        }
        Ok(())
    }
    fn observations(&mut self) -> Result<(), SinkVerifyError> {
        for (&(point, owner), _) in &self.table.uses {
            self.owner(owner)?;
            self.work.spend(1, owner)?;
        }
        let use_points = self
            .table
            .uses
            .keys()
            .map(|&(point, owner)| (point, owner))
            .collect::<Vec<_>>();
        for (point, owner) in use_points {
            self.add_point(point, owner)?;
        }
        let frame_points = self.table.frames.keys().copied().collect::<Vec<_>>();
        for (point, frame) in frame_points {
            ensure(
                frame.index() < self.func.frames.len(),
                Value(0),
                Some(point),
                Why::MissingRootView,
            )?;
            self.add_point(point, Value(0))?;
        }
        for (i, inst) in self.func.insts.iter().enumerate() {
            if let Some(frame) = inst.frame {
                self.required_frames
                    .entry(RecipePoint::Before(Inst(i as u32)))
                    .or_default()
                    .push(frame);
            }
        }
        for (pc, source) in self.func.source_states.iter().enumerate() {
            if let Some(source) = source {
                for point in [
                    RecipePoint::SourcePre(pc as u32),
                    RecipePoint::SourcePost(pc as u32),
                ] {
                    self.required_frames
                        .entry(point)
                        .or_default()
                        .push(source.frame);
                }
            }
        }
        for (i, block) in self.func.blocks.iter().enumerate() {
            if let Term::Deopt(frame) = block.term {
                self.required_frames
                    .entry(RecipePoint::Term(Block(i as u32)))
                    .or_default()
                    .push(frame);
            }
        }
        let required = self.required_frames.keys().copied().collect::<Vec<_>>();
        for point in required {
            self.add_point(point, Value(0))?;
        }
        Ok(())
    }
    fn current(
        &mut self,
        state: &State,
        owner: Value,
        version: RecipeVersionId,
        point: RecipePoint,
    ) -> Result<(), SinkVerifyError> {
        self.work.spend(1, owner)?;
        let cut = self.index.point(self.func, point, owner)?;
        let data = self.version(version)?;
        ensure(data.owner == owner, owner, Some(point), Why::MissingOwner)?;
        for field in tuple(data.fields) {
            ensure(
                self.index.available(self.func, field, cut),
                owner,
                Some(point),
                Why::FutureBoxVersion,
            )?;
        }
        ensure(
            state.get(&owner) == Some(&version),
            owner,
            Some(point),
            Why::StaleBoxVersion,
        )
    }
    fn frame_owners(
        &mut self,
        frame: FrameId,
        state: &State,
    ) -> Result<Vec<Value>, SinkVerifyError> {
        // Each view describes one frame. Parent frames have their own views;
        // unboxed Cons children belong to this frame's recursive root closure.
        let frame = self
            .func
            .frames
            .get(frame.index())
            .ok_or_else(|| error(Value(0), None, Why::MissingRootView))?;
        let mut pending = frame.stack.to_vec();
        let mut seen = HashSet::new();
        let mut owners = Vec::new();
        while let Some(value) = pending.pop() {
            self.work.spend(1, value)?;
            let owner = self.resolved(value)?;
            if !self.table.owners.contains_key(&owner) || !seen.insert(owner) {
                continue;
            }
            owners.push(owner);
            let version = *state
                .get(&owner)
                .ok_or_else(|| error(owner, None, Why::MissingRootView))?;
            if let RecipeFields::Cons(fields) = self.version(version)?.fields
                && !self.boxed[version.0 as usize]
            {
                ensure(
                    self.is_unboxed_cons(version)?,
                    owner,
                    None,
                    Why::MissingRootView,
                )?;
                pending.extend([fields.car, fields.cdr]);
            }
        }
        owners.sort_unstable();
        Ok(owners)
    }
    fn at_cut(&mut self, cut: Cut, state: &State) -> Result<(), SinkVerifyError> {
        let points = self.points.get(&cut).cloned().unwrap_or_default();
        if points.is_empty() {
            return Ok(());
        }
        // Entry/Before/After/source observations can name the same CFG cut.
        // Their state and proved identity partition are identical, while all
        // point-specific current-version and frame checks below remain exact.
        let mut representatives = HashMap::new();
        for &owner in state.keys() {
            self.work.spend(1, owner)?;
            let group = self.identity[self.owner_index[&owner]];
            representatives
                .entry(group)
                .and_modify(|old: &mut Value| *old = (*old).min(owner))
                .or_insert(owner);
        }
        for point in points {
            let uses = self.point_uses.get(&point).cloned().unwrap_or_default();
            for &owner in state.keys() {
                self.canonical.insert(
                    (point, owner),
                    representatives[&self.identity[self.owner_index[&owner]]],
                );
            }
            for (owner, version) in uses {
                self.current(state, owner, version, point)?;
            }
            let mut frames = self
                .required_frames
                .get(&point)
                .cloned()
                .unwrap_or_default();
            frames.extend(self.point_frames.get(&point).into_iter().flatten().copied());
            let mut pending = frames.clone();
            let mut seen = HashSet::new();
            while let Some(frame) = pending.pop() {
                self.work.spend(1, Value(0))?;
                if !seen.insert(frame) {
                    continue;
                }
                let data = self
                    .func
                    .frames
                    .get(frame.index())
                    .ok_or_else(|| error(Value(0), Some(point), Why::MissingRootView))?;
                if let Some(parent) = data.parent {
                    frames.push(parent);
                    pending.push(parent);
                }
            }
            frames.sort_unstable();
            frames.dedup();
            for frame in frames {
                let owners = self.frame_owners(frame, state)?;
                let view = self.table.frames.get(&(point, frame));
                if owners.is_empty() {
                    ensure(
                        view.is_none_or(|v| v.versions.is_empty()),
                        Value(0),
                        Some(point),
                        Why::MissingRootView,
                    )?;
                    continue;
                }
                let view =
                    view.ok_or_else(|| error(owners[0], Some(point), Why::MissingRootView))?;
                self.work.spend(view.versions.len(), owners[0])?;
                ensure(
                    view.versions.len() == owners.len()
                        && view
                            .versions
                            .iter()
                            .map(|&(owner, _)| owner)
                            .eq(owners.iter().copied()),
                    owners[0],
                    Some(point),
                    Why::MissingRootView,
                )?;
                for &(owner, version) in &view.versions {
                    self.current(state, owner, version, point)?;
                    ensure(
                        self.table.uses.get(&(point, owner)) == Some(&version),
                        owner,
                        Some(point),
                        Why::MissingRootView,
                    )?;
                }
                // Root traversal proves nested semantic owners have an exact
                // current use map too; it never scans every historical owner.
                let mut pending = owners;
                let mut seen = HashSet::new();
                while let Some(owner) = pending.pop() {
                    self.work.spend(1, owner)?;
                    if !seen.insert(owner) {
                        continue;
                    }
                    let version = *state
                        .get(&owner)
                        .ok_or_else(|| error(owner, Some(point), Why::MissingRootView))?;
                    if let RecipeFields::Cons(f) = self.version(version)?.fields
                        && !self.boxed[version.0 as usize]
                    {
                        ensure(
                            self.is_unboxed_cons(version)?,
                            owner,
                            Some(point),
                            Why::MissingRootView,
                        )?;
                        for child in [f.car, f.cdr] {
                            if self.table.owners.contains_key(&child) {
                                ensure(
                                    self.table.uses.get(&(point, child)) == state.get(&child),
                                    child,
                                    Some(point),
                                    Why::MissingRootView,
                                )?;
                                pending.push(child);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn is_unboxed_cons(&self, version: RecipeVersionId) -> Result<bool, SinkVerifyError> {
        unboxed_cons_version(self.table, version, &self.work)
    }
    fn merge(
        &mut self,
        block: Block,
        ends: &[Option<State>],
        strict: bool,
    ) -> Result<State, SinkVerifyError> {
        let cut = Cut { block, position: 0 };
        self.work
            .spend(self.index.preds[block.index()].len(), Value(0))?;
        let mut known = self.index.preds[block.index()]
            .iter()
            .filter_map(|&(pred, _)| ends[pred.index()].as_ref());
        let mut state = if let Some(first) = known.next() {
            first.clone()
        } else {
            State::new()
        };
        self.work.spend(state.len(), Value(0))?;
        for input in known {
            self.work.spend(state.len(), Value(0))?;
            state.retain(|owner, version| input.get(owner) == Some(version));
        }
        state.retain(|&owner, _| self.index.available(self.func, owner, cut));
        for &param in &self.func.blocks[block.index()].params {
            self.work.spend(1, param)?;
            if let Some(info) = self.table.owners.get(&param) {
                state.insert(param, info.definition_version);
            }
        }
        if let Some(phis) = self.cache_phis.get(&block) {
            for &version in phis {
                state.insert(self.version(version)?.owner, version);
            }
        }
        self.apply_alias_updates(cut, &mut state, strict)?;
        Ok(state)
    }
    fn apply_alias_updates(
        &mut self,
        cut: Cut,
        state: &mut State,
        strict: bool,
    ) -> Result<(), SinkVerifyError> {
        let mut waiting = self.updates.get(&cut).cloned().unwrap_or_default();
        let events = waiting.clone();
        // A CacheAfter is applied first; dependent aliases can appear in any
        // table order and are topologically consumed against the actual state.
        loop {
            let mut progress = false;
            let mut next = Vec::new();
            for id in waiting {
                self.work.spend(1, self.version(id)?.owner)?;
                let version = self.version(id)?.clone();
                let owner = version.owner;
                let (previous, input) = match version.cause {
                    VersionCause::CacheAfter { previous, .. } => (previous, None),
                    VersionCause::AliasCacheAfter { previous, input } => (previous, Some(input)),
                    _ => unreachable!(),
                };
                if state.get(&owner) != Some(&previous)
                    || input.is_some_and(|input| {
                        state.get(&self.table.versions[input.0 as usize].owner) != Some(&input)
                    })
                {
                    next.push(id);
                    continue;
                }
                state.insert(owner, id);
                progress = true;
                if strict {
                    let point = if cut.position == 0 {
                        RecipePoint::Entry(cut.block)
                    } else {
                        RecipePoint::After(
                            self.func.blocks[cut.block.index()].insts[cut.position - 1],
                        )
                    };
                    for field in tuple(version.fields) {
                        ensure(
                            self.index.available(self.func, field, cut),
                            owner,
                            Some(point),
                            Why::FutureBoxVersion,
                        )?;
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            if !progress {
                if strict {
                    return Err(error(
                        self.version(next[0])?.owner,
                        None,
                        Why::StaleBoxVersion,
                    ));
                }
                break;
            }
            waiting = next;
        }
        if strict {
            // A materialized identity must update every currently available
            // proven alias, even when that alias has different phi fields.
            // Otherwise a later cold view could allocate it a second time.
            for id in events {
                let version = self.version(id)?;
                let group = self.identity[self.owner_index[&version.owner]];
                for &alias in &self.identity_members[group] {
                    self.work.spend(1, alias)?;
                    let Some(&current) = state.get(&alias) else {
                        continue;
                    };
                    if self.identity_equal(alias, version.owner) {
                        ensure(
                            self.same_box(
                                box_field(self.version(current)?.fields),
                                box_field(version.fields),
                            )?,
                            alias,
                            None,
                            Why::StaleBoxVersion,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
    fn physical_box_use(
        &self,
        state: &State,
        value: Value,
        point: RecipePoint,
    ) -> Result<(), SinkVerifyError> {
        let value = self.resolved(value)?;
        let mut transport = false;
        // Preserve exact box-field SSA matching: resolving an unrelated box
        // definition here would broaden the old transport acceptance rule.
        for &id in self.box_versions.get(&value).into_iter().flatten() {
            self.work.spend(1, value)?;
            let version = self.version(id)?;
            if box_field(version.fields) == value {
                transport = matches!(
                    self.value(value)?.def,
                    ValueDef::Param { .. } | ValueDef::Inst(_)
                );
                if let ValueDef::Inst(inst) = self.value(value)?.def {
                    transport = matches!(
                        self.func.insts[inst.index()].op,
                        Opcode::Sink(
                            SinkOp::RecipeField(RecipeField::RealBox) | SinkOp::CacheBoxAfter
                        )
                    );
                }
                if !transport {
                    return Ok(());
                }
                if let Some(&current) = state.get(&version.owner) {
                    if self.boxed[current.0 as usize]
                        && self.same_box(value, box_field(self.version(current)?.fields))?
                    {
                        return Ok(());
                    }
                }
            }
        }
        ensure(!transport, value, Some(point), Why::StaleBoxVersion)
    }
    fn transfer(
        &mut self,
        block: Block,
        mut state: State,
        strict: bool,
    ) -> Result<State, SinkVerifyError> {
        let instructions = self.func.blocks[block.index()].insts.clone();
        for position in 0..=instructions.len() {
            let cut = Cut { block, position };
            self.work.spend(state.len() + 1, Value(0))?;
            if strict {
                self.at_cut(cut, &state)?;
            }
            let Some(&inst) = instructions.get(position) else {
                if strict {
                    let operands = match self.func.blocks[block.index()].term {
                        Term::Return(v) => vec![v],
                        Term::Branch { flag, .. } => vec![flag],
                        Term::Switch { value, table, .. } => vec![value, table],
                        _ => Vec::new(),
                    };
                    for operand in operands {
                        self.physical_box_use(&state, operand, RecipePoint::Term(block))?;
                    }
                }
                break;
            };
            let data = &self.func.insts[inst.index()];
            if strict && !matches!(data.op, Opcode::Sink(SinkOp::RecipeField(_))) {
                for (arg_index, &arg) in data.args.iter().enumerate() {
                    if self.table.owners.contains_key(&arg) {
                        let current = *state.get(&arg).ok_or_else(|| {
                            error(arg, Some(RecipePoint::Before(inst)), Why::StaleBoxVersion)
                        })?;
                        ensure(
                            self.table.uses.get(&(RecipePoint::Before(inst), arg))
                                == Some(&current),
                            arg,
                            Some(RecipePoint::Before(inst)),
                            Why::MissingRootView,
                        )?;
                        if arg_index == 0
                            && matches!(
                                data.op,
                                Opcode::Sink(SinkOp::MaterializeNum | SinkOp::MaterializeCons)
                            )
                        {
                            let mut expected = vec![arg];
                            expected.extend(tuple(self.version(current)?.fields));
                            ensure(
                                data.args == expected,
                                arg,
                                Some(RecipePoint::Before(inst)),
                                Why::StaleBoxVersion,
                            )?;
                        }
                        if arg_index > 0 && matches!(data.op, Opcode::Sink(SinkOp::MaterializeCons))
                        {
                            ensure(
                                self.boxed[current.0 as usize],
                                arg,
                                Some(RecipePoint::Before(inst)),
                                Why::StaleBoxVersion,
                            )?;
                        }
                    }
                }
                if !matches!(data.op, Opcode::Sink(_)) {
                    for &arg in &data.args {
                        self.physical_box_use(&state, arg, RecipePoint::Before(inst))?;
                    }
                }
            }
            if strict && let Some(&(owner, input)) = self.same_identity_inputs.get(&inst) {
                ensure(
                    state.get(&self.version(input)?.owner) == Some(&input),
                    owner,
                    Some(RecipePoint::Before(inst)),
                    Why::StaleBoxVersion,
                )?;
            }
            if let Some(&(owner, version)) = self.definitions.get(&inst) {
                state.insert(owner, version);
            }
            self.apply_alias_updates(
                Cut {
                    block,
                    position: position + 1,
                },
                &mut state,
                strict,
            )?;
        }
        Ok(state)
    }
    fn flow(&mut self) -> Result<(), SinkVerifyError> {
        let mut ends = vec![None; self.func.blocks.len()];
        loop {
            let mut changed = false;
            for block in self.index.order.clone() {
                let start = self.merge(block, &ends, false)?;
                let finish = self.transfer(block, start, false)?;
                if ends[block.index()].as_ref() != Some(&finish) {
                    ends[block.index()] = Some(finish);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // Stable predecessor state, not the transformation's map, certifies
        // every incoming tuple/cache phi and every early/late observer cut.
        for edge in &self.table.edges {
            let state = ends[edge.source.index()]
                .as_ref()
                .ok_or_else(|| error(edge.incoming_owner, None, Why::InvalidPoint))?;
            ensure(
                state.get(&edge.incoming_owner) == Some(&edge.incoming_version),
                edge.incoming_owner,
                Some(RecipePoint::Term(edge.source)),
                Why::StaleBoxVersion,
            )?;
        }
        for version in &self.table.versions {
            if let VersionCause::CachePhi {
                block,
                ref incoming,
                ..
            } = version.cause
            {
                for CachePhiEdge {
                    source,
                    version: incoming,
                    ..
                } in incoming
                {
                    ensure(
                        ends[source.index()]
                            .as_ref()
                            .and_then(|state| state.get(&version.owner))
                            == Some(incoming),
                        version.owner,
                        Some(RecipePoint::Entry(block)),
                        Why::WrongCachePhi,
                    )?;
                }
            }
        }
        for block in self.index.order.clone() {
            let start = self.merge(block, &ends, true)?;
            self.transfer(block, start, true)?;
        }
        Ok(())
    }
    fn raw_and_logical_uses(&mut self) -> Result<(), SinkVerifyError> {
        let mut raw_words = HashSet::new();
        let mut payloads = HashSet::new();
        let mut ready_fields = HashSet::new();
        let mut permitted_sink = HashSet::new();
        for version in &self.table.versions {
            if let RecipeFields::Number(f) = version.fields {
                raw_words.insert(f.word);
                payloads.insert(f.payload);
                ready_fields.insert(f.ready);
            }
            match version.cause {
                VersionCause::CacheAfter {
                    materialize,
                    box_projection,
                    ..
                } => {
                    permitted_sink.insert(materialize);
                    permitted_sink.insert(box_projection);
                }
                _ => {}
            }
        }
        for info in self.table.owners.values() {
            match info.origin {
                RecipeOrigin::Borrow { inst, .. }
                | RecipeOrigin::NumericSource { inst, .. }
                | RecipeOrigin::ConsSource { inst, .. } => {
                    permitted_sink.insert(inst);
                    let cut = self.index.positions[inst.index()].unwrap();
                    let count = if info.kind == RecipeKind::Cons { 1 } else { 4 };
                    permitted_sink.extend(
                        self.func.blocks[cut.block.index()].insts
                            [cut.position + 1..cut.position + 1 + count]
                            .iter()
                            .copied(),
                    );
                }
                RecipeOrigin::SameIdentityView { inst, .. } => {
                    let version = self.version(info.definition_version)?;
                    let VersionCause::SameIdentity { input } = version.cause else {
                        return Err(error(info.owner, None, Why::InvalidSourceOperation));
                    };
                    if version.fields != self.version(input)?.fields {
                        // owners_and_producers has independently validated the
                        // complete copied unit; Refine itself is not a Sink op.
                        let cut = self.index.positions[inst.index()].unwrap();
                        permitted_sink.extend(
                            self.func.blocks[cut.block.index()].insts
                                [cut.position + 1..cut.position + 5]
                                .iter()
                                .copied(),
                        );
                    }
                }
                _ => {}
            }
        }
        for (i, value) in self.func.values.iter().enumerate() {
            self.work.spend(1, Value(i as u32))?;
            if value.rep == Rep::RawWord {
                ensure(
                    raw_words.contains(&self.resolved(Value(i as u32))?),
                    Value(i as u32),
                    None,
                    Why::UnknownRawWordUse,
                )?;
            }
            if value.rep == Rep::NumPair {
                ensure(
                    self.table.owners.contains_key(&Value(i as u32)),
                    Value(i as u32),
                    None,
                    Why::MissingOwner,
                )?;
            }
        }
        for (i, inst) in self.func.insts.iter().enumerate() {
            if matches!(inst.op, Opcode::Sink(_)) {
                ensure(
                    permitted_sink.contains(&Inst(i as u32)),
                    inst.result.unwrap_or(Value(0)),
                    None,
                    Why::InvalidSourceOperation,
                )?;
            }
            for (arg_index, &arg) in inst.args.iter().enumerate() {
                self.work.spend(1, arg)?;
                let arg = self.resolved(arg)?;
                if raw_words.contains(&arg)
                    || payloads.contains(&arg)
                    || ready_fields.contains(&arg)
                {
                    ensure(
                        matches!(inst.op, Opcode::Sink(SinkOp::MaterializeNum))
                            && matches!(arg_index, 1 | 2 | 3),
                        arg,
                        None,
                        Why::UnknownRawWordUse,
                    )?;
                }
                if self.table.owners.contains_key(&arg) {
                    if matches!(inst.op, Opcode::Refine(_)) {
                        ensure(
                            inst.result.is_some_and(|result| {
                                self.table.owners.get(&result).is_some_and(|owner| {
                                    matches!(owner.origin, RecipeOrigin::SameIdentityView { .. })
                                })
                            }),
                            arg,
                            None,
                            Why::InvalidSourceOperation,
                        )?;
                    }
                    ensure(
                        matches!(
                            inst.op,
                            Opcode::Sink(
                                SinkOp::SourceNum(_)
                                    | SinkOp::SourceSqrt
                                    | SinkOp::SourceCons(_)
                                    | SinkOp::RecipeField(_)
                                    | SinkOp::MaterializeNum
                                    | SinkOp::MaterializeCons
                                    | SinkOp::CacheBoxAfter
                            ) | Opcode::Refine(_)
                        ),
                        arg,
                        None,
                        Why::InvalidSourceOperation,
                    )?;
                }
            }
        }
        for (i, block) in self.func.blocks.iter().enumerate() {
            for edge in block.term.edges() {
                for (&arg, &param) in edge
                    .args
                    .iter()
                    .zip(&self.func.blocks[edge.target.index()].params)
                {
                    if raw_words.contains(&arg)
                        || payloads.contains(&arg)
                        || ready_fields.contains(&arg)
                    {
                        ensure(
                            raw_words.contains(&param)
                                || payloads.contains(&param)
                                || ready_fields.contains(&param),
                            arg,
                            Some(RecipePoint::Term(Block(i as u32))),
                            Why::UnknownRawWordUse,
                        )?;
                    }
                }
            }
            let escaped = match block.term {
                Term::Return(v) => vec![v],
                Term::Branch { flag, .. } => vec![flag],
                Term::Switch { value, table, .. } => vec![value, table],
                _ => Vec::new(),
            };
            for value in escaped {
                ensure(
                    !raw_words.contains(&value)
                        && !payloads.contains(&value)
                        && !ready_fields.contains(&value)
                        && !self.table.owners.contains_key(&value),
                    value,
                    Some(RecipePoint::Term(Block(i as u32))),
                    Why::UnknownRawWordUse,
                )?;
            }
        }
        for frame in &self.func.frames {
            for &value in &frame.stack {
                self.work.spend(1, value)?;
                let value = self.resolved(value)?;
                ensure(
                    !raw_words.contains(&value)
                        && !payloads.contains(&value)
                        && !ready_fields.contains(&value),
                    value,
                    None,
                    Why::UnknownRawWordUse,
                )?;
            }
        }
        for stack in &self.func.entry_stacks {
            for &value in stack {
                self.work.spend(1, value)?;
                let value = self.resolved(value)?;
                ensure(
                    !raw_words.contains(&value)
                        && !payloads.contains(&value)
                        && !ready_fields.contains(&value),
                    value,
                    None,
                    Why::UnknownRawWordUse,
                )?;
            }
        }
        for source in self.func.source_states.iter().flatten() {
            for &value in source.pre.iter().chain(source.post.iter()) {
                self.work.spend(1, value)?;
                let value = self.resolved(value)?;
                ensure(
                    !raw_words.contains(&value)
                        && !payloads.contains(&value)
                        && !ready_fields.contains(&value),
                    value,
                    None,
                    Why::UnknownRawWordUse,
                )?;
            }
        }
        Ok(())
    }
}

pub(crate) fn verify_recipes<'a>(
    func: &Func,
    table: &'a SinkRecipes,
) -> Result<VerifiedSinkRecipes<'a>, SinkVerifyError> {
    #[cfg(test)]
    super::super::native_verify_observer::entered(
        super::super::native_verify_observer::Checker::Recipes,
    );
    let mut work = Work(std::cell::Cell::new(0));
    work.spend(
        table.owners.len()
            + table.versions.len()
            + table.edges.len()
            + table.uses.len()
            + table.frames.len(),
        Value(0),
    )?;
    let index = Index::new(func, &mut work)?;
    let mut owners = table.owners.keys().copied().collect::<Vec<_>>();
    owners.sort_unstable();
    let owner_index = owners.iter().enumerate().map(|(i, &v)| (v, i)).collect();
    let mut phi_edges: HashMap<Value, Vec<usize>> = HashMap::new();
    for (i, edge) in table.edges.iter().enumerate() {
        phi_edges.entry(edge.owner_param).or_default().push(i);
    }
    let mut point_uses: HashMap<RecipePoint, Vec<(Value, RecipeVersionId)>> = HashMap::new();
    for (&(point, owner), &version) in &table.uses {
        point_uses.entry(point).or_default().push((owner, version));
    }
    let mut point_frames: HashMap<RecipePoint, Vec<FrameId>> = HashMap::new();
    for &(point, frame) in table.frames.keys() {
        point_frames.entry(point).or_default().push(frame);
    }
    let mut box_versions: HashMap<Value, Vec<RecipeVersionId>> = HashMap::new();
    let mut materializer_versions: HashMap<Inst, Vec<RecipeVersionId>> = HashMap::new();
    for (i, version) in table.versions.iter().enumerate() {
        work.spend(1, version.owner)?;
        let id = RecipeVersionId(i as u32);
        box_versions
            .entry(box_field(version.fields))
            .or_default()
            .push(id);
        if let VersionCause::CacheAfter { materialize, .. } = version.cause {
            materializer_versions
                .entry(materialize)
                .or_default()
                .push(id);
        }
    }
    let mut check = Check {
        func,
        table,
        index,
        work,
        dependencies: vec![Vec::new(); owners.len()],
        identity: Vec::new(),
        box_versions,
        materializer_versions,
        identity_members: Vec::new(),
        boxed: Vec::new(),
        owners,
        owner_index,
        definitions: HashMap::new(),
        same_identity_inputs: HashMap::new(),
        updates: HashMap::new(),
        cache_phis: HashMap::new(),
        points: HashMap::new(),
        required_frames: HashMap::new(),
        phi_edges,
        point_uses,
        point_frames,
        canonical: HashMap::new(),
    };
    check.owners_and_producers()?;
    check.grounding()?;
    check.identities()?;
    check.versions()?;
    check.edges()?;
    check.observations()?;
    check.raw_and_logical_uses()?;
    check.flow()?;
    Ok(VerifiedSinkRecipes {
        recipes: table,
        canonical: check.canonical,
        boxed: check.boxed,
    })
}

/// Roots are expanded from the full exact original frame and actual live SSA,
/// never all metadata. This function requires the independent capability.
/// Guaranteed Cons caches STOP traversal of historical car/cdr values, before
/// and after real mutation; otherwise old weak/finalizer reachability changes.
pub(crate) fn roots_at(
    func: &Func,
    verified: &VerifiedSinkRecipes<'_>,
    point: RecipePoint,
    frame: FrameId,
    live: &[Value],
) -> Result<Vec<Value>, SinkVerifyError> {
    let table = verified.recipes;
    let work = Work(std::cell::Cell::new(0));
    work.spend(live.len(), Value(0))?;
    let mut pending = live.to_vec();
    let mut next = Some(frame);
    let mut frames = HashSet::new();
    let view = table.frames.get(&(point, frame));
    while let Some(frame) = next {
        work.spend(1, Value(0))?;
        ensure(
            frames.insert(frame),
            Value(0),
            Some(point),
            Why::MissingRootView,
        )?;
        let data = func
            .frames
            .get(frame.index())
            .ok_or_else(|| error(Value(0), Some(point), Why::MissingRootView))?;
        work.spend(data.stack.len(), Value(0))?;
        pending.extend_from_slice(&data.stack);
        next = data.parent;
    }
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    while let Some(value) = pending.pop() {
        work.spend(1, value)?;
        let value = func
            .resolve(value)
            .ok_or_else(|| error(value, Some(point), Why::MissingOwner))?;
        if !seen.insert(value) {
            continue;
        }
        let data = func
            .values
            .get(value.index())
            .ok_or_else(|| error(value, Some(point), Why::MissingOwner))?;
        if table.owners.contains_key(&value) {
            let version = view
                .and_then(|view| {
                    view.versions
                        .iter()
                        .find(|(owner, _)| *owner == value)
                        .map(|entry| entry.1)
                })
                .or_else(|| table.uses.get(&(point, value)).copied())
                .ok_or_else(|| error(value, Some(point), Why::MissingRootView))?;
            let fields = table.versions[version.0 as usize].fields;
            match fields {
                RecipeFields::Number(f) => {
                    if func.values[f.real_box.index()].ty.may_need_root() {
                        pending.push(f.real_box);
                    }
                }
                RecipeFields::Cons(f) => {
                    if verified.guaranteed_boxed(version) {
                        pending.push(f.real_box);
                    } else {
                        ensure(
                            unboxed_cons_version(table, version, &work)?,
                            value,
                            Some(point),
                            Why::MissingRootView,
                        )?;
                        pending.extend([f.car, f.cdr]);
                    }
                }
            }
        } else {
            match data.rep {
                Rep::Tagged if data.ty.may_need_root() => roots.push(value),
                Rep::RawPtr { base } => pending.push(base),
                Rep::Virtual(_) | Rep::NumPair => {
                    return Err(error(value, Some(point), Why::MissingOwner));
                }
                Rep::Tagged
                | Rep::TaggedFix
                | Rep::RawInt
                | Rep::RawF64
                | Rep::RawWord
                | Rep::Bool => {}
            }
        }
    }
    roots.sort_unstable();
    roots.dedup();
    Ok(roots)
}

#[cfg(test)]
#[path = "tests/verify.rs"]
mod tests;
