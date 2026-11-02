//! Per-site retreat data (T1-pre).
//!
//! Moved definitions from `emacs_core::jit::retreat`; the table logic and the
//! upper callers stay there. The data is 16-bit atomics, no Lisp state.

use std::sync::OnceLock;
use std::sync::atomic::AtomicU16;

/// One retreat a deopt can force at a pc. The value is the bit in
/// [`SiteRetreat`]'s low byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum RetreatBit {
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
pub(crate) struct SiteRetreat(pub(crate) AtomicU16);

/// One source's retreat words, one per pc, allocated on the first mark.
#[derive(Default, Debug)]
pub(crate) struct SiteRetreatTable(pub(crate) OnceLock<Box<[SiteRetreat]>>);
