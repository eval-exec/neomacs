use crate::backend::{
    CompletedFrameImport, DecodedFrame, DecoderOutputRejection, FrameImportOutcome, FrameImporter,
    ImportedFrame,
};
use crate::sampling::{GpuVideoContext, PreparedBiPlanarTexture, PreparedSampledTexture};
use crate::surface_pool::{BoundedSurfacePool, SurfacePoolAcquire};
use crate::{GpuVideoFrame, VideoCompositorImport, VideoFrameFormat};

use super::dmabuf::{ImportedDmaBufSurface, import_dmabuf, validate_surface_layout};
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

    fn compositor_import(&self, frame: &DecodedFrame<LinuxFrameLease>) -> VideoCompositorImport {
        match &frame.lease.storage {
            LinuxFrameStorage::DmaBuf(_) => VideoCompositorImport::BorrowedNativeSurface,
            LinuxFrameStorage::CpuPacked(_) => VideoCompositorImport::CpuUpload,
        }
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
            output_generation,
            ..
        } = frame;
        match &lease.storage {
            LinuxFrameStorage::DmaBuf(surface) => {
                // Validate before cache reuse as well as before raw import:
                // a decoder's declared format must match the actual surface.
                let allocation_bytes =
                    match validate_surface_layout(surface.fourcc, geometry, format) {
                        Ok(bytes) => bytes,
                        Err(reason) => {
                            return Ok(FrameImportOutcome::ReconfigureDecoder {
                                rejection: DecoderOutputRejection {
                                    generation: output_generation,
                                    format,
                                    reason,
                                },
                            });
                        }
                    };
                let key = dmabuf_cache_key(
                    surface,
                    geometry.coded_width,
                    geometry.coded_height,
                    colorimetry,
                )?;
                let cached = match self.imported.acquire(key) {
                    SurfacePoolAcquire::Reused(lease) => lease,
                    SurfacePoolAcquire::Allocate(reservation) => {
                        let (texture, imported) = match import_dmabuf(
                            self.gpu.device(),
                            self.gpu.queue(),
                            surface,
                            geometry,
                            format,
                        ) {
                            Ok(imported) => imported,
                            Err(reason) => {
                                return Ok(FrameImportOutcome::ReconfigureDecoder {
                                    rejection: DecoderOutputRejection {
                                        generation: output_generation,
                                        format,
                                        reason,
                                    },
                                });
                            }
                        };
                        let prepared = match format {
                            VideoFrameFormat::Packed(_) => PreparedLinuxSample::Packed(
                                self.gpu.prepare_texture(texture, allocation_bytes),
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
                cached.value().imported.acquire()?;
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
                Ok(FrameImportOutcome::Ready(ImportedFrame {
                    sampled,
                    completed_import: CompletedFrameImport::BorrowedNativeSurface,
                }))
            }
            LinuxFrameStorage::CpuPacked(surface) => {
                let sampled =
                    self.gpu
                        .upload_rgba(geometry, format, &surface.bytes, surface.stride)?;
                Ok(FrameImportOutcome::Ready(ImportedFrame {
                    sampled,
                    completed_import: CompletedFrameImport::CpuUpload {
                        bytes: u64::from(surface.stride)
                            .saturating_mul(u64::from(geometry.coded_height)),
                    },
                }))
            }
        }
    }

    fn surface_pool_diagnostics(&self) -> Option<crate::VideoSurfacePoolDiagnostics> {
        Some(
            self.imported
                .diagnostics(crate::VideoSurfacePoolRole::CompositorImport),
        )
    }

    fn begin_measurement_epoch(&mut self) {
        self.imported.begin_measurement_epoch();
    }
}
