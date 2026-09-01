use expect_test::expect;

use super::ParityBatchCase;

fn abc_mode_opens_a_real_tunebook_and_sets_up_the_editing_environment() -> ParityBatchCase {
    ParityBatchCase::value(
        "abc_mode_opens_a_real_tunebook_and_sets_up_the_editing_environment",
        r##"(let ((buffer (abc-test-open "book/session.abc" abc-test-tunebook))
      (other (abc-test-open "book/session.abp" abc-test-tunebook))
      (plain (abc-test-open "book/session.txt" abc-test-tunebook)))
  (unwind-protect
      (list
       (with-current-buffer buffer
         (list major-mode
               mode-name
               (derived-mode-p 'text-mode)
               comment-start
               comment-end
               page-delimiter
               (local-variable-p 'page-delimiter)
               abc-use-song-as-page-delimiter
               (buffer-size)
               (point)
               (buffer-modified-p)
               (eq (current-local-map) abc-mode-map)
               (key-binding (kbd "C-c C-n"))
               (key-binding (kbd "M-n"))
               (key-binding (kbd "C-c C-d c"))))
       (with-current-buffer other major-mode)
       (with-current-buffer plain major-mode))
    (dolist (each (list buffer other plain))
      (kill-buffer each))))"##,
        expect![[
            r#"OK ((abc-mode "abc" text-mode "%" "" "^[ \11]*X[ \11]*:[ \11]*\\([0-9]+\\)" t t 265 1 nil t abc-renumber-songs abc-forward-song abc-crescendo-region) abc-mode text-mode)"#
        ]],
    )
}

fn abc_mode_song_motion_reports_the_reference_number_of_each_tune() -> ParityBatchCase {
    ParityBatchCase::value(
        "abc_mode_song_motion_reports_the_reference_number_of_each_tune",
        r##"(let ((buffer (abc-test-open "book/navigate.abc" abc-test-tunebook)))
  (unwind-protect
      (with-current-buffer buffer
        (goto-char (point-min))
        (let (forward numbers)
          (dotimes (_ 4)
            (push (abc-forward-song) forward)
            (push (abc-current-song-number t) numbers))
          (goto-char (point-max))
          (let* ((back-one (abc-backward-song))
                 (back-number (abc-current-song-number t))
                 (back-two (abc-backward-song))
                 (title (buffer-substring-no-properties
                         (line-beginning-position 2)
                         (line-end-position 2))))
            (list (nreverse forward)
                  (nreverse numbers)
                  back-one
                  back-number
                  back-two
                  title
                  (point)
                  (save-excursion
                    (goto-char (point-min))
                    (forward-page 1)
                    (point))
                  (buffer-modified-p)))))
    (kill-buffer buffer)))"##,
        expect![[r#"OK ((4 127 199 nil) (7 7 2 2) 196 2 124 "T:The Butterfly" 124 4 nil)"#]],
    )
}

fn abc_mode_renumbers_a_tunebook_with_duplicate_reference_numbers() -> ParityBatchCase {
    ParityBatchCase::value(
        "abc_mode_renumbers_a_tunebook_with_duplicate_reference_numbers",
        r##"(let ((buffer (abc-test-open "book/renumber.abc" abc-test-tunebook)))
  (unwind-protect
      (with-current-buffer buffer
        (let ((kill-ring nil)
              (kill-ring-yank-pointer nil))
          (goto-char (point-max))
          (abc-renumber-songs)
          (list (buffer-string)
                (point)
                (buffer-modified-p)
                kill-ring
                (save-excursion
                  (goto-char (point-min))
                  (list (abc-forward-song) (abc-current-song-number t))))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK ("X:1\nT:Si Beag, Si Mor\nC:Turlough O'Carolan\nM:3/4\nL:1/8\nQ:1/4=120\nK:D\n|:A2|d3 e f2|e3 d B2|A3 B A2|F4 A2|\n%% a comment line\nX:2\nT:The Butterfly\nM:9/8\nL:1/8\nK:Em\n|:B3 AFE|B2 E E2 F|G3 AGF|GFE FED|\nX:3\nT:Planxty Irwin\nM:3/4\nL:1/8\nK:G\nD2|G3 A B2|d3 e d2|B3 A G2|E4 D2|\n" 266 t ("X:2" "X:7" "X:7") (4 1))"#
        ]],
    )
}

fn abc_mode_wraps_a_selected_phrase_in_slur_crescendo_and_repeat_marks() -> ParityBatchCase {
    ParityBatchCase::value(
        "abc_mode_wraps_a_selected_phrase_in_slur_crescendo_and_repeat_marks",
        r##"(let ((buffer (abc-test-open "book/marks.abc" "X:1\nT:Marks\nK:D\nA2 B2 c2 d2|e2 f2 g2 a2|\n")))
  (unwind-protect
      (with-current-buffer buffer
        (let ((kill-ring nil)
              (kill-ring-yank-pointer nil))
          (goto-char (point-min))
          (forward-line 3)
          (push-mark (line-end-position) t t)
          (goto-char (point))
          (set-mark (+ (point) 11))
          (abc-slur-region)
          (let ((slurred (buffer-string)))
            (set-mark (point-min))
            (goto-char (point-max))
            (goto-char (line-beginning-position))
            (set-mark (line-end-position))
            (abc-crescendo-region)
            (let ((crescendo (buffer-string)))
              (goto-char (line-beginning-position))
              (set-mark (line-end-position))
              (abc-repeat-region)
              (let ((repeated (buffer-string)))
                (goto-char (line-beginning-position))
                (set-mark (line-end-position))
                (abc-diminuendo-region)
                (list slurred
                      crescendo
                      repeated
                      (buffer-string)
                      (point)
                      (length kill-ring)))))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK ("X:1\nT:Marks\nK:D\n(A2 B2 c2 d2)|e2 f2 g2 a2|\n" "X:1\nT:Marks\nK:D\n(A2 B2 c2 d2)|e2 f2 g2 a2|\n!crescendo(!!crescendo)!" "X:1\nT:Marks\nK:D\n(A2 B2 c2 d2)|e2 f2 g2 a2|\n |: !crescendo(!!crescendo)! :| " "X:1\nT:Marks\nK:D\n(A2 B2 c2 d2)|e2 f2 g2 a2|\n!diminuendo(! |: !crescendo(!!crescendo)! :| !diminuendo)!" 102 4)"#
        ]],
    )
}

fn abc_mode_lists_every_tune_title_in_an_occur_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "abc_mode_lists_every_tune_title_in_an_occur_buffer",
        r##"(save-window-excursion
  (let ((buffer (abc-test-open "book/titles.abc" abc-test-tunebook)))
    (unwind-protect
        (progn
          (when (get-buffer "*Occur*")
            (kill-buffer "*Occur*"))
          (set-window-buffer (selected-window) buffer)
          (set-buffer buffer)
          (abc-list-buffer-songs)
          (let ((occur (get-buffer "*Occur*")))
            (list (buffer-name (window-buffer (selected-window)))
                  (buffer-name)
                  (with-current-buffer occur
                    (list major-mode
                          (buffer-substring-no-properties (point-min) (point-max))
                          (line-number-at-pos (point-max)))))))
      (when (get-buffer "*Occur*")
        (kill-buffer "*Occur*"))
      (kill-buffer buffer))))"##,
        expect![[
            r#"OK ("*Occur*" "*Occur*" (occur-mode "3 matches for \"^T:\" in buffer: titles.abc\n      2:T:Si Beag, Si Mor\n     11:T:The Butterfly\n     17:T:Planxty Irwin\n" 5))"#
        ]],
    )
}

