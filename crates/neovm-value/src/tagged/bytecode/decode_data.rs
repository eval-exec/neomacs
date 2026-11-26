//! Decoded-stack proof storage data (T1 storage cut).

/// A successful operand-stack proof.
///
/// `depths` is the proven operand depth at every instruction,
/// [`StackProof::UNREACHABLE`] where none exists. A resume reads one entry:
/// the verified driver is safe at that pc only when the live depth equals
/// it. The table is also what a `Switch` uses after sealing, because a jump
/// table is a live Lisp object and can retarget. The VERIFIED driver takes a
/// switch hit exactly when the resolved instruction's proven depth equals the
/// executing switch's proven post-pop depth — recorded in `depths` at that
/// switch's fall-through instruction (`pc + 1`). Different switches in the
/// same function may therefore sit at different stack depths while still
/// rejecting any mutated-table jump to a target proven at another depth.
/// The proof is per instruction, so entering any instruction at its proven
/// depth keeps every invariant the unchecked driver relies on downstream,
/// whatever a `puthash` after sealing did to the table (GNU follows such a
/// table blindly).
///
/// `switch_targets` pre-resolves every jump-table value the proof admitted:
/// `(raw value, instruction index)`, sorted by raw value and deduplicated
/// across the function's switches (a GNU byte offset names one instruction
/// whichever switch holds it). It only replaces the byte-offset search; the
/// per-site depth check above still decides every hit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[doc(hidden)]
pub struct StackProof {
    pub(crate) switch_targets: Box<[(i64, u32)]>,
    pub(crate) depths: Vec<u32>,
    /// `max_stack` the verifier used. A resume may trust this proof only
    /// when the function still declares the same limit.
    pub(crate) max_stack: u32,
}

// The proof's own vocabulary lives WITH the data (T1 §4 proof ownership):
// the upper verifier constructs the proof, and the upper VM reads it; no
// consumer of the lowered type needs an upper-layer helper to interpret it.
impl StackProof {
    #[doc(hidden)]
    pub const UNREACHABLE: u32 = u32::MAX;

    /// The pre-resolved instruction index of an admitted jump-table value.
    #[inline]
    #[doc(hidden)]
    pub fn switch_target(&self, raw: i64) -> Option<usize> {
        self.switch_targets
            .binary_search_by_key(&raw, |entry| entry.0)
            .ok()
            .map(|k| self.switch_targets[k].1 as usize)
    }

    /// Whether instruction `pc` is proven at operand depth `depth`.
    #[inline]
    #[doc(hidden)]
    pub fn proves_depth(&self, pc: usize, depth: usize) -> bool {
        self.depths
            .get(pc)
            .is_some_and(|&proven| proven != Self::UNREACHABLE && proven as usize == depth)
    }

    /// Proven operand depth at instruction `pc`, or `None` if `pc` is out of
    /// range or unreachable.
    #[inline]
    #[doc(hidden)]
    pub fn proven_depth(&self, pc: usize) -> Option<usize> {
        let &proven = self.depths.get(pc)?;
        (proven != Self::UNREACHABLE).then_some(proven as usize)
    }

    /// Operand-stack limit this proof was computed against.
    #[inline]
    #[doc(hidden)]
    pub fn max_stack(&self) -> usize {
        self.max_stack as usize
    }

    /// Copy the per-instruction depth table. The `Vec` stays private.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn copy_depths_for_test(&self, out: &mut Vec<u32>) {
        out.extend_from_slice(&self.depths);
    }

    /// Copy the pre-resolved jump-table pairs. The box stays private.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn copy_switch_targets_for_test(&self, out: &mut Vec<(i64, u32)>) {
        out.extend_from_slice(&self.switch_targets);
    }
}

impl StackProof {
    /// # Safety
    /// `depths` must be the verifier's result for exactly these ops, and
    /// `max_stack` must be the limit that verifier ran with. Resume trusts
    /// both.
    #[doc(hidden)]
    pub unsafe fn from_parts(
        switch_targets: Box<[(i64, u32)]>,
        depths: Vec<u32>,
        max_stack: usize,
    ) -> Self {
        debug_assert!(max_stack <= u32::MAX as usize);
        Self {
            switch_targets,
            depths,
            max_stack: max_stack as u32,
        }
    }
}
