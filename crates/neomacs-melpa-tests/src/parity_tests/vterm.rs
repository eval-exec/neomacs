use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, VTERM_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

// VTerm's Elisp layer has a narrow native-module interface. Recording that
// interface keeps these workflows deterministic while executing the real
// pinned mode, process, filter, navigation, and editing implementation.
const PRELUDE: &str = r###"
(require 'cl-lib)

(defvar neomacs-vterm-test-original-module-file-suffix module-file-suffix)
(defvar neomacs-vterm-test-native-calls nil)
(defvar neomacs-vterm-test-icrnl t)
(defvar neomacs-vterm-test-raw-pwd nil)
(defvar neomacs-vterm-test-eval-calls nil)

(defun neomacs-vterm-test-record-native (operation &rest arguments)
  "Record one deterministic native VTerm OPERATION with ARGUMENTS."
  (push (cons operation arguments) neomacs-vterm-test-native-calls))

(defun vterm--new (&rest arguments)
  (apply #'neomacs-vterm-test-record-native 'new arguments)
  'neomacs-vterm-test-term)

(defun vterm--update (&rest arguments)
  (apply #'neomacs-vterm-test-record-native 'update arguments))

(defun vterm--redraw (&rest arguments)
  (apply #'neomacs-vterm-test-record-native 'redraw arguments))

(defun vterm--write-input (&rest arguments)
  (apply #'neomacs-vterm-test-record-native 'write-input arguments))

(defun vterm--set-size (&rest arguments)
  (apply #'neomacs-vterm-test-record-native 'set-size arguments))

(defun vterm--set-pty-name (&rest arguments)
  (apply #'neomacs-vterm-test-record-native 'set-pty-name arguments))

(defun vterm--reset-point (&rest arguments)
  (apply #'neomacs-vterm-test-record-native 'reset-point arguments)
  (point))

(defun vterm--get-pwd-raw (&rest arguments)
  (apply #'neomacs-vterm-test-record-native 'get-pwd-raw arguments)
  neomacs-vterm-test-raw-pwd)

(defun vterm--get-icrnl (&rest arguments)
  (apply #'neomacs-vterm-test-record-native 'get-icrnl arguments)
  neomacs-vterm-test-icrnl)

(defun neomacs-vterm-test-deploy (&rest arguments)
  "Record a whitelisted shell-to-Emacs deployment request."
  (push arguments neomacs-vterm-test-eval-calls)
  (cons 'deployed arguments))

(provide 'vterm-module)
"###;

fn package_contract_exposes_native_boundary_commands_keys_faces_and_defaults() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'vterm package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :features (mapcar #'featurep '(vterm vterm-module)))
   :module
   (list :suffix neomacs-vterm-test-original-module-file-suffix
         :functions
         (mapcar #'fboundp
                 '(vterm--new vterm--update vterm--redraw vterm--write-input
                   vterm--set-size vterm--set-pty-name vterm--reset-point
                   vterm--get-pwd-raw vterm--get-icrnl)))
   :commands
   (mapcar #'commandp
           '(vterm vterm-other-window vterm-mode vterm-copy-mode
             vterm-copy-mode-done vterm-send-next-key vterm-send-return
             vterm-clear vterm-clear-scrollback vterm-next-prompt
             vterm-previous-prompt))
   :keys
   (mapcar (lambda (key) (lookup-key vterm-mode-map (kbd key)))
           '("RET" "TAB" "DEL" "C-l" "C-c C-l" "C-c C-r"
             "C-c C-n" "C-c C-p" "C-c C-t" "C-a" "C-x" "<f5>"))
   :copy-keys
   (mapcar (lambda (key) (lookup-key vterm-copy-mode-map (kbd key)))
           '("RET" "C-a" "C-e" "C-c C-n" "C-c C-p" "C-c C-t"))
   :defaults
   (list (equal vterm-shell shell-file-name)
         vterm-buffer-name vterm-max-scrollback
         vterm-min-window-width vterm-kill-buffer-on-exit
         vterm-term-environment-variable vterm-environment
         vterm-keymap-exceptions vterm-copy-exclude-prompt
         vterm-use-vterm-prompt-detection-method vterm-timer-delay)
   :palette (append vterm-color-palette nil)
   :selection (list vterm--selection-clipboard vterm--selection-primary)))
"###;
    let expected = expect![[
        r#"OK (:package (:name vterm :version "20260730.1414" :requirements ((emacs (25 1))) :features (t t)) :module (:suffix ".so" :functions (t t t t t t t t t)) :commands (t t t t t t t t t t t) :keys (vterm-send-return vterm-send-tab vterm-send-backspace vterm-clear vterm-clear-scrollback vterm-reset-cursor-point vterm-next-prompt vterm-previous-prompt vterm-copy-mode vterm--self-insert nil vterm--self-insert) :copy-keys (vterm-copy-mode-done vterm-beginning-of-line vterm-end-of-line vterm-next-prompt vterm-previous-prompt vterm-copy-mode) :defaults (t "*vterm*" 1000 80 t "xterm-256color" nil ("C-c" "C-x" "C-u" "C-g" "C-h" "C-l" "M-x" "M-o" "C-y" "M-y") t t 0.1) :palette (vterm-color-black vterm-color-red vterm-color-green vterm-color-yellow vterm-color-blue vterm-color-magenta vterm-color-cyan vterm-color-white vterm-color-bright-black vterm-color-bright-red vterm-color-bright-green vterm-color-bright-yellow vterm-color-bright-blue vterm-color-bright-magenta vterm-color-bright-cyan vterm-color-bright-white) :selection (1 2))"#
    ]];
    ParityBatchCase::value(
        "package_contract_exposes_native_boundary_commands_keys_faces_and_defaults",
        elisp_form,
        expected,
    )
}

fn real_mode_setup_connects_native_term_process_environment_and_buffer_contract() -> ParityBatchCase
{
    let elisp_form = r###"
(with-temp-buffer
  (setq default-directory
        (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
  (let ((vterm-shell "/bin/sh")
        (vterm-environment '("APP_ENV=parity" "DEPLOYMENT=canary"))
        (vterm-term-environment-variable "xterm-parity")
        (vterm-max-scrollback 4321)
        (vterm-min-window-width 80)
        (vterm-kill-buffer-on-exit nil)
        (vterm-exit-functions nil)
        (neomacs-vterm-test-native-calls nil)
        process-spec process-environment-snapshot process)
    (cl-letf
        (((symbol-function 'make-process)
          (lambda (&rest arguments)
            (setq process-spec
                  (list :name (plist-get arguments :name)
                        :same-buffer
                        (eq (plist-get arguments :buffer) (current-buffer))
                        :command (plist-get arguments :command)
                        :connection-type (plist-get arguments :connection-type)
                        :file-handler (plist-get arguments :file-handler)
                        :filter (plist-get arguments :filter)
                        :sentinel (plist-get arguments :sentinel))
                  process-environment-snapshot
                  (mapcar (lambda (name) (cons name (getenv name)))
                          '("TERM" "INSIDE_EMACS" "APP_ENV" "DEPLOYMENT"
                            "LINES" "COLUMNS")))
            (setq process
                  (make-pipe-process
                   :name "vterm-parity-mode-process"
                   :buffer (current-buffer)
                   :noquery t))
            (set-process-filter process (plist-get arguments :filter))
            process)))
      (vterm-mode)
      (unwind-protect
          (list
           :mode
           (list major-mode mode-name buffer-read-only buffer-undo-list
                 truncate-lines scroll-conservatively scroll-margin
                 hscroll-margin hscroll-step font-lock-defaults)
           :term vterm--term
           :process
           (list :live (process-live-p vterm--process)
                 :spec process-spec
                 :environment process-environment-snapshot
                 :adjust
                 (process-get vterm--process 'adjust-window-size-function))
           :native (nreverse neomacs-vterm-test-native-calls)
           :display
           (list :truncation
                 (display-table-slot buffer-display-table 'truncation)
                 :next-error next-error-function
                 :bookmark bookmark-make-record-function
                 :directory (file-relative-name list-buffers-directory
                                                default-directory)
                 :line-number-remap (and vterm--linenum-remapping t))
           :change-mode-guard
           (condition-case error-data
               (progn (run-hooks 'change-major-mode-hook) :allowed)
             (error (list (car error-data) (cadr error-data)))))
        (when (process-live-p process) (delete-process process))))))
"###;
    let expected = expect![[
        r#"OK (:mode (vterm-mode "VTerm" t t t 101 0 0 1 (nil t)) :term neomacs-vterm-test-term :process (:live (open listen connect stop) :spec (:name "vterm" :same-buffer t :command ("/bin/sh" "-c" "stty -nl sane iutf8 erase ^? rows 23 columns 80 >/dev/null && exec /bin/sh") :connection-type pty :file-handler t :filter vterm--filter :sentinel nil) :environment (("TERM" . "xterm-parity") ("INSIDE_EMACS" . "vterm") ("APP_ENV" . "parity") ("DEPLOYMENT" . "canary") ("LINES") ("COLUMNS")) :adjust vterm--window-adjust-process-window-size) :native ((new 23 80 4321 nil nil nil t nil) (set-pty-name neomacs-vterm-test-term nil)) :display (:truncation 32 :next-error vterm-next-error-function :bookmark vterm--bookmark-make-record :directory "./" :line-number-remap t) :change-mode-guard (user-error "You cannot change major mode in vterm buffers"))"#
    ]];
    ParityBatchCase::value(
        "real_mode_setup_connects_native_term_process_environment_and_buffer_contract",
        elisp_form,
        expected,
    )
}

fn process_filter_forwards_text_control_sequences_and_split_utf8_without_loss() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((buffer (get-buffer-create " *vterm-filter-parity*"))
       (process (make-pipe-process
                 :name "vterm-parity-filter-process" :buffer buffer :noquery t)))
  (unwind-protect
      (with-current-buffer buffer
        (erase-buffer)
        (setq-local vterm--term 'neomacs-vterm-test-term
                    vterm--undecoded-bytes nil)
        (let ((locale-coding-system 'utf-8)
              (neomacs-vterm-test-native-calls nil))
          (vterm--filter process "plain\e[31mred\e[0m\n")
          (vterm--filter process (unibyte-string 226 130))
          (let ((pending
                 (list :bytes (string-to-list vterm--undecoded-bytes)
                       :multibyte (multibyte-string-p vterm--undecoded-bytes))))
            (vterm--filter process (unibyte-string 172))
            (list :pending pending
                  :after-pending vterm--undecoded-bytes
                  :calls (nreverse neomacs-vterm-test-native-calls)
                  :buffer (buffer-string)))))
    (when (process-live-p process) (delete-process process))
    (when (buffer-live-p buffer) (kill-buffer buffer))))
"###;
    let expected = expect![[
        r#"OK (:pending (:bytes (4194274 4194178) :multibyte t) :after-pending nil :calls ((write-input neomacs-vterm-test-term "plain") (write-input neomacs-vterm-test-term "\33[31m") (write-input neomacs-vterm-test-term "red") (write-input neomacs-vterm-test-term "\33[0m") (write-input neomacs-vterm-test-term "\n") (update neomacs-vterm-test-term) (write-input neomacs-vterm-test-term "") (update neomacs-vterm-test-term) (write-input neomacs-vterm-test-term "€") (update neomacs-vterm-test-term)) :buffer "")"#
    ]];
    ParityBatchCase::value(
        "process_filter_forwards_text_control_sequences_and_split_utf8_without_loss",
        elisp_form,
        expected,
    )
}

fn keyboard_translation_paste_and_return_preserve_modifiers_characters_and_tty_policy()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (setq-local vterm--term 'neomacs-vterm-test-term
              vterm--process 'neomacs-vterm-test-process)
  (let ((neomacs-vterm-test-native-calls nil)
        (neomacs-vterm-test-icrnl t)
        process-input
        (accepted 0))
    (cl-letf (((symbol-function 'accept-process-output)
               (lambda (&rest _) (setq accepted (1+ accepted)) t))
              ((symbol-function 'process-send-string)
               (lambda (process string)
                 (push (list process string) process-input))))
      (let ((translations
             (mapcar #'vterm--translate-event-to-args
                     (list (aref (kbd "C-a") 0)
                           (aref (kbd "C-S-a") 0)
                           (aref (kbd "M-<left>") 0)
                           (aref (kbd "C-M-<f5>") 0)))))
        (vterm-send "C-S-a")
        (vterm-send-key "<left>" nil t nil)
        (vterm-send-string "λ=42" t)
        (vterm-insert "deploy" ?\n)
        (vterm-send-return)
        (setq neomacs-vterm-test-icrnl nil)
        (vterm-send-return)
        (list :translations translations
              :native (nreverse neomacs-vterm-test-native-calls)
              :process-input (nreverse process-input)
              :accepted accepted
              :redraw-immediately vterm--redraw-immediately)))))
"###;
    let expected = expect![[
        r#"OK (:translations ((("a" nil nil (control))) (("a" (shift . #1=(control)) nil #1#)) (("<left>" nil (meta) nil)) (("<f5>" nil (meta . #2=(control)) #2#))) :native ((update neomacs-vterm-test-term "a" (shift . #3=(control)) nil #3#) (update neomacs-vterm-test-term "<left>" nil t nil) (update neomacs-vterm-test-term "<start_paste>") (update neomacs-vterm-test-term "λ") (update neomacs-vterm-test-term "=") (update neomacs-vterm-test-term "4") (update neomacs-vterm-test-term "2") (update neomacs-vterm-test-term "<end_paste>") (update neomacs-vterm-test-term "<start_paste>") (update neomacs-vterm-test-term "d") (update neomacs-vterm-test-term "e") (update neomacs-vterm-test-term "p") (update neomacs-vterm-test-term "l") (update neomacs-vterm-test-term "o") (update neomacs-vterm-test-term "y") (update neomacs-vterm-test-term "\n") (update neomacs-vterm-test-term "<end_paste>") (get-icrnl neomacs-vterm-test-term) (get-icrnl neomacs-vterm-test-term)) :process-input ((neomacs-vterm-test-process "\n") (neomacs-vterm-test-process "\15")) :accepted 2 :redraw-immediately t)"#
    ]];
    ParityBatchCase::value(
        "keyboard_translation_paste_and_return_preserve_modifiers_characters_and_tty_policy",
        elisp_form,
        expected,
    )
}

fn wrapped_terminal_lines_filter_remove_and_restore_render_only_newlines() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "checkout --target \nrelease --jobs 4\ncomplete\n")
  (goto-char (point-min))
  (search-forward "\n")
  (put-text-property (1- (point)) (point) 'vterm-line-wrap t)
  (search-forward "\n")
  (put-text-property (1- (point)) (point) 'vterm-line-wrap t)
  (let* ((original (buffer-string))
         (filtered (vterm--filter-buffer-substring original)))
    (goto-char (point-max))
    (vterm--remove-fake-newlines t)
    (let ((removed (buffer-string))
          (positions vterm--copy-mode-fake-newlines))
      (vterm--reinsert-fake-newlines)
      (list :original original
            :filtered filtered
            :removed removed
            :positions positions
            :restored (buffer-string)
            :restored-wraps
            (let ((position (point-min)) result)
              (while (< position (point-max))
                (when (get-text-property position 'vterm-line-wrap)
                  (push position result))
                (setq position (1+ position)))
              (nreverse result))
            :remaining-state vterm--copy-mode-fake-newlines))))
"###;
    let expected = expect![[
        r#"OK (:original #("checkout --target \nrelease --jobs 4\ncomplete\n" 18 19 (vterm-line-wrap t) 35 36 (vterm-line-wrap t)) :filtered "checkout --target release --jobs 4complete\n" :removed "checkout --target release --jobs 4complete\n" :positions (19 36) :restored #("checkout --target \nrelease --jobs 4\ncomplete\n" 18 19 (rear-nonsticky t vterm-line-wrap t) 35 36 (rear-nonsticky t vterm-line-wrap t)) :restored-wraps (19 36) :remaining-state nil)"#
    ]];
    ParityBatchCase::value(
        "wrapped_terminal_lines_filter_remove_and_restore_render_only_newlines",
        elisp_form,
        expected,
    )
}

fn prompt_navigation_and_copy_mode_copy_only_the_command_then_resume_output() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "$ deploy --canary\n"
          "build complete\n"
          "$ status\n"
          "healthy\n")
  (goto-char (point-min))
  (put-text-property (point) (+ (point) 2) 'vterm-prompt t)
  (forward-line 2)
  (put-text-property (point) (+ (point) 2) 'vterm-prompt t)
  (setq major-mode 'vterm-mode
        mode-name "VTerm"
        buffer-read-only t)
  (setq-local vterm--term 'neomacs-vterm-test-term
              vterm--prompt-tracking-enabled-p nil
              vterm--copy-mode-fake-newlines nil)
  (let ((vterm-copy-exclude-prompt t)
        (vterm-use-vterm-prompt-detection-method t)
        (neomacs-vterm-test-native-calls nil)
        next previous copied)
    (goto-char (point-min))
    (vterm-next-prompt 1)
    (setq next (list (line-number-at-pos) (current-column)))
    (goto-char (point-max))
    (vterm-previous-prompt 1)
    (setq previous (list (line-number-at-pos) (current-column)))
    (goto-char (point-min))
    (end-of-line)
    (deactivate-mark)
    (vterm-copy-mode 1)
    (vterm-copy-mode-done nil)
    (setq copied (current-kill 0 t))
    (list :navigation (list next previous)
          :copied copied
          :copy-mode vterm-copy-mode
          :read-only buffer-read-only
          :local-map (eq (current-local-map) vterm-mode-map)
          :native (nreverse neomacs-vterm-test-native-calls))))
"###;
    let expected = expect![[
        r#"OK (:navigation ((3 0) (3 1)) :copied #(" deploy --canary" 0 1 (vterm-prompt t)) :copy-mode nil :read-only t :local-map t :native ((update neomacs-vterm-test-term "<stop>" nil nil nil) (reset-point neomacs-vterm-test-term) (update neomacs-vterm-test-term "<start>" nil nil nil)))"#
    ]];
    ParityBatchCase::value(
        "prompt_navigation_and_copy_mode_copy_only_the_command_then_resume_output",
        elisp_form,
        expected,
    )
}

fn local_and_tramp_directory_workflows_select_shells_and_update_buffer_location() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((sandbox (file-name-as-directory
                 (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (fixture (file-name-as-directory
                 (expand-file-name "vterm-directory-fixture" sandbox)))
       (missing (expand-file-name "vterm-missing-directory" sandbox))
       (vterm-shell "/bin/fish")
       (vterm-tramp-shells
        '(("ssh" "/bin/zsh") ("docker" "/bin/sh") (t "/bin/fallback"))))
  (when (file-exists-p fixture) (delete-directory fixture t))
  (make-directory fixture t)
  (unwind-protect
      (with-temp-buffer
        (setq default-directory sandbox)
        (let ((local (vterm--get-directory fixture))
              (remote (vterm--get-directory
                       "deploy@example.test:/srv/checkout"))
              (invalid (vterm--get-directory missing))
              (local-shell (vterm--get-shell))
              remote-shell)
          (let ((default-directory
                 "/ssh:deploy@example.test:/srv/checkout/"))
            (setq remote-shell (vterm--get-shell)))
          (vterm--set-directory fixture)
          (list :directories
                (list :local (file-relative-name local sandbox)
                      :remote remote :invalid invalid
                      :current (file-relative-name default-directory sandbox)
                      :list-buffer
                      (file-relative-name list-buffers-directory sandbox))
                :shells
                (list :local local-shell :remote remote-shell
                      :ssh (vterm--tramp-get-shell "ssh")
                      :docker (vterm--tramp-get-shell "docker")
                      :default (vterm--tramp-get-shell t)
                      :missing (vterm--tramp-get-shell "sudo")))))
    (when (file-exists-p fixture) (delete-directory fixture t))))
"###;
    let expected = expect![[
        r#"OK (:directories (:local "vterm-directory-fixture/" :remote "/-:deploy@example.test:/srv/checkout/" :invalid nil :current "vterm-directory-fixture/" :list-buffer "vterm-directory-fixture/") :shells (:local "/bin/fish" :remote "/bin/zsh" :ssh "/bin/zsh" :docker "/bin/sh" :default "/bin/fallback" :missing nil))"#
    ]];
    ParityBatchCase::value(
        "local_and_tramp_directory_workflows_select_shells_and_update_buffer_location",
        elisp_form,
        expected,
    )
}

fn shell_message_allowlist_and_osc52_selection_enforce_explicit_capabilities() -> ParityBatchCase {
    let elisp_form = r###"
(let ((vterm-eval-cmds
       '(("deploy" neomacs-vterm-test-deploy)
         ("message" message)))
      (neomacs-vterm-test-eval-calls nil)
      (vterm-enable-manipulate-selection-data-by-osc52 t)
      copied)
  (let ((known (vterm--eval "deploy payments canary 3"))
        (quoted (vterm--eval "deploy \"search service\" blue"))
        (unknown (vterm--eval "delete-project production")))
    (cl-letf (((symbol-function 'kill-new)
               (lambda (data &rest _) (push data copied) data)))
      (vterm--set-selection
       (logior vterm--selection-clipboard vterm--selection-primary)
       "incident-481")
      (let ((vterm-enable-manipulate-selection-data-by-osc52 nil))
        (vterm--set-selection vterm--selection-clipboard "blocked")))
    (list :results (list known quoted unknown)
          :eval-calls (nreverse neomacs-vterm-test-eval-calls)
          :selection (nreverse copied)
          :message (current-message))))
"###;
    let expected = expect![[
        r#"OK (:results ((deployed . #1=("payments" "canary" "3")) (deployed . #2=("search service" "blue")) "Failed to find command: delete-project.  To execute a command,\n                add it to the ‘vterm-eval-cmd’ list") :eval-calls (#1# #2#) :selection ("incident-481") :message nil)"#
    ]];
    ParityBatchCase::value(
        "shell_message_allowlist_and_osc52_selection_enforce_explicit_capabilities",
        elisp_form,
        expected,
    )
}

fn session_buffer_naming_reuses_numbered_and_named_terminals_with_bookmark_metadata()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((original (current-buffer))
      (vterm-buffer-name "*vterm-parity-session*")
      (vterm-buffer-name-string "terminal:%s")
      (created 0)
      selected buffers)
  (cl-letf (((symbol-function 'vterm-mode)
             (lambda ()
               (setq major-mode 'vterm-mode mode-name "VTerm")
               (setq created (1+ created)))))
    (unwind-protect
        (let* ((pop-function
                (lambda (buffer)
                  (push (buffer-name buffer) selected)
                  (set-buffer buffer)))
               (default-one (vterm--internal pop-function nil))
               (default-two (vterm--internal pop-function nil))
               (numbered (vterm--internal pop-function 3))
               (named (vterm--internal pop-function "*operations-terminal*"))
               (fresh (vterm--internal pop-function t)))
          (setq buffers
                (delete-dups
                 (list default-one default-two numbered named fresh)))
          (with-current-buffer default-one
            (setq default-directory "/srv/checkout/")
            (let ((bookmark (vterm--bookmark-make-record)))
              (with-current-buffer numbered
                (vterm--set-title "checkout-canary"))
              (list :names
                    (mapcar #'buffer-name
                            (list default-one default-two numbered named fresh))
                    :reuse (eq default-one default-two)
                    :created created
                    :selected (nreverse selected)
                    :bookmark bookmark))))
      (set-buffer original)
      (dolist (buffer buffers)
        (when (buffer-live-p buffer) (kill-buffer buffer))))))
"###;
    let expected = expect![[
        r#"OK (:names ("*vterm-parity-session*" "*vterm-parity-session*" "terminal:checkout-canary" "*operations-terminal*" "*vterm-parity-session*<2>") :reuse t :created 4 :selected ("*vterm-parity-session*" "*vterm-parity-session*" "*vterm-parity-session*<3>" "*operations-terminal*" "*vterm-parity-session*<2>") :bookmark (nil (handler . vterm--bookmark-handler) (thisdir . "/srv/checkout/") (buf-name . "*vterm-parity-session*") (defaults)))"#
    ]];
    ParityBatchCase::value(
        "session_buffer_naming_reuses_numbered_and_named_terminals_with_bookmark_metadata",
        elisp_form,
        expected,
    )
}

#[test]
fn vterm_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(VTERM_MELPA_PIN, "vterm.el")
            .expect("prepare revision-pinned VTerm below ./tmp")
            .with_timeout(Duration::from_secs(300))
            .with_prelude(PRELUDE),
        "vterm-package-batch",
        "VTerm",
        &[
            package_contract_exposes_native_boundary_commands_keys_faces_and_defaults(),
            real_mode_setup_connects_native_term_process_environment_and_buffer_contract(),
            process_filter_forwards_text_control_sequences_and_split_utf8_without_loss(),
            keyboard_translation_paste_and_return_preserve_modifiers_characters_and_tty_policy(),
            wrapped_terminal_lines_filter_remove_and_restore_render_only_newlines(),
            prompt_navigation_and_copy_mode_copy_only_the_command_then_resume_output(),
            local_and_tramp_directory_workflows_select_shells_and_update_buffer_location(),
            shell_message_allowlist_and_osc52_selection_enforce_explicit_capabilities(),
            session_buffer_naming_reuses_numbered_and_named_terminals_with_bookmark_metadata(),
        ],
    );
}
