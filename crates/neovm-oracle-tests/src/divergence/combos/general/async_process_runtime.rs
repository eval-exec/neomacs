//! Combo divergence-hunt: async make-process filters/sentinels, pipes,
//! call-process, timers, and accept-process-output runtime parity.
//! Goal: surface unknown divergences at the process/timer boundary.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn async_proc_accept_output_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (start-process "neo-apo-xxx" nil "sleep" "0.05")))
  (set-process-query-on-exit-flag proc nil)
  (let ((r (accept-process-output proc 2)))
    (while (process-live-p proc) (accept-process-output proc 0.1))
    (list (booleanp r) (eq (process-status proc) 'exit))))"##,
        expect,
    );
}

#[test]
fn async_proc_async_filter_accumulate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 9 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((acc "") (done nil))
  (let ((proc (make-process :name "neo-af-xxx"
               :command '("sh" "-c" "printf 'aa\nbb\ncc\n'")
               :connection-type 'pipe
               :filter (lambda (_p s) (setq acc (concat acc s)))
               :sentinel (lambda (_p e) (when (string-match "finished" e) (setq done t))))))
    (set-process-query-on-exit-flag proc nil)
    (while (process-live-p proc) (accept-process-output proc 1))
    (while (not done) (accept-process-output proc 0.05))
    (list (string= acc "aa\nbb\ncc\n") (length acc) done)))"##,
        expect,
    );
}

#[test]
fn async_proc_call_process_exit_codes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 1 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (call-process "true") (call-process "false")
      (call-process "sh" nil nil nil "-c" "exit 7"))"##,
        expect,
    );
}

#[test]
fn async_proc_call_process_string_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 \"result-data\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((code (call-process "printf" nil t nil "%s" "result-data")))
    (list code (buffer-string) (= (point) (point-max)))))"##,
        expect,
    );
}

#[test]
fn async_proc_delete_process_status() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (signal nil (signal exit))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (start-process "neo-dp-xxx" nil "sleep" "10")))
  (set-process-query-on-exit-flag proc nil)
  (delete-process proc)
  (list (process-status proc) (process-live-p proc)
        (memq (process-status proc) '(signal exit))))"##,
        expect,
    );
}

#[test]
fn async_proc_process_contact_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"sleep\" \"5\") \"neo-pc-xxx\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (start-process "neo-pc-xxx" nil "sleep" "5")))
  (set-process-query-on-exit-flag proc nil)
  (prog1 (list (process-command proc) (process-name proc)
               (process-contact proc) (eq (process-type proc) 'real))
    (delete-process proc)))"##,
        expect,
    );
}

#[test]
fn async_proc_process_filter_default_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"XMIDY\\nProcess neo-pfd-xxx finished\\n\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((buf (generate-new-buffer " neo-pfd-xxx")))
  (let ((proc (start-process "neo-pfd-xxx" buf "printf" "X%sY" "MID")))
    (set-process-query-on-exit-flag proc nil)
    (while (process-live-p proc) (accept-process-output proc 1))
    (prog1 (with-current-buffer buf (list (buffer-string) (= (point) (point-max))))
      (kill-buffer buf))))"##,
        expect,
    );
}

#[test]
fn async_proc_process_get_buffer_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((buf (generate-new-buffer " neo-pm-xxx")))
  (let ((proc (start-process "neo-pm-xxx" buf "echo" "hi")))
    (set-process-query-on-exit-flag proc nil)
    (while (process-live-p proc) (accept-process-output proc 1))
    (prog1 (list (markerp (process-mark proc))
                 (eq (marker-buffer (process-mark proc)) buf))
      (kill-buffer buf))))"##,
        expect,
    );
}

#[test]
fn async_proc_process_plist_get_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 \"baz\" nil 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (start-process "neo-pp-xxx" nil "sleep" "5")))
  (set-process-query-on-exit-flag proc nil)
  (process-put proc 'foo 42) (process-put proc 'bar "baz")
  (prog1 (list (process-get proc 'foo) (process-get proc 'bar)
               (process-get proc 'missing) (plist-get (process-plist proc) 'foo))
    (delete-process proc)))"##,
        expect,
    );
}

#[test]
fn async_proc_process_send_string_cat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((acc ""))
  (let ((proc (make-process :name "neo-cat-xxx" :command '("cat")
               :connection-type 'pipe
               :filter (lambda (_p s) (setq acc (concat acc s))))))
    (set-process-query-on-exit-flag proc nil)
    (process-send-string proc "hello\nworld\n") (process-send-eof proc)
    (while (process-live-p proc) (accept-process-output proc 1))
    (list (string= acc "hello\nworld\n") (length acc))))"##,
        expect,
    );
}

