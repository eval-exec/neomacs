use super::*;

#[test]
fn property_keys_parser_preserves_explicit_inputs() {
    use std::ffi::OsStr;
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
}

#[test]
fn property_keys_numeric_scope_restores_nested_and_unwound_policy_and_counts() {
    use crate::neovm_bridge::property_keys_test_support::{self as probes, Guard};
    let initial = probes::forced();
    let initial_counts = probes::counts();
    {
        let _outer = Guard::set(false);
        assert!(!enabled());
        probes::note_heap_materialization();
        let outer_counts = probes::counts();
        let unwind = std::panic::catch_unwind(|| {
            let _inner = Guard::set(true);
            assert!(enabled());
            probes::note_inline_construction();
            panic!("numeric property-key scope unwind control");
        });
        assert!(unwind.is_err());
        assert!(!enabled());
        assert_eq!(probes::counts(), outer_counts);
    }
    assert_eq!(probes::forced(), initial);
    assert_eq!(probes::counts(), initial_counts);
}
