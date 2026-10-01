//! Bounded text outside an authoritative viewport. Coverage has its own body
//! hit index; it never extends window-end or the frame's query geometry.

use std::sync::Arc;

use crate::{
    FrameDisplayState, FrameGlyph, FrameGlyphBuffer, FrameRect, GlyphRowRole,
    PresentationFramePoint, PresentedHit, PresentedHitError, PresentedHitIndex, PresentedHitQuery,
    PresentedRegionKind, Rect, WindowMatrixEntry,
};

/// A destination already chosen by canonical command dispatch. This carries
/// no command implementation and grants no permission to mutate the evaluator.
#[derive(Clone, Debug)]
pub struct ResolvedScrollIntent {
    pub frame: u64,
    pub window: crate::DisplayWindowId,
    pub presentation: crate::PresentationId,
    pub epoch: u64,
    pub offset: f32,
    pub inputs: Vec<crate::input_progress::InputReceipt>,
}

/// Exact row/font transport, owned separately from authoritative window rows.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ScrollCoverage {
    pub epoch: u64,
    #[serde(default)]
    pub predict_pixels: bool,
    #[serde(default)]
    pub compositor_enabled: bool,
    /// Row index whose source body-row number is zero.
    pub anchor_row: usize,
    pub viewport: Rect,
    /// Translate frame Y into nonnegative coverage coordinates.
    pub origin: f32,
    pub content: WindowMatrixEntry,
    pub faces: crate::FrameFaceMap,
    pub fonts: crate::font::ResolvedFontTable,
    pub char_fonts: crate::font::CharFontTable,
    pub shaped_clusters: crate::font::ShapedClusterTable,
    pub hit_index: PresentedHitIndex,
    #[serde(default)]
    pub pointer_source: crate::PresentedPointerSourceMap,
}

/// Materialized once on frame ingestion, shared by render clones.
#[derive(Clone, Debug)]
pub struct ScrollSurface {
    coverage: Arc<ScrollCoverage>,
    glyphs: Vec<FrameGlyph>,
    fringe_bitmaps: rustc_hash::FxHashMap<u16, crate::frame_glyphs::FringeBitmapData>,
}

