//! The staged resize plan required by GNU's low-level Lisp split primitive.
use crate::window::{
    CombinationLimit, Frame, SplitAttachment, SplitDirection, SplitSizeError, SplitSizes, Window,
    WindowId, WindowPixels, WindowSizeError, WindowTree,
};

/// Dynamic Lisp policy decoded once at the primitive boundary.
/// Immutable and heap-free, so it may be shared between mutators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SiblingResize {
    Preserve,
    Resize,
}

impl From<crate::emacs_core::value::Value> for SiblingResize {
    fn from(value: crate::emacs_core::value::Value) -> Self {
        if value.is_nil() {
            Self::Preserve
        } else {
            Self::Resize
        }
    }
}

/// Borrows the current mutator's immutable frame for the validation call only.
/// Its thread capabilities follow Frame; it owns no GC pointers or mutable
/// state, and validation invokes no Lisp callbacks or concurrent frame edits.
pub(super) struct SplitRequestInput<'a> {
    pub(super) frame: &'a Frame,
    pub(super) old: WindowId,
    pub(super) size: i64,
    pub(super) direction: SplitDirection,
    pub(super) limit: CombinationLimit,
    pub(super) sibling_resize: SiblingResize,
}

impl std::fmt::Debug for SplitRequestInput<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SplitRequestInput")
            .field("frame", &self.frame.id)
            .field("old", &self.old)
            .field("size", &self.size)
            .field("direction", &self.direction)
            .field("limit", &self.limit)
            .field("sibling_resize", &self.sibling_resize)
            .finish()
    }
}

/// Immutable, heap-free request with Send/Sync scalar components. It is tied
/// logically to the frame state validated by the current mutator: even though
/// it can cross threads, a consumer must hold exclusive frame access and allow
/// no callbacks or geometry edits between validation and consumption.
/// The provisional size only creates tree nodes; the requested staged plan is
/// applied immediately afterward. This preserves direct Rust split semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SplitRequest {
    size: WindowPixels,
    staged_geometry: Option<(WindowId, SplitDirection, WindowPixels)>,
    parent_restore: Option<(WindowId, WindowPixels)>,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
pub(super) enum SplitRequestError {
    #[error("Size of new window too small (after split)")]
    NewTooSmall,
    #[error("Resizing old window failed")]
    OldResizeFailed,
    #[error("Sum of sizes of old and new window don’t fit")]
    SumDoesNotFit,
    #[error("Window sizes don’t fit")]
    ParentResizeFailed {
        parent: WindowId,
        pending: crate::tagged::value::Fixnum,
    },
}

fn physical_extent(window: &Window, direction: SplitDirection) -> i64 {
    let bounds = window.bounds();
    (match direction {
        SplitDirection::Horizontal => bounds.width,
        SplitDirection::Vertical => bounds.height,
    }) as i64
}

/// GNU window.c:4716–4804. This checks the primitive's staged slots, rather
/// than the general Rust resize helper's fallback to physical extents.
fn staged_resize_fits(
    tree: &WindowTree,
    id: WindowId,
    direction: SplitDirection,
    minimum: i64,
    root_extent: Option<i64>,
) -> bool {
    let Some(window) = tree.find(id) else {
        return false;
    };
    let extent = root_extent.unwrap_or_else(|| window.new_pixel().unwrap_or(0));
    match WindowPixels::try_from(extent) {
        Ok(_) => {}
        Err(WindowSizeError::OutOfRange) => return false,
    }
    let (combination, children) = match window {
        Window::Leaf { .. } => return extent >= minimum,
        Window::Internal {
            direction,
            children,
            ..
        } => (direction, children),
    };
    let mut remaining = i128::from(extent);
    for child in children {
        let Some(child_window) = tree.find(*child) else {
            return false;
        };
        let child_extent = child_window.new_pixel().unwrap_or(0);
        if *combination == direction {
            remaining -= i128::from(child_extent);
            if remaining < 0 {
                return false;
            }
        } else if child_extent != extent {
            return false;
        }
        if !staged_resize_fits(tree, *child, direction, minimum, None) {
            return false;
        }
    }
    *combination != direction || remaining == 0
}

