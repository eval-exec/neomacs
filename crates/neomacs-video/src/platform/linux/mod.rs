mod decoder;
mod dmabuf;
mod frame;
mod importer;

use crate::backend::{Platform, ProductionPlatform};
use crate::sampling::GpuVideoContext;
use crate::{FrameTransferPolicy, GpuVideoFrame, VideoDecodeBackend, VideoInitError, VideoWake};

use decoder::{GstreamerDecoder, NativeVideoFormatSupport};
use frame::LinuxFrameLease;
use importer::LinuxFrameImporter;

pub(crate) struct LinuxPlatform;

impl Platform for LinuxPlatform {
    const BACKEND: VideoDecodeBackend = VideoDecodeBackend::GStreamer;
    type Frame = LinuxFrameLease;
    type Sampled = GpuVideoFrame;
    type Decoder = GstreamerDecoder;
    type Importer = LinuxFrameImporter;
}

impl ProductionPlatform for LinuxPlatform {
    fn create(
        gpu: GpuVideoContext,
        policy: FrameTransferPolicy,
        wake: VideoWake,
    ) -> Result<(Self::Decoder, Self::Importer), VideoInitError> {
        let renderer_drm_device = gpu.linux_render_device();
        let renderer_features = gpu.device().features();
        let native_formats = NativeVideoFormatSupport::new(
            renderer_features.contains(wgpu::Features::TEXTURE_FORMAT_NV12),
            renderer_features.contains(wgpu::Features::TEXTURE_FORMAT_P010),
        );
        let decoder = GstreamerDecoder::new(wake, policy, renderer_drm_device, native_formats)
            .map_err(|message| VideoInitError::Backend {
                backend: VideoDecodeBackend::GStreamer,
                message,
            })?;
        Ok((decoder, LinuxFrameImporter::new(gpu)))
    }
}
