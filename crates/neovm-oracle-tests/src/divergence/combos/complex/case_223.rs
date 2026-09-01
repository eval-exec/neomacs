//! Complex combo batch 223 — `yasnippet` / `tempo` / `skeleton` /
//! `auto-insert` / `auto-capitalize` / `auto-compile` template
//! expansion availability and metadata.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx223_yasnippet_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'yasnippet)
          (fboundp 'yas-minor-mode)
          (fboundp 'yas-expand)
          (boundp 'yas-snippet-dirs))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx223_tempo_template_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'tempo)
      (list (fboundp 'tempo-define-template)
            (fboundp 'tempo-insert-template)
            (boundp 'tempo-tags)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx223_skeleton_template_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'skeleton)
      (list (fboundp 'define-skeleton)
            (fboundp 'skeleton-insert)
            (boundp 'skeleton-further-elements)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx223_auto_insert_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'autoinsert)
      (list (fboundp 'auto-insert-mode)
            (boundp 'auto-insert-alist)
            (boundp 'auto-insert-query)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx223_tempo_define_and_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'tempo)
      (tempo-define-template "neo-cx223-test"
                             '("Hello " (p "Name: ") "!"))
      (list (assq 'neo-cx223-test tempo-tags)
            (boundp 'tempo-tags)
            (consp tempo-tags)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx223_skeleton_define_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'skeleton)
      (define-skeleton neo-cx223-skel
        "Test skeleton."
        "Enter name: "
        "Hello " str "!")
      (list (fboundp 'neo-cx223-skel)
            (fboundp 'define-skeleton)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx223_auto_capitalize_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'auto-capitalize)
          (fboundp 'auto-capitalize-mode))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx223_auto_compile_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'auto-compile)
          (fboundp 'auto-compile-on-save-mode))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx223_abbrev_expansion_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"neocx223-expanded\" neo)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((table (make-abbrev-table)))
      (define-abbrev table "neo" "neocx223-expanded" (lambda () (message "expanded")))
      (list (abbrev-table-p table)
            (abbrev-expansion "neo" table)
            (abbrev-symbol "neo" table)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx223_template_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'tempo)
      (require 'skeleton)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Template mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (fboundp 'tempo-define-template)
                             (fboundp 'define-skeleton)
                             (boundp 'tempo-tags)
                             (boundp 'skeleton-further-elements)
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
