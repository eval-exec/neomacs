//! Source-addressed hover paint follows the same translation as scroll glyphs.
use super::*;

impl PresentedPointerSourceMap {
    pub(crate) fn validate_scroll_source(
        &self,
        window: DisplayWindowId,
        bounds: FrameRect,
    ) -> Result<(), PresentedPointerMapError> {
        if self.regions.len() > 65_536 || self.appearances.len() > 65_536 {
            return Err(PresentedPointerMapError::PaintSpanOutOfRange);
        }
        for region in &self.regions {
            if region.owner
                != Some(PresentedRegionId::new(
                    Some(window),
                    PresentedRegionKind::TextBody,
                ))
                || region.interaction.is_some()
                || !rect_contains_rect(bounds, region.bounds)
            {
                return Err(PresentedPointerMapError::InvalidRegionGeometry);
            }
        }
        let mut count = 0usize;
        for appearance in &self.appearances {
            for span in &appearance.paint_spans {
                count = count
                    .checked_add(span.len as usize)
                    .ok_or(PresentedPointerMapError::PaintSpanOutOfRange)?;
                if count > 65_536
                    || span.len == 0
                    || span.slot.window_id != window
                    || span.row_role != crate::GlyphRowRole::Text
                    || span.kind != PresentedPrimitiveKind::Glyph
                    || !rect_contains_rect(bounds, span.clip)
                {
                    return Err(PresentedPointerMapError::PaintSpanOutOfRange);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn replace_scrolled_body(
        &self,
        window: DisplayWindowId,
        coverage: &Self,
        viewport: Rect,
        offset: f32,
    ) -> Result<Self, PresentedPointerMapError> {
        let owns_body = |region: &PresentedPointerRegion| {
            region.owner.is_some_and(|owner| {
                owner.window() == Some(window) && owner.kind() == PresentedRegionKind::TextBody
            })
        };
        let mut result = self.clone();
        result.regions.retain(|region| !owns_body(region));
        for appearance in &mut result.appearances {
            appearance.paint_spans.retain(|span| {
                span.slot.window_id != window || span.row_role != crate::GlyphRowRole::Text
            });
        }
        let translate = |rect: FrameRect| {
            let x = rect.x().max(viewport.x);
            let y = (rect.y() - offset).max(viewport.y);
            let right = (rect.x() + rect.width()).min(viewport.right());
            let bottom = (rect.bottom() - offset).min(viewport.bottom());
            (right > x && bottom > y)
                .then(|| FrameRect::new(x, y, right - x, bottom - y).ok())
                .flatten()
        };
        let mut moved = coverage.clone();
        for region in &moved.regions {
            if !owns_body(region) {
                return Err(PresentedPointerMapError::InvalidRegionGeometry);
            }
        }
        moved.regions = moved
            .regions
            .into_iter()
            .filter_map(|mut region| {
                region.bounds = translate(region.bounds)?;
                Some(region)
            })
            .collect();
        for appearance in &mut moved.appearances {
            if appearance.paint_spans.iter().any(|span| {
                span.slot.window_id != window || span.row_role != crate::GlyphRowRole::Text
            }) {
                return Err(PresentedPointerMapError::PrimitiveKindMismatch);
            }
            appearance.paint_spans = appearance
                .paint_spans
                .drain(..)
                .filter_map(|mut span| {
                    span.clip = translate(span.clip)?;
                    Some(span)
                })
                .collect();
        }
        result.compact_scroll_appearances()?;
        moved.compact_scroll_appearances()?;
        result.append(moved)?;
        Ok(result)
    }

    fn compact_scroll_appearances(&mut self) -> Result<(), PresentedPointerMapError> {
        let mut remap = std::collections::HashMap::new();
        let mut appearances = Vec::new();
        for region in &mut self.regions {
            if let Some(id) = region.appearance {
                let appearance = self
                    .appearances
                    .get(id.get() as usize)
                    .ok_or(PresentedPointerMapError::UnknownAppearance(id))?;
                if appearance.paint_spans.is_empty() {
                    region.appearance = None;
                    continue;
                }
                let next = *remap.entry(id).or_insert_with(|| {
                    let next = PointerAppearanceId::try_from(appearances.len())
                        .expect("bounded source map");
                    appearances.push(appearance.clone());
                    next
                });
                region.appearance = Some(next);
            }
        }
        self.regions
            .retain(|region| region.appearance.is_some() || region.interaction.is_some());
        self.appearances = appearances;
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/scroll_test.rs"]
mod tests;
