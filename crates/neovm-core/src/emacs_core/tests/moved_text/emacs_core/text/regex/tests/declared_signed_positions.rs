//! Signed GNU positions from entirely initialized declared-count names.
use neovm_text::emacs_core::regex::*;

fn declared_source() -> neovm_value::heap_types::LispString {
    neovm_value::heap_types::LispString::from_emacs_bytes_with_declared_chars(
        b"abcde".to_vec(),
        neovm_value::heap_types::GnuDeclaredCharacterCount::try_from(0_i128).expect("zero count"),
    )
}

#[test]
fn declared_regex_keeps_negative_return_and_end_registers() {
    crate::test_utils::init_test_tracing();
    let _context = Context::new();
    neovm_value::string_pos_cache::reset_string_pos_cache();
    let source = neovm_value::value::Value::heap_string(declared_source());
    let string = source.as_lisp_string().expect("source");
    let pattern = neovm_value::heap_types::LispString::from_utf8("bcd");
    let compiled =
        compile_string_search_pattern(&pattern, string, false, false, None, &DefaultSyntaxLookup)
            .expect("compile");
    let mut registers = SearchRegisters::default();
    assert!(
        string_search_compiled_into(&compiled, string, 0, &DefaultSyntaxLookup, &mut registers)
            .expect("search")
    );
    let mut data = None;
    let result = registers.publish_string_into(SearchedString::Heap(source), Some(&mut data));
    assert_eq!(result.get(), 1);
    let group = data
        .as_ref()
        .expect("data")
        .lisp_group(0)
        .expect("participating start");
    assert_eq!((group.start(), group.end()), (1, -1));
    assert!(
        data.as_ref()
            .expect("data")
            .group_zero_based_char_range(0)
            .is_none()
    );
    let snapshot = data
        .as_ref()
        .expect("data")
        .string_register_snapshot()
        .expect("string snapshot");
    let restored = MatchData::from_string_register_snapshot(snapshot);
    let restored_group = restored
        .lisp_group(0)
        .expect("signed snapshot preserves participation");
    assert_eq!((restored_group.start(), restored_group.end()), (1, -1));
    assert!(restored.searched_string().is_none());
}

#[test]
fn declared_regex_negative_start_is_not_a_participating_group() {
    crate::test_utils::init_test_tracing();
    let _context = Context::new();
    neovm_value::string_pos_cache::reset_string_pos_cache();
    let source = neovm_value::value::Value::heap_string(declared_source());
    let string = source.as_lisp_string().expect("source");
    let pattern = neovm_value::heap_types::LispString::from_utf8("d");
    let compiled =
        compile_string_search_pattern(&pattern, string, false, false, None, &DefaultSyntaxLookup)
            .expect("compile");
    let mut registers = SearchRegisters::default();
    assert!(
        string_search_compiled_into(&compiled, string, 0, &DefaultSyntaxLookup, &mut registers)
            .expect("search")
    );
    let mut data = None;
    assert_eq!(
        registers
            .publish_string_into(SearchedString::Heap(source), Some(&mut data))
            .get(),
        -2
    );
    assert!(data.as_ref().expect("data").lisp_group(0).is_none());
}
