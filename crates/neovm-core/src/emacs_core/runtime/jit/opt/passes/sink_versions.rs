//! Real SSA cache/version construction for selected allocation recipes. No runtime/TLS state is retained.

use super::front::{edge_mut, numeric_values};
use crate::emacs_core::jit::opt::sink_recipes::*;
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    types::TypeSet,
    verify::VerifyError,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub(crate) enum PassError {
    Invalid(VerifyError),
    AnalysisLimit,
    Decline,
}
impl From<VerifyError> for PassError {
    fn from(error: VerifyError) -> Self {
        Self::Invalid(error)
    }
}

/// Scoped compiler state. Current versions are copied into immutable point
/// maps; the backend never consults this mutable map. Alias groups contain only
/// copied views or phi pairs proved identical on every original edge occurrence.
pub(crate) struct Versions {
    pub(crate) current: HashMap<Value, RecipeVersionId>,
    pub(crate) aliases: HashMap<Value, Vec<Value>>,
    pub(crate) work: usize,
    pub(crate) limit: usize,
}

impl Versions {
    pub(crate) fn spend(&mut self, n: usize) -> Result<(), PassError> {
        self.work = self.work.saturating_add(n);
        if self.work > self.limit {
            return Err(PassError::AnalysisLimit);
        }
        Ok(())
    }

    pub(crate) fn version(&self, owner: Value) -> Result<RecipeVersionId, PassError> {
        self.current
            .get(&owner)
            .copied()
            .ok_or_else(|| VerifyError::BadDefinition(owner).into())
    }

    /// Record precisely the available version at this instruction/source cut.
    /// Sidecar proof retention does not make all these owners runtime roots.
    pub(crate) fn observe(
        &mut self,
        func: &Func,
        table: &mut SinkRecipes,
        point: RecipePoint,
        frame: Option<FrameId>,
    ) -> Result<(), PassError> {
        self.spend(self.current.len())?;
        for (&owner, &version) in &self.current {
            table.uses.insert((point, owner), version);
        }
        let mut next = frame;
        let mut seen = HashSet::new();
        while let Some(id) = next {
            if !seen.insert(id) {
                return Err(VerifyError::FrameCycle(id).into());
            }
            let data = &func.frames[id.index()];
            let mut pending = data.stack.to_vec();
            let mut owners = HashSet::new();
            let mut versions = Vec::new();
            while let Some(value) = pending.pop() {
                self.spend(1)?;
                let value = func.resolve(value).ok_or(VerifyError::AliasCycle(value))?;
                if !table.owners.contains_key(&value) || !owners.insert(value) {
                    continue;
                }
                let version = self.version(value)?;
                versions.push((value, version));
                if let RecipeFields::Cons(fields) = table.versions[version.0 as usize].fields {
                    if !guaranteed_word(table, version, self.limit)? {
                        pending.extend([fields.car, fields.cdr]);
                    }
                }
            }
            versions.sort_unstable_by_key(|entry| entry.0);
            table.frames.insert(
                (point, id),
                FrameRecipeView {
                    versions: versions.into(),
                },
            );
            next = data.parent;
        }
        Ok(())
    }

    /// Fresh definitions reset here on EVERY runtime execution, including an
    /// operation revisited in a loop. A static source Inst is not an identity.
    pub(crate) fn define(&mut self, table: &SinkRecipes, owner: Value) {
        self.current
            .insert(owner, table.owners[&owner].definition_version);
    }

    /// A copied logical view joins the complete proved-identity group. Updating
    /// only its immediate input would leave a sibling copied view with a stale
    /// NIL cache after that same object escapes through another alias.
    pub(crate) fn add_alias(&mut self, owner: Value, input: Value) {
        let mut group = vec![owner, input];
        group.extend(self.aliases.get(&owner).into_iter().flatten().copied());
        group.extend(self.aliases.get(&input).into_iter().flatten().copied());
        group.sort_unstable();
        group.dedup();
        for &alias in &group {
            self.aliases.insert(alias, group.clone());
        }
    }

