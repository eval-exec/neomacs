use expect_test::expect;

use super::ParityBatchCase;

fn asm_blox_sources_peek_and_pop_zero_negative_and_exhausted_values_in_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_sources_peek_and_pop_zero_negative_and_exhausted_values_in_order",
        r##"(let ((source
                (asm-blox--cell-source-create
                 :row -1
                 :col 2
                 :data '(0 -7 42)
                 :idx 0
                 :name "I")))
         (list
          (asm-blox-test-source-summary source)
          (asm-blox--cell-source-current-value source)
          (asm-blox--cell-source-pop source)
          (asm-blox--cell-source-current-value source)
          (asm-blox--cell-source-pop source)
          (asm-blox--cell-source-pop source)
          (asm-blox--cell-source-current-value source)
          (asm-blox--cell-source-pop source)
          (asm-blox-test-source-summary source)
          (condition-case error
              (asm-blox--cell-source-pop '(not a source))
            (error
             (list
              (car error)
              (cdr error))))))"##,
        expect![[
            r#"OK ((-1 2 #1=(0 -7 42) "I" 0) 0 0 -7 -7 42 nil nil (-1 2 #1# "I" 4) (error ("Cell-source-pop type error")))"#
        ]],
    )
}

fn asm_blox_boundary_sources_feed_real_get_instructions_and_advance_only_when_consumed()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_boundary_sources_feed_real_get_instructions_and_advance_only_when_consumed",
        r##"(let* ((source-top
                 (asm-blox--cell-source-create
                  :row -1 :col 0
                  :data '(10 20)
                  :name "T"))
                (source-right
                 (asm-blox--cell-source-create
                  :row 2 :col 4
                  :data '(30)
                  :name "R"))
                (asm-blox--extra-gameboard-cells
                 (asm-blox--problem-spec-create
                  :sources
                  (list source-top source-right)
                  :sinks nil))
                (asm-blox--gameboard
                 (asm-blox-test-create-gameboard
                  '((0 0 "(get up) (get up) (add)")
                    (2 3 "(get right)"))))
                trace)
         (asm-blox--reset-extra-gameboard-cells-state)
         (dotimes (_ 4)
           (asm-blox-test-step)
           (push
            (list
             (asm-blox-test-runtime-summary
              (asm-blox--cell-at-row-col 0 0))
             (asm-blox-test-runtime-summary
              (asm-blox--cell-at-row-col 2 3))
             (asm-blox-test-source-summary source-top)
             (asm-blox-test-source-summary source-right))
            trace))
         (nreverse trace))"##,
        expect![[
            r#"OK (((:row 0 :col 0 :pc 1 :stack #1=(10) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 2 :col 3 :pc 0 :stack #2=(30) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (-1 0 #3=(10 20) "T" 1) (2 4 #4=(30) "R" 1)) ((:row 0 :col 0 :pc 2 :stack (20 . #1#) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 2 :col 3 :pc 0 :stack #2# :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (-1 0 #3# "T" 2) (2 4 #4# "R" 1)) ((:row 0 :col 0 :pc 0 :stack #5=(30) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 2 :col 3 :pc 0 :stack #2# :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (-1 0 #3# "T" 2) (2 4 #4# "R" 1)) ((:row 0 :col 0 :pc 0 :stack #5# :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 2 :col 3 :pc 0 :stack #2# :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (-1 0 #3# "T" 2) (2 4 #4# "R" 1)))"#
        ]],
    )
}

