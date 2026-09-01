use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, MAGIT_POPUP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const MAGIT_POPUP_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const MAGIT_POPUP_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'magit-popup)

(defvar neomacs-mpopup-test-action-history nil)
(defvar neomacs-mpopup-test-reader-history nil)
(defvar neomacs-mpopup-test-sequence-active nil)

(defun neomacs-mpopup-test-read-option (prompt previous)
  "Return a deterministic deployment option while recording the prompt."
  (push (list :prompt prompt :previous previous)
        neomacs-mpopup-test-reader-history)
  (cond ((string-prefix-p "--environment=" prompt) "production")
        ((string-prefix-p "--timeout=" prompt) "45")
        (t (error "Unexpected option prompt: %s" prompt))))

(defun neomacs-mpopup-test-record-action (kind arguments)
  "Record a practical popup action of KIND with ARGUMENTS."
  (push (list :kind kind
              :this-command this-command
              :popup magit-current-popup
              :popup-action magit-current-popup-action
              :arguments arguments
              :environment
              (magit-current-popup-args :only "--environment=")
              :without-dry-run
              (magit-current-popup-args :not "--dry-run")
              :pre-popup-buffer
              (and magit-current-pre-popup-buffer
                   (buffer-name magit-current-pre-popup-buffer))
              :current-buffer (buffer-name)
              :prefix current-prefix-arg)
        neomacs-mpopup-test-action-history))

(defun neomacs-mpopup-test-preview (arguments)
  "Preview a deployment using popup ARGUMENTS."
  (interactive (list (neomacs-mpopup-test-deploy-arguments)))
  (neomacs-mpopup-test-record-action 'preview arguments))

(defun neomacs-mpopup-test-execute (arguments)
  "Execute a deployment using popup ARGUMENTS."
  (interactive (list (neomacs-mpopup-test-deploy-arguments)))
  (neomacs-mpopup-test-record-action 'execute arguments))

(defun neomacs-mpopup-test-sequence-action ()
  "Record which release sequence action was invoked."
  (interactive)
  (neomacs-mpopup-test-record-action
   this-command (neomacs-mpopup-test-release-arguments)))

(magit-define-popup neomacs-mpopup-test-deploy-popup
  "Configure and run a deterministic deployment."
  :man-page "deploy-tool"
  :max-action-columns 2
  :switches '((?d "Dry run" "--dry-run")
              (?f "Force" "--force")
              (?v "Verbose" "--verbose"))
  :options '((?e "Environment" "--environment="
                  neomacs-mpopup-test-read-option)
             (?t "Timeout" "--timeout="
                  neomacs-mpopup-test-read-option))
  :default-arguments '("--dry-run" "--environment=staging")
  :actions '((?p "Preview deployment" neomacs-mpopup-test-preview)
             (?x "Execute deployment" neomacs-mpopup-test-execute))
  :default-action 'neomacs-mpopup-test-preview)

(magit-define-popup neomacs-mpopup-test-release-popup
  "Operate a release or continue an active sequence."
  :sequence-predicate (lambda () neomacs-mpopup-test-sequence-active)
  :switches '((?n "No notifications" "--no-notify"))
  :options '((?c "Channel" "--channel="
                  neomacs-mpopup-test-read-option))
  :default-arguments '("--channel=stable")
  :actions '((?s "Start release" neomacs-mpopup-test-sequence-action))
  :sequence-actions
  '((?c "Continue rollout" neomacs-mpopup-test-sequence-action)
    (?a "Abort rollout" neomacs-mpopup-test-sequence-action)))

(magit-define-popup neomacs-mpopup-test-custom-popup
  "Popup extended at runtime."
  :switches '((?a "All records" "--all"))
  :options '((?f "Format" "--format="
                  neomacs-mpopup-test-read-option))
  :default-arguments '("--all" "--format=text")
  :actions '((?r "Run report" neomacs-mpopup-test-preview)))

(magit-define-popup-switch 'neomacs-mpopup-test-deferred-popup
  ?v "Verbose" "--verbose")
