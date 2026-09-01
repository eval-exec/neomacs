use expect_test::expect;

use super::ParityBatchCase;

fn typing_a_paragraph_key_by_key_wraps_it_while_the_user_types_and_undo_takes_it_back()
-> ParityBatchCase {
    ParityBatchCase::value(
        "typing_a_paragraph_key_by_key_wraps_it_while_the_user_types_and_undo_takes_it_back",
        r##"(let ((buffer (afp-test-open 'text-mode 38)))
  (unwind-protect
      (progn
        (aggressive-fill-paragraph-mode 1)
        ;; A short opening burst: still inside the fill column, still one line.
        (afp-test-type "The parser retries ")
        (let ((short (afp-test-text))
              (short-where (afp-test-where)))
          ;; Carrying on past the fill column wraps the paragraph as the user
          ;; types, which is the whole point of the package.
          (afp-test-type "every failed request before it gives up on the queue. ")
          (let ((wrapped (afp-test-text))
                (wrapped-where (afp-test-where)))
            ;; The user changes their mind and presses C-/.
            (afp-test-press "C-/")
            (list (list short short-where)
                  (list wrapped wrapped-where)
                  (list (afp-test-text) (afp-test-where))
                  (key-binding (kbd "C-/"))
                  aggressive-fill-paragraph-mode
                  fill-column
                  (and (memq #'aggressive-fill-paragraph-post-self-insert-function
                             post-self-insert-hook)
                       t)))))
    (afp-test-close buffer)))"##,
        expect![[
            r#"OK (("The parser retries " (20 1 19 "The parser retries ")) ("The parser retries every failed\nrequest before it gives up on the\nqueue. " (74 3 7 "queue. ")) ("The parser retries every failed\nrequest before it gives up on" (62 2 29 "request before it gives up on")) undo t 38 t)"#
        ]],
    )
}

