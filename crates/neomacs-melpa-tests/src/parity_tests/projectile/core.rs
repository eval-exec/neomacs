use expect_test::expect;

use super::ParityBatchCase;

fn projectile_version_and_platform_helpers_are_stable() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_version_and_platform_helpers_are_stable",
        r##"(list
              (projectile-version)
              (projectile-unixy-system-p)
              (projectile-parent "/alpha/beta/gamma/")
              (projectile-default-project-name "/alpha/beta/")
              (projectile-uniquify-dirname-transform "/alpha/beta/"))"##,
        expect![[r#"OK ("3.4.0-snapshot" t "/alpha/beta" "beta" "/alpha/beta/")"#]],
    )
}

fn projectile_path_pattern_normalization_partitions_rooted_entries() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_path_pattern_normalization_partitions_rooted_entries",
        r##"(list
              (projectile-normalise-paths
               '("plain" "/rooted" "/nested/path" "" "/"))
              (projectile-normalise-patterns
               '("plain" "/rooted" "/nested/path" "" "/"))
              (projectile--directory-ancestors "src/foo/bar.el")
              (projectile--directory-ancestors "top.el")
              (projectile--wildcard-p "*.el")
              (projectile--wildcard-p "plain.el"))"##,
        expect![[r#"OK (("rooted" "nested/path" "") ("plain" "") ("src/" "src/foo/") nil 0 nil)"#]],
    )
}

fn projectile_glob_and_ignore_pattern_translation_handles_gitignore_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_glob_and_ignore_pattern_translation_handles_gitignore_shapes",
        r##"(let ((samples
                    '("foo.el" "src/foo.el" "build/" "build/x.o"
                      "src/generated/x.c" "src/keep/x.c")))
               (mapcar
                (lambda (pattern)
                  (let ((regexp
                         (projectile--ignore-pattern-to-regexp pattern)))
                    (cons pattern
                          (mapcar
                           (lambda (sample)
                             (and (string-match-p regexp sample) t))
                           samples))))
                '("*.el" "build/" "/build/" "src/generated/**"
                  "src/?eep/*.c")))"##,
        expect![[
            r#"OK (("*.el" t t nil nil nil nil) ("build/" nil nil t t nil nil) ("/build/" nil nil t t nil nil) ("src/generated/**" nil nil nil nil t nil) ("src/?eep/*.c" nil nil nil nil nil t))"#
        ]],
    )
}

fn projectile_dirconfig_parser_preserves_keep_ignore_ensure_and_legacy_entries() -> ParityBatchCase
{
    ParityBatchCase::value(
        "projectile_dirconfig_parser_preserves_keep_ignore_ensure_and_legacy_entries",
        r##"(let* ((projectile-dirconfig-comment-prefix ?#)
                    (config
                     (projectile--parse-dirconfig-string
                      " + src\n+tests/\n-build/\n!build/keep.txt\n# comment\nlegacy\n  \n")))
               (list
                (projectile-dirconfig-keep config)
                (projectile-dirconfig-ignore config)
                (projectile-dirconfig-ensure config)
                (projectile-dirconfig-prefixless-ignore config)
                (projectile--dirconfig-classify-line " \t+ lib ")
                (projectile--dirconfig-classify-line "  # note")
                (projectile--dirconfig-classify-line "")))"##,
        expect![[
            r#"OK (("src/" "tests/") ("build/" "legacy") ("build/keep.txt") ("legacy") (:keep . "lib") (:comment) nil)"#
        ]],
    )
}

fn projectile_project_type_registration_and_updates_preserve_attributes() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_project_type_registration_and_updates_preserve_attributes",
        r##"(let ((projectile-project-types nil)
                    (projectile-project-root-files nil)
                    (projectile-project-root-files-bottom-up '(".git")))
               (projectile-register-project-type
                'demo
                '((:any "demo.toml" "demo.json") "src")
                :test-suffix "_spec"
                :src-dir "src/"
                :test-dir "test/")
               (projectile-update-project-type
                'demo
                :test-prefix "test_"
                :compile "make all")
               (list
                (projectile-project-type-attribute 'demo 'project-file)
                (projectile-project-type-attribute 'demo 'test-suffix)
                (projectile-project-type-attribute 'demo 'test-prefix)
                (projectile-project-type-attribute 'demo 'compile-command)
                projectile-project-root-files
                projectile-project-root-files-bottom-up))"##,
        expect![[
            r#"OK (("demo.toml" "demo.json") "_spec" "test_" "make all" ("demo.json" "demo.toml") (".git"))"#
        ]],
    )
}

fn projectile_combine_plists_uses_rightmost_values_including_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_combine_plists_uses_rightmost_values_including_nil",
        r##"(list
              (projectile--combine-plists
               '(:foo "first" :bar "bar")
               '(:foo "second" :baz "baz")
               '(:bar nil))
              (projectile--combine-plists nil '(:x 1))
              (projectile--combine-plists '(:x 1) nil))"##,
        expect![[r#"OK ((:foo "second" :bar nil :baz "baz") (:x 1) (:x 1))"#]],
    )
}

pub(super) fn core_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        projectile_version_and_platform_helpers_are_stable(),
        projectile_path_pattern_normalization_partitions_rooted_entries(),
        projectile_glob_and_ignore_pattern_translation_handles_gitignore_shapes(),
        projectile_dirconfig_parser_preserves_keep_ignore_ensure_and_legacy_entries(),
        projectile_project_type_registration_and_updates_preserve_attributes(),
        projectile_combine_plists_uses_rightmost_values_including_nil(),
    ]
}
