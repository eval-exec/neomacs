use expect_test::expect;

use super::ParityBatchCase;

fn asm_blox_puzzle_registry_has_complete_ordered_names_difficulties_io_shapes_and_bans()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_puzzle_registry_has_complete_ordered_names_difficulties_io_shapes_and_bans",
        r##"(mapcar
         (lambda (generator)
           (random
            (concat
             "asm-blox-corpus:"
             (symbol-name generator)))
           (asm-blox-test-problem-shape
            (funcall generator)))
         asm-blox-puzzles)"##,
        expect![[
            r#"OK (("Indentation I" medium nil ((1 5 0 "O" nil 54)) nil) ("Constant Generator" tutorial nil ((0 4 40 "N" nil nil)) nil) ("Identity" tutorial ((-1 0 40 "X")) ((3 3 40 "X" nil nil)) nil) ("Number Addition" easy ((-1 0 40 "A") (-1 1 40 "B") (-1 2 40 "C")) ((3 1 40 "S" nil nil)) nil) ("Number Filter" easy ((1 -1 40 "I")) ((1 4 40 "O" nil nil)) nil) ("Number Sum" easy ((-1 3 40 "I")) ((2 4 40 "O" nil nil)) nil) ("Number Chooser" easy ((-1 0 40 "A") (-1 1 40 "B")) ((0 4 40 "L" nil nil) (2 4 40 "R" nil nil)) nil) ("Clock Hours" easy ((1 -1 40 "H")) ((3 1 40 "T" nil nil)) nil) ("List Length" medium ((1 -1 31 "I")) ((1 4 5 "O" nil nil)) nil) ("List Reverse" medium ((-1 2 40 "L")) ((3 1 40 "R" nil nil)) nil) ("Increment Cout" medium ((1 -1 40 "I")) ((1 4 1 "O" nil nil)) nil) ("Upcase" easy ((1 -1 40 "C")) ((1 4 40 "O" nil nil)) nil) ("Merge Step" hard ((0 -1 20 "A") (2 -1 20 "B")) ((1 4 40 "C" nil nil)) nil) ("Editor Basics" easy nil ((1 5 0 "O" 2 11)) nil) ("Simple Graph" medium ((1 -1 10 "A")) ((1 5 0 "O" 0 52)) nil) ("Meeting point" hard ((1 -1 10 "N")) ((2 4 1 "O" nil nil)) nil) ("Turing" hard ((0 -1 17 "X")) ((1 4 6 "O" nil nil)) nil) ("Stack Machine" hard ((0 -1 11 "O") (0 4 40 "A")) ((3 1 2 "T" nil nil)) nil) ("Delete Word" hard ((-1 3 1 "I")) ((1 5 0 "O" nil 52)) nil) ("Triangle Area" easy ((3 2 40 "B") (1 4 40 "H")) ((-1 1 40 "A" nil nil)) nil) ("Diagnostic Test" tutorial ((0 -1 40 "A") (2 -1 40 "B")) ((0 4 40 "X" nil nil) (2 4 40 "Y" nil nil)) nil) ("Signal Amplifier" tutorial ((-1 2 40 "I")) ((3 1 40 "O" nil nil)) nil) ("Differential Converter" tutorial ((-1 1 40 "A") (-1 2 40 "B")) ((3 1 40 "P" nil nil) (3 2 40 "N" nil nil)) nil) ("Signal Comparator" tutorial ((-1 0 40 "I")) ((3 1 40 "G" nil nil) (3 2 40 "E" nil nil) (3 3 40 "L" nil nil)) nil) ("Sequence Generator" easy ((-1 1 8 "A") (-1 2 8 "B")) ((3 2 24 "O" nil nil)) nil) ("Sequence Counter" easy ((-1 1 40 "I")) ((3 2 3 "S" nil nil) (3 3 3 "L" nil nil)) nil) ("Signal Edge Detector" easy ((-1 1 40 "I")) ((3 2 40 "O" nil nil)) nil) ("Interrupt Handler" easy ((-1 0 40 "1") (-1 1 40 "2") (-1 2 40 "3") (-1 3 40 "4")) ((3 2 39 "O" nil nil)) nil) ("Signal Pattern Detector" medium ((-1 1 40 "I")) ((3 2 40 "O" nil nil)) nil) ("Sequence Peak Detector" medium ((-1 1 40 "I")) ((3 1 7 "N" nil nil) (3 2 7 "X" nil nil)) nil) ("Sequence Reverser" medium ((-1 1 40 "I")) ((3 2 40 "R" nil nil)) nil) ("Signal Multiplier" medium ((-1 1 40 "A") (-1 2 40 "B")) ((3 2 40 "M" nil nil)) (MUL)) ("Signal Window Filter" hard ((-1 1 40 "I")) ((3 1 40 "3" nil nil) (3 2 40 "5" nil nil)) nil) ("Signal Divider" hard ((-1 1 40 "A") (-1 2 40 "B")) ((3 1 40 "R" nil nil) (3 2 40 "Q" nil nil)) (DIV REM)) ("Sequence Indexer" hard ((-1 1 11 "D") (-1 2 40 "X")) ((3 1 40 "V" nil nil)) nil) ("Sequence Sorter" hard ((-1 1 40 "I")) ((3 2 40 "O" nil nil)) nil))"#
        ]],
    )
}

