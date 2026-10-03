//! Frozen compiler-local cold reconstruction for verified selected sink recipes.
//! Register alongside the compiler-level numeric_carrier child. This owns
//! frozen compiler-local CLIF names for ONE original frame at ONE point, not
//! runtime/Lisp/TLS cache state. Root's VerifiedSinkRecipes capability must
//! establish semantic identity, exact versions, cache kinds and root liveness
//! BEFORE constructing this snapshot. The local constructor checks physical
//! shape and graph structure only; it is not an independent recipe verifier.

use super::numeric_carrier::{self, NumericCarrier};
use super::*;

type ColdRuntimeValue = (ClifValue, SlotRep);

/// An already validated ordinary semantic operand, or one earlier logical
/// reconstruction identity. Threading: compilation-local immutable metadata.
/// Legacy Flonum reps are intentionally refused; their semantic identity must
/// become a Number node or be materialized before capture, never grouped by
/// F64 payload. RawFixnum and Bool use only their shared tagged conversions.
#[derive(Clone, Copy, Debug)]
pub(super) enum ColdField {
    Runtime(ColdRuntimeValue),
    Node(usize),
}

/// One logical equivalence identity at the frozen source point. Threading:
/// immutable compilation-owned names; no object address is dereferenced here.
/// Number carries exact payload/word/ready/box fields. Cons cache must be
/// proven exact same-identity CONS|NIL; mere Tagged type is insufficient.
/// A version guaranteed boxed is an ordinary Tagged field, never this node
/// (especially after real mutation of a previously virtual Cons).
#[derive(Clone, Copy, Debug)]
pub(super) enum SinkColdNode {
    Number(NumericCarrier),
    Cons {
        car: ColdField,
        cdr: ColdField,
        real_box: ClifValue,
    },
}

/// All original frame slots of this one independently proved equivalence
/// identity. Distinct producer identities never share a group by equal bits,
/// payload, fields, allocation-site number or machine SSA names.
#[derive(Clone, Debug)]
pub(super) struct SinkColdSlots {
    pub(super) node: usize,
    pub(super) slots: Box<[usize]>,
}

/// Frozen point/frame snapshot. Threading: one compiler invocation owns this;
/// clones retain the exact captured tuple IDs, never a mutable latest version.
/// Every dependency has a smaller node index; original virtual frame slots
/// contain Tagged literal NIL placeholders until reconstruction.
#[derive(Clone, Debug)]
pub(super) struct SinkColdSnapshot {
    stack: Box<[ColdRuntimeValue]>,
    nodes: Box<[SinkColdNode]>,
    slots: Box<[SinkColdSlots]>,
}

fn check_runtime(fb: &FunctionBuilder, (word, rep): ColdRuntimeValue) -> bool {
    let want = match rep {
        SlotRep::Tagged | SlotRep::RawFixnum => types::I64,
        SlotRep::Bool => types::I8,
        SlotRep::Flonum { .. } => return false,
    };
    fb.func.dfg.value_type(word) == want
}

fn check_number(fb: &FunctionBuilder, value: NumericCarrier) -> bool {
    fb.func.dfg.value_type(value.payload) == types::F64
        && fb.func.dfg.value_type(value.word) == types::I64
        && fb.func.dfg.value_type(value.ready) == types::I8
        && fb.func.dfg.value_type(value.real_box) == types::I64
}

fn check_field(fb: &FunctionBuilder, field: ColdField, owner: usize) -> bool {
    match field {
        ColdField::Runtime(value) => check_runtime(fb, value),
        ColdField::Node(child) => child < owner,
    }
}

impl SinkColdSnapshot {
    /// Prevent an integration from pairing frozen tuples with a different
    /// pending frame after snapshot override. Semantic point/version proof is
    /// still supplied by root; this compares the exact captured machine view.
    pub(super) fn matches_frame(&self, stack: &[ClifValue], reps: &[SlotRep]) -> bool {
        stack.len() == reps.len()
            && stack.len() == self.stack.len()
            && self
                .stack
                .iter()
                .copied()
                .eq(stack.iter().copied().zip(reps.iter().copied()))
    }

