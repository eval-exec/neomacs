use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HELM_DESCBINDS_MELPA_PIN, HELM_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HELM_DESCBINDS_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const HELM_DESCBINDS_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'seq)
(require 'helm-descbinds)

(defvar neomacs-helm-descbinds-test-command-log nil)

(defun neomacs-helm-descbinds-test-deploy ()
  "Deploy the release selected by a parity workflow."
  (interactive)
  (push 'deploy neomacs-helm-descbinds-test-command-log))

(defun neomacs-helm-descbinds-test-preview ()
  "Preview the release selected by a parity workflow."
  (interactive)
  (push 'preview neomacs-helm-descbinds-test-command-log))

(defun neomacs-helm-descbinds-test-rollback ()
  "Roll back the release selected by a parity workflow."
  (interactive)
  (push 'rollback neomacs-helm-descbinds-test-command-log))

(defun neomacs-helm-descbinds-test-global-status ()
  "Show global release status for a parity workflow."
  (interactive)
  (push 'global-status neomacs-helm-descbinds-test-command-log))

(defvar neomacs-helm-descbinds-test-mode-map
  (let ((map (make-sparse-keymap))
        (deploy-map (make-sparse-keymap)))
    (define-key map (kbd "C-c d") deploy-map)
    (define-key deploy-map (kbd "d") #'neomacs-helm-descbinds-test-deploy)
    (define-key deploy-map (kbd "p") #'neomacs-helm-descbinds-test-preview)
    (define-key deploy-map (kbd "k") "preview-deploy")
    (define-key map [f8] "macro-output")
    map))

(define-derived-mode neomacs-helm-descbinds-test-mode fundamental-mode
  "Descbinds Test"
  "Major mode for realistic Helm-Descbinds parity workflows.")

(defvar neomacs-helm-descbinds-test-minor-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c m r") #'neomacs-helm-descbinds-test-rollback)
    map))

(define-minor-mode neomacs-helm-descbinds-test-minor-mode
  "Expose a realistic rollback binding to Helm-Descbinds."
  :init-value nil
  :lighter " HDB"
  :keymap neomacs-helm-descbinds-test-minor-mode-map)

(defun neomacs-helm-descbinds-test-relevant-sections (sections)
  "Keep only parity commands from SECTIONS while preserving section order."
  (delq
   nil
   (mapcar
    (lambda (section)
      (let ((bindings
             (seq-filter
              (lambda (binding)
                (string-prefix-p "neomacs-helm-descbinds-test-"
                                 (cdr binding)))
              (cdr section))))
        (when bindings
          (cons (car section) bindings))))
    sections)))

(defun neomacs-helm-descbinds-test-source-names (sources)
  "Return the package, minor-mode, and global names from Helm SOURCES."
  (seq-filter
   (lambda (name)
     (or (string-match-p "neomacs-helm-descbinds-test" name)
         (string= name "Global Bindings:")))
   (delq nil
         (mapcar
          (lambda (source)
            (and source (helm-get-attr 'name source)))
          sources))))

(defun neomacs-helm-descbinds-test-candidate-snapshot (candidate)
  "Return the display, real value, and face spans of CANDIDATE."
  (let ((display (car candidate))
        spans)
    (dolist (interval (object-intervals display))
      (when-let ((face (plist-get (nth 2 interval) 'face)))
        (push (list (car interval) (cadr interval) face) spans)))
    (list :display (substring-no-properties display)
          :real (cdr candidate)
          :faces (nreverse spans))))

(defun neomacs-helm-descbinds-test-capture-signal (function)
  "Return stable signal data from FUNCTION, or its value."
  (condition-case error-data
      (list :value (funcall function))
    (error
     (list :signal (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defun neomacs-helm-descbinds-test-reset ()
  "Restore state changed by a Helm-Descbinds parity case."
  (when helm-descbinds-mode
    (ignore-errors (helm-descbinds-mode -1)))
  (advice-remove 'describe-bindings #'helm-descbinds)
  (advice-remove 'which-key-mode #'helm-descbinds--override-which-key)
  (dolist (name '("*Help*" "*helm-descbinds*"))
    (when-let ((buffer (get-buffer name)))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (setq neomacs-helm-descbinds-test-command-log nil))

(defun neomacs-helm-descbinds-test-with-reset (function)
  "Run FUNCTION without leaking Helm-Descbinds state."
  (neomacs-helm-descbinds-test-reset)
  (unwind-protect
      (funcall function)
    (neomacs-helm-descbinds-test-reset)))
"###;

fn helm_descbinds_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_DESCBINDS_MELPA_PIN, "helm-descbinds.el")
        .expect("prepare revision-pinned Helm-Descbinds source below ./tmp")
        .with_melpa_dependency(HELM_MELPA_PIN)
        .expect("prepare revision-pinned Helm dependency below ./tmp")
        .with_prelude(HELM_DESCBINDS_TEST_PRELUDE)
        .with_timeout(HELM_DESCBINDS_TEST_TIMEOUT)
}

fn real_major_minor_and_global_bindings_preserve_gnu_section_precedence() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-descbinds-test-with-reset
 (lambda ()
   (let* ((global-key (kbd "C-c g s"))
          (old-global (lookup-key (current-global-map) global-key)))
     (unwind-protect
         (progn
           (global-set-key global-key
                           #'neomacs-helm-descbinds-test-global-status)
           (with-temp-buffer
             (neomacs-helm-descbinds-test-mode)
             (neomacs-helm-descbinds-test-minor-mode 1)
             (list
              :sections
              (neomacs-helm-descbinds-test-relevant-sections
               (helm-descbinds-all-sections (current-buffer)))
              :active
              (mapcar
               (lambda (key)
                 (list key (key-binding (kbd key))))
               '("C-c d d" "C-c d p" "C-c d k"
                 "C-c m r" "C-c g s")))))
       (define-key (current-global-map) global-key
         (if (numberp old-global) nil old-global))))))
"###;
    let expected = expect![[
        r###"OK (:sections (("`neomacs-helm-descbinds-test-minor-mode' Minor Mode Bindings:" ("C-c m r" . "neomacs-helm-descbinds-test-rollback")) ("`neomacs-helm-descbinds-test-mode' Major Mode Bindings:" ("C-c d d" . "neomacs-helm-descbinds-test-deploy") ("C-c d p" . "neomacs-helm-descbinds-test-preview")) ("Global Bindings:" ("C-c g s" . "neomacs-helm-descbinds-test-global-status"))) :active (("C-c d d" neomacs-helm-descbinds-test-deploy) ("C-c d p" neomacs-helm-descbinds-test-preview) ("C-c d k" "preview-deploy") ("C-c m r" neomacs-helm-descbinds-test-rollback) ("C-c g s" neomacs-helm-descbinds-test-global-status)))"###
    ]];
    ParityBatchCase::value(
        "real_major_minor_and_global_bindings_preserve_gnu_section_precedence",
        elisp_form,
        expected,
    )
}

fn prefix_narrowing_returns_only_the_real_deployment_submap() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-descbinds-test-with-reset
 (lambda ()
   (with-temp-buffer
     (neomacs-helm-descbinds-test-mode)
     (neomacs-helm-descbinds-test-minor-mode 1)
     (let ((sections
            (helm-descbinds-all-sections
             (current-buffer) (kbd "C-c d") nil)))
       (list
        :sections sections
        :relevant
        (copy-tree
         (neomacs-helm-descbinds-test-relevant-sections sections))
        :all-bindings
        (apply #'+ (mapcar (lambda (section) (length (cdr section)))
                           sections)))))))
"###;
    let expected = expect![[
        r###"OK (:sections ((nil) ("`neomacs-helm-descbinds-test-mode' Major Mode Bindings Starting With C-c d:" ("C-c d d" . "neomacs-helm-descbinds-test-deploy") ("C-c d k" . "Keyboard Macro") ("C-c d p" . "neomacs-helm-descbinds-test-preview"))) :relevant (("`neomacs-helm-descbinds-test-mode' Major Mode Bindings Starting With C-c d:" ("C-c d d" . "neomacs-helm-descbinds-test-deploy") ("C-c d p" . "neomacs-helm-descbinds-test-preview"))) :all-bindings 3)"###
    ]];
    ParityBatchCase::value(
        "prefix_narrowing_returns_only_the_real_deployment_submap",
        elisp_form,
        expected,
    )
}

fn function_key_translations_are_exposed_as_a_searchable_binding_section() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-descbinds-test-with-reset
 (lambda ()
   (let ((local-function-key-map (make-sparse-keymap)))
     (define-key local-function-key-map [f24] (kbd "C-c d d"))
     (with-temp-buffer
       (neomacs-helm-descbinds-test-mode)
       (let ((sections
              (helm-descbinds-all-sections (current-buffer))))
         (seq-filter
          (lambda (section)
            (and (car section)
                 (string-match-p "Function key map translations"
                                 (car section))))
          sections))))))
"###;
    let expected = expect![[
        r###"OK (("Function key map translations:" ("<f24>" . "C-c d d\11END OF TEXT")))"###
    ]];
    ParityBatchCase::value(
        "function_key_translations_are_exposed_as_a_searchable_binding_section",
        elisp_form,
        expected,
    )
}

fn candidate_pipeline_formats_keys_and_resolves_commands_prefixes_and_macros() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-descbinds-test-with-reset
 (lambda ()
   (let* ((raw
           '(("C-c d d" . "neomacs-helm-descbinds-test-deploy")
             ("C-c d" . "Prefix Command")
             ("<f8>" . "Keyboard Macro")
             ("C-c z" . "literal deployment note")))
          (transformed (helm-descbinds-transform-candidates raw))
          (source (helm-descbinds-source "Release bindings" raw)))
     (list
      :candidates
      (mapcar #'neomacs-helm-descbinds-test-candidate-snapshot transformed)
      :source
      (list :name (helm-get-attr 'name source)
            :representation (type-of source)
            :raw (helm-get-attr 'candidates source)
            :transformer (helm-get-attr 'candidate-transformer source)
            :persistent (helm-get-attr 'persistent-action source)
            :action-variable (helm-get-attr 'action source)
            :actions
            (mapcar
             #'car
             (symbol-value (helm-get-attr 'action source))))
      :empty (helm-descbinds-source "Empty" nil)))))
"###;
    let expected = expect![[
        r###"OK (:candidates ((:display "C-c d d   \11neomacs-helm-descbinds-test-deploy" :real ("C-c d d" . neomacs-helm-descbinds-test-deploy) :faces ((0 10 helm-descbinds-key) (11 45 helm-descbinds-binding))) (:display "C-c d     \11Prefix Command" :real ("C-c d" . "Prefix Command") :faces ((0 10 helm-descbinds-key) (11 25 helm-descbinds-binding))) (:display "<f8>      \11Keyboard Macro" :real ("<f8>" . "Keyboard Macro") :faces ((0 10 helm-descbinds-key) (11 25 helm-descbinds-binding))) (:display "C-c z     \11literal deployment note" :real ("C-c z" . "literal deployment note") :faces ((0 10 helm-descbinds-key) (11 34 helm-descbinds-binding)))) :source (:name "Release bindings" :representation cons :raw (("C-c d d" . "neomacs-helm-descbinds-test-deploy") ("C-c d" . "Prefix Command") ("<f8>" . "Keyboard Macro") ("C-c z" . "literal deployment note")) :transformer helm-descbinds-transform-candidates :persistent helm-descbinds-action:describe :action-variable helm-descbinds-actions :actions ("Execute" "Describe" "Find Function")) :empty nil)"###
    ]];
    ParityBatchCase::value(
        "candidate_pipeline_formats_keys_and_resolves_commands_prefixes_and_macros",
        elisp_form,
        expected,
    )
}

fn execute_action_runs_commands_inserts_literals_and_dispatches_keyboard_macros() -> ParityBatchCase
{
    let elisp_form = r###"
(neomacs-helm-descbinds-test-with-reset
 (lambda ()
   (with-temp-buffer
     (neomacs-helm-descbinds-test-mode)
     (let ((neomacs-helm-descbinds-test-command-log nil)
           dispatched)
       (cl-letf (((symbol-function 'command-execute)
                  (lambda (command &rest _)
                    (setq dispatched command))))
         (helm-descbinds-action:execute
          '("C-c d d" . neomacs-helm-descbinds-test-deploy))
         (helm-descbinds-action:execute
          '("literal" . "release=2.4.1 status=ready"))
         (insert "|")
         (helm-descbinds-action:execute
          '("<f8>" . "Keyboard Macro")))
       (list :text (buffer-string)
             :commands (nreverse neomacs-helm-descbinds-test-command-log)
             :macro (key-description dispatched)
             :point (point)
             :mode major-mode)))))
"###;
    let expected = expect![[
        r###"OK (:text "release=2.4.1 status=ready|" :commands (deploy) :macro "<f8>" :point 28 :mode neomacs-helm-descbinds-test-mode)"###
    ]];
    ParityBatchCase::value(
        "execute_action_runs_commands_inserts_literals_and_dispatches_keyboard_macros",
        elisp_form,
        expected,
    )
}

fn help_and_definition_actions_route_commands_prefixes_and_keyboard_macros() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-descbinds-test-with-reset
 (lambda ()
   (let (described-key described-function found-function prefix-help)
     (cl-letf (((symbol-function 'describe-key)
                (lambda (key &rest _)
                  (setq described-key (key-description key))))
               ((symbol-function 'find-function)
                (lambda (function &rest _)
                  (setq found-function function)))
               (helm-describe-function-function
                (lambda (function)
                  (setq described-function function))))
       (helm-descbinds-action:describe
        '("<f8>" . "Keyboard Macro"))
       (helm-descbinds-action:describe
        '("C-c d" . "Prefix Command"))
       (setq prefix-help
             (with-current-buffer "*Help*"
               (list :mode major-mode
                     :read-only buffer-read-only
                     :text
                     (buffer-substring-no-properties
                      (point-min) (point-max)))))
       (helm-descbinds-action:describe
        '("C-c d d" . neomacs-helm-descbinds-test-deploy))
       (helm-descbinds-action:find-func
        '("C-c d d" . neomacs-helm-descbinds-test-deploy)))
     (list :key described-key
           :function described-function
           :found found-function
           :prefix-help prefix-help))))
"###;
    let expected = expect![[
        r###"OK (:key "<f8>" :function neomacs-helm-descbinds-test-deploy :found neomacs-helm-descbinds-test-deploy :prefix-help (:mode help-mode :read-only t :text "This is a prefix key, hit RET to see all bindings using this prefix.\n\nA “prefix key” is a key sequence whose binding is a keymap.  The keymap\ndefines what to do with key sequences that extend the prefix key.  For\nexample, ‘C-x’ is a prefix key, and it uses a keymap that is also stored\nin the variable ‘ctl-x-map’.  This keymap defines bindings for key\nsequences starting with ‘C-x’.\nSee (info \"(elisp) Prefix Keys\") for more infos."))"###
    ]];
    ParityBatchCase::value(
        "help_and_definition_actions_route_commands_prefixes_and_keyboard_macros",
        elisp_form,
        expected,
    )
}

fn prefix_action_transformer_narrows_real_prefixes_and_rejects_frame_pseudo_keys() -> ParityBatchCase
{
    let elisp_form = r###"
(neomacs-helm-descbinds-test-with-reset
 (lambda ()
   (let ((actions helm-descbinds-actions)
         described-prefix
         pseudo-message)
     (cl-letf (((symbol-function 'describe-bindings)
                (lambda (&optional prefix buffer)
                  (setq described-prefix
                        (list (key-description prefix) buffer))))
               ((symbol-function 'message)
                (lambda (format &rest args)
                  (setq pseudo-message (apply #'format format args)))))
       (let* ((prefix-actions
               (helm-descbinds-action-transformer
                actions '("C-c d" . "Prefix Command")))
              (pseudo-actions
               (helm-descbinds-action-transformer
                actions '("<make-frame-visible>" . "Prefix Command")))
              (command-actions
               (helm-descbinds-action-transformer
                actions
                '("C-c d d" . neomacs-helm-descbinds-test-deploy))))
         (funcall (cdar prefix-actions) '("C-c d" . "Prefix Command"))
         (funcall (cdar pseudo-actions)
                  '("<make-frame-visible>" . "Prefix Command"))
         (list :prefix-actions (mapcar #'car prefix-actions)
               :described described-prefix
               :pseudo-message pseudo-message
               :command-actions (eq command-actions actions)))))))
"###;
    let expected = expect![[
        r###"OK (:prefix-actions ("helm-descbinds this prefix") :described ("C-c d" nil) :pseudo-message "Key is bound to `ignore' because there is nothing to do" :command-actions t)"###
    ]];
    ParityBatchCase::value(
        "prefix_action_transformer_narrows_real_prefixes_and_rejects_frame_pseudo_keys",
        elisp_form,
        expected,
    )
}

fn global_mode_disables_conflicting_which_key_and_restores_editor_bindings() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-descbinds-test-with-reset
 (lambda ()
   (let* ((help-key (kbd "<help> C-h"))
          (old-help (lookup-key (current-global-map) help-key))
          (which-key-mode t)
          (which-calls nil)
          enabled attempted disabled)
     (unwind-protect
         (cl-letf (((symbol-function 'which-key-mode)
                    (lambda (&optional arg)
                      (push arg which-calls)
                      (setq which-key-mode
                            (if (null arg)
                                (not which-key-mode)
                              (> (prefix-numeric-value arg) 0))))))
           (helm-descbinds-mode 1)
           (helm-descbinds-mode 1)
           (setq enabled
                 (list :mode helm-descbinds-mode
                       :describe-advice
                       (and (advice-member-p #'helm-descbinds
                                             'describe-bindings)
                            t)
                       :which which-key-mode
                       :which-advice
                       (and (advice-member-p
                             #'helm-descbinds--override-which-key
                             'which-key-mode)
                            t)
                       :help (lookup-key (current-global-map) help-key)))
           (setq attempted
                 (neomacs-helm-descbinds-test-capture-signal
                  (lambda () (which-key-mode 1))))
           (helm-descbinds-mode -1)
           (setq disabled
                 (list :mode helm-descbinds-mode
                       :describe-advice
                       (advice-member-p #'helm-descbinds 'describe-bindings)
                       :which which-key-mode
                       :which-advice
                       (advice-member-p
                        #'helm-descbinds--override-which-key 'which-key-mode)
                       :help (lookup-key (current-global-map) help-key)))
           (list :enabled enabled
                 :attempted attempted
                 :disabled disabled
                 :which-calls (nreverse which-calls)))
       (define-key (current-global-map) help-key
         (if (numberp old-help) nil old-help))))))
"###;
    let expected = expect![[
        r###"OK (:enabled (:mode t :describe-advice t :which nil :which-advice t :help nil) :attempted (:signal error :data ("‘which-key-mode’ can’t be used with ‘helm-descbinds-mode’") :message "‘which-key-mode’ can’t be used with ‘helm-descbinds-mode’") :disabled (:mode nil :describe-advice nil :which t :which-advice nil :help help-for-help) :which-calls (-1 t))"###
    ]];
    ParityBatchCase::value(
        "global_mode_disables_conflicting_which_key_and_restores_editor_bindings",
        elisp_form,
        expected,
    )
}

fn launch_workflow_builds_real_sources_and_applies_each_window_policy() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-helm-descbinds-test-with-reset
 (lambda ()
   (with-temp-buffer
     (neomacs-helm-descbinds-test-mode)
     (neomacs-helm-descbinds-test-minor-mode 1)
     (let ((target (current-buffer))
           calls)
       (dolist (style '(one-window same-window split-window))
         (let ((helm-descbinds-window-style style)
               (helm-full-frame nil)
               (helm-before-initialize-hook '(existing-hook)))
           (cl-letf (((symbol-function 'helm)
                      (lambda (&rest arguments)
                        (push
                         (list
                          :style style
                          :full-frame (and helm-full-frame t)
                          :before-hook
                          (copy-sequence helm-before-initialize-hook)
                          :source-names
                          (neomacs-helm-descbinds-test-source-names
                           (plist-get arguments :sources))
                          :buffer (plist-get arguments :buffer)
                          :resume (plist-get arguments :resume)
                          :allow-nest (plist-get arguments :allow-nest))
                         calls)
                        :launched)))
             (helm-descbinds nil target))))
       (list :calls (nreverse calls)
             :initial-full-frame helm-descbind--initial-full-frame)))))
"###;
    let expected = expect![[
        r###"OK (:calls ((:style one-window :full-frame t :before-hook (delete-other-windows existing-hook) :source-names ("`neomacs-helm-descbinds-test-mode' Major Mode Bindings:" "`neomacs-helm-descbinds-test-minor-mode' Minor Mode Bindings:" "Global Bindings:") :buffer "*helm-descbinds*" :resume noresume :allow-nest t) (:style same-window :full-frame t :before-hook (existing-hook) :source-names ("`neomacs-helm-descbinds-test-mode' Major Mode Bindings:" "`neomacs-helm-descbinds-test-minor-mode' Minor Mode Bindings:" "Global Bindings:") :buffer "*helm-descbinds*" :resume noresume :allow-nest t) (:style split-window :full-frame nil :before-hook (existing-hook) :source-names ("`neomacs-helm-descbinds-test-mode' Major Mode Bindings:" "`neomacs-helm-descbinds-test-minor-mode' Minor Mode Bindings:" "Global Bindings:") :buffer "*helm-descbinds*" :resume noresume :allow-nest t)) :initial-full-frame nil)"###
    ]];
    ParityBatchCase::value(
        "launch_workflow_builds_real_sources_and_applies_each_window_policy",
        elisp_form,
        expected,
    )
}

#[test]
fn helm_descbinds_package_batch() {
    let cases = vec![
        real_major_minor_and_global_bindings_preserve_gnu_section_precedence(),
        prefix_narrowing_returns_only_the_real_deployment_submap(),
        function_key_translations_are_exposed_as_a_searchable_binding_section(),
        candidate_pipeline_formats_keys_and_resolves_commands_prefixes_and_macros(),
        execute_action_runs_commands_inserts_literals_and_dispatches_keyboard_macros(),
        help_and_definition_actions_route_commands_prefixes_and_keyboard_macros(),
        prefix_action_transformer_narrows_real_prefixes_and_rejects_frame_pseudo_keys(),
        global_mode_disables_conflicting_which_key_and_restores_editor_bindings(),
        launch_workflow_builds_real_sources_and_applies_each_window_policy(),
    ];
    assert_oracle_batch_cases(
        helm_descbinds_oracle(),
        "helm-descbinds-package-batch",
        "Helm-Descbinds",
        &cases,
    );
}
