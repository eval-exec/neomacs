use expect_test::expect;

use super::ParityBatchCase;

fn auto_highlight_symbol_forward_and_backward_navigation_wrap_real_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_forward_and_backward_navigation_wrap_real_matches",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha one alpha two alpha")
                             (goto-char 2)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-selected-window
                              (selected-window))
                             (ahs-highlight
                              "alpha"
                              1
                              6)
                             (let (positions)
                               (dotimes (_ 4)
                                 (ahs-forward)
                                 (push
                                  (list
                                   (point)
                                   (overlay-start
                                    (ahs-current-overlay-window)))
                                  positions))
                               (dotimes (_ 4)
                                 (ahs-backward)
                                 (push
                                  (list
                                   (point)
                                   (overlay-start
                                    (ahs-current-overlay-window)))
                                  positions))
                               (nreverse positions))))"##,
        expect!["OK ((12 11) (22 21) (2 1) (12 11) (2 1) (22 21) (12 11) (2 1))"],
    )
}

fn auto_highlight_symbol_definition_navigation_skips_ordinary_occurrences_and_wraps()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_definition_navigation_skips_ordinary_occurrences_and_wraps",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha alpha alpha alpha")
                             (let* ((window
                                     (selected-window))
                                    (current
                                     (make-overlay 1 6)))
                               (setq
                                ahs-current-overlay
                                (list current)
                                ahs-overlay-list
                                (mapcar
                                 (lambda (case)
                                   (let ((overlay
                                          (make-overlay
                                           (car case)
                                           (+ 5
                                              (car case)))))
                                     (overlay-put
                                      overlay
                                      'window
                                      window)
                                     (overlay-put
                                      overlay
                                      'face
                                      (cdr case))
                                     overlay))
                                 '((7 . ahs-face)
                                   (13 . ahs-definition-face)
                                   (19 . ahs-definition-face))))
                               (overlay-put
                                current
                                'window
                                window)
                               (goto-char 2)
                               (let (positions)
                                 (dotimes (_ 3)
                                   (ahs-forward-definition)
                                   (push
                                    (point)
                                    positions))
                                 (dotimes (_ 3)
                                   (ahs-backward-definition)
                                   (push
                                    (point)
                                    positions))
                                 (nreverse positions)))))"##,
        expect!["OK (20 20 20 14 14 14)"],
    )
}

fn auto_highlight_symbol_back_to_start_restores_original_match_after_navigation() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_highlight_symbol_back_to_start_restores_original_match_after_navigation",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha alpha alpha")
                             (goto-char 8)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer
                              ahs-selected-window
                              (selected-window))
                             (ahs-highlight
                              "alpha"
                              7
                              12)
                             (let ((start
                                    (list
                                     ahs-start-point
                                     (point))))
                               (ahs-forward)
                               (let ((after-forward
                                      (point)))
                                 (ahs-back-to-start)
                                 (list
                                  start
                                  after-forward
                                  (point)
                                  (overlay-start
                                   (ahs-current-overlay-window)))))))"##,
        expect!["OK ((7 8) 14 8 7)"],
    )
}

fn auto_highlight_symbol_navigation_predicates_classify_position_definition_display_and_hidden()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_navigation_predicates_classify_position_definition_display_and_hidden",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha alpha alpha")
                             (let* ((window
                                     (selected-window))
                                    (current
                                     (make-overlay 7 12))
                                    (before
                                     (make-overlay 1 6))
                                    (after
                                     (make-overlay 13 18))
                                    (hidden
                                     (make-overlay 13 18)))
                               (overlay-put
                                current
                                'window
                                window)
                               (overlay-put
                                before
                                'window
                                window)
                               (overlay-put
                                after
                                'window
                                window)
                               (overlay-put
                                after
                                'face
                                'ahs-definition-face)
                               (overlay-put
                                hidden
                                'invisible
                                t)
                               (setq
                                ahs-current-overlay
                                (list current)
                                ahs-overlay-list
                                (list before after)
                                ahs-start-point
                                13)
                               (goto-char 9)
                               (mapcar
                                (lambda (overlay)
                                  (list
                                   (overlay-start
                                    overlay)
                                   (ahs-forward-p
                                    overlay)
                                   (ahs-backward-p
                                    overlay)
                                   (ahs-definition-p
                                    overlay)
                                   (ahs-start-point-p
                                    overlay)
                                   (ahs-inside-overlay-p
                                    overlay)
                                   (ahs-inside-display-p
                                    overlay)
                                   (ahs-hidden-p
                                    overlay)))
                                (list
                                 before
                                 current
                                 after)))))"##,
        expect![
            "OK ((1 nil t nil nil nil t nil) (7 nil nil nil nil t t nil) (13 t nil t t nil t t))"
        ],
    )
}

