use expect_test::expect;

use super::ParityBatchCase;

fn asm_blox_saved_puzzle_ids_and_next_filename_ignore_unrelated_files_and_sort_numeric_ids()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_saved_puzzle_ids_and_next_filename_ignore_unrelated_files_and_sort_numeric_ids",
        r##"(let* ((asm-blox-save-directory-name
                 (asm-blox-test-sandbox-path
                  "saved-games"))
                (files
                 '("Fixture-10.asbx"
                   "Fixture-2.asbx"
                   "Fixture-7.asbx"
                   "Fixture-other.asbx"
                   "Other-99.asbx"
                   ".Fixture-10.asbx.win.txt")))
         (make-directory
          asm-blox-save-directory-name t)
         (dolist (file files)
           (with-temp-file
               (expand-file-name
                file
                asm-blox-save-directory-name)
             (insert file)))
         (list
          (asm-blox--saved-puzzle-ct-ids
           "Fixture")
          (file-name-nondirectory
           (asm-blox--generate-new-puzzle-filename
            "Fixture"))
          (file-name-nondirectory
           (asm-blox--make-puzzle-idx-file-name
            "Fixture" 42))
          (asm-blox--saved-puzzle-ct-ids
           "Other")))"##,
        expect![[r#"OK ((2 7 10) "Fixture-11.asbx" "Fixture-42.asbx" (99))"#]],
    )
}

fn asm_blox_backup_workflow_writes_exact_buffer_contents_beside_the_active_solution()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_backup_workflow_writes_exact_buffer_contents_beside_the_active_solution",
        r##"(let* ((directory
                 (asm-blox-test-sandbox-path
                  "backup-workflow"))
                (solution
                 (expand-file-name
                  "Fixture-1.asbx"
                  directory))
                (backup
                 (expand-file-name
                  ".Fixture-1.asbx.backup.txt"
                  directory)))
         (make-directory directory t)
         (with-temp-buffer
           (insert
            "cell one\ncell two\nλ\n")
           (set-visited-file-name solution)
           (asm-blox--backup-file-for-current-buffer)
           (list
            (file-exists-p backup)
            (with-temp-buffer
              (insert-file-contents-literally backup)
              (buffer-string))
            (buffer-string)
            (buffer-modified-p))))"##,
        expect![[r#"OK (t "cell one\ncell two\n\316\273\n" "cell one\ncell two\nλ\n" t)"#]],
    )
}

fn asm_blox_win_workflow_copies_origin_solution_to_hidden_win_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_win_workflow_copies_origin_solution_to_hidden_win_file",
        r##"(let* ((directory
                 (asm-blox-test-sandbox-path
                  "win-workflow"))
                (solution
                 (expand-file-name
                  "Identity-3.asbx"
                  directory))
                (win-file
                 (expand-file-name
                  ".Identity-3.asbx.win.txt"
                  directory))
                (origin
                 (generate-new-buffer
                  " *asm-blox-origin-fixture*"))
                (execution
                 (get-buffer-create
                  "*asm-blox-execution*")))
         (make-directory directory t)
         (unwind-protect
             (progn
               (with-current-buffer origin
                 (insert
                  "compiled board\nsolution body\n")
                 (set-visited-file-name solution))
               (with-current-buffer execution
                 (setq
                  asm-blox-execution-origin-buffer
                  origin)
                 (asm-blox--win-file-for-current-buffer))
               (list
                (file-exists-p win-file)
                (with-temp-buffer
                  (insert-file-contents-literally
                   win-file)
                  (buffer-string))
                (buffer-live-p origin)))
           (when
               (buffer-live-p origin)
             (with-current-buffer origin
               (set-buffer-modified-p nil))
             (kill-buffer origin))
           (when
               (buffer-live-p execution)
             (with-current-buffer execution
               (set-buffer-modified-p nil))
             (kill-buffer execution))))"##,
        expect![[r#"OK (t "compiled board\nsolution body\n" t)"#]],
    )
}

