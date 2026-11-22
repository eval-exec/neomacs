//! Export certified contiguous rows without publishing speculative query data.

use super::*;
use neomacs_display_protocol::glyph_matrix::{GlyphMatrix, MatrixRow};
use neomacs_display_protocol::scroll_coverage::{ScrollCoverage, ScrollCoverageRenewal};
use neomacs_display_protocol::{FrameDisplayState, GlyphRowRole, Rect};
use neovm_core::window::{PresentedBodyRowSnapshot, WindowDisplaySnapshot};
use std::rc::Rc;
use std::sync::Arc;

const MAX_COVERAGE_ROWS: usize = 192;

/// An isolated process can retain the old exporter as the paired cost control.
/// The setting is immutable for that process, like the point-row controls.
fn coverage_export_reuse_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("NEOMACS_COVERAGE_EXPORT_REUSE")
                .ok()
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref(),
            Some("off" | "0" | "false" | "no")
        )
    })
}

#[derive(Clone, Copy, PartialEq)]
struct ExportPlacement {
    viewport: Rect,
    window: Rect,
    text: Rect,
    clip: Option<Rect>,
    selected: bool,
}

impl ExportPlacement {
    fn new(
        window: DisplayWindowId,
        current: &RetainedWindowMatrix,
        state: &FrameDisplayState,
    ) -> Option<Self> {
        let original = state
            .window_matrices
            .iter()
            .find(|entry| entry.window_id == window)?;
        Some(Self {
            viewport: current.display_snapshot.regions.text_body,
            window: original.pixel_bounds,
            text: original.text_pixel_bounds,
            clip: original.text_clip_bounds,
            selected: original.selected,
        })
    }
}

// A frame can be coalesced before the renderer acquires its surface. Keep
// exporting until activation acknowledges one of the full-surface publishes.
#[derive(Clone, Copy)]
enum ExportReceipt {
    AwaitingActivation { first: u64, last: u64 },
    // Every accepted frame passes through export, which either carries this
    // epoch or revokes/re-keys it. Renderer activation acknowledges a full
    // publish; CoverageRetention keeps its CPU surface across GPU device loss
    // (frame_windows.rs::clear_gpu_resident_state drops GPU history only).
    // An activation older than this receipt must not authorize renewal.
    Active { first: u64 },
}

pub(super) struct CoverageExportInputs {
    key: RetainedWindowKey,
    placement: ExportPlacement,
    pages: Vec<Rc<PreparedViewportPayload>>,
    font_catalog: neomacs_display_protocol::font::FontCatalogGeneration,
    surface: Arc<neomacs_display_protocol::scroll_coverage::ScrollSurface>,
    receipt: ExportReceipt,
}

impl CoverageExportInputs {
    fn new(
        window: DisplayWindowId,
        current: &RetainedWindowMatrix,
        state: &FrameDisplayState,
        pages: &[&PreparedViewport],
        surface: &Arc<neomacs_display_protocol::scroll_coverage::ScrollSurface>,
        receipt: ExportReceipt,
    ) -> Option<Self> {
        Some(Self {
            key: current.key.clone(),
            placement: ExportPlacement::new(window, current, state)?,
            pages: pages.iter().map(|page| Rc::clone(&page.payload)).collect(),
            font_catalog: state.font_catalog_generation,
            surface: Arc::clone(surface),
            receipt,
        })
    }

    fn matches(
        &self,
        window: DisplayWindowId,
        current: &RetainedWindowMatrix,
        state: &FrameDisplayState,
        pages: &[&PreparedViewport],
    ) -> bool {
        // Fault injection for the real exporter regression rows. The product
        // has no switch that can bypass receipt freshness validation.
        #[cfg(test)]
        if std::env::var_os("NEOMACS_TEST_SKIP_COVERAGE_RECEIPT_INPUTS").is_some() {
            return true;
        }
        // Only point may differ. In particular preserve this window's own
        // start, hscroll, vscroll, partition, source and font invalidators.
        let mut key = current.key.clone();
        key.point = self.key.point;
        key == self.key
            && ExportPlacement::new(window, current, state) == Some(self.placement)
            && self.font_catalog == state.font_catalog_generation
            && self.surface.frame_resources_match(state)
            && self.pages.len() == pages.len()
            && self
                .pages
                .iter()
                .zip(pages)
                .all(|(old, page)| Rc::ptr_eq(old, &page.payload))
    }

