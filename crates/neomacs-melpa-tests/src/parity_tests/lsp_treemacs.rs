use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, LSP_TREEMACS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(setq max-lisp-eval-depth 10000)
(require 'cl-lib)
(require 'lsp-treemacs)

(defconst neomacs-lsp-treemacs-test-buffers
  '("*LSP Generic Test*" "*LSP Symbols List*" "*LSP Lookup*"
    "*Call Hierarchy*" "*LSP Error List*"))

(defun neomacs-lsp-treemacs-test-reset ()
  "Remove package windows, buffers, timers, and global hooks between cases."
  (when (timerp lsp-treemacs--symbols-timer)
    (cancel-timer lsp-treemacs--symbols-timer))
  (setq lsp-treemacs--symbols-timer nil
        lsp-treemacs--symbols-current-buffer nil
        lsp-treemacs--symbols-last-buffer nil)
  (when lsp-treemacs-sync-mode (lsp-treemacs-sync-mode -1))
  (remove-hook 'lsp-diagnostics-updated-hook
               #'lsp-treemacs-errors-list--refresh)
  (when-let ((main-window
              (seq-find
               (lambda (window)
                 (not (window-parameter window 'window-side)))
               (window-list))))
    (select-window main-window))
  (delete-other-windows)
  (switch-to-buffer (get-buffer-create "*scratch*"))
  (dolist (name neomacs-lsp-treemacs-test-buffers)
    (when (get-buffer name) (kill-buffer name))))

(defun neomacs-lsp-treemacs-test-clean-file-buffers (root)
  "Kill buffers visiting files below ROOT."
  (dolist (buffer (buffer-list))
    (with-current-buffer buffer
      (when (and buffer-file-name
                 (file-in-directory-p buffer-file-name root))
        (set-buffer-modified-p nil)
        (kill-buffer buffer)))))

(defun neomacs-lsp-treemacs-test-normalize (value root)
  "Normalize VALUE recursively, replacing the sandbox ROOT."
  (cond
   ((stringp value)
    (replace-regexp-in-string (regexp-quote root) "$ROOT" value t t))
   ((vectorp value)
    (mapcar (lambda (item)
              (neomacs-lsp-treemacs-test-normalize item root))
            value))
   ((consp value)
    (cons (neomacs-lsp-treemacs-test-normalize (car value) root)
          (neomacs-lsp-treemacs-test-normalize (cdr value) root)))
   (t value)))

(defun neomacs-lsp-treemacs-test-visible-nodes (root)
  "Capture ordered rendered node metadata, normalizing paths below ROOT."
  (save-excursion
    (goto-char (point-min))
    (let ((button (next-button (point-min) t))
          nodes)
      (while button
        (let ((label (treemacs--get-label-of button)))
          (push
           (list :label
               (and label
                    (neomacs-lsp-treemacs-test-normalize
                     (substring-no-properties label) root))
               :key
               (neomacs-lsp-treemacs-test-normalize
                (treemacs-button-get button :key) root)
               :state (treemacs-button-get button :state)
               :depth (treemacs-button-get button :depth))
           nodes))
        (setq button (next-button (treemacs-button-end button))))
      (nreverse nodes))))

(defun neomacs-lsp-treemacs-test-snapshot (buffer root)
  "Describe BUFFER's rendered tree and mode state relative to ROOT."
  (with-current-buffer buffer
    (list :text
          (neomacs-lsp-treemacs-test-normalize
           (buffer-substring-no-properties (point-min) (point-max)) root)
          :nodes (neomacs-lsp-treemacs-test-visible-nodes root)
          :generic-mode lsp-treemacs-generic-mode
          :major-mode major-mode
          :mode-line
          (and mode-line-format
               (substring-no-properties
                (if (stringp mode-line-format)
                    mode-line-format
                  (format-mode-line mode-line-format)))))))

(defun neomacs-lsp-treemacs-test-goto-label (label)
  "Move to the first visible Treemacs node whose exact label is LABEL."
  (goto-char (point-min))
  (let ((button (next-button (point-min) t))
        found)
    (while (and button (not found))
      (if (equal label (substring-no-properties (treemacs--get-label-of button)))
          (setq found button)
        (setq button (next-button (treemacs-button-end button)))))
    (unless found
      (error "No rendered node labeled %S among %S"
             label
             (mapcar (lambda (node) (plist-get node :label))
                     (neomacs-lsp-treemacs-test-visible-nodes
                      default-directory))))
    (goto-char (marker-position found))
    found))

(defun neomacs-lsp-treemacs-test-position (line character)
  "Construct an LSP position at zero-based LINE and CHARACTER."
  (lsp-make-position :line line :character character))

(defun neomacs-lsp-treemacs-test-range (line start end)
  "Construct an LSP range on LINE from START through END."
  (lsp-make-range
   :start (neomacs-lsp-treemacs-test-position line start)
   :end (neomacs-lsp-treemacs-test-position line end)))
"####;

fn renders_a_nested_release_dashboard_and_dispatches_its_action() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-lsp-treemacs-test-reset)
  (let ((root default-directory)
        chosen)
    (unwind-protect
        (let* ((tree
                `((:label "Release Ω" :key release :icon folder
                   :children
                   ((:label "Checks" :key checks :icon folder
                     :children
                     ((:label "Unit: 128 passed" :key unit :icon boolean
                       :ret-action ,(lambda () (interactive)
                                      (setq chosen 'unit)))
                      (:label "Deploy staging" :key deploy :icon event
                       :ret-action ,(lambda () (interactive)
                                      (setq chosen 'deploy)))))
                    (:label "Artifacts signed" :key signed :icon boolean
                     :ret-action ,(lambda () (interactive)
                                    (setq chosen 'signed)))))))
               (buffer (lsp-treemacs-render tree "Release Dashboard" t
                                            "*LSP Generic Test*")))
          (pop-to-buffer buffer)
          (let ((rendered
                 (neomacs-lsp-treemacs-test-snapshot buffer root)))
            (neomacs-lsp-treemacs-test-goto-label "Deploy staging")
            (lsp-treemacs-perform-ret-action)
            (list :rendered rendered
                  :chosen chosen
                  :binding (key-binding (kbd "RET")))))
      (neomacs-lsp-treemacs-test-reset))))
"####;
    let expected = expect![[
        r#"OK (:rendered (:text "Hidden node\n▾   Release Ω\n  ▾   Checks\n        Unit: 128 passed\n        Deploy staging\n      Artifacts signed\n" :nodes ((:label "Hidden node\n" :key lsp-treemacs-generic-root :state treemacs-lsp-treemacs-generic-root-open :depth -1) (:label "Release Ω" :key release :state treemacs-lsp-treemacs-generic-node-open :depth 0) (:label "Checks" :key checks :state treemacs-lsp-treemacs-generic-node-open :depth 1) (:label "Unit: 128 passed" :key unit :state treemacs-lsp-treemacs-generic-node-open :depth 2) (:label "Deploy staging" :key deploy :state treemacs-lsp-treemacs-generic-node-open :depth 2) (:label "Artifacts signed" :key signed :state treemacs-lsp-treemacs-generic-node-open :depth 1)) :generic-mode t :major-mode treemacs-mode :mode-line "Release Dashboard") :chosen deploy :binding treemacs-RET-action)"#
    ]];
    ParityBatchCase::value(
        "renders_a_nested_release_dashboard_and_dispatches_its_action",
        elisp_form,
        expected,
    )
}

fn displays_document_symbols_and_jumps_to_a_nested_method() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-lsp-treemacs-test-reset)
  (let* ((sandbox (make-temp-file "lsp-treemacs-symbols-" t))
         (project (expand-file-name "workspace" sandbox))
         (source (expand-file-name "release.rs" project))
         (uri (lsp--path-to-uri source))
         (requests nil)
         (jumps 0)
         (lsp-treemacs-after-jump-hook nil)
         jump
         source-buffer)
    (unwind-protect
        (progn
          (make-directory project)
          (with-temp-file source
            (insert "struct Release;\nfn ship(order: &str) { publish(order); }\nfn rollback() {}\n"))
          (setq source-buffer (find-file-noselect source))
          (switch-to-buffer source-buffer)
          (let* ((ship
                  (lsp-make-document-symbol
                   :name "ship" :detail "(order: &str)" :kind 12
                   :range (neomacs-lsp-treemacs-test-range 1 0 39)
                   :selection-range (neomacs-lsp-treemacs-test-range 1 3 7)))
                 (rollback
                  (lsp-make-document-symbol
                   :name "rollback" :detail "()" :kind 12
                   :range (neomacs-lsp-treemacs-test-range 2 0 16)
                   :selection-range (neomacs-lsp-treemacs-test-range 2 3 11)))
                 (release
                  (lsp-make-document-symbol
                   :name "Release" :detail "class" :kind 5
                   :range (lsp-make-range
                           :start (neomacs-lsp-treemacs-test-position 0 0)
                           :end (neomacs-lsp-treemacs-test-position 2 16))
                   :selection-range (neomacs-lsp-treemacs-test-range 0 7 14)
                   :children (vector ship rollback)))
                 (lsp-treemacs-detailed-outline t)
                 (lsp-treemacs-symbols-sort-functions
                  '(lsp-treemacs-sort-by-position)))
            (add-hook 'lsp-treemacs-after-jump-hook
                      (lambda () (setq jumps (1+ jumps))))
            (cl-letf (((symbol-function 'lsp--find-workspaces-for)
                       (lambda (&rest _) t))
                      ((symbol-function 'lsp--text-document-identifier)
                       (lambda () (list :uri uri)))
                      ((symbol-function 'lsp-request-async)
                       (lambda (method params callback &rest _)
                         (push
                          (list
                           method
                           (file-relative-name
                            (lsp--uri-to-path
                             (plist-get (lsp-get params :textDocument) :uri))
                            project))
                          requests)
                         (funcall callback (vector release)))))
              (lsp-treemacs-symbols))
            (let* ((buffer (get-buffer lsp-treemacs-symbols-buffer-name))
                   (rendered
                    (neomacs-lsp-treemacs-test-snapshot buffer project)))
              (with-current-buffer buffer
                (neomacs-lsp-treemacs-test-goto-label "ship (order: &str)")
                (lsp-treemacs-perform-ret-action)
                (with-current-buffer (window-buffer (selected-window))
                  (setq jump
                        (list :file (file-relative-name buffer-file-name project)
                              :line (line-number-at-pos)
                              :column (current-column)
                              :text (buffer-substring-no-properties
                                     (line-beginning-position)
                                     (line-end-position))))))
              (list :request
                    (car requests)
                    :rendered rendered
                    :jump (append jump (list :hooks jumps))))))
      (neomacs-lsp-treemacs-test-reset)
      (neomacs-lsp-treemacs-test-clean-file-buffers sandbox)
      (delete-directory sandbox t))))
"####;
    let expected = expect![[
        r#"OK (:request ("textDocument/documentSymbol" "release.rs") :rendered (:text "Hidden node\n▾   Release class\n      ship (order: &str)\n      rollback ()\n" :nodes ((:label "Hidden node\n" :key lsp-treemacs-generic-root :state treemacs-lsp-treemacs-generic-root-open :depth -1) (:label "Release class" :key "Release" :state treemacs-lsp-treemacs-generic-node-open :depth 0) (:label "ship (order: &str)" :key "ship" :state treemacs-lsp-treemacs-generic-node-open :depth 1) (:label "rollback ()" :key "rollback" :state treemacs-lsp-treemacs-generic-node-open :depth 1)) :generic-mode t :major-mode treemacs-mode :mode-line " LSP Symbols ") :jump (:file "release.rs" :line 2 :column 3 :text "fn ship(order: &str) { publish(order); }" :hooks 1))"#
    ]];
    ParityBatchCase::value(
        "displays_document_symbols_and_jumps_to_a_nested_method",
        elisp_form,
        expected,
    )
}

fn groups_references_by_project_and_file_then_opens_the_selected_line() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-lsp-treemacs-test-reset)
  (let* ((sandbox (make-temp-file "lsp-treemacs-refs-" t))
         (project (expand-file-name "workspace" sandbox))
         (main (expand-file-name "src/main.rs" project))
         (tests (expand-file-name "tests/release.rs" project))
         (requests nil)
         (jumps 0)
         pending-callback
         jump
         (xref-after-jump-hook nil))
    (unwind-protect
        (progn
          (make-directory (file-name-directory main) t)
          (make-directory (file-name-directory tests) t)
          (with-temp-file main
            (insert "fn deploy() {\n    let result = ship(order);\n}\n"))
          (with-temp-file tests
            (insert "#[test]\nfn ships_release() { ship(test_order); }\n"))
          (switch-to-buffer (find-file-noselect main))
          (let ((refs
                 (list
                  (lsp-make-location
                   :uri (lsp--path-to-uri main)
                   :range (neomacs-lsp-treemacs-test-range 1 17 21))
                  (lsp-make-location
                   :uri (lsp--path-to-uri tests)
                   :range (neomacs-lsp-treemacs-test-range 1 21 25)))))
            (add-hook 'xref-after-jump-hook
                      (lambda () (setq jumps (1+ jumps))))
            (cl-letf (((symbol-function 'lsp--text-document-position-params)
                       (lambda () '(:textDocument (:uri "file:///workspace/src/main.rs")
                                    :position (:line 1 :character 19))))
                      ((symbol-function 'lsp-workspace-root)
                       (lambda (&optional _) project))
                      ((symbol-function 'lsp-request-async)
                       (lambda (method params callback &rest _)
                         (push (list method params) requests)
                         (setq pending-callback callback))))
              (lsp-treemacs-references t)
              (funcall pending-callback refs))
            (let* ((buffer (get-buffer "*LSP Lookup*"))
                   (rendered
                    (neomacs-lsp-treemacs-test-snapshot buffer project)))
              (with-current-buffer buffer
                (neomacs-lsp-treemacs-test-goto-label
                 "let result = ship(order); 2 line")
                (lsp-treemacs-perform-ret-action)
                (with-current-buffer (window-buffer (selected-window))
                  (setq jump
                        (list :file (file-relative-name buffer-file-name project)
                              :line (line-number-at-pos)
                              :column (current-column)
                              :text (buffer-substring-no-properties
                                     (line-beginning-position)
                                     (line-end-position))))))
              (list :request (car requests)
                    :rendered rendered
                    :jump (append jump (list :hooks jumps))))))
      (neomacs-lsp-treemacs-test-reset)
      (neomacs-lsp-treemacs-test-clean-file-buffers sandbox)
      (delete-directory sandbox t))))
"####;
    let expected = expect![[
        r#"OK (:request ("textDocument/references" (:context (:includeDeclaration t) :textDocument (:uri "file:///workspace/src/main.rs") :position (:line 1 :character 19))) :rendered (:text "Hidden node\n▾   workspace 2 references\n  ▾   main.rs 1 references\n      let result = ship(order); 2 line\n  ▾   release.rs 1 references\n      fn ships_release() { ship(test_order); } 2 line\n" :nodes ((:label "Hidden node\n" :key lsp-treemacs-generic-root :state treemacs-lsp-treemacs-generic-root-open :depth -1) (:label "workspace 2 references" :key "$ROOT" :state treemacs-lsp-treemacs-generic-node-open :depth 0) (:label "main.rs 1 references" :key "$ROOT/src/main.rs" :state treemacs-lsp-treemacs-generic-node-open :depth 1) (:label "let result = ship(order); 2 line" :key #("    let result = ship(order);" 17 21 (face highlight)) :state treemacs-lsp-treemacs-generic-node-open :depth 2) (:label "release.rs 1 references" :key "$ROOT/tests/release.rs" :state treemacs-lsp-treemacs-generic-node-open :depth 1) (:label "fn ships_release() { ship(test_order); } 2 line" :key #("fn ships_release() { ship(test_order); }" 21 25 (face highlight)) :state treemacs-lsp-treemacs-generic-node-open :depth 2)) :generic-mode t :major-mode treemacs-mode :mode-line " Found 2 references ") :jump (:file "src/main.rs" :line 2 :column 17 :text "    let result = ship(order);" :hooks 1))"#
    ]];
    ParityBatchCase::value(
        "groups_references_by_project_and_file_then_opens_the_selected_line",
        elisp_form,
        expected,
    )
}

fn expands_outgoing_call_hierarchy_and_jumps_to_the_callee() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-lsp-treemacs-test-reset)
  (let* ((sandbox (make-temp-file "lsp-treemacs-calls-" t))
         (project (expand-file-name "workspace" sandbox))
         (main (expand-file-name "src/main.rs" project))
         (audit (expand-file-name "src/audit.rs" project))
         (requests nil)
         (jumps 0)
         jump
         (xref-after-jump-hook nil))
    (unwind-protect
        (progn
          (make-directory (file-name-directory main) t)
          (with-temp-file main
            (insert "fn ship() {\n    audit(order);\n}\n"))
          (with-temp-file audit
            (insert "fn audit(order: Order) { persist(order); }\n"))
          (switch-to-buffer (find-file-noselect main))
          (let* ((ship
                  (lsp-make-call-hierarchy-item
                   :name "ship" :kind 12 :uri (lsp--path-to-uri main)
                   :range (neomacs-lsp-treemacs-test-range 0 0 9)
                   :selection-range (neomacs-lsp-treemacs-test-range 0 3 7)))
                 (audit-item
                  (lsp-make-call-hierarchy-item
                   :name "audit" :detail "(order: Order)" :kind 12
                   :uri (lsp--path-to-uri audit)
                   :range (neomacs-lsp-treemacs-test-range 0 0 42)
                   :selection-range (neomacs-lsp-treemacs-test-range 0 3 8)))
                 (call
                  (lsp-make-call-hierarchy-outgoing-call
                   :to audit-item
                   :from-ranges
                   (vector (neomacs-lsp-treemacs-test-range 1 4 9))))
                 (lsp-treemacs-call-hierarchy-expand-depth t))
            (add-hook 'xref-after-jump-hook
                      (lambda () (setq jumps (1+ jumps))))
            (cl-letf (((symbol-function 'lsp-feature?)
                       (lambda (&rest _) t))
                      ((symbol-function 'lsp--text-document-position-params)
                       (lambda () '(:textDocument (:uri "file:///workspace/src/main.rs")
                                    :position (:line 0 :character 4))))
                      ((symbol-function 'lsp-request)
                       (lambda (method _params &rest _)
                         (push (list :sync method) requests)
                         (vector ship)))
                      ((symbol-function 'lsp-request-async)
                       (lambda (method params callback &rest _)
                         (let ((name (lsp:call-hierarchy-item-name
                                      (plist-get params :item))))
                           (push (list :async method name) requests)
                           (funcall callback
                                    (if (equal name "ship")
                                        (vector call)
                                      []))))))
              (lsp-treemacs-call-hierarchy t))
            (let* ((buffer (get-buffer "*Call Hierarchy*"))
                   (rendered
                    (neomacs-lsp-treemacs-test-snapshot buffer project)))
              (with-current-buffer buffer
                (neomacs-lsp-treemacs-test-goto-label "audit (order: Order)")
                (lsp-treemacs-perform-ret-action)
                (with-current-buffer (window-buffer (selected-window))
                  (setq jump
                        (list :file (file-relative-name buffer-file-name project)
                              :line (line-number-at-pos)
                              :column (current-column)
                              :text (buffer-substring-no-properties
                                     (line-beginning-position)
                                     (line-end-position))))))
              (list :requests (nreverse requests)
                    :rendered rendered
                    :jump (append jump (list :hooks jumps))))))
      (neomacs-lsp-treemacs-test-reset)
      (neomacs-lsp-treemacs-test-clean-file-buffers sandbox)
      (delete-directory sandbox t))))
"####;
    let expected = expect![[
        r#"OK (:requests ((:sync "textDocument/prepareCallHierarchy") (:async "callHierarchy/outgoingCalls" "ship") (:async "callHierarchy/outgoingCalls" "audit")) :rendered (:text "Hidden node\n▾   ship\n  ▾   audit (order: Order)\n      audit(order); 2 line\n" :nodes ((:label "Hidden node\n" :key lsp-treemacs-generic-root :state treemacs-lsp-treemacs-generic-root-open :depth -1) (:label "ship" :key "ship" :state treemacs-lsp-treemacs-generic-node-open :depth 0) (:label "audit (order: Order)" :key #("audit (order: Order)" 5 20 (face lsp-signature-face)) :state treemacs-lsp-treemacs-generic-node-open :depth 1) (:label "audit(order); 2 line" :key ("file://$ROOT/src/main.rs" 1 4 9) :state treemacs-lsp-treemacs-generic-node-open :depth 2)) :generic-mode t :major-mode treemacs-mode :mode-line "Outgoing Call Hierarchy") :jump (:file "src/audit.rs" :line 1 :column 3 :text "fn audit(order: Order) { persist(order); }" :hooks 1))"#
    ]];
    ParityBatchCase::value(
        "expands_outgoing_call_hierarchy_and_jumps_to_the_callee",
        elisp_form,
        expected,
    )
}

fn filters_diagnostics_by_severity_and_runs_quick_fix_at_point() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-lsp-treemacs-test-reset)
  (let* ((sandbox (make-temp-file "lsp-treemacs-errors-" t))
         (project (expand-file-name "workspace" sandbox))
         (source (expand-file-name "src/release.rs" project))
         (diagnostics (make-hash-table :test 'equal))
         (lsp-diagnostic-stats (make-hash-table :test 'equal))
         (lsp--session (make-lsp-session :folders (list project)))
         (lsp-treemacs-error-list-severity 3)
         (lsp-treemacs-error-list-expand-depth t)
         (quick-fixes 0)
         jump)
    (unwind-protect
        (progn
          (make-directory (file-name-directory source) t)
          (with-temp-file source
            (insert "fn ship() {\n    publish(order)\n    retry();\n}\n"))
          (puthash
           source
           (list
            (lsp-make-diagnostic
             :severity 2 :source "rustc" :message "missing semicolon"
             :range (neomacs-lsp-treemacs-test-range 1 18 18))
            (lsp-make-diagnostic
             :severity 1 :source "policy" :message "unsafe release path"
             :range (neomacs-lsp-treemacs-test-range 0 3 7))
            (lsp-make-diagnostic
             :severity 3 :source "clippy" :message "retry can be simplified\nconsider a loop"
             :range (neomacs-lsp-treemacs-test-range 2 4 9)))
           diagnostics)
          (puthash project [0 1 1 1 0] lsp-diagnostic-stats)
          (puthash source [0 1 1 1 0] lsp-diagnostic-stats)
          (cl-letf (((symbol-function 'lsp-workspaces) (lambda () nil))
                    ((symbol-function 'lsp-diagnostics)
                     (lambda (&optional _) diagnostics))
                    ((symbol-function 'lsp-execute-code-action)
                     (lambda () (interactive)
                       (setq quick-fixes (1+ quick-fixes)))))
            (lsp-treemacs-errors-list)
            (let* ((buffer (get-buffer lsp-treemacs-errors-buffer-name))
                   (all (neomacs-lsp-treemacs-test-snapshot buffer sandbox)))
              (with-current-buffer buffer
                (lsp-treemacs-cycle-severity))
              (let ((warning-and-error
                     (neomacs-lsp-treemacs-test-snapshot buffer sandbox)))
                (with-current-buffer buffer
                  (lsp-treemacs-cycle-severity))
                (let ((errors-only
                       (neomacs-lsp-treemacs-test-snapshot buffer sandbox)))
                  (with-current-buffer buffer
                    (setq lsp-treemacs-error-list-severity 3)
                    (lsp-treemacs-errors-list--refresh)
                    (neomacs-lsp-treemacs-test-goto-label
                     "[rustc] missing semicolon (1:18)")
                    (lsp-treemacs-quick-fix)
                    (with-current-buffer (window-buffer (selected-window))
                      (setq jump
                            (list :file
                                  (file-relative-name buffer-file-name project)
                                  :line (line-number-at-pos)
                                  :column (current-column)))))
                  (list :all all
                        :warning-and-error warning-and-error
                        :errors-only errors-only
                        :quick-fix
                        (append (list :count quick-fixes) jump)))))))
      (neomacs-lsp-treemacs-test-reset)
      (neomacs-lsp-treemacs-test-clean-file-buffers sandbox)
      (delete-directory sandbox t))))
"####;
    let expected = expect![[
        r#"OK (:all (:text "Hidden node\n▾ workspace 1/1/1 $ROOT\n  ▾   release.rs 1/1/1 src/\n      • [policy] unsafe release path (0:3)\n      • [rustc] missing semicolon (1:18)\n      • [clippy] retry can be simplified, consider a loop (2:4)" :nodes ((:label "Hidden node\n" :key lsp-treemacs-generic-root :state treemacs-lsp-treemacs-generic-root-open :depth -1) (:label "workspace 1/1/1 $ROOT" :key nil :state treemacs-lsp-treemacs-generic-node-open :depth 0) (:label "release.rs 1/1/1 src/" :key nil :state treemacs-lsp-treemacs-generic-node-open :depth 1) (:label "[policy] unsafe release path (0:3)" :key nil :state treemacs-lsp-treemacs-generic-node-open :depth 2) (:label "[rustc] missing semicolon (1:18)" :key nil :state treemacs-lsp-treemacs-generic-node-closed :depth 2) (:label "[clippy] retry can be simplified, consider a loop (2:4)" :key nil :state treemacs-lsp-treemacs-generic-node-closed :depth 2)) :generic-mode t :major-mode treemacs-mode :mode-line "Errors List") :warning-and-error (:text "Hidden node\n▾ workspace 1/1 $ROOT\n  ▾   release.rs 1/1/1 src/\n      • [policy] unsafe release path (0:3)\n      • [rustc] missing semicolon (1:18)" :nodes ((:label "Hidden node\n" :key lsp-treemacs-generic-root :state treemacs-lsp-treemacs-generic-root-open :depth -1) (:label "workspace 1/1 $ROOT" :key nil :state treemacs-lsp-treemacs-generic-node-open :depth 0) (:label "release.rs 1/1/1 src/" :key nil :state treemacs-lsp-treemacs-generic-node-open :depth 1) (:label "[policy] unsafe release path (0:3)" :key nil :state treemacs-lsp-treemacs-generic-node-open :depth 2) (:label "[rustc] missing semicolon (1:18)" :key nil :state treemacs-lsp-treemacs-generic-node-closed :depth 2)) :generic-mode t :major-mode treemacs-mode :mode-line "Errors List") :errors-only (:text "Hidden node\n▾ workspace 1 $ROOT\n  ▾   release.rs 1/1/1 src/\n      • [policy] unsafe release path (0:3)" :nodes ((:label "Hidden node\n" :key lsp-treemacs-generic-root :state treemacs-lsp-treemacs-generic-root-open :depth -1) (:label "workspace 1 $ROOT" :key nil :state treemacs-lsp-treemacs-generic-node-open :depth 0) (:label "release.rs 1/1/1 src/" :key nil :state treemacs-lsp-treemacs-generic-node-open :depth 1) (:label "[policy] unsafe release path (0:3)" :key nil :state treemacs-lsp-treemacs-generic-node-closed :depth 2)) :generic-mode t :major-mode treemacs-mode :mode-line "Errors List") :quick-fix (:count 1 :file "src/release.rs" :line 2 :column 18))"#
    ]];
    ParityBatchCase::value(
        "filters_diagnostics_by_severity_and_runs_quick_fix_at_point",
        elisp_form,
        expected,
    )
}

fn synchronizes_real_treemacs_projects_in_both_directions() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox (make-temp-file "lsp-treemacs-sync-" t))
       (app (expand-file-name "application" sandbox))
       (docs (expand-file-name "documentation" sandbox))
       (treemacs-persist-file (expand-file-name "state/treemacs" sandbox))
       (treemacs-last-error-persist-file
        (expand-file-name "state/treemacs-error" sandbox))
       (workspace (treemacs-workspace->create! :name "Parity"))
       (treemacs--workspaces (list workspace))
       (treemacs--disabled-workspaces nil)
       (treemacs-create-project-functions nil)
       (treemacs-delete-project-functions nil)
       (treemacs-workspace-edit-hook nil)
       (treemacs-switch-workspace-hook nil)
       (lsp-workspace-folders-changed-functions nil)
       added removed)
  (unwind-protect
      (progn
        (make-directory app t)
        (make-directory docs t)
        (setf (treemacs-current-workspace) workspace)
        (cl-letf (((symbol-function 'lsp-workspace-folders-add)
                   (lambda (path) (push (file-relative-name path sandbox) added)))
                  ((symbol-function 'lsp-workspace-folders-remove)
                   (lambda (path) (push (file-relative-name path sandbox) removed))))
          (lsp-treemacs-sync-mode 1)
          (let ((add-result
                 (treemacs-do-add-project-to-workspace app "Application")))
            (run-hook-with-args 'lsp-workspace-folders-changed-functions
                                (list docs) (list app))
            (let ((projects
                   (mapcar
                    (lambda (project)
                      (list (treemacs-project->name project)
                            (file-relative-name
                             (treemacs-project->path project) sandbox)))
                    (treemacs-workspace->projects workspace))))
              (lsp-treemacs-sync-mode -1)
              (list :add-result (car add-result)
                    :outgoing-adds (nreverse added)
                    :outgoing-removes (nreverse removed)
                    :projects projects
                    :disabled-hooks
                    (list
                     (memq #'lsp-treemacs--on-folder-added
                           treemacs-create-project-functions)
                     (memq #'lsp-treemacs--on-folder-remove
                           treemacs-delete-project-functions)
                     (memq #'lsp-treemacs--sync-folders
                           lsp-workspace-folders-changed-functions)
                     (memq #'lsp-treemacs--treemacs->lsp
                           treemacs-workspace-edit-hook)))))))
    (when lsp-treemacs-sync-mode (lsp-treemacs-sync-mode -1))
    (delete-directory sandbox t)))
"####;
    let expected = expect![[
        r#"OK (:add-result success :outgoing-adds ("application") :outgoing-removes nil :projects (("Application" "application") ("documentation" "documentation")) :disabled-hooks (nil nil nil nil))"#
    ]];
    ParityBatchCase::value(
        "synchronizes_real_treemacs_projects_in_both_directions",
        elisp_form,
        expected,
    )
}

fn lsp_treemacs_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LSP_TREEMACS_MELPA_PIN, "lsp-treemacs.el")
        .expect("prepare pinned LSP Treemacs and exact dependencies below ./tmp")
        .with_timeout(Duration::from_secs(420))
        .with_prelude(PRELUDE)
}

#[test]
fn lsp_treemacs_practical_workflows_batch() {
    let cases = vec![
        renders_a_nested_release_dashboard_and_dispatches_its_action(),
        displays_document_symbols_and_jumps_to_a_nested_method(),
        groups_references_by_project_and_file_then_opens_the_selected_line(),
        expands_outgoing_call_hierarchy_and_jumps_to_the_callee(),
        filters_diagnostics_by_severity_and_runs_quick_fix_at_point(),
        synchronizes_real_treemacs_projects_in_both_directions(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("lsp-treemacs practical workflow parity batch");
    assert_oracle_batch_cases(
        lsp_treemacs_oracle(),
        test_name,
        "lsp-treemacs parity",
        &cases,
    );
}
