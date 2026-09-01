use expect_test::expect;

use super::ParityBatchCase;

fn asm_blox_yaml_and_sexp_stack_definitions_produce_equivalent_runtime_configuration()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_yaml_and_sexp_stack_definitions_produce_equivalent_runtime_configuration",
        r##"(let ((yaml
                (asm-blox--parse-cell
                 '(1 2)
                 "apiVersion: v1\nkind: Stack\nmetadata:\n  name: work\nspec:\n  inputPorts: [left, up]\n  outputPort: right\n  sizePort: down\n  size: 6\n  logLevel: debug\n"))
               (sexp
                (asm-blox--parse-cell
                 '(1 2)
                 "(module stack :input-ports (left up) :output-port right :size-port down :size 6 :log-level debug)")))
         (mapcar
          (lambda (runtime)
            (list
             (asm-blox-test-runtime-summary runtime)
             (asm-blox--cell-runtime-run-function runtime)
             (asm-blox--cell-runtime-message-function runtime)
             (asm-blox--cell-runtime-run-spec runtime)))
          (list yaml sexp)))"##,
        expect![[
            r#"OK (((:row 1 :col 2 :pc nil :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) asm-blox--yaml-step-stack asm-blox--yaml-message-stack ((inputPorts "left" "up") (outputPort . "right") (sizePort . "down") (size . 6) (logLevel . "debug"))) ((:row 1 :col 2 :pc nil :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) asm-blox--yaml-step-stack asm-blox--yaml-message-stack ((logLevel . "debug") (size . 6) (sizePort . "down") (outputPort . "right") (inputPorts "left" "up"))))"#
        ]],
    )
}

fn asm_blox_yaml_dispatch_reports_api_kind_spec_and_stack_validation_errors_precisely()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_yaml_dispatch_reports_api_kind_spec_and_stack_validation_errors_precisely",
        r##"(mapcar
         (lambda (source)
           (list
            source
            (condition-case error
                (asm-blox--parse-cell '(0 0) source)
              (error
               (list
                :signaled
                (car error)
                (cdr error))))))
         '("apiVersion: v2\nkind: Stack\nspec:\n  inputPort: left\n  outputPort: right\n"
           "apiVersion: v1\nkind: Unknown\nspec:\n  inputPort: left\n"
           "apiVersion: v1\nkind: Stack\n"
           "apiVersion: v1\nkind: Stack\nspec: {}\n"
           "apiVersion: v1\nkind: Stack\nspec:\n  outputPort: right\n"
           "apiVersion: v1\nkind: Stack\nspec:\n  inputPort: diagonal\n  outputPort: right\n"
           "apiVersion: v1\nkind: Stack\nspec:\n  inputPort: left\n  outputPort: right\n  size: 1000\n"
           "apiVersion: v1\nkind: Container\nspec:\n  image: fixture\n"))"##,
        expect![[
            r#"OK (("apiVersion: v2\nkind: Stack\nspec:\n  inputPort: left\n  outputPort: right\n" (error 0 "bad api version")) ("apiVersion: v1\nkind: Unknown\nspec:\n  inputPort: left\n" (error 0 "unknown kind")) ("apiVersion: v1\nkind: Stack\n" #1=(error 0 "must define spec")) ("apiVersion: v1\nkind: Stack\nspec: {}\n" #1#) ("apiVersion: v1\nkind: Stack\nspec:\n  outputPort: right\n" (error 0 "missing inputPort")) ("apiVersion: v1\nkind: Stack\nspec:\n  inputPort: diagonal\n  outputPort: right\n" (error 0 "invalid inputPort")) ("apiVersion: v1\nkind: Stack\nspec:\n  inputPort: left\n  outputPort: right\n  size: 1000\n" (error 0 "invalid size")) ("apiVersion: v1\nkind: Container\nspec:\n  image: fixture\n" (:signaled error ("Container not implemented"))))"#
        ]],
    )
}

