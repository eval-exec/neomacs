//! Where a surface point lands in the presentation the compositor is drawing.
//!
//! While panes are settled these are the same place, and every hit test can
//! take the pointer's frame-local coordinates at face value. During a layout
//! morph they are not: a pane on its way from one rect to another shows its
//! destination content at an offset, so the pixel under the pointer belongs to
//! a different position in the destination presentation than the raw
//! coordinates say. Hit testing with the raw point during a `split-window`
//! would select the wrong window, or the right window at the wrong buffer
//! position, for as long as the motion lasts.
//!
//! This module makes that impossible to get wrong by construction. A
//! [`PresentedHitQuery`](crate::presented_pointer::PresentedHitQuery) can only
//! be built from a [`PresentationFramePoint`], and a `PresentationFramePoint`
//! can only be built by mapping a surface point through a witness — a
//! [`PresentMapping`](crate::present_mapping::PresentMapping) or an
//! [`InteractionProjection`]. There is no constructor that takes two `f32`s,
//! so no call site can quietly skip the transform.
//!
//! # Why translation and a clip, not a scale
//!
//! A moving pane is not a scaled pane. Its glyphs keep their size throughout —
//! scaling text mid-motion would resample it and read as a zoom, which is not
//! what splitting a window does. What actually changes is where the pane's
//! content is drawn and how much of it is visible: the content translates and
//! the clip rectangle animates. So a [`PaneProjection`] is a translation plus a
//! clip, which is both the honest description and the reason the inverse is
//! exact rather than approximate.

use crate::frame_chrome::PresentationId;
use crate::geometry::{
    GeometryError, GeometryPoint, GeometryRect, LogicalPixels, RootSurfaceSpace, SpaceTranslation,
};
use crate::types::LiveDisplayWindowId;

/// The coordinate space of one presentation's frame.
///
/// Which presentation is a runtime value, not a type parameter: the compositor
/// holds one presentation at a time and the answer must name it, so it rides in
/// [`PresentationFramePoint`] rather than in this marker.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PresentationFrameSpace {}

/// The translation from root-surface coordinates into a presentation's frame.
pub type SurfaceToPresentation =
    SpaceTranslation<RootSurfaceSpace, PresentationFrameSpace, LogicalPixels>;

/// A point known to be in the frame space of `presentation`.
///
/// Only [`InteractionProjection`] and `PresentMapping` produce one, and both
/// produce it by applying the same transform the frame was drawn with. A point
/// that reached here therefore agrees with the pixels, which is the whole
/// property hit testing needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentationFramePoint {
    presentation: PresentationId,
    point: GeometryPoint<PresentationFrameSpace, LogicalPixels>,
}

impl PresentationFramePoint {
    /// The witnessed point, for a caller that already holds the transform.
    #[must_use]
    pub(crate) const fn from_witnessed(
        presentation: PresentationId,
        point: GeometryPoint<PresentationFrameSpace, LogicalPixels>,
    ) -> Self {
        Self {
            presentation,
            point,
        }
    }

    #[must_use]
    pub const fn presentation(self) -> PresentationId {
        self.presentation
    }

    #[must_use]
    pub fn x(self) -> f32 {
        self.point.x()
    }

    #[must_use]
    pub fn y(self) -> f32 {
        self.point.y()
    }
}

/// One pane's contribution to a composition's interaction transform.
///
/// `translation` carries a surface point into the destination presentation's
/// frame; `clip` is the pane's rect on the surface right now, which is what
/// decides whether the point belongs to this pane at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneProjection {
    window: LiveDisplayWindowId,
    translation: SurfaceToPresentation,
    clip: GeometryRect<RootSurfaceSpace, LogicalPixels>,
}

impl PaneProjection {
    /// A pane drawn at `clip` whose content came from `content_origin` in the
    /// destination presentation.
    pub fn new(
        window: LiveDisplayWindowId,
        clip: GeometryRect<RootSurfaceSpace, LogicalPixels>,
        content_origin: GeometryPoint<PresentationFrameSpace, LogicalPixels>,
    ) -> Result<Self, GeometryError> {
        // The pane's top-left on the surface shows `content_origin` of the
        // destination, so a surface point maps by the difference between them.
        let translation = SurfaceToPresentation::from_px(
            content_origin.x() - clip.x(),
            content_origin.y() - clip.y(),
        )?;
        Ok(Self {
            window,
            translation,
            clip,
        })
    }

    #[must_use]
    pub const fn window(self) -> LiveDisplayWindowId {
        self.window
    }

    #[must_use]
    pub const fn clip(self) -> GeometryRect<RootSurfaceSpace, LogicalPixels> {
        self.clip
    }

    /// Whether `point` falls inside this pane as it is drawn right now.
    fn contains(self, point: GeometryPoint<RootSurfaceSpace, LogicalPixels>) -> bool {
        point.x() >= self.clip.x()
            && point.y() >= self.clip.y()
            && point.x() < self.clip.x() + self.clip.width()
            && point.y() < self.clip.y() + self.clip.height()
    }
}

/// Every pane's transform for one composition, immutable once built.
///
/// Built by the compositor from the same sample it renders with, so the
/// projection and the pixels cannot disagree.
#[derive(Clone, Debug, PartialEq)]
pub struct InteractionProjection {
    presentation: PresentationId,
    /// Sorted by destination z-order: the first pane containing a point wins.
    panes: Vec<PaneProjection>,
}

impl InteractionProjection {
    #[must_use]
    pub fn new(presentation: PresentationId, panes: Vec<PaneProjection>) -> Self {
        Self {
            presentation,
            panes,
        }
    }

    /// The projection for a frame whose panes are where they belong.
    ///
    /// With nothing in motion a surface point is already a frame point, so this
    /// carries no panes and maps by identity. It is the ordinary case, not a
    /// fallback: most frames are composed while settled.
    #[must_use]
    pub fn settled(presentation: PresentationId) -> Self {
        Self {
            presentation,
            panes: Vec::new(),
        }
    }

    #[must_use]
    pub const fn presentation(&self) -> PresentationId {
        self.presentation
    }

    #[must_use]
    pub fn panes(&self) -> &[PaneProjection] {
        &self.panes
    }

    /// Where `point` lands in the destination presentation.
    ///
    /// Returns `None` only when a pane's transform would put the point outside
    /// representable geometry. A point in no moving pane maps by identity: the
    /// area outside every morphing pane is drawn straight from the destination.
    #[must_use]
    pub fn map(
        &self,
        point: GeometryPoint<RootSurfaceSpace, LogicalPixels>,
    ) -> Option<PresentationFramePoint> {
        let mapped = match self.panes.iter().find(|pane| pane.contains(point)) {
            Some(pane) => pane.translation.map_point(&point).ok()?,
            None => GeometryPoint::<PresentationFrameSpace, LogicalPixels>::from_px(
                point.x(),
                point.y(),
            )
            .ok()?,
        };
        Some(PresentationFramePoint::from_witnessed(
            self.presentation,
            mapped,
        ))
    }
}

#[cfg(test)]
#[path = "interaction_projection_test.rs"]
mod tests;
