//! Pure Rust text rendering using cosmic-text
//!
//! This module provides text shaping and rasterization using:
//! - cosmic-text for text layout and glyph caching
//! - GdkTexture for GPU upload (gtk4-backend only)
//! - GskTextureNode for rendering (gtk4-backend only)

mod engine;
#[cfg(feature = "gtk4-backend")]
mod atlas;

pub use engine::TextEngine;
#[cfg(feature = "gtk4-backend")]
pub use atlas::{GlyphAtlas, GlyphKey, CachedGlyph};
