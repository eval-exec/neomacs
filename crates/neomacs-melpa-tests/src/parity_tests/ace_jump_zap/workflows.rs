use expect_test::expect;

use super::ParityBatchCase;

/// The package's headline workflow: `M-Z' asks for a character, labels every
/// occurrence in the window and deletes from point up to the one that is
/// picked.  The labels are the interesting part - `ajz/sort-by-closest' puts
/// the *nearest* match on `a', so the later "garlic" is labelled before the
/// earlier "400 g" - and with the default `ajz/zap-function' the removed text
/// is deleted rather than killed, so the kill ring stays empty.
fn zap_up_to_char_deletes_forward_and_labels_the_closest_match_first() -> ParityBatchCase {
    ParityBatchCase::value(
        "zap_up_to_char_deletes_forward_and_labels_the_closest_match_first",
        r##"(ajz-test-with-live-buffer
 (insert ajz-test-recipe)
 (goto-char 58)
 (let ((kill-ring nil))
   (ajz-test-tracing
    (execute-kbd-macro (kbd "M-Z g a"))
    (ajz-test-state))))"##,
        expect![[
            r#"OK (((nil "" 58 nil nil nil nil) (ace-jump-zap-up-to-char "M-Z g" 58 t nil 58 (("a" . 68) ("b" . 33))) (ace-jump-move "a" 58 nil nil nil nil)) "Pasta with tomato sauce\n  - 400 g tomatoes, peeled\n  - 2 garlic\n  - olive oil, salt, pepper\nSimmer for 20 minutes, then serve.\n" 58 58 t nil nil nil 0 nil)"#
        ]],
    )
}

fn zap_to_char_and_up_to_char_differ_in_both_directions() -> ParityBatchCase {
    ParityBatchCase::value(
        "zap_to_char_and_up_to_char_differ_in_both_directions",
        r##"(ajz-test-with-live-buffer
 (let ((kill-ring nil))
   (list
    (progn (erase-buffer) (insert ajz-test-recipe) (goto-char 58)
           (execute-kbd-macro (kbd "M-z g a"))
           (list :to-char-forward (buffer-substring-no-properties 52 (min (point-max) 76))
                 (point) (mark t)))
    (progn (erase-buffer) (insert ajz-test-recipe) (goto-char 58)
           (execute-kbd-macro (kbd "M-z g b"))
           (list :to-char-backward (buffer-substring-no-properties 25 (min (point-max) 50))
                 (point) (mark t)))
    (progn (erase-buffer) (insert ajz-test-recipe) (goto-char 58)
           (execute-kbd-macro (kbd "M-Z g b"))
           (list :up-to-char-backward (buffer-substring-no-properties 25 (min (point-max) 50))
                 (point) (mark t)))
    kill-ring)))"##,
        expect![[
            r#"OK ((:to-char-forward "  - 2 arlic\n  - olive oi" 58 58) (:to-char-backward "  - 400 cloves of garlic\n" 33 33) (:up-to-char-backward "  - 400 gcloves of garlic" 34 34) nil)"#
        ]],
    )
}

fn kill_region_zapping_puts_the_deleted_text_on_the_kill_ring() -> ParityBatchCase {
    ParityBatchCase::value(
        "kill_region_zapping_puts_the_deleted_text_on_the_kill_ring",
        r##"(ajz-test-with-live-buffer
 (insert ajz-test-recipe)
 (goto-char 58)
 (let ((kill-ring nil)
       (kill-ring-yank-pointer nil)
       (ajz/zap-function 'kill-region))
   (execute-kbd-macro (kbd "M-Z g a"))
   (let ((after-kill (list (buffer-string) (point) kill-ring)))
     (goto-char (point-max))
     (yank)
     (list after-kill
           (buffer-substring-no-properties 103 (point-max))
           (point)
           kill-ring
           (current-kill 0 t)))))"##,
        expect![[
            r#"OK (("Pasta with tomato sauce\n  - 400 g tomatoes, peeled\n  - 2 garlic\n  - olive oil, salt, pepper\nSimmer for 20 minutes, then serve.\n" 58 #1=("cloves of ")) " 20 minutes, then serve.\ncloves of " 138 #1# "cloves of ")"#
        ]],
    )
}

fn forward_only_zapping_ignores_matches_behind_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "forward_only_zapping_ignores_matches_behind_point",
        r##"(ajz-test-with-live-buffer
 (insert ajz-test-recipe)
 (let ((kill-ring nil)
       (ajz/forward-only t))
   (list
    (progn
      (goto-char 62)
      (ajz-test-tracing
       (execute-kbd-macro (kbd "M-Z o b"))
       (list (buffer-string) (point) (mark t))))
    (progn
      (erase-buffer) (insert ajz-test-recipe) (goto-char 62)
      (list (condition-case error
                (progn (execute-kbd-macro (kbd "M-Z w")) :no-error)
              (error error))
            (buffer-string)
            (point)
            ajz/zapping
            ajz/saved-point
            ajz/to-char
            (length (overlays-in (point-min) (point-max))))))))"##,
        expect![[
            r#"OK ((((nil "" 62 nil nil nil nil) (ace-jump-zap-up-to-char "M-Z o" 62 t nil 62 (("a" . 65) ("b" . 79) ("c" . 85) ("d" . 111))) (ace-jump-move "b" 62 nil nil nil nil)) "Pasta with tomato sauce\n  - 400 g tomatoes, peeled\n  - 2 clovolive oil, salt, pepper\nSimmer for 20 minutes, then serve.\n" 62 62) ((error "[AceJump] No one found") "Pasta with tomato sauce\n  - 400 g tomatoes, peeled\n  - 2 cloves of garlic\n  - olive oil, salt, pepper\nSimmer for 20 minutes, then serve.\n" 62 t 62 nil 0))"#
        ]],
    )
}