fn asm_blox_list_helpers_split_terminated_streams_and_generate_bounded_practical_sequences()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_list_helpers_split_terminated_streams_and_generate_bounded_practical_sequences",
        r##"(list
         (mapcar
          #'asm-blox-puzzles-list-of-lists-to-lisp
          '((1 2 0 3 0)
            (0 0)
            (5 6 7 0)
            (1 0 2 3 0 4 5 6 0)))
         (cl-letf
             (((symbol-function 'random)
               #'asm-blox-test-random))
           (let ((asm-blox-test-random-values
                  (number-sequence 0 200)))
             (let ((stream
                    (asm-blox-puzzles-random-list-of-lists
                     7)))
               (list
                stream
                (length stream)
                (seq-every-p
                 (lambda (value)
                   (<= 0 value 7))
                 stream)
                (asm-blox-puzzles-list-of-lists-to-lisp
                 stream))))))"##,
        expect![
            "OK ((((1 2) (3)) (nil nil) ((5 6 7)) ((1) (2 3) (4 5 6))) ((1 2 3 4 5 0 7 1 2 3 0 5 6 7 1 0 3 4 5 6 0 1 2 3 4 0 6 7 1 2 0 4 5 6 7 0 2 3 4 0) 40 t ((1 2 3 4 5) (7 1 2 3) (5 6 7 1) (3 4 5 6) (1 2 3 4) (6 7 1 2) (4 5 6 7) (2 3 4))))"
        ],
    )
}

fn asm_blox_stack_machine_solver_models_push_add_negate_and_emit_operation_streams()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_stack_machine_solver_models_push_add_negate_and_emit_operation_streams",
        r##"(mapcar
         (lambda (case)
           (list
            case
            (apply
             #'asm-blox-puzzles--stack-machine-solver
             case)))
         '(((1 2 3 4) (0 0 0 0))
           ((1 2 3 4) (1 0 0 0))
           ((1 2 3 4) (2 0 2 0))
           ((5 6 7 8 9) (1 2 1 0))
           ((10 -3 4) (2 1 0))))"##,
        expect![
            "OK ((((1 2 3 4) (0 0 0 0)) (4 3 2 1)) (((1 2 3 4) (1 0 0 0)) (7 2 1)) (((1 2 3 4) (2 0 2 0)) (-4 -3)) (((5 6 7 8 9) (1 2 1 0)) (-10)) (((10 -3 4) (2 1 0)) (-7)))"
        ],
    )
}

