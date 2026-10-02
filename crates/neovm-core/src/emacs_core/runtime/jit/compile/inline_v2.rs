//! P2.3's annotated static-callee fuser front.
//!
//! Enabled v2 sites use the bounded, forward-only chain admission; closure/all
//! modes propagate in-unit make-closure provenance across agreeing CFG edges. Side
//! tables describe entry, return expansion, source pc and closure captures
//! without adding an opcode.
//! Threading: these tables are immutable compile-local Rust data; callers
//! belong to the compiling mutator, whose fused body's constant pool roots
//! every template. No mutable Lisp state is cached or shared across mutators.

use std::collections::BTreeMap;

use super::FusedBody;
use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::value::Value;

#[path = "inline_v2_tags.rs"]
mod tags;

/// How a flat static region obtains its executing callee and constants.
/// Immutable compile-local metadata, never shared mutable Lisp state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegionKind {
    Constant,
    Closure { prefix: usize, const_base: usize },
}

pub(crate) use crate::emacs_core::jit::vframe::HofKind;

/// One mapc/mapcar call with an admitted callback. The `Call(2)` stays in the
/// op vector; lowering emits its internal list loop from this annotation.
/// Threading: immutable compile-local data. `callback` is rooted by the owning
/// fused body's constant pool, and no closure captures are cached here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HofSite {
    pub(crate) callback: Value,
    /// In-unit make-closure provenance requires a source-identity guard;
    /// a constant callback requires the exact object's bits instead.
    pub(crate) closure: bool,
    pub(crate) prefix: usize,
    pub(crate) kind: HofKind,
    pub(crate) call_site_pc: usize,
    /// Appended callback constants use this base in the fused pool.
    pub(crate) const_base: usize,
}

/// Profitability's call credit uses the exact admission table lowering uses.
/// The caller scopes its fused body around the gate; legacy/off bodies grant
/// no credit. This reads compile-local immutable metadata, never Lisp state.
pub(crate) fn hof_profit_credit_at(pc: usize) -> bool {
    super::active_fused().is_some_and(|body| body.admitted_hof_at(pc).is_some())
}

/// Immutable program-counter set after the front has finished building it.
/// No mutator runtime data or synchronization is needed for readers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PcSet {
    words: Box<[u64]>,
}

impl PcSet {
    fn new(len: usize) -> Self {
        Self {
            words: vec![0; len.div_ceil(64)].into_boxed_slice(),
        }
    }

    fn insert(&mut self, pc: usize) {
        self.words[pc / 64] |= 1 << (pc % 64);
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "front marker verification uses this in tests")
    )]
    pub(crate) fn contains(&self, pc: usize) -> bool {
        self.words
            .get(pc / 64)
            .is_some_and(|word| word & (1 << (pc % 64)) != 0)
    }
}

/// Immutable annotations of a fused-v2 op vector. Current observation-free
/// callback bodies have no materialization points; later producers must
/// classify observing ops before filling this table. Threading: annotations
/// belong to one compilation and contain no shared mutable mutator state.
#[derive(Clone, Debug)]
pub(crate) struct FusedV2 {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "emission currently uses InlineRegion.start")
    )]
    pub(crate) entry_at: PcSet,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "return expansions remain ordinary fused ops")
    )]
    pub(crate) exit_at: PcSet,
    pub(crate) callee_pc_of_fused: Box<[u32]>,
    /// One kind per `FusedBody::regions` entry, in the same order.
    pub(crate) region_kind: Box<[RegionKind]>,
    /// Admitted HOF sites, keyed by fused `Call(2)` pc.
    pub(crate) hof_at: BTreeMap<usize, HofSite>,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "materialized frames arrive in later P2.3 stages")
    )]
    pub(crate) materialize_at: BTreeMap<usize, Box<[usize]>>,
}

/// Splice admitted constant and in-unit closure sites, then describe their
/// source instructions and return expansions for chain framestate emission.
pub(crate) fn fuse_calls_v2(
    ops: &[Op],
    constants: &[Value],
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    arity: usize,
    feedback: &[NumericFeedback],
) -> Option<FusedBody> {
    let front = tags::fuse_static(ops, constants, offset_map, arity, feedback)?;
    let mut body = front.body;
    let mut entry_at = PcSet::new(body.ops.len());
    let mut exit_at = PcSet::new(body.ops.len());
    let mut callee_pc_of_fused: Vec<u32> = body
        .caller_of_fused
        .iter()
        .map(|&pc| u32::try_from(pc))
        .collect::<Result<_, _>>()
        .ok()?;
    for region in &body.regions {
        entry_at.insert(region.start);
        let callee = Value::from_bits(region.callee_bits as usize).get_bytecode_data()?;
        let mut cursor = region.start;
        for (callee_pc, op) in callee.executable_ops().iter().enumerate() {
            let pc = u32::try_from(callee_pc).ok()?;
            if matches!(op, Op::Return) {
                // Mark the first discard, before the return expansion
                // consumes the callee's stack and function slot.
                exit_at.insert(cursor);
                while matches!(body.ops.get(cursor), Some(Op::DiscardN(raw)) if raw & 0x80 != 0) {
                    *callee_pc_of_fused.get_mut(cursor)? = pc;
                    cursor += 1;
                }
                if body.ops.get(cursor) != Some(&Op::Goto(region.end as u32)) {
                    return None;
                }
            }
            *callee_pc_of_fused.get_mut(cursor)? = pc;
            cursor += 1;
        }
        if cursor != region.end {
            return None;
        }
    }
    body.v2 = Some(FusedV2 {
        entry_at,
        exit_at,
        callee_pc_of_fused: callee_pc_of_fused.into_boxed_slice(),
        region_kind: front.region_kind,
        hof_at: front.hof_at,
        materialize_at: BTreeMap::new(),
    });
    Some(body)
}