fn asm_blox_sinks_consume_every_board_edge_and_record_first_practical_mismatch() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asm_blox_sinks_consume_every_board_edge_and_record_first_practical_mismatch",
        r##"(let* ((asm-blox--gameboard
                 (asm-blox-test-create-gameboard nil))
                (sinks
                 (list
                  (asm-blox--cell-sink-create
                   :row -1 :col 0
                   :expected-data '(11) :name "T")
                  (asm-blox--cell-sink-create
                   :row 3 :col 1
                   :expected-data '(22) :name "B")
                  (asm-blox--cell-sink-create
                   :row 1 :col -1
                   :expected-data '(33) :name "L")
                  (asm-blox--cell-sink-create
                   :row 2 :col 4
                   :expected-data '(99) :name "R"))))
         (setf
          (asm-blox--cell-runtime-up
           (asm-blox--cell-at-row-col 0 0)) 11
          (asm-blox--cell-runtime-down
           (asm-blox--cell-at-row-col 2 1)) 22
          (asm-blox--cell-runtime-left
           (asm-blox--cell-at-row-col 1 0)) 33
          (asm-blox--cell-runtime-right
           (asm-blox--cell-at-row-col 2 3)) 44)
         (let ((statuses
                (mapcar
                 #'asm-blox--cell-sink-get
                 sinks)))
           (list
            statuses
            (mapcar
             #'asm-blox-test-sink-summary
             sinks)
            asm-blox--gameboard-state
            (mapcar
             (lambda (cell)
               (asm-blox-test-runtime-summary cell))
             (list
              (asm-blox--cell-at-row-col 0 0)
              (asm-blox--cell-at-row-col 2 1)
              (asm-blox--cell-at-row-col 1 0)
              (asm-blox--cell-at-row-col 2 3))))))"##,
        expect![[
            r#"OK ((nil nil nil nil) ((-1 0 (11) "T" 1 nil nil nil nil) (3 1 (22) "B" 1 nil nil nil nil) (1 -1 (33) "L" 1 nil nil nil nil) (2 4 (99) "R" 1 44 nil nil nil)) error ((:row 0 :col 0 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 2 :col 1 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 1 :col 0 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (:row 2 :col 3 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil)))"#
        ]],
    )
}

fn asm_blox_editor_sink_supports_insert_newline_backspace_delete_and_bounded_cursor_workflows()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_editor_sink_supports_insert_newline_backspace_delete_and_bounded_cursor_workflows",
        r##"(let ((sink
                (asm-blox--cell-sink-create
                 :row 1 :col 4
                 :expected-data nil
                 :name "E"
                 :editor-text "alpha beta"
                 :editor-point 7
                 :expected-text "alpha!\nbeta")))
         (let (trace)
           (dolist (operation
                    '((insert 33)
                      (insert 10)
                      (move 100)
                      (insert 63)
                      (insert 8)
                      (move -20)
                      (insert 8)
                      (move 4)
                      (insert -2)))
             (pcase operation
               (`(insert ,character)
                (asm-blox--cell-sink-insert-character
                 sink character))
               (`(move ,point)
                (asm-blox--cell-sink-move-point
                 sink point)))
             (push
              (list
               operation
               (asm-blox--cell-sink-editor-text sink)
               (asm-blox--cell-sink-editor-point sink))
              trace))
           (nreverse trace)))"##,
        expect![[
            r#"OK (((insert 33) "alpha !beta" 8) ((insert 10) "alpha !\nbeta" 9) ((move 100) "alpha !\nbeta" 13) ((insert 63) "alpha !\nbeta?" 14) ((insert 8) "alpha !\nbeta" 13) ((move -20) "alpha !\nbeta" 1) ((insert 8) "alpha !\nbeta" 1) ((move 4) "alpha !\nbeta" 4) ((insert -2) "alpa !\nbeta" 4))"#
        ]],
    )
}

fn asm_blox_reset_restores_sources_numeric_sinks_and_editor_sinks_to_initial_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_reset_restores_sources_numeric_sinks_and_editor_sinks_to_initial_state",
        r##"(let* ((source
                 (asm-blox--cell-source-create
                  :row -1 :col 0
                  :data '(1 2) :idx 2 :name "I"))
                (numeric
                 (asm-blox--cell-sink-create
                  :row 3 :col 0
                  :expected-data '(3)
                  :idx 4 :err-val 99
                  :name "N"))
                (editor-default
                 (asm-blox--cell-sink-create
                  :row 3 :col 1
                  :expected-data nil
                  :idx 2 :err-val 7
                  :name "D"
                  :default-editor-text "seed"
                  :editor-text "changed"
                  :editor-point 5
                  :expected-text "target"))
                (editor-empty
                 (asm-blox--cell-sink-create
                  :row 3 :col 2
                  :expected-data nil
                  :name "E"
                  :editor-text "changed"
                  :editor-point 5
                  :expected-text "target"))
                (asm-blox--extra-gameboard-cells
                 (asm-blox--problem-spec-create
                  :sources (list source)
                  :sinks
                  (list numeric editor-default editor-empty))))
         (asm-blox--reset-extra-gameboard-cells-state)
         (list
          (asm-blox-test-source-summary source)
          (mapcar
           #'asm-blox-test-sink-summary
           (list numeric editor-default editor-empty))))"##,
        expect![[
            r#"OK ((-1 0 (1 2) "I" 0) ((3 0 (3) "N" 0 nil nil nil nil) (3 1 nil "D" 0 nil "seed" 1 "target") (3 2 nil "E" 0 nil "" 1 "target")))"#
        ]],
    )
}

