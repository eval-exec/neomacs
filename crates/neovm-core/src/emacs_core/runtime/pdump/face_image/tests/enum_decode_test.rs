use super::*;

#[test]
fn face_font_slant_rejects_unknown_code_with_typed_error() {
    let mut cursor = Cursor::new(&[u8::MAX]);
    let error = read_font_slant(&mut cursor).unwrap_err();
    assert!(matches!(error, DumpError::InvalidFontSlant(source) if source.number == u8::MAX));
}

#[test]
fn underline_style_rejects_unknown_code_with_typed_error() {
    let mut cursor = Cursor::new(&[u8::MAX]);
    let error = read_underline_style(&mut cursor).unwrap_err();
    assert!(matches!(error, DumpError::InvalidUnderlineStyle(source) if source.number == u8::MAX));
}

#[test]
fn box_style_rejects_unknown_code_with_typed_error() {
    let mut cursor = Cursor::new(&[u8::MAX]);
    let error = read_box_style(&mut cursor).unwrap_err();
    assert!(matches!(error, DumpError::InvalidBoxStyle(source) if source.number == u8::MAX));
}
