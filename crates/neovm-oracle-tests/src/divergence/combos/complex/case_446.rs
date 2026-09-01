//! Complex combo batch 446 — 15 more probes: url-parse, url-encode-url,
//! url-decode-url, url-hexify-string, url-unhex-string, url-path-and-query,
//! url-basepath, url-file-extension, text-mode, prog-mode, fundamental-mode,
//! special-mode, outline-mode, html-mode, sgml-mode.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// url-parse: parsing URL strings.
#[test]
fn div_cx446_url_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"https\" \"example.com\" \"/path?q=1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'url-parse)
  (let ((url (url-generic-parse-url "https://example.com/path?q=1#frag")))
    (list (url-type url) (url-host url) (url-filename url))))"##,
        expect,
    );
}

/// url-encode-url / url-decode-url.
#[test]
fn div_cx446_url_encode_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function url-decode-url)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'url-util)
  (list (url-encode-url "https://example.com/path with spaces")
        (url-decode-url "https%3A%2F%2Fexample.com")))"##,
        expect,
    );
}

/// url-hexify-string / url-unhex-string.
#[test]
fn div_cx446_url_hexify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"hello%20world\" \"hello world\" \"caf%C3%A9\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'url-util)
  (list (url-hexify-string "hello world")
        (url-unhex-string "hello%20world")
        (url-hexify-string "café")))
"##,
        expect,
    );
}

/// url-path-and-query / url-basepath.
#[test]
fn div_cx446_url_path_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument url \"https://example.com/a/b?q=1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'url)
  (list (url-path-and-query "https://example.com/a/b?q=1")
        (url-basepath "https://example.com/a/b/c")))
"##,
        expect,
    );
}

/// url-file-extension.
#[test]
fn div_cx446_url_file_ext() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \".png\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'url)
  (url-file-extension "image.png"))
"##,
        expect,
    );
}

/// text-mode activation and properties.
#[test]
fn div_cx446_text_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (text-mode text-mode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (text-mode)
  (list major-mode (derived-mode-p 'text-mode)))"##,
        expect,
    );
}

/// prog-mode activation.
#[test]
fn div_cx446_prog_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (prog-mode prog-mode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (prog-mode)
  (list major-mode (derived-mode-p 'prog-mode)))"##,
        expect,
    );
}

/// fundamental-mode activation.
#[test]
fn div_cx446_fundamental_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK fundamental-mode""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (fundamental-mode)
  major-mode)"##,
        expect,
    );
}

/// special-mode activation.
#[test]
fn div_cx446_special_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (special-mode special-mode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'simple)
  (with-temp-buffer
    (special-mode)
    (list major-mode (derived-mode-p 'special-mode))))"##,
        expect,
    );
}

/// outline-mode activation.
#[test]
fn div_cx446_outline_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (outline-mode outline-mode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'outline)
  (with-temp-buffer
    (outline-mode)
    (list major-mode (derived-mode-p 'outline-mode))))"##,
        expect,
    );
}

/// emacs-lisp-mode key features.
#[test]
fn div_cx446_emacs_lisp_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (emacs-lisp-mode t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (emacs-lisp-mode)
  (list major-mode (boundp 'emacs-lisp-mode-map)))"##,
        expect,
    );
}

/// lisp-interaction-mode.
#[test]
fn div_cx446_lisp_interaction_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (lisp-interaction-mode t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (lisp-interaction-mode)
  (list major-mode (boundp 'lisp-interaction-mode-map)))"##,
        expect,
    );
}

/// hexl-mode: hex editing.
#[test]
fn div_cx446_hexl_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'hexl)
  (list (fboundp 'hexl-mode) (fboundp 'hexl-find-file)))"##,
        expect,
    );
}

/// view-mode: read-only viewing.
#[test]
fn div_cx446_view_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'view)
  (with-temp-buffer
    (view-mode 1)
    (list view-mode (boundp 'view-mode-map))))"##,
        expect,
    );
}

/// read-only-mode: toggling read-only.
#[test]
fn div_cx446_read_only_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t buffer-read-only)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "test")
  (read-only-mode 1)
  (list buffer-read-only
        (condition-case e (insert "x") (error (car e)))))"##,
        expect,
    );
}
