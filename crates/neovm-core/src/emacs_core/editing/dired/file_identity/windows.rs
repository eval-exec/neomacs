use super::{Ownership, Principal};
use std::fs;
use std::path::Path;

fn from_windows(identity: crate::emacs_core::w32::security::Ownership) -> Ownership {
    Ownership {
        user: Principal {
            id: identity.user.id,
            name: identity.user.name,
        },
        group: Principal {
            id: identity.group.id,
            name: identity.group.name,
        },
    }
}

pub(super) fn query(path: &Path, _metadata: &fs::Metadata) -> Ownership {
    crate::emacs_core::w32::security::file_ownership(path)
        .or_else(crate::emacs_core::w32::security::current_process_ownership)
        .map(from_windows)
        .unwrap_or(Ownership {
            user: Principal { id: 0, name: None },
            group: Principal { id: 0, name: None },
        })
}
