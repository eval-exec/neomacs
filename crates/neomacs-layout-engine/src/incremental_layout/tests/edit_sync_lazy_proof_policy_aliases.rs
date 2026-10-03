//! Explicit-input Lazy policy controls survive an absence-only promotion.
//! Pure OsStr inputs read no Lisp state and mutate no environment or OnceLock.
//! Independent test threads own their inputs; no policy is published here.
use super::parse_lazy_proof;
use std::ffi::OsStr;

#[test]
fn explicit_lazy_proof_preserves_aliases_and_invalid_values() {
    for value in ["on", "1", "true", "yes", " ON ", " True ", "YeS", "\ton\n"] {
        assert!(
            parse_lazy_proof(Some(OsStr::new(value))),
            "explicit ON: {value:?}"
        );
    }
    for value in [
        "off", "0", "false", "no", " OFF ", "False", "legacy", "prove", "", " ", "\t\n", "invalid",
        "sync", "gnu", "enabled", "onx", "2", "同期",
    ] {
        assert!(
            !parse_lazy_proof(Some(OsStr::new(value))),
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
            assert!(!parse_lazy_proof(Some(OsStr::from_bytes(bytes))));
        }
    }
}
