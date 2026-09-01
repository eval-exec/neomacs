/// Batch 494: font-lock-add-keywords, font-lock-remove-keywords, font-lock-unfontify.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx494_font_lock_add_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'font-lock)
  (with-temp-buffer
    (emacs-lisp-mode)
    (font-lock-add-keywords nil '(("\\<my-fn\\>" . 'bold)))
    (font-lock-remove-keywords nil '(("\\<my-fn\\>" . 'bold)))
    t))
"##,
        expect,
    );
}

#[test]
fn div_cx494_font_lock_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'font-lock)
  (boundp 'font-lock-defaults-alist))
"##,
        expect,
    );
}

#[test]
fn div_cx494_font_lock_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"unspecified-fg\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(face-attribute 'font-lock-keyword-face :foreground nil 'default)
"##,
        expect,
    );
}

#[test]
fn div_cx494_font_lock_syntactic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'font-lock)
  (boundp 'font-lock-syntactic-keywords))
"##,
        expect,
    );
}

#[test]
fn div_cx494_font_lock_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'font-lock)
  (list (fboundp 'font-lock-mode) (fboundp 'font-lock-fontify-buffer)))
"##,
        expect,
    );
}

#[test]
fn div_cx494_font_lock_ensure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'font-lock)
  (with-temp-buffer
    (emacs-lisp-mode)
    (font-lock-ensure)
    t))
"##,
        expect,
    );
}

#[test]
fn div_cx494_font_lock_face_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified])""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (facep 'font-lock-comment-face)
      (facep 'font-lock-string-face)
      (facep 'font-lock-keyword-face)
      (facep 'font-lock-function-name-face)
      (facep 'font-lock-variable-name-face)
      (facep 'font-lock-type-face)
      (facep 'font-lock-constant-face)
      (facep 'font-lock-warning-face))
"##,
        expect,
    );
}

#[test]
fn div_cx494_font_lock_keyword_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'font-lock)
  (list (fboundp 'font-lock-keywords) (boundp 'font-lock-keywords-alist)))
"##,
        expect,
    );
}

#[test]
fn div_cx494_font_lock_preprocessor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified]""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(facep 'font-lock-preprocessor-face)
"##,
        expect,
    );
}

#[test]
fn div_cx494_font_lock_doc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified]""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(facep 'font-lock-doc-face)
"##,
        expect,
    );
}

#[test]
fn div_cx494_jit_lock_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'jit-lock)
  (list (boundp 'jit-lock-mode) (fboundp 'jit-lock-register)))
"##,
        expect,
    );
}

#[test]
fn div_cx494_jit_lock_refontify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'jit-lock)
  (fboundp 'jit-lock-refontify))
"##,
        expect,
    );
}

#[test]
fn div_cx494_lazy_lock_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"lazy-lock\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'lazy-lock)
  (list (boundp 'lazy-lock-mode) (fboundp 'lazy-lock-fontify-after-install)))
"##,
        expect,
    );
}

#[test]
fn div_cx494_font_lock_face_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified])""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (facep 'font-lock-comment-delimiter-face)
      (facep 'font-lock-negation-char-face))
"##,
        expect,
    );
}

#[test]
fn div_cx494_font_lock_keyword_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'font-lock)
  (fboundp 'font-lock-compile-keywords))
"##,
        expect,
    );
}
