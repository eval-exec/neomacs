use expect_test::expect;

use super::ParityBatchCase;

fn compat_ensure_list_and_proper_list_cover_atoms_dotted_and_circular_inputs() -> ParityBatchCase {
    ParityBatchCase::value(
        "compat_ensure_list_and_proper_list_cover_atoms_dotted_and_circular_inputs",
        r##"(let ((circle (list 1 2 3)))
               (setcdr (last circle) circle)
               (list
                (ensure-list nil)
                (ensure-list 1)
                (ensure-list '(1 . 2))
                (ensure-proper-list nil)
                (ensure-proper-list 1)
                (ensure-proper-list '(1 . 2))
                (mapcar #'proper-list-p
                        (list nil
                              '(1)
                              '(1 2 3)
                              '(1 . 2)
                              '(1 2 . 3)
                              circle
                              1
                              "abc"
                              [1 2 3]))))"##,
        expect![[r#"OK (nil (1) (1 . 2) nil (1) ((1 . 2)) (0 1 3 nil nil nil nil nil nil))"#]],
    )
}

fn compat_take_drop_and_ntake_preserve_exact_copy_and_mutation_semantics() -> ParityBatchCase {
    ParityBatchCase::value(
        "compat_take_drop_and_ntake_preserve_exact_copy_and_mutation_semantics",
        r##"(let* ((source (list 1 2 3 4))
                    (taken (take 2 source))
                    (dropped (drop 2 source))
                    (mutable (list 'a 'b 'c 'd))
                    (tail (cddr mutable))
                    (ntaken (ntake 2 mutable)))
               (list
                (copy-tree taken)
                (copy-tree dropped)
                (copy-tree source)
                (eq taken source)
                (eq dropped (cddr source))
                (copy-tree ntaken)
                (copy-tree mutable)
                (copy-tree tail)
                (eq ntaken mutable)))"##,
        expect![[r#"OK ((1 2) (3 4) (1 2 3 4) nil t (a b) (a b) (c d) t)"#]],
    )
}

fn compat_sequence_predicates_cover_closures_function_values_and_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "compat_sequence_predicates_cover_closures_function_values_and_boundaries",
        r##"(let ((numbers '(3 2 1 0 -1 -2 -3))
                    (threshold 1))
               (list
                (copy-tree
                 (drop-while #'plusp numbers))
                (copy-tree
                 (drop-while
                  (lambda (number)
                    (> number threshold))
                  numbers))
                (copy-tree
                 (take-while #'plusp numbers))
                (copy-tree
                 (take-while
                  (lambda (number)
                    (> number threshold))
                  numbers))
                (all #'numberp numbers)
                (all #'plusp numbers)
                (copy-tree
                 (member-if #'zerop numbers))
                (copy-tree
                 (funcall
                  (identity #'member-if)
                  #'minusp numbers))))"##,
        expect![[
            r#"OK ((0 -1 -2 -3) (1 0 -1 -2 -3) (3 2 1) (3 2) t nil (0 -1 -2 -3) (-1 -2 -3))"#
        ]],
    )
}

fn compat_length_comparators_cover_lists_vectors_and_equal_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "compat_length_comparators_cover_lists_vectors_and_equal_boundaries",
        r##"(list
               (mapcar
                (lambda (limit)
                  (length< '(a b c) limit))
                '(0 2 3 4))
               (mapcar
                (lambda (limit)
                  (length> [a b c] limit))
                '(0 2 3 4))
               (length= nil 0)
               (length= "abc" 3)
               (length= '(a b c) 2))"##,
        expect![[r#"OK ((nil nil nil t) (t t nil nil) t t nil)"#]],
    )
}

fn compat_length_comparator_rejects_non_sequence() -> ParityBatchCase {
    ParityBatchCase::signal(
        "compat_length_comparator_rejects_non_sequence",
        "(length< 3 1)",
        expect![[r#"ERR (wrong-type-argument sequencep 3)"#]],
    )
}

fn compat_hash_table_contains_distinguishes_missing_from_nil_value() -> ParityBatchCase {
    ParityBatchCase::value(
        "compat_hash_table_contains_distinguishes_missing_from_nil_value",
        r##"(let ((table (make-hash-table :test #'equal)))
               (puthash "present-nil" nil table)
               (puthash "present-value" 7 table)
               (list
                (hash-table-contains-p "present-nil" table)
                (gethash "present-nil" table 'fallback)
                (hash-table-contains-p "present-value" table)
                (gethash "present-value" table)
                (hash-table-contains-p "missing" table)
                (gethash "missing" table 'fallback)))"##,
        expect![[r#"OK (t nil t 7 nil fallback)"#]],
    )
}

pub(super) fn collections_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        compat_ensure_list_and_proper_list_cover_atoms_dotted_and_circular_inputs(),
        compat_take_drop_and_ntake_preserve_exact_copy_and_mutation_semantics(),
        compat_sequence_predicates_cover_closures_function_values_and_boundaries(),
        compat_length_comparators_cover_lists_vectors_and_equal_boundaries(),
        compat_length_comparator_rejects_non_sequence(),
        compat_hash_table_contains_distinguishes_missing_from_nil_value(),
    ]
}
