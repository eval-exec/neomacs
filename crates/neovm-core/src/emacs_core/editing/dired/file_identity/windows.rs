use super::{IdentityDetail, Ownership, Principal};
use std::fs;
use std::path::Path;

fn from_windows(identity: neovm_value::system_vocab::w32_security::Ownership) -> Ownership {
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

// `_detail` is accepted but not acted on: the security-descriptor query
// returns the SID and its resolved name together, so there is no cheaper
// ids-only path to take here.
pub(super) fn query(
    path: &Path,
    _metadata: &fs::Metadata,
    _detail: IdentityDetail,
) -> Ownership {
    neovm_value::system_vocab::w32_security::file_ownership(path)
        .or_else(neovm_value::system_vocab::w32_security::current_process_ownership)
        .map(from_windows)
        .unwrap_or(Ownership {
            user: Principal { id: 0, name: None },
            group: Principal { id: 0, name: None },
        })
}
