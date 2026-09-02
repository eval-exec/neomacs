//! Runtime-image boundary for hosts without file-backed memory maps.
//!
//! The image FORMAT (section kinds, relocation encoding) is shared with the
//! native owner through `image_format`; only the file mapping capability is
//! absent here. A browser runtime image is restored from the pointer-free
//! portable snapshot format and is never represented by this file-mapping
//! owner, so every operation below reports the missing capability instead of
//! producing an image nothing can read back.

use std::path::Path;

use super::DumpError;
pub(crate) use super::image_format::{
    DumpSectionKind, ImageRelocation, ImageSection, relocation_section_bytes,
};

/// Owner type retained in the portable ABI so `Context` has one shape across
/// hosts. No value is ever produced: `load_image` always fails.
pub(crate) struct LoadedMmapImage;

impl LoadedMmapImage {
    pub(crate) fn section(&self, _kind: DumpSectionKind) -> Option<&[u8]> {
        None
    }

    pub(crate) fn section_mut_ptr(&self, _kind: DumpSectionKind) -> Option<(*mut u8, usize)> {
        None
    }

    pub(crate) fn apply_relocations(&mut self) -> Result<(), DumpError> {
        Err(unavailable())
    }

    pub(crate) fn contains_ptr(&self, _ptr: *const u8) -> bool {
        false
    }
}

pub(crate) fn write_image(_path: &Path, _sections: &[ImageSection<'_>]) -> Result<(), DumpError> {
    Err(unavailable())
}

pub(crate) fn load_image(_path: &Path) -> Result<LoadedMmapImage, DumpError> {
    Err(unavailable())
}

fn unavailable() -> DumpError {
    DumpError::ImageFormatError(
        "this host has no file-backed runtime-image mapping capability".into(),
    )
}
