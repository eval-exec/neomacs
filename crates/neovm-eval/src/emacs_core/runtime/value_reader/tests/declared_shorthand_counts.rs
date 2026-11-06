use super::{ReadShorthandError, ReadSymbolShorthands, Value};
use neovm_value::heap_types::LispString;

fn shorthands(short: LispString, long: LispString) -> ReadSymbolShorthands {
    let pair = Value::cons(Value::heap_string(short), Value::heap_string(long));
    ReadSymbolShorthands::from_lisp_value(Value::list(vec![pair])).expect("valid test pair")
}

#[test]
fn mb_reader_preserves_arithmetic_count_for_mixed_prefix_encodings() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let rewrite = shorthands(
        LispString::from_unibyte(vec![195, 169]),
        LispString::from_utf8("b"),
    );
    let name = rewrite
        .rewrite(&LispString::from_utf8("éé"))
        .expect("valid metadata")
        .expect("matched prefix");
    assert_eq!(name.schars(), 1);
    assert_eq!(name.sbytes(), 3);
    assert_eq!(name.as_bytes(), &[b'b', 195, 169]);
    assert!(name.is_multibyte());
}

#[test]
fn mb_reader_negative_count_recounts_instead_of_rejecting() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let rewrite = shorthands(
        LispString::from_unibyte(vec![195, 169]),
        LispString::from_utf8(""),
    );
    let name = rewrite
        .rewrite(&LispString::from_utf8("é"))
        .expect("negative metadata requests recount")
        .expect("matched prefix");
    assert_eq!(name.schars(), 0);
    assert_eq!(name.sbytes(), 0);
    assert!(name.is_multibyte());
}

#[test]
fn mb_reader_invalid_negative_recount_is_typed_error() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let rewrite = shorthands(
        LispString::from_unibyte(vec![195, 169, 195]),
        LispString::from_utf8(""),
    );
    assert!(matches!(
        rewrite.rewrite(&LispString::from_utf8("éé")),
        Err(ReadShorthandError::InvalidMultibyteRecount)
    ));
}

#[test]
fn ordinary_unicode_reader_rewrite_preserves_bytes_and_source_flag() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let rewrite = shorthands(LispString::from_utf8("a"), LispString::from_utf8("λ"));
    let name = rewrite
        .rewrite(&LispString::from_utf8("aé"))
        .expect("valid metadata")
        .expect("matched prefix");
    assert_eq!(name.schars(), 2);
    assert_eq!(name.sbytes(), 4);
    assert_eq!(name.as_bytes(), "λé".as_bytes());
    assert!(name.is_multibyte());
}
