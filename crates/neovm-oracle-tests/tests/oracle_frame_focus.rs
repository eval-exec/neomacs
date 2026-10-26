//! Public switch-frame handler controls; batch cannot establish native focus.

use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn switch_frame_handler_preserves_prefix_and_same_frame_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let* ((frame (selected-frame))
                  (window (selected-window))
                  (buffer (current-buffer))
                  (origin (point))
                  (current-prefix-arg '(16))
                  (prefix-arg nil)
                  (this-command 'issue395-current)
                  (last-command 'issue395-previous)
                  (mouse-leave-buffer-hook nil))
              (let ((result (handle-switch-frame (list 'switch-frame frame))))
                (list (eq result frame)
                      (eq (selected-frame) frame)
                      (eq (selected-window) window)
                      (eq (current-buffer) buffer)
                      (= (point) origin)
                      (equal prefix-arg '(16))
                      (eq this-command 'issue395-current)
                      (eq last-command 'issue395-previous))))"#,
        expect_test::expect![[r#""OK (t t t t t t t t)""#]],
    );
}
