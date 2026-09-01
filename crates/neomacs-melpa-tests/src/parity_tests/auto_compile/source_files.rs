use expect_test::expect;

use super::ParityBatchCase;

fn auto_compile_source_file_predicate_recognizes_plain_and_representation_suffixes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_source_file_predicate_recognizes_plain_and_representation_suffixes",
        r##"(let ((load-file-rep-suffixes
                                '("" ".gz" ".xz")))
         (mapcar
          (lambda (file)
            (list file
                  (auto-compile-source-file-p file)))
          '("library.el"
            "library.el.gz"
            "library.el.xz"
            "library.elc"
            "library.el.gz.backup"
            ".el"
            "LIBRARY.EL"
            "/nested/path/library.el")))"##,
        expect![[
            r#"OK (("library.el" 7) ("library.el.gz" 7) ("library.el.xz" 7) ("library.elc" nil) ("library.el.gz.backup" nil) (".el" 0) ("LIBRARY.EL" 7) ("/nested/path/library.el" 20))"#
        ]],
    )
}

fn auto_compile_source_resolution_prefers_first_existing_representation() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_source_resolution_prefers_first_existing_representation",
        r##"(let* ((load-file-rep-suffixes
                                 '("" ".gz"))
                                (plain
                                 (auto-compile-test-write
                                  "resolve/library.el"
                                  "(provide 'library)\n"))
                                (dest
                                 (concat
                                  (file-name-sans-extension plain)
                                  ".elc"))
                                (compressed
                                 (concat plain ".gz")))
         (auto-compile-test-write
          "resolve/library.el.gz"
          "compressed")
         (list
          (file-name-nondirectory
           (auto-compile--byte-compile-source-file dest))
          (progn
            (delete-file plain)
            (file-name-nondirectory
             (auto-compile--byte-compile-source-file dest)))
          (progn
            (delete-file compressed)
            (auto-compile--byte-compile-source-file dest t))
          (file-name-nondirectory
           (auto-compile--byte-compile-source-file dest nil))))"##,
        expect![[r#"OK ("library.el" "library.el.gz" nil "library.el")"#]],
    )
}