fn asm_blox_interrupt_solver_emits_rising_edge_port_numbers_and_rejects_simultaneous_edges()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_interrupt_solver_emits_rising_edge_port_numbers_and_rejects_simultaneous_edges",
        r##"(mapcar
         (lambda (signals)
           (list
            signals
            (apply
             #'asm-blox-puzzles--make-interrupt-handler-solution
             signals)))
         '(((0 1 1 0 0)
             (0 0 0 0 0)
             (0 0 0 1 1)
             (0 0 0 0 0))
           ((0 1 1)
             (0 1 1)
             (0 0 0)
             (0 0 0))
           ((1 1 0 1)
             (0 0 0 0)
             (0 0 1 1)
             (0 0 0 0))
           ((0 0 0)
             (0 0 0)
             (0 0 0)
             (0 0 0))))"##,
        expect![
            "OK ((((0 1 1 0 0) (0 0 0 0 0) (0 0 0 1 1) (0 0 0 0 0)) (1 0 3 0)) (((0 1 1) (0 1 1) (0 0 0) (0 0 0)) nil) (((1 1 0 1) (0 0 0 0) (0 0 1 1) (0 0 0 0)) (0 3 1)) (((0 0 0) (0 0 0) (0 0 0) (0 0 0)) (0 0)))"
        ],
    )
}

fn asm_blox_arithmetic_generators_publish_outputs_derived_from_their_actual_seeded_inputs()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_arithmetic_generators_publish_outputs_derived_from_their_actual_seeded_inputs",
        r##"(mapcar
         (lambda (generator)
           (random
            (concat
             "asm-blox-arithmetic:"
             (symbol-name generator)))
           (let* ((problem
                   (funcall generator))
                  (sources
                   (asm-blox--problem-spec-sources problem))
                  (sinks
                   (asm-blox--problem-spec-sinks problem)))
             (list
              (asm-blox--problem-spec-name problem)
              (mapcar
               #'asm-blox--cell-source-data
               sources)
              (mapcar
               #'asm-blox--cell-sink-expected-data
               sinks))))
         '(asm-blox-puzzles--triangle-area
           asm-blox-puzzles--number-sum
           asm-blox-puzzles--add
           asm-blox-puzzles--differential-converter
           asm-blox-puzzles--signal-amplifier
           asm-blox-puzzles--signal-multiplier
           asm-blox-puzzles--signal-divider))"##,
        expect![[
            r#"OK (("Triangle Area" ((16 8 18 10 10 16 14 16 2 16 16 4 6 14 4 2 10 12 14 18 16 10 12 18 4 16 12 8 10 8 4 16 10 16 14 20 4 10 12 20) (9 4 15 17 9 4 8 15 6 5 15 1 15 14 7 17 6 15 1 8 5 2 17 1 16 9 12 12 1 4 8 14 13 3 10 6 13 15 12 7)) ((72 16 135 85 45 32 56 120 6 40 120 2 45 98 14 17 30 90 7 72 40 10 102 9 32 72 72 48 5 16 16 112 65 24 70 60 26 75 72 70))) ("Number Sum" ((10 10 9 2 1 4 7 6 5 2 10 6 4 6 6 2 8 9 8 5 8 4 1 1 2 7 6 1 5 5 5 2 9 9 7 9 5 1 10 10)) ((55 55 45 3 1 10 28 21 15 3 55 21 10 21 21 3 36 45 36 15 36 10 1 1 3 28 21 1 15 15 15 3 45 45 28 45 15 1 55 55))) ("Number Addition" ((7 4 7 1 9 2 4 2 0 6 5 2 8 8 8 1 4 4 6 9 9 7 6 7 3 4 2 3 7 8 3 1 3 3 2 9 9 5 0 6) (0 2 7 2 4 3 7 0 9 6 7 3 1 4 6 5 3 1 7 5 7 2 8 9 8 5 1 9 6 7 8 7 0 8 3 0 9 0 5 1) (0 6 3 0 4 5 0 3 3 1 1 7 8 7 8 9 0 6 2 0 1 8 4 1 5 9 2 0 7 1 4 2 7 5 4 7 3 5 1 5)) ((7 12 17 3 17 10 11 5 12 13 13 12 17 19 22 15 7 11 15 14 17 17 18 17 16 18 5 12 20 16 15 10 10 16 9 16 21 10 6 12))) ("Differential Converter" ((3 4 0 1 4 4 3 6 2 2 4 3 9 5 5 0 6 8 0 0 6 4 0 9 0 9 2 2 4 9 0 4 5 4 8 6 0 6 6 5) (3 9 9 4 4 6 3 7 4 6 9 8 3 7 5 1 5 4 4 3 7 1 5 6 7 5 4 6 1 4 8 5 6 1 7 6 5 9 7 1)) ((0 -5 -9 -3 0 -2 0 -1 -2 -4 -5 -5 6 -2 0 -1 1 4 -4 -3 -1 3 -5 3 -7 4 -2 -4 3 5 -8 -1 -1 3 1 0 -5 -3 -1 4) (0 5 9 3 0 2 0 1 2 4 5 5 -6 2 0 1 -1 -4 4 3 1 -3 5 -3 7 -4 2 4 -3 -5 8 1 1 -3 -1 0 5 3 1 -4))) ("Signal Amplifier" ((6 9 5 5 0 0 6 9 1 9 3 2 1 2 5 3 9 7 7 9 3 8 8 3 8 4 3 9 8 4 3 8 0 3 8 4 1 9 9 9)) ((12 18 10 10 0 0 12 18 2 18 6 4 2 4 10 6 18 14 14 18 6 16 16 6 16 8 6 18 16 8 6 16 0 6 16 8 2 18 18 18))) ("Signal Multiplier" ((0 9 2 1 7 8 2 7 8 4 6 1 4 7 5 5 4 9 9 2 1 3 7 4 0 2 1 3 0 7 6 0 3 6 1 6 9 5 5 1) (5 1 3 6 6 4 3 7 7 1 1 1 1 8 0 6 4 5 9 2 2 7 9 3 1 4 7 4 8 3 7 6 6 2 6 0 0 3 6 4)) ((0 9 6 6 42 32 6 49 56 4 6 1 4 56 0 30 16 45 81 4 2 21 63 12 0 8 7 12 0 21 42 0 18 12 6 0 0 15 30 4))) ("Signal Divider" ((10 14 12 13 15 17 18 19 12 17 19 15 15 14 10 16 18 17 14 12 11 14 10 17 16 11 15 17 19 11 17 10 16 14 13 19 17 16 10 18) (6 5 9 3 6 3 5 11 3 5 4 8 11 8 10 11 5 8 10 5 5 11 4 5 3 2 2 4 4 3 10 7 8 8 5 7 11 7 7 11)) ((4 4 3 1 3 2 3 8 0 2 3 7 4 6 0 5 3 1 4 2 1 3 2 2 1 1 1 1 3 2 7 3 0 6 3 5 6 2 3 7) (1 2 1 4 2 5 3 1 4 3 4 1 1 1 1 1 3 2 1 2 2 1 2 3 5 5 7 4 4 3 1 1 2 1 2 2 1 2 1 1))))"#
        ]],
    )
}

