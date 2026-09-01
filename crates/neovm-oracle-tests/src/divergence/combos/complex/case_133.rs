//! Complex combo batch 133 — `proced` / `ibuffer` / `info` / `woman` /
//! `help` / `apropos` / `info-lookup` availability and queries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx133_proced_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'proced)
      (list (fboundp 'proced)
            (boundp 'proced-format)
            (boundp 'proced-filter)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx133_ibuffer_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ibuffer)
      (list (fboundp 'ibuffer)
            (boundp 'ibuffer-formats)
            (boundp 'ibuffer-show-empty-filter-groups)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx133_info_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'info)
      (list (fboundp 'info)
            (boundp 'Info-directory-list)
            (boundp 'Info-default-directory-list)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx133_woman_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'woman)
      (list (fboundp 'woman)
            (boundp 'woman-manpath)
            (boundp 'woman-path)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx133_help_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'describe-function)
          (fboundp 'describe-variable)
          (fboundp 'describe-symbol)
          (fboundp 'describe-key)
          (fboundp 'describe-bindings)
          (fboundp 'describe-mode)
          (fboundp 'apropos)
          (fboundp 'apropos-command))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx133_apropos_basic_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (excessive-lisp-nesting 1601)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((results (apropos-internal "buffer")))
      (list (consp results)
            (> (length results) 0)
            (memq 'buffer results)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx133_info_lookup_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'info-lookup-symbol)
          (fboundp 'info-lookup-file)
          (boundp 'info-lookup-alist))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx133_describe_function_in_temp_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((buf (help-buffer)))
      (list (bufferp buf)
            (buffer-live-p buf)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx133_apropos_with_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t (marker marker-buffer marker-insertion-type marker-last-position marker-position markerp move-marker number-or-marker number-or-marker-p point-marker point-max-marker point-min-marker project-vc-extra-root-markers set-marker set-marker-insertion-type xref-location-marker xref-marker-stack-empty-p xref-pop-marker-stack) (set-marker set-marker-insertion-type xref-location-marker xref-marker-stack-empty-p xref-pop-marker-stack))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((all (apropos-internal "marker")))
      (list (consp all)
            (memq 'marker all)
            (memq 'set-marker all)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx133_help_echo_via_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"this is help text\" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "text with help-echo")
  (put-text-property 1 5 'help-echo "this is help text")
  (list (get-text-property 1 'help-echo)
        (get-text-property 5 'help-echo)
        (get-text-property 6 'help-echo)))
"##,
        expect,
    );
}

#[test]
fn div_cx133_help_symbol_complete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((coll (all-completions "buf" obarray)))
      (list (consp coll)
            (> (length coll) 0)
            (memq 'buffer coll)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx133_help_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments mapconcat 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((sym-list (apropos-internal "buffer")))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (mapconcat #'symbol-name sym-list " " :test #'stringp))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 5 20)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 30)
      (let ((state (list (length sym-list)
                         (memq 'buffer sym-list)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
