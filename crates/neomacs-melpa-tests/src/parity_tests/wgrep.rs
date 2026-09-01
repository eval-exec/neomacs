use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, WGREP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'grep)
(require 'wgrep)

(defun neomacs-wgrep-test-root (name)
  "Create a deterministic project-local Wgrep sandbox for NAME."
  (let ((root (file-name-as-directory
               (expand-file-name
                (concat "wgrep-" name)
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-wgrep-test-write (root relative contents)
  "Write CONTENTS to RELATIVE below ROOT and return the full path."
  (let ((path (expand-file-name relative root))
        (coding-system-for-write 'utf-8-unix))
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert contents))
    path))

(defun neomacs-wgrep-test-read (root relative)
  "Read RELATIVE below ROOT without text properties."
  (with-temp-buffer
    (insert-file-contents (expand-file-name relative root))
    (buffer-substring-no-properties (point-min) (point-max))))

(defun neomacs-wgrep-test-fixture (name files result-lines)
  "Create FILES and a realistic stopped grep buffer with RESULT-LINES."
  (let* ((root (neomacs-wgrep-test-root name))
         (buffer (generate-new-buffer (format " *wgrep-%s*" name))))
    (dolist (file files)
      (neomacs-wgrep-test-write root (car file) (cdr file)))
    (with-current-buffer buffer
      (grep-mode)
      (setq default-directory root)
      (let ((inhibit-read-only t))
        (erase-buffer)
        (insert "grep -nH release *.txt\n"
                "Grep started\n"
                "\n"
                "Matches:\n")
        (dolist (line result-lines)
          (insert line "\n"))
        (insert "\nGrep finished\n"))
      (set-buffer-modified-p nil)
      (let ((grep-use-null-filename-separator nil))
        (wgrep-setup)))
    (list :root root :buffer buffer)))

(defun neomacs-wgrep-test-cleanup (fixture)
  "Kill buffers and remove files owned by FIXTURE."
  (let ((root (plist-get fixture :root))
        (grep-buffer (plist-get fixture :buffer)))
    (dolist (buffer (buffer-list))
      (with-current-buffer buffer
        (when (and buffer-file-name
                   (string-prefix-p root (expand-file-name buffer-file-name)))
          (set-buffer-modified-p nil)
          (kill-buffer buffer))))
    (when (buffer-live-p grep-buffer)
      (with-current-buffer grep-buffer
        (set-buffer-modified-p nil)
        (kill-buffer grep-buffer)))
    (when (file-exists-p root)
      (delete-directory root t))))

(defun neomacs-wgrep-test-line-records ()
  "Return parsed result and ignored lines from the current grep buffer."
  (save-excursion
    (goto-char (point-min))
    (let (records)
      (while (not (eobp))
        (let* ((begin (line-beginning-position))
               (end (line-end-position))
               (file (get-text-property begin 'wgrep-line-filename))
               (line (get-text-property begin 'wgrep-line-number))
               (ignored (get-text-property begin 'wgrep-ignore)))
          (when (or file ignored)
            (let ((contents-start
                   (and file
                        (next-single-property-change
                         begin 'wgrep-line-filename nil end))))
              (push
               (list :text (buffer-substring-no-properties begin end)
                     :file file
                     :line line
                     :ignored (and ignored t)
                     :header-read-only
                     (and (get-text-property begin 'read-only) t)
                     :contents-read-only
                     (and contents-start
                          (get-text-property contents-start 'read-only)
                          t))
               records))))
        (forward-line 1))
      (nreverse records))))

(defun neomacs-wgrep-test-overlay-records ()
  "Return stable Wgrep overlay state in the current buffer."
  (mapcar
   (lambda (overlay)
     (list :range (list (overlay-start overlay) (overlay-end overlay))
           :text (and (overlay-start overlay)
                      (buffer-substring-no-properties
                       (overlay-start overlay) (overlay-end overlay)))
           :change (and (overlay-get overlay 'wgrep-changed) t)
           :result (and (overlay-get overlay 'wgrep-result) t)
           :file (let ((file (overlay-get overlay 'wgrep-filename)))
                   (and file (file-name-nondirectory file)))
           :line (overlay-get overlay 'wgrep-linum)
           :old (overlay-get overlay 'wgrep-old-text)
           :new (overlay-get overlay 'wgrep-edit-text)
           :face (overlay-get overlay 'face)
           :reject (overlay-get overlay 'wgrep-reject-message)))
   (sort
    (cl-remove-if-not
     (lambda (overlay) (overlay-get overlay 'wgrep))
     (overlays-in (point-min) (point-max)))
    (lambda (left right)
      (if (= (overlay-start left) (overlay-start right))
          (< (or (overlay-get left 'priority) -1)
             (or (overlay-get right 'priority) -1))
        (< (overlay-start left) (overlay-start right)))))))

(defun neomacs-wgrep-test-replace-result (file line replacement)
  "Replace FILE at LINE in the current writable grep buffer."
  (unless (wgrep-goto-grep-line file line)
    (error "No result for %s:%s" file line))
  (delete-region (point) (line-end-position))
  (insert replacement))

(defun neomacs-wgrep-test-file-state (root relative)
  "Return live buffer and disk state for RELATIVE below ROOT."
  (let* ((path (expand-file-name relative root))
         (buffer (get-file-buffer path)))
    (list :buffer-live (and (buffer-live-p buffer) t)
          :buffer-text
          (and (buffer-live-p buffer)
               (with-current-buffer buffer
                 (buffer-substring-no-properties (point-min) (point-max))))
          :modified
          (and (buffer-live-p buffer)
               (with-current-buffer buffer (buffer-modified-p)))
          :file-overlays
          (and (buffer-live-p buffer)
               (with-current-buffer buffer
                 (mapcar (lambda (overlay)
                           (list (overlay-start overlay)
                                 (overlay-end overlay)
                                 (overlay-get overlay 'face)))
                         (wgrep-file-overlays))))
          :disk (neomacs-wgrep-test-read root relative))))

(defun neomacs-wgrep-test-capture-error (function)
  "Return FUNCTION's value or exact error data."
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"###;

fn package_registration_exposes_grep_hook_commands_keymaps_and_defaults() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'wgrep package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'wgrep) t))
   :hook (and (memq #'wgrep-setup grep-setup-hook) t)
   :defaults
   (list wgrep-change-readonly-file wgrep-enable-key
         wgrep-auto-save-buffer wgrep-too-many-file-length)
   :surface
   (mapcar #'fboundp
           '(wgrep-change-to-wgrep-mode wgrep-finish-edit
             wgrep-abort-changes wgrep-mark-deletion
             wgrep-toggle-readonly-area wgrep-remove-change
             wgrep-remove-all-change wgrep-save-all-buffers))
   :edit-bindings
   (mapcar (lambda (key) (cons key (lookup-key wgrep-mode-map (kbd key))))
           '("C-c C-c" "C-c C-d" "C-c C-e" "C-c C-p"
             "C-c C-r" "C-c C-u" "C-c C-k" "C-x C-q" "C-x C-s"))
   :grep-entry
   (with-temp-buffer
     (grep-mode)
     (wgrep-setup)
     (list (key-binding (kbd "C-c C-p"))
           (local-variable-p 'wgrep-line-file-regexp)
           (eq wgrep-original-mode-map (current-local-map))))))
"###;
    let expected = expect![[
        r#"OK (:package (:name wgrep :version "20230203.1214" :requirements ((emacs (25 1))) :feature t) :hook t :defaults (nil "\3\20" nil 10) :surface (t t t t t t t t) :edit-bindings (("C-c C-c" . wgrep-finish-edit) ("C-c C-d" . wgrep-mark-deletion) ("C-c C-e" . wgrep-finish-edit) ("C-c C-p" . wgrep-toggle-readonly-area) ("C-c C-r" . wgrep-remove-change) ("C-c C-u" . wgrep-remove-all-change) ("C-c C-k" . wgrep-abort-changes) ("C-x C-q" . wgrep-exit) ("C-x C-s" . wgrep-finish-edit)) :grep-entry (wgrep-change-to-wgrep-mode t t))"#
    ]];
    ParityBatchCase::value(
        "package_registration_exposes_grep_hook_commands_keymaps_and_defaults",
        elisp_form,
        expected,
    )
}

fn entering_wgrep_parses_match_context_and_ignored_lines_into_editable_fields() -> ParityBatchCase {
    let elisp_form = r###"
(let ((fixture
       (neomacs-wgrep-test-fixture
        "parse-context"
        '(("app.txt" . "alpha release\nbeta release\ngamma stable\n")
          ("notes.txt" . "draft\nrelease checklist\n"))
        '("app.txt-1-alpha release"
          "app.txt:2:beta release"
          "app.txt-3-gamma stable"
          "--"
          "notes.txt:2:release checklist"
          "missing.txt:9:ghost"))))
  (unwind-protect
      (with-current-buffer (plist-get fixture :buffer)
        (wgrep-change-to-wgrep-mode)
        (list :lines (neomacs-wgrep-test-line-records)
              :state
              (list :prepared wgrep-prepared
                    :readonly-state wgrep-readonly-state
                    :buffer-read-only buffer-read-only
                    :sibling-live (and (buffer-live-p wgrep-sibling-buffer) t)
                    :after-change
                    (and (memq #'wgrep-after-change-function
                               after-change-functions)
                         t)
                    :map (eq (current-local-map) wgrep-mode-map))))
    (neomacs-wgrep-test-cleanup fixture)))
"###;
    let expected = expect![[
        r#"OK (:lines ((:text "app.txt-1-alpha release" :file "app.txt" :line 1 :ignored nil :header-read-only t :contents-read-only nil) (:text "app.txt:2:beta release" :file "app.txt" :line 2 :ignored nil :header-read-only t :contents-read-only nil) (:text "app.txt-3-gamma stable" :file "app.txt" :line 3 :ignored nil :header-read-only t :contents-read-only nil) (:text "--" :file nil :line nil :ignored t :header-read-only t :contents-read-only nil) (:text "notes.txt:2:release checklist" :file "notes.txt" :line 2 :ignored nil :header-read-only t :contents-read-only nil)) :state (:prepared t :readonly-state t :buffer-read-only nil :sibling-live t :after-change t :map t))"#
    ]];
    ParityBatchCase::value(
        "entering_wgrep_parses_match_context_and_ignored_lines_into_editable_fields",
        elisp_form,
        expected,
    )
}

fn finishing_two_file_edits_updates_live_buffers_then_save_all_persists_them() -> ParityBatchCase {
    let elisp_form = r###"
(let ((fixture
       (neomacs-wgrep-test-fixture
        "multi-file"
        '(("app.txt" . "alpha old\nbeta old\n")
          ("notes.txt" . "run old\nverify old\n"))
        '("app.txt:1:alpha old"
          "notes.txt:2:verify old"))))
  (unwind-protect
      (with-current-buffer (plist-get fixture :buffer)
        (let ((root (plist-get fixture :root)))
          (wgrep-change-to-wgrep-mode)
          (neomacs-wgrep-test-replace-result "app.txt" 1 "alpha new")
          (neomacs-wgrep-test-replace-result "notes.txt" 2 "verify new")
          (let ((pending (neomacs-wgrep-test-overlay-records)))
            (wgrep-finish-edit)
            (let ((before-save
                   (list :app (neomacs-wgrep-test-file-state root "app.txt")
                         :notes (neomacs-wgrep-test-file-state root "notes.txt")
                         :grep-read-only buffer-read-only
                         :grep-results (neomacs-wgrep-test-overlay-records))))
              (wgrep-save-all-buffers)
              (list :pending pending
                    :before-save before-save
                    :after-save
                    (list :app (neomacs-wgrep-test-file-state root "app.txt")
                          :notes (neomacs-wgrep-test-file-state root "notes.txt")))))))
    (neomacs-wgrep-test-cleanup fixture)))
"###;
    let expected = expect![[
        r#"OK (:pending ((:range (47 66) :text "app.txt:1:alpha new" :change t :result nil :file "app.txt" :line 1 :old "alpha old" :new "alpha new" :face wgrep-face :reject nil) (:range (67 89) :text "notes.txt:2:verify new" :change t :result nil :file "notes.txt" :line 2 :old "verify old" :new "verify new" :face wgrep-face :reject nil)) :before-save (:app (:buffer-live t :buffer-text "alpha new\nbeta old\n" :modified t :file-overlays ((1 10 wgrep-file-face)) :disk "alpha old\nbeta old\n") :notes (:buffer-live t :buffer-text "run old\nverify new\n" :modified t :file-overlays ((9 19 wgrep-file-face)) :disk "run old\nverify old\n") :grep-read-only t :grep-results ((:range (57 66) :text "alpha new" :change nil :result t :file nil :line nil :old nil :new nil :face wgrep-done-face :reject nil) (:range (79 89) :text "verify new" :change nil :result t :file nil :line nil :old nil :new nil :face wgrep-done-face :reject nil))) :after-save (:app (:buffer-live t :buffer-text "alpha new\nbeta old\n" :modified nil :file-overlays nil :disk "alpha new\nbeta old\n") :notes (:buffer-live t :buffer-text "run old\nverify new\n" :modified nil :file-overlays nil :disk "run old\nverify new\n")))"#
    ]];
    ParityBatchCase::value(
        "finishing_two_file_edits_updates_live_buffers_then_save_all_persists_them",
        elisp_form,
        expected,
    )
}

fn newline_insertion_and_context_line_deletion_apply_as_one_transaction() -> ParityBatchCase {
    let elisp_form = r###"
(let ((fixture
       (neomacs-wgrep-test-fixture
        "newline-delete"
        '(("release.txt" . "HOGE\nFOO\nBAZ\n"))
        '("release.txt:1:HOGE"
          "release.txt-2-FOO"
          "release.txt-3-BAZ"))))
  (unwind-protect
      (with-current-buffer (plist-get fixture :buffer)
        (let ((root (plist-get fixture :root)))
          (wgrep-change-to-wgrep-mode)
          (neomacs-wgrep-test-replace-result
           "release.txt" 1 "FIRST\nSECOND")
          (wgrep-goto-grep-line "release.txt" 2)
          (wgrep-mark-deletion)
          (let ((pending (neomacs-wgrep-test-overlay-records)))
            (wgrep-finish-edit)
            (wgrep-save-all-buffers)
            (list :pending pending
                  :final (neomacs-wgrep-test-read root "release.txt")
                  :results (neomacs-wgrep-test-overlay-records)))))
    (neomacs-wgrep-test-cleanup fixture)))
"###;
    let expected = expect![[
        r#"OK (:pending ((:range (47 73) :text "release.txt:1:FIRST\nSECOND" :change t :result nil :file "release.txt" :line 1 :old "HOGE" :new "FIRST\nSECOND" :face wgrep-face :reject nil) (:range (74 88) :text "release.txt-2-" :change t :result nil :file "release.txt" :line 2 :old "FOO" :new nil :face wgrep-delete-face :reject nil)) :final "FIRST\nSECOND\nBAZ\n" :results ((:range (61 73) :text "FIRST\nSECOND" :change nil :result t :file nil :line nil :old nil :new nil :face wgrep-done-face :reject nil) (:range (88 88) :text "" :change nil :result t :file nil :line nil :old nil :new nil :face wgrep-done-face :reject nil)))"#
    ]];
    ParityBatchCase::value(
        "newline_insertion_and_context_line_deletion_apply_as_one_transaction",
        elisp_form,
        expected,
    )
}

fn abort_restores_the_original_grep_buffer_and_leaves_the_source_untouched() -> ParityBatchCase {
    let elisp_form = r###"
(let ((fixture
       (neomacs-wgrep-test-fixture
        "abort"
        '(("runbook.txt" . "deploy old\nverify old\n"))
        '("runbook.txt:1:deploy old"
          "runbook.txt:2:verify old"))))
  (unwind-protect
      (with-current-buffer (plist-get fixture :buffer)
        (let ((root (plist-get fixture :root))
              (original (buffer-substring-no-properties
                         (point-min) (point-max))))
          (wgrep-change-to-wgrep-mode)
          (neomacs-wgrep-test-replace-result "runbook.txt" 1 "deploy new")
          (let ((edited (buffer-substring-no-properties
                         (point-min) (point-max)))
                (pending (neomacs-wgrep-test-overlay-records)))
            (wgrep-abort-changes)
            (list :edited edited
                  :pending pending
                  :restored (buffer-substring-no-properties
                             (point-min) (point-max))
                  :same-as-original
                  (equal original
                         (buffer-substring-no-properties
                          (point-min) (point-max)))
                  :read-only buffer-read-only
                  :overlays (neomacs-wgrep-test-overlay-records)
                  :source (neomacs-wgrep-test-read root "runbook.txt")))))
    (neomacs-wgrep-test-cleanup fixture)))
"###;
    let expected = expect![[
        r#"OK (:edited "grep -nH release *.txt\nGrep started\n\nMatches:\nrunbook.txt:1:deploy new\nrunbook.txt:2:verify old\n\nGrep finished\n" :pending ((:range (47 71) :text "runbook.txt:1:deploy new" :change t :result nil :file "runbook.txt" :line 1 :old "deploy old" :new "deploy new" :face wgrep-face :reject nil)) :restored "grep -nH release *.txt\nGrep started\n\nMatches:\nrunbook.txt:1:deploy old\nrunbook.txt:2:verify old\n\nGrep finished\n" :same-as-original t :read-only t :overlays nil :source "deploy old\nverify old\n")"#
    ]];
    ParityBatchCase::value(
        "abort_restores_the_original_grep_buffer_and_leaves_the_source_untouched",
        elisp_form,
        expected,
    )
}

fn unmarking_one_of_two_edits_applies_only_the_remaining_change() -> ParityBatchCase {
    let elisp_form = r###"
(let ((fixture
       (neomacs-wgrep-test-fixture
        "unmark"
        '(("service.txt" . "alpha old\nbeta old\n"))
        '("service.txt:1:alpha old"
          "service.txt:2:beta old"))))
  (unwind-protect
      (with-current-buffer (plist-get fixture :buffer)
        (let ((root (plist-get fixture :root)))
          (wgrep-change-to-wgrep-mode)
          (neomacs-wgrep-test-replace-result "service.txt" 1 "alpha draft")
          (neomacs-wgrep-test-replace-result "service.txt" 2 "beta final")
          (let* ((both (wgrep-edit-field-overlays))
                 (first (car both)))
            (wgrep-remove-change (overlay-start first) (overlay-end first)))
          (let ((remaining (neomacs-wgrep-test-overlay-records))
                (grep-text (buffer-substring-no-properties
                            (point-min) (point-max))))
            (wgrep-finish-edit)
            (wgrep-save-all-buffers)
            (list :grep-text grep-text
                  :remaining remaining
                  :source (neomacs-wgrep-test-read root "service.txt")
                  :results (neomacs-wgrep-test-overlay-records)))))
    (neomacs-wgrep-test-cleanup fixture)))
"###;
    let expected = expect![[
        r#"OK (:grep-text "grep -nH release *.txt\nGrep started\n\nMatches:\nservice.txt:1:alpha draft\nservice.txt:2:beta final\n\nGrep finished\n" :remaining ((:range (73 97) :text "service.txt:2:beta final" :change t :result nil :file "service.txt" :line 2 :old "beta old" :new "beta final" :face wgrep-face :reject nil)) :source "alpha old\nbeta final\n" :results ((:range (87 97) :text "beta final" :change nil :result t :file nil :line nil :old nil :new nil :face wgrep-done-face :reject nil)))"#
    ]];
    ParityBatchCase::value(
        "unmarking_one_of_two_edits_applies_only_the_remaining_change",
        elisp_form,
        expected,
    )
}

fn stale_source_buffer_rejects_the_edit_and_preserves_both_versions_for_resolution()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((fixture
       (neomacs-wgrep-test-fixture
        "stale-source"
        '(("config.txt" . "owner old\nstatus ready\n"))
        '("config.txt:1:owner old"))))
  (unwind-protect
      (with-current-buffer (plist-get fixture :buffer)
        (let* ((root (plist-get fixture :root))
               (path (expand-file-name "config.txt" root)))
          (wgrep-change-to-wgrep-mode)
          (neomacs-wgrep-test-replace-result "config.txt" 1 "owner proposed")
          (with-current-buffer (find-file-noselect path)
            (goto-char (point-min))
            (delete-region (line-beginning-position) (line-end-position))
            (insert "owner external"))
          (wgrep-finish-edit)
          (list :source (neomacs-wgrep-test-file-state root "config.txt")
                :grep-read-only buffer-read-only
                :unapplied-count (length (wgrep-edit-field-overlays))
                :overlays (neomacs-wgrep-test-overlay-records))))
    (neomacs-wgrep-test-cleanup fixture)))
"###;
    let expected = expect![[
        r#"OK (:source (:buffer-live t :buffer-text "owner external\nstatus ready\n" :modified t :file-overlays nil :disk "owner old\nstatus ready\n") :grep-read-only t :unapplied-count 1 :overlays ((:range (47 74) :text "config.txt:1:owner proposed" :change t :result nil :file "config.txt" :line 1 :old "owner old" :new "owner proposed" :face wgrep-face :reject nil) (:range (60 74) :text "owner proposed" :change nil :result t :file nil :line nil :old nil :new nil :face wgrep-reject-face :reject "Buffer was changed after grep.")))"#
    ]];
    ParityBatchCase::value(
        "stale_source_buffer_rejects_the_edit_and_preserves_both_versions_for_resolution",
        elisp_form,
        expected,
    )
}

fn read_only_boundaries_protect_headers_and_can_be_toggled_without_dirtying_the_buffer()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((fixture
       (neomacs-wgrep-test-fixture
        "read-only"
        '(("app.txt" . "release ready\n"))
        '("app.txt:1:release ready"))))
  (unwind-protect
      (with-current-buffer (plist-get fixture :buffer)
        (wgrep-change-to-wgrep-mode)
        (let ((header-error
               (neomacs-wgrep-test-capture-error
                (lambda ()
                  (goto-char (point-min))
                  (delete-char 1))))
              (modified-before (buffer-modified-p)))
          (wgrep-toggle-readonly-area)
          (let ((editable
                 (list :state wgrep-readonly-state
                       :header (get-text-property (point-min) 'read-only)
                       :result
                       (progn
                         (wgrep-goto-grep-line "app.txt" 1)
                         (get-text-property (line-beginning-position)
                                            'read-only))
                       :modified (buffer-modified-p))))
            (wgrep-toggle-readonly-area)
            (list :header-error header-error
                  :modified-before modified-before
                  :editable editable
                  :protected
                  (list :state wgrep-readonly-state
                        :header (and (get-text-property
                                      (point-min) 'read-only)
                                     t)
                        :result
                        (progn
                          (wgrep-goto-grep-line "app.txt" 1)
                          (and (get-text-property
                                (line-beginning-position) 'read-only)
                               t))
                        :modified (buffer-modified-p))))))
    (neomacs-wgrep-test-cleanup fixture)))
"###;
    let expected = expect![[
        r#"OK (:header-error (:error text-read-only :data nil :message "Text is read-only") :modified-before nil :editable (:state nil :header nil :result nil :modified nil) :protected (:state t :header t :result t :modified nil))"#
    ]];
    ParityBatchCase::value(
        "read_only_boundaries_protect_headers_and_can_be_toggled_without_dirtying_the_buffer",
        elisp_form,
        expected,
    )
}

#[test]
fn wgrep_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(WGREP_MELPA_PIN, "wgrep.el")
            .expect("prepare revision-pinned Wgrep source below ./tmp")
            .with_timeout(Duration::from_secs(300))
            .with_prelude(PRELUDE),
        "wgrep-package-batch",
        "Wgrep",
        &[
            package_registration_exposes_grep_hook_commands_keymaps_and_defaults(),
            entering_wgrep_parses_match_context_and_ignored_lines_into_editable_fields(),
            finishing_two_file_edits_updates_live_buffers_then_save_all_persists_them(),
            newline_insertion_and_context_line_deletion_apply_as_one_transaction(),
            abort_restores_the_original_grep_buffer_and_leaves_the_source_untouched(),
            unmarking_one_of_two_edits_applies_only_the_remaining_change(),
            stale_source_buffer_rejects_the_edit_and_preserves_both_versions_for_resolution(),
            read_only_boundaries_protect_headers_and_can_be_toggled_without_dirtying_the_buffer(),
        ],
    );
}
