use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, M_BUFFER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const M_BUFFER_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const M_BUFFER_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'm-buffer)
(require 'm-buffer-at)

(defun m-buffer-test-position (position)
  (if (markerp position)
      (marker-position position)
    position))

(defun m-buffer-test-match-records (buffer matches)
  (save-match-data
    (with-current-buffer buffer
      (mapcar
       (lambda (match)
         (list
          :positions (mapcar #'m-buffer-test-position match)
          :strings
          (cl-loop for (begin end) on match by #'cddr
                   collect
                   (and begin end
                        (buffer-substring-no-properties begin end)))))
       matches))))

(defun m-buffer-test-marker (buffer position &optional insertion-type)
  (let ((marker (set-marker (make-marker) position buffer)))
    (set-marker-insertion-type marker insertion-type)
    marker))

(defun m-buffer-test-marker-leaves (tree)
  (cond
   ((markerp tree) (list tree))
   ((consp tree)
    (append (m-buffer-test-marker-leaves (car tree))
            (m-buffer-test-marker-leaves (cdr tree))))
   (t nil)))

(defun m-buffer-test-marker-state (tree)
  (mapcar
   (lambda (marker)
     (list (marker-position marker)
           (and (marker-buffer marker) t)
           (marker-insertion-type marker)))
   (m-buffer-test-marker-leaves tree)))
"##;

fn m_buffer_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(M_BUFFER_MELPA_PIN, "m-buffer.el")
        .expect("prepare pinned m-buffer source below ./tmp")
        .with_prelude(M_BUFFER_TEST_PRELUDE)
        .with_timeout(M_BUFFER_TEST_TIMEOUT)
}

fn scoped_order_searches_respect_case_narrowing_and_restore_all_editor_state() -> ParityBatchCase {
    let elisp_form = r##"
(let ((target (generate-new-buffer " *m-buffer-orders*"))
      result)
  (unwind-protect
      (with-temp-buffer
        (insert "caller-state")
        (goto-char 4)
        (string-match "\\(caller\\)-\\(state\\)" "caller-state")
        (let ((caller (current-buffer))
              (caller-point (point))
              (caller-match (match-data)))
          (with-current-buffer target
            (insert
             "REPORT HEADER\n"
             "order=REL-417 owner=Alice\n"
             "order=rel-418 owner=Bob\n"
             "order=REL-419 owner=Chloé\n"
             "REPORT FOOTER\n")
            (goto-char (point-min))
            (forward-line 1)
            (let ((begin (point)))
              (forward-line 3)
              (narrow-to-region begin (point)))
            (goto-char (+ (point-min) 8)))
          (let* ((target-before
                  (with-current-buffer target
                    (list (point) (point-min) (point-max)
                          (buffer-narrowed-p))))
                 (restricted
                  (m-buffer-match
                   target
                   "^order=\\(REL-[0-9]+\\) owner=\\(.+\\)$"
                   :case-fold-search nil
                   :numeric t))
                 (widened
                  (m-buffer-match
                   target
                   "^order=\\(REL-[0-9]+\\) owner=\\(.+\\)$"
                   :case-fold-search t
                   :widen t
                   :numeric t)))
            (setq result
                  (list
                   :restricted
                   (m-buffer-test-match-records target restricted)
                   :widened
                   (m-buffer-test-match-records target widened)
                   :caller
                   (list :same-buffer (eq (current-buffer) caller)
                         :point (= (point) caller-point)
                         :match-data (match-data)
                         :match-data-restored
                         (equal (match-data) caller-match))
                   :target
                   (with-current-buffer target
                     (list :state (list (point) (point-min) (point-max)
                                       (buffer-narrowed-p))
                           :state-restored
                           (equal target-before
                                  (list (point) (point-min) (point-max)
                                        (buffer-narrowed-p))))))))))
    (when (buffer-live-p target)
      (kill-buffer target)))
  result)
"##;
    let expect = expect![[
        r##"OK (:restricted ((:positions (15 40 21 28 35 40) :strings ("order=REL-417 owner=Alice" "REL-417" "Alice")) (:positions (65 90 71 78 85 90) :strings ("order=REL-419 owner=Chloé" "REL-419" "Chloé"))) :widened ((:positions (15 40 21 28 35 40) :strings ("order=REL-417 owner=Alice" "REL-417" "Alice")) (:positions (41 64 47 54 61 64) :strings ("order=rel-418 owner=Bob" "rel-418" "Bob")) (:positions (65 90 71 78 85 90) :strings ("order=REL-419 owner=Chloé" "REL-419" "Chloé"))) :caller (:same-buffer t :point t :match-data (0 12 0 6 7 12) :match-data-restored t) :target (:state (23 15 91 t) :state-restored t))"##
    ]];
    ParityBatchCase::value(
        "scoped_order_searches_respect_case_narrowing_and_restore_all_editor_state",
        elisp_form,
        expect,
    )
}

fn marker_backed_rewrite_expands_every_match_forward_and_then_releases_markers() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (insert
   "api env=staging owner=alice\n"
   "worker env=staging owner=bob\n"
   "cron env=staging owner=carol\n")
  (goto-char 5)
  (string-match "\\(sentinel\\)" "sentinel")
  (let* ((point-before (point))
         (match-before (match-data))
         (matches
          (m-buffer-match
           (current-buffer) "env=\\(staging\\)"))
         (before (m-buffer-test-match-records (current-buffer) matches))
         (replaced
          (m-buffer-replace-match matches "production" t t 1))
         (after (m-buffer-test-match-records (current-buffer) replaced))
         (marker-state-before-cleanup
          (m-buffer-test-marker-state matches)))
    (m-buffer-nil-marker matches)
    (list
     :before before
     :after after
     :content (buffer-substring-no-properties (point-min) (point-max))
     :point (list :before point-before :after (point))
     :match-data-restored (equal (match-data) match-before)
     :markers
     (list :before-cleanup marker-state-before-cleanup
           :after-cleanup (m-buffer-test-marker-state matches)))))
