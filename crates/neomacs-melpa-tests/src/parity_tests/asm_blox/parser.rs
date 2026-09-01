use expect_test::expect;

use super::ParityBatchCase;

fn asm_blox_parser_handles_nested_commands_comments_numbers_symbols_chars_and_positions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_parser_handles_nested_commands_comments_numbers_symbols_chars_and_positions",
        r##"(mapcar
         #'asm-blox-test-code-summary
         (asm-blox--parse-assembly
          "; calculate and route\n(const -999)\n(send right (add (const 12) (const ?A)))\n(const ?\\n)\n(const ?\\s)\n(const ?\\b)"))"##,
        expect![
            "OK (((CONST -999) 23 35) ((SEND RIGHT ((ADD ((CONST 12) 53 63) ((CONST 65) 64 74)) 48 75)) 36 76) ((CONST 10) 77 88) ((CONST 32) 89 100) ((CONST 8) 101 112))"
        ],
    )
}

fn asm_blox_parser_reports_precise_malformed_input_boundaries_and_messages() -> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_parser_reports_precise_malformed_input_boundaries_and_messages",
        r##"(mapcar
         (lambda (source)
           (list
            source
            (asm-blox--parse-assembly source)))
         '(")"
           "(const 1"
           "(const 1000)"
           "(const -1000)"
           "(const ? )"
           "(const ?\\x)"
           "(const 1 @)"
           "(const -)"
           "((const 1))"))"##,
        expect![[
            r#"OK ((")" (error 1 "SYNTAX ERROR")) ("(const 1" (error 9 "SYNTAX ERROR")) ("(const 1000)" (error 12 "TOO HIGH NUMBER")) ("(const -1000)" (error 13 "TOO LOW NUMBER")) ("(const ? )" (error 9 "INVALID CHAR")) ("(const ?\\x)" (error 10 "BAD ESCAPE CODE")) ("(const 1 @)" (error 10 "unexpected character")) ("(const -)" (#s(asm-blox-code-node (CONST 0) 1 10))) ("((const 1))" (#s(asm-blox-code-node (#s(asm-blox-code-node (CONST 1) 2 11)) 1 12))))"#
        ]],
    )
}

fn asm_blox_cell_parser_dispatches_empty_wat_yaml_and_sexp_cells_into_runtime_shapes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_cell_parser_dispatches_empty_wat_yaml_and_sexp_cells_into_runtime_shapes",
        r##"(let ((cases
                (list
                 ""
                 "; only a comment"
                 "(const 7)"
                 "apiVersion: v1\nkind: Stack\nspec:\n  inputPort: left\n  outputPort: right\n  size: 4\n"
                 "(module stack :input-port left :output-port right :size 4)")))
         (mapcar
          (lambda (source)
            (let ((result
                   (asm-blox--parse-cell '(1 2) source)))
              (if
                  (asm-blox--cell-runtime-p result)
                  (list
                   (asm-blox-test-runtime-summary result)
                   (mapcar
                    #'asm-blox-test-instruction-summary
                    (asm-blox--cell-runtime-instructions result))
                   (asm-blox--cell-runtime-run-function result)
                   (asm-blox--cell-runtime-message-function result)
                   (asm-blox--cell-runtime-run-spec result))
                result)))
          cases))"##,
        expect![[
            r#"OK (((:row 1 :col 2 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) nil nil nil nil) ((:row 1 :col 2 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) nil nil nil nil) ((:row 1 :col 2 :pc 0 :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (((CONST 7) 1 10)) nil nil nil) ((:row 1 :col 2 :pc nil :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) nil asm-blox--yaml-step-stack asm-blox--yaml-message-stack ((inputPort . "left") (outputPort . "right") (size . 4))) ((:row 1 :col 2 :pc nil :stack nil :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) nil asm-blox--yaml-step-stack asm-blox--yaml-message-stack ((size . 4) (outputPort . "right") (inputPort . "left"))))"#
        ]],
    )
}

