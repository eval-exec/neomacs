use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, TABLIST_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TABLIST_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const TABLIST_TEST_PRELUDE: &str = r#####"
(require 'cl-lib)
(require 'tablist)

(defun tablist-parity-entries ()
  (list
   (list 'docs   ["Docs"   "ready"   "0" "writers"])
   (list 'api    ["API"    "failed"  "3" "ops-east"])
   (list 'web    ["Web"    "running" "1" "frontend"])
   (list 'deploy ["Deploy" "failed"  "2" "ops-west"])))

(defun tablist-parity-setup (&optional entries)
  (tablist-mode)
  (setq tabulated-list-format
        (vector
         (list "Task" 10 t)
         (list "Status" 10 t)
         (list "Retries" 8
               (tablist-generate-sorter 2 #'< #'string-to-number)
               :right-align t)
         (list "Owner" 12 t)))
  (setq tabulated-list-padding 2)
  (setq tablist-major-columns '(0 1))
  (setq tabulated-list-entries (or entries (tablist-parity-entries)))
  (tabulated-list-init-header)
  (tabulated-list-print)
  (goto-char (point-min)))

(defun tablist-parity-row-state ()
  (save-excursion
    (goto-char (point-min))
    (let (rows)
      (while (not (eobp))
        (let ((id (tabulated-list-get-id (line-beginning-position))))
          (when id
            (push
             (list
              :id id
              :mark (char-to-string (char-after (line-beginning-position)))
              :hidden (and (invisible-p (line-beginning-position)) t)
              :text (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position))
              :entry (append (tabulated-list-get-entry
                              (line-beginning-position)) nil))
             rows)))
        (forward-line))
      (nreverse rows))))

(defun tablist-parity-visible-ids ()
  (cl-loop for row in (tablist-parity-row-state)
           unless (plist-get row :hidden)
           collect (plist-get row :id)))

(defun tablist-parity-goto-id (id)
  (goto-char (point-min))
  (while (and (not (eobp))
              (not (equal id (tabulated-list-get-id))))
    (forward-line))
  (unless (equal id (tabulated-list-get-id))
    (error "No parity row named %s" id))
  (point))
"#####;

fn tablist_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TABLIST_MELPA_PIN, "tablist.el")
        .expect("prepare pinned Tablist source below ./tmp")
        .with_prelude(TABLIST_TEST_PRELUDE)
        .with_timeout(TABLIST_TEST_TIMEOUT)
}

fn release_queue_marks_flags_toggles_and_survives_refresh_by_entry_id() -> ParityBatchCase {
    let elisp_form = r#####"
(with-temp-buffer
  (tablist-parity-setup)
  (tablist-parity-goto-id 'docs)
  (tablist-mark-forward 1)
  (tablist-flag-forward 1)
  (tablist-parity-goto-id 'deploy)
  (tablist-mark-forward 1)
  (let ((initial (tablist-parity-row-state))
        (starred (mapcar #'car (tablist-get-marked-items))))
    (tablist-change-marks ?D ?*)
    (let ((changed (tablist-parity-row-state))
          (all-marked (mapcar #'car (tablist-get-marked-items))))
      (tablist-parity-goto-id 'api)
      (tablist-move-to-column 1)
      (setq tabulated-list-entries (reverse tabulated-list-entries))
      (tablist-revert)
      (let ((refreshed (tablist-parity-row-state))
            (point-state
             (list (tabulated-list-get-id)
                   (tablist-current-column)
                   (current-column))))
        (tablist-toggle-marks)
        (list
         :initial initial
         :starred starred
         :changed changed
         :all-marked all-marked
         :refreshed refreshed
         :point point-state
         :toggled (tablist-parity-row-state)
         :marked-after-toggle
         (mapcar #'car (tablist-get-marked-items)))))))
"#####;
    let expect = expect![[
        r####"OK (:initial ((:id docs :mark "*" :hidden nil :text "* Docs       ready             0 writers" :entry ("Docs" "ready" "0" "writers")) (:id api :mark "D" :hidden nil :text "D API        failed            3 ops-east" :entry ("API" "failed" "3" "ops-east")) (:id web :mark " " :hidden nil :text "  Web        running           1 frontend" :entry ("Web" "running" "1" "frontend")) (:id deploy :mark "*" :hidden nil :text "* Deploy     failed            2 ops-west" :entry ("Deploy" "failed" "2" "ops-west"))) :starred (docs deploy) :changed ((:id docs :mark "*" :hidden nil :text "* Docs       ready             0 writers" :entry ("Docs" "ready" "0" "writers")) (:id api :mark "*" :hidden nil :text "* API        failed            3 ops-east" :entry ("API" "failed" "3" "ops-east")) (:id web :mark " " :hidden nil :text "  Web        running           1 frontend" :entry ("Web" "running" "1" "frontend")) (:id deploy :mark "*" :hidden nil :text "* Deploy     failed            2 ops-west" :entry ("Deploy" "failed" "2" "ops-west"))) :all-marked (docs api deploy) :refreshed ((:id deploy :mark "*" :hidden nil :text "* Deploy     failed            2 ops-west" :entry ("Deploy" "failed" "2" "ops-west")) (:id web :mark " " :hidden nil :text "  Web        running           1 frontend" :entry ("Web" "running" "1" "frontend")) (:id api :mark "*" :hidden nil :text "* API        failed            3 ops-east" :entry ("API" "failed" "3" "ops-east")) (:id docs :mark "*" :hidden nil :text "* Docs       ready             0 writers" :entry ("Docs" "ready" "0" "writers"))) :point (api 1 13) :toggled ((:id deploy :mark " " :hidden nil :text "  Deploy     failed            2 ops-west" :entry ("Deploy" "failed" "2" "ops-west")) (:id web :mark "*" :hidden nil :text "* Web        running           1 frontend" :entry ("Web" "running" "1" "frontend")) (:id api :mark " " :hidden nil :text "  API        failed            3 ops-east" :entry ("API" "failed" "3" "ops-east")) (:id docs :mark " " :hidden nil :text "  Docs       ready             0 writers" :entry ("Docs" "ready" "0" "writers"))) :marked-after-toggle (web))"####
    ]];
    ParityBatchCase::value(
        "release_queue_marks_flags_toggles_and_survives_refresh_by_entry_id",
        elisp_form,
        expect,
    )
}

fn complex_filter_language_round_trips_and_selects_real_rows() -> ParityBatchCase {
    let elisp_form = r#####"
(with-temp-buffer
  (tablist-parity-setup)
  (let* ((source
          "Status == failed && (Retries >= 3 || Owner =~ \"^ops-w\")")
         (parsed (tablist-filter-parse source))
         (unparsed (tablist-filter-unparse parsed))
         (round-trip (tablist-filter-parse unparsed))
         (named
          `(("actionable" . ,parsed)
            ("owned-by-ops" . (=~ "Owner" "^ops-"))))
         (entries (tablist-parity-entries)))
    (list
     :parsed parsed
     :unparsed unparsed
     :round-trip-equal (equal parsed round-trip)
     :matches
     (mapcar
      (lambda (entry)
        (list (car entry)
              (and (tablist-filter-eval parsed (car entry) (cadr entry)) t)
              (and (tablist-filter-eval "actionable"
                                        (car entry) (cadr entry) named) t)
              (and (tablist-filter-eval "owned-by-ops"
                                        (car entry) (cadr entry) named) t)))
      entries)
     :combinators
     (list
      (tablist-filter-push '(== "Status" "failed")
                           '(>= "Retries" "2"))
      (tablist-filter-push '(== "Status" "failed")
                           '(== "Owner" "frontend") t)
      (tablist-filter-negate '(== "Status" "failed"))
      (tablist-filter-pop
       '(and (== "Status" "failed") (>= "Retries" "2")))
      (tablist-filter-map
       (lambda (leaf) (list :leaf leaf))
       '(or (== "Status" "failed") (not "saved"))))
     :errors
     (list
      (condition-case error
          (list :ok (tablist-filter-parse "Status =="))
        (error (list :error (car error) (cadr error))))
      (condition-case error
          (list :ok
                (tablist-filter-eval
                 '(mystery "Status" "x") 'api
                 ["API" "failed" "3" "ops-east"]))
        (error (list :error (car error) (cadr error))))
      (condition-case error
          (list :ok (tablist-filter-unparse '(and only-one)))
        (error (list :error (car error) (cadr error))))))))
"#####;
    let expect = expect![[
        r####"OK (:parsed (and (== "Status" "failed") (or (>= "Retries" "3") (=~ "Owner" "^ops-w"))) :unparsed "Status == failed && (Retries >= 3 || Owner =~ ^ops-w)" :round-trip-equal t :matches ((docs nil nil nil) (api t t t) (web nil nil nil) (deploy t t t)) :combinators ((and (== "Status" "failed") (>= "Retries" "2")) (or (== "Status" "failed") (== "Owner" "frontend")) (not (== "Status" "failed")) (>= "Retries" "2") (or (:leaf (== "Status" "failed")) (not (:leaf "saved")))) :errors ((:error error "Syntax error, unexpected end of input(nil), expecting operand") (:error error "Undefined binary operator: mystery") (:error error "Invalid filter: (and only-one)")))"####
    ]];
    ParityBatchCase::value(
        "complex_filter_language_round_trips_and_selects_real_rows",
        elisp_form,
        expect,
    )
}

fn live_filters_hide_unmark_suspend_resume_and_expand_named_filters() -> ParityBatchCase {
    let elisp_form = r#####"
(with-temp-buffer
  (tablist-parity-setup)
  (tablist-parity-goto-id 'docs)
  (tablist-mark-forward 1)
  (tablist-parity-goto-id 'api)
  (tablist-mark-forward 1)
  (tablist-push-filter '(=~ "Status" "^failed"))
  (tablist-push-filter '(>= "Retries" "2"))
  (let ((filtered
         (list
          :filter tablist-current-filter
          :display (tablist-filter-unparse tablist-current-filter)
          :visible (tablist-parity-visible-ids)
          :rows (tablist-parity-row-state)
          :marked (mapcar #'car (tablist-get-marked-items)))))
    (tablist-suspend-filter t)
    (let ((suspended
           (list tablist-filter-suspended
                 (tablist-parity-visible-ids))))
      (tablist-suspend-filter nil)
      (let ((resumed (tablist-parity-visible-ids)))
        (setq tablist-named-filter nil)
        (tablist-put-named-filter
         "ops failures"
         '(and (== "Status" "failed") (=~ "Owner" "^ops-")))
        (setq tablist-current-filter "ops failures")
        (tablist-apply-filter)
        (let ((named
               (list
                (tablist-filter-names)
                (copy-tree (tablist-get-named-filter "ops failures"))
                (tablist-parity-visible-ids)
                (tablist-filter-unparse tablist-current-filter))))
          (tablist-deconstruct-named-filter)
          (let ((expanded (copy-tree tablist-current-filter)))
            (tablist-name-current-filter "release blockers")
            (tablist-delete-named-filter "ops failures")
            (list
             :filtered filtered
             :suspended suspended
             :resumed resumed
             :named named
             :expanded expanded
             :renamed
             (list tablist-current-filter
                   (tablist-filter-names)
                   (copy-tree
                    (tablist-get-named-filter "release blockers"))))))))))
"#####;
    let expect = expect![[
        r####"OK (:filtered (:filter (and (=~ "Status" "^failed") (>= "Retries" "2")) :display "Status =~ ^failed && Retries >= 2" :visible (api deploy) :rows ((:id docs :mark " " :hidden t :text "  Docs       ready             0 writers" :entry ("Docs" "ready" "0" "writers")) (:id api :mark "*" :hidden nil :text "* API        failed            3 ops-east" :entry ("API" "failed" "3" "ops-east")) (:id web :mark " " :hidden t :text "  Web        running           1 frontend" :entry ("Web" "running" "1" "frontend")) (:id deploy :mark " " :hidden nil :text "  Deploy     failed            2 ops-west" :entry ("Deploy" "failed" "2" "ops-west"))) :marked (api)) :suspended (t (docs api web deploy)) :resumed (api deploy) :named (("ops failures") (and (== "Status" "failed") (=~ "Owner" "^ops-")) (api deploy) "\"ops failures\"") :expanded (and (== "Status" "failed") (=~ "Owner" "^ops-")) :renamed ("release blockers" ("release blockers") (and (== "Status" "failed") (=~ "Owner" "^ops-"))))"####
    ]];
    ParityBatchCase::value(
        "live_filters_hide_unmark_suspend_resume_and_expand_named_filters",
        elisp_form,
        expect,
    )
}

fn in_place_sort_resize_and_column_navigation_preserve_selection_and_marks() -> ParityBatchCase {
    let elisp_form = r#####"
(with-temp-buffer
  (tablist-parity-setup)
  (tablist-parity-goto-id 'api)
  (tablist-mark-forward 1)
  (tablist-parity-goto-id 'web)
  (tablist-move-to-column 2)
  (let ((before
         (list (tablist-parity-visible-ids)
               (tabulated-list-get-id)
               (tablist-current-column)
               (tablist-column-offsets))))
    (tablist-sort "Retries")
    (let ((ascending
           (list tabulated-list-sort-key
                 (tablist-parity-visible-ids)
                 (tabulated-list-get-id)
                 (tablist-current-column)
                 (mapcar #'car (tablist-get-marked-items)))))
      (tablist-sort "Retries")
      (let ((descending
             (list tabulated-list-sort-key
                   (tablist-parity-visible-ids)
                   (tabulated-list-get-id)
                   (tablist-current-column))))
        (tablist-enlarge-column 0 4)
        (tablist-shrink-column 1 3)
        (tablist-parity-goto-id 'web)
        (tablist-move-to-column 0)
        (let (columns)
          (dotimes (_ 5)
            (push (list (tablist-current-column) (current-column)) columns)
            (tablist-forward-column 1))
          (tablist-backward-column 2)
          (list
           :before before
           :ascending ascending
           :descending descending
           :format
           (mapcar
            (lambda (column)
              (list
               (nth 0 column)
               (nth 1 column)
               (cond ((eq (nth 2 column) t) t)
                     ((functionp (nth 2 column)) :function)
                     (t nil))
               (nthcdr 3 column)))
            (append tabulated-list-format nil))
           :offsets (tablist-column-offsets)
           :rows (tablist-parity-row-state)
           :column-cycle (nreverse columns)
           :after-backward
           (list (tablist-current-column) (current-column))
           :last-column-error
           (condition-case error
               (progn (tablist-enlarge-column 3 1) :unexpected-success)
             (error (list (car error) (cadr error))))))))))
"#####;
    let expect = expect![[
        r####"OK (:before ((docs api web deploy) web 2 (2 13 24 33)) :ascending (("Retries" . t) (api deploy web docs) web 2 (api)) :descending (("Retries") (docs web deploy api) web 2) :format (("Task" 14 t nil) ("Status" 7 t nil) ("Retries" 8 :function (:right-align t)) ("Owner" 12 t nil)) :offsets (2 17 25 34) :rows ((:id docs :mark " " :hidden nil :text "  Docs           ready          0 writers" :entry ("Docs" "ready" "0" "writers")) (:id web :mark " " :hidden nil :text "  Web            running        1 frontend" :entry ("Web" "running" "1" "frontend")) (:id deploy :mark " " :hidden nil :text "  Deploy         failed         2 ops-west" :entry ("Deploy" "failed" "2" "ops-west")) (:id api :mark "*" :hidden nil :text "* API            failed         3 ops-east" :entry ("API" "failed" "3" "ops-east"))) :column-cycle ((0 2) (1 17) (2 32) (3 34) (0 2)) :after-backward (3 34) :last-column-error (error "Can’t resize last column"))"####
    ]];
    ParityBatchCase::value(
        "in_place_sort_resize_and_column_navigation_preserve_selection_and_marks",
        elisp_form,
        expect,
    )
}

fn editable_rows_complete_commit_find_and_delete_through_operations_contract() -> ParityBatchCase {
    let elisp_form = r#####"
(with-temp-buffer
  (let ((entries (tablist-parity-entries))
        (events nil)
        completion-call)
    (tablist-parity-setup entries)
    (setq-local
     tablist-operations-function
     (lambda (operation &rest arguments)
       (push (cons operation arguments) events)
       (pcase operation
         ('supported-operations '(edit-column complete find-entry delete))
         ('edit-column
          (let* ((id (nth 0 arguments))
                 (column (nth 1 arguments))
                 (value (nth 2 arguments))
                 (entry (cadr (assq id entries))))
            (aset entry column value)
            entry))
         ('complete '("ops-east" "ops-west" "platform"))
         ('find-entry (car arguments))
         ('delete
          (let ((ids (car arguments)))
            (setq entries
                  (cl-remove-if (lambda (entry) (memq (car entry) ids))
                                entries))
            (setq tabulated-list-entries entries)
            ids)))))
    (tablist-parity-goto-id 'web)
    (tablist-edit-column 1)
    (let ((bounds (tablist-edit-column-bounds t))
          (inhibit-read-only t))
      (delete-region (car bounds) (cdr bounds))
      (goto-char (car bounds))
      (dolist (character (string-to-list "queued"))
        (let ((last-command-event character))
          (self-insert-command 1))))
    (tablist-edit-column-commit)
    (let ((after-edit
           (list (tablist-parity-row-state)
                 (append (cadr (assq 'web entries)) nil)
                 tablist-edit-column-minor-mode)))
      (tablist-parity-goto-id 'api)
      (tablist-edit-column 3)
      (cl-letf (((symbol-function 'completion-in-region)
                 (lambda (beg end collection &rest _arguments)
                   (setq completion-call
                         (list
                          (buffer-substring-no-properties beg end)
                          (- (point) beg)
                          collection))
                   t)))
        (tablist-edit-column-complete))
      (tablist-edit-column-quit)
      (tablist-parity-goto-id 'deploy)
      (let ((found (tablist-find-entry)))
        (tablist-put-mark)
        (cl-letf (((symbol-function 'tablist-yes-or-no-p)
                   (lambda (&rest _arguments) t)))
          (tablist-do-delete))
        (list
         :after-edit after-edit
         :completion completion-call
         :found found
         :after-delete
         (list (mapcar #'car entries)
               (tablist-parity-row-state))
         :events (nreverse events))))))
"#####;
    let expect = expect![[
        r####"OK (:after-edit (((:id docs :mark " " :hidden nil :text "  Docs       ready             0 writers" :entry ("Docs" "ready" "0" "writers")) (:id api :mark " " :hidden nil :text "  API        failed            3 ops-east" :entry ("API" "failed" "3" "ops-east")) (:id web :mark " " :hidden nil :text "  Web        queued            1 frontend" :entry ("Web" "queued" "1" "frontend")) (:id deploy :mark " " :hidden nil :text "  Deploy     failed            2 ops-west" :entry ("Deploy" "failed" "2" "ops-west"))) ("Web" "queued" "1" "frontend") nil) :completion ("ops-east" 0 ("ops-east" "ops-west" "platform")) :found deploy :after-delete ((docs api web) ((:id docs :mark " " :hidden nil :text "  Docs       ready             0 writers" :entry ("Docs" "ready" "0" "writers")) (:id api :mark " " :hidden nil :text "  API        failed            3 ops-east" :entry ("API" "failed" "3" "ops-east")) (:id web :mark " " :hidden nil :text "  Web        queued            1 frontend" :entry ("Web" "queued" "1" "frontend")))) :events ((supported-operations) (edit-column web 1 "queued") (supported-operations) (supported-operations) (complete api 3 "ops-east" 0) (supported-operations) (find-entry deploy) (supported-operations) (delete (deploy))))"####
    ]];
    ParityBatchCase::value(
        "editable_rows_complete_commit_find_and_delete_through_operations_contract",
        elisp_form,
        expect,
    )
}

fn csv_export_quotes_real_fields_and_can_include_or_exclude_filtered_rows() -> ParityBatchCase {
    let elisp_form = r#####"
(with-temp-buffer
  (let* ((entries
          (list
           (list 'plain ["Build" "ready" "0" "ci"])
           (list 'quoted ["Deploy; prod" "failed" "2" "ops \"red\""])
           (list 'button
                 (vector
                  (list "Retry; now" 'action #'ignore)
                  "running" "1" "ops-west"))))
         (visible-output (generate-new-buffer " *tablist visible csv*"))
         (all-output (generate-new-buffer " *tablist all csv*"))
         result)
    (unwind-protect
        (progn
          (tablist-parity-setup entries)
          (setq tablist-current-filter '(not (== "Status" "failed")))
          (tablist-apply-filter)
          (tablist-export-csv ";" nil nil visible-output nil)
          (tablist-export-csv "," t t all-output nil)
          (setq result
                (list
                 :visible-ids (tablist-parity-visible-ids)
                 :visible-csv
                 (with-current-buffer visible-output (buffer-string))
                 :all-csv
                 (with-current-buffer all-output (buffer-string))
                 :source-rows (tablist-parity-row-state))))
      (when (buffer-live-p visible-output) (kill-buffer visible-output))
      (when (buffer-live-p all-output) (kill-buffer all-output)))
    result))
"#####;
    let expect = expect![[
        r####"OK (:visible-ids (plain button) :visible-csv "Task;Status;Retries;Owner\nBuild;ready;0;ci\n\"Retry; now\";running;1;ops-west\n" :all-csv "\"Task\",\"Status\",\"Retries\",\"Owner\"\n\"Build\",\"ready\",\"0\",\"ci\"\n\"Deploy; prod\",\"failed\",\"2\",\"ops \"\"red\"\"\"\n\"Retry; now\",\"running\",\"1\",\"ops-west\"\n" :source-rows ((:id plain :mark " " :hidden nil :text "  Build      ready             0 ci" :entry ("Build" "ready" "0" "ci")) (:id quoted :mark " " :hidden t :text "  Deploy; prod failed            2 ops \"red\"" :entry ("Deploy; prod" "failed" "2" "ops \"red\"")) (:id button :mark " " :hidden nil :text "  Retry; now running           1 ops-west" :entry (("Retry; now" action ignore) "running" "1" "ops-west"))))"####
    ]];
    ParityBatchCase::value(
        "csv_export_quotes_real_fields_and_can_include_or_exclude_filtered_rows",
        elisp_form,
        expect,
    )
}

#[test]
fn tablist_package_batch() {
    let cases = vec![
        release_queue_marks_flags_toggles_and_survives_refresh_by_entry_id(),
        complex_filter_language_round_trips_and_selects_real_rows(),
        live_filters_hide_unmark_suspend_resume_and_expand_named_filters(),
        in_place_sort_resize_and_column_navigation_preserve_selection_and_marks(),
        editable_rows_complete_commit_find_and_delete_through_operations_contract(),
        csv_export_quotes_real_fields_and_can_include_or_exclude_filtered_rows(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Tablist parity test");
    assert_oracle_batch_cases(tablist_oracle(), test_name, "tablist_parity", &cases);
}