"##;
    let expect = expect![[
        r##"OK (:before ((:positions (5 16 9 16) :strings ("env=staging" "staging")) (:positions (36 47 40 47) :strings ("env=staging" "staging")) (:positions (63 74 67 74) :strings ("env=staging" "staging"))) :after ((:positions (9 19) :strings ("production")) (:positions (43 53) :strings ("production")) (:positions (73 83) :strings ("production"))) :content "api env=production owner=alice\nworker env=production owner=bob\ncron env=production owner=carol\n" :point (:before 5 :after 5) :match-data-restored t :markers (:before-cleanup ((5 t nil) (19 t nil) (9 t nil) (19 t nil) (39 t nil) (53 t nil) (43 t nil) (53 t nil) (69 t nil) (83 t nil) (73 t nil) (83 t nil)) :after-cleanup ((nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil) (nil nil nil))))"##
    ]];
    ParityBatchCase::value(
        "marker_backed_rewrite_expands_every_match_forward_and_then_releases_markers",
        elisp_form,
        expect,
    )
}

fn deployment_config_classification_separates_comments_blank_and_active_lines() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   "# deployment targets\n"
   "api=ready\n"
   "   \n"
   "\n"
   "worker=blocked\n"
   "# rollback policy\n"
   "cron=paused\n")
  (m-buffer-with-markers
      ((all-lines (m-buffer-match-line (current-buffer)))
       (empty-lines (m-buffer-match-empty-line (current-buffer)))
       (whitespace-lines
        (m-buffer-match-whitespace-line (current-buffer)))
       (non-whitespace-lines
        (m-buffer-match-non-whitespace-line (current-buffer)))
       (comments (m-buffer-match (current-buffer) "^#.*$"))
       (active
        (m-buffer-match
         (current-buffer) "^[[:alnum:]-]+=[^[:space:]].*$")))
    (let ((without-comments
           (m-buffer-match-subtract non-whitespace-lines comments)))
      (list
       :all (m-buffer-match-string-no-properties all-lines)
       :empty (m-buffer-match-string-no-properties empty-lines)
       :whitespace
       (m-buffer-match-string-no-properties whitespace-lines)
       :reported-non-whitespace
       (m-buffer-match-string-no-properties non-whitespace-lines)
       :comments (m-buffer-match-string-no-properties comments)
       :active (m-buffer-match-string-no-properties active)
       :without-comments
       (m-buffer-match-string-no-properties without-comments)
       :active-positions
       (m-buffer-marker-tree-to-pos active)))))
"##;
    let expect = expect![[
        r##"OK (:all ("# deployment targets" "api=ready" "   " "" "worker=blocked" "# rollback policy" "cron=paused" "") :empty ("" "") :whitespace ("   \n") :reported-non-whitespace ("# deployment targets" "api=ready" "   " "" "worker=blocked" "# rollback policy" "cron=paused" "") :comments ("# deployment targets" "# rollback policy") :active ("api=ready" "worker=blocked" "cron=paused") :without-comments ("api=ready" "   " "" "worker=blocked" "cron=paused" "") :active-positions ((22 31) (37 51) (70 81)))"##
    ]];
    ParityBatchCase::value(
        "deployment_config_classification_separates_comments_blank_and_active_lines",
        elisp_form,
        expect,
    )
}

