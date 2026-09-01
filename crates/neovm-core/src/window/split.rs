//! The split-attachment decision: *where* a new window joins the tree.
//!
//! This is ONE decode of GNU `Fsplit_window_internal`'s `combination_limit`
//! (`src/window.c:5423-5431`):
//!
//! ```c
//! /* Set combination_limit if we have to make a new parent window.
//!    We do that if either `window-combination-limit' is t, or OLD has no
//!    parent, or OLD is ortho-combined.  */
//! bool combination_limit
//!   = (EQ (Vwindow_combination_limit, Qt)
//!      || NILP (o->parent)
//!      || (horflag
//!          ? WINDOW_VERTICAL_COMBINATION_P (XWINDOW (o->parent))
//!          : WINDOW_HORIZONTAL_COMBINATION_P (XWINDOW (o->parent))));
//! ```
//!
//! # Why this lives in its own module
//!
//! GNU has *two* distinct things spelled "combination limit", and conflating
//! them is what produced the bug this module exists to prevent:
//!
//! | GNU name                     | Kind             | Governs                       |
//! |------------------------------|------------------|-------------------------------|
//! | `Vwindow_combination_limit`  | dynamic variable | nesting on **split**          |
//! | `w->combination_limit`       | per-window slot  | recombining on **delete**     |
//!
//! `Fsplit_window_internal` reads only the *variable*; the *slot* is read only
//! by `recombine_windows` (`src/window.c:2616`). Both are booleans in the C
//! source, so nothing stops one being substituted for the other — and neomacs
//! did exactly that: the split path consulted the parent node's stored slot
//! and ignored the variable entirely. Every Lisp binding of
//! `window-combination-limit` was therefore silently dropped, including the
//! one `split-window` installs when the split target has a side-window sibling
//! (`lisp/window.el`), which is what wrongly flattened the frame's main area
//! into the root combination and left later side windows mid-frame.
//!
//! Giving the two concepts distinct Rust types makes that substitution a
//! compile error rather than a behavioural divergence.

use super::SplitDirection;

/// How the space freed by deleting a window is reclaimed.
///
/// GNU splits this across two layers. `lisp/window.el`'s `delete-window` picks
/// the sibling that absorbs the space —
/// `(sibling (or (window-left window) (window-right window)))`, i.e. the
/// previous sibling when there is one, else the next — and *stages* its new
/// size with `window--resize-this-window`. `Fdelete_window_internal` then
/// *commits* the staged `new_pixel` values with `window_resize_apply`.
///
/// So the primitive must never invent a layout: the Lisp layer already decided
/// one. (When `window-combination-resize` is `t`, that same Lisp layer instead
/// spreads the space across all siblings via `window--resize-siblings` — which
/// is why re-deriving "give it to the neighbour" in Rust would be wrong too.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteResize {
    /// Commit the sizes `window.el` staged in `new_pixel` — what the GNU
    /// primitive does, and what the Lisp entry point must ask for.
    ApplyStaged,
    /// No Lisp layer ran, so nothing was staged: spread the freed space over
    /// the remaining children. Only for direct manipulation of the tree from
    /// Rust, which has no `window.el` to defer to.
    Redistribute,
}

/// What a delete did to the subtree it was applied to.
///
/// The `Promoted` case is what drives recombination: GNU calls
/// `recombine_windows` on the surviving sibling *only* in the matryoshka branch
/// of `Fdelete_window_internal` (`src/window.c:5801`), i.e. only when the
/// deleted window's parent collapsed and the sibling took its place. Reporting
/// it as a distinct outcome keeps that "only then" from decaying into "whenever
/// the shape happens to look right".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteOutcome {
    /// The target is not in this subtree.
    NotFound,
    /// The target was removed; this subtree's root still stands.
    Removed,
    /// The target was removed and this subtree's root was replaced by the sole
    /// surviving sibling, which may now be iso-combined with its new parent.
    RemovedAndPromoted,
}

impl DeleteOutcome {
    /// Whether the target was removed, however the subtree was reshaped.
    pub fn removed(self) -> bool {
        !matches!(self, Self::NotFound)
    }
}

/// The value of the dynamic Lisp variable `window-combination-limit`, as read
/// by the split path.
///
/// GNU compares against the symbol `t` with `EQ`, so every other value —
/// `nil`, `window-size`, `temp-buffer`, `temp-buffer-resize`, `display-buffer`
/// — behaves identically here and collapses into [`Self::TreeDecides`]. Those
/// other values matter to *`display-buffer`*, which binds the variable to `t`
/// when it recognises its own symbol; by the time the split runs, only "is it
/// `t`?" is left to ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombinationLimit {
    /// The variable is exactly `t`: always interpose a fresh parent window,
    /// and seal that parent against later recombination.
    ForceNewParent,
    /// Any other value: the shape of the tree alone decides.
    TreeDecides,
}

impl CombinationLimit {
    /// Decode the dynamic variable's value. Only the symbol `t` forces a new
    /// parent (GNU `EQ (Vwindow_combination_limit, Qt)`).
    pub fn from_is_t(is_t: bool) -> Self {
        if is_t {
            Self::ForceNewParent
        } else {
            Self::TreeDecides
        }
    }
}

/// Whether a newly interposed parent window is sealed against recombination.
///
/// GNU stores `t` in the new parent's `combination_limit` slot **only** when
/// the dynamic variable was `t` (`src/window.c:5557-5560`):
///
/// ```c
/// if (EQ (Vwindow_combination_limit, Qt))
///   /* Store t in the new parent's combination_limit slot to avoid
///      that its children get merged into another window.  */
///   wset_combination_limit (p, Qt);
/// ```
///
/// A parent interposed merely because the target was the root or was
/// ortho-combined stays unsealed, so deleting one of its children may later
/// splice the rest into the grandparent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParentSeal {
    /// `w->combination_limit` = `t`: children must never be merged upward.
    Sealed,
    /// `w->combination_limit` = `nil`: recombination is allowed.
    Unsealed,
}

