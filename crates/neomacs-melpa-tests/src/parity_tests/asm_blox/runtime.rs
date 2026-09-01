use expect_test::expect;

use super::ParityBatchCase;

fn asm_blox_upstream_wat_gameboard_scenarios_execute_stack_send_and_receive_workflows()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_upstream_wat_gameboard_scenarios_execute_stack_send_and_receive_workflows",
        r##"(let (results)
         (let ((asm-blox--gameboard
                (asm-blox-test-create-gameboard
                 '((0 0 "(const 1) (const 2) (const 3) (const 4)")))))
           (asm-blox-test-step 4)
           (push
            (asm-blox-test-runtime-summary
             (asm-blox--cell-at-row-col 0 0))
            results))
         (let ((asm-blox--gameboard
                (asm-blox-test-create-gameboard
                 '((0 0 "(const 1) (send down) (const 3) (send right)")))))
           (asm-blox-test-step 4)
           (push
            (asm-blox-test-runtime-summary
             (asm-blox--cell-at-row-col 0 0))
            results))
         (let ((asm-blox--gameboard
                (asm-blox-test-create-gameboard
                 '((0 0 "(const 1) (send down)")
                   (1 0 "(get up)")))))
           (asm-blox-test-step 4)
           (push
            (list
             (asm-blox-test-runtime-summary
              (asm-blox--cell-at-row-col 0 0))
             (asm-blox-test-runtime-summary
              (asm-blox--cell-at-row-col 1 0)))
            results))
         (nreverse results))"##,
        expect![
            "OK ((:row 0 :col 0 :pc 0 :stack (4 3 2 1) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 0 :pc 0 :stack nil :ports (nil 3 1 nil) :staging (nil sent sent nil) :state nil) ((:row 0 :col 0 :pc 0 :stack nil :ports (nil nil 1 nil) :staging (nil nil sent nil) :state nil) (:row 1 :col 0 :pc 0 :stack (1) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil)))"
        ],
    )
}

fn asm_blox_real_arithmetic_pipeline_obeys_operand_order_integer_math_and_wraparound()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_real_arithmetic_pipeline_obeys_operand_order_integer_math_and_wraparound",
        r##"(let ((programs
                '("(add (const 800) (const 500))"
                  "(sub (const 20) (const 7))"
                  "(mul (const -60) (const 40))"
                  "(div (const -17) (const 5))"
                  "(rem (const -17) (const 5))"
                  "(neg (const -999))"
                  "(abs (const -321))")))
         (mapcar
          (lambda (program)
            (let* ((runtime
                    (asm-blox--parse-cell '(1 1) program))
                   (steps
                    (length
                     (asm-blox--cell-runtime-instructions runtime))))
              (dotimes (_ steps)
                (asm-blox--cell-runtime-step runtime))
              (list
               program
               (asm-blox-test-runtime-summary runtime))))
          programs))"##,
        expect![[
            r#"OK (("(add (const 800) (const 500))" (:row 1 :col 1 :pc 0 :stack (-698) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil)) ("(sub (const 20) (const 7))" (:row 1 :col 1 :pc 0 :stack (13) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil)) ("(mul (const -60) (const 40))" (:row 1 :col 1 :pc 0 :stack (-402) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil)) ("(div (const -17) (const 5))" (:row 1 :col 1 :pc 0 :stack (-3) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil)) ("(rem (const -17) (const 5))" (:row 1 :col 1 :pc 0 :stack (-2) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil)) ("(neg (const -999))" (:row 1 :col 1 :pc 0 :stack (999) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil)) ("(abs (const -321))" (:row 1 :col 1 :pc 0 :stack (321) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil)))"#
        ]],
    )
}