fn ordered_log_matching_and_partitioning_reconstruct_two_deployment_runs() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   "BEGIN REL-417\n"
   "step build\n"
   "step verify\n"
   "END REL-417\n"
   "heartbeat\n"
   "BEGIN REL-418\n"
   "step build\n"
   "step verify\n"
   "END REL-418\n")
  (m-buffer-with-markers
      ((sequence
        (m-buffer-match-multi
         '("^BEGIN REL-417$" "^step build$"
           "^step verify$" "^END REL-417$")
         :buffer (current-buffer)))
       (line-starts
        (m-buffer-match-line-start (current-buffer)))
       (run-starts
        (m-buffer-match-begin (current-buffer) "^BEGIN REL-")))
    (let ((partitions
           (m-buffer-partition-by-marker line-starts run-starts)))
      (list
       :ordered-sequence
       (m-buffer-match-string-no-properties sequence)
       :sequence-regions
       (m-buffer-marker-tree-to-pos sequence)
       :run-starts (m-buffer-marker-to-pos run-starts)
       :partitions (m-buffer-marker-tree-to-pos partitions)
       :partition-lines
       (mapcar
        (lambda (partition)
          (mapcar
           (lambda (position)
             (save-excursion
               (goto-char position)
               (buffer-substring-no-properties
                (line-beginning-position) (line-end-position))))
           (cdr partition)))
        partitions)))))
"##;
    let expect = expect![[
        r##"OK (:ordered-sequence ("BEGIN REL-417" "step build" "step verify" "END REL-417") :sequence-regions ((1 14) (15 25) (26 37) (38 49)) :run-starts (1 60) :partitions ((nil) (1 1 15 26 38 50) (60 60 74 85 97 109)) :partition-lines (nil ("BEGIN REL-417" "step build" "step verify" "END REL-417" "heartbeat") ("BEGIN REL-418" "step build" "step verify" "END REL-418" "")))"##
    ]];
    ParityBatchCase::value(
        "ordered_log_matching_and_partitioning_reconstruct_two_deployment_runs",
        elisp_form,
        expect,
    )
}

