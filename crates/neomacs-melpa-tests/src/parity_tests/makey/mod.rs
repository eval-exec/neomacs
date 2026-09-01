use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MAKEY_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const MAKEY_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const MAKEY_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'makey)

(defvar makey-test-action-log nil)
(defvar makey-test-help-events nil)
(defvar makey-test-inputs nil)
(defvar makey-test-prompts nil)
(defvar makey-test-trace nil)
(defvar makey-test-retries 2)

(defun makey-test-read-string (prompt)
  (push prompt makey-test-prompts)
  (pop makey-test-inputs))

(defun makey-test-read-number (prompt)
  (push prompt makey-test-prompts)
  (string-to-number (pop makey-test-inputs)))

(defun makey-test-normalize-options (options)
  (mapcar
   (lambda (option)
     (if (consp option)
         (cons (car option) (cdr option))
       option))
   options))

(defun makey-test-hash-entries (table)
  (let (entries)
    (maphash
     (lambda (key value)
       (push (cons key value) entries))
     table)
    (sort entries
          (lambda (left right)
            (string< (car left) (car right))))))

(defun makey-test-record-action (kind)
  (setq makey-test-action-log
        (list
         :kind kind
         :custom-options
         (makey-test-normalize-options makey-custom-options)
         :trace makey-test-trace
         :retries makey-test-retries
         :prefix current-prefix-arg
         :buffer (buffer-name)
         :point (point)
         :windows (length (window-list nil 'nomini)))))

(defun makey-test-deploy ()
  (interactive)
  (makey-test-record-action 'deploy))

(defun makey-test-rollback ()
  (interactive)
  (makey-test-record-action 'rollback))

(defconst makey-test-groups
  '((deployctl
     (description . "Deploy control center")
     (man-page "deployctl")
     (switches
      ("v" "Verbose logs" "--verbose")
      ("f" "Force rollout" "--force"))
     (lisp-switches
      ("t" "Trace hooks" makey-test-trace t nil))
     (arguments
      ("e" "Environment" "--environment=" makey-test-read-string))
     (lisp-arguments
      ("r" "Retry count" "makey-test-retries" makey-test-read-number))
     (actions
      ("Release"
       ("d" "Deploy release" makey-test-deploy)
       ("b" "Rollback release" makey-test-rollback))
      ("Template"
       ("i" "Insert completion mark" "!"))))))

(defun makey-test-reset ()
  (when (and makey-key-mode-last-buffer
             (buffer-live-p makey-key-mode-last-buffer))
    (kill-buffer makey-key-mode-last-buffer))
  (delete-other-windows)
  (when-let ((scratch (get-buffer "*scratch*")))
    (switch-to-buffer scratch))
  (setq makey-key-mode-keymaps nil
        makey-key-mode-last-buffer nil
        makey-pre-key-mode-window-conf nil
        makey-key-mode-prefix nil
        makey-key-mode-current-args nil
        makey-key-mode-current-lisp-arguments nil
        makey-key-mode-current-lisp-options nil
        makey-key-mode-current-options nil
        makey-custom-options nil
        makey-key-mode-show-usage nil
        makey-key-mode-args-in-cols nil
        makey-test-action-log nil
        makey-test-help-events nil
        makey-test-inputs nil
        makey-test-prompts nil
        makey-test-trace nil
        makey-test-retries 2)
  (makey-initialize-key-groups makey-test-groups))

(defun makey-test-goto-key (key)
  (goto-char (point-min))
  (let ((position
         (cdr (assoc key
                     (makey-key-mode-build-exec-point-alist)))))
    (unless position
      (error "No rendered Makey key %s" key))
    (goto-char position)
    (skip-chars-forward " ")
    (point)))

(defun makey-test-button-text (key)
  (save-excursion
    (makey-test-goto-key key)
    (let ((start
           (or (previous-single-property-change
                (1+ (point)) 'key-group-executor)
               (point-min)))
          (end
           (or (next-single-property-change
                (point) 'key-group-executor)
               (point-max))))
      (buffer-substring-no-properties start end))))

(defun makey-test-state ()
  (list
   :options
   (makey-test-normalize-options makey-key-mode-current-options)
   :lisp-options
   (makey-test-normalize-options makey-key-mode-current-lisp-options)
   :args (makey-test-hash-entries makey-key-mode-current-args)
   :lisp-args
   (makey-test-hash-entries makey-key-mode-current-lisp-arguments)
   :point-key (get-text-property (point) 'key-group-executor)
   :buttons
   (mapcar
    (lambda (key)
      (list key (makey-test-button-text key)))
    '("v" "f" "t" "e" "r" "d" "b" "i"))))

(defun makey-test-property-runs ()
  (let ((position (point-min))
        runs)
    (while (< position (point-max))
      (let ((next (next-property-change position nil (point-max))))
        (push
         (list
          (buffer-substring-no-properties position next)
          (get-text-property position 'key-group-executor)
          (get-text-property position 'face))
         runs)
        (setq position next)))
    (nreverse runs)))

(defun makey-test-window-state ()
  (let* ((services-buffer (get-buffer " *makey-services*"))
         (events-buffer (get-buffer " *makey-events*"))
         (menu-buffer (get-buffer "*makey-key: deployctl*"))
         (services
          (and services-buffer
               (get-buffer-window services-buffer)))
         (events
          (and events-buffer
               (get-buffer-window events-buffer)))
         (menu
          (and menu-buffer
               (get-buffer-window menu-buffer)))
         (services-edges (and services (window-edges services)))
         (events-edges (and events (window-edges events)))
         (menu-edges (and menu (window-edges menu))))
    (list
     :count (length (window-list nil 'nomini))
     :services
     (and services
          (list :point (window-point services)
                :width (window-total-width services)))
     :events
     (and events
          (list :point (window-point events)
                :width (window-total-width events)))
     :dashboard-side-by-side
     (and services events
          (= (cadr services-edges) (cadr events-edges))
          (= (nth 3 services-edges) (nth 3 events-edges))
          (= (nth 2 services-edges) (car events-edges)))
     :menu
     (and menu
          (list
           :point (window-point menu)
           :width (window-total-width menu)
           :left-column-stack
           (and services events
                (= (car menu-edges) (car services-edges))
                (= (nth 2 menu-edges) (nth 2 services-edges))
                (= (cadr menu-edges) (nth 3 services-edges))
                (= (nth 2 services-edges) (car events-edges))
                (= (cadr services-edges) (cadr events-edges))
                (= (nth 3 menu-edges) (nth 3 events-edges))))))))

(defun makey-test-help-outcome (group key)
  (setq makey-test-help-events nil)
  (let (prompt)
    (cl-letf (((symbol-function 'read-key-sequence)
               (lambda (message)
                 (setq prompt message)
                 key))
              ((symbol-function 'describe-function)
               (lambda (function)
                 (push (list :describe function)
                       makey-test-help-events)))
              ((symbol-function 'man)
               (lambda (page)
                 (push (list :man page)
                       makey-test-help-events))))
      (let ((outcome
             (condition-case problem
                 (progn
                   (makey-key-mode-help group)
                   :ok)
               (error
                (list (car problem)
                      (error-message-string problem))))))
        (list :prompt prompt
              :outcome outcome
              :events (nreverse makey-test-help-events))))))
"##;

fn makey_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MAKEY_MELPA_PIN, "makey.el")
        .expect("prepare pinned Makey source below ./tmp")
        .with_prelude(MAKEY_TEST_PRELUDE)
        .with_timeout(MAKEY_TEST_TIMEOUT)
}

fn generated_group_configuration_supports_safe_runtime_extension_and_removal() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (makey-test-reset)
  (let* ((groups
          '((shipctl
             (description . "Shipping console")
             (switches)
             (arguments)
             (actions
              ("Release"
               ("d" "Dispatch shipment"
                makey-test-deploy))))))
         (master (copy-tree groups)))
    (makey-initialize-key-groups master)
    (let ((generated
           (list
            (fboundp 'makey-key-mode-options-for-shipctl)
            (commandp 'makey-key-mode-popup-shipctl)
            (copy-tree
             (makey-key-mode-options-for-shipctl)))))
      (makey-key-mode-insert-switch
       'shipctl "v" "Verbose manifest" "--verbose")
      (makey-key-mode-insert-argument
       'shipctl "e" "Destination" "--destination="
       'makey-test-read-string)
      (let* ((first-map (makey-key-mode-get-key-map 'shipctl))
             (duplicate
              (condition-case problem
                  (progn
                    (makey-key-mode-insert-action
                     'shipctl "v" "Conflicting action"
                     'makey-test-rollback)
                    :accepted)
                (error
                 (list (car problem)
                       (error-message-string problem)))))
             (_
              (makey-key-mode-insert-switch
               'shipctl "f" "Force dispatch" "--force"))
             (second-map (makey-key-mode-get-key-map 'shipctl))
             (updated
              (copy-tree
               (makey-key-mode-options-for-shipctl)))
             (defined
              (mapcar
               (lambda (key)
                 (list key
                       (makey-key-mode-key-defined-p
                        'shipctl key)
                       (commandp
                        (lookup-key second-map (kbd key)))))
               '("d" "v" "e" "f" "x")))
             (remaining
              (makey-key-mode-delete-group 'shipctl master)))
        (list
         :generated generated
         :updated updated
         :duplicate duplicate
         :cache-refreshed (not (eq first-map second-map))
         :defined defined
         :deleted
         (list :remaining remaining
               :options-bound
               (fboundp 'makey-key-mode-options-for-shipctl)
               :popup-bound
               (fboundp 'makey-key-mode-popup-shipctl)))))))
"##;
    let expect = expect![[
        r##"OK (:generated (t t ((description . "Shipping console") (switches) (arguments) (actions ("Release" ("d" "Dispatch shipment" makey-test-deploy))))) :updated ((description . "Shipping console") (switches ("v" "Verbose manifest" "--verbose") ("f" "Force dispatch" "--force")) (arguments ("e" "Destination" "--destination=" makey-test-read-string)) (actions ("Release" ("d" "Dispatch shipment" makey-test-deploy)))) :duplicate (error "v is already defined in the shipctl group.") :cache-refreshed t :defined (("d" t t) ("v" t t) ("e" t t) ("f" t t) ("x" nil nil)) :deleted (:remaining nil :options-bound t :popup-bound nil))"##
    ]];
    ParityBatchCase::value(
        "generated_group_configuration_supports_safe_runtime_extension_and_removal",
        elisp_form,
        expect,
    )
}

fn rendered_operations_popup_exposes_exact_content_properties_and_bindings() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (makey-test-reset)
  (let ((source (generate-new-buffer " *makey-release*"))
        result)
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer source)
          (insert "release dashboard\nREL-417 ready\n")
          (goto-char 24)
          (let ((source-point (point))
                (source-window (selected-window))
                (current-prefix-arg '(4)))
            (makey-key-mode-popup-deployctl)
            (let* ((menu (current-buffer))
                   (map (current-local-map))
                   (opened
                    (list
                     :selected-menu
                     (eq menu (get-buffer "*makey-key: deployctl*"))
                     :windows (length (window-list nil 'nomini))
                     :mode major-mode
                     :mode-name mode-name
                     :header header-line-format
                     :read-only buffer-read-only
                     :prefix makey-key-mode-prefix
                     :content
                     (buffer-substring-no-properties
                      (point-min) (point-max))
                     :state (makey-test-state)
                     :property-runs (makey-test-property-runs)
                     :bindings
                     (mapcar
                      (lambda (key)
                        (list key
                              (commandp
                               (lookup-key map (kbd key)))))
                      '("RET" "TAB" "C-g" "q" "?" "d" "b"
                        "i" "v" "f" "t" "e" "r")))))
              (call-interactively (lookup-key map (kbd "q")))
              (setq result
                    (list
                     :opened opened
                     :closed
                     (list
                      :menu-live (buffer-live-p menu)
                      :selected-source
                      (and (eq (selected-window) source-window)
                           (eq (current-buffer) source))
                      :source-point (point)
                      :point-restored (= (point) source-point)
                      :windows
                      (length (window-list nil 'nomini))))))))
      (when (buffer-live-p source)
        (kill-buffer source))
      (makey-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:opened (:selected-menu t :windows 2 :mode makey-key-mode :mode-name "makey-key-mode" :header "Deploy control center" :read-only t :prefix (4) :content "Switches\n v: Verbose logs (--verbose)    f: Force rollout (--force)\nSwitches\n t: Trace hooks (makey-test-trace)\nArgs\n e: Environment (--environment=)\n\nArgs\n r: Retry count (makey-test-retries)\n\nRelease Actions\n d: Deploy release      b: Rollback release\nTemplate Actions\n i: Insert completion mark\n" :state (:options ((makey-test-trace)) :lisp-options ((makey-test-trace)) :args nil :lisp-args nil :point-key "d" :buttons (("v" " v: Verbose logs (--verbose)") ("f" " f: Force rollout (--force)") ("t" " t: Trace hooks (makey-test-trace)") ("e" " e: Environment (--environment=)") ("r" " r: Retry count (makey-test-retries)") ("d" " d: Deploy release") ("b" " b: Rollback release") ("i" " i: Insert completion mark"))) :property-runs (("Switches" nil makey-key-mode-header-face) ("\n" nil nil) (" " "v" nil) ("v" "v" makey-key-mode-button-face) (": Verbose logs (--verbose)" "v" nil) ("   " nil nil) (" " "f" nil) ("f" "f" makey-key-mode-button-face) (": Force rollout (--force)" "f" nil) ("\n" nil nil) ("Switches" nil makey-key-mode-header-face) ("\n" nil nil) (" " "t" nil) ("t" "t" makey-key-mode-button-face) (": Trace hooks (makey-test-trace)" "t" nil) ("\n" nil nil) ("Args" nil makey-key-mode-header-face) ("\n" nil nil) (" " "e" nil) ("e" "e" makey-key-mode-button-face) (": Environment (--environment=)" "e" nil) ("\n\n" nil nil) ("Args" nil makey-key-mode-header-face) ("\n" nil nil) (" " "r" nil) ("r" "r" makey-key-mode-button-face) (": Retry count (makey-test-retries)" "r" nil) ("\n\n" nil nil) ("Release Actions" nil makey-key-mode-header-face) ("\n" nil nil) (" " "d" nil) ("d" "d" makey-key-mode-button-face) (": Deploy release" "d" nil) ("     " nil nil) (" " "b" nil) ("b" "b" makey-key-mode-button-face) (": Rollback release" "b" nil) ("\n" nil nil) ("Template Actions" nil makey-key-mode-header-face) ("\n" nil nil) (" " "i" nil) ("i" "i" makey-key-mode-button-face) (": Insert completion mark" "i" nil) ("\n" nil nil)) :bindings (("RET" nil) ("TAB" nil) ("C-g" t) ("q" t) ("?" t) ("d" t) ("b" t) ("i" t) ("v" t) ("f" t) ("t" t) ("e" t) ("r" t))) :closed (:menu-live nil :selected-source t :source-point 24 :point-restored t :windows 1))"##
    ]];
    ParityBatchCase::value(
        "rendered_operations_popup_exposes_exact_content_properties_and_bindings",
        elisp_form,
        expect,
    )
}

fn keyboard_updates_command_and_lisp_options_arguments_and_preserves_focus() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (makey-test-reset)
  (let ((source (generate-new-buffer " *makey-keyboard*"))
        result)
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer source)
          (insert "deployment controls")
          (setq makey-test-inputs '("production east" "5"))
          (makey-key-mode-popup-deployctl)
          (makey-test-goto-key "v")
          (call-interactively
           (lookup-key (current-local-map) (kbd "v")))
          (let ((verbose (makey-test-state)))
            (makey-test-goto-key "t")
            (call-interactively
             (lookup-key (current-local-map) (kbd "t")))
            (let ((trace-enabled (makey-test-state)))
              (makey-test-goto-key "e")
              (call-interactively
               (lookup-key (current-local-map) (kbd "e")))
              (let ((environment (makey-test-state)))
                (makey-test-goto-key "r")
                (call-interactively
                 (lookup-key (current-local-map) (kbd "r")))
                (let ((retries (makey-test-state)))
                  (makey-test-goto-key "t")
                  (call-interactively
                   (lookup-key (current-local-map) (kbd "t")))
                  (setq result
                        (list
                         :verbose verbose
                         :trace-enabled trace-enabled
                         :environment environment
                         :retries retries
                         :trace-disabled (makey-test-state)
                         :prompts (nreverse makey-test-prompts)))))))
          (makey-key-mode-command nil))
      (when (buffer-live-p source)
        (kill-buffer source))
      (makey-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:verbose (:options ("--verbose" (makey-test-trace)) :lisp-options ((makey-test-trace)) :args nil :lisp-args nil :point-key "v" :buttons (("v" " v: Verbose logs (--verbose)") ("f" " f: Force rollout (--force)") ("t" " t: Trace hooks (makey-test-trace)") ("e" " e: Environment (--environment=)") ("r" " r: Retry count (makey-test-retries)") ("d" " d: Deploy release") ("b" " b: Rollback release") ("i" " i: Insert completion mark"))) :trace-enabled (:options ("--verbose" (makey-test-trace)) :lisp-options ((makey-test-trace . t) (makey-test-trace)) :args nil :lisp-args nil :point-key "t" :buttons (("v" " v: Verbose logs (--verbose)") ("f" " f: Force rollout (--force)") ("t" " t: Trace hooks (makey-test-trace)") ("e" " e: Environment (--environment=)") ("r" " r: Retry count (makey-test-retries)") ("d" " d: Deploy release") ("b" " b: Rollback release") ("i" " i: Insert completion mark"))) :environment (:options ("--verbose" (makey-test-trace)) :lisp-options ((makey-test-trace . t) (makey-test-trace)) :args (("--environment=" . "production east")) :lisp-args nil :point-key "e" :buttons (("v" " v: Verbose logs (--verbose)") ("f" " f: Force rollout (--force)") ("t" " t: Trace hooks (makey-test-trace)") ("e" " e: Environment (--environment=) production east") ("r" " r: Retry count (makey-test-retries)") ("d" " d: Deploy release") ("b" " b: Rollback release") ("i" " i: Insert completion mark"))) :retries (:options ("--verbose" (makey-test-trace)) :lisp-options ((makey-test-trace . t) (makey-test-trace)) :args (("--environment=" . "production east")) :lisp-args (("makey-test-retries" . 5)) :point-key "r" :buttons (("v" " v: Verbose logs (--verbose)") ("f" " f: Force rollout (--force)") ("t" " t: Trace hooks (makey-test-trace)") ("e" " e: Environment (--environment=) production east") ("r" " r: Retry count (makey-test-retries) 5") ("d" " d: Deploy release") ("b" " b: Rollback release") ("i" " i: Insert completion mark"))) :trace-disabled (:options ("--verbose" (makey-test-trace)) :lisp-options ((makey-test-trace)) :args (("--environment=" . "production east")) :lisp-args (("makey-test-retries" . 5)) :point-key "t" :buttons (("v" " v: Verbose logs (--verbose)") ("f" " f: Force rollout (--force)") ("t" " t: Trace hooks (makey-test-trace)") ("e" " e: Environment (--environment=) production east") ("r" " r: Retry count (makey-test-retries) 5") ("d" " d: Deploy release") ("b" " b: Rollback release") ("i" " i: Insert completion mark"))) :prompts ("--environment=: " "makey-test-retries: "))"##
    ]];
    ParityBatchCase::value(
        "keyboard_updates_command_and_lisp_options_arguments_and_preserves_focus",
        elisp_form,
        expect,
    )
}

fn deploy_action_receives_cli_options_dynamic_lisp_bindings_prefix_and_caller() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (makey-test-reset)
  (let ((source (generate-new-buffer " *makey-action*"))
        result)
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer source)
          (insert "REL-417\nready\n")
          (goto-char 5)
          (let ((source-point (point))
                (current-prefix-arg '(16)))
            (setq makey-test-inputs '("production east" "7"))
            (makey-key-mode-popup-deployctl)
            (dolist (key '("v" "f" "t" "e" "r"))
              (makey-test-goto-key key)
              (call-interactively
               (lookup-key (current-local-map) (kbd key))))
            (makey-test-goto-key "d")
            (makey-key-mode-exec-at-point)
            (setq result
                  (list
                   :action makey-test-action-log
                   :menu-live
                   (and (get-buffer "*makey-key: deployctl*") t)
                   :source
                   (list :buffer (buffer-name)
                         :point (point)
                         :point-restored (= (point) source-point)
                         :content (buffer-string))
                   :globals-after
                   (list makey-test-trace
                         makey-test-retries)))))
      (when (buffer-live-p source)
        (kill-buffer source))
      (makey-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:action (:kind deploy :custom-options ("--environment=production east" "--force" "--verbose" (makey-test-trace)) :trace t :retries 7 :prefix (16) :buffer " *makey-action*" :point 5 :windows 1) :menu-live nil :source (:buffer " *makey-action*" :point 5 :point-restored t :content "REL-417\nready\n") :globals-after (nil 2))"##
    ]];
    ParityBatchCase::value(
        "deploy_action_receives_cli_options_dynamic_lisp_bindings_prefix_and_caller",
        elisp_form,
        expect,
    )
}

fn literal_action_restores_the_caller_and_matches_gnu_self_insert_behavior() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (makey-test-reset)
  (let ((source (generate-new-buffer " *makey-template*"))
        result)
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer source)
          (insert "deployment complete")
          (let ((source-window (selected-window)))
            (makey-key-mode-popup-deployctl)
            (makey-test-goto-key "i")
            (let ((outcome
                   (condition-case problem
                       (progn
                         (makey-key-mode-exec-at-point)
                         :ok)
                     (error
                      (list (car problem)
                            (error-message-string problem))))))
              (setq result
                    (list
                     :outcome outcome
                     :menu-live
                     (and (get-buffer "*makey-key: deployctl*") t)
                     :selected-source
                     (and (eq (selected-window) source-window)
                          (eq (current-buffer) source))
                     :content (buffer-string)
                     :point (point))))))
      (when (buffer-live-p source)
        (kill-buffer source))
      (makey-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:outcome :ok :menu-live nil :selected-source t :content "deployment complete" :point 20)"##
    ]];
    ParityBatchCase::value(
        "literal_action_restores_the_caller_and_matches_gnu_self_insert_behavior",
        elisp_form,
        expect,
    )
}

fn contextual_help_routes_actions_manuals_and_errors_without_opening_a_popup() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (makey-test-reset)
  (let ((without-man
         '((actions
            ("Local"
             ("d" "Deploy" makey-test-deploy))))))
    (eval
     `(defun makey-key-mode-options-for-local-tool ()
        ',without-man))
    (unwind-protect
        (list
         :action
         (makey-test-help-outcome 'deployctl "d")
         :manual
         (makey-test-help-outcome 'deployctl "?")
         :unknown
         (makey-test-help-outcome 'deployctl "x")
         :missing-manual
         (makey-test-help-outcome 'local-tool "?")
         :definitions
         (mapcar
          (lambda (key)
            (list key
                  (makey-key-mode-key-defined-p
                   'deployctl key)))
          '("d" "v" "t" "e" "r" "x")))
      (fmakunbound 'makey-key-mode-options-for-local-tool))))
"##;
    let expect = expect![[
        r##"OK (:action (:prompt "Enter command prefix, `?' for man `deployctl': " :outcome :ok :events ((:describe makey-test-deploy))) :manual (:prompt "Enter command prefix, `?' for man `deployctl': " :outcome :ok :events ((:man "deployctl"))) :unknown (:prompt "Enter command prefix, `?' for man `deployctl': " :outcome (error "No help associated with ‘x’") :events nil) :missing-manual (:prompt "Enter command prefix: " :outcome (error "No man page associated with ‘local-tool’") :events nil) :definitions (("d" t) ("v" t) ("t" nil) ("e" t) ("r" nil) ("x" nil)))"##
    ]];
    ParityBatchCase::value(
        "contextual_help_routes_actions_manuals_and_errors_without_opening_a_popup",
        elisp_form,
        expect,
    )
}

fn popup_restores_a_two_pane_operations_dashboard_after_cancel() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (makey-test-reset)
  (let ((services (generate-new-buffer " *makey-services*"))
        (events (generate-new-buffer " *makey-events*"))
        result)
    (unwind-protect
        (save-window-excursion
          (delete-other-windows)
          (switch-to-buffer services)
          (insert "api\nworker\nbilling\n")
          (goto-char 6)
          (let ((right (split-window-right)))
            (set-window-buffer right events)
            (with-current-buffer events
              (insert "queued\nrunning\ndone\n")
              (goto-char 16))
            (set-window-point right 16))
          (let ((before (makey-test-window-state)))
            (makey-key-mode-popup-deployctl)
            (let ((opened (makey-test-window-state))
                  (selected
                   (list (buffer-name)
                         (get-text-property
                          (point) 'key-group-executor))))
              (makey-key-mode-command nil)
              (setq result
                    (list
                     :before before
                     :opened opened
                     :selected selected
                     :restored (makey-test-window-state)
                     :selected-after
                     (list (buffer-name) (point)))))))
      (dolist (buffer (list services events))
        (when (buffer-live-p buffer)
          (kill-buffer buffer)))
      (makey-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:before (:count 2 :services (:point 6 :width 40) :events (:point 16 :width 40) :dashboard-side-by-side t :menu nil) :opened (:count 3 :services (:point 6 :width 40) :events (:point 16 :width 40) :dashboard-side-by-side nil :menu (:point 210 :width 40 :left-column-stack t)) :selected ("*makey-key: deployctl*" "d") :restored (:count 2 :services (:point 6 :width 40) :events (:point 16 :width 40) :dashboard-side-by-side t :menu nil) :selected-after (" *makey-services*" 6))"##
    ]];
    ParityBatchCase::value(
        "popup_restores_a_two_pane_operations_dashboard_after_cancel",
        elisp_form,
        expect,
    )
}

#[test]
fn makey_package_batch() {
    let cases = vec![
        generated_group_configuration_supports_safe_runtime_extension_and_removal(),
        rendered_operations_popup_exposes_exact_content_properties_and_bindings(),
        keyboard_updates_command_and_lisp_options_arguments_and_preserves_focus(),
        deploy_action_receives_cli_options_dynamic_lisp_bindings_prefix_and_caller(),
        literal_action_restores_the_caller_and_matches_gnu_self_insert_behavior(),
        contextual_help_routes_actions_manuals_and_errors_without_opening_a_popup(),
        popup_restores_a_two_pane_operations_dashboard_after_cancel(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Makey parity test");
    assert_oracle_batch_cases(makey_oracle(), test_name, "makey_parity", &cases);
}
