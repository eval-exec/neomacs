//! GNU has no C DEFVAR for `read-file-name-completion-ignore-case`: its
//! defcustom (`lisp/minibuffer.el:3117`) selects the platform default, which
//! is t on darwin/windows-nt/ms-dos/cygwin. Pre-seeding the name in the Rust
//! bootstrap shadowed that initializer (#667).

use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn read_file_name_completion_ignore_case_matches_gnus_platform_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(eq read-file-name-completion-ignore-case
               (if (memq system-type '(ms-dos windows-nt darwin cygwin)) t nil))"#,
        expect_test::expect![[r#""OK t""#]],
    );
}

/// Exercise the real Lisp initializer and its file-completion table on either
/// host platform. The explicitly unbound members are initializer controls;
/// fresh-runtime absence is covered by the native bootstrap regression.
#[test]
fn read_file_name_real_initializer_preserves_preferences_and_controls_completion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let* ((variable 'read-file-name-completion-ignore-case)
                  (directory (make-temp-file "neovm-completion-default-" t))
                  (default-directory (file-name-as-directory directory)))
             (unwind-protect
                 (progn
                   (make-directory (expand-file-name "Games" directory))
                   (mapcar
                    (lambda (entry)
                      (let ((system-type (car entry)))
                        (put variable 'saved-value nil)
                        (cond
                         ((eq (cadr entry) 'unbound) (makunbound variable))
                         ((eq (cadr entry) 'existing) (set variable (caddr entry)))
                         ((eq (cadr entry) 'saved)
                          (makunbound variable)
                          (put variable 'saved-value (list (caddr entry)))))
                        (load "minibuffer" nil t)
                        (let ((completion-ignore-case (symbol-value variable)))
                          (list system-type (cadr entry) (symbol-value variable)
                                (read-file-name-internal "ga" nil t)))))
                    '((darwin unbound) (gnu/linux unbound)
                      (darwin existing nil) (gnu/linux existing t)
                      (darwin saved nil) (gnu/linux saved t))))
               (delete-directory directory t)))"#,
        expect_test::expect![[
            r#""OK ((darwin unbound t (\"Games/\")) (gnu/linux unbound nil nil) (darwin existing nil nil) (gnu/linux existing t (\"Games/\")) (darwin saved nil nil) (gnu/linux saved t (\"Games/\")))""#
        ]],
    );
}