fn incident_annotations_attach_overlays_and_text_properties_to_exact_records() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   "INFO deployment queued\n"
   "WARN retry budget low\n"
   "ERROR payment backend unavailable\n"
   "WARN fallback enabled\n"
   "ERROR release aborted\n")
  (m-buffer-with-markers
      ((warnings (m-buffer-match (current-buffer) "^WARN.*$"))
       (errors (m-buffer-match (current-buffer) "^ERROR.*$")))
    (let ((face-results
           (m-buffer-overlay-face-match errors 'error)))
      (m-buffer-add-text-property-match
       warnings '(category deployment-warning severity 2))
      (m-buffer-text-property-font-lock-face
       warnings 'font-lock-warning-face)
      (m-buffer-put-text-property-match
       errors 'incident-id "INC-417")
      (let ((error-overlays
             (sort
              (overlays-in (point-min) (point-max))
              (lambda (left right)
                (< (overlay-start left) (overlay-start right))))))
        (list
         :content (buffer-substring-no-properties (point-min) (point-max))
         :face-results face-results
         :warning-records
         (mapcar
          (lambda (match)
            (let ((start (car match)))
              (list
               (buffer-substring-no-properties start (cadr match))
               (get-text-property start 'category)
               (get-text-property start 'severity)
               (get-text-property start 'font-lock-face))))
          warnings)
         :error-records
         (mapcar
          (lambda (match)
            (let ((start (car match)))
              (list
               (buffer-substring-no-properties start (cadr match))
               (get-text-property start 'incident-id))))
          errors)
         :overlays
         (mapcar
          (lambda (overlay)
            (list :range (list (overlay-start overlay)
                               (overlay-end overlay))
                  :buffer (eq (overlay-buffer overlay) (current-buffer))
                  :face (overlay-get overlay 'face)))
          error-overlays))))))
"##;
    let expect = expect![[
        r##"OK (:content "INFO deployment queued\nWARN retry budget low\nERROR payment backend unavailable\nWARN fallback enabled\nERROR release aborted\n" :face-results (error error) :warning-records (("WARN retry budget low" deployment-warning 2 font-lock-warning-face) ("WARN fallback enabled" deployment-warning 2 font-lock-warning-face)) :error-records (("ERROR payment backend unavailable" "INC-417") ("ERROR release aborted" "INC-417")) :overlays ((:range (46 79) :buffer t :face error) (:range (102 123) :buffer t :face error)))"##
    ]];
    ParityBatchCase::value(
        "incident_annotations_attach_overlays_and_text_properties_to_exact_records",
        elisp_form,
        expect,
    )
}

fn stateless_location_queries_and_managed_markers_preserve_caller_and_target_points()
-> ParityBatchCase {
    let elisp_form = r##"
(let ((target (generate-new-buffer " *m-buffer-location*"))
      result)
  (unwind-protect
      (with-temp-buffer
        (insert "caller")
        (goto-char 3)
        (let ((caller (current-buffer))
              (caller-point (point)))
          (with-current-buffer target
            (insert "alpha\nbeta\ngamma\n")
            (narrow-to-region 7 11)
            (goto-char 9))
          (let* ((line-start (m-buffer-test-marker target 7))
                 (line-end (m-buffer-test-marker target 11))
                 (target-state
                  (with-current-buffer target
                    (list (point) (point-min) (point-max))))
                 marker-references
                 (managed
                  (m-buffer-with-markers
                      ((start (m-buffer-test-marker target 7 t))
                       (nested
                        (list (m-buffer-test-marker target 9 t)
                              (list (m-buffer-test-marker target 11 t)))))
                    (setq marker-references
                          (m-buffer-test-marker-leaves
                           (list start nested)))
                    (list
                     :positions
                     (mapcar #'marker-position marker-references)
                     :buffers
                     (mapcar
                      (lambda (marker)
                        (eq (marker-buffer marker) target))
                      marker-references)))))
            (setq result
                  (list
                   :queries
                   (list
                    :point (m-buffer-at-point target)
                    :start-bol (m-buffer-at-bolp line-start)
                    :start-eol (m-buffer-at-eolp line-start)
                    :end-bol (m-buffer-at-bolp line-end)
                    :end-eol (m-buffer-at-eolp line-end)
                    :bounds
                    (list
                     (m-buffer-at-line-beginning-position target 9)
                     (m-buffer-at-line-end-position target 9))
                    :narrowed (m-buffer-at-narrowed-p target)
                    :visible-string (m-buffer-at-string target))
                   :scoped-bodies
                   (list
                    (m-buffer-with-current-marker line-start
                      (list (eq (current-buffer) target) (point)))
                    (m-buffer-with-current-position target 10
                      (list (eq (current-buffer) target) (point)))
                    (m-buffer-with-current-location (list target 8)
                      (list (eq (current-buffer) target) (point))))
                   :managed
                   (list :value managed
                         :after-cleanup
                         (m-buffer-test-marker-state
                          marker-references))
                   :caller-restored
                   (list (eq (current-buffer) caller)
                         (= (point) caller-point))
                   :target-restored
                   (with-current-buffer target
                     (equal target-state
                            (list (point) (point-min) (point-max))))))
            (set-marker line-start nil)
            (set-marker line-end nil))))
    (when (buffer-live-p target)
      (kill-buffer target)))
  result)
"##;
    let expect = expect![[
        r##"OK (:queries (:point 9 :start-bol t :start-eol nil :end-bol nil :end-eol t :bounds (7 11) :narrowed t :visible-string "beta") :scoped-bodies ((t 7) (t 10) (t 8)) :managed (:value (:positions (7 9 11) :buffers (t t t)) :after-cleanup ((nil nil t) (nil nil t) (nil nil t))) :caller-restored (t t) :target-restored t)"##
    ]];
    ParityBatchCase::value(
        "stateless_location_queries_and_managed_markers_preserve_caller_and_target_points",
        elisp_form,
        expect,
    )
}

#[test]
fn m_buffer_package_batch() {
    let cases = vec![
        scoped_order_searches_respect_case_narrowing_and_restore_all_editor_state(),
        marker_backed_rewrite_expands_every_match_forward_and_then_releases_markers(),
        deployment_config_classification_separates_comments_blank_and_active_lines(),
        ordered_log_matching_and_partitioning_reconstruct_two_deployment_runs(),
        incident_annotations_attach_overlays_and_text_properties_to_exact_records(),
        stateless_location_queries_and_managed_markers_preserve_caller_and_target_points(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed m-buffer parity test");
    assert_oracle_batch_cases(m_buffer_oracle(), test_name, "m_buffer_parity", &cases);
}
