use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HYDRA_MELPA_PIN, LV_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HYDRA_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const HYDRA_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'hydra)
(require 'imenu)

(setq hydra-hint-display-type 'message)

(defvar hydra-test-mode-map (make-sparse-keymap))
(defvar hydra-test-orders ["ORD-417" "ORD-418" "ORD-419" "ORD-420"])
(defvar hydra-test-position 0)
(defvar hydra-test-events nil)

(defun hydra-test-current-order ()
  (aref hydra-test-orders hydra-test-position))

(defun hydra-test-next-order (count)
  (interactive "p")
  (setq hydra-test-position
        (mod (+ hydra-test-position count) (length hydra-test-orders)))
  (push (list :next count (hydra-test-current-order)) hydra-test-events))

(defun hydra-test-previous-order (count)
  (interactive "p")
  (setq hydra-test-position
        (mod (- hydra-test-position count) (length hydra-test-orders)))
  (push (list :previous count (hydra-test-current-order)) hydra-test-events))

(defun hydra-test-retry-order (count)
  (interactive "p")
  (push (list :retry count (hydra-test-current-order)) hydra-test-events))

(defhydra hydra-test-order-navigation
  (hydra-test-mode-map "C-c o"
   :color pink
   :hint nil
   :pre (push (list :pre (hydra-test-current-order)) hydra-test-events)
   :post (push (list :post (hydra-test-current-order)) hydra-test-events))
  ("n" hydra-test-next-order "next")
  ("p" hydra-test-previous-order "previous")
  ("r" hydra-test-retry-order "retry")
  ("q" nil "quit" :color blue))

(defvar hydra-test-environment "staging")
(defvar hydra-test-dry-run t)
(defvar hydra-test-deploy-log nil)

(defun hydra-test-stage ()
  (interactive)
  (push (list :stage hydra-test-environment hydra-test-dry-run)
        hydra-test-deploy-log))

(defun hydra-test-verify ()
  (interactive)
  (push (list :verify hydra-test-environment) hydra-test-deploy-log))

(defun hydra-test-release ()
  (interactive)
  (push (list :release hydra-test-environment hydra-test-dry-run)
        hydra-test-deploy-log))

(defhydra hydra-test-deploy (:color pink)
  "
Deploy %(upcase hydra-test-environment)
_s_: stage   _v_: verify
Dry run: %`hydra-test-dry-run
_q_: quit
"
  ("s" hydra-test-stage "stage")
  ("v" hydra-test-verify "verify")
  ("q" nil "quit"))

(defhydra+ hydra-test-deploy ()
  ("d" hydra-test-release "release" :color blue))

(defhydradio hydra-test-view ()
  (sort "Sort orders" [recent priority owner])
  (scope "Visible scope" [project workspace all]))

(defun hydra-test-faced-characters (string)
  (cl-loop for index from 0 below (length string)
           for face = (get-text-property index 'face string)
           when face
           collect (list (substring-no-properties string index (1+ index))
                         face)))

(defun hydra-test-face-at (text)
  (goto-char (point-min))
  (search-forward text)
  (get-text-property (match-beginning 0) 'face))
"##;

fn hydra_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HYDRA_MELPA_PIN, "hydra.el")
        .expect("prepare pinned Hydra source below ./tmp")
        .with_melpa_dependency(LV_MELPA_PIN)
        .expect("prepare pinned lv dependency")
        .with_prelude(HYDRA_TEST_PRELUDE)
        .with_timeout(HYDRA_TEST_TIMEOUT)
}

fn command_family_generates_exact_heads_bindings_and_color_semantics() -> ParityBatchCase {
    let elisp_form = r##"
(list
 :params hydra-test-order-navigation/params
 :heads hydra-test-order-navigation/heads
 :prefix-bindings
 (mapcar
  (lambda (key)
    (list key (lookup-key hydra-test-mode-map (kbd (concat "C-c o " key)))))
  '("n" "p" "r" "q"))
 :transient-bindings
 (mapcar
  (lambda (key)
    (list key (lookup-key hydra-test-order-navigation/keymap (kbd key))))
  '("n" "p" "r" "q" "3" "-" "C-u"))
 :commands
 (mapcar #'commandp
         '(hydra-test-order-navigation/body
           hydra-test-order-navigation/hydra-test-next-order
           hydra-test-order-navigation/hydra-test-previous-order
           hydra-test-order-navigation/hydra-test-retry-order
           hydra-test-order-navigation/nil)))
"##;
    let expect = expect![[
        r##"OK (:params (hydra-test-mode-map "C-c o" :exit nil :foreign-keys run :hint nil :post (push (list :post (hydra-test-current-order)) hydra-test-events) :pre (push (list :pre (hydra-test-current-order)) hydra-test-events)) :heads (("n" hydra-test-next-order "next" :exit nil) ("p" hydra-test-previous-order "previous" :exit nil) ("r" hydra-test-retry-order "retry" :exit nil) ("q" nil "quit" :exit t)) :prefix-bindings (("n" hydra-test-order-navigation/hydra-test-next-order) ("p" hydra-test-order-navigation/hydra-test-previous-order) ("r" hydra-test-order-navigation/hydra-test-retry-order) ("q" nil)) :transient-bindings (("n" hydra-test-order-navigation/hydra-test-next-order) ("p" hydra-test-order-navigation/hydra-test-previous-order) ("r" hydra-test-order-navigation/hydra-test-retry-order) ("q" hydra-test-order-navigation/nil) ("3" hydra--digit-argument) ("-" hydra--negative-argument) ("C-u" hydra--universal-argument)) :commands (t t t t t))"##
    ]];
    ParityBatchCase::value(
        "command_family_generates_exact_heads_bindings_and_color_semantics",
        elisp_form,
        expect,
    )
}

fn keyboard_workflow_keeps_short_bindings_applies_prefix_and_runs_exit_hook() -> ParityBatchCase {
    let elisp_form = r##"
(let ((buffer (generate-new-buffer " *hydra-order-workflow*")))
  (unwind-protect
      (save-window-excursion
        (switch-to-buffer buffer)
        (use-local-map hydra-test-mode-map)
        (setq hydra-test-position 0
              hydra-test-events nil)
        (hydra-keyboard-quit)
        (execute-kbd-macro (kbd "C-c o n n p 3 r q"))
        (list
         :position hydra-test-position
         :selected (hydra-test-current-order)
         :events (nreverse hydra-test-events)
         :lifecycle
         (list hydra-curr-map
               hydra-curr-on-exit
               (memq 'hydra--clearfun pre-command-hook)
               (memq hydra-test-order-navigation/keymap
                     overriding-terminal-local-map))))
    (hydra-keyboard-quit)
    (when (buffer-live-p buffer) (kill-buffer buffer))))
"##;
    let expect = expect![[
        r##"OK (:position 1 :selected "ORD-418" :events ((:pre "ORD-417") (:next 1 "ORD-418") (:pre "ORD-418") (:next 1 "ORD-419") (:pre "ORD-419") (:previous 1 "ORD-418") (:pre "ORD-418") (:retry 3 "ORD-418") (:pre "ORD-418") (:post "ORD-418")) :lifecycle (nil nil nil nil))"##
    ]];
    ParityBatchCase::value(
        "keyboard_workflow_keeps_short_bindings_applies_prefix_and_runs_exit_hook",
        elisp_form,
        expect,
    )
}

fn extended_deployment_hydra_updates_dynamic_hint_and_executes_added_head() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((hydra-test-environment "staging")
       (hydra-test-dry-run t)
       (initial-hint (eval hydra-test-deploy/hint t)))
  (setq hydra-test-deploy-log nil)
  (hydra-test-deploy/body)
  (execute-kbd-macro (kbd "s v d"))
  (let* ((hydra-test-environment "production")
         (hydra-test-dry-run nil)
         (production-hint (eval hydra-test-deploy/hint t)))
    (list
     :heads hydra-test-deploy/heads
     :bindings
     (mapcar
      (lambda (key)
        (list key (lookup-key hydra-test-deploy/keymap (kbd key))))
      '("s" "v" "d" "q"))
     :initial-hint (substring-no-properties initial-hint)
     :production-hint (substring-no-properties production-hint)
     :faced-characters (hydra-test-faced-characters production-hint)
     :log (nreverse hydra-test-deploy-log)
     :exited (null hydra-curr-map))))
"##;
    let expect = expect![[
        r##"OK (:heads (("s" hydra-test-stage "stage" :exit nil) ("v" hydra-test-verify "verify" :exit nil) ("q" nil "quit" :exit t) ("d" hydra-test-release "release" :exit t)) :bindings (("s" hydra-test-deploy/hydra-test-stage) ("v" hydra-test-deploy/hydra-test-verify) ("d" hydra-test-deploy/hydra-test-release-and-exit) ("q" hydra-test-deploy/nil)) :initial-hint "Deploy \"STAGING\"\ns: stage   v: verify\nDry run: t\nq: quit\n[s]: stage, [v]: verify, [q]: quit, [d]: release." :production-hint "Deploy \"PRODUCTION\"\ns: stage   v: verify\nDry run: nil\nq: quit\n[s]: stage, [v]: verify, [q]: quit, [d]: release." :faced-characters (("s" hydra-face-pink) ("v" hydra-face-pink) ("q" hydra-face-blue) ("s" hydra-face-pink) ("v" hydra-face-pink) ("q" hydra-face-blue) ("d" hydra-face-blue)) :log ((:stage "staging" t) (:verify "staging") (:release "staging" t)) :exited t)"##
    ]];
    ParityBatchCase::value(
        "extended_deployment_hydra_updates_dynamic_hint_and_executes_added_head",
        elisp_form,
        expect,
    )
}

fn radio_preferences_cycle_ranges_wrap_and_reject_unknown_state() -> ParityBatchCase {
    let elisp_form = r##"
(let ((hydra-test-view/sort 'recent)
      (hydra-test-view/scope 'project))
  (list
   :names hydra-test-view/names
   :ranges
   (mapcar (lambda (name) (get name 'range)) hydra-test-view/names)
   :sort
   (cl-loop repeat 4
            collect (progn (hydra-test-view/sort)
                           hydra-test-view/sort))
   :scope
   (cl-loop repeat 3
            collect (progn (hydra-test-view/scope)
                           hydra-test-view/scope))
   :invalid
   (let ((hydra-test-view/sort 'unknown))
     (condition-case err
         (progn (hydra-test-view/sort) :not-signaled)
       (error (list (car err) (error-message-string err)))))))
"##;
    let expect = expect![[
        r##"OK (:names (hydra-test-view/sort hydra-test-view/scope) :ranges ([recent priority owner] [project workspace all]) :sort (priority owner recent priority) :scope (workspace all project) :invalid (error "Val not in range for hydra-test-view/sort"))"##
    ]];
    ParityBatchCase::value(
        "radio_preferences_cycle_ranges_wrap_and_reject_unknown_state",
        elisp_form,
        expect,
    )
}

fn elisp_source_integration_fontifies_and_indexes_hydra_definitions() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert
   ";;; checkout-commands.el --- Checkout command families\n\n"
   "(defhydra hydra-checkout ()\n"
   "  (\"s\" checkout-submit \"submit\"))\n\n"
   "(defhydradio checkout-view ()\n"
   "  (scope \"Scope\" [project all]))\n\n"
   "(defhydra hydra-refunds (:color blue)\n"
   "  (\"r\" refund-order \"refund\"))\n")
  (emacs-lisp-mode)
  (hydra-add-font-lock)
  (hydra-add-imenu)
  (font-lock-flush (point-min) (point-max))
  (font-lock-ensure (point-min) (point-max))
  (let* ((index (imenu--make-index-alist t))
         (hydras (cdr (assoc "Hydras" index))))
    (list
     :faces
     (list (hydra-test-face-at "defhydra")
           (hydra-test-face-at "hydra-checkout")
           (hydra-test-face-at "defhydradio")
           (hydra-test-face-at "checkout-view"))
     :hydras
     (mapcar
      (lambda (entry)
        (list (car entry) (line-number-at-pos (cdr entry))))
      hydras))))
"##;
    let expect = expect![[
        r##"OK (:faces (font-lock-keyword-face font-lock-type-face font-lock-keyword-face font-lock-type-face) :hydras (("hydra-checkout" 3) ("hydra-refunds" 9)))"##
    ]];
    ParityBatchCase::value(
        "elisp_source_integration_fontifies_and_indexes_hydra_definitions",
        elisp_form,
        expect,
    )
}

#[test]
fn hydra_package_batch() {
    let cases = vec![
        command_family_generates_exact_heads_bindings_and_color_semantics(),
        keyboard_workflow_keeps_short_bindings_applies_prefix_and_runs_exit_hook(),
        extended_deployment_hydra_updates_dynamic_hint_and_executes_added_head(),
        radio_preferences_cycle_ranges_wrap_and_reject_unknown_state(),
        elisp_source_integration_fontifies_and_indexes_hydra_definitions(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Hydra parity test");
    assert_oracle_batch_cases(hydra_oracle(), test_name, "hydra_parity", &cases);
}
