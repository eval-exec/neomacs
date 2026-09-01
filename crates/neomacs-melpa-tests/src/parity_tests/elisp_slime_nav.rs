use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ELISP_SLIME_NAV_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const ELISP_SLIME_NAV_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ELISP_SLIME_NAV_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'xref)
(require 'elisp-slime-nav)

;; This is the package's documented setup.  It also makes M-, available in
;; definitions visited through find-function/find-variable.
(add-hook 'emacs-lisp-mode-hook #'elisp-slime-nav-mode)
(setq xref-history-storage #'xref-global-history)

(defun neomacs-elisp-slime-nav-test-path (name)
  "Return NAME below this oracle process's deterministic sandbox."
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun neomacs-elisp-slime-nav-test-write (path contents)
  "Write CONTENTS to PATH and return PATH."
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents))
  path)

(defun neomacs-elisp-slime-nav-test-fixture (name)
  "Create a real Emacs Lisp library and caller project named NAME."
  (let* ((root (neomacs-elisp-slime-nav-test-path (concat name "/")))
         (library (expand-file-name "neomacs-slime-nav-fixture.el" root))
         (caller (expand-file-name "release-plan.el" root)))
    (when (file-exists-p root)
      (delete-directory root t))
    (neomacs-elisp-slime-nav-test-write
     library
     ";;; neomacs-slime-nav-fixture.el --- navigation fixture -*- lexical-binding: t; -*-\n\n(defvar neomacs-slime-nav-retry-limit 3\n  \"Maximum deployment retries before escalation.\")\n\n(defun neomacs-slime-nav-deploy (release region)\n  \"Build the deployment plan for RELEASE in REGION.\"\n  (list :release release :region region\n        :retries neomacs-slime-nav-retry-limit))\n\n(defface neomacs-slime-nav-alert-face\n  '((t :weight bold :foreground \"red\"))\n  \"Face for failed deployment checks.\")\n\n(provide 'neomacs-slime-nav-fixture)\n;;; neomacs-slime-nav-fixture.el ends here\n")
    (neomacs-elisp-slime-nav-test-write
     caller
     ";;; release-plan.el --- caller fixture -*- lexical-binding: t; -*-\n\n(neomacs-slime-nav-deploy \"REL-417\" 'us-east)\nneomacs-slime-nav-retry-limit\nneomacs-slime-nav-fixture\nneomacs-slime-nav-alert-face\nneomacs-slime-nav-missing-symbol\n")
    (list :root root :library library :caller caller)))

