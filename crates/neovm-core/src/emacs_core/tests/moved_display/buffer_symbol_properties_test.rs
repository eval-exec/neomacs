//! Symbol properties that GNU's `syms_of_buffer` (src/buffer.c) puts on buffer-related symbols.

#[test]
fn buffer_symbol_properties_match_gnu() {
    let form = r#"(list
      (get 'kill-buffer-hook 'permanent-local)
      (get 'vertical-scroll-bar 'choice)
      (get 'fraction 'range)
      (get 'overwrite-mode 'choice)
      (get 'protected-field 'error-conditions)
      (condition-case err (signal 'protected-field nil)
        (error (error-message-string err))))"#;
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(form),
        concat!(
            r#"OK (t (nil t left right) (0.0 . 1.0)"#,
            r#" (nil overwrite-mode-textual overwrite-mode-binary)"#,
            r#" (protected-field error) "Attempt to modify a protected field")"#,
        )
    );
}

/// The `vertical-scroll-bar' part of GNU's data-tests-per-buffer-var-predicates.
#[test]
fn vertical_scroll_bar_accepts_each_of_its_choices() {
    let form = r#"(with-temp-buffer
      (let (seen)
        (dolist (v (get 'vertical-scroll-bar 'choice))
          (setq vertical-scroll-bar v)
          (push vertical-scroll-bar seen))
        (list (nreverse seen)
              (condition-case nil (progn (setq vertical-scroll-bar 'foo) 'no-error)
                (error 'error))
              vertical-scroll-bar)))"#;
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(form),
        "OK ((nil t left right) error right)"
    );
}