    fn activated(&mut self, active: Option<neomacs_display_protocol::PresentationId>) -> bool {
        let Some(active) = active else { return false };
        match self.receipt {
            ExportReceipt::AwaitingActivation { first, last }
                if (first..=last).contains(&active.get()) =>
            {
                self.receipt = ExportReceipt::Active { first };
                true
            }
            ExportReceipt::Active { first } => active.get() >= first,
            ExportReceipt::AwaitingActivation { .. } => false,
        }
    }
}

impl PreparedViewports {
    pub(in crate::engine) fn export(
        &mut self,
        frame: neovm_core::window::FrameId,
        retained: &rustc_hash::FxHashMap<DisplayWindowId, RetainedWindowMatrix>,
        stats: &mut LayoutStats,
        active_presentation: Option<neomacs_display_protocol::PresentationId>,
        state: &mut FrameDisplayState,
        arena: &FrameFaceArena,
        metrics: &mut Option<crate::font::metrics::FontMetricsService>,
        window_policy: impl Fn(DisplayWindowId) -> (bool, bool),
    ) {
        let reuse_enabled = coverage_export_reuse_enabled();
        self.retire_invalid_dependencies();
        self.exports
            .retain(|export| export.frame != frame || retained.contains_key(&export.window));
        for (window, current) in retained {
            let index = match self
                .exports
                .iter()
                .position(|export| export.frame == frame && export.window == *window)
            {
                Some(index) => {
                    let export = &mut self.exports[index];
                    if !RetainedWindowKey::row_content_eligible(&export.key, &current.key) {
                        export.key = current.key.clone();
                        export.epoch = next_epoch();
                        export.certified = false;
                        export.last_export = None;
                    }
                    index
                }
                None => {
                    self.exports.push(ExportedCoverage {
                        frame,
                        window: *window,
                        key: current.key.clone(),
                        epoch: next_epoch(),
                        certified: false,
                        last_export: None,
                        backward_start: None,
                        visible_source: text_source_range(current),
                    });
                    self.exports.len() - 1
                }
            };
            self.exports[index].visible_source = text_source_range(current);
            let pages: Vec<_> = self
                .entries
                .iter()
                .filter(|entry| {
                    reuse_enabled && entry.export_eligible(frame, *window, &current.key)
                })
                .collect();
            // GNU xdisp.c:19867-19935 keeps the current matrix for ordinary
            // point motion. Consume THIS window's accepted replay decision;
            // a still sibling or minibuffer cannot certify its body geometry.
            let body_reused = stats.window_classes.get(window) == Some(&LayoutClass::CursorOnly);
            let same_inputs = body_reused
                && self.exports[index]
                    .last_export
                    .as_ref()
                    .is_some_and(|inputs| inputs.matches(*window, current, state, &pages));
            if same_inputs
                && self.exports[index].certified
                && self.exports[index]
                    .last_export
                    .as_mut()
                    .is_some_and(|inputs| inputs.activated(active_presentation))
            {
                let (compositor_enabled, predict_pixels) = window_policy(*window);
                state.scroll_coverage_renewals.push(ScrollCoverageRenewal {
                    window: *window,
                    epoch: self.exports[index].epoch,
                    predict_pixels,
                    compositor_enabled,
                });
                continue;
            }
            *stats.coverage_export_calls.entry(*window).or_default() += 1;
            let coverage = self.export_window(
                frame,
                *window,
                current,
                state,
                arena,
                metrics,
                self.exports[index].epoch,
            );
            self.exports[index].backward_start = coverage
                .as_ref()
                .and_then(|coverage| coverage.content.matrix.rows.first())
                .map(|row| CharPos0::new(row.start_charpos));
            match coverage {
                Some(mut coverage) => {
                    let (compositor_enabled, predict_pixels) = window_policy(*window);
                    self.exports[index].certified = true;
                    coverage.compositor_enabled = compositor_enabled;
                    coverage.predict_pixels = predict_pixels;
                    let coverage = Arc::new(coverage);
                    // Activation alone does not prove coverage admission. A
                    // rejected surface must keep retrying the normal path.
                    let first = match self.exports[index].last_export.as_ref() {
                        Some(CoverageExportInputs {
                            receipt: ExportReceipt::AwaitingActivation { first, .. },
                            ..
                        }) if same_inputs => *first,
                        _ => state.presentation_id.get(),
                    };
                    let surface = if reuse_enabled {
                        state.prepare_scroll_coverage(Arc::clone(&coverage))
                    } else {
                        None
                    };
                    // The isolated cost probe distinguishes entry attempts
                    // from real exports and observes admission's actual full
                    // position-vector construction state. Clean timings keep
                    // this trace target disabled.
                    tracing::trace!(
                        target: "neomacs_layout_engine::coverage_cost",
                        window = window.get(),
                        rows = coverage.content.matrix.rows.len(),
                        admitted = surface.is_some(),
                        text_deferred = coverage.hit_index.text_deferred(),
                        "prepared scroll coverage hit geometry"
                    );
                    self.exports[index].last_export = surface.as_ref().and_then(|surface| {
                        CoverageExportInputs::new(
                            *window,
                            current,
                            state,
                            &pages,
                            surface,
                            ExportReceipt::AwaitingActivation {
                                first,
                                last: state.presentation_id.get(),
                            },
                        )
                    });
                    state.scroll_coverage.push(coverage);
                }
                // A publish that re-exports nothing (cursor-only redisplay,
                // drained worker pages, or an exactly-viewport body) must not
                // read as revocation on the compositor side. Renew the last
                // certified epoch explicitly; policy bits are stamped by the
                // evaluator alongside fresh coverages. Absence of both a
                // surface and a renewal expires the retained coverage.
                None if self.exports[index].certified => {
                    let (compositor_enabled, predict_pixels) = window_policy(*window);
                    self.exports[index].last_export = None;
                    state.scroll_coverage_renewals.push(ScrollCoverageRenewal {
                        window: *window,
                        epoch: self.exports[index].epoch,
                        predict_pixels,
                        compositor_enabled,
                    });
                    tracing::debug!(target: "neomacs_layout_engine::scroll_coverage",
                        window = window.get(), epoch = self.exports[index].epoch,
                        "renewed certified scroll coverage without a fresh surface");
                }
                None => {
                    self.exports[index].last_export = None;
                }
            }
        }
    }

