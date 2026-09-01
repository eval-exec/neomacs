use expect_test::expect;

use super::ParityBatchCase;

fn auto_indent_mode_repository_detection_walks_all_marker_types_and_caches_result()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_repository_detection_walks_all_marker_types_and_caches_result",
        r##"(let ((root
                                (expand-file-name
                                 "auto-indent-repositories"
                                 default-directory)))
         (when (file-exists-p root)
           (delete-directory root t))
         (unwind-protect
             (progn
               (make-directory root t)
               (mapcar
                (lambda (marker)
                  (let* ((repo
                          (expand-file-name
                           (substring marker 1)
                           root))
                         (nested
                          (expand-file-name "src/deep" repo))
                         (file
                          (expand-file-name "code.el" nested)))
                    (make-directory nested t)
                    (make-directory
                     (expand-file-name marker repo) t)
                    (with-temp-buffer
                      (setq buffer-file-name file
                            auto-indent-is-repository nil)
                      (let ((first
                             (auto-indent-is-repository-p))
                            (stored
                             auto-indent-is-repository))
                        (delete-directory
                         (expand-file-name marker repo) t)
                        (list
                         marker
                         first
                         (auto-indent-test-relative-or-value
                          stored root)
                         (auto-indent-is-repository-p))))))
                '(".git" ".hg" ".bzr" "_darcs")))
           (when (file-exists-p root)
             (delete-directory root t))))"##,
        expect![[
            r#"OK ((".git" t "git/" t) (".hg" t "../../../../" t) (".bzr" t "../../../../" t) ("_darcs" t "../../../../" t))"#
        ]],
    )
}

fn auto_indent_mode_non_repository_result_is_cached_as_sentinel() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_non_repository_result_is_cached_as_sentinel",
        r##"(let ((root
                                (expand-file-name
                                 "auto-indent-plain-tree"
                                 default-directory)))
         (when (file-exists-p root)
           (delete-directory root t))
         (unwind-protect
             (progn
               (make-directory
                (expand-file-name "nested" root) t)
               (with-temp-buffer
                 (setq buffer-file-name
                       (expand-file-name "nested/file.el" root)
                       auto-indent-is-repository nil)
                 (let ((first
                        (auto-indent-is-repository-p)))
                   (make-directory
                    (expand-file-name ".git" root) t)
                   (list
                    first
                    auto-indent-is-repository
                    (auto-indent-is-repository-p)))))
           (when (file-exists-p root)
             (delete-directory root t))))"##,
        expect![[r#"OK (t "[ORACLE-WORKSPACE]/" t)"#]],
    )
}

fn auto_indent_mode_aggressive_policy_combines_file_style_and_repository_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_indent_mode_aggressive_policy_combines_file_style_and_repository_state",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (setq buffer-file-name (nth 0 case)
                   auto-indent-indent-style (nth 1 case)
                   auto-indent-is-repository (nth 2 case))
             (list case (auto-indent-aggressive-p))))
         '((nil moderate nil)
           ("/file.el" aggressive not-repository)
           ("/file.el" conservative not-repository)
           ("/file.el" moderate "/repo/")
           ("/file.el" moderate not-repository)
           ("/file.el" unknown "/repo/")))"##,
        expect![[
            r#"OK (((nil moderate nil) t) (("/file.el" aggressive not-repository) t) (("/file.el" conservative not-repository) nil) (("/file.el" moderate "/repo/") nil) (("/file.el" moderate not-repository) t) (("/file.el" unknown "/repo/") t))"#
        ]],
    )
}

fn auto_indent_mode_add_to_alist_inserts_replaces_and_honors_no_replace() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_add_to_alist_inserts_replaces_and_honors_no_replace",
        r##"(progn
         (defvar auto-indent-test-alist)
         (let ((auto-indent-test-alist
                '(("Alpha" . 1)
                  ("beta" . 2))))
         (list
          (progn
            (auto-indent-add-to-alist
             'auto-indent-test-alist '("gamma" . 3))
            (copy-tree auto-indent-test-alist))
          (progn
            (auto-indent-add-to-alist
             'auto-indent-test-alist '("Alpha" . 10))
            (copy-tree auto-indent-test-alist))
          (progn
            (auto-indent-add-to-alist
             'auto-indent-test-alist '("Alpha" . 99) t)
            (copy-tree auto-indent-test-alist))
          (progn
            (auto-indent-add-to-alist
             'auto-indent-test-alist '("BETA" . 20))
            (copy-tree auto-indent-test-alist)))))"##,
        expect![[
            r#"OK ((("gamma" . 3) ("Alpha" . 1) ("beta" . 2)) (("gamma" . 3) ("Alpha" . 10) ("beta" . 2)) (("gamma" . 3) ("Alpha" . 10) ("beta" . 2)) (("BETA" . 20) ("gamma" . 3) ("Alpha" . 10) ("beta" . 2)))"#
        ]],
    )
}

