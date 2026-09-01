//! Divergence tests: complex window + buffer + process combinations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_save_window_excursion_buffer_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (t t (#<buffer *test-swe*> 20 \"temp buffer content\"))""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((orig-buf (current-buffer))
        (orig-point (point))
        (result (save-window-excursion
                  (let ((tmp (generate-new-buffer \"*test-swe*\")))
                    (with-current-buffer tmp
                      (insert \"temp buffer content\")
                      (goto-char (point-max)))
                    (set-buffer tmp)
                    (list (current-buffer)
                          (point)
                          (buffer-string))))))
  (list (eq (current-buffer) orig-buf)
        (= (point) orig-point)
        result)) ",
        expect,
    );
}

#[test]
fn divergence_temp_buffer_insert_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 3 8)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (put-text-property 1 6 'face 'bold)
  (let ((result
         (with-temp-buffer
           (insert-buffer-substring (other-buffer (current-buffer)) 3 8)
           (list (buffer-string)
                 (get-text-property 1 'face)
                 (get-text-property 3 'face)))))
    (list result (buffer-string)))) ",
        expect,
    );
}

#[test]
fn divergence_buffer_list_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (require 'cl-lib)
  (let* ((b1 (generate-new-buffer \"*test-bl-1*\"))
        (b2 (generate-new-buffer \"*test-bl-2*\"))
        (b3 (generate-new-buffer \"*test-bl-3*\")))
  (with-current-buffer b1 (insert \"one\"))
  (with-current-buffer b2 (insert \"two\"))
  (with-current-buffer b3 (insert \"three\"))
  (bury-buffer b1)
  (let ((order (mapcar (lambda (b) (buffer-name b))
                       (cl-remove-if-not
                        (lambda (b) (string-match \"test-bl\" (buffer-name b)))
                        (buffer-list)))))
    (kill-buffer b1)
    (kill-buffer b2)
    (kill-buffer b3)
    (list (length order) (>= (length order) 3))))) ",
        expect,
    );
}

#[test]
fn divergence_call_process_to_temp_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello world\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((result
        (with-temp-buffer
          (call-process \"echo\" nil t nil \"hello\" \"world\")
          (let ((output (buffer-string)))
            (list (string-trim output)
                  (string= (string-trim output) \"hello world\"))))))
  result) ",
        expect,
    );
}

#[test]
fn divergence_buffer_local_vars_across_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (buf1 buf2 buf1 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((b1 (generate-new-buffer \"*test-blv-1*\"))
        (b2 (generate-new-buffer \"*test-blv-2*\")))
  (with-current-buffer b1
    (setq-local test-cross-blv-xxx 'buf1)
    (insert \"content1\"))
  (with-current-buffer b2
    (setq-local test-cross-blv-xxx 'buf2)
    (insert \"content2\"))
  (let ((v1 (with-current-buffer b1 test-cross-blv-xxx))
        (v2 (with-current-buffer b2 test-cross-blv-xxx))
        (v3 (prog1
                (with-current-buffer b1 test-cross-blv-xxx)
              (set-buffer b2))))
    (kill-buffer b1)
    (kill-buffer b2)
    (list v1 v2 v3 (eq v1 'buf1) (eq v2 'buf2)))) ",
        expect,
    );
}

#[test]
fn divergence_get_buffer_create_kill_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((name \"*test-cycle-buf*\")
        (b1 (get-buffer-create name)))
  (with-current-buffer b1 (insert \"data\"))
  (let ((alive1 (buffer-live-p b1)))
    (kill-buffer b1)
    (let ((alive2 (buffer-live-p b1))
          (b2 (get-buffer-create name)))
      (with-current-buffer b2 (insert \"new\"))
      (list alive1 alive2
            (eq b1 b2)
            (not (eq b1 b2))
            (buffer-live-p b2)
            (kill-buffer b2))))) ",
        expect,
    );
}

#[test]
fn divergence_window_config_save_restore_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 1 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((wc (current-window-configuration)))
  (split-window nil nil 'right)
  (let ((n1 (length (window-list))))
    (set-window-configuration wc)
    (let ((n2 (length (window-list))))
      (list n1 n2
            (= n2 1)
            (>= n1 2))))) ",
        expect,
    );
}

#[test]
fn divergence_shell_command_parse_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((raw (shell-command-to-string \"echo 'line1'; echo 'line2'; echo 'line3'\"))
        (lines (split-string raw \"\\n\" t)))
  (list (length lines)
        (>= (length lines) 3)
        (string= (nth 0 lines) \"line1\")
        (string= (nth 1 lines) \"line2\"))) ",
        expect,
    );
}

#[test]
fn divergence_minibuffer_window_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((mw (minibuffer-window)))
  (list (windowp mw)
        (minibufferp (window-buffer mw))
        (>= (window-height mw) 1)
        (>= (window-width mw) 1)
        (eq (window-frame mw) (selected-frame)))) ",
        expect,
    );
}

#[test]
fn divergence_with_temp_buffer_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((outer-buf (current-buffer)))
  (with-temp-buffer
    (insert \"temp content\")
    (let ((temp-buf (current-buffer)))
      (list (not (eq temp-buf outer-buf))
            (buffer-string)
            (point-max))))
  (list (eq (current-buffer) outer-buf)
        (= (point) (point-min)))) ",
        expect,
    );
}
