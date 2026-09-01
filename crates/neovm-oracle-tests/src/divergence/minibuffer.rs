//! Minibuffer subsystem divergence probes (calibration).
//!
//! Probes deterministic, non-interactive minibuffer surface: format-prompt
//! text + text-properties, minibuffer-prompt-properties, minibuffer-depth,
//! minibuffer-window predicates, minibuffer keymaps, history / add-to-history,
//! completion config (styles/category-defaults), and EOF behavior of the
//! interactive readers (both error end-of-file under --batch + closed stdin).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_mb_format_prompt_text_no_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Prompt: \"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(format-prompt "Prompt" nil)
"##,
        expect,
    );
}

#[test]
fn div_mb_format_prompt_text_with_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Prompt (default x): \" \"Prompt (default 42): \" \"Prompt: \")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format-prompt "Prompt" "x")
      (format-prompt "Prompt" 42)
      (format-prompt "Prompt" ""))
"##,
        expect,
    );
}

#[test]
fn div_mb_format_prompt_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Prompt (default d): \" nil 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (format-prompt "Prompt" "d")))
  (list p (text-properties-at 0 p) (length p)))
"##,
        expect,
    );
}

#[test]
fn div_mb_prompt_properties_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (read-only t face minibuffer-prompt)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
minibuffer-prompt-properties
"##,
        expect,
    );
}

#[test]
fn div_mb_minibuffer_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function minibuffer-depth-indicator-mode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (minibuffer-depth) (minibuffer-depth-indicator-mode 0))
"##,
        expect,
    );
}

#[test]
fn div_mb_minibuffer_window_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (windowp (minibuffer-window))
      (window-minibuffer-p (minibuffer-window))
      (minibuffer-window-active-p (minibuffer-window))
      (eq (active-minibuffer-window) nil))
"##,
        expect,
    );
}

#[test]
fn div_mb_keymaps_exist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (keymapp minibuffer-local-map)
      (keymapp minibuffer-local-completion-map)
      (keymapp minibuffer-local-must-match-map)
      (keymapp minibuffer-local-filename-completion-map)
      (keymapp minibuffer-local-ns-map))
"##,
        expect,
    );
}

#[test]
fn div_mb_minibuffer_map_key_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (exit-minibuffer exit-minibuffer minibuffer-complete abort-minibuffers)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (lookup-key minibuffer-local-map "\r")
      (lookup-key minibuffer-local-map "\n")
      (lookup-key minibuffer-local-completion-map "\t")
      (lookup-key minibuffer-local-map "\C-g"))
"##,
        expect,
    );
}

#[test]
fn div_mb_history_default_and_boundp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'minibuffer-history)
      (listp minibuffer-history)
      (boundp 'extended-command-history))
"##,
        expect,
    );
}

#[test]
fn div_mb_add_to_history_dedup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable h)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((h nil))
  (add-to-history 'h "a")
  (add-to-history 'h "b")
  (add-to-history 'h "a")
  (add-to-history 'h "c")
  h)
"##,
        expect,
    );
}

#[test]
fn div_mb_history_length_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable h)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((h nil) (history-length 3))
  (dotimes (i 6) (add-to-history 'h (number-to-string i)))
  (list h (length h)))
"##,
        expect,
    );
}

#[test]
fn div_mb_completion_styles_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((basic partial-completion emacs22) (emacs21 emacs22 basic partial-completion substring flex initials shorthand))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list completion-styles
      (mapcar #'car completion-styles-alist))
"##,
        expect,
    );
}

#[test]
fn div_mb_completion_category_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((buffer (styles basic substring)) (unicode-name (styles basic substring)) (project-file (styles substring)) (xref-location (styles substring)) (info-menu (styles basic substring)) (symbol-help (styles basic shorthand substring)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
completion-category-defaults
"##,
        expect,
    );
}

#[test]
fn div_mb_minibuffer_default_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'minibuffer-default)
      (boundp 'minibuffer-default-add-function)
      (boundp 'minibuffer-completion-predicate))
"##,
        expect,
    );
}

#[test]
fn div_mb_read_from_minibuffer_eof() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"
(list (condition-case e (read-from-minibuffer "P: " "def") (error (car e)))
      (condition-case e (read-string "P: " "def") (error (car e)))
      (condition-case e (read-number "P: " 42) (error (car e))))
"##,
    );
}

#[test]
fn div_mb_completing_read_eof() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"
(list (condition-case e (completing-read "P: " '("a" "b") nil t "b") (error (car e)))
      (condition-case e (read-buffer "P: " "x" t) (error (car e))))
"##,
    );
}

#[test]
fn div_mb_read_command_variable_filename_eof() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"
(list (condition-case e (read-command "P: ") (error (car e)))
      (condition-case e (read-variable "P: ") (error (car e)))
      (condition-case e (read-regexp "P: ") (error (car e))))
"##,
    );
}
