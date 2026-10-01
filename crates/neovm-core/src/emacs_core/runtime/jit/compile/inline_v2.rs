//! P2.3's annotated constant-callee fuser front.
//!
//! The transform reuses the legacy fuser's exact admission and ops. Side
//! tables describe entry, return expansion, source pc and nesting without
//! adding an opcode. All current regions remain replay-deopt regions.
//! Threading: these tables are immutable compile-local Rust data; callers
//! may share read-only tables between compiler workers. They hold no new
//! Lisp values beyond those already owned by `FusedBody`.

use std::collections::BTreeMap;

use super::{FusedBody, fuse_calls};
use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::value::Value;

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
        expect(dead_code, reason = "entry/exit emission arrives in later P2.3 stages")
    )]
    pub(crate) fn contains(&self, pc: usize) -> bool {
        self.words
            .get(pc / 64)
            .is_some_and(|word| word & (1 << (pc % 64)) != 0)
    }
}

/// Immutable annotations of a fused-v2 op vector. Current allocation-free
/// constant bodies have no materialization points; later producers must
/// classify observing ops before filling this table.
#[derive(Clone, Debug)]
pub(crate) struct FusedV2 {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "entry emission arrives in later P2.3 stages")
    )]
    pub(crate) entry_at: PcSet,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "exit emission arrives in later P2.3 stages")
    )]
    pub(crate) exit_at: PcSet,
    pub(crate) callee_pc_of_fused: Box<[u32]>,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "materialized frames arrive in later P2.3 stages")
    )]
    pub(crate) materialize_at: BTreeMap<usize, Box<[usize]>>,
}

/// The v2 front uses the existing fuser verbatim and then annotates the
/// resulting vector. No admission or fusion semantics change here.
pub(crate) fn fuse_calls_v2(
    ops: &[Op],
    constants: &[Value],
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    arity: usize,
    feedback: &[NumericFeedback],
) -> Option<FusedBody> {
    let mut body = fuse_calls(ops, constants, offset_map, arity, feedback)?;
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
        materialize_at: BTreeMap::new(),
    });
    Some(body)
}
