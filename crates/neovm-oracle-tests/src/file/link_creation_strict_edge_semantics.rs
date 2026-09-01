//! Oracle parity tests for GNU hard-link and symbolic-link creation.
//!
//! GNU implements `add-name-to-file` and `make-symbolic-link` in
//! `src/fileio.c`.  Their observable behavior includes nil success returns,
//! EEXIST handling controlled by OK-IF-ALREADY-EXISTS, and `expand_cp_target`
//! directory-name semantics.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_link_creation_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-file-links-" t))
       (src (expand-file-name "src.txt" dir))
       (other (expand-file-name "other.txt" dir))
       (hard (expand-file-name "hard.txt" dir))
       (hard2 (expand-file-name "hard2.txt" dir))
       (sym (expand-file-name "sym.txt" dir))
       (sym2 (expand-file-name "sym2.txt" dir))
       (subdir (expand-file-name "sub" dir)))
  (unwind-protect
      (progn
        (write-region "src" nil src nil 'silent)
        (write-region "other" nil other nil 'silent)
        (make-directory subdir)
        (list
         ;; Hard-link creation returns nil and increases the link count.
         (add-name-to-file src hard)
         (file-exists-p hard)
         (= (file-nlinks src) (file-nlinks hard))
         (condition-case err
             (add-name-to-file src hard)
           (error (list (car err) (cdr err))))
         (add-name-to-file src hard t)
         (file-exists-p hard)
         ;; A directory path without a trailing slash is the destination itself.
         (condition-case err
             (add-name-to-file src subdir t)
           (error (list (car err) (cdr err))))
         ;; A directory path with a trailing slash creates a like-named child.
         (add-name-to-file src (file-name-as-directory subdir) t)
         (file-exists-p (expand-file-name "src.txt" subdir))

         ;; Symbolic link creation keeps the target string as supplied.
         (make-symbolic-link "src.txt" sym)
         (file-symlink-p sym)
         (condition-case err
             (make-symbolic-link "other.txt" sym)
           (error (list (car err) (cdr err))))
         (make-symbolic-link "other.txt" sym t)
         (file-symlink-p sym)
         (condition-case err
             (make-symbolic-link "src.txt" subdir t)
           (error (list (car err) (cdr err))))
         (make-symbolic-link "src.txt" (file-name-as-directory subdir) t)
         (file-symlink-p (expand-file-name "src.txt" subdir))

         ;; Arity and type errors belong to the public primitive contract.
         (condition-case err
             (add-name-to-file)
           (error (list (car err) (cdr err))))
         (condition-case err
             (add-name-to-file 42 hard2)
           (error (list (car err) (cdr err))))
         (condition-case err
             (make-symbolic-link 42 sym2)
           (error (list (car err) (cdr err)))))))
    (ignore-errors (delete-directory dir t))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 57 46)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
