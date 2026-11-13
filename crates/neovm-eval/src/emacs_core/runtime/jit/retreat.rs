//! Per-site retreat state: what deopts taught a source about one bytecode
//! pc, kept across its recompiles (P2.0 §3.4).
//!
//! One 16-bit word per pc of a source, allocated the first time a deopt
//! marks any of its sites ([`SiteRetreatTable`]): the low byte is a set of
//! [`RetreatBit`]s, the high byte a saturating deopt count. It is the one
//! table the speculating tiers share: P0.4's no-inline call sites live here
//! today, and P2.1's deopt history, hoisting and block-pruning retreats and
//! P2.2's float-only retreat are bits reserved in it.
//!
//! Relaxed atomics throughout: the bits only ever turn on, the count only
//! grows, and a compile that misses a concurrent mark sees it on the next
//! one (the same contract as the numeric feedback beside it).

pub(crate) use neovm_value::value::jit_runtime::retreat::{
    RetreatBit, SiteRetreat, SiteRetreatTable,
};
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

#[cfg(test)]
#[path = "retreat/tests/retreat_test.rs"]
mod tests;
