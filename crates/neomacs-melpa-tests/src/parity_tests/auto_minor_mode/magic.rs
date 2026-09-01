use expect_test::expect;

use super::ParityBatchCase;

fn auto_minor_mode_magic_regex_matches_practical_file_headers_only_at_start() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_magic_regex_matches_practical_file_headers_only_at_start",
        r##"(mapcar
                           (lambda (contents)
                             (with-temp-buffer
                               (auto-minor-mode-test-reset)
                               (insert contents)
                               (setq
                                auto-minor-mode-magic-alist
                                '(("\\`#!.*\\bpython\\b"
                                   . auto-minor-mode-test-alpha-mode)))
                               (auto-minor-mode-set)
                               (list
                                contents
                                auto-minor-mode-test-alpha-mode)))
                           '("#!/usr/bin/env python\nprint('ok')\n"
                             "#!/usr/bin/python3\n"
                             "#!/bin/sh\npython app.py\n"
                             "\n#!/usr/bin/env python\n"
                             ""))"##,
        expect![[
            r##"OK (("#!/usr/bin/env python\nprint('ok')\n" t) ("#!/usr/bin/python3\n" nil) ("#!/bin/sh\npython app.py\n" nil) ("\n#!/usr/bin/env python\n" nil) ("" nil))"##
        ]],
    )
}

fn auto_minor_mode_magic_matching_is_always_case_folded() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_magic_matching_is_always_case_folded",
        r##"(mapcar
                           (lambda (contents)
                             (with-temp-buffer
                               (auto-minor-mode-test-reset)
                               (insert contents)
                               (setq
                                auto-minor-mode-magic-alist
                                '(("\\`PROJECT:"
                                   . auto-minor-mode-test-alpha-mode)))
                               (auto-minor-mode-set)
                               (list
                                contents
                                auto-minor-mode-test-alpha-mode)))
                           '("PROJECT: alpha"
                             "Project: beta"
                             "project: gamma"
                             "xPROJECT: delta"))"##,
        expect![[
            r#"OK (("PROJECT: alpha" t) ("Project: beta" t) ("project: gamma" t) ("xPROJECT: delta" nil))"#
        ]],
    )
}

fn auto_minor_mode_magic_runner_enables_all_matching_modes_and_preserves_order() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_minor_mode_magic_runner_enables_all_matching_modes_and_preserves_order",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (insert
                            "---\n"
                            "kind: service\n"
                            "language: rust\n")
                           (setq
                            auto-minor-mode-magic-alist
                            '(("\\`---"
                               . auto-minor-mode-test-alpha-mode)
                              ("\\`---\\(?:.\\|\n\\)*kind: service"
                               . auto-minor-mode-test-beta-mode)
                              ("\\`---\\(?:.\\|\n\\)*language: rust"
                               . auto-minor-mode-test-gamma-mode)))
                           (auto-minor-mode-set)
                           (list
                            auto-minor-mode-test-alpha-mode
                            auto-minor-mode-test-beta-mode
                            auto-minor-mode-test-gamma-mode
                            (nreverse
                             auto-minor-mode-test-events)))"##,
        expect![
            "OK (t t t ((:alpha 1 t 1 fundamental-mode) (:beta 1 t 1 fundamental-mode) (:gamma 1 t 1 fundamental-mode)))"
        ],
    )
}

fn auto_minor_mode_magic_function_matchers_see_reset_point_and_limited_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_magic_function_matchers_see_reset_point_and_limited_buffer",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (insert "MAGIC payload beyond-boundary")
                           (let ((magic-mode-regexp-match-limit
                                  13)
                                 matcher-events)
                             (setq
                              auto-minor-mode-magic-alist
                              (list
                               (cons
                                (lambda ()
                                  (push
                                   (list
                                    :first
                                    (point)
                                    (point-min)
                                    (point-max)
                                    (buffer-string))
                                   matcher-events)
                                  (search-forward
                                   "payload"
                                   nil
                                   t))
                                'auto-minor-mode-test-alpha-mode)
                               (cons
                                (lambda ()
                                  (push
                                   (list
                                    :second
                                    (point)
                                    (point-min)
                                    (point-max)
                                    (buffer-string))
                                   matcher-events)
                                  (looking-at "MAGIC"))
                                'auto-minor-mode-test-beta-mode)))
                             (auto-minor-mode-set)
                             (list
                              (nreverse matcher-events)
                              (nreverse
                               auto-minor-mode-test-events)
                              auto-minor-mode-test-alpha-mode
                              auto-minor-mode-test-beta-mode)))"##,
        expect![[
            r#"OK (((:first 1 1 14 "MAGIC payload") (:second 1 1 14 "MAGIC payload")) ((:alpha 1 t 14 fundamental-mode) (:beta 1 t 1 fundamental-mode)) t t)"#
        ]],
    )
}

fn auto_minor_mode_magic_match_limit_has_exact_boundary_behavior() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_magic_match_limit_has_exact_boundary_behavior",
        r##"(mapcar
                           (lambda (contents)
                             (with-temp-buffer
                               (auto-minor-mode-test-reset)
                               (insert contents)
                               (let ((magic-mode-regexp-match-limit
                                      5)
                                     (auto-minor-mode-magic-alist
                                      '(("\\`a*x"
                                         . auto-minor-mode-test-alpha-mode))))
                                 (auto-minor-mode-set)
                                 (list
                                  contents
                                  (buffer-size)
                                  auto-minor-mode-test-alpha-mode))))
                           '("aaaax"
                             "aaaaax"
                             " aaaax"
                             "x"
                             "aaaa"))"##,
        expect![[
            r#"OK (("aaaax" 5 t) ("aaaaax" 6 nil) (" aaaax" 6 nil) ("x" 1 t) ("aaaa" 4 nil))"#
        ]],
    )
}