fn asm_blox_comparison_and_boolean_instructions_cover_negative_zero_and_positive_truth_tables()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_comparison_and_boolean_instructions_cover_negative_zero_and_positive_truth_tables",
        r##"(mapcar
         (lambda (program)
           (let* ((runtime
                   (asm-blox--parse-cell '(0 0) program))
                  (steps
                   (length
                    (asm-blox--cell-runtime-instructions runtime))))
             (dotimes (_ steps)
               (asm-blox--cell-runtime-step runtime))
             (cons
              program
              (asm-blox--cell-runtime-stack runtime))))
         '("(eq (const 4) (const 4))"
           "(ne (const 4) (const 5))"
           "(lt (const -2) (const 3))"
           "(le (const 3) (const 3))"
           "(gt (const 7) (const 2))"
           "(ge (const 2) (const 3))"
           "(and (const -1) (const 8))"
           "(and (const 0) (const 8))"
           "(or (const 0) (const -8))"
           "(not (const 0))"
           "(not (const 5))"
           "(eqz (const 0))"
           "(gz (const 1))"
           "(lz (const -1))"))"##,
        expect![[
            r#"OK (("(eq (const 4) (const 4))" 1) ("(ne (const 4) (const 5))" 1) ("(lt (const -2) (const 3))" 1) ("(le (const 3) (const 3))" 1) ("(gt (const 7) (const 2))" 1) ("(ge (const 2) (const 3))" 0) ("(and (const -1) (const 8))" 1) ("(and (const 0) (const 8))" 0) ("(or (const 0) (const -8))" 1) ("(not (const 0))" 1) ("(not (const 5))" 0) ("(eqz (const 0))" 1) ("(gz (const 1))" 1) ("(lz (const -1))" 1))"#
        ]],
    )
}

fn asm_blox_stack_program_exercises_get_set_inc_dec_dup_drop_and_clear_with_real_state_changes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_stack_program_exercises_get_set_inc_dec_dup_drop_and_clear_with_real_state_changes",
        r##"(let* ((runtime
                 (asm-blox--parse-cell
                  '(2 3)
                  "(const 10) (const 20) (get 0) (get -1) (set 1) (inc 0) (dec -1) (drop) (dup) (clr) (const 42)"))
                (trace nil))
         (dotimes (_
                   (length
                    (asm-blox--cell-runtime-instructions runtime)))
           (asm-blox--cell-runtime-step runtime)
           (push
            (list
             (asm-blox--cell-runtime-pc runtime)
             (copy-sequence
              (asm-blox--cell-runtime-stack runtime))
             asm-blox-runtime-error
             asm-blox--gameboard-state)
            trace))
         (nreverse trace))"##,
        expect![
            "OK ((1 (10) nil nil) (2 (20 10) nil nil) (3 (10 20 10) nil nil) (4 (10 10 20 10) nil nil) (5 (10 10 10) nil nil) (6 (10 10 11) nil nil) (7 (9 10 11) nil nil) (8 (10 11) nil nil) (9 (10 11 10 11) nil nil) (10 nil nil nil) (0 (42) nil nil))"
        ],
    )
}

