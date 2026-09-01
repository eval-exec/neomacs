//! Complex combo batch 305 — `cl-generic` final deep: dispatch with
//! `&context` specializer (major-mode), `:method-combination` with
//! `nconc`/`append`, `cl-defgeneric` with `:documentation`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx305_cl_generic_with_context_major_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:elisp-mode :text-mode :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx305-context (obj))
      (cl-defmethod neo-cx305-context (obj &context (major-mode emacs-lisp-mode))
        :elisp-mode)
      (cl-defmethod neo-cx305-context (obj &context (major-mode text-mode))
        :text-mode)
      (cl-defmethod neo-cx305-context (obj)
        :default)
      (list (with-temp-buffer (emacs-lisp-mode) (neo-cx305-context "test"))
            (with-temp-buffer (text-mode) (neo-cx305-context "test"))
            (with-temp-buffer (neo-cx305-context "test"))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx305_cl_generic_method_combination_nconc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx305-nconc (obj) (:method-combination nconc))
      (cl-defmethod neo-cx305-nconc nconc ((obj string)) '(1 2))
      (cl-defmethod neo-cx305-nconc nconc ((obj string)) '(3 4))
      (cl-defmethod neo-cx305-nconc nconc ((obj string)) '(5 6))
      (neo-cx305-nconc "test"))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx305_cl_generic_method_combination_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx305-append (obj) (:method-combination append))
      (cl-defmethod neo-cx305-append append ((obj string)) '("a"))
      (cl-defmethod neo-cx305-append append ((obj string)) '("b"))
      (cl-defmethod neo-cx305-append append ((obj string)) '("c"))
      (neo-cx305-append "test"))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx305_cl_generic_with_documentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Documentation for the generic function.\\n\\n(fn OBJ)\" :str)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx305-doc (obj)
        "Documentation for the generic function.")
      (cl-defmethod neo-cx305-doc ((obj string))
        "Documentation for the string method."
        :str)
      (list (documentation 'neo-cx305-doc)
            (neo-cx305-doc "test")))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx305_cl_generic_call_next_method_deep_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx305-chain (obj))
      (cl-defmethod neo-cx305-chain (obj)
        (if (next-method-p) (cons :root (cl-call-next-method)) :root))
      (cl-defmethod neo-cx305-chain ((obj string))
        (if (next-method-p) (cons :str (cl-call-next-method)) :str))
      (cl-defmethod neo-cx305-chain ((obj (eql :special)))
        (if (next-method-p) (cons :eql (cl-call-next-method)) :eql))
      (list (neo-cx305-chain "test")
            (neo-cx305-chain :special)
            (neo-cx305-chain 42)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx305_cl_generic_find_method_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx305-findm (obj))
      (cl-defmethod neo-cx305-findm ((obj string)) :s)
      (cl-defmethod neo-cx305-findm ((obj integer)) :i)
      (cl-defmethod neo-cx305-findm ((obj null)) :nil)
      (let ((methods (cl-generic-methods 'neo-cx305-findm)))
        (list (consp methods)
              (= (length methods) 3))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx305_cl_generic_around_with_call_next_through_3_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:around-l1 . :primary-l1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (defclass neo-cx305-l1 () ())
      (defclass neo-cx305-l2 (neo-cx305-l1) ())
      (defclass neo-cx305-l3 (neo-cx305-l2) ())
      (cl-defgeneric neo-cx305-aro (obj))
      (cl-defmethod neo-cx305-aro :around ((obj neo-cx305-l1))
        (cons :around-l1 (cl-call-next-method)))
      (cl-defmethod neo-cx305-aro ((obj neo-cx305-l1))
        :primary-l1)
      (let ((inst (make-instance 'neo-cx305-l3)))
        (neo-cx305-aro inst)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx305_cl_generic_keyword_and_optional_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"test\" nil nil) (\"test\" :opt-val nil) (\"test\" :opt-val :m))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx305-kw-opt (obj &optional opt &key mode))
      (cl-defmethod neo-cx305-kw-opt ((obj string) &optional opt &key mode)
        (list obj opt mode))
      (list (neo-cx305-kw-opt "test")
            (neo-cx305-kw-opt "test" :opt-val)
            (neo-cx305-kw-opt "test" :opt-val :mode :m)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx305_cl_generic_no_applicable_returns_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :caught-no-applicable""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx305-noapp (obj))
      (condition-case err
          (neo-cx305-noapp "test")
        (cl-no-applicable-method :caught-no-applicable)
        (error (list :caught-error (car err)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx305_cl_generic_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cl-generic)
      (cl-defgeneric neo-cx305-mega (obj))
      (cl-defmethod neo-cx305-mega ((obj string)) :str-result)
      (cl-defmethod neo-cx305-mega ((obj integer)) :int-result)
      (cl-defmethod neo-cx305-mega (obj) :default)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "cl-generic mega milestone test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (neo-cx305-mega "hello")
                             (neo-cx305-mega 42)
                             (neo-cx305-mega :sym)
                             (length (cl-generic-methods 'neo-cx305-mega))
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
    )
}
