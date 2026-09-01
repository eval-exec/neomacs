//! Deep combo: process × start-process × call-process × shell-command ×
//! process-filter × process-sentinel × marker × overlay × text-prop ×
//! undo × buffer-local × narrow.
//!
//! Stresses process interaction with buffer state: processes that insert
//! text into buffers, process filters that modify buffer content, and
//! process sentinels that run after process completion. Processes are
//! tricky in a Rust rewrite because they involve async I/O and must
//! interact correctly with the buffer's edit pipeline.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_start_process_insert_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-sp")))
    (with-current-buffer buf
      (insert "BEFORE-AFTER")
      (put-text-property 1 7 'part 'before)
      (put-text-property 8 13 'part 'after)
      (let ((m1 (copy-marker 7 nil))
            (m2 (copy-marker 7 t))
            (ov (make-overlay 1 13)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let ((proc (start-process "echo-sp" buf "echo" "INSERTED")))
          (accept-process-output proc 1)
          (sit-for 0.3))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'part)
                           (get-text-property 8 'part))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'part)
                                (get-text-property 8 'part))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_call_process_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-cp")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (call-process "echo" nil buf nil "INSERTED")
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_shell_command_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-sc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (shell-command "echo -n INSERTED" buf)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_process_filter_marker_overlay_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"1\\n2\\n3\\n\" 6 6 t)) (#(\"START-END\\nProcess seq-pf finished\\n\" 0 5 (part start) 6 9 (part end)) 6 6 t all start))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-pf2"))
        (filter-outputs nil))
    (with-current-buffer buf
      (insert "START-END")
      (put-text-property 1 6 'part 'start)
      (put-text-property 7 10 'part 'end)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 6 t))
            (ov (make-overlay 1 10)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let ((proc (start-process "seq-pf" buf "seq" "1" "3")))
          (set-process-filter proc
            (lambda (p output)
              (push (list output
                          (marker-position m1)
                          (marker-position m2)
                          (and (overlay-start ov) t))
                    filter-outputs)))
          (while (accept-process-output proc 0.5))
          (sit-for 0.2)
          (let ((final (list (buffer-string)
                             (marker-position m1)
                             (marker-position m2)
                             (and (overlay-start ov) t)
                             (overlay-get ov 'scope)
                             (get-text-property 1 'part))))
            (kill-buffer buf)
            (list (nreverse filter-outputs) final))))))) "#,
        expect,
    );
}

#[test]
fn combo_process_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-pbl")))
    (with-current-buffer buf
      (make-local-variable 'proc-local)
      (setq proc-local 'buffer-specific)
      (insert "HELLO-WORLD")
      (put-text-property 1 6 'word 'hello)
      (put-text-property 7 12 'word 'world)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 6 t))
            (ov (make-overlay 1 12)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let ((proc (start-process "echo-pbl" buf "echo" "INSERTED")))
          (accept-process-output proc 1)
          (sit-for 0.3))
        (let ((after (list (buffer-string)
                           proc-local
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'word)
                           (get-text-property 7 'word))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                proc-local
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'word)
                                (get-text-property 7 'word))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}