fn asm_blox_stack_limits_and_bad_indices_surface_runtime_errors_without_hiding_partial_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_stack_limits_and_bad_indices_surface_runtime_errors_without_hiding_partial_state",
        r##"(mapcar
         (lambda (program)
           (let ((runtime
                  (asm-blox--parse-cell '(2 1) program))
                 (asm-blox-runtime-error nil)
                 (asm-blox--gameboard-state nil))
             (list
              program
              (condition-case error
                  (catch 'runtime-error
                    (dotimes (_ 12)
                      (asm-blox--cell-runtime-step runtime))
                    :completed)
                (error
                 (list
                  :signaled
                  (car error)
                  (cdr error))))
              (asm-blox-test-runtime-summary runtime)
              asm-blox-runtime-error
              asm-blox--gameboard-state)))
         '("(drop)"
           "(const 1) (const 2) (const 3) (const 4) (const 5)"
           "(const 1) (get 8)"
           "(const 1) (inc 4)"
           "(const 1) (set -3)"))"##,
        expect![[
            r#"OK (("(drop)" (error "Stack underflow" 2 1) (:row 2 :col 1 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) nil nil) ("(const 1) (const 2) (const 3) (const 4) (const 5)" (error "Stack overflow" 2 1) (:row 2 :col 1 :pc 4 :stack (4 3 2 1) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) nil nil) ("(const 1) (get 8)" (error "Stack overflow" 2 1) (:row 2 :col 1 :pc 0 :stack (nil 1 nil 1) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) ("Bad idx 8/3" 2 1) nil) ("(const 1) (inc 4)" (error "Stack overflow" 2 1) (:row 2 :col 1 :pc 0 :stack (2 2 2 2) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) ("Idx out of bounds" 2 1) error) ("(const 1) (set -3)" (:signaled wrong-type-argument (consp nil)) (:row 2 :col 1 :pc 1 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) ("Idx out of bounds" 2 1) error))"#
        ]],
    )
}

fn asm_blox_send_backpressure_keeps_stack_and_program_counter_blocked_until_port_clears()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_send_backpressure_keeps_stack_and_program_counter_blocked_until_port_clears",
        r##"(let* ((runtime
                 (asm-blox--parse-cell
                  '(0 0)
                  "(const 7) (send right) (const 8) (send right)"))
                trace)
         (dotimes (_ 5)
           (asm-blox--cell-runtime-step runtime)
           (asm-blox--cell-runtime-merge-ports-with-staging runtime)
           (push
            (asm-blox-test-runtime-summary runtime)
            trace))
         (asm-blox--remove-value-from-direction runtime 'RIGHT)
         (asm-blox--cell-runtime-merge-ports-with-staging runtime)
         (asm-blox--cell-runtime-step runtime)
         (asm-blox--cell-runtime-merge-ports-with-staging runtime)
         (nreverse
          (cons
           (asm-blox-test-runtime-summary runtime)
           trace)))"##,
        expect![
            "OK ((:row 0 :col 0 :pc 1 :stack (7) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 0 :pc 2 :stack nil :ports (nil 7 nil nil) :staging (nil sent nil nil) :state nil) (:row 0 :col 0 :pc 3 :stack (8) :ports (nil 7 nil nil) :staging (nil sent nil nil) :state nil) (:row 0 :col 0 :pc 3 :stack (8) :ports (nil 7 nil nil) :staging (nil sent nil nil) :state nil) (:row 0 :col 0 :pc 3 :stack (8) :ports (nil 7 nil nil) :staging (nil sent nil nil) :state nil) (:row 0 :col 0 :pc 0 :stack nil :ports (nil 8 nil nil) :staging (nil sent nil nil) :state nil))"
        ],
    )
}

fn asm_blox_get_blocks_until_neighbor_data_arrives_then_consumes_exactly_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_get_blocks_until_neighbor_data_arrives_then_consumes_exactly_once",
        r##"(let* ((asm-blox--gameboard
                 (asm-blox-test-create-gameboard
                  '((0 0 "(get right) (const 1) (add)")
                    (0 1 "(const 41) (send left)"))))
                trace)
         (dotimes (_ 5)
           (asm-blox-test-step)
           (push
            (list
             (asm-blox-test-runtime-summary
              (asm-blox--cell-at-row-col 0 0))
             (asm-blox-test-runtime-summary
              (asm-blox--cell-at-row-col 0 1)))
            trace))
         (nreverse trace))"##,
        expect![
            "OK (((:row 0 :col 0 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 1 :pc 1 :stack (41) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil)) ((:row 0 :col 0 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 1 :pc 0 :stack nil :ports (nil nil nil 41) :staging (nil nil nil sent) :state nil)) ((:row 0 :col 0 :pc 1 :stack #1=(41) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 1 :pc 1 :stack (41) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil)) ((:row 0 :col 0 :pc 2 :stack (1 . #1#) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 1 :pc 0 :stack nil :ports (nil nil nil 41) :staging (nil nil nil sent) :state nil)) ((:row 0 :col 0 :pc 0 :stack (42) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 1 :pc 1 :stack (41) :ports (nil nil nil 41) :staging (nil nil nil sent) :state nil)))"
        ],
    )
}

