//! Complex combo batch 412 — 20 probes targeting process I/O, network,
//! JSON deep, dired, compile, shell-command, pretty-print, backtrace,
//! debug, ert, and benchmark systems for additional divergence surface.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// process-send-string / process-status after sending.
#[test]
fn div_cx412_process_send_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK exit""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "neo-cx412-ps"
                          :command '("sh" "-c" "exit 42")
                          :connection-type 'pipe :buffer nil)))
  (accept-process-output proc 2)
  (let ((status (process-status proc)))
    (delete-process proc)
    status))
"##,
        expect,
    );
}

/// process-plist / process-get / process-put:
/// process property list storage.
#[test]
fn div_cx412_process_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "neo-cx412-pp"
                          :command '("echo" "done")
                          :connection-type 'pipe :buffer nil)))
  (accept-process-output proc 2)
  (process-put proc 'my-key 'my-value)
  (process-put proc 'other-key 42)
  (let ((plist (process-plist proc)))
    (list (plist-get plist 'my-key)
          (plist-get plist 'other-key)
          (process-get proc 'my-key)
          (process-get proc 'nonexistent)))
  (delete-process proc))
"##,
        expect,
    );
}

/// process-connection / process-type: querying process connection.
#[test]
fn div_cx412_process_connection_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "neo-cx412-ct"
                          :command '("echo" "test")
                          :connection-type 'pipe :buffer nil)))
  (accept-process-output proc 2)
  (list (process-type proc)
        (condition-case e (process-connection proc) (error (car e))))
  (delete-process proc))
"##,
        expect,
    );
}

/// make-network-process: creating a network connection (may be stubbed).
#[test]
fn div_cx412_make_network_process() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #<process neo-cx412-net>""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (make-network-process :name "neo-cx412-net"
                          :host "localhost" :service 0
                          :server t :reuseaddr t)
  (error (car e)))
"##,
        expect,
    );
}

/// json-read-from-string with various JSON types.
#[test]
fn div_cx412_json_read_from_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function json-read-from-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((json-array-type 'list)
      (json-object-type 'alist))
  (list (json-read-from-string "{\"a\":1,\"b\":2}")
        (json-read-from-string "[1,2,3]")
        (json-read-from-string "true")
        (json-read-from-string "null")
        (condition-case e (json-read-from-string "{invalid}") (error (car e)))))
"##,
        expect,
    );
}

/// json-encode with hash-table, vector, alist, plist outputs.
#[test]
fn div_cx412_json_encode_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function json-encode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht)
  (puthash "b" 2 ht)
  (list (json-encode ht)
        (json-encode [1 2 3])
        (json-encode '((a . 1) (b . 2)))
        (json-encode :keyword)))
"##,
        expect,
    );
}

/// dired-get-filename: extracting filenames in dired buffer.
#[test]
fn div_cx412_dired_get_filename() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"test.txt\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(require 'dired)
(let ((tmpdir (make-temp-file "neo-cx412-dir-" t)))
  (with-temp-file (expand-file-name "test.txt" tmpdir) (insert "x"))
  (unwind-protect
      (with-temp-buffer
        (dired tmpdir)
        (list (stringp (dired-get-filename))
              (dired-get-filename t)))
    (delete-directory tmpdir t)))
"##,
        expect,
    );
}

/// dired-mark / dired-mark-files / dired-get-marked-files.
#[test]
fn div_cx412_dired_mark_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(require 'dired)
(let ((tmpdir (make-temp-file "neo-cx412-dm-" t)))
  (with-temp-file (expand-file-name "a.txt" tmpdir) (insert "x"))
  (with-temp-file (expand-file-name "b.txt" tmpdir) (insert "y"))
  (unwind-protect
      (with-temp-buffer
        (dired tmpdir)
        (dired-mark 1)
        (length (dired-get-marked-files)))
    (delete-directory tmpdir t)))
"##,
        expect,
    );
}

/// shell-command / shell-command-to-string deeper.
#[test]
fn div_cx412_shell_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\" \"line1\\nline2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-trim-right (shell-command-to-string "echo hello"))
      (string-trim-right (shell-command-to-string "printf 'line1\nline2\n'")))
"##,
        expect,
    );
}

