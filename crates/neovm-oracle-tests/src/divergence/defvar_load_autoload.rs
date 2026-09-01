//! Divergence tests: defvar/defconst, load, provide/require, autoload.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_defvar_only_sets_when_void() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK 100""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-dv-test-1 100)
  (defvar my-dv-test-1 999)
  my-dv-test-1)"#, expect);
}

#[test]
fn divergence_defconst_always_sets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK 999""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defconst my-dc-test-1 100)
  (defconst my-dc-test-1 999)
  my-dc-test-1)"#, expect);
}

#[test]
fn divergence_defvar_inside_let_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK 0""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-dv-shadow 0)
  (let ((my-dv-shadow 42))
    (defvar my-dv-shadow 99)
    (list my-dv-shadow
          (eval 'my-dv-shadow)))
  my-dv-shadow)"#, expect);
}

#[test]
fn divergence_special_variable_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-spec-var 0)
  (list (special-variable-p 'my-spec-var)
        (special-variable-p 'my-nonspec-var)
        (special-variable-p 'load-file-name)
        (special-variable-p 'buffer-read-only)))"#, expect);
}

#[test]
fn divergence_defvar_local_bare_declare() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (nil nil)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ()
  (defvar my-bare-dv)
  (list (special-variable-p 'my-bare-dv)
        (boundp 'my-bare-dv)))"#, expect);
}

#[test]
fn divergence_featurep_after_provide() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (t (my-test-feature gv warnings icons cl-loaddefs cl-lib rmc iso-transl tooltip cconv eldoc paren electric uniquify ediff-hook vc-hooks lisp-float-type elisp-mode mwheel term/x-win x-win term/common-win x-dnd touch-screen tool-bar dnd fontset image regexp-opt fringe tabulated-list replace newcomment text-mode lisp-mode prog-mode register page tab-bar menu-bar rfn-eshadow isearch easymenu timer select scroll-bar mouse jit-lock font-lock syntax font-core term/tty-colors frame minibuffer nadvice seq simple cl-generic indonesian philippine cham georgian utf-8-lang misc-lang vietnamese tibetan thai tai-viet lao korean japanese eucjp-ms cp51932 hebrew greek romanian slovak czech european ethiopic indian cyrillic chinese composite emoji-zwj charscript charprop case-table epa-hook jka-cmpr-hook help abbrev obarray oclosure cl-preloaded button loaddefs theme-loaddefs faces cus-face macroexp files window text-properties overlay sha1 md5 base64 format env code-pages mule custom widget keymap hashtable-print-readable backquote threads dbusbind inotify lcms2 dynamic-setting system-font-setting font-render-setting cairo gtk x-toolkit xinput2 x multi-tty move-toolbar make-network-process tty-child-frames emacs))""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (provide 'my-test-feature)
  (list (featurep 'my-test-feature)
        (member 'my-test-feature features)))"#, expect);
}

#[test]
fn divergence_provide_subfeature() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp (my-test-sub . 42))""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (provide '(my-test-sub . 42))
  (list (featurep 'my-test-sub)
        (featurep 'my-test-sub 42)))"#, expect);
}

#[test]
fn divergence_autoload_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (t t \"doc\")""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (autoload 'my-autoload-fn "nonexistent-file" "doc" t)
  (list (fboundp 'my-autoload-fn)
        (autoloadp (symbol-function 'my-autoload-fn))
        (documentation 'my-autoload-fn)))"#, expect);
}

#[test]
fn divergence_load_path_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (error \"Lisp nesting exceeds ‘max-lisp-eval-depth’\")""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list (member (expand-file-name "emacs-lisp" (car load-path)) load-path)
              (consp load-path)
              (> (length load-path) 0))"#, expect);
}

#[test]
fn divergence_load_file_name_during_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (\"/tmp/nix-shell.XcUf3d/neovm-inline-oracle-sf3ylzib/program-15400-100.el\" t t)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list load-file-name
              load-in-progress
              (booleanp load-in-progress))"#, expect);
}

#[test]
fn divergence_variable_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (void-function variable-binding-alias)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-alias-source 42)
  (defvaralias 'my-alias-target 'my-alias-source)
  (list my-alias-target
        (symbol-value 'my-alias-target)
        (variable-binding-alias 'my-alias-target)
        (setq my-alias-target 99)
        my-alias-source))"#, expect);
}
