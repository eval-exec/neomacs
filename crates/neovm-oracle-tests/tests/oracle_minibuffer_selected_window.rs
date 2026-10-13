//! Issue #560: the saved minibuffer caller must still be usable.
#[path = "../src/common.rs"]
mod common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deleted_caller_allows_the_selected_window_fallback() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((executing-kbd-macro t)
      (origin (split-window))
      (observations nil))
  (select-window origin)
  (catch 'issue560-done
    (minibuffer-with-setup-hook
      (lambda ()
        (push (eq (minibuffer-selected-window) origin) observations)
        (delete-window origin)
        (push (list (minibuffer-selected-window)
                    (condition-case err
                        (with-selected-window
                            (or (minibuffer-selected-window) (selected-window)) 'ok)
                      (error (car err)))) observations)
        (throw 'issue560-done nil))
    (read-from-minibuffer "Prompt: ")))
  (nreverse observations))
"#,
        expect_test::expect![[r#""OK (t (nil ok))""#]],
    );
}

#[test]
fn only_a_selected_active_minibuffer_exposes_its_caller() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((executing-kbd-macro t)
      (origin (split-window))
      (observations nil))
  (select-window origin)
  (push (minibuffer-selected-window) observations)
  (catch 'issue560-done
    (minibuffer-with-setup-hook
        (lambda ()
          (let ((mini (selected-window)))
            (push (list (> (minibuffer-depth) 0)
                        (window-minibuffer-p)
                        (eq (minibuffer-selected-window) origin)) observations)
            (select-window origin)
            (push (list (> (minibuffer-depth) 0)
                        (window-minibuffer-p)
                        (minibuffer-selected-window)) observations)
            (select-window mini)
            (push (eq (minibuffer-selected-window) origin) observations))
          (throw 'issue560-done nil))
      (read-from-minibuffer "Prompt: ")))
  (push (list (minibuffer-depth) (minibuffer-selected-window)) observations)
  (delete-window origin)
  (nreverse observations))
"#,
        expect_test::expect![[r#""OK (nil (t t t) (t nil nil) t (0 nil))""#]],
    );
}