impl ParentSeal {
    /// The stored slot value, in the `bool` shape the window node uses.
    pub fn as_stored_slot(self) -> bool {
        matches!(self, Self::Sealed)
    }
}

/// How the target's parent is combined, or that the target has no parent.
///
/// Modelling "the target is the frame root" as `None` rather than as a
/// sentinel direction is what keeps GNU's `NILP (o->parent)` term from being
/// silently forgotten: the caller cannot construct this value without
/// answering the question.
pub type ParentCombination = Option<SplitDirection>;

/// Where the new window attaches to the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitAttachment {
    /// Replace the target with a fresh internal node whose children are the
    /// target and the new window (GNU `make_parent_window` +
    /// `replace_window`).
    NewParent(ParentSeal),
    /// Splice the new window into the target's existing parent combination
    /// (GNU `p = XWINDOW (o->parent)`).
    ///
    /// Note this says nothing about whether the target is a leaf: GNU splices
    /// a sibling next to an internal node just as readily, which is precisely
    /// how a side window is attached beside the frame's main-window group.
    ReuseParent,
}

impl SplitAttachment {
    /// GNU `src/window.c:5423-5431`.
    ///
    /// `parent` is the combination direction of the target's parent, or `None`
    /// when the target is the frame's root window. `split` is the direction of
    /// the combination the new window needs.
    pub fn decide(
        limit: CombinationLimit,
        parent: ParentCombination,
        split: SplitDirection,
    ) -> Self {
        if let CombinationLimit::ForceNewParent = limit {
            // `EQ (Vwindow_combination_limit, Qt)`
            return Self::NewParent(ParentSeal::Sealed);
        }
        match parent {
            // `NILP (o->parent)` -- nothing to reuse.
            None => Self::NewParent(ParentSeal::Unsealed),
            // OLD is ortho-combined: its parent stacks along the other axis,
            // so the new sibling cannot live in it.
            Some(dir) if dir != split => Self::NewParent(ParentSeal::Unsealed),
            // Iso-combined: reuse the parent.
            Some(_) => Self::ReuseParent,
        }
    }

    /// The seal to stamp on a parent interposed by this attachment.
    ///
    /// [`Self::ReuseParent`] interposes no parent, so the question does not
    /// arise; it answers [`ParentSeal::Unsealed`], which is inert.
    pub fn new_parent_seal(self) -> ParentSeal {
        match self {
            Self::NewParent(seal) => seal,
            Self::ReuseParent => ParentSeal::Unsealed,
        }
    }

    /// Whether the new window joins the target's existing combination.
    pub fn reuses_parent(self) -> bool {
        matches!(self, Self::ReuseParent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SplitDirection::{Horizontal, Vertical};

    /// `EQ (Vwindow_combination_limit, Qt)` short-circuits every other term,
    /// and the resulting parent is sealed.
    #[test]
    fn limit_t_always_makes_a_sealed_new_parent() {
        for parent in [None, Some(Horizontal), Some(Vertical)] {
            for split in [Horizontal, Vertical] {
                assert_eq!(
                    SplitAttachment::decide(CombinationLimit::ForceNewParent, parent, split),
                    SplitAttachment::NewParent(ParentSeal::Sealed),
                    "parent={parent:?} split={split:?}"
                );
            }
        }
    }

    /// `NILP (o->parent)`: splitting the frame root always interposes a
    /// parent, but does not seal it.
    #[test]
    fn splitting_the_root_makes_an_unsealed_new_parent() {
        for split in [Horizontal, Vertical] {
            assert_eq!(
                SplitAttachment::decide(CombinationLimit::TreeDecides, None, split),
                SplitAttachment::NewParent(ParentSeal::Unsealed)
            );
        }
    }

    /// An ortho-combined parent cannot hold the new sibling.
    #[test]
    fn ortho_combined_parent_makes_an_unsealed_new_parent() {
        assert_eq!(
            SplitAttachment::decide(CombinationLimit::TreeDecides, Some(Vertical), Horizontal),
            SplitAttachment::NewParent(ParentSeal::Unsealed)
        );
        assert_eq!(
            SplitAttachment::decide(CombinationLimit::TreeDecides, Some(Horizontal), Vertical),
            SplitAttachment::NewParent(ParentSeal::Unsealed)
        );
    }

    /// An iso-combined parent is reused -- the flat-combination case.
    #[test]
    fn iso_combined_parent_is_reused() {
        assert_eq!(
            SplitAttachment::decide(CombinationLimit::TreeDecides, Some(Horizontal), Horizontal),
            SplitAttachment::ReuseParent
        );
        assert_eq!(
            SplitAttachment::decide(CombinationLimit::TreeDecides, Some(Vertical), Vertical),
            SplitAttachment::ReuseParent
        );
    }

    /// Only `t` forces a new parent; `nil` and the other `display-buffer`
    /// values defer to the tree.
    #[test]
    fn only_the_symbol_t_forces_a_new_parent() {
        assert_eq!(
            CombinationLimit::from_is_t(true),
            CombinationLimit::ForceNewParent
        );
        assert_eq!(
            CombinationLimit::from_is_t(false),
            CombinationLimit::TreeDecides
        );
    }

    /// The seal is what `recombine_windows` will later read back.
    #[test]
    fn seal_maps_to_the_stored_slot() {
        assert!(ParentSeal::Sealed.as_stored_slot());
        assert!(!ParentSeal::Unsealed.as_stored_slot());
    }
}
