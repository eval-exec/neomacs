//! Portable wgpu surface lifecycle shared by Neomacs product adapters.

#![forbid(unsafe_code)]

mod surface;
mod surface_frame;

pub use surface::{
    PresentationOutcome, PresentationSkipReason, SurfaceClearColor, SurfaceExtent,
    SurfaceInitError, SurfacePresentError, SurfaceRuntime, SurfaceWindow,
};
pub use surface_frame::{
    SurfaceFrameInitError, SurfaceFramePresentError, SurfaceFrameRenderer, SurfaceScaleError,
};