#[test]
fn async_proc_sentinel_normal_exit_msg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"finished\\n\" 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((msg nil))
  (let ((proc (make-process :name "neo-sn-xxx" :command '("true")
               :sentinel (lambda (_p e) (setq msg e)))))
    (set-process-query-on-exit-flag proc nil)
    (while (process-live-p proc) (accept-process-output proc 0.1))
    (while (null msg) (accept-process-output proc 0.05))
    (list msg (process-exit-status proc))))"##,
        expect,
    );
}

#[test]
fn async_proc_shell_command_status_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (shell-command-to-string "true")
  (list (call-process-shell-command "exit 0")
        (call-process-shell-command "exit 5")))"##,
        expect,
    );
}

#[test]
fn async_proc_call_process_dest_buffer_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 \"PRE\\nX\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (insert "PRE\n")
  (let ((code (call-process "printf" nil t nil "%s\n" "X")))
    (list code (buffer-string))))"##,
        expect,
    );
}

#[test]
fn async_proc_call_process_missing_program() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (error file-missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case err
    (call-process "neo-nonexistent-prog-xyz-123" nil nil nil)
  (error (list 'error (car err))))"##,
        expect,
    );
}

#[test]
fn async_proc_current_time_string_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Mon Apr 22 14:32:48 2024\" \"Monday April 22\" \"113\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (current-time-string '(26150 29968) t)
      (format-time-string "%A %B %d" '(26150 29968) t)
      (format-time-string "%j" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn async_proc_make_pipe_process() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t open (open listen connect stop))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-pipe-process :name "neo-pipe-xxx" :noquery t)))
  (prog1 (list (processp p) (eq (process-type p) 'pipe)
               (process-status p) (process-live-p p))
    (delete-process p)))"##,
        expect,
    );
}

#[test]
fn async_proc_process_chunked_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"line1\" \"line2\" \"line3\" \"line4\" \"line5\") 1 \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((lines nil) (acc ""))
  (let ((proc (make-process :name "neo-chl-xxx"
               :command '("sh" "-c" "for i in 1 2 3 4 5; do echo line$i; done")
               :connection-type 'pipe
               :filter (lambda (_p s) (setq acc (concat acc s))
                         (while (string-match "\n" acc)
                           (push (substring acc 0 (match-beginning 0)) lines)
                           (setq acc (substring acc (match-end 0))))))))
    (set-process-query-on-exit-flag proc nil)
    (while (process-live-p proc) (accept-process-output proc 1))
    (list (nreverse lines) (length lines) acc)))"##,
        expect,
    );
}

#[test]
fn async_proc_process_inside_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"nested\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((result nil))
  (run-with-timer 0.02 nil
    (lambda () (setq result (string-trim (shell-command-to-string "echo nested")))))
  (let ((n 0)) (while (and (not result) (< n 50)) (accept-process-output nil 0.05) (setq n (1+ n))))
  (list result (equal result "nested")))"##,
        expect,
    );
}

#[test]
fn async_proc_process_send_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"REGION-DATA-123\" 15)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((acc ""))
  (with-temp-buffer (insert "REGION-DATA-123")
    (let ((proc (make-process :name "neo-psr-xxx" :command '("cat")
                 :connection-type 'pipe
                 :filter (lambda (_p s) (setq acc (concat acc s))))))
      (set-process-query-on-exit-flag proc nil)
      (process-send-region proc (point-min) (point-max)) (process-send-eof proc)
      (while (process-live-p proc) (accept-process-output proc 1))
      (list acc (length acc)))))"##,
        expect,
    );
}

#[test]
fn async_proc_process_utf8_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 14 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((acc ""))
  (let ((proc (make-process :name "neo-u8-xxx" :command '("cat")
               :connection-type 'pipe :coding 'utf-8
               :filter (lambda (_p s) (setq acc (concat acc s))))))
    (set-process-query-on-exit-flag proc nil)
    (process-send-string proc "héllo ⚡ wörld\n") (process-send-eof proc)
    (while (process-live-p proc) (accept-process-output proc 1))
    (list (string= acc "héllo ⚡ wörld\n") (length acc) (string-bytes acc))))"##,
        expect,
    );
}

#[test]
fn async_proc_sentinel_status_in_callback() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((exit) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((seen nil))
  (let ((proc (make-process :name "neo-sstc-xxx" :command '("true")
               :sentinel (lambda (p _e) (push (process-status p) seen)))))
    (set-process-query-on-exit-flag proc nil)
    (while (process-live-p proc) (accept-process-output proc 0.1))
    (let ((n 0)) (while (and (null seen) (< n 40)) (accept-process-output proc 0.05) (setq n (1+ n))))
    (list (nreverse seen) (eq (process-status proc) 'exit))))"##,
        expect,
    );
}

#[test]
fn async_proc_start_process_shell_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK \"hi\\nbye\\n\\nProcess neo-spsc-xxx finished\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((buf (generate-new-buffer " neo-spsc-xxx")))
  (let ((proc (start-process-shell-command "neo-spsc-xxx" buf "echo hi && echo bye")))
    (set-process-query-on-exit-flag proc nil)
    (while (process-live-p proc) (accept-process-output proc 1))
    (prog1 (with-current-buffer buf (buffer-string)) (kill-buffer buf))))"##,
        expect,
    );
}