fn asm_blox_sequence_generators_publish_sort_reverse_peak_window_and_index_outputs_for_real_inputs()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_sequence_generators_publish_sort_reverse_peak_window_and_index_outputs_for_real_inputs",
        r##"(mapcar
         (lambda (generator)
           (random
            (concat
             "asm-blox-sequence:"
             (symbol-name generator)))
           (let* ((problem
                   (funcall generator))
                  (sources
                   (asm-blox--problem-spec-sources problem))
                  (sinks
                   (asm-blox--problem-spec-sinks problem)))
             (list
              (asm-blox--problem-spec-name problem)
              (mapcar
               #'asm-blox--cell-source-data
               sources)
              (mapcar
               #'asm-blox--cell-sink-expected-data
               sinks))))
         '(asm-blox-puzzles--list-reverse
           asm-blox-puzzles--sequence-generator
           asm-blox-puzzles--sequence-counter
           asm-blox-puzzles--sequence-peak-detector
           asm-blox-puzzles--sequence-reverser
           asm-blox-puzzles--signal-window-filter
           asm-blox-puzzles--sequence-indexer
           asm-blox-puzzles--sequence-sorter))"##,
        expect![[
            r#"OK (("List Reverse" ((372 8 0 221 485 0 558 939 259 587 507 342 0 292 151 0 299 150 22 207 629 226 82 0 436 400 0 626 23 463 0 431 889 943 567 153 223 723 390 0)) ((8 372 0 485 221 0 342 507 587 259 939 558 0 151 292 0 82 226 629 207 22 150 299 0 400 436 0 463 23 626 0 390 723 223 153 567 943 889 431 0))) ("Sequence Generator" ((5 17 11 7 19 5 9 17) (2 16 4 6 18 16 18 0)) ((2 5 0 16 17 0 4 11 0 6 7 0 18 19 0 5 16 0 9 18 0 0 17 0))) ("Sequence Counter" ((14 0 17 28 26 0 8 2 23 0 14 0 5 20 13 13 21 22 23 0 25 5 27 3 21 23 24 27 22 19 6 26 0 16 26 0 13 14 25 0)) ((14 71 33 14 117 228 42 52) (1 3 3 1 7 12 2 3))) ("Sequence Peak Detector" ((44 0 64 0 54 75 93 41 44 61 92 47 8 8 0 52 72 57 63 63 0 61 41 76 0 35 32 0 44 58 48 69 0 9 88 71 35 95 91 0)) ((44 64 8 52 41 32 44 9) (44 64 93 72 76 35 69 95))) ("Sequence Reverser" ((11 81 0 80 58 0 47 1 18 73 48 81 45 34 0 76 40 1 75 0 24 44 42 10 43 6 54 44 40 15 99 0 70 43 34 69 0 73 51 0)) ((81 11 0 58 80 0 34 45 81 48 73 18 1 47 0 75 1 40 76 0 99 15 40 44 54 6 43 10 42 44 24 0 69 34 43 70 0 51 73 0))) ("Signal Window Filter" ((6 6 3 3 3 8 9 4 6 4 8 7 9 7 7 4 1 5 9 6 5 1 0 5 6 2 9 4 6 5 9 7 3 4 2 5 2 9 7 3)) ((6 12 15 12 9 14 20 21 19 14 18 19 24 23 23 18 12 10 15 20 20 12 6 6 11 13 17 15 19 15 20 21 19 14 9 11 9 16 18 19) (6 12 15 18 21 23 26 27 30 31 31 29 34 35 38 34 28 24 26 25 26 26 21 17 17 14 22 26 27 26 33 31 30 28 25 21 16 22 25 26))) ("Sequence Indexer" ((758 308 671 301 722 636 365 548 636 589 0) (5 5 5 3 2 4 4 1 6 6 2 2 5 1 6 7 3 9 7 8 3 0 6 4 6 1 7 9 2 3 0 6 6 1 9 1 9 5 2 6)) ((636 636 636 301 671 722 722 308 365 365 671 671 636 308 365 548 301 589 548 636 301 758 365 722 365 308 548 589 671 301 758 365 365 308 589 308 589 636 671 365))) ("Sequence Sorter" ((75 0 11 30 26 85 14 54 40 16 45 87 36 0 88 57 0 86 46 0 84 6 70 10 0 30 36 15 4 86 47 32 93 22 89 76 33 15 99 0)) ((75 0 11 14 16 26 30 36 40 45 54 85 87 0 57 88 0 46 86 0 6 10 70 84 0 4 15 15 22 30 32 33 36 47 76 86 89 93 99 0))))"#
        ]],
    )
}

