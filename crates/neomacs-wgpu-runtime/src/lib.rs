//! Portable wgpu surface lifecycle shared by Neomacs product adapters.

#![forbid(unsafe_code)]

mod surface;
mod surface_frame;
mod winit_input;

pub use surface::{
    PresentationOutcome, PresentationSkipReason, SurfaceClearColor, SurfaceExtent,
    SurfaceInitError, SurfacePresentError, SurfaceRuntime, SurfaceWindow,
};
pub use surface_frame::{
    SurfaceCursorVisibility, SurfaceFrameInitError, SurfaceFramePresentError, SurfaceFrameRenderer,
    SurfaceScaleError,
};
pub use winit_input::WinitFrontendInput;
