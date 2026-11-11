//! Runtime NUL-byte detection policy and definition attributes.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// GNU's C-level NUL flag follows dynamic bindings even when eval supplies an
/// opposing lexical binding of the same symbol.
#[test]
fn coding_null_byte_inhibition_ignores_lexical_eval_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(mapcar
             (lambda (dynamic)
               (let ((inhibit-null-byte-detection dynamic))
                 (eval
                  '(let ((bytes (unibyte-string 0 231 161 172 10)))
                     (list
                      (detect-coding-string bytes t)
                      (with-temp-buffer
                        (set-buffer-multibyte nil)
                        (insert bytes)
                        (detect-coding-region (point-min) (point-max) t))
                      (mapcar
                       (lambda (coding)
                         (let* ((decoded (decode-coding-string bytes coding))
                                (used last-coding-system-used))
                           (list (equal decoded "\0硬\n") used)))
                       '(undecided prefer-utf-8))))
                  (list (cons 'inhibit-null-byte-detection (not dynamic))))))
             '(nil t))"#,
        expect_test::expect![[
            r#""OK ((no-conversion no-conversion ((nil no-conversion) (nil no-conversion))) (utf-8-unix utf-8-unix ((t utf-8-unix) (t utf-8-unix))))""#
        ]],
    );
}

/// Synchronous subprocess detection honors the live NUL flag before selecting
/// the decoder, for both direct output and region input.
#[test]
fn coding_null_byte_inhibition_synchronous_process_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(mapcar
             (lambda (flag)
               (let ((inhibit-null-byte-detection flag)
                     (coding-system-for-read 'undecided)
                     (coding-system-for-write 'no-conversion))
                 (list
                  (with-temp-buffer
                    (let* ((status
                            (call-process "sh" nil t nil "-c"
                                          "printf '\\000\\347\\241\\254\\012'"))
                           (used last-coding-system-used))
                      (list status (equal (buffer-string) "\0硬\n") used)))
                  (with-temp-buffer
                    (let* ((destination (current-buffer))
                           (status
                            (with-temp-buffer
                              (set-buffer-multibyte nil)
                              (insert (unibyte-string 0 231 161 172 10))
                              (call-process-region
                               (point-min) (point-max) "cat" nil destination nil)))
                           (used last-coding-system-used))
                      (list status (equal (buffer-string) "\0硬\n") used))))))
             '(nil t))"#,
        expect_test::expect![[
            r#""OK (((0 nil no-conversion) (0 nil no-conversion)) ((0 t utf-8-unix) (0 t utf-8-unix)))""#
        ]],
    );
}

/// A dynamic NUL-inhibition binding affects both reporting and decoding, and
/// nested bindings restore the enclosing decision.
#[test]
fn coding_null_byte_inhibition_scopes_string_region_and_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((bytes (unibyte-string 0 231 161 172 10))
                  (wanted "\0硬\n")
                  (inhibit-null-byte-detection nil)
                  (probe
                   (lambda ()
                     (list (detect-coding-string bytes t)
                           (with-temp-buffer
                             (set-buffer-multibyte nil)
                             (insert bytes)
                             (detect-coding-region (point-min) (point-max) t))
                           (mapcar
                            (lambda (coding)
                              (let* ((decoded (decode-coding-string bytes coding))
                                     (used last-coding-system-used))
                                (list (equal decoded wanted) used)))
                            '(undecided prefer-utf-8))))))
             (list (funcall probe)
                   (let ((inhibit-null-byte-detection t))
                     (list (funcall probe)
                           (let ((inhibit-null-byte-detection nil)) (funcall probe))
                           (funcall probe)))
                   (funcall probe)))"#,
        expect_test::expect![[
            r#""OK ((no-conversion no-conversion ((nil no-conversion) (nil no-conversion))) ((utf-8-unix utf-8-unix ((t utf-8-unix) (t utf-8-unix))) (no-conversion no-conversion ((nil no-conversion) (nil no-conversion))) (utf-8-unix utf-8-unix ((t utf-8-unix) (t utf-8-unix)))) (no-conversion no-conversion ((nil no-conversion) (nil no-conversion))))""#
        ]],
    );
}

/// Definition-time t/nil/inherit attributes override the ambient NUL policy;
/// aliases and explicit EOL variants retain that definition's attributes.
#[test]
fn coding_null_byte_inhibition_definition_overrides_and_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
             (dolist (entry '((neovm-null-never . nil)
                              (neovm-null-always . t)
                              (neovm-null-follow . 0)))
               (define-coding-system
                 (car entry) "NUL detection attribute probe"
                 :coding-type 'undecided :mnemonic ?N :charset-list '(emacs)
                 :inhibit-null-byte-detection (cdr entry)))
             (define-coding-system-alias 'neovm-null-alias 'neovm-null-follow-dos)
             (define-coding-system-alias 'neovm-null-never-alias 'neovm-null-never)
             (define-coding-system-alias 'neovm-null-always-alias 'neovm-null-always)
             (define-coding-system-alias 'neovm-null-follow-alias 'neovm-null-follow)
             (let ((bytes (unibyte-string 231 161 172 0 13 10)))
               (mapcar
                (lambda (flag)
                  (let ((inhibit-null-byte-detection flag))
                    (mapcar
                     (lambda (coding)
                       (let* ((decoded (decode-coding-string bytes coding))
                              (used last-coding-system-used))
                         (list coding (equal decoded "硬\0\n") used)))
                     '(neovm-null-never neovm-null-always neovm-null-follow
                       neovm-null-follow-dos neovm-null-alias
                       neovm-null-never-alias-dos neovm-null-always-alias-dos
                       neovm-null-follow-alias-dos))))
                '(nil t))))"#,
        expect_test::expect![[
            r#""OK (((neovm-null-never nil no-conversion) (neovm-null-always t utf-8-dos) (neovm-null-follow nil no-conversion) (neovm-null-follow-dos nil no-conversion) (neovm-null-alias nil no-conversion) (neovm-null-never-alias-dos nil no-conversion) (neovm-null-always-alias-dos t utf-8-dos) (neovm-null-follow-alias-dos nil no-conversion)) ((neovm-null-never nil no-conversion) (neovm-null-always t utf-8-dos) (neovm-null-follow t utf-8-dos) (neovm-null-follow-dos t utf-8-dos) (neovm-null-alias t utf-8-dos) (neovm-null-never-alias-dos nil no-conversion) (neovm-null-always-alias-dos t utf-8-dos) (neovm-null-follow-alias-dos t utf-8-dos)))""#
        ]],
    );
}

/// Async buffers and filters detect under the binding active when output is
/// read, rather than the opposite binding used when the process was created.
#[test]
fn coding_null_byte_inhibition_asynchronous_process_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(mapcar
             (lambda (use-filter)
               (mapcar
                (lambda (flag)
                  (let* ((inhibit-null-byte-detection (not flag))
                         (buffer (generate-new-buffer " *null-policy-output*"))
                         (pieces nil)
                         (process
                          (make-process
                           :name "null-policy-output" :buffer buffer
                           :command '("cat") :connection-type 'pipe :noquery t
                           :coding '(undecided . no-conversion) :sentinel #'ignore
                           :filter (and use-filter
                                        (lambda (_ chunk) (push chunk pieces))))))
                    (unwind-protect
                        (let ((inhibit-null-byte-detection flag)
                              (deadline (+ (float-time) 5)))
                          (process-send-string process (unibyte-string 0 231 161 172 10))
                          (process-send-eof process)
                          (while (and (process-live-p process)
                                      (< (float-time) deadline))
                            (accept-process-output process 0.01))
                          (when (process-live-p process) (error "Process output timeout"))
                          (accept-process-output process 0.01)
                          (let ((output (if use-filter
                                            (apply #'concat (nreverse pieces))
                                          (with-current-buffer buffer (buffer-string)))))
                            (list (equal output "\0硬\n")
                                  (car (process-coding-system process)))))
                      (when (process-live-p process) (delete-process process))
                      (kill-buffer buffer))))
                '(nil t)))
             '(nil t))"#,
        expect_test::expect![[
            r#""OK (((nil no-conversion) (t utf-8-unix)) ((nil no-conversion) (t utf-8-unix)))""#
        ]],
    );
}

/// coding-system-put changes the exposed plist but does not replace GNU's
/// compiled, definition-time undecided detection attributes.
#[test]
fn coding_null_byte_inhibition_ignores_plist_only_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_expect(
        r#"(progn
             (define-coding-system
               'neovm-null-defined-on "Always ignore NUL during detection"
               :coding-type 'undecided :mnemonic ?N :charset-list '(emacs)
               :inhibit-null-byte-detection t)
             (define-coding-system
               'neovm-null-defined-off "Always detect NUL"
               :coding-type 'undecided :mnemonic ?N :charset-list '(emacs)
               :inhibit-null-byte-detection nil)
             (coding-system-put 'neovm-null-defined-on :inhibit-null-byte-detection nil)
             (coding-system-put 'neovm-null-defined-off :inhibit-null-byte-detection t)
             (let ((bytes (unibyte-string 231 161 172 0 10)))
               (mapcar
                (lambda (flag)
                  (let ((inhibit-null-byte-detection flag))
                    (mapcar
                     (lambda (coding)
                       (let* ((decoded (decode-coding-string bytes coding))
                              (used last-coding-system-used))
                         (list (coding-system-get coding :inhibit-null-byte-detection)
                               (equal decoded "硬\0\n") used)))
                     '(neovm-null-defined-on neovm-null-defined-off))))
                '(nil t))))"#,
        expect_test::expect![[
            r#""OK (((nil t utf-8-unix) (t nil no-conversion)) ((nil t utf-8-unix) (t nil no-conversion)))""#
        ]],
    );
}
