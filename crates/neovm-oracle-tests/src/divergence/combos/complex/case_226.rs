//! Complex combo batch 226 — `eldoc` / `find-function` / `find-variable` /
//! `find-library` / `xref-find-definitions` source navigation and
//! documentation echo availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx226_eldoc_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'eldoc-mode)
      (boundp 'eldoc-idle-delay)
      (boundp 'eldoc-documentation-function)
      (boundp 'eldoc-echo-area-use-multiline-p))
"##,
        expect,
    );
}

#[test]
fn div_cx226_find_function_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'find-func)
      (list (fboundp 'find-function)
            (fboundp 'find-variable)
            (fboundp 'find-library)
            (fboundp 'find-function-other-window)
            (boundp 'find-function-recenter-line)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx226_find_function_search_for_subr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((loc (find-function-search-for-symbol 'car nil nil)))
      (list (consp loc)
            (bufferp (car loc))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx226_find_variable_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((loc (find-function-search-for-symbol 'load-path nil nil)))
      (list (consp loc)
            (bufferp (car loc))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx226_eldoc_documentation_function_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'eldoc-documentation-functions)
      (boundp 'eldoc-documentation-strategy)
      (boundp 'eldoc-minor-mode-string))
"##,
        expect,
    );
}

#[test]
fn div_cx226_xref_find_definitions_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'xref)
      (list (fboundp 'xref-find-definitions)
            (fboundp 'xref-find-references)
            (fboundp 'xref-pop-marker-stack)
            (fboundp 'xref-find-apropos)
            (boundp 'xref-marker-ring-length)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx226_help_function_definitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'help-function-arglist)
      (fboundp 'help-C-file-name)
      (fboundp 'find-lisp-object-file-name))
"##,
        expect,
    );
}

#[test]
fn div_cx226_locate_library_cl_lib() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t \"elc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((path (locate-library "cl-lib")))
      (list (stringp path)
            (file-exists-p path)
            (file-name-extension path)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx226_symbol_file_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((file (symbol-file 'car)))
      (list (or (null file) (stringp file))
            (when file (file-exists-p file))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx226_eldoc_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((lib-path (locate-library "cl-lib")))
      (with-temp-buffer
        (buffer-enable-undo)
        (insert (format "Eldoc mega: lib=%s" lib-path))
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 10))
              (ov (make-overlay 4 18)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 25)
          (let ((state (list (boundp 'eldoc-documentation-function)
                             (fboundp 'find-function)
                             lib-path
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
