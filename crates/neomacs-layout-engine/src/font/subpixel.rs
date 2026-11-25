//! Display subpixel geometry independent of font discovery.
//!
//! The renderer reads the subpixel order without the layout engine, so it
//! lives in the font materializer and is re-exported here under its
//! layout-engine path (MILESTONES M4).

pub use neomacs_font_materializer::subpixel::{FontconfigSubpixelOrder, default_subpixel_order};

#[cfg(all(test, target_os = "macos"))]
#[path = "tests/subpixel_test.rs"]
mod subpixel_test;
