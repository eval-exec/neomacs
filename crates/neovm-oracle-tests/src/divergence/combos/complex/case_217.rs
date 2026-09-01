//! Complex combo batch 217 — `flyspell` / `ispell` / `hunspell` /
//! `spell-fu` spell check availability and predicates.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx217_flyspell_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'flyspell)
      (list (fboundp 'flyspell-mode)
            (fboundp 'flyspell-prog-mode)
            (fboundp 'flyspell-buffer)
            (fboundp 'flyspell-region)
            (boundp 'flyspell-issue-message-flag)
            (boundp 'flyspell-highlight-properties)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx217_ispell_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ispell)
      (list (fboundp 'ispell)
            (fboundp 'ispell-buffer)
            (fboundp 'ispell-region)
            (fboundp 'ispell-word)
            (fboundp 'ispell-complete-word)
            (boundp 'ispell-program-name)
            (boundp 'ispell-dictionary)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx217_ispell_local_dictionary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'ispell-local-dictionary)
          (boundp 'ispell-personal-dictionary)
          (boundp 'ispell-alternate-dictionary)
          (boundp 'ispell-silently-save-pdict))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx217_spell_fu_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'spell-fu)
          (fboundp 'spell-fu-mode))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx217_flyspell_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'flyspell-mode)
          (boundp 'flyspell-mode-line-string)
          (boundp 'pre-redisplay-function))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx217_flyspell_correct_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'flyspell-correct)
          (fboundp 'flyspell-correct-word)
          (fboundp 'flyspell-correct-previous-word))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx217_which_key_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'which-key)
          (fboundp 'which-key-mode)
          (boundp 'which-key-idle-delay))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx217_helm_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'helm)
          (fboundp 'helm)
          (boundp 'helm-command-prefix-key))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx217_ivy_counsel_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'ivy)
          (fboundp 'ivy-mode)
          (featurep 'counsel)
          (fboundp 'counsel-M-x))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx217_flyspell_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'flyspell)
      (require 'ispell)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Flyspell mega test buffer content with words")
        (put-text-property 1 8 'face 'bold)
        (let ((m (set-marker (make-marker) 10))
              (ov (make-overlay 4 18)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 25)
          (let ((state (list (fboundp 'flyspell-mode)
                             (boundp 'ispell-program-name)
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
