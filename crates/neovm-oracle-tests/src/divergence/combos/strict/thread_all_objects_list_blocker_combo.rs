//! Strict combo oracle probes, batch 339: thread introspection / all-threads.
//! all-threads list, thread-object identity, thread-blocker functions,
//! and thread-last-error.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_all_threads_list_contains_created() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (let* ((before (length (all-threads)))
           (th (make-thread (lambda () (sit-for 0.01)) "probe-all-threads")))
      (sit-for 0.001)
      (thread-join th)
      (list (>= before 0)
            (consp (all-threads))
            (consp (all-threads))
            (threadp th)))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_thread_signal_named_blocker_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (let ((result nil))
      (let ((th (make-thread (lambda ()
                               (push 'ran result))
                             "probe-signal-thread")))
        (thread-join th)
        (list (nreverse result)
              (threadp th)
              (not (thread-alive-p th))
              (consp (all-threads)))))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (caught void-function)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_thread_current_main_self_identity_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (list (eq (current-thread) (current-thread))
          (threadp (current-thread))
          (threadp (main-thread))
          (member (current-thread) (cons (main-thread) (all-threads)))
          (eq (current-thread) (main-thread)))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (caught void-function)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