fn asm_blox_sexp_spec_transform_handles_kebab_case_lists_symbols_numbers_and_invalid_keys()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_sexp_spec_transform_handles_kebab_case_lists_symbols_numbers_and_invalid_keys",
        r##"(list
         (asm-blox--transform-sexp-data
          '(:input-port left
            :input-ports (up down)
            :size 8
            :log-level debug
            :data (one two three)))
         (catch 'error
           (asm-blox--transform-sexp-data
            '(input-port left)))
         (catch 'error
           (asm-blox--transform-sexp-data
            '(7 left)))
         (mapcar
          (lambda (source)
            (condition-case error
                (asm-blox--parse-cell '(0 3) source)
              (error
               (list
                :signaled
                (car error)
                (cdr error)))))
          '("(module)"
            "(module mystery :size 2)"
            "(module stack)"
            "(module container :image x)")))"##,
        expect![[
            r#"OK (((data "one" "two" "three") (logLevel . "debug") (size . 8) (inputPorts "up" "down") (inputPort . "left")) (error 0 "invalid spec key") (error 0 "invalid spec key") (#1=(error 8 "invalid kind") #1# (error 0 "must define spec") (:signaled error ("Container not implemented"))))"#
        ]],
    )
}

fn asm_blox_yaml_stack_accepts_multiple_input_ports_applies_capacity_and_publishes_size_and_lifo_output()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_yaml_stack_accepts_multiple_input_ports_applies_capacity_and_publishes_size_and_lifo_output",
        r##"(let* ((asm-blox--gameboard
                 (asm-blox-test-create-gameboard nil))
                (stack
                 (asm-blox--parse-cell
                  '(1 1)
                  "(module stack :input-ports (left up) :output-port right :size-port down :size 3)"))
                (left
                 (asm-blox--cell-at-row-col 1 0))
                (up
                 (asm-blox--cell-at-row-col 0 1))
                trace)
         (asm-blox--set-cell-at-row-col 1 1 stack)
         (setf
          (asm-blox--cell-runtime-right left) 10
          (asm-blox--cell-runtime-down up) 20)
         (dotimes (_ 4)
           (asm-blox--yaml-step-stack stack)
           (asm-blox--cell-runtime-merge-ports-with-staging stack)
           (push
            (list
             (asm-blox-test-runtime-summary stack)
             (asm-blox-test-runtime-summary left)
             (asm-blox-test-runtime-summary up)
             (asm-blox--yaml-message-stack stack))
            trace)
           (asm-blox--remove-value-from-direction stack 'RIGHT)
           (asm-blox--remove-value-from-direction stack 'DOWN))
         (nreverse trace))"##,
        expect![[
            r#"OK (((:row 1 :col 1 :pc nil :stack nil :ports (nil 20 2 nil) :staging (nil sent sent nil) :state (10)) (:row 1 :col 0 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 1 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) "top:20 size:2/3") ((:row 1 :col 1 :pc nil :stack nil :ports (nil 10 1 nil) :staging (nil sent sent nil) :state nil) (:row 1 :col 0 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 1 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) "top:10 size:1/3") ((:row 1 :col 1 :pc nil :stack nil :ports (nil nil 0 nil) :staging (nil nil sent nil) :state nil) (:row 1 :col 0 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 1 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) "empty stack") ((:row 1 :col 1 :pc nil :stack nil :ports (nil nil 0 nil) :staging (nil nil sent nil) :state nil) (:row 1 :col 0 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 1 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) "empty stack"))"#
        ]],
    )
}

fn asm_blox_yaml_stack_backpressure_requeues_output_and_overflow_preserves_diagnostic_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_yaml_stack_backpressure_requeues_output_and_overflow_preserves_diagnostic_state",
        r##"(let* ((asm-blox--gameboard
                 (asm-blox-test-create-gameboard nil))
                (stack
                 (asm-blox--parse-cell
                  '(1 1)
                  "(module stack :input-port left :output-port right :size 1)"))
                (left
                 (asm-blox--cell-at-row-col 1 0)))
         (asm-blox--set-cell-at-row-col 1 1 stack)
         (setf
          (asm-blox--cell-runtime-run-state stack) '(30)
          (asm-blox--cell-runtime-right stack) 40
          (asm-blox--cell-runtime-right left) 50)
         (list
          (catch 'runtime-error
            (asm-blox--yaml-step-stack stack)
            :completed)
          (asm-blox-test-runtime-summary stack)
          (asm-blox-test-runtime-summary left)
          (asm-blox--yaml-message-stack stack)))"##,
        expect![[
            r#"OK ((error "Stack overflow 3/1" 1 1) (:row 1 :col 1 :pc nil :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state (50 40 30)) (:row 1 :col 0 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) "top:50 size:3/1")"#
        ]],
    )
}

