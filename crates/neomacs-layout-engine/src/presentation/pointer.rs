//! Pointer and hover projections derived while sealing a presentation.
//!
//! Layout rows and frame chrome plans are the sources of truth.  Mutable
//! display builders never publish a pointer map: the presentation composer
//! derives it once, after geometry and rows are final, and installs it together
//! with the other revision-bound spatial projections.

use neomacs_display_protocol::{
    FrameDisplayState, FrameRect, GlyphRowRole, PresentedPointerMapError, PresentedPointerSourceMap,
};

use crate::display_status_line::TabBarPresentedPointerPlan;
use crate::presentation::presented_pointer_map::{
    PresentedPointerMapBuildError, PresentedPointerMapBuilder,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PresentationPointerError {
    Build(PresentedPointerMapBuildError),
    Merge(PresentedPointerMapError),
    MissingTabBarBand,
}

impl From<PresentedPointerMapBuildError> for PresentationPointerError {
    fn from(error: PresentedPointerMapBuildError) -> Self {
        Self::Build(error)
    }
}

impl From<PresentedPointerMapError> for PresentationPointerError {
    fn from(error: PresentedPointerMapError) -> Self {
        Self::Merge(error)
    }
}

pub(crate) struct PresentationPointerPlan {
    source: PresentedPointerSourceMap,
}

impl PresentationPointerPlan {
    pub(crate) fn compile(
        state: &FrameDisplayState,
        tab_bar: Option<TabBarPresentedPointerPlan>,
    ) -> Result<Self, PresentationPointerError> {
        let mut source = window_pointer_source_map(state)?;
        if let Some(tab_bar) = tab_bar {
            let band = state
                .frame_chrome
                .band(neomacs_display_protocol::frame_chrome::FrameChromeKind::TabBar)
                .ok_or(PresentationPointerError::MissingTabBarBand)?;
            let canonical_row = band.canonical_row(state.char_height);
            source.append(tab_bar.into_source_map(band.bounds(), canonical_row)?)?;
        }
        Ok(Self { source })
    }

    pub(crate) fn seal(self, state: &mut FrameDisplayState) {
        state.presented_pointer_source = self.source;
    }
}

/// Derive pointer behavior from finalized window rows.
///
/// This is `pub(crate)` so row-builder tests can assert the derived projection
/// without teaching the mutable builder how to publish presentation metadata.
pub(crate) fn window_pointer_source_map(
    state: &FrameDisplayState,
) -> Result<PresentedPointerSourceMap, PresentedPointerMapBuildError> {
    let mut builder = PresentedPointerMapBuilder::new();
    for entry in &state.window_matrices {
        for (row_index, row) in entry.matrix.rows.iter().enumerate() {
            if !row.enabled {
                continue;
            }
            let row_bounds = entry.row_pixel_bounds(row.role);
            let row_clip = if matches!(row.role, GlyphRowRole::Text | GlyphRowRole::Minibuffer) {
                entry.text_area_clip_rect()
            } else {
                row_bounds
            };
            let row_height = if row.height_px > 0.0 {
                row.height_px
            } else {
                state.char_height
            };
            let y = row_bounds.y
                + if row.height_px > 0.0 {
                    row.pixel_y
                } else {
                    row_index as f32 * state.char_height
                };
            for run in row.pointer_runs() {
                let row_origin = if row.reversed_p {
                    row_bounds.x
                } else {
                    row_bounds.x + row.pixel_x.max(0.0)
                };
                let x = row_origin + run.x;
                let visible_left = x.max(row_clip.x);
                let visible_top = y.max(row_clip.y);
                let visible_right = (x + run.width)
                    .min(row_clip.x + row_clip.width)
                    .min(row_bounds.x + row_bounds.width);
                let visible_bottom = (y + row_height).min(row_clip.y + row_clip.height);
                if let Some(pointer) = row.pointer_appearance(run.appearance).copied()
                    && visible_right > visible_left
                    && visible_bottom > visible_top
                    && let Ok(bounds) = FrameRect::new(
                        visible_left,
                        visible_top,
                        visible_right - visible_left,
                        visible_bottom - visible_top,
                    )
                {
                    builder.observe_glyph_run(
                        entry.window_id,
                        row.role,
                        row_index as u32,
                        u32::from(row.start_col)
                            .saturating_add(run.first_col)
                            .min(u32::from(u16::MAX)) as u16,
                        run.glyph_len,
                        run.kind,
                        bounds,
                        pointer,
                    );
                }
            }
        }
    }
    builder.finish()
}

#[cfg(test)]
#[path = "pointer_test.rs"]
mod tests;