fn asm_blox_puzzle_won_detection_matches_only_named_hidden_win_artifacts() -> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_puzzle_won_detection_matches_only_named_hidden_win_artifacts",
        r##"(let ((asm-blox-save-directory-name
                (asm-blox-test-sandbox-path
                 "won-detection")))
         (make-directory
          asm-blox-save-directory-name t)
         (dolist (file
                  '(".Identity-1.asbx.win.txt"
                    "Identity-2.asbx"
                    ".Other-1.asbx.win.txt"
                    "Identity-winning-note.txt"))
           (with-temp-file
               (expand-file-name
                file
                asm-blox-save-directory-name)
             (insert "fixture")))
         (mapcar
          (lambda (name)
            (list
             name
             (asm-blox--puzzle-won-p name)))
          '("Identity" "Other" "Missing" "entity")))"##,
        expect![[
            r#"OK (("Identity" ".Identity-1.asbx.win.txt") ("Other" ".Other-1.asbx.win.txt") ("Missing" nil) ("entity" ".Identity-1.asbx.win.txt"))"#
        ]],
    )
}

fn asm_blox_rendered_board_roundtrips_all_twelve_code_cells_and_problem_identity() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asm_blox_rendered_board_roundtrips_all_twelve_code_cells_and_problem_identity",
        r##"(let* ((fixture-problem
                 (lambda ()
                   (asm-blox--problem-spec-create
                    :name "Round Trip"
                    :difficulty 'medium
                    :sources nil
                    :sinks nil
                    :description
                    "Render and parse every cell.")))
                (asm-blox-puzzles
                 (list fixture-problem))
                (asm-blox--extra-gameboard-cells
                 (funcall fixture-problem))
                (asm-blox--display-mode 'edit)
                (asm-blox-box-contents
                 (make-hash-table
                  :test 'equal)))
         (dotimes (row
                   asm-blox--gameboard-row-ct)
           (dotimes (col
                     asm-blox--gameboard-col-ct)
             (puthash
              (list row col)
              (format
               "(const %d)\n(send right)"
               (+ (* row 10) col))
              asm-blox-box-contents)))
         (with-temp-buffer
           (asm-blox-display-game-board)
           (let ((rendered
                  (buffer-string)))
             (setq
              asm-blox-box-contents nil
              asm-blox--extra-gameboard-cells nil)
             (asm-blox--parse-saved-buffer)
             (list
              (asm-blox--problem-spec-name
               asm-blox--extra-gameboard-cells)
              (mapcar
               (lambda (coords)
                 (list
                  coords
                  (gethash
                   coords
                   asm-blox-box-contents)))
               '((0 0) (0 1) (0 2) (0 3)
                 (1 0) (1 1) (1 2) (1 3)
                 (2 0) (2 1) (2 2) (2 3)))
              (length rendered)
              (substring rendered
                         (- (length rendered) 47))))))"##,
        expect![[
            r#"OK ("Round Trip" (((0 0) "(const 0)\n(send right)") ((0 1) "(const 1)\n(send right)") ((0 2) "(const 2)\n(send right)") ((0 3) "(const 3)\n(send right)") ((1 0) "(const 10)\n(send right)") ((1 1) "(const 11)\n(send right)") ((1 2) "(const 12)\n(send right)") ((1 3) "(const 13)\n(send right)") ((2 0) "(const 20)\n(send right)") ((2 1) "(const 21)\n(send right)") ((2 2) "(const 22)\n(send right)") ((2 3) "(const 23)\n(send right)")) 5803 "   \n\n\nRound Trip:\nRender and parse every cell.\n")"#
        ]],
    )
}

pub(super) fn files_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asm_blox_saved_puzzle_ids_and_next_filename_ignore_unrelated_files_and_sort_numeric_ids(),
        asm_blox_backup_workflow_writes_exact_buffer_contents_beside_the_active_solution(),
        asm_blox_win_workflow_copies_origin_solution_to_hidden_win_file(),
        asm_blox_puzzle_won_detection_matches_only_named_hidden_win_artifacts(),
        asm_blox_rendered_board_roundtrips_all_twelve_code_cells_and_problem_identity(),
    ]
}
