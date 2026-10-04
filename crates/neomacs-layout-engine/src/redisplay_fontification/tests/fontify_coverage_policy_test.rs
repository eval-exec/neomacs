//! Pure process-input contracts: no mutation of startup environment or Lisp.
use super::parse;
use std::ffi::OsStr;
#[test]
fn coverage_policy_retains_default_off_and_explicit_input_aliases() {
    assert!(!parse(None));
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
