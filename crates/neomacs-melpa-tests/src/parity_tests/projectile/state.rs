use expect_test::expect;

use super::ParityBatchCase;

fn projectile_known_project_serialization_and_merge_preserve_disk_and_memory_changes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_known_project_serialization_and_merge_preserve_disk_and_memory_changes",
        r##"(let* ((root (make-temp-file "projectile-state-" t))
                    (projectile-known-projects-file
                     (expand-file-name "known-projects.eld" root))
                    (projectile-known-projects nil))
               (unwind-protect
                   (progn
                     (projectile-serialize
                      '("a1" "a2" "a3" "a4" "a5")
                      projectile-known-projects-file)
                     (projectile-load-known-projects)
                     (let ((loaded (copy-sequence projectile-known-projects)))
                       (projectile-serialize
                        '("a3" "b1" "a1" "a4" "b2")
                        projectile-known-projects-file)
                       (setq projectile-known-projects
                             '("a6" "a1" "a2" "a3" "a5"))
                       (projectile-merge-known-projects)
                       (list
                        loaded
                        projectile-known-projects
                        (projectile-unserialize
                         projectile-known-projects-file))))
                 (delete-directory root t)))"##,
        expect![[
            r#"OK (("a1" "a2" "a3" "a4" "a5") ("a6" "a1" "a3" "b1" "b2") ("a6" "a1" "a3" "b1" "b2"))"#
        ]],
    )
}

fn projectile_known_project_cleanup_and_recursive_forget_update_persisted_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "projectile_known_project_cleanup_and_recursive_forget_update_persisted_state",
        r##"(let* ((root (file-name-as-directory
                              (make-temp-file "projectile-clean-" t)))
                    (child-a (file-name-as-directory
                              (expand-file-name "a" root)))
                    (child-b (file-name-as-directory
                              (expand-file-name "b" root)))
                    (nested (file-name-as-directory
                             (expand-file-name "a/nested" root)))
                    (outside (file-name-as-directory
                              (make-temp-file
                               "projectile-clean-outside-" t)))
                    (projectile-known-projects-file
                     (expand-file-name "known-projects.eld" root))
                    (projectile-known-projects
                     (list child-a child-b nested outside)))
               (unwind-protect
                   (progn
                     (make-directory nested t)
                     (make-directory child-b t)
                     (projectile-save-known-projects)
                     (delete-directory child-b)
                     (projectile--cleanup-known-projects)
                     (let ((after-cleanup
                            (mapcar
                             (lambda (path)
                               (if (string-prefix-p root path)
                                   (file-relative-name path root)
                                 "outside/"))
                             projectile-known-projects))
                           (removed
                            (projectile-forget-projects-under root t)))
                       (list
                        after-cleanup
                        removed
                        (mapcar
                         (lambda (path)
                           (if (equal path outside)
                               "outside/"
                             (file-relative-name path root)))
                         projectile-known-projects)
                        (mapcar
                         (lambda (path)
                           (if (equal path outside)
                               "outside/"
                             (file-relative-name path root)))
                         (projectile-unserialize
                          projectile-known-projects-file)))))
                 (delete-directory root t)
                 (delete-directory outside t)))"##,
        expect![[r#"OK (("a/" "a/nested/" "outside/") 2 ("outside/") ("outside/"))"#]],
    )
}

fn projectile_buffer_conditions_compose_name_mode_file_and_boolean_operators() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_buffer_conditions_compose_name_mode_file_and_boolean_operators",
        r##"(let ((buffer (generate-new-buffer
                           "*projectile-condition-test*")))
               (unwind-protect
                   (with-current-buffer buffer
                     (emacs-lisp-mode)
                     (setq buffer-file-name "/virtual/project/src/demo.el")
                     (list
                      (projectile--buffer-matches-conditions
                       buffer '("\\`\\*projectile-condition-test\\*\\'"))
                      (projectile--buffer-matches-conditions
                       buffer '(buffer-file-name))
                      (projectile--buffer-matches-conditions
                       buffer '((major-mode . emacs-lisp-mode)))
                      (projectile--buffer-matches-conditions
                       buffer '((derived-mode . prog-mode)))
                      (projectile--buffer-matches-conditions
                       buffer
                       '((and buffer-file-name
                              (derived-mode . prog-mode))))
                      (projectile--buffer-matches-conditions
                       buffer
                       '((or (derived-mode . text-mode)
                             (major-mode . emacs-lisp-mode))))
                      (projectile--buffer-matches-conditions
                       buffer
                       '((not (derived-mode . text-mode))))
                      (projectile--buffer-matches-conditions
                       buffer
                       '((and buffer-file-name
                              (derived-mode . text-mode))))
                      (projectile--buffer-matches-conditions buffer nil)))
                 (kill-buffer buffer)))"##,
        expect![[r#"OK (t t t t t t t nil nil)"#]],
    )
}

fn projectile_project_info_outside_a_project_signals_the_friendly_error() -> ParityBatchCase {
    ParityBatchCase::signal(
        "projectile_project_info_outside_a_project_signals_the_friendly_error",
        r##"(let ((default-directory "/virtual/no-project/")
                   (projectile-require-project-root t))
               (projectile-ensure-project nil))"##,
        expect![[
            r#"ERR (user-error "Projectile cannot find a project definition in /virtual/no-project/")"#
        ]],
    )
}

pub(super) fn state_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        projectile_known_project_serialization_and_merge_preserve_disk_and_memory_changes(),
        projectile_known_project_cleanup_and_recursive_forget_update_persisted_state(),
        projectile_buffer_conditions_compose_name_mode_file_and_boolean_operators(),
        projectile_project_info_outside_a_project_signals_the_friendly_error(),
    ]
}