fn auto_indent_mode_pair_interval_uses_major_mode_then_default_and_throttle() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_pair_interval_uses_major_mode_then_default_and_throttle",
        r##"(with-temp-buffer
         (insert "one\ntwo\nthree\nfour\nfive\n")
         (setq auto-indent-pairs-begin (point-min)
               auto-indent-pairs-end (point-max)
               major-mode 'fixture-mode)
         (mapcar
          (lambda (case)
            (setq auto-indent-next-pair-timer-geo-mean
                  (nth 0 case)
                  auto-indent-next-pair-throttle
                  (nth 1 case))
            (list case
                  (auto-indent-par-region-interval)))
          '((((fixture-mode 0.2 4)
              (default 0.5 1))
             nil)
            (((other-mode 0.2 4)
              (default 0.5 1))
             nil)
            (((fixture-mode 0.2 4)
              (default 0.5 1))
             0.3)
            (((fixture-mode "NaN" 4)
              (default 0.5 1))
             1))))"##,
        expect![[
            r#"OK (((((fixture-mode 0.2 4) (default 0.5 1)) nil) 1.0) ((((other-mode 0.2 4) (default 0.5 1)) nil) 2.5) ((((fixture-mode 0.2 4) (default 0.5 1)) 0.3) 0.3) ((((fixture-mode "NaN" 4) (default 0.5 1)) 1) 0.0025))"#
        ]],
    )
}

fn auto_indent_mode_pair_interval_update_records_observed_rate_per_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_pair_interval_update_records_observed_rate_per_line",
        r##"(with-temp-buffer
         (insert "one\ntwo\nthree\nfour\n")
         (setq auto-indent-pairs-begin (point-min)
               auto-indent-pairs-end (point-max)
               major-mode 'fixture-mode
               auto-indent-next-pair-timer-geo-mean
               '((default 0.001 0)))
         (let ((first
                (progn
                  (auto-indent-par-region-interval-update 0.04)
                  (copy-tree
                   auto-indent-next-pair-timer-geo-mean)))
               (second
                (progn
                  (auto-indent-par-region-interval-update 0.08)
                  (copy-tree
                   auto-indent-next-pair-timer-geo-mean))))
           (list first second)))"##,
        expect![
            "OK (((fixture-mode 0.01 1) (default 0.001 0)) ((fixture-mode 0.0010000000000000002 2) (default 0.001 0)))"
        ],
    )
}

fn auto_indent_mode_save_interval_only_persists_when_both_options_enable() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_save_interval_only_persists_when_both_options_enable",
        r##"(let (calls)
         (cl-letf (((symbol-function 'customize-save-variable)
                    (lambda (symbol value)
                      (push (list symbol value) calls)
                      :saved)))
           (mapcar
            (lambda (case)
              (setq auto-indent-next-pair (car case)
                    auto-indent-save-next-pair (cdr case)
                    auto-indent-next-pair-timer-geo-mean
                    '((default 0.02 3)))
              (let ((result
                     (auto-indent-save-par-region-interval)))
                (list case result (copy-tree calls))))
            '((nil . nil)
              (t . nil)
              (nil . t)
              (t . t)))))"##,
        expect![
            "OK (((nil) nil nil) ((t) nil nil) ((nil . t) nil nil) ((t . t) :saved ((auto-indent-next-pair-timer-geo-mean ((default 0.02 3))))))"
        ],
    )
}

fn auto_indent_mode_save_interval_swallows_customization_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_save_interval_swallows_customization_errors",
        r##"(let ((auto-indent-next-pair t)
             (auto-indent-save-next-pair t)
             (auto-indent-next-pair-timer-geo-mean
              '((default 0.02 3))))
         (cl-letf (((symbol-function 'customize-save-variable)
                    (lambda (&rest _arguments)
                      (error "read-only customization file"))))
           (list
            (auto-indent-save-par-region-interval)
            auto-indent-next-pair-timer-geo-mean)))"##,
        expect!["OK (nil ((default 0.02 3)))"],
    )
}

fn auto_indent_mode_repository_cache_is_buffer_local_between_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_repository_cache_is_buffer_local_between_files",
        r##"(let ((first
                                (generate-new-buffer
                                 " *auto-indent-repo*"))
             (second
              (generate-new-buffer
               " *auto-indent-plain*")))
         (unwind-protect
             (progn
               (with-current-buffer first
                 (set (make-local-variable
                       'auto-indent-is-repository)
                      "/repo/"))
               (with-current-buffer second
                 (set (make-local-variable
                       'auto-indent-is-repository)
                      'not-repository))
               (list
                (with-current-buffer first
                  (list auto-indent-is-repository
                        (auto-indent-is-repository-p)))
                (with-current-buffer second
                  (list auto-indent-is-repository
                        (auto-indent-is-repository-p)))))
           (kill-buffer first)
           (kill-buffer second)))"##,
        expect![[r#"OK (("/repo/" t) (not-repository nil))"#]],
    )
}

pub(super) fn repository_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_indent_mode_repository_detection_walks_all_marker_types_and_caches_result(),
        auto_indent_mode_non_repository_result_is_cached_as_sentinel(),
        auto_indent_mode_aggressive_policy_combines_file_style_and_repository_state(),
        auto_indent_mode_add_to_alist_inserts_replaces_and_honors_no_replace(),
        auto_indent_mode_pair_interval_uses_major_mode_then_default_and_throttle(),
        auto_indent_mode_pair_interval_update_records_observed_rate_per_line(),
        auto_indent_mode_save_interval_only_persists_when_both_options_enable(),
        auto_indent_mode_save_interval_swallows_customization_errors(),
        auto_indent_mode_repository_cache_is_buffer_local_between_files(),
    ]
}
