//! Oracle parity tests for GNU `locate-file` and `locate-file-internal`.
//!
//! GNU exposes `locate-file` from `lisp/files.el`, backed by
//! `locate-file-internal` in `src/lread.c` via `openp`.  Important observable
//! behavior includes optional suffixes, nil path entries using
//! `default-directory`, callable predicates filtering candidates, directory
//! rejection unless the predicate returns `dir-ok`, and the public two-argument
//! `locate-file` call where suffixes defaults to nil.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_locate_file_suffix_path_predicate_and_arity_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((root (make-temp-file "neomacs-oracle-locate-file-" t))
       (d1 (expand-file-name "d1" root))
       (d2 (expand-file-name "d2" root))
       (default-directory (file-name-as-directory d1)))
  (unwind-protect
      (progn
        (make-directory d1)
        (make-directory d2)
        (write-region "plain" nil (expand-file-name "plain" d1) nil 'silent)
        (write-region "el" nil (expand-file-name "tool.el" d1) nil 'silent)
        (write-region "elc" nil (expand-file-name "tool.elc" d2) nil 'silent)
        (write-region "txt" nil (expand-file-name "tool.txt" d2) nil 'silent)
        (make-directory (expand-file-name "adir" d1))
        (let ((rel (lambda (file)
                     (and file (file-relative-name file root)))))
          (list
           ;; GNU `locate-file` accepts two required args; nil suffixes means
           ;; the empty suffix list.
           (funcall rel (locate-file "plain" (list d1)))
           (funcall rel (locate-file "tool" (list d2 d1) '(".el" ".elc")))
           (funcall rel (locate-file-internal "tool" (list d2 d1) '(".txt" ".el")))
           ;; A nil PATH element is expanded against `default-directory`.
           (funcall rel (locate-file "plain" (list nil) nil))
           ;; Callable predicates can reject existing candidates.
           (locate-file "plain" (list d1) nil (lambda (_file) nil))
           (funcall rel
                    (locate-file "plain" (list d1) nil
                                 (lambda (file)
                                   (and (string-match-p "plain\\'" file)
                                        t))))
           ;; Directories are skipped unless the predicate returns `dir-ok`.
           (locate-file "adir" (list d1) nil)
           (locate-file "adir" (list d1) nil #'file-directory-p)
           (funcall rel
                    (locate-file "adir" (list d1) nil
                                 (lambda (file)
                                   (and (file-directory-p file) 'dir-ok))))
           (condition-case err
               (locate-file)
             (error (list (car err) (cdr err))))
           (condition-case err
               (locate-file 42 nil)
             (error (list (car err) (cdr err))))
           (condition-case err
               (locate-file "x" 42)
             (error (list (car err) (cdr err))))
           (condition-case err
               (locate-file "x" nil '(42))
             (error (list (car err) (cdr err)))))))
    (delete-directory root t)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"d1/plain\" \"d2/tool.elc\" \"d2/tool.txt\" \"d1/plain\" nil \"d1/plain\" nil nil \"d1/adir\" (wrong-number-of-arguments ((2 . 4) 0)) (wrong-type-argument (stringp 42)) nil (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
