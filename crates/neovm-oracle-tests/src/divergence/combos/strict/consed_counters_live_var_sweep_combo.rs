//! v0.0.21 #778: the `*-consed` DEFVAR_INTs are live allocation counters.
//!
//! GNU binds the seven variables to the same `intmax_t` slots that
//! `memory-use-counts` reports (`src/alloc.c:7429-7448`). The probe asserts the
//! observable contract rather than absolute counts (which differ between any
//! two editors): after one allocation of each kind the variable advances, and
//! a read of the variable agrees with its `(memory-use-counts)` field at the
//! same instant. Connes are advanced but not compared for exact equality: the
//! `memory-use-counts` call itself conses the returned list, so the cons cell
//! counter moves between the two reads. Every printed element is a boolean, so
//! the oracle comparison is exact on both editors.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_consed_counters_are_live_and_match_memory_use_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((index 0) (same nil) (advanced nil))
  (dolist (sym '(cons-cells-consed floats-consed vector-cells-consed
                 symbols-consed string-chars-consed intervals-consed
                 strings-consed))
    (let ((v (symbol-value sym)))
      ;; CONSES is skipped for exact equality: building the returned list of
      ;; the memory-use-counts call conses between the two reads.
      (when (> index 0)
        (push (= v (nth index (memory-use-counts))) same)))
    (setq index (1+ index)))
  (setq index 0)
  (dolist (case '((cons-cells-consed    . (lambda () (make-list 1000 nil)))
                  (floats-consed        . (lambda () (dotimes (i 1000) (* 1.5 i))))
                  (vector-cells-consed  . (lambda () (make-vector 1000 nil)))
                  (symbols-consed       . (lambda () (dotimes (_ 1000) (make-symbol "s"))))
                  (string-chars-consed  . (lambda () (make-string 1000 ?x)))
                  (intervals-consed     . (lambda () (dotimes (_ 1000) (propertize "x" 'face 'bold))))
                  (strings-consed       . (lambda () (dotimes (_ 1000) (make-string 10 ?x))))))
    (let ((v0 (symbol-value (car case))))
      (funcall (eval (cdr case) t))
      (push (> (symbol-value (car case)) v0) advanced))
    (setq index (1+ index)))
  (list (nreverse same) (nreverse advanced)))
"##;
    let expect = expect_test::expect![[r#""OK ((t t t t t t) (t t t t t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
