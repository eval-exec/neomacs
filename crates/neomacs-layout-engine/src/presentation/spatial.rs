use neomacs_display_protocol::frame_chrome::FrameChromeKind;
use neomacs_display_protocol::frame_glyphs::{
    GlyphRowRole, PresentedCellOrigin, PresentedWindowGeometry, PresentedWindowRegions,
};
use neomacs_display_protocol::glyph_matrix::{FrameDisplayState, GlyphArea, GlyphProvenance};
use neomacs_display_protocol::{
    DisplaySlotId, DisplayWindowId, FrameRect, PresentedHitError, PresentedHitIndex,
    PresentedHitRegion, PresentedRegionKind, PresentedResizeAxis, PresentedResizeEdge,
    PresentedResizeHandle, PresentedStringPosition, PresentedTextPosition,
    PresentedWindowChromeArea,
};
use neovm_core::window::{WindowDisplaySnapshot, WindowPresentationSnapshot};

/// All spatial products compiled from one completed redisplay snapshot.
///
/// The plan is deliberately not a transport object: it can only be consumed
/// by [`Self::seal`], which installs window metadata and hit-test geometry as
/// one validated operation.
pub(crate) struct PresentationSpatialPlan {
    windows: Vec<(DisplayWindowId, PresentedWindowGeometry)>,
    hit_index: PresentedHitIndex,
}

