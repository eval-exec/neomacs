//! Oracle parity tests for GNU file name component splitting.
//!
//! GNU implements `file-name-directory` and `file-name-nondirectory` in
//! `src/fileio.c`.  These functions are syntactic and preserve repeated slash
//! structure in the returned component.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_name_directory_and_nondirectory_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (file-name-directory "")
 (file-name-nondirectory "")
 (file-name-directory "plain")
 (file-name-nondirectory "plain")
 (file-name-directory "/")
 (file-name-nondirectory "/")
 (file-name-directory "//")
 (file-name-nondirectory "//")
 (file-name-directory "///")
 (file-name-nondirectory "///")
 (file-name-directory "a/b")
 (file-name-nondirectory "a/b")
 (file-name-directory "a/b/")
 (file-name-nondirectory "a/b/")
 (file-name-directory "a//b")
 (file-name-nondirectory "a//b")
 (file-name-directory "/a//b")
 (file-name-nondirectory "/a//b")
 (file-name-directory "/a//b/")
 (file-name-nondirectory "/a//b/")
 (condition-case err
     (file-name-directory)
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-name-nondirectory 42)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil \"\" nil \"plain\" \"/\" \"\" \"//\" \"\" \"///\" \"\" \"a/\" \"b\" \"a/b/\" \"\" \"a//\" \"b\" \"/a//\" \"b\" \"/a//b/\" \"\" (wrong-number-of-arguments (file-name-directory 0)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_name_directory_handler_result_contract_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defun neomacs--oracle-file-name-component-handler (operation &rest args)
    (list 'handler operation args))
  (defun neomacs--oracle-file-name-component-bad-handler (operation &rest args)
    42)
  (unwind-protect
      (let ((file-name-handler-alist
             '(("\\`/component:" . neomacs--oracle-file-name-component-handler)
               ("\\`/bad-component:" . neomacs--oracle-file-name-component-bad-handler))))
        (list
         ;; GNU returns a string handler result directly.
         (let ((file-name-handler-alist
                '(("\\`/component:" . (lambda (&rest _) "handled-dir/")))))
           (file-name-directory "/component:path"))
         (let ((file-name-handler-alist
                '(("\\`/component:" . (lambda (&rest _) "handled-base")))))
           (file-name-nondirectory "/component:path"))
         ;; But non-string handler results differ by operation:
         ;; `file-name-directory' coerces to nil, while
         ;; `file-name-nondirectory' signals an invalid handler.
         (file-name-directory "/bad-component:path")
         (condition-case err
             (file-name-nondirectory "/bad-component:path")
           (error (list (car err) (cdr err))))))
    (fmakunbound 'neomacs--oracle-file-name-component-handler)
    (fmakunbound 'neomacs--oracle-file-name-component-bad-handler)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"handled-dir/\" \"handled-base\" nil (error (\"Invalid handler in ‘file-name-handler-alist’\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_name_extension_and_version_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (mapcar (lambda (name)
           (list name
                 (file-name-extension name)
                 (file-name-extension name t)
                 (file-name-sans-extension name)
                 (file-name-base name)
                 (file-name-sans-versions name)
                 (file-name-sans-versions name t)))
         '("plain"
           "plain."
           ".emacs"
           ".emacs.el"
           "archive.tar.gz"
           "/tmp/archive.tar.gz"
           "/tmp/.hidden"
           "/tmp/.hidden.el"
           "/tmp/dir.with.dots/file"
           "/tmp/dir.with.dots/file."
           "foo.~12~"
           "foo.el.~12~"
           "foo.el.~12~.~3~"
           "foo.~~"
           "foo.js.~HEAD~1~"))
 (condition-case err
     (file-name-extension 42)
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-name-extension "x" nil nil)
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-name-sans-extension 42)
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-name-sans-versions 42)
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-name-base)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((\"plain\" nil \"\" \"plain\" \"plain\" \"plain\" \"plain\") (\"plain.\" \"\" \".\" \"plain\" \"plain\" \"plain.\" \"plain.\") (\".emacs\" nil \"\" \".emacs\" \".emacs\" \".emacs\" \".emacs\") (\".emacs.el\" \"el\" \".el\" \".emacs\" \".emacs\" \".emacs.el\" \".emacs.el\") (\"archive.tar.gz\" \"gz\" \".gz\" \"archive.tar\" \"archive.tar\" \"archive.tar.gz\" \"archive.tar.gz\") (\"/tmp/archive.tar.gz\" \"gz\" \".gz\" \"/tmp/archive.tar\" \"archive.tar\" \"/tmp/archive.tar.gz\" \"/tmp/archive.tar.gz\") (\"/tmp/.hidden\" nil \"\" \"/tmp/.hidden\" \".hidden\" \"/tmp/.hidden\" \"/tmp/.hidden\") (\"/tmp/.hidden.el\" \"el\" \".el\" \"/tmp/.hidden\" \".hidden\" \"/tmp/.hidden.el\" \"/tmp/.hidden.el\") (\"/tmp/dir.with.dots/file\" nil \"\" \"/tmp/dir.with.dots/file\" \"file\" \"/tmp/dir.with.dots/file\" \"/tmp/dir.with.dots/file\") (\"/tmp/dir.with.dots/file.\" \"\" \".\" \"/tmp/dir.with.dots/file\" \"file\" \"/tmp/dir.with.dots/file.\" \"/tmp/dir.with.dots/file.\") (\"foo.~12~\" nil \"\" \"foo.~12~\" \"foo.~12~\" \"foo\" \"foo.~12~\") (\"foo.el.~12~\" \"el\" \".el\" \"foo\" \"foo\" \"foo.el\" \"foo.el.~12~\") (\"foo.el.~12~.~3~\" \"~12~\" \".~12~\" \"foo.el\" \"foo.el\" \"foo.el.~12~\" \"foo.el.~12~.~3~\") (\"foo.~~\" \"~\" \".~\" \"foo\" \"foo\" \"foo.~\" \"foo.~~\") (\"foo.js.~HEAD~1~\" \"js\" \".js\" \"foo\" \"foo\" \"foo.js\" \"foo.js.~HEAD~1~\")) (wrong-type-argument (stringp 42)) (wrong-number-of-arguments ((1 . 2) 3)) (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_name_with_extension_strict_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (file-name-with-extension "plain" "el")
 (file-name-with-extension "plain" ".el")
 (file-name-with-extension "plain.txt" "el")
 (file-name-with-extension "archive.tar.gz" "xz")
 (file-name-with-extension "/tmp/archive.tar.gz" ".xz")
 (file-name-with-extension "/tmp/.hidden" "el")
 (file-name-with-extension "/tmp/.hidden.el" "txt")
 (file-name-with-extension "foo.~12~" "el")
 (file-name-with-extension "foo.el.~12~" "txt")
 (condition-case err
     (file-name-with-extension "" "el")
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-name-with-extension "plain" "")
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-name-with-extension "plain" ".")
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-name-with-extension "/tmp/dir/" "el")
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-name-with-extension 42 "el")
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-name-with-extension "plain" 42)
   (error (list (car err) (cdr err))))
 (condition-case err
     (file-name-with-extension "plain")
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"plain.el\" \"plain.el\" \"plain.el\" \"archive.tar.xz\" \"/tmp/archive.tar.xz\" \"/tmp/.hidden.el\" \"/tmp/.hidden.txt\" \"foo.~12~.el\" \"foo.txt\" (error (\"Empty filename\")) (error (\"Malformed extension: \")) (error (\"Malformed extension: .\")) (error (\"Filename is a directory: /tmp/dir/\")) (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)) (wrong-number-of-arguments ((2 . 2) 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_name_split_and_parent_directory_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((default-directory "/tmp/neomacs-oracle-parent/base/"))
  (list
   (mapcar (lambda (name)
             (list name
                   (file-name-split name)
                   (string-join (file-name-split name) "/")))
           '("" "." ".." "plain" "plain/" "a/b" "a/b/"
             "/"
             "//"
             "///"
             "/a"
             "/a/"
             "/a//b"
             "/a//b/"
             "a//b"
             "a/./b"
             "a/../b"))
   (mapcar (lambda (name)
             (list name (file-name-parent-directory name)))
           '("/" "//" "///" "/a" "/a/" "/a/b" "/a/b/"
             "plain" "plain/" "a/b" "a/b/" "." ".." ""))
   (condition-case err
       (file-name-split 42)
     (error (list (car err) (cdr err))))
   (condition-case err
       (file-name-parent-directory 42)
     (error (list (car err) (cdr err))))
   (condition-case err
       (file-name-parent-directory)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((\"\" nil \"\") (\".\" (\".\") \".\") (\"..\" (\"..\") \"..\") (\"plain\" (\"plain\") \"plain\") (\"plain/\" (\"plain\" \"\") \"plain/\") (\"a/b\" (\"a\" \"b\") \"a/b\") (\"a/b/\" (\"a\" \"b\" \"\") \"a/b/\") (\"/\" (\"\" \"\" \"\") \"//\") (\"//\" (\"/\" \"\" \"\") \"///\") (\"///\" (\"\" \"\" \"\") \"//\") (\"/a\" (\"\" \"a\") \"/a\") (\"/a/\" (\"\" \"a\" \"\") \"/a/\") (\"/a//b\" (\"\" \"a\" \"b\") \"/a/b\") (\"/a//b/\" (\"\" \"a\" \"b\" \"\") \"/a/b/\") (\"a//b\" (\"a\" \"b\") \"a/b\") (\"a/./b\" (\"a\" \".\" \"b\") \"a/./b\") (\"a/../b\" (\"a\" \"..\" \"b\") \"a/../b\")) ((\"/\" nil) (\"//\" nil) (\"///\" nil) (\"/a\" \"/\") (\"/a/\" \"/\") (\"/a/b\" \"/a/\") (\"/a/b/\" \"/a/\") (\"plain\" \"./\") (\"plain/\" \"./\") (\"a/b\" \"a/\") (\"a/b/\" \"a/\") (\".\" \"../\") (\"..\" \"../../\") (\"\" \"../\")) (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)) (wrong-number-of-arguments ((1 . 1) 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
