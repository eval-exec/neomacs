//! Expectations captured from GNU Emacs 31.1 through sandbox-run.sh.
#[test]
fn tsb_backward_char_keeps_the_negated_count_out_of_lisp_values() {
    let form = r#"(with-temp-buffer
      (insert "héllo") (goto-char 2)
      (list (condition-case e (backward-char most-negative-fixnum) (error e))
            (point)
            (condition-case e (backward-char most-positive-fixnum) (error e))
            (point)))"#;
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one(form),
        "OK ".to_owned() + include_str!("tsb-backward-char.expect").trim_end()
    );
}
