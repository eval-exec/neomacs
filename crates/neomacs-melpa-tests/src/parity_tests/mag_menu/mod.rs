use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MAG_MENU_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const MAG_MENU_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const MAG_MENU_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'mag-menu)

(defvar mag-menu-test-action-log nil)
(defvar mag-menu-test-help-events nil)
(defvar mag-menu-test-inputs nil)
(defvar mag-menu-test-history nil)
(defvar mag-menu-test-result nil)

(defun mag-menu-test-normalize-options (options)
  (sort
   (mapcar
    (lambda (option)
      (cons (car option)
            (and (cdr option)
                 (substring-no-properties (cdr option)))))
    options)
   (lambda (left right) (string< (car left) (car right)))))

(defun mag-menu-test-args ()
  (let (args)
    (maphash
     (lambda (name value)
       (push (cons name (substring-no-properties value)) args))
     mag-menu-current-args)
    (sort args (lambda (left right) (string< (car left) (car right))))))

(defun mag-menu-test-action (kind options)
  (setq mag-menu-test-action-log
        (list
         :kind kind
         :options (mag-menu-test-normalize-options options)
         :prefix current-prefix-arg
         :buffer (buffer-name)
         :point (point)
         :windows (length (window-list nil 'nomini)))))

(defun mag-menu-test-deploy (options)
  (mag-menu-test-action 'deploy options))

(defun mag-menu-test-rollback (options)
  (mag-menu-test-action 'rollback options))

(defun mag-menu-test-exclusive-switch (option-name options)
  (setq options (mag-menu-remove-option options "--force"))
  (setq options (mag-menu-remove-option options "--dry-run"))
  (mag-menu-set-option options option-name nil))

(defun mag-menu-test-read-argument
    (option-name options history-variable)
  (let ((value (pop mag-menu-test-inputs)))
    (when (and value (> (length value) 0))
      (set history-variable
           (cons value (symbol-value history-variable))))
    (if (or (null value) (= (length value) 0))
        (mag-menu-remove-option options option-name)
      (mag-menu-set-option options option-name value))))

(defconst mag-menu-test-group
  '(deployctl
    (man-page "deployctl")
    (actions
     ("d" "Deploy release" mag-menu-test-deploy)
     ("r" "Rollback release" mag-menu-test-rollback))
    (switches
     ("v" "Verbose logs" "--verbose" nil)
     ("f" "Force rollout" "--force" mag-menu-test-exclusive-switch)
     ("n" "Dry run" "--dry-run" mag-menu-test-exclusive-switch))
    (arguments
     ("e" "Environment" "--environment=" mag-menu-test-read-argument
      mag-menu-test-history)
     ("o" "Owner" "--owner=" mag-menu-test-read-argument
      mag-menu-test-history))))

(defun mag-menu-test-reset ()
  (when-let ((buffer (get-buffer mag-menu-buf-name)))
    (kill-buffer buffer))
  (delete-other-windows)
  (when-let ((scratch (get-buffer "*scratch*")))
    (switch-to-buffer scratch))
  (setq mag-menu-current-args nil
        mag-menu-current-options nil
        mag-menu-previous-window-config nil
        mag-menu-key-maps nil
        mag-menu-prefix nil
        mag-menu-test-action-log nil
        mag-menu-test-help-events nil
        mag-menu-test-inputs nil
        mag-menu-test-history nil
        mag-menu-use-splitter-shrink nil
        mag-menu-args-in-cols nil))

(defun mag-menu-test-goto-key (key)
  (goto-char (point-min))
  (let ((position
         (cdr (assoc key (mag-menu-build-exec-point-alist)))))
    (unless position
      (error "No rendered menu key %s" key))
    (goto-char position)
    (skip-chars-forward " ")
    (point)))

(defun mag-menu-test-button-text (key)
  (save-excursion
    (mag-menu-test-goto-key key)
    (let ((start
           (or (previous-single-property-change
                (1+ (point)) 'key-group-executor)
               (point-min)))
          (end
           (or (next-single-property-change
                (point) 'key-group-executor)
               (point-max))))
      (buffer-substring-no-properties start end))))

(defun mag-menu-test-button-state ()
  (list
   :options mag-menu-current-options
   :args (mag-menu-test-args)
   :point-key (get-text-property (point) 'key-group-executor)
   :buttons
   (mapcar
    (lambda (key)
      (list key (mag-menu-test-button-text key)))
    '("v" "f" "n" "e" "o"))))

(defun mag-menu-test-property-runs ()
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

(defun mag-menu-test-window-state ()
  (let* ((services-buffer (get-buffer " *mag-menu-services*"))
         (events-buffer (get-buffer " *mag-menu-events*"))
         (menu-buffer (get-buffer mag-menu-buf-name))
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
     :side-by-side
     (and services events
          (= (cadr services-edges) (cadr events-edges))
          (= (nth 3 services-edges) (nth 3 events-edges))
          (= (nth 2 services-edges) (car events-edges)))
     :menu
     (and menu
          (list
           :point (window-point menu)
           :width (window-total-width menu)
           :below-top-panes
           (and services events
                (= (cadr menu-edges) (nth 3 services-edges))
                (= (cadr menu-edges) (nth 3 events-edges)))
           :spans-top-panes
           (and services events
                (= (car menu-edges) (car services-edges))
                (= (nth 2 menu-edges) (nth 2 events-edges))))))))

(defun mag-menu-test-help-outcome (group key)
  (setq mag-menu-test-help-events nil)
  (let (prompt)
    (cl-letf (((symbol-function 'read-key-sequence)
               (lambda (message)
                 (setq prompt message)
                 key))
              ((symbol-function 'describe-function)
               (lambda (function)
                 (push (list :describe function)
                       mag-menu-test-help-events)))
              ((symbol-function 'man)
               (lambda (page)
                 (push (list :man page)
                       mag-menu-test-help-events))))
      (let ((outcome
             (condition-case problem
                 (progn
                   (mag-menu-help group)
                   :ok)
               (error
                (list (car problem)
                      (error-message-string problem))))))
        (list :prompt prompt
              :outcome outcome
              :events (nreverse mag-menu-test-help-events))))))
"##;

fn mag_menu_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MAG_MENU_MELPA_PIN, "mag-menu.el")
        .expect("prepare pinned Mag Menu source below ./tmp")
        .with_prelude(MAG_MENU_TEST_PRELUDE)
        .with_timeout(MAG_MENU_TEST_TIMEOUT)
}

fn deployment_options_round_trip_without_mutating_the_callers_defaults() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (mag-menu-test-reset)
  (let* ((defaults
          '(("--verbose")
            ("--environment" . "staging west")
            ("--owner" . "Anaïs")))
         (defaults-before (copy-tree defaults))
         (with-timeout
          (mag-menu-set-option defaults "--timeout" "30 seconds"))
         (updated-owner
          (mag-menu-set-option with-timeout "--owner" "Béa Ops"))
         (without-verbose
          (mag-menu-toggle-switch updated-owner "--verbose"))
         (with-dry-run
          (mag-menu-toggle-switch without-verbose "--dry-run"))
         (parts
          (mag-menu-extract-switches-and-args with-dry-run))
         (switches (car parts))
         (args (cadr parts))
         (round-tripped
          (mag-menu-form-options-alist switches args))
         generic-prompts
         directory-prompts)
    (list
     :defaults-unchanged (equal defaults defaults-before)
     :stages
     (mapcar
      #'mag-menu-test-normalize-options
      (list with-timeout updated-owner without-verbose with-dry-run))
     :switches switches
     :args
     (let (entries)
       (maphash (lambda (key value) (push (cons key value) entries))
                args)
       (sort entries
             (lambda (left right)
               (string< (car left) (car right)))))
     :round-trip
     (mag-menu-test-normalize-options round-tripped)
     :readers
     (cl-letf
         (((symbol-function 'read-from-minibuffer)
           (lambda (prompt &rest _)
             (push prompt generic-prompts)
             "production east"))
          ((symbol-function 'read-directory-name)
           (lambda (prompt &rest _)
             (push prompt directory-prompts)
             "./deploy root/")))
       (list
        (mag-menu-test-normalize-options
         (mag-menu-read-generic
          "--environment" defaults 'mag-menu-test-history))
        (mag-menu-test-normalize-options
         (mag-menu-read-directory-name
          "--root" defaults 'mag-menu-test-history))
        :generic-prompts (nreverse generic-prompts)
        :directory-prompts (nreverse directory-prompts))))))
"##;
    let expect = expect![[
        r##"OK (:defaults-unchanged t :stages ((("--environment" . "staging west") ("--owner" . "Anaïs") ("--timeout" . "30 seconds") ("--verbose")) (("--environment" . "staging west") ("--owner" . "Béa Ops") ("--timeout" . "30 seconds") ("--verbose")) (("--environment" . "staging west") ("--owner" . "Béa Ops") ("--timeout" . "30 seconds")) (("--dry-run") ("--environment" . "staging west") ("--owner" . "Béa Ops") ("--timeout" . "30 seconds"))) :switches ("--dry-run") :args (("--environment=" . "staging west") ("--owner=" . "Béa Ops") ("--timeout=" . "30 seconds")) :round-trip (("--dry-run") ("--environment" . "staging west") ("--owner" . "Béa Ops") ("--timeout" . "30 seconds")) :readers ((("--environment" . "production east") ("--owner" . "Anaïs") ("--verbose")) (("--environment" . "staging west") ("--owner" . "Anaïs") ("--root" . "./deploy root/") ("--verbose")) :generic-prompts ("--environment: ") :directory-prompts ("--root: ")))"##
    ]];
    ParityBatchCase::value(
        "deployment_options_round_trip_without_mutating_the_callers_defaults",
        elisp_form,
        expect,
    )
}

fn rendered_menu_exposes_exact_buttons_state_and_restores_the_calling_window() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (mag-menu-test-reset)
  (let ((source (generate-new-buffer " *mag-menu-release*"))
        result)
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer source)
          (insert "release dashboard\nREL-417 ready\n")
          (goto-char 24)
          (let ((source-point (point))
                (source-window (selected-window)))
            (mag-menu
             mag-menu-test-group
             '(("--verbose")
               ("--environment" . "staging west")
               ("--owner" . "Anaïs")))
            (let* ((menu (current-buffer))
                   (map (current-local-map))
                   (opened
                    (list
                     :selected-menu
                     (and (eq menu (get-buffer mag-menu-buf-name)) t)
                     :windows (length (window-list nil 'nomini))
                     :mode major-mode
                     :mode-name mode-name
                     :read-only buffer-read-only
                     :content
                     (buffer-substring-no-properties
                      (point-min) (point-max))
                     :state (mag-menu-test-button-state)
                     :property-runs (mag-menu-test-property-runs)
                     :bindings
                     (mapcar
                      (lambda (key)
                        (list key
                              (commandp (lookup-key map (kbd key)))))
                      '("RET" "TAB" "q" "?" "d" "r"
                        "v" "f" "n" "e" "o")))))
              (mag-menu-command nil)
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
      (mag-menu-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:opened (:selected-menu t :windows 2 :mode mag-menu-mode :mode-name "mag-menu-mode" :read-only t :content "Switches\n v: Verbose logs (--verbose)    f: Force rollout (--force)\n n: Dry run (--dry-run)\nArgs\n e: Environment (--environment=) staging west\n o: Owner (--owner=) Anaïs\n\nActions\n d: Deploy release      r: Rollback release\n" :state (:options ("--verbose") :args (("--environment=" . "staging west") ("--owner=" . "Anaïs")) :point-key "d" :buttons (("v" " v: Verbose logs (--verbose)") ("f" " f: Force rollout (--force)") ("n" " n: Dry run (--dry-run)") ("e" " e: Environment (--environment=) staging west") ("o" " o: Owner (--owner=) Anaïs"))) :property-runs (("Switches" nil font-lock-keyword-face) ("\n" nil nil) (" " "v" nil) ("v" "v" font-lock-builtin-face) (": Verbose logs (" "v" nil) ("--verbose" "v" font-lock-warning-face) (")" "v" nil) ("   " nil nil) (" " "f" nil) ("f" "f" font-lock-builtin-face) (": Force rollout (--force)" "f" nil) ("\n" nil nil) (" " "n" nil) ("n" "n" font-lock-builtin-face) (": Dry run (--dry-run)" "n" nil) ("\n" nil nil) ("Args" nil font-lock-keyword-face) ("\n" nil nil) (" " "e" nil) ("e" "e" font-lock-builtin-face) (": Environment (--environment=) " "e" nil) ("staging west" "e" widget-field) ("\n" nil nil) (" " "o" nil) ("o" "o" font-lock-builtin-face) (": Owner (--owner=) " "o" nil) ("Anaïs" "o" widget-field) ("\n\n" nil nil) ("Actions" nil font-lock-keyword-face) ("\n" nil nil) (" " "d" nil) ("d" "d" font-lock-builtin-face) (": Deploy release" "d" nil) ("     " nil nil) (" " "r" nil) ("r" "r" font-lock-builtin-face) (": Rollback release" "r" nil) ("\n" nil nil)) :bindings (("RET" t) ("TAB" t) ("q" t) ("?" t) ("d" t) ("r" t) ("v" t) ("f" t) ("n" t) ("e" t) ("o" t))) :closed (:menu-live nil :selected-source t :source-point 24 :point-restored t :windows 1))"##
    ]];
    ParityBatchCase::value(
        "rendered_menu_exposes_exact_buttons_state_and_restores_the_calling_window",
        elisp_form,
        expect,
    )
}

fn keyboard_switches_and_arguments_redraw_in_place_with_mutual_exclusion() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (mag-menu-test-reset)
  (let ((source (generate-new-buffer " *mag-menu-keyboard*")))
    (setq mag-menu-test-result nil)
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer source)
          (insert "keyboard deployment")
          (mag-menu
           mag-menu-test-group
           '(("--verbose") ("--environment" . "staging")))
          (mag-menu-test-goto-key "f")
          (call-interactively
           (lookup-key (current-local-map) (kbd "f")))
          (let ((forced (mag-menu-test-button-state)))
            (mag-menu-test-goto-key "n")
            (call-interactively
             (lookup-key (current-local-map) (kbd "n")))
            (let ((dry-run (mag-menu-test-button-state)))
              (setq mag-menu-test-inputs
                    '("production east" ""))
              (mag-menu-test-goto-key "e")
              (call-interactively
               (lookup-key (current-local-map) (kbd "e")))
              (let ((environment-set
                     (mag-menu-test-button-state)))
                (call-interactively
                 (lookup-key (current-local-map) (kbd "e")))
                (setq mag-menu-test-result
                      (list
                       :forced forced
                       :dry-run dry-run
                       :environment-set environment-set
                       :environment-cleared
                       (mag-menu-test-button-state)
                       :history mag-menu-test-history))))))
      (when (buffer-live-p source)
        (kill-buffer source))
      (mag-menu-test-reset))
    mag-menu-test-result))
"##;
    let expect = expect![[
        r##"OK (:forced (:options ("--verbose" "--force") :args (("--environment=" . "staging")) :point-key "f" :buttons (("v" " v: Verbose logs (--verbose)") ("f" " f: Force rollout (--force)") ("n" " n: Dry run (--dry-run)") ("e" " e: Environment (--environment=) staging") ("o" " o: Owner (--owner=)"))) :dry-run (:options ("--verbose" "--dry-run") :args (("--environment=" . "staging")) :point-key "n" :buttons (("v" " v: Verbose logs (--verbose)") ("f" " f: Force rollout (--force)") ("n" " n: Dry run (--dry-run)") ("e" " e: Environment (--environment=) staging") ("o" " o: Owner (--owner=)"))) :environment-set (:options ("--verbose" "--dry-run") :args (("--environment=" . "production east")) :point-key "e" :buttons (("v" " v: Verbose logs (--verbose)") ("f" " f: Force rollout (--force)") ("n" " n: Dry run (--dry-run)") ("e" " e: Environment (--environment=) production east") ("o" " o: Owner (--owner=)"))) :environment-cleared (:options ("--verbose" "--dry-run") :args nil :point-key "e" :buttons (("v" " v: Verbose logs (--verbose)") ("f" " f: Force rollout (--force)") ("n" " n: Dry run (--dry-run)") ("e" " e: Environment (--environment=)") ("o" " o: Owner (--owner=)"))) :history ("production east"))"##
    ]];
    ParityBatchCase::value(
        "keyboard_switches_and_arguments_redraw_in_place_with_mutual_exclusion",
        elisp_form,
        expect,
    )
}

fn tab_navigation_wraps_across_every_button_and_ret_dispatches_the_action() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (mag-menu-test-reset)
  (let ((source (generate-new-buffer " *mag-menu-navigation*"))
        result)
    (unwind-protect
        (save-window-excursion
          (switch-to-buffer source)
          (insert "REL-417\nready\n")
          (goto-char 5)
          (let ((source-point (point))
                (current-prefix-arg '(16)))
            (mag-menu
             mag-menu-test-group
             '(("--dry-run")
               ("--environment" . "staging")))
            (let ((positions
                   (mapcar
                    (lambda (entry)
                      (cons (car entry) (cdr entry)))
                    (mag-menu-build-exec-point-alist)))
                  navigation)
              (dotimes (_ 9)
                (push
                 (list
                  (get-text-property
                   (point) 'key-group-executor)
                  (point))
                 navigation)
                (mag-menu-jump-to-next-exec))
              (mag-menu-test-goto-key "r")
              (mag-menu-exec-at-point)
              (setq result
                    (list
                     :positions positions
                     :navigation (nreverse navigation)
                     :action mag-menu-test-action-log
                     :menu-live
                     (and (get-buffer mag-menu-buf-name) t)
                     :source
                     (list :buffer (buffer-name)
                           :point (point)
                           :point-restored
                           (= (point) source-point)))))))
      (when (buffer-live-p source)
        (kill-buffer source))
      (mag-menu-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:positions (("v" . 10) ("f" . 41) ("n" . 69) ("e" . 98) ("o" . 139) ("d" . 169) ("r" . 192)) :navigation (("d" 170) ("r" 193) ("v" 11) ("f" 42) ("n" 70) ("e" 99) ("o" 140) ("d" 170) ("r" 193)) :action (:kind rollback :options (("--dry-run") ("--environment" . "staging")) :prefix (16) :buffer " *mag-menu-navigation*" :point 5 :windows 1) :menu-live nil :source (:buffer " *mag-menu-navigation*" :point 5 :point-restored t))"##
    ]];
    ParityBatchCase::value(
        "tab_navigation_wraps_across_every_button_and_ret_dispatches_the_action",
        elisp_form,
        expect,
    )
}

fn contextual_help_routes_actions_manuals_and_errors_without_mutating_menu_state() -> ParityBatchCase
{
    let elisp_form = r##"
(progn
  (mag-menu-test-reset)
  (let ((without-man
         '(local-tool
           (actions
            ("d" "Deploy" mag-menu-test-deploy)))))
    (list
     :action
     (mag-menu-test-help-outcome mag-menu-test-group "d")
     :manual
     (mag-menu-test-help-outcome mag-menu-test-group "?")
     :unknown
     (mag-menu-test-help-outcome mag-menu-test-group "x")
     :missing-manual
     (mag-menu-test-help-outcome without-man "?")
     :definitions
     (mapcar
      (lambda (key)
        (list key
              (mag-menu-key-defined-p
               mag-menu-test-group key)))
      '("d" "v" "e" "x")))))
"##;
    let expect = expect![[
        r##"OK (:action (:prompt "Enter command prefix, `?' for man `deployctl': " :outcome :ok :events ((:describe mag-menu-test-deploy))) :manual (:prompt "Enter command prefix, `?' for man `deployctl': " :outcome :ok :events ((:man "deployctl"))) :unknown (:prompt "Enter command prefix, `?' for man `deployctl': " :outcome (error "No help associated with ‘x’") :events nil) :missing-manual (:prompt "Enter command prefix: " :outcome (error "No man page associated with ‘local-tool’") :events nil) :definitions (("d" t) ("v" t) ("e" t) ("x" nil)))"##
    ]];
    ParityBatchCase::value(
        "contextual_help_routes_actions_manuals_and_errors_without_mutating_menu_state",
        elisp_form,
        expect,
    )
}

fn splitter_popup_preserves_a_two_pane_dashboard_and_restores_it_after_cancel() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (mag-menu-test-reset)
  (let ((services (generate-new-buffer " *mag-menu-services*"))
        (events (generate-new-buffer " *mag-menu-events*"))
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
          (let ((before (mag-menu-test-window-state))
                (mag-menu-use-splitter-shrink t))
            (mag-menu
             mag-menu-test-group
             '(("--environment" . "production")))
            (let ((opened (mag-menu-test-window-state))
                  (selected
                   (list (buffer-name)
                         (get-text-property
                          (point) 'key-group-executor))))
              (mag-menu-command nil)
              (setq result
                    (list
                     :before before
                     :opened opened
                     :selected selected
                     :restored (mag-menu-test-window-state)
                     :selected-after
                     (list (buffer-name) (point)))))))
      (dolist (buffer (list services events))
        (when (buffer-live-p buffer)
          (kill-buffer buffer)))
      (mag-menu-test-reset))
    result))
"##;
    let expect = expect![[
        r##"OK (:before (:count 2 :services (:point 6 :width 40) :events (:point 16 :width 40) :side-by-side t :menu nil) :opened (:count 3 :services (:point 6 :width 40) :events (:point 16 :width 40) :side-by-side t :menu (:point 173 :width 80 :below-top-panes t :spans-top-panes t)) :selected ("*mag-menu*" "d") :restored (:count 2 :services (:point 6 :width 40) :events (:point 16 :width 40) :side-by-side t :menu nil) :selected-after (" *mag-menu-services*" 6))"##
    ]];
    ParityBatchCase::value(
        "splitter_popup_preserves_a_two_pane_dashboard_and_restores_it_after_cancel",
        elisp_form,
        expect,
    )
}

#[test]
fn mag_menu_package_batch() {
    let cases = vec![
        deployment_options_round_trip_without_mutating_the_callers_defaults(),
        rendered_menu_exposes_exact_buttons_state_and_restores_the_calling_window(),
        keyboard_switches_and_arguments_redraw_in_place_with_mutual_exclusion(),
        tab_navigation_wraps_across_every_button_and_ret_dispatches_the_action(),
        contextual_help_routes_actions_manuals_and_errors_without_mutating_menu_state(),
        splitter_popup_preserves_a_two_pane_dashboard_and_restores_it_after_cancel(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Mag Menu parity test");
    assert_oracle_batch_cases(mag_menu_oracle(), test_name, "mag_menu_parity", &cases);
}
