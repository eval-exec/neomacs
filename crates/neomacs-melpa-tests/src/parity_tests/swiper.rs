use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, SWIPER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const SWIPER_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const SWIPER_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'swiper)

(global-set-key (kbd "C-c s") #'swiper)
(global-set-key (kbd "C-c i") #'swiper-isearch)
(global-set-key (kbd "C-c r") #'swiper-isearch-backward)
(global-set-key (kbd "C-c t") #'swiper-thing-at-point)
(global-set-key (kbd "C-c a") #'swiper-all)

(defun neomacs-swiper-test-in-buffer (text position body)
  "Run BODY in a displayed work buffer containing TEXT at POSITION."
  (let ((buffer (generate-new-buffer "*swiper-parity-work*")))
    (unwind-protect
        (save-window-excursion
          (set-window-buffer (selected-window) buffer)
          (set-buffer buffer)
          (insert text)
          (goto-char (if (eq position :end) (point-max) position))
          (funcall body))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))

(defun neomacs-swiper-test-position-summary ()
  "Describe point as a stable line, column, and surrounding line."
  (list :line (line-number-at-pos)
        :column (current-column)
        :text (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))))

(defun neomacs-swiper-test-candidate-summary (candidates)
  "Describe CANDIDATES as positions, lines, columns, and matched text."
  (mapcar
   (lambda (position)
     (save-excursion
       (goto-char position)
       (list position
             (line-number-at-pos)
             (current-column)
             (buffer-substring-no-properties
              (line-beginning-position) (line-end-position)))))
   candidates))
"##;

fn swiper_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SWIPER_MELPA_PIN, "swiper.el")
        .expect("prepare revision-pinned Swiper source below ./tmp")
        .with_prelude(SWIPER_TEST_PRELUDE)
        .with_timeout(SWIPER_TEST_TIMEOUT)
}

fn line_search_navigates_repeated_incidents_and_records_the_search_origin() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-swiper-test-in-buffer
 "INC-417 status:queued owner:alice\nINC-418 status:retry owner:bob\nINC-419 status:retry owner:carol\nINC-420 status:resolved owner:dave\n"
 1
 (lambda ()
   (let ((swiper-history nil)
         (regexp-search-ring nil)
         (mark-ring nil)
         (swiper-verbose nil))
     (execute-kbd-macro
      (vconcat (kbd "C-c s") "status:retry" (kbd "C-n RET")))
     (list
      :selected (neomacs-swiper-test-position-summary)
      :point (point)
      :mark (mark t)
      :search-ring regexp-search-ring
      :swiper-history (mapcar #'substring-no-properties swiper-history)
      :overlays-left
      (cl-count-if
       (lambda (overlay)
         (memq (overlay-get overlay 'face)
               (append swiper-faces swiper-background-faces)))
       (overlays-in (point-min) (point-max)))))))
"##;
    let expected = expect![[
        r####"OK (:selected (:line 3 :column 20 :text "INC-419 status:retry owner:carol") :point 86 :mark 1 :search-ring ("status:retry") :swiper-history ("status:retry" " INC-419 status:retry owner:carol") :overlays-left 0)"####
    ]];
    ParityBatchCase::value(
        "line_search_navigates_repeated_incidents_and_records_the_search_origin",
        elisp_form,
        expected,
    )
}

fn match_search_handles_regex_navigation_backward_search_and_smart_case() -> ParityBatchCase {
    let elisp_form = r##"
(list
 :regexp
 (neomacs-swiper-test-in-buffer
  "(defun deploy-preview ())\nnotes\n(defvar deploy-region nil)\n(defun deploy-production ())\n"
  1
  (lambda ()
    (let ((swiper-history nil)
          (regexp-search-ring nil)
          (swiper-verbose nil))
      (execute-kbd-macro
       (vconcat (kbd "C-c i") "defun\\|defvar" (kbd "C-n RET")))
      (list (neomacs-swiper-test-position-summary)
            regexp-search-ring
            (mapcar #'substring-no-properties swiper-history)))))
 :backward
 (neomacs-swiper-test-in-buffer
  "deploy alpha\nnoise\ndeploy beta\nnoise\ndeploy gamma\n"
  :end
  (lambda ()
    (let ((swiper-history nil)
          (swiper-verbose nil))
      (execute-kbd-macro (vconcat (kbd "C-c r") "deploy" (kbd "RET")))
      (neomacs-swiper-test-position-summary))))
 :smart-case
 (neomacs-swiper-test-in-buffer
  "service foo\nservice Foo\nservice FOO\n"
  1
  (lambda ()
    (let ((ivy-case-fold-search-default 'auto)
          (swiper-verbose nil))
      (execute-kbd-macro (vconcat (kbd "C-c i") "Foo" (kbd "RET")))
      (neomacs-swiper-test-position-summary)))))
"##;
    let expected = expect![[
        r####"OK (:regexp ((:line 3 :column 7 :text "(defvar deploy-region nil)") ("defun\\|defvar") ("defun\\|defvar")) :backward (:line 5 :column 0 :text "deploy gamma") :smart-case (:line 2 :column 11 :text "service Foo"))"####
    ]];
    ParityBatchCase::value(
        "match_search_handles_regex_navigation_backward_search_and_smart_case",
        elisp_form,
        expected,
    )
}

fn query_replace_renames_selected_incidents_with_captured_identifiers() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-swiper-test-in-buffer
 "ticket-417 state:failed\nticket-418 state:healthy\nticket-419 state:failed\n"
 1
 (lambda ()
   (let ((ivy-re-builders-alist '((t . ivy--regex)))
         (swiper-verbose nil))
     (execute-kbd-macro
      (vconcat (kbd "C-c i")
               "ticket-\\([0-9]+\\) state:failed"
               (kbd "M-q")
               "INC-\\2 state:retry"
               (kbd "C-j !")))
     (list :text (buffer-string)
           :point (neomacs-swiper-test-position-summary)
           :query-overlays (length swiper--query-replace-overlays)))))
"##;
    let expected = expect![[
        r####"OK (:text "INC-417 state:retry\nticket-418 state:healthy\nINC-419 state:retry\n" :point (:line 3 :column 19 :text "INC-419 state:retry") :query-overlays 0)"####
    ]];
    ParityBatchCase::value(
        "query_replace_renames_selected_incidents_with_captured_identifiers",
        elisp_form,
        expected,
    )
}

fn occur_exports_capture_groups_from_the_live_search_session() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-swiper-test-in-buffer
 "owner:alice state:failed\nowner:bob state:queued\nowner:carol state:failed\n"
 1
 (lambda ()
   (let ((ivy-re-builders-alist '((t . ivy--regex)))
         (swiper-verbose nil)
         occur-buffer result)
     (execute-kbd-macro
      (vconcat (kbd "C-c i")
               "owner:\\([[:alpha:]]+\\) state:failed"
               (kbd "C-c C-o")))
     (setq occur-buffer (current-buffer))
     (setq result
           (list :mode major-mode
                 :read-only buffer-read-only
                 :text (buffer-substring-no-properties
                        (point-min) (point-max))
                 :source-live
                 (buffer-live-p (ivy-state-buffer ivy-occur-last))))
     (unless (string= (buffer-name occur-buffer) "*swiper-parity-work*")
       (kill-buffer occur-buffer))
     result)))
"##;
    let expected = expect![[
        r####"OK (:mode fundamental-mode :read-only nil :text "alice\ncarol" :source-live t)"####
    ]];
    ParityBatchCase::value(
        "occur_exports_capture_groups_from_the_live_search_session",
        elisp_form,
        expected,
    )
}

fn candidate_engine_respects_invisible_text_narrowing_direction_and_zero_width_matches()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-swiper-test-in-buffer
 "deploy visible\ndeploy folded\nnoise\ndeploy scoped\ndeploy final\n"
 1
 (lambda ()
   (let* ((fold-start (save-excursion (forward-line 1) (point)))
          (fold-end (save-excursion (forward-line 2) (point)))
          (fold (make-overlay fold-start fold-end))
          (buffer-invisibility-spec '((folded)))
          (ivy--regex-function #'ivy--regex)
          (swiper--opoint (point-min))
          (swiper-goto-start-of-match t)
          visible all scoped backward zero-width)
     (overlay-put fold 'invisible 'folded)
     (ivy-set-text "deploy")
     (let ((search-invisible nil)
           (swiper--isearch-backward nil))
       (setq visible (swiper--isearch-function ivy-text)))
     (let ((search-invisible t)
           (swiper--isearch-backward nil))
       (setq all (swiper--isearch-function ivy-text)))
     (save-restriction
       (narrow-to-region
        (save-excursion (goto-char (point-min)) (forward-line 2) (point))
        (save-excursion (goto-char (point-min)) (forward-line 4) (point)))
       (let ((search-invisible nil)
             (swiper--isearch-backward nil))
         (setq scoped (swiper--isearch-function ivy-text))))
     (let ((search-invisible nil)
           (swiper--isearch-backward t)
           (swiper--opoint (point-max)))
       (setq backward (swiper--isearch-function ivy-text)))
     (ivy-set-text "^")
     (let ((search-invisible nil)
           (swiper--isearch-backward nil))
       (setq zero-width (swiper--isearch-function ivy-text)))
     (prog1
         (list
          :visible (neomacs-swiper-test-candidate-summary visible)
          :all (neomacs-swiper-test-candidate-summary all)
          :scoped (neomacs-swiper-test-candidate-summary scoped)
          :backward (neomacs-swiper-test-candidate-summary backward)
          :zero-width (neomacs-swiper-test-candidate-summary zero-width))
       (delete-overlay fold)))))
"##;
    let expected = expect![[
        r####"OK (:visible ((1 1 0 "deploy visible") (36 4 0 "deploy scoped") (50 5 0 "deploy final")) :all ((1 1 0 "deploy visible") (16 2 0 "deploy folded") (36 4 0 "deploy scoped") (50 5 0 "deploy final")) :scoped ((36 4 0 "deploy scoped")) :backward ((1 1 0 "deploy visible") (36 4 0 "deploy scoped") (50 5 0 "deploy final")) :zero-width ((1 1 0 "deploy visible") (30 3 0 "noise") (36 4 0 "deploy scoped") (50 5 0 "deploy final")))"####
    ]];
    ParityBatchCase::value(
        "candidate_engine_respects_invisible_text_narrowing_direction_and_zero_width_matches",
        elisp_form,
        expected,
    )
}

fn all_buffer_search_switches_to_the_exact_matching_operational_log() -> ParityBatchCase {
    let elisp_form = r##"
(let ((orders (generate-new-buffer "*swiper-orders-log*"))
      (inventory (generate-new-buffer "*swiper-inventory-log*")))
  (unwind-protect
      (save-window-excursion
        (with-current-buffer orders
          (setq buffer-file-name
                (expand-file-name "orders.log" temporary-file-directory))
          (insert "INC-8001 queued\nINC-8002 resolved\n"))
        (with-current-buffer inventory
          (setq buffer-file-name
                (expand-file-name "inventory.log" temporary-file-directory))
          (insert "SKU-42 healthy\nINC-9002 inventory drift\n"))
        (switch-to-buffer orders)
        (let ((swiper-verbose nil))
          (execute-kbd-macro (vconcat (kbd "C-c a") "INC-9002" (kbd "RET"))))
        (list :buffer (buffer-name)
              :position (neomacs-swiper-test-position-summary)
              :point (point)))
    (when (buffer-live-p orders) (kill-buffer orders))
    (when (buffer-live-p inventory) (kill-buffer inventory))))
"##;
    let expected = expect![[
        r####"OK (:buffer "*swiper-inventory-log*" :position (:line 2 :column 8 :text "INC-9002 inventory drift") :point 24)"####
    ]];
    ParityBatchCase::value(
        "all_buffer_search_switches_to_the_exact_matching_operational_log",
        elisp_form,
        expected,
    )
}

#[test]
fn swiper_package_batch() {
    assert_oracle_batch_cases(
        swiper_oracle(),
        "swiper-package-batch",
        "Swiper",
        &[
            line_search_navigates_repeated_incidents_and_records_the_search_origin(),
            match_search_handles_regex_navigation_backward_search_and_smart_case(),
            query_replace_renames_selected_incidents_with_captured_identifiers(),
            occur_exports_capture_groups_from_the_live_search_session(),
            candidate_engine_respects_invisible_text_narrowing_direction_and_zero_width_matches(),
            all_buffer_search_switches_to_the_exact_matching_operational_log(),
        ],
    );
}
