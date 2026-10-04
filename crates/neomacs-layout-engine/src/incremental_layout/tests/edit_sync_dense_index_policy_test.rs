use super::*;
use std::ffi::OsStr;

#[test]
fn selector_preserves_explicit_invalid_empty_and_nonunicode_inputs() {
    for value in [
        "", " ", "off", "false", "0", "no", "legacy", "prove", "sync", "invalid",
    ] {
        assert!(!parse(Some(OsStr::new(value))), "{value:?}");
    }
    for value in ["on", "1", "true", "yes", " ON ", " True ", "YES"] {
        assert!(parse(Some(OsStr::new(value))), "{value:?}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        assert!(!parse(Some(OsStr::from_bytes(b"\xffon"))));
    }
}

#[test]
fn numeric_range_rejects_empty_sparse_duplicate_reverse_and_signed_endpoint_overflow() {
    assert_eq!(DenseRows::from_indices([]), None);
    assert_eq!(
        DenseRows::from_indices([2, 3, 4]),
        Some(DenseRows { first: 2, end: 5 })
    );
    for indices in [[2, 4], [2, 2], [3, 2]] {
        assert_eq!(DenseRows::from_indices(indices), None);
    }
    let range = DenseRows::from_indices([2, 3, 4]).unwrap();
    assert!(!range.contains(-1));
    assert!(!range.contains(1));
    assert!(!range.contains(5));
    assert!(range.contains(2) && range.contains(4));
    if let Ok(last) = usize::try_from(i64::MAX) {
        assert_eq!(DenseRows::from_indices([last]), None);
        assert_eq!(DenseRows::from_indices([last - 1, last]), None);
        assert_eq!(DenseRows::from_indices([usize::MAX]), None);
    }
}
