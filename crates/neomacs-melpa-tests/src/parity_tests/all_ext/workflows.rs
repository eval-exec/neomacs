use expect_test::expect;

use super::ParityBatchCase;

/// `all' collects every matching line into `*All*': a header naming the regexp
/// and the source, then one piece per match.  Each piece is an overlay holding
/// a marker into the source buffer - those markers are what makes the buffer
/// editable - alongside the left-margin line numbers and the `match' face on
/// the matched text.  The source is left untouched by the collection itself.
fn collecting_matches_builds_a_linked_all_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "collecting_matches_builds_a_linked_all_buffer",
        r##"(ae-test-with-source
 (all "alpha")
 (list (with-current-buffer "*All*"
         (list major-mode
               (buffer-name all-buffer)
               next-error-function
               buffer-read-only
               (point)))
       (ae-test-text "*All*")
       (ae-test-pieces)
       (ae-test-line-numbers)
       (ae-test-match-faces)
       (ae-test-text source)))"##,
        expect![[
            r#"OK ((all-mode "notes.txt" all-next-error nil 1) "Lines matching \"alpha\" in buffer notes.txt.\n--------\nalpha one\nalpha four\nalpha six\n" ((54 64 "alpha one\n" 1) (64 75 "alpha four\n" 32) (75 85 "alpha six\n" 54)) ((54 . "1") (64 . "4") (75 . "6")) ((54 59 "alpha") (64 69 "alpha") (75 80 "alpha")) "alpha one\nbeta two\ngamma three\nalpha four\ndelta five\nalpha six\n")"#
        ]],
    )
}

fn editing_a_collected_line_writes_back_to_the_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "editing_a_collected_line_writes_back_to_the_source",
        r##"(ae-test-with-source
 (all "alpha")
 (with-current-buffer "*All*"
   (goto-char (point-min))
   (search-forward "alpha four")
   (replace-match "ALPHA FOUR!")
   (let ((after-first (list (ae-test-text "*All*") (ae-test-text source))))
     (goto-char (point-min))
     (search-forward "alpha six")
     (goto-char (line-beginning-position))
     (insert "TODO ")
     (list after-first
           (ae-test-text "*All*")
           (ae-test-text source)
           (with-current-buffer source (list (point) (buffer-modified-p)))
           (ae-test-pieces)))))"##,
        expect![[
            r#"OK (("Lines matching \"alpha\" in buffer notes.txt.\n--------\nalpha one\nALPHA FOUR!\nalpha six\n" "alpha one\nbeta two\ngamma three\nALPHA FOUR!\ndelta five\nalpha six\n") "Lines matching \"alpha\" in buffer notes.txt.\n--------\nalpha one\nALPHA FOUR!\nTODO alpha six\n" "alpha one\nbeta two\ngamma three\nALPHA FOUR!\ndelta five\nTODO alpha six\n" (1 t) ((54 64 "alpha one\n" 1) (64 76 "ALPHA FOUR!\n" 32) (76 91 "TODO alpha six\n" 55)))"#
        ]],
    )
}

fn deleting_and_extending_collected_text_reaches_the_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "deleting_and_extending_collected_text_reaches_the_source",
        r##"(ae-test-with-source
 (all "alpha")
 (let ((deleted
        (with-current-buffer "*All*"
          (goto-char (point-min))
          (search-forward "alpha four")
          (delete-region (line-beginning-position) (line-end-position))
          (list (ae-test-text "*All*") (ae-test-text source)))))
   (with-current-buffer "*All*"
     (goto-char (point-min))
     (search-forward "alpha six")
     (insert " six")
     (list deleted
           (ae-test-text "*All*")
           (ae-test-text source)
           (ae-test-pieces)))))"##,
        expect![[
            r#"OK (("Lines matching \"alpha\" in buffer notes.txt.\n--------\nalpha one\n\nalpha six\n" "alpha one\nbeta two\ngamma three\n\ndelta five\nalpha six\n") "Lines matching \"alpha\" in buffer notes.txt.\n--------\nalpha one\n\nalpha six six\n" "alpha one\nbeta two\ngamma three\n\ndelta five\nalpha six six\n" ((54 64 "alpha one\n" 1) (64 65 "\n" 32) (65 79 "alpha six six\n" 44)))"#
        ]],
    )
}

fn an_edit_spanning_two_matches_is_refused() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_edit_spanning_two_matches_is_refused",
        r##"(ae-test-with-source
 (all "alpha")
 (with-current-buffer "*All*"
   (goto-char (point-min))
   (let* ((start (progn (search-forward "alpha one") (- (point) 3)))
          (end (progn (search-forward "alpha four") (- (point) 4))))
     (let ((refused (condition-case error (progn (delete-region start end) :deleted)
                      (error error))))
       (goto-char (point-min))
       (search-forward "alpha one")
       (insert "!")
       (list refused
             (ae-test-text "*All*")
             (ae-test-text source)
             (with-current-buffer source (buffer-modified-p)))))))"##,
        expect![[
            r#"OK ((error "Changes should be limited to a single text piece") "Lines matching \"alpha\" in buffer notes.txt.\n--------\nalpha one!\nalpha four\nalpha six\n" "alpha one!\nbeta two\ngamma three\nalpha four\ndelta five\nalpha six\n" t)"#
        ]],
    )
}

