use expect_test::expect;

use super::ParityBatchCase;

fn auto_async_byte_compile_bug_report_variables_preserve_exact_address_and_salutation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_bug_report_variables_preserve_exact_address_and_salutation",
        r##"(list
          aabc/maintainer-mail-address
          (length
           aabc/bug-report-salutation)
          (secure-hash
           'sha256
           aabc/bug-report-salutation)
          (string-prefix-p
           "Describe bug below"
           aabc/bug-report-salutation)
          (string-suffix-p
           "write in Japanese:-)"
           aabc/bug-report-salutation))"##,
        expect![[
            r#"OK ("rubikitch@ruby-lang.org" 462 "dbdbcbaacd1a0f71ee29216201bfa0b7be5c359a017421ef6b2eac9de95cb844" t t)"#
        ]],
    )
}

fn auto_async_byte_compile_bug_report_command_forwards_exact_metadata_and_variable_inventory()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_async_byte_compile_bug_report_command_forwards_exact_metadata_and_variable_inventory",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'reporter-submit-bug-report)
                (lambda (&rest arguments)
                  (push arguments calls)
                  :report-opened)))
            (list
             (aabc/-send-bug-report)
             (mapcar
              (lambda (arguments)
                (list
                 (nth 0 arguments)
                 (nth 1 arguments)
                 (nth 2 arguments)
                 (nth 3 arguments)
                 (nth 4 arguments)
                 (nth 5 arguments)))
              (nreverse calls)))))"##,
        expect![[
            r#"OK (:report-opened (("rubikitch@ruby-lang.org" "auto-async-byte-compile.el" (aabc/bug-report-salutation aabc/maintainer-mail-address aabc/result-buffer auto-async-byte-compile-display-function auto-async-byte-compile-exclude-files-regexp auto-async-byte-compile-hook auto-async-byte-compile-init-file auto-async-byte-compile-mode auto-async-byte-compile-mode-hook auto-async-byte-compile-suppress-warnings) nil nil "Describe bug below, using a precise recipe.\n\nWhen I executed M-x ...\n\nHow to send a bug report:\n  1) Be sure to use the LATEST version of auto-async-byte-compile.el.\n  2) Enable debugger. M-x toggle-debug-on-error or (setq debug-on-error t)\n  3) Use Lisp version instead of compiled one: (load \"auto-async-byte-compile.el\")\n  4) If you got an error, please paste *Backtrace* buffer.\n  5) Type C-c C-c to send.\n# If you are a Japanese, please write in Japanese:-)")))"#
        ]],
    )
}

pub(super) fn report_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_async_byte_compile_bug_report_variables_preserve_exact_address_and_salutation(),
        auto_async_byte_compile_bug_report_command_forwards_exact_metadata_and_variable_inventory(),
    ]
}