fn asm_blox_editor_puzzles_publish_concrete_initial_cursor_and_target_text_workflows()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_editor_puzzles_publish_concrete_initial_cursor_and_target_text_workflows",
        r##"(mapcar
         (lambda (generator)
           (random
            (concat
             "asm-blox-editor:"
             (symbol-name generator)))
           (let* ((problem
                   (funcall generator))
                  (sink
                   (car
                    (asm-blox--problem-spec-sinks problem))))
             (list
              (asm-blox--problem-spec-name problem)
              (mapcar
               #'asm-blox--cell-source-data
               (asm-blox--problem-spec-sources problem))
              (asm-blox--cell-sink-default-editor-text sink)
              (asm-blox--cell-sink-editor-text sink)
              (asm-blox--cell-sink-editor-point sink)
              (asm-blox--cell-sink-expected-text sink))))
         '(asm-blox-puzzles--delete-word
           asm-blox-puzzles--indentation
           asm-blox-puzzles--simple-graph
           asm-blox-puzzles--hello-world))"##,
        expect![[
            r#####"OK (("Delete Word" ((1)) "lamp camera square\nmouse thought case\ncase book thought" nil 0 "lamp  square\nmouse thought case\ncase book thought") ("Indentation I" nil "func main () {\nfmt.Println(\"hello world\")\nreturn\n}" nil 1 "func main () {\n  fmt.Println(\"hello world\")\n  return\n}") ("Simple Graph" ((4 10 4 7 4 7 10 9 3 3)) nil "" 1 "####\n##########\n####\n#######\n####\n#######\n##########\n#########\n###\n###") ("Editor Basics" nil nil "01" 3 "Hello World"))"#####
        ]],
    )
}