    /// Build ONLY after root has validated actual recipes at the exact source
    /// cut/frame, including cache semantics and dominance. This bounded linear
    /// scan adds machine-type, DAG, placeholder and exact slot checks. It does
    /// not manufacture semantic proof from generic box words or copied fields.
    pub(super) fn from_verified_point(
        fb: &FunctionBuilder,
        stack: &[ClifValue],
        reps: &[SlotRep],
        nodes: Vec<SinkColdNode>,
        slots: Vec<SinkColdSlots>,
    ) -> Result<Self, CompileError> {
        if stack.len() != reps.len() || nodes.is_empty() || slots.is_empty() {
            return Err(CompileError::UnsupportedOp("opt-sink:cold-shape"));
        }
        for (&word, &rep) in stack.iter().zip(reps) {
            if !check_runtime(fb, (word, rep)) {
                return Err(CompileError::UnsupportedOp("opt-sink:cold-ordinary-rep"));
            }
        }
        for (index, node) in nodes.iter().enumerate() {
            let valid = match *node {
                SinkColdNode::Number(value) => check_number(fb, value),
                SinkColdNode::Cons { car, cdr, real_box } => {
                    fb.func.dfg.value_type(real_box) == types::I64
                        && check_field(fb, car, index)
                        && check_field(fb, cdr, index)
                }
            };
            if !valid {
                return Err(CompileError::UnsupportedOp("opt-sink:cold-node-shape"));
            }
        }
        let mut used_slots = vec![false; stack.len()];
        let mut group_nodes = vec![false; nodes.len()];
        let mut reachable = vec![false; nodes.len()];
        for group in &slots {
            if group.node >= nodes.len() || group.slots.is_empty() || group_nodes[group.node] {
                return Err(CompileError::UnsupportedOp("opt-sink:cold-slot-owner"));
            }
            group_nodes[group.node] = true;
            reachable[group.node] = true;
            for &slot in &group.slots {
                if slot >= stack.len()
                    || used_slots[slot]
                    || reps[slot] != SlotRep::Tagged
                    || lowering::iconst_bits(fb, stack[slot]) != Some(Value::NIL.bits() as i64)
                {
                    return Err(CompileError::UnsupportedOp(
                        "opt-sink:cold-slot-placeholder",
                    ));
                }
                used_slots[slot] = true;
            }
        }
        // Reverse topological propagation visits every edge once, without
        // recursive traversal or cloning/revalidating the entire recipe Func.
        for index in (0..nodes.len()).rev() {
            if reachable[index] {
                if let SinkColdNode::Cons { car, cdr, .. } = nodes[index] {
                    for field in [car, cdr] {
                        if let ColdField::Node(child) = field {
                            reachable[child] = true;
                        }
                    }
                }
            }
        }
        if reachable.iter().any(|&live| !live) {
            return Err(CompileError::UnsupportedOp("opt-sink:cold-unused-node"));
        }
        Ok(Self {
            stack: stack.iter().copied().zip(reps.iter().copied()).collect(),
            nodes: nodes.into_boxed_slice(),
            slots: slots.into_boxed_slice(),
        })
    }
}

fn tagged_runtime(fb: &mut FunctionBuilder, (word, rep): ColdRuntimeValue) -> ClifValue {
    match rep {
        SlotRep::Tagged => word,
        SlotRep::RawFixnum => retag_fixnum(fb, word),
        SlotRep::Bool => super::boolean::tagged_bool_view(fb, word),
        SlotRep::Flonum { .. } => unreachable!("validated cold fields exclude legacy flonums"),
    }
}

fn tagged_field(fb: &mut FunctionBuilder, values: &[ClifValue], field: ColdField) -> ClifValue {
    match field {
        ColdField::Runtime(value) => tagged_runtime(fb, value),
        ColdField::Node(child) => values[child],
    }
}

