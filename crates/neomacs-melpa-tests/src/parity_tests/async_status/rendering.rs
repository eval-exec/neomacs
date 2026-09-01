use super::ParityBatchCase;
use expect_test::expect;

fn truncation_preserves_short_strings_and_enforces_normal_maximum_widths() -> ParityBatchCase {
    ParityBatchCase::value(
        "truncation_preserves_short_strings_and_enforces_normal_maximum_widths",
        r##"(mapcar
 (lambda (case)
   (apply #'async-status--print-truncated-string case))
 '(("" 10)
   ("short" 5)
   ("sixsix" 6)
   ("1234567" 6)
   ("abcdefghijklmnop" 10)
   ("雪λalphaβgamma" 8)))"##,
        expect!["OK (\"\" \"short\" \"sixsix\" \"123...\" \"abcdefg...\" \"雪λalp...\")"],
    )
}

fn truncation_exposes_the_package_behavior_for_tiny_and_negative_limits() -> ParityBatchCase {
    ParityBatchCase::value(
        "truncation_exposes_the_package_behavior_for_tiny_and_negative_limits",
        r##"(mapcar
 (lambda (limit)
   (async-status-test-error
    (lambda ()
      (async-status--print-truncated-string
       "abcdef" limit))))
 '(3 2 1 0 -1 -10))"##,
        expect![[
            r#"OK ((:ok "...") (:ok "abcde...") (:ok "abcd...") (:ok "abc...") (:ok "ab...") (:error args-out-of-range ("abcdef" 0 -13)))"#
        ]],
    )
}

