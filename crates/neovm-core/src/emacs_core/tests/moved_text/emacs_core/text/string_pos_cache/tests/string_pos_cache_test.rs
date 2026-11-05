use neovm_value::emacs_char;

use neovm_value::heap_types::LispString;

use neovm_value::string_pos_cache::*;

use neovm_value::value::Value;

#[test]
fn never_stores_while_the_notification_hook_is_uninstalled() {
    crate::test_utils::init_test_tracing();
    // No Context is constructed in this process, and the hook is published
    // only by the evaluator's one-time seams, so the cache must refuse to
    // fill. A mutation that therefore skipped notification cannot leave a
    // stale entry behind.
    assert!(
        !neovm_value::heap_types::string_bytes_changed_hook_installed(),
        "this test needs a process with no evaluator-constructed hook"
    );
    reset_string_pos_cache();
    let value = Value::heap_string(LispString::from_emacs_bytes("aжжb".as_bytes().to_vec()));
    let original_ptr = value.as_lisp_string().expect("string").as_bytes().as_ptr();
    let original_sbytes = value.as_lisp_string().expect("string").sbytes();
    // This conversion would normally fill the cache; it must compute uncached.
    assert_eq!(
        string_char_to_byte(value, value.as_lisp_string().unwrap(), 2),
        3
    );
    assert!(
        cached_pair(value, value.as_lisp_string().unwrap()).is_none(),
        "the cache must not store while the byte-mutation hook is uninstalled"
    );
    // The same object, SBYTES and data pointer, different character
    // boundaries: a stored pair would now be stale.
    value.with_lisp_string_mut(|string| {
        string.mutate_bytes(|bytes| {
            bytes.clear();
            bytes.extend_from_slice("жaжb".as_bytes());
        })
    });
    let changed = value.as_lisp_string().expect("string");
    assert_eq!(changed.as_bytes().as_ptr(), original_ptr);
    assert_eq!(changed.sbytes(), original_sbytes);
    assert_eq!(string_char_to_byte(value, changed, 1), 2);
    assert!(cached_pair(value, changed).is_none());
}
