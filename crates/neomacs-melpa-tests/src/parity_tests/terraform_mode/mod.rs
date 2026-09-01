//! Practical parity coverage for rank 416 `terraform-mode`.
//!
//! These cases drive the public mode, documentation, formatter, region, and
//! format-on-save workflows with an exact owned Terraform process boundary.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, TERRAFORM_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(120);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'imenu)
(require 'terraform-mode)

(get-buffer-create " *code-conversion-work*")

(defconst tf416-test-upstream-main-sha
  "897cd8c421f0c09b713117d6477cc8725bfca08accf87b55743cc23066772459")
(defconst tf416-test-installed-main-sha
  "36a5641bc92ac30330c412d87d3247c51fc924c5e820c319e7ed6e1be97d3a6f")
(defconst tf416-test-installed-pkg-sha
  "560c74aa903059f9fd532e54f6993013958715d3ecd005d0d25243506deb4c45")

(defvar tf416-test-root nil)
(defvar tf416-test-root-owned nil)
(defvar tf416-test-format-plan nil)
(defvar tf416-test-format-ledger nil)
(defvar tf416-test-save-ledger nil)

(defun tf416-test-file-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun tf416-test-source-state ()
  (let* ((main (file-truename (locate-library "terraform-mode.el")))
         (pkg (expand-file-name "terraform-mode-pkg.el"
                                (file-name-directory main)))
         (manifest
          (list (cons "terraform-mode-pkg.el" (tf416-test-file-sha pkg))
                (cons "terraform-mode.el" (tf416-test-file-sha main)))))
    (unless (and (file-regular-p main) (not (file-symlink-p main))
                 (file-regular-p pkg) (not (file-symlink-p pkg))
                 (equal manifest
                        `(("terraform-mode-pkg.el" . ,tf416-test-installed-pkg-sha)
                          ("terraform-mode.el" . ,tf416-test-installed-main-sha))))
      (error "Terraform Mode installed source mismatch: %S" manifest))
    (list :upstream-sha256 tf416-test-upstream-main-sha
          :installed-sha256 manifest
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'terraform-mode package-alist))))
          :feature (featurep 'terraform-mode))))

(defun tf416-test-write (relative contents)
  (let ((file (expand-file-name relative tf416-test-root)))
    (unless (and tf416-test-root-owned
                 (file-in-directory-p file tf416-test-root))
      (error "Refusing Terraform Mode write outside owned root: %s" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file file (insert contents)))
    file))

(defun tf416-test-manifest (root)
  (sort
   (mapcar (lambda (file)
             (unless (and (file-regular-p file) (not (file-symlink-p file)))
               (error "Unexpected Terraform fixture entry: %s" file))
             (cons (file-relative-name file root) (tf416-test-file-sha file)))
           (directory-files-recursively root "."))
   (lambda (left right) (string< (car left) (car right)))))

(defun tf416-test-condition (thunk)
  (condition-case condition
      (list :returned (funcall thunk))
    (error (list :error (car condition)
                 :data (copy-tree (cdr condition))
                 :message (error-message-string condition)))))

(defun tf416-test-buffer-state ()
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :mark (and (mark t) (mark t))
        :modified (buffer-modified-p)
        :mode major-mode
        :format-mode terraform-format-on-save-mode
        :before-save (and (memq #'terraform-format-buffer before-save-hook) t)))

(defun tf416-test-face-runs ()
  (let ((position (point-min)) runs)
    (while (< position (point-max))
      (let* ((next (next-single-property-change position 'face nil (point-max)))
             (face (get-text-property position 'face)))
        (when face
          (push (list position next
                      (buffer-substring-no-properties position next) face)
                runs))
        (setq position next)))
    (nreverse runs)))

(defun tf416-test-normalize-imenu (value)
  (mapcar
   (lambda (entry)
     (let ((name (substring-no-properties (car entry))))
       (if (imenu--subalist-p entry)
           (cons name (tf416-test-normalize-imenu (cdr entry)))
         (cons name
             (if (markerp (cdr entry))
                 (marker-position (cdr entry))
               (cdr entry))))))
   value))

(defun tf416-test-call-process-region
    (start end program delete destination display &rest arguments)
  (unless tf416-test-format-plan
    (error "Unexpected Terraform formatter invocation"))
  (unless (and (equal program "terraform") (null delete) (bufferp destination)
               (equal (buffer-name destination) "*terraform-fmt*")
               (null display))
    (error "Unexpected Terraform formatter call: %S"
           (list start end program delete destination display arguments)))
  (let ((input (buffer-substring-no-properties start end))
        (plan (pop tf416-test-format-plan)))
    (unless (equal arguments (plist-get plan :arguments))
      (error "Unexpected Terraform formatter argv: %S" arguments))
    (push (list :input input :argv (cons program arguments)
                :status (plist-get plan :status)
                :output (plist-get plan :output))
          tf416-test-format-ledger)
    (with-current-buffer destination
      (erase-buffer)
      (insert (plist-get plan :output)))
    (plist-get plan :status)))

(defun tf416-test-forbid-external (operation &rest arguments)
  (error "Unexpected Terraform Mode external boundary: %S %S"
         operation arguments))

(defun tf416-test-park-buffer (name)
  (when-let ((buffer (get-buffer name)))
    (let ((parked (generate-new-buffer-name (concat " *parked " name "*"))))
      (with-current-buffer buffer (rename-buffer parked t))
      (cons buffer name))))

(defun tf416-test-run (case-name body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (concat "terraform-mode-" case-name "/")
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (window-before (current-window-configuration))
         (source-before (tf416-test-source-state))
         (kill-ring-before kill-ring)
         (kill-ring-yank-before kill-ring-yank-pointer)
         (tf416-test-root root)
         (tf416-test-root-owned nil)
         (tf416-test-format-plan nil)
         (tf416-test-format-ledger nil)
         (tf416-test-save-ledger nil)
         (terraform-command "terraform")
         (terraform-indent-level 2)
         (terraform-format-on-save nil)
         (transient-mark-mode nil)
         parked result source-after cleanup-errors)
    (unwind-protect
        (progn
          (unless (and root (file-name-absolute-p root))
            (error "Missing absolute Terraform Mode sandbox root"))
          (when (file-exists-p root)
            (error "Terraform Mode sandbox root exists: %s" root))
          (when-let ((entry (tf416-test-park-buffer "*terraform-fmt*")))
            (push entry parked))
          (make-directory root)
          (setq tf416-test-root-owned t)
          (cl-letf (((symbol-function 'call-process)
                     (lambda (&rest arguments)
                       (apply #'tf416-test-forbid-external 'call-process arguments)))
                    ((symbol-function 'call-process-region)
                     #'tf416-test-call-process-region)
                    ((symbol-function 'make-process)
                     (lambda (&rest arguments)
                       (apply #'tf416-test-forbid-external 'make-process arguments)))
                    ((symbol-function 'process-file)
                     (lambda (&rest arguments)
                       (apply #'tf416-test-forbid-external 'process-file arguments)))
                    ((symbol-function 'start-file-process)
                     (lambda (&rest arguments)
                       (apply #'tf416-test-forbid-external 'start-file-process arguments)))
                    ((symbol-function 'start-process)
                     (lambda (&rest arguments)
                       (apply #'tf416-test-forbid-external 'start-process arguments)))
                    ((symbol-function 'url-retrieve)
                     (lambda (&rest arguments)
                       (apply #'tf416-test-forbid-external 'url-retrieve arguments)))
                    ((symbol-function 'url-retrieve-synchronously)
                     (lambda (&rest arguments)
                       (apply #'tf416-test-forbid-external
                              'url-retrieve-synchronously arguments))))
            (setq result (funcall body root)))
          (when tf416-test-format-plan
            (error "Unused Terraform formatter plan: %S" tf416-test-format-plan))
          (setq source-after (tf416-test-source-state))
          (unless (equal source-before source-after)
            (error "Terraform Mode source changed")))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error (push (list label (car condition) (copy-tree (cdr condition)))
                            cleanup-errors)))))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (or (memq buffer buffers-before) (assq buffer parked))
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda () (kill-buffer buffer)))))
        (dolist (timer (copy-sequence timer-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (dolist (entry parked)
          (attempt (list 'parked (cdr entry))
                   (lambda () (with-current-buffer (car entry)
                                (rename-buffer (cdr entry) t)))))
        (setq kill-ring kill-ring-before kill-ring-yank-pointer kill-ring-yank-before)
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))
        (when tf416-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers (mapcar #'buffer-name
                                      (seq-remove (lambda (b) (memq b buffers-before))
                                                  (buffer-list)))
                 :new-processes (length (seq-remove
                                         (lambda (p) (memq p processes-before))
                                         (process-list)))
                 :new-timers (length (seq-remove
                                      (lambda (timer) (memq timer timers-before))
                                      timer-list))
                 :new-frames (length (seq-remove
                                      (lambda (frame) (memq frame frames-before))
                                      (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :kill-ring-restored
                 (and (eq kill-ring kill-ring-before)
                      (eq kill-ring-yank-pointer kill-ring-yank-before))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "Terraform Mode cleanup failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TERRAFORM_MODE_MELPA_PIN, "terraform-mode.el")
        .expect("prepare exact terraform-mode source and dependency closure below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_mode_font_lock_imenu_and_outline_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_mode_font_lock_imenu_and_outline_lifecycle",
        r####"
(tf416-test-run
 "mode"
 (lambda (_root)
   (with-temp-buffer
     (insert "resource \"aws_instance\" \"café_界\" {\n  ami = \"ami-1\"\n}\n\nvariable \"region\" {\n  default = null\n}\n\nephemeral \"vault_token\" \"short\" {\n}\n")
     (terraform-mode)
     (font-lock-ensure)
     (let ((mode-state
            (list :mode major-mode :derived (derived-mode-p 'hcl-mode)
                  :indent hcl-indent-level
                  :imenu-function imenu-create-index-function
                  :keys (mapcar (lambda (key) (cons key (key-binding (kbd key))))
                                '("C-c C-d C-w" "C-c C-d C-c"
                                  "C-c C-d C-r" "C-c C-f"))))
           (faces (tf416-test-face-runs))
           (imenu (tf416-test-normalize-imenu (imenu--make-index-alist t))))
       (outline-minor-mode 1)
       (goto-char (point-min))
       (call-interactively #'outline-toggle-children)
       (let ((folded (outline-invisible-p (line-beginning-position 2))))
         (call-interactively #'outline-toggle-children)
         (list :mode mode-state :faces faces :imenu imenu
               :folded folded
               :opened (not (outline-invisible-p (line-beginning-position 2)))))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "897cd8c421f0c09b713117d6477cc8725bfca08accf87b55743cc23066772459" :installed-sha256 (("terraform-mode-pkg.el" . "560c74aa903059f9fd532e54f6993013958715d3ecd005d0d25243506deb4c45") ("terraform-mode.el" . "36a5641bc92ac30330c412d87d3247c51fc924c5e820c319e7ed6e1be97d3a6f")) :version "20251115.2210" :feature t) :result (:mode (:mode terraform-mode :derived hcl-mode :indent 2 :imenu-function terraform--generate-imenu :keys (("C-c C-d C-w" . terraform-open-doc) ("C-c C-d C-c" . terraform-kill-doc-url) ("C-c C-d C-r" . terraform-insert-doc-in-comment) ("C-c C-f" . outline-toggle-children))) :faces ((1 9 "resource" terraform-builtin-face) (10 24 "\"aws_instance\"" terraform-resource-type-face) (25 33 "\"café_界\"" terraform-resource-name-face) (38 41 "ami" terraform-variable-name-face) (44 51 "\"ami-1\"" font-lock-string-face) (55 63 "variable" terraform-builtin-face) (64 72 "\"region\"" terraform-resource-name-face) (77 84 "default" terraform-variable-name-face) (87 91 "null" font-lock-constant-face) (95 104 "ephemeral" terraform-builtin-face) (105 118 "\"vault_token\"" terraform-resource-type-face) (119 126 "\"short\"" terraform-resource-name-face)) :imenu (("*Rescan*" . -99) ("ephemeral" ("vault_token/short" . 105)) ("resource" ("aws_instance/café_界" . 10)) ("variable" ("region" . 64))) :folded t :opened t) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :kill-ring-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_documentation_commands_derive_provider_urls_without_processes() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_documentation_commands_derive_provider_urls_without_processes",
        r####"
(tf416-test-run
 "docs"
 (lambda (_root)
   (with-temp-buffer
     (insert "terraform {\n  required_providers {\n    aws = {\n      source = \"hashicorp/aws\"\n    }\n  }\n}\n\nresource \"aws_instance\" \"café_界\" {\n}\n\ndata \"aws_ami\" \"selected\" {\n}\n")
     (terraform-mode)
     (goto-char (point-min))
     (search-forward "café_界")
     (call-interactively #'terraform-insert-doc-in-comment)
     (let ((resource-text (buffer-substring-no-properties (point-min) (point-max))))
       (search-forward "selected")
       (call-interactively #'terraform-kill-doc-url)
       (list :resource-text resource-text
             :data-url (current-kill 0 t)
             :modified (buffer-modified-p))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "897cd8c421f0c09b713117d6477cc8725bfca08accf87b55743cc23066772459" :installed-sha256 (("terraform-mode-pkg.el" . "560c74aa903059f9fd532e54f6993013958715d3ecd005d0d25243506deb4c45") ("terraform-mode.el" . "36a5641bc92ac30330c412d87d3247c51fc924c5e820c319e7ed6e1be97d3a6f")) :version "20251115.2210" :feature t) :result (:resource-text "terraform {\n  required_providers {\n    aws = {\n      source = \"hashicorp/aws\"\n    }\n  }\n}\n\n# https://registry.terraform.io/providers/hashicorp/aws/latest/docs/resources/instance\nresource \"aws_instance\" \"café_界\" {\n}\n\ndata \"aws_ami\" \"selected\" {\n}\n" :data-url "https://registry.terraform.io/providers/hashicorp/aws/latest/docs/data-sources/ami" :modified t) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :kill-ring-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_buffer_and_region_formatting_use_exact_process_boundary() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_buffer_and_region_formatting_use_exact_process_boundary",
        r####"
(tf416-test-run
 "format"
 (lambda (_root)
   (let (buffer-state region-state)
     (with-temp-buffer
       (insert "resource \"null_resource\" \"界\"{triggers={name=\"café\"}}\n")
       (terraform-mode)
       (setq tf416-test-format-plan
             '((:status 0 :arguments ("fmt" "-no-color" "-")
                        :output "resource \"null_resource\" \"界\" {\n  triggers = { name = \"café\" }\n}\n")))
       (goto-char 18)
       (call-interactively #'terraform-format-buffer)
       (setq buffer-state (tf416-test-buffer-state)))
     (with-temp-buffer
       (insert "prefix\nresource \"null_resource\" \"part\"{triggers={x=1}}\nsuffix\n")
       (terraform-mode)
       (setq transient-mark-mode t)
       (goto-char (point-min))
       (forward-line 1)
       (push-mark (point) t t)
       (forward-line 1)
       (setq tf416-test-format-plan
             '((:status 0 :arguments ("fmt" "-")
                        :output "resource \"null_resource\" \"part\" {\n  triggers = { x = 1 }\n}\n")))
       (call-interactively #'terraform-format-region)
       (setq region-state (tf416-test-buffer-state)))
     (list :buffer buffer-state :region region-state
           :calls (nreverse tf416-test-format-ledger)))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "897cd8c421f0c09b713117d6477cc8725bfca08accf87b55743cc23066772459" :installed-sha256 (("terraform-mode-pkg.el" . "560c74aa903059f9fd532e54f6993013958715d3ecd005d0d25243506deb4c45") ("terraform-mode.el" . "36a5641bc92ac30330c412d87d3247c51fc924c5e820c319e7ed6e1be97d3a6f")) :version "20251115.2210" :feature t) :result (:buffer (:text "resource \"null_resource\" \"界\" {\n  triggers = { name = \"café\" }\n}\n" :point 18 :mark nil :modified t :mode terraform-mode :format-mode nil :before-save nil) :region (:text "prefix\nresource \"null_resource\" \"part\" {\n  triggers = { x = 1 }\n}\nsuffix\n" :point 56 :mark 8 :modified t :mode terraform-mode :format-mode nil :before-save nil) :calls ((:input "resource \"null_resource\" \"界\"{triggers={name=\"café\"}}\n" :argv ("terraform" "fmt" "-no-color" "-") :status 0 :output "resource \"null_resource\" \"界\" {\n  triggers = { name = \"café\" }\n}\n") (:input "resource \"null_resource\" \"part\"{triggers={x=1}}\n" :argv ("terraform" "fmt" "-") :status 0 :output "resource \"null_resource\" \"part\" {\n  triggers = { x = 1 }\n}\n"))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :kill-ring-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_format_failure_is_atomic_then_save_mode_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_format_failure_is_atomic_then_save_mode_recovers",
        r####"
(tf416-test-run
 "save"
 (lambda (root)
   (let* ((file (tf416-test-write
                 "main.tf"
                 "resource \"null_resource\" \"save\"{triggers={label=\"界\"}}\n"))
          (default-directory root)
          (enable-dir-local-variables nil)
          (terraform-format-on-save t)
          (buffer (find-file-noselect file))
          failure before after-save messages)
     (with-current-buffer buffer
       (terraform-mode)
       (setq before (tf416-test-buffer-state)
             tf416-test-format-plan
             '((:status 1 :arguments ("fmt" "-no-color" "-")
                        :output "Error: invalid café expression\n")))
       (cl-letf (((symbol-function 'message)
                  (lambda (format-string &rest arguments)
                    (push (apply #'format format-string arguments) messages))))
         (setq failure
               (tf416-test-condition
                (lambda () (call-interactively #'terraform-format-buffer)))))
       (let ((after-failure (tf416-test-buffer-state)))
         (goto-char (point-max))
         (insert "# save recovery\n")
         (add-hook 'after-save-hook
                   (lambda ()
                     (push (list (file-relative-name buffer-file-name root)
                                 (tf416-test-file-sha buffer-file-name))
                           tf416-test-save-ledger)) nil t)
         (setq tf416-test-format-plan
               '((:status 0 :arguments ("fmt" "-no-color" "-")
                          :output "resource \"null_resource\" \"save\" {\n  triggers = { label = \"界\" }\n}\n# save recovery\n")))
         (save-buffer)
         (setq after-save (tf416-test-buffer-state))
         (list :before before :failure failure
               :messages (nreverse messages)
               :failure-atomic (equal before after-failure)
               :after-save after-save
               :save-ledger (nreverse tf416-test-save-ledger)
               :manifest (tf416-test-manifest root)
               :calls (nreverse tf416-test-format-ledger)))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "897cd8c421f0c09b713117d6477cc8725bfca08accf87b55743cc23066772459" :installed-sha256 (("terraform-mode-pkg.el" . "560c74aa903059f9fd532e54f6993013958715d3ecd005d0d25243506deb4c45") ("terraform-mode.el" . "36a5641bc92ac30330c412d87d3247c51fc924c5e820c319e7ed6e1be97d3a6f")) :version "20251115.2210" :feature t) :result (:before (:text "resource \"null_resource\" \"save\"{triggers={label=\"界\"}}\n" :point 1 :mark nil :modified nil :mode terraform-mode :format-mode t :before-save t) :failure (:returned t) :messages ("terraform fmt: Error: invalid café expression\n") :failure-atomic t :after-save (:text "resource \"null_resource\" \"save\" {\n  triggers = { label = \"界\" }\n}\n# save recovery\n" :point 71 :mark nil :modified nil :mode terraform-mode :format-mode t :before-save t) :save-ledger (("main.tf" "cbde2bc886e7344b46f6acfa52d093f484af22f58994e1ae439adb8b24e76713")) :manifest (("main.tf" . "cbde2bc886e7344b46f6acfa52d093f484af22f58994e1ae439adb8b24e76713") ("main.tf~" . "202c8e884032f8b7a68b01de38baf9e4d89851e5b7d86476efd89cb1b8911159")) :calls ((:input "resource \"null_resource\" \"save\"{triggers={label=\"界\"}}\n" :argv ("terraform" "fmt" "-no-color" "-") :status 1 :output "Error: invalid café expression\n") (:input "resource \"null_resource\" \"save\"{triggers={label=\"界\"}}\n# save recovery\n" :argv ("terraform" "fmt" "-no-color" "-") :status 0 :output "resource \"null_resource\" \"save\" {\n  triggers = { label = \"界\" }\n}\n# save recovery\n"))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :kill-ring-restored t :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn terraform_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_mode_font_lock_imenu_and_outline_lifecycle(),
        public_documentation_commands_derive_provider_urls_without_processes(),
        public_buffer_and_region_formatting_use_exact_process_boundary(),
        public_format_failure_is_atomic_then_save_mode_recovers(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "terraform-mode-rank416",
        "terraform_mode_parity",
        &cases,
    );
}
