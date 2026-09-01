//! One-way frame presentation pipeline.
//!
//! Layout may freely assemble a [`FrameDisplayState`] until it is wrapped as a
//! [`ResolvedFrame`].  Composition then consumes that value, derives every
//! spatial projection from the same window snapshots, validates the result,
//! and returns [`SealedFramePresentation`].  The sealed wrapper exposes no
//! mutable transport access, so renderer and TTY adapters observe one revision.

use neomacs_display_protocol::{
    DisplayFrameId, FrameDisplayState, PresentationId, PresentedHitError, PresentedResizeEdge,
    SealFramePresentationError, SealedFramePresentation,
};
use neovm_core::window::WindowPresentationSnapshot;

use crate::display_status_line::TabBarPresentedPointerPlan;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FrameRevision(PresentationId);

impl FrameRevision {
    pub(crate) const fn presentation(self) -> PresentationId {
        self.0
    }
}

#[derive(Debug)]
pub(crate) struct ResolvedFrame {
    revision: FrameRevision,
    transport: FrameDisplayState,
}

impl ResolvedFrame {
    pub(crate) fn new(transport: FrameDisplayState) -> Result<Self, PresentationComposeError> {
        let expected = transport.presentation_id;
        if expected == PresentationId::default() {
            return Err(PresentationComposeError::MissingRevision);
        }
        let placement = transport.frame_placement;
        if placement.presentation() != expected {
            return Err(PresentationComposeError::StaleFramePlacement {
                frame: placement.frame(),
                expected,
                available: placement.presentation(),
            });
        }
        Ok(Self {
            revision: FrameRevision(expected),
            transport,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PresentationComposeError {
    MissingRevision,
    StaleFramePlacement {
        frame: DisplayFrameId,
        expected: PresentationId,
        available: PresentationId,
    },
    Spatial(PresentedHitError),
    Pointer(crate::presentation::pointer::PresentationPointerError),
    Seal(SealFramePresentationError),
}

impl From<PresentedHitError> for PresentationComposeError {
    fn from(error: PresentedHitError) -> Self {
        Self::Spatial(error)
    }
}

impl From<crate::presentation::pointer::PresentationPointerError> for PresentationComposeError {
    fn from(error: crate::presentation::pointer::PresentationPointerError) -> Self {
        Self::Pointer(error)
    }
}

impl From<SealFramePresentationError> for PresentationComposeError {
    fn from(error: SealFramePresentationError) -> Self {
        Self::Seal(error)
    }
}

pub(crate) struct PresentationInputs<'a> {
    window_snapshots: &'a [WindowPresentationSnapshot],
    zero_width_vertical_border_edge: PresentedResizeEdge,
    tab_bar_pointer: Option<TabBarPresentedPointerPlan>,
}

impl<'a> PresentationInputs<'a> {
    pub(crate) const fn new(
        window_snapshots: &'a [WindowPresentationSnapshot],
        zero_width_vertical_border_edge: PresentedResizeEdge,
    ) -> Self {
        Self {
            window_snapshots,
            zero_width_vertical_border_edge,
            tab_bar_pointer: None,
        }
    }

    pub(crate) fn with_tab_bar_pointer(
        mut self,
        pointer: Option<TabBarPresentedPointerPlan>,
    ) -> Self {
        self.tab_bar_pointer = pointer;
        self
    }
}

pub(crate) struct PresentationComposer;

impl PresentationComposer {
    pub(crate) fn compose(
        resolved: ResolvedFrame,
        inputs: PresentationInputs<'_>,
    ) -> Result<SealedFramePresentation, PresentationComposeError> {
        let ResolvedFrame {
            revision,
            mut transport,
        } = resolved;
        let spatial = crate::presentation::spatial::PresentationSpatialPlan::compile(
            &transport,
            inputs.window_snapshots,
            inputs.zero_width_vertical_border_edge,
        )?;
        let pointer = crate::presentation::pointer::PresentationPointerPlan::compile(
            &transport,
            inputs.tab_bar_pointer,
        )?;
        spatial.seal(&mut transport)?;
        pointer.seal(&mut transport);
        let sealed = SealedFramePresentation::seal(transport)?;
        debug_assert_eq!(revision.presentation(), sealed.presentation());
        Ok(sealed)
    }
}

#[cfg(test)]
#[path = "frame_presentation_test.rs"]
mod tests;