    /// Borrow/guaranteed cached versions already contain the exact Lisp word.
    /// Other versions emit explicit Materialize + CacheBoxAfter instructions.
    /// Cons children are completed first; post-escape reads never use fields.
    pub(crate) fn materialize(
        &mut self,
        func: &mut Func,
        table: &mut SinkRecipes,
        owner: Value,
        frame: Option<FrameId>,
        pc: u32,
        order: &mut Vec<Inst>,
        stats: &mut SinkStats,
    ) -> Result<Value, PassError> {
        let mut pending = vec![(owner, false)];
        let mut active = HashSet::new();
        while let Some((owner, children_done)) = pending.pop() {
            self.spend(1)?;
            let version = self.version(owner)?;
            if guaranteed_word(table, version, self.limit)? {
                continue;
            }
            let fields = table.versions[version.0 as usize].fields;
            if !children_done {
                if !active.insert(owner) {
                    return Err(VerifyError::BadDefinition(owner).into());
                }
                pending.push((owner, true));
                if let RecipeFields::Cons(cons) = fields {
                    for child in [cons.cdr, cons.car] {
                        let child = func.resolve(child).ok_or(VerifyError::AliasCycle(child))?;
                        if table.owners.contains_key(&child) {
                            pending.push((child, false));
                        }
                    }
                }
                continue;
            }
            active.remove(&owner);
            let semantic = table.owners[&owner].semantic_type;
            let (op, mut args, old_box) = match fields {
                RecipeFields::Number(fields) => (
                    SinkOp::MaterializeNum,
                    numeric_values(fields),
                    fields.real_box,
                ),
                RecipeFields::Cons(fields) => (
                    SinkOp::MaterializeCons,
                    vec![fields.car, fields.cdr, fields.real_box],
                    fields.real_box,
                ),
            };
            args.insert(0, owner);
            let materialize = Inst(func.insts.len() as u32);
            self.observe(func, table, RecipePoint::Before(materialize), frame)?;
            let boxed = append(
                func,
                Opcode::Sink(op),
                args,
                semantic,
                Rep::Tagged,
                Effects::ALLOCATES,
                frame,
                pc,
                order,
            );
            self.observe(func, table, RecipePoint::After(materialize), frame)?;
            let cache_inst = Inst(func.insts.len() as u32);
            self.observe(func, table, RecipePoint::Before(cache_inst), None)?;
            let cache = append(
                func,
                Opcode::Sink(SinkOp::CacheBoxAfter),
                vec![owner, old_box, boxed],
                semantic,
                Rep::Tagged,
                Effects::PURE,
                None,
                pc,
                order,
            );
            let next = append_version(
                table,
                owner,
                with_box(fields, cache),
                VersionCause::CacheAfter {
                    previous: version,
                    materialize,
                    box_projection: cache_inst,
                },
            );
            self.current.insert(owner, next);
            self.publish_aliases(table, owner, next)?;
            self.observe(func, table, RecipePoint::After(cache_inst), None)?;
            stats.materializations += 1;
        }
        let version = self.version(owner)?;
        Ok(box_field(table.versions[version.0 as usize].fields))
    }

    fn publish_aliases(
        &mut self,
        table: &mut SinkRecipes,
        owner: Value,
        input: RecipeVersionId,
    ) -> Result<(), PassError> {
        let aliases = self.aliases.get(&owner).cloned().unwrap_or_default();
        let shared = box_field(table.versions[input.0 as usize].fields);
        for alias in aliases {
            if alias == owner {
                continue;
            }
            let Some(previous) = self.current.get(&alias).copied() else {
                continue;
            };
            let fields = table.versions[previous.0 as usize].fields;
            // Identity proof permits aliases to share the actual cache word.
            // Do not append a typed view here: this update takes effect just
            // after CacheBoxAfter, before any subsequently appended adapter.
            // An ordinary consumer may request its own exact-point word view.
            let next = append_version(
                table,
                alias,
                with_box(fields, shared),
                VersionCause::AliasCacheAfter { previous, input },
            );
            self.current.insert(alias, next);
        }
        Ok(())
    }
}

pub(crate) fn append(
    func: &mut Func,
    op: Opcode,
    args: Vec<Value>,
    ty: TypeSet,
    rep: Rep,
    eff: Effects,
    frame: Option<FrameId>,
    pc: u32,
    order: &mut Vec<Inst>,
) -> Value {
    let inst = Inst(func.insts.len() as u32);
    let result = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Inst(inst),
    });
    func.insts.push(InstData {
        op,
        args,
        result: Some(result),
        eff,
        mem: AliasClass::None,
        frame,
        pc,
    });
    order.push(inst);
    result
}

pub(crate) fn append_version(
    table: &mut SinkRecipes,
    owner: Value,
    fields: RecipeFields,
    cause: VersionCause,
) -> RecipeVersionId {
    let version = RecipeVersionId(table.versions.len() as u32);
    table.versions.push(RecipeVersion {
        owner,
        fields,
        cause,
    });
    version
}

pub(crate) fn box_field(fields: RecipeFields) -> Value {
    match fields {
        RecipeFields::Number(fields) => fields.real_box,
        RecipeFields::Cons(fields) => fields.real_box,
    }
}

pub(crate) fn with_box(mut fields: RecipeFields, value: Value) -> RecipeFields {
    match &mut fields {
        RecipeFields::Number(fields) => fields.real_box = value,
        RecipeFields::Cons(fields) => fields.real_box = value,
    }
    fields
}

