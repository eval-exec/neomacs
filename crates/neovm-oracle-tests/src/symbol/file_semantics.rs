//! Oracle parity tests for GNU `symbol-file` semantics.
//!
//! GNU implements `symbol-file` in `lisp/subr.el`.  It checks autoloaded
//! function cells through `locate-library`, then searches `load-history` for
//! variables, functions, faces, arbitrary property types, and
//! `define-symbol-props` entries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_symbol_file_reads_load_history_for_core_and_custom_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((load-history
       '(("/tmp/neomacs-oracle-symbol-file-a.el"
          neomacs--oracle-symbol-file-var
          (defun . neomacs--oracle-symbol-file-fun)
          (defface . neomacs--oracle-symbol-file-face)
          (custom-prop . neomacs--oracle-symbol-file-custom)
          (require . neomacs--oracle-symbol-file-required)
          (define-symbol-props
           (custom-prop neomacs--oracle-symbol-file-defined-prop)))
         ("/tmp/neomacs-oracle-symbol-file-b.el"
          neomacs--oracle-symbol-file-other))))
  (list
   (symbol-file 'neomacs--oracle-symbol-file-var)
   (symbol-file 'neomacs--oracle-symbol-file-var 'defvar)
   (symbol-file 'neomacs--oracle-symbol-file-fun)
   (symbol-file 'neomacs--oracle-symbol-file-fun 'defun)
   (symbol-file 'neomacs--oracle-symbol-file-face 'defface)
   (symbol-file 'neomacs--oracle-symbol-file-custom 'custom-prop)
   (symbol-file 'neomacs--oracle-symbol-file-defined-prop 'custom-prop)
   (symbol-file 'neomacs--oracle-symbol-file-required)
   (symbol-file 'neomacs--oracle-symbol-file-missing)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"/tmp/neomacs-oracle-symbol-file-a.el\" \"/tmp/neomacs-oracle-symbol-file-a.el\" \"/tmp/neomacs-oracle-symbol-file-a.el\" \"/tmp/neomacs-oracle-symbol-file-a.el\" \"/tmp/neomacs-oracle-symbol-file-a.el\" \"/tmp/neomacs-oracle-symbol-file-a.el\" \"/tmp/neomacs-oracle-symbol-file-a.el\" nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_symbol_file_autoload_uses_locate_library_not_raw_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (progn
      (autoload 'neomacs--oracle-symbol-file-autoload
        "definitely-missing-neomacs-oracle-library")
      (list
       (autoloadp (symbol-function 'neomacs--oracle-symbol-file-autoload))
       (symbol-file 'neomacs--oracle-symbol-file-autoload)
       (symbol-file 'neomacs--oracle-symbol-file-autoload 'defun)
       (symbol-file 'neomacs--oracle-symbol-file-autoload 'defvar)))
  (when (fboundp 'neomacs--oracle-symbol-file-autoload)
    (fmakunbound 'neomacs--oracle-symbol-file-autoload)))
"#;

    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_symbol_file_accepts_non_symbol_and_native_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((load-history
       '(("/tmp/neomacs-oracle-symbol-file-native.el"
          (defun . neomacs--oracle-symbol-file-native-fun)))))
  (list
   (symbol-file 42)
   (symbol-file "not-a-symbol")
   (symbol-file 'neomacs--oracle-symbol-file-native-fun 'defun)
   (symbol-file 'neomacs--oracle-symbol-file-native-fun 'defun t)
   (condition-case err
       (symbol-file 'neomacs--oracle-symbol-file-native-fun 'defun t :extra)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil nil \"/tmp/neomacs-oracle-symbol-file-native.el\" \"/tmp/neomacs-oracle-symbol-file-native.el\" (wrong-number-of-arguments ((1 . 3) 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
