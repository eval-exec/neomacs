//! Strict combo oracle probes, batch 45: HTML/CSS rendering loaded libraries
//! via assert_oracle_parity_with_load — net/shr.el (HTML dom -> text) and
//! textmodes/css.el (CSS parsing/expansion). These are complex and commonly
//! used by eww/notmuch/etc.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_i2_shr_render_document() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"Hello\\nworld\\n\" 0 1 (face shr-text shr-indentation nil) 1 5 (face shr-text) 5 6 (face nil) 6 11 (face (shr-text bold)))""#
    ]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK #("Hello\nworld\n" ...) — shr renders the <b> child on a
    //   new line after the plain text node within the <p>.
    // Neomacs:   OK #("Hello world\n" ...) — shr keeps the inline <b> content
    //   on the same line as the preceding text.
    // shr-insert-document renders inline sibling content differently.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (let ((dom '(html nil (body nil (p nil "Hello " (b nil "world"))))))
    (shr-insert-document dom))
  (buffer-string))
"##,
        &["net/shr.el"],
        expect,
    );
}

#[test]
fn div_i2_shr_render_list_and_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"*\\n one\\n \\n*\\n two\\n \\n\\nlink\\n\" 0 1 (shr-prefix-length 2 shr-continuation-indentation 2 shr-indentation nil) 1 2 (face nil) 2 3 (face nil shr-prefix-length 1 display (space :width (2.0 . width))) 3 6 (face shr-text) 7 8 (shr-prefix-length 1 display (space :width (2.0 . width))) 9 10 (shr-prefix-length 2 shr-continuation-indentation 2 shr-indentation nil) 10 11 (face nil) 11 12 (face nil shr-prefix-length 1 display (space :width (2.0 . width))) 12 15 (face shr-text) 16 17 (shr-prefix-length 1 display (space :width (2.0 . width))) 19 20 (keymap (keymap (13 . shr-browse-url) (79 . shr-save-contents) (118 . shr-browse-url) (117 . shr-maybe-probe-and-copy-url) (119 . shr-maybe-probe-and-copy-url) (73 . shr-insert-image) (C-down-mouse-1 . shr-mouse-browse-url-new-window) (mouse-2 . shr-browse-url) (follow-link . mouse-face) (9 . shr-next-link) (122 . shr-zoom-image) (27 keymap (9 . shr-previous-link) (105 . shr-browse-image)) (97 . shr-show-alt-text)) shr-tab-stop t mouse-face (highlight) follow-link t help-echo \"http://x\" category shr button t shr-url \"http://x\" face (shr-text shr-link) shr-indentation nil) 20 23 (keymap (keymap (13 . shr-browse-url) (79 . shr-save-contents) (118 . shr-browse-url) (117 . shr-maybe-probe-and-copy-url) (119 . shr-maybe-probe-and-copy-url) (73 . shr-insert-image) (C-down-mouse-1 . shr-mouse-browse-url-new-window) (mouse-2 . shr-browse-url) (follow-link . mouse-face) (9 . shr-next-link) (122 . shr-zoom-image) (27 keymap (9 . shr-previous-link) (105 . shr-browse-image)) (97 . shr-show-alt-text)) mouse-face (highlight) follow-link t help-echo \"http://x\" category shr button t shr-url \"http://x\" face (shr-text shr-link)))""#
    ]];
    // Divergence surfaced 2026-06-27: shr <ul>/<li>/<a> rendering diverges
    // from GNU (rendered+propertized output differs in length and content:
    // ~1355 vs ~1670 bytes). HTML->text list/link rendering is not equivalent.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (let ((dom '(html nil
                (body nil
                  (ul nil (li nil "one") (li nil "two"))
                  (a ((href . "http://x")) "link")))))
    (shr-insert-document dom))
  (buffer-string))
"##,
        &["net/shr.el"],
        expect,
    );
}

#[test]
fn div_i2_css_expand_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"[ORACLE-LOAD-ROOT]/textmodes/css.el\")""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (condition-case err (css-expand-value 'margin '(1 2 3 4)) (error (cons 'err (car err))))
      (condition-case err (css-expand-value 'color "red") (error (cons 'err (car err)))))
"##,
        &["textmodes/css.el"],
        expect,
    );
}

#[test]
fn div_i2_css_color_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"[ORACLE-LOAD-ROOT]/textmodes/css.el\")""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (condition-case err (css-color-string-to-hsl "#ff0000") (error (cons 'err (car err))))
      (condition-case err (css-color-parse-hex "#00ff00") (error (cons 'err (car err)))))
"##,
        &["textmodes/css.el"],
        expect,
    );
}
