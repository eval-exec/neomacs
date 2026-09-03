use super::{Ownership, Principal};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub(super) fn query(_path: &Path, metadata: &fs::Metadata) -> Ownership {
    let uid = metadata.uid();
    let gid = metadata.gid();
    Ownership {
        user: Principal {
            id: i64::from(uid),
            name: super::super::uid_to_name(uid),
        },
        group: Principal {
            id: i64::from(gid),
            name: super::super::gid_to_name(gid),
        },
    }
}
