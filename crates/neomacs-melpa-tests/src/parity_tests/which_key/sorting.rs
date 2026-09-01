use expect_test::expect;

use super::ParityBatchCase;

fn which_key_upstream_sort_orders_match_for_uppercase_and_lowercase_priority() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_upstream_sort_orders_match_for_uppercase_and_lowercase_priority",
        r##"(let ((keys '(("a" . "z")
                             ("A" . "Z")
                             ("b" . "y")
                             ("B" . "Y")
                             ("p" . "prefix")
                             ("SPC" . "x")
                             ("C-a" . "w"))))
               (list
                (let ((which-key-sort-uppercase-first t))
                  (mapcar #'car
                          (sort (copy-tree keys) #'which-key-key-order)))
                (let (which-key-sort-uppercase-first)
                  (mapcar #'car
                          (sort (copy-tree keys) #'which-key-key-order)))
                (let ((which-key-sort-uppercase-first t))
                  (mapcar #'car
                          (sort (copy-tree keys)
                                #'which-key-key-order-alpha)))
                (let (which-key-sort-uppercase-first)
                  (mapcar #'car
                          (sort (copy-tree keys)
                                #'which-key-key-order-alpha)))))"##,
        expect![[
            r#"OK (("SPC" "A" "B" "a" "b" "p" "C-a") ("SPC" "a" "b" "p" "A" "B" "C-a") ("SPC" "A" "a" "B" "b" "p" "C-a") ("SPC" "a" "A" "b" "B" "p" "C-a"))"#
        ]],
    )
}

fn which_key_upstream_prefix_sort_orders_match_in_both_directions() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_upstream_prefix_sort_orders_match_in_both_directions",
        r##"(let ((keys '(("a" . "z")
                             ("A" . "Z")
                             ("b" . "y")
                             ("B" . "Y")
                             ("p" . "prefix")
                             ("SPC" . "x")
                             ("C-a" . "w"))))
               (list
                (let ((which-key-sort-uppercase-first t))
                  (mapcar #'car
                          (sort (copy-tree keys)
                                #'which-key-prefix-then-key-order)))
                (let (which-key-sort-uppercase-first)
                  (mapcar #'car
                          (sort (copy-tree keys)
                                #'which-key-prefix-then-key-order)))
                (let ((which-key-sort-uppercase-first t))
                  (mapcar #'car
                          (sort (copy-tree keys)
                                #'which-key-prefix-then-key-order-reverse)))
                (let (which-key-sort-uppercase-first)
                  (mapcar #'car
                          (sort (copy-tree keys)
                                #'which-key-prefix-then-key-order-reverse)))))"##,
        expect![[
            r#"OK (("SPC" "A" "B" "a" "b" "C-a" "p") ("SPC" "a" "b" "A" "B" "C-a" "p") ("p" "SPC" "A" "B" "a" "b" "C-a") ("p" "SPC" "a" "b" "A" "B" "C-a"))"#
        ]],
    )
}

fn which_key_description_order_is_case_insensitive_and_stable_for_ties() -> ParityBatchCase {
    ParityBatchCase::value(
        "which_key_description_order_is_case_insensitive_and_stable_for_ties",
        r##"(let ((keys '(("a" . "z")
                             ("A" . "Z")
                             ("b" . "y")
                             ("B" . "Y")
                             ("p" . "prefix")
                             ("SPC" . "x")
                             ("C-a" . "w"))))
               (list
                (let ((which-key-sort-uppercase-first t))
                  (mapcar #'car
                          (sort (copy-tree keys)
                                #'which-key-description-order)))
                (let (which-key-sort-uppercase-first)
                  (mapcar #'car
                          (sort (copy-tree keys)
                                #'which-key-description-order)))))"##,
        expect![[r#"OK (("p" "C-a" "SPC" "b" "B" "a" "A") ("p" "C-a" "SPC" "b" "B" "a" "A"))"#]],
    )
}

fn which_key_key_order_handles_empty_ranges_specials_function_keys_and_modifiers() -> ParityBatchCase
{
    ParityBatchCase::value(
        "which_key_key_order_handles_empty_ranges_specials_function_keys_and_modifiers",
        r##"(let ((keys '(("" . "empty")
                             ("z .. a" . "range")
                             ("TAB" . "tab")
                             ("RET" . "ret")
                             ("a" . "lower")
                             ("A" . "upper")
                             ("<f12>" . "f12")
                             ("<f2>" . "f2")
                             ("M-b" . "meta-b")
                             ("C-a" . "control-a")
                             ("long" . "other"))))
               (list
                (let ((which-key-sort-uppercase-first t))
                  (mapcar #'car
                          (sort (copy-tree keys) #'which-key-key-order)))
                (let (which-key-sort-uppercase-first)
                  (mapcar #'car
                          (sort (copy-tree keys)
                                #'which-key-key-order-alpha)))))"##,
        expect![[
            r#"OK (("" "RET" "TAB" "A" "a" "z .. a" "<f2>" "<f12>" "C-a" "M-b" "long") ("" "RET" "TAB" "a" "A" "z .. a" "<f2>" "<f12>" "C-a" "M-b" "long"))"#
        ]],
    )
}

pub(super) fn sorting_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        which_key_upstream_sort_orders_match_for_uppercase_and_lowercase_priority(),
        which_key_upstream_prefix_sort_orders_match_in_both_directions(),
        which_key_description_order_is_case_insensitive_and_stable_for_ties(),
        which_key_key_order_handles_empty_ranges_specials_function_keys_and_modifiers(),
    ]
}
