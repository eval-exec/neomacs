use expect_test::expect;

use super::ParityBatchCase;

fn projectile_nested_extensions_and_associations_match_upstream_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_nested_extensions_and_associations_match_upstream_contract",
        r##"(let ((projectile-other-file-alist
                    '(("cpp" "h" "hpp" "ipp")
                      ("gz" "zip")
                      ("js" "js"))))
               (list
                (projectile--file-name-extensions "foo.el")
                (projectile--file-name-extensions "dir/bar.tar.gz")
                (projectile--file-name-extensions ".emacs")
                (projectile--file-name-extensions "Makefile")
                (projectile--file-name-sans-extensions "foo.el")
                (projectile--file-name-sans-extensions
                 "dir/bar.tar.gz")
                (projectile--file-name-sans-extensions ".emacs")
                (projectile-associated-file-name-extensions "foo.cpp")
                (projectile-associated-file-name-extensions
                 "archive.tar.gz")
                (projectile-associated-file-name-extensions
                 "foo.unknown")))"##,
        expect![[r#"OK ("el" "tar.gz" "" "" "foo" "bar" ".emacs" ("h" "hpp" "ipp") ("zip") nil)"#]],
    )
}

fn projectile_related_file_function_merging_flattens_without_mutation() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_related_file_function_merging_flattens_without_mutation",
        r##"(let* ((shared '("first-test.el"))
                    (first
                     (lambda (_path)
                       (list :test shared :doc "README")))
                    (second
                     (lambda (_path)
                       '(:test ("second-test.el")
                         :impl ("impl.el"))))
                    (merged
                     (projectile--merge-related-files-fns
                      (list first second)))
                    (result (funcall merged "src.el")))
               (list result shared))"##,
        expect![[
            r#"OK ((:test ("first-test.el" "second-test.el") :doc ("README") :impl ("impl.el")) ("first-test.el"))"#
        ]],
    )
}

fn projectile_related_file_generators_transform_groups_extensions_and_tests() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_related_file_generators_transform_groups_extensions_and_tests",
        r##"(let* ((groups
                    (projectile-related-files-fn-groups
                     :peer
                     '(("a.el" "b.el" "c.el")
                       ("x.el" "y.el"))))
                   (extensions
                    (projectile-related-files-fn-extensions
                     :peer '("c" "h")))
                   (prefix
                    (projectile-related-files-fn-test-with-prefix
                     "el" "test-"))
                   (suffix
                    (projectile-related-files-fn-test-with-suffix
                     "rb" "_spec"))
                   (extension-result (funcall extensions "src/demo.c"))
                   (extension-predicate
                    (plist-get extension-result :peer))
                   (prefix-test-result (funcall prefix "src/demo.el"))
                   (prefix-test-predicate
                    (plist-get prefix-test-result :test))
                   (prefix-impl-result
                    (funcall prefix "src/test-demo.el"))
                   (prefix-impl-predicate
                    (plist-get prefix-impl-result :impl))
                   (suffix-test-result (funcall suffix "app/user.rb"))
                   (suffix-test-predicate
                    (plist-get suffix-test-result :test))
                   (suffix-impl-result
                    (funcall suffix "app/user_spec.rb"))
                   (suffix-impl-predicate
                    (plist-get suffix-impl-result :impl)))
               (list
                (funcall groups "b.el")
                (funcall groups "missing.el")
                (mapcar extension-predicate
                        '("demo.h" "src/demo.h" "demo.cpp" "other.h"))
                (mapcar prefix-test-predicate
                        '("test-demo.el"
                          "test/test-demo.el"
                          "mytest-demo.el"))
                (mapcar prefix-impl-predicate
                        '("demo.el" "src/demo.el" "test-demo.el"))
                (mapcar suffix-test-predicate
                        '("user_spec.rb"
                          "spec/user_spec.rb"
                          "other_spec.rb"))
                (mapcar suffix-impl-predicate
                        '("user.rb" "app/user.rb" "user_spec.rb"))))"##,
        expect![[
            r#"OK ((:peer ("a.el" "c.el")) nil (t t nil nil) (t t nil) (t t nil) (t t nil) (t t nil))"#
        ]],
    )
}

fn projectile_candidate_grouping_prefers_shared_parent_segments() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_candidate_grouping_prefers_shared_parent_segments",
        r##"(list
              (projectile-dirname-matching-count
               "src/food/sea.c" "src/food/cat.c")
              (projectile-dirname-matching-count
               "src/weed/sea.c" "src/food/sea.c")
              (projectile-group-file-candidates
               "src/foo/test.el"
               '("src/foo/impl.el" "other/x.el" "src/bar.el"))
              (projectile-group-file-candidates
               "src/foo/test.el" nil))"##,
        expect![[r#"OK (2 0 ((2 "src/foo/impl.el") (0 "other/x.el" "src/bar.el")) nil)"#]],
    )
}

fn projectile_name_inflection_and_test_name_transforms_cover_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "projectile_name_inflection_and_test_name_transforms_cover_boundaries",
        r##"(let ((projectile-project-types
                    '((demo marker-files ("Demo")
                            test-prefix "test_"
                            test-suffix "_spec"))))
               (cl-letf (((symbol-function 'projectile-project-type)
                          (lambda (&optional _) 'demo)))
                 (list
                  (projectile--singularize "categories")
                  (projectile--singularize "buses")
                  (projectile--pluralize "category")
                  (projectile--pluralize "box")
                  (projectile--test-name-for-impl-name "user.rb")
                  (projectile--impl-name-for-test-name
                   "test_user_spec.rb")
                  (file-relative-name
                   (projectile-complementary-dir
                    "src/domain/user/" "src/" "test/")
                   default-directory))))"##,
        expect![[
            r#"OK ("category" "bus" "categories" "boxes" "test_user.rb" "user_spec.rb" "test/domain/user/")"#
        ]],
    )
}

pub(super) fn relations_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        projectile_nested_extensions_and_associations_match_upstream_contract(),
        projectile_related_file_function_merging_flattens_without_mutation(),
        projectile_related_file_generators_transform_groups_extensions_and_tests(),
        projectile_candidate_grouping_prefers_shared_parent_segments(),
        projectile_name_inflection_and_test_name_transforms_cover_boundaries(),
    ]
}