impl PresentationSpatialPlan {
    pub(crate) fn compile(
        state: &FrameDisplayState,
        snapshots: &[WindowPresentationSnapshot],
        zero_width_vertical_border_edge: PresentedResizeEdge,
    ) -> Result<Self, PresentedHitError> {
        let mut windows = Vec::new();
        let mut regions = Vec::new();
        let mut resize_handles = Vec::new();
        let mut positions = Vec::new();

        for (window_z, info) in state.window_infos.iter().enumerate() {
            let Some(snapshot) = snapshots
                .iter()
                .map(WindowPresentationSnapshot::display_snapshot)
                .find(|snapshot| snapshot.window_id.0 as i64 == info.window_id.get())
            else {
                continue;
            };
            let cell_origin = PresentedCellOrigin {
                column: snapshot.cell_origin.column().get(),
                line: snapshot.cell_origin.line().get(),
            };
            let geometry = if snapshot.regions_materialized {
                PresentedWindowGeometry::Complete {
                    cell_origin,
                    regions: snapshot.regions,
                }
            } else {
                PresentedWindowGeometry::Skipped {
                    cell_origin,
                    outer: snapshot.regions.outer,
                }
            };
            windows.push((info.window_id, geometry));

            if !snapshot.regions_materialized {
                continue;
            }
            let window_regions = snapshot.regions;
            let base_z = i32::try_from(window_z)
                .unwrap_or(i32::MAX)
                .saturating_mul(100);
            push_zero_width_horizontal_resize_handles(
                &mut resize_handles,
                info.window_id,
                info.is_minibuffer,
                snapshot,
                snapshots,
                state.char_width,
                zero_width_vertical_border_edge,
            )?;
            let window = Some(info.window_id);
            push_region(
                &mut regions,
                window,
                PresentedRegionKind::TextBody,
                window_regions.text_body,
                base_z,
            )?;
            for (kind, rect, priority) in [
                (
                    PresentedRegionKind::LeftMargin,
                    window_regions.left_margin,
                    10,
                ),
                (
                    PresentedRegionKind::RightMargin,
                    window_regions.right_margin,
                    10,
                ),
                (
                    PresentedRegionKind::LeftFringe,
                    window_regions.left_fringe,
                    10,
                ),
                (
                    PresentedRegionKind::RightFringe,
                    window_regions.right_fringe,
                    10,
                ),
                (
                    PresentedRegionKind::LeftScrollBar,
                    window_regions.left_scroll_bar,
                    20,
                ),
                (
                    PresentedRegionKind::RightScrollBar,
                    window_regions.right_scroll_bar,
                    20,
                ),
                (
                    PresentedRegionKind::HorizontalScrollBar,
                    window_regions.horizontal_scroll_bar,
                    20,
                ),
                (PresentedRegionKind::TabLine, window_regions.tab_line, 20),
                (
                    PresentedRegionKind::HeaderLine,
                    window_regions.header_line,
                    20,
                ),
                (PresentedRegionKind::ModeLine, window_regions.mode_line, 20),
                (
                    PresentedRegionKind::RightDivider,
                    window_regions.right_divider,
                    30,
                ),
                (
                    PresentedRegionKind::BottomDivider,
                    window_regions.bottom_divider,
                    30,
                ),
            ] {
                if let Some(rect) = rect {
                    push_region(&mut regions, window, kind, rect, base_z + priority)?;
                }
            }

            // Points are ordered by buffer position, so consecutive points
            // almost always share a row: memoize the last hit in front of
            // the (sorted body_rows) binary search. The previous per-point
            // linear scan was O(points x rows) per window per frame.
            let mut last_body_row: Option<&neovm_core::window::PresentedBodyRowSnapshot> = None;
            for point in &snapshot.points {
                let body_row = match last_body_row.filter(|row| row.output_row == point.row) {
                    Some(row) => row,
                    None => {
                        let row = snapshot.body_row_for_output_row(point.row).ok_or(
                            PresentedHitError::MissingBodyRow {
                                window: info.window_id,
                                output_row: point.row,
                            },
                        )?;
                        last_body_row = Some(row);
                        row
                    }
                };
                let raw_x = window_regions.text_body.x + point.x as f32;
                let raw_y = window_regions.text_body.y + body_row.body_y as f32;
                let left = raw_x.max(window_regions.text_body.x);
                let top = raw_y.max(window_regions.text_body.y);
                let right = (raw_x + point.width.max(1) as f32)
                    .min(window_regions.text_body.x + window_regions.text_body.width);
                let bottom = (raw_y + point.height.max(1) as f32)
                    .min(window_regions.text_body.y + window_regions.text_body.height);
                if right <= left || bottom <= top {
                    continue;
                }
                let bounds = FrameRect::new(left, top, right - left, bottom - top)
                    .map_err(|_| PresentedHitError::InvalidTextPositionGeometry)?;
                positions.push(PresentedTextPosition::new(
                    info.window_id,
                    bounds,
                    point.buffer_pos.as_i64(),
                    body_row.body_row,
                    point.col,
                ));
            }
            push_row_fallback_positions(
                &mut positions,
                info.window_id,
                snapshot,
                window_regions.text_body,
            )?;
        }

        for band in state.frame_chrome.bands() {
            let kind = match band.kind() {
                FrameChromeKind::MenuBar => PresentedRegionKind::MenuBar,
                FrameChromeKind::ToolBar => PresentedRegionKind::ToolBar,
                FrameChromeKind::CompactBar => PresentedRegionKind::CompactBar,
                FrameChromeKind::TabBar => PresentedRegionKind::TabBar,
            };
            regions.push(PresentedHitRegion::new(None, kind, band.bounds(), i32::MAX));
        }

        let string_positions = window_chrome_string_positions(state)?;

        Ok(Self {
            windows,
            hit_index: PresentedHitIndex::from_parts_with_strings(
                state.presentation_id,
                regions,
                positions,
                string_positions,
            )?
            .with_resize_handles(resize_handles)?,
        })
    }

    #[cfg(test)]
    pub(crate) fn hit_index(&self) -> &PresentedHitIndex {
        &self.hit_index
    }

