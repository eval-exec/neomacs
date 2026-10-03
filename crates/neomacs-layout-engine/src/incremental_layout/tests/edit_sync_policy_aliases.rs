//! Explicit-input parser aliases and malformed values preserve the existing policy.
//! Pure OsStr inputs do not mutate the environment or reset a process OnceLock.
use super::{EditSyncMode, parse_edit_sync_mode};
use std::ffi::OsStr;

#[test]
fn explicit_edit_sync_policy_preserves_aliases_and_invalid_values() {
    for value in ["sync", "on", "1", " SYNC ", "On"] {
        assert_eq!(
            parse_edit_sync_mode(Some(OsStr::new(value))),
            EditSyncMode::Sync,
            "explicit ON: {value:?}"
        );
    }
    for value in [
        "off", "0", "false", "no", "legacy", "prove", "", "  ", "unknown", "enabled", "同期",
        "onx", "true", "yes", "gnu",
    ] {
        assert_eq!(
            parse_edit_sync_mode(Some(OsStr::new(value))),
            EditSyncMode::Prove,
            "explicit baseline or malformed: {value:?}"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        for bytes in [
            b"\xff".as_slice(),
            b"on\xff".as_slice(),
            b"\xffon".as_slice(),
        ] {
            assert_eq!(
                parse_edit_sync_mode(Some(OsStr::from_bytes(bytes))),
                EditSyncMode::Prove
            );
        }
    }
}
