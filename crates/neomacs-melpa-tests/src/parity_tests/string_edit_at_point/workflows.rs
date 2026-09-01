use expect_test::expect;

use super::ParityBatchCase;

/// The surface: the autoloaded entry command, the minor mode with its
/// lighter and C-c C-k/C-c C-c keymap, the pre-mode hook, and the
/// payload.
fn the_minor_mode_surface_and_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_minor_mode_surface_and_payload",
        r####"(list
 :source (sep879-test-source-state)
 :commands
 (list :edit (commandp 'string-edit-at-point)
       :abort (commandp 'string-edit-at-point-abort)
       :conclude (commandp 'string-edit-at-point-conclude))
 :mode
 (list :lighter (cdr (assoc 'string-edit-at-point-mode minor-mode-alist))
       :abort-key (lookup-key string-edit-at-point-mode-map (kbd "C-c C-k"))
       :conclude-key (lookup-key string-edit-at-point-mode-map (kbd "C-c C-c")))
 :hook (boundp 'string-edit-at-point-hook))"####,
        expect![[
            r#"OK (:source (:upstream-tree "56dce032374cbd78a8e95dbe7778c7f60edc82a3" :feature t :version "20230118.1933" :dash "20260221.1346") :commands (:edit t :abort t :conclude t) :mode (:lighter (" StringEdit") :abort-key string-edit-at-point-abort :conclude-key string-edit-at-point-conclude) :hook t)"#
        ]],
    )
}

/// String detection: the quotes char and string bounds at point, and
/// `se/find-original''s alist with the RAW (unescaped) content and the
/// string's beginning position.
fn string_detection_reports_quotes_bounds_and_raw_content() -> ParityBatchCase {
    ParityBatchCase::value(
        "string_detection_reports_quotes_bounds_and_raw_content",
        r####"(unwind-protect
    (progn
      (sep879-test-reset)
      (let ((buffer (generate-new-buffer "sep-fixture-detect")))
        (with-current-buffer buffer
          (emacs-lisp-mode)
          (insert "(message \"a \\\"b\\\" and \\\\n end\")\n")
          (goto-char (point-min))
          (search-forward "a ")
          (let ((quotes (se/current-quotes-char))
                (inside (and (se/point-inside-string-p) t))
                (bounds (progn
                          (goto-char (point-min))
                          (search-forward "b")
                          (se/string-position-at-point)))
                (original (progn
                            (goto-char (point-min))
                            (search-forward "and ")
                            (se/find-original))))
            (list :quotes (char-to-string quotes)
                  :inside inside
                  :bounds bounds
                  :raw (se/aget :raw original)
                  :beg (se/aget :beg original)
                  :cleanup (and (functionp (se/aget :cleanup original)) t)
                  :escape (and (functionp (se/aget :escape original)) t)))
          (kill-buffer))))
  (sep879-test-reset))"####,
        expect!["OK t"],
    )
}

/// The escape transforms: `se/escape'/`se/unescape' flip backslash-quote
/// sequences and plain quotes; `se/escape-ws'/`se/unescape-ws' flip
/// whitespace signifiers; each over a real buffer region.
fn the_escape_transforms_round_trip() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_escape_transforms_round_trip",
        r####"(let ((run
       (lambda (setup transform)
         (with-temp-buffer
           (insert setup)
           (funcall transform)
           (buffer-substring-no-properties (point-min) (point-max))))))
  (list
   :unescape-quotes (funcall run "a \\\"b\\\" c"
                             (lambda () (se/unescape "\"")))
   :escape-quotes (funcall run "a \"b\" c"
                           (lambda () (se/escape "\"")))
   :unescape-nl (funcall run "a\\nb"
                         (lambda () (se/unescape-ws "n" "\n")))
   :escape-nl (funcall run "a\nb"
                       (lambda () (se/escape-ws "n" "\n")))
   :unescape-tab (funcall run "a\\tb"
                          (lambda () (se/unescape-ws "t" "\t")))
   :escape-tab (funcall run "a	tb"
                        (lambda () (se/escape-ws "t" "\t")))))"####,
        expect![[
            r#"OK (:unescape-quotes "a \"b\" c" :escape-quotes "a \\\"b\\\" c" :unescape-nl "a\nb" :escape-nl "a\\nb" :unescape-tab "a\11b" :escape-tab "a\\ttb")"#
        ]],
    )
}

/// The full round trip: a buffer with an escaped string, point inside,
/// `string-edit-at-point' pops the raw content up, editing it and
/// concluding writes the re-escaped string back into the original
/// buffer; aborting leaves the original untouched.
fn the_full_round_trip_edits_and_reescapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_full_round_trip_edits_and_reescapes",
        r####"(unwind-protect
    (progn
      (sep879-test-reset)
      (let ((concluded-buffer (generate-new-buffer "sep-fixture-conclude"))
            concluded)
        (with-current-buffer concluded-buffer
          (emacs-lisp-mode)
          (insert "(message \"a \\\"b\\\"\")\n")
          (goto-char (point-min))
          (search-forward "b")
          (string-edit-at-point)
          (let ((popup (current-buffer)))
            (setq concluded
                  (list :popup-buffer (buffer-name popup)
                        :popup-mode string-edit-at-point-mode
                        :popup-content
                        (buffer-substring-no-properties
                         (point-min) (point-max))))
            (goto-char (point-max))
            (insert " and \\\"more\\\"")
            (string-edit-at-point-conclude)
            (setq concluded
                  (append concluded
                          (list :after-buffer (buffer-name (current-buffer))
                                :after-content
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))))))
        (let ((aborted-buffer (generate-new-buffer "sep-fixture-abort")))
          (with-current-buffer aborted-buffer
            (emacs-lisp-mode)
            (insert "(message \"keep \\\"this\\\"\")\n")
            (goto-char (point-min))
            (search-forward "keep")
            (string-edit-at-point)
            (string-edit-at-point-abort))
          (list :concluded concluded
                :aborted
                (with-current-buffer aborted-buffer
                  (list :still-exists (and (buffer-live-p aborted-buffer) t)
                        :content (buffer-substring-no-properties
                                  (point-min) (point-max))))))))
  (sep879-test-reset))"####,
        expect![[
            r#"OK (:concluded (:popup-buffer "*string-edit-at-point*" :popup-mode t :popup-content "a \"b\"" :after-buffer "sep-fixture-conclude" :after-content "(message \"a \\\"b\\\" and \\\\\\\"more\\\\\\\"\")\n") :aborted (:still-exists t :content "(message \"keep \\\"this\\\"\")\n"))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_minor_mode_surface_and_payload(),
        string_detection_reports_quotes_bounds_and_raw_content(),
        the_escape_transforms_round_trip(),
        the_full_round_trip_edits_and_reescapes(),
    ]
}