    pub(crate) fn seal(self, state: &mut FrameDisplayState) -> Result<(), PresentedHitError> {
        for (window, geometry) in self.windows {
            let Some(info) = state
                .window_infos
                .iter_mut()
                .find(|info| info.window_id == window)
            else {
                continue;
            };
            info.geometry = geometry;
            match geometry {
                PresentedWindowGeometry::Complete { regions, .. } => {
                    info.bounds = regions.outer;
                    info.tab_line_height = regions.tab_line.map_or(0.0, |rect| rect.height);
                    info.header_line_height = regions.header_line.map_or(0.0, |rect| rect.height);
                    info.mode_line_height = regions.mode_line.map_or(0.0, |rect| rect.height);
                }
                PresentedWindowGeometry::Skipped { outer, .. } => {
                    info.bounds = outer;
                    info.tab_line_height = 0.0;
                    info.header_line_height = 0.0;
                    info.mode_line_height = 0.0;
                }
            }
        }
        state.presented_hit_index = self.hit_index;
        state.validate_spatial_projections()
    }
}

/// Publish GNU's interaction geometry for a zero-pixel vertical divider.
///
/// A vertical scroll bar suppresses the body-side handle, but tab, header,
/// and mode lines retain one at the applicable edge.  A horizontal scroll bar
/// is an interaction band in its own right and splits, rather than joins, the
/// otherwise continuous resize target.
fn push_zero_width_horizontal_resize_handles(
    handles: &mut Vec<PresentedResizeHandle>,
    window: DisplayWindowId,
    is_minibuffer: bool,
    snapshot: &WindowDisplaySnapshot,
    snapshots: &[WindowPresentationSnapshot],
    char_width: f32,
    edge: PresentedResizeEdge,
) -> Result<(), PresentedHitError> {
    let regions = snapshot.regions;
    if is_minibuffer || regions.right_divider.is_some() {
        return Ok(());
    }

    if !has_adjacent_window(snapshot, snapshots, edge) {
        return Ok(());
    }

    let width = char_width.max(1.0).min(regions.outer.width);
    let has_vertical_scroll_bar =
        regions.left_scroll_bar.is_some() || regions.right_scroll_bar.is_some();
    if has_vertical_scroll_bar {
        for line in [regions.tab_line, regions.header_line, regions.mode_line]
            .into_iter()
            .flatten()
        {
            push_horizontal_resize_handle(
                handles,
                window,
                edge,
                regions,
                width,
                line.y,
                line.height,
            )?;
        }
        return Ok(());
    }

    let content_bottom = regions
        .bottom_divider
        .map_or_else(|| regions.outer.bottom(), |divider| divider.y);
    if let Some(scroll_bar) = regions.horizontal_scroll_bar {
        push_horizontal_resize_handle(
            handles,
            window,
            edge,
            regions,
            width,
            regions.outer.y,
            scroll_bar.y - regions.outer.y,
        )?;
        push_horizontal_resize_handle(
            handles,
            window,
            edge,
            regions,
            width,
            scroll_bar.bottom(),
            content_bottom - scroll_bar.bottom(),
        )?;
    } else {
        push_horizontal_resize_handle(
            handles,
            window,
            edge,
            regions,
            width,
            regions.outer.y,
            content_bottom - regions.outer.y,
        )?;
    }
    Ok(())
}

fn push_horizontal_resize_handle(
    handles: &mut Vec<PresentedResizeHandle>,
    window: DisplayWindowId,
    edge: PresentedResizeEdge,
    regions: PresentedWindowRegions,
    width: f32,
    y: f32,
    height: f32,
) -> Result<(), PresentedHitError> {
    if width <= 0.0 || height <= 0.0 {
        return Ok(());
    }
    let x = match edge {
        PresentedResizeEdge::Leading => regions.outer.x,
        PresentedResizeEdge::Trailing => regions.outer.right() - width,
    };
    let bounds = FrameRect::new(x, y, width, height)
        .map_err(|_| PresentedHitError::InvalidResizeHandleGeometry)?;
    handles.push(PresentedResizeHandle::new(
        window,
        PresentedResizeAxis::Horizontal,
        edge,
        bounds,
    ));
    Ok(())
}