fn auto_compile_tree_member_finds_top_level_and_deep_nested_tails() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_tree_member_finds_top_level_and_deep_nested_tails",
        r##"(let ((tree
                '(alpha
                  (beta gamma delta)
                  ((epsilon zeta) eta)
                  theta)))
         (list
          (auto-compile--tree-member 'alpha tree)
          (auto-compile--tree-member 'gamma tree)
          (auto-compile--tree-member 'zeta tree)
          (auto-compile--tree-member 'theta tree)
          (auto-compile--tree-member 'missing tree)
          (auto-compile--tree-member 'alpha 'atom)))"##,
        expect![
            "OK ((alpha (beta . #1=(gamma delta)) ((epsilon . #2=(zeta)) eta) . #3=(theta)) #1# #2# #3# nil nil)"
        ],
    )
}

fn auto_compile_tree_member_delete_mutates_middle_last_and_nested_members_correctly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_tree_member_delete_mutates_middle_last_and_nested_members_correctly",
        r##"(let ((middle
                (copy-tree
                 '(alpha beta gamma delta)))
               (last
                (copy-tree
                 '(alpha beta gamma)))
               (nested
                (copy-tree
                 '(alpha (beta gamma delta) omega))))
         (list
          (auto-compile--tree-member
           'beta middle 'delete)
          middle
          (auto-compile--tree-member
           'gamma last 'delete)
          last
          (auto-compile--tree-member
           'gamma nested 'delete)
          nested
          (auto-compile--tree-member
           'missing nested 'delete)
          nested))"##,
        expect![
            "OK (nil (alpha gamma delta) nil (alpha beta) nil #1=(alpha (beta delta) omega) nil #1#)"
        ],
    )
}

fn auto_compile_modify_mode_line_moves_single_control_between_nested_anchors() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_modify_mode_line_moves_single_control_between_nested_anchors",
        r##"(let ((original
                (default-value 'mode-line-format)))
         (unwind-protect
             (progn
               (set-default
                'mode-line-format
                '(alpha
                  (beta gamma)
                  delta
                  mode-line-auto-compile))
               (auto-compile-modify-mode-line 'gamma)
               (let ((after-gamma
                      (copy-tree
                       (default-value
                        'mode-line-format))))
                 (auto-compile-modify-mode-line 'alpha)
                 (list
                  after-gamma
                  (default-value 'mode-line-format)
                  (length
                   (seq-filter
                    (lambda (item)
                      (eq item
                          'mode-line-auto-compile))
                    (flatten-tree
                     (default-value
                      'mode-line-format)))))))
           (set-default 'mode-line-format original)))"##,
        expect![
            "OK ((alpha (beta gamma mode-line-auto-compile) delta) (alpha mode-line-auto-compile (beta gamma) delta) 1)"
        ],
    )
}

fn auto_compile_modify_mode_line_missing_anchor_removes_old_control_and_reports_message()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_modify_mode_line_missing_anchor_removes_old_control_and_reports_message",
        r##"(let ((original
                (default-value 'mode-line-format)))
         (unwind-protect
             (progn
               (set-default
                'mode-line-format
                '(alpha mode-line-auto-compile omega))
               (auto-compile-modify-mode-line
                'not-present)
               (list
                (default-value 'mode-line-format)
                (current-message)))
           (set-default 'mode-line-format original)))"##,
        expect!["OK ((alpha omega) nil)"],
    )
}

fn auto_compile_mode_line_reports_missing_destination_and_failed_compile_states() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_compile_mode_line_reports_missing_destination_and_failed_compile_states",
        r##"(let* ((source
                 (auto-compile-test-write
                  "mode-line/missing.el"
                  "(provide 'missing)\n"))
                (buffer
                 (find-file-noselect source)))
         (unwind-protect
             (with-current-buffer buffer
               (emacs-lisp-mode)
               (auto-compile-mode 1)
               (setq auto-compile-mode-line-counter t
                     auto-compile-warnings 0)
               (let ((missing
                      (mode-line-auto-compile-control)))
                 (setq auto-compile-pretend-byte-compiled t)
                 (let ((failed
                        (mode-line-auto-compile-control)))
                   (mapcar
                    (lambda (control)
                      (mapcar
                       (lambda (item)
                         (and
                          (stringp item)
                          (list
                           (substring-no-properties item)
                           (get-text-property
                            0 'help-echo item))))
                       control))
                    (list missing failed)))))
           (kill-buffer buffer)))"##,
        expect![[
            r#"OK (((":" "No compile warnings\nmouse-1 display compile log") ("-" "Byte-compile destination is writable") ("%%" "Byte-compiled file doesn't exist\nmouse-1 create")) ((":" "No compile warnings\nmouse-1 display compile log") ("-" "Byte-compile destination is writable") ("!" "Failed to byte-compile\nmouse-1 retry")))"#
        ]],
    )
}

fn auto_compile_mode_line_distinguishes_outdated_and_up_to_date_bytecode() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_mode_line_distinguishes_outdated_and_up_to_date_bytecode",
        r##"(let* ((source
                 (auto-compile-test-write
                  "mode-line/times.el"
                  "(provide 'times)\n"))
                (dest
                 (auto-compile-test-write
                  "mode-line/times.elc"
                  "placeholder"))
                (buffer
                 (find-file-noselect source)))
         (unwind-protect
             (with-current-buffer buffer
               (emacs-lisp-mode)
               (auto-compile-mode 1)
               (auto-compile-test-set-time dest 1000)
               (auto-compile-test-set-time source 2000)
               (let ((outdated
                      (mode-line-auto-compile-control)))
                 (auto-compile-test-set-time dest 3000)
                 (let ((current
                        (mode-line-auto-compile-control)))
                   (mapcar
                    (lambda (control)
                      (mapcar
                       (lambda (item)
                         (and
                          (stringp item)
                          (list
                           (substring-no-properties item)
                           (get-text-property
                            0 'help-echo item))))
                       control))
                    (list outdated current)))))
           (kill-buffer buffer)))"##,
        expect![[
            r#"OK ((("" nil) ("-" "Byte-compile destination is writable") ("*" "Byte-compiled file needs updating\nmouse-1 update")) (("" nil) ("-" "Byte-compile destination is writable") ("-" "Byte-compiled file is up-to-date\nmouse-1 remove")))"#
        ]],
    )
    .fresh_process()
}

fn auto_compile_mode_line_warning_counter_carries_practical_display_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_mode_line_warning_counter_carries_practical_display_metadata",
        r##"(let* ((source
                 (auto-compile-test-write
                  "mode-line/warnings.el"
                  "(provide 'warnings)\n"))
                (buffer
                 (find-file-noselect source)))
         (unwind-protect
             (with-current-buffer buffer
               (emacs-lisp-mode)
               (setq auto-compile-mode-line-counter t
                     auto-compile-warnings 3)
               (let* ((control
                       (mode-line-auto-compile-control))
                      (counter (car control)))
                 (list
                  (substring-no-properties counter)
                  (get-text-property
                   0 'help-echo counter)
                  (get-text-property
                   0 'face counter)
                  (get-text-property
                   0 'mouse-face counter)
                  (lookup-key
                   (get-text-property
                    0 'local-map counter)
                   [mode-line mouse-1]))))
           (kill-buffer buffer)))"##,
        expect![[
            r#"OK ("3" "3 compile warnings\nmouse-1 display compile log" error mode-line-highlight auto-compile-display-log)"#
        ]],
    )
}

pub(super) fn source_files_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_compile_source_file_predicate_recognizes_plain_and_representation_suffixes(),
        auto_compile_source_resolution_prefers_first_existing_representation(),
        auto_compile_tree_member_finds_top_level_and_deep_nested_tails(),
        auto_compile_tree_member_delete_mutates_middle_last_and_nested_members_correctly(),
        auto_compile_modify_mode_line_moves_single_control_between_nested_anchors(),
        auto_compile_modify_mode_line_missing_anchor_removes_old_control_and_reports_message(),
        auto_compile_mode_line_reports_missing_destination_and_failed_compile_states(),
        auto_compile_mode_line_distinguishes_outdated_and_up_to_date_bytecode(),
        auto_compile_mode_line_warning_counter_carries_practical_display_metadata(),
    ]
}