fn auto_minor_mode_magic_scan_restores_point_mark_and_full_restriction() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_magic_scan_restores_point_mark_and_full_restriction",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (insert
                            "prefix\nMAGIC body\nsuffix")
                           (goto-char 12)
                           (set-mark 4)
                           (let ((before
                                  (list
                                   (point)
                                   (mark)
                                   (point-min)
                                   (point-max)))
                                 (auto-minor-mode-magic-alist
                                  (list
                                   (cons
                                    (lambda ()
                                      (goto-char
                                       (point-max))
                                      t)
                                    'auto-minor-mode-test-alpha-mode))))
                             (auto-minor-mode-set)
                             (list
                              before
                              (list
                               (point)
                               (mark)
                               (point-min)
                               (point-max))
                              (nreverse
                               auto-minor-mode-test-events))))"##,
        expect!["OK ((12 4 1 25) (12 4 1 25) ((:alpha 1 t 25 fundamental-mode)))"],
    )
}

fn auto_minor_mode_magic_scan_honors_preexisting_narrowing() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_magic_scan_honors_preexisting_narrowing",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (insert "ignored\nMAGIC\nignored")
                           (narrow-to-region 9 14)
                           (goto-char 11)
                           (let ((auto-minor-mode-magic-alist
                                  '(("\\`MAGIC\\'"
                                     . auto-minor-mode-test-alpha-mode))))
                             (auto-minor-mode-set)
                             (list
                              auto-minor-mode-test-alpha-mode
                              (point)
                              (point-min)
                              (point-max)
                              (buffer-string))))"##,
        expect![[r#"OK (t 11 9 14 "MAGIC")"#]],
    )
}

fn auto_minor_mode_magic_keep_skips_enabled_mode_and_runs_disabled_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_magic_keep_skips_enabled_mode_and_runs_disabled_mode",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (insert "MAGIC service")
                           (setq
                            auto-minor-mode-test-alpha-mode
                            t
                            auto-minor-mode-magic-alist
                            '(("\\`MAGIC"
                               . auto-minor-mode-test-alpha-mode)
                              ("\\`MAGIC"
                               . auto-minor-mode-test-beta-mode)))
                           (auto-minor-mode-set t)
                           (list
                            auto-minor-mode-test-alpha-mode
                            auto-minor-mode-test-beta-mode
                            (nreverse
                             auto-minor-mode-test-events)))"##,
        expect!["OK (t t ((:beta 1 t 1 fundamental-mode)))"],
    )
}

fn auto_minor_mode_magic_rules_work_in_non_file_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_magic_rules_work_in_non_file_buffers",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (rename-buffer
                            "generated-config")
                           (insert "GENERATED: true\n")
                           (setq
                            buffer-file-name nil
                            auto-minor-mode-magic-alist
                            '(("\\`GENERATED:"
                               . auto-minor-mode-test-alpha-mode)))
                           (auto-minor-mode-set)
                           (list
                            (buffer-name)
                            buffer-file-name
                            auto-minor-mode-test-alpha-mode
                            (nreverse
                             auto-minor-mode-test-events)))"##,
        expect![[r#"OK ("generated-config" nil t ((:alpha 1 t 1 fundamental-mode)))"#]],
    )
}

fn auto_minor_mode_magic_function_runs_after_major_mode_selection_and_invalid_matcher_signals()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_magic_function_runs_after_major_mode_selection_and_invalid_matcher_signals",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (insert "plain text")
                           (setq
                            buffer-file-name
                            "/project/notes.txt"
                            auto-mode-alist
                            '(("\\.txt\\'" . text-mode))
                            auto-minor-mode-magic-alist
                            (list
                             (cons
                              (lambda ()
                                (eq major-mode
                                    'text-mode))
                              'auto-minor-mode-test-alpha-mode)))
                           (set-auto-mode)
                           (let ((major-state
                                  (list
                                   major-mode
                                   auto-minor-mode-test-alpha-mode
                                   (nreverse
                                    auto-minor-mode-test-events))))
                             (list
                              major-state
                              (auto-minor-mode-test-error
                               (lambda ()
                                 (auto-minor-mode--run-magic
                                  '((17
                                     . auto-minor-mode-test-beta-mode))
                                  nil))))))"##,
        expect![
            "OK ((text-mode t ((:alpha 1 t 1 text-mode))) (:signal wrong-type-argument (stringp 17)))"
        ],
    )
}

pub(super) fn magic_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_minor_mode_magic_regex_matches_practical_file_headers_only_at_start(),
        auto_minor_mode_magic_matching_is_always_case_folded(),
        auto_minor_mode_magic_runner_enables_all_matching_modes_and_preserves_order(),
        auto_minor_mode_magic_function_matchers_see_reset_point_and_limited_buffer(),
        auto_minor_mode_magic_match_limit_has_exact_boundary_behavior(),
        auto_minor_mode_magic_scan_restores_point_mark_and_full_restriction(),
        auto_minor_mode_magic_scan_honors_preexisting_narrowing(),
        auto_minor_mode_magic_keep_skips_enabled_mode_and_runs_disabled_mode(),
        auto_minor_mode_magic_rules_work_in_non_file_buffers(),
        auto_minor_mode_magic_function_runs_after_major_mode_selection_and_invalid_matcher_signals(
        ),
    ]
}
