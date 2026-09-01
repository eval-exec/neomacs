//! Immutable publication boundary for a completed display presentation.
//!
//! Layout builders use [`FrameDisplayState`] while a frame is speculative.
//! GUI and TTY adapters accept this wrapper instead, which proves the frame's
//! revision, placement, window regions, and hit index were validated together
//! and prevents downstream mutation after publication.

use std::ops::Deref;

use crate::{
    DisplayFrameId, FrameDisplayState, FrameGlyphBuffer, PresentationId, PresentedHitError,
    RetainedImageSet,
};

#[derive(Clone, Debug)]
pub struct SealedFramePresentation {
    presentation: PresentationId,
    state: FrameDisplayState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SealFramePresentationError {
    MissingRevision,
    StaleFramePlacement {
        frame: DisplayFrameId,
        expected: PresentationId,
        available: PresentationId,
    },
    Spatial(PresentedHitError),
}

impl From<PresentedHitError> for SealFramePresentationError {
    fn from(error: PresentedHitError) -> Self {
        Self::Spatial(error)
    }
}

impl SealedFramePresentation {
    pub fn seal(state: FrameDisplayState) -> Result<Self, SealFramePresentationError> {
        let presentation = state.presentation_id;
        if presentation == PresentationId::default() {
            return Err(SealFramePresentationError::MissingRevision);
        }
        let placement = state.frame_placement;
        if placement.presentation() != presentation {
            return Err(SealFramePresentationError::StaleFramePlacement {
                frame: placement.frame(),
                expected: presentation,
                available: placement.presentation(),
            });
        }
        state.validate_spatial_projections()?;
        Ok(Self {
            presentation,
            state,
        })
    }

    #[must_use]
    pub const fn presentation(&self) -> PresentationId {
        self.presentation
    }

    #[must_use]
    pub const fn state(&self) -> &FrameDisplayState {
        &self.state
    }

    #[must_use]
    pub fn into_state(self) -> FrameDisplayState {
        self.state
    }

    #[must_use]
    pub fn materialize(&self) -> FrameGlyphBuffer {
        self.state.materialize()
    }

    /// Image identities pinned by this queued presentation.
    #[must_use]
    pub fn referenced_images(&self) -> RetainedImageSet {
        self.state.referenced_images()
    }
}

impl Deref for SealedFramePresentation {
    type Target = FrameDisplayState;

    fn deref(&self) -> &Self::Target {
        self.state()
    }
}
