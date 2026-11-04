//! Upper extension traits for evaluator-owned window data (stage-4 W2/E5).
//!
//! The data moved into `emacs_core::eval::data::window_state`; the
//! behaviour that binds this engine lives here as extension traits, the
//! N17 shape the split requires (core cannot add inherent impls to eval
//! types once eval is a crate).

use super::{
    ChildExtent, Rect, Window, WindowId, WindowParameters, WindowTree, display, sibling_bounds,
};

/// Window behaviour that mints display state rather than reading data.
pub trait DisplayWindowExt {
    /// Return a mutable reference to this window's Lisp-visible parameter alist.
    fn parameters_mut(&mut self) -> &mut WindowParameters;
}

impl DisplayWindowExt for Window {
    /// Return a mutable reference to this window's Lisp-visible parameter alist.
    fn parameters_mut(&mut self) -> &mut WindowParameters {
        let next_generation = display::next_window_parameters_generation();
        match self {
            Window::Leaf {
                parameters,
                parameters_generation,
                ..
            }
            | Window::Internal {
                parameters,
                parameters_generation,
                ..
            } => {
                *parameters_generation = next_generation;
                parameters
            }
        }
    }
}

/// Window-tree behaviour that binds the sibling layout algorithm.
pub trait DisplayWindowTreeExt {
    fn sibling_bounds_of(&self, parent: WindowId, bounds: Rect) -> Vec<Rect>;
    fn redistribute_bounds(&mut self, parent: WindowId, bounds: Rect);
}

impl DisplayWindowTreeExt for WindowTree {
    /// The rectangles `parent`'s children should occupy inside `bounds`, in
    /// child order.
    ///
    /// Reading each child's extent finishes before any of them is written, so
    /// no two nodes are borrowed at once.  The arithmetic itself lives in
    /// [`sibling_layout`], where it is testable without a window tree.
    fn sibling_bounds_of(&self, parent: WindowId, bounds: Rect) -> Vec<Rect> {
        let extents: Vec<ChildExtent> = self
            .child_ids(parent)
            .iter()
            .filter_map(|child| self.find(*child))
            .map(|child| ChildExtent {
                bounds: *child.bounds(),
                fixed_width_cols: child.fixed_width_cols(),
                fixed_height_lines: child.fixed_height_lines(),
            })
            .collect();
        sibling_bounds(bounds, &extents)
    }

    /// Give `parent`'s children their share of `bounds`, without descending.
    fn redistribute_bounds(&mut self, parent: WindowId, bounds: Rect) {
        let children = self.child_ids(parent).to_vec();
        for (child, rect) in children
            .into_iter()
            .zip(self.sibling_bounds_of(parent, bounds))
        {
            if let Some(node) = self.find_mut(child) {
                node.set_bounds(rect);
            }
        }
    }
}
