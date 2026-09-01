use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, WFNAMES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const WFNAMES_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const WFNAMES_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'sort)

(defun wfnames-parity-root (name)
  (let ((root
         (file-name-as-directory
          (expand-file-name
           name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-directory-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun wfnames-parity-write (path contents)
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents))
  path)

(defun wfnames-parity-read (path)
  (and (file-exists-p path)
       (with-temp-buffer
         (insert-file-contents path)
         (buffer-string))))

(defun wfnames-parity-relative (path)
  (and path
       (file-relative-name path (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun wfnames-parity-lines ()
  (save-excursion
    (goto-char (point-min))
    (let (lines)
      (while (not (eobp))
        (let* ((bol (line-beginning-position))
               (text
                (buffer-substring-no-properties
                 bol (line-end-position)))
               (old (get-text-property bol 'old-name))
               (prefix (get-text-property bol 'line-prefix)))
          (push
           (list :text (wfnames-parity-relative text)
                 :old (wfnames-parity-relative old)
                 :face (get-text-property bol 'face)
                 :prefix (and prefix
                              (substring-no-properties prefix))
                 :prefix-face (and prefix
                                   (> (length prefix) 0)
                                   (get-text-property 0 'face prefix)))
           lines))
        (forward-line 1))
      (nreverse lines))))

(defun wfnames-parity-overlays ()
  (mapcar
   (lambda (overlay)
     (let ((start (overlay-start overlay))
           (end (overlay-end overlay)))
       (list :line (line-number-at-pos start)
           :starts-at-bol
           (save-excursion
             (goto-char start)
             (= start (line-beginning-position)))
           :ends-at-eol
           (save-excursion
             (goto-char end)
             (= end (line-end-position)))
           :face (overlay-get overlay 'face)
           :changed (overlay-get overlay 'hff-changed)
           :priority (overlay-get overlay 'priority)
           :evaporate (overlay-get overlay 'evaporate))))
   (sort (overlays-in (point-min) (point-max))
         (lambda (left right)
           (< (overlay-start left) (overlay-start right))))))

(defun wfnames-parity-replace-line (line new-name)
  "Edit LINE like a user replacing its selected absolute filename."
  (goto-char (point-min))
  (forward-line line)
  (let ((bol (line-beginning-position))
        (eol (line-end-position)))
    ;; Preserve the first character and its `old-name' property.  A real user
    ;; replacing a filename leaves some original text properties behind too;
    ;; wfnames' after-change hook then propagates them across the new text.
    (delete-region (1+ bol) eol)
    (goto-char (1+ bol))
    (insert (substring new-name 1))))

(defun wfnames-parity-display (_buffer-name)
  nil)

(defun wfnames-parity-kill-buffer ()
  (when (get-buffer wfnames-buffer)
    (kill-buffer wfnames-buffer)))
"####;

fn wfnames_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(WFNAMES_MELPA_PIN, "wfnames.el")
        .expect("prepare pinned wfnames source below ./tmp")
        .with_prelude(WFNAMES_TEST_PRELUDE)
        .with_timeout(WFNAMES_TEST_TIMEOUT)
}

fn setup_buffer_models_mixed_files_and_appends_without_duplicates() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (wfnames-parity-root "wfnames-setup"))
       (regular (wfnames-parity-write
                 (expand-file-name "inbox/report.txt" root)
                 "quarterly report\n"))
       (directory (expand-file-name "projects/client-a" root))
       (link (expand-file-name "latest-report" root))
       (extra (wfnames-parity-write
               (expand-file-name "inbox/notes.txt" root)
               "meeting notes\n")))
  (make-directory directory t)
  (make-symbolic-link regular link)
  (unwind-protect
      (progn
        (wfnames-setup-buffer
         (list regular directory link) #'wfnames-parity-display)
        (set-buffer wfnames-buffer)
        (let ((initial
               (list :buffer (buffer-name)
                     :mode major-mode
                     :basename-at-point
                     (buffer-substring-no-properties
                      (point) (line-end-position))
                     :capf completion-at-point-functions
                     :lines (wfnames-parity-lines))))
          (wfnames-setup-buffer
           (list regular extra) #'wfnames-parity-display t)
          (list :initial initial
                :appended-lines (wfnames-parity-lines)
                :line-count (count-lines (point-min) (point-max)))))
    (wfnames-parity-kill-buffer)))
"####;
    let expect = expect![[
        r####"OK (:initial (:buffer "*Wfnames*" :mode wfnames-mode :basename-at-point "report.txt" :capf wfnames-capf :lines ((:text "wfnames-setup/inbox/report.txt" :old "wfnames-setup/inbox/report.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-setup/projects/client-a" :old "wfnames-setup/projects/client-a" :face wfnames-dir :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-setup/latest-report" :old "wfnames-setup/latest-report" :face wfnames-symlink :prefix "* " :prefix-face wfnames-prefix))) :appended-lines ((:text "wfnames-setup/inbox/report.txt" :old "wfnames-setup/inbox/report.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-setup/projects/client-a" :old "wfnames-setup/projects/client-a" :face wfnames-dir :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-setup/latest-report" :old "wfnames-setup/latest-report" :face wfnames-symlink :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-setup/inbox/notes.txt" :old "wfnames-setup/inbox/notes.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix)) :line-count 4)"####
    ]];
    ParityBatchCase::value(
        "setup_buffer_models_mixed_files_and_appends_without_duplicates",
        elisp_form,
        expect,
    )
}

fn editing_and_committing_moves_files_into_new_parent_directories() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (wfnames-parity-root "wfnames-move"))
       (report (wfnames-parity-write
                (expand-file-name "inbox/report.txt" root)
                "revenue=42\n"))
       (config (wfnames-parity-write
                (expand-file-name "inbox/app.conf" root)
                "mode=production\n"))
       (new-report (expand-file-name
                    "archive/2026/report-final.txt" root))
       (new-config (expand-file-name
                    "deploy/service/app.conf" root))
       events)
  (unwind-protect
      (progn
        (wfnames-setup-buffer
         (list report config) #'wfnames-parity-display)
        (set-buffer wfnames-buffer)
        (wfnames-parity-replace-line 0 new-report)
        (wfnames-parity-replace-line 1 new-config)
        (let ((before
               (list :lines (wfnames-parity-lines)
                     :modified
                     (mapcar #'wfnames-parity-relative wfnames--modified)
                     :overlays (wfnames-parity-overlays))))
          (let ((wfnames-interactive-rename nil)
                (wfnames-create-parent-directories t)
                (wfnames-after-commit-hook
                 (list (lambda () (push 'hook events))))
                (wfnames-after-commit-function
                 (lambda (buffer)
                   (push (list 'function
                               buffer
                               (buffer-live-p (get-buffer buffer)))
                         events))))
            (wfnames-commit-buffer))
          (list
           :before before
           :after
           (list :old-exist (list (file-exists-p report)
                                  (file-exists-p config))
                 :new-exist (list (file-exists-p new-report)
                                  (file-exists-p new-config))
                 :contents (list (wfnames-parity-read new-report)
                                 (wfnames-parity-read new-config))
                 :parent-dirs
                 (list (file-directory-p
                        (file-name-directory new-report))
                       (file-directory-p
                        (file-name-directory new-config)))
                 :lines (wfnames-parity-lines)
                 :overlays (wfnames-parity-overlays)
                 :events (nreverse events)
                 :message (current-message)))))
    (wfnames-parity-kill-buffer)))
"####;
    let expect = expect![[
        r####"OK (:before (:lines ((:text "wfnames-move/archive/2026/report-final.txt" :old "wfnames-move/inbox/report.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-move/deploy/service/app.conf" :old "wfnames-move/inbox/app.conf" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix)) :modified ("wfnames-move/inbox/app.conf" "wfnames-move/inbox/report.txt") :overlays ((:line 1 :starts-at-bol t :ends-at-eol t :face wfnames-modified :changed t :priority -1 :evaporate t) (:line 2 :starts-at-bol t :ends-at-eol t :face wfnames-modified :changed t :priority -1 :evaporate t))) :after (:old-exist (nil nil) :new-exist (t t) :contents ("revenue=42\n" "mode=production\n") :parent-dirs (t t) :lines ((:text "wfnames-move/archive/2026/report-final.txt" :old "wfnames-move/archive/2026/report-final.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-move/deploy/service/app.conf" :old "wfnames-move/deploy/service/app.conf" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix)) :overlays nil :events (hook (function "*Wfnames*" t)) :message nil))"####
    ]];
    ParityBatchCase::value(
        "editing_and_committing_moves_files_into_new_parent_directories",
        elisp_form,
        expect,
    )
}

fn swapping_two_names_preserves_both_payloads_without_temporary_debris() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (wfnames-parity-root "wfnames-swap"))
       (alpha (wfnames-parity-write
               (expand-file-name "alpha.txt" root) "alpha payload\n"))
       (beta (wfnames-parity-write
              (expand-file-name "beta.txt" root) "beta payload\n")))
  (unwind-protect
      (progn
        (wfnames-setup-buffer
         (list alpha beta) #'wfnames-parity-display)
        (set-buffer wfnames-buffer)
        (wfnames-parity-replace-line 0 beta)
        (wfnames-parity-replace-line 1 alpha)
        (let ((before
               (list :lines (wfnames-parity-lines)
                     :modified
                     (mapcar #'wfnames-parity-relative wfnames--modified)
                     :overlay-faces
                     (mapcar (lambda (overlay)
                               (overlay-get overlay 'face))
                             (sort (overlays-in (point-min) (point-max))
                                   (lambda (left right)
                                     (< (overlay-start left)
                                        (overlay-start right))))))))
          (let ((wfnames-interactive-rename nil)
                (wfnames-after-commit-function #'ignore))
            (wfnames-commit-buffer))
          (list
           :before before
           :alpha-content (wfnames-parity-read alpha)
           :beta-content (wfnames-parity-read beta)
           :directory-files
           (directory-files root nil directory-files-no-dot-files-regexp)
           :lines (wfnames-parity-lines)
           :message (current-message))))
    (wfnames-parity-kill-buffer)))
"####;
    let expect = expect![[
        r####"OK (:before (:lines ((:text "wfnames-swap/beta.txt" :old "wfnames-swap/alpha.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-swap/alpha.txt" :old "wfnames-swap/beta.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix)) :modified ("wfnames-swap/beta.txt" "wfnames-swap/alpha.txt") :overlay-faces (wfnames-modified-exists wfnames-modified-exists)) :alpha-content "beta payload\n" :beta-content "alpha payload\n" :directory-files ("alpha.txt" "beta.txt") :lines ((:text "wfnames-swap/beta.txt" :old "wfnames-swap/beta.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-swap/alpha.txt" :old "wfnames-swap/alpha.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix)) :message nil)"####
    ]];
    ParityBatchCase::value(
        "swapping_two_names_preserves_both_payloads_without_temporary_debris",
        elisp_form,
        expect,
    )
}

fn overwrite_confirmation_drives_accepted_and_declined_file_outcomes() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (wfnames-parity-root "wfnames-overwrite"))
       (accepted-source (wfnames-parity-write
                         (expand-file-name "accepted/source.txt" root)
                         "replacement\n"))
       (accepted-target (wfnames-parity-write
                         (expand-file-name "accepted/target.txt" root)
                         "obsolete\n"))
       (declined-source (wfnames-parity-write
                         (expand-file-name "declined/source.txt" root)
                         "keep source\n"))
       (declined-target (wfnames-parity-write
                         (expand-file-name "declined/target.txt" root)
                         "keep target\n"))
       prompts accepted declined)
  (unwind-protect
      (progn
        (wfnames-setup-buffer
         (list accepted-source) #'wfnames-parity-display)
        (set-buffer wfnames-buffer)
        (wfnames-parity-replace-line 0 accepted-target)
        (cl-letf (((symbol-function 'y-or-n-p)
                   (lambda (prompt)
                     (push (list 'accepted prompt) prompts)
                     t)))
          (let ((wfnames-interactive-rename t)
                (wfnames-after-commit-function #'ignore))
            (wfnames-commit-buffer)))
        (setq accepted
              (list :source-exists (file-exists-p accepted-source)
                    :target-content
                    (wfnames-parity-read accepted-target)
                    :line (car (wfnames-parity-lines))
                    :message (current-message)))
        (wfnames-parity-kill-buffer)
        (wfnames-setup-buffer
         (list declined-source) #'wfnames-parity-display)
        (set-buffer wfnames-buffer)
        (wfnames-parity-replace-line 0 declined-target)
        (cl-letf (((symbol-function 'y-or-n-p)
                   (lambda (prompt)
                     (push (list 'declined prompt) prompts)
                     nil)))
          (let ((wfnames-interactive-rename t)
                (wfnames-after-commit-function #'ignore))
            (wfnames-commit-buffer)))
        (setq declined
              (list :source-content
                    (wfnames-parity-read declined-source)
                    :target-content
                    (wfnames-parity-read declined-target)
                    :line (car (wfnames-parity-lines))
                    :message (current-message)))
        (list :prompts (nreverse prompts)
              :accepted accepted
              :declined declined))
    (wfnames-parity-kill-buffer)))
"####;
    let expect = expect![[
        r####"OK (:prompts ((accepted "File `[ORACLE-SANDBOX]/wfnames-overwrite/accepted/target.txt' exists, overwrite? ") (declined "File `[ORACLE-SANDBOX]/wfnames-overwrite/declined/target.txt' exists, overwrite? ")) :accepted (:source-exists nil :target-content "replacement\n" :line (:text "wfnames-overwrite/accepted/target.txt" :old "wfnames-overwrite/accepted/target.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) :message nil) :declined (:source-content "keep source\n" :target-content "keep target\n" :line (:text "wfnames-overwrite/declined/target.txt" :old "wfnames-overwrite/declined/target.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) :message nil))"####
    ]];
    ParityBatchCase::value(
        "overwrite_confirmation_drives_accepted_and_declined_file_outcomes",
        elisp_form,
        expect,
    )
}

fn reverting_selected_and_all_edits_restores_paths_and_change_markers() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (wfnames-parity-root "wfnames-revert"))
       (alpha (wfnames-parity-write
               (expand-file-name "draft/alpha.txt" root) "alpha\n"))
       (beta (wfnames-parity-write
              (expand-file-name "draft/beta.txt" root) "beta\n"))
       (gamma (wfnames-parity-write
               (expand-file-name "draft/gamma.txt" root) "gamma\n"))
       (alpha-new (expand-file-name "ready/alpha-final.txt" root))
       (beta-new (expand-file-name "ready/beta-final.txt" root)))
  (unwind-protect
      (progn
        (wfnames-setup-buffer
         (list alpha beta gamma) #'wfnames-parity-display)
        (set-buffer wfnames-buffer)
        (wfnames-parity-replace-line 0 alpha-new)
        (wfnames-parity-replace-line 1 beta-new)
        (let ((edited
               (list :lines (wfnames-parity-lines)
                     :overlays (wfnames-parity-overlays))))
          (goto-char (point-min))
          (wfnames-revert-current-line 1)
          (let ((one-reverted
                 (list :lines (wfnames-parity-lines)
                       :overlays (wfnames-parity-overlays)
                       :basename-at-point
                       (buffer-substring-no-properties
                        (point) (line-end-position)))))
            (wfnames-revert-changes nil nil)
            (list :edited edited
                  :one-reverted one-reverted
                  :all-reverted
                  (list :lines (wfnames-parity-lines)
                        :overlays (wfnames-parity-overlays)
                        :modified
                        (mapcar #'wfnames-parity-relative
                                wfnames--modified)
                        :files-still-present
                        (mapcar #'file-exists-p
                                (list alpha beta gamma)))))))
    (wfnames-parity-kill-buffer)))
"####;
    let expect = expect![[
        r####"OK (:edited (:lines ((:text "wfnames-revert/ready/alpha-final.txt" :old "wfnames-revert/draft/alpha.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-revert/ready/beta-final.txt" :old "wfnames-revert/draft/beta.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-revert/draft/gamma.txt" :old "wfnames-revert/draft/gamma.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix)) :overlays ((:line 1 :starts-at-bol t :ends-at-eol t :face wfnames-modified :changed t :priority -1 :evaporate t) (:line 2 :starts-at-bol t :ends-at-eol t :face wfnames-modified :changed t :priority -1 :evaporate t))) :one-reverted (:lines ((:text "wfnames-revert/draft/alpha.txt" :old "wfnames-revert/draft/alpha.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-revert/ready/beta-final.txt" :old "wfnames-revert/draft/beta.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-revert/draft/gamma.txt" :old "wfnames-revert/draft/gamma.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix)) :overlays ((:line 2 :starts-at-bol t :ends-at-eol t :face wfnames-modified :changed t :priority -1 :evaporate t)) :basename-at-point "beta-final.txt") :all-reverted (:lines ((:text "wfnames-revert/draft/alpha.txt" :old "wfnames-revert/draft/alpha.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-revert/draft/beta.txt" :old "wfnames-revert/draft/beta.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-revert/draft/gamma.txt" :old "wfnames-revert/draft/gamma.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix)) :overlays nil :modified ("wfnames-revert/draft/beta.txt" nil "wfnames-revert/draft/alpha.txt") :files-still-present (t t t)))"####
    ]];
    ParityBatchCase::value(
        "reverting_selected_and_all_edits_restores_paths_and_change_markers",
        elisp_form,
        expect,
    )
}

fn reordering_a_batch_keeps_each_filename_identity_and_completion_context() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (wfnames-parity-root "wfnames-order"))
       (alpha (wfnames-parity-write
               (expand-file-name "queue/alpha.txt" root) "alpha\n"))
       (beta (wfnames-parity-write
              (expand-file-name "queue/beta.txt" root) "beta\n"))
       (gamma (wfnames-parity-write
               (expand-file-name "queue/gamma.txt" root) "gamma\n"))
       (docs (expand-file-name "queue/docs" root)))
  (make-directory docs t)
  (unwind-protect
      (progn
        (wfnames-setup-buffer
         (list alpha beta gamma) #'wfnames-parity-display)
        (set-buffer wfnames-buffer)
        ;; A user promotes beta to the front and moves alpha to the end.
        (goto-char (point-min))
        (forward-line 1)
        (wfnames-move-line-up)
        (forward-line 1)
        (wfnames-move-line-down)
        (let ((ordered (wfnames-parity-lines)))
          ;; Then the user starts replacing the final basename and asks for
          ;; filename completion within that exact line.
          (goto-char (point-max))
          (forward-line -1)
          (let* ((bol (line-beginning-position))
                 (eol (line-end-position)))
            (delete-region (- eol (length "alpha.txt")) eol)
            (goto-char (line-end-position))
            (insert "d")
            (let* ((capf (wfnames-capf))
                   (beg (nth 0 capf))
                   (end (nth 1 capf))
                   (table (nth 2 capf))
                   (input
                    (buffer-substring-no-properties beg end))
                   (candidates
                    (completion-all-completions
                     input table nil (length input)))
                   (candidate-tail candidates)
                   candidate-items)
              ;; `completion-all-completions' returns an improper list whose
              ;; final cdr is the completion base size.  Preserve both halves
              ;; of that public protocol instead of pretending it is a plain
              ;; list.
              (while (consp candidate-tail)
                (push (car candidate-tail) candidate-items)
                (setq candidate-tail (cdr candidate-tail)))
              (list
               :ordered ordered
               :edited-lines (wfnames-parity-lines)
               :capf-region
               input
               :capf-bounds
               (list :beg-at-bol (= beg bol)
                     :end-at-point (= end (point)))
               :table (if (eq table #'completion-file-name-table)
                          'completion-file-name-table
                        table)
               :candidates
               (mapcar #'wfnames-parity-relative
                       (nreverse candidate-items))
               :completion-base-from-end
               (- candidate-tail (length input))
               :overlay (wfnames-parity-overlays))))))
    (wfnames-parity-kill-buffer)))
"####;
    let expect = expect![[
        r####"OK (:ordered ((:text "wfnames-order/queue/beta.txt" :old "wfnames-order/queue/beta.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-order/queue/gamma.txt" :old "wfnames-order/queue/gamma.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-order/queue/alpha.txt" :old "wfnames-order/queue/alpha.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix)) :edited-lines ((:text "wfnames-order/queue/beta.txt" :old "wfnames-order/queue/beta.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-order/queue/gamma.txt" :old "wfnames-order/queue/gamma.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix) (:text "wfnames-order/queue/d" :old "wfnames-order/queue/alpha.txt" :face wfnames-files :prefix "* " :prefix-face wfnames-prefix)) :capf-region "[ORACLE-SANDBOX]/wfnames-order/queue/d" :capf-bounds (:beg-at-bol t :end-at-point t) :table completion-file-name-table :candidates ("docs/") :completion-base-from-end -1 :overlay ((:line 3 :starts-at-bol t :ends-at-eol t :face wfnames-modified :changed t :priority -1 :evaporate t)))"####
    ]];
    ParityBatchCase::value(
        "reordering_a_batch_keeps_each_filename_identity_and_completion_context",
        elisp_form,
        expect,
    )
}

#[test]
fn wfnames_package_batch() {
    let cases = vec![
        setup_buffer_models_mixed_files_and_appends_without_duplicates(),
        editing_and_committing_moves_files_into_new_parent_directories(),
        swapping_two_names_preserves_both_payloads_without_temporary_debris(),
        overwrite_confirmation_drives_accepted_and_declined_file_outcomes(),
        reverting_selected_and_all_edits_restores_paths_and_change_markers(),
        reordering_a_batch_keeps_each_filename_identity_and_completion_context(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed wfnames parity test");
    assert_oracle_batch_cases(wfnames_oracle(), test_name, "wfnames_parity", &cases);
}
