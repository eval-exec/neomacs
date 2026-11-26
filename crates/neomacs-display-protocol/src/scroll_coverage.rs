//! Bounded text outside an authoritative viewport. Coverage has its own body
//! hit index; it never extends window-end or the frame's query geometry.

use std::sync::Arc;

use crate::{
    FrameDisplayState, FrameGlyph, FrameGlyphBuffer, FrameRect, GlyphRowRole,
    PresentationFramePoint, PresentedHit, PresentedHitError, PresentedHitIndex, PresentedHitQuery,
    PresentedRegionKind, Rect, WindowMatrixEntry,
};

/// Identifies immutable coverage coordinates, independently of their content
/// lease and logical frame. Proven identical retries may share an identity;
/// growing a prefix may preserve the lease while changing these coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CoveragePublicationId(std::num::NonZeroU64);

impl CoveragePublicationId {
    /// Allocate a process-unique identity for new coordinates. Exhaustion
    /// refuses another export rather than authorizing a foreign offset.
    pub fn allocate() -> Option<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .ok()
        .and_then(std::num::NonZeroU64::new)
        .map(Self)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// The publishing redisplay's assertion that a certified coverage epoch still
/// describes this frame's rows, carried on publishes that re-export no
/// coverage surface. A publish re-exports nothing whenever preparation adds no
/// content (for example a cursor-only redisplay, or once the offscreen worker
/// has drained), so surface presence alone cannot distinguish "still valid"
/// from "revoked". The renewal can: it names the epoch that remains certified
/// plus this publish's projection policy, and the compositor keeps its
/// retained surface valid for exactly this presentation. Silence — neither a
/// surface nor a renewal — expires the retained coverage.
/// A renewal certifies whichever immutable export the renderer retained; it
/// does not claim that the renderer acquired the evaluator's latest export.
/// Exact-coordinate consumers additionally require its publication identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScrollCoverageRenewal {
    pub window: crate::DisplayWindowId,
    pub epoch: u64,
    #[serde(default)]
    pub predict_pixels: bool,
    #[serde(default)]
    pub compositor_enabled: bool,
}

/// A destination already chosen by canonical command dispatch. This carries
/// no command implementation and grants no permission to mutate the evaluator.
#[derive(Clone, Debug)]
pub struct ResolvedScrollIntent {
    pub frame: u64,
    pub window: crate::DisplayWindowId,
    pub presentation: crate::PresentationId,
    pub epoch: u64,
    pub publication: CoveragePublicationId,
    pub offset: f32,
    pub inputs: Vec<crate::input_progress::InputReceipt>,
}

/// Exact row/font transport, owned separately from authoritative window rows.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ScrollCoverage {
    pub epoch: u64,
    pub publication: CoveragePublicationId,
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

/// Complete desired coverage for one logical frame, delivered without a new
/// evaluator presentation. Absence revokes a window even when `base` is stale;
/// installing a surface requires that exact base and its admitted resources.
#[derive(Clone, Debug)]
pub struct AuxiliaryScrollCoverage {
    pub frame: crate::DisplayFrameId,
    pub base: crate::PresentationId,
    pub surfaces: Vec<Arc<ScrollSurface>>,
}

/// Materialized once on frame ingestion, shared by render clones.
#[derive(Clone, Debug)]
pub struct ScrollSurface {
    coverage: Arc<ScrollCoverage>,
    glyphs: Vec<FrameGlyph>,
    fringe_bitmaps: rustc_hash::FxHashMap<u16, crate::frame_glyphs::FringeBitmapData>,
    cell_size: (f32, f32),
    scroll_bars: Vec<crate::glyph_matrix::ScrollBarItem>,
    scroll_bar_glyphs: Vec<FrameGlyph>,
    font_catalog_generation: crate::font::FontCatalogGeneration,
}