fn disabling_proximity_sorting_labels_matches_in_buffer_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "disabling_proximity_sorting_labels_matches_in_buffer_order",
        r##"(ajz-test-with-live-buffer
 (insert ajz-test-recipe)
 (goto-char 58)
 (let ((kill-ring nil)
       (ajz/sort-by-closest nil))
   (ajz-test-tracing
    (execute-kbd-macro (kbd "M-Z g a"))
    (list (buffer-substring-no-properties 25 (min (point-max) 50))
          (point)
          (mark t)
          kill-ring))))"##,
        expect![[
            r#"OK (((nil "" 58 nil nil nil nil) (ace-jump-zap-up-to-char "M-Z g" 58 t nil 58 (("a" . 33) ("b" . 68))) (ace-jump-move "a" 34 nil nil nil nil)) "  - 400 gcloves of garlic" 34 34 nil)"#
        ]],
    )
}

fn the_fifty_two_character_limit_bounds_how_far_a_zap_can_reach() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_fifty_two_character_limit_bounds_how_far_a_zap_can_reach",
        r##"(ajz-test-with-live-buffer
 (dotimes (_ 18) (insert "xxx\n"))
 (let ((kill-ring nil))
   (list
    (progn
      (goto-char (point-min))
      (ajz-test-tracing
       (execute-kbd-macro (kbd "M-Z x Z"))
       (list (buffer-size)
             (point)
             (buffer-substring-no-properties (point) (min (point-max) (+ (point) 8))))))
    (progn
      (erase-buffer)
      (dotimes (_ 18) (insert "xxx\n"))
      (goto-char (point-min))
      (let ((ajz/52-character-limit nil)
            (ajz-test-captured-labels nil))
        (add-hook 'ace-jump-mode-before-jump-hook #'ajz-test-capture-labels)
        (unwind-protect
            (progn
              (execute-kbd-macro (kbd "M-Z x Z"))
              (list (length ajz-test-captured-labels)
                    (buffer-size)
                    (point)
                    (car (last ajz-test-captured-labels))))
          (remove-hook 'ace-jump-mode-before-jump-hook #'ajz-test-capture-labels)))))))"##,
        expect![[
            r#"OK ((((nil "" 1 nil nil nil nil) (ace-jump-zap-up-to-char "M-Z x" 1 t nil 1 (("A" . 35) ("B" . 37) ("C" . 38) ("D" . 39) ("E" . 41) ("F" . 42) ("G" . 43) ("H" . 45) ("I" . 46) ("J" . 47) ("K" . 49) ("L" . 50) ("M" . 51) ("N" . 53) ("O" . 54) ("P" . 55) ("Q" . 57) ("R" . 58) ("S" . 59) ("T" . 61) ("U" . 62) ("V" . 63) ("W" . 65) ("X" . 66) ("Y" . 67) ("Z" . 69) ("a" . 1) ("b" . 2) ("c" . 3) ("d" . 5) ("e" . 6) ("f" . 7) ("g" . 9) ("h" . 10) ("i" . 11) ("j" . 13) ("k" . 14) ("l" . 15) ("m" . 17) ("n" . 18) ("o" . 19) ("p" . 21) ("q" . 22) ("r" . 23) ("s" . 25) ("t" . 26) ("u" . 27) ("v" . 29) ("w" . 30) ("x" . 31) ("y" . 33) ("z" . 34))) (ace-jump-move "Z" 1 nil nil nil nil)) 4 1 "xxx\n") (54 2 1 ("z" . 37)))"#
        ]],
    )
}

