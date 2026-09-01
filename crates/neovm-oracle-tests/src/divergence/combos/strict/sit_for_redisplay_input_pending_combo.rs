//! Strict combo oracle probes, batch 324: sit-for / redisplay / input-pending.
//! sit-for return value, redisplay, input-pending-p, and event-pending-p.
//! Uses assert_oracle_parity_expect format.
//!
//! NOTE: sit-for with non-zero delay that returns on input is hard to test in
//! batch (no input). We use sit-for 0 (returns t immediately) and redisplay
//! (returns t) which are deterministic.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_sit_for_redisplay_input_pending_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (sit-for 0)
      (redisplay t)
      (input-pending-p)
      (or (null (input-pending-p)) (input-pending-p))
      (sit-for 0.001))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_redisplay_force_mode_line_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-current-buffer (get-buffer-create " *probe-redraw*")
  (insert "content")
  (let ((r1 (redisplay t))
        (m1 (force-mode-line-update t)))
    (kill-buffer (current-buffer))
    (list r1 m1 (eq m1 nil))))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_sleep_for_minimal_no_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (sleep-for 0.001)
      (sit-for 0)
      (redisplay))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
