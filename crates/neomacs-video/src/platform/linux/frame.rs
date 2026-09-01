use std::os::fd::{AsRawFd, OwnedFd};

use gstreamer as gst;

use crate::{VideoColorimetry, VideoTransferPath};

pub(super) struct DmaBufObject {
    pub(super) fd: OwnedFd,
    pub(super) modifier: u64,
}

pub(super) struct DmaBufPlane {
    pub(super) object_index: usize,
    pub(super) stride: u32,
    pub(super) offset: u32,
}

pub(super) struct DmaBufSurface {
    pub(super) objects: Vec<DmaBufObject>,
    pub(super) planes: Vec<DmaBufPlane>,
    pub(super) fourcc: u32,
}

pub(super) struct CpuPackedSurface {
    pub(super) bytes: Vec<u8>,
    pub(super) stride: u32,
}

pub(super) enum LinuxFrameStorage {
    DmaBuf(DmaBufSurface),
    CpuPacked(CpuPackedSurface),
}

pub(crate) struct LinuxFrameLease {
    pub(super) _sample: gst::Sample,
    pub(super) storage: LinuxFrameStorage,
    pub(super) transfer_path: VideoTransferPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct DmaBufSurfaceKey {
    objects: Vec<DmaBufObjectKey>,
    planes: Vec<DmaBufPlaneKey>,
    fourcc: u32,
    width: u32,
    height: u32,
    colorimetry: VideoColorimetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DmaBufPlaneKey {
    object_index: usize,
    stride: u32,
    offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DmaBufObjectKey {
    device: u64,
    inode: u64,
    modifier: u64,
}

/// Stable identity of one decoder-pool allocation. Duplicated descriptors for
/// the same DMA-BUF retain the same device/inode pair, unlike raw FD numbers,
/// which the process may recycle immediately.
pub(super) fn dmabuf_cache_key(
    surface: &DmaBufSurface,
    width: u32,
    height: u32,
    colorimetry: VideoColorimetry,
) -> Result<DmaBufSurfaceKey, String> {
    let objects = surface
        .objects
        .iter()
        .map(|object| {
            let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe { libc::fstat(object.fd.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
                return Err(format!(
                    "failed to identify DMA-BUF object: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let stat = unsafe { stat.assume_init() };
            Ok(DmaBufObjectKey {
                device: stat.st_dev,
                inode: stat.st_ino,
                modifier: object.modifier,
            })
        })
        .collect::<Result<_, String>>()?;
    let planes = surface
        .planes
        .iter()
        .map(|plane| DmaBufPlaneKey {
            object_index: plane.object_index,
            stride: plane.stride,
            offset: plane.offset,
        })
        .collect();
    Ok(DmaBufSurfaceKey {
        objects,
        planes,
        fourcc: surface.fourcc,
        width,
        height,
        colorimetry,
    })
}

#[cfg(test)]
#[path = "frame_test.rs"]
mod tests;
