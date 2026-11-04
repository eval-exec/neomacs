use super::*;

#[test]
fn localized_forwarder_rejects_unknown_code_with_typed_error() {
    let error = decode_localized_forwarder(u8::MAX).unwrap_err();
    assert!(
        matches!(error, DumpError::InvalidLocalizedForwarder(source) if source.number == u8::MAX)
    );
    assert_eq!(decode_localized_forwarder(0).unwrap(), None);
    for code in u8::MIN..=u8::MAX {
        if let Ok(kind) = DumpLocalizedForwarder::try_from(code) {
            assert_eq!(
                decode_localized_forwarder(encode_localized_forwarder(Some(kind))).unwrap(),
                Some(kind)
            );
        }
    }
}
