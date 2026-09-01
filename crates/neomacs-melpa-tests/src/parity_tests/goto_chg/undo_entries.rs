use expect_test::expect;

use super::ParityBatchCase;

fn goto_chg_public_defaults_match_the_pinned_release() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_public_defaults_match_the_pinned_release",
        r##"(list
               glc-default-span
               glc-current-span
               glc-probe-depth
               glc-direction
               (commandp 'goto-last-change)
               (commandp 'goto-last-change-reverse))"##,
        expect!["OK (8 8 0 1 t t)"],
    )
    .fresh_process()
}

fn goto_chg_center_ellipsis_covers_short_exact_even_odd_and_custom_markers() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_center_ellipsis_covers_short_exact_even_odd_and_custom_markers",
        r##"(list
               (glc-center-ellipsis "short" 8)
               (glc-center-ellipsis "exactly" 7)
               (glc-center-ellipsis "abcdefghij" 8)
               (glc-center-ellipsis "abcdefghijk" 8)
               (glc-center-ellipsis "abcdefghij" 7 "..")
               (glc-center-ellipsis "abcdefghij" 6 "…"))"##,
        expect![[r#"OK ("short" "exactly" "ab...ij" "ab...jk" "ab..ij" "ab…ij")"#]],
    )
}

fn goto_chg_fixup_edit_extracts_emacs_combined_undo_entries_only() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_fixup_edit_extracts_emacs_combined_undo_entries_only",
        r##"(let ((combined
                    '(apply 1 2 3
                      undo--wrap-and-run-primitive-undo
                      ((4 . 9))))
                    (function-apply
                     (list 'apply #'ignore 2 3
                           'undo--wrap-and-run-primitive-undo
                           '((4 . 9))))
                    (wrong-wrapper
                     '(apply 1 2 3 other-wrapper ((4 . 9))))
                    (multiple
                     '(apply 1 2 3
                       undo--wrap-and-run-primitive-undo
                       ((4 . 9) (10 . 12)))))
               (list
                (glc-fixup-edit combined)
                (glc-fixup-edit function-apply)
                (glc-fixup-edit wrong-wrapper)
                (glc-fixup-edit multiple)
                (glc-fixup-edit '(2 . 7))
                (glc-fixup-edit nil)))"##,
        expect![
            "OK ((4 . 9) (apply ignore 2 3 undo--wrap-and-run-primitive-undo ((4 . 9))) (apply 1 2 3 other-wrapper ((4 . 9))) (apply 1 2 3 undo--wrap-and-run-primitive-undo ((4 . 9) (10 . 12))) (2 . 7) nil)"
        ],
    )
}

