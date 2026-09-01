//! Oracle parity tests for GNU overlay helper semantics.
//!
//! GNU implements `copy-overlay` and `remove-overlays` in `lisp/subr.el`.
//! The exact behavior matters because `remove-overlays` can delete, move, or
//! split matching overlays while preserving non-targeted properties.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_copy_overlay_preserves_properties_and_deleted_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdefghij")
  (let* ((live (make-overlay 2 8))
         (deleted (make-overlay 4 6)))
    (overlay-put live 'face 'bold)
    (overlay-put live 'priority 7)
    (overlay-put live 'payload '(a b c))
    (overlay-put deleted 'tag 'gone)
    (delete-overlay deleted)
    (let ((live-copy (copy-overlay live))
          (deleted-copy (copy-overlay deleted)))
      (list
       (list (overlay-start live-copy)
             (overlay-end live-copy)
             (eq (overlay-buffer live-copy) (current-buffer))
             (overlay-get live-copy 'face)
             (overlay-get live-copy 'priority)
             (overlay-get live-copy 'payload))
       (list (overlay-start deleted-copy)
             (overlay-end deleted-copy)
             (overlay-buffer deleted-copy)
             (overlay-get deleted-copy 'tag))))))
"#;

    let expect = expect_test::expect![[r#""OK ((2 8 t bold 7 (a b c)) (nil nil nil gone))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_remove_overlays_deletes_moves_and_splits_matching_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdefghijklmnopqrstuvwxyz")
  (let ((left (make-overlay 2 8))
        (right (make-overlay 10 18))
        (outer (make-overlay 1 25))
        (inner (make-overlay 12 14))
        (miss (make-overlay 6 20)))
    (overlay-put left 'kind 'target)
    (overlay-put right 'kind 'target)
    (overlay-put outer 'kind 'target)
    (overlay-put inner 'kind 'target)
    (overlay-put miss 'kind 'miss)
    (overlay-put left 'name "left")
    (overlay-put right 'name "right")
    (overlay-put outer 'name "outer")
    (overlay-put inner 'name "inner")
    (overlay-put miss 'name "miss")
    (remove-overlays 5 15 'kind 'target)
    (sort
     (mapcar (lambda (o)
               (list (overlay-get o 'name)
                     (overlay-start o)
                     (overlay-end o)
                     (overlay-get o 'kind)))
             (overlays-in 1 (point-max)))
     (lambda (a b) (string< (car a) (car b))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"left\" 2 5 target) (\"miss\" 6 20 miss) (\"outer\" 1 5 target) (\"outer\" 15 25 target) (\"right\" 15 18 target))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
