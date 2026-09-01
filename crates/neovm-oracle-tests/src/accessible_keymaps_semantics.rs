//! Oracle parity tests for GNU `accessible-keymaps`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_accessible_keymaps_breadth_first_prefixes_and_filtering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/keymap.c:Faccessible_keymaps returns ([] . KEYMAP) first, then
    // walks reachable prefix keymaps breadth-first.  Normalize each entry to
    // its prefix vector/list and a few lookup probes so comparison does not
    // depend on keymap object printing.
    let form = r#"
(let ((root (make-sparse-keymap))
      (cx (make-sparse-keymap))
      (cx4 (make-sparse-keymap))
      (cc (make-sparse-keymap)))
  (define-key root [?\C-x] cx)
  (define-key root [?\C-c] cc)
  (define-key cx [?f] 'find-file)
  (define-key cx [?4] cx4)
  (define-key cx4 [?b] 'switch-to-buffer-other-window)
  (define-key cc [?c] 'compile)
  (list
   (mapcar (lambda (entry)
             (let ((keys (car entry))
                   (map (cdr entry)))
               (list (append keys nil)
                     (keymapp map)
                     (lookup-key map [?f])
                     (lookup-key map [?4])
                     (lookup-key map [?b])
                     (lookup-key map [?c]))))
           (accessible-keymaps root))
   (mapcar (lambda (entry)
             (list (append (car entry) nil)
                   (keymapp (cdr entry))))
           (accessible-keymaps root [?\C-x]))
   (mapcar (lambda (entry)
             (list (append (car entry) nil)
                   (keymapp (cdr entry))))
           (accessible-keymaps root (string ?\C-x)))
   (accessible-keymaps root [?z])))
"#;
    let expect = expect_test::expect![
        r#""OK (((nil t nil nil nil nil) ((3) t nil nil nil compile) ((24) t find-file (keymap (98 . switch-to-buffer-other-window)) nil nil) ((24 52) t nil nil switch-to-buffer-other-window nil)) (((24) t) ((24 52) t)) (((24) t) ((24 52) t)) nil)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_accessible_keymaps_type_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((root (make-sparse-keymap)))
  (list
   (condition-case err
       (accessible-keymaps 42)
     (error (list (car err) (cdr err))))
   (condition-case err
       (accessible-keymaps root t)
     (error (list (car err) (cdr err))))
   (condition-case err
       (accessible-keymaps root '(a))
     (error (list (car err) (cdr err))))
   (condition-case err
       (accessible-keymaps)
     (error (list (car err) (cdr err))))
   (condition-case err
       (accessible-keymaps root [] nil)
     (error (list (car err) (cdr err))))))
"#;
    let expect = expect_test::expect![
        r#""OK ((wrong-type-argument (keymapp 42)) (wrong-type-argument (sequencep t)) (wrong-type-argument (arrayp (a))) (wrong-number-of-arguments (accessible-keymaps 0)) (wrong-number-of-arguments (accessible-keymaps 3)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}
