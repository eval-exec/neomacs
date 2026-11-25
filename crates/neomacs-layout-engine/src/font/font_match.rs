//! Shared font matching helpers.
//!
//! The renderer matches families without the layout engine, so the helpers
//! live in the font materializer and are re-exported here under their
//! layout-engine path (MILESTONES M4).

pub use neomacs_font_materializer::font_match::*;

#[cfg(test)]
#[path = "font_match/tests/font_match_test.rs"]
mod tests;
