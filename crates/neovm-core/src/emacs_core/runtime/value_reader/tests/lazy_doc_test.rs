//! Unit tests for GNU's saved `#@` block ring and the typed lazy-doc
//! admission. The end-to-end conversion is exercised from the load tests,
//! where a reader has a real environment and heap.

use super::lazy_doc::{LazyDocPolicy, LazyDocRef, SavedDocBlocks};
use crate::emacs_core::doc::DocBytePosition;
use crate::emacs_core::value::Value;

fn pos(offset: usize) -> DocBytePosition {
    DocBytePosition::from_file_offset(offset)
}

#[test]
fn saved_doc_blocks_keep_the_last_two_and_find_positions_inside_them() {
    let mut blocks = SavedDocBlocks::new();
    blocks.record(pos(10), b"aaaa\x1f");
    blocks.record(pos(20), b"bbbb\x1f");
    assert_eq!(blocks.find(pos(10)), Some(&b"aaaa\x1f"[..]));
    assert_eq!(blocks.find(pos(13)), Some(&b"a\x1f"[..]));
    assert_eq!(blocks.find(pos(20)), Some(&b"bbbb\x1f"[..]));
    assert_eq!(blocks.find(pos(9)), None);
    // The terminator is part of the saved block: its offsets are the bytes GNU
    // copied after the `#@N ` header.
    assert_eq!(blocks.find(pos(14)), Some(&b"\x1f"[..]));
    assert_eq!(blocks.find(pos(24)), Some(&b"\x1f"[..]));
    assert_eq!(blocks.find(pos(25)), None);

    // GNU exchanges the two slots: the third record evicts the first.
    blocks.record(pos(30), b"cccc\x1f");
    assert_eq!(blocks.find(pos(10)), None);
    assert_eq!(blocks.find(pos(20)), Some(&b"bbbb\x1f"[..]));
    assert_eq!(blocks.find(pos(30)), Some(&b"cccc\x1f"[..]));
}

#[test]
fn saved_doc_blocks_unquote_with_the_saved_block_rules() {
    let mut blocks = SavedDocBlocks::new();
    blocks.record(pos(5), b"Bad\x01xEscape\x1f");
    assert_eq!(blocks.unquoted(pos(5)), Some(Ok(b"BadxEscape".to_vec())));
    assert_eq!(blocks.unquoted(pos(4)), None);
}

#[test]
fn lazy_doc_admission_requires_the_live_identity_and_a_fixnum_cdr() {
    // Immediate values keep this test free of heap allocation: the witness is
    // the exact object the pair's car holds.
    let witness = Value::T;
    let forced = LazyDocPolicy::observe(Some(Value::T), Some(witness));

    let admitted = LazyDocRef::admit(Value::cons(witness, Value::fixnum(7)), forced)
        .expect("identity car and fixnum cdr are admitted");
    assert_eq!(admitted.position().get(), 7);
    assert_eq!(admitted.file_value().bits(), witness.bits());

    // The whole object's car/cdr: `(x #$ . 7)` is not a lazy reference.
    assert!(
        LazyDocRef::admit(
            Value::cons(witness, Value::cons(Value::fixnum(1), Value::fixnum(2))),
            forced
        )
        .is_none()
    );
    // A non-fixnum cdr is ordinary data.
    assert!(LazyDocRef::admit(Value::cons(witness, Value::NIL), forced).is_none());
    // Force off never admits.
    assert!(
        LazyDocRef::admit(
            Value::cons(witness, Value::fixnum(7)),
            LazyDocPolicy::observe(None, Some(witness))
        )
        .is_none()
    );
    // A nil load-file-name cannot be a witness even when the flag is on.
    assert!(
        LazyDocRef::admit(
            Value::cons(Value::NIL, Value::fixnum(7)),
            LazyDocPolicy::observe(Some(Value::T), None)
        )
        .is_none()
    );
}