fn redraw_formats_numeric_progress_and_forwards_complete_svg_geometry() -> ParityBatchCase {
    ParityBatchCase::value(
        "redraw_formats_numeric_progress_and_forwards_complete_svg_geometry",
        r##"(let ((buffer (get-buffer-create "*async-status*"))
      (item
       (make-async-status--item
        :msg-id "compile"
        :progress 0.375
        :label "Compile"))
      inserted-images)
  (setq async-status-test-svg-calls nil)
  (with-current-buffer buffer
    (erase-buffer))
  (cl-letf (((symbol-function 'window-font-height)
             (lambda (&optional _window) 10))
            ((symbol-function 'window-font-width)
             (lambda (&optional _window) 5))
            ((symbol-function 'insert-image)
             (lambda (image &rest _arguments)
               (push image inserted-images)
               (insert "<image>"))))
    (async-status--redraw-item item))
  (prog1
      (list
       (with-current-buffer buffer
         (buffer-string))
       (nreverse async-status-test-svg-calls)
       (nreverse inserted-images))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK ("Compile                   <image>\n" ((0.375 (:background "test-background" :foreground "test-foreground") :height 2.0 :width 30.0)) ((image :type svg :progress 0.375)))"#
        ]],
    )
}

fn redraw_converts_string_progress_and_truncates_long_labels() -> ParityBatchCase {
    ParityBatchCase::value(
        "redraw_converts_string_progress_and_truncates_long_labels",
        r##"(let ((buffer (get-buffer-create "*async-status*"))
      (item
       (make-async-status--item
        :progress "0.875"
        :label "A very long compilation target label"))
      progress)
  (with-current-buffer buffer
    (erase-buffer))
  (cl-letf (((symbol-function 'window-font-height)
             (lambda (&optional _window) 20))
            ((symbol-function 'window-font-width)
             (lambda (&optional _window) 10))
            ((symbol-function 'svg-lib-progress-bar)
             (lambda (value &rest _arguments)
               (setq progress value)
               '(image :type svg)))
            ((symbol-function 'insert-image)
             (lambda (&rest _arguments)
               (insert "[bar]"))))
    (async-status--redraw-item item))
  (prog1
      (list
       progress
       (with-current-buffer buffer
         (buffer-string)))
    (kill-buffer buffer)))"##,
        expect![[r#"OK (0.875 "A very long compil...     [bar]\n")"#]],
    )
}

fn redraw_treats_non_numeric_progress_strings_as_zero() -> ParityBatchCase {
    ParityBatchCase::value(
        "redraw_treats_non_numeric_progress_strings_as_zero",
        r##"(let ((buffer (get-buffer-create "*async-status*"))
      (item
       (make-async-status--item
        :progress "pending"
        :label "Waiting"))
      observed)
  (with-current-buffer buffer
    (erase-buffer))
  (cl-letf (((symbol-function 'svg-lib-progress-bar)
             (lambda (value &rest _arguments)
               (setq observed value)
               '(image :type svg)))
            ((symbol-function 'insert-image)
             (lambda (&rest _arguments)
               (insert "[zero]"))))
    (async-status--redraw-item item))
  (prog1
      (list
       observed
       (with-current-buffer buffer
         (buffer-string)))
    (kill-buffer buffer)))"##,
        expect![[r#"OK (0 "Waiting                   [zero]\n")"#]],
    )
}

fn refresh_erases_stale_content_and_redraws_items_in_list_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "refresh_erases_stale_content_and_redraws_items_in_list_order",
        r##"(let ((buffer (get-buffer-create "*async-status*"))
      calls)
  (setq async-status--shown-items
        (list
         (make-async-status--item
          :msg-id "second"
          :label "Second")
         (make-async-status--item
          :msg-id "first"
          :label "First")))
  (with-current-buffer buffer
    (erase-buffer)
    (insert "stale"))
  (cl-letf (((symbol-function 'async-status--redraw-item)
             (lambda (item)
               (push
                (async-status--item-msg-id item)
                calls)
               (with-current-buffer "*async-status*"
                 (insert
                  (async-status--item-label item)
                  "\n")))))
    (async-status--refresh-status-bar))
  (prog1
      (list
       (nreverse calls)
       (with-current-buffer buffer
         (buffer-string)))
    (setq async-status--shown-items nil)
    (kill-buffer buffer)))"##,
        expect![[r#"OK (("second" "first") "Second\nFirst\n")"#]],
    )
}

fn show_forwards_the_complete_posframe_contract_then_refreshes() -> ParityBatchCase {
    ParityBatchCase::value(
        "show_forwards_the_complete_posframe_contract_then_refreshes",
        r##"(let ((async-status-indicator-width 44)
      (async-status--shown-items
       (list
        (make-async-status--item :msg-id "one")
        (make-async-status--item :msg-id "two")))
      calls)
  (cl-letf (((symbol-function 'foreground-color-at-point)
             (lambda () "test-fg"))
            ((symbol-function 'posframe-show)
             (lambda (&rest arguments)
               (push (cons :show arguments) calls)
               :shown))
            ((symbol-function 'async-status--refresh-status-bar)
             (lambda ()
               (push :refresh calls))))
    (let ((result (async-status-show)))
      (list
       (and
        (eq (car result) :refresh)
        (= (length result) 2))
       (nreverse calls)))))"##,
        expect![[
            r#"OK (t ((:show "*async-status*" :border-color "test-fg" :border-width 2 :left-fringe 10 :right-fringe 10 :min-width 44 :max-width 44 :min-height 2 :max-height 2 :poshandler posframe-poshandler-frame-top-center) :refresh))"#
        ]],
    )
}

fn show_uses_zero_height_for_an_empty_indicator_collection() -> ParityBatchCase {
    ParityBatchCase::value(
        "show_uses_zero_height_for_an_empty_indicator_collection",
        r##"(let ((async-status--shown-items nil)
      observed)
  (cl-letf (((symbol-function 'foreground-color-at-point)
             (lambda () "fg"))
            ((symbol-function 'posframe-show)
             (lambda (&rest arguments)
               (setq observed arguments)))
            ((symbol-function 'async-status--refresh-status-bar)
             #'ignore))
    (async-status-show)
    (list
     (plist-get (cdr observed) :min-height)
     (plist-get (cdr observed) :max-height)
     (car observed))))"##,
        expect!["OK (0 0 \"*async-status*\")"],
    )
}

fn hide_obeys_force_and_empty_collection_rules_without_extra_calls() -> ParityBatchCase {
    ParityBatchCase::value(
        "hide_obeys_force_and_empty_collection_rules_without_extra_calls",
        r##"(let (calls active-results empty-results)
  (cl-letf (((symbol-function 'posframe-hide)
             (lambda (&rest arguments)
               (push arguments calls)
               :hidden)))
    (let ((async-status--shown-items
           (list
            (make-async-status--item
             :msg-id "active"))))
      (setq active-results
            (list
             (async-status-hide)
             (async-status-hide nil)
             (async-status-hide t))))
    (let ((async-status--shown-items nil))
      (setq empty-results
            (list
             (async-status-hide)
             (async-status-hide nil)
             (async-status-hide t))))
    (list
     active-results
     empty-results
     (nreverse calls))))"##,
        expect![[
            r#"OK ((nil nil :hidden) (:hidden :hidden :hidden) (("*async-status*") ("*async-status*") ("*async-status*") ("*async-status*")))"#
        ]],
    )
}

pub(super) fn rendering_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        truncation_preserves_short_strings_and_enforces_normal_maximum_widths(),
        truncation_exposes_the_package_behavior_for_tiny_and_negative_limits(),
        redraw_formats_numeric_progress_and_forwards_complete_svg_geometry(),
        redraw_converts_string_progress_and_truncates_long_labels(),
        redraw_treats_non_numeric_progress_strings_as_zero(),
        refresh_erases_stale_content_and_redraws_items_in_list_order(),
        show_forwards_the_complete_posframe_contract_then_refreshes(),
        show_uses_zero_height_for_an_empty_indicator_collection(),
        hide_obeys_force_and_empty_collection_rules_without_extra_calls(),
    ]
}
