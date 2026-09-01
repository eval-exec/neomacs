use expect_test::expect;

use super::ParityBatchCase;

fn asilea_state_to_option_list_flattens_real_compiler_groups_in_group_and_option_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_state_to_option_list_flattens_real_compiler_groups_in_group_and_option_order",
        r##"(let ((options
                [["-O0" "-O2" "-O3" "-Os"]
                 [nil "-g"]
                 [nil ("-march=native" "-mtune=native")]
                 ["-Wall" ("-Wall" "-Wextra")]
                 [("-DNAME=hello world"
                   "-Iinclude path")]]))
         (mapcar
          (lambda (state)
            (list
             (append state nil)
             (asilea--state-to-option-list
              state
              options)))
          '([0 0 0 0 0]
            [1 1 1 1 0]
            [2 0 1 0 0]
            [3 1 0 1 0])))"##,
        expect![[
            r#"OK (((0 0 0 0 0) ("-O0" "-Wall" "-DNAME=hello world" "-Iinclude path")) ((1 1 1 1 0) ("-O2" "-g" "-march=native" "-mtune=native" "-Wall" "-Wextra" "-DNAME=hello world" "-Iinclude path")) ((2 0 1 0 0) ("-O3" "-march=native" "-mtune=native" "-Wall" "-DNAME=hello world" "-Iinclude path")) ((3 1 0 1 0) ("-Os" "-g" "-Wall" "-Wextra" "-DNAME=hello world" "-Iinclude path")))"#
        ]],
    )
}

fn asilea_state_to_option_list_preserves_empty_duplicate_unicode_and_non_string_values()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_state_to_option_list_preserves_empty_duplicate_unicode_and_non_string_values",
        r##"(let ((options
                [[nil "" "λ"]
                 [("a" "a" "") []]
                 [42 symbol ("日本" 7 nil)]
                 [(("--nested"))]]))
         (mapcar
          (lambda (state)
            (condition-case error-data
                (list
                 state
                 :ok
                 (asilea--state-to-option-list
                  state options))
              (error
               (list
                state
                :error
                (car error-data)
                (cdr error-data)))))
          '([0 0 0 0]
            [1 0 1 0]
            [2 1 2 0])))"##,
        expect![[
            r#"OK (([0 0 0 0] :ok ("a" "a" "" 42 #1=("--nested"))) ([1 0 1 0] :ok ("" "a" "a" "" symbol #1#)) ([2 1 2 0] :ok ("λ" [] "日本" 7 nil #1#)))"#
        ]],
    )
}

fn asilea_state_to_option_list_does_not_mutate_state_options_or_nested_lists() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_state_to_option_list_does_not_mutate_state_options_or_nested_lists",
        r##"(let* ((nested
                  (list "-ffoo" "-fbar"))
                 (options
                  (vector
                   (vector "-O2" "-O3")
                   (vector nil nested)))
                 (state [1 1])
                 (state-before
                  (copy-sequence state))
                 (options-before
                  (copy-tree options))
                 (nested-before
                  (copy-sequence nested))
                 (result
                  (asilea--state-to-option-list
                   state options)))
         (list
          result
          state
          (equal state state-before)
          options
          (equal options options-before)
          nested
          (equal nested nested-before)
          (eq nested (aref (aref options 1) 1))
          (eq nested result)))"##,
        expect![[
            r#"OK (("-O3" "-ffoo" "-fbar") [1 1] t [["-O2" "-O3"] [nil #1=("-ffoo" "-fbar")]] t #1# t t nil)"#
        ]],
    )
}

fn asilea_state_to_option_list_invalid_shape_and_indices_signal_exact_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_state_to_option_list_invalid_shape_and_indices_signal_exact_errors",
        r##"(mapcar
         (lambda (spec)
           (condition-case error-data
               (list
                spec
                :ok
                (asilea--state-to-option-list
                 (car spec)
                 (cadr spec)))
             (error
              (list
               spec
               :error
               (car error-data)
               (cdr error-data)))))
         '(([0] [["x"]])
           ([1] [["x"]])
           ([-1] [["x"]])
           ([0 0] [["x"]])
           ([0] [])
           ([] [["x"]])
           ("state" [["x"]])
           ([0] "options")
           ([nil] [["x"]])
           ([0] [nil])))"##,
        expect![[
            r#"OK ((([0] [["x"]]) :ok ("x")) (([1] [#1=["x"]]) :error args-out-of-range (#1# 1)) (([-1] [#2=["x"]]) :error args-out-of-range (#2# -1)) (([0 0] #3=[["x"]]) :error args-out-of-range (#3# 1)) (([0] []) :error args-out-of-range ([] 0)) (([] [["x"]]) :ok nil) (("state" [#4=["x"]]) :error args-out-of-range (#4# 115)) (([0] "options") :error wrong-type-argument (arrayp 111)) (([nil] [["x"]]) :error wrong-type-argument (fixnump nil)) (([0] [nil]) :error wrong-type-argument (arrayp nil)))"#
        ]],
    )
}