fn asm_blox_winning_conditions_require_complete_clean_data_and_trimmed_editor_line_parity()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_winning_conditions_require_complete_clean_data_and_trimmed_editor_line_parity",
        r##"(let* ((data-sink
                 (asm-blox--cell-sink-create
                  :row 3 :col 0
                  :expected-data '(1 2)
                  :idx 1
                  :name "D"))
                (editor-sink
                 (asm-blox--cell-sink-create
                  :row 3 :col 1
                  :expected-data nil
                  :name "E"
                  :editor-text "alpha  \nbeta   \n"
                  :expected-text "alpha\nbeta"))
                (asm-blox--extra-gameboard-cells
                 (asm-blox--problem-spec-create
                  :sources nil
                  :sinks
                  (list data-sink editor-sink)))
                (wins nil)
                (asm-blox--gameboard-state nil))
         (cl-letf
             (((symbol-function
                'asm-blox--win-file-for-current-buffer)
               (lambda ()
                 (push :win-file wins))))
           (asm-blox-check-winning-conditions)
           (let ((before
                  (list
                   asm-blox--gameboard-state
                   wins)))
             (setf
              (asm-blox--cell-sink-idx data-sink) 2)
             (asm-blox-check-winning-conditions)
             (let ((won
                    (list
                     asm-blox--gameboard-state
                     wins)))
               (setq asm-blox--gameboard-state nil)
               (setf
                (asm-blox--cell-sink-err-val data-sink) 77)
               (asm-blox-check-winning-conditions)
               (list
                before
                won
                (list
                 asm-blox--gameboard-state
                 wins))))))"##,
        expect!["OK ((nil nil) (win #1=(:win-file)) (nil #1#))"],
    )
}

fn asm_blox_display_register_helpers_map_sources_sinks_and_internal_ports_to_correct_edges()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_display_register_helpers_map_sources_sinks_and_internal_ports_to_correct_edges",
        r##"(let* ((source-top
                 (asm-blox--cell-source-create
                  :row -1 :col 1
                  :data '(17) :name "T"))
                (source-left
                 (asm-blox--cell-source-create
                  :row 2 :col -1
                  :data '(23) :name "L"))
                (sink-bottom
                 (asm-blox--cell-sink-create
                  :row 3 :col 2
                  :expected-data '(4) :name "B"))
                (sink-right
                 (asm-blox--cell-sink-create
                  :row 0 :col 4
                  :expected-data '(5) :name "R"))
                (asm-blox--extra-gameboard-cells
                 (asm-blox--problem-spec-create
                  :sources
                  (list source-top source-left)
                  :sinks
                  (list sink-bottom sink-right)))
                (asm-blox--gameboard
                 (asm-blox-test-create-gameboard nil)))
         (asm-blox--reset-extra-gameboard-cells-state)
         (setf
          (asm-blox--cell-runtime-right
           (asm-blox--cell-at-row-col 1 1)) 31
          (asm-blox--cell-runtime-down
           (asm-blox--cell-at-row-col 0 2)) 37)
         (list
          (mapcar
           (lambda (case)
             (list
              case
              (apply
               #'asm-blox--get-direction-col-registers
               case)))
           '((2 0 RIGHT)
             (0 4 LEFT)
             (1 2 RIGHT)
             (1 2 LEFT)))
          (mapcar
           (lambda (case)
             (list
              case
              (apply
               #'asm-blox--get-direction-row-registers
               case)))
           '((0 1 DOWN)
             (3 2 UP)
             (1 2 DOWN)
             (1 2 UP)))
          (mapcar
           (lambda (coords)
             (list
              coords
              (apply
               #'asm-blox--get-source-idx-at-position
               coords)
              (apply
               #'asm-blox--get-sink-name-at-position
               coords)))
           '((-1 1) (2 -1) (3 2) (0 4) (1 1)))))"##,
        expect![[
            r#"OK ((((2 0 RIGHT) 23) ((0 4 LEFT) nil) ((1 2 RIGHT) 31) ((1 2 LEFT) nil)) (((0 1 DOWN) 17) ((3 2 UP) nil) ((1 2 DOWN) 37) ((1 2 UP) nil)) (((-1 1) "T" nil) ((2 -1) "L" nil) ((3 2) nil "B") ((0 4) nil "R") ((1 1) nil nil)))"#
        ]],
    )
}

pub(super) fn sources_sinks_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asm_blox_sources_peek_and_pop_zero_negative_and_exhausted_values_in_order(),
        asm_blox_boundary_sources_feed_real_get_instructions_and_advance_only_when_consumed(),
        asm_blox_sinks_consume_every_board_edge_and_record_first_practical_mismatch(),
        asm_blox_editor_sink_supports_insert_newline_backspace_delete_and_bounded_cursor_workflows(
        ),
        asm_blox_reset_restores_sources_numeric_sinks_and_editor_sinks_to_initial_state(),
        asm_blox_winning_conditions_require_complete_clean_data_and_trimmed_editor_line_parity(),
        asm_blox_display_register_helpers_map_sources_sinks_and_internal_ports_to_correct_edges(),
    ]
}