#[cfg(test)]
thread_local! {
    static COVERAGE_MATERIALIZE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn coverage_materialize_count() -> usize {
    COVERAGE_MATERIALIZE_COUNT.with(std::cell::Cell::get)
}

/// One admission result for immutable coverage and the frame inputs it read.
#[derive(Clone, Debug)]
pub(crate) enum ScrollCoverageAdmission {
    Ready(Arc<ScrollSurface>),
    Rejected(Box<RejectedCoverageInputs>),
}

#[derive(Clone, Debug)]
pub(crate) struct RejectedCoverageInputs {
    coverage: Arc<ScrollCoverage>,
    presentation: crate::PresentationId,
    cell_size: (f32, f32),
    scroll_bars: Vec<crate::glyph_matrix::ScrollBarItem>,
    // Conservatively retain the whole map: a missing bitmap may become
    // available even though no successful surface identified its resources.
    fringe_bitmaps: rustc_hash::FxHashMap<u16, crate::frame_glyphs::FringeBitmapData>,
}

impl ScrollCoverageAdmission {
    pub(crate) fn prepare(coverage: Arc<ScrollCoverage>, frame: &FrameDisplayState) -> Self {
        if let Some(surface) = coverage.materialize(frame) {
            Self::Ready(surface)
        } else {
            let window = coverage.content.window_id;
            Self::Rejected(Box::new(RejectedCoverageInputs {
                coverage,
                presentation: frame.presentation_id,
                cell_size: (frame.char_width, frame.char_height),
                scroll_bars: frame
                    .scroll_bars
                    .iter()
                    .filter(|bar| bar.window_id == window)
                    .cloned()
                    .collect(),
                fringe_bitmaps: frame.fringe_bitmaps.clone(),
            }))
        }
    }

    pub(crate) fn presentation(&self) -> crate::PresentationId {
        match self {
            Self::Ready(surface) => surface.coverage.hit_index.presentation(),
            Self::Rejected(inputs) => inputs.presentation,
        }
    }

    pub(crate) fn window(&self) -> crate::DisplayWindowId {
        match self {
            Self::Ready(surface) => surface.coverage.content.window_id,
            Self::Rejected(inputs) => inputs.coverage.content.window_id,
        }
    }

    pub(crate) fn matches(
        &self,
        coverage: &Arc<ScrollCoverage>,
        frame: &FrameDisplayState,
    ) -> bool {
        match self {
            Self::Ready(surface) => surface.reusable_for(coverage, frame),
            Self::Rejected(inputs) => {
                Arc::ptr_eq(&inputs.coverage, coverage)
                    && inputs.presentation == frame.presentation_id
                    && inputs.cell_size == (frame.char_width, frame.char_height)
                    && inputs.scroll_bars.iter().eq(frame
                        .scroll_bars
                        .iter()
                        .filter(|bar| bar.window_id == coverage.content.window_id))
                    && inputs.fringe_bitmaps == frame.fringe_bitmaps
            }
        }
    }

    pub(crate) fn surface(&self) -> Option<Arc<ScrollSurface>> {
        match self {
            Self::Ready(surface) => Some(Arc::clone(surface)),
            Self::Rejected(_) => None,
        }
    }
}

impl ScrollCoverage {
    /// Prove every face and resolved font referenced by this coverage is owned
    /// inside its own tables (self-closure) and keyed by its own embedded ID.
    ///
    /// - Every `(id, face)` in `self.faces` must satisfy `face.id == *id`, and
    ///   every `(id, font)` in `self.fonts` must satisfy `font.id == *id`.
    ///   Checking `contains_key` alone is insufficient: `ScrollCoverage` is
    ///   `Serialize + Deserialize` with public maps, so `apply_auxiliary` must
    ///   not trust the producer. Downstream render sinks index `frame_fonts` by
    ///   `font.id` (`GlyphAtlas::install_frame_fonts`) and `frame_char_fonts` /
    ///   `frame_shaped_clusters` by `face.id`; a mismatched key/id entry (e.g.
    ///   `fonts[99] = ResolvedFont { id: 1 }`) would bypass map-key conflict
    ///   checks in `resources_compatible`, flush the GPU glyph atlas, overwrite
    ///   font `1`, or alias another face's tables.
    /// - Every enabled row's `referenced_face_ids()` and every `FaceId` key in
    ///   `char_fonts` / `shaped_clusters` must exist in `self.faces`.
    /// - Every `ResolvedFontId` referenced by `self.faces[*].default_resolved_font_id`,
    ///   `self.char_fonts`, or `self.shaped_clusters` must exist in `self.fonts`.
    pub fn resources_self_closed(&self) -> bool {
        let face_ids_match_keys = self.faces.iter().all(|(id, face)| face.id == *id);
        let font_ids_match_keys = self.fonts.iter().all(|(id, font)| font.id == *id);
        let matrix_faces_closed = self
            .content
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled)
            .all(|row| {
                row.referenced_face_ids()
                    .all(|face_id| self.faces.contains_key(&face_id))
            });
        let table_faces_closed = self
            .char_fonts
            .keys()
            .chain(self.shaped_clusters.keys())
            .all(|face_id| self.faces.contains_key(face_id));
        let face_default_fonts_closed = self
            .faces
            .values()
            .filter_map(|face| face.default_resolved_font_id)
            .all(|font_id| self.fonts.contains_key(&font_id));
        let char_fonts_closed = self.char_fonts.values().all(|chars| {
            chars
                .values()
                .all(|glyph| self.fonts.contains_key(&glyph.resolved_font_id))
        });
        let shaped_clusters_closed = self.shaped_clusters.values().all(|clusters| {
            clusters.values().all(|glyphs| {
                glyphs
                    .iter()
                    .all(|glyph| self.fonts.contains_key(&glyph.resolved_font_id))
            })
        });
        face_ids_match_keys
            && font_ids_match_keys
            && matrix_faces_closed
            && table_faces_closed
            && face_default_fonts_closed
            && char_fonts_closed
            && shaped_clusters_closed
    }

    fn resources_compatible(
        &self,
        faces: &crate::FrameFaceMap,
        fonts: &crate::font::ResolvedFontTable,
        char_fonts: &crate::font::CharFontTable,
        shaped_clusters: &crate::font::ShapedClusterTable,
    ) -> bool {
        self.resources_self_closed()
            && self
                .faces
                .iter()
                .all(|(id, face)| faces.get(id).is_none_or(|base| base == face))
            && self
                .fonts
                .iter()
                .all(|(id, font)| fonts.get(id).is_none_or(|base| base == font))
            && self.char_fonts.iter().all(|(face, chars)| {
                chars.iter().all(|(ch, glyph)| {
                    char_fonts
                        .get(face)
                        .and_then(|table| table.get(ch))
                        .is_none_or(|base| base == glyph)
                })
            })
            && self.shaped_clusters.iter().all(|(face, clusters)| {
                clusters.iter().all(|(text, glyphs)| {
                    shaped_clusters
                        .get(face)
                        .and_then(|table| table.get(text))
                        .is_none_or(|base| base == glyphs)
                })
            })
    }

    /// Auxiliary publication (Aux v2) may introduce self-closed additive
    /// faces/fonts/glyph mappings for offscreen rows, but can never carry
    /// dangling references or conflict with any existing binding in the base
    /// presentation. Nested maps are checked entry by entry.
    pub fn resources_compatible_with_frame_state(&self, frame: &FrameDisplayState) -> bool {
        self.resources_compatible(
            &frame.faces,
            &frame.fonts,
            &frame.char_fonts,
            &frame.shaped_clusters,
        )
    }

    /// Symmetric render-thread check against the active `FrameGlyphBuffer`.
    pub fn resources_compatible_with_frame_buffer(&self, frame: &FrameGlyphBuffer) -> bool {
        self.resources_compatible(
            &frame.faces,
            &frame.fonts,
            &frame.char_fonts,
            &frame.shaped_clusters,
        )
    }

    /// Refuse malformed, disjoint, foreign-window or unbounded transport.
    /// Only ordinary text and whitespace are admitted by this first source
    /// domain; media and Lisp strings need their own interaction ownership.
    pub fn materialize(self: &Arc<Self>, frame: &FrameDisplayState) -> Option<Arc<ScrollSurface>> {
        #[cfg(test)]
        COVERAGE_MATERIALIZE_COUNT.with(|count| count.set(count.get() + 1));
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
            || !self
                .hit_index
                .text_positions_fit_region(window, region.bounds(), 65_536)
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
        let mut scroll_bar_glyphs = Vec::new();
        // Scroll bars supply fringe geometry but remain stationary frame chrome.
        source.for_each_glyph(|glyph| {
            if matches!(glyph, FrameGlyph::ScrollBar { .. }) {
                scroll_bar_glyphs.push(glyph);
            } else {
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
            cell_size: (frame.char_width, frame.char_height),
            scroll_bars: source.scroll_bars,
            scroll_bar_glyphs,
            font_catalog_generation: frame.font_catalog_generation,
        }))
    }
}

