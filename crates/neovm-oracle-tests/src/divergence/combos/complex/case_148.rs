//! Complex combo batch 148 — `process` IPC patterns: stderr separation,
//! connection types, multi-process interactions, and process mark.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx148_process_mark_position_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 37 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx148-mark*"))
       (p (make-process :name "neo-cx148-mark"
                        :command '("echo" "hi")
                        :buffer buf)))
  (accept-process-output p 2)
  (sit-for 0.05)
  (let ((mark (process-mark p)))
    (prog1 (list (markerp mark)
                 (marker-position mark)
                 (eq (marker-buffer mark) buf))
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx148_process_list_after_multiple_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((procs-before (process-list)))
  (let ((p1 (make-process :name "neo-cx148-p1" :command '("echo" "1")))
        (p2 (make-process :name "neo-cx148-p2" :command '("echo" "2")))
        (p3 (make-process :name "neo-cx148-p3" :command '("echo" "3"))))
    (let ((procs-after (process-list)))
      (list (>= (length procs-after) (+ 3 (length procs-before)))
            (memq p1 procs-after)
            (memq p2 procs-after)
            (memq p3 procs-after))))
  (dolist (p (process-list))
    (when (string-prefix-p "neo-cx148-p" (process-name p))
      (delete-process p))))
"##,
        expect,
    );
}

#[test]
fn div_cx148_process_query_before_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx148-query"
                        :command '("sh" "-c" "sleep 0.1"))))
  (list (process-live-p p)
        (eq (process-status p) 'run)
        (memq (process-status p) '(run exit signal)))
"##,
        expect,
    );
}

#[test]
fn div_cx148_process_filter_vs_buffer_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\nProcess neo-cx148-flt finished\\n\" \"via-filter\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((filter-collected nil)
       (buf (get-buffer-create " *neo-cx148-flt*"))
       (p (make-process :name "neo-cx148-flt"
                        :command '("echo" "via-filter")
                        :buffer buf
                        :filter (lambda (proc data) (push data filter-collected)))))
  (accept-process-output p 2)
  (sit-for 0.05)
  (let ((buf-content (with-current-buffer buf (buffer-string))))
    (kill-buffer buf)
    (list buf-content
          (apply #'concat (nreverse filter-collected)))))
"##,
        expect,
    );
}

#[test]
fn div_cx148_process_default_filter_appends_to_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"default-filter\\n\\nProcess neo-cx148-defflt finished\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx148-defflt*"))
       (p (make-process :name "neo-cx148-defflt"
                        :command '("echo" "default-filter")
                        :buffer buf)))
  (accept-process-output p 2)
  (sit-for 0.05)
  (let ((content (string-trim (with-current-buffer buf (buffer-string)))))
    (kill-buffer buf)
    content))
"##,
        expect,
    );
}

#[test]
fn div_cx148_set_process_buffer_change_after_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Process neo-cx148-switchbuf not running: finished\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf-a (get-buffer-create " *neo-cx148-a*"))
       (buf-b (get-buffer-create " *neo-cx148-b*"))
       (p (make-process :name "neo-cx148-switchbuf"
                        :command '("echo" "switch")
                        :buffer buf-a)))
  (accept-process-output p 1)
  (sit-for 0.05)
  (let ((in-a (with-current-buffer buf-a (buffer-string))))
    (set-process-buffer p buf-b)
    (process-send-string p "second")
    (accept-process-output p 1)
    (sit-for 0.05)
    (let ((in-a-after (with-current-buffer buf-a (buffer-string)))
          (in-b-after (with-current-buffer buf-b (buffer-string))))
      (kill-buffer buf-a)
      (kill-buffer buf-b)
      (list in-a in-a-after in-b-after))))
"##,
        expect,
    );
}

#[test]
fn div_cx148_process_stderr_separate_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"OUT\\n\\nProcess neo-cx148-stderr finished\" \"ERR\\n\\nProcess neo-cx148-stderr stderr finished\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((err-buf (get-buffer-create " *neo-cx148-err*"))
           (stdout-buf (get-buffer-create " *neo-cx148-out*"))
           (p (make-process :name "neo-cx148-stderr"
                            :command '("sh" "-c" "echo OUT; echo ERR >&2")
                            :buffer stdout-buf
                            :stderr err-buf)))
      (accept-process-output p 2)
      (sit-for 0.05)
      (let ((out (string-trim (with-current-buffer stdout-buf (buffer-string))))
            (err (string-trim (with-current-buffer err-buf (buffer-string)))))
        (kill-buffer stdout-buf)
        (kill-buffer err-buf)
        (list out err)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx148_process_stderr_to_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let (stderr-data)
      (let* ((stdout-buf (get-buffer-create " *neo-cx148-stderr-flt-out*"))
             (p (make-process :name "neo-cx148-stderr-flt"
                              :command '("sh" "-c" "echo OUT; echo ERR >&2")
                              :buffer stdout-buf
                              :stderr (lambda (proc data) (push data stderr-data)))))
        (accept-process-output p 2)
        (sit-for 0.05)
        (let ((out (string-trim (with-current-buffer stdout-buf (buffer-string))))
              (err (string-trim (apply #'concat (nreverse stderr-data)))))
          (kill-buffer stdout-buf)
          (list out err))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx148_process_plist_set_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:val nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx148-plist" :command '("echo" "p"))))
  (process-put p 'neo-cx148-custom :val)
  (let ((got (process-get p 'neo-cx148-custom))
        (missing (process-get p 'missing)))
    (delete-process p)
    (list got missing)))
"##,
        expect,
    );
}

#[test]
fn div_cx148_process_connection_type_pipe_vs_pty_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((p-pipe (make-process :name "neo-cx148-pipe"
                                 :command '("echo" "pipe")
                                 :connection-type 'pipe))
           (p-pty (make-process :name "neo-cx148-pty"
                                :command '("echo" "pty")
                                :connection-type 'pty)))
      (accept-process-output p-pipe 1)
      (accept-process-output p-pty 1)
      (sit-for 0.05)
      (prog1 (list (processp p-pipe)
                   (processp p-pty)
                   (eq (process-status p-pipe) 'exit)
                   (eq (process-status p-pty) 'exit))
        (delete-process p-pipe)
        (delete-process p-pty)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx148_process_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx148-mega*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "Process mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)))
  (let ((p (make-process :name "neo-cx148-mega-p"
                         :command '("sh" "-c" "printf 'PROC'")
                         :buffer buf)))
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
        (list state (buffer-string))))))
"##,
        expect,
    );
}