fn asm_blox_command_validator_accepts_real_programs_and_rejects_bad_arity_type_and_names()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_command_validator_accepts_real_programs_and_rejects_bad_arity_type_and_names",
        r##"(mapcar
         (lambda (source)
           (let* ((parsed
                   (car
                    (asm-blox--parse-assembly source)))
                  (result
                   (asm-blox--code-node-validate parsed)))
             (list source result)))
         '("(const 7)"
           "(send right (add (const 2) (const 3)))"
           "(get -1)"
           "(get left)"
           "(clr)"
           "(const)"
           "(const left)"
           "(const 1 2)"
           "(send diagonal)"
           "(send right 8)"
           "(unknown 1)"
           "()"))"##,
        expect![[
            r#"OK (("(const 7)" nil) ("(send right (add (const 2) (const 3)))" nil) ("(get -1)" nil) ("(get left)" nil) ("(clr)" nil) ("(const)" (error 1 "not enough args")) ("(const left)" (error 1 "bad arg to 'CONST'")) ("(const 1 2)" (error 1 "too many args")) ("(send diagonal)" (error 1 "bad arg to 'SEND'")) ("(send right 8)" (error 1 "bad end expressions")) ("(unknown 1)" (error 1 "Command not found")) ("()" (error 1 "No command found")))"#
        ]],
    )
}

fn asm_blox_problem_banned_commands_are_enforced_during_practical_cell_compilation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_problem_banned_commands_are_enforced_during_practical_cell_compilation",
        r##"(let ((asm-blox--extra-gameboard-cells
                (asm-blox--problem-spec-create
                 :name "No shortcuts"
                 :banned-commands '(MUL DIV REM))))
         (mapcar
          (lambda (source)
            (let ((result
                   (asm-blox--parse-cell '(0 0) source)))
              (if
                  (asm-blox--cell-runtime-p result)
                  (mapcar
                   #'asm-blox-test-instruction-summary
                   (asm-blox--cell-runtime-instructions result))
                result)))
          '("(add (const 2) (const 3))"
            "(mul (const 2) (const 3))"
            "(div (const 8) (const 2))"
            "(rem (const 8) (const 3))")))"##,
        expect![[
            r#"OK ((((CONST 2) 6 15) ((CONST 3) 16 25) ((ADD #s(asm-blox-code-node (CONST 2) 6 15) #s(asm-blox-code-node (CONST 3) 16 25)) 1 26)) (error 1 . #1=("Command banned")) (error 1 . #1#) (error 1 . #1#))"#
        ]],
    )
}

fn asm_blox_compiler_flattens_subexpressions_in_execution_order_and_preserves_source_spans()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_compiler_flattens_subexpressions_in_execution_order_and_preserves_source_spans",
        r##"(let* ((tree
                 (asm-blox--parse-assembly
                  "(send right (add (const 12) (mul (const 3) (const 4))))"))
                (assembly
                 (asm-blox--parse-tree-to-asm tree)))
         (mapcar
          #'asm-blox-test-instruction-summary
          assembly))"##,
        expect![
            "OK (((CONST 12) 18 28) ((CONST 3) 34 43) ((CONST 4) 44 53) ((MUL #1=#s(asm-blox-code-node (CONST 3) 34 43) #2=#s(asm-blox-code-node (CONST 4) 44 53)) 29 54) ((ADD #3=#s(asm-blox-code-node (CONST 12) 18 28) #4=#s(asm-blox-code-node (MUL #1# #2#) 29 54)) 13 55) ((SEND RIGHT #s(asm-blox-code-node (ADD #3# #4#) 13 55)) 1 56))"
        ],
    )
}

