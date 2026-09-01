use expect_test::expect;

use super::ParityBatchCase;

fn default_keys_bind_outer_and_inner_line_objects() -> ParityBatchCase {
    ParityBatchCase::value(
        "default_keys_bind_outer_and_inner_line_objects",
        r####"
(list :outer (lookup-key evil-outer-text-objects-map "l")
      :inner (lookup-key evil-inner-text-objects-map "l")
      :a-key evil-textobj-line-a-key
      :i-key evil-textobj-line-i-key)
"####,
        expect![[r#"OK (:outer evil-a-line :inner evil-inner-line :a-key "l" :i-key "l")"#]],
    )
}

fn line_range_selects_full_line_and_trimmed_inner_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "line_range_selects_full_line_and_trimmed_inner_line",
        r####"
(neomacs-evil-textobj-line-test-with-buffer
 "alpha\n  release train  \nomega\n"
 "release"
 (lambda ()
   (list :outer (neomacs-evil-textobj-line-test-range #'evil-a-line)
         :inner (neomacs-evil-textobj-line-test-range #'evil-inner-line)
         :inclusive (let ((range (evil-line-range nil nil nil nil t)))
                      (list (evil-range-beginning range)
                            (evil-range-end range)
                            (buffer-substring-no-properties
                             (evil-range-beginning range)
                             (evil-range-end range))))
         :exclusive (let ((range (evil-line-range nil nil nil nil nil)))
                      (list (evil-range-beginning range)
                            (evil-range-end range)
                            (buffer-substring-no-properties
                             (evil-range-beginning range)
                             (evil-range-end range)))))))
"####,
        expect![[
            r#"OK (:outer (evil-a-line :begin 7 :end 24 :type inclusive :text "  release train  ") :inner (evil-inner-line :begin 9 :end 22 :type inclusive :text "release train") :inclusive (7 24 "  release train  ") :exclusive (9 22 "release train"))"#
        ]],
    )
}

fn operator_pending_il_and_al_delete_exact_line_spans() -> ParityBatchCase {
    ParityBatchCase::value(
        "operator_pending_il_and_al_delete_exact_line_spans",
        r####"
(list
 :inner
 (neomacs-evil-textobj-line-test-with-buffer
  "keep\n  payload here  \nkeep\n"
  "payload"
  (lambda ()
    (evil-normal-state)
    (execute-kbd-macro (kbd "d i l"))
    (list :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :state evil-state)))
 :outer
 (neomacs-evil-textobj-line-test-with-buffer
  "keep\n  payload here  \nkeep\n"
  "payload"
  (lambda ()
    (evil-normal-state)
    (execute-kbd-macro (kbd "d a l"))
    (list :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :state evil-state))))
"####,
        expect![[
            r#"OK (:inner (:text "keep\n    \nkeep\n" :point 8 :state normal) :outer (:text "keep\n\nkeep\n" :point 6 :state normal))"#
        ]],
    )
}

fn blank_and_single_word_lines_still_yield_stable_ranges() -> ParityBatchCase {
    ParityBatchCase::value(
        "blank_and_single_word_lines_still_yield_stable_ranges",
        r####"
(list
 :blank
 (neomacs-evil-textobj-line-test-with-buffer
  "a\n   \nb\n"
  nil
  (lambda ()
    (goto-char (point-min))
    (forward-line 1)
    (list :outer (neomacs-evil-textobj-line-test-range #'evil-a-line)
          :inner (neomacs-evil-textobj-line-test-range #'evil-inner-line))))
 :word
 (neomacs-evil-textobj-line-test-with-buffer
  "solo\n"
  "solo"
  (lambda ()
    (list :outer (neomacs-evil-textobj-line-test-range #'evil-a-line)
          :inner (neomacs-evil-textobj-line-test-range #'evil-inner-line)))))
"####,
        expect![[
            r#"OK (:blank (:outer (evil-a-line :begin 3 :end 6 :type inclusive :text "   ") :inner (evil-inner-line :begin 3 :end 6 :type inclusive :text "   ")) :word (:outer (evil-a-line :begin 1 :end 5 :type inclusive :text "solo") :inner (evil-inner-line :begin 1 :end 5 :type inclusive :text "solo")))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        default_keys_bind_outer_and_inner_line_objects(),
        line_range_selects_full_line_and_trimmed_inner_line(),
        operator_pending_il_and_al_delete_exact_line_spans(),
        blank_and_single_word_lines_still_yield_stable_ranges(),
    ]
}
