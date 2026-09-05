//! Native stat collection. No Lisp values or evaluator access belongs here.

use super::super::{
    FileAttributeSnapshot, FileAttributeType, FileIdentity, FileMode, FileTimestamp,
};
use std::{fs, io, path::Path};

mod file_identity;

pub(super) fn read(path: &Path) -> io::Result<FileAttributeSnapshot> {
    let stat = fs::symlink_metadata(path)?;
    let kind = if stat.file_type().is_symlink() {
        FileAttributeType::SymbolicLink(fs::read_link(path)?)
    } else if stat.is_dir() {
        FileAttributeType::Directory
    } else {
        FileAttributeType::Other
    };
    let ownership = file_identity::for_path(path, &stat);
    let mut attributes = FileAttributeSnapshot {
        kind,
        links: Some(1),
        user: Some(ownership.user),
        group: Some(ownership.group),
        accessed: None,
        modified: None,
        changed: None,
        len: stat.len(),
        mode: None,
        identity: None,
        legacy_group_change: false,
    };
    std::cfg_select! {
        unix => {
            use std::os::unix::fs::MetadataExt;
            attributes.links = Some(stat.nlink());
            attributes.accessed = Some(FileTimestamp { seconds: stat.atime(), nanoseconds: stat.atime_nsec() as u32 });
            attributes.modified = Some(FileTimestamp { seconds: stat.mtime(), nanoseconds: stat.mtime_nsec() as u32 });
            attributes.changed = Some(FileTimestamp { seconds: stat.ctime(), nanoseconds: stat.ctime_nsec() as u32 });
            attributes.mode = Some(FileMode::from_bits_truncate(stat.mode()));
            attributes.identity = Some(FileIdentity { inode: stat.ino(), device: stat.dev() });
            attributes.legacy_group_change = true;
        }
        _ => {
            // Preserve the existing native non-Unix attribute contract.
            attributes.accessed = stat.accessed().ok().and_then(FileTimestamp::from_system_time);
            attributes.modified = stat.modified().ok().and_then(FileTimestamp::from_system_time);
            attributes.changed = stat.created().ok().and_then(FileTimestamp::from_system_time);
            attributes.mode = Some(FileMode::from_bits_truncate(if stat.is_dir() { 0o755 } else { 0o644 }));
            attributes.identity = Some(FileIdentity { inode: 0, device: 0 });
        }
    }
    Ok(attributes)
}