    fn export_window(
        &self,
        frame: neovm_core::window::FrameId,
        window: DisplayWindowId,
        current: &RetainedWindowMatrix,
        state: &FrameDisplayState,
        arena: &FrameFaceArena,
        metrics: &mut Option<crate::font::metrics::FontMetricsService>,
        epoch: u64,
    ) -> Option<ScrollCoverage> {
        tracing::trace!(target: "neomacs_layout_engine::coverage_cost",
            window = window.get(), start = current.key.window_start,
            hscroll = current.key.hscroll, vscroll = current.key.vscroll,
            epoch, "building scroll coverage");
        let entries: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| entry.export_eligible(frame, window, &current.key))
            .collect();
        // Build an extra scroll surface only when it adds prepared content.
        // A current-only surface repeats resource and hit-index work on every
        // frame for just the clipped remainder of the visible bottom row.
        if entries.is_empty() {
            return None;
        }
        let original = state
            .window_matrices
            .iter()
            .find(|entry| entry.window_id == window)?;
        let body: Vec<_> = current
            .matrix
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.enabled && row.role == GlyphRowRole::Text)
            .collect();
        let (_, anchor) = *body.first()?;
        let mut rows = std::collections::BTreeMap::new();
        let mut attempt = arena.begin_attempt();
        attempt
            .admit_prepared(
                body.iter().flat_map(|(_, row)| scroll_row_faces(row)),
                &arena.prepared_snapshot(),
                arena,
            )
            .ok()?;
        // Complete physical rows and certified visual continuations may extend this surface.
        // Source continuity is proved again after selecting the connected set.
        for (index, row) in &body {
            if rows.insert(row.start_charpos, (current, *index)).is_some() {
                return None;
            }
        }
        for entry in entries {
            attempt
                .admit_prepared(
                    entry
                        .retained
                        .matrix
                        .rows
                        .iter()
                        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
                        .flat_map(|row| scroll_row_faces(row)),
                    &entry.faces,
                    arena,
                )
                .ok()?;
            let prepared: Vec<_> = entry
                .retained
                .matrix
                .rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.enabled && row.role == GlyphRowRole::Text)
                .collect();
            // A viewport can begin inside a physical line, changing tab/wrap
            // context for that paragraph. Keep the accepted body authoritative.
            // Reject the whole conflicting continuation chain; rows after its
            // completed line boundary have independent layout context.
            for paragraph in prepared.split_inclusive(|(_, row)| !row.continued) {
                let conflict = paragraph.iter().find(|(_, row)| {
                    // Different wrap origins need not share any row start.
                    // Reject interval overlaps too: otherwise inserting the
                    // new starts between accepted rows breaks their adjacency.
                    let previous_conflict = rows
                        .range(..=row.start_charpos)
                        .next_back()
                        .is_some_and(|(_, (source, old_index))| {
                            let old = &source.matrix.rows[*old_index];
                            if old.start_charpos == row.start_charpos {
                                old.height_px != row.height_px
                                    || old.end_charpos != row.end_charpos
                                    || old.continued != row.continued
                            } else {
                                old.next_buffer_row_start()
                                    .is_some_and(|end| end > row.start_charpos)
                            }
                        });
                    previous_conflict
                        || rows
                            .range((
                                std::ops::Bound::Excluded(row.start_charpos),
                                std::ops::Bound::Unbounded,
                            ))
                            .next()
                            .is_some_and(|(next, _)| {
                                row.next_buffer_row_start().is_none_or(|end| *next < end)
                            })
                });
                if let Some((_, row)) = conflict {
                    tracing::debug!(target: "neomacs_layout_engine::scroll_coverage",
                        window = window.get(), epoch,
                        body_start = anchor.start_charpos,
                        prepared_start = entry.retained.key.window_start,
                        paragraph_start = ?paragraph.first().map(|(_, row)| row.start_charpos),
                        row_start = row.start_charpos, row_end = row.end_charpos,
                        row_next = ?row.next_buffer_row_start(), row_continued = row.continued,
                        previous = ?rows.range(..=row.start_charpos).next_back().map(|(_, (source, index))| {
                            let row = &source.matrix.rows[*index];
                            (row.start_charpos, row.end_charpos, row.continued, row.height_px)
                        }),
                        next = ?rows.range((std::ops::Bound::Excluded(row.start_charpos), std::ops::Bound::Unbounded)).next().map(|(start, _)| *start),
                        "prepared paragraph conflicts with selected coverage");
                    continue;
                }
                for (index, row) in paragraph {
                    rows.entry(row.start_charpos)
                        .or_insert((&entry.retained, *index));
                }
            }
        }
        let all: Vec<_> = rows.into_values().collect();
        let anchor_index = all.iter().position(|(source, index)| {
            source.matrix.rows[*index].start_charpos == anchor.start_charpos
        })?;
        let adjacent = |left: usize, right: usize| {
            let (a, ai) = all[left];
            let (b, bi) = all[right];
            a.matrix.rows[ai].next_buffer_row_start() == Some(b.matrix.rows[bi].start_charpos)
        };
        let mut begin = anchor_index;
        while begin > 0 && adjacent(begin - 1, begin) {
            begin -= 1;
        }
        if begin > 0 {
            let (left, li) = all[begin - 1];
            let (right, ri) = all[begin];
            tracing::debug!(target: "neomacs_layout_engine::scroll_coverage",
                window = window.get(), epoch, body_start = anchor.start_charpos,
                left_origin = left.key.window_start,
                left_start = left.matrix.rows[li].start_charpos,
                left_end = left.matrix.rows[li].end_charpos,
                left_continued = left.matrix.rows[li].continued,
                left_next = ?left.matrix.rows[li].next_buffer_row_start(),
                right_origin = right.key.window_start,
                right_start = right.matrix.rows[ri].start_charpos,
                "prepared backward coverage is disconnected");
        }
        let mut end = anchor_index + 1;
        while end < all.len() && adjacent(end - 1, end) {
            end += 1;
        }
        if end - begin > MAX_COVERAGE_ROWS {
            // More prepared pages must not revoke an already usable surface.
            // Keep the complete visible body, divide spare rows around it,
            // then use any unfilled allowance on the available side.
            let last_visible = body.last()?.1.start_charpos;
            let visible_end = all.iter().position(|(source, index)| {
                source.matrix.rows[*index].start_charpos == last_visible
            })? + 1;
            let visible_rows = visible_end.checked_sub(anchor_index)?;
            if visible_end > end || visible_rows > MAX_COVERAGE_ROWS {
                return None;
            }
            let above = (MAX_COVERAGE_ROWS - visible_rows) / 2;
            let first = anchor_index.saturating_sub(above).max(begin);
            end = end.min(first + MAX_COVERAGE_ROWS);
            begin = begin.max(end.saturating_sub(MAX_COVERAGE_ROWS));
        }
        let mut y = anchor.pixel_y
            - all[begin..anchor_index]
                .iter()
                .map(|(source, index)| source.matrix.rows[*index].height_px)
                .sum::<f32>();
        let top = original.text_pixel_bounds.y + y;
        let origin = -top;
        let mut matrix = GlyphMatrix::new(end - begin, current.matrix.ncols);
        let mut snapshot = WindowDisplaySnapshot::default();
        snapshot.regions = current.display_snapshot.regions;
        let row_storage = neovm_core::window::display_point_rows_mode().enabled();
        snapshot.point_rows = row_storage.then(neovm_core::window::DisplayPointRows::default);
        // Group points once per source, preserving per-face hit-test heights.
        let mut points = rustc_hash::FxHashMap::<(usize, i64), Vec<_>>::default();
        for (source, _) in &all[begin..end] {
            let identity = *source as *const RetainedWindowMatrix as usize;
            if points.contains_key(&(identity, -1)) {
                continue;
            }
            points.insert((identity, -1), Vec::new());
            if row_storage && source.display_snapshot.point_rows.is_some() {
                continue;
            }
            for point in source.display_snapshot.iter_points() {
                points.entry((identity, point.row)).or_default().push(point);
            }
        }
        for (output_row, (source, index)) in all[begin..end].iter().enumerate() {
            let original_row = &source.matrix.rows[*index];
            let mut row = original_row.as_ref().clone();
            row.pixel_y = y + origin;
            row.cursor_col = None;
            row.cursor_type = None;
            let mut row_snapshot = source
                .display_snapshot
                .rows
                .iter()
                .find(|row| row.row == *index as i64)?
                .clone();
            row_snapshot.row = output_row as i64;
            row_snapshot.y = y.round() as i64;
            snapshot.rows.push(row_snapshot);
            snapshot.body_rows.push(PresentedBodyRowSnapshot {
                output_row: output_row as i64,
                body_row: output_row as i64 - (anchor_index - begin) as i64,
                body_y: (original.text_pixel_bounds.y + y - top).round() as i64,
            });
            let identity = *source as *const RetainedWindowMatrix as usize;
            let frozen = source
                .display_snapshot
                .point_rows
                .as_ref()
                .and_then(|rows| rows.row(*index as i64));
            if let Some(frozen) = frozen.filter(|row| row_storage && row.has_uniform_y()) {
                // The legacy export makes every point's Y equal to this row's
                // Y. Uniform rows can make exactly that change by placement,
                // retaining their immutable point arrays.
                snapshot
                    .point_rows
                    .as_mut()?
                    .rows
                    .push(frozen.replaced_placement(output_row as i64, y.round() as i64, 0));
            } else {
                let source_points: Vec<_> = if row_storage && frozen.is_some() {
                    frozen?.points().collect()
                } else {
                    points
                        .get(&(identity, *index as i64))
                        .cloned()
                        .unwrap_or_default()
                };
                let placed: Vec<_> = source_points
                    .into_iter()
                    .map(|mut point| {
                        point.row = output_row as i64;
                        point.y = y.round() as i64;
                        point
                    })
                    .collect();
                if let Some(rows) = &mut snapshot.point_rows {
                    if !placed.is_empty() {
                        rows.rows
                            .push(neovm_core::window::DisplayPointRow::from_points(placed));
                    }
                } else {
                    snapshot.points.extend(placed);
                }
            }
            y += row.height_px;
            matrix.rows[output_row] = MatrixRow::new(row);
        }
        let viewport = current.display_snapshot.regions.text_body;
        let bounds = Rect::new(
            viewport.x,
            top,
            viewport.width,
            original.text_pixel_bounds.y + y - top,
        );
        if top > viewport.y
            || bounds.bottom() < viewport.bottom()
            || (top == viewport.y && bounds.bottom() == viewport.bottom())
        {
            return None;
        }
        let bounds = Rect::new(bounds.x, 0.0, bounds.width, bounds.height);
        snapshot.regions.text_body = bounds;
        let hit_index = crate::presentation::spatial::coverage_hit_index(
            state.presentation_id,
            window,
            Arc::new(snapshot),
            bounds,
        )
        .ok()?;
        let mut content = original.clone();
        content.matrix = matrix;
        content.text_clip_bounds = Some(bounds);
        let mut fonts = FrameDisplayState::new(0, 0, state.char_width, state.char_height);
        fonts.font_catalog_generation = state.font_catalog_generation;
        fonts.font_pixel_size = state.font_pixel_size;
        fonts.faces = attempt.faces();
        fonts.window_matrices.push(content.clone());
        crate::font::metrics::realize_frame_fonts(&mut fonts, metrics);
        let pointer_source =
            crate::presentation::pointer::window_pointer_source_map(&fonts).ok()?;
        Some(ScrollCoverage {
            epoch,
            predict_pixels: false,
            compositor_enabled: false,
            anchor_row: anchor_index - begin,
            viewport,
            origin,
            content,
            faces: fonts.faces,
            fonts: fonts.fonts,
            char_fonts: fonts.char_fonts,
            shaped_clusters: fonts.shaped_clusters,
            hit_index,
            pointer_source,
        })
    }
}

fn next_epoch() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn scroll_row_faces(
    row: &neomacs_display_protocol::GlyphRow,
) -> impl Iterator<Item = neomacs_display_protocol::FaceId> + '_ {
    row.referenced_face_ids().chain(
        [
            row.left_fringe_bitmap,
            row.right_fringe_bitmap,
            row.overlay_arrow_bitmap,
        ]
        .into_iter()
        .flatten()
        .map(|bitmap| bitmap.face_id),
    )
}

#[cfg(test)]
#[path = "tests/export_test.rs"]
mod tests;
