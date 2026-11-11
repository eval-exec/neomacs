//! Runtime NUL-byte detection policy and definition attributes.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

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
                       neovm-null-follow-dos neovm-null-alias))))
                '(nil t))))"#,
        expect_test::expect![[
            r#""OK (((neovm-null-never nil no-conversion) (neovm-null-always t utf-8-dos) (neovm-null-follow nil no-conversion) (neovm-null-follow-dos nil no-conversion) (neovm-null-alias nil no-conversion)) ((neovm-null-never nil no-conversion) (neovm-null-always t utf-8-dos) (neovm-null-follow t utf-8-dos) (neovm-null-follow-dos t utf-8-dos) (neovm-null-alias t utf-8-dos)))""#
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