/// compile / compilation-mode: starting compilation.
#[test]
fn div_cx412_compile_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (buffer-read-only #<buffer *neo-cx412-compile*>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(require 'compile)
(let ((buf (get-buffer-create "*neo-cx412-compile*")))
  (with-current-buffer buf
    (compilation-mode)
    (insert "test.o:123:10: warning: test\n")
    (compilation-parse-errors (point-min) (point-max))
    (list (compilation-next-error 1)
          (compilation-next-error 1)))
  (kill-buffer buf))
"##,
        expect,
    );
}

/// next-error / previous-error: error navigation.
#[test]
fn div_cx412_next_error_func() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (buffer-read-only #<buffer *neo-cx412-next*>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(require 'compile)
(let ((buf (get-buffer-create "*neo-cx412-next*")))
  (with-current-buffer buf
    (compilation-mode)
    (insert "file.c:5:10: error: test1\n")
    (insert "file.c:10:20: error: test2\n")
    (compilation-parse-errors (point-min) (point-max)))
  (prog1
      (with-current-buffer buf
        (goto-char (point-min))
        (list (next-error 1) (line-number-at-pos)))
    (kill-buffer buf)))
"##,
        expect,
    );
}

/// pp-to-string / pp-display-expression: pretty printing.
#[test]
fn div_cx412_pp_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"(a (b c) d (e (f g)))\\n\" \"((lambda (x) (* x 2)) 5)\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (pp-to-string '(a (b c) d (e (f g))))
      (pp-to-string '((lambda (x) (* x 2)) 5)))
"##,
        expect,
    );
}

/// backtrace / backtrace-frame: backtrace introspection.
#[test]
fn div_cx412_backtrace_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t backtrace-frames setq)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frames ()))
  (defun neo-cx412-bt-a () (neo-cx412-bt-b))
  (defun neo-cx412-bt-b ()
    (setq frames (backtrace-frames)))
  (neo-cx412-bt-a)
  (list (> (length frames) 2)
        (nth 1 (car frames))
        (nth 1 (nth 1 frames))))
"##,
        expect,
    );
}

/// debug-on-entry / cancel-debug-on-entry: debugger control.
#[test]
fn div_cx412_debug_on_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (neo-cx412-debug neo-cx412-debug)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((sym (make-symbol "neo-cx412-debug")))
  (defalias sym (lambda (x) (* x 2)))
  (list (condition-case e (debug-on-entry sym) (error (car e)))
        (condition-case e (cancel-debug-on-entry sym) (error (car e)))))
"##,
        expect,
    );
}

/// ert-run-tests-batch: running ERT tests programmatically.
#[test]
fn div_cx412_ert_run_tests_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"No clause matching ‘passed’\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'ert)
  (ert-deftest neo-cx412-ert-test () (should (equal 1 1)))
  (let ((result (ert-run-tests-batch "neo-cx412-ert-test")))
    (list (ert-test-result-type-p result 'passed)
          (ert-test-passed-p result))))
"##,
        expect,
    );
}

/// benchmark-run / benchmark-run-compiled: benchmarking.
#[test]
fn div_cx412_benchmark_run() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((result (benchmark-run 100 (+ 1 2 3))))
  (list (listp result)
        (= (length result) 3)
        (numberp (nth 0 result))
        (numberp (nth 1 result))
        (numberp (nth 2 result))))
"##,
        expect,
    );
}

/// encode-time / decode-time roundtrip with timezone.
#[test]
fn div_cx412_encode_decode_time_zone() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((0 0 6 16 6 2026 2 t -14400) (27185 33040))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t1 (encode-time 0 0 12 16 6 2026 "Europe/Zurich")))
  (list (condition-case e (decode-time t1) (error (car e)))
        (condition-case e (encode-time 0 0 12 16 6 2026 "EST") (error (car e)))))
"##,
        expect,
    );
}

/// string-to-number with different radix and multibyte.
#[test]
fn div_cx412_string_to_number_radix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (255 10 63 123 0 123)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-to-number "ff" 16)
      (string-to-number "1010" 2)
      (string-to-number "77" 8)
      (string-to-number "123" 10)
      (string-to-number "0xff")
      (string-to-number "0123"))
"##,
        expect,
    );
}

/// number-to-string / format with different number bases.
#[test]
fn div_cx412_number_to_string_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"42\" \"ff\" \"377\" \"FF\" \"1010\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (number-to-string 42)
      (format "%x" 255)
      (format "%o" 255)
      (format "%X" 255)
      (format "%b" 10))
"##,
        expect,
    );
}
