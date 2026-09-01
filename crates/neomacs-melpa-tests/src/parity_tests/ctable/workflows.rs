use expect_test::expect;

use super::ParityBatchCase;

fn easy_model_and_text_rendering_preserve_headers_alignment_unicode_and_truncation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "easy_model_and_text_rendering_preserve_headers_alignment_unicode_and_truncation",
        r##"
(let* ((rows '((17 "queued" "Deploy λ service")
               (3 "done" "Short")
               (42 "failed" "A very long operational description")))
       (model (ctbl:make-model-from-list rows '("ID" "State" "Description")))
       (columns (ctbl:model-column-model model)))
  (setf (ctbl:cmodel-align (nth 0 columns)) 'right
        (ctbl:cmodel-align (nth 1 columns)) 'center
        (ctbl:cmodel-align (nth 2 columns)) 'left
        (ctbl:cmodel-max-width (nth 2 columns)) 18)
  (let ((text (ctbl:get-table-text :width 48 :height 20 :model model)))
    (list :columns
          (mapcar (lambda (column)
                    (list (ctbl:cmodel-title column)
                          (ctbl:cmodel-min-width column)
                          (ctbl:cmodel-max-width column)
                          (ctbl:cmodel-align column)))
                  columns)
          :rows (ctbl:model-row-length model)
          :cols (ctbl:model-column-length model)
          :text (substring-no-properties text))))
"##,
        expect![[
            r#"OK (:columns (("ID" 5 nil right) ("State" 5 nil center) ("Description" 5 18 left)) :rows 3 :cols 3 :text "|     ID      |    State    |   Description    |\n+-------------+-------------+------------------+\n|           17|   queued    |Deploy λ service  |\n|            3|    done     |Short             |\n|           42|   failed    |A very long operat|\n")"#
        ]],
    )
}