fn goto_chg_get_pos_classifies_every_supported_undo_entry_shape() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_get_pos_classifies_every_supported_undo_entry_shape",
        r##"(let ((marker (make-marker)))
               (list
                (glc-get-pos 12)
                (glc-get-pos nil)
                (glc-get-pos '(3 . 8))
                (glc-get-pos '("gone" . 15))
                (glc-get-pos '("gone" . -15))
                (glc-get-pos '(nil face bold 4 . 11))
                (glc-get-pos '(t 1 2 3))
                (glc-get-pos (cons marker 2))
                (glc-get-pos
                 '(apply 1 2 3
                   undo--wrap-and-run-primitive-undo
                   ((6 . 10))))))"##,
        expect!["OK (12 nil 8 15 15 11 nil nil 10)"],
    )
}

fn goto_chg_descriptions_cover_position_insert_delete_property_and_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_descriptions_cover_position_insert_delete_property_and_metadata",
        r##"(with-temp-buffer
               (insert
                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789")
               (list
                (glc-get-descript 4 1)
                (glc-get-descript nil 1)
                (glc-get-descript '(2 . 7) 1)
                (glc-get-descript '(2 . 7) 2)
                (glc-get-descript '("removed" . 9) 3)
                (glc-get-descript
                 (cons
                  (make-string 70 ?x)
                  9)
                 4)
                (glc-get-descript '(nil face bold 2 . 8) 1)
                (glc-get-descript '(nil face bold 2 . 8) 5)
                (glc-get-descript '(t 1 2 3) 6)))"##,
        expect![[
            r#"OK ("New position" nil "T-1: Inserted 5 chars \"bcdef\"" "T-2: Inserted 5 chars" "T-3: Deleted \"removed\"" "T-4: Deleted \"xxxxxxxxxxxxxxxxxxxxxxxxxxxx...xxxxxxxxxxxxxxxxxxxxxxxxxxxx\"" "T-1: Property change" "T-5: Property change" nil)"#
        ]],
    )
}

fn goto_chg_description_rejects_an_omitted_numeric_depth() -> ParityBatchCase {
    ParityBatchCase::signal(
        "goto_chg_description_rejects_an_omitted_numeric_depth",
        r##"(glc-get-descript '(2 . 7))"##,
        expect![[r#"ERR (error "Format specifier doesn’t match argument type")"#]],
    )
}

fn goto_chg_positionable_and_filetime_predicates_cover_all_entry_classes() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_positionable_and_filetime_predicates_cover_all_entry_classes",
        r##"(mapcar
               (lambda (entry)
                 (list
                  (glc-is-positionable entry)
                  (glc-is-filetime entry)))
               '(12
                 nil
                 (3 . 8)
                 ("gone" . 15)
                 (nil face bold 4 . 11)
                 (t 1 2 3)
                 (marker . 2)))"##,
        expect!["OK ((nil nil) (nil nil) (8 nil) (15 nil) (11 nil) (nil t) (nil nil))"],
    )
}

fn goto_chg_adjust_pos2_obeys_span_boundaries_and_edit_offsets() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_adjust_pos2_obeys_span_boundaries_and_edit_offsets",
        r##"(list
               (let ((glc-current-span 2))
                 (mapcar
                  (lambda (pos)
                    (glc-adjust-pos2 pos 10 15 4))
                  '(1 8 9 10 15 17 18 30)))
               (let ((glc-current-span 0))
                 (mapcar
                  (lambda (pos)
                    (glc-adjust-pos2 pos 10 15 -3))
                  '(9 10 12 15 16)))
               (let ((glc-current-span 5))
                 (list
                  (glc-adjust-pos2 2 10 10 7)
                  (glc-adjust-pos2 30 10 10 -4))))"##,
        expect!["OK ((1 8 nil nil nil nil 22 34) (9 10 10 10 13) (2 26))"],
    )
}

fn goto_chg_adjust_pos_handles_insert_delete_property_marker_and_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_adjust_pos_handles_insert_delete_property_marker_and_boundaries",
        r##"(let ((glc-current-span 0)
                    (marker (make-marker)))
               (list
                (glc-adjust-pos 20 nil)
                (glc-adjust-pos 20 7)
                (glc-adjust-pos 20 '(5 . 9))
                (glc-adjust-pos 20 '("abc" . 5))
                (glc-adjust-pos 20 '("abc" . -5))
                (glc-adjust-pos 20 '(nil face bold 5 . 8))
                (glc-adjust-pos 20 (cons marker 3))
                (glc-adjust-pos 7 '(5 . 9))
                (glc-adjust-pos 6 '("abc" . 5))))"##,
        expect!["OK (20 20 24 17 17 20 20 11 5)"],
    )
}

fn goto_chg_adjust_list_tracks_an_old_edit_through_newer_coordinate_changes() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_chg_adjust_list_tracks_an_old_edit_through_newer_coordinate_changes",
        r##"(list
               (let ((glc-current-span 0))
                 (glc-adjust-list
                  '((10 . 12)
                    (2 . 5)
                    ("xx" . 7)
                    (nil face bold 1 . 3))))
               (let ((glc-current-span 2))
                 (glc-adjust-list
                  '((10 . 12)
                    (8 . 13))))
               (let ((glc-current-span 0))
                 (glc-adjust-list nil))
               (let ((glc-current-span 0))
                 (glc-adjust-list
                  '((apply 1 2 3
                     undo--wrap-and-run-primitive-undo
                     ((4 . 9)))
                    (1 . 3)))))"##,
        expect!["OK (13 17 nil 11)"],
    )
}

pub(super) fn undo_entries_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        goto_chg_public_defaults_match_the_pinned_release(),
        goto_chg_center_ellipsis_covers_short_exact_even_odd_and_custom_markers(),
        goto_chg_fixup_edit_extracts_emacs_combined_undo_entries_only(),
        goto_chg_get_pos_classifies_every_supported_undo_entry_shape(),
        goto_chg_descriptions_cover_position_insert_delete_property_and_metadata(),
        goto_chg_description_rejects_an_omitted_numeric_depth(),
        goto_chg_positionable_and_filetime_predicates_cover_all_entry_classes(),
        goto_chg_adjust_pos2_obeys_span_boundaries_and_edit_offsets(),
        goto_chg_adjust_pos_handles_insert_delete_property_marker_and_boundaries(),
        goto_chg_adjust_list_tracks_an_old_edit_through_newer_coordinate_changes(),
    ]
}