fn has_adjacent_window(
    snapshot: &WindowDisplaySnapshot,
    snapshots: &[WindowPresentationSnapshot],
    edge: PresentedResizeEdge,
) -> bool {
    const EDGE_EPSILON: f32 = 0.01;
    let outer = snapshot.regions.outer;
    snapshots
        .iter()
        .map(WindowPresentationSnapshot::display_snapshot)
        .filter(|candidate| {
            candidate.regions_materialized && candidate.window_id != snapshot.window_id
        })
        .any(|candidate| {
            let candidate_outer = candidate.regions.outer;
            let shares_edge = match edge {
                PresentedResizeEdge::Leading => {
                    (candidate_outer.right() - outer.x).abs() <= EDGE_EPSILON
                }
                PresentedResizeEdge::Trailing => {
                    (candidate_outer.x - outer.right()).abs() <= EDGE_EPSILON
                }
            };
            let overlaps_vertically =
                outer.y < candidate_outer.bottom() && candidate_outer.y < outer.bottom();
            shares_edge && overlaps_vertically
        })
}

fn window_chrome_string_positions(
    state: &FrameDisplayState,
) -> Result<Vec<PresentedStringPosition>, PresentedHitError> {
    let mut sources = std::collections::HashMap::new();
    for entry in &state.window_matrices {
        for (row_index, row) in entry.matrix.rows.iter().enumerate() {
            let Some(region) = window_chrome_region(row.role) else {
                continue;
            };
            if !row.enabled {
                continue;
            }
            let mut col = row.start_col;
            for area in GlyphArea::ALL {
                for glyph in &row.glyphs[area.index()] {
                    if glyph.padding {
                        continue;
                    }
                    let slot = DisplaySlotId {
                        window_id: entry.window_id,
                        row: row_index as u32,
                        col,
                    };
                    if let GlyphProvenance::Str { source, index } = glyph.provenance
                        && let Some(source) = row.string_source(source)
                    {
                        sources.insert(slot, (region, source.string(), index as u64));
                    }
                    col = col.saturating_add(glyph.materialized_slot_span());
                }
            }
        }
    }

    let mut positions = Vec::with_capacity(sources.len());
    state.for_each_glyph(|glyph| {
        let Some(slot) = glyph.slot_id() else {
            return;
        };
        let Some(&(region, string, char_index)) = sources.get(&slot) else {
            return;
        };
        let Some(bounds) = glyph.geometry() else {
            return;
        };
        let Ok(bounds) = FrameRect::new(bounds.x, bounds.y, bounds.width, bounds.height) else {
            return;
        };
        positions.push(PresentedStringPosition::new(
            slot.window_id,
            region,
            bounds,
            string,
            char_index,
        ));
    });
    Ok(positions)
}

fn window_chrome_region(role: GlyphRowRole) -> Option<PresentedWindowChromeArea> {
    match role {
        GlyphRowRole::TabLine => Some(PresentedWindowChromeArea::TabLine),
        GlyphRowRole::HeaderLine => Some(PresentedWindowChromeArea::HeaderLine),
        GlyphRowRole::ModeLine => Some(PresentedWindowChromeArea::ModeLine),
        GlyphRowRole::Text | GlyphRowRole::Minibuffer | GlyphRowRole::TabBar => None,
    }
}