fn an_unassigned_key_cancels_the_zap_and_leaves_the_text_untouched() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_unassigned_key_cancels_the_zap_and_leaves_the_text_untouched",
        r##"(ajz-test-with-live-buffer
 (insert ajz-test-recipe)
 (goto-char 58)
 (let ((kill-ring nil))
   (list
    (ajz-test-tracing
     (execute-kbd-macro (kbd "M-Z g !"))
     (ajz-test-state))
    (progn
      (goto-char 58)
      (ajz-test-tracing
       (execute-kbd-macro (kbd "M-z g C-g"))
       (ajz-test-state)))
    (key-binding (kbd "M-Z")))))"##,
        expect![[
            r#"OK ((((nil "" 58 nil nil nil nil) (ace-jump-zap-up-to-char "M-Z g" 58 t nil 58 (("a" . 68) ("b" . 33))) (ajz/keyboard-reset "!" 58 nil nil nil nil)) "Pasta with tomato sauce\n  - 400 g tomatoes, peeled\n  - 2 cloves of garlic\n  - olive oil, salt, pepper\nSimmer for 20 minutes, then serve.\n" 58 nil nil nil nil nil 0 nil) (((nil "" 58 nil nil nil nil) (ace-jump-zap-to-char "M-z g" 58 t t 58 (("a" . 68) ("b" . 33))) (ajz/keyboard-reset "C-g" 58 nil nil nil nil)) "Pasta with tomato sauce\n  - 400 g tomatoes, peeled\n  - 2 cloves of garlic\n  - olive oil, salt, pepper\nSimmer for 20 minutes, then serve.\n" 58 nil nil nil nil nil 0 nil) ace-jump-zap-up-to-char)"#
        ]],
    )
}

fn the_dwim_commands_dispatch_between_the_builtin_zap_and_ace_jump() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_dwim_commands_dispatch_between_the_builtin_zap_and_ace_jump",
        r##"(ajz-test-with-live-buffer
 (list
  (let ((kill-ring nil))
    (erase-buffer) (insert ajz-test-recipe) (goto-char 58)
    (ajz-test-tracing
     (execute-kbd-macro (kbd "C-c z g"))
     (list (buffer-substring-no-properties 52 (min (point-max) 76))
           (point) (mark t) kill-ring)))
  (let ((kill-ring nil))
    (erase-buffer) (insert ajz-test-recipe) (goto-char 58)
    (ajz-test-tracing
     (execute-kbd-macro (kbd "C-u C-c z g a"))
     (list (buffer-substring-no-properties 52 (min (point-max) 76))
           (point) (mark t) kill-ring)))
  (let ((kill-ring nil))
    (erase-buffer) (insert ajz-test-recipe) (goto-char 58)
    (execute-kbd-macro (kbd "C-c Z g"))
    (list (buffer-substring-no-properties 52 (min (point-max) 76))
          (point) kill-ring))))"##,
        expect![[
            r#"OK ((((nil "" 58 nil nil nil nil) (ace-jump-zap-to-char-dwim "" 14 nil nil nil nil) (kill-region "C-c z" 58 nil nil nil nil)) "  - 2 arlic\n  - olive oi" 58 nil ("cloves of g")) (((nil "" 58 nil nil nil nil) (nil "C-u" 58 nil nil nil nil) (ace-jump-zap-to-char-dwim "C-c z g" 58 t t 58 (("a" . 68) ("b" . 33))) (ace-jump-move "a" 58 nil nil nil nil)) "  - 2 arlic\n  - olive oi" 58 58 nil) ("  - 2 garlic\n  - olive o" 58 ("cloves of ")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        zap_up_to_char_deletes_forward_and_labels_the_closest_match_first(),
        zap_to_char_and_up_to_char_differ_in_both_directions(),
        kill_region_zapping_puts_the_deleted_text_on_the_kill_ring(),
        forward_only_zapping_ignores_matches_behind_point(),
        disabling_proximity_sorting_labels_matches_in_buffer_order(),
        the_fifty_two_character_limit_bounds_how_far_a_zap_can_reach(),
        an_unassigned_key_cancels_the_zap_and_leaves_the_text_untouched(),
        the_dwim_commands_dispatch_between_the_builtin_zap_and_ace_jump(),
    ]
}