impl ScrollCoverage {
    /// Refuse malformed, disjoint, foreign-window or unbounded transport.
    /// Only ordinary text and whitespace are admitted by this first source
    /// domain; media and Lisp strings need their own interaction ownership.
    pub fn materialize(self: &Arc<Self>, frame: &FrameDisplayState) -> Option<Arc<ScrollSurface>> {
        let window = self.content.window_id;
        let bounds = self.content.text_clip_bounds?;
        let valid_rect = |rect: Rect| {
            FrameRect::new(rect.x, rect.y, rect.width, rect.height).is_ok()
                && rect.width > 0.0
                && rect.height > 0.0
        };
        if self.epoch == 0
            || self.anchor_row >= self.content.matrix.rows.len()
            || !valid_rect(bounds)
            || !valid_rect(self.viewport)
            || bounds.x != self.viewport.x
            || bounds.width != self.viewport.width
            || !self.origin.is_finite()
            || bounds.y > self.viewport.y + self.origin
            || bounds.bottom() < self.viewport.bottom() + self.origin
            || self.content.matrix.rows.len() > 192
            || self.hit_index.presentation() != frame.presentation_id
            || self.hit_index.regions().len() != 1
            || !self.hit_index.string_positions().is_empty()
            || !self.hit_index.resize_handles().is_empty()
        {
            return None;
        }
        let region = self.hit_index.regions()[0];
        if region.window() != Some(window)
            || region.kind() != PresentedRegionKind::TextBody
            || region.bounds().raw() != bounds
            || self.hit_index.text_positions().len() > 65_536
            || self.hit_index.text_positions().iter().any(|position| {
                let rect = position.bounds().raw();
                position.window() != window
                    || rect.x < bounds.x
                    || rect.right() > bounds.right()
                    || rect.y < bounds.y
                    || rect.bottom() > bounds.bottom()
            })
        {
            return None;
        }
        let mut glyph_count = 0usize;
        let mut bottom = bounds.y;
        let mut next_source = None;
        for row in &self.content.matrix.rows {
            if !row.enabled {
                continue;
            }
            if row.role != GlyphRowRole::Text
                || row.height_px <= 0.0
                || !row.height_px.is_finite()
                || (self.content.text_pixel_bounds.y + row.pixel_y - bottom).abs() > 0.01
                || next_source.is_some_and(|next| row.start_charpos != next)
            {
                return None;
            }
            glyph_count =
                glyph_count.checked_add(row.glyphs.iter().map(Vec::len).sum::<usize>())?;
            if glyph_count > 65_536 {
                return None;
            }
            bottom += row.height_px;
            next_source = row.next_buffer_row_start();
        }
        if (bottom - bounds.bottom()).abs() > 0.01 {
            return None;
        }
        let mut source = FrameDisplayState::new(0, 0, frame.char_width, frame.char_height);
        source.faces = self.faces.clone();
        source.fonts = self.fonts.clone();
        source.char_fonts = self.char_fonts.clone();
        source.shaped_clusters = self.shaped_clusters.clone();
        source.window_matrices.push(self.content.clone());
        source.scroll_bars.extend(
            frame
                .scroll_bars
                .iter()
                .filter(|bar| bar.window_id == window)
                .cloned(),
        );
        let mut glyphs = Vec::with_capacity(glyph_count);
        // Scroll bars supply fringe geometry but remain stationary frame chrome.
        source.for_each_glyph(|glyph| {
            if !matches!(glyph, FrameGlyph::ScrollBar { .. }) {
                glyphs.push(glyph);
            }
        });
        if glyphs.iter().any(|glyph| {
            !matches!(
                glyph,
                FrameGlyph::Char {
                    row_role: GlyphRowRole::Text,
                    ..
                } | FrameGlyph::Stretch {
                    row_role: GlyphRowRole::Text,
                    ..
                } | FrameGlyph::FringeBitmap {
                    row_role: GlyphRowRole::Text,
                    ..
                }
            )
        }) {
            return None;
        }
        let mut fringe_bitmaps = rustc_hash::FxHashMap::default();
        let mut bitmap_bytes = 0usize;
        for glyph in &glyphs {
            if let FrameGlyph::FringeBitmap {
                bitmap_index,
                face_id,
                ..
            } = glyph
            {
                self.faces.get(face_id)?;
                if !fringe_bitmaps.contains_key(bitmap_index) {
                    let bitmap = frame.fringe_bitmaps.get(bitmap_index)?;
                    bitmap_bytes = bitmap_bytes.checked_add(bitmap.bits.len().checked_mul(2)?)?;
                    if bitmap_bytes > 65_536 {
                        return None;
                    }
                    fringe_bitmaps.insert(*bitmap_index, bitmap.clone());
                }
            }
        }
        if !self.pointer_source.is_empty() {
            self.pointer_source
                .validate_scroll_source(
                    window,
                    FrameRect::new(bounds.x, bounds.y, bounds.width, bounds.height).ok()?,
                )
                .ok()?;
            let mut validation = FrameGlyphBuffer::with_size(bounds.right(), bounds.bottom());
            validation.presentation_id = frame.presentation_id;
            validation.faces = self.faces.clone();
            validation.glyphs = glyphs.clone();
            validation
                .install_presented_hit_index(self.hit_index.clone())
                .ok()?;
            validation
                .install_presented_pointer_source_map(&self.pointer_source)
                .ok()?;
        }
        Some(Arc::new(ScrollSurface {
            coverage: Arc::clone(self),
            glyphs,
            fringe_bitmaps,
        }))
    }
}

impl ScrollSurface {
    pub fn coverage(&self) -> &ScrollCoverage {
        &self.coverage
    }

    /// Complete certified body glyphs in coverage coordinates. A retained
    /// rasterizer must use the coverage clip, then crop to the viewport with
    /// the same origin/offset as `paint` and `hit`.
    pub fn coverage_glyphs(&self) -> &[FrameGlyph] {
        &self.glyphs
    }

    /// Positive offsets expose later buffer text. Coverage is a hard limit,
    /// including for a reversed gesture; no blank frontier may be exposed.
    pub fn clamp_offset(&self, offset: f32) -> f32 {
        if !offset.is_finite() {
            return 0.0;
        }
        let viewport = self.coverage.viewport;
        let bounds = self
            .coverage
            .content
            .text_clip_bounds
            .expect("validated coverage");
        offset.clamp(
            bounds.y - viewport.y - self.coverage.origin,
            bounds.bottom() - viewport.bottom() - self.coverage.origin,
        )
    }

