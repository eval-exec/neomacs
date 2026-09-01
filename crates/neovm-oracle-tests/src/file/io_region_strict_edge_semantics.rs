//! Oracle parity tests for GNU file read/write region semantics.
//!
//! GNU implements `insert-file-contents` and `write-region` in `src/fileio.c`;
//! `append-to-file` is a Lisp wrapper in `lisp/files.el`.  These tests keep
//! byte-range reads, string writes, numeric append offsets, non-nil append,
//! replace mode, and exact wrapper behavior visible.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_write_region_insert_file_contents_and_append_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((root (make-temp-file "neomacs-oracle-file-io-region-" t))
       (file (expand-file-name "data.txt" root))
       (target (expand-file-name "target.txt" root))
       (existing (expand-file-name "existing.txt" root)))
  (cl-labels
      ((rel (name)
            (and (stringp name)
                 (file-relative-name name root)))
       (norm (x)
             (cond
              ((stringp x)
               (if (string-prefix-p root x) (rel x) x))
              ((consp x)
               (cons (norm (car x)) (norm (cdr x))))
              (t x)))
       (read-file (name)
          (with-temp-buffer
            (insert-file-contents-literally name)
            (buffer-string))))
    (unwind-protect
        (progn
          ;; String START ignores END and writes that string directly.
          (write-region "abcdef" 'ignored file nil 'silent)
          ;; Numeric APPEND seeks to the byte offset and overwrites without
          ;; truncating the remaining file tail.
          (write-region "XY" nil file 2 'silent)
          ;; Non-nil nonnumeric APPEND appends to existing contents.
          (write-region "ZZ" nil file t 'silent)
          ;; Region writes use START/END buffer positions.
          (with-temp-buffer
            (insert "0123456789")
            (write-region 3 7 target nil 'silent))
          ;; append-to-file delegates to write-region with append=t.
          (with-temp-buffer
            (insert "pre")
            (append-to-file (point-min) (point-max) target)
            (append-to-file "post" nil target))
          (write-region "exists" nil existing nil 'silent)
          (list
           (read-file file)
           (read-file target)
           ;; BEG and END are byte offsets in the file, and the return value
           ;; is (absolute-filename inserted-character-count).
           (with-temp-buffer
             (let ((ret (insert-file-contents file nil 1 5)))
               (list (list (rel (car ret)) (cadr ret))
                     (buffer-string))))
           ;; REPLACE replaces the accessible buffer contents and reports the
           ;; number of characters replacing the previous accessible text.
           (with-temp-buffer
             (insert "old contents")
             (let ((ret (insert-file-contents file nil nil nil t)))
               (list (list (rel (car ret)) (cadr ret))
                     (buffer-string)
                     (point))))
           ;; Visit cannot be combined with BEG/END, per GNU fileio.c.
           (norm
            (condition-case err
                (with-temp-buffer
                  (insert-file-contents file t 1 nil))
              (error (list (car err) (cdr err)))))
           ;; MUSTBENEW='excl signals if the destination already exists.
           (norm
            (condition-case err
                (write-region "new" nil existing nil 'silent nil 'excl)
              (error (list (car err) (cdr err)))))
           (condition-case err
               (write-region)
             (error (list (car err) (cdr err))))
           (condition-case err
               (insert-file-contents)
             (error (list (car err) (cdr err))))
           (condition-case err
               (append-to-file)
             (error (list (car err) (cdr err)))))))
      (ignore-errors (delete-directory root t)))))
"#;

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_write_region_intersperses_annotation_hook_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((file (make-temp-file "neomacs-write-region-annotation-")))
  (unwind-protect
      (with-temp-buffer
        (insert "line\n")
        (let ((write-region-annotate-functions
               (list (lambda (start _end)
                       (list (cons start "[STAMP] "))))))
          (write-region (point-min) (point-max) file nil 'silent))
        (with-temp-buffer
          (insert-file-contents file)
          (buffer-string)))
    (delete-file file)))
"#;

    let expect = expect_test::expect![[r#""OK \"[STAMP] line\\n\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