impl ScrollSurface {
    fn metric_and_bitmap_resources_match(
        &self,
        cell_size: (f32, f32),
        catalog: crate::font::FontCatalogGeneration,
        bitmaps: &rustc_hash::FxHashMap<u16, crate::frame_glyphs::FringeBitmapData>,
    ) -> bool {
        self.cell_size == cell_size
            && self.font_catalog_generation == catalog
            && self
                .fringe_bitmaps
                .iter()
                .all(|(index, bitmap)| bitmaps.get(index) == Some(bitmap))
    }

    /// Admission's resource footprint, without presentation or source authority.
    pub fn frame_resources_match(&self, frame: &FrameDisplayState) -> bool {
        self.metric_and_bitmap_resources_match(
            (frame.char_width, frame.char_height),
            frame.font_catalog_generation,
            &frame.fringe_bitmaps,
        ) && self.scroll_bars.iter().eq(frame
            .scroll_bars
            .iter()
            .filter(|bar| bar.window_id == self.coverage.content.window_id))
    }

    /// The same resource proof against the renderer's materialized base. Bars
    /// are compared in their materialized form captured by the admission walk.
    pub fn frame_buffer_resources_match(&self, frame: &FrameGlyphBuffer) -> bool {
        self.metric_and_bitmap_resources_match(
            (frame.char_width, frame.char_height),
            frame.font_catalog_generation,
            &frame.fringe_bitmaps,
        ) && self
            .scroll_bar_glyphs
            .iter()
            .eq(frame.glyphs.iter().filter(|glyph| {
                matches!(glyph, FrameGlyph::ScrollBar { window_id, .. }
                if *window_id == self.coverage.content.window_id)
            }))
    }