(defun neomacs-elisp-slime-nav-test-activate (fixture)
  "Load FIXTURE's library and visit its caller buffer."
  (load (plist-get fixture :library) nil 'nomessage)
  (switch-to-buffer (find-file-noselect (plist-get fixture :caller)))
  (goto-char (point-min)))

(defun neomacs-elisp-slime-nav-test-cleanup (fixture)
  "Remove buffers, definitions, history, and files belonging to FIXTURE."
  (let ((root (plist-get fixture :root)))
    (dolist (buffer (buffer-list))
      (when (and (buffer-file-name buffer)
                 (string-prefix-p root (buffer-file-name buffer)))
        (kill-buffer buffer)))
    (setq features (delq 'neomacs-slime-nav-fixture features))
    (dolist (name '("neomacs-slime-nav-deploy"
                    "neomacs-slime-nav-retry-limit"
                    "neomacs-slime-nav-alert-face"))
      (when (intern-soft name)
        (unintern name obarray)))
    (xref-global-history (cons nil nil))
    (when (file-exists-p root)
      (delete-directory root t))))

(defun neomacs-elisp-slime-nav-test-reset-history ()
  "Install a private empty global xref history."
  (xref-global-history (cons nil nil)))

(defun neomacs-elisp-slime-nav-test-marker (marker)
  "Return a stable description of MARKER."
  (let ((buffer (marker-buffer marker)))
    (when buffer
      (with-current-buffer buffer
        (list (and buffer-file-name
                   (file-name-nondirectory buffer-file-name))
              (marker-position marker)
              (line-number-at-pos marker)
              (save-excursion
                (goto-char marker)
                (current-column)))))))

(defun neomacs-elisp-slime-nav-test-history ()
  "Return backward and forward xref histories in a stable representation."
  (let ((history (xref-global-history)))
    (list :backward
          (mapcar #'neomacs-elisp-slime-nav-test-marker (car history))
          :forward
          (mapcar #'neomacs-elisp-slime-nav-test-marker (cdr history)))))

(defun neomacs-elisp-slime-nav-test-location ()
  "Describe the selected source location."
  (list :file (and buffer-file-name
                   (file-name-nondirectory buffer-file-name))
        :line (line-number-at-pos)
        :column (current-column)
        :text (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))))

(defun neomacs-elisp-slime-nav-test-on (needle)
  "Move to the beginning of NEEDLE in the current buffer."
  (goto-char (point-min))
  (search-forward needle)
  (goto-char (match-beginning 0)))

(defun neomacs-elisp-slime-nav-test-normalize (text fixture)
  "Replace FIXTURE's absolute root in TEXT with a stable label."
  (replace-regexp-in-string
   (regexp-quote (plist-get fixture :root)) "[FIXTURE]/" text t t))
"###;

fn elisp_slime_nav_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ELISP_SLIME_NAV_MELPA_PIN, "elisp-slime-nav.el")
        .expect("prepare revision-pinned Elisp Slime Nav source below ./tmp")
        .with_prelude(ELISP_SLIME_NAV_TEST_PRELUDE)
        .with_timeout(ELISP_SLIME_NAV_TEST_TIMEOUT)
}

fn configured_keys_navigate_functions_and_variables_and_return_to_exact_call_sites()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((fixture (neomacs-elisp-slime-nav-test-fixture "definitions"))
       (load-path (cons (plist-get fixture :root) load-path)))
  (unwind-protect
      (progn
        (neomacs-elisp-slime-nav-test-activate fixture)
        (let ((configuration
               (list :mode elisp-slime-nav-mode
                     :lighter (cdr (assq 'elisp-slime-nav-mode minor-mode-alist))
                     :keys (mapcar #'key-binding
                                   (mapcar #'kbd
                                           '("M-." "M-," "C-c C-d d"
                                             "C-c C-d C-d")))
                     :legacy-alias
                     (eq (indirect-function 'turn-on-elisp-slime-nav-mode)
                         (indirect-function 'elisp-slime-nav-mode)))))
          (neomacs-elisp-slime-nav-test-reset-history)
          (neomacs-elisp-slime-nav-test-on "neomacs-slime-nav-deploy")
          (let ((function-caller (neomacs-elisp-slime-nav-test-location)))
            (execute-kbd-macro (kbd "M-."))
            (let ((function-target (neomacs-elisp-slime-nav-test-location))
                  (function-jump-history
                   (neomacs-elisp-slime-nav-test-history)))
              (execute-kbd-macro (kbd "M-,"))
              (let ((function-return (neomacs-elisp-slime-nav-test-location))
                    (function-return-history
                     (neomacs-elisp-slime-nav-test-history)))
                (neomacs-elisp-slime-nav-test-on
                 "neomacs-slime-nav-retry-limit")
                (neomacs-elisp-slime-nav-test-reset-history)
                (execute-kbd-macro (kbd "M-."))
                (let ((variable-target
                       (neomacs-elisp-slime-nav-test-location))
                      (variable-jump-history
                       (neomacs-elisp-slime-nav-test-history)))
                  (execute-kbd-macro (kbd "M-,"))
                  (list :configuration configuration
                        :function
                        (list :caller function-caller
                              :target function-target
                              :jump-history function-jump-history
                              :returned function-return
                              :return-history function-return-history)
                        :variable
                        (list :target variable-target
                              :jump-history variable-jump-history
                              :returned
                              (neomacs-elisp-slime-nav-test-location)
                              :return-history
                              (neomacs-elisp-slime-nav-test-history)))))))))
    (neomacs-elisp-slime-nav-test-cleanup fixture)))
"###;
    let expected = expect![[
        r####"OK (:configuration (:mode t :lighter (" SliNav") :keys (elisp-slime-nav-find-elisp-thing-at-point pop-tag-mark elisp-slime-nav-describe-elisp-thing-at-point elisp-slime-nav-describe-elisp-thing-at-point) :legacy-alias t) :function (:caller (:file "release-plan.el" :line 3 :column 1 :text "(neomacs-slime-nav-deploy \"REL-417\" 'us-east)") :target (:file "neomacs-slime-nav-fixture.el" :line 6 :column 0 :text "(defun neomacs-slime-nav-deploy (release region)") :jump-history (:backward (("release-plan.el" 70 3 1)) :forward nil) :returned (:file "release-plan.el" :line 3 :column 1 :text "(neomacs-slime-nav-deploy \"REL-417\" 'us-east)") :return-history (:backward nil :forward (("neomacs-slime-nav-fixture.el" 178 6 0)))) :variable (:target (:file "neomacs-slime-nav-fixture.el" :line 3 :column 0 :text "(defvar neomacs-slime-nav-retry-limit 3") :jump-history (:backward (("release-plan.el" 115 4 0)) :forward nil) :returned (:file "release-plan.el" :line 4 :column 0 :text "neomacs-slime-nav-retry-limit") :return-history (:backward nil :forward (("neomacs-slime-nav-fixture.el" 86 3 0)))))"####
    ]];
    ParityBatchCase::value(
        "configured_keys_navigate_functions_and_variables_and_return_to_exact_call_sites",
        elisp_form,
        expected,
    )
}

fn library_and_face_symbols_open_their_real_source_definitions_and_preserve_return_history()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((fixture (neomacs-elisp-slime-nav-test-fixture "library-face"))
       (load-path (cons (plist-get fixture :root) load-path)))
  (unwind-protect
      (progn
        (neomacs-elisp-slime-nav-test-activate fixture)
        (let (results)
          (dolist (symbol '("neomacs-slime-nav-fixture"
                            "neomacs-slime-nav-alert-face"))
            (switch-to-buffer (find-file-noselect (plist-get fixture :caller)))
            (neomacs-elisp-slime-nav-test-on symbol)
            (neomacs-elisp-slime-nav-test-reset-history)
            (let ((origin (neomacs-elisp-slime-nav-test-location)))
              (execute-kbd-macro (kbd "M-."))
              (let ((target (neomacs-elisp-slime-nav-test-location))
                    (jump-history (neomacs-elisp-slime-nav-test-history)))
                (execute-kbd-macro (kbd "M-,"))
                (push (list :symbol symbol
                            :origin origin
                            :target target
                            :jump-history jump-history
                            :returned
                            (neomacs-elisp-slime-nav-test-location)
                            :return-history
                            (neomacs-elisp-slime-nav-test-history))
                      results))))
          (nreverse results)))
    (neomacs-elisp-slime-nav-test-cleanup fixture)))
"###;
    let expected = expect![[
        r####"OK ((:symbol "neomacs-slime-nav-fixture" :origin (:file "release-plan.el" :line 5 :column 0 :text "neomacs-slime-nav-fixture") :target (:file "neomacs-slime-nav-fixture.el" :line 1 :column 0 :text ";;; neomacs-slime-nav-fixture.el --- navigation fixture -*- lexical-binding: t; -*-") :jump-history (:backward (("release-plan.el" 145 5 0)) :forward nil) :returned (:file "release-plan.el" :line 5 :column 0 :text "neomacs-slime-nav-fixture") :return-history (:backward nil :forward (("neomacs-slime-nav-fixture.el" 1 1 0)))) (:symbol "neomacs-slime-nav-alert-face" :origin (:file "release-plan.el" :line 6 :column 0 :text "neomacs-slime-nav-alert-face") :target (:file "neomacs-slime-nav-fixture.el" :line 11 :column 0 :text "(defface neomacs-slime-nav-alert-face") :jump-history (:backward (("release-plan.el" 171 6 0)) :forward nil) :returned (:file "release-plan.el" :line 6 :column 0 :text "neomacs-slime-nav-alert-face") :return-history (:backward nil :forward (("neomacs-slime-nav-fixture.el" 370 11 0)))))"####
    ]];
    ParityBatchCase::value(
        "library_and_face_symbols_open_their_real_source_definitions_and_preserve_return_history",
        elisp_form,
        expected,
    )
}

fn prefix_and_blank_point_prompts_select_navigable_symbols_then_jump_and_return() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((fixture (neomacs-elisp-slime-nav-test-fixture "prompted"))
       (load-path (cons (plist-get fixture :root) load-path)))
  (unwind-protect
      (progn
        (neomacs-elisp-slime-nav-test-activate fixture)
        (neomacs-elisp-slime-nav-test-on "neomacs-slime-nav-retry-limit")
        (neomacs-elisp-slime-nav-test-reset-history)
        (execute-kbd-macro
         (vconcat (kbd "C-u M-.")
                  (string-to-vector "neomacs-slime-nav-deploy")
                  (kbd "RET")))
        (let ((forced-target (neomacs-elisp-slime-nav-test-location))
              (forced-history (neomacs-elisp-slime-nav-test-history)))
          (execute-kbd-macro (kbd "M-,"))
          (goto-char (point-min))
          (forward-line 1)
          (end-of-line)
          (neomacs-elisp-slime-nav-test-reset-history)
          (execute-kbd-macro
           (vconcat (kbd "M-.")
                    (string-to-vector "neomacs-slime-nav-retry-limit")
                    (kbd "RET")))
          (let ((blank-target (neomacs-elisp-slime-nav-test-location))
                (blank-history (neomacs-elisp-slime-nav-test-history)))
            (execute-kbd-macro (kbd "M-,"))
            (list :prefix-forced
                  (list :target forced-target :history forced-history)
                  :blank-point
                  (list :target blank-target :history blank-history)
                  :returned (neomacs-elisp-slime-nav-test-location)
                  :candidates-contain
                  (mapcar
                   (lambda (name)
                     (and (member name
                                  (elisp-slime-nav--all-navigable-symbol-names))
                          t))
                   '("neomacs-slime-nav-deploy"
                     "neomacs-slime-nav-retry-limit"
                     "neomacs-slime-nav-alert-face"))))))
    (neomacs-elisp-slime-nav-test-cleanup fixture)))
"###;
    let expected = expect![[
        r####"OK (:prefix-forced (:target (:file "neomacs-slime-nav-fixture.el" :line 6 :column 0 :text "(defun neomacs-slime-nav-deploy (release region)") :history (:backward (("release-plan.el" 115 4 0)) :forward nil)) :blank-point (:target (:file "neomacs-slime-nav-fixture.el" :line 3 :column 0 :text "(defvar neomacs-slime-nav-retry-limit 3") :history (:backward (("release-plan.el" 68 2 0)) :forward nil)) :returned (:file "release-plan.el" :line 2 :column 0 :text "") :candidates-contain (t t t))"####
    ]];
    ParityBatchCase::value(
        "prefix_and_blank_point_prompts_select_navigable_symbols_then_jump_and_return",
        elisp_form,
        expected,
    )
}

fn describe_key_builds_real_help_for_the_function_and_variable() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((fixture (neomacs-elisp-slime-nav-test-fixture "help"))
       (load-path (cons (plist-get fixture :root) load-path)))
  (unwind-protect
      (progn
        (neomacs-elisp-slime-nav-test-activate fixture)
        (let (function-help variable-help)
          (neomacs-elisp-slime-nav-test-on "neomacs-slime-nav-deploy")
          (execute-kbd-macro (kbd "C-c C-d d"))
          (setq function-help
                (let ((help (get-buffer "*Help*")))
                  (with-current-buffer help
                    (list :buffer (buffer-name)
                          :displayed (and (get-buffer-window help) t)
                          :mode major-mode
                          :text
                          (neomacs-elisp-slime-nav-test-normalize
                           (buffer-substring-no-properties
                            (point-min) (point-max))
                           fixture)))))
          (switch-to-buffer (find-file-noselect (plist-get fixture :caller)))
          (neomacs-elisp-slime-nav-test-on "neomacs-slime-nav-retry-limit")
          (execute-kbd-macro (kbd "C-c C-d C-d"))
          (setq variable-help
                (let ((help (get-buffer "*Help*")))
                  (with-current-buffer help
                    (list :buffer (buffer-name)
                          :displayed (and (get-buffer-window help) t)
                          :mode major-mode
                          :text
                          (neomacs-elisp-slime-nav-test-normalize
                           (buffer-substring-no-properties
                            (point-min) (point-max))
                           fixture)))))
          (list :function function-help :variable variable-help)))
    (when (get-buffer "*Help*")
      (kill-buffer "*Help*"))
    (neomacs-elisp-slime-nav-test-cleanup fixture)))
"###;
    let expected = expect![[
        r####"OK (:function (:buffer "*Help*" :displayed t :mode help-mode :text "neomacs-slime-nav-deploy is an interpreted-function in\n‘neomacs-slime-nav-fixture.el’.\n\n(neomacs-slime-nav-deploy RELEASE REGION)\n\nBuild the deployment plan for RELEASE in REGION.\n") :variable (:buffer "*Help*" :displayed t :mode help-mode :text "neomacs-slime-nav-retry-limit is a variable defined in ‘neomacs-slime-nav-fixture.el’.\n\nIts value is 3\n\nMaximum deployment retries before escalation.\n\n[back]\n"))"####
    ]];
    ParityBatchCase::value(
        "describe_key_builds_real_help_for_the_function_and_variable",
        elisp_form,
        expected,
    )
}

fn stale_symbol_failure_returns_to_the_caller_and_records_xref_forward_history() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((fixture (neomacs-elisp-slime-nav-test-fixture "missing"))
       (load-path (cons (plist-get fixture :root) load-path)))
  (unwind-protect
      (progn
        (neomacs-elisp-slime-nav-test-activate fixture)
        (neomacs-elisp-slime-nav-test-on "neomacs-slime-nav-missing-symbol")
        (neomacs-elisp-slime-nav-test-reset-history)
        (let ((before (neomacs-elisp-slime-nav-test-location))
              outcome)
          (setq outcome
                (condition-case error-data
                    (list :value (execute-kbd-macro (kbd "M-.")))
                  (error
                   (let ((message (error-message-string error-data)))
                     (list :signal (car error-data)
                           :message (substring-no-properties message)
                           :message-properties
                           (text-properties-at 24 message))))))
          (list :before before
                :outcome outcome
                :after (neomacs-elisp-slime-nav-test-location)
                :history (neomacs-elisp-slime-nav-test-history))))
    (neomacs-elisp-slime-nav-test-cleanup fixture)))
