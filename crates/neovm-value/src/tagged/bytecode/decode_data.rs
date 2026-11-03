//! Decoded-stack proof storage data (T1 storage cut).

/// A successful operand-stack proof.
///
/// `depths` is kept only when the stream has a `Switch` (the only instruction
/// whose targets can change after the proof, because a jump table is a live
/// Lisp object): the proven operand depth at every instruction,
/// [`StackProof::UNREACHABLE`] where none exists. The VERIFIED driver takes a
/// switch hit exactly when the resolved instruction's proven depth equals the
/// fixed switch depth: all reachable switches must have the same post-pop
/// depth. The proof is per instruction, so entering any instruction at
/// its proven depth keeps every invariant the unchecked driver relies on
/// downstream, whatever a `puthash` after sealing did to the table (GNU
/// follows such a table blindly).
///
/// `switch_targets` pre-resolves every jump-table value the proof admitted:
/// `(raw value, instruction index)`, sorted by raw value and deduplicated
/// across the function's switches (a GNU byte offset names one instruction
/// whichever switch holds it). It only replaces the byte-offset search; the
/// depth check above still decides every hit. All admitted switches share
/// that fixed depth, including when a mutated table reuses another target.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct StackProof {
    pub(crate) switch_depth: Option<usize>,
    pub(crate) switch_targets: Box<[(i64, u32)]>,
    pub(crate) depths: Vec<u32>,
}

// The proof's own vocabulary lives WITH the data (T1 §4 proof ownership):
// the upper verifier constructs the proof, and the upper VM reads it; no
// consumer of the lowered type needs an upper-layer helper to interpret it.
impl StackProof {
    pub(crate) const UNREACHABLE: u32 = u32::MAX;

    /// The pre-resolved instruction index of an admitted jump-table value.
    #[inline]
    pub(crate) fn switch_target(&self, raw: i64) -> Option<usize> {
        self.switch_targets
            .binary_search_by_key(&raw, |entry| entry.0)
            .ok()
            .map(|k| self.switch_targets[k].1 as usize)
    }

    /// Whether instruction `pc` is proven at operand depth `depth`.
    #[inline]
    pub(crate) fn proves_depth(&self, pc: usize, depth: usize) -> bool {
        self.depths
            .get(pc)
            .is_some_and(|&proven| proven != Self::UNREACHABLE && proven as usize == depth)
    }
}