fn asm_blox_yaml_heap_seek_write_set_read_peek_and_offset_ports_form_a_real_memory_workflow()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_yaml_heap_seek_write_set_read_peek_and_offset_ports_form_a_real_memory_workflow",
        r##"(let* ((asm-blox--gameboard
                 (asm-blox-test-create-gameboard nil))
                (heap
                 (asm-blox--yaml-create-heap
                  1 1 nil
                  '((size . 4)
                    (data . [4 5])
                    (seekPort . "left")
                    (writePort . "up")
                    (setPort . "down")
                    (readPort . "right")
                    (offsetPort . "down")
                    (peekPort . "up"))))
                (left
                 (asm-blox--cell-at-row-col 1 0))
                (up
                 (asm-blox--cell-at-row-col 0 1))
                (down
                 (asm-blox--cell-at-row-col 2 1))
                trace)
         (asm-blox--set-cell-at-row-col 1 1 heap)
         (setf
          (asm-blox--cell-runtime-right left) 2)
         (asm-blox--yaml-step-heap heap)
         (asm-blox--cell-runtime-merge-ports-with-staging heap)
         (push
          (list
           :seek
           (asm-blox-test-runtime-summary heap)
           (asm-blox--yaml-message-heap heap))
          trace)
         (setf
          (asm-blox--cell-runtime-down up) 77)
         (asm-blox--remove-value-from-direction heap 'UP)
         (asm-blox--remove-value-from-direction heap 'DOWN)
         (asm-blox--remove-value-from-direction heap 'RIGHT)
         (asm-blox--yaml-step-heap heap)
         (asm-blox--cell-runtime-merge-ports-with-staging heap)
         (push
          (list
           :write
           (asm-blox-test-runtime-summary heap)
           (asm-blox-test-runtime-summary up)
           (asm-blox--yaml-message-heap heap))
         trace)
         (setf
          (asm-blox--cell-runtime-up down) 88
          (asm-blox--cell-runtime-right left) 1)
         (asm-blox--remove-value-from-direction heap 'UP)
         (asm-blox--remove-value-from-direction heap 'DOWN)
         (asm-blox--remove-value-from-direction heap 'RIGHT)
         (asm-blox--yaml-step-heap heap)
         (asm-blox--cell-runtime-merge-ports-with-staging heap)
         (push
          (list
           :set-read
           (asm-blox-test-runtime-summary heap)
           (asm-blox-test-runtime-summary down)
           (asm-blox--yaml-message-heap heap))
          trace)
         (nreverse trace))"##,
        expect![[
            r#"OK ((:seek (:row 1 :col 1 :pc nil :stack nil :ports (0 0 2 nil) :staging (sent sent sent nil) :state (2 . #1=[4 88 0 77])) "0 @2/3") (:write (:row 1 :col 1 :pc nil :stack nil :ports (-999 -999 4 nil) :staging (sent sent sent nil) :state (4 . #1#)) (:row 0 :col 1 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) "end of file") (:set-read (:row 1 :col 1 :pc nil :stack nil :ports (88 88 1 nil) :staging (sent sent sent nil) :state (1 . #1#)) (:row 2 :col 1 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) "88 @1/3"))"#
        ]],
    )
}

fn asm_blox_yaml_heap_validation_rejects_duplicate_direction_invalid_size_and_bad_ports()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_yaml_heap_validation_rejects_duplicate_direction_invalid_size_and_bad_ports",
        r##"(mapcar
         (lambda (source)
           (list
            source
            (condition-case error
                (asm-blox--parse-cell '(2 2) source)
              (error
               (list
                :signaled
                (car error)
                (cdr error))))))
         '("(module heap :read-port right :peek-port right)"
           "(module heap :write-port left :seek-port left)"
           "(module heap :read-port diagonal)"
           "(module heap :size 0)"
           "(module heap :size 999)"
           "(module heap :size text)"
           "(module heap :size 3 :data (1 2 3 4))"))"##,
        expect![[
            r#"OK (("(module heap :read-port right :peek-port right)" (error 0 "same port: right")) ("(module heap :write-port left :seek-port left)" (error 0 "same port: left")) ("(module heap :read-port diagonal)" (error 0 "invalid readPort")) ("(module heap :size 0)" #1=(error 0 "invalid sizePort")) ("(module heap :size 999)" #1#) ("(module heap :size text)" #1#) ("(module heap :size 3 :data (1 2 3 4))" (:signaled wrong-type-argument (symbolp 1))))"#
        ]],
    )
}