    /// A per-publication admission also requires the original presentation.
    pub(crate) fn frame_inputs_match(&self, frame: &FrameDisplayState) -> bool {
        self.coverage.hit_index.presentation() == frame.presentation_id
            && self.frame_resources_match(frame)
    }

    fn reusable_for(&self, coverage: &Arc<ScrollCoverage>, frame: &FrameDisplayState) -> bool {
        Arc::ptr_eq(&self.coverage, coverage) && self.frame_inputs_match(frame)
    }

    pub fn coverage(&self) -> &ScrollCoverage {
        &self.coverage
    }

    pub fn coverage_arc(&self) -> &Arc<ScrollCoverage> {
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
        for (index, bitmap) in &self.fringe_bitmaps {
            frame
                .fringe_bitmaps
                .entry(*index)
                .or_insert_with(|| bitmap.clone());
        }
        for (id, face) in &self.coverage.faces {
            frame.faces.entry(*id).or_insert_with(|| face.clone());
        }
        for (id, font) in &self.coverage.fonts {
            frame.fonts.entry(*id).or_insert_with(|| font.clone());
        }
        for (face, chars) in &self.coverage.char_fonts {
            let table = frame.char_fonts.entry(*face).or_default();
            for (ch, glyph) in chars {
                table.entry(*ch).or_insert(*glyph);
            }
        }
        for (face, clusters) in &self.coverage.shaped_clusters {
            let table = frame.shaped_clusters.entry(*face).or_default();
            for (text, glyphs) in clusters {
                if !table.contains_key(text.as_ref()) {
                    table.insert(text.clone(), glyphs.clone());
                }
            }
        }
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
        // The index answers in its own frame of reference: it keeps its
        // original export's stamp even while a renewal keeps the surface
        // certified for later presentations, and the caller has already
        // validated the projection against the presentation being asked about.
        let hit = self.coverage.hit_index.resolve(PresentedHitQuery::new(
            PresentationFramePoint::from_witnessed(self.coverage.hit_index.presentation(), mapped),
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

#[cfg(test)]
mod prepared_surface_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::frame_glyphs::FringeBitmapData;
    use crate::glyph_matrix::{FringeBitmapInfo, ScrollBarItem};
    use crate::{Color, DisplayWindowId, Face, FaceId, Glyph, GlyphMatrix, GlyphRow, MatrixRow};

    pub(super) fn frame() -> FrameDisplayState {
        let mut state = FrameDisplayState::new(10, 2, 10.0, 10.0);
        state.presentation_id = crate::PresentationId::new(1);
        let window = DisplayWindowId::new(1);
        let viewport = Rect::new(0.0, 0.0, 100.0, 20.0);
        let bounds = Rect::new(0.0, 0.0, 100.0, 30.0);
        let mut matrix = GlyphMatrix::new(3, 10);
        for index in 0..3 {
            let mut row = GlyphRow::new(GlyphRowRole::Text);
            row.pixel_y = index as f32 * 10.0;
            row.height_px = 10.0;
            row.ascent_px = 8.0;
            row.start_charpos = index * 2;
            row.end_charpos = index * 2 + 1;
            let mut glyph = Glyph::char('a', FaceId::new(0), index * 2);
            glyph.pixel_width = 10.0;
            row.glyphs[crate::GlyphArea::Text.index()].push(glyph);
            if index == 0 {
                row.right_fringe_bitmap = Some(FringeBitmapInfo {
                    bitmap_index: 25,
                    face_id: FaceId::new(0),
                });
            }
            matrix.rows[index] = MatrixRow::new(row);
        }
        let content = WindowMatrixEntry {
            window_id: window,
            matrix,
            pixel_bounds: Rect::new(0.0, 0.0, 120.0, 20.0),
            text_pixel_bounds: viewport,
            text_clip_bounds: Some(bounds),
            selected: true,
        };
        let mut faces = crate::FrameFaceMap::default();
        faces.insert(FaceId::new(0), Face::new(FaceId::new(0)));
        let hit_index = PresentedHitIndex::from_parts(
            state.presentation_id,
            vec![crate::PresentedHitRegion::new(
                Some(window),
                PresentedRegionKind::TextBody,
                FrameRect::new(0.0, 0.0, 100.0, 30.0).unwrap(),
                0,
            )],
            Vec::new(),
        )
        .unwrap();
        state.fringe_bitmaps.insert(
            25,
            FringeBitmapData {
                bits: vec![0x8000],
                width: 1,
                height: 1,
                period: 0,
                align: 0,
            },
        );
        state.scroll_bars.push(ScrollBarItem {
            window_id: window,
            row_role: GlyphRowRole::Text,
            clip_rect: None,
            horizontal: false,
            x: 110.0,
            y: 0.0,
            width: 10.0,
            height: 20.0,
            position: 0,
            portion: 20,
            whole: 100,
            thumb_start: 0.0,
            thumb_size: 4.0,
            track_color: Color::BLACK,
            thumb_color: Color::WHITE,
        });
        state.scroll_coverage.push(Arc::new(ScrollCoverage {
            epoch: 1,
            publication: CoveragePublicationId::allocate().unwrap(),
            predict_pixels: true,
            compositor_enabled: true,
            anchor_row: 0,
            viewport,
            origin: 0.0,
            content,
            faces,
            fonts: Default::default(),
            char_fonts: Default::default(),
            shaped_clusters: Default::default(),
            hit_index,
            pointer_source: Default::default(),
        }));
        state
    }

    fn prepare(state: &mut FrameDisplayState) -> Arc<ScrollSurface> {
        state
            .prepare_scroll_coverage(Arc::clone(&state.scroll_coverage[0]))
            .unwrap()
    }

    #[derive(Debug)]
    struct AdmissionTextSource {
        positions: Vec<crate::PresentedTextPosition>,
        prove_region: bool,
        flattened: AtomicUsize,
    }

    impl crate::PresentedTextPositionSource for AdmissionTextSource {
        fn text_positions(&self) -> Result<Vec<crate::PresentedTextPosition>, PresentedHitError> {
            self.flattened.fetch_add(1, Ordering::Relaxed);
            Ok(self.positions.clone())
        }

        fn validate_region(
            &self,
            window: DisplayWindowId,
            bounds: FrameRect,
            max_positions: usize,
        ) -> crate::PresentedTextRegionValidation {
            let bounds = bounds.raw();
            if self.prove_region
                && self.positions.len() <= max_positions
                && self.positions.iter().all(|position| {
                    let rect = position.bounds().raw();
                    position.window() == window
                        && FrameRect::new(rect.x, rect.y, rect.width, rect.height).is_ok()
                        && rect.x >= bounds.x
                        && rect.right() <= bounds.right()
                        && rect.y >= bounds.y
                        && rect.bottom() <= bounds.bottom()
                })
            {
                crate::PresentedTextRegionValidation::Validated
            } else {
                crate::PresentedTextRegionValidation::Unsupported
            }
        }
    }

    fn text_position(window: i64, x: f32) -> crate::PresentedTextPosition {
        crate::PresentedTextPosition::new(
            DisplayWindowId::new(window),
            FrameRect::new(x, 0.0, 10.0, 10.0).unwrap(),
            1,
            0,
            0,
        )
    }

    fn deferred_coverage(
        state: &FrameDisplayState,
        positions: Vec<crate::PresentedTextPosition>,
        prove_region: bool,
    ) -> (Arc<ScrollCoverage>, Arc<AdmissionTextSource>) {
        let source = Arc::new(AdmissionTextSource {
            positions,
            prove_region,
            flattened: AtomicUsize::new(0),
        });
        let mut coverage = state.scroll_coverage[0].as_ref().clone();
        coverage.hit_index = coverage.hit_index.with_deferred_text(source.clone());
        (Arc::new(coverage), source)
    }

    #[test]
    fn deferred_scroll_hit_source_stays_cold_through_real_admission() {
        let state = frame();
        let (coverage, source) = deferred_coverage(&state, vec![text_position(1, 0.0)], true);
        assert!(coverage.hit_index.text_deferred());
        let surface = coverage.materialize(&state).unwrap();
        assert!(surface.coverage().hit_index.text_deferred());
        assert_eq!(source.flattened.load(Ordering::Relaxed), 0);

        // Structural equality and transport still observe the exact flat data.
        let mut eager = state.scroll_coverage[0].as_ref().clone();
        eager.hit_index = PresentedHitIndex::from_parts(
            coverage.hit_index.presentation(),
            coverage.hit_index.regions().to_vec(),
            source.positions.clone(),
        )
        .unwrap();
        assert_eq!(coverage.hit_index, eager.hit_index);
        assert_eq!(source.flattened.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn unsupported_scroll_hit_source_keeps_exact_owner_bounds_and_budget_checks() {
        let state = frame();
        for (name, positions, accepted) in [
            ("at budget", vec![text_position(1, 0.0); 65_536], true),
            ("over budget", vec![text_position(1, 0.0); 65_537], false),
            ("foreign owner", vec![text_position(2, 0.0)], false),
            ("outside body", vec![text_position(1, 95.0)], false),
        ] {
            let (coverage, source) = deferred_coverage(&state, positions, false);
            assert_eq!(coverage.materialize(&state).is_some(), accepted, "{name}");
            assert!(!coverage.hit_index.text_deferred(), "{name}");
            assert_eq!(source.flattened.load(Ordering::Relaxed), 1, "{name}");
        }
    }

    #[test]
    fn serialized_scroll_hit_source_loses_validation_proof() {
        let state = frame();
        for (name, positions, accepted) in [
            ("valid", vec![text_position(1, 0.0)], true),
            ("foreign owner", vec![text_position(2, 0.0)], false),
            ("outside body", vec![text_position(1, 95.0)], false),
            ("over budget", vec![text_position(1, 0.0); 65_537], false),
        ] {
            let (coverage, source) = deferred_coverage(&state, positions, true);
            let encoded = serde_json::to_vec(coverage.as_ref()).unwrap();
            let decoded: ScrollCoverage = serde_json::from_slice(&encoded).unwrap();
            assert!(!decoded.hit_index.text_deferred(), "{name}");
            assert_eq!(source.flattened.load(Ordering::Relaxed), 1, "{name}");
            assert_eq!(
                Arc::new(decoded).materialize(&state).is_some(),
                accepted,
                "{name}"
            );
        }
    }

    #[test]
    fn admitted_scroll_surface_is_reused_without_rematerializing() {
        let mut state = frame();
        let surface = prepare(&mut state);
        let rendered = state.materialize();
        assert!(Arc::ptr_eq(&rendered.scroll_surfaces[0], &surface));
        let cloned = state.clone().materialize();
        assert!(Arc::ptr_eq(&cloned.scroll_surfaces[0], &surface));
    }

    #[test]
    fn prepared_surface_rechecks_referenced_bitmap_and_own_scroll_bars() {
        let mut state = frame();
        let surface = prepare(&mut state);
        state.fringe_bitmaps.get_mut(&25).unwrap().bits[0] = 0x4000;
        assert!(!surface.frame_resources_match(&state));
        let rendered = state.materialize();
        assert!(!Arc::ptr_eq(&rendered.scroll_surfaces[0], &surface));
        assert_eq!(
            rendered.scroll_surfaces[0].fringe_bitmaps[&25].bits,
            vec![0x4000]
        );
        state.fringe_bitmaps.remove(&25);
        assert!(state.materialize().scroll_surfaces.is_empty());

        let mut state = frame();
        let surface = prepare(&mut state);
        state.scroll_bars[0].x = 105.0;
        assert!(!Arc::ptr_eq(
            &state.materialize().scroll_surfaces[0],
            &surface
        ));
    }

    #[test]
    fn prepared_surface_rechecks_cell_size_presentation_and_coverage_identity() {
        let mut state = frame();
        let surface = prepare(&mut state);
        state.char_width = 12.0;
        assert!(!Arc::ptr_eq(
            &state.materialize().scroll_surfaces[0],
            &surface
        ));
        state.char_width = 10.0;
        state.char_height = 12.0;
        assert!(!Arc::ptr_eq(
            &state.materialize().scroll_surfaces[0],
            &surface
        ));
        state.char_height = 10.0;
        state.scroll_coverage[0] = Arc::new((*state.scroll_coverage[0]).clone());
        assert!(!Arc::ptr_eq(
            &state.materialize().scroll_surfaces[0],
            &surface
        ));
        Arc::make_mut(&mut state.scroll_coverage[0]).predict_pixels = false;
        let changed = state.materialize();
        assert!(!changed.scroll_surfaces[0].coverage().predict_pixels);
        state.presentation_id = crate::PresentationId::new(2);
        assert!(surface.frame_resources_match(&state));
        assert!(state.materialize().scroll_surfaces.is_empty());
    }

    #[test]
    fn unrelated_resources_preserve_prepared_surface_and_cache_is_not_serialized() {
        let mut state = frame();
        let surface = prepare(&mut state);
        state.fringe_bitmaps.insert(
            99,
            FringeBitmapData {
                bits: vec![0x1000],
                width: 1,
                height: 1,
                period: 0,
                align: 0,
            },
        );
        let mut other_bar = state.scroll_bars[0].clone();
        other_bar.window_id = DisplayWindowId::new(2);
        state.scroll_bars.push(other_bar);
        assert!(surface.frame_resources_match(&state));
        assert!(Arc::ptr_eq(
            &state.materialize().scroll_surfaces[0],
            &surface
        ));
        let serialized = serde_json::to_value(&state).unwrap();
        assert!(serialized.get("prepared_scroll_admissions").is_none());
    }

    #[test]
    fn preparing_coverage_replaces_each_window_and_retires_old_publications() {
        let mut state = frame();
        let first = prepare(&mut state);
        for _ in 0..8 {
            assert!(Arc::ptr_eq(&prepare(&mut state), &first));
        }
        assert_eq!(state.prepared_scroll_admission_count(), 1);

        state.fringe_bitmaps.get_mut(&25).unwrap().bits[0] = 0x4000;
        let replacement = prepare(&mut state);
        assert_eq!(state.prepared_scroll_admission_count(), 1);
        assert!(Arc::ptr_eq(
            &state.materialize().scroll_surfaces[0],
            &replacement
        ));

        let mut second = (*state.scroll_coverage[0]).clone();
        second.content.window_id = DisplayWindowId::new(2);
        second.hit_index = PresentedHitIndex::from_parts(
            state.presentation_id,
            vec![crate::PresentedHitRegion::new(
                Some(DisplayWindowId::new(2)),
                PresentedRegionKind::TextBody,
                FrameRect::new(0.0, 0.0, 100.0, 30.0).unwrap(),
                0,
            )],
            Vec::new(),
        )
        .unwrap();
        let second = Arc::new(second);
        assert!(state.prepare_scroll_coverage(Arc::clone(&second)).is_some());
        state.scroll_coverage.push(second);
        assert_eq!(state.prepared_scroll_admission_count(), 2);

        state.presentation_id = crate::PresentationId::new(2);
        state.scroll_coverage.truncate(1);
        Arc::make_mut(&mut state.scroll_coverage[0]).hit_index = PresentedHitIndex::from_parts(
            state.presentation_id,
            vec![crate::PresentedHitRegion::new(
                Some(DisplayWindowId::new(1)),
                PresentedRegionKind::TextBody,
                FrameRect::new(0.0, 0.0, 100.0, 30.0).unwrap(),
                0,
            )],
            Vec::new(),
        )
        .unwrap();
        let latest = prepare(&mut state);
        assert_eq!(state.prepared_scroll_admission_count(), 1);
        assert!(Arc::ptr_eq(
            &state.materialize().scroll_surfaces[0],
            &latest
        ));
    }

    #[test]
    fn admission_runs_once_for_ready_and_rejected_inputs_then_recovers_after_changes() {
        let mut state = frame();
        let before = coverage_materialize_count();
        let surface = prepare(&mut state);
        assert!(Arc::ptr_eq(&prepare(&mut state), &surface));
        assert!(Arc::ptr_eq(
            &state.materialize().scroll_surfaces[0],
            &surface
        ));
        assert_eq!(coverage_materialize_count() - before, 1);

        let mut state = frame();
        let bitmap = state.fringe_bitmaps.remove(&25).unwrap();
        let coverage = Arc::clone(&state.scroll_coverage[0]);
        let before = coverage_materialize_count();
        assert!(
            state
                .prepare_scroll_coverage(Arc::clone(&coverage))
                .is_none()
        );
        assert!(
            state
                .prepare_scroll_coverage(Arc::clone(&coverage))
                .is_none()
        );
        assert!(state.materialize().scroll_surfaces.is_empty());
        assert_eq!(coverage_materialize_count() - before, 1);

        state.char_width = 12.0;
        assert!(
            state
                .prepare_scroll_coverage(Arc::clone(&coverage))
                .is_none()
        );
        assert!(state.materialize().scroll_surfaces.is_empty());
        assert_eq!(coverage_materialize_count() - before, 2);
        state.scroll_bars[0].x = 105.0;
        assert!(
            state
                .prepare_scroll_coverage(Arc::clone(&coverage))
                .is_none()
        );
        assert_eq!(coverage_materialize_count() - before, 3);

        state.fringe_bitmaps.insert(25, bitmap);
        let recovered = state.prepare_scroll_coverage(coverage).unwrap();
        assert!(Arc::ptr_eq(
            &state.materialize().scroll_surfaces[0],
            &recovered
        ));
        assert_eq!(coverage_materialize_count() - before, 4);
        assert_eq!(state.prepared_scroll_admission_count(), 1);
    }
}

#[cfg(test)]
#[path = "scroll_coverage/tests/auxiliary_resource_test.rs"]
mod auxiliary_resource_tests;
