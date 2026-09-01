use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_exuberant_ctags_finds_tags_in_current_project_directory() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_finds_tags_in_current_project_directory",
        r##"(let* ((root
                                 (auto-complete-exuberant-ctags-test-root
                                  "current-project"))
                                (tags
                                 (expand-file-name "tags" root)))
                           (auto-complete-exuberant-ctags-test-write
                            tags
                            "main\tmain.c\tkind:f\tlanguage:C\n")
                           (auto-complete-exuberant-ctags-test-relative
                            (ac-exuberant-ctags-find-tag-file root)
                            root))"##,
        expect![[r#"OK "./""#]],
    )
}

fn auto_complete_exuberant_ctags_walks_multiple_ancestors_to_project_root() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_walks_multiple_ancestors_to_project_root",
        r##"(let* ((root
                                 (auto-complete-exuberant-ctags-test-root
                                  "ancestor-project"))
                                (nested
                                 (expand-file-name
                                  "src/ui/widgets/"
                                  root)))
                           (make-directory nested t)
                           (auto-complete-exuberant-ctags-test-write
                            (expand-file-name "tags" root)
                            "Widget\twidget.rs\tkind:s\tlanguage:Rust\n")
                           (auto-complete-exuberant-ctags-test-relative
                            (ac-exuberant-ctags-find-tag-file nested)
                            root))"##,
        expect![[r#"OK "./""#]],
    )
}

fn auto_complete_exuberant_ctags_search_limit_includes_exact_boundary_only() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_search_limit_includes_exact_boundary_only",
        r##"(let* ((root
                                 (auto-complete-exuberant-ctags-test-root
                                  "search-boundary"))
                                (nested
                                 (expand-file-name "a/b/c/" root)))
                           (make-directory nested t)
                           (auto-complete-exuberant-ctags-test-write
                            (expand-file-name "tags" root)
                            "")
                           (mapcar
                            (lambda (limit)
                              (let ((ac-exuberant-ctags-tag-file-search-limit
                                     limit))
                                (auto-complete-exuberant-ctags-test-relative
                                 (ac-exuberant-ctags-find-tag-file nested)
                                 root)))
                            '(0 1 2 3 4)))"##,
        expect![[r#"OK (nil nil nil "./" "./")"#]],
    )
}

fn auto_complete_exuberant_ctags_custom_filename_selects_only_requested_database() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_custom_filename_selects_only_requested_database",
        r##"(let* ((root
                                 (auto-complete-exuberant-ctags-test-root
                                  "custom-name"))
                                (nested
                                 (expand-file-name "lib/" root))
                                (ac-exuberant-ctags-tag-file-name
                                 ".project-tags"))
                           (make-directory nested t)
                           (auto-complete-exuberant-ctags-test-write
                            (expand-file-name "tags" nested)
                            "wrong")
                           (auto-complete-exuberant-ctags-test-write
                            (expand-file-name ".project-tags" root)
                            "right")
                           (auto-complete-exuberant-ctags-test-relative
                            (ac-exuberant-ctags-find-tag-file nested)
                            root))"##,
        expect![[r#"OK "./""#]],
    )
}

fn auto_complete_exuberant_ctags_get_tag_file_returns_path_and_records_directory() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_get_tag_file_returns_path_and_records_directory",
        r##"(let* ((root
                                 (auto-complete-exuberant-ctags-test-root
                                  "get-tag-file"))
                                (nested
                                 (expand-file-name "src/deep/" root))
                                (default-directory nested)
                                (ac-exuberant-ctags-tag-file-dir
                                 'unset))
                           (make-directory nested t)
                           (auto-complete-exuberant-ctags-test-write
                            (expand-file-name "tags" root)
                            "entry")
                           (list
                            (auto-complete-exuberant-ctags-test-relative
                             (ac-exuberant-ctags-get-tag-file)
                             root)
                            (auto-complete-exuberant-ctags-test-relative
                             ac-exuberant-ctags-tag-file-dir
                             root)))"##,
        expect![[r#"OK ("tags" "./")"#]],
    )
}

fn auto_complete_exuberant_ctags_missing_file_preserves_stale_directory_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_missing_file_preserves_stale_directory_state",
        r##"(let* ((root
                                 (auto-complete-exuberant-ctags-test-root
                                  "missing-tag-file"))
                                (empty
                                 (expand-file-name "empty/" root))
                                (default-directory empty)
                                (ac-exuberant-ctags-tag-file-search-limit
                                 0)
                                (ac-exuberant-ctags-tag-file-dir
                                 "stale-project/"))
                           (make-directory empty t)
                           (list
                            (ac-exuberant-ctags-get-tag-file)
                            ac-exuberant-ctags-tag-file-dir))"##,
        expect![[r#"OK (nil "stale-project/")"#]],
    )
}

fn auto_complete_exuberant_ctags_ignores_unrelated_sibling_tag_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_exuberant_ctags_ignores_unrelated_sibling_tag_files",
        r##"(let* ((root
                                 (auto-complete-exuberant-ctags-test-root
                                  "sibling-projects"))
                                (left
                                 (expand-file-name "left/src/" root))
                                (right
                                 (expand-file-name "right/src/" root))
                                (ac-exuberant-ctags-tag-file-search-limit
                                 1))
                           (make-directory left t)
                           (make-directory right t)
                           (auto-complete-exuberant-ctags-test-write
                            (expand-file-name "../tags" left)
                            "left")
                           (list
                            (auto-complete-exuberant-ctags-test-relative
                             (ac-exuberant-ctags-find-tag-file left)
                             root)
                            (ac-exuberant-ctags-find-tag-file right)))"##,
        expect![[r#"OK ("left/" nil)"#]],
    )
}

pub(super) fn discovery_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_exuberant_ctags_finds_tags_in_current_project_directory(),
        auto_complete_exuberant_ctags_walks_multiple_ancestors_to_project_root(),
        auto_complete_exuberant_ctags_search_limit_includes_exact_boundary_only(),
        auto_complete_exuberant_ctags_custom_filename_selects_only_requested_database(),
        auto_complete_exuberant_ctags_get_tag_file_returns_path_and_records_directory(),
        auto_complete_exuberant_ctags_missing_file_preserves_stale_directory_state(),
        auto_complete_exuberant_ctags_ignores_unrelated_sibling_tag_files(),
    ]
}