fn abc_mode_extracts_chords_and_aligns_bar_lines_of_a_melody_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "abc_mode_extracts_chords_and_aligns_bar_lines_of_a_melody_line",
        r##"(let ((buffer
       (abc-test-open
        "book/chords.abc"
        (concat "X:1\nT:Chords\nK:G\n"
                "\"G\"G2 A2 |\"C\"c2 [ce] |\"D7\"d2 ^f2 |\"G\"g4 |\n"
                "|G2 A2   |  c2 e2|d2 f2    |g4|\n"))))
  (unwind-protect
      (with-current-buffer buffer
        (let ((kill-ring nil)
              (kill-ring-yank-pointer nil))
          (goto-char (point-min))
          (forward-line 3)
          (abc-extract-chords)
          (let ((chords (buffer-substring-no-properties
                         (line-beginning-position)
                         (line-end-position))))
            (forward-line 1)
            (abc-align-bars (line-beginning-position) (line-end-position))
            (list chords
                  (buffer-substring-no-properties (point-min) (point-max))
                  (memq 'abc-mode align-text-modes)
                  (buffer-modified-p)))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK ("\"G\"x2 x2 |\"C\"x2 xx |\"D7\"x2 x2 |\"G\"x4 |" "X:1\nT:Chords\nK:G\n\"G\"x2 x2 |\"C\"x2 xx |\"D7\"x2 x2 |\"G\"x4 |\n|G2 A2\11| c2 e2\11|d2 f2\11\11|g4\11|\n" (abc-mode text-mode outline-mode) t)"#
        ]],
    )
}

fn abc_mode_runs_abc2ps_and_abc2midi_on_the_saved_tunebook() -> ParityBatchCase {
    ParityBatchCase::value(
        "abc_mode_runs_abc2ps_and_abc2midi_on_the_saved_tunebook",
        r##"(let ((buffer (abc-test-open "run/book.abc" abc-test-tunebook)))
  (abc-test-setup-tools)
  (unwind-protect
      (with-current-buffer buffer
        (set-window-buffer (selected-window) buffer)
        (goto-char (point-min))
        (abc-forward-song)
        (execute-kbd-macro (kbd "C-c C-p 1 RET"))
        (execute-kbd-macro (kbd "C-c C-m m RET"))
        (list (abc-test-commands)
              abc-executable
              abc-midi-executable
              abc-preferred-options
              (buffer-modified-p)))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK (("abcm2ps -e 7 book.abc -O =" "abc2midi book.abc") "abcm2ps" "abc2midi" "" nil)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        abc_mode_opens_a_real_tunebook_and_sets_up_the_editing_environment(),
        abc_mode_song_motion_reports_the_reference_number_of_each_tune(),
        abc_mode_renumbers_a_tunebook_with_duplicate_reference_numbers(),
        abc_mode_wraps_a_selected_phrase_in_slur_crescendo_and_repeat_marks(),
        abc_mode_lists_every_tune_title_in_an_occur_buffer(),
        abc_mode_extracts_chords_and_aligns_bar_lines_of_a_melody_line(),
        abc_mode_runs_abc2ps_and_abc2midi_on_the_saved_tunebook(),
    ]
}