    /// Paint and pointer lookup use this exact same offset and viewport.
    pub fn paint(
        &self,
        frame: &mut FrameGlyphBuffer,
        offset: f32,
    ) -> Result<(), crate::PresentedPointerMapError> {
        let offset = self.clamp_offset(offset) + self.coverage.origin;
        let window = self.coverage.content.window_id;
        let clip = self.coverage.viewport;
        let pointer_source = frame.scroll_pointer_source()?.replace_scrolled_body(
            window,
            &self.coverage.pointer_source,
            clip,
            offset,
        )?;
        let original_glyphs = std::mem::take(&mut frame.glyphs);
        frame.glyphs.extend(
            original_glyphs
                .iter()
                .filter(|glyph| {
                    glyph.window_id() != Some(window)
                        || glyph.row_role() != Some(GlyphRowRole::Text)
                        || matches!(
                            glyph,
                            FrameGlyph::ScrollBar { .. } | FrameGlyph::Border { .. }
                        )
                })
                .cloned(),
        );
        for glyph in &self.glyphs {
            let mut glyph = glyph.clone();
            match &mut glyph {
                FrameGlyph::Char {
                    y,
                    baseline,
                    height,
                    clip_rect,
                    ..
                } => {
                    *y -= offset;
                    *baseline -= offset;
                    *clip_rect = Some(clip);
                    if *y >= clip.bottom() || *y + *height <= clip.y {
                        continue;
                    }
                }
                FrameGlyph::Stretch {
                    y,
                    height,
                    clip_rect,
                    ..
                } => {
                    *y -= offset;
                    *clip_rect = Some(clip);
                    if *y >= clip.bottom() || *y + *height <= clip.y {
                        continue;
                    }
                }
                FrameGlyph::FringeBitmap {
                    y,
                    height,
                    clip_rect,
                    ..
                } => {
                    *y -= offset;
                    *clip_rect = Some(Rect::new(
                        self.coverage.content.pixel_bounds.x,
                        clip.y,
                        self.coverage.content.pixel_bounds.width,
                        clip.height,
                    ));
                    if *y >= clip.bottom() || *y + *height <= clip.y {
                        continue;
                    }
                }
                _ => unreachable!("validated text coverage"),
            }
            frame.glyphs.push(glyph);
        }
        frame.fringe_bitmaps.extend(self.fringe_bitmaps.clone());
        frame.faces.extend(self.coverage.faces.clone());
        frame.fonts.extend(self.coverage.fonts.clone());
        frame.char_fonts.extend(self.coverage.char_fonts.clone());
        frame
            .shaped_clusters
            .extend(self.coverage.shaped_clusters.clone());
        if let Err(error) = frame.install_presented_pointer_source_map(&pointer_source) {
            frame.glyphs = original_glyphs;
            return Err(error);
        }
        Ok(())
    }

    /// A point must first belong to this window's visible body. Inverting a
    /// scroll can never fall through to another pane or its mode line.
    pub fn hit(
        &self,
        point: PresentationFramePoint,
        offset: f32,
    ) -> Result<Option<PresentedHit>, PresentedHitError> {
        let viewport = self.coverage.viewport;
        if point.x() < viewport.x
            || point.x() >= viewport.right()
            || point.y() < viewport.y
            || point.y() >= viewport.bottom()
        {
            return Ok(None);
        }
        let mapped = crate::GeometryPoint::<
            crate::interaction_projection::PresentationFrameSpace,
            crate::LogicalPixels,
        >::from_px(
            point.x(),
            point.y() + self.clamp_offset(offset) + self.coverage.origin,
        )
        .expect("validated coverage offset");
        let hit = self.coverage.hit_index.resolve(PresentedHitQuery::new(
            PresentationFramePoint::from_witnessed(point.presentation(), mapped),
        ))?;
        let offset = self.clamp_offset(offset) + self.coverage.origin;
        let first = self
            .coverage
            .content
            .matrix
            .rows
            .iter()
            .position(|row| {
                row.enabled
                    && self.coverage.content.text_pixel_bounds.y + row.pixel_y + row.height_px
                        > viewport.y + offset
            })
            .unwrap_or(0);
        let row_delta = self.coverage.anchor_row as i64 - first as i64;
        Ok(hit.map(|hit| hit.project_scrolled_body(viewport, offset, row_delta)))
    }
}

impl PartialEq for ScrollSurface {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.coverage, &other.coverage)
            || (self.coverage.epoch == other.coverage.epoch
                && self.coverage.compositor_enabled == other.coverage.compositor_enabled
                && self.coverage.predict_pixels == other.coverage.predict_pixels
                && self.coverage.origin == other.coverage.origin
                && self.coverage.anchor_row == other.coverage.anchor_row
                && self.coverage.viewport == other.coverage.viewport
                && self.coverage.content.text_clip_bounds
                    == other.coverage.content.text_clip_bounds
                && self.coverage.hit_index == other.coverage.hit_index
                && self.coverage.pointer_source == other.coverage.pointer_source
                && self.coverage.faces == other.coverage.faces
                && self.coverage.fonts == other.coverage.fonts
                && self.coverage.char_fonts == other.coverage.char_fonts
                && self.coverage.shaped_clusters == other.coverage.shaped_clusters
                && self.fringe_bitmaps == other.fringe_bitmaps
                && self.glyphs == other.glyphs)
    }
}
