//! Pure process-input contracts: no mutation of startup environment or Lisp.
use super::parse;
use std::ffi::OsStr;
#[test]
fn explicit_policy_preserves_aliases_empty_invalid_and_nonunicode_inputs() {
    for value in [
        "off", "false", "0", "no", "legacy", "prove", "", " ", "invalid",
    ] {
        assert!(
            !parse(Some(OsStr::new(value))),
            "explicit OFF/invalid {value:?}"
        );
    }
    for value in ["on", "1", "true", "yes", " ON ", "True", "YES"] {
        assert!(parse(Some(OsStr::new(value))), "ON alias {value:?}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        assert!(!parse(Some(OsStr::from_bytes(b"\xff"))));
    }
}
