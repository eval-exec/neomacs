//! Complex combo batch 337 — `process` ultimate: filter capture, sentinel
//! events, coding-system override, environment override, connection-type
//! variants, stderr capture, pipe-process lifecycle, process-list ordering.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx337_process_filter_chunked_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 18 \"line1\\nline2\\nline3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((collected nil)
       (buf (get-buffer-create " *neo-cx337-pf*"))
       (p (make-process :name "neo-cx337-pf"
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
fn div_cx337_process_sentinel_exit_and_signal_events() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:exit . \"exited abnormally with code 7\\n\") (:sig . \"terminated\\n\") 7 exit 15 signal)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((events nil))
  (let ((p1 (make-process :name "neo-cx337-exit"
                          :command '("sh" "-c" "exit 7")
                          :sentinel (lambda (proc ev) (push (cons :exit ev) events))))
        (p2 (make-process :name "neo-cx337-sig"
                          :command '("sh" "-c" "kill -TERM $$")
                          :sentinel (lambda (proc ev) (push (cons :sig ev) events)))))
    ;; The sentinel IS the subject here, so we keep both sentinels.  GNU
    ;; status_notify services all changed processes, even when
    ;; accept-process-output targets a single process, so the two async
    ;; sentinels can fire in either order.  Wait for the tagged events and
    ;; assert their payloads independent of that delivery order.
    (while (process-live-p p1) (accept-process-output p1 1))
    (while (not (assq :exit events)) (accept-process-output p1 1))
    (while (process-live-p p2) (accept-process-output p2 1))
    (while (not (assq :sig events)) (accept-process-output p2 1))
    (while (accept-process-output nil 0))
    (list (assq :exit events)
          (assq :sig events)
          (process-exit-status p1)
          (process-status p1)
          (process-exit-status p2)
          (process-status p2))))
"##,
        expect,
    )
}

#[test]
fn div_cx337_process_coding_system_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"世界\\nProcess neo-cx337-cs finished\\n\" 33)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx337-cs*"))
       (p (make-process :name "neo-cx337-cs"
                        :command '("printf" "世界")
                        :buffer buf)))
  (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
  (let ((attempts 0))
    (while (and (process-live-p p) (< attempts 100))
      (accept-process-output p 0.05)
      (setq attempts (1+ attempts)))
    (when (process-live-p p)
      (error "process did not exit")))
  (while (accept-process-output p 0.01))
  (let ((content (with-current-buffer buf (buffer-string))))
    (kill-buffer buf)
    (list content (length content))))
"##,
        expect,
    )
}

#[test]
fn div_cx337_process_environment_override_with_multiple_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Process neo-cx337-env finished\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((buf (get-buffer-create " *neo-cx337-env*"))
           (p (make-process :name "neo-cx337-env"
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
fn div_cx337_process_stderr_capture_separate_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"OUT\\n\\nProcess neo-cx337-stderr finished\" \"ERR\\n\\nProcess neo-cx337-stderr stderr finished\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((stderr-buf (get-buffer-create " *neo-cx337-err*"))
           (stdout-buf (get-buffer-create " *neo-cx337-out*"))
           (p (make-process :name "neo-cx337-stderr"
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
fn div_cx337_make_pipe_process_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((pipe-buf (generate-new-buffer " *neo-cx337-pipe*"))
           (p (make-pipe-process :name "neo-cx337-pipe"
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
fn div_cx337_process_send_string_and_eof_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"alpha beta gamma\\n\\nProcess neo-cx337-eof finished\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx337-eof*"))
       (p (make-process :name "neo-cx337-eof"
                        :command '("cat")
                        :buffer buf
                        :connection-type 'pipe)))
  (process-send-string p "alpha beta gamma\n")
  (process-send-eof p)
  (accept-process-output p 2)
  (sit-for 0.05)
  (let ((content (string-trim (with-current-buffer buf (buffer-string)))))
    (kill-buffer buf)
    content))
"##,
        expect,
    )
}

#[test]
fn div_cx337_process_plist_set_get_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:val1 :val2 99 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx337-plist" :command '("echo" "test"))))
  (process-put p 'neo-cx337-key1 :val1)
  (process-put p 'neo-cx337-key2 :val2)
  (process-put p 'neo-cx337-key3 99)
  (let ((v1 (process-get p 'neo-cx337-key1))
        (v2 (process-get p 'neo-cx337-key2))
        (v3 (process-get p 'neo-cx337-key3))
        (missing (process-get p 'missing)))
    (delete-process p)
    (list v1 v2 v3 missing)))
"##,
        expect,
    )
}

#[test]
fn div_cx337_process_mark_position_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 44 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx337-mark*"))
       (p (make-process :name "neo-cx337-mark"
                        :command '("echo" "mark-test")
                        :buffer buf)))
  (accept-process-output p 2)
  (sit-for 0.05)
  (let ((mark (process-mark p)))
    (prog1 (list (markerp mark)
                 (marker-position mark)
                 (eq (marker-buffer mark) buf))
      (delete-process p)
      (kill-buffer buf))))
"##,
        expect,
    )
}

#[test]
fn div_cx337_process_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx337-mega*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "Process ultimate mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)))
  (let ((p (make-process :name "neo-cx337-mega-p"
                         :command '("sh" "-c" "printf 'PROCUR'")
                         :buffer buf)))
    (process-put p 'neo-cx337-tag :mega)
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1)
    (sit-for 0.05))
  (let ((content (with-current-buffer buf (buffer-string))))
    (with-current-buffer buf
      (widen)
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
