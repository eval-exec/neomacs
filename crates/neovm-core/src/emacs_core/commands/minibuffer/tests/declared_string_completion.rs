use super::*;
use crate::heap_types::GnuDeclaredCharacterCount;

#[test]
fn completion_iterator_honors_declared_count_with_bounded_tails() {
    crate::test_utils::init_test_tracing();
    let _context = crate::test_utils::new_context();
    let declared = LispString::from_emacs_bytes_with_declared_chars(
        vec![b'b', 0xc3, 0xa9],
        GnuDeclaredCharacterCount::try_from(1i128).unwrap(),
    );
    assert_eq!(lisp_string_chars(&declared).collect::<Vec<_>>(), vec![98]);
    // A deterministic bounded tail is the safe policy when GNU would walk
    // beyond initialized bytes, rather than a GNU padding-value expectation.
    let truncated = LispString::from_emacs_bytes_with_declared_chars(
        vec![0xe9],
        GnuDeclaredCharacterCount::try_from(2i128).unwrap(),
    );
    let codes = lisp_string_chars(&truncated).collect::<Vec<_>>();
    assert_eq!(codes.len(), 2);
    assert!(codes.iter().all(|code| *code <= 0x3f_ffff));
}
