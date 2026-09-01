//! Practical parity for Ivy Hydra's documented minibuffer menu.
//!
//! The cases enter the real `hydra-ivy/body` command from an Ivy minibuffer,
//! drive navigation and option heads, cycle and dispatch real Ivy actions,
//! export a filtered session to Ivy Occur, and follow the menu's documented
//! source-definition action.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HYDRA_MELPA_PIN, IVY_HYDRA_MELPA_PIN, IVY_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'ivy-hydra)

(defconst ih410-test-source-manifest
  '(("ivy-hydra-pkg.el" . "8f64889fa1bae9a3064ad9b27db1afff560de271bae8af72b6fa67ec8b06ca94")
    ("ivy-hydra.el" . "58afeb6059494eff420bd99d8515628f1d927438c71a0c0f4520f29d241234b8")))

(defun ih410-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let* ((loaded (symbol-file 'hydra-ivy/body 'defun))
       (directory (and loaded (file-name-directory loaded)))
       (payload
        (and directory
             (sort
              (seq-filter
               (lambda (name)
                 (and (string-prefix-p "ivy-hydra" name)
                      (string-suffix-p ".el" name)
                      (not (string-suffix-p "-autoloads.el" name))))
               (directory-files directory nil nil t))
              #'string<))))
  (unless
      (and (file-regular-p loaded)
           (not (file-symlink-p loaded))
           (equal payload (mapcar #'car ih410-test-source-manifest))
           (cl-every
            (lambda (entry)
              (let ((file (expand-file-name (car entry) directory)))
                (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (ih410-test-file-sha256 file) (cdr entry)))))
            ih410-test-source-manifest))
    (error "Unexpected installed Ivy Hydra payload: %S" (list loaded payload))))

(defvar ih410-test-expression nil)
(defvar ih410-test-result nil)
(defvar ih410-test-action-events nil)

(defun ih410-test-evaluate-expression ()
  (interactive)
  (setq ih410-test-result (eval ih410-test-expression t)))

(defun ih410-test-copy-candidate (candidate)
  (if (stringp candidate)
      (substring-no-properties candidate)
    (copy-tree candidate)))

(defun ih410-test-action-state (kind candidate)
  (push
   (list :kind kind
         :candidate (ih410-test-copy-candidate candidate)
         :index ivy--index
         :height ivy-height
         :max-height max-mini-window-height
         :truncate truncate-lines
         :case-fold ivy-case-fold-search
         :matcher (ivy--matcher-desc)
         :calling ivy-calling
         :action (ivy-action-name))
   ih410-test-action-events))

(defun ih410-test-open (candidate)
  (ih410-test-action-state 'open candidate))

(defun ih410-test-escalate (candidate)
  (ih410-test-action-state 'escalate candidate))

(defun ih410-test-archive (candidate)
  (ih410-test-action-state 'archive candidate))

(defun ih410-test-minibuffer-state ()
  (interactive)
  (unless (minibufferp)
    (error "Ivy Hydra state probe escaped the minibuffer"))
  (ih410-test-action-state 'minibuffer (ivy-state-current ivy-last)))

(defun ih410-test-window-state ()
  (mapcar
   (lambda (window)
     (list (buffer-name (window-buffer window))
           (window-point window)
           (window-start window)
           (window-dedicated-p window)))
   (seq-mapcat (lambda (frame) (window-list frame 'nomini)) (frame-list))))

(defun ih410-test-run (expression keys)
  (let* ((buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (windows-before (ih410-test-window-state))
         (window-configuration-before (current-window-configuration))
         (buffer-before (current-buffer))
         (driver (generate-new-buffer " *ih410-driver*"))
         (ih410-test-expression expression)
         (ih410-test-result nil)
         (ih410-test-action-events nil)
         (ivy-last ivy-last)
         (ivy-occur-last ivy-occur-last)
         (ivy-minibuffer-map (copy-keymap ivy-minibuffer-map))
         (ivy--actions-list (copy-tree ivy--actions-list))
         (ivy-height 7)
         (ivy-case-fold-search-default nil)
         (ivy-case-fold-search nil)
         (ivy-calling nil)
         (ivy-action-wrap t)
         (ivy-read-action-function ivy-read-action-function)
         (ivy-preferred-re-builders (copy-tree ivy-preferred-re-builders))
         (ivy-dispatching-done-columns ivy-dispatching-done-columns)
         (ivy-dispatching-done-idle nil)
         (ivy-dispatching-done-hydra-exit-keys
          (copy-tree ivy-dispatching-done-hydra-exit-keys))
         (hydra-hint-display-type 'message)
         (next-error-last-buffer next-error-last-buffer)
         (inhibit-message t)
         (message-log-max nil)
         body-error cleanup-errors)
    (unless (and (null hydra-curr-map) (null hydra-curr-on-exit))
      (error "Ivy Hydra case started inside an ambient Hydra"))
    (define-key ivy-minibuffer-map (kbd "<f14>")
                #'ih410-test-minibuffer-state)
    (unwind-protect
        (condition-case condition
            (save-window-excursion
              (switch-to-buffer driver)
              (use-local-map (make-sparse-keymap))
              (local-set-key (kbd "<f13>") #'ih410-test-evaluate-expression)
              (execute-kbd-macro
               (vconcat (kbd "<f13>") (kbd keys))))
          (error (setq body-error condition)))
      (condition-case condition
          (hydra-keyboard-quit)
        (error (push condition cleanup-errors)))
      (setq unread-command-events nil)
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case condition
              (cancel-timer timer)
            (error (push condition cleanup-errors)))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition
              (delete-process process)
            (error (push condition cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case condition
              (delete-frame frame t)
            (error (push condition cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (when (and (not (memq buffer buffers-before))
                   (buffer-live-p buffer)
                   (or (eq buffer driver)
                       (string-prefix-p "*ivy-occur" (buffer-name buffer))
                       (equal (and (buffer-file-name buffer)
                                   (file-name-nondirectory
                                    (buffer-file-name buffer)))
                              "ivy-hydra.el")))
          (condition-case condition
              (kill-buffer buffer)
            (error (push condition cleanup-errors)))))
      (condition-case condition
          (set-window-configuration window-configuration-before)
        (error (push condition cleanup-errors)))
      (when (buffer-live-p buffer-before)
        (set-buffer buffer-before)))
    (list
     :source (copy-tree ih410-test-source-manifest)
     :result ih410-test-result
     :cleanup
     (list :body-error body-error
           :cleanup-errors (nreverse cleanup-errors)
           :hydra-cleared (and (null hydra-curr-map)
                               (null hydra-curr-on-exit)
                               (not (memq 'hydra--clearfun pre-command-hook)))
           :input-clean (null unread-command-events)
           :processes-restored (equal processes-before (process-list))
           :timers-restored
           (equal timers-before (append timer-list timer-idle-list))
           :frames-restored (equal frames-before (frame-list))
           :windows-restored (equal windows-before (ih410-test-window-state))
           :buffer-restored (eq buffer-before (current-buffer))
           :owned-buffers-removed
           (null
            (seq-filter
             (lambda (buffer)
               (and (not (memq buffer buffers-before))
                    (buffer-live-p buffer)
                    (or (string-prefix-p "*ivy-occur" (buffer-name buffer))
                        (eq buffer driver)
                        (equal (and (buffer-file-name buffer)
                                    (file-name-nondirectory
                                     (buffer-file-name buffer)))
                               "ivy-hydra.el"))))
             (buffer-list)))))))
"####;

fn ivy_hydra_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(IVY_HYDRA_MELPA_PIN, "ivy-hydra.el")
        .expect("prepare pinned Ivy Hydra source below ./tmp")
        .with_melpa_dependency(IVY_MELPA_PIN)
        .expect("prepare pinned Ivy dependency below ./tmp")
        .with_melpa_dependency(HYDRA_MELPA_PIN)
        .expect("prepare pinned Hydra dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn menu_navigation_and_options_change_the_live_ivy_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "menu_navigation_and_options_change_the_live_ivy_session",
        r##"
(ih410-test-run
 '(progn
    (ivy-read
     "Deploy: "
     '("alpha service" "beta café" "gamma 界")
     :action #'ih410-test-open
     :caller 'ih410-test-navigation)
    (list :events (nreverse ih410-test-action-events)
          :menu-binding (lookup-key ivy-minibuffer-map (kbd "C-o"))))
 "C-o j g <f14> T C M > <f14> g d")
"##,
        expect![[
            r#"OK (:source (("ivy-hydra-pkg.el" . "8f64889fa1bae9a3064ad9b27db1afff560de271bae8af72b6fa67ec8b06ca94") ("ivy-hydra.el" . "58afeb6059494eff420bd99d8515628f1d927438c71a0c0f4520f29d241234b8")) :result (:events ((:kind open :candidate "beta café" :index 1 :height 7 :max-height 0.25 :truncate nil :case-fold nil :matcher "ivy" :calling nil :action "[1/3] default") (:kind minibuffer :candidate "beta café" :index 1 :height 7 :max-height 7 :truncate t :case-fold nil :matcher "ivy" :calling nil :action "[1/3] default") (:kind minibuffer :candidate "alpha service" :index 0 :height 8 :max-height 8 :truncate nil :case-fold auto :matcher "order" :calling nil :action "[1/3] default") (:kind open :candidate "alpha service" :index 0 :height 8 :max-height 0.25 :truncate nil :case-fold auto :matcher "order" :calling nil :action "[1/3] default") (:kind open :candidate "alpha service" :index 0 :height 7 :max-height 0.25 :truncate nil :case-fold auto :matcher "order" :calling nil :action "[1/3] default")) :menu-binding hydra-ivy/body) :cleanup (:body-error nil :cleanup-errors nil :hydra-cleared t :input-clean t :processes-restored t :timers-restored t :frames-restored t :windows-restored t :buffer-restored t :owned-buffers-removed t))"#
        ]],
    )
}

fn menu_cycles_actions_calls_one_and_returns_to_the_default() -> ParityBatchCase {
    ParityBatchCase::value(
        "menu_cycles_actions_calls_one_and_returns_to_the_default",
        r##"
(ih410-test-run
 '(let ((ivy--actions-list (copy-tree ivy--actions-list)))
    (ivy-set-actions
     'ih410-test-action-cycle
     '(("e" ih410-test-escalate "escalate")
       ("a" ih410-test-archive "archive")))
    (ivy-read
     "Incident: "
     '("INC-417 queued" "INC-418 café" "INC-419 界")
     :action #'ih410-test-open
     :caller 'ih410-test-action-cycle)
    (list :events (nreverse ih410-test-action-events)
          :actions
          (mapcar (lambda (action) (list (car action) (nth 2 action)))
                  (plist-get ivy--actions-list 'ih410-test-action-cycle))))
 "INC-418 C-o s s s g w w w d")
"##,
        expect![[
            r#"OK (:source (("ivy-hydra-pkg.el" . "8f64889fa1bae9a3064ad9b27db1afff560de271bae8af72b6fa67ec8b06ca94") ("ivy-hydra.el" . "58afeb6059494eff420bd99d8515628f1d927438c71a0c0f4520f29d241234b8")) :result (:events ((:kind escalate :candidate "INC-418 café" :index 0 :height 7 :max-height 0.25 :truncate nil :case-fold nil :matcher "ivy" :calling nil :action "[4/5] escalate") (:kind open :candidate "INC-418 café" :index 0 :height 7 :max-height 0.25 :truncate nil :case-fold nil :matcher "ivy" :calling nil :action "[1/5] default")) :actions (("e" "escalate") ("a" "archive"))) :cleanup (:body-error nil :cleanup-errors nil :hydra-cleared t :input-clean t :processes-restored t :timers-restored t :frames-restored t :windows-restored t :buffer-restored t :owned-buffers-removed t))"#
        ]],
    )
}

fn dispatching_done_uses_the_generated_hydra_action_menu() -> ParityBatchCase {
    ParityBatchCase::value(
        "dispatching_done_uses_the_generated_hydra_action_menu",
        r##"
(ih410-test-run
 '(let ((ivy--actions-list (copy-tree ivy--actions-list))
        (ivy-read-action-function #'ivy-hydra-read-action))
    (ivy-set-actions
     'ih410-test-action-menu
     '(("e" ih410-test-escalate "escalate")
       ("a" ih410-test-archive "archive")))
    (ivy-read
     "Incident action: "
     '("INC-417 queued" "INC-418 café" "INC-419 界")
     :action #'ih410-test-open
     :caller 'ih410-test-action-menu)
    (list :events (nreverse ih410-test-action-events)
          :generated-heads
          (mapcar (lambda (head) (list (car head) (nth 2 head)))
                  ivy-read-action/heads)))
 "INC-419 M-o a")
"##,
        expect![[r#"OK (:source (("ivy-hydra-pkg.el" . "8f64889fa1bae9a3064ad9b27db1afff560de271bae8af72b6fa67ec8b06ca94") ("ivy-hydra.el" . "58afeb6059494eff420bd99d8515628f1d927438c71a0c0f4520f29d241234b8")) :result (:events ((:kind archive :candidate "INC-419 界" :index 0 :height 7 :max-height 0.25 :truncate nil :case-fold nil :matcher "ivy" :calling nil :action "[5/5] archive")) :generated-heads (("o" "default") ("i" "insert") ("w" "copy") ("e" "escalate") ("a" "archive") ("M-o" "back") ("C-g" ""))) :cleanup (:body-error nil :cleanup-errors nil :hydra-cleared t :input-clean t :processes-restored t :timers-restored t :frames-restored t :windows-restored t :buffer-restored t :owned-buffers-removed t))"#]],
    )
    .fresh_process()
}

fn menu_exports_filtered_candidates_to_a_real_ivy_occur_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "menu_exports_filtered_candidates_to_a_real_ivy_occur_buffer",
        r##"
(ih410-test-run
 '(let ((return-value
         (ivy-read
          "Service: "
          '("api alpha" "api beta café" "worker gamma" "api delta 界")
          :action #'ih410-test-open
          :caller 'ih410-test-occur)))
    (list
     :return return-value
     :mode major-mode
     :name (buffer-name)
     :text (buffer-substring-no-properties (point-min) (point-max))
     :remembered
     (list :caller (ivy-state-caller ivy-occur-last)
           :text (ivy-state-text ivy-occur-last)
           :collection (copy-tree (ivy-state-collection ivy-occur-last)))
     :keys (list (key-binding (kbd "j"))
                 (key-binding (kbd "k"))
                 (key-binding (kbd "RET")))
     :next-error (eq next-error-last-buffer (current-buffer))))
 "api C-o U")
"##,
        expect![[
            r#"OK (:source (("ivy-hydra-pkg.el" . "8f64889fa1bae9a3064ad9b27db1afff560de271bae8af72b6fa67ec8b06ca94") ("ivy-hydra.el" . "58afeb6059494eff420bd99d8515628f1d927438c71a0c0f4520f29d241234b8")) :result (:return "api alpha" :mode ivy-occur-mode :name "*ivy-occur ih410-test-occur \"api\"*" :text "3 candidates:\n    api alpha\n    api beta café\n    api delta 界\n" :remembered (:caller ih410-test-occur :text "api" :collection ("api alpha" "api beta café" "worker gamma" "api delta 界")) :keys (ivy-occur-next-line ivy-occur-previous-line ivy-occur-press-and-switch) :next-error t) :cleanup (:body-error nil :cleanup-errors nil :hydra-cleared t :input-clean t :processes-restored t :timers-restored t :frames-restored t :windows-restored t :buffer-restored t :owned-buffers-removed t))"#
        ]],
    )
}

fn definition_head_visits_the_pinned_menu_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "definition_head_visits_the_pinned_menu_source",
        r##"
(ih410-test-run
 '(progn
    (ivy-read "Definition: " '("menu") :caller 'ih410-test-definition)
    (let* ((window (selected-window))
           (buffer (window-buffer window))
           (point (window-point window)))
      (with-current-buffer buffer
        (list :file (file-name-nondirectory buffer-file-name)
              :line (line-number-at-pos point)
              :text (save-excursion
                      (goto-char point)
                      (buffer-substring-no-properties
                       (line-beginning-position) (line-end-position)))
              :menu-binding (lookup-key ivy-minibuffer-map (kbd "C-o"))))))
 "C-o D")
"##,
        expect![[
            r#"OK (:source (("ivy-hydra-pkg.el" . "8f64889fa1bae9a3064ad9b27db1afff560de271bae8af72b6fa67ec8b06ca94") ("ivy-hydra.el" . "58afeb6059494eff420bd99d8515628f1d927438c71a0c0f4520f29d241234b8")) :result (:file "ivy-hydra.el" :line 1 :text ";;; ivy-hydra.el --- Additional key bindings for Ivy  -*- lexical-binding: t -*-" :menu-binding hydra-ivy/body) :cleanup (:body-error nil :cleanup-errors nil :hydra-cleared t :input-clean t :processes-restored t :timers-restored t :frames-restored t :windows-restored t :buffer-restored t :owned-buffers-removed t))"#
        ]],
    )
}

#[test]
fn ivy_hydra_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        menu_navigation_and_options_change_the_live_ivy_session(),
        menu_cycles_actions_calls_one_and_returns_to_the_default(),
        dispatching_done_uses_the_generated_hydra_action_menu(),
        menu_exports_filtered_candidates_to_a_real_ivy_occur_buffer(),
        definition_head_visits_the_pinned_menu_source(),
    ];
    assert_oracle_batch_cases(
        ivy_hydra_oracle(),
        std::thread::current()
            .name()
            .unwrap_or("unnamed Ivy Hydra parity test"),
        "ivy_hydra_parity",
        &cases,
    );
}
