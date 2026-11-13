use super::*;

#[test]
fn hash_table_test_rejects_unknown_code_with_typed_error() {
    let mut cursor = Cursor::new(&[u8::MAX]);
    let error = cursor.read_hash_table_test().unwrap_err();
    assert!(matches!(error, DumpError::InvalidHashTableTest(source) if source.number == u8::MAX));
}

#[test]
fn hash_table_weakness_rejects_unknown_code_with_typed_error() {
    let mut cursor = Cursor::new(&[1, u8::MAX]);
    let error = cursor.read_opt_hash_table_weakness().unwrap_err();
    assert!(
        matches!(error, DumpError::InvalidHashTableWeakness(source) if source.number == u8::MAX)
    );
}
