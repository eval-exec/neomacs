//! Complex combo batch 277 — `process` environment override via
//! `make-process :environment`, `:connection-type` pty vs pipe,
//! `:stderr` to filter function, `set-process-thread`, process
//! `:noquery` flag, and `process-list` ordering.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx277_make_process_with_environment_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Process neo-cx277-env finished\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((buf (get-buffer-create " *neo-cx277-env*"))
           (p (make-process :name "neo-cx277-env"
                            :command '("sh" "-c" "echo $NEO_CX277_VAR")
                            :buffer buf
                            :environment (cons "NEO_CX277_VAR=hello-env" process-environment))))
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
fn div_cx277_process_connection_type_pipe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"via-pipe\\n\\nProcess neo-cx277-pipe finished\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx277-pipe*"))
       (p (make-process :name "neo-cx277-pipe"
                        :command '("echo" "via-pipe")
                        :buffer buf
                        :connection-type 'pipe)))
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
fn div_cx277_process_stderr_to_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let (stderr-data)
      (let* ((stdout-buf (get-buffer-create " *neo-cx277-stdout*"))
             (p (make-process :name "neo-cx277-stderr"
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
    )
}

#[test]
fn div_cx277_process_noquery_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"neo-cx277-noquery\" exit)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx277-noquery"
                        :command '("echo" "test")
                        :noquery t)))
  (accept-process-output p 2)
  (sit-for 0.05)
  (list (processp p)
        (process-name p)
        (process-status p)))
"##,
        expect,
    )
}

#[test]
fn div_cx277_process_list_ordering_after_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t (#<process neo-cx277-order-2> #<process neo-cx277-order-1> #<process neo-cx277-order-0>) (#<process neo-cx277-order-1> #<process neo-cx277-order-0>) (#<process neo-cx277-order-0>))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((procs-before (process-list))
      created)
  (dotimes (i 3)
    (push (make-process :name (format "neo-cx277-order-%d" i)
                        :command '("echo" "test"))
          created))
  (let ((procs-after (process-list)))
    (dolist (p created) (delete-process p))
    (list (>= (length procs-after) (+ 3 (length procs-before)))
          (memq (car created) procs-after)
          (memq (cadr created) procs-after)
          (memq (caddr created) procs-after))))
"##,
        expect,
    )
}

#[test]
fn div_cx277_set_process_thread_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'set-process-thread)
          (fboundp 'process-thread)
          (boundp 'thread-list))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx277_process_file_with_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx277-pf*")))
  (with-current-buffer buf (erase-buffer))
  (let ((status (process-file "echo" nil buf nil "hello")))
    (let ((content (string-trim (with-current-buffer buf (buffer-string)))))
      (kill-buffer buf)
      (list status content))))
"##,
        expect,
    )
}

#[test]
fn div_cx277_make_pipe_process_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((pipe-buf (generate-new-buffer " *neo-cx277-pipe*"))
           (p (make-pipe-process :name "neo-cx277-pipe"
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
fn div_cx277_process_filter_default_appends_to_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"default-filter\\n\\nProcess neo-cx277-defflt finished\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx277-defflt*"))
       (p (make-process :name "neo-cx277-defflt"
                        :command '("echo" "default-filter")
                        :buffer buf)))
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
fn div_cx277_process_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments widen 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (get-buffer-create " *neo-cx277-mega*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "Process environment mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)))
  (let ((p (make-process :name "neo-cx277-mega-p"
                         :command '("sh" "-c" "printf 'PROCENV'")
                         :buffer buf
                         :environment (cons "NEO_CX277_MEGA=v" process-environment))))
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