impl TryFrom<SplitRequestInput<'_>> for SplitRequest {
    type Error = SplitRequestError;

    fn try_from(input: SplitRequestInput<'_>) -> Result<Self, Self::Error> {
        // Accept only a canonical Lisp fixnum even if an internal Rust caller
        // constructs an input directly instead of going through expect_fixnum.
        crate::tagged::value::Fixnum::try_from(input.size).map_err(|error| match error {
            crate::tagged::value::FixnumRangeError::OutOfRange(_) => {
                SplitRequestError::OldResizeFailed
            }
        })?;
        let tree = input.frame.tree();
        let old = tree
            .find(input.old)
            .ok_or(SplitRequestError::OldResizeFailed)?;
        let (cell, cells) = match input.direction {
            SplitDirection::Horizontal => (input.frame.char_width, 2),
            SplitDirection::Vertical => (input.frame.char_height, 1),
        };
        let cell = (cell as i64).max(1);
        if input.size / cell < cells {
            return Err(SplitRequestError::NewTooSmall);
        }
        let minimum = cell.saturating_mul(cells);
        let parent = tree.parent_of(input.old);
        let parent_combination = parent.and_then(|parent| match tree.find(parent)? {
            Window::Internal { direction, .. } => Some(*direction),
            Window::Leaf { .. } => None,
        });
        let reuse_parent =
            match SplitAttachment::decide(input.limit, parent_combination, input.direction) {
                SplitAttachment::NewParent(_) => false,
                SplitAttachment::ReuseParent => true,
            };
        let resize_siblings = match input.sibling_resize {
            SiblingResize::Preserve => false,
            SiblingResize::Resize => reuse_parent,
        };
        let parent_restore = if resize_siblings {
            let parent = parent.ok_or(SplitRequestError::OldResizeFailed)?;
            let physical = physical_extent(
                tree.find(parent)
                    .ok_or(SplitRequestError::OldResizeFailed)?,
                input.direction,
            );
            // Both operands fit the signed fixnum domain at the Lisp boundary.
            // GNU make_fixnum canonicalizes this temporary parent slot.
            let pending =
                crate::tagged::value::Fixnum::from_payload_bits((physical - input.size) as u64);
            if !staged_resize_fits(
                tree,
                parent,
                input.direction,
                minimum,
                Some(i64::from(pending)),
            ) {
                return Err(SplitRequestError::ParentResizeFailed { parent, pending });
            }
            Some((
                parent,
                WindowPixels::try_from(physical).map_err(|error| match error {
                    WindowSizeError::OutOfRange => SplitRequestError::OldResizeFailed,
                })?,
            ))
        } else {
            if !staged_resize_fits(tree, input.old, input.direction, minimum, None) {
                return Err(SplitRequestError::OldResizeFailed);
            }
            if i128::from(input.size) + i128::from(old.new_pixel().unwrap_or(0))
                != i128::from(physical_extent(old, input.direction))
            {
                return Err(SplitRequestError::SumDoesNotFit);
            }
            None
        };
        let size = WindowPixels::try_from(input.size).map_err(|error| match error {
            WindowSizeError::OutOfRange => SplitRequestError::OldResizeFailed,
        })?;
        // A sibling plan can provide more pixels than OLD originally had.
        // The direct Rust helper needs a positive provisional rectangle; use
        // OLD's checked future extent plus the new size, then apply the staged
        // parent immediately. No Lisp observes this intermediate geometry.
        let staged_geometry = if parent_restore.is_some() {
            let total = i128::from(old.new_pixel().unwrap_or(0)) + i128::from(input.size);
            let total = i64::try_from(total).map_err(|_| SplitRequestError::OldResizeFailed)?;
            let total = WindowPixels::try_from(total).map_err(|error| match error {
                WindowSizeError::OutOfRange => SplitRequestError::OldResizeFailed,
            })?;
            Some((input.old, input.direction, total))
        } else {
            None
        };
        // Prove the existing geometry helper cannot reject after the live
        // provisional rectangle has been published, including f32 rounding at
        // large extents. A rejected request has changed no live geometry.
        let geometry_extent = staged_geometry.map_or_else(
            || physical_extent(old, input.direction),
            |(_, _, total)| i64::from(total),
        );
        SplitSizes::new(geometry_extent as f32, Some(input.size)).map_err(|error| match error {
            SplitSizeError::NewTooSmall => SplitRequestError::OldResizeFailed,
            SplitSizeError::OldTooSmall => SplitRequestError::OldResizeFailed,
        })?;
        Ok(Self {
            size,
            staged_geometry,
            parent_restore,
        })
    }
}

impl SplitRequest {
    pub(super) fn size(self) -> i64 {
        i64::from(self.size)
    }

    pub(super) fn prepare_tree_split(self, frame: &mut Frame) {
        if let Some((id, pixels)) = self.parent_restore
            && let Some(parent) = frame.find_window_mut(id)
        {
            parent.set_new_pixel(Some(i64::from(pixels)));
        }
        if let Some((id, direction, total)) = self.staged_geometry
            && let Some(old) = frame.find_window_mut(id)
        {
            match direction {
                SplitDirection::Horizontal => old.bounds_mut().width = i64::from(total) as f32,
                SplitDirection::Vertical => old.bounds_mut().height = i64::from(total) as f32,
            }
        }
    }
}

static_assertions::assert_impl_all!(SplitRequest: Send, Sync);

static_assertions::assert_impl_all!(SiblingResize: Send, Sync);