fn asm_blox_yaml_controller_edits_text_and_publishes_cursor_and_character_observations()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_yaml_controller_edits_text_and_publishes_cursor_and_character_observations",
        r##"(let* ((sink
                 (asm-blox--cell-sink-create
                  :row 3 :col 0
                  :expected-data nil
                  :name "E"
                  :editor-text "abc"
                  :editor-point 2
                  :expected-text "fixture"))
                (asm-blox--extra-gameboard-cells
                 (asm-blox--problem-spec-create
                  :sources nil
                  :sinks (list sink)))
                (asm-blox--gameboard
                 (asm-blox-test-create-gameboard nil))
                (controller
                 (asm-blox--parse-cell
                  '(1 1)
                  "(module controller :input-port left :set-point-port up :char-at-port right :point-port down)"))
                (left
                 (asm-blox--cell-at-row-col 1 0))
                (up
                 (asm-blox--cell-at-row-col 0 1)))
         (asm-blox--set-cell-at-row-col 1 1 controller)
         (setf
          (asm-blox--cell-runtime-right left) ?X
          (asm-blox--cell-runtime-down up) 3)
         (asm-blox--yaml-step-controller controller)
         (asm-blox--cell-runtime-merge-ports-with-staging controller)
         (let ((first
                (list
                 (asm-blox-test-sink-summary sink)
                 (asm-blox-test-runtime-summary controller)
                 (asm-blox-test-runtime-summary left)
                 (asm-blox-test-runtime-summary up))))
           (asm-blox--remove-value-from-direction controller 'RIGHT)
           (asm-blox--remove-value-from-direction controller 'DOWN)
           (setf
            (asm-blox--cell-runtime-right left) ?\n
            (asm-blox--cell-runtime-down up) 2)
           (asm-blox--yaml-step-controller controller)
           (asm-blox--cell-runtime-merge-ports-with-staging controller)
           (list
            first
            (asm-blox-test-sink-summary sink)
            (asm-blox-test-runtime-summary controller))))"##,
        expect![[
            r#"OK (((3 0 nil "E" 0 nil "abXc" 4 "fixture") (:row 1 :col 1 :pc nil :stack nil :ports (nil 99 4 nil) :staging (nil sent sent nil) :state nil) (:row 1 :col 0 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 0 :col 1 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil)) (3 0 nil "E" 0 nil "a\nbXc" 3 "fixture") (:row 1 :col 1 :pc nil :stack nil :ports (nil 98 3 nil) :staging (nil sent sent nil) :state nil))"#
        ]],
    )
}

fn asm_blox_yaml_cell_messages_cover_empty_full_and_end_of_heap_states() -> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_yaml_cell_messages_cover_empty_full_and_end_of_heap_states",
        r##"(let* ((stack
                 (asm-blox--parse-cell
                  '(0 0)
                  "(module stack :input-port left :output-port right :size 2)"))
                (heap
                 (asm-blox--yaml-create-heap
                  0 1 nil
                  '((size . 2)
                    (data . [8 9])
                    (readPort . "right")))))
         (list
          (asm-blox--yaml-message-stack stack)
          (progn
            (setf
             (asm-blox--cell-runtime-run-state stack) '(3 2)
             (asm-blox--cell-runtime-right stack) 4)
            (asm-blox--yaml-message-stack stack))
          (asm-blox--yaml-message-heap heap)
          (progn
            (setf
             (asm-blox--cell-runtime-run-state heap)
             (cons 1 (vector 8 9)))
            (asm-blox--yaml-message-heap heap))
          (progn
            (setf
             (asm-blox--cell-runtime-run-state heap)
             (cons 2 (vector 8 9)))
            (asm-blox--yaml-message-heap heap))))"##,
        expect![[r#"OK ("empty stack" "top:4 size:3/2" "~~~" "9 @1/1" "end of file")"#]],
    )
}

pub(super) fn yaml_cells_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asm_blox_yaml_and_sexp_stack_definitions_produce_equivalent_runtime_configuration(),
        asm_blox_yaml_dispatch_reports_api_kind_spec_and_stack_validation_errors_precisely(),
        asm_blox_sexp_spec_transform_handles_kebab_case_lists_symbols_numbers_and_invalid_keys(),
        asm_blox_yaml_stack_accepts_multiple_input_ports_applies_capacity_and_publishes_size_and_lifo_output(),
        asm_blox_yaml_stack_backpressure_requeues_output_and_overflow_preserves_diagnostic_state(),
        asm_blox_yaml_heap_seek_write_set_read_peek_and_offset_ports_form_a_real_memory_workflow(),
        asm_blox_yaml_heap_validation_rejects_duplicate_direction_invalid_size_and_bad_ports(),
        asm_blox_yaml_controller_edits_text_and_publishes_cursor_and_character_observations(),
        asm_blox_yaml_cell_messages_cover_empty_full_and_end_of_heap_states(),
    ]
}