/// Exact same-word adapter. Type narrowing is only allowed after an independent
/// semantic identity proof; a semantic recipe type is never a numeric guard.
pub(crate) fn word_view(
    func: &mut Func,
    value: Value,
    ty: TypeSet,
    rep: Rep,
    pc: u32,
    order: &mut Vec<Inst>,
) -> Result<Value, PassError> {
    let input = &func.values[value.index()];
    let ty = ty.meet(input.ty);
    if !input.rep.is_tagged() || ty.is_bottom() {
        return Err(VerifyError::TypeMismatch(value).into());
    }
    if input.rep == rep && input.ty == ty {
        return Ok(value);
    }
    Ok(append(
        func,
        Opcode::Refine(ty),
        vec![value],
        ty,
        rep,
        Effects::PURE,
        None,
        pc,
        order,
    ))
}

/// Structural, producer-grounded availability of an actual word. Parameter and
/// CachePhi cycles conservatively return false unless all inputs are grounded
/// boxed. No payload/SWP equality or sentinel-as-root assumption is used.
pub(crate) fn guaranteed_word(
    table: &SinkRecipes,
    start: RecipeVersionId,
    limit: usize,
) -> Result<bool, PassError> {
    let mut pending = vec![start];
    let mut seen = HashSet::new();
    let mut work = 0;
    while let Some(version) = pending.pop() {
        work += 1;
        if work > limit {
            return Err(PassError::AnalysisLimit);
        }
        if !seen.insert(version) {
            continue;
        }
        let data = &table.versions[version.0 as usize];
        match &data.cause {
            VersionCause::CacheAfter { .. } => {}
            VersionCause::AliasCacheAfter { input, .. } | VersionCause::SameIdentity { input } => {
                pending.push(*input)
            }
            VersionCause::Definition => {
                if !matches!(
                    table.owners[&data.owner].origin,
                    RecipeOrigin::Borrow { .. }
                ) {
                    return Ok(false);
                }
            }
            VersionCause::Parameter => return Ok(false),
            VersionCause::CachePhi { incoming, .. } => {
                if incoming.is_empty() {
                    return Ok(false);
                }
                pending.extend(incoming.iter().map(|edge| edge.version));
            }
        }
    }
    Ok(true)
}

/// Append a real physical cache phi for a numeric owner dominating a join.
/// Incoming versions/actual edge args are filled only after all predecessor
/// bodies are transformed. Other fields keep their dominating original IDs.
pub(crate) fn create_cache_phi(
    func: &mut Func,
    table: &mut SinkRecipes,
    owner: Value,
    block: Block,
    stats: &mut SinkStats,
) -> Result<RecipeVersionId, PassError> {
    let previous = table.owners[&owner].definition_version;
    let RecipeFields::Number(fields) = table.versions[previous.0 as usize].fields else {
        return Err(VerifyError::TypeMismatch(owner).into());
    };
    let value = Value(func.values.len() as u32);
    let index = func.blocks[block.index()].params.len() as u32;
    let ty = table.owners[&owner].semantic_type.join(TypeSet::NIL);
    func.values.push(ValueData {
        ty,
        rep: Rep::Tagged,
        def: ValueDef::Param { block, index },
    });
    func.blocks[block.index()].params.push(value);
    stats.cache_phis += 1;
    Ok(append_version(
        table,
        owner,
        RecipeFields::Number(NumericFields {
            real_box: value,
            ..fields
        }),
        VersionCause::CachePhi {
            block,
            box_param: value,
            incoming: Box::new([]),
        },
    ))
}

/// Fill every edge occurrence, in parameter append order. Parallel true/false
/// edges to the same target remain separate entries and transport separate boxes.
pub(crate) fn fill_cache_phi(
    func: &mut Func,
    table: &mut SinkRecipes,
    version: RecipeVersionId,
    incoming: &[(Block, usize, RecipeVersionId)],
) -> Result<(), PassError> {
    let VersionCause::CachePhi {
        block, box_param, ..
    } = table.versions[version.0 as usize].cause.clone()
    else {
        return Err(VerifyError::BadDefinition(table.versions[version.0 as usize].owner).into());
    };
    let ValueDef::Param { index, .. } = func.values[box_param.index()].def else {
        return Err(VerifyError::BadDefinition(box_param).into());
    };
    let owner = table.versions[version.0 as usize].owner;
    let mut entries = Vec::new();
    for &(source, edge_index, incoming_version) in incoming {
        let data = &table.versions[incoming_version.0 as usize];
        if data.owner != owner {
            return Err(VerifyError::BadDefinition(owner).into());
        }
        let value = box_field(data.fields);
        let edge = edge_mut(&mut func.blocks[source.index()].term, edge_index)
            .ok_or(VerifyError::InvalidBlock(source))?;
        if edge.target != block || edge.args.len() != index as usize {
            return Err(VerifyError::EdgeArity(source, block).into());
        }
        edge.args.push(value);
        entries.push(CachePhiEdge {
            source,
            edge_index: edge_index as u32,
            version: incoming_version,
        });
    }
    table.versions[version.0 as usize].cause = VersionCause::CachePhi {
        block,
        box_param,
        incoming: entries.into(),
    };
    Ok(())
}
