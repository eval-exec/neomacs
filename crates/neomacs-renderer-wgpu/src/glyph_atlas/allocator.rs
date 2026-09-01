//! Shelf-based atlas page allocator.
//!
//! A simple left-to-right, top-to-bottom allocator that packs glyph
//! rectangles onto a fixed-size atlas page. Each glyph gets padding
//! around all four sides. When a shelf fills up, a new shelf is started.
//! When the page fills up, the caller must create a new page.
//!
//! No behavior change — this is introduced alongside the existing code
//! and will be wired in during later steps.

use std::num::NonZeroU32;

use super::types::{AtlasAllocationRect, AtlasContentRect, PixelSize};

/// Result of a shelf allocation attempt.
pub struct Allocation {
    /// The full padded allocation rect (content + padding).
    pub allocation_rect: AtlasAllocationRect,
    /// The inner content rect (where glyph pixels go).
    pub content_rect: AtlasContentRect,
}

/// Shelf-based allocator for a single atlas page.
///
/// Allocates rectangles left-to-right on horizontal shelves. When the
/// current shelf cannot fit the requested width, a new shelf starts at
/// `cursor_y + shelf_height`. When the page cannot fit the requested
/// height, allocation fails and the caller should create a new page.
#[derive(Debug)]
pub struct ShelfAllocator {
    page_size: u32,
    padding: u32,
    cursor_x: u32,
    cursor_y: u32,
    shelf_height: u32,
}

impl ShelfAllocator {
    pub fn new(page_size: u32, padding: u32) -> Self {
        Self {
            page_size,
            padding,
            cursor_x: 0,
            cursor_y: 0,
            shelf_height: 0,
        }
    }

    /// Attempt to allocate space for a glyph of the given pixel size.
    ///
    /// Returns `None` if the glyph is too large for the page or the page
    /// is full.
    pub fn allocate(&mut self, glyph_size: PixelSize) -> Option<Allocation> {
        let max_content = self.page_size.saturating_sub(2 * self.padding);
        if glyph_size.width() > max_content || glyph_size.height() > max_content {
            return None;
        }

        let alloc_w = glyph_size.width() + 2 * self.padding;
        let alloc_h = glyph_size.height() + 2 * self.padding;

        let (x, y) = self.find_position(alloc_w, alloc_h)?;

        let alloc_rect = AtlasAllocationRect::new(
            x,
            y,
            NonZeroU32::new(alloc_w).unwrap(),
            NonZeroU32::new(alloc_h).unwrap(),
        );

        let content_x = x + self.padding;
        let content_y = y + self.padding;
        let content_rect = AtlasContentRect::new(
            content_x,
            content_y,
            NonZeroU32::new(glyph_size.width()).unwrap(),
            NonZeroU32::new(glyph_size.height()).unwrap(),
        );

        Some(Allocation {
            allocation_rect: alloc_rect,
            content_rect,
        })
    }

    fn find_position(&mut self, alloc_w: u32, alloc_h: u32) -> Option<(u32, u32)> {
        if self.cursor_x + alloc_w <= self.page_size {
            if self.cursor_y + alloc_h > self.page_size {
                return None;
            }

            let x = self.cursor_x;
            let y = self.cursor_y;
            self.cursor_x += alloc_w;
            self.shelf_height = self.shelf_height.max(alloc_h);
            return Some((x, y));
        }

        let new_y = self.cursor_y + self.shelf_height;
        if new_y + alloc_h > self.page_size {
            return None;
        }

        // Place this glyph at the start of the new shelf and advance the cursor
        // past it, exactly like the same-shelf branch above. Leaving `cursor_x`
        // at 0 here would place the NEXT glyph on top of this one, overlapping
        // them in the atlas texture (they would render as each other).
        self.cursor_x = alloc_w;
        self.cursor_y = new_y;
        self.shelf_height = alloc_h;

        Some((0, new_y))
    }
}

#[cfg(test)]
#[path = "allocator_test.rs"]
mod tests;
