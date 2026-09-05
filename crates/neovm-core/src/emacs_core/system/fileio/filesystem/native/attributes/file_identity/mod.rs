//! Compile-time-selected file owner/group identity.
//!
//! `file-attributes` exposes one portable Lisp contract, but the source of its
//! principals is native: Unix metadata IDs versus Windows security-descriptor
//! SIDs.  Keep that distinction behind this typed boundary.

use std::fs;
use std::path::Path;

use super::super::super::FilePrincipal as Principal;

pub(super) struct Ownership {
    pub(super) user: Principal,
    pub(super) group: Principal,
}

std::cfg_select! {
    unix => {
        mod unix;
        use unix::query;
    }
    windows => {
        mod windows;
        use windows::query;
    }
    _ => {
        mod unsupported;
        use unsupported::query;
    }
}

pub(super) fn for_path(path: &Path, metadata: &fs::Metadata) -> Ownership {
    query(path, metadata)
}
