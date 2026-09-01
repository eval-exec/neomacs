use expect_test::expect;

use super::ParityBatchCase;

fn unfill_paragraph_unwraps_filled_prose() -> ParityBatchCase {
    ParityBatchCase::value(
        "unfill_paragraph_unwraps_filled_prose",
        r####"
(with-temp-buffer
  (insert "Alpha beta gamma delta epsilon zeta eta theta iota kappa.\n")
  (goto-char (point-min))
  (let ((fill-column 20))
    (fill-paragraph)
    (let ((filled (buffer-string)))
      (goto-char (point-min))
      (unfill-paragraph)
      (list :filled-has-newline
            (and (string-match-p "\n" (string-trim-right filled)) t)
            :unfilled (string-trim-right (buffer-string))
            :single-line
            (and (= 1 (length (split-string (buffer-string) "\n" t))) t)))))
"####,
        expect![[
            r#"OK (:filled-has-newline t :unfilled "Alpha beta gamma delta epsilon zeta eta theta iota kappa." :single-line t)"#
        ]],
    )
}

fn unfill_region_only_touches_selected_span() -> ParityBatchCase {
    ParityBatchCase::value(
        "unfill_region_only_touches_selected_span",
        r####"
(with-temp-buffer
  (insert "One two three four five six seven eight.\n\nKeep this paragraph alone.\n")
  (goto-char (point-min))
  (let ((fill-column 12))
    (fill-paragraph)
    (let* ((filled-first (buffer-substring-no-properties
                          (point-min)
                          (save-excursion
                            (goto-char (point-min))
                            (forward-paragraph)
                            (point))))
           (start (point-min))
           (end (save-excursion
                  (goto-char (point-min))
                  (forward-paragraph)
                  (point))))
      (unfill-region start end)
      (list :first (string-trim-right
                    (buffer-substring-no-properties
                     (point-min)
                     (save-excursion
                       (goto-char (point-min))
                       (forward-paragraph)
                       (point))))
            :second (string-trim
                     (buffer-substring-no-properties
                      (save-excursion
                        (goto-char (point-min))
                        (forward-paragraph)
                        (point))
                      (point-max)))
            :filled-had-newline
            (and (string-match-p "\n" (string-trim-right filled-first)) t)))))
"####,
        expect![[
            r#"OK (:first "One two three four five six seven eight." :second "Keep this paragraph alone." :filled-had-newline t)"#
        ]],
    )
}

fn unfill_toggle_fills_then_unwraps_on_repeat() -> ParityBatchCase {
    ParityBatchCase::value(
        "unfill_toggle_fills_then_unwraps_on_repeat",
        r####"
(with-temp-buffer
  (insert "Alpha beta gamma delta epsilon zeta eta theta.\n")
  (goto-char (point-min))
  (let ((fill-column 18)
        (this-command 'unfill-toggle)
        (last-command 'other))
    (call-interactively #'unfill-toggle)
    (let ((after-fill (string-trim-right (buffer-string))))
      (setq last-command this-command
            this-command 'unfill-toggle)
      (call-interactively #'unfill-toggle)
      (list :after-fill-has-newline
            (and (string-match-p "\n" after-fill) t)
            :after-unfill (string-trim-right (buffer-string))
            :after-unfill-single-line
            (and (= 1 (length (split-string (buffer-string) "\n" t))) t)))))
"####,
        expect![[
            r#"OK (:after-fill-has-newline t :after-unfill "Alpha beta gamma delta epsilon zeta eta theta." :after-unfill-single-line t)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        unfill_paragraph_unwraps_filled_prose(),
        unfill_region_only_touches_selected_span(),
        unfill_toggle_fills_then_unwraps_on_repeat(),
    ]
}
