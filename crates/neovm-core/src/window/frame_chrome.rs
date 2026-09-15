//! Whether frame bars participate in window layout.
//!
//! This is independent of visibility: iconifying a realized frame must not
//! remove its reserved bar space. Batch frames retain unrealized geometry.

use super::Frame;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameChromeLayout {
    #[default]
    Unrealized,
    Realized,
}

impl Frame {
    pub fn chrome_layout(&self) -> FrameChromeLayout {
        self.chrome_layout
    }

    /// Change chrome participation and reconcile the entire window tree.
    /// Callers cannot publish the new state without its geometry/invalidation.
    pub fn set_chrome_layout(&mut self, layout: FrameChromeLayout) {
        self.chrome_layout = layout;
        self.sync_window_area_bounds();
    }

    pub fn displays_chrome(&self) -> bool {
        self.chrome_layout == FrameChromeLayout::Realized
    }
}