fn asm_blox_compiler_resolves_nested_block_loop_and_branch_targets_deterministically()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_compiler_resolves_nested_block_loop_and_branch_targets_deterministically",
        r##"(cl-letf
         (((symbol-function 'random)
           #'asm-blox-test-random))
         (let* ((asm-blox-test-random-values
                 '(11 22 33 44 55 66))
                (tree
                 (asm-blox--parse-assembly
                  "(block (const 3) (loop (dec -1) (dup) (br_if 0)) (br 0) (const 99))"))
                (assembly
                 (asm-blox--parse-tree-to-asm tree)))
           (mapcar
            #'asm-blox-test-instruction-summary
            assembly)))"##,
        expect![
            "OK (((CONST 3) 8 17) ((LABEL L_22_2) nil nil) ((DEC -1) 24 32) ((DUP) 33 38) ((JMP_IF 1) 39 48) ((JMP 7) 50 56) ((CONST 99) 57 67) ((LABEL L_11_1) nil nil))"
        ],
    )
}

fn asm_blox_branch_compiler_rejects_out_of_scope_labels_at_nested_depths() -> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_branch_compiler_rejects_out_of_scope_labels_at_nested_depths",
        r##"(cl-letf
         (((symbol-function 'random)
           #'asm-blox-test-random))
         (let ((asm-blox-test-random-values
                '(1 2 3 4)))
           (mapcar
            (lambda (source)
              (list
               source
               (asm-blox--parse-tree-to-asm
                (asm-blox--parse-assembly source))))
            '("(br 0)"
              "(block (br 1))"
              "(loop (br_if 2))"
              "(block (loop (br 1)))"))))"##,
        expect![[
            r#"OK (("(br 0)" (error 1 . #1=("Label not found"))) ("(block (br 1))" (error 8 . #1#)) ("(loop (br_if 2))" (error 7 "Label not found")) ("(block (loop (br 1)))" (#s(asm-blox-code-node (LABEL L_4_2) nil nil) #s(asm-blox-code-node (JMP 2) 14 20) #s(asm-blox-code-node (LABEL L_3_1) nil nil))))"#
        ]],
    )
}

fn asm_blox_label_resolver_mutates_all_jump_kinds_and_leaves_other_instructions_intact()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_label_resolver_mutates_all_jump_kinds_and_leaves_other_instructions_intact",
        r##"(let* ((assembly
                 (mapcar
                  (lambda (children)
                    (asm-blox--code-node-create
                     :children children
                     :start-pos 1
                     :end-pos 2))
                  '((LABEL alpha)
                    (CONST 7)
                    (JMP beta)
                    (JMP_IF alpha)
                    (LABEL beta)
                    (JMP_IF_NOT beta)
                    (SEND RIGHT)))))
         (asm-blox--resolve-labels assembly)
         (mapcar
          #'asm-blox-test-instruction-summary
          assembly))"##,
        expect![
            "OK (((LABEL alpha) 1 2) ((CONST 7) 1 2) ((JMP 4) 1 2) ((JMP_IF 0) 1 2) ((LABEL beta) 1 2) ((JMP_IF_NOT 4) 1 2) ((SEND RIGHT) 1 2))"
        ],
    )
}

fn asm_blox_flatten_and_port_predicates_cover_proper_improper_nil_string_and_symbol_inputs()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_flatten_and_port_predicates_cover_proper_improper_nil_string_and_symbol_inputs",
        r##"(list
         (mapcar
          (lambda (tree)
            (asm-blox--flatten-list tree))
          '(((1 (2 nil (3)) 4))
            (a (b . c) d)
            nil
            (1 2 . 3)))
         (mapcar
          (lambda (port)
            (list
             port
             (asm-blox--portp port)))
          '(UP DOWN LEFT RIGHT
            up down
            "UP" "RIGHT" "up" "diagonal"
            nil 1)))"##,
        expect![[
            r#"OK (((1 2 3 4) (a b c d) nil (1 2 3)) ((UP (UP . #1=(DOWN . #2=(LEFT . #3=(RIGHT))))) (DOWN #1#) (LEFT #2#) (RIGHT #3#) (up nil) (down nil) ("UP" (UP DOWN LEFT . #4=(RIGHT))) ("RIGHT" #4#) ("up" nil) ("diagonal" nil) (nil nil) (1 nil)))"#
        ]],
    )
}

pub(super) fn parser_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asm_blox_parser_handles_nested_commands_comments_numbers_symbols_chars_and_positions(),
        asm_blox_parser_reports_precise_malformed_input_boundaries_and_messages(),
        asm_blox_cell_parser_dispatches_empty_wat_yaml_and_sexp_cells_into_runtime_shapes(),
        asm_blox_command_validator_accepts_real_programs_and_rejects_bad_arity_type_and_names(),
        asm_blox_problem_banned_commands_are_enforced_during_practical_cell_compilation(),
        asm_blox_compiler_flattens_subexpressions_in_execution_order_and_preserves_source_spans(),
        asm_blox_compiler_resolves_nested_block_loop_and_branch_targets_deterministically(),
        asm_blox_branch_compiler_rejects_out_of_scope_labels_at_nested_depths(),
        asm_blox_label_resolver_mutates_all_jump_kinds_and_leaves_other_instructions_intact(),
        asm_blox_flatten_and_port_predicates_cover_proper_improper_nil_string_and_symbol_inputs(),
    ]
}
