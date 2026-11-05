//! Known GNU divergences of `parse-partial-sexp`, pinned in process against
//! the answers GNU 31.1 gave in `neovm-oracle-tests` `syntax/parse_state.rs`
//! (`NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`). Each pin fails on purpose
//! once Neomacs answers like GNU.
use crate::test_utils::{oracle_expect_transcript, runtime_startup_eval_one};

/// KNOWN DIVERGENCE, pinned (`neovm-oracle-tests` `syntax/parse_state.rs`
/// `oracle_prop_stopbefore_prev_syntax_divergence`): after a STOPBEFORE stop
/// GNU reports element 10 for the character before the stop; Neomacs reports
/// the stop character's own syntax. The `pps_propertize` sweep leaves element 10 of
/// STOPBEFORE rows out for this reason. Fails, on purpose, once fixed.
#[test]
fn stopbefore_prev_syntax_divergence_is_pinned() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?\( "()1n" st)
    (modify-syntax-entry ?\) ")(4n" st)
    (modify-syntax-entry ?* ". 23n" st)
    (set-syntax-table st))
  (insert ". (d) /x")
  (list (parse-partial-sexp 1 5 nil t) (point)))
"#;
    assert_ne!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(r#""OK ((0 nil nil nil nil nil 0 nil nil nil nil) 3)""#),
        "the STOPBEFORE element-10 divergence is fixed: flip both pins to parity"
    );
}