#[test]
fn async_proc_timer_fires_during_wait() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (yes t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((fired nil))
  (run-with-timer 0.05 nil (lambda () (setq fired 'yes)))
  (let ((deadline 0))
    (while (and (not fired) (< deadline 40))
      (accept-process-output nil 0.05) (setq deadline (1+ deadline))))
  (list fired (eq fired 'yes)))"##,
        expect,
    );
}

#[test]
fn async_proc_timer_list_membership() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((before (length timer-list)) (timer (run-at-time 1000 nil #'ignore)) (mid (length timer-list)))
  (cancel-timer timer)
  (let ((after (length timer-list))) (list (= mid (1+ before)) (= after before))))"##,
        expect,
    );
}

#[test]
fn async_proc_process_sort_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"apple\\nbanana\\ncherry\\n\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((acc ""))
  (let ((proc (make-process :name "neo-sort-xxx" :command '("sort")
               :connection-type 'pipe
               :filter (lambda (_p s) (setq acc (concat acc s))))))
    (set-process-query-on-exit-flag proc nil)
    (process-send-string proc "banana\napple\ncherry\n") (process-send-eof proc)
    (while (process-live-p proc) (accept-process-output proc 1))
    (list acc (string= acc "apple\nbanana\ncherry\n"))))"##,
        expect,
    );
}

#[test]
fn async_proc_repeat_timer_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((n 0) (timer nil))
  (setq timer (run-with-timer 0.02 0.02 (lambda () (setq n (1+ n)))))
  (let ((k 0)) (while (and (< n 3) (< k 100)) (accept-process-output nil 0.02) (setq k (1+ k))))
  (cancel-timer timer) (list (>= n 3) (integerp n)))"##,
        expect,
    );
}

#[test]
fn async_proc_accept_output_nil_timeout() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (accept-process-output nil 0.05) (booleanp (accept-process-output nil 0.05)))"##,
        expect,
    );
}

#[test]
fn async_proc_process_filter_t_discard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (make-process :name "neo-ft-xxx" :command '("echo" "discarded") :filter t)))
  (set-process-query-on-exit-flag proc nil)
  (while (process-live-p proc) (accept-process-output proc 1))
  (list (eq (process-status proc) 'exit) (= (process-exit-status proc) 0)
        (eq (process-filter proc) t)))"##,
        expect,
    );
}

#[test]
fn async_proc_process_coding_system_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (make-process :name "neo-pcs-xxx" :command '("cat")
             :connection-type 'pipe :coding 'utf-8-unix)))
  (set-process-query-on-exit-flag proc nil)
  (prog1 (let ((cs (process-coding-system proc)))
           (list (consp cs) (coding-system-p (car cs)) (coding-system-p (cdr cs))))
    (delete-process proc)))"##,
        expect,
    );
}

#[test]
fn async_proc_processes_ordered_sentinels() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"a\" \"b\" \"c\") 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((order nil) (left 3))
  (dolist (tag '("a" "b" "c"))
    (let ((this tag))
      (make-process :name (concat "neo-os-" this) :command (list "echo" this) :noquery t
        :filter (lambda (_p s) (push (string-trim s) order))
        :sentinel (lambda (_p _e) (setq left (1- left))))))
  (let ((k 0)) (while (and (> left 0) (< k 200)) (accept-process-output nil 0.02) (setq k (1+ k))))
  (list (sort (copy-sequence order) #'string<) left))"##,
        expect,
    );
}

#[test]
fn async_proc_call_process_region_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK \"keep1\\nDELME\\nkeep2\\nkeep1\\nDELME\\nkeep2\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (insert "keep1\nDELME\nkeep2\n")
  (call-process-region (point-min) (point-max) "cat" nil t nil) (buffer-string))"##,
        expect,
    );
}

#[test]
fn async_proc_combo_timer_then_process() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"fromtimer\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((result nil) (done nil))
  (run-with-timer 0.02 nil
    (lambda () (make-process :name "neo-ctp-xxx" :command '("echo" "fromtimer") :noquery t
                 :filter (lambda (_p s) (setq result (string-trim s)))
                 :sentinel (lambda (_p _e) (setq done t)))))
  (let ((k 0)) (while (and (not done) (< k 200)) (accept-process-output nil 0.02) (setq k (1+ k))))
  (list result done (equal result "fromtimer")))"##,
        expect,
    );
}
