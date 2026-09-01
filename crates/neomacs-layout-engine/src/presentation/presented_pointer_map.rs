//! Production builder for source-addressed pointer presentation metadata.

use std::collections::HashMap;

use neomacs_display_protocol::glyph_matrix::{GlyphPointerAppearance, GlyphPointerSourceIdentity};
use neomacs_display_protocol::{
    DisplaySlotId, DisplayWindowId, FrameRect, GlyphRowRole, PointerAppearanceId, PointerDrawMode,
    PresentedPointerRegion, PresentedPointerSourceAppearance, PresentedPointerSourceMap,
    PresentedPrimitiveKind, PresentedRegionId, PresentedRegionKind, PresentedSourcePaintSpan,
};

/// Stable producer-local identity used by non-buffer pointer plans.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PointerAppearanceRangeId(u64);

impl PointerAppearanceRangeId {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct AppearanceKey {
    window_id: DisplayWindowId,
    source: GlyphPointerSourceIdentity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PresentedPointerMapBuildError {
    ConflictingAppearanceModes,
    TooManyAppearances,
    Protocol(neomacs_display_protocol::PresentedPointerMapError),
}

impl From<neomacs_display_protocol::PresentedPointerMapError> for PresentedPointerMapBuildError {
    fn from(error: neomacs_display_protocol::PresentedPointerMapError) -> Self {
        Self::Protocol(error)
    }
}

impl std::fmt::Display for PresentedPointerMapBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "failed to build presented pointer map: {self:?}")
    }
}

impl std::error::Error for PresentedPointerMapBuildError {}

struct PendingRegion {
    owner: PresentedRegionId,
    bounds: FrameRect,
    appearance_index: usize,
}

struct AppearanceAggregate {
    paint_spans: Vec<PresentedSourcePaintSpan>,
    mode: PointerDrawMode,
}

/// Aggregates finalized row runs into the frame's canonical source map.
pub(crate) struct PresentedPointerMapBuilder {
    positions: HashMap<AppearanceKey, usize>,
    appearances: Vec<AppearanceAggregate>,
    regions: Vec<PendingRegion>,
    error: Option<PresentedPointerMapBuildError>,
}

impl PresentedPointerMapBuilder {
    pub(crate) fn new() -> Self {
        Self {
            positions: HashMap::new(),
            appearances: Vec::new(),
            regions: Vec::new(),
            error: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_glyph_run(
        &mut self,
        window_id: DisplayWindowId,
        row_role: GlyphRowRole,
        row: u32,
        first_col: u16,
        glyph_len: u32,
        appearance_kind: PresentedPrimitiveKind,
        bounds: FrameRect,
        appearance: GlyphPointerAppearance,
    ) {
        if glyph_len == 0 {
            return;
        }
        let key = AppearanceKey {
            window_id,
            source: appearance.source,
        };
        let owner = PresentedRegionId::new(
            (!matches!(row_role, GlyphRowRole::TabBar)).then_some(window_id),
            match row_role {
                GlyphRowRole::Text | GlyphRowRole::Minibuffer => PresentedRegionKind::TextBody,
                GlyphRowRole::TabLine => PresentedRegionKind::TabLine,
                GlyphRowRole::HeaderLine => PresentedRegionKind::HeaderLine,
                GlyphRowRole::ModeLine => PresentedRegionKind::ModeLine,
                GlyphRowRole::TabBar => PresentedRegionKind::TabBar,
            },
        );
        let mode = PointerDrawMode::Face(appearance.face_id);
        let index = if let Some(&index) = self.positions.get(&key) {
            index
        } else {
            let index = self.appearances.len();
            self.positions.insert(key, index);
            self.appearances.push(AppearanceAggregate {
                paint_spans: Vec::new(),
                mode,
            });
            index
        };

        let span = PresentedSourcePaintSpan::new_run(
            appearance_kind,
            row_role,
            DisplaySlotId {
                window_id,
                row,
                col: first_col,
            },
            glyph_len,
            bounds,
        )
        .with_modes(mode, mode);
        let spans = &mut self.appearances[index].paint_spans;
        if let Some(previous) = spans.last()
            && previous.slot().window_id == span.slot().window_id
            && previous.slot().row == span.slot().row
            && u32::from(span.slot().col) < u32::from(previous.slot().col) + previous.len()
            && u32::from(previous.slot().col) < u32::from(span.slot().col) + span.len()
            && (previous.hover() != span.hover() || previous.pressed() != span.pressed())
        {
            self.error
                .get_or_insert(PresentedPointerMapBuildError::ConflictingAppearanceModes);
            return;
        }
        if let Some(previous) = spans.last_mut()
            && previous.kind() == span.kind()
            && previous.row_role() == span.row_role()
            && previous.slot().window_id == span.slot().window_id
            && previous.slot().row == span.slot().row
            && u32::from(previous.slot().col) + previous.len() == u32::from(span.slot().col)
            && previous.hover() == span.hover()
            && previous.pressed() == span.pressed()
            && let Some(combined) = adjacent_rect(previous.clip(), span.clip())
        {
            *previous = PresentedSourcePaintSpan::new_run(
                previous.kind(),
                previous.row_role(),
                previous.slot(),
                previous.len().saturating_add(span.len()),
                combined,
            );
        } else {
            spans.push(span);
        }

        if let Some(previous) = self.regions.last_mut()
            && previous.owner == owner
            && previous.appearance_index == index
            && let Some(combined) = adjacent_rect(previous.bounds, bounds)
        {
            previous.bounds = combined;
        } else {
            self.regions.push(PendingRegion {
                owner,
                bounds,
                appearance_index: index,
            });
        }
    }

    pub(crate) fn finish(self) -> Result<PresentedPointerSourceMap, PresentedPointerMapBuildError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let appearances = self
            .appearances
            .into_iter()
            .map(|appearance| {
                PresentedPointerSourceAppearance::new(
                    appearance.paint_spans,
                    appearance.mode,
                    appearance.mode,
                )
            })
            .collect();
        let regions = self
            .regions
            .into_iter()
            .map(|region| {
                let appearance = PointerAppearanceId::try_from(region.appearance_index)
                    .map_err(|_| PresentedPointerMapBuildError::TooManyAppearances)?;
                Ok::<_, PresentedPointerMapBuildError>(PresentedPointerRegion::new_owned(
                    region.owner,
                    region.bounds,
                    None,
                    Some(appearance),
                ))
            })
            .collect::<Result<Vec<_>, PresentedPointerMapBuildError>>()?;
        Ok(PresentedPointerSourceMap::new(regions, appearances))
    }
}

fn adjacent_rect(left: FrameRect, right: FrameRect) -> Option<FrameRect> {
    if left.y() != right.y()
        || left.height() != right.height()
        || left.x() + left.width() != right.x()
    {
        return None;
    }
    FrameRect::new(
        left.x(),
        left.y(),
        left.width() + right.width(),
        left.height(),
    )
    .ok()
}

#[cfg(test)]
#[path = "presented_pointer_map_test.rs"]
mod tests;
