//! Private matcher tests move with their single completion implementation.

use super::*;

#[test]
fn completion_prefix_models_empty_input_as_match_all() {
    let empty = LispString::from_utf8("");
    let nonempty = LispString::from_utf8("M-x");

    assert_eq!(
        CompletionPrefix::from_lisp_string(&empty),
        CompletionPrefix::Empty
    );
    assert_eq!(
        CompletionPrefix::from_lisp_string(&nonempty),
        CompletionPrefix::Characters(vec!['M' as u32, '-' as u32, 'x' as u32])
    );
}

#[test]
fn completion_candidate_keeps_the_global_obarray_scan_compact() {
    assert!(
        std::mem::size_of::<CompletionCandidate>() <= 5 * std::mem::size_of::<usize>(),
        "completion candidates should carry compact ownership handles, not inline LispString payloads; got {} bytes",
        std::mem::size_of::<CompletionCandidate>()
    );
}
