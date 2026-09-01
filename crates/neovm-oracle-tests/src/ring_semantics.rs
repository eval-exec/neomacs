//! Oracle parity tests for GNU `ring.el` semantics.
//!
//! GNU represents rings as `(head length . vector)` and implements wraparound
//! indexing, oldest/newest insertion modes, resize, and duplicate-removal
//! helpers in `lisp/emacs-lisp/ring.el`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_ring_insert_ref_remove_and_wraparound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'ring)
  (let ((r (make-ring 3)))
    (ring-insert r 'a)
    (ring-insert r 'b)
    (ring-insert r 'c)
    (let ((full (list (ring-elements r)
                      (ring-ref r 0)
                      (ring-ref r 1)
                      (ring-ref r 2)
                      (ring-ref r 3))))
      (ring-insert r 'd)
      (let ((wrapped (list (ring-elements r)
                           (ring-ref r 0)
                           (ring-ref r 2)
                           (ring-ref r 5)))
            (removed-newest (ring-remove r 0))
            (after-newest nil)
            (removed-oldest nil)
            (after-oldest nil))
        (setq after-newest (ring-elements r))
        (setq removed-oldest (ring-remove r))
        (setq after-oldest (ring-elements r))
        (list full wrapped removed-newest after-newest
              removed-oldest after-oldest r)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((c b a) c b a c) ((d c b) d b b) d (c b) b (c) (1 1 . [nil c nil]))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_ring_beginning_copy_resize_and_extend() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'ring)
  (let ((r (make-ring 4)))
    (ring-insert r 'newer)
    (ring-insert-at-beginning r 'oldest)
    (ring-insert r 'newest)
    (let* ((before (list (ring-elements r) (ring-length r) (ring-size r)))
           (copy (ring-copy r))
           (copy-before (list (equal copy r) (eq copy r) (eq (cddr copy) (cddr r)))))
      (ring-resize r 2)
      (let ((resized (list (ring-elements r) (ring-length r) (ring-size r))))
        (ring-insert+extend r 'x t)
        (ring-insert+extend r 'y t)
        (list before copy-before resized
              (list (ring-elements r) (ring-length r) (ring-size r))
              (ring-elements copy))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((newest newer oldest) 3 4) (t nil nil) ((nil newest) 2 2) ((y x nil newest) 4 4) (newest newer oldest))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_ring_member_next_previous_and_remove_insert_extend() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'ring)
  (let ((r (make-ring 3)))
    (dolist (x '(a b c))
      (ring-insert r x))
    (let ((initial (list (ring-elements r)
                         (ring-member r 'c)
                         (ring-member r 'a)
                         (ring-member r 'missing)
                         (ring-next r 'c)
                         (ring-previous r 'c))))
      (ring-remove+insert+extend r 'b nil)
      (let ((moved-no-grow (list (ring-elements r) (ring-size r))))
        (ring-remove+insert+extend r 'a t)
        (ring-remove+insert+extend r 'd t)
        (list initial moved-no-grow
              (list (ring-elements r) (ring-size r)))))))
"#;

    let expect =
        expect_test::expect![[r#""OK (((c b a) 0 2 nil b a) ((b c a) 3) ((d a b c) 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_ring_errors_and_sequence_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'ring)
  (let ((empty (make-ring 2)))
    (list
     (ring-p empty)
     (ring-empty-p empty)
     (condition-case err
         (ring-ref empty 0)
       (error (list (car err) (cadr err))))
     (condition-case err
         (ring-remove empty)
       (error (list (car err) (cadr err))))
     (condition-case err
         (ring-next empty 'x)
       (error (list (car err) (cadr err))))
     (let ((r (ring-convert-sequence-to-ring '(a a b b c a))))
       (list (ring-elements r) (ring-size r) (ring-p r)))
     (let ((r2 (make-ring 1)))
       (eq r2 (ring-convert-sequence-to-ring r2))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t (error \"Accessing an empty ring\") (error \"Ring empty\") (error \"Item is not in the ring: ‘x’\") ((a b b c) 6 t) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