fn multi_column_sorting_and_header_actions_toggle_primary_sort_direction() -> ParityBatchCase {
    ParityBatchCase::value(
        "multi_column_sorting_and_header_actions_toggle_primary_sort_direction",
        r##"
(let* ((columns
        (list
         (make-ctbl:cmodel :title "Priority"
                           :sorter #'ctbl:sort-number-lessp)
         (make-ctbl:cmodel :title "Owner"
                           :sorter #'ctbl:sort-string-lessp)
         (make-ctbl:cmodel :title "Task"
                           :sorter #'ctbl:sort-string-lessp)))
       (rows '((2 "zoe" "test")
               (1 "zoe" "deploy")
               (1 "amy" "review")
               (2 "amy" "build")))
       (model (make-ctbl:model :column-model columns
                               :data rows
                               :sort-state '(1 2)))
       component)
  (unwind-protect
      (progn
        (setq component
              (ctbl:create-table-component-buffer
               :width 40 :height 20 :model model))
        (let ((ascending (copy-tree (ctbl:component-sorted-data component)))
              states descending restored)
          (neomacs-ctable-test-fire-header component 0)
          (setq states (list (copy-sequence (ctbl:model-sort-state model))))
          (setq descending (copy-tree (ctbl:component-sorted-data component)))
          (neomacs-ctable-test-fire-header component 0)
          (push (copy-sequence (ctbl:model-sort-state model)) states)
          (setq restored (copy-tree (ctbl:component-sorted-data component)))
          (list :ascending ascending
                :states (nreverse states)
                :descending descending
                :restored restored
                :text (neomacs-ctable-test-buffer-text component))))
    (neomacs-ctable-test-kill component)))
"##,
        expect![[
            r#"OK (:ascending ((1 "amy" "review") (1 "zoe" "deploy") (2 "amy" "build") (2 "zoe" "test")) :states ((-1 2) (1 2)) :descending ((2 "amy" "build") (2 "zoe" "test") (1 "amy" "review") (1 "zoe" "deploy")) :restored ((1 "amy" "review") (1 "zoe" "deploy") (2 "amy" "build") (2 "zoe" "test")) :text "|   Priority   |   Owner   |   Task    |\n+--------------+-----------+-----------+\n|             1|        amy|     review|\n|             1|        zoe|     deploy|\n|             2|        amy|      build|\n|             2|        zoe|       test|\n")"#
        ]],
    )
}

fn buffer_navigation_selection_and_click_hooks_follow_visible_sorted_rows() -> ParityBatchCase {
    ParityBatchCase::value(
        "buffer_navigation_selection_and_click_hooks_follow_visible_sorted_rows",
        r##"
(let* ((model
        (ctbl:make-model-from-list
         '(("alpha" 30 "queued")
           ("beta" 10 "done")
           ("gamma" 20 "running"))
         '("Name" "Age" "State")))
       component events)
  (setf (ctbl:model-sort-state model) '(2)
        (ctbl:cmodel-sorter (nth 1 (ctbl:model-column-model model)))
        #'ctbl:sort-number-lessp)
  (unwind-protect
      (progn
        (setq component
              (ctbl:create-table-component-buffer
               :width 42 :height 20 :model model))
        (ctbl:cp-add-selection-change-hook
         component
         (lambda ()
           (push (list :selection (ctbl:cp-get-selected component)
                       :row (ctbl:cp-get-selected-data-row component)
                       :cell (ctbl:cp-get-selected-data-cell component))
                 events)))
        (ctbl:cp-add-click-hook
         component
         (lambda ()
           (push (list :click (ctbl:cp-get-selected component)
                       :cell (ctbl:cp-get-selected-data-cell component))
                 events)))
        (with-current-buffer (ctbl:cp-get-buffer component)
          (goto-char (point-min))
          (ctbl:navi-goto-cell '(1 . 1))
          (ctbl:navi-move-right)
          (ctbl:navi-move-down)
          (ctbl:navi-move-left-most)
          (ctbl:cp-fire-click-hooks component))
        (list :selected (ctbl:cp-get-selected component)
              :row (ctbl:cp-get-selected-data-row component)
              :cell (ctbl:cp-get-selected-data-cell component)
              :events (nreverse events)
              :overlays
              (with-current-buffer (ctbl:cp-get-buffer component)
                (mapcar
                 (lambda (overlay)
                   (list (overlay-get overlay 'face)
                         (buffer-substring-no-properties
                          (overlay-start overlay) (overlay-end overlay))))
                 (sort (copy-sequence
                        (ctbl:dest-select-ol
                         (ctbl:component-dest component)))
                       (lambda (left right)
                         (< (overlay-start left) (overlay-start right))))))))
    (neomacs-ctable-test-kill component)))
"##,
        expect![[
            r#"OK (:selected #3=(2 . 0) :row #2=("alpha" 30 "queued") :cell "alpha" :events ((:selection (1 . 1) :row #1=("gamma" 20 "running") :cell 20) (:selection (1 . 2) :row #1# :cell "running") (:selection (2 . 2) :row #2# :cell "queued") (:selection #3# :row #2# :cell "alpha") (:click #3# :cell "alpha")) :overlays ((ctbl:face-cell-select "       alpha") (ctbl:face-row-select "          30") (ctbl:face-row-select "        queued")))"#
        ]],
    )
}

fn model_replacement_and_destructive_updates_fire_hooks_and_preserve_selection() -> ParityBatchCase
{
    ParityBatchCase::value(
        "model_replacement_and_destructive_updates_fire_hooks_and_preserve_selection",
        r##"
(let* ((first (ctbl:make-model-from-list
               '(("alpha" 1) ("beta" 2))
               '("Name" "Count")))
       (second (ctbl:make-model-from-list
                '(("alpha" 10) ("beta" 20) ("gamma" 30))
                '("Name" "Count")))
       component events)
  (unwind-protect
      (progn
        (setq component
              (ctbl:create-table-component-buffer
               :width 30 :height 20 :model first))
        (ctbl:cp-add-update-hook
         component
         (lambda ()
           (push (list :rows
                       (length (ctbl:component-sorted-data component))
                       :selected (ctbl:cp-get-selected component))
                 events)))
        (ctbl:cp-set-selected-cell component '(1 . 1))
        (ctbl:cp-set-model component second)
        (setf (ctbl:model-data second)
              '(("alpha" 100) ("beta" 200) ("gamma" 300) ("delta" 400)))
        (ctbl:cp-update component)
        (list :model-is-second (eq (ctbl:cp-get-model component) second)
              :selected (ctbl:cp-get-selected component)
              :row (ctbl:cp-get-selected-data-row component)
              :cell (ctbl:cp-get-selected-data-cell component)
              :events (nreverse events)
              :text (neomacs-ctable-test-buffer-text component)))
    (neomacs-ctable-test-kill component)))
"##,
        expect![[
            r#"OK (:model-is-second t :selected #1=(0 . 0) :row ("alpha" 100) :cell "alpha" :events ((:rows 3 :selected #1#) (:rows 4 :selected #1#)) :text "|     Name     |    Count    |\n+--------------+-------------+\n|         alpha|          100|\n|          beta|          200|\n|         gamma|          300|\n|         delta|          400|\n")"#
        ]],
    )
}

fn embedded_region_component_updates_without_touching_surrounding_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "embedded_region_component_updates_without_touching_surrounding_document",
        r##"
(with-temp-buffer
  (insert "Release dashboard\n\n")
  (let* ((begin (point))
         (model (ctbl:make-model-from-list
                 '(("api" "green") ("worker" "yellow"))
                 '("Service" "Health")))
         (component
          (ctbl:create-table-component-region
           :width 32 :height 20 :model model)))
    (goto-char (point-max))
    (insert "\nOperator notes\n")
    (let ((initial (buffer-substring-no-properties (point-min) (point-max)))
          component-at-cell)
      (goto-char begin)
      (setq component-at-cell (ctbl:cp-get-component))
      (setf (ctbl:model-data model)
            '(("api" "green") ("worker" "green") ("cron λ" "red")))
      (ctbl:cp-update component)
      (list :same-component (eq component component-at-cell)
            :initial initial
            :updated (buffer-substring-no-properties (point-min) (point-max))
            :prefix (buffer-substring-no-properties (point-min) begin)
            :suffix (buffer-substring-no-properties
                     (funcall
                      (ctbl:dest-max-func (ctbl:component-dest component)))
                     (point-max))))))
"##,
        expect![[
            r#"OK (:same-component t :initial "Release dashboard\n\n|    Service    |    Health    |\n+---------------+--------------+\n|            api|         green|\n|         worker|        yellow|\n \nOperator notes\n" :updated "Release dashboard\n\n|    Service    |    Health    |\n+---------------+--------------+\n|            api|         green|\n|         worker|         green|\n|         cron λ|           red|\n \nOperator notes\n" :prefix "Release dashboard\n\n" :suffix "\nOperator notes\n")"#
        ]],
    )
}

fn formatting_and_custom_render_parameters_control_exact_table_bytes() -> ParityBatchCase {
    ParityBatchCase::value(
        "formatting_and_custom_render_parameters_control_exact_table_bytes",
        r##"
(let* ((model
        (make-ctbl:model
         :column-model
         (list (make-ctbl:cmodel :title "Key" :align 'left :min-width 6)
               (make-ctbl:cmodel :title "Value" :align 'center :min-width 8))
         :data '(("retry" "enabled") ("owner" "λ-team"))))
       (param (copy-ctbl:param ctbl:default-rendering-param)))
  (setf (ctbl:param-draw-vlines param) 'all
        (ctbl:param-draw-hlines param) 'all
        (ctbl:param-vertical-line param) ?!
        (ctbl:param-horizontal-line param) ?=
        (ctbl:param-left-top-corner param) ?1
        (ctbl:param-top-junction param) ?2
        (ctbl:param-right-top-corner param) ?3
        (ctbl:param-left-junction param) ?4
        (ctbl:param-cross-junction param) ?5
        (ctbl:param-right-junction param) ?6
        (ctbl:param-left-bottom-corner param) ?7
        (ctbl:param-bottom-junction param) ?8
        (ctbl:param-right-bottom-corner param) ?9)
  (list
   :left (substring-no-properties (ctbl:format-left 8 "λ-value"))
   :center (substring-no-properties (ctbl:format-center 9 "mid"))
   :right (substring-no-properties (ctbl:format-right 7 "42"))
   :truncate
   (let ((value (ctbl:format-truncate "first line\nsecond line" 10 t)))
     (list (substring-no-properties value)
           (get-text-property 0 'help-echo value)))
   :text
   (substring-no-properties
    (ctbl:get-table-text :width 20 :height 20 :model model :param param))))
"##,
        expect![[
            r#"OK (:left "λ-value " :center "   mid   " :right "     42" :truncate ("first lin…" "first line second line") :text "1========2=========3\n!  Key   !  Value  !\n4========5=========6\n!retry   ! enabled !\n4========5=========6\n!owner   ! λ-team  !\n7========8=========9\n")"#
        ]],
    )
}

fn async_wrapper_pages_rows_and_reset_replays_the_initial_page() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_wrapper_pages_rows_and_reset_replays_the_initial_page",
        r##"
(let* ((rows '((0 "zero") (1 "one") (2 "two") (3 "three") (4 "four")))
       (model (ctbl:async-model-wrapper rows 2 2))
       (request (ctbl:async-model-request model))
       pages)
  (funcall request 0 2 (lambda (page) (push page pages))
           (lambda (error) (push (list :error error) pages)))
  (funcall request 2 2 (lambda (page) (push page pages))
           (lambda (error) (push (list :error error) pages)))
  (funcall request 4 2 (lambda (page) (push page pages))
           (lambda (error) (push (list :error error) pages)))
  (funcall request 6 2 (lambda (page) (push page pages))
           (lambda (error) (push (list :error error) pages)))
  (let ((before-reset (nreverse pages)))
    (funcall (ctbl:async-model-reset model))
    (setq pages nil)
    (funcall request 0 3 (lambda (page) (push page pages))
             (lambda (error) (push (list :error error) pages)))
    (list :init (ctbl:async-model-init-num model)
          :more (ctbl:async-model-more-num model)
          :pages before-reset
          :after-reset (nreverse pages))))
"##,
        expect![[
            r#"OK (:init 2 :more 2 :pages ((#1=(0 "zero") #2=(1 "one")) (#3=(2 "two") (3 "three")) ((4 "four")) nil) :after-reset ((#1# #2# #3#)))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        easy_model_and_text_rendering_preserve_headers_alignment_unicode_and_truncation(),
        multi_column_sorting_and_header_actions_toggle_primary_sort_direction(),
        buffer_navigation_selection_and_click_hooks_follow_visible_sorted_rows(),
        model_replacement_and_destructive_updates_fire_hooks_and_preserve_selection(),
        embedded_region_component_updates_without_touching_surrounding_document(),
        formatting_and_custom_render_parameters_control_exact_table_bytes(),
        async_wrapper_pages_rows_and_reset_replays_the_initial_page(),
    ]
}