fn auto_highlight_symbol_skip_invisible_policy_bypasses_hidden_match_during_navigation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_skip_invisible_policy_bypasses_hidden_match_during_navigation",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha alpha alpha")
                             (let* ((window
                                     (selected-window))
                                    (current
                                     (make-overlay 1 6))
                                    (hidden-match
                                     (make-overlay 7 12))
                                    (visible-match
                                     (make-overlay 13 18))
                                    (fold
                                     (make-overlay 7 12)))
                               (dolist
                                   (overlay
                                    (list
                                     current
                                     hidden-match
                                     visible-match))
                                 (overlay-put
                                  overlay
                                  'window
                                  window))
                               (overlay-put
                                fold
                                'invisible
                                t)
                               (overlay-put
                                fold
                                'isearch-open-invisible
                                (lambda (_overlay)
                                  (push
                                   :open
                                   auto-highlight-symbol-test-events)))
                               (setq
                                ahs-current-overlay
                                (list current)
                                ahs-overlay-list
                                (list
                                 hidden-match
                                 visible-match)
                                ahs-select-invisible
                                'skip)
                               (goto-char 2)
                               (ahs-forward)
                               (list
                                (point)
                                (overlay-start
                                 (ahs-current-overlay-window))
                                auto-highlight-symbol-test-events
                                (overlay-get
                                 fold
                                 'invisible)))))"##,
        expect!["OK (14 13 nil t)"],
    )
    .fresh_process()
}

fn auto_highlight_symbol_immediate_invisible_policy_opens_selected_fold_then_recloses_old_fold()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_immediate_invisible_policy_opens_selected_fold_then_recloses_old_fold",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert
                              "alpha alpha alpha")
                             (let* ((window
                                     (selected-window))
                                    (current
                                     (make-overlay 1 6))
                                    (second
                                     (make-overlay 7 12))
                                    (third
                                     (make-overlay 13 18))
                                    (second-fold
                                     (make-overlay 7 12)))
                               (dolist
                                   (overlay
                                    (list
                                     current
                                     second
                                     third))
                                 (overlay-put
                                  overlay
                                  'window
                                  window))
                               (overlay-put
                                second-fold
                                'invisible
                                'fixture-fold)
                               (overlay-put
                                second-fold
                                'intangible
                                t)
                               (overlay-put
                                second-fold
                                'isearch-open-invisible
                                (lambda (overlay)
                                  (overlay-put
                                   overlay
                                   'invisible
                                   nil)))
                               (setq
                                ahs-current-overlay
                                (list current)
                                ahs-overlay-list
                                (list third second)
                                ahs-select-invisible
                                'immediate)
                               (goto-char 2)
                               (ahs-forward)
                               (let ((at-hidden
                                      (list
                                       (point)
                                       (overlay-get
                                        second-fold
                                        'invisible)
                                       (overlay-get
                                        second-fold
                                        'isearch-invisible)
                                       (length
                                        ahs-opened-overlay-list))))
                                 (ahs-forward)
                                 (list
                                  at-hidden
                                  (point)
                                  (overlay-get
                                   second-fold
                                   'invisible)
                                  (overlay-get
                                   second-fold
                                   'isearch-invisible)
                                  ahs-opened-overlay-list)))))"##,
        expect!["OK ((8 nil fixture-fold 1) 14 fixture-fold nil nil)"],
    )
}

fn auto_highlight_symbol_temporary_and_open_invisible_policies_differ_on_cleanup() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_highlight_symbol_temporary_and_open_invisible_policies_differ_on_cleanup",
        r##"(save-window-excursion
                           (mapcar
                            (lambda (policy)
                              (with-temp-buffer
                                (switch-to-buffer
                                 (current-buffer))
                                (insert
                                 "alpha alpha")
                                (let* ((window
                                        (selected-window))
                                       (current
                                        (make-overlay 1 6))
                                       (next
                                        (make-overlay 7 12))
                                       (fold
                                        (make-overlay 7 12))
                                       calls)
                                  (dolist
                                      (overlay
                                       (list current next))
                                    (overlay-put
                                     overlay
                                     'window
                                     window))
                                  (overlay-put
                                   fold
                                   'invisible
                                   'fixture)
                                  (overlay-put
                                   fold
                                   'isearch-open-invisible
                                   (lambda (overlay)
                                     (push
                                      :permanent-open
                                      calls)
                                     (overlay-put
                                      overlay
                                      'invisible
                                      nil)))
                                  (overlay-put
                                   fold
                                   'isearch-open-invisible-temporary
                                   (lambda (overlay hide)
                                     (push
                                      (list
                                       :temporary
                                       hide)
                                      calls)
                                     (overlay-put
                                      overlay
                                      'invisible
                                      (and hide
                                           'fixture))))
                                  (setq
                                   ahs-current-overlay
                                   (list current)
                                   ahs-overlay-list
                                   (list next)
                                   ahs-select-invisible
                                   policy)
                                  (goto-char 2)
                                  (ahs-select
                                   #'ahs-forward-p
                                   t)
                                  (let ((selected
                                         (list
                                          (point)
                                          (overlay-get
                                           fold
                                           'invisible)
                                          (length
                                           ahs-opened-overlay-list))))
                                    (goto-char 2)
                                    (ahs-remove-all-overlay
                                     t)
                                    (list
                                     policy
                                     selected
                                     (nreverse calls)
                                     (overlay-get
                                      fold
                                      'invisible)
                                     ahs-opened-overlay-list)))))
                            '(temporary open)))"##,
        expect![[
            r#"OK ((temporary (8 nil 1) ((:temporary nil) (:temporary t)) fixture nil) (open (8 nil 1) ((:temporary nil) :permanent-open) nil nil))"#
        ]],
    )
}

fn auto_highlight_symbol_selection_preserves_intra_symbol_cursor_offset() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_selection_preserves_intra_symbol_cursor_offset",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (insert "alpha alpha")
                             (let ((window
                                    (selected-window))
                                   (current
                                    (make-overlay 1 6))
                                   (next
                                    (make-overlay 7 12)))
                               (overlay-put
                                current
                                'window
                                window)
                               (overlay-put
                                next
                                'window
                                window)
                               (setq
                                ahs-current-overlay
                                (list current)
                                ahs-overlay-list
                                (list next))
                               (goto-char 4)
                               (ahs-forward)
                               (list
                                (point)
                                (current-column)
                                (overlay-start current)
                                (overlay-end current)))))"##,
        expect!["OK (10 9 7 12)"],
    )
}

pub(super) fn navigation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_highlight_symbol_forward_and_backward_navigation_wrap_real_matches(),
        auto_highlight_symbol_definition_navigation_skips_ordinary_occurrences_and_wraps(),
        auto_highlight_symbol_back_to_start_restores_original_match_after_navigation(),
        auto_highlight_symbol_navigation_predicates_classify_position_definition_display_and_hidden(
        ),
        auto_highlight_symbol_skip_invisible_policy_bypasses_hidden_match_during_navigation(),
        auto_highlight_symbol_immediate_invisible_policy_opens_selected_fold_then_recloses_old_fold(
        ),
        auto_highlight_symbol_temporary_and_open_invisible_policies_differ_on_cleanup(),
        auto_highlight_symbol_selection_preserves_intra_symbol_cursor_offset(),
    ]
}
