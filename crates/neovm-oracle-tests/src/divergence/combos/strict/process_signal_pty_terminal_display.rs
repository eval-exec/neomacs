//! Strict combo oracle probes, batch 127: process signal delivery combo,
//! PTY vs pipe output comparison, terminal parameter ops, and display
//! combo (window-vscroll, scroll-up/down, recenter with pixel offsets).
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_u1_process_signal_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((results nil))
  (dolist (sig '(interrupt quit stop kill))
    (let ((proc (make-process :name (format "probe-sig-%s" sig)
                              :command (list shell-file-name shell-command-switch "read line")
                              :connection-type 'pipe
                              :sentinel (lambda (&rest _) nil))))
      (set-process-query-on-exit-flag proc nil)
      (accept-process-output proc 0.1)
      (condition-case err
          (funcall (intern (format "%s-process" sig)) proc)
        (error (push (cons sig 'error) results)))
      (accept-process-output proc 0.1)
      (push (cons sig (process-status proc)) results)
      (when (process-live-p proc) (delete-process proc))))
  (nreverse results))
"##;
    let expect = expect_test::expect![[
        r#""OK ((interrupt . signal) (quit . run) (stop . run) (kill . signal))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u1_pty_vs_pipe_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((pipe-out nil) (pty-out nil))
  (let ((pipe (make-process :name "probe-pipe-out"
                            :command (list shell-file-name shell-command-switch "echo hello")
                            :connection-type 'pipe
                            :buffer (generate-new-buffer " *probe-pipe-buf*")
                            :sentinel (lambda (&rest _) nil))))
    (set-process-query-on-exit-flag pipe nil)
    (accept-process-output pipe 1)
    (setq pipe-out (with-current-buffer (process-buffer pipe) (buffer-string)))
    (kill-buffer (process-buffer pipe)))
  (let ((pty (make-process :name "probe-pty-out"
                           :command (list shell-file-name shell-command-switch "echo hello")
                           :connection-type 'pty
                           :buffer (generate-new-buffer " *probe-pty-buf*")
                           :sentinel (lambda (&rest _) nil))))
    (set-process-query-on-exit-flag pty nil)
    (accept-process-output pty 1)
    (setq pty-out (with-current-buffer (process-buffer pty) (buffer-string)))
    (kill-buffer (process-buffer pty)))
  (list pipe-out
        pty-out
        (string-trim pipe-out)
        (string-trim pty-out)
        (string= (string-trim pipe-out) (string-trim pty-out))))
"##;
    let expect =
        expect_test::expect![[r#""OK (\"hello\\n\" \"hello\\n\" \"hello\" \"hello\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u1_terminal_parameter_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((term (car (terminal-list))))
  (list (terminalp term)
        (terminal-live-p term)
        (eq (terminal-name term) (terminal-name (car (terminal-list))))
        (progn (set-terminal-parameter term 'probe-tt-param 99)
               (terminal-parameter term 'probe-tt-param))
        (terminal-parameter term 'nonexistent)
        (length (terminal-list))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function terminalp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u1_display_vscroll_scroll_recenter_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-vscombo*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (with-current-buffer b
          (dotimes (i 80) (insert (format "line%02d\n" i))))
        (set-window-vscroll nil 3)
        (let ((vs1 (window-vscroll))
              (start1 (window-start)))
          (condition-case err (scroll-up 5) (error nil))
          (let ((vs2 (window-vscroll))
                (start2 (window-start)))
            (recenter 5)
            (list vs1 start1
                  vs2 start2
                  (window-start)
                  (point)
                  (window-end nil t)
                  (count-lines (window-start) (window-end nil t))))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (0 1 0 519 526 561 561 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u1_condition_case_nested_error_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (condition-case outer
      (condition-case inner
          (signal 'arith-error '("inner data"))
        (wrong-type-argument
         (push 'wrong-type-inner log)
         (signal 'wrong-type-argument '("rethrown")))
      (arith-error
       (push (cons 'arith (cdr outer)) log))
      (error
       (push (cons 'error (cdr outer)) log)))
  (condition-case err
      (signal 'my-error '(arg1 arg2 arg3))
    (my-error (push (cdr err) log)))
  (condition-case err
      (apply 'signal '(void-function void-fn-xyz))
    (void-function (push (cdr err) log)))
  (nreverse log))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
