//! Practical parity for Node.js REPL's live comint workflows.
//!
//! These cases start the pinned Node 22 executable, exercise public source
//! submission and completion commands, and prove recovery after a JavaScript
//! error.  The harness owns the process, filesystem, windows, and package
//! globals, and waits for a new complete prompt before observing output.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, NODEJS_REPL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'nodejs-repl)

(defconst node405-test-source-sha256
  "18409462373f01a4933decf72c10a9c903785edaec74a532a9566e69caaba212")
(defconst node405-test-node-sha256
  "8aee2b5233e91de07502156662e8823eaba85f6a86bd5e717baed79cc807830d")
(defconst node405-test-node-environment-names
  '("NODE_OPTIONS" "NODE_PATH" "NODE_REPL_HISTORY" "NODE_REPL_HISTORY_SIZE"
    "NODE_DISABLE_COLORS" "NO_COLOR" "FORCE_COLOR" "NODE_EXTRA_CA_CERTS"
    "NODE_PENDING_DEPRECATION" "NODE_NO_WARNINGS" "NODE_REDIRECT_WARNINGS"
    "NODE_TLS_REJECT_UNAUTHORIZED" "NODE_ICU_DATA" "NODE_DEBUG"
    "NODE_CHANNEL_FD" "NODE_V8_COVERAGE"))

(defun node405-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defconst node405-test-node
  (let ((candidate (executable-find "node")))
    (and candidate (file-truename candidate))))

