//! Per-site retreat data (T1-pre).
//!
//! Moved definitions from `emacs_core::jit::retreat`; the table logic and the
//! upper callers stay there. The data is 16-bit atomics, no Lisp state.

/// One retreat a deopt can force at a pc. The value is the bit in
/// [`SiteRetreat`]'s low byte.
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU16, Ordering};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
#[doc(hidden)]
pub enum RetreatBit {
    /// Do not splice, MIR-inline or intrinsify the call at this pc (P0.4).
    NoInline = 1 << 0,
    /// Do not hoist this site's guards above it (P2.1 entry guards, P2.2
    /// LICM).
    NoHoist = 1 << 1,
    /// Do not speculate float-only at this arithmetic site (P2.2).
    NoFloatOnly = 1 << 2,
    /// Compile the block at this leader even if the profile never reached
    /// it (P2.1's uncommon-trap pruning).
    KeepBlock = 1 << 3,
    /// A deopt here invalidated a leaf: never speculate here again (P2.1).
    Invalidated = 1 << 4,
}

/// The retreat word of one pc: [`RetreatBit`]s in the low byte, a
/// saturating deopt count in the high byte.
#[derive(Default)]
#[doc(hidden)]
pub struct SiteRetreat(pub(crate) AtomicU16);

/// One source's retreat words, one per pc, allocated on the first mark.
#[derive(Default, Debug)]
#[doc(hidden)]
pub struct SiteRetreatTable(pub(crate) OnceLock<Box<[SiteRetreat]>>);

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
    pub(crate) const BITS_MASK: u16 = 0x00ff;
    pub(crate) const COUNT_SHIFT: u32 = 8;

    /// Turn `bit` on.
    #[doc(hidden)]
    pub fn set(&self, bit: RetreatBit) {
        self.0.fetch_or(bit as u16, Ordering::Relaxed);
    }

    /// Whether `bit` is on.
    #[inline]
    pub(crate) fn has(&self, bit: RetreatBit) -> bool {
        self.0.load(Ordering::Relaxed) & bit as u16 != 0
    }

    /// Deopts counted at this pc: the high byte, saturating at 255. Written
    /// by [`Self::note_deopt`] (P2.1's deopt history).
    #[doc(hidden)]
    pub fn count(&self) -> u8 {
        (self.0.load(Ordering::Relaxed) >> Self::COUNT_SHIFT) as u8
    }

    /// Count one precise deopt at this pc (saturating) and return the new
    /// count. The history outlives every leaf of the source: a leaf's own
    /// `LeafObs` counts go with the leaf when it is replaced.
    #[doc(hidden)]
    pub fn note_deopt(&self) -> u8 {
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
    #[doc(hidden)]
    pub fn bits(&self) -> impl Iterator<Item = RetreatBit> {
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
    #[doc(hidden)]
    pub fn site(&self, pc: usize, ops_len: usize) -> Option<&SiteRetreat> {
        self.0
            .get_or_init(|| (0..ops_len).map(|_| SiteRetreat::default()).collect())
            .get(pc)
    }

    /// The word of `pc` if the table exists: a read never allocates.
    #[inline]
    #[doc(hidden)]
    pub fn get(&self, pc: usize) -> Option<&SiteRetreat> {
        self.0.get().and_then(|sites| sites.get(pc))
    }

    /// Whether `bit` is on at `pc` (false before any mark).
    #[inline]
    #[doc(hidden)]
    pub fn has(&self, pc: usize, bit: RetreatBit) -> bool {
        self.get(pc).is_some_and(|site| site.has(bit))
    }
}
