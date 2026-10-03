#[test]
fn property_keys_environment_is_forwarded_by_exact_name_only() {
    use std::ffi::OsString;
    let variables = [
        (
            OsString::from("NEOMACS_LAYOUT_PROPERTY_KEYS_INLINE"),
            OsString::from("on"),
        ),
        (
            OsString::from("NEOMACS_LAYOUT_PROPERTY_KEYS_INLINE_TRACE"),
            OsString::from("unrelated"),
        ),
    ];
    assert_eq!(
        crate::harness::passthrough_from(variables)
            .into_iter()
            .collect::<Vec<_>>(),
        vec![(
            "NEOMACS_LAYOUT_PROPERTY_KEYS_INLINE".to_owned(),
            OsString::from("on")
        )]
    );
}
