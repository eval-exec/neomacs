use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HELM_XREF_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HELM_XREF_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const HELM_XREF_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'xref)

(setq xref-history-storage #'xref-global-history
      xref-prompt-for-identifier nil)

(defvar neomacs-helm-xref-test-definitions nil)
(defvar neomacs-helm-xref-test-references nil)
(defvar neomacs-helm-xref-test-definition-fetches 0)
(defvar neomacs-helm-xref-test-reference-fetches 0)
(defvar neomacs-helm-xref-test-active-xrefs nil)
(defvar neomacs-helm-xref-test-current-root nil)
(defvar neomacs-helm-xref-test-helm-calls nil)
(defvar neomacs-helm-xref-test-selection-index nil)
(defvar neomacs-helm-xref-test-action-kind nil)
(defvar neomacs-helm-xref-test-jump-log nil)
(defvar neomacs-helm-xref-test-return-log nil)

(defun neomacs-helm-xref-test-backend ()
  "Return the deterministic Xref backend used by these workflows."
  'neomacs-helm-xref-test)

(cl-defmethod xref-backend-identifier-at-point
  ((_backend (eql neomacs-helm-xref-test)))
  (thing-at-point 'symbol t))

(cl-defmethod xref-backend-definitions
  ((_backend (eql neomacs-helm-xref-test)) _identifier)
  (setq neomacs-helm-xref-test-definition-fetches
        (1+ neomacs-helm-xref-test-definition-fetches))
  (copy-sequence neomacs-helm-xref-test-definitions))

(cl-defmethod xref-backend-references
  ((_backend (eql neomacs-helm-xref-test)) _identifier)
  (setq neomacs-helm-xref-test-reference-fetches
        (1+ neomacs-helm-xref-test-reference-fetches))
  (copy-sequence neomacs-helm-xref-test-references))

(defun neomacs-helm-xref-test-root (name)
  "Create a deterministic sandbox directory for workflow NAME."
  (let ((root (file-name-as-directory
               (expand-file-name
                (concat "helm-xref-" name)
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-helm-xref-test-write (path contents)
  "Write CONTENTS to PATH and return PATH."
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents))
  path)

(defun neomacs-helm-xref-test-visit (path identifier)
  "Visit PATH and leave point at the start of IDENTIFIER."
  (switch-to-buffer (find-file-noselect path))
  (goto-char (point-min))
  (search-forward identifier)
  (goto-char (match-beginning 0))
  (setq-local xref-backend-functions '(neomacs-helm-xref-test-backend))
  (current-buffer))

(defun neomacs-helm-xref-test-normalize (value &optional root)
  "Replace ROOT in string VALUE with a stable marker."
  (if (and root (stringp value))
      (replace-regexp-in-string
       (regexp-quote (file-name-as-directory root)) "$ROOT/" value t t)
    value))

(defun neomacs-helm-xref-test-location (&optional root)
  "Describe point in the current buffer relative to ROOT."
  (list
   :buffer (buffer-name)
   :file (and buffer-file-name
              (neomacs-helm-xref-test-normalize buffer-file-name root))
   :line (line-number-at-pos)
   :column (current-column)
   :text (buffer-substring-no-properties
          (line-beginning-position) (line-end-position))
   :symbol (thing-at-point 'symbol t)))

(defun neomacs-helm-xref-test-marker (marker root)
  "Describe history MARKER relative to ROOT."
  (let ((buffer (marker-buffer marker)))
    (when buffer
      (with-current-buffer buffer
        (save-excursion
          (goto-char marker)
          (list
           :buffer (buffer-name)
           :file (and buffer-file-name
                      (neomacs-helm-xref-test-normalize
                       buffer-file-name root))
           :position (marker-position marker)
           :line (line-number-at-pos)
           :column (current-column)))))))

(defun neomacs-helm-xref-test-history (root)
  "Describe the exact global Xref history relative to ROOT."
  (let ((history (xref-global-history)))
    (list
     :backward
     (mapcar (lambda (marker)
               (neomacs-helm-xref-test-marker marker root))
             (car history))
     :forward
     (mapcar (lambda (marker)
               (neomacs-helm-xref-test-marker marker root))
             (cdr history)))))

(defun neomacs-helm-xref-test-face-runs (text root)
  "Describe every font-lock face run in candidate TEXT."
  (let ((position 0)
        (length (length text))
        runs)
    (while (< position length)
      (let* ((face (get-text-property position 'font-lock-face text))
             (next (or (next-single-property-change
                        position 'font-lock-face text)
                       length)))
        (when face
          (push
           (list :text
                 (neomacs-helm-xref-test-normalize
                  (substring-no-properties text position next) root)
                 :face face)
           runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-helm-xref-test-candidate (candidate root)
  "Describe Helm Xref CANDIDATE without materializing its marker."
  (let* ((display (car candidate))
         (item (cdr candidate))
         (location (xref-item-location item)))
    (list
     :display
     (neomacs-helm-xref-test-normalize
      (substring-no-properties display) root)
     :faces (neomacs-helm-xref-test-face-runs display root)
     :summary (substring-no-properties (xref-item-summary item))
     :group
     (neomacs-helm-xref-test-normalize
      (xref-location-group location) root)
     :line (xref-location-line location)
     :identity-index
     (cl-position item neomacs-helm-xref-test-active-xrefs :test #'eq))))

(defun neomacs-helm-xref-test-source (source root)
  "Describe the real Helm Xref SOURCE relative to ROOT."
  (let ((candidates (helm-get-candidates source)))
    (list
     :name (assoc-default 'name source)
     :group (assoc-default 'group source)
     :candidate-number-limit
     (assoc-default 'candidate-number-limit source)
     :actions (mapcar #'car (assoc-default 'action source))
     :candidates
     (mapcar (lambda (candidate)
               (neomacs-helm-xref-test-candidate candidate root))
             candidates))))

(defun neomacs-helm-xref-test-action-result (result)
  "Normalize a real Helm action RESULT."
  (cond
   ((bufferp result) (list :buffer (buffer-name result)))
   ((overlayp result)
    (list :overlay (overlay-start result) (overlay-end result)
          (overlay-get result 'face)))
   (t result)))

(defun neomacs-helm-xref-test-helm (&rest arguments)
  "Stand in only for Helm's unattended interactive chooser boundary."
  (let* ((source (plist-get arguments :sources))
         (candidates (helm-get-candidates source))
         (candidate (and (integerp neomacs-helm-xref-test-selection-index)
                         (nth neomacs-helm-xref-test-selection-index
                              candidates)))
         (source-state
          (neomacs-helm-xref-test-source
           source neomacs-helm-xref-test-current-root))
         action-name
         action-result)
    (when candidate
      (if (eq neomacs-helm-xref-test-action-kind 'persistent)
          (progn
            (setq action-name :persistent)
            (setq action-result
                  (funcall (assoc-default 'persistent-action source)
                           (cdr candidate))))
        (let ((action
               (nth neomacs-helm-xref-test-action-kind
                    (assoc-default 'action source))))
          (setq action-name (car action))
          (setq action-result (funcall (cdr action) (cdr candidate))))))
    (push
     (list
      :buffer (plist-get arguments :buffer)
      :truncate-lines (plist-get arguments :truncate-lines)
      :input-present (and (plist-member arguments :input) t)
      :input (plist-get arguments :input)
      :source source-state
      :selection
      (and candidate
           (list :index neomacs-helm-xref-test-selection-index
                 :action action-name
                 :result
                 (neomacs-helm-xref-test-action-result action-result))))
     neomacs-helm-xref-test-helm-calls)
    :helm-complete))

(defun neomacs-helm-xref-test-after-jump ()
  "Record the public Xref jump hook with its bound item."
  (push
   (list :summary (xref-item-summary xref-current-item)
         :location
         (neomacs-helm-xref-test-location
          neomacs-helm-xref-test-current-root))
   neomacs-helm-xref-test-jump-log))

(defun neomacs-helm-xref-test-after-return ()
  "Record the public Xref return hook at its destination."
  (push
   (neomacs-helm-xref-test-location
    neomacs-helm-xref-test-current-root)
   neomacs-helm-xref-test-return-log))

(defun neomacs-helm-xref-test-overlay ()
  "Describe Helm's current-line preview overlay."
  (when (overlayp helm-match-line-overlay)
    (list
     :range (list (overlay-start helm-match-line-overlay)
                  (overlay-end helm-match-line-overlay))
     :text (with-current-buffer (overlay-buffer helm-match-line-overlay)
             (buffer-substring-no-properties
              (overlay-start helm-match-line-overlay)
              (overlay-end helm-match-line-overlay)))
     :face (overlay-get helm-match-line-overlay 'face))))

(defun neomacs-helm-xref-test-windows ()
  "Describe live non-minibuffer windows starting with the selected one."
  (let ((selected (selected-window)))
    (mapcar
     (lambda (window)
       (list :selected (eq window selected)
             :buffer (buffer-name (window-buffer window))
             :point (window-point window)))
     (window-list nil 'nomini selected))))

(defun neomacs-helm-xref-test-custom-formatter (file line summary)
  "Format an Xref candidate like a user's custom compact formatter."
  (format "%s|%s|%s"
          (file-name-nondirectory file)
          (if (integerp line) line "buffer")
          (upcase summary)))

(defun neomacs-helm-xref-test-reset ()
  "Reset shared package, backend, hook, and history state."
  (when (overlayp helm-match-line-overlay)
    (delete-overlay helm-match-line-overlay))
  (setq helm-match-line-overlay nil
        helm-xref-alist nil
        neomacs-helm-xref-test-definitions nil
        neomacs-helm-xref-test-references nil
        neomacs-helm-xref-test-definition-fetches 0
        neomacs-helm-xref-test-reference-fetches 0
        neomacs-helm-xref-test-active-xrefs nil
        neomacs-helm-xref-test-current-root nil
        neomacs-helm-xref-test-helm-calls nil
        neomacs-helm-xref-test-selection-index nil
        neomacs-helm-xref-test-action-kind nil
        neomacs-helm-xref-test-jump-log nil
        neomacs-helm-xref-test-return-log nil)
  (xref-global-history (cons nil nil)))

(defun neomacs-helm-xref-test-cleanup (root &rest extra-buffers)
  "Kill test buffers, remove ROOT, and reset shared state."
  (dolist (buffer (append extra-buffers (buffer-list)))
    (when (and (buffer-live-p buffer)
               (or (memq buffer extra-buffers)
                   (and root
                        (buffer-file-name buffer)
                        (string-prefix-p
                         root (expand-file-name (buffer-file-name buffer))))))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (when (and root (file-exists-p root))
    (delete-directory root t))
  (neomacs-helm-xref-test-reset))
"###;

fn helm_xref_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_XREF_MELPA_PIN, "helm-xref.el")
        .expect("prepare pinned helm-xref source and Helm dependencies below ./tmp")
        .with_prelude(HELM_XREF_TEST_PRELUDE)
        .with_timeout(HELM_XREF_TEST_TIMEOUT)
}

fn single_definition_jumps_without_helm_then_returns_and_goes_forward() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-helm-xref-test-root "single-definition"))
       (origin (expand-file-name "app/deploy.el" root))
       (target (expand-file-name "lib/release.el" root)))
  (unwind-protect
      (save-current-buffer
        (save-window-excursion
          (neomacs-helm-xref-test-reset)
        (neomacs-helm-xref-test-write
         origin "(release-publish \"REL-42\")\n")
        (neomacs-helm-xref-test-write
         target
         ";;; release.el --- Release operations\n(defun release-publish (release)\n  (format \"published %s\" release))\n")
        (neomacs-helm-xref-test-visit origin "release-publish")
        (setq neomacs-helm-xref-test-current-root root
              neomacs-helm-xref-test-definitions
              (list
               (xref-make
                "Publish a release"
                (xref-make-file-location target 2 7)))
              neomacs-helm-xref-test-active-xrefs
              neomacs-helm-xref-test-definitions)
        (let ((xref-after-jump-hook
               '(neomacs-helm-xref-test-after-jump))
              (xref-after-return-hook
               '(neomacs-helm-xref-test-after-return)))
          (cl-letf (((symbol-function 'helm)
                     #'neomacs-helm-xref-test-helm))
            (execute-kbd-macro (kbd "M-."))
            (let ((destination
                   (list
                    :location (neomacs-helm-xref-test-location root)
                    :history (neomacs-helm-xref-test-history root))))
              (execute-kbd-macro (kbd "M-,"))
              (let ((returned
                     (list
                      :location (neomacs-helm-xref-test-location root)
                      :history (neomacs-helm-xref-test-history root))))
                (xref-go-forward)
                (list
                 :fetches neomacs-helm-xref-test-definition-fetches
                 :helm-calls (nreverse neomacs-helm-xref-test-helm-calls)
                 :destination destination
                 :returned returned
                 :forward
                 (list
                  :location (neomacs-helm-xref-test-location root)
                  :history (neomacs-helm-xref-test-history root))
                 :jump-hooks (nreverse neomacs-helm-xref-test-jump-log)
                 :return-hooks
                 (nreverse neomacs-helm-xref-test-return-log))))))))
    (neomacs-helm-xref-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:fetches 1 :helm-calls nil :destination (:location (:buffer "release.el" :file "$ROOT/lib/release.el" :line 2 :column 7 :text "(defun release-publish (release)" :symbol "release-publish") :history (:backward ((:buffer "deploy.el" :file "$ROOT/app/deploy.el" :position 2 :line 1 :column 1)) :forward nil)) :returned (:location (:buffer "deploy.el" :file "$ROOT/app/deploy.el" :line 1 :column 1 :text "(release-publish \"REL-42\")" :symbol "release-publish") :history (:backward nil :forward ((:buffer "release.el" :file "$ROOT/lib/release.el" :position 46 :line 2 :column 7)))) :forward (:location (:buffer "release.el" :file "$ROOT/lib/release.el" :line 2 :column 7 :text "(defun release-publish (release)" :symbol "release-publish") :history (:backward ((:buffer "deploy.el" :file "$ROOT/app/deploy.el" :position 2 :line 1 :column 1)) :forward nil)) :jump-hooks ((:summary "Publish a release" :location (:buffer "release.el" :file "$ROOT/lib/release.el" :line 2 :column 7 :text "(defun release-publish (release)" :symbol "release-publish"))) :return-hooks ((:buffer "deploy.el" :file "$ROOT/app/deploy.el" :line 1 :column 1 :text "(release-publish \"REL-42\")" :symbol "release-publish") (:buffer "release.el" :file "$ROOT/lib/release.el" :line 2 :column 7 :text "(defun release-publish (release)" :symbol "release-publish")))"#
    ]];
    ParityBatchCase::value(
        "single_definition_jumps_without_helm_then_returns_and_goes_forward",
        elisp_form,
        expected,
    )
}

fn multiple_definitions_fetch_once_and_select_the_correct_duplicate_basename() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-helm-xref-test-root "multiple-definitions"))
       (origin (expand-file-name "app/caller.el" root))
       (api (expand-file-name "services/api/release.el" root))
       (worker (expand-file-name "services/worker/release.el" root))
       (notes (expand-file-name "docs/release notes.el" root)))
  (unwind-protect
      (save-current-buffer
        (save-window-excursion
          (neomacs-helm-xref-test-reset)
        (neomacs-helm-xref-test-write origin "(release-publish current)\n")
        (neomacs-helm-xref-test-write
         api ";;; API release\n(defun release-publish-api ()\n  'api)\n")
        (neomacs-helm-xref-test-write
         worker ";;; Worker release\n;; canary\n(defun release-publish-worker ()\n  'worker)\n")
        (neomacs-helm-xref-test-write
         notes ";;; Release notes\n(defun release-publish-notes ()\n  'notes)\n")
        (neomacs-helm-xref-test-visit origin "release-publish")
        (setq neomacs-helm-xref-test-current-root root
              neomacs-helm-xref-test-definitions
              (list
               (xref-make
                "Publish from API"
                (xref-make-file-location api 2 7))
               (xref-make
                "Publish canary β from worker"
                (xref-make-file-location worker 3 7))
               (xref-make
                "Publish release notes"
                (xref-make-file-location notes 2 7)))
              neomacs-helm-xref-test-active-xrefs
              neomacs-helm-xref-test-definitions
              neomacs-helm-xref-test-selection-index 1
              neomacs-helm-xref-test-action-kind 0)
        (let ((xref-after-jump-hook
               '(neomacs-helm-xref-test-after-jump))
              (xref-after-return-hook
               '(neomacs-helm-xref-test-after-return))
              (helm-xref-candidate-formatting-function
               'helm-xref-format-candidate-short)
              (helm-xref-input "canary"))
          (let ((before-open
                 (mapcar (lambda (file) (and (get-file-buffer file) t))
                         (list api worker notes))))
            (cl-letf (((symbol-function 'helm)
                       #'neomacs-helm-xref-test-helm))
              (xref-find-definitions "release-publish")
              (let ((destination
                     (list
                      :location (neomacs-helm-xref-test-location root)
                      :history (neomacs-helm-xref-test-history root)))
                    (after-open
                     (mapcar (lambda (file) (and (get-file-buffer file) t))
                             (list api worker notes))))
                (execute-kbd-macro (kbd "M-,"))
                (let ((returned
                       (list
                        :location (neomacs-helm-xref-test-location root)
                        :history (neomacs-helm-xref-test-history root))))
                  (xref-go-forward)
                  (list
                   :fetches neomacs-helm-xref-test-definition-fetches
                   :before-open before-open
                   :after-open after-open
                   :helm-calls
                   (nreverse neomacs-helm-xref-test-helm-calls)
                   :destination destination
                   :returned returned
                   :forward
                   (list
                    :location (neomacs-helm-xref-test-location root)
                    :history (neomacs-helm-xref-test-history root))
                   :jump-hooks (nreverse neomacs-helm-xref-test-jump-log)
                   :return-hooks
                   (nreverse neomacs-helm-xref-test-return-log)))))))))
    (neomacs-helm-xref-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:fetches 1 :before-open (nil nil nil) :after-open (nil t nil) :helm-calls ((:buffer "*helm-xref*" :truncate-lines t :input-present nil :input nil :source (:name "Helm Xref" :group helm :candidate-number-limit 9999 :actions ("Switch to buffer" "Other window") :candidates ((:display "release.el:2:Publish from API" :faces ((:text "release.el" :face helm-xref-file-name) (:text "2" :face helm-xref-line-number)) :summary "Publish from API" :group "$ROOT/services/api/release.el" :line 2 :identity-index 0) (:display "release.el:3:Publish canary β from worker" :faces ((:text "release.el" :face helm-xref-file-name) (:text "3" :face helm-xref-line-number)) :summary "Publish canary β from worker" :group "$ROOT/services/worker/release.el" :line 3 :identity-index 1) (:display "release notes.el:2:Publish release notes" :faces ((:text "release notes.el" :face helm-xref-file-name) (:text "2" :face helm-xref-line-number)) :summary "Publish release notes" :group "$ROOT/docs/release notes.el" :line 2 :identity-index 2))) :selection (:index 1 :action "Switch to buffer" :result (:buffer "release.el")))) :destination (:location (:buffer "release.el" :file "$ROOT/services/worker/release.el" :line 3 :column 7 :text "(defun release-publish-worker ()" :symbol "release-publish-worker") :history (:backward ((:buffer "caller.el" :file "$ROOT/app/caller.el" :position 2 :line 1 :column 1)) :forward nil)) :returned (:location (:buffer "caller.el" :file "$ROOT/app/caller.el" :line 1 :column 1 :text "(release-publish current)" :symbol "release-publish") :history (:backward nil :forward ((:buffer "release.el" :file "$ROOT/services/worker/release.el" :position 37 :line 3 :column 7)))) :forward (:location (:buffer "release.el" :file "$ROOT/services/worker/release.el" :line 3 :column 7 :text "(defun release-publish-worker ()" :symbol "release-publish-worker") :history (:backward ((:buffer "caller.el" :file "$ROOT/app/caller.el" :position 2 :line 1 :column 1)) :forward nil)) :jump-hooks nil :return-hooks ((:buffer "caller.el" :file "$ROOT/app/caller.el" :line 1 :column 1 :text "(release-publish current)" :symbol "release-publish") (:buffer "release.el" :file "$ROOT/services/worker/release.el" :line 3 :column 7 :text "(defun release-publish-worker ()" :symbol "release-publish-worker")))"#
    ]];
    ParityBatchCase::value(
        "multiple_definitions_fetch_once_and_select_the_correct_duplicate_basename",
        elisp_form,
        expected,
    )
}

fn reference_workflow_applies_every_formatter_lazily_and_discards_stale_results() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((root (neomacs-helm-xref-test-root "reference-formatting"))
       (origin (expand-file-name "app/search.el" root))
       (target (expand-file-name "services/release notes.el" root))
       (scratch (generate-new-buffer "*helm-xref scratch*")))
  (unwind-protect
      (save-current-buffer
        (save-window-excursion
          (neomacs-helm-xref-test-reset)
        (neomacs-helm-xref-test-write origin "release-publish\n")
        (neomacs-helm-xref-test-write
         target ";;; Notes\nrelease-publish ships β\n")
        (with-current-buffer scratch
          (insert "scratch reference to release-publish\n"))
        (neomacs-helm-xref-test-visit origin "release-publish")
        (setq neomacs-helm-xref-test-current-root root
              neomacs-helm-xref-test-references
              (list
               (xref-make
                "release-publish ships β"
                (xref-make-file-location target 2 0))
               (xref-make
                "scratch reference"
                (xref-make-buffer-location scratch 1)))
              neomacs-helm-xref-test-active-xrefs
              neomacs-helm-xref-test-references)
        (let ((formatters
               '(helm-xref-format-candidate-short
                 helm-xref-format-candidate-full-path
                 helm-xref-format-candidate-long
                 neomacs-helm-xref-test-custom-formatter))
              runs)
          (dolist (formatter formatters)
            (setq helm-xref-alist '(("stale candidate" . stale))
                  neomacs-helm-xref-test-reference-fetches 0
                  neomacs-helm-xref-test-helm-calls nil
                  neomacs-helm-xref-test-selection-index nil
                  neomacs-helm-xref-test-action-kind nil)
            (xref-global-history (cons nil nil))
            (let ((helm-xref-candidate-formatting-function formatter)
                  (helm-xref-input "publish β"))
              (cl-letf (((symbol-function 'helm)
                         #'neomacs-helm-xref-test-helm))
                (xref-find-references "release-publish")))
            (push
             (list
              :formatter formatter
              :fetches neomacs-helm-xref-test-reference-fetches
              :target-open (and (get-file-buffer target) t)
              :history (neomacs-helm-xref-test-history root)
              :helm-calls (nreverse neomacs-helm-xref-test-helm-calls))
             runs))
          (nreverse runs))))
    (neomacs-helm-xref-test-cleanup root scratch)))
"###;
    let expected = expect![[
        r#"OK ((:formatter helm-xref-format-candidate-short :fetches 1 :target-open nil :history (:backward ((:buffer "search.el" :file "$ROOT/app/search.el" :position 1 :line 1 :column 0)) :forward nil) :helm-calls ((:buffer "*helm-xref*" :truncate-lines t :input-present nil :input nil :source (:name "Helm Xref" :group helm :candidate-number-limit 9999 :actions ("Switch to buffer" "Other window") :candidates ((:display "release notes.el:2:release-publish ships β" :faces ((:text "release notes.el" :face helm-xref-file-name) (:text "2" :face helm-xref-line-number)) :summary "release-publish ships β" :group "$ROOT/services/release notes.el" :line 2 :identity-index 0) (:display "(buffer *helm-xref scratch*):scratch reference" :faces ((:text "(buffer *helm-xref scratch*)" :face helm-xref-file-name)) :summary "scratch reference" :group "(buffer *helm-xref scratch*)" :line nil :identity-index 1))) :selection nil))) (:formatter helm-xref-format-candidate-full-path :fetches 1 :target-open nil :history (:backward ((:buffer "search.el" :file "$ROOT/app/search.el" :position 1 :line 1 :column 0)) :forward nil) :helm-calls ((:buffer "*helm-xref*" :truncate-lines t :input-present nil :input nil :source (:name "Helm Xref" :group helm :candidate-number-limit 9999 :actions ("Switch to buffer" "Other window") :candidates ((:display "$ROOT/services/release notes.el:2:release-publish ships β" :faces ((:text "$ROOT/services/release notes.el" :face helm-xref-file-name) (:text "2" :face helm-xref-line-number)) :summary "release-publish ships β" :group "$ROOT/services/release notes.el" :line 2 :identity-index 0) (:display "(buffer *helm-xref scratch*):scratch reference" :faces ((:text "(buffer *helm-xref scratch*)" :face helm-xref-file-name)) :summary "scratch reference" :group "(buffer *helm-xref scratch*)" :line nil :identity-index 1))) :selection nil))) (:formatter helm-xref-format-candidate-long :fetches 1 :target-open nil :history (:backward ((:buffer "search.el" :file "$ROOT/app/search.el" :position 1 :line 1 :column 0)) :forward nil) :helm-calls ((:buffer "*helm-xref*" :truncate-lines t :input-present nil :input nil :source (:name "Helm Xref" :group helm :candidate-number-limit 9999 :actions ("Switch to buffer" "Other window") :candidates ((:display "$ROOT/services/release notes.el\n:2:release-publish ships β" :faces ((:text "$ROOT/services/release notes.el" :face helm-xref-file-name) (:text "2" :face helm-xref-line-number)) :summary "release-publish ships β" :group "$ROOT/services/release notes.el" :line 2 :identity-index 0) (:display "(buffer *helm-xref scratch*):scratch reference" :faces ((:text "(buffer *helm-xref scratch*)" :face helm-xref-file-name)) :summary "scratch reference" :group "(buffer *helm-xref scratch*)" :line nil :identity-index 1))) :selection nil))) (:formatter neomacs-helm-xref-test-custom-formatter :fetches 1 :target-open nil :history (:backward ((:buffer "search.el" :file "$ROOT/app/search.el" :position 1 :line 1 :column 0)) :forward nil) :helm-calls ((:buffer "*helm-xref*" :truncate-lines t :input-present nil :input nil :source (:name "Helm Xref" :group helm :candidate-number-limit 9999 :actions ("Switch to buffer" "Other window") :candidates ((:display "release notes.el|2|RELEASE-PUBLISH SHIPS Β" :faces nil :summary "release-publish ships β" :group "$ROOT/services/release notes.el" :line 2 :identity-index 0) (:display "(buffer *helm-xref scratch*)|buffer|SCRATCH REFERENCE" :faces nil :summary "scratch reference" :group "(buffer *helm-xref scratch*)" :line nil :identity-index 1))) :selection nil))))"#
    ]];
    ParityBatchCase::value(
        "reference_workflow_applies_every_formatter_lazily_and_discards_stale_results",
        elisp_form,
        expected,
    )
}

fn preview_highlights_the_target_and_other_window_action_preserves_exact_layout() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((root (neomacs-helm-xref-test-root "preview-and-window"))
       (origin (expand-file-name "app/dashboard.el" root))
       (target (expand-file-name "lib/release.el" root)))
  (unwind-protect
      (save-current-buffer
        (save-window-excursion
          (neomacs-helm-xref-test-reset)
        (delete-other-windows)
        (neomacs-helm-xref-test-write origin "release-status\n")
        (neomacs-helm-xref-test-write
         target
         ";;; Release status\n(defun release-status ()\n  \"green\")\n")
        (neomacs-helm-xref-test-visit origin "release-status")
        (setq neomacs-helm-xref-test-current-root root
              neomacs-helm-xref-test-references
              (list
               (xref-make
                "Current release status"
                (xref-make-file-location target 2 7)))
              neomacs-helm-xref-test-active-xrefs
              neomacs-helm-xref-test-references
              neomacs-helm-xref-test-selection-index 0
              neomacs-helm-xref-test-action-kind 'persistent)
        (cl-letf (((symbol-function 'helm)
                   #'neomacs-helm-xref-test-helm))
          (xref-find-references "release-status")
          (let ((preview
                 (list
                  :fetches neomacs-helm-xref-test-reference-fetches
                  :location (neomacs-helm-xref-test-location root)
                  :overlay (neomacs-helm-xref-test-overlay)
                  :windows (neomacs-helm-xref-test-windows)
                  :history (neomacs-helm-xref-test-history root)
                  :helm-calls
                  (nreverse neomacs-helm-xref-test-helm-calls))))
            (execute-kbd-macro (kbd "M-,"))
            (let ((preview-return
                   (list
                    :location (neomacs-helm-xref-test-location root)
                    :history (neomacs-helm-xref-test-history root))))
              (when (overlayp helm-match-line-overlay)
                (delete-overlay helm-match-line-overlay))
              (setq helm-match-line-overlay nil
                    neomacs-helm-xref-test-reference-fetches 0
                    neomacs-helm-xref-test-helm-calls nil
                    neomacs-helm-xref-test-action-kind 1)
              (xref-global-history (cons nil nil))
              (delete-other-windows)
              (neomacs-helm-xref-test-visit origin "release-status")
              (xref-find-references "release-status")
              (let ((other-window
                     (list
                      :fetches neomacs-helm-xref-test-reference-fetches
                      :location (neomacs-helm-xref-test-location root)
                      :windows (neomacs-helm-xref-test-windows)
                      :history (neomacs-helm-xref-test-history root)
                      :helm-calls
                      (nreverse neomacs-helm-xref-test-helm-calls))))
                (execute-kbd-macro (kbd "M-,"))
                (list
                 :preview preview
                 :preview-return preview-return
                 :other-window other-window
                 :other-window-return
                 (list
                  :location (neomacs-helm-xref-test-location root)
                  :windows (neomacs-helm-xref-test-windows)
                  :history (neomacs-helm-xref-test-history root)))))))))
    (neomacs-helm-xref-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:preview (:fetches 1 :location (:buffer "release.el" :file "$ROOT/lib/release.el" :line 2 :column 7 :text "(defun release-status ()" :symbol "release-status") :overlay (:range (20 45) :text "(defun release-status ()\n" :face helm-selection-line) :windows ((:selected t :buffer "release.el" :point 27)) :history (:backward ((:buffer "dashboard.el" :file "$ROOT/app/dashboard.el" :position 1 :line 1 :column 0)) :forward nil) :helm-calls ((:buffer "*helm-xref*" :truncate-lines t :input-present nil :input nil :source (:name "Helm Xref" :group helm :candidate-number-limit 9999 :actions ("Switch to buffer" "Other window") :candidates ((:display "release.el:2:Current release status" :faces ((:text "release.el" :face helm-xref-file-name) (:text "2" :face helm-xref-line-number)) :summary "Current release status" :group "$ROOT/lib/release.el" :line 2 :identity-index 0))) :selection (:index 0 :action :persistent :result nil)))) :preview-return (:location (:buffer "dashboard.el" :file "$ROOT/app/dashboard.el" :line 1 :column 0 :text "release-status" :symbol "release-status") :history (:backward nil :forward ((:buffer "release.el" :file "$ROOT/lib/release.el" :position 27 :line 2 :column 7)))) :other-window (:fetches 1 :location (:buffer "release.el" :file "$ROOT/lib/release.el" :line 2 :column 7 :text "(defun release-status ()" :symbol "release-status") :windows ((:selected t :buffer "release.el" :point 27) (:selected nil :buffer "release.el" :point 27)) :history (:backward ((:buffer "dashboard.el" :file "$ROOT/app/dashboard.el" :position 1 :line 1 :column 0)) :forward nil) :helm-calls ((:buffer "*helm-xref*" :truncate-lines t :input-present nil :input nil :source (:name "Helm Xref" :group helm :candidate-number-limit 9999 :actions ("Switch to buffer" "Other window") :candidates ((:display "release.el:2:Current release status" :faces ((:text "release.el" :face helm-xref-file-name) (:text "2" :face helm-xref-line-number)) :summary "Current release status" :group "$ROOT/lib/release.el" :line 2 :identity-index 0))) :selection (:index 0 :action "Other window" :result (:buffer "release.el"))))) :other-window-return (:location (:buffer "dashboard.el" :file "$ROOT/app/dashboard.el" :line 1 :column 0 :text "release-status" :symbol "release-status") :windows ((:selected t :buffer "dashboard.el" :point 1) (:selected nil :buffer "release.el" :point 27)) :history (:backward nil :forward ((:buffer "release.el" :file "$ROOT/lib/release.el" :position 27 :line 2 :column 7)))))"#
    ]];
    ParityBatchCase::value(
        "preview_highlights_the_target_and_other_window_action_preserves_exact_layout",
        elisp_form,
        expected,
    )
}

fn missing_definition_resignals_without_opening_helm_or_mutating_history() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-helm-xref-test-root "missing-definition"))
       (origin (expand-file-name "app/missing.el" root)))
  (unwind-protect
      (save-window-excursion
        (neomacs-helm-xref-test-reset)
        (delete-other-windows)
        (neomacs-helm-xref-test-write origin "release-missing\n")
        (neomacs-helm-xref-test-visit origin "release-missing")
        (setq neomacs-helm-xref-test-current-root root
              neomacs-helm-xref-test-definitions nil)
        (let ((before (neomacs-helm-xref-test-location root))
              outcome)
          (cl-letf (((symbol-function 'helm)
                     #'neomacs-helm-xref-test-helm))
            (setq outcome
                  (condition-case err
                      (progn
                        (xref-find-definitions "release-missing")
                        :unexpected-success)
                    (error
                     (list :condition (car err)
                           :data (cdr err)
                           :message (error-message-string err))))))
          (list
           :outcome outcome
           :fetches neomacs-helm-xref-test-definition-fetches
           :helm-calls (nreverse neomacs-helm-xref-test-helm-calls)
           :before before
           :after (neomacs-helm-xref-test-location root)
           :windows (neomacs-helm-xref-test-windows)
           :history (neomacs-helm-xref-test-history root)
           :helm-buffer (and (get-buffer "*helm-xref*") t))))
    (neomacs-helm-xref-test-cleanup root)))
"###;
    let expected = expect![[
        r#"OK (:outcome (:condition user-error :data ("No definitions found for: release-missing") :message "No definitions found for: release-missing") :fetches 1 :helm-calls nil :before (:buffer "missing.el" :file "$ROOT/app/missing.el" :line 1 :column 0 :text "release-missing" :symbol "release-missing") :after (:buffer "missing.el" :file "$ROOT/app/missing.el" :line 1 :column 0 :text "release-missing" :symbol "release-missing") :windows ((:selected t :buffer "missing.el" :point 1)) :history (:backward nil :forward nil) :helm-buffer nil)"#
    ]];
    ParityBatchCase::value(
        "missing_definition_resignals_without_opening_helm_or_mutating_history",
        elisp_form,
        expected,
    )
}

#[test]
fn helm_xref_package_batch() {
    let cases = [
        single_definition_jumps_without_helm_then_returns_and_goes_forward(),
        multiple_definitions_fetch_once_and_select_the_correct_duplicate_basename(),
        reference_workflow_applies_every_formatter_lazily_and_discards_stale_results(),
        preview_highlights_the_target_and_other_window_action_preserves_exact_layout(),
        missing_definition_resignals_without_opening_helm_or_mutating_history(),
    ];
    assert_oracle_batch_cases(
        helm_xref_oracle(),
        "helm-xref-package-batch",
        "helm-xref parity",
        &cases,
    );
}
