use super::*;

#[test]
fn autoload_type_rejects_unknown_code_with_typed_error() {
    let entry = DumpAutoloadEntry {
        file: DumpLispString {
            data: Vec::new(),
            size: 0,
            size_byte: 0,
        },
        docstring: None,
        interactive: false,
        autoload_type: DumpAutoloadType::Function,
    };
    let mut bytes = Vec::new();
    write_autoload_entry(&mut bytes, &entry).unwrap();
    *bytes.last_mut().unwrap() = u8::MAX;
    let mut cursor = Cursor::new(&bytes);
    let error = read_autoload_entry(&mut cursor).unwrap_err();
    assert!(matches!(error, DumpError::InvalidAutoloadType(source) if source.number == u8::MAX));
}
