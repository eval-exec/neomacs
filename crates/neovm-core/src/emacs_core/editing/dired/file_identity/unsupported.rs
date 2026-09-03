use super::{Ownership, Principal};
use crate::emacs_core::runtime_identity::{
    CredentialScope, process_group_id, process_user_id,
};
use std::fs;
use std::path::Path;

pub(super) fn query(_path: &Path, _metadata: &fs::Metadata) -> Ownership {
    Ownership {
        user: Principal {
            id: i64::from(process_user_id(CredentialScope::Effective)),
            name: None,
        },
        group: Principal {
            id: i64::from(process_group_id(CredentialScope::Effective)),
            name: None,
        },
    }
}
