//! The GNU oracle forms of `neovm-oracle-tests`
//! `line/selective_display_count.rs`, run in process against the answers
//! GNU 31.1 gave there (`NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`).
use crate::test_utils::{oracle_expect_transcript, runtime_startup_eval_one};

#[test]
fn line_number_at_pos_counts_carriage_returns_under_selective_display() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(with-temp-buffer
  (insert "a\rb\nc\rd\ne\r")
  (let ((at (lambda () (mapcar #'line-number-at-pos '(1 2 3 4 5 6 7 8 9 10 11)))))
    (list (funcall at)
          (progn (setq selective-display t) (funcall at))
          (progn (setq selective-display 2) (funcall at))
          (progn (setq selective-display 'hide) (funcall at))
          (progn (setq selective-display t)
                 (narrow-to-region 3 9)
                 (list (line-number-at-pos 9)
                       (line-number-at-pos 9 t)
                       (line-number-at-pos 5)))
          (progn (widen)
                 (list (count-lines 1 (point-max))
                       (progn (goto-char 1) (forward-line 2) (point))
                       (line-number-at-pos))))))
"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(
            r#""OK ((1 1 1 1 2 2 2 2 3 3 3) (1 1 2 2 3 3 4 4 5 5 6) (1 1 1 1 2 2 2 2 3 3 3) (1 1 2 2 3 3 4 4 5 5 6) (4 5 2) (5 9 5))""#
        )
    );
}
