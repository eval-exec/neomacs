//! Portable wgpu surface lifecycle shared by Neomacs product adapters.

#![forbid(unsafe_code)]

mod surface;

pub use surface::{
    PresentationOutcome, PresentationSkipReason, SurfaceClearColor, SurfaceExtent,
    SurfaceInitError, SurfacePresentError, SurfaceRuntime, SurfaceWindow,
};