fn the_all_buffer_navigates_back_to_the_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_all_buffer_navigates_back_to_the_source",
        r##"(ae-test-with-source
 (all "alpha")
 (set-window-buffer (selected-window) "*All*")
 (with-current-buffer "*All*"
   (goto-char (point-min))
   (search-forward "alpha four")
   (goto-char (line-beginning-position))
   (let ((keys (list (key-binding (kbd "C-c C-c"))
                     (key-binding (kbd "C-x h"))
                     (key-binding (kbd "C-c C-k"))
                     (key-binding (kbd "C-c C-m")))))
     (execute-kbd-macro (kbd "C-c C-c"))
     (let ((jumped (list (ae-test-copy (buffer-name (current-buffer)))
                         (ae-test-copy (buffer-name (window-buffer (selected-window))))
                         (with-current-buffer source
                           (list (point) (line-number-at-pos)
                                 (copy-sequence
                                  (buffer-substring-no-properties
                                   (line-beginning-position) (line-end-position))))))))
       (set-window-buffer (selected-window) "*All*")
       (with-current-buffer "*All*" (goto-char (point-min)))
       (let ((stepped (list (condition-case error (progn (next-error) :moved)
                              (error error))
                            (with-current-buffer source
                              (list (point) (line-number-at-pos)))
                            (with-current-buffer "*All*" (point)))))
         (set-window-buffer (selected-window) "*All*")
         (with-current-buffer "*All*"
           (execute-kbd-macro (kbd "C-x h"))
           (list keys jumped stepped
                 (list (point) (mark t) mark-active
                       (copy-sequence
                        (buffer-substring-no-properties
                         (min (point) (mark t)) (max (point) (mark t))))))))))))"##,
        expect![[
            r#"OK ((all-mode-goto all-mark-whole-contents quit-window mc/edit-lines-in-all) ("notes.txt" "notes.txt" (32 4 "alpha four")) (:moved (1 1) 54) (54 85 t "alpha one\nalpha four\nalpha six\n"))"#
        ]],
    )
}

fn context_lines_merge_overlapping_matches_into_one_piece() -> ParityBatchCase {
    ParityBatchCase::value(
        "context_lines_merge_overlapping_matches_into_one_piece",
        r##"(ae-test-with-source
 (all "alpha" 1)
 (list (ae-test-text "*All*")
       (ae-test-pieces)
       (ae-test-line-numbers)
       (ae-test-match-faces)))"##,
        expect![[
            r#"OK ("Lines matching \"alpha\" in buffer notes.txt.\n--------\nalpha one\nbeta two\ngamma three\nalpha four\ndelta five\nalpha six\n--------\n" ((54 117 "alpha one\nbeta two\ngamma three\nalpha four\ndelta five\nalpha six\n" 1)) ((54 . "1") (64 . "2") (73 . "3") (85 . "4") (96 . "5") (107 . "6")) ((54 59 "alpha") (85 90 "alpha") (107 112 "alpha")))"#
        ]],
    )
}

fn the_first_invocation_fails_because_it_kills_a_buffer_that_is_not_there() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_first_invocation_fails_because_it_kills_a_buffer_that_is_not_there",
        r##"(let ((source (generate-new-buffer "notes.txt")))
  (unwind-protect
      (with-current-buffer source
        (insert ae-test-notes)
        (goto-char (point-min))
        (list (get-buffer "*All*")
              (condition-case error (progn (all "alpha") :collected) (error error))
              (get-buffer "*All*")
              (condition-case error (progn (all "alpha") :collected) (error error))
              (progn (get-buffer-create "*All*")
                     (condition-case error (progn (all "alpha") :collected) (error error)))
              (ae-test-text "*All*")))
    (when (get-buffer "*All*") (kill-buffer "*All*"))
    (kill-buffer source)))"##,
        expect![[
            r#"OK (nil (error "No buffer named *All*") nil (error "No buffer named *All*") :collected "Lines matching \"alpha\" in buffer notes.txt.\n--------\nalpha one\nalpha four\nalpha six\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        collecting_matches_builds_a_linked_all_buffer(),
        editing_a_collected_line_writes_back_to_the_source(),
        deleting_and_extending_collected_text_reaches_the_source(),
        an_edit_spanning_two_matches_is_refused(),
        the_all_buffer_navigates_back_to_the_source(),
        context_lines_merge_overlapping_matches_into_one_piece(),
        the_first_invocation_fails_because_it_kills_a_buffer_that_is_not_there(),
    ]
}
