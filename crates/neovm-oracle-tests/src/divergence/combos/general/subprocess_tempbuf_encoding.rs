//! Divergence tests: subprocess + temp buffer + encoding + multibyte combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_process_multibyte_to_temp_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " test-mb-proc-xxx")))
    (with-current-buffer buf
      (let ((proc (start-process "test-mb-xxx" buf "printf" "caf\xc3\xa9 r\xc3\xa9sum\xc3\xa9")))
        (set-process-query-on-exit-flag proc nil)
        (while (process-live-p proc) (accept-process-output proc 1))
        (let ((output (buffer-string))
              (len (length (buffer-string))))
          (kill-buffer buf)
          (list (string-match "caf" output)
                (string-match "r" output)
                (>= len 5)
                (= (length output) (+ (length output) 0))))))) "#,
        expect,
    );
}

#[test]
fn divergence_temp_file_write_read_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil 21 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((tmp (make-temp-file "test-mb-"))
        (content "Hello \xc3\xa9 World \xc3\xa0 End"))
    (unwind-protect
        (progn
          (with-temp-buffer
            (insert content)
            (write-region (point-min) (point-max) tmp nil 'silent))
          (with-temp-buffer
            (insert-file-contents tmp)
            (let ((read-back (buffer-string)))
              (list (string= read-back content)
                    (= (length read-back) (length content))
                    (length content)
                    (= (length content) 10)))))
      (delete-file tmp)))) "#,
        expect,
    );
}

#[test]
fn divergence_encode_decode_in_temp_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil 6 6 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (with-temp-buffer
    (insert "\xc3\xa9\xc3\xa0\xc3\xb9")
    (let ((raw (buffer-string))
          (encoded (encode-coding-string (buffer-string) 'utf-8)))
      (erase-buffer)
      (insert encoded)
      (let ((roundtrip (decode-coding-string (buffer-string) 'utf-8)))
        (list (string= raw roundtrip)
              (= (length raw) (length roundtrip))
              (length raw)
              (length encoded)
              (= (length raw) 3)))))) "#,
        expect,
    );
}

#[test]
fn divergence_shell_command_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 t 0 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((out (shell-command-to-string "printf 'caf\\xc3\\xa9'")))
    (list (length out)
          (>= (length out) 3)
          (string-match "caf" out)
          (= (string-match "caf" out) 0)
          (stringp out)
          (> (length out) 0)))) "#,
        expect,
    );
}

#[test]
fn divergence_call_process_with_multibyte_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Input: \xc3\xa9\xc3\xa0\xc3\xb9 data")
  (let ((buf (generate-new-buffer " test-cpm-xxx")))
    (call-process-region 1 15 "cat" nil buf)
    (let ((output (with-current-buffer buf (buffer-string))))
      (kill-buffer buf)
      (list (string= output "Input: \xc3\xa9\xc3\xa0\xc3\xb9")
            (= (length output) 14)
            (length output))))) "#,
        expect,
    );
}

#[test]
fn divergence_temp_buffer_with_overlays_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((#(\"AAAAXXX-BBBB-CCCC-DDDD\" 0 3 (type start)) 5 12 temp 13 start) nil t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((result nil))
    (with-temp-buffer
      (insert "AAAA-BBBB-CCCC-DDDD")
      (let ((ov (make-overlay 5 9))
            (m (copy-marker 10 t)))
        (overlay-put ov 'tag 'temp)
        (put-text-property 1 4 'type 'start)
        (goto-char 5)
        (insert "XXX")
        (setq result (list (buffer-string)
                           (overlay-start ov) (overlay-end ov)
                           (overlay-get ov 'tag)
                           (marker-position m)
                           (get-text-property 1 'type)))))
    (list result
          (string= (car result) "AAAAXXXX-BBBB-CCCC-DDDD")
          (eq (nth 3 result) 'temp)
          (> (nth 4 result) 10)
          (eq (nth 5 result) 'start)))) "#,
        expect,
    );
}

#[test]
fn divergence_process_coding_system_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " test-coding-xxx")))
    (with-current-buffer buf
      (set-buffer-file-coding-system 'utf-8)
      (let ((proc (start-process "test-coding-xxx" buf "echo" "test-output")))
        (set-process-query-on-exit-flag proc nil)
        (while (process-live-p proc) (accept-process-output proc 1))
        (let ((output (buffer-string)))
          (kill-buffer buf)
          (list (string-match "test-output" output)
                (stringp output)
                (> (length output) 0)))))) "#,
        expect,
    );
}

#[test]
fn divergence_temp_file_with_rename() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t \"content1\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((tmp1 (make-temp-file "test-rn1-"))
        (tmp2 (make-temp-name "/tmp/test-rn2-")))
    (unwind-protect
        (progn
          (write-region "content1" nil tmp1 nil 'silent)
          (rename-file tmp1 tmp2)
          (list (not (file-exists-p tmp1))
                (file-exists-p tmp2)
                (with-temp-buffer
                  (insert-file-contents tmp2)
                  (buffer-string))
                (string= (with-temp-buffer
                           (insert-file-contents tmp2)
                           (buffer-string))
                         "content1")))
      (when (file-exists-p tmp2) (delete-file tmp2))
      (when (file-exists-p tmp1) (delete-file tmp1))))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_file_name_temp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " test-bfn-xxx")))
    (with-current-buffer buf
      (insert "test content")
      (let ((tmp (make-temp-file " test-bfn-")))
        (write-region (point-min) (point-max) tmp nil 'silent)
        (set-visited-file-name tmp)
        (let ((result (list (stringp (buffer-file-name))
                            (file-exists-p tmp)
                            (buffer-modified-p))))
          (set-buffer-modified-p nil)
          (kill-buffer buf)
          (delete-file tmp)
          result))))) "#,
        expect,
    );
}

#[test]
fn divergence_process_sentinel_with_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-sentinel-log-xxx nil)
  (let ((buf (generate-new-buffer " test-sent-xxx")))
    (with-current-buffer buf
      (let ((proc (start-process "test-sent-xxx" buf "echo" "sentinel-test")))
        (set-process-query-on-exit-flag proc nil)
        (set-process-sentinel proc
                              (lambda (proc event)
                                (push (format "event: %s" event) test-sentinel-log-xxx)))
        (while (process-live-p proc) (accept-process-output proc 1))
        (accept-process-output nil 0.1)
        (let ((output (buffer-string))
              (log test-sentinel-log-xxx))
          (kill-buffer buf)
          (list (string-match "sentinel-test" output)
                (>= (length log) 1)
                (stringp output)))))) "#,
        expect,
    );
}