fn asilea_generate_random_state_uses_each_group_length_and_scripted_draw_in_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_generate_random_state_uses_each_group_length_and_scripted_draw_in_order",
        r##"(let ((options
                [["-O0" "-O2" "-O3"]
                 [nil "-g"]
                 ["a" "b" "c" "d"]
                 ["only"]])
               (draws '(2 0 3 0))
               calls)
         (let ((state
                (asilea--generate-random-state
                 options
                 (lambda (limit)
                   (push limit calls)
                   (pop draws)))))
           (list
            state
            (append state nil)
            (nreverse calls)
            draws
            options
            (asilea--state-to-option-list
             state options))))"##,
        expect![[
            r#"OK ([2 0 3 0] (2 0 3 0) (3 2 4 1) nil [["-O0" "-O2" "-O3"] [nil "-g"] ["a" "b" "c" "d"] ["only"]] ("-O3" "d" "only"))"#
        ]],
    )
}

fn asilea_generate_random_state_returns_new_vector_without_mutating_option_groups()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_generate_random_state_returns_new_vector_without_mutating_option_groups",
        r##"(let* ((first
                  (vector "a" "b"))
                 (second
                  (vector "c" "d" "e"))
                 (options
                  (vector first second))
                 (result
                  (asilea--generate-random-state
                   options
                   (lambda (_limit) 0))))
         (list
          result
          options
          (eq result options)
          (eq first (aref options 0))
          (eq second (aref options 1))
          (eq (aref result 0) (aref options 0))
          (mapcar #'length
                  (append options nil))))"##,
        expect![[r#"OK ([0 0] [["a" "b"] ["c" "d" "e"]] nil t t nil (2 3))"#]],
    )
}

fn asilea_neighboring_state_changes_scripted_coordinate_and_preserves_original() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asilea_neighboring_state_changes_scripted_coordinate_and_preserves_original",
        r##"(let ((options
                [["-O0" "-O2" "-O3"]
                 [nil "-g"]
                 ["a" "b" "c" "d"]])
               (specs
                '((0 2)
                  (1 1)
                  (2 3)
                  (1 0))))
         (mapcar
          (lambda (draws)
            (let* ((state [1 0 2])
                   (original
                    (copy-sequence state))
                   calls
                   (remaining
                    (copy-sequence draws))
                   (neighbor
                    (asilea--neighboring-state
                     state
                     options
                     (lambda (limit)
                       (push limit calls)
                       (pop remaining)))))
              (list
               draws
               original
               state
               neighbor
               (eq state neighbor)
               (equal state original)
               (nreverse calls)
               remaining
               (asilea--state-to-option-list
                neighbor options))))
          specs))"##,
        expect![[
            r#"OK (((0 2) [1 0 2] #1=[1 0 2] [2 0 2] nil t (3 3) nil ("-O3" "c")) ((1 1) [1 0 2] #1# [1 1 2] nil t (3 2) nil ("-O2" "-g" "c")) ((2 3) [1 0 2] #1# [1 0 3] nil t (3 4) nil ("-O2" "d")) ((1 0) [1 0 2] #1# [1 0 2] nil t (3 2) nil ("-O2" "c")))"#
        ]],
    )
}

fn asilea_random_state_helpers_surface_empty_and_invalid_shapes_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_random_state_helpers_surface_empty_and_invalid_shapes_exactly",
        r##"(let ((random-function
                (lambda (limit)
                  (if
                      (and
                       (numberp limit)
                       (> limit 0))
                      0
                    (error
                     "bad limit: %S"
                     limit)))))
         (mapcar
          (lambda (form)
            (condition-case error-data
                (list
                 form
                 :ok
                 (eval form t))
              (error
               (list
                form
                :error
                (car error-data)
                (cdr error-data)))))
          `((asilea--generate-random-state
             []
             ,random-function)
            (asilea--generate-random-state
             [[]]
             ,random-function)
            (asilea--neighboring-state
             []
             []
             ,random-function)
            (asilea--neighboring-state
             [0]
             [[]]
             ,random-function)
            (asilea--neighboring-state
             "x"
             [["x"]]
             ,random-function))))"##,
        expect![[
            r#"OK (((asilea--generate-random-state [] #1=#[(limit) ((if (and (numberp limit) (> limit 0)) 0 (error "bad limit: %S" limit))) (t)]) :ok []) ((asilea--generate-random-state [[]] #1#) :error error ("bad limit: 0")) ((asilea--neighboring-state [] [] #1#) :error error ("bad limit: 0")) ((asilea--neighboring-state [0] [[]] #1#) :error error ("bad limit: 0")) ((asilea--neighboring-state "x" [["x"]] #1#) :ok "\0"))"#
        ]],
    )
}

pub(super) fn options_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asilea_state_to_option_list_flattens_real_compiler_groups_in_group_and_option_order(),
        asilea_state_to_option_list_preserves_empty_duplicate_unicode_and_non_string_values(),
        asilea_state_to_option_list_does_not_mutate_state_options_or_nested_lists(),
        asilea_state_to_option_list_invalid_shape_and_indices_signal_exact_errors(),
        asilea_generate_random_state_uses_each_group_length_and_scripted_draw_in_order(),
        asilea_generate_random_state_returns_new_vector_without_mutating_option_groups(),
        asilea_neighboring_state_changes_scripted_coordinate_and_preserves_original(),
        asilea_random_state_helpers_surface_empty_and_invalid_shapes_exactly(),
    ]
}
