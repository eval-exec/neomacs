//! Print-circle candidate test re-homed from neovm-eval's print test module.
//!
//! The fixture round-trips printer output through `read`, a core-owned
//! minibuffer/reader command. Eval's unit runtime carries the upper entry as
//! an upper-layer refusal, so the test belongs to the runtime that owns the
//! primitive. Core's `new_context` constructs that same surface with the
//! canonical hook-installation proof. Every assertion and expected string is
//! unchanged from the eval copy.

fn print_eval_one(src: &str) -> String {
    let mut ev = crate::test_utils::new_context();
    let result = ev.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

#[test]
fn print_circle_candidate_set_matches_gnu_for_bool_vectors_and_char_tables() {
    crate::test_utils::init_test_tracing();
    // GNU's `print_circle_candidate_p` matches CLOSUREP || CHAR_TABLE_P ||
    // SUB_CHAR_TABLE_P || HASH_TABLE_P || FONTP || RECORDP for non-vector
    // vectorlikes; `VECTORP` excludes bool-vectors (a distinct pseudovector).
    //
    // Bug (a), cosmetic over-labeling: a bool-vector printed twice under
    // `print-circle' must NOT be labeled, because GNU does not treat
    // bool-vectors as circle candidates.  GNU prints each bool-vector in
    // full (`#&3"\7"`, where \7 is the 0x07 bit-pack byte for 3 set bits)
    // with no `#N=' / `#N#' label: bytes `# & 3 " \x07 "` per element.
    assert_eq!(
        print_eval_one(
            "(let ((v (make-bool-vector 3 t))) \
               (let ((print-circle t)) (prin1-to-string (list v v))))"
        ),
        "OK \"(#&3\\\"\u{7}\\\" #&3\\\"\u{7}\\\")\"",
    );
    // Bug (b), functional round-trip break: a genuinely shared char-table
    // printed twice under `print-circle' MUST get a `#N=' / `#N#' label so
    // that reading the output back preserves shared identity.  GNU returns t.
    assert_eq!(
        print_eval_one(
            "(let* ((v (make-char-table 'test)) \
                    (r (read (let ((print-circle t)) \
                               (prin1-to-string (list v v)))))) \
               (eq (nth 0 r) (nth 1 r)))"
        ),
        "OK t",
    );
    // A non-shared char-table (printed once) must NOT be labeled: GNU only
    // labels objects that appear more than once.
    assert_eq!(
        print_eval_one(
            "(let ((v (make-char-table 'test))) \
               (let ((print-circle t)) \
                 (string-match \"#[0-9]+=\" (prin1-to-string v) nil t)))"
        ),
        "OK nil",
    );
}
