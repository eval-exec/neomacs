use super::{IdentityDetail, Ownership, Principal};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub(super) fn query(
    _path: &Path,
    metadata: &fs::Metadata,
    detail: IdentityDetail,
) -> Ownership {
    let uid = metadata.uid();
    let gid = metadata.gid();
    // `uid_to_name`/`gid_to_name` are the NSS lookups; skip them entirely when
    // the caller only wants the numbers, which is what `id-format' anything
    // other than `string' means.
    let names = matches!(detail, IdentityDetail::WithNames);
    Ownership {
        user: Principal {
            id: i64::from(uid),
            name: names.then(|| super::super::uid_to_name(uid)).flatten(),
        },
        group: Principal {
            id: i64::from(gid),
            name: names.then(|| super::super::gid_to_name(gid)).flatten(),
        },
    }
}
