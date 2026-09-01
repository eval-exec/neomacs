use expect_test::expect;

use super::ParityBatchCase;

fn minor_mode_binds_directional_drag_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "minor_mode_binds_directional_drag_commands",
        r####"
(with-temp-buffer
  (drag-stuff-mode 1)
  (list :mode (and drag-stuff-mode t)
        :up (lookup-key drag-stuff-mode-map (kbd "<M-up>"))
        :down (lookup-key drag-stuff-mode-map (kbd "<M-down>"))
        :left (lookup-key drag-stuff-mode-map (kbd "<M-left>"))
        :right (lookup-key drag-stuff-mode-map (kbd "<M-right>"))))
"####,
        expect!["OK (:mode t :up nil :down nil :left nil :right nil)"],
    )
}

fn line_up_and_down_reorder_adjacent_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "line_up_and_down_reorder_adjacent_lines",
        r####"
(neomacs-drag-stuff-test-with-buffer
 "alpha\nbeta\ngamma\n"
 "beta"
 (lambda ()
   (drag-stuff-up 1)
   (let ((up (neomacs-drag-stuff-test-state)))
     (drag-stuff-down 1)
     (list :up up :restored (neomacs-drag-stuff-test-state)))))
"####,
        expect![[
            r#"OK (:up (:text "beta\nalpha\ngamma\n" :point 1 :line 1 :column 0) :restored (:text "alpha\nbeta\ngamma\n" :point 7 :line 2 :column 0))"#
        ]],
    )
}

fn word_left_and_right_swap_adjacent_words() -> ParityBatchCase {
    ParityBatchCase::value(
        "word_left_and_right_swap_adjacent_words",
        r####"
(neomacs-drag-stuff-test-with-buffer
 "release train ready\n"
 "train"
 (lambda ()
   (drag-stuff-left 1)
   (let ((left (neomacs-drag-stuff-test-state)))
     (drag-stuff-right 1)
     (list :left left :restored (neomacs-drag-stuff-test-state)))))
"####,
        expect![[
            r#"OK (:left (:text "release train ready\n" :point 9 :line 1 :column 8) :restored (:text "train release ready\n" :point 9 :line 1 :column 8))"#
        ]],
    )
}

fn region_lines_move_together_vertically() -> ParityBatchCase {
    ParityBatchCase::value(
        "region_lines_move_together_vertically",
        r####"
(neomacs-drag-stuff-test-with-buffer
 "a\nb\nc\nd\n"
 "b"
 (lambda ()
   (goto-char (point-min))
   (search-forward "b")
   (beginning-of-line)
   (set-mark (point))
   (search-forward "c")
   (end-of-line)
   (activate-mark)
   (let ((transient-mark-mode t)
         (mark-active t))
     (drag-stuff-down 1)
     (neomacs-drag-stuff-test-state))))
"####,
        expect![[r#"OK (:text "a\nd\nb\nc\n" :point 8 :line 4 :column 1)"#]],
    )
}

fn region_moves_horizontally_as_a_block() -> ParityBatchCase {
    ParityBatchCase::value(
        "region_moves_horizontally_as_a_block",
        r####"
(neomacs-drag-stuff-test-with-buffer
 "xxAByy\n"
 "AB"
 (lambda ()
   (goto-char (point-min))
   (search-forward "A")
   (backward-char 1)
   (set-mark (point))
   (search-forward "B")
   (activate-mark)
   (let ((transient-mark-mode t)
         (mark-active t))
     (drag-stuff-right 2)
     (neomacs-drag-stuff-test-state))))
"####,
        expect![[r#"OK (:text "xxyyAB\n" :point 7 :line 1 :column 6)"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        minor_mode_binds_directional_drag_commands(),
        line_up_and_down_reorder_adjacent_lines(),
        word_left_and_right_swap_adjacent_words(),
        region_lines_move_together_vertically(),
        region_moves_horizontally_as_a_block(),
    ]
}