/// Fill the source-position gaps that have no glyph rectangle of their own.
///
/// Newlines and empty display rows do not produce glyphs, while the area after
/// the last glyph on a row is still clickable.  These spans make the sealed
/// presentation's hit map total over every visible body row.  Exact glyph
/// positions are installed first and therefore win if a font's ink/advance
/// geometry overlaps one of these fallback spans.
fn push_row_fallback_positions(
    positions: &mut Vec<PresentedTextPosition>,
    window: DisplayWindowId,
    snapshot: &WindowDisplaySnapshot,
    text_body: neomacs_display_protocol::Rect,
) -> Result<(), PresentedHitError> {
    if text_body.width <= 0.0 || text_body.height <= 0.0 {
        return Ok(());
    }
    let body_left = text_body.x;
    let body_right = text_body.x + text_body.width;

    // Group the window's points by output row ONCE. The previous per-row
    // `points.iter().filter(...)` re-scanned every point for every row —
    // O(rows x points) per window per frame.
    let mut points_by_row: std::collections::HashMap<
        i64,
        Vec<&neovm_core::window::DisplayPointSnapshot>,
    > = std::collections::HashMap::new();
    for point in &snapshot.points {
        points_by_row.entry(point.row).or_default().push(point);
    }
    for row_points in points_by_row.values_mut() {
        row_points.sort_by_key(|point| (point.x, point.col, point.buffer_pos));
    }

    for row in &snapshot.rows {
        let Some(row_anchor) = row.start_buffer_pos.or(row.end_buffer_pos) else {
            continue;
        };
        let (body_row, body_y) = snapshot.text_body_position(row.row, row.y);
        let top = (text_body.y + body_y as f32).max(text_body.y);
        let bottom = (text_body.y + body_y as f32 + row.height.max(1) as f32)
            .min(text_body.y + text_body.height);
        if bottom <= top {
            continue;
        }

        let row_points = points_by_row
            .get(&row.row)
            .map(|points| points.as_slice())
            .unwrap_or(&[]);

        let mut covered_right = body_left;
        let mut preceding = None;
        for &point in row_points {
            let point_left = (text_body.x + point.x as f32).clamp(body_left, body_right);
            let point_right = (text_body.x + point.x as f32 + point.width.max(1) as f32)
                .clamp(body_left, body_right);
            if point_left > covered_right {
                let (buffer_position, column) = preceding.map_or(
                    (point.buffer_pos.as_i64(), point.col),
                    |previous: &neovm_core::window::DisplayPointSnapshot| {
                        (previous.buffer_pos.as_i64(), previous.col)
                    },
                );
                push_text_position_span(
                    positions,
                    window,
                    covered_right,
                    top,
                    point_left - covered_right,
                    bottom - top,
                    buffer_position,
                    body_row,
                    column,
                )?;
            }
            covered_right = covered_right.max(point_right);
            preceding = Some(point);
        }

        if covered_right < body_right {
            let (buffer_position, column) = preceding
                .map_or((row_anchor.as_i64(), row.start_col), |point| {
                    (point.buffer_pos.as_i64(), point.col)
                });
            push_text_position_span(
                positions,
                window,
                covered_right,
                top,
                body_right - covered_right,
                bottom - top,
                buffer_position,
                body_row,
                column,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_text_position_span(
    positions: &mut Vec<PresentedTextPosition>,
    window: DisplayWindowId,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    buffer_position: i64,
    row: i64,
    column: i64,
) -> Result<(), PresentedHitError> {
    if width <= 0.0 || height <= 0.0 {
        return Ok(());
    }
    let bounds = FrameRect::new(x, y, width, height)
        .map_err(|_| PresentedHitError::InvalidTextPositionGeometry)?;
    positions.push(PresentedTextPosition::new(
        window,
        bounds,
        buffer_position,
        row,
        column,
    ));
    Ok(())
}

fn push_region(
    regions: &mut Vec<PresentedHitRegion>,
    window: Option<DisplayWindowId>,
    kind: PresentedRegionKind,
    rect: neomacs_display_protocol::Rect,
    z_order: i32,
) -> Result<(), PresentedHitError> {
    if rect.width == 0.0 || rect.height == 0.0 {
        return Ok(());
    }
    let bounds = FrameRect::new(rect.x, rect.y, rect.width, rect.height)
        .map_err(|_| PresentedHitError::InvalidRegionGeometry)?;
    regions.push(PresentedHitRegion::new(window, kind, bounds, z_order));
    Ok(())
}