"###;
    let expected = expect![[
        r####"OK (:before (:file "release-plan.el" :line 7 :column 0 :text "neomacs-slime-nav-missing-symbol") :outcome (:signal error :message "Don’t know how to find ’neomacs-slime-nav-missing-symbol’" :message-properties (fontified nil)) :after (:file "release-plan.el" :line 7 :column 0 :text "neomacs-slime-nav-missing-symbol") :history (:backward nil :forward (("release-plan.el" 200 7 0))))"####
    ]];
    ParityBatchCase::value(
        "stale_symbol_failure_returns_to_the_caller_and_records_xref_forward_history",
        elisp_form,
        expected,
    )
}

#[test]
fn elisp_slime_nav_package_batch() {
    assert_oracle_batch_cases(
        elisp_slime_nav_oracle(),
        "elisp-slime-nav-package-batch",
        "Elisp Slime Nav",
        &[
            configured_keys_navigate_functions_and_variables_and_return_to_exact_call_sites(),
            library_and_face_symbols_open_their_real_source_definitions_and_preserve_return_history(
            ),
            prefix_and_blank_point_prompts_select_navigable_symbols_then_jump_and_return(),
            describe_key_builds_real_help_for_the_function_and_variable(),
            stale_symbol_failure_returns_to_the_caller_and_records_xref_forward_history(),
        ],
    );
}
