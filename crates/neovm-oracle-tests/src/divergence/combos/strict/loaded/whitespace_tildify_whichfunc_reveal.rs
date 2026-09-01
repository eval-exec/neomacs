//! Strict combo oracle probes, batch 46: mode/util loaded libraries via
//! assert_oracle_parity_with_load — whitespace.el (whitespace-mode active
//! style), textmodes/tildify.el (tildify-region), progmodes/which-func.el
//! (which-function in emacs-lisp-mode), and reveal.el (reveal-mode over
//! outline hiding).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_i3_whitespace_mode_active_style() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "foo bar   ")
  (let ((whitespace-style '(trailing tabs space-before-tab)))
    (whitespace-mode 1))
  (list whitespace-active-style
        (boundp 'whitespace-active-style)))
"##,
        &["whitespace.el"],
        expect,
    );
}

#[test]
fn div_i3_tildify_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (tildify-region (point-min) (point-max))
  (buffer-string))
"##,
        &["textmodes/tildify.el"],
        expect,
    );
}

#[test]
fn div_i3_which_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"foo\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo ()\n  body)\n")
  (goto-char 5)
  (which-function-mode 1)
  (list (which-function)))
"##,
        &["progmodes/which-func.el"],
        expect,
    );
}

#[test]
fn div_i3_reveal_over_outline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (outline-mode)
  (insert "* H1\n** s1\n* H2\n")
  (outline-hide-body)
  (reveal-mode 1)
  (goto-char 1)
  (list (get-text-property (point) 'invisible)
        (next-single-property-change 1 'invisible)))
"##,
        &["reveal.el", "outline.el"],
        expect,
    );
}

#[test]
fn div_i3_whitespace_trailing_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 0 nil)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "ok   \nclean\n\t tabbed\n")
  (let ((whitespace-style '(tabs trailing)))
    (whitespace-mode 1))
  (list (overlays-in (point-min) (point-max))
        (length (overlays-in (point-min) (point-max)))
        (get-text-property 3 'font-lock-face)))
"##,
        &["whitespace.el"],
        expect,
    );
}
