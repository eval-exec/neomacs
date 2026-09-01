//! Complex combo batch 237 — `org-babel` code block execution /
//! `org-src` editing / `org-attach` / `org-id` availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx237_org_babel_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ob)
      (list (fboundp 'org-babel-execute-src-block)
            (fboundp 'org-babel-get-src-block-info)
            (boundp 'org-babel-load-languages)
            (boundp 'org-confirm-babel-evaluate)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx237_org_babel_elisp_execution() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ob)
      (require 'ob-emacs-lisp)
      (with-temp-buffer
        (org-mode)
        (insert "#+BEGIN_SRC emacs-lisp\n(+ 40 2)\n#+END_SRC\n")
        (let ((org-confirm-babel-evaluate nil))
          (org-babel-execute-src-block))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx237_org_babel_get_block_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"emacs-lisp\" \"(+ x 32)\" ((:var x . 10) (:colname-names) (:rowname-names) (:result-params \"replace\") (:result-type . value) (:results . \"replace\") (:exports . \"code\") (:lexical . \"no\") (:tangle . \"no\") (:hlines . \"no\") (:noweb . \"no\") (:cache . \"no\") (:session . \"none\")) \"\" nil 1 \"(ref:%s)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ob)
      (with-temp-buffer
        (org-mode)
        (insert "#+BEGIN_SRC emacs-lisp :var x=10\n(+ x 32)\n#+END_SRC\n")
        (goto-char 1)
        (org-babel-get-src-block-info)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx237_org_src_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org-src)
      (list (fboundp 'org-edit-src-code)
            (fboundp 'org-src-mode)
            (boundp 'org-src-window-setup)
            (boundp 'org-edit-src-content-indentation)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx237_org_attach_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org-attach)
      (list (fboundp 'org-attach)
            (fboundp 'org-attach-attach)
            (boundp 'org-attach-id-dir)
            (boundp 'org-attach-method)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx237_org_id_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org-id)
      (list (fboundp 'org-id-get-create)
            (fboundp 'org-id-get)
            (fboundp 'org-id-find)
            (boundp 'org-id-link-to-org-use-id)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx237_org_export_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ox)
      (list (fboundp 'org-export-as)
            (fboundp 'org-html-export-as-html)
            (boundp 'org-export-backends)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx237_org_link_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-variable)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "[[https://example.com][Example Link]] and [[file:./local.org][Local File]]")
        (goto-char 1)
        (list (org-in-regexp org-link-bracket-realm)
              (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx237_org_tangle_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ob-tangle)
      (list (fboundp 'org-babel-tangle)
            (fboundp 'org-babel-tangle-file)
            (boundp 'org-babel-tangle-use-relative-file-links)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx237_org_babel_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ob)
      (with-temp-buffer
        (buffer-enable-undo)
        (org-mode)
        (insert "* Babel mega\n#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n")
        (put-text-property 1 8 'face 'bold)
        (let ((m (set-marker (make-marker) 20))
              (ov (make-overlay 5 30)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 40)
          (let ((state (list (fboundp 'org-babel-execute-src-block)
                             (boundp 'org-babel-load-languages)
                             (eq major-mode 'org-mode)
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
