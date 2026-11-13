use super::*;

#[test]
fn buffer_text_backend_rejects_unknown_code_with_typed_error() {
    let mut cursor = Cursor::new(&[u8::MAX]);
    let error = read_buffer_text_backend_kind(&mut cursor).unwrap_err();
    assert!(
        matches!(error, DumpError::InvalidBufferTextBackendKind(source) if source.number == u8::MAX)
    );
}