fn asm_blox_compiled_nested_loop_runs_a_countdown_and_branches_to_the_correct_targets()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_compiled_nested_loop_runs_a_countdown_and_branches_to_the_correct_targets",
        r##"(cl-letf
         (((symbol-function 'random)
           #'asm-blox-test-random))
         (let* ((asm-blox-test-random-values
                 '(10 20 30 40))
                (runtime
                 (asm-blox--parse-cell
                  '(1 1)
                  "(block (const 3) (loop (dec -1) (dup) (br_if 0)) (drop) (const 77))"))
                trace)
           (dotimes (_ 16)
             (asm-blox--cell-runtime-step runtime)
             (push
              (list
               (asm-blox--cell-runtime-pc runtime)
               (copy-sequence
                (asm-blox--cell-runtime-stack runtime)))
              trace))
           (list
            (mapcar
             #'asm-blox-test-instruction-summary
             (asm-blox--cell-runtime-instructions runtime))
            (nreverse trace))))"##,
        expect![
            "OK ((((CONST 3) 8 17) ((LABEL L_20_2) nil nil) ((DEC -1) 24 32) ((DUP) 33 38) ((JMP_IF 1) 39 48) ((DROP) 50 56) ((CONST 77) 57 67) ((LABEL L_10_1) nil nil)) ((2 (3)) (3 (2)) (4 (2 2)) (2 (2)) (3 (1)) (4 (1 1)) (2 (1)) (3 (0)) (4 (0 0)) (5 (0)) (6 nil) (0 (77)) (2 (3 77)) (3 (2 77)) (4 (2 77 2 77)) (2 (77 2 77))))"
        ],
    )
}

fn asm_blox_direction_coordinates_mirroring_bounds_and_outside_cells_cover_every_board_edge()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_direction_coordinates_mirroring_bounds_and_outside_cells_cover_every_board_edge",
        r##"(let ((asm-blox--gameboard
                (asm-blox-test-create-gameboard nil)))
         (list
          (mapcar
           (lambda (direction)
             (list
              direction
              (asm-blox--mirror-direction direction)))
           '(UP RIGHT DOWN LEFT diagonal))
          (mapcar
           (lambda (case)
             (list
              case
              (apply
               #'asm-blox--valid-position
               case)))
           '((0 0)
             (2 3)
             (-1 0)
             (3 0)
             (0 0 UP)
             (0 0 LEFT)
             (0 0 RIGHT)
             (2 3 DOWN)))
          (mapcar
           (lambda (case)
             (let ((cell
                    (apply
                     #'asm-blox--cell-at-row-col
                     case)))
               (list
                case
                (asm-blox--cell-runtime-p cell)
                (asm-blox--cell-runtime-row cell)
                (asm-blox--cell-runtime-col cell))))
           '((0 0) (2 3) (-1 0) (3 4)))))"##,
        expect![
            "OK (((UP DOWN) (RIGHT LEFT) (DOWN UP) (LEFT RIGHT) (diagonal nil)) (((0 0) t) ((2 3) t) ((-1 0) nil) ((3 0) nil) ((0 0 UP) nil) ((0 0 LEFT) nil) ((0 0 RIGHT) t) ((2 3 DOWN) nil)) (((0 0) t 0 0) ((2 3) t 2 3) ((-1 0) t nil nil) ((3 4) t nil nil)))"
        ],
    )
}

