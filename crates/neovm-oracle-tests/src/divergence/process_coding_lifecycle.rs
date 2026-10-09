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

const PROCESS_SEND_ENCODING_CASES: &str =
    include_str!("../../../neomacs-gui-tests/fixtures/process-send-encoding-cases.el");

fn process_encoding_case_results(function: &str) -> String {
    format!(
        "(progn {PROCESS_SEND_ENCODING_CASES} (mapcar (lambda (case) (list (nth 2 case) (nth 4 case))) ({function})))"
    )
}

#[test]
fn process_send_string_and_region_encode_japanese_multibyte_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"a4aba4f3a4b80a\" t) (\"a4aba4f3a4b80a\" t) (\"82a982f182b60a\" t) (\"82a982f182b60a\" t) (\"a4aba4f3a4b80a\" t) (\"a4aba4f3a4b80a\" t) (\"e3818be38293e381980a\" t) (\"e3818be38293e381980a\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        &process_encoding_case_results("neomacs-process-encoding-multibyte-cases"),
        expect,
    );
}

#[test]
fn process_send_unibyte_string_and_region_keep_bytes_and_eol_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"a4ab0d0a410d0d0a\" t) (\"a4ab0a410d0a\" t) (\"a4ab0d0a410d0d0a\" t) (\"a4ab0a410d0a\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        &process_encoding_case_results("neomacs-process-encoding-unibyte-cases"),
        expect,
    );
}

#[test]
fn process_send_iso2022_preserves_state_between_calls_and_eof() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"1b2442242b24732438\" t) (\"1b2442242b24732438\" t) (\"1b2442242b24732438\" t) (\"1b2442242b80a41b24422473\" t) (\"1b2442242b411b24422473\" t) (\"1b2442242b1b24422473\" t) (\"1b2442242b1b2842411b24422473\" t) (\"1b2442242b2473\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        &process_encoding_case_results("neomacs-process-encoding-stateful-cases"),
        expect,
    );
}

#[test]
fn process_send_legacy_pre_write_hooks_transform_and_reenter_processes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"82a95a\" t) (\"82a95a\" t) (\"1b24422473242b24732438\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        &process_encoding_case_results("neomacs-process-encoding-hook-cases"),
        expect,
    );
}

#[test]
fn process_send_retains_live_nested_and_initial_raw_eol_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"58e3818b58e38293\" t) (\"410a\" t) (\"410a\" t))""#]];
    crate::common::assert_oracle_parity_expect(
        &process_encoding_case_results("neomacs-process-encoding-descriptor-cases"),
        expect,
    );
}

#[test]
fn process_send_repeated_binary_sends_retain_last_coding_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (binary binary)""#]];
    crate::common::assert_oracle_parity_expect(
        &format!(
            "(progn {PROCESS_SEND_ENCODING_CASES} (neomacs-process-encoding-binary-metadata))"
        ),
        expect,
    );
}

#[test]
fn accepted_tcp_clients_isolate_plist_spines_but_share_nested_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let fixture = include_str!("../../../neomacs-gui-tests/fixtures/process-plist-isolation.el");
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t 3)""#]];
    crate::common::assert_oracle_parity_expect(
        &format!("(progn {fixture} (neomacs-process-plist-isolation-check))"),
        expect,
    );
}
