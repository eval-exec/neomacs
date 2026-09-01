use expect_test::expect;

use super::ParityBatchCase;

fn projectile_task_safety_rejects_executable_or_malformed_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_task_safety_rejects_executable_or_malformed_values",
        r##"(list
              (projectile-tasks-safe-p nil)
              (projectile-tasks-safe-p
               '(("lint" . "make lint")
                 ("docs" . "make docs")))
              (projectile-tasks-safe-p '(("lint" . ignore)))
              (projectile-tasks-safe-p
               '(("lint" . (lambda () "make lint"))))
              (projectile-tasks-safe-p "make lint")
              (projectile-tasks-safe-p '("lint"))
              (projectile-tasks-safe-p '((lint . "make lint")))
              (projectile-tasks-safe-p
               '(("lint" . "make lint") . "junk")))"##,
        expect![[r#"OK (t t nil nil nil nil nil nil)"#]],
    )
}

fn projectile_project_tasks_merge_configured_and_type_tasks_with_stable_precedence()
-> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_project_tasks_merge_configured_and_type_tasks_with_stable_precedence",
        r##"(let ((projectile-project-types nil)
                    (projectile-project-root-files nil)
                    (projectile-discover-tasks nil)
                    (projectile-tasks
                     '(("lint" . "make custom-lint")
                       ("docs" . "make docs"))))
               (projectile-register-project-type
                'tasked '("Taskedfile")
                :tasks
                '(("lint" . "make lint")
                  ("bench" . "make bench")))
               (list
                (copy-tree (projectile-project-tasks 'tasked))
                (let ((projectile-tasks nil))
                  (copy-tree (projectile-project-tasks 'tasked)))
                (copy-tree (projectile-project-tasks 'generic))))"##,
        expect![[
            r#"OK ((("lint" . "make custom-lint") ("docs" . "make docs") ("bench" . "make bench")) (("lint" . "make lint") ("bench" . "make bench")) (("lint" . "make custom-lint") ("docs" . "make docs")))"#
        ]],
    )
}

pub(super) fn tasks_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        projectile_task_safety_rejects_executable_or_malformed_values(),
        projectile_project_tasks_merge_configured_and_type_tasks_with_stable_precedence(),
    ]
}