fn asm_blox_puzzle_lookup_difficulty_sorting_and_color_mapping_operate_on_real_registry()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_puzzle_lookup_difficulty_sorting_and_color_mapping_operate_on_real_registry",
        r##"(let ((names
                '("Constant Generator"
                  "Identity"
                  "Number Addition"
                  "Sequence Sorter"
                  "Missing Puzzle")))
         (list
          (mapcar
           (lambda (name)
             (let ((generator
                    (asm-blox--get-puzzle-by-id name)))
               (list
                name
                (and generator
                     (symbol-name generator)))))
           names)
          (mapcar
           (lambda (difficulty)
             (list
              difficulty
              (asm-blox--font-for-difficulty
               difficulty)))
           '(tutorial easy medium hard unknown))
          (mapcar
           (lambda (generator)
             (random
              (concat
               "asm-blox-order:"
               (symbol-name generator)))
             (let ((problem
                    (funcall generator)))
               (list
                (asm-blox--problem-spec-difficulty problem)
                (asm-blox--problem-spec-name problem))))
           (asm-blox--puzzles-by-difficulty))))"##,
        expect![[
            r#"OK ((("Constant Generator" "asm-blox-puzzles--constant") ("Identity" "asm-blox-puzzles--identity") ("Number Addition" "asm-blox-puzzles--add") ("Sequence Sorter" "asm-blox-puzzles--sequence-sorter") ("Missing Puzzle" nil)) ((tutorial "LavenderBlush2") (easy "forest green") (medium "goldenrod") (hard "orange red") (unknown nil)) ((tutorial "Constant Generator") (tutorial "Identity") (tutorial "Diagnostic Test") (tutorial "Signal Amplifier") (tutorial "Differential Converter") (tutorial "Signal Comparator") (easy "Number Addition") (easy "Number Filter") (easy "Number Sum") (easy "Number Chooser") (easy "Clock Hours") (easy "Upcase") (easy "Editor Basics") (easy "Triangle Area") (easy "Sequence Generator") (easy "Sequence Counter") (easy "Signal Edge Detector") (easy "Interrupt Handler") (medium "Indentation I") (medium "List Length") (medium "List Reverse") (medium "Increment Cout") (medium "Simple Graph") (medium "Signal Pattern Detector") (medium "Sequence Peak Detector") (medium "Sequence Reverser") (medium "Signal Multiplier") (hard "Merge Step") (hard "Meeting point") (hard "Turing") (hard "Stack Machine") (hard "Delete Word") (hard "Signal Window Filter") (hard "Signal Divider") (hard "Sequence Indexer") (hard "Sequence Sorter")))"#
        ]],
    )
}

pub(super) fn puzzles_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asm_blox_puzzle_registry_has_complete_ordered_names_difficulties_io_shapes_and_bans(),
        asm_blox_list_helpers_split_terminated_streams_and_generate_bounded_practical_sequences(),
        asm_blox_stack_machine_solver_models_push_add_negate_and_emit_operation_streams(),
        asm_blox_interrupt_solver_emits_rising_edge_port_numbers_and_rejects_simultaneous_edges(),
        asm_blox_arithmetic_generators_publish_outputs_derived_from_their_actual_seeded_inputs(),
        asm_blox_sequence_generators_publish_sort_reverse_peak_window_and_index_outputs_for_real_inputs(),
        asm_blox_editor_puzzles_publish_concrete_initial_cursor_and_target_text_workflows(),
        asm_blox_puzzle_lookup_difficulty_sorting_and_color_mapping_operate_on_real_registry(),
    ]
}
