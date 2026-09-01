//! Oracle parity tests for GNU `file-nlinks` semantics.
//!
//! GNU implements this in `lisp/files.el` as `(car (cdr (file-attributes
//! filename)))`.  That means missing files and invalid filename objects follow
//! `file-attributes` nil-return behavior rather than signaling here.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_nlinks_regular_hardlink_missing_and_type_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-nlinks-" t))
       (file (expand-file-name "file.txt" dir))
       (hardlink (expand-file-name "hardlink.txt" dir))
       (missing (expand-file-name "missing.txt" dir)))
  (unwind-protect
      (progn
        (write-region "payload" nil file nil 'silent)
        (list
         (file-nlinks file)
         (condition-case err
             (progn
               (add-name-to-file file hardlink)
               (file-nlinks file))
           (error (list (car err) (cdr err))))
         (file-nlinks missing)
         (file-nlinks 42)
         (condition-case err
             (file-nlinks)
           (error (list (car err) (cdr err))))))
    (ignore-errors (delete-file hardlink))
    (ignore-errors (delete-file file))
    (ignore-errors (delete-directory dir))))
"#;

    let expect =
        expect_test::expect![[r#""OK (1 2 nil nil (wrong-number-of-arguments ((1 . 1) 0)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
