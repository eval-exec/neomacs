use expect_test::expect;

use super::ParityBatchCase;

fn evil_exclusive_type_normalizes_boundaries_and_describes_character_counts() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_exclusive_type_normalizes_boundaries_and_describes_character_counts",
        r##"(with-temp-buffer
               (insert "first line\nsecond line\nthird")
               (let ((second-line
                      (save-excursion
                        (goto-char (point-min))
                        (forward-line 1)
                        (point))))
                 (list
                  (evil-normalize 1 1 'exclusive)
                  (evil-normalize 2 second-line 'exclusive)
                  (evil-normalize 1 second-line 'exclusive)
                  (evil-describe 1 1 'exclusive)
                  (evil-describe 1 2 'exclusive)
                  (evil-describe 5 2 'exclusive))))"##,
        expect![[
            r#"OK ((1 1 exclusive) (2 11 inclusive :expanded t) (1 12 line :expanded t) "0 characters" "1 character" "3 characters")"#
        ]],
    )
}

fn evil_inclusive_type_expands_contracts_reversed_positions_and_describes_counts() -> ParityBatchCase
{
    ParityBatchCase::value(
        "evil_inclusive_type_expands_contracts_reversed_positions_and_describes_counts",
        r##"(with-temp-buffer
               (insert "abcdefgh")
               (list
                (evil-expand 1 1 'inclusive)
                (evil-expand 5 2 'inclusive)
                (evil-contract 1 2 'inclusive)
                (evil-contract 6 2 'inclusive)
                (evil-describe 1 1 'inclusive)
                (evil-describe 5 2 'inclusive)))"##,
        expect![[
            r#"OK ((1 2 inclusive :expanded t) (2 6 inclusive :expanded t) (1 1 inclusive :expanded nil) (2 5 inclusive :expanded nil) "1 character" "4 characters")"#
        ]],
    )
}

fn evil_line_and_block_types_expand_contract_and_describe_dimensions() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_line_and_block_types_expand_contract_and_describe_dimensions",
        r##"(with-temp-buffer
               (insert "alpha\nbravo\ncharlie\n")
               (let ((first 1)
                     (second
                      (save-excursion
                        (goto-char 1)
                        (forward-line 1)
                        (point)))
                     (third
                      (save-excursion
                        (goto-char 1)
                        (forward-line 2)
                        (point))))
                 (list
                  (evil-expand first first 'line)
                  (evil-expand first second 'line)
                  (evil-describe first second 'line)
                  (evil-expand first first 'block)
                  (evil-expand first second 'block)
                  (evil-expand first (1+ third) 'block)
                  (evil-contract first (1+ second) 'block)
                  (evil-describe first (1+ third) 'block))))"##,
        expect![[
            r#"OK ((1 7 line :expanded t) (1 13 line :expanded t) "2 lines" (1 2 block :expanded t) (1 8 block :expanded t) (1 15 block :expanded t) (1 7 block :expanded nil) "3 rows and 2 columns")"#
        ]],
    )
}

fn evil_transform_handles_nil_types_markers_and_existing_expansion_flags() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_transform_handles_nil_types_markers_and_existing_expansion_flags",
        r##"(with-temp-buffer
               (insert "abcdef")
               (let ((marker (copy-marker 2)))
                 (unwind-protect
                     (list
                      (evil-transform nil 1 2 'block)
                      (evil-transform :expand 1 2 nil)
                      (evil-transform nil 1 2 nil)
                      (evil-transform :expand marker 2 'inclusive)
                      (evil-expand 1 2 'inclusive :expanded t))
                   (set-marker marker nil))))"##,
        expect![
            "OK ((1 2 block) (1 2) (1 2) (2 3 inclusive :expanded t) (1 2 inclusive :expanded t))"
        ],
    )
}

fn evil_ranges_sort_positions_preserve_properties_and_support_copying_mutators() -> ParityBatchCase
{
    ParityBatchCase::value(
        "evil_ranges_sort_positions_preserve_properties_and_support_copying_mutators",
        r##"(with-temp-buffer
               (let* ((range (evil-range 9 3 'inclusive
                                         :foo 'one :bar 'two))
                      (copy (evil-copy-range range))
                      (changed
                       (evil-set-range copy 2 12 'line
                                       :foo 'changed :baz t)))
                 (list
                  range
                  (evil-range-p range)
                  (evil-range-beginning range)
                  (evil-range-end range)
                  (evil-type range)
                  (evil-range-properties range)
                  changed
                  range)))"##,
        expect![
            "OK (#2=(1 1 inclusive . #1=(:foo one :bar two)) t 1 1 inclusive #1# (1 1 line :foo changed :bar two :baz t) #2#)"
        ],
    )
}

fn evil_range_component_mutators_distinguish_in_place_and_copy_updates() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_range_component_mutators_distinguish_in_place_and_copy_updates",
        r##"(with-temp-buffer
               (let* ((original
                       (evil-range 2 8 'exclusive :name 'original))
                      (beg-copy
                       (evil-set-range-beginning original 1 t))
                      (end-copy
                       (evil-set-range-end original 9 t))
                      (type-copy
                       (evil-set-range-type original 'line t))
                      (props-copy
                       (evil-set-range-properties
                        original '(:name copy :extra t) t)))
                 (list original beg-copy end-copy type-copy props-copy)))"##,
        expect![
            "OK ((1 1 exclusive :name original) (1 1 exclusive :name original) (1 9 exclusive :name original) (1 1 line :name original) (1 1 exclusive :name copy :extra t))"
        ],
    )
}

fn evil_range_union_combines_extent_type_and_property_precedence() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_range_union_combines_extent_type_and_property_precedence",
        r##"(with-temp-buffer
               (let ((left (evil-range 1 5 'inclusive :left t))
                     (right (evil-range 4 10 'line :right t)))
                 (list
                  (evil-range-union left right)
                  (evil-range-union left right 'block)
                  (evil-range-union nil right)
                  (evil-range-union left nil))))"##,
        expect!["OK ((1 1 inclusive) (1 1 block) nil nil)"],
    )
}

pub(super) fn types_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        evil_exclusive_type_normalizes_boundaries_and_describes_character_counts(),
        evil_inclusive_type_expands_contracts_reversed_positions_and_describes_counts(),
        evil_line_and_block_types_expand_contract_and_describe_dimensions(),
        evil_transform_handles_nil_types_markers_and_existing_expansion_flags(),
        evil_ranges_sort_positions_preserve_properties_and_support_copying_mutators(),
        evil_range_component_mutators_distinguish_in_place_and_copy_updates(),
        evil_range_union_combines_extent_type_and_property_precedence(),
    ]
}
