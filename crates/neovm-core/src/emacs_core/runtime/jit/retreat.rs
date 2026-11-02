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

pub(crate) use crate::emacs_core::value::jit_runtime::retreat::{
    RetreatBit, SiteRetreat, SiteRetreatTable,
};
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

impl RetreatBit {
    /// Every bit, in bit order.
    pub(crate) const ALL: [RetreatBit; 5] = [
        RetreatBit::NoInline,
        RetreatBit::NoHoist,
        RetreatBit::NoFloatOnly,
        RetreatBit::KeepBlock,
        RetreatBit::Invalidated,
    ];

    /// The bit's name in traces.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            RetreatBit::NoInline => "no_inline",
            RetreatBit::NoHoist => "no_hoist",
            RetreatBit::NoFloatOnly => "no_float_only",
            RetreatBit::KeepBlock => "keep_block",
            RetreatBit::Invalidated => "invalidated",
        }
    }
}

impl SiteRetreat {
    const BITS_MASK: u16 = 0x00ff;
    const COUNT_SHIFT: u32 = 8;

    /// Turn `bit` on.
    pub(crate) fn set(&self, bit: RetreatBit) {
        self.0.fetch_or(bit as u16, Ordering::Relaxed);
    }

    /// Whether `bit` is on.
    #[inline]
    pub(crate) fn has(&self, bit: RetreatBit) -> bool {
        self.0.load(Ordering::Relaxed) & bit as u16 != 0
    }

    /// Deopts counted at this pc: the high byte, saturating at 255. Written
    /// by [`Self::note_deopt`] (P2.1's deopt history).
    pub(crate) fn count(&self) -> u8 {
        (self.0.load(Ordering::Relaxed) >> Self::COUNT_SHIFT) as u8
    }

    /// Count one precise deopt at this pc (saturating) and return the new
    /// count. The history outlives every leaf of the source: a leaf's own
    /// `LeafObs` counts go with the leaf when it is replaced.
    pub(crate) fn note_deopt(&self) -> u8 {
        let word = self.0.load(Ordering::Relaxed);
        let count = (word >> Self::COUNT_SHIFT) as u8;
        if count == u8::MAX {
            return count;
        }
        // The mutator is the only writer; `set` ORs bits in with a
        // read-modify-write, which this add of the high byte cannot lose.
        self.0.fetch_add(1 << Self::COUNT_SHIFT, Ordering::Relaxed);
        count + 1
    }

    /// The bits that are on, in [`RetreatBit::ALL`] order.
    pub(crate) fn bits(&self) -> impl Iterator<Item = RetreatBit> {
        let word = self.0.load(Ordering::Relaxed) & Self::BITS_MASK;
        RetreatBit::ALL
            .into_iter()
            .filter(move |bit| word & *bit as u16 != 0)
    }
}

impl std::fmt::Debug for SiteRetreat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SiteRetreat")
            .field(
                "bits",
                &self.bits().map(RetreatBit::name).collect::<Vec<_>>(),
            )
            .field("count", &self.count())
            .finish()
    }
}

impl SiteRetreatTable {
    /// An empty table (nothing allocated).
    pub(crate) const fn new() -> Self {
        SiteRetreatTable(OnceLock::new())
    }

    /// The word of `pc`, allocating the table for a body of `ops_len` ops on
    /// first use. `None` when `pc` is outside the body the table was first
    /// sized for.
    pub(crate) fn site(&self, pc: usize, ops_len: usize) -> Option<&SiteRetreat> {
        self.0
            .get_or_init(|| (0..ops_len).map(|_| SiteRetreat::default()).collect())
            .get(pc)
    }

    /// The word of `pc` if the table exists: a read never allocates.
    #[inline]
    pub(crate) fn get(&self, pc: usize) -> Option<&SiteRetreat> {
        self.0.get().and_then(|sites| sites.get(pc))
    }

    /// Whether `bit` is on at `pc` (false before any mark).
    #[inline]
    pub(crate) fn has(&self, pc: usize, bit: RetreatBit) -> bool {
        self.get(pc).is_some_and(|site| site.has(bit))
    }
}

#[cfg(test)]
#[path = "retreat/tests/retreat_test.rs"]
mod tests;