(magit-define-popup-option 'neomacs-mpopup-test-deferred-popup
  ?o "Output" "--output=" 'neomacs-mpopup-test-read-option)
(magit-define-popup neomacs-mpopup-test-deferred-popup
  "Popup whose infixes were registered before its definition."
  :actions '((?r "Run" neomacs-mpopup-test-preview)))

(defun neomacs-mpopup-test-event-summary (event)
  "Return stable public fields from a converted popup EVENT."
  (if (magit-popup-event-p event)
      (list :key (magit-popup-event-key event)
            :description (magit-popup-event-dsc event)
            :argument (magit-popup-event-arg event)
            :function (magit-popup-event-fun event)
            :enabled (magit-popup-event-use event)
            :value (magit-popup-event-val event))
    event))

(defun neomacs-mpopup-test-button-summary ()
  "Return ordered visible popup button metadata."
  (mapcar
   (lambda (button)
     (list :start (button-start button)
           :end (button-end button)
           :label (buffer-substring-no-properties
                   (button-start button) (button-end button))
           :type (button-get button 'type)
           :event (button-get button 'event)
           :prefix (button-get button 'prefix)
           :function (button-get button 'function)))
   (sort
    (cl-remove-if-not
     (lambda (overlay) (overlay-get overlay 'button))
     (overlays-in (point-min) (point-max)))
    (lambda (left right)
      (< (overlay-start left) (overlay-start right))))))

(defun neomacs-mpopup-test-popup-view ()
  "Return strict visible and logical state for the current popup."
  (list :buffer (buffer-name)
        :major-mode major-mode
        :read-only buffer-read-only
        :modified (buffer-modified-p)
        :popup magit-this-popup
        :pre-popup-buffer (buffer-name magit-pre-popup-buffer)
        :arguments (magit-popup-get-args)
        :switches
        (mapcar #'neomacs-mpopup-test-event-summary
                (magit-popup-get :switches))
        :options
        (mapcar #'neomacs-mpopup-test-event-summary
                (magit-popup-get :options))
        :actions
        (mapcar #'neomacs-mpopup-test-event-summary
                (magit-popup-get :actions))
        :text (buffer-substring-no-properties (point-min) (point-max))
        :buttons (neomacs-mpopup-test-button-summary)
        :bindings
        (let ((map (current-local-map)))
          (list
           (cons "- <t>" (lookup-key map (kbd "- <t>")))
           (cons "= <t>" (lookup-key map (kbd "= <t>")))
           (cons "self-insert remap"
                 (lookup-key map [remap self-insert-command]))
           (cons "C-g" (lookup-key map (kbd "C-g")))
           (cons "C-t" (lookup-key map (kbd "C-t")))
           (cons "C-c C-c" (lookup-key map (kbd "C-c C-c")))))))

(defun neomacs-mpopup-test-popup-definition (popup)
  "Return stable unconverted event definitions for POPUP."
  (let ((definition (symbol-value popup)))
    (copy-tree
     (list :variable (plist-get definition :variable)
           :switches (plist-get definition :switches)
           :options (plist-get definition :options)
           :actions (plist-get definition :actions)
           :sequence-actions (plist-get definition :sequence-actions)
           :default-arguments (plist-get definition :default-arguments)
           :deferred (get popup 'magit-popup-deferred)))))

(defun neomacs-mpopup-test-capture-signal (function)
  "Run FUNCTION and return complete stable signal information."
  (condition-case error-data
      (progn (funcall function) 'no-signal)
    (error
     (list :symbol (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defun neomacs-mpopup-test-run (name function)
  "Run FUNCTION from a deterministic source buffer named for NAME."
  (let ((source (generate-new-buffer
                 (format "*deployment-source-%s*" name)))
        result)
    (unwind-protect
        (save-window-excursion
          (delete-other-windows)
          (switch-to-buffer source)
          (setq result (funcall function source)))
      (dolist (buffer-name
               '("*neomacs-mpopup-test-deploy-popup*"
                 "*neomacs-mpopup-test-release-popup*"
                 "*neomacs-mpopup-test-custom-popup*"
                 "*neomacs-mpopup-test-deferred-popup*"))
        (when-let ((buffer (get-buffer buffer-name)))
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (kill-buffer buffer)))
      (when (buffer-live-p source)
        (kill-buffer source)))
    result))
"####;

fn magit_popup_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MAGIT_POPUP_MELPA_PIN, "magit-popup.el")
        .expect("prepare revision-pinned Magit Popup source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare revision-pinned Dash dependency below ./tmp")
        .with_prelude(MAGIT_POPUP_TEST_PRELUDE)
        .with_timeout(MAGIT_POPUP_TEST_TIMEOUT)
}

fn interactive_deployment_popup_refreshes_and_dispatches_arguments() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-mpopup-test-run
 "interactive"
 (lambda (source)
   (setq neomacs-mpopup-test-action-history nil
         neomacs-mpopup-test-reader-history nil)
   (let ((magit-popup-show-help-echo nil))
     (neomacs-mpopup-test-deploy-popup)
     (let ((initial (neomacs-mpopup-test-popup-view))
           changed common)
       (magit-invoke-popup-switch ?f)
       (magit-invoke-popup-switch ?d)
       (magit-invoke-popup-option ?e)
       (magit-invoke-popup-option ?e)
       (magit-invoke-popup-option ?t)
       (setq changed (neomacs-mpopup-test-popup-view))
       (magit-popup-toggle-show-common-commands)
       (setq common (neomacs-mpopup-test-popup-view))
       (magit-invoke-popup-action ?p)
       (list :initial initial
             :changed changed
             :common common
             :reader-history (nreverse neomacs-mpopup-test-reader-history)
             :action-history (nreverse neomacs-mpopup-test-action-history)
             :returned-to-source (eq (current-buffer) source)
             :popup-live
             (and (get-buffer "*neomacs-mpopup-test-deploy-popup*") t))))))
"####;
    let expected = expect![[
        r#"OK (:initial (:buffer "*neomacs-mpopup-test-deploy-popup*" :major-mode magit-popup-mode :read-only t :modified nil :popup neomacs-mpopup-test-deploy-popup :pre-popup-buffer "*deployment-source-interactive*" :arguments ("--dry-run" "--environment=staging") :switches ((:key 100 :description "Dry run" :argument "--dry-run" :function nil :enabled t :value nil) (:key 102 :description "Force" :argument "--force" :function nil :enabled nil :value nil) (:key 118 :description "Verbose" :argument "--verbose" :function nil :enabled nil :value nil)) :options ((:key 101 :description "Environment" :argument "--environment=" :function neomacs-mpopup-test-read-option :enabled t :value "staging") (:key 116 :description "Timeout" :argument "--timeout=" :function neomacs-mpopup-test-read-option :enabled nil :value nil)) :actions ((:key 112 :description "Preview deployment" :argument nil :function neomacs-mpopup-test-preview :enabled nil :value nil) (:key 120 :description "Execute deployment" :argument nil :function neomacs-mpopup-test-execute :enabled nil :value nil)) :text "Switches\n -d Dry run (--dry-run)\n -f Force (--force)\n -v Verbose (--verbose)\n\nOptions\n =e Environment (--environment=\"staging\")\n =t Timeout (--timeout=)\n\nActions\n p Preview deployment    x Execute deployment\n\n" :buttons ((:start 10 :end 33 :label " -d Dry run (--dry-run)" :type magit-popup-switch-button :event 100 :prefix 45 :function magit-invoke-popup-switch) (:start 34 :end 53 :label " -f Force (--force)" :type magit-popup-switch-button :event 102 :prefix 45 :function magit-invoke-popup-switch) (:start 54 :end 77 :label " -v Verbose (--verbose)" :type magit-popup-switch-button :event 118 :prefix 45 :function magit-invoke-popup-switch) (:start 87 :end 128 :label " =e Environment (--environment=\"staging\")" :type magit-popup-option-button :event 101 :prefix 61 :function magit-invoke-popup-option) (:start 129 :end 153 :label " =t Timeout (--timeout=)" :type magit-popup-option-button :event 116 :prefix 61 :function magit-invoke-popup-option) (:start 163 :end 184 :label " p Preview deployment" :type magit-popup-action-button :event 112 :prefix nil :function magit-invoke-popup-action) (:start 187 :end 208 :label " x Execute deployment" :type magit-popup-action-button :event 120 :prefix nil :function magit-invoke-popup-action)) :bindings (("- <t>" . magit-invoke-popup-switch) ("= <t>" . magit-invoke-popup-option) ("self-insert remap" . magit-invoke-popup-action) ("C-g" . magit-popup-quit) ("C-t" . magit-popup-toggle-show-common-commands) ("C-c C-c" . magit-popup-set-default-arguments))) :changed (:buffer "*neomacs-mpopup-test-deploy-popup*" :major-mode magit-popup-mode :read-only t :modified nil :popup neomacs-mpopup-test-deploy-popup :pre-popup-buffer "*deployment-source-interactive*" :arguments ("--force" "--environment=production" "--timeout=45") :switches ((:key 100 :description "Dry run" :argument "--dry-run" :function nil :enabled nil :value nil) (:key 102 :description "Force" :argument "--force" :function nil :enabled t :value nil) (:key 118 :description "Verbose" :argument "--verbose" :function nil :enabled nil :value nil)) :options ((:key 101 :description "Environment" :argument "--environment=" :function neomacs-mpopup-test-read-option :enabled t :value "production") (:key 116 :description "Timeout" :argument "--timeout=" :function neomacs-mpopup-test-read-option :enabled t :value "45")) :actions ((:key 112 :description "Preview deployment" :argument nil :function neomacs-mpopup-test-preview :enabled nil :value nil) (:key 120 :description "Execute deployment" :argument nil :function neomacs-mpopup-test-execute :enabled nil :value nil)) :text "Switches\n -d Dry run (--dry-run)\n -f Force (--force)\n -v Verbose (--verbose)\n\nOptions\n =e Environment (--environment=\"production\")\n =t Timeout (--timeout=\"45\")\n\nActions\n p Preview deployment    x Execute deployment\n\n" :buttons ((:start 10 :end 33 :label " -d Dry run (--dry-run)" :type magit-popup-switch-button :event 100 :prefix 45 :function magit-invoke-popup-switch) (:start 34 :end 53 :label " -f Force (--force)" :type magit-popup-switch-button :event 102 :prefix 45 :function magit-invoke-popup-switch) (:start 54 :end 77 :label " -v Verbose (--verbose)" :type magit-popup-switch-button :event 118 :prefix 45 :function magit-invoke-popup-switch) (:start 87 :end 131 :label " =e Environment (--environment=\"production\")" :type magit-popup-option-button :event 101 :prefix 61 :function magit-invoke-popup-option) (:start 132 :end 160 :label " =t Timeout (--timeout=\"45\")" :type magit-popup-option-button :event 116 :prefix 61 :function magit-invoke-popup-option) (:start 170 :end 191 :label " p Preview deployment" :type magit-popup-action-button :event 112 :prefix nil :function magit-invoke-popup-action) (:start 194 :end 215 :label " x Execute deployment" :type magit-popup-action-button :event 120 :prefix nil :function magit-invoke-popup-action)) :bindings (("- <t>" . magit-invoke-popup-switch) ("= <t>" . magit-invoke-popup-option) ("self-insert remap" . magit-invoke-popup-action) ("C-g" . magit-popup-quit) ("C-t" . magit-popup-toggle-show-common-commands) ("C-c C-c" . magit-popup-set-default-arguments))) :common (:buffer "*neomacs-mpopup-test-deploy-popup*" :major-mode magit-popup-mode :read-only t :modified nil :popup neomacs-mpopup-test-deploy-popup :pre-popup-buffer "*deployment-source-interactive*" :arguments ("--force" "--environment=production" "--timeout=45") :switches ((:key 100 :description "Dry run" :argument "--dry-run" :function nil :enabled nil :value nil) (:key 102 :description "Force" :argument "--force" :function nil :enabled t :value nil) (:key 118 :description "Verbose" :argument "--verbose" :function nil :enabled nil :value nil)) :options ((:key 101 :description "Environment" :argument "--environment=" :function neomacs-mpopup-test-read-option :enabled t :value "production") (:key 116 :description "Timeout" :argument "--timeout=" :function neomacs-mpopup-test-read-option :enabled t :value "45")) :actions ((:key 112 :description "Preview deployment" :argument nil :function neomacs-mpopup-test-preview :enabled nil :value nil) (:key 120 :description "Execute deployment" :argument nil :function neomacs-mpopup-test-execute :enabled nil :value nil)) :text "Switches\n -d Dry run (--dry-run)\n -f Force (--force)\n -v Verbose (--verbose)\n\nOptions\n =e Environment (--environment=\"production\")\n =t Timeout (--timeout=\"45\")\n\nActions\n p Preview deployment    x Execute deployment\n\nCommon Commands\n C-c C-c Set defaults       C-h i View popup manual\n C-t Toggle this section    C-x C-s Save defaults\n ?     Popup help prefix    C-g Abort\n\n" :buttons ((:start 10 :end 33 :label " -d Dry run (--dry-run)" :type magit-popup-switch-button :event 100 :prefix 45 :function magit-invoke-popup-switch) (:start 34 :end 53 :label " -f Force (--force)" :type magit-popup-switch-button :event 102 :prefix 45 :function magit-invoke-popup-switch) (:start 54 :end 77 :label " -v Verbose (--verbose)" :type magit-popup-switch-button :event 118 :prefix 45 :function magit-invoke-popup-switch) (:start 87 :end 131 :label " =e Environment (--environment=\"production\")" :type magit-popup-option-button :event 101 :prefix 61 :function magit-invoke-popup-option) (:start 132 :end 160 :label " =t Timeout (--timeout=\"45\")" :type magit-popup-option-button :event 116 :prefix 61 :function magit-invoke-popup-option) (:start 170 :end 191 :label " p Preview deployment" :type magit-popup-action-button :event 112 :prefix nil :function magit-invoke-popup-action) (:start 194 :end 215 :label " x Execute deployment" :type magit-popup-action-button :event 120 :prefix nil :function magit-invoke-popup-action) (:start 233 :end 254 :label " C-c C-c Set defaults" :type magit-popup-internal-command-button :event [3 3] :prefix nil :function magit-popup-set-default-arguments) (:start 260 :end 284 :label " C-h i View popup manual" :type magit-popup-internal-command-button :event [8 105] :prefix nil :function magit-popup-info) (:start 285 :end 309 :label " C-t Toggle this section" :type magit-popup-internal-command-button :event [20] :prefix nil :function magit-popup-toggle-show-common-commands) (:start 312 :end 334 :label " C-x C-s Save defaults" :type magit-popup-internal-command-button :event [24 19] :prefix nil :function magit-popup-save-default-arguments) (:start 335 :end 359 :label " ?     Popup help prefix" :type magit-popup-internal-command-button :event [63] :prefix nil :function magit-popup-help) (:start 362 :end 372 :label " C-g Abort" :type magit-popup-internal-command-button :event [7] :prefix nil :function magit-popup-quit)) :bindings (("- <t>" . magit-invoke-popup-switch) ("= <t>" . magit-invoke-popup-option) ("self-insert remap" . magit-invoke-popup-action) ("C-g" . magit-popup-quit) ("C-t" . magit-popup-toggle-show-common-commands) ("C-c C-c" . magit-popup-set-default-arguments))) :reader-history ((:prompt "--environment=" :previous "staging") (:prompt "--timeout=" :previous nil)) :action-history ((:kind preview :this-command neomacs-mpopup-test-preview :popup neomacs-mpopup-test-deploy-popup :popup-action neomacs-mpopup-test-preview :arguments ("--force" "--environment=production" "--timeout=45") :environment ("--environment=production") :without-dry-run ("--force" "--environment=production" "--timeout=45") :pre-popup-buffer "*deployment-source-interactive*" :current-buffer "*deployment-source-interactive*" :prefix nil)) :returned-to-source t :popup-live nil)"#
    ]];
    ParityBatchCase::value(
        "interactive_deployment_popup_refreshes_and_dispatches_arguments",
        elisp_form,
        expected,
    )
}

fn prefix_and_direct_invocation_use_buffer_local_defaults() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-mpopup-test-run
 "prefix-defaults"
 (lambda (_source)
   (setq-local neomacs-mpopup-test-deploy-arguments
               '("--force" "--timeout=90"))
   (setq neomacs-mpopup-test-action-history nil)
   (neomacs-mpopup-test-deploy-popup '(4))
   (neomacs-mpopup-test-deploy-popup '(8))
   (let ((current-prefix-arg nil))
     (call-interactively #'neomacs-mpopup-test-preview))
   (list :local-defaults neomacs-mpopup-test-deploy-arguments
         :history (nreverse neomacs-mpopup-test-action-history)
         :popup-live
         (and (get-buffer "*neomacs-mpopup-test-deploy-popup*") t))))
"####;
    let expected = expect![[
        r#"OK (:local-defaults #1=("--force" "--timeout=90") :history ((:kind preview :this-command neomacs-mpopup-test-preview :popup (neomacs-mpopup-test-deploy-popup default) :popup-action nil :arguments #1# :environment nil :without-dry-run ("--force" "--timeout=90") :pre-popup-buffer nil :current-buffer "*deployment-source-prefix-defaults*" :prefix nil) (:kind preview :this-command neomacs-mpopup-test-preview :popup (neomacs-mpopup-test-deploy-popup default) :popup-action nil :arguments #1# :environment nil :without-dry-run ("--force" "--timeout=90") :pre-popup-buffer nil :current-buffer "*deployment-source-prefix-defaults*" :prefix (2)) (:kind preview :this-command neomacs-mpopup-test-preview :popup nil :popup-action nil :arguments #1# :environment nil :without-dry-run nil :pre-popup-buffer nil :current-buffer "*deployment-source-prefix-defaults*" :prefix nil)) :popup-live nil)"#
    ]];
    ParityBatchCase::value(
        "prefix_and_direct_invocation_use_buffer_local_defaults",
        elisp_form,
        expected,
    )
}

fn popup_extension_apis_preserve_order_and_deferred_infixes() -> ParityBatchCase {
    let elisp_form = r####"
(let ((original (copy-tree neomacs-mpopup-test-custom-popup)))
  (unwind-protect
      (let ((initial
             (neomacs-mpopup-test-popup-definition
              'neomacs-mpopup-test-custom-popup)))
        (magit-define-popup-switch 'neomacs-mpopup-test-custom-popup
          ?v "Verbose" "--verbose" nil ?a nil)
        (magit-define-popup-switch 'neomacs-mpopup-test-custom-popup
          ?q "Quiet" "--quiet" t ?a t)
        (magit-define-popup-option 'neomacs-mpopup-test-custom-popup
          ?t "Timeout" "--timeout=" 'neomacs-mpopup-test-read-option
          "30" ?f t)
        (magit-define-popup-action 'neomacs-mpopup-test-custom-popup
          ?p "Preview" 'neomacs-mpopup-test-preview ?r t)
        (let ((extended
               (neomacs-mpopup-test-popup-definition
                'neomacs-mpopup-test-custom-popup)))
          (magit-change-popup-key
           'neomacs-mpopup-test-custom-popup :switch ?v ?V)
          (magit-remove-popup-key
           'neomacs-mpopup-test-custom-popup :switch ?a)
          (list
           :initial initial
           :extended extended
           :changed
           (neomacs-mpopup-test-popup-definition
            'neomacs-mpopup-test-custom-popup)
           :deferred
           (neomacs-mpopup-test-popup-definition
            'neomacs-mpopup-test-deferred-popup))))
    (setq neomacs-mpopup-test-custom-popup original)))
"####;
    let expected = expect![[
        r#"OK (:initial (:variable neomacs-mpopup-test-custom-arguments :switches ((97 "All records" "--all")) :options ((102 "Format" "--format=" neomacs-mpopup-test-read-option)) :actions ((114 "Run report" neomacs-mpopup-test-preview)) :sequence-actions nil :default-arguments ("--all" "--format=text") :deferred nil) :extended (:variable neomacs-mpopup-test-custom-arguments :switches ((113 "Quiet" "--quiet" t) (97 "All records" "--all") (118 "Verbose" "--verbose" nil)) :options ((116 "Timeout" "--timeout=" neomacs-mpopup-test-read-option "30") (102 "Format" "--format=" neomacs-mpopup-test-read-option)) :actions ((112 "Preview" neomacs-mpopup-test-preview) (114 "Run report" neomacs-mpopup-test-preview)) :sequence-actions nil :default-arguments ("--all" "--format=text") :deferred nil) :changed (:variable neomacs-mpopup-test-custom-arguments :switches ((113 "Quiet" "--quiet" t) (86 "Verbose" "--verbose" nil)) :options ((116 "Timeout" "--timeout=" neomacs-mpopup-test-read-option "30") (102 "Format" "--format=" neomacs-mpopup-test-read-option)) :actions ((112 "Preview" neomacs-mpopup-test-preview) (114 "Run report" neomacs-mpopup-test-preview)) :sequence-actions nil :default-arguments ("--all" "--format=text") :deferred nil) :deferred (:variable neomacs-mpopup-test-deferred-arguments :switches ((118 "Verbose" "--verbose" nil)) :options ((111 "Output" "--output=" neomacs-mpopup-test-read-option nil)) :actions ((114 "Run" neomacs-mpopup-test-preview)) :sequence-actions nil :default-arguments nil :deferred nil))"#
    ]];
    ParityBatchCase::value(
        "popup_extension_apis_preserve_order_and_deferred_infixes",
        elisp_form,
        expected,
    )
}

fn cli_argument_filters_and_file_transport_build_real_command_inputs() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((arguments '("--dry-run"
                    "--environment=production"
                    "--timeout=45"
                    "--verbose"))
       (magit-current-popup-args arguments)
       (with-files
        (magit-popup-import-file-args
         arguments '("deploy plan.yml" "λ-release.toml")))
       (exported (magit-popup-export-file-args with-files)))
  (list
   :environment-and-timeout
   (magit-current-popup-args :only "--environment=" "--timeout=")
   :without-flags
   (magit-current-popup-args :not "--dry-run" "--verbose")
   :exact-and-prefix-matches
   (mapcar (lambda (pair)
             (list (car pair) (cadr pair)
                   (magit-popup-arg-match (car pair) (cadr pair))))
           '(("--dry-run" "--dry-run")
             ("--dry" "--dry-run")
             ("--environment=" "--environment=production")
             ("-X" "-Xours")
             ("-x" "-xours")))
   :with-files with-files
   :exported-arguments (car exported)
   :exported-files (cadr exported)
   :empty-import (magit-popup-import-file-args '("--quiet") nil)
   :empty-export (magit-popup-export-file-args '("--quiet"))))
"####;
    let expected = expect![[
        r#"OK (:environment-and-timeout ("--environment=production" "--timeout=45") :without-flags ("--environment=production" "--timeout=45") :exact-and-prefix-matches (("--dry-run" "--dry-run" t) ("--dry" "--dry-run" nil) ("--environment=" "--environment=production" 0) ("-X" "-Xours" 0) ("-x" "-xours" 0)) :with-files ("-- deploy plan.yml,λ-release.toml" "--dry-run" "--environment=production" "--timeout=45" "--verbose") :exported-arguments ("--dry-run" "--environment=production" "--timeout=45" "--verbose") :exported-files ("deploy plan.yml" "λ-release.toml") :empty-import ("--quiet") :empty-export (("--quiet") nil))"#
    ]];
    ParityBatchCase::value(
        "cli_argument_filters_and_file_transport_build_real_command_inputs",
        elisp_form,
        expected,
    )
}

fn release_sequence_replaces_normal_infixes_and_actions() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-mpopup-test-run
 "sequence"
 (lambda (_source)
   (setq neomacs-mpopup-test-action-history nil
         neomacs-mpopup-test-sequence-active nil)
   (let ((magit-popup-show-help-echo nil))
     (neomacs-mpopup-test-release-popup)
     (let ((normal (neomacs-mpopup-test-popup-view)))
       (magit-popup-quit)
       (setq neomacs-mpopup-test-sequence-active t)
       (neomacs-mpopup-test-release-popup)
       (let ((sequence (neomacs-mpopup-test-popup-view))
             missing-switch)
         (setq missing-switch
               (neomacs-mpopup-test-capture-signal
                (lambda () (magit-invoke-popup-switch ?n))))
         (magit-invoke-popup-action ?c)
         (list :normal normal
               :sequence sequence
               :missing-switch missing-switch
               :action-history
               (nreverse neomacs-mpopup-test-action-history)))))))
"####;
    let expected = expect![[
        r#"OK (:normal (:buffer "*neomacs-mpopup-test-release-popup*" :major-mode magit-popup-mode :read-only t :modified nil :popup neomacs-mpopup-test-release-popup :pre-popup-buffer "*deployment-source-sequence*" :arguments ("--channel=stable") :switches ((:key 110 :description "No notifications" :argument "--no-notify" :function nil :enabled nil :value nil)) :options ((:key 99 :description "Channel" :argument "--channel=" :function neomacs-mpopup-test-read-option :enabled t :value "stable")) :actions ((:key 115 :description "Start release" :argument nil :function neomacs-mpopup-test-sequence-action :enabled nil :value nil)) :text "Switches\n -n No notifications (--no-notify)\n\nOptions\n =c Channel (--channel=\"stable\")\n\nActions\n s Start release\n\n" :buttons ((:start 10 :end 44 :label " -n No notifications (--no-notify)" :type magit-popup-switch-button :event 110 :prefix 45 :function magit-invoke-popup-switch) (:start 54 :end 86 :label " =c Channel (--channel=\"stable\")" :type magit-popup-option-button :event 99 :prefix 61 :function magit-invoke-popup-option) (:start 96 :end 112 :label " s Start release" :type magit-popup-action-button :event 115 :prefix nil :function magit-invoke-popup-action)) :bindings (("- <t>" . magit-invoke-popup-switch) ("= <t>" . magit-invoke-popup-option) ("self-insert remap" . magit-invoke-popup-action) ("C-g" . magit-popup-quit) ("C-t" . magit-popup-toggle-show-common-commands) ("C-c C-c" . magit-popup-set-default-arguments))) :sequence (:buffer "*neomacs-mpopup-test-release-popup*" :major-mode magit-popup-mode :read-only t :modified nil :popup neomacs-mpopup-test-release-popup :pre-popup-buffer "*deployment-source-sequence*" :arguments nil :switches nil :options nil :actions ((:key 99 :description "Continue rollout" :argument nil :function neomacs-mpopup-test-sequence-action :enabled nil :value nil) (:key 97 :description "Abort rollout" :argument nil :function neomacs-mpopup-test-sequence-action :enabled nil :value nil)) :text "Actions\n c Continue rollout    a Abort rollout\n\n" :buttons ((:start 9 :end 28 :label " c Continue rollout" :type magit-popup-action-button :event 99 :prefix nil :function magit-invoke-popup-action) (:start 31 :end 47 :label " a Abort rollout" :type magit-popup-action-button :event 97 :prefix nil :function magit-invoke-popup-action)) :bindings (("- <t>" . magit-invoke-popup-switch) ("= <t>" . magit-invoke-popup-option) ("self-insert remap" . magit-invoke-popup-action) ("C-g" . magit-popup-quit) ("C-t" . magit-popup-toggle-show-common-commands) ("C-c C-c" . magit-popup-set-default-arguments))) :missing-switch (:symbol user-error :data ("n isn’t bound to any switch") :message "n isn’t bound to any switch") :action-history ((:kind neomacs-mpopup-test-sequence-action :this-command neomacs-mpopup-test-sequence-action :popup neomacs-mpopup-test-release-popup :popup-action neomacs-mpopup-test-sequence-action :arguments nil :environment nil :without-dry-run nil :pre-popup-buffer "*deployment-source-sequence*" :current-buffer "*deployment-source-sequence*" :prefix (2))))"#
    ]];
    ParityBatchCase::value(
        "release_sequence_replaces_normal_infixes_and_actions",
        elisp_form,
        expected,
    )
}

#[test]
fn magit_popup_package_batch() {
    let cases = vec![
        interactive_deployment_popup_refreshes_and_dispatches_arguments(),
        prefix_and_direct_invocation_use_buffer_local_defaults(),
        popup_extension_apis_preserve_order_and_deferred_infixes(),
        cli_argument_filters_and_file_transport_build_real_command_inputs(),
        release_sequence_replaces_normal_infixes_and_actions(),
    ];
    assert_oracle_batch_cases(
        magit_popup_oracle(),
        "magit-popup-package-batch",
        "magit-popup",
        &cases,
    );
}
