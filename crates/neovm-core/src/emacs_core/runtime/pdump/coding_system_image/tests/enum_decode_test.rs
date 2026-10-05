use super::*;

#[test]
fn coding_eol_type_rejects_unknown_code_with_typed_error() {
    let mut cursor = Cursor::new(&[u8::MAX]);
    let error = read_eol_type(&mut cursor).unwrap_err();
    assert!(matches!(error, DumpError::InvalidEolType(source) if source.number == u8::MAX));
}
