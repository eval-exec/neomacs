//! Cross-platform GPU-resident video playback.
//!
//! The public interface deliberately contains no GStreamer, CoreVideo,
//! Media Foundation, DMA-BUF, Metal, or Direct3D types. Platform-native frame
//! ownership and import stay behind the private platform seam.

mod backend;
mod clock;
mod color;
mod mailbox;
mod model;
mod platform;
mod sampling;
mod surface_pool;
mod system;

pub(crate) use model::FrameTiming;
pub use model::{
    BiPlanarVideoFormat, FrameImportPolicy, InitialPlayback, LoopMode, MediaTime,
    MissingVideoPlugin, MissingVideoPlugins, PackedVideoFormat, PixelAspectRatio, PixelRect,
    PlaybackAction, PlaybackEpoch, PlaybackRate, PresentationVisibility, VideoChromaLocation,
    VideoColorPrimaries, VideoColorRange, VideoColorimetry, VideoCommand, VideoCommandError,
    VideoCompositorImport, VideoDecodeBackend, VideoDecodeResidency, VideoDecoderIdentity,
    VideoDecoderKind, VideoDiagnostics, VideoEvent, VideoFrameFormat, VideoFrameLayoutError,
    VideoFramePath, VideoFrameReady, VideoGeometry, VideoGpuTiming, VideoGpuTimingStatus,
    VideoGraphicsBackend, VideoImportCounts, VideoInitError, VideoInstallerHint,
    VideoMatrixCoefficients, VideoModelError, VideoOpenRequest, VideoPlaneFormat,
    VideoPresentationCounts, VideoPresentationPath, VideoPresentationTiming, VideoRecoveryManifest,
    VideoRendererIdentity, VideoRotation, VideoSampleKind, VideoSamplingTransform,
    VideoServiceRequest, VideoServiceResult, VideoServiceTiming, VideoSessionDiagnostics,
    VideoSessionRecovery, VideoSessionState, VideoSource, VideoSurfacePoolDiagnostics,
    VideoSurfacePoolRole, VideoTextureCoordinates, VideoTransferCharacteristic,
};
pub(crate) use sampling::GpuVideoFrame;
#[cfg(target_os = "linux")]
pub use sampling::linux_render_device_numbers;
pub use sampling::{
    GpuGeneration, PreparedVideoDraw, PreparedVideoDraws, VideoCommandEncoder, VideoRenderPass,
    VideoSamplingResources,
};
pub use system::{VideoSystem, VideoWake};

/// Run the native plane-import validation fixture selected by the GUI hardware
/// test target. Missing hardware or validation-layer prerequisites panic loudly.
#[cfg(all(target_os = "linux", feature = "native-validation-fixtures"))]
#[doc(hidden)]
pub fn assert_native_dmabuf_plane_validation(format: BiPlanarVideoFormat) {
    platform::assert_native_dmabuf_plane_validation(format);
}

#[cfg(test)]
#[path = "tests/system_test.rs"]
mod system_test;

#[cfg(test)]
#[path = "tests/surface_pool_test.rs"]
mod surface_pool_test;

#[cfg(test)]
#[path = "tests/model_test.rs"]
mod model_test;

#[cfg(test)]
#[path = "tests/sampling_test.rs"]
mod sampling_test;

#[cfg(test)]
#[path = "tests/color_test.rs"]
mod color_test;

#[cfg(test)]
#[path = "tests/mailbox_test.rs"]
mod mailbox_test;
