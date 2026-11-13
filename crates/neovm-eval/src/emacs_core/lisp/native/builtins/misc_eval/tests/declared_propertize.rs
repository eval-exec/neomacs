use super::*;
use neovm_value::heap_types::{GnuDeclaredCharacterCount, LispString};

#[test]
fn propertize_preserves_supplied_character_count_and_source_bytes() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let property = Value::symbol("gdj-declared-property");
    for (bytes, count) in [
        ("bé".as_bytes(), 1),
        (b"a".as_slice(), 0),
        ("bé".as_bytes(), 2),
    ] {
        let string = LispString::from_emacs_bytes_with_declared_chars(
            bytes.to_vec(),
            GnuDeclaredCharacterCount::try_from(count).expect("test count"),
        );
        let input = Value::heap_string(string);
        let expected = input.as_lisp_string().expect("test input").clone();
        for args in [vec![input], vec![input, property, Value::fixnum(17)]] {
            let output = builtin_propertize(args).expect("copy properties");
            let actual = output.as_lisp_string().expect("string result");
            assert_eq!(actual.schars(), expected.schars());
            assert_eq!(actual.sbytes(), expected.sbytes());
            assert_eq!(actual.is_multibyte(), expected.is_multibyte());
            assert_eq!(actual.as_bytes(), expected.as_bytes());
        }
    }
}
