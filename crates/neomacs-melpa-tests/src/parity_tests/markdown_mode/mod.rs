use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MARKDOWN_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const MARKDOWN_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const MARKDOWN_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'markdown-mode)

(defun markdown-test-select (text)
  (goto-char (point-min))
  (unless (search-forward text nil t)
    (error "Missing Markdown fixture text: %s" text))
  (set-mark (match-beginning 0))
  (goto-char (match-end 0))
  (activate-mark)
  (cons (region-beginning) (region-end)))

(defun markdown-test-find (text)
  (goto-char (point-min))
  (unless (search-forward text nil t)
    (error "Missing Markdown fixture text: %s" text))
  (goto-char (match-beginning 0))
  (point))

(defun markdown-test-normalize-position (position)
  (if (markerp position)
      (marker-position position)
    position))

(defun markdown-test-normalize-index (index)
  (mapcar
   (lambda (entry)
     (let ((value (cdr entry)))
       (cons
        (car entry)
        (cond
         ((or (integerp value) (markerp value))
          (markdown-test-normalize-position value))
         ((listp value)
          (markdown-test-normalize-index value))
         (t value)))))
   index))

(defun markdown-test-token-state (text)
  (save-excursion
    (markdown-test-find text)
    (let ((start (point))
          (end (+ (point) (length text))))
      (list
       :text text
       :range (list start end)
       :face (get-text-property start 'face)
       :font-lock-face (get-text-property start 'font-lock-face)
       :invisible (get-char-property start 'invisible)
       :keymap (and (get-text-property start 'keymap) t)
       :help (get-text-property start 'help-echo)))))

(defun markdown-test-list-bounds (text)
  (save-excursion
    (markdown-test-find text)
    (mapcar
     #'markdown-test-normalize-position
     (markdown-cur-list-item-bounds))))

(defun markdown-test-task-state (text)
  (save-excursion
    (markdown-test-find text)
    (let ((line (buffer-substring-no-properties
                 (line-beginning-position)
                 (line-end-position))))
      (beginning-of-line)
      (unless (re-search-forward "\\[[ xX]\\]" (line-end-position) t)
        (error "Missing GFM checkbox for task: %s" text))
      (let ((start (match-beginning 0)))
        (list
         :task text
         :line line
         :status (match-string-no-properties 0)
         :range (list start (match-end 0))
         :face (get-text-property start 'face)
         :keymap (and (get-text-property start 'keymap) t)
         :button (and (button-at start) t))))))
"##;

fn markdown_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MARKDOWN_MODE_MELPA_PIN, "markdown-mode.el")
        .expect("prepare pinned Markdown Mode source below ./tmp")
        .with_prelude(MARKDOWN_MODE_TEST_PRELUDE)
        .with_timeout(MARKDOWN_MODE_TEST_TIMEOUT)
}

fn release_note_editing_wraps_toggles_and_inserts_real_links_and_images() -> ParityBatchCase {
    let elisp_form = r#####"
(with-temp-buffer
  (insert
   "Deploy REL-417 to production on Tuesday.\n"
   "Owner: Anaïs\n")
  (markdown-mode)
  (let ((transient-mark-mode t))
    (markdown-test-select "REL-417")
    (markdown-insert-bold)
    (deactivate-mark)
    (markdown-test-select "Tuesday")
    (markdown-insert-italic)
    (deactivate-mark)
    (markdown-test-find "production")
    (forward-char 2)
    (markdown-insert-code)
    (goto-char (point-max))
    (insert "\nReferences: ")
    (markdown-insert-inline-link
     "runbook"
     "https://ops.example.test/runbooks/REL-417"
     "Production rollout")
    (insert " ")
    (markdown-insert-inline-image
     "deployment graph"
     "images/rel-417.png"
     "REL-417 topology")
    (let ((styled (buffer-substring-no-properties (point-min) (point-max))))
      (font-lock-ensure)
      (let ((tokens
             (mapcar
              #'markdown-test-token-state
              '("REL-417" "production" "Tuesday" "runbook"
                "https://ops.example.test/runbooks/REL-417"
                "deployment graph"))))
        (markdown-test-find "REL-417")
        (forward-char 2)
        (markdown-insert-bold)
        (list
         :styled styled
         :tokens tokens
         :bold-toggled-off (buffer-substring-no-properties (point-min) (point-max))
         :point (point)
         :mark (mark t)
         :region-active (region-active-p)
         :modified (buffer-modified-p))))))
"#####;
    let expect = expect![[
        r######"OK (:styled "Deploy **REL-417** to `production` on *Tuesday*.\nOwner: Anaïs\n\nReferences: [runbook](https://ops.example.test/runbooks/REL-417 \"Production rollout\") ![deployment graph](images/rel-417.png \"REL-417 topology\")" :tokens ((:text "REL-417" :range (10 17) :face (markdown-bold-face) :font-lock-face nil :invisible nil :keymap nil :help nil) (:text "production" :range (24 34) :face (markdown-inline-code-face) :font-lock-face nil :invisible nil :keymap nil :help nil) (:text "Tuesday" :range (40 47) :face (markdown-italic-face) :font-lock-face nil :invisible nil :keymap nil :help nil) (:text "runbook" :range (77 84) :face markdown-link-face :font-lock-face nil :invisible nil :keymap t :help "\"Production rollout\"\nhttps://ops.example.test/runbooks/REL-417") (:text "https://ops.example.test/runbooks/REL-417" :range (86 127) :face markdown-url-face :font-lock-face nil :invisible markdown-markup :keymap t :help nil) (:text "deployment graph" :range (152 168) :face markdown-link-face :font-lock-face nil :invisible nil :keymap t :help "\"REL-417 topology\"\nimages/rel-417.png")) :bold-toggled-off "Deploy REL-417 to `production` on *Tuesday*.\nOwner: Anaïs\n\nReferences: [runbook](https://ops.example.test/runbooks/REL-417 \"Production rollout\") ![deployment graph](images/rel-417.png \"REL-417 topology\")" :point 8 :mark 35 :region-active nil :modified t)"######
    ]];
    ParityBatchCase::value(
        "release_note_editing_wraps_toggles_and_inserts_real_links_and_images",
        elisp_form,
        expect,
    )
}

fn outline_reorganization_demotes_promotes_moves_and_reindexes_a_release_plan() -> ParityBatchCase {
    let elisp_form = r#####"
(with-temp-buffer
  (insert
   "# Release Plan\n"
   "Overview for REL-417.\n\n"
   "## API\n"
   "Deploy API.\n\n"
   "### Checks\n"
   "Run smoke tests.\n\n"
   "## Worker\n"
   "Deploy workers.\n\n"
   "## Rollback\n"
   "Restore previous images.\n")
  (markdown-mode)
  (font-lock-ensure)
  (let ((initial-index
         (markdown-test-normalize-index
          (markdown-imenu-create-nested-index))))
    (markdown-test-find "## API")
    (markdown-demote-subtree)
    (let ((demoted (buffer-substring-no-properties (point-min) (point-max))))
      (markdown-test-find "## Rollback")
      (markdown-move-subtree-up)
      (let ((moved (buffer-substring-no-properties (point-min) (point-max))))
        (markdown-test-find "### API")
        (markdown-promote-subtree)
        (font-lock-flush)
        (font-lock-ensure)
        (let ((final-index
               (markdown-test-normalize-index
                (markdown-imenu-create-nested-index)))
              navigation)
          (markdown-test-find "## Rollback")
          (push
           (list :rollback (point)
                 :level (markdown-outline-level))
           navigation)
          (markdown-forward-same-level 1)
          (push
           (list :next (point)
                 :line (buffer-substring-no-properties
                        (line-beginning-position)
                        (line-end-position))
                 :level (markdown-outline-level))
           navigation)
          (markdown-up-heading 1)
          (push
           (list :parent (point)
                 :line (buffer-substring-no-properties
                        (line-beginning-position)
                        (line-end-position))
                 :level (markdown-outline-level))
           navigation)
          (list
           :initial-index initial-index
           :demoted demoted
           :moved moved
           :final (buffer-substring-no-properties (point-min) (point-max))
           :final-index final-index
           :navigation (nreverse navigation)))))))
"#####;
    let expect = expect![[
        r######"OK (:initial-index (("Release Plan" ("." . 1) ("API" ("." . 39) ("Checks" . 59)) ("Worker" . 88) ("Rollback" . 115))) :demoted "# Release Plan\nOverview for REL-417.\n\n### API\n\nDeploy API.\n\n#### Checks\n\nRun smoke tests.\n\n## Worker\nDeploy workers.\n\n## Rollback\nRestore previous images.\n" :moved "# Release Plan\nOverview for REL-417.\n\n### API\n\nDeploy API.\n\n#### Checks\n\nRun smoke tests.\n\n## Rollback\nRestore previous images.\n## Worker\nDeploy workers.\n\n" :final "# Release Plan\nOverview for REL-417.\n\n## API\n\nDeploy API.\n\n### Checks\n\nRun smoke tests.\n\n## Rollback\nRestore previous images.\n## Worker\nDeploy workers.\n\n" :final-index (("Release Plan" ("." . 1) ("API" ("." . 39) ("Checks" . 60)) ("Rollback" . 90) ("Worker" . 127))) :navigation ((:rollback 90 :level nil) (:next 127 :line "## Worker" :level 2) (:parent 1 :line "# Release Plan" :level 1)))"######
    ]];
    ParityBatchCase::value(
        "outline_reorganization_demotes_promotes_moves_and_reindexes_a_release_plan",
        elisp_form,
        expect,
    )
}

fn ordered_rollout_and_task_lists_support_insertion_reordering_nesting_and_toggles()
-> ParityBatchCase {
    let elisp_form = r#####"
(with-temp-buffer
  (insert
   "# Rollout\n\n"
   "1. Prepare\n"
   "2. Deploy\n"
   "3. Verify\n\n"
   "- [ ] Notify support\n"
   "- [x] Update status\n")
  (gfm-mode)
  (font-lock-ensure)
  (markdown-test-find "Deploy")
  (end-of-line)
  (markdown-insert-list-item 1)
  (insert "Canary")
  (markdown-cleanup-list-numbers)
  (font-lock-flush)
  (font-lock-ensure)
  (let ((inserted (buffer-substring-no-properties (point-min) (point-max))))
    (markdown-test-find "Verify")
    (beginning-of-line)
    (markdown-move-list-item-up)
    (let ((reordered (buffer-substring-no-properties (point-min) (point-max))))
      (markdown-test-find "Canary")
      (beginning-of-line)
      (markdown-demote-list-item)
      (let ((nested (buffer-substring-no-properties (point-min) (point-max))))
        (markdown-test-find "Verify")
        (beginning-of-line)
        (markdown-promote-list-item)
        (markdown-test-find "Update status")
        (end-of-line)
        (markdown-insert-list-item 1)
        (insert "Close incident")
        (markdown-test-find "Notify support")
        (beginning-of-line)
        (let ((first-toggle (markdown-toggle-gfm-checkbox)))
          (markdown-test-find "Update status")
          (beginning-of-line)
          (let ((second-toggle (markdown-toggle-gfm-checkbox)))
            (font-lock-flush)
            (font-lock-ensure)
            (list
             :inserted inserted
             :reordered reordered
             :nested nested
             :final (buffer-substring-no-properties (point-min) (point-max))
             :toggles (list first-toggle second-toggle)
             :bounds
             (mapcar
              #'markdown-test-list-bounds
              '("Prepare" "Verify" "Canary" "Notify support"
                "Update status" "Close incident"))
             :checkboxes
             (mapcar
              #'markdown-test-task-state
              '("Notify support" "Update status" "Close incident")))))))))
"#####;
    let expect = expect![[
        r######"OK (:inserted "# Rollout\n\n1. Prepare\n2. Deploy\n3. Canary\n4. Verify\n\n- [ ] Notify support\n- [x] Update status\n" :reordered "# Rollout\n\n1. Prepare\n2. Deploy\n4. Verify\n3. Canary\n\n- [ ] Notify support\n- [x] Update status\n" :nested "# Rollout\n\n1. Prepare\n2. Deploy\n    4. Verify\n3. Canary\n\n- [ ] Notify support\n- [x] Update status\n" :final "# Rollout\n\n1. Prepare\n2. Deploy\n4. Verify\n3. Canary\n\n- [x] Notify support\n- [ ] Update status\n- [ ] Close incident\n" :toggles ("[x]" "[ ]") :bounds ((12 22 0 3 "1. " nil (12 15 12 12 12 14 14 15)) (33 42 0 3 "4. " nil (33 36 33 33 33 35 35 36)) (43 52 0 3 "3. " nil (43 46 43 43 43 45 45 46)) (54 74 0 2 "- " "[x] " (54 60 54 54 54 55 55 56 56 60)) (75 94 0 2 "- " "[ ] " (75 81 75 75 75 76 76 77 77 81)) (95 115 0 2 "- " "[ ] " (95 101 95 95 95 96 96 97 97 101))) :checkboxes ((:task "Notify support" :line "- [x] Notify support" :status "[x]" :range (56 59) :face nil :keymap nil :button t) (:task "Update status" :line "- [ ] Update status" :status "[ ]" :range (77 80) :face nil :keymap nil :button t) (:task "Close incident" :line "- [ ] Close incident" :status "[ ]" :range (97 100) :face nil :keymap nil :button t)))"######
    ]];
    ParityBatchCase::value(
        "ordered_rollout_and_task_lists_support_insertion_reordering_nesting_and_toggles",
        elisp_form,
        expect,
    )
}

fn runbook_references_and_footnotes_round_trip_and_report_missing_links() -> ParityBatchCase {
    let elisp_form = r#####"
(with-temp-buffer
  (insert
   "# Operations\n\n"
   "Follow the runbook before rollback.\n"
   "Review the [dashboard][status] after deployment.\n\n"
   "[unused]: https://legacy.example.test/\n")
  (markdown-mode)
  (let ((markdown-reference-location 'end)
        (markdown-footnote-location 'end)
        before-kill marker-point definition-point)
    (markdown-test-find "runbook")
    (delete-region (point) (+ (point) (length "runbook")))
    (markdown-insert-reference-link
     "runbook" "ops"
     "https://ops.example.test/runbooks/REL-417"
     "Production procedure")
    (markdown-test-find "rollback")
    (goto-char (+ (point) (length "rollback")))
    (markdown-insert-footnote)
    (setq definition-point (point))
    (insert "Rollback requires incident commander approval.\n"
            "    Record the image digest before proceeding.")
    (markdown-footnote-return)
    (setq marker-point (point))
    (markdown-footnote-goto-text)
    (setq before-kill
          (list
           :content (buffer-substring-no-properties (point-min) (point-max))
           :marker-point marker-point
           :definition-point definition-point
           :returned-definition-point (point)
           :defined (markdown-get-defined-references)
           :undefined (markdown-get-undefined-refs)
           :unused (markdown-get-unused-refs)
           :used-uris (markdown-get-used-uris)
           :footnotes (markdown-get-defined-footnotes)
           :imenu
           (markdown-test-normalize-index
            (markdown-imenu-create-flat-index))))
    (markdown-footnote-kill)
    (list
     :before-kill before-kill
     :after-kill
     (list :content (buffer-substring-no-properties (point-min) (point-max))
           :point (point)
           :kill (current-kill 0 t)
           :footnotes (markdown-get-defined-footnotes)
           :undefined (markdown-get-undefined-refs)))))
"#####;
    let expect = expect![[
        r######"OK (:before-kill (:content "# Operations\n\nFollow the [runbook][ops] before rollback[^1].\nReview the [dashboard][status] after deployment.\n\n[unused]: https://legacy.example.test/\n\n[ops]: https://ops.example.test/runbooks/REL-417 \"Production procedure\"\n\n[^1]: Rollback requires incident commander approval.\n    Record the image digest before proceeding." :marker-point 60 :definition-point 231 :returned-definition-point 231 :defined (("unused" . 6) ("ops" . 8) ("^1" . 10)) :undefined (("status" ("dashboard" . 4))) :unused (("unused" . 6) ("^1" . 10)) :used-uris ("https://legacy.example.test/" "https://ops.example.test/runbooks/REL-417") :footnotes (("^1" . 225)) :imenu (("Operations" . 1) ("^1" . 225))) :after-kill (:content "# Operations\n\nFollow the [runbook][ops] before rollback.\nReview the [dashboard][status] after deployment.\n\n[unused]: https://legacy.example.test/\n\n[ops]: https://ops.example.test/runbooks/REL-417 \"Production procedure\"\n" :point 220 :kill "Rollback requires incident commander approval.\n    Record the image digest before proceeding." :footnotes nil :undefined (("status" ("dashboard" . 4)))))"######
    ]];
    ParityBatchCase::value(
        "runbook_references_and_footnotes_round_trip_and_report_missing_links",
        elisp_form,
        expect,
    )
}

fn incident_csv_becomes_an_aligned_sorted_and_transposed_markdown_report() -> ParityBatchCase {
    let elisp_form = r#####"
(with-temp-buffer
  (insert
   "service,owner,latency\n"
   "api,Anaïs,120\n"
   "worker,Bob,45\n"
   "cron,Chloé,90")
  (markdown-mode)
  (let ((markdown-table-align-p t))
    (markdown-table-convert-region
     (point-min) (point-max) '(4))
    (let ((converted (buffer-substring-no-properties (point-min) (point-max))))
      (markdown-test-find "latency")
      (markdown-table-goto-column 3)
      (markdown-table-sort-lines ?n)
      (let ((sorted (buffer-substring-no-properties (point-min) (point-max))))
        (markdown-test-find "worker")
        (let ((cell-before
               (list
                :column (markdown-table-get-column)
                :cell (markdown-table-get-cell)
                :line (line-number-at-pos))))
          (markdown-table-forward-cell)
          (let ((next-cell
                 (list
                  :column (markdown-table-get-column)
                  :cell (markdown-table-get-cell)
                  :line (line-number-at-pos))))
            (markdown-test-find "service")
            (markdown-table-transpose)
            (list
             :converted converted
             :sorted sorted
             :navigation (list cell-before next-cell)
             :transposed (buffer-substring-no-properties (point-min) (point-max))
             :table-bounds
             (list (markdown-table-begin)
                   (markdown-table-end))
             :point (point))))))))
"#####;
    let expect = expect![[
        r######"OK (:converted "| service | owner | latency |\n| api     | Anaïs | 120     |\n| worker  | Bob   | 45      |\n| cron    | Chloé | 90      |\n" :sorted "| service | owner | latency |\n| worker  | Bob   | 45      |\n| cron    | Chloé | 90      |\n| api     | Anaïs | 120     |\n" :navigation ((:column 1 :cell "worker" :line 2) (:column 2 :cell "Bob" :line 2)) :transposed "| service | worker | cron  | api   |\n| owner   | Bob    | Chloé | Anaïs |\n| latency | 45     | 90    | 120   |\n" :table-bounds (1 112) :point 3)"######
    ]];
    ParityBatchCase::value(
        "incident_csv_becomes_an_aligned_sorted_and_transposed_markdown_report",
        elisp_form,
        expect,
    )
}

fn gfm_code_fences_preserve_code_and_rich_document_semantics_during_fontification()
-> ParityBatchCase {
    let elisp_form = r#####"
(with-temp-buffer
  (insert
   "# Deployment\n\n"
   "Use **careful rollout**, *observe metrics*, and `verify --all`.\n"
   "Read the [runbook](https://ops.example.test/runbook).\n\n"
   "> Stop on elevated errors.\n\n"
   "- [ ] Notify support\n\n"
   "fn main() {\n"
   "    println!(\"# not a heading\");\n"
   "}\n\n"
   "## Results\n"
   "All checks passed.\n")
  (gfm-mode)
  (let ((transient-mark-mode t))
    (goto-char (point-min))
    (search-forward "fn main()")
    (beginning-of-line)
    (set-mark (point))
    (search-forward "}\n")
    (activate-mark)
    (markdown-insert-gfm-code-block "rust" nil)
    (deactivate-mark))
  (font-lock-flush)
  (font-lock-ensure)
  (markdown-test-find "println!")
  (let* ((code-point (point))
         (block (markdown-code-block-at-pos code-point))
         (language
          (save-excursion
            (markdown-code-block-lang)))
         (syntax (syntax-ppss code-point))
         (tokens
          (mapcar
           #'markdown-test-token-state
           '("Deployment" "careful rollout" "observe metrics"
             "verify --all" "runbook"
             "https://ops.example.test/runbook"
             "Stop on elevated errors" "[ ]" "println!"
             "Results"))))
    (list
     :mode
     (list major-mode
           (derived-mode-p 'markdown-mode)
           tab-width comment-start comment-end
           (eq syntax-propertize-function
               #'markdown-syntax-propertize)
           (eq imenu-create-index-function
               #'markdown-imenu-create-nested-index)
           (eq fill-paragraph-function
               #'markdown-fill-paragraph))
     :content (buffer-substring-no-properties (point-min) (point-max))
     :code
     (list :point code-point
           :block (mapcar #'markdown-test-normalize-position block)
           :predicate (and (markdown-code-block-at-point-p code-point) t)
           :language language
           :syntax
           (list :depth (car syntax)
                 :string (nth 3 syntax)
                 :comment (nth 4 syntax)))
     :languages markdown-gfm-used-languages
     :index
     (markdown-test-normalize-index
      (markdown-imenu-create-nested-index))
     :tokens tokens
     :bindings
     (mapcar
      (lambda (key)
        (list key
              (lookup-key (current-local-map) (kbd key))))
      '("C-c C-s b" "C-c C-s i" "C-c C-s c"
        "C-c C-s l" "M-RET" "C-c C-x C")))))
"#####;
    let expect = expect![[
        r######"OK (:mode (gfm-mode markdown-mode 4 "<!-- " " -->" t t t) :content "# Deployment\n\nUse **careful rollout**, *observe metrics*, and `verify --all`.\nRead the [runbook](https://ops.example.test/runbook).\n\n> Stop on elevated errors.\n\n- [ ] Notify support\n\n``` rust\nfn main() {\n    println!(\"# not a heading\");\n}\n```\n\n## Results\nAll checks passed.\n" :code (:point 209 :block (184 243) :predicate t :language "rust" :syntax (:depth 1 :string nil :comment nil)) :languages ("rust") :index (("Deployment" ("." . 1) ("Results" . 245))) :tokens ((:text "Deployment" :range (3 13) :face markdown-header-face-1 :font-lock-face nil :invisible nil :keymap nil :help nil) (:text "careful rollout" :range (21 36) :face (markdown-bold-face) :font-lock-face nil :invisible nil :keymap nil :help nil) (:text "observe metrics" :range (41 56) :face (markdown-italic-face) :font-lock-face nil :invisible nil :keymap nil :help nil) (:text "verify --all" :range (64 76) :face (markdown-inline-code-face) :font-lock-face nil :invisible nil :keymap nil :help nil) (:text "runbook" :range (89 96) :face markdown-link-face :font-lock-face nil :invisible nil :keymap t :help "https://ops.example.test/runbook") (:text "https://ops.example.test/runbook" :range (98 130) :face markdown-url-face :font-lock-face nil :invisible markdown-markup :keymap t :help nil) (:text "Stop on elevated errors" :range (136 159) :face (markdown-blockquote-face) :font-lock-face nil :invisible nil :keymap nil :help nil) (:text "[ ]" :range (164 167) :face nil :font-lock-face nil :invisible nil :keymap nil :help nil) (:text "println!" :range (209 217) :face (markdown-pre-face markdown-code-face) :font-lock-face nil :invisible nil :keymap nil :help nil) (:text "Results" :range (248 255) :face markdown-header-face-2 :font-lock-face nil :invisible nil :keymap nil :help nil)) :bindings (("C-c C-s b" markdown-insert-bold) ("C-c C-s i" markdown-insert-italic) ("C-c C-s c" markdown-insert-code) ("C-c C-s l" markdown-insert-link) ("M-RET" markdown-insert-list-item) ("C-c C-x C" nil)))"######
    ]];
    ParityBatchCase::value(
        "gfm_code_fences_preserve_code_and_rich_document_semantics_during_fontification",
        elisp_form,
        expect,
    )
}

#[test]
fn markdown_mode_package_batch() {
    let cases = vec![
        release_note_editing_wraps_toggles_and_inserts_real_links_and_images(),
        outline_reorganization_demotes_promotes_moves_and_reindexes_a_release_plan(),
        ordered_rollout_and_task_lists_support_insertion_reordering_nesting_and_toggles(),
        runbook_references_and_footnotes_round_trip_and_report_missing_links(),
        incident_csv_becomes_an_aligned_sorted_and_transposed_markdown_report(),
        gfm_code_fences_preserve_code_and_rich_document_semantics_during_fontification(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Markdown Mode parity test");
    assert_oracle_batch_cases(
        markdown_mode_oracle(),
        test_name,
        "markdown_mode_parity",
        &cases,
    );
}
