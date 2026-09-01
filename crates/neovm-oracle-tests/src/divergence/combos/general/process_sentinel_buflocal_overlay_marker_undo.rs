//! Deep combo: process sentinel × buffer-local × overlay × marker ×
//! undo × text-prop × narrow × regex.
//!
//! Stresses process interaction with buffer state: sentinels that modify
//! buffers, process filters that insert text, and how markers/overlays
//! survive process output. Process sentinels are tricky in a Rust rewrite
//! because they run asynchronously and must interact correctly with the
//! buffer's edit pipeline (undo, markers, overlays, text properties).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_process_sentinel_buffer_state_after_finish() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"finished\" #(\"BEFORE-PROCESS-RUN-AFTEROUTPUT-TEXT\\n\" 0 6 (sect before) 18 24 (sect after)) 7 19 t middle before)""#
    ]];
    // Process sentinel modifies buffer, markers/overlays must track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ps"))
        (sentinel-result nil))
    (with-current-buffer buf
      (insert "BEFORE-PROCESS-RUN-AFTER")
      (let ((m1 (copy-marker 7 nil))
            (m2 (copy-marker 19 t))
            (ov (make-overlay 7 19)))
        (overlay-put ov 'zone 'middle)
        (put-text-property 1 7 'sect 'before)
        (put-text-property 19 25 'sect 'after)
        (undo-boundary)
        (let ((proc (start-process "echo-test" buf "echo" "OUTPUT-TEXT")))
          (set-process-sentinel proc
            (lambda (p event)
              (setq sentinel-result
                    (list (string-trim event)
                          (buffer-string)
                          (marker-position m1)
                          (marker-position m2)
                          (and (overlay-start ov) t)
                          (overlay-get ov 'zone)
                          (get-text-property 1 'sect)))))
          (accept-process-output proc 1)
          (sit-for 0.5)
          (kill-buffer buf)
          sentinel-result))))) "#,
        expect,
    );
}

#[test]
fn combo_process_filter_insert_with_markers_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"1\\n2\\n3\\n4\\n5\\n\" 1 6 t)) (#(\"START-END\\nProcess seq-test finished\\n\" 0 5 (kind init)) 1 6 t all init))""#
    ]];
    // Process filter inserts text incrementally; markers must track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-pf"))
        (filter-outputs nil))
    (with-current-buffer buf
      (insert "START-END")
      (let ((m-start (copy-marker 1 t))
            (m-end (copy-marker 6 nil))
            (ov (make-overlay 1 6)))
        (overlay-put ov 'scope 'all)
        (put-text-property 1 6 'kind 'init)
        (undo-boundary)
        (let ((proc (start-process "seq-test" buf "seq" "1" "5")))
          (set-process-filter proc
            (lambda (p output)
              (push (list output
                          (marker-position m-start)
                          (marker-position m-end)
                          (and (overlay-start ov) t))
                    filter-outputs)))
          (while (accept-process-output proc 0.5))
          (sit-for 0.2)
          (let ((final (list (buffer-string)
                             (marker-position m-start)
                             (marker-position m-end)
                             (and (overlay-start ov) t)
                             (overlay-get ov 'scope)
                             (get-text-property 1 'kind))))
            (kill-buffer buf)
            (list (nreverse filter-outputs) final))))))) "#,
        expect,
    );
}

#[test]
fn combo_process_pipe_buffer_local_overlay_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // shell-command-on-region with buffer-local vars, overlays, markers.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-pipe")))
    (with-current-buffer buf
      (make-local-variable 'my-local)
      (setq my-local 'pipe-test)
      (insert "hello world foo bar baz")
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 22)))
        (overlay-put ov 'scope 'all)
        (put-text-property 1 6 'word 'hello)
        (put-text-property 7 12 'word 'world)
        (undo-boundary)
        (shell-command-on-region 1 6 "tr a-z A-Z" buf t)
        (let ((after-cmd (list (buffer-string)
                               my-local
                               (marker-position m1)
                               (marker-position m2)
                               (and (overlay-start ov) t)
                               (get-text-property 1 'word))))
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo (list (buffer-string)
                                  my-local
                                  (marker-position m1)
                                  (marker-position m2)
                                  (get-text-property 1 'word))))
            (kill-buffer buf)
            (list after-cmd after-undo))))))) "#,
        expect,
    );
}

#[test]
fn combo_process_insert_with_narrow_undo_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Process output into narrowed buffer, then undo.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-pnarrow")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t)))
        (undo-boundary)
        (narrow-to-region 6 15)
        (goto-char (point-min))
        (let ((proc (start-process "echo-narrow" buf "echo" "INSERTED")))
          (accept-process-output proc 1)
          (sit-for 0.3))
        (widen)
        (let ((after-insert (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (get-text-property 1 'zone)
                                  (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (get-text-property 1 'zone)
                                  (get-text-property 6 'zone))))
            (kill-buffer buf)
            (list after-insert after-undo))))))) "#,
        expect,
    );
}
