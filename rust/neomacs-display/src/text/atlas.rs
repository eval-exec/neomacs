//! Glyph texture atlas for efficient GPU rendering
//!
//! Caches rasterized glyphs in a GPU texture atlas for batched rendering.
//!
//! This module is only available with the `gtk4-backend` feature.

#![cfg(feature = "gtk4-backend")]

use gtk4::gdk;
use std::collections::HashMap;

/// A cached glyph in the atlas
#[derive(Debug, Clone)]
pub struct CachedGlyph {
    /// Texture containing this glyph
    pub texture: gdk::Texture,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bearing X (offset from origin)
    pub bearing_x: f32,
    /// Bearing Y (offset from baseline)
    pub bearing_y: f32,
}

/// Key for glyph cache lookup
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GlyphKey {
    /// Character code
    pub charcode: u32,
    /// Face ID (determines font, size, style)
    pub face_id: u32,
}

/// Glyph atlas - caches rasterized glyphs as GPU textures
pub struct GlyphAtlas {
    /// Cached glyphs: (charcode, face_id) → CachedGlyph
    cache: HashMap<GlyphKey, CachedGlyph>,
    /// Maximum cache size
    max_size: usize,
}

impl GlyphAtlas {
    /// Create a new glyph atlas
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            max_size: 4096, // Max glyphs to cache
        }
    }

    /// Create with custom max size
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            cache: HashMap::new(),
            max_size,
        }
    }

    /// Get a cached glyph
    pub fn get(&self, key: &GlyphKey) -> Option<&CachedGlyph> {
        self.cache.get(key)
    }

    /// Insert a glyph into the cache
    pub fn insert(&mut self, key: GlyphKey, glyph: CachedGlyph) {
        // Simple eviction: clear half the cache when full
        if self.cache.len() >= self.max_size {
            let keys_to_remove: Vec<_> = self.cache.keys()
                .take(self.max_size / 2)
                .cloned()
                .collect();
            for k in keys_to_remove {
                self.cache.remove(&k);
            }
        }

        self.cache.insert(key, glyph);
    }

    /// Insert a glyph with individual parameters
    pub fn insert_texture(&mut self, key: GlyphKey, texture: gdk::Texture, width: u32, height: u32, bearing_x: f32, bearing_y: f32) {
        let glyph = CachedGlyph {
            texture,
            width,
            height,
            bearing_x,
            bearing_y,
        };
        self.insert(key, glyph);
    }

    /// Check if glyph is cached
    pub fn contains(&self, key: &GlyphKey) -> bool {
        self.cache.contains_key(key)
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Number of cached glyphs
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}
