//! Oracle parity tests for GNU memory profiler state semantics.
//!
//! GNU implements these in `src/profiler.c`: `profiler-memory-start` is
//! stateful, returns t, rejects a second start while running, and
//! `profiler-memory-stop` returns whether the profiler had been running.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_profiler_memory_start_stop_running_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((initial (profiler-memory-running-p))
      (start (profiler-memory-start))
      (running (profiler-memory-running-p))
      (second (condition-case err
                  (profiler-memory-start)
                (error (cons (car err) (cdr err)))))
      (stop (profiler-memory-stop))
      (stopped (profiler-memory-running-p))
      (stop2 (profiler-memory-stop)))
  (list initial start running second stop stopped stop2))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil t t (error \"Memory profiler is already running\") t nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
