use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use super::{DmaBufObject, DmaBufPlane, DmaBufSurface, dmabuf_cache_key};
use crate::VideoColorimetry;

#[test]
fn dmabuf_cache_identity_survives_descriptor_duplication() {
    let mut fds = [-1; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    // SAFETY: `pipe` returned two newly owned descriptors.
    let read = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    // SAFETY: ownership of the write descriptor is transferred once.
    let _write = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    let duplicate = unsafe { libc::dup(read.as_raw_fd()) };
    assert!(duplicate >= 0);
    // SAFETY: successful `dup` returns a newly owned descriptor.
    let duplicate = unsafe { OwnedFd::from_raw_fd(duplicate) };

    let first = DmaBufSurface {
        objects: vec![DmaBufObject {
            fd: read,
            modifier: 7,
        }],
        planes: vec![DmaBufPlane {
            object_index: 0,
            stride: 256,
            offset: 16,
        }],
        fourcc: 0x3432_5241,
    };
    let second = DmaBufSurface {
        objects: vec![DmaBufObject {
            fd: duplicate,
            modifier: 7,
        }],
        planes: vec![DmaBufPlane {
            object_index: 0,
            stride: 256,
            offset: 16,
        }],
        fourcc: 0x3432_5241,
    };

    assert_eq!(
        dmabuf_cache_key(&first, 64, 32, VideoColorimetry::SRGB).unwrap(),
        dmabuf_cache_key(&second, 64, 32, VideoColorimetry::SRGB).unwrap()
    );
    assert_ne!(
        dmabuf_cache_key(&first, 64, 32, VideoColorimetry::SRGB).unwrap(),
        dmabuf_cache_key(&second, 128, 32, VideoColorimetry::SRGB).unwrap(),
        "geometry participates in the imported image identity"
    );
    assert_ne!(
        dmabuf_cache_key(&first, 64, 32, VideoColorimetry::SRGB).unwrap(),
        dmabuf_cache_key(&second, 64, 32, VideoColorimetry::BT709_LIMITED).unwrap(),
        "shader color metadata participates in the prepared-surface identity"
    );
}
