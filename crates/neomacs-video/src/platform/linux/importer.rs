use crate::backend::{
    CompletedFrameTransfer, DecodedFrame, FrameImportOutcome, FrameImporter, ImportedFrame,
};
use crate::sampling::{GpuVideoContext, PreparedBiPlanarTexture, PreparedSampledTexture};
use crate::surface_pool::{BoundedSurfacePool, SurfacePoolAcquire};
use crate::{GpuVideoFrame, VideoFrameFormat, VideoTransferPath};

use super::dmabuf::{ImportedDmaBufSurface, import_dmabuf};
use super::frame::{DmaBufSurfaceKey, LinuxFrameLease, LinuxFrameStorage, dmabuf_cache_key};

const IMPORTED_SURFACE_CAPACITY: usize = 64;

struct CachedImportedSurface {
    prepared: PreparedLinuxSample,
    imported: ImportedDmaBufSurface,
}

#[derive(Clone)]
enum PreparedLinuxSample {
    Packed(PreparedSampledTexture),
    BiPlanar(PreparedBiPlanarTexture),
}

pub(crate) struct LinuxFrameImporter {
    gpu: GpuVideoContext,
    imported: BoundedSurfacePool<DmaBufSurfaceKey, CachedImportedSurface>,
}

impl LinuxFrameImporter {
    pub(super) fn new(gpu: GpuVideoContext) -> Self {
        Self {
            gpu,
            imported: BoundedSurfacePool::new(IMPORTED_SURFACE_CAPACITY),
        }
    }
}

impl FrameImporter<LinuxFrameLease> for LinuxFrameImporter {
    type Sampled = GpuVideoFrame;

    fn transfer_path(&self, frame: &DecodedFrame<LinuxFrameLease>) -> VideoTransferPath {
        frame.lease.transfer_path
    }

    fn import(
        &mut self,
        frame: DecodedFrame<LinuxFrameLease>,
    ) -> Result<FrameImportOutcome<Self::Sampled>, String> {
        let DecodedFrame {
            lease,
            geometry,
            format,
            colorimetry,
            ..
        } = frame;
        match &lease.storage {
            LinuxFrameStorage::DmaBuf(surface) => {
                let path = lease.transfer_path;
                let key = dmabuf_cache_key(
                    surface,
                    geometry.coded_width,
                    geometry.coded_height,
                    colorimetry,
                )?;
                let cached = match self.imported.acquire(key) {
                    SurfacePoolAcquire::Reused(lease) => lease,
                    SurfacePoolAcquire::Allocate(reservation) => {
                        let (texture, imported) = import_dmabuf(
                            self.gpu.device(),
                            surface,
                            geometry.coded_width,
                            geometry.coded_height,
                        )?;
                        let prepared = match format {
                            VideoFrameFormat::Packed(_) => PreparedLinuxSample::Packed(
                                self.gpu.prepare_texture(
                                    texture,
                                    format
                                        .allocation_bytes(geometry)
                                        .map_err(|error| error.to_string())?,
                                ),
                            ),
                            VideoFrameFormat::BiPlanar420(format) => PreparedLinuxSample::BiPlanar(
                                self.gpu.prepare_multi_planar_texture(
                                    texture,
                                    format,
                                    colorimetry,
                                    geometry,
                                )?,
                            ),
                        };
                        reservation.fulfill(CachedImportedSurface { prepared, imported })
                    }
                    SurfacePoolAcquire::Backpressured => {
                        return Ok(FrameImportOutcome::Backpressured);
                    }
                };
                cached
                    .value()
                    .imported
                    .acquire(self.gpu.device(), self.gpu.queue())?;
                let prepared = cached.value().prepared.clone();
                let release = cached.value().imported.release();
                let sampled = match prepared {
                    PreparedLinuxSample::Packed(prepared) => {
                        self.gpu.wrap_prepared_texture_with_release(
                            geometry,
                            prepared,
                            release,
                            (lease, cached),
                        )
                    }
                    PreparedLinuxSample::BiPlanar(prepared) => {
                        self.gpu.wrap_prepared_bi_planar_texture_with_release(
                            geometry,
                            prepared,
                            release,
                            (lease, cached),
                        )
                    }
                };
                let transfer = match path {
                    VideoTransferPath::DirectExternalSurface => {
                        CompletedFrameTransfer::DirectExternalSurface
                    }
                    VideoTransferPath::GpuInteropCopy => CompletedFrameTransfer::GpuInteropCopy {
                        // A decoder-side conversion may have happened, but
                        // its byte volume is not exposed through this ABI.
                        reported_bytes: None,
                    },
                    VideoTransferPath::CpuUpload => {
                        unreachable!("a DMA-BUF surface cannot be classified as a CPU upload")
                    }
                };
                Ok(FrameImportOutcome::Ready(ImportedFrame {
                    sampled,
                    transfer,
                }))
            }
            LinuxFrameStorage::CpuPacked(surface) => {
                let sampled =
                    self.gpu
                        .upload_rgba(geometry, format, &surface.bytes, surface.stride)?;
                Ok(FrameImportOutcome::Ready(ImportedFrame {
                    sampled,
                    transfer: CompletedFrameTransfer::CpuUpload {
                        bytes: u64::from(surface.stride)
                            .saturating_mul(u64::from(geometry.coded_height)),
                    },
                }))
            }
        }
    }
}
