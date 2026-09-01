//! Strict combo oracle probes, batch 178: frame / terminal API. selected-frame,
//! framep, frame-live-p, frame-parameter / frame-parameters subset,
//! terminal-live-p, frame-terminal, and frame-list membership. (Skips
//! window-system-dependent parameters and make-frame which needs a display.)
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_frame_selected_framep_livep_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((f (selected-frame)))
  (list (framep f)
        (frame-live-p f)
        (frame-live-p (make-frame))
        (consp (frame-list))
        (memq f (frame-list))
        (eq f (car (frame-list)))
        (frame-live-p (selected-frame))
        (terminal-live-p (frame-terminal f))))
"##;
    let expect = expect_test::expect![[r#""ERR (error \"Unknown terminal type\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_frame_parameters_subset_terminal_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((f (selected-frame)))
  (list (stringp (frame-parameter f 'name))
        (frame-parameter f 'window-system)
        (frame-parameter f 'minibuffer)
        (consp (frame-parameters f))
        (assq 'name (frame-parameters f))
        (terminalp (frame-terminal f))
        (stringp (terminal-name (frame-terminal f)))
        (eq (frame-terminal f) (frame-terminal f))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function terminalp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_frame_modify_restore_parameter_visible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((f (selected-frame))
      (saved (frame-parameter nil 'title)))
  (unwind-protect
      (progn
        (modify-frame-parameters f '((title . "probe-frame-title")))
        (list (frame-parameter f 'title)
              (frame-parameter f 'buried-buffers)
              (frame-parameter f 'buffer-list)))
    (when (stringp saved)
      (modify-frame-parameters f (list (cons 'title saved))))))
"##;
    let expect =
        expect_test::expect![[r#""OK (\"probe-frame-title\" nil (#<buffer *scratch*>))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
