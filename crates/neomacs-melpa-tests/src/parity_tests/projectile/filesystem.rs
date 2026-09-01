use expect_test::expect;

use super::ParityBatchCase;

fn projectile_root_strategies_find_expected_marker_levels() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_root_strategies_find_expected_marker_levels",
        r##"(let* ((root (make-temp-file "projectile-root-" t))
                    (default-directory (file-name-as-directory root))
                    (project (expand-file-name "project/" root))
                    (nested (expand-file-name "src/lib/" project))
                    (sandbox-parent
                     (file-name-directory (directory-file-name root)))
                    (locate-dominating-stop-dir-regexp
                     (concat "\\`"
                             (regexp-quote sandbox-parent)
                             "\\'")))
               (unwind-protect
                   (progn
                     (make-directory (expand-file-name ".git/" project) t)
                     (make-directory nested t)
                     (with-temp-file
                         (expand-file-name "Makefile" project))
                     (with-temp-file
                         (expand-file-name "package.json"
                                           (file-name-directory nested)))
                     (list
                      (equal
                       (projectile-root-bottom-up nested '(".git"))
                       project)
                      (equal
                       (projectile-root-bottom-up nested '("package.json"))
                       (file-name-directory nested))
                      (equal
                       (projectile-root-top-down nested '("Makefile"))
                       project)
                      (projectile-root-top-down nested '(".git"))
                      (equal
                       (projectile-root-top-down-recurring nested '(".git"))
                       project)))
                 (delete-directory root t)))"##,
        expect![[r#"OK (t t t nil t)"#]],
    )
}

fn projectile_project_root_contract_handles_marked_project_and_nil_directory() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_project_root_contract_handles_marked_project_and_nil_directory",
        r##"(let* ((root (make-temp-file "projectile-project-" t))
                    (project (file-name-as-directory root))
                    (nested (expand-file-name "src/lib/" project))
                    (projectile-project-root-cache
                     (make-hash-table :test 'equal)))
               (unwind-protect
                   (progn
                     (make-directory nested t)
                     (with-temp-file
                         (expand-file-name ".projectile" project))
                     (list
                      (equal (projectile-project-root nested) project)
                      (file-name-absolute-p
                       (projectile-project-root nested))
                      (let ((default-directory nil))
                        (projectile-project-root))))
                 (delete-directory root t)))"##,
        expect![[r#"OK (t t nil)"#]],
    )
}

fn projectile_native_directory_indexing_filters_ignored_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_native_directory_indexing_filters_ignored_paths",
        r##"(let* ((root (make-temp-file "projectile-index-" t))
                    (project (file-name-as-directory root))
                    (projectile-globally-ignored-directories
                     '(".git" "build"))
                    (projectile-globally-ignored-files '("ignored.txt"))
                    (projectile-globally-ignored-file-suffixes '(".log"))
                    (projectile-globally-unignored-directories nil)
                    (projectile-globally-unignored-files nil)
                    (projectile-dirconfig-file ".projectile"))
               (unwind-protect
                   (progn
                     (make-directory (expand-file-name ".git/" root))
                     (make-directory (expand-file-name "build/" root))
                     (make-directory (expand-file-name "src/" root))
                     (dolist (file '("keep.el" "ignored.txt" "trace.log"))
                       (with-temp-file (expand-file-name file root)))
                     (with-temp-file
                         (expand-file-name "src/nested.el" root))
                     (let ((default-directory project))
                       (sort (projectile-dir-files-native project)
                             #'string<)))
                 (delete-directory root t)))"##,
        expect![[r#"OK ("keep.el" "src/nested.el")"#]],
    )
}

fn projectile_task_manifest_parsers_read_controlled_project_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_task_manifest_parsers_read_controlled_project_files",
        r##"(let* ((root (make-temp-file "projectile-tasks-" t))
                    (project (file-name-as-directory root)))
               (unwind-protect
                   (progn
                     (with-temp-file (expand-file-name "package.json" root)
                       (insert
                        "{\"scripts\":{\"build\":\"tsc\",\"test\":\"vitest run\"}}"))
                     (with-temp-file (expand-file-name "deno.json" root)
                       (insert
                        "{\"tasks\":{\"dev\":\"deno run -A main.ts\"}}"))
                     (with-temp-file (expand-file-name "composer.json" root)
                       (insert
                        "{\"scripts\":{\"lint\":\"php-cs-fixer fix\"}}"))
                     (list
                      (projectile-tasks-from-npm project)
                      (projectile-tasks-from-deno project)
                      (projectile-tasks-from-composer project)))
                 (delete-directory root t)))"##,
        expect![[
            r#"OK ((("npm:build" . "npm run build") ("npm:test" . "npm run test")) (("deno:dev" . "deno task dev")) (("composer:lint" . "composer run-script lint")))"#
        ]],
    )
}

fn projectile_text_task_parsers_ignore_assignments_and_nested_keys() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_text_task_parsers_ignore_assignments_and_nested_keys",
        r##"(let* ((root (make-temp-file "projectile-text-tasks-" t))
                    (project (file-name-as-directory root)))
               (unwind-protect
                   (progn
                     (with-temp-file (expand-file-name "justfile" root)
                       (insert
                        "set shell := [\"bash\", \"-c\"]\nversion := \"1.0\"\n\nbuild:\n    cargo build\n\n@fmt:\n    cargo fmt\n\ntest filter=\"\":\n    cargo test {{filter}}\n"))
                     (with-temp-file (expand-file-name "Taskfile.yml" root)
                       (insert
                        "tasks:\n  build:\n    desc: Build it\n    cmds:\n      - go build ./...\n  test:\n    cmds:\n      - go test ./...\n"))
                     (with-temp-file (expand-file-name "Makefile" root)
                       (insert
                        "CC := gcc\n.PHONY: all test\nall: main.o\n\t$(CC) -o demo main.o\ntest:\n\t./run-tests\nmain.o: main.c\n\t$(CC) -c main.c\n%.o: %.c\n\t$(CC) -c $<\n"))
                     (list
                      (projectile-tasks-from-just project)
                      (projectile-tasks-from-taskfile project)
                      (projectile-tasks-from-make project)))
                 (delete-directory root t)))"##,
        expect![[
            r#"OK ((("just:build" . "just build") ("just:fmt" . "just fmt") ("just:test" . "just test")) (("task:build" . "task build") ("task:test" . "task test")) (("make:all" . "make all") ("make:test" . "make test")))"#
        ]],
    )
}

pub(super) fn filesystem_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        projectile_root_strategies_find_expected_marker_levels(),
        projectile_project_root_contract_handles_marked_project_and_nil_directory(),
        projectile_native_directory_indexing_filters_ignored_paths(),
        projectile_task_manifest_parsers_read_controlled_project_files(),
        projectile_text_task_parsers_ignore_assignments_and_nested_keys(),
    ]
}
