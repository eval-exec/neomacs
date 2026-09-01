use expect_test::expect;

use super::ParityBatchCase;

fn transient_define_prefix_builds_exact_layout_and_reuses_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "transient_define_prefix_builds_exact_layout_and_reuses_it",
        r##"(progn
               (transient-define-suffix neomacs-test-a ()
                 :key "a" (interactive))
               (transient-define-suffix neomacs-test-b ()
                 :key "b" (interactive))
               (transient-define-suffix neomacs-test-c ()
                 :key "c" (interactive))
               (transient-define-suffix neomacs-test-m ()
                 (interactive))
               (transient-define-prefix neomacs-test-menu ()
                 [(neomacs-test-a)
                  (neomacs-test-b :key "b")
                  (neomacs-test-c :key "C")
                  (neomacs-test-m :key "m")])
               (let ((layout (transient--get-layout
                              'neomacs-test-menu)))
                 (list layout
                       (eq layout
                           (transient--get-layout
                            'neomacs-test-menu)))))"##,
        expect![[
            r#"OK ([2 nil ([transient-column nil ((transient-suffix :command neomacs-test-a) (transient-suffix :command neomacs-test-b :key "b") (transient-suffix :command neomacs-test-c :key "C") (transient-suffix :command neomacs-test-m :key "m"))])] t)"#
        ]],
    )
}

fn transient_groups_preserve_rows_columns_and_included_layouts() -> ParityBatchCase {
    ParityBatchCase::value(
        "transient_groups_preserve_rows_columns_and_included_layouts",
        r##"(progn
               (dolist (entry '((neomacs-test-a "a")
                                (neomacs-test-b "b")
                                (neomacs-test-c "c")
                                (neomacs-test-d "d")
                                (neomacs-test-e "e")
                                (neomacs-test-f "f")))
                 (eval `(transient-define-suffix ,(car entry) ()
                          :key ,(cadr entry) (interactive))))
               (transient-define-group neomacs-test-row
                 [:class transient-row
                  (neomacs-test-a)
                  (neomacs-test-b)])
               (transient-define-group neomacs-test-columns
                 [[(neomacs-test-c)]
                  [(neomacs-test-d)]])
               (transient-define-group neomacs-test-list
                 (neomacs-test-e)
                 (neomacs-test-f))
               (transient-define-prefix neomacs-test-menu ()
                 'neomacs-test-row
                 neomacs-test-columns
                 [neomacs-test-list])
               (list
                (transient--get-layout 'neomacs-test-row)
                (transient--get-layout 'neomacs-test-columns)
                (transient--get-layout 'neomacs-test-list)
                (transient--get-layout 'neomacs-test-menu)))"##,
        expect![[
            r#"OK ([2 nil ([transient-row nil ((transient-suffix :command neomacs-test-a) (transient-suffix :command neomacs-test-b))])] [2 nil ([transient-columns nil ([transient-column nil ((transient-suffix :command neomacs-test-c))] [transient-column nil ((transient-suffix :command neomacs-test-d))])])] [2 nil ((transient-suffix :command neomacs-test-e) (transient-suffix :command neomacs-test-f))] [2 nil (neomacs-test-row neomacs-test-columns [transient-columns nil (neomacs-test-list)])])"#
        ]],
    )
}

fn transient_get_suffix_supports_positive_negative_key_and_nested_coordinates() -> ParityBatchCase {
    ParityBatchCase::value(
        "transient_get_suffix_supports_positive_negative_key_and_nested_coordinates",
        r##"(progn
               (dolist (entry '((neomacs-test-a "a")
                                (neomacs-test-b "b")
                                (neomacs-test-c "c")
                                (neomacs-test-d "d")))
                 (eval `(transient-define-suffix ,(car entry) ()
                          :key ,(cadr entry) (interactive))))
               (transient-define-prefix neomacs-test-menu ()
                 [(neomacs-test-a)
                  (neomacs-test-b)
                  (neomacs-test-c)]
                 [[(neomacs-test-d :description "nested")]])
               (list
                (transient-get-suffix 'neomacs-test-menu [0 0])
                (transient-get-suffix 'neomacs-test-menu [0 -1])
                (transient-get-suffix 'neomacs-test-menu "b")
                (transient-get-suffix
                 'neomacs-test-menu [1 0 "d"])
                (copy-tree
                 (transient-get-suffix
                  'neomacs-test-menu [-1 -1 -1]))))"##,
        expect![[
            r#"OK ((transient-suffix :command neomacs-test-a) (transient-suffix :command neomacs-test-c) (transient-suffix :command neomacs-test-b) (transient-suffix :command neomacs-test-d :description "nested") (transient-suffix :command neomacs-test-d :description "nested"))"#
        ]],
    )
}

fn transient_suffix_put_mutates_keys_in_place() -> ParityBatchCase {
    ParityBatchCase::value(
        "transient_suffix_put_mutates_keys_in_place",
        r##"(progn
               (dolist (entry '((neomacs-test-a "a")
                                (neomacs-test-b "b")
                                (neomacs-test-c "c")
                                (neomacs-test-d "d")))
                 (eval `(transient-define-suffix ,(car entry) ()
                          :key ,(cadr entry) (interactive))))
               (transient-define-prefix neomacs-test-menu ()
                 [(neomacs-test-a)
                  (neomacs-test-b)
                  (neomacs-test-c)
                  (neomacs-test-d)])
               (let ((layout
                      (transient--get-layout 'neomacs-test-menu)))
                 (transient-suffix-put
                  'neomacs-test-menu "a" :key "A")
                 (transient-suffix-put
                  'neomacs-test-menu 'neomacs-test-b :key "B")
                 (transient-suffix-put
                  'neomacs-test-menu [0 -2] :key "C")
                 (transient-suffix-put
                  'neomacs-test-menu "d" :key "D")
                 (list
                  (eq layout
                      (transient--get-layout 'neomacs-test-menu))
                  (transient--get-layout 'neomacs-test-menu))))"##,
        expect![[
            r#"OK (t [2 nil ([transient-column nil ((transient-suffix :command neomacs-test-a :key "A") (transient-suffix :command neomacs-test-b :key "B") (transient-suffix :command neomacs-test-c :key "C") (transient-suffix :command neomacs-test-d :key "D"))])])"#
        ]],
    )
}

fn transient_insert_append_and_replace_apply_at_exact_locations() -> ParityBatchCase {
    ParityBatchCase::value(
        "transient_insert_append_and_replace_apply_at_exact_locations",
        r##"(progn
               (dolist (entry '((neomacs-test-a "a")
                                (neomacs-test-b "b")
                                (neomacs-test-c "c")
                                (neomacs-test-x "x")
                                (neomacs-test-y "y")
                                (neomacs-test-z "z")))
                 (eval `(transient-define-suffix ,(car entry) ()
                          :key ,(cadr entry) (interactive))))
               (transient-define-prefix neomacs-test-menu ()
                 [(neomacs-test-a)
                  (neomacs-test-b)
                  (neomacs-test-c)])
               (transient-insert-suffix
                'neomacs-test-menu "a" '(neomacs-test-z))
               (transient-append-suffix
                'neomacs-test-menu "a" '(neomacs-test-y))
               (transient-replace-suffix
                'neomacs-test-menu "b"
                '(neomacs-test-x :description "replacement"))
               (transient-append-suffix
                'neomacs-test-menu "c" '(neomacs-test-b))
               (transient--get-layout 'neomacs-test-menu))"##,
        expect![[
            r#"OK [2 nil ([transient-column nil ((transient-suffix :command neomacs-test-z) (transient-suffix :command neomacs-test-a) (transient-suffix :command neomacs-test-y) (transient-suffix :command neomacs-test-x :description "replacement") (transient-suffix :command neomacs-test-c) (transient-suffix :command neomacs-test-b))])]"#
        ]],
    )
}

fn transient_remove_suffix_accepts_keys_commands_and_coordinates() -> ParityBatchCase {
    ParityBatchCase::value(
        "transient_remove_suffix_accepts_keys_commands_and_coordinates",
        r##"(progn
               (dolist (entry '((neomacs-test-a "a")
                                (neomacs-test-b "b")
                                (neomacs-test-c "c")
                                (neomacs-test-d "d")
                                (neomacs-test-e "e")
                                (neomacs-test-f "f")
                                (neomacs-test-g "g")))
                 (eval `(transient-define-suffix ,(car entry) ()
                          :key ,(cadr entry) (interactive))))
               (transient-define-prefix neomacs-test-menu ()
                 [(neomacs-test-a)
                  (neomacs-test-b)
                  (neomacs-test-c)
                  (neomacs-test-d)
                  (neomacs-test-e)
                  (neomacs-test-f)
                  (neomacs-test-g)])
               (transient-remove-suffix 'neomacs-test-menu "a")
               (transient-remove-suffix
                'neomacs-test-menu 'neomacs-test-b)
               (transient-remove-suffix 'neomacs-test-menu "c")
               (transient-remove-suffix 'neomacs-test-menu [0 0])
               (transient-remove-suffix 'neomacs-test-menu [0 -1])
               (transient--get-layout 'neomacs-test-menu))"##,
        expect![[
            r#"OK [2 nil ([transient-column nil ((transient-suffix :command neomacs-test-e) (transient-suffix :command neomacs-test-f))])]"#
        ]],
    )
}

fn transient_inline_group_expands_nested_includes_without_reordering() -> ParityBatchCase {
    ParityBatchCase::value(
        "transient_inline_group_expands_nested_includes_without_reordering",
        r##"(progn
               (dolist (entry '((neomacs-test-a "a")
                                (neomacs-test-b "b")
                                (neomacs-test-c "c")
                                (neomacs-test-d "d")))
                 (eval `(transient-define-suffix ,(car entry) ()
                          :key ,(cadr entry) (interactive))))
               (transient-define-group neomacs-test-group-a
                 [(neomacs-test-a)])
               (transient-define-group neomacs-test-group-b
                 [(neomacs-test-b)])
               (transient-define-group neomacs-test-group-d
                 [(neomacs-test-d)])
               (transient-define-prefix neomacs-test-menu ()
                 'neomacs-test-group-a
                 [neomacs-test-group-b
                  [(neomacs-test-c)]
                  neomacs-test-group-d])
               (transient-inline-group
                'neomacs-test-menu 'neomacs-test-group-a)
               (transient-inline-group
                'neomacs-test-menu 'neomacs-test-group-b)
               (transient-inline-group
                'neomacs-test-menu 'neomacs-test-group-d)
               (transient--get-layout 'neomacs-test-menu))"##,
        expect![[
            r#"OK [2 nil ([transient-column nil ((transient-suffix :command neomacs-test-a))] [transient-columns nil ([transient-column nil ((transient-suffix :command neomacs-test-b))] [transient-column nil ((transient-suffix :command neomacs-test-c))] [transient-column nil ((transient-suffix :command neomacs-test-d))])])]"#
        ]],
    )
}

pub(super) fn layout_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        transient_define_prefix_builds_exact_layout_and_reuses_it(),
        transient_groups_preserve_rows_columns_and_included_layouts(),
        transient_get_suffix_supports_positive_negative_key_and_nested_coordinates(),
        transient_suffix_put_mutates_keys_in_place(),
        transient_insert_append_and_replace_apply_at_exact_locations(),
        transient_remove_suffix_accepts_keys_commands_and_coordinates(),
        transient_inline_group_expands_nested_includes_without_reordering(),
    ]
}