fn asm_blox_board_runs_code_cells_before_custom_cells_and_captures_thrown_runtime_errors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_board_runs_code_cells_before_custom_cells_and_captures_thrown_runtime_errors",
        r##"(let* ((events nil)
                (asm-blox--gameboard
                 (asm-blox-test-create-gameboard
                  '((0 0 "(const 5)"))))
                (custom
                 (asm-blox--cell-at-row-col 2 3)))
         (setf
          (asm-blox--cell-runtime-run-function custom)
          (lambda (runtime)
            (push
             (list
              :custom
              (asm-blox--cell-runtime-row runtime)
              (asm-blox--cell-runtime-col runtime)
              (copy-sequence
               (asm-blox--cell-runtime-stack
                (asm-blox--cell-at-row-col 0 0))))
             events)))
         (asm-blox--gameboard-step)
         (setf
          (asm-blox--cell-runtime-run-function custom)
          (lambda (_runtime)
            (throw
             'runtime-error
             '(error "fixture failure" 2 3))))
         (asm-blox--gameboard-step)
         (list
          (nreverse events)
          asm-blox--gameboard-state
          asm-blox-runtime-error))"##,
        expect![[r#"OK (((:custom 2 3 (5))) error ("fixture failure" 2 3))"#]],
    )
}

fn asm_blox_current_instruction_empty_program_wraparound_and_label_skipping_contracts_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_current_instruction_empty_program_wraparound_and_label_skipping_contracts_match",
        r##"(let* ((empty
                 (asm-blox--cell-runtime-create
                  :instructions nil
                  :pc 0
                  :stack nil
                  :row 0
                  :col 0))
                (labeled
                 (asm-blox--cell-runtime-create
                  :instructions
                  (mapcar
                   (lambda (children)
                     (asm-blox--code-node-create
                      :children children))
                   '((LABEL a)
                     (LABEL b)
                     (CONST 9)))
                  :pc 0
                  :stack nil
                  :row 0
                  :col 1)))
         (asm-blox--cell-runtime-skip-labels labeled)
         (list
          (asm-blox-test-instruction-summary
           (asm-blox--cell-runtime-current-instruction empty))
          (asm-blox-test-runtime-summary empty)
          (asm-blox-test-runtime-summary labeled)
          (asm-blox-test-instruction-summary
           (asm-blox--cell-runtime-current-instruction labeled))
          (progn
            (asm-blox--cell-runtime-step labeled)
            (asm-blox-test-runtime-summary labeled))))"##,
        expect![
            "OK (((_EMPTY) nil nil) (:row 0 :col 0 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 1 :pc 2 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) ((CONST 9) nil nil) (:row 0 :col 1 :pc 2 :stack (9) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil))"
        ],
    )
}

pub(super) fn runtime_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asm_blox_upstream_wat_gameboard_scenarios_execute_stack_send_and_receive_workflows(),
        asm_blox_real_arithmetic_pipeline_obeys_operand_order_integer_math_and_wraparound(),
        asm_blox_comparison_and_boolean_instructions_cover_negative_zero_and_positive_truth_tables(
        ),
        asm_blox_stack_program_exercises_get_set_inc_dec_dup_drop_and_clear_with_real_state_changes(
        ),
        asm_blox_stack_limits_and_bad_indices_surface_runtime_errors_without_hiding_partial_state(),
        asm_blox_send_backpressure_keeps_stack_and_program_counter_blocked_until_port_clears(),
        asm_blox_get_blocks_until_neighbor_data_arrives_then_consumes_exactly_once(),
        asm_blox_compiled_nested_loop_runs_a_countdown_and_branches_to_the_correct_targets(),
        asm_blox_direction_coordinates_mirroring_bounds_and_outside_cells_cover_every_board_edge(),
        asm_blox_board_runs_code_cells_before_custom_cells_and_captures_thrown_runtime_errors(),
        asm_blox_current_instruction_empty_program_wraparound_and_label_skipping_contracts_match(),
    ]
}
