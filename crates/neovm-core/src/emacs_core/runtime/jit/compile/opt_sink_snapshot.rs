//! Exact-point cold frames for independently verified sink recipes.
//! Threading: immutable recipe capabilities and SSA names belong to one
//! compilation. No runtime objects, payload-keyed identities or caches live here.
use super::super::sink_cold_snapshot::{ColdField, SinkColdNode, SinkColdSlots, SinkColdSnapshot};
use super::*;
use crate::emacs_core::jit::opt::sink_recipes::RecipeVersionId;

fn failure() -> CompileError {
    CompileError::UnsupportedOp("opt-sink:point-frame")
}

pub(super) fn number(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    version: RecipeVersionId,
    fields: sink_recipes::NumericFields,
) -> numeric_carrier::NumericCarrier {
    numeric_carrier::NumericCarrier {
        payload: ctx.values.read(ctx.fb, ctx.func, local, fields.payload).0,
        word: ctx.values.read(ctx.fb, ctx.func, local, fields.word).0,
        ready: ctx.values.read(ctx.fb, ctx.func, local, fields.ready).0,
        real_box: ctx.values.read(ctx.fb, ctx.func, local, fields.real_box).0,
        facts: ctx.numeric_facts.at(version),
    }
}

pub(super) fn current_number(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    owner: ir::Value,
) -> Result<numeric_carrier::NumericCarrier, CompileError> {
    let proof = ctx.verified_sink.ok_or_else(failure)?;
    let owner = canonical(ctx.func, owner);
    let version = proof.version_at(ctx.point, owner).ok_or_else(failure)?;
    let RecipeFields::Number(fields) = proof.version(version).ok_or_else(failure)?.fields else {
        return Err(failure());
    };
    Ok(number(ctx, local, version, fields))
}

/// Ordinary operations receive only actual Tagged SSA operands. Logical
/// children of a Cons materializer must already have an independently proved
/// boxed successor version; this never boxes from a mutable backend cache.
pub(super) fn semantic_tagged(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    value: ir::Value,
) -> Result<ClifValue, CompileError> {
    let value = canonical(ctx.func, value);
    if !ctx.values.logical[value.index()] {
        return Ok(tagged(ctx, local, value));
    }
    let proof = ctx.verified_sink.ok_or_else(failure)?;
    let version = proof.version_at(ctx.point, value).ok_or_else(failure)?;
    if !proof.guaranteed_boxed(version) {
        return Err(CompileError::UnsupportedOp(
            "opt-sink:unboxed-ordinary-operand",
        ));
    }
    let field = match proof.version(version).ok_or_else(failure)?.fields {
        RecipeFields::Number(fields) => fields.real_box,
        RecipeFields::Cons(fields) => fields.real_box,
    };
    Ok(ctx.values.read(ctx.fb, ctx.func, local, field).0)
}

fn capture_field(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    value: ir::Value,
    version: Option<RecipeVersionId>,
    nodes: &mut Vec<SinkColdNode>,
    identities: &mut HashMap<ir::Value, usize>,
    visiting: &mut HashSet<ir::Value>,
) -> Result<ColdField, CompileError> {
    let value = canonical(ctx.func, value);
    if !ctx.values.logical[value.index()] {
        let runtime = ctx.values.read(ctx.fb, ctx.func, local, value);
        if runtime.1.is_flonum() {
            return Err(CompileError::UnsupportedOp("opt-sink:cold-legacy-float"));
        }
        return Ok(ColdField::Runtime(runtime));
    }
    let proof = ctx.verified_sink.ok_or_else(failure)?;
    let version = version
        .or_else(|| proof.version_at(ctx.point, value))
        .ok_or_else(failure)?;
    let fields = proof.version(version).ok_or_else(failure)?.fields;
    if proof.guaranteed_boxed(version) {
        let boxed = match fields {
            RecipeFields::Number(f) => f.real_box,
            RecipeFields::Cons(f) => f.real_box,
        };
        return Ok(ColdField::Runtime(
            ctx.values.read(ctx.fb, ctx.func, local, boxed),
        ));
    }
    let identity = proof
        .canonical_identity_at(ctx.point, value)
        .ok_or_else(failure)?;
    if let Some(&node) = identities.get(&identity) {
        return Ok(ColdField::Node(node));
    }
    if !visiting.insert(identity) {
        return Err(CompileError::UnsupportedOp("opt-sink:cold-cycle"));
    }
    let node = match fields {
        RecipeFields::Number(fields) => SinkColdNode::Number(number(ctx, local, version, fields)),
        RecipeFields::Cons(fields) => {
            let car = capture_field(ctx, local, fields.car, None, nodes, identities, visiting)?;
            let cdr = capture_field(ctx, local, fields.cdr, None, nodes, identities, visiting)?;
            let real_box = ctx.values.read(ctx.fb, ctx.func, local, fields.real_box).0;
            SinkColdNode::Cons { car, cdr, real_box }
        }
    };
    visiting.remove(&identity);
    let index = nodes.len();
    nodes.push(node);
    identities.insert(identity, index);
    Ok(ColdField::Node(index))
}

pub(super) fn capture(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    frame: ir::FrameId,
) -> Result<Snapshot, CompileError> {
    let proof = ctx.verified_sink.ok_or_else(failure)?;
    let frame_view = proof.frame_view(ctx.point, frame).ok_or_else(failure)?;
    let versions: HashMap<_, _> = frame_view.versions.iter().copied().collect();
    let values = ctx.func.frames[frame.index()].stack.to_vec();
    let mut stack = Vec::with_capacity(values.len());
    let mut reps = Vec::with_capacity(values.len());
    let mut nodes = Vec::new();
    let mut identities = HashMap::new();
    let mut visiting = HashSet::new();
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for (slot, value) in values.into_iter().enumerate() {
        let value = canonical(ctx.func, value);
        match capture_field(
            ctx,
            local,
            value,
            versions.get(&value).copied(),
            &mut nodes,
            &mut identities,
            &mut visiting,
        )? {
            ColdField::Runtime((word, rep)) => {
                stack.push(word);
                reps.push(rep);
            }
            ColdField::Node(node) => {
                stack.push(ctx.fb.ins().iconst(types::I64, Value::NIL.bits() as i64));
                reps.push(SlotRep::Tagged);
                groups.entry(node).or_default().push(slot);
            }
        }
    }
    let cold = if nodes.is_empty() {
        None
    } else {
        let mut slots: Vec<_> = groups
            .into_iter()
            .map(|(node, slots)| SinkColdSlots {
                node,
                slots: slots.into_boxed_slice(),
            })
            .collect();
        slots.sort_unstable_by_key(|group| group.node);
        Some(SinkColdSnapshot::from_verified_point(
            ctx.fb, &stack, &reps, nodes, slots,
        )?)
    };
    Ok((stack, reps, cold))
}
