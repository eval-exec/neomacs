//! Process coding/lifecycle parity: set/get coding-system, filter-multibyte,
//! tty-name, send+eof through wc, call-process exit/signal, unibyte high-byte
//! send, and :stop flag handling.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_proc_send_unibyte_highbyte_latin1() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Cannot convert character at index 1 to unibyte\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((acc ""))
  (let ((proc (make-process :name "neo-cl1-xxx" :command '("cat")
               :connection-type 'pipe :coding 'latin-1
               :filter (lambda (_p s) (setq acc (concat acc s))))))
    (set-process-query-on-exit-flag proc nil)
    (process-send-string proc (unibyte-string 72 233 108 108 111 10))
    (process-send-eof proc)
    (while (process-live-p proc) (accept-process-output proc 1))
    (list (length acc) (multibyte-string-p acc) (append (string-to-unibyte acc) nil))))"##,
        expect,
    );
}

#[test]
fn proc_exit_code_via_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 \"Terminated\" 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (call-process "sh" nil nil nil "-c" "exit 0")
        (call-process "sh" nil nil nil "-c" "kill -TERM $$")
        (call-process-shell-command "true"))"##,
        expect,
    );
}

#[test]
fn proc_filter_multibyte_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function set-process-filter-multibyte)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (make-process :name "neo-fmb-xxx" :command '("cat") :connection-type 'pipe :noquery t)))
  (set-process-filter-multibyte proc nil)
  (prog1 (list (process-filter-multibyte-p proc))
    (set-process-filter-multibyte proc t)
    (list (process-filter-multibyte-p proc))
    (delete-process proc)))"##,
        expect,
    );
}

#[test]
fn divergence_proc_make_process_stop_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument null t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (make-process :name "neo-stp-xxx" :command '("cat")
                          :connection-type 'pipe :stop t :noquery t)))
  (prog1 (list (process-status proc) (processp proc))
    (continue-process proc) (delete-process proc)))"##,
        expect,
    );
}

#[test]
fn proc_send_then_close() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((acc ""))
  (let ((proc (make-process :name "neo-stc-xxx" :command '("wc" "-c")
               :connection-type 'pipe
               :filter (lambda (_p s) (setq acc (concat acc s))))))
    (set-process-query-on-exit-flag proc nil)
    (process-send-string proc "12345")
    (process-send-eof proc)
    (while (process-live-p proc) (accept-process-output proc 1))
    (string-to-number (string-trim acc))))"##,
        expect,
    );
}

#[test]
fn proc_set_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (utf-8-unix latin-1-unix)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (make-process :name "neo-scs-xxx" :command '("cat") :connection-type 'pipe :noquery t)))
  (set-process-coding-system proc 'utf-8-unix 'latin-1-unix)
  (prog1 (let ((cs (process-coding-system proc))) (list (car cs) (cdr cs)))
    (delete-process proc)))"##,
        expect,
    );
}

#[test]
fn proc_tty_name_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (make-process :name "neo-tty-xxx" :command '("cat") :connection-type 'pipe :noquery t)))
  (prog1 (list (process-tty-name proc) (null (process-tty-name proc)))
    (delete-process proc)))"##,
        expect,
    );
}
