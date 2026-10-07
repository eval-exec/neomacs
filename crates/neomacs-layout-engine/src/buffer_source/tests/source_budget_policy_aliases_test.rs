//! Explicit-input parser aliases and malformed values preserve the existing policy.
//! Pure OsStr inputs do not mutate the environment or reset a process OnceLock.
use super::parse_sync_source_budget;
use std::ffi::OsStr;

#[test]
fn explicit_source_budget_policy_preserves_aliases_and_invalid_values() {
    for value in ["on", "1", "true", "yes", " ON ", "True"] {
        assert_eq!(
            parse_sync_source_budget(Some(OsStr::new(value))),
            true,
            "explicit ON: {value:?}"
        );
    }
    for value in [
        "off", "0", "false", "no", "legacy", "prove", "", "  ", "unknown", "enabled", "同期", "onx",
    ] {
        assert_eq!(
            parse_sync_source_budget(Some(OsStr::new(value))),
            false,
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
                parse_sync_source_budget(Some(OsStr::from_bytes(bytes))),
                false
            );
        }
    }
}
