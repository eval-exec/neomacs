/// Batch 543: process status, process exit, process-filter, process-buffer deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx543_process_status_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK exit""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-process :name "cx543" :command '("true") :connection-type 'pipe :buffer nil)))
  (accept-process-output p 2)
  (prog1 (process-status p) (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_exit_status() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-process :name "cx543-es" :command '("sh" "-c" "exit 42") :connection-type 'pipe :buffer nil)))
  (accept-process-output p 2)
  (prog1 (process-exit-status p) (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"cx543-name\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-process :name "cx543-name" :command '("echo" "hi") :connection-type 'pipe :buffer nil)))
  (accept-process-output p 2)
  (prog1 (process-name p) (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #<buffer  *cx543-pb*>""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((buf (get-buffer-create " *cx543-pb*")))
  (let ((p (make-process :name "cx543-pb" :command '("echo" "buffer") :connection-type 'pipe :buffer buf)))
    (accept-process-output p 2)
    (prog1 (process-buffer p) (delete-process p))))
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_mark() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-process :name "cx543-pm" :command '("echo" "mark") :connection-type 'pipe :buffer nil)))
  (accept-process-output p 2)
  (prog1 (markerp (process-mark p)) (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(fboundp 'process-id)
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK utf-8-unix""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-process :name "cx543-pc" :command '("echo" "coding") :connection-type 'pipe :buffer nil :coding 'utf-8-unix)))
  (accept-process-output p 2)
  (prog1 (car (process-coding-system p)) (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_contact() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-process :name "cx543-contact" :command '("echo" "contact") :connection-type 'pipe :buffer nil)))
  (accept-process-output p 2)
  (prog1 (process-contact p) (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_plist_self() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK my-val""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-process :name "cx543-pp" :command '("echo" "plist") :connection-type 'pipe :buffer nil)))
  (accept-process-output p 2)
  (process-put p 'my-key 'my-val)
  (prog1 (process-get p 'my-key) (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_get_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-process :name "cx543-gn" :command '("echo" "gn") :connection-type 'pipe :buffer nil)))
  (accept-process-output p 2)
  (prog1 (process-get p 'nonexistent-key) (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK real""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-process :name "cx543-pt" :command '("echo" "type") :connection-type 'pipe :buffer nil)))
  (accept-process-output p 2)
  (prog1 (process-type p) (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_send_string_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(let ((buf (get-buffer-create " *cx543-ss*")))
  (let ((p (make-process :name "cx543-ss" :command '("cat") :connection-type 'pipe :buffer buf)))
    (set-process-sentinel p #'ignore)
    (set-process-query-on-exit-flag p nil)
    (process-send-string p "test\n")
    (process-send-eof p)
    (accept-process-output p 2)
    (prog1 (with-current-buffer buf (string-trim-right (buffer-string)))
      (kill-buffer buf))))
"##,
    );
}

#[test]
fn div_cx543_process_query_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 99""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-process :name "cx543-qe" :command '("sh" "-c" "exit 99") :connection-type 'pipe :buffer nil)))
  (accept-process-output p 2)
  (prog1 (process-exit-status p) (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_connection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function process-connection)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-process :name "cx543-con" :command '("echo" "conn") :connection-type 'pipe :buffer nil)))
  (accept-process-output p 2)
  (prog1 (process-connection p) (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx543_process_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"echo\" \"cmd\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (make-process :name "cx543-cmd" :command '("echo" "cmd") :connection-type 'pipe :buffer nil)))
  (accept-process-output p 2)
  (prog1 (process-command p) (delete-process p)))
"##,
        expect,
    );
}