(let* ((loaded (symbol-file 'nodejs-repl 'defun))
       (source (and loaded
                    (if (string-suffix-p ".elc" loaded)
                        (concat (file-name-sans-extension loaded) ".el")
                      loaded)))
       (directory (and source (file-name-directory source)))
       (payload
        (and directory
             (sort (seq-filter
                    (lambda (name)
                      (and (string-suffix-p ".el" name)
                           (not (string-suffix-p "-autoloads.el" name))
                           (not (string-suffix-p "-pkg.el" name))))
                    (directory-files directory nil nil t))
                   #'string<))))
  (unless (and (file-regular-p source)
               (not (file-symlink-p source))
               (equal (file-name-nondirectory source) "nodejs-repl.el")
               (equal payload '("nodejs-repl.el"))
               (equal (node405-test-file-sha256 source)
                      node405-test-source-sha256))
    (error "Unexpected installed Node.js REPL source: %S %S" source payload)))

(unless (and (file-name-absolute-p node405-test-node)
             (file-regular-p node405-test-node)
             (not (file-symlink-p node405-test-node))
             (equal (node405-test-file-sha256 node405-test-node)
                    node405-test-node-sha256))
  (error "Unexpected Node executable: %s" node405-test-node))
(let ((buffer (generate-new-buffer " *node405-version*"))
      (process-environment (copy-sequence process-environment)))
  (unwind-protect
      (progn
        (dolist (name node405-test-node-environment-names) (setenv name nil))
        (setenv "LC_ALL" "C.UTF-8")
        (let ((status (call-process node405-test-node nil buffer nil "--version")))
          (unless (and (equal status 0)
                       (equal (with-current-buffer buffer (buffer-string))
                              "v22.22.2\n"))
            (error "Unexpected Node version: %S %S" status
                   (with-current-buffer buffer (buffer-string))))))
    (kill-buffer buffer)))

(defvar node405-test-root nil)
(defvar node405-test-launches nil)
(defvar node405-test-version-calls nil)
(defvar node405-test-original-make-comint nil)
(defvar node405-test-original-start-file-process nil)
(defvar node405-test-original-start-process nil)
(defvar node405-test-original-make-process nil)
(defvar node405-test-original-shell-command-to-string nil)
(defvar node405-test-original-call-process nil)
(defvar node405-test-process-descent 0)
(defvar node405-test-version-descent 0)

(defun node405-test-condition (condition)
  (list :type (car condition)
        :data (node405-test-normalize (copy-tree (cdr condition)))
        :message (node405-test-normalize (error-message-string condition))))

(defun node405-test-normalize (value)
  (cond
   ((stringp value)
    (let ((text (copy-sequence value)))
      (when node405-test-root
        (setq text (replace-regexp-in-string
                    (regexp-quote node405-test-root) "[ROOT]/" text t t)))
      (replace-regexp-in-string
       (regexp-quote node405-test-node) "[NODE]" text t t)))
   ((consp value)
    (cons (node405-test-normalize (car value))
          (node405-test-normalize (cdr value))))
   ((vectorp value)
    (apply #'vector (mapcar #'node405-test-normalize value)))
   (t value)))

(defun node405-test-window-state ()
  (mapcar (lambda (window)
            (list (window-buffer window) (window-point window)
                  (window-start window) (window-dedicated-p window)))
          (seq-mapcat (lambda (frame) (window-list frame 'nomini))
                      (frame-list))))

(defun node405-test-write-file (root relative contents)
  (let ((path (expand-file-name relative root)))
    (unless (file-in-directory-p path root)
      (error "Unsafe Node.js REPL fixture path: %s" path))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert contents)
      (let ((coding-system-for-write 'utf-8-unix))
        (write-region (point-min) (point-max) path nil 'silent)))
    path))

(defun node405-test-manifest (root)
  (mapcar
   (lambda (path)
     (unless (and (file-in-directory-p path root)
                  (file-regular-p path) (not (file-symlink-p path)))
       (error "Unsafe Node.js REPL fixture: %s" path))
     (list (file-relative-name path root)
           (node405-test-file-sha256 path)))
   (sort (directory-files-recursively root "." nil nil nil) #'string-lessp)))

(defun node405-test-park-buffer (name)
  (when-let* ((buffer (get-buffer name)))
    (let ((old-name (buffer-name buffer)))
      (with-current-buffer buffer
        (rename-buffer (format " *node405-parked-%s*" (sxhash-eq buffer)) t))
      (cons buffer old-name))))

(defun node405-test-shell-command (command)
  (unless (equal command (concat node405-test-node " --version"))
    (error "Unexpected Node.js REPL shell command: %S" command))
  (let* ((node405-test-version-descent 1)
         (output (funcall node405-test-original-shell-command-to-string
                          command)))
    (unless (equal output "v22.22.2\n")
      (error "Unexpected Node.js REPL version output: %S" output))
    (push command node405-test-version-calls)
    output))

(defun node405-test-call-process
    (program infile destination display &rest arguments)
  (unless (and (> node405-test-version-descent 0)
               (equal program shell-file-name)
               (null infile)
               (eq destination t)
               (null display)
               (equal arguments
                      (list shell-command-switch
                            (concat node405-test-node " --version"))))
    (error "Unexpected Node call-process: %S"
           (list program infile destination display arguments)))
  (let ((node405-test-version-descent
         (1+ node405-test-version-descent)))
    (apply node405-test-original-call-process
           program infile destination display arguments)))

(defun node405-test-make-comint (name program startfile &rest switches)
  (let* ((mode (or (getenv "NODE_REPL_MODE") "sloppy"))
         (code (format nodejs-repl-code-format
                       nodejs-repl-prompt nodejs-repl-use-global mode))
         (expected (list "TERM=xterm" node405-test-node "-e" code)))
    (unless (and (equal name nodejs-repl-process-name)
                 (equal program "env")
                 (null startfile)
                 (equal switches expected)
                 (null (get-process name)))
      (error "Unexpected Node.js REPL launch: %S"
             (list name program startfile switches expected)))
    (let* ((before (process-list))
           (buffer (let ((node405-test-process-descent 1))
                     (apply node405-test-original-make-comint
                            name program startfile switches)))
           (process (get-buffer-process buffer)))
      (unless (and (buffer-live-p buffer)
                   (processp process)
                   (not (memq process before))
                   (eq (process-buffer process) buffer)
                   (equal (process-name process) name)
                   (equal (process-command process)
                          (cons program switches))
                   (process-live-p process))
        (when (processp process)
          (set-process-query-on-exit-flag process nil)
          (delete-process process))
        (error "Invalid Node.js REPL process result: %S" process))
      (push (list :name name :program program :startfile startfile
                  :switches (node405-test-normalize (copy-tree switches))
                  :environment
                  (cons (getenv "LC_ALL")
                        (mapcar #'getenv node405-test-node-environment-names))
                  :buffer (buffer-name buffer))
            node405-test-launches)
      buffer)))

(defun node405-test-start-file-process (&rest arguments)
  (unless (> node405-test-process-descent 0)
    (error "Unexpected direct Node start-file-process: %S" arguments))
  (let ((node405-test-process-descent (1+ node405-test-process-descent)))
    (apply node405-test-original-start-file-process arguments)))

(defun node405-test-start-process (&rest arguments)
  (unless (> node405-test-process-descent 0)
    (error "Unexpected direct Node start-process: %S" arguments))
  (let ((node405-test-process-descent (1+ node405-test-process-descent)))
    (apply node405-test-original-start-process arguments)))

(defun node405-test-make-process (&rest arguments)
  (unless (> node405-test-process-descent 0)
    (error "Unexpected direct Node make-process: %S" arguments))
  (let ((node405-test-process-descent (1+ node405-test-process-descent)))
    (apply node405-test-original-make-process arguments)))

(defun node405-test-wait-for-prompt (process start)
  (let ((attempt 0) complete)
    (while (and (< attempt 100) (process-live-p process) (not complete))
      (accept-process-output process 0.05)
      (setq attempt (1+ attempt)
            complete
            (with-current-buffer (process-buffer process)
              (let ((mark (marker-position (process-mark process))))
                (and (> mark start)
                     (>= (- mark start) (length nodejs-repl-prompt))
                     (equal (buffer-substring-no-properties
                             (- mark (length nodejs-repl-prompt)) mark)
                            nodejs-repl-prompt))))))
    (unless complete
      (error "Node.js REPL did not produce a new prompt: %S"
             (with-current-buffer (process-buffer process) (buffer-string))))
    (let ((attempt 0)
          (stable 0)
          (settled
           (with-current-buffer (process-buffer process)
             (list (buffer-size) (marker-position (process-mark process))
                   (buffer-substring-no-properties start
                                                   (process-mark process))))))
      (while (and (< attempt 100) (< stable 5))
        (accept-process-output process 0.05)
        (let ((sample
               (with-current-buffer (process-buffer process)
                 (list (buffer-size)
                       (marker-position (process-mark process))
                       (buffer-substring-no-properties
                        start (process-mark process))))))
          (if (equal settled sample)
              (setq stable (1+ stable))
            (setq settled sample
                  stable 0)))
        (setq attempt (1+ attempt)))
      (unless (= stable 5)
        (error "Node.js REPL output did not become stable: %S" settled)))
    (marker-position (process-mark process))))

(defun node405-test-send-and-capture (process thunk)
  (let ((start (with-current-buffer (process-buffer process)
                 (marker-position (process-mark process)))))
    (funcall thunk)
    (node405-test-wait-for-prompt process start)
    (let ((output
           (with-current-buffer (process-buffer process)
             (buffer-substring-no-properties start (process-mark process)))))
      ;; A prompt is the completion delimiter, not command output.  Node's
      ;; `.load` writes two prompts and the package's chunk-local filter may
      ;; receive them separately, so discard every settled trailing prompt.
      (while (string-suffix-p nodejs-repl-prompt output)
        (setq output
              (substring output 0 (- (length output)
                                     (length nodejs-repl-prompt)))))
      (node405-test-normalize output))))

(defun node405-test-start ()
  (let ((start-buffer (current-buffer)))
    (call-interactively #'nodejs-repl)
    (let ((process (get-process nodejs-repl-process-name)))
      (unless (and (processp process)
                   (eq (process-buffer process) (current-buffer)))
        (error "Node.js REPL public start did not select its process buffer"))
      (node405-test-wait-for-prompt process (point-min))
      (when (buffer-live-p start-buffer)
        (set-buffer start-buffer))
      process)))

(defun node405-test-run (files expected-launches body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "nodejs-repl/" sandbox))))
         (window-before (current-window-configuration))
         (window-state-before (node405-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (original-make-comint (symbol-function 'make-comint))
         (original-start-file-process (symbol-function 'start-file-process))
         (original-start-process (symbol-function 'start-process))
         (original-make-process (symbol-function 'make-process))
         (original-shell-command-to-string
          (symbol-function 'shell-command-to-string))
         (original-call-process (symbol-function 'call-process))
         (nodejs-repl-command node405-test-node)
         (nodejs-repl-arguments nil)
         (nodejs-repl-prompt "> ")
         (nodejs-repl-use-global "true")
         (nodejs-repl-input-ignoredups t)
         (nodejs-repl-process-echoes t)
         (nodejs-repl-process-name "node405")
         (nodejs-repl-temp-buffer-name "*node405-output*")
         (nodejs-repl-nodejs-version nil)
         (nodejs-repl-prompt-re
          (format nodejs-repl-prompt-re-format "> " "> "))
         (nodejs-repl-cache-token "")
         (nodejs-repl-cache-completions nil)
         (nodejs-repl-get-completions-for-require-p nil)
         (nodejs-repl-prompt-deletion-required-p nil)
         (node405-test-root root)
         (node405-test-launches nil)
         (node405-test-version-calls nil)
         (node405-test-original-make-comint original-make-comint)
         (node405-test-original-start-file-process original-start-file-process)
         (node405-test-original-start-process original-start-process)
         (node405-test-original-make-process original-make-process)
         (node405-test-original-shell-command-to-string
          original-shell-command-to-string)
         (node405-test-original-call-process original-call-process)
         (node405-test-process-descent 0)
         (node405-test-version-descent 0)
         (process-environment (copy-sequence process-environment))
         (default-directory default-directory)
         (message-log-max nil)
         (print-circle nil)
         (parked nil)
         (root-owned nil)
         fixture-before fixture-after result body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Node.js REPL sandbox root"))
              (when (file-exists-p root)
                (error "Node.js REPL sandbox root already exists: %s" root))
              (dolist (name '("*node405*" "*node405-output*"))
                (when-let* ((entry (node405-test-park-buffer name)))
                  (push entry parked)))
              (make-directory root t)
              (setq root-owned t)
              (dolist (file files)
                (node405-test-write-file root (car file) (cdr file)))
              (setq fixture-before (node405-test-manifest root)
                    default-directory root)
              (dolist (name node405-test-node-environment-names)
                (setenv name nil))
              (setenv "LC_ALL" "C.UTF-8")
              (setenv "NODE_REPL_MODE" "sloppy")
              (setq result
                    (cl-letf (((symbol-function 'shell-command-to-string)
                               #'node405-test-shell-command)
                              ((symbol-function 'make-comint)
                               #'node405-test-make-comint)
                              ((symbol-function 'start-file-process)
                               #'node405-test-start-file-process)
                              ((symbol-function 'start-process)
                               #'node405-test-start-process)
                              ((symbol-function 'make-process)
                               #'node405-test-make-process)
                              ((symbol-function 'call-process)
                               #'node405-test-call-process)
                              ((symbol-function 'process-file)
                               (lambda (&rest arguments)
                                 (error "Unexpected Node process-file: %S"
                                        arguments)))
                              ((symbol-function 'make-network-process)
                               (lambda (&rest arguments)
                                 (error "Unexpected Node network process: %S"
                                        arguments)))
                              ((symbol-function 'open-network-stream)
                               (lambda (&rest arguments)
                                 (error "Unexpected Node network stream: %S"
                                        arguments))))
                      (funcall body root)))
              (unless (and (= (length node405-test-launches) expected-launches)
                           (= (length node405-test-version-calls)
                              expected-launches))
                (error "Unexpected Node.js REPL lifecycle: %S %S"
                       node405-test-launches node405-test-version-calls))
              (setq fixture-after (node405-test-manifest root))
              (unless (equal fixture-before fixture-after)
                (error "Node.js REPL fixture changed: %S %S"
                       fixture-before fixture-after)))
          (error (setq body-error condition)))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition
              (progn
                (set-process-query-on-exit-flag process nil)
                (delete-process process)
                (while (process-live-p process)
                  (accept-process-output process 0.05)))
            (error (push (list :delete-process condition) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case condition
              (progn
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error (push (list :kill-buffer (buffer-name buffer) condition)
                         cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case condition
              (cancel-timer timer)
            (error (push (list :cancel-timer condition) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case condition
              (delete-frame frame t)
            (error (push (list :delete-frame condition) cleanup-errors)))))
      (condition-case condition
          (progn
            (when (buffer-live-p buffer-before) (set-buffer buffer-before))
            (set-window-configuration window-before))
        (error (push (list :restore-window condition) cleanup-errors)))
      (dolist (entry parked)
        (condition-case condition
            (let ((buffer (car entry)) (name (cdr entry)))
              (unless (buffer-live-p buffer)
                (error "Parked Node.js REPL buffer died: %s" name))
              (with-current-buffer buffer (rename-buffer name t)))
          (error (push (list :restore-buffer condition) cleanup-errors))))
      (condition-case condition
          (when root-owned
            (when (file-exists-p root) (delete-directory root t)))
        (error (push (list :delete-root condition) cleanup-errors)))
      ;; Unicode filename handling can lazily create GNU's internal
      ;; code-conversion work buffer during the first cleanup pass.  Reap any
      ;; such reaction-created buffers before checking the shared baseline.
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case condition
              (progn
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error (push (list :kill-reaction-buffer
                               (buffer-name buffer) condition)
                         cleanup-errors))))))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer) (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process) (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     (append timer-list timer-idle-list)))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :window-restored (equal window-state-before
                                         (node405-test-window-state))
                 :buffer-restored (eq buffer-before (current-buffer))
                 :body-error (and body-error (node405-test-condition body-error))
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Node.js REPL workflow failed: %S" cleanup)
        (list :result (node405-test-normalize result) :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(NODEJS_REPL_MELPA_PIN, "nodejs-repl.el")
        .expect("prepare exact shallow Node.js REPL source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_start_creates_the_exact_node22_comint_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_start_creates_the_exact_node22_comint_session",
        r####"(node405-test-run nil 1
 (lambda (_root)
   (let* ((process (node405-test-start))
          (buffer (process-buffer process)))
     (with-current-buffer buffer
       (list :launches (reverse node405-test-launches)
             :versions (reverse node405-test-version-calls)
             :package-version nodejs-repl-version
             :node-version nodejs-repl-nodejs-version
             :mode major-mode
             :derived (and (derived-mode-p 'comint-mode) t)
             :process (list (process-name process) (process-status process)
                            (and (process-live-p process) t)
                            (process-query-on-exit-flag process))
             :prompt nodejs-repl-prompt
             :text (buffer-substring-no-properties (point-min) (point-max))
             :mark (marker-position (process-mark process))
             :settings (list comint-prompt-regexp comint-input-ignoredups
                             comint-process-echoes
                             (and
                              (memq #'nodejs-repl--completion-at-point-function
                                    completion-at-point-functions)
                              t))
             :keys (list (key-binding (kbd "TAB"))
                         (key-binding (kbd "C-c C-c"))))))))"####,
        expect![[
            r#"OK (:result (:launches ((:name "node405" :program "env" :startfile nil :switches ("TERM=xterm" "[NODE]" "-e" "require('repl').start({prompt: '> ', useGlobal: true, replMode: require('repl')['REPL_MODE_' + 'sloppy'.toUpperCase()], preview: false})") :environment ("C.UTF-8" nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil) :buffer "*node405*")) :versions ("[NODE] --version") :package-version "0.2.4" :node-version "22.22.2" :mode nodejs-repl-mode :derived t :process ("node405" run t t) :prompt "> " :text "> " :mark 3 :settings ("^" t t t) :keys (completion-at-point nodejs-repl-quit-or-cancel)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_source_commands_submit_line_region_expression_and_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_source_commands_submit_line_region_expression_and_file",
        r####"(node405-test-run
 '(("loaded.js" . "globalThis.releaseLabel = 'loaded 界';\n")) 1
 (lambda (root)
   (let* ((source (generate-new-buffer "release.js"))
          (process (node405-test-start))
          line region expression loaded value)
     (switch-to-buffer source)
     (js-mode)
     (insert "1 + 2\nconst café = '界';\ncafé;\n(40 + 2);\n")
     (goto-char (point-min))
     (setq line
           (node405-test-send-and-capture
            process (lambda () (call-interactively #'nodejs-repl-send-line))))
     (forward-line 1)
     (let ((start (line-beginning-position))
           (end (progn (forward-line 2) (line-beginning-position))))
       (setq region
             (node405-test-send-and-capture
              process (lambda () (nodejs-repl-send-region start end)))))
     (goto-char (point-max))
     (setq expression
           (node405-test-send-and-capture
            process
            (lambda () (call-interactively #'nodejs-repl-send-last-expression))))
     (setq loaded
           (node405-test-send-and-capture
            process
            (lambda ()
              (nodejs-repl-load-file (expand-file-name "loaded.js" root)))))
     (erase-buffer)
     (insert "globalThis.releaseLabel")
     (setq value
           (node405-test-send-and-capture
            process (lambda () (call-interactively #'nodejs-repl-send-line))))
     (list :line line :region region :expression expression
           :load loaded :loaded-value value
           :source-mode major-mode :source-text (buffer-string)))))"####,
        expect![[
            r#"OK (:result (:line "1 + 2\n3\n" :region ".editor\n// Entering editor mode (Ctrl+D to finish, Ctrl+C to cancel)\nconst café = '界';\ncafé;\n\n\n'界'\n" :expression ".editor\n// Entering editor mode (Ctrl+D to finish, Ctrl+C to cancel)\n(40 + 2);\n\n\n42\n" :load ".load [ROOT]/loaded.js\nglobalThis.releaseLabel = 'loaded 界';\n\n'loaded 界'\n" :loaded-value "globalThis.releaseLabel\n'loaded 界'\n" :source-mode js-mode :source-text "globalThis.releaseLabel") :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_completion_and_execute_use_the_live_repl() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_completion_and_execute_use_the_live_repl",
        r####"(node405-test-run
 '(("release notes 界.js" . "module.exports = 'fixture';\n")) 1
 (lambda (root)
   (let* ((source (generate-new-buffer "completion.js"))
          (process (node405-test-start))
          completion completed file-completion completed-file execute-output)
     (switch-to-buffer (process-buffer process))
     (goto-char (process-mark process))
     (insert "Math.ma")
     (setq completion (call-interactively #'completion-at-point))
     (setq completed (buffer-substring-no-properties
                      (process-mark process) (point-max)))
     (nodejs-repl-clear-line)
     (let ((inhibit-read-only t))
       (delete-region (process-mark process) (point-max))
       (goto-char (process-mark process))
       (setq default-directory root)
       (insert "'release n")
       (setq file-completion (call-interactively #'completion-at-point)
             completed-file (buffer-substring-no-properties
                             (process-mark process) (point-max))))
     (nodejs-repl-clear-line)
     (with-current-buffer source
       (js-mode)
       (nodejs-repl-minor-mode 1))
     (nodejs-repl-execute "['café', '界'].join(':')")
     (setq execute-output
           (with-current-buffer nodejs-repl-temp-buffer-name
             (buffer-substring-no-properties (point-min) (point-max))))
     (list :token-completion (list completion completed)
           :file-completion (list file-completion completed-file)
           :execute execute-output
           :minor (with-current-buffer source
                    (list nodejs-repl-minor-mode
                          (key-binding (kbd "C-c C-j"))))
           :cache (list nodejs-repl-cache-token
                        (copy-sequence nodejs-repl-cache-completions))))))"####,
        expect![[
            r#"OK (:result (:token-completion (t "Math.max") :file-completion (t "'release notes 界.js") :execute "'café:界'\n" :minor (t nodejs-repl-send-line) :cache ("Math.ma" ("Math.max"))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_error_then_buffer_submission_recovers_in_the_same_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_error_then_buffer_submission_recovers_in_the_same_session",
        r####"(node405-test-run nil 1
 (lambda (_root)
   (let* ((source (generate-new-buffer "recovery.js"))
          (process (node405-test-start))
          failure recovery switched)
     (switch-to-buffer source)
     (js-mode)
     (insert "throw new Error('boom café')")
     (setq failure
           (node405-test-send-and-capture
            process (lambda () (call-interactively #'nodejs-repl-send-buffer))))
     (erase-buffer)
     (insert "({status: 'recovered', glyph: '界'})")
     (setq recovery
           (node405-test-send-and-capture
            process (lambda () (call-interactively #'nodejs-repl-send-buffer))))
     (call-interactively #'nodejs-repl-switch-to-repl)
     (setq switched
           (list (eq (current-buffer) (process-buffer process))
                 (eq (window-buffer (selected-window)) (process-buffer process))
                 (and (process-live-p process) t)))
     (list :failure failure :recovery recovery :switched switched))))"####,
        expect![[
            r#"OK (:result (:failure ".editor\n// Entering editor mode (Ctrl+D to finish, Ctrl+C to cancel)\nthrow new Error('boom café')\n\nUncaught Error: boom café\n" :recovery ".editor\n// Entering editor mode (Ctrl+D to finish, Ctrl+C to cancel)\n({status: 'recovered', glyph: '界'})\n\n{ status: 'recovered', glyph: '界' }\n" :switched (t t t)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn nodejs_repl_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_start_creates_the_exact_node22_comint_session(),
        public_source_commands_submit_line_region_expression_and_file(),
        public_completion_and_execute_use_the_live_repl(),
        public_error_then_buffer_submission_recovers_in_the_same_session(),
    ];
    assert_oracle_batch_cases(oracle(), "nodejs-repl-rank405", "nodejs-repl", &cases);
}
