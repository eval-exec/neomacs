//! Compile-time-selected file owner/group identity.
//!
//! `file-attributes` exposes one portable Lisp contract, but the source of its
//! principals is native: Unix metadata IDs versus Windows security-descriptor
//! SIDs.  Keep that distinction behind this typed boundary.

use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Principal {
    pub(super) id: i64,
    pub(super) name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

/// What the caller needs from a file's owner and group.
///
/// Turning a numeric id into a NAME goes through the system name service --
/// NSS on Unix, which may consult files, sssd, LDAP or the network -- while
/// the id itself is already sitting in the `stat` result. The two are not the
/// same request, and asking for the expensive one by accident is exactly what
/// happened: `file-attributes` resolved names for EVERY call and then threw
/// them away unless `id-format' was `string', making it 146x GNU (3044ms vs
/// 21ms over 2000 calls) and `directory-files-and-attributes' 125x.
///
/// Making it an argument rather than a default means a caller has to say which
/// it wants, and a new backend has to handle both.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum IdentityDetail {
    /// Numeric ids only, read from metadata already in hand.
    IdsOnly,
    /// Ids and their names, resolved through the name service.
    WithNames,
}

pub(super) fn for_path(path: &Path, metadata: &fs::Metadata, detail: IdentityDetail) -> Ownership {
    query(path, metadata, detail)
}
