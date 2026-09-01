//! Complex combo batch 389 — `process`/`network`/`serial` ultimate:
//! filter capture, sentinel events, coding override, environment override,
//! connection-type, stderr capture, pipe-process, send-string+eof,
//! process-mark, network-interface, serial-process.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx389_process_filter_chunked_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 18 \"line1\\nline2\\nline3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((collected nil)
       (buf (get-buffer-create " *neo-cx389-pf*"))
       (p (make-process :name "neo-cx389-pf"
                        :command '("sh" "-c" "for i in 1 2 3; do printf 'line%d\\n' $i; done")
                        :buffer buf
                        :filter (lambda (proc data) (push data collected)))))
  (accept-process-output p 2)
  (sit-for 0.05)
  (let ((all (apply #'concat (nreverse collected))))
    (kill-buffer buf)
    (list (length collected) (length all) (string-trim all))))
"##,
        expect,
    )
}

#[test]
fn div_cx389_process_sentinel_exit_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 exit 15 signal)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((events nil))
  (let ((p1 (make-process :name "neo-cx389-exit"
                          :command '("sh" "-c" "exit 7")
                          :sentinel (lambda (proc ev) (push (cons :exit ev) events))))
        (p2 (make-process :name "neo-cx389-sig"
                          :command '("sh" "-c" "kill -TERM $$")
                          :sentinel (lambda (proc ev) (push (cons :sig ev) events)))))
    (accept-process-output p1 2)
    (accept-process-output p2 2)
    (sit-for 0.05)
    (list (process-exit-status p1) (process-status p1)
          (process-exit-status p2) (process-status p2))))
"##,
        expect,
    )
}

#[test]
fn div_cx389_process_coding_environment_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Process neo-cx389-env finished\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((buf (get-buffer-create " *neo-cx389-env*"))
           (p (make-process :name "neo-cx389-env"
                            :command '("sh" "-c" "echo $NEO_A $NEO_B $NEO_C")
                            :buffer buf
                            :environment (append '("NEO_A=alpha" "NEO_B=beta" "NEO_C=gamma")
                                                 process-environment))))
      (accept-process-output p 2)
      (sit-for 0.05)
      (let ((content (string-trim (with-current-buffer buf (buffer-string)))))
        (kill-buffer buf)
        content))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx389_process_stderr_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"OUT\\n\\nProcess neo-cx389-stderr finished\" \"ERR\\n\\nProcess neo-cx389-stderr stderr finished\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((stderr-buf (get-buffer-create " *neo-cx389-err*"))
           (stdout-buf (get-buffer-create " *neo-cx389-out*"))
           (p (make-process :name "neo-cx389-stderr"
                            :command '("sh" "-c" "echo OUT; echo ERR >&2")
                            :buffer stdout-buf
                            :stderr stderr-buf)))
      (accept-process-output p 2)
      (sit-for 0.05)
      (let ((out (string-trim (with-current-buffer stdout-buf (buffer-string))))
            (err (string-trim (with-current-buffer stderr-buf (buffer-string)))))
        (kill-buffer stdout-buf)
        (kill-buffer stderr-buf)
        (list out err)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx389_process_send_string_eof_pipe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((pipe-buf (generate-new-buffer " *neo-cx389-pipe*"))
           (p (make-pipe-process :name "neo-cx389-pipe"
                                 :buffer pipe-buf
                                 :coding 'utf-8-unix)))
      (process-send-string p "first\n")
      (process-send-string p "second\n")
      (process-send-eof p)
      (accept-process-output p 1)
      (sit-for 0.05)
      (let ((content (with-current-buffer pipe-buf (buffer-string))))
        (delete-process p)
        (kill-buffer pipe-buf)
        content))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx389_process_plist_and_mark() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 42 t :val1 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx389-pm*"))
       (p (make-process :name "neo-cx389-pm"
                        :command '("echo" "mark-test")
                        :buffer buf)))
  (accept-process-output p 2)
  (sit-for 0.05)
  (process-put p 'neo-cx389-key1 :val1)
  (process-put p 'neo-cx389-key2 99)
  (let ((mark (process-mark p))
        (v1 (process-get p 'neo-cx389-key1))
        (v2 (process-get p 'neo-cx389-key2)))
    (prog1 (list (markerp mark) (marker-position mark)
                 (eq (marker-buffer mark) buf) v1 v2)
      (delete-process p)
      (kill-buffer buf))))
"##,
        expect,
    )
}

#[test]
fn div_cx389_network_interface_list_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ifaces (network-interface-list)))
      (list (or (null ifaces) (consp ifaces))
            (when ifaces (> (length ifaces) 0))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx389_serial_process_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'make-serial-process)
      (fboundp 'serial-process-configure))
"##,
        expect,
    )
}

#[test]
fn div_cx389_process_query_before_after_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((run open listen connect stop) \"neo-cx389-q\" (\"sh\" \"-c\" \"echo start; exit 5\") t t nil exit 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx389-q"
                        :command '("sh" "-c" "echo start; exit 5"))))
  (list (process-live-p p)
        (process-name p)
        (process-command p)
        (accept-process-output p 2)
        (sit-for 0.05)
        (process-live-p p)
        (process-status p)
        (process-exit-status p)))
"##,
        expect,
    )
}

#[test]
fn div_cx389_process_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments widen 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx389-mega*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "Process ultimate mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)))
  (let ((p (make-process :name "neo-cx389-mega-p"
                         :command '("sh" "-c" "printf 'PROCUR'")
                         :buffer buf)))
    (process-put p 'neo-cx389-tag :mega)
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1)
    (sit-for 0.05))
  (let ((content (with-current-buffer buf (buffer-string))))
    (with-current-buffer buf
      (widen()
      (let ((state (list content (length content)
                         (length (overlays-in 1 20))
                         (text-properties-at 1))))
        (undo)
        (kill-buffer buf)
        (list state (buffer-string)))))))
"##,
        expect,
    )
}
