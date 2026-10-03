use super::*;

#[test]
fn face_gather_parser_is_default_off_and_scoped_policy_is_numeric() {
    use std::ffi::OsStr;
    assert!(!parse(None));
    for value in [
        "", " ", "off", "0", "false", "no", "invalid", "verify", "only",
    ] {
        assert!(!parse(Some(OsStr::new(value))), "{value:?}");
    }
    for value in ["on", "1", "true", "yes", " ON ", " True "] {
        assert!(parse(Some(OsStr::new(value))), "{value:?}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        assert!(!parse(Some(OsStr::from_bytes(b"on\xff"))));
    }
    let _off = crate::engine::retained_face_gather_test_support::Guard::set(false);
    assert!(!enabled());
    {
        let _on = crate::engine::retained_face_gather_test_support::Guard::set(true);
        assert!(enabled());
    }
    assert!(!enabled());
}
