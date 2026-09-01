//! Strict oracle parity for small GNU `subr.el` helpers.
//!
//! These helpers are pure Lisp in GNU Emacs.  The cases below target exact
//! regex/path handling, dynamic variable dependence, and hash/obarray side
//! effects that are easy to approximate incorrectly.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_subr_misc_package_unmsys_prefix_apropos_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (mapcar #'package--description-file
         '("/tmp/pkg-1.2.3/"
           "/tmp/pkg-1pre2"
           "/tmp/pkg-1.0beta3"
           "/tmp/pkg-1snapshot4"
           "/tmp/.hidden-1.2"
           "/tmp/pkg-nover"
           "/tmp/pkg-1.2.3-extra"
           "/tmp/pkg-20200101"))
 (let ((system-type 'gnu/linux))
   (mapcar #'unmsys--file-name
           '("/c/foo/bar" "/C/foo" "/notdrive/foo" "relative")))
 (let ((system-type 'windows-nt))
   (mapcar #'unmsys--file-name
           '("/c/foo/bar" "/C/foo" "/notdrive/foo" "relative")))
 (let ((definition-prefixes (make-hash-table :test 'equal)))
   (register-definition-prefixes "a.el" '("foo-" "bar-"))
   (register-definition-prefixes "b.el" '("foo-"))
   (list (gethash "foo-" definition-prefixes)
         (gethash "bar-" definition-prefixes)
         (gethash "missing-" definition-prefixes)))
 (let ((obarray (make-vector 17 0)))
   (set (intern "nmo-alpha") 1)
   (intern "nmo-beta")
   (intern "other")
   (list (apropos-internal "\\`nmo-")
         (apropos-internal "\\`nmo-" #'boundp))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"pkg-pkg.el\" \"pkg-pkg.el\" \"pkg-pkg.el\" \"pkg-pkg.el\" \"hidden-pkg.el\" \"pkg-nover-pkg.el\" \"pkg-1.2.3-extra-pkg.el\" \"pkg-pkg.el\") (\"/c/foo/bar\" \"/C/foo\" \"/notdrive/foo\" \"relative\") (\"c:/foo/bar\" \"C:/foo\" \"/notdrive/foo\" \"relative\") ((\"b.el\" \"a.el\") (\"a.el\") nil) ((nmo-alpha nmo-beta) (nmo-alpha)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_system_type_dynamic_binding_reaches_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (special-variable-p 'system-type)
 (let ((system-type 'windows-nt))
   (list system-type
         (symbol-value 'system-type)
         (funcall (lambda () system-type))))
 (progn
   (defun neomacs--oracle-system-type-reader ()
     system-type)
   (unwind-protect
       (let ((system-type 'windows-nt))
         (list (neomacs--oracle-system-type-reader)
               (funcall #'neomacs--oracle-system-type-reader)))
     (fmakunbound 'neomacs--oracle-system-type-reader))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t (windows-nt windows-nt windows-nt) (windows-nt windows-nt))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_defvar_lisp_runtime_variables_are_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (mapcar (lambda (sym)
           (list sym (boundp sym) (special-variable-p sym)))
         '(system-type
           system-configuration
           system-configuration-options
           system-configuration-features
           emacs-version
           system-name
           operating-system-release
           command-line-args
           user-full-name
           user-login-name
           user-real-login-name
           overriding-plist-environment))
 (let ((system-configuration "oracle-config")
       (system-configuration-options "oracle-options")
       (emacs-version "99.99-oracle")
       (system-name "oracle-host")
       (operating-system-release "oracle-kernel")
       (command-line-args '("oracle-emacs" "--flag"))
       (overriding-plist-environment '((oracle-symbol oracle-prop oracle-value))))
   (list (symbol-value 'system-configuration)
         (funcall (lambda () system-configuration-options))
         emacs-version
         (funcall (lambda () system-name))
         operating-system-release
         command-line-args
         (symbol-value 'overriding-plist-environment)
         (funcall (lambda () overriding-plist-environment)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((system-type t t) (system-configuration t t) (system-configuration-options t t) (system-configuration-features t t) (emacs-version t t) (system-name t t) (operating-system-release t t) (command-line-args t t) (user-full-name t t) (user-login-name t t) (user-real-login-name t t) (overriding-plist-environment t t)) (\"oracle-config\" \"oracle-options\" \"99.99-oracle\" \"oracle-host\" \"oracle-kernel\" (\"oracle-emacs\" \"--flag\") ((oracle-symbol oracle-prop oracle-value)) ((oracle-symbol oracle-prop oracle-value))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_defvar_bool_int_runtime_variables_are_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (mapcar (lambda (sym)
           (list sym (boundp sym) (special-variable-p sym)))
         '(case-fold-search
           debug-on-error
           gc-cons-threshold
           max-lisp-eval-depth
           max-specpdl-size
           inhibit-quit
           noninteractive
           purify-flag))
 (let ((case-fold-search nil)
       (debug-on-error t)
       (gc-cons-threshold 1234567)
       (max-lisp-eval-depth 9876)
       (max-specpdl-size 8765)
       (inhibit-quit t)
       (noninteractive nil)
       (purify-flag t))
   (list (symbol-value 'case-fold-search)
         (funcall (lambda () debug-on-error))
         gc-cons-threshold
         (funcall (lambda () max-lisp-eval-depth))
         max-specpdl-size
         inhibit-quit
         noninteractive
         purify-flag)))
"#;

    let expect = expect_test::expect![[
        r#""OK (((case-fold-search t t) (debug-on-error t t) (gc-cons-threshold t t) (max-lisp-eval-depth t t) (max-specpdl-size t t) (inhibit-quit t t) (noninteractive t t) (purify-flag t t)) (nil t 1234567 9876 8765 t nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_defvar_per_buffer_special_and_local_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (mapcar (lambda (sym)
           (list sym (boundp sym) (special-variable-p sym) (local-variable-p sym)))
         '(major-mode
           mode-name
           fill-column
           tab-width
           default-directory
           buffer-file-name
           buffer-read-only
           buffer-undo-list
           cursor-type
           truncate-lines))
 (let ((fill-column 17)
       (tab-width 3)
       (major-mode 'oracle-mode)
       (mode-name "Oracle"))
   (list (symbol-value 'fill-column)
         (funcall (lambda () tab-width))
         major-mode
         mode-name))
 (with-temp-buffer
   (setq-local fill-column 33)
   (setq-local tab-width 5)
   (setq-local major-mode 'temp-oracle-mode)
   (setq-local mode-name "Temp Oracle")
   (list fill-column
         tab-width
         major-mode
         mode-name
         (local-variable-p 'fill-column)
         (local-variable-p 'major-mode)
         (buffer-local-value 'fill-column (current-buffer))
         (default-value 'fill-column))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((major-mode t t t) (mode-name t t t) (fill-column t t nil) (tab-width t t nil) (default-directory t t t) (buffer-file-name t t t) (buffer-read-only t t t) (buffer-undo-list t t t) (cursor-type t t nil) (truncate-lines t t nil)) (17 3 oracle-mode \"Oracle\") (33 5 temp-oracle-mode \"Temp Oracle\" t t 33 70))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
