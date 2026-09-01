//! Oracle parity tests for GNU `subr.el` `add-minor-mode` semantics.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_add_minor_mode_inserts_after_and_replaces_existing_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((map-a (make-sparse-keymap))
       (map-anchor (make-sparse-keymap))
       (map-omega (make-sparse-keymap))
       (map-new (make-sparse-keymap))
       (map-repl (make-sparse-keymap))
       (minor-mode-list '(alpha anchor omega))
       (minor-mode-alist (copy-tree '((alpha " A") (anchor " Anchor") (omega " O"))))
       (minor-mode-map-alist (list (cons 'alpha map-a)
                                   (cons 'anchor map-anchor)
                                   (cons 'omega map-omega))))
  (put 'neo-mode :minor-mode-function nil)
  (add-minor-mode 'neo-mode " Neo" map-new 'anchor 'neo-toggle)
  (let ((after-add
         (list (copy-sequence minor-mode-list)
               (copy-tree minor-mode-alist)
               (mapcar (lambda (cell)
                         (cons (car cell)
                               (cond ((eq (cdr cell) map-new) 'new)
                                     ((eq (cdr cell) map-repl) 'repl)
                                     ((eq (cdr cell) map-anchor) 'anchor-map)
                                     ((eq (cdr cell) map-a) 'alpha-map)
                                     ((eq (cdr cell) map-omega) 'omega-map)
                                     (t 'other))))
                       minor-mode-map-alist)
               (get 'neo-mode :minor-mode-function))))
    (add-minor-mode 'neo-mode " New" map-repl nil)
    (list after-add
          minor-mode-list
          minor-mode-alist
          (mapcar (lambda (cell)
                    (cons (car cell)
                          (cond ((eq (cdr cell) map-new) 'new)
                                ((eq (cdr cell) map-repl) 'repl)
                                ((eq (cdr cell) map-anchor) 'anchor-map)
                                ((eq (cdr cell) map-a) 'alpha-map)
                                ((eq (cdr cell) map-omega) 'omega-map)
                                (t 'other))))
                  minor-mode-map-alist)
          (get 'neo-mode :minor-mode-function))))"#;
    let expect = expect_test::expect![[
        r#""OK (((neo-mode alpha anchor omega) ((alpha \" A\") (anchor \" Anchor\") (neo-mode \" Neo\") (omega \" O\")) ((alpha . alpha-map) (anchor . anchor-map) (neo-mode . new) (omega . omega-map)) neo-toggle) (neo-mode alpha anchor omega) ((alpha \" A\") (anchor \" Anchor\") (neo-mode \" New\") (omega \" O\")) ((alpha . alpha-map) (anchor . anchor-map) (neo-mode . repl) (omega . omega-map)) neo-toggle)""#
    ]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        r#"(((neo-mode alpha anchor omega) ((alpha " A") (anchor " Anchor") (neo-mode " Neo") (omega " O")) ((alpha . alpha-map) (anchor . anchor-map) (neo-mode . new) (omega . omega-map)) neo-toggle) (neo-mode alpha anchor omega) ((alpha " A") (anchor " Anchor") (neo-mode " New") (omega " O")) ((alpha . alpha-map) (anchor . anchor-map) (neo-mode . repl) (omega . omega-map)) neo-toggle)"#,
        &oracle,
        &neovm,
    );
}
