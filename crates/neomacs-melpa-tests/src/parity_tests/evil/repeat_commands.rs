use expect_test::expect;

use super::ParityBatchCase;

fn evil_normalize_repeat_info_concatenates_adjacent_key_arrays_around_symbols() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_normalize_repeat_info_concatenates_adjacent_key_arrays_around_symbols",
        r##"(mapcar
               #'evil-normalize-repeat-info
               '(("abc")
                 ("M-f")
                 (SYM)
                 ("abc" [XX YY] "def")
                 (BEG MID END)
                 (BEG "abc" [XX YY] "def")
                 ("abc" [XX YY] "def" END)
                 ("abc" [XX YY] MID "def")
                 (BEG "abc" [XX YY] MID "def" END)))"##,
        expect![
            "OK (([97 98 99]) ([77 45 102]) (SYM) ([97 98 99 XX YY 100 101 102]) (BEG MID END) (BEG [97 98 99 XX YY 100 101 102]) ([97 98 99 XX YY 100 101 102] END) ([97 98 99 XX YY] MID [100 101 102]) (BEG [97 98 99 XX YY] MID [100 101 102] END))"
        ],
    )
}

fn evil_dot_repeat_replays_replace_delete_insert_and_change_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_dot_repeat_replays_replace_delete_insert_and_change_commands",
        r##"(mapcar
               (lambda (case)
                 (with-temp-buffer
                   (insert "one two three four")
                   (goto-char (point-min))
                   (evil-local-mode 1)
                   (evil-normal-state)
                   (execute-kbd-macro (kbd (car case)))
                   (execute-kbd-macro (kbd (cdr case)))
                   (list
                    (buffer-string)
                    (point)
                    evil-state)))
               '(("r X" . "w .")
                 ("dw" . "w .")
                 ("i X ESC" . "w .")
                 ("cw X ESC" . "w .")))"##,
        expect![[
            r#"OK (("rXw." 5 nil) ("rXw.dww." 9 nil) ("rXw.dww.iXw." 13 nil) ("rXw.dww.iXw.cwXw." 18 nil))"#
        ]],
    )
    .fresh_process()
}

fn evil_keypress_parser_handles_counts_operators_zero_and_incomplete_input() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_keypress_parser_handles_counts_operators_zero_and_incomplete_input",
        r##"(with-temp-buffer
               (evil-local-mode 1)
               (evil-operator-state)
               (list
                (evil-keypress-parser '(?d))
                (evil-keypress-parser '(?2 ?d))
                (evil-keypress-parser '(?2 ?0 ?2 ?d))
                (evil-keypress-parser '(?4 ?0 ?4 ?g ??))
                (evil-keypress-parser '(?0))
                (let ((unread-command-events '(?d)))
                  (evil-keypress-parser '(?2)))))"##,
        expect![
            "OK ((evil-delete nil) (evil-delete 2) (evil-delete 202) (evil-rot13 404) (evil-beginning-of-line nil) (evil-delete 2))"
        ],
    )
}

fn evil_command_properties_add_replace_remove_and_declare_behavior_flags() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_command_properties_add_replace_remove_and_declare_behavior_flags",
        r##"(progn
               (defun neomacs-evil-property-command ()
                 (interactive))
               (evil-set-command-properties
                'neomacs-evil-property-command
                :type 'exclusive
                :repeat nil)
               (let ((initial
                      (evil-command-properties
                       'neomacs-evil-property-command)))
                 (evil-add-command-properties
                  'neomacs-evil-property-command
                  :repeat t :keep-visual t)
                 (let ((added
                        (evil-command-properties
                         'neomacs-evil-property-command)))
                   (evil-remove-command-properties
                    'neomacs-evil-property-command :type)
                   (let ((removed
                          (evil-command-properties
                           'neomacs-evil-property-command)))
                     (evil-declare-motion
                      'neomacs-evil-property-command)
                     (list
                      initial
                      added
                      removed
                      (evil-command-properties
                       'neomacs-evil-property-command)
                      (evil-get-command-property
                       'neomacs-evil-property-command
                       :missing 'fallback)
                      (evil-has-command-property-p
                       'neomacs-evil-property-command
                       :keep-visual))))))"##,
        expect![
            "OK (#1=(:type exclusive :repeat t :keep-visual t) #1# #2=(:repeat motion . #3=(:keep-visual t)) #2# fallback #3#)"
        ],
    )
}

fn evil_markers_store_local_positions_advance_flags_and_raw_marker_objects() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_markers_store_local_positions_advance_flags_and_raw_marker_objects",
        r##"(with-temp-buffer
               (insert "alpha\nbeta\ngamma")
               (let ((evil-markers-alist nil))
                 (goto-char 3)
                 (evil-set-marker ?a)
                 (goto-char 8)
                 (evil-set-marker ?b nil t)
                 (let ((raw-a (evil-get-marker ?a t))
                       (raw-b (evil-get-marker ?b t)))
                   (list
                    (evil-get-marker ?a)
                    (evil-get-marker ?b)
                    (markerp raw-a)
                    (marker-insertion-type raw-a)
                    (markerp raw-b)
                    (marker-insertion-type raw-b)
                    (evil-get-marker ?z)
                    (mapcar #'car evil-markers-alist)))))"##,
        expect!["OK (3 8 t nil t t nil (98 97))"],
    )
}

fn evil_set_marker_rejects_a_read_only_special_marker() -> ParityBatchCase {
    ParityBatchCase::signal(
        "evil_set_marker_rejects_a_read_only_special_marker",
        r##"(with-temp-buffer
               (evil-set-marker ?<))"##,
        expect!["ERR (wrong-type-argument markerp nil)"],
    )
}

fn evil_yank_helpers_attach_character_line_and_rectangle_handlers_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_yank_helpers_attach_character_line_and_rectangle_handlers_exactly",
        r##"(with-temp-buffer
               (insert "alpha\nbravo\ncharlie\n")
               (let ((kill-ring nil)
                     (kill-ring-yank-pointer nil))
                 (evil-yank-characters 1 6)
                 (let ((characters
                        (list
                         (car kill-ring)
                         (get-text-property
                          0 'yank-handler (car kill-ring)))))
                   (evil-yank-lines 1 7)
                   (let ((lines
                          (list
                           (car kill-ring)
                           (get-text-property
                            0 'yank-handler (car kill-ring)))))
                     (evil-yank-rectangle 1 9)
                     (list
                      characters
                      lines
                      (car kill-ring)
                      (get-text-property
                       0 'yank-handler (car kill-ring)))))))"##,
        expect![[
            r#"OK (("alpha" nil) (#("alpha\n" 0 6 (yank-handler (evil-yank-line-handler nil t))) (evil-yank-line-handler nil t)) #("al\nbr" 0 5 (yank-handler (evil-yank-block-handler ("al" "br") t evil-delete-yanked-rectangle))) (evil-yank-block-handler ("al" "br") t evil-delete-yanked-rectangle))"#
        ]],
    )
}

pub(super) fn repeat_commands_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        evil_normalize_repeat_info_concatenates_adjacent_key_arrays_around_symbols(),
        evil_dot_repeat_replays_replace_delete_insert_and_change_commands(),
        evil_keypress_parser_handles_counts_operators_zero_and_incomplete_input(),
        evil_command_properties_add_replace_remove_and_declare_behavior_flags(),
        evil_markers_store_local_positions_advance_flags_and_raw_marker_objects(),
        evil_set_marker_rejects_a_read_only_special_marker(),
        evil_yank_helpers_attach_character_line_and_rectangle_handlers_exactly(),
    ]
}
