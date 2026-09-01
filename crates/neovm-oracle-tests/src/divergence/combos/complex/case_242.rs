//! Complex combo batch 242 — `minibuffer` / `recursive-edit` /
//! `enable-recursive-minibuffers` / `minibuffer-depth` /
//! `read-from-minibuffer` / `read-string` / `completing-read` availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx242_minibuffer_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'read-from-minibuffer)
      (fboundp 'read-string)
      (fboundp 'read-no-blanks-input)
      (fboundp 'completing-read)
      (fboundp 'read-char)
      (fboundp 'read-event)
      (fboundp 'read-key)
      (fboundp 'read-command)
      (fboundp 'read-variable)
      (fboundp 'read-function))
"##,
        expect,
    );
}

#[test]
fn div_cx242_minibuffer_depth_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (>= (minibuffer-depth) 0)
      (integerp (minibuffer-depth)))
"##,
        expect,
    );
}

#[test]
fn div_cx242_enable_recursive_minibuffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'enable-recursive-minibuffers)
      (booleanp enable-recursive-minibuffers))
"##,
        expect,
    );
}

#[test]
fn div_cx242_minibuffer_window_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument bufferp #<window 2 on  *Minibuf-0*>)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((mw (minibuffer-window)))
  (list (windowp mw)
        (minibufferp mw)
        (window-minibuffer-p mw)))
"##,
        expect,
    );
}

#[test]
fn div_cx242_active_minibuffer_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (windowp (active-minibuffer-window))
      (eq (active-minibuffer-window) (minibuffer-window)))
"##,
        expect,
    );
}

#[test]
fn div_cx242_minibuffer_history_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'minibuffer-history)
      (boundp 'file-name-history)
      (boundp 'extended-command-history)
      (boundp 'command-history)
      (boundp 'shell-command-history)
      (boundp 'regexp-history)
      (boundp 'search-ring)
      (boundp 'regexp-search-ring))
"##,
        expect,
    );
}

#[test]
fn div_cx242_minibuffer_prompt_setup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'minibuffer-prompt)
          (fboundp 'minibuffer-message)
          (fboundp 'minibuffer-complete)
          (fboundp 'minibuffer-complete-word)
          (boundp 'minibuffer-prompt-properties)
          (boundp 'minibuffer-electric-default-map))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx242_minibuffer_completion_helpers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'minibuffer-completion-help)
      (fboundp 'minibuffer-complete-and-exit)
      (fboundp 'exit-minibuffer)
      (fboundp 'minibuffer-completion-confirm)
      (boundp 'completion-show-commit-message))
"##,
        expect,
    );
}

#[test]
fn div_cx242_minibuffer_keymap_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'minibuffer-local-map)
      (boundp 'minibuffer-local-ns-map)
      (boundp 'minibuffer-local-completion-map)
      (boundp 'minibuffer-local-must-match-map)
      (keymapp minibuffer-local-map)
      (keymapp minibuffer-local-completion-map))
"##,
        expect,
    );
}

#[test]
fn div_cx242_minibuffer_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument bufferp #<window 2 on  *Minibuf-0*>)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((mw (minibuffer-window)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Minibuffer mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (minibuffer-depth)
                         (windowp mw)
                         (minibufferp mw)
                         (boundp 'enable-recursive-minibuffers)
                         (boundp 'minibuffer-local-map)
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
