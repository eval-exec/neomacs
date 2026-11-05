//! Pure sibling-layout arithmetic for one window split.
//!
//! Laying a parent's children out along the split axis needs only four numbers
//! per child -- its current rectangle and its two `window-fixed-size` extents --
//! yet the arithmetic used to be written directly against `&mut [Window]`. That
//! coupling made the hardest part of window layout the part hardest to test, and
//! it is the one place that cannot survive a `children: Vec<Window>` ->
//! `Vec<WindowId>` flip unchanged, because arena children cannot be borrowed
//! mutably while their siblings are read.
//!
//! So the math lives here instead, over [`ChildExtent`] values that a caller
//! gathers before touching the tree. `sibling_bounds` is a total function of its
//! inputs: gather extents, drop the borrow, compute, write the rectangles back.

use super::{Rect, SplitDirection};

/// Everything sibling layout needs to know about one child window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChildExtent {
    /// The child's rectangle before the redistribution.
    pub bounds: Rect,
    /// `window-size-fixed` width in columns; 0 when the width may flex.
    pub fixed_width_cols: usize,
    /// `window-size-fixed` height in lines; 0 when the height may flex.
    pub fixed_height_lines: usize,
}

impl ChildExtent {
    /// The child's extent along `direction`, clamped to a whole non-negative
    /// pixel count.
    fn current(&self, direction: SplitDirection) -> f32 {
        match direction {
            SplitDirection::Horizontal => self.bounds.width,
            SplitDirection::Vertical => self.bounds.height,
        }
        .round()
        .max(0.0)
    }

    /// Whether this child refuses to flex along `direction`.
    fn is_fixed(&self, direction: SplitDirection) -> bool {
        let fixed_cells = match direction {
            SplitDirection::Horizontal => self.fixed_width_cols,
            SplitDirection::Vertical => self.fixed_height_lines,
        };
        fixed_cells > 0
    }
}

/// The axis `children` are already laid out along, inferred from the first two
/// siblings: children that differ in `x` sit side by side, anything else is
/// stacked. `None` when there is no pair to compare.
pub(crate) fn detect_direction(children: &[ChildExtent]) -> Option<SplitDirection> {
    let (first, second) = (children.first()?, children.get(1)?);
    Some(if (first.bounds.x - second.bounds.x).abs() > 0.1 {
        SplitDirection::Horizontal
    } else {
        SplitDirection::Vertical
    })
}

/// The rectangles `children` should occupy inside `parent`, in child order.
///
/// The returned vector always has one rectangle per child, so a caller can zip
/// it straight back over the siblings it gathered the extents from.
pub(crate) fn sibling_bounds(parent: Rect, children: &[ChildExtent]) -> Vec<Rect> {
    let Some(direction) = detect_direction(children) else {
        // No pair to infer an axis from: a lone child inherits the parent's
        // rectangle verbatim, unrounded, and an empty parent lays out nothing.
        return children.iter().map(|_| parent).collect();
    };

    match direction {
        SplitDirection::Horizontal => {
            let widths = sizes_preserving_fixed(parent.width, children, direction);
            let mut edge = parent.x.round();
            widths
                .into_iter()
                .map(|width| {
                    let rect = Rect::new(edge, parent.y.round(), width, parent.height.round());
                    edge += width;
                    rect
                })
                .collect()
        }
        SplitDirection::Vertical => {
            let heights = sizes_preserving_fixed(parent.height, children, direction);
            let mut edge = parent.y.round();
            heights
                .into_iter()
                .map(|height| {
                    let rect = Rect::new(parent.x.round(), edge, parent.width.round(), height);
                    edge += height;
                    rect
                })
                .collect()
        }
    }
}

/// Split `total` into `n` whole pixels as evenly as possible, handing the
/// remainder to the leading children one pixel at a time.
fn distributed_sizes(total: f32, n: usize) -> Vec<f32> {
    let total_px = total.round().max(0.0) as i64;
    let n = n as i64;
    let base = total_px / n;
    let remainder = total_px % n;
    (0..n)
        .map(|idx| (base + i64::from(idx < remainder)) as f32)
        .collect()
}

/// Split `total` along `direction`, holding every fixed-size child at the size
/// it already has and sharing what is left among the rest in proportion to
/// their current sizes.
///
/// Falls back to an even split whenever the fixed children alone would fill
/// `total`, or when nothing is fixed at all.
fn sizes_preserving_fixed(
    total: f32,
    children: &[ChildExtent],
    direction: SplitDirection,
) -> Vec<f32> {
    let total_px = total.round().max(0.0);
    let mut sizes = vec![0.0; children.len()];
    let mut flexible = Vec::new();
    let mut fixed_total = 0.0;
    let mut flexible_current_total = 0.0;

    for (idx, child) in children.iter().enumerate() {
        let current = child.current(direction);
        if child.is_fixed(direction) {
            sizes[idx] = current;
            fixed_total += current;
        } else {
            flexible.push(idx);
            flexible_current_total += current;
        }
    }

    if flexible.is_empty() || fixed_total >= total_px {
        return distributed_sizes(total, children.len());
    }

    let flexible_total = total_px - fixed_total;
    if flexible_current_total <= 0.0 {
        // Nothing to scale in proportion to: share the slack out evenly.
        let flexible_sizes = distributed_sizes(flexible_total, flexible.len());
        for (idx, size) in flexible.into_iter().zip(flexible_sizes) {
            sizes[idx] = size;
        }
        return sizes;
    }

    let mut assigned = 0.0;
    let last_flexible = flexible.len().saturating_sub(1);
    for (flex_idx, idx) in flexible.into_iter().enumerate() {
        let current = children[idx].current(direction);
        let size = if flex_idx == last_flexible {
            // The last flexible child absorbs the rounding drift so the
            // children always tile the parent exactly.
            (flexible_total - assigned).max(0.0)
        } else {
            (flexible_total * (current / flexible_current_total))
                .round()
                .max(0.0)
        };
        sizes[idx] = size;
        assigned += size;
    }
    sizes
}

#[cfg(test)]
#[path = "sibling_layout/tests/sibling_layout_test.rs"]
mod tests;
