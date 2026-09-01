//! Oracle parity tests for GNU `expand-file-name` edge semantics.
//!
//! GNU implements `expand-file-name` in `src/fileio.c`.  It consults the
//! Lisp-visible environment via `get_homedir`, preserves unregistered `~USER`
//! spellings as relative path components, falls back to root when
//! `default-directory` is not a string, and rejects null bytes before path
//! canonicalization.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_expand_file_name_home_default_and_null_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((process-environment (copy-sequence process-environment))
      (default-directory "/tmp/neomacs-oracle-base/dir/"))
  (setenv "HOME" "/tmp/neomacs-oracle-home")
  (list
   (expand-file-name "~")
   (expand-file-name "~/alpha/../beta")
   (expand-file-name "~definitely-no-such-neomacs-oracle-user/leaf"
                     "/tmp/neomacs-oracle-base/")
   (expand-file-name "" "/tmp/neomacs-oracle-base/dir/")
   (expand-file-name "relative" nil)
   (let ((default-directory nil))
     (expand-file-name "relative"))
   (expand-file-name "../x/./y/" "/tmp/neomacs-oracle-base/dir")
   (expand-file-name "///triple//slash" "/ignored")
   (condition-case err
       (expand-file-name 42)
     (error (list (car err) (cdr err))))
   (condition-case err
       (expand-file-name (string ?a 0 ?b) "/tmp/neomacs-oracle-base/")
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"/tmp/neomacs-oracle-home\" \"/tmp/neomacs-oracle-home/beta\" \"/tmp/neomacs-oracle-base/~definitely-no-such-neomacs-oracle-user/leaf\" \"/tmp/neomacs-oracle-base/dir\" \"/tmp/neomacs-oracle-base/dir/relative\" \"/relative\" \"/tmp/neomacs-oracle-base/x/y/\" \"/triple/slash\" (wrong-type-argument (stringp 42)) (wrong-type-argument (filenamep \"a\\0b\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_expand_file_name_root_and_relative_default_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; GNU documents this root behavior explicitly: `..' is not always a
 ;; filesystem parent traversal after canonicalization.
 (expand-file-name ".." "/")
 (expand-file-name "../.." "/")
 (expand-file-name "a/../../b" "/")
 ;; Relative `default-directory' values are first expanded against
 ;; `invocation-directory' when the buffer default is used.
 (let ((default-directory "relative/base/"))
   (let ((expanded (expand-file-name "leaf")))
     (list (file-name-absolute-p expanded)
           (string-suffix-p "/relative/base/leaf" expanded))))
 ;; Explicit relative DEFAULT-DIRECTORY is recursively expanded against the
 ;; current buffer default directory, then NAME is appended.
 (let ((expanded (expand-file-name "leaf" "relative/default")))
   (list (file-name-absolute-p expanded)
         (string-suffix-p "/relative/default/leaf" expanded)))
 ;; GNU rejects a non-string buffer-local `default-directory` before it can be
 ;; used as the implicit DEFAULT-DIRECTORY.
 (condition-case err
     (let ((default-directory 42))
       (expand-file-name "leaf"))
   (error (list (car err) (cdr err))))
 ;; GNU treats an explicit bad DEFAULT-DIRECTORY as root instead of signaling.
 (condition-case err
     (expand-file-name "leaf" 42)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"/..\" \"/\" \"/../b\" (t t) (t t) (wrong-type-argument (stringp 42)) \"/leaf\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_expand_file_name_handler_dispatch_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (setq neomacs--oracle-expand-file-name-calls nil)
  (defun neomacs--oracle-expand-file-name-handler (operation &rest args)
    (push (cons operation args) neomacs--oracle-expand-file-name-calls)
    (cond
     ((eq operation 'expand-file-name)
      (concat "/handled/"
              (number-to-string (length neomacs--oracle-expand-file-name-calls))))
     (t
      (let ((file-name-handler-alist nil))
        (apply operation args)))))
  (unwind-protect
      (let ((file-name-handler-alist
             '(("\\`/oracle-expand:" . neomacs--oracle-expand-file-name-handler))))
        (list
         ;; GNU first checks NAME for an `expand-file-name' handler.
         (expand-file-name "/oracle-expand:name" "/plain/default/")
         neomacs--oracle-expand-file-name-calls
         (setq neomacs--oracle-expand-file-name-calls nil)
         ;; If NAME is ordinary, GNU checks DEFAULT-DIRECTORY next.
         (expand-file-name "leaf" "/oracle-expand:default/")
         neomacs--oracle-expand-file-name-calls
         (setq neomacs--oracle-expand-file-name-calls nil)
         ;; If DEFAULT-DIRECTORY is relative, GNU recursively expands it and
         ;; gives the current buffer default's handler a chance to handle that
         ;; recursive expansion before appending NAME.
         (let ((default-directory "/oracle-expand:base/"))
           (expand-file-name "leaf" "relative/default/"))
         neomacs--oracle-expand-file-name-calls
         (setq neomacs--oracle-expand-file-name-calls nil)
         ;; GNU's recursive relative-default guard uses object identity
         ;; (`EQ'), not string equality.
         (let* ((same (copy-sequence "relative/default/"))
                (left (copy-sequence "relative/default/"))
                (right (copy-sequence "relative/default/")))
           (list (expand-file-name same same)
                 (expand-file-name left right)))
         ;; GNU requires an `expand-file-name' handler to return a string.
         (let ((file-name-handler-alist
                '(("\\`/oracle-expand:" . (lambda (&rest _args) 42)))))
           (condition-case err
               (expand-file-name "/oracle-expand:bad" "/plain/default/")
             (error (list (car err) (cdr err)))))))
    (fmakunbound 'neomacs--oracle-expand-file-name-handler)
    (makunbound 'neomacs--oracle-expand-file-name-calls)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"/handled/1\" ((expand-file-name \"/oracle-expand:name\" \"/plain/default/\")) nil \"/handled/1\" ((expand-file-name \"leaf\" \"/oracle-expand:default/\")) nil \"/handled/1/leaf\" ((expand-file-name \"relative/default/\" \"/oracle-expand:base/\")) nil (\"relative/default/relative/default/\" \"[ORACLE-PROJECT-ROOT]/crates/neovm-oracle-tests/relative/default/relative/default/\") (error (\"Invalid handler in ‘file-name-handler-alist’\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
