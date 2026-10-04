//! Request-owned exact passthrough; no process environment mutation.
#[test]
fn fontify_coverage_control_is_forwarded_and_lookalike_is_rejected() {
    use std::ffi::OsString;
    let variables = [
        (
            OsString::from("NEOMACS_EDIT_SYNC_FONTIFY_COVERAGE"),
            OsString::from("on"),
        ),
        (
            OsString::from("NEOMACS_EDIT_SYNC_FONTIFY_COVERAGE_TRACE"),
            OsString::from("unrelated"),
        ),
    ];
    assert_eq!(
        crate::harness::passthrough_from(variables)
            .into_iter()
            .collect::<Vec<_>>(),
        vec![(
            "NEOMACS_EDIT_SYNC_FONTIFY_COVERAGE".to_owned(),
            OsString::from("on")
        )]
    );
}
