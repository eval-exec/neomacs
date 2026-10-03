use super::parse_lazy_proof;
use std::ffi::{OsStr, OsString};

#[test]
fn lazy_proof_selector_defaults_off_and_accepts_only_explicit_aliases() {
    assert!(!parse_lazy_proof(None));
    for value in ["", " ", "off", "0", "false", "no", "invalid", "sync", "gnu"] {
        assert!(!parse_lazy_proof(Some(OsStr::new(value))), "{value:?}");
    }
    for value in ["on", "1", "true", "yes", " ON ", " True "] {
        assert!(parse_lazy_proof(Some(OsStr::new(value))), "{value:?}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let invalid = OsString::from_vec(vec![0xff]);
        assert!(!parse_lazy_proof(Some(invalid.as_os_str())));
    }
}
