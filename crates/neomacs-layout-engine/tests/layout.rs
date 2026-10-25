//! Every neomacs-layout-engine integration suite in one test binary.
//!
//! Each former per-file binary linked the crate dependency closure again;
//! nextest runs every test in its own process, so one binary is equivalent
//! at run time.

#[path = "child_frame_border.rs"]
mod child_frame_border;

#[path = "direct_write.rs"]
mod direct_write;

#[path = "font_coverage.rs"]
mod font_coverage;

#[path = "issue_396_pixel_scroll.rs"]
mod issue_396_pixel_scroll;

#[path = "neovm_bridge_minibuffer.rs"]
mod neovm_bridge_minibuffer;

#[path = "text_extent.rs"]
mod text_extent;