fn inserting_words_inside_a_filled_paragraph_reflows_it_and_leaves_point_after_the_typed_space()
-> ParityBatchCase {
    ParityBatchCase::value(
        "inserting_words_inside_a_filled_paragraph_reflows_it_and_leaves_point_after_the_typed_space",
        r##"(let ((buffer (afp-test-open 'text-mode 40)))
  (unwind-protect
      (progn
        (aggressive-fill-paragraph-mode 1)
        (insert "The parser retries every failed request before it gives up on the queue.")
        (fill-paragraph)
        (let ((before (afp-test-text)))
          ;; Go back into the first line and add two words there.  The refill
          ;; runs while point is in the middle of the paragraph: an
          ;; implementation that let `fill-paragraph' move point would leave the
          ;; typed space, and the cursor, somewhere else entirely.
          (goto-char (point-min))
          (search-forward "retries ")
          (let ((entry-where (afp-test-where)))
            (afp-test-type "quietly and ")
            (list before
                  entry-where
                  (afp-test-text)
                  (afp-test-where)
                  (char-before)
                  (buffer-substring-no-properties (- (point) 12) (point))))))
    (afp-test-close buffer)))"##,
        expect![[
            r#"OK ("The parser retries every failed request\nbefore it gives up on the queue." (20 1 19 "The parser retries every failed request") "The parser retries quietly and every\nfailed request before it gives up on the\nqueue." (32 1 31 "The parser retries quietly and every") 32 "quietly and ")"#
        ]],
    )
}

fn set_fill_column_changes_the_width_the_next_automatic_refill_uses() -> ParityBatchCase {
    ParityBatchCase::value(
        "set_fill_column_changes_the_width_the_next_automatic_refill_uses",
        r##"(let ((buffer (afp-test-open 'text-mode 70)))
  (unwind-protect
      (progn
        (aggressive-fill-paragraph-mode 1)
        (set-fill-column 72)
        (insert "The parser retries every failed request before it gives up on the queue and reports the recoverable failure state")
        (afp-test-type " ")
        (let ((wide (afp-test-text))
              (wide-column fill-column))
          ;; C-x f to a narrow column: the next typed space reflows what is
          ;; already there, it does not only wrap the new text.
          (set-fill-column 34)
          (afp-test-type "now ")
          (let ((narrow (afp-test-text))
                (narrow-column fill-column))
            ;; And widening again unwraps it.
            (set-fill-column 100)
            (afp-test-type "again ")
            (list (list wide-column wide)
                  (list narrow-column narrow)
                  (list fill-column (afp-test-text))
                  (afp-test-where)))))
    (afp-test-close buffer)))"##,
        expect![[
            r#"OK ((72 "The parser retries every failed request before it gives up on the queue\nand reports the recoverable failure state ") (34 "The parser retries every failed\nrequest before it gives up on the\nqueue and reports the recoverable\nfailure state now ") (100 "The parser retries every failed request before it gives up on the queue and reports the recoverable\nfailure state now again ") (125 2 24 "failure state now again "))"#
        ]],
    )
}

fn only_the_configured_fill_keys_refill_and_a_second_whitespace_character_suppresses_it()
-> ParityBatchCase {
    ParityBatchCase::value(
        "only_the_configured_fill_keys_refill_and_a_second_whitespace_character_suppresses_it",
        r##"(let ((sentence "The parser retries every failed request before it gives up"))
  (cl-flet ((typed (keys &optional already-there configure)
              ;; Insert the over-long line without the hook seeing it, so the
              ;; only thing that can refill the paragraph is KEYS.
              (let ((buffer (afp-test-open 'text-mode 40)))
                (unwind-protect
                    (progn
                      (when configure (funcall configure))
                      (aggressive-fill-paragraph-mode 1)
                      (insert (concat sentence already-there))
                      (afp-test-type keys)
                      (list (afp-test-text) (afp-test-where)))
                  (afp-test-close buffer)))))
    (list
     ;; The two documented default fill keys.
     :space (typed " ")
     :period (typed ".")
     ;; Not a fill key, so the long line is left exactly as it was.
     :at-sign (typed "@")
     ;; ... until the user puts it in `afp-fill-keys' as the README describes.
     :at-sign-configured
     (typed "@" nil (lambda () (setq-local afp-fill-keys (cons ?@ afp-fill-keys))))
     ;; A space typed after existing whitespace is suppressed, so a user can
     ;; type a double space without the paragraph rearranging itself.
     :second-space (typed " " " ")
     :space-after-tab (typed " " "\t")
     ;; And `just-one-space' inserts a space without going through
     ;; `post-self-insert-hook' at all, the README's escape hatch.
     :escape-hatch
     (let ((buffer (afp-test-open 'text-mode 40)))
       (unwind-protect
           (progn
             (aggressive-fill-paragraph-mode 1)
             (insert sentence)
             (afp-test-press "M-SPC")
             (list (afp-test-text) (afp-test-where) (key-binding (kbd "M-SPC"))))
         (afp-test-close buffer))))))"##,
        expect![[
            r#"OK (:space ("The parser retries every failed request\nbefore it gives up " (60 2 19 "before it gives up ")) :period ("The parser retries every failed request\nbefore it gives up." (60 2 19 "before it gives up.")) :at-sign ("The parser retries every failed request before it gives up@" (60 1 59 "The parser retries every failed request before it gives up@")) :at-sign-configured ("The parser retries every failed request\nbefore it gives up@" (60 2 19 "before it gives up@")) :second-space ("The parser retries every failed request before it gives up  " (61 1 60 "The parser retries every failed request before it gives up  ")) :space-after-tab ("The parser retries every failed request before it gives up\11 " (61 1 65 "The parser retries every failed request before it gives up\11 ")) :escape-hatch ("The parser retries every failed request before it gives up " (60 1 59 "The parser retries every failed request before it gives up ") cycle-spacing))"#
        ]],
    )
}

fn adding_ruby_mode_to_the_comments_only_list_stops_the_refill_rewrapping_code() -> ParityBatchCase
{
    ParityBatchCase::value(
        "adding_ruby_mode_to_the_comments_only_list_stops_the_refill_rewrapping_code",
        r##"(cl-flet ((session (comments-only)
            ;; `ruby-mode' is not in `afp-fill-comments-only-mode-list' and
            ;; leaves `fill-paragraph-function' nil, so the default
            ;; `fill-paragraph' is what runs -- and it wraps code.
            (let ((buffer (afp-test-open 'ruby-mode 44))
                  (afp-fill-comments-only-mode-list
                   (if comments-only
                       (cons 'ruby-mode afp-fill-comments-only-mode-list)
                     afp-fill-comments-only-mode-list)))
              (unwind-protect
                  (progn
                    (aggressive-fill-paragraph-mode 1)
                    (insert "# Retry the request when the parser reports a recoverable failure state\n"
                            "notify(recipient, subject, body, priority, retries")
                    ;; Finish the argument list: the space after the comma is a
                    ;; fill key, so this is where code would get rewrapped.
                    (afp-test-type ", timeout)")
                    (let ((after-code (afp-test-text)))
                      ;; Then extend the comment above it.
                      (goto-char (point-min))
                      (end-of-line)
                      (afp-test-type " now")
                      (list (afp-choose-fill-function)
                            after-code
                            (afp-test-text)
                            (afp-test-where))))
                (afp-test-close buffer)))))
  (list :default (session nil) :comments-only (session t)))"##,
        expect![[
            r##"OK (:default (fill-paragraph "# Retry the request when the parser reports a recoverable failure state\nnotify(recipient, subject, body, priority,\nretries, timeout)" "# Retry the request when the parser reports\n# a recoverable failure state now\nnotify(recipient, subject, body, priority,\nretries, timeout)" (78 2 33 "# a recoverable failure state now")) :comments-only (afp-only-fill-comments "# Retry the request when the parser reports a recoverable failure state\nnotify(recipient, subject, body, priority, retries, timeout)" "# Retry the request when the parser reports\n# a recoverable failure state now\nnotify(recipient, subject, body, priority, retries, timeout)" (78 2 33 "# a recoverable failure state now")))"##
        ]],
    )
}

fn a_user_supplied_suppression_predicate_protects_marker_lines_while_prose_still_wraps()
-> ParityBatchCase {
    ParityBatchCase::value(
        "a_user_supplied_suppression_predicate_protects_marker_lines_while_prose_still_wraps",
        r##"(progn
  (defun afp-test-todo-line-p ()
    "Suppress filling on a line the user has marked TODO."
    (string-prefix-p "TODO:" (afp-current-line)))
  (cl-flet ((session (custom)
              (let ((buffer (afp-test-open 'text-mode 40))
                    (afp-suppress-fill-pfunction-list
                     (if custom
                         (cons #'afp-test-todo-line-p
                               afp-suppress-fill-pfunction-list)
                       afp-suppress-fill-pfunction-list)))
                (unwind-protect
                    (progn
                      (aggressive-fill-paragraph-mode 1)
                      (insert "TODO: teach the parser to retry a recoverable failure before giving up")
                      (afp-test-type " ")
                      (let ((marker-line (afp-test-text)))
                        (goto-char (point-max))
                        (insert "\n\nOrdinary prose still wraps because the predicate only matches the marker line")
                        (afp-test-type " ")
                        (list (length afp-suppress-fill-pfunction-list)
                              marker-line
                              (afp-test-text))))
                  (afp-test-close buffer)))))
    (let ((descriptor (cadr (assq 'aggressive-fill-paragraph package-alist))))
      (list
       ;; `afp-suppress-fill?' runs the predicate list through dash's `-any?',
       ;; so this workflow is also what proves the declared dependency is the
       ;; one doing the work.
       (list (package-version-join (package-desc-version descriptor))
             (package-desc-reqs descriptor)
             (file-name-base (symbol-file '-any? 'defun)))
       :default (session nil)
       :custom (session t)))))"##,
        expect![[
            r#"OK (("20240213.2320" ((dash (2 10 0))) "dash") :default (5 "TODO: teach the parser to retry a\nrecoverable failure before giving up " "TODO: teach the parser to retry a\nrecoverable failure before giving up \n\nOrdinary prose still wraps because the\npredicate only matches the marker line ") :custom (6 "TODO: teach the parser to retry a recoverable failure before giving up " "TODO: teach the parser to retry a recoverable failure before giving up \n\nOrdinary prose still wraps because the\npredicate only matches the marker line "))"#
        ]],
    )
}

fn typing_into_a_visited_file_wraps_the_paragraph_and_saves_the_wrapped_bytes() -> ParityBatchCase {
    ParityBatchCase::value(
        "typing_into_a_visited_file_wraps_the_paragraph_and_saves_the_wrapped_bytes",
        r##"(let* ((notes (expand-file-name "release-notes.txt"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       ;; The sandbox sits inside the neomacs worktree, whose own
       ;; `.dir-locals.el' would otherwise impose fill-column 72 on every
       ;; visited file, after the major mode has chosen its own.  A user
       ;; editing their notes has no such directory above them.
       (enable-dir-local-variables nil)
       (ignored (write-region "Release notes\n\n" nil notes nil 'silent))
       (buffer (find-file-noselect notes)))
  (ignore ignored)
  (unwind-protect
      (progn
        (set-window-buffer (selected-window) buffer)
        (set-buffer buffer)
        (setq fill-column 44)
        (aggressive-fill-paragraph-mode 1)
        (goto-char (point-max))
        (afp-test-type "The parser now retries a recoverable failure before it gives up. ")
        (save-buffer)
        (list major-mode
              fill-column
              (afp-test-text)
              (afp-test-where)
              (buffer-modified-p)
              ;; What actually reached the disk, read as bytes.
              (with-temp-buffer
                (insert-file-contents-literally notes)
                (copy-sequence (buffer-string)))))
    (afp-test-close buffer)))"##,
        expect![[
            r#"OK (text-mode 44 "Release notes\n\nThe parser now retries a recoverable failure\nbefore it gives up. \n" (81 4 20 "before it gives up. ") nil "Release notes\n\nThe parser now retries a recoverable failure\nbefore it gives up. \n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        typing_a_paragraph_key_by_key_wraps_it_while_the_user_types_and_undo_takes_it_back(),
        inserting_words_inside_a_filled_paragraph_reflows_it_and_leaves_point_after_the_typed_space(
        ),
        set_fill_column_changes_the_width_the_next_automatic_refill_uses(),
        only_the_configured_fill_keys_refill_and_a_second_whitespace_character_suppresses_it(),
        adding_ruby_mode_to_the_comments_only_list_stops_the_refill_rewrapping_code(),
        a_user_supplied_suppression_predicate_protects_marker_lines_while_prose_still_wraps(),
        typing_into_a_visited_file_wraps_the_paragraph_and_saves_the_wrapped_bytes(),
    ]
}
