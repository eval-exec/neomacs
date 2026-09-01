use std::time::Duration;

use expect_test::{Expect, expect};

use crate::{CachedMelpaOracle, HELM_ORG_RIFLE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'org)
(require 'helm-org-rifle)

;; Prime Org's one-time menu/keymap setup before any shared-case baseline.
(with-temp-buffer (org-mode))
(set-window-configuration (current-window-configuration))

(defconst hor387-test-source-sha256
  "6b0041645674b533368e202fbe9223b20504ca2e86577a75a2975558fcfb2e5b")

(defvar hor387-test-owned-buffers nil)
(defvar hor387-test-owned-roots nil)
(defvar hor387-test-helm-plans nil)
(defvar hor387-test-helm-calls nil)

(defun hor387-test-scrub-root (value root)
  (cond ((bufferp value) (list :buffer (buffer-name value)))
        ((stringp value)
         (replace-regexp-in-string (regexp-quote root) "" value t t))
        ((consp value)
         (cons (hor387-test-scrub-root (car value) root)
               (hor387-test-scrub-root (cdr value) root)))
        (t value)))

(defun hor387-test-condition (condition &optional root)
  (let ((state (list :type (car condition) :data (copy-tree (cdr condition))
                     :message (error-message-string condition))))
    (if root (hor387-test-scrub-root state root) state)))

(defun hor387-test-source-file ()
  (let* ((loaded (symbol-file 'helm-org-rifle-current-buffer 'defun))
         (source (and loaded
                      (if (string-suffix-p ".elc" loaded)
                          (substring loaded 0 -1)
                        loaded))))
    (unless (and source (file-regular-p source))
      (error "Missing installed helm-org-rifle source: %S" source))
    source))

(defun hor387-test-source-hash ()
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally (hor387-test-source-file))
    (secure-hash 'sha256 (current-buffer))))

(defun hor387-test-own-buffer (name text &optional file)
  (when (get-buffer name)
    (error "Owned Helm Org Rifle buffer already exists: %s" name))
  (let ((buffer (generate-new-buffer name)))
    (push buffer hor387-test-owned-buffers)
    (with-current-buffer buffer
      (when file (setq buffer-file-name file))
      (insert text)
      (org-mode)
      (goto-char (point-min))
      (set-buffer-modified-p nil))
    buffer))

(defun hor387-test-write-file (path text)
  (make-directory (file-name-directory path) t)
  (with-temp-file path (insert text))
  path)

(defun hor387-test-candidate-state (candidate source root)
  (let* ((display (car candidate))
         ;; Untransformed package candidates are (DISPLAY BUFFER . POSITION).
         ;; Its timestamp transformer returns (DISPLAY POSITION), relying on
         ;; the Helm source to retain the buffer.
         (transformed (and (consp (cdr candidate)) (null (cddr candidate))))
         (real (and (not transformed) (cdr candidate)))
         (buffer (if transformed (helm-attr 'buffer source) (car real)))
         (position (if transformed (cadr candidate) (cdr real))))
    (list :display (substring-no-properties display)
          :buffer (buffer-name buffer)
          :file (and (buffer-file-name buffer)
                     (file-relative-name (buffer-file-name buffer) root))
          :position position
          :heading (with-current-buffer buffer
                     (save-excursion
                       (goto-char position)
                       (substring-no-properties
                        (org-get-heading t t t t)))))))

(defun hor387-test-source-candidates (source root)
  (let* ((helm-current-source source)
         (generator (helm-attr 'candidates source))
         (candidates (if (functionp generator)
                         (funcall generator)
                       generator)))
    ;; This is Helm's real candidate-transformer boundary.  Calling the
    ;; package transformer directly would skip Helm's source semantics.
    (setq candidates (helm-transform-candidates candidates source))
    (mapcar (lambda (candidate)
              (cons candidate (hor387-test-candidate-state candidate source root)))
            candidates)))

(defun hor387-test-run-helm-plan (root thunk plans)
  (let ((hor387-test-helm-plans (copy-tree plans))
        (hor387-test-helm-calls nil))
    (cl-letf
        (((symbol-function 'helm)
          (lambda (&rest arguments)
            (unless hor387-test-helm-plans
              (error "Unexpected Helm invocation: %S" arguments))
            (let* ((plan (pop hor387-test-helm-plans))
                   (query (plist-get plan :query))
                   (action-name (plist-get plan :action))
                   (sources-value (plist-get arguments :sources))
                   (sources (if (and (listp sources-value)
                                     (assq 'name sources-value))
                                (list sources-value)
                              sources-value))
                   (helm-pattern query)
                   (helm-sources sources)
              source-states selected)
              (when (and action-name (not (get-buffer helm-buffer)))
                (push (get-buffer-create helm-buffer)
                      hor387-test-owned-buffers))
              (dolist (source sources)
                (let ((pairs (hor387-test-source-candidates source root)))
                  (push (list :name (helm-attr 'name source)
                              :new-buffer (and (helm-attr 'new-buffer source) t)
                              :transformer (helm-attr 'candidate-transformer source)
                              :candidates (mapcar #'cdr pairs))
                        source-states)
                  (when (and action-name (null selected) pairs)
                    (let* ((actions-value (helm-attr 'action source))
                           (actions (if (symbolp actions-value)
                                        (symbol-value actions-value)
                                      actions-value))
                           (action (cdr (assoc action-name actions))))
                      (unless (functionp action)
                        (error "Missing planned Helm action: %S" action-name))
                      (setq selected (copy-tree (cdar pairs)))
                      (let ((helm-current-source source))
                        (funcall action (cdar (mapcar #'car pairs))))))))
              (push (list :query query
                          :buffer (plist-get arguments :buffer)
                          :sources (nreverse source-states)
                          :action action-name
                          :selected selected
                          :selected-window-buffer
                          (buffer-name (window-buffer (selected-window)))
                          :selected-window-point
                          (window-point (selected-window)))
                    hor387-test-helm-calls)
              (when (plist-get plan :abort)
                (let ((helm-exit-status 1))
                  (run-hooks 'helm-cleanup-hook)))
              nil))))
      (funcall thunk))
    (when hor387-test-helm-plans
      (error "Missing Helm invocations: %S" hor387-test-helm-plans))
    (nreverse hor387-test-helm-calls)))

(defun hor387-test-occur-state (buffer root)
  (with-current-buffer buffer
    (let ((position (point-min)) nodes)
      (while (< position (point-max))
        (let ((node (get-text-property position :node-beg))
              (source (get-text-property position :buffer))
              (next (next-single-property-change
                     position :node-beg nil (point-max))))
          (when node
            (push (list :source (buffer-name source)
                        :file (and (buffer-file-name source)
                                   (file-relative-name
                                    (buffer-file-name source) root))
                        :node-beg node
                        :text (buffer-substring-no-properties position next))
                  nodes))
          (setq position next)))
      (list :mode major-mode :read-only buffer-read-only
            :text (buffer-substring-no-properties (point-min) (point-max))
            :nodes (nreverse nodes)))))

(defun hor387-test-indirect-state (buffer source)
  (with-current-buffer buffer
    (list :name (buffer-name)
          :base-is-source (eq (buffer-base-buffer) source)
          :narrowed (buffer-narrowed-p)
          :point-min (point-min)
          :point-max (point-max)
          :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :heading (save-excursion
                     (org-back-to-heading t)
                     (substring-no-properties (org-get-heading t t t t))))))

(defun hor387-test-park-buffer (name suffix)
  (when-let* ((buffer (get-buffer name)))
    (with-current-buffer buffer
      (let ((old-name (buffer-name)))
        (rename-buffer (format " %s-%s" suffix (sxhash-eq buffer)) t)
        (cons buffer old-name)))))

(defun hor387-test-run (body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "helm-org-rifle/" sandbox))))
         (window-before (current-window-configuration))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (parked (delq nil
                       (list
                        (hor387-test-park-buffer
                         helm-org-rifle-occur-results-buffer-name "hor387-occur")
                        (hor387-test-park-buffer
                         helm-org-rifle-fontify-buffer-name "hor387-fontify")
                        (hor387-test-park-buffer helm-buffer "hor387-helm"))))
         (hor387-test-owned-buffers nil)
         (hor387-test-owned-roots nil)
         (helm-org-rifle-occur-last-input nil)
         (helm-org-rifle-transformer nil)
         (helm-org-rifle-sort-order nil)
         (helm-org-rifle-sort-order-persist nil)
         (org-mark-ring nil)
         result body-error cleanup-errors)
    (unless (and root (file-name-absolute-p root))
      (error "Missing absolute Helm Org Rifle sandbox root"))
    (unless (equal (hor387-test-source-hash) hor387-test-source-sha256)
      (error "Unexpected installed Helm Org Rifle source identity"))
    (when (file-exists-p root)
      (error "Helm Org Rifle sandbox root already exists: %s" root))
    (make-directory root t)
    (push root hor387-test-owned-roots)
    (condition-case condition
        (cl-letf (((symbol-function 'call-process)
                   (lambda (&rest arguments)
                     (error "Unexpected call-process: %S" arguments)))
                  ((symbol-function 'call-process-region)
                   (lambda (&rest arguments)
                     (error "Unexpected call-process-region: %S" arguments)))
                  ((symbol-function 'start-process)
                   (lambda (&rest arguments)
                     (error "Unexpected start-process: %S" arguments)))
                  ((symbol-function 'make-process)
                   (lambda (&rest arguments)
                     (error "Unexpected make-process: %S" arguments)))
                  ((symbol-function 'make-network-process)
                   (lambda (&rest arguments)
                     (error "Unexpected network process: %S" arguments)))
                  ((symbol-function 'url-retrieve)
                   (lambda (&rest arguments)
                     (error "Unexpected URL retrieval: %S" arguments))))
          (save-window-excursion
            (save-current-buffer
              (setq result (funcall body root)))))
      (t (setq body-error (hor387-test-condition condition))))
    (dolist (timer (seq-difference timer-list timers-before #'eq))
      (condition-case condition (cancel-timer timer)
        (t (push (hor387-test-condition condition) cleanup-errors))))
    (dolist (process (seq-difference (process-list) processes-before #'eq))
      (condition-case condition (delete-process process)
        (t (push (hor387-test-condition condition) cleanup-errors))))
    (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
      (condition-case condition
          (when (buffer-live-p buffer)
            (with-current-buffer buffer
              (let ((kill-buffer-hook nil)
                    (kill-buffer-query-functions nil))
                (kill-buffer buffer))))
        (t (push (hor387-test-condition condition) cleanup-errors))))
    (dolist (frame (seq-difference (frame-list) frames-before #'eq))
      (condition-case condition (delete-frame frame t)
        (t (push (hor387-test-condition condition) cleanup-errors))))
    (dolist (owned-root hor387-test-owned-roots)
      (condition-case condition
          (when (file-exists-p owned-root) (delete-directory owned-root t))
        (t (push (hor387-test-condition condition) cleanup-errors))))
    (condition-case condition (set-window-configuration window-before)
      (t (push (hor387-test-condition condition) cleanup-errors)))
    (dolist (entry parked)
      (condition-case condition
          (if (buffer-live-p (car entry))
              (with-current-buffer (car entry)
                (rename-buffer (cdr entry) t))
            (error "Parked buffer died: %S" (cdr entry)))
        (t (push (hor387-test-condition condition) cleanup-errors))))
    (when (buffer-live-p buffer-before) (set-buffer buffer-before))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter #'buffer-live-p
                                     (seq-difference (buffer-list) buffers-before #'eq)))
                 :new-processes (length (seq-difference
                                         (process-list) processes-before #'eq))
                 :new-timers (length (seq-difference timer-list timers-before #'eq))
                 :new-frames (length (seq-difference (frame-list) frames-before #'eq))
                 :roots-exist (seq-some #'file-exists-p hor387-test-owned-roots)
                 :window-restored
                 (compare-window-configurations
                  (current-window-configuration) window-before)
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Helm Org Rifle workflow failed: %S" (list result cleanup))
        (list :source-sha (hor387-test-source-hash)
              :result result :cleanup cleanup)))))
"####;

fn package_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_ORG_RIFLE_MELPA_PIN, "helm-org-rifle.el")
        .expect("prepare exact shallow Helm Org Rifle source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed helm-org-rifle parity test")
        .into()
}

fn workflow(name: &'static str, probe: &'static str, expected: Expect) -> ParityBatchCase {
    ParityBatchCase::value(name, probe, expected)
}

fn cases() -> Vec<ParityBatchCase> {
    vec![
        workflow(
            "public_current_buffer_search_matches_terms_tags_todo_and_exclusion",
            r####"
(hor387-test-run
 (lambda (root)
   (let* ((text "* TODO [#A] Release café :ship:\nLaunch café payload safely.\n* TODO Secret release café :ship:secret:\nExcluded only by secret.\n* TODO Release café without tag\nExcluded only by missing ship tag.\n* TODO Release without unicode :ship:\nExcluded because the Unicode token is absent.\n* DONE Release café :ship:\nExcluded only by TODO state.\n")
          (buffer (hor387-test-own-buffer "hor387-search.org" text)))
     (with-current-buffer buffer
       (let ((helm-org-rifle-fontify-headings nil)
             (helm-org-rifle-show-path nil)
             (helm-org-rifle-context-characters 12))
         (hor387-test-run-helm-plan
          root #'helm-org-rifle-current-buffer
          '((:query "TODO café :ship: !secret"))))))))
"####,
            expect![[
                r#"OK (:source-sha "6b0041645674b533368e202fbe9223b20504ca2e86577a75a2975558fcfb2e5b" :result ((:query "TODO café :ship: !secret" :buffer nil :sources ((:name "hor387-search.org" :new-buffer nil :transformer nil :candidates ((:display "* [#A] Release café :ship:\nLaunch café payload saf" :buffer "hor387-search.org" :file nil :position 1 :heading "Release café")))) :action nil :selected nil :selected-window-buffer "*scratch*" :selected-window-point 1)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
            ]],
        ),
        workflow(
            "public_helm_actions_show_real_and_indirect_entries",
            r####"
(hor387-test-run
 (lambda (root)
   (let* ((text "* Parent\nparent body\n** Acquire target\nneedle 界 body\n*** Child\nchild body\n")
          (buffer (hor387-test-own-buffer "hor387-actions.org" text)))
     (with-current-buffer buffer
       (let ((helm-org-rifle-fontify-headings nil)
             (helm-org-rifle-show-path nil)
             real indirect)
         (setq real
               (hor387-test-run-helm-plan
                root #'helm-org-rifle-current-buffer
                '((:query "needle" :action "Show entry in real buffer"))))
         (set-buffer buffer)
         (setq indirect
               (hor387-test-run-helm-plan
                root #'helm-org-rifle-current-buffer
                '((:query "needle" :action "Show entry in indirect buffer"))))
         (let ((indirect-buffers
                (seq-filter (lambda (candidate)
                              (eq (buffer-base-buffer candidate) buffer))
                            (buffer-list))))
           (list :real real :indirect indirect
                 :source-point (with-current-buffer buffer (point))
                 :indirect-states
                 (mapcar (lambda (candidate)
                           (hor387-test-indirect-state candidate buffer))
                         indirect-buffers))))))))
"####,
            expect![[
                r#"OK (:source-sha "6b0041645674b533368e202fbe9223b20504ca2e86577a75a2975558fcfb2e5b" :result (:real ((:query "needle" :buffer nil :sources ((:name "hor387-actions.org" :new-buffer nil :transformer nil :candidates ((:display "**  Acquire target \nneedle 界 body" :buffer "hor387-actions.org" :file nil :position 22 :heading "Acquire target")))) :action "Show entry in real buffer" :selected (:display "**  Acquire target \nneedle 界 body" :buffer "hor387-actions.org" :file nil :position 22 :heading "Acquire target") :selected-window-buffer "hor387-actions.org" :selected-window-point 22)) :indirect ((:query "needle" :buffer nil :sources ((:name "hor387-actions.org" :new-buffer nil :transformer nil :candidates ((:display "**  Acquire target \nneedle 界 body" :buffer "hor387-actions.org" :file nil :position 22 :heading "Acquire target")))) :action "Show entry in indirect buffer" :selected (:display "**  Acquire target \nneedle 界 body" :buffer "hor387-actions.org" :file nil :position 22 :heading "Acquire target") :selected-window-buffer "hor387-actions.org" :selected-window-point 22)) :source-point 22 :indirect-states ((:name "hor387-actions.org::Acquire target" :base-is-source t :narrowed t :point-min 22 :point-max 75 :text "** Acquire target\nneedle 界 body\n*** Child\nchild body\n" :point 22 :heading "Acquire target"))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
            ]],
        ),
        workflow(
            "public_path_search_supports_forward_and_reverse_outline_paths",
            r####"
(hor387-test-run
 (lambda (root)
   (let* ((text "* Projects\nparent body\n** Café Launch\nneedle payload\n*** Checklist\nneedle checklist\n")
          (buffer (hor387-test-own-buffer "hor387-path.org" text))
          forward reverse)
     (with-current-buffer buffer
       (let ((helm-org-rifle-fontify-headings t)
             (helm-org-rifle-show-path t)
             (helm-org-rifle-test-against-path t))
         (setq forward
               (hor387-test-run-helm-plan
                root #'helm-org-rifle-current-buffer
                '((:query "Projects needle"))))
         (let ((helm-org-rifle-reverse-paths t))
           (setq reverse
                 (hor387-test-run-helm-plan
                  root #'helm-org-rifle-current-buffer
                  '((:query "Projects needle")))))
         (list :forward forward :reverse reverse))))))
"####,
            expect![[
                r#"OK (:source-sha "6b0041645674b533368e202fbe9223b20504ca2e86577a75a2975558fcfb2e5b" :result (:forward ((:query "Projects needle" :buffer nil :sources ((:name "hor387-path.org" :new-buffer nil :transformer nil :candidates ((:display "Projects/Café Launch\nneedle payload" :buffer "hor387-path.org" :file nil :position 24 :heading "Café Launch") (:display "Projects/Café Launch/Checklist\nneedle checklist" :buffer "hor387-path.org" :file nil :position 54 :heading "Checklist")))) :action nil :selected nil :selected-window-buffer "*scratch*" :selected-window-point 1)) :reverse ((:query "Projects needle" :buffer nil :sources ((:name "hor387-path.org" :new-buffer nil :transformer nil :candidates ((:display "Café Launch\\Projects\nneedle payload" :buffer "hor387-path.org" :file nil :position 24 :heading "Café Launch") (:display "Checklist\\Café Launch\\Projects\nneedle checklist" :buffer "hor387-path.org" :file nil :position 54 :heading "Checklist")))) :action nil :selected nil :selected-window-buffer "*scratch*" :selected-window-point 1))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
            ]],
        ),
        workflow(
            "public_timestamp_sort_boundary_reports_failure_then_recovers_unsorted",
            r####"
(hor387-test-run
 (lambda (root)
   (let* ((text "* Older release\n<2025-01-01 Wed>\n* Newer release\n<2026-08-12 Wed>\n* Middle release\n<2026-02-03 Tue>\n")
          (buffer (hor387-test-own-buffer "hor387-sort.org" text))
          failure recovery)
     (with-current-buffer buffer
       (let ((helm-org-rifle-fontify-headings nil)
             (helm-org-rifle-show-path nil))
         (setq failure
               (condition-case condition
                   (progn
                     (hor387-test-run-helm-plan
                      root #'helm-org-rifle-current-buffer-sort-by-latest-timestamp
                      '((:query "release")))
                     nil)
                 (error
                  (list :type (car condition)
                        :predicate (nth 1 condition)
                        :operand-is-source-buffer (eq (nth 2 condition) buffer)))))
         (setq recovery
               (hor387-test-run-helm-plan
                root #'helm-org-rifle-current-buffer
                '((:query "release"))))
         (list :sort-failure failure
               :sort-mode-after helm-org-rifle-sort-order
               :recovery recovery))))))
"####,
            expect![[
                r#"OK (:source-sha "6b0041645674b533368e202fbe9223b20504ca2e86577a75a2975558fcfb2e5b" :result (:sort-failure (:type wrong-type-argument :predicate integer-or-marker-p :operand-is-source-buffer t) :sort-mode-after nil :recovery ((:query "release" :buffer nil :sources ((:name "hor387-sort.org" :new-buffer nil :transformer nil :candidates ((:display "*  Older release \n<2025-01-01 Wed>" :buffer "hor387-sort.org" :file nil :position 1 :heading "Older release") (:display "*  Newer release \n<2026-08-12 Wed>" :buffer "hor387-sort.org" :file nil :position 34 :heading "Newer release") (:display "*  Middle release \n<2026-02-03 Tue>" :buffer "hor387-sort.org" :file nil :position 67 :heading "Middle release")))) :action nil :selected nil :selected-window-buffer "*scratch*" :selected-window-point 1))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
            ]],
        ),
        workflow(
            "public_occur_search_navigates_or_recovers_after_render_failure",
            r####"
(hor387-test-run
 (lambda (root)
   (let* ((text "* Alpha\nplain body\n* Release café\nneedle café line\nmore context\n* Final\nneedle final\n")
          (buffer (hor387-test-own-buffer "hor387-occur-source.org" text))
          idle-callback idle-arguments timer-plan prompt failure)
     (condition-case condition
         (cl-letf (((symbol-function 'run-with-idle-timer)
                    (lambda (seconds repeat function &rest arguments)
                      (when (and (= seconds 0.25) (eq repeat 'repeat))
                        (when idle-callback
                          (error "Duplicate Helm Org Rifle occur timer"))
                        (setq timer-plan (list seconds repeat function)
                              idle-callback function
                              idle-arguments arguments))
                      (run-at-time 3600 nil #'ignore)))
                   ((symbol-function 'minibuffer-contents)
                    (lambda () "needle"))
                   ((symbol-function 'read-from-minibuffer)
                    (lambda (actual-prompt &rest _)
                      (setq prompt actual-prompt)
                      (run-hooks 'minibuffer-setup-hook)
                      (unless (functionp idle-callback)
                        (error "Occur command did not install its idle callback"))
                      (apply idle-callback idle-arguments)
                      "needle")))
           (with-current-buffer buffer
             (let ((helm-org-rifle-fontify-headings nil)
                   (helm-org-rifle-show-path nil))
               (helm-org-rifle-occur-current-buffer))))
       (error (setq failure (hor387-test-condition condition))))
     (if failure
         (list :boundary (list :prompt prompt
                               :delay (nth 0 timer-plan)
                               :repeat (nth 1 timer-plan)
                               :callback (and (functionp (nth 2 timer-plan)) t))
               :failure failure
               :recovery
               (with-current-buffer buffer
                 (let ((helm-org-rifle-fontify-headings nil)
                       (helm-org-rifle-show-path nil))
                   (hor387-test-run-helm-plan
                    root #'helm-org-rifle-current-buffer
                    '((:query "needle"))))))
       (let* ((results (get-buffer helm-org-rifle-occur-results-buffer-name))
            (initial (hor387-test-occur-state results root))
            goto-state deleted)
       (with-current-buffer results
         (goto-char (point-min))
         (search-forward "Release café")
         (helm-org-rifle-occur-goto-entry))
       (setq goto-state
             (list :buffer (buffer-name (window-buffer (selected-window)))
                   :point (window-point (selected-window))
                   :heading (with-current-buffer buffer
                              (save-excursion
                                (goto-char (window-point (selected-window)))
                                (substring-no-properties
                                 (org-get-heading t t t t))))))
       (pop-to-buffer results)
       (goto-char (point-min))
       (search-forward "Release café")
       (helm-org-rifle-occur-delete-entry)
       (setq deleted (hor387-test-occur-state results root))
       (list :boundary (list :prompt prompt
                             :delay (nth 0 timer-plan)
                             :repeat (nth 1 timer-plan)
                             :callback (and (functionp (nth 2 timer-plan)) t))
             :initial initial :goto goto-state :after-delete deleted))))))
"####,
            expect![[
                r#"OK (:source-sha "6b0041645674b533368e202fbe9223b20504ca2e86577a75a2975558fcfb2e5b" :result (:boundary (:prompt "pattern: " :delay 0.25 :repeat repeat :callback t) :initial (:mode org-mode :read-only t :text "\n hor387-occur-source.org\n\n* *  Release café \nneedle café line\nmore context\n\n* *  Final \nneedle final\n" :nodes ((:source "hor387-occur-source.org" :file nil :node-beg 20 :text "*  Release café \nneedle café line\nmore context\n") (:source "hor387-occur-source.org" :file nil :node-beg 65 :text "*  Final \nneedle final\n"))) :goto (:buffer "hor387-occur-source.org" :point 37 :heading "Release café") :after-delete (:mode org-mode :read-only t :text "\n hor387-occur-source.org\n\n* \n* *  Final \nneedle final\n" :nodes ((:source "hor387-occur-source.org" :file nil :node-beg 65 :text "*  Final \nneedle final\n")))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
            ]],
        ),
        workflow(
            "public_directory_search_reports_empty_then_recovers_with_recursion_toggle",
            r####"
(hor387-test-run
 (lambda (root)
   (let* ((directory (expand-file-name "notes/" root))
          (nested (expand-file-name "nested/" directory))
          empty-error recursive flat)
     (make-directory directory t)
     (setq empty-error
           (condition-case condition
               (progn (helm-org-rifle-directories directory) nil)
             (error (hor387-test-condition condition root))))
     (hor387-test-write-file
      (expand-file-name "one.org" directory)
      "* Root release\nneedle root\n")
     (hor387-test-write-file
      (expand-file-name "two.txt" directory)
      "* Ignored\nneedle ignored\n")
     (hor387-test-write-file
      (expand-file-name "three.org" nested)
      "* Nested release\nneedle nested\n")
     (setq recursive
           (hor387-test-run-helm-plan
            root (lambda () (helm-org-rifle-directories directory nil))
            '((:query "needle" :abort t))))
     (setq flat
           (hor387-test-run-helm-plan
            root (lambda () (helm-org-rifle-directories directory t))
            '((:query "needle" :abort t))))
     (list :empty empty-error :recursive recursive :flat flat
           :opened-org-buffers
           (mapcar #'buffer-name
                   (seq-filter
                    (lambda (buffer)
                      (with-current-buffer buffer
                        (and buffer-file-name
                             (string-prefix-p directory buffer-file-name))))
                    (buffer-list)))))))
"####,
            expect![[
                r#"OK (:source-sha "6b0041645674b533368e202fbe9223b20504ca2e86577a75a2975558fcfb2e5b" :result (:empty (:type error :data ("No org files found in directories: notes/") :message "No org files found in directories: notes/") :recursive ((:query "needle" :buffer nil :sources ((:name "one.org" :new-buffer t :transformer nil :candidates ((:display "Root release\nneedle root" :buffer "one.org" :file "notes/one.org" :position 1 :heading "Root release"))) (:name "three.org" :new-buffer t :transformer nil :candidates ((:display "Nested release\nneedle nested" :buffer "three.org" :file "notes/nested/three.org" :position 1 :heading "Nested release")))) :action nil :selected nil :selected-window-buffer "*scratch*" :selected-window-point 1)) :flat ((:query "needle" :buffer nil :sources ((:name "one.org" :new-buffer t :transformer nil :candidates ((:display "Root release\nneedle root" :buffer "one.org" :file "notes/one.org" :position 1 :heading "Root release")))) :action nil :selected nil :selected-window-buffer "*scratch*" :selected-window-point 1)) :opened-org-buffers nil) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :roots-exist nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
            ]],
        ),
    ]
}

#[test]
fn helm_org_rifle_package_batch() {
    assert_oracle_batch_cases(
        package_oracle(),
        &current_test_name(),
        "helm_org_rifle_parity",
        &cases(),
    );
}