/// Reconstruct the entire selected original GNU frame BEFORE its first spill.
/// Every logical node has one emitted materializer, shared by all stack/field
/// aliases. Existing boxed objects retain exact identity. No GC, Lisp call,
/// Poll, callback or readback is inserted between allocations and the caller's
/// complete Tagged spill. Root's runtime must preserve the existing no-GC
/// return/readback interval; this helper changes no runtime ownership rule.
///
/// Dynamic Cons caches demand children only on the NIL/unboxed arm. This is
/// not permission to accept unsupported maybe-boxed recipes: root's verifier
/// AND roots-at-point must prove arm-valid fields/conditional roots, otherwise
/// decline those recipes. Guaranteed-boxed Cons captures no old fields at all.
pub(super) fn emit_cold_snapshot(
    fb: &mut FunctionBuilder,
    refs: &RtRefs,
    snapshot: &SinkColdSnapshot,
) -> Result<Vec<ClifValue>, CompileError> {
    let zero = fb.ins().iconst(types::I8, 0);
    let one = fb.ins().iconst(types::I8, 1);
    let mut needed = vec![zero; snapshot.nodes.len()];
    // Validated snapshot slots require these identities on EVERY cold exit.
    // This flag records demand only; it proves no readiness or cache state.
    let mut direct = vec![false; snapshot.nodes.len()];
    for group in &snapshot.slots {
        needed[group.node] = one;
        direct[group.node] = true;
    }
    // Metadata is child-before-parent. Demand flows parent-to-child BEFORE
    // allocation: no old child is materialized merely because a cached Cons
    // once referenced it. Tests here inspect only scalar cache words, no heap.
    for index in (0..snapshot.nodes.len()).rev() {
        if let SinkColdNode::Cons { car, cdr, real_box } = snapshot.nodes[index] {
            let empty = lowering::icmp_imm_p(fb, IntCC::Equal, real_box, Value::NIL.bits() as i64);
            let needs_children = fb.ins().band(needed[index], empty);
            for field in [car, cdr] {
                if let ColdField::Node(child) = field {
                    if !direct[child] {
                        needed[child] = fb.ins().bor(needed[child], needs_children);
                    }
                }
            }
        }
    }
    let mut values = Vec::with_capacity(snapshot.nodes.len());
    for (index, node) in snapshot.nodes.iter().enumerate() {
        if direct[index] {
            if let SinkColdNode::Number(value) = *node {
                let value = numeric_carrier::materialize(fb, refs, None, value, true)?;
                values.push(value.tagged);
                continue;
            }
        }
        let result = fb.declare_var(types::I64);
        let merge = if direct[index] {
            // A direct Cons still needs its cached/build merge. Its original
            // cache test and conditional child demand remain unchanged.
            let merge = fb.create_block();
            fb.set_cold_block(merge);
            merge
        } else {
            // Child-only nodes retain their complete original demand path.
            let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
            fb.def_var(result, nil);
            let work = fb.create_block();
            let merge = fb.create_block();
            fb.set_cold_block(work);
            fb.set_cold_block(merge);
            fb.ins().brif(needed[index], work, &[], merge, &[]);
            fb.switch_to_block(work);
            fb.seal_block(work);
            merge
        };
        match *node {
            SinkColdNode::Number(value) => {
                let value = numeric_carrier::materialize(fb, refs, None, value, true)?;
                fb.def_var(result, value.tagged);
                fb.ins().jump(merge, &[]);
            }
            SinkColdNode::Cons { car, cdr, real_box } => {
                let cached = fb.create_block();
                let build = fb.create_block();
                fb.set_cold_block(cached);
                fb.set_cold_block(build);
                let has_box =
                    lowering::icmp_imm_p(fb, IntCC::NotEqual, real_box, Value::NIL.bits() as i64);
                fb.ins().brif(has_box, cached, &[], build, &[]);
                fb.switch_to_block(cached);
                fb.seal_block(cached);
                fb.def_var(result, real_box);
                fb.ins().jump(merge, &[]);
                fb.switch_to_block(build);
                fb.seal_block(build);
                let car = tagged_field(fb, &values, car);
                let cdr = tagged_field(fb, &values, cdr);
                let allocator = refs.get(fb.func, Shim::Cons);
                let call = fb.ins().call(allocator, &[car, cdr]);
                let value = fb.inst_results(call)[0];
                fb.def_var(result, value);
                fb.ins().jump(merge, &[]);
            }
        }
        fb.switch_to_block(merge);
        fb.seal_block(merge);
        values.push(fb.use_var(result));
    }
    let mut stack: Vec<_> = snapshot
        .stack
        .iter()
        .copied()
        .map(|value| tagged_runtime(fb, value))
        .collect();
    for group in &snapshot.slots {
        let value = values[group.node];
        for &slot in &group.slots {
            stack[slot] = value;
        }
    }
    // New Number/Cons identity caches are the `values` vector indexed by
    // independently proved owner equivalence, never the legacy payload map.
    // No tagged frame store has occurred yet: the caller now spills this whole
    // stack, stores existing metadata and returns STATUS_DEOPT_AT unchanged.
    Ok(stack)
}
