//! Exact DRM render-node resolution for the selected Vulkan device.
//!
//! Vendor/product heuristics are intentionally absent: identical GPUs can
//! coexist, and choosing the wrong node invalidates zero-copy device-locality
//! evidence.

use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

/// Resolve an exact kernel DRM device number to its verified render-node path.
///
/// The major/minor pair comes from the selected Vulkan physical device. The
/// candidate under `/dev/dri` must itself be a character device with that same
/// pair, which keeps containers and bind mounts from silently relabeling a
/// different device.
pub fn render_node_from_device_number(major: u32, minor: u32) -> Option<PathBuf> {
    render_node_from_device_number_in(
        major,
        minor,
        Path::new("/sys/dev/char"),
        Path::new("/dev/dri"),
    )
}

fn render_node_from_device_number_in(
    major: u32,
    minor: u32,
    sysfs_char_root: &Path,
    dev_dri_root: &Path,
) -> Option<PathBuf> {
    let sysfs_device = fs::canonicalize(sysfs_char_root.join(format!("{major}:{minor}"))).ok()?;
    let name = sysfs_device.file_name()?.to_str()?;
    if !name.starts_with("renderD")
        || !name["renderD".len()..]
            .chars()
            .all(|ch| ch.is_ascii_digit())
    {
        return None;
    }
    let render_node = dev_dri_root.join(name);
    let metadata = fs::metadata(&render_node).ok()?;
    if !metadata.file_type().is_char_device()
        || libc::major(metadata.rdev()) != major
        || libc::minor(metadata.rdev()) != minor
    {
        return None;
    }
    Some(render_node)
}

#[cfg(test)]
mod tests;
