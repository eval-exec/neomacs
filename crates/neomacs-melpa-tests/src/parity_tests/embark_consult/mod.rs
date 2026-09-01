//! Practical parity for Embark Consult's documented integration workflows.
//!
//! The cases export real Consult location candidates to Occur and Grep,
//! navigate the resulting buffers, preserve Consult match styling, and collect
//! real Imenu/outline tables of contents from owned buffers.

use std::time::Duration;

use expect_test::expect;

use crate::{
    COMPAT_GNU_ELPA_PIN, CONSULT_MELPA_PIN, CachedMelpaOracle, EMBARK_CONSULT_MELPA_PIN,
    EMBARK_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'embark-consult)
(require 'grep)
(get-buffer-create " *code-conversion-work*")
(get-buffer-create " *Minibuf-1*")
(let ((executing-kbd-macro t)
      (unread-command-events (listify-key-sequence (kbd "RET"))))
  (minibuffer-with-setup-hook
      (lambda () (insert "baseline"))
    (completing-read "Embark Consult infrastructure baseline: "
                     '("baseline") nil t)))
(set-window-configuration (current-window-configuration))

(defconst ec408-test-source-manifest
  '(("embark-consult-pkg.el" . "c25307a35ea66daf4d144636410ae42df8cb5ab52a8732602d5a18493ec3c350")
    ("embark-consult.el" . "1c33aeee234e19282ddb3d6b255911dc22b764001e53d69a589b5af73e1948f7")))

(defun ec408-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let* ((loaded (symbol-file 'embark-consult-export-location-occur 'defun))
       (directory (and loaded (file-name-directory loaded)))
       (payload
        (and directory
             (sort
              (seq-filter
               (lambda (name)
                 (and (string-prefix-p "embark-consult" name)
                      (string-suffix-p ".el" name)
                      (not (string-suffix-p "-autoloads.el" name))))
               (directory-files directory nil nil t))
              #'string<))))
  (unless
      (and (file-regular-p loaded)
           (not (file-symlink-p loaded))
           (equal payload (mapcar #'car ec408-test-source-manifest))
           (cl-every
            (lambda (entry)
              (let ((file (expand-file-name (car entry) directory)))
                (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (ec408-test-file-sha256 file) (cdr entry)))))
            ec408-test-source-manifest))
    (error "Unexpected installed Embark Consult payload: %S" (list loaded payload))))

(defun ec408-test-normalize (value root)
  (cond
   ((stringp value)
    (replace-regexp-in-string
     (regexp-quote (directory-file-name root)) "[ROOT]" value t t))
   ((consp value)
    (cons (ec408-test-normalize (car value) root)
          (ec408-test-normalize (cdr value) root)))
   ((vectorp value)
    (apply #'vector
           (mapcar (lambda (item) (ec408-test-normalize item root)) value)))
   (t value)))

(defun ec408-test-window-state ()
  (mapcar
   (lambda (window)
     (list (buffer-name (window-buffer window))
           (window-point window)
           (window-start window)
           (window-dedicated-p window)))
   (seq-mapcat (lambda (frame) (window-list frame 'nomini)) (frame-list))))

(defun ec408-test-park-buffer (name)
  (when-let* ((buffer (get-buffer name)))
    (let ((old-name (buffer-name buffer)))
      (with-current-buffer buffer
        (rename-buffer (format " *ec408-parked-%s*" (sxhash-eq buffer)) t))
      (cons buffer old-name))))

(defun ec408-test-write-file (root relative contents)
  (let ((file (expand-file-name relative root)))
    (unless (file-in-directory-p file root)
      (error "Refusing Embark Consult fixture outside root: %s" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file file (insert contents)))
    (unless (and (file-regular-p file) (not (file-symlink-p file)))
      (error "Unsafe Embark Consult fixture: %s" file))
    file))

(defun ec408-test-manifest (root)
  (mapcar
   (lambda (file)
     (unless (and (file-regular-p file) (not (file-symlink-p file)))
       (error "Unsafe Embark Consult manifest entry: %s" file))
     (cons (file-relative-name file root) (ec408-test-file-sha256 file)))
   (sort (directory-files-recursively root "." nil nil t) #'string-lessp)))

(defun ec408-test-location (buffer line text &optional hidden-suffix)
  (with-current-buffer buffer
    (save-excursion
      (goto-char (point-min))
      (forward-line (1- line))
      (let* ((marker (copy-marker (point)))
             (candidate (concat text (or hidden-suffix ""))))
        (put-text-property 0 1 'consult-location (cons marker line) candidate)
        (when hidden-suffix
          (put-text-property (- (length candidate) (length hidden-suffix))
                             (length candidate) 'consult-strip t candidate))
        candidate))))

(defun ec408-test-property-runs ()
  (let ((properties '(occur-prefix occur-target occur-match font-lock-face
                      face read-only follow-link help-echo mouse-face
                      compilation-message wgrep-header wgrep-footer))
        (position (point-min))
        runs)
    (while (< position (point-max))
      (let ((next (next-property-change position nil (point-max)))
            values)
        (dolist (property properties)
          (when-let* ((value (get-text-property position property)))
            (push (list property
                        (cond ((markerp value) :marker)
                              ((and (consp value) (markerp (car value))) :location)
                              ((eq property 'compilation-message) :message)
                              (t value)))
                  values)))
        (when values
          (push (list position next
                      (buffer-substring-no-properties position next)
                      (nreverse values))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun ec408-test-locus ()
  (let* ((window (selected-window))
         (buffer (window-buffer window)))
    (with-current-buffer buffer
      (list :buffer (buffer-name buffer)
            :file buffer-file-name
            :point (window-point window)
            :line (line-number-at-pos (window-point window))
            :text (save-excursion
                    (goto-char (window-point window))
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))))))

(defun ec408-test-export-state ()
  (list :mode major-mode
        :name (buffer-name)
        :text (buffer-substring-no-properties (point-min) (point-max))
        :properties (ec408-test-property-runs)
        :matches (and (boundp 'grep-num-matches-found)
                      grep-num-matches-found)
        :next-error (eq next-error-last-buffer (current-buffer))
        :g-binding (key-binding (kbd "g"))
        :revert-remap (command-remapping #'revert-buffer)))

(defvar ec408-test-export-input nil)
(defvar ec408-test-export-ledger nil)
(defvar ec408-test-export-settle-p nil)
(defvar ec408-test-make-process-plan nil)
(defvar ec408-test-process-file-shell-plan nil)
(defvar ec408-test-grep-processes nil)
(defvar ec408-test-grep-terminals nil)
(defvar ec408-test-boundary-trace nil)
(defconst ec408-test-original-make-process (symbol-function 'make-process))

(defun ec408-test-make-process-boundary (&rest arguments)
  (unless ec408-test-make-process-plan
    (error "Unexpected Embark Consult make-process: %S" arguments))
  (funcall ec408-test-make-process-plan arguments))

(defun ec408-test-process-file-shell-boundary (command &rest arguments)
  (unless ec408-test-process-file-shell-plan
    (error "Unexpected Embark Consult shell process: %S"
           (cons command arguments)))
  (funcall ec408-test-process-file-shell-plan command arguments))

(defun ec408-test-grep-sentinel (real-sentinel process event)
  (let* ((command (process-command process))
         (input (nth (- (length command) 2) command))
         callback-error)
    (unless (seq-find (lambda (entry) (eq (cdr entry) process))
                      ec408-test-grep-processes)
      (push (cons input process) ec408-test-grep-processes))
    (condition-case condition
        (funcall real-sentinel process event)
      (error (setq callback-error condition)))
    (when (memq (process-status process) '(exit signal))
      (let* ((terminal
              (list :input (nth (- (length command) 2) command)
                    :status (process-status process)
                    :exit (process-exit-status process)
                    :event event
                    :callback-error callback-error)))
        (push terminal ec408-test-grep-terminals)))
    (when callback-error
      (signal (car callback-error) (cdr callback-error)))))

(defun ec408-test-export-command-observer ()
  (when (and (eq this-command 'ec408-test-export-settle)
             (not (eq (key-binding (kbd "<f13>")) this-command)))
    (error "Embark Consult settle command used the wrong binding"))
  (when (and (eq this-command 'ec408-test-export-ready)
             (not (eq (key-binding (kbd "<f14>")) this-command)))
    (error "Embark Consult ready command used the wrong binding"))
  (push (list :command this-command
              :keys (cond
                     ((eq this-command 'ec408-test-export-settle) "F13")
                     ((eq this-command 'ec408-test-export-ready) "F14")
                     (t (this-command-keys-vector)))
              :input (minibuffer-contents-no-properties))
        ec408-test-export-ledger))

(defun ec408-test-export-settle ()
  (interactive)
  (unless (minibufferp)
    (error "Embark Consult settle command escaped the minibuffer"))
  (let (terminal)
    (with-timeout
        (3 (error "Timed out waiting for Consult grep sentinel: %S"
                  (list ec408-test-grep-processes
                        ec408-test-grep-terminals
                        ec408-test-boundary-trace)))
      (while
          (not
           (setq terminal
                 (seq-find
                  (lambda (entry)
                    (equal (plist-get entry :input) "project"))
                  ec408-test-grep-terminals)))
        (accept-process-output nil 0.05)))
    (unless (equal terminal
                   '(:input "project" :status exit :exit 0
                     :event "finished\n" :callback-error nil))
      (error "Unexpected Consult grep terminal: %S" terminal)))
  (setq unread-command-events (list 'f14)))

(defun ec408-test-export-ready ()
  (interactive)
  (unless (minibufferp)
    (error "Embark Consult ready command escaped the minibuffer"))
  (let* ((raw (embark-minibuffer-candidates))
         (candidates (if (symbolp (car raw)) (cdr raw) raw)))
    (unless
        (and (= (length candidates) 1)
             (string-match-p
              "project Ω/alpha\\.el.*3:.*Café alpha project"
              (substring-no-properties (car candidates))))
      (error "Consult grep candidates did not settle: %S" candidates)))
  (unless (eq (key-binding (kbd "<f15>")) #'embark-export)
    (error "Embark Consult export command used the wrong binding"))
  (setq unread-command-events (list 'f15)))

(defun ec408-test-public-export-setup ()
  (use-local-map (copy-keymap (current-local-map)))
  (push (list :prompt (minibuffer-prompt)
              :initial (minibuffer-contents-no-properties)
              :category
              (completion-metadata-get
               (completion-metadata
                "" minibuffer-completion-table minibuffer-completion-predicate)
               'category))
        ec408-test-export-ledger)
  (add-hook 'post-command-hook #'ec408-test-export-command-observer nil t)
  (local-set-key (kbd "<f13>") #'ec408-test-export-settle)
  (local-set-key (kbd "<f14>") #'ec408-test-export-ready)
  (local-set-key (kbd "<f15>") #'embark-export)
  (local-set-key (kbd "C-c e") #'embark-export)
  (setq unread-command-events
        (append (string-to-list ec408-test-export-input)
                (listify-key-sequence
                 (kbd (if ec408-test-export-settle-p "<f13>" "C-c e")))
                unread-command-events)))

(defun ec408-test-drive-public-export
    (command input invoke mode name-prefix &optional input-prefix settle)
  (when (or unread-command-events (active-minibuffer-window))
    (error "Dirty Embark Consult input before export: %S"
           unread-command-events))
  (let ((executing-kbd-macro t)
        (this-command command)
        (real-this-command command)
        (ec408-test-export-input
         (if input-prefix (string-remove-prefix input-prefix input) input))
        (ec408-test-export-settle-p settle)
        (ec408-test-export-ledger nil)
        (command-errors nil)
        (command-error-function
         (lambda (data context caller)
           (push (list data context caller) command-errors))))
    (condition-case condition
        (with-timeout
            (8 (error "Timed out driving public Embark export: %S"
                      (list command unread-command-events (minibuffer-depth)
                            ec408-test-export-ledger command-errors
                            ec408-test-boundary-trace)))
          (minibuffer-with-setup-hook #'ec408-test-public-export-setup
            (funcall invoke)))
      (quit nil)
      (error (signal (car condition) (cdr condition))))
    ;; `embark--quit-and-run' intentionally uses a zero-delay timer when a
    ;; recursive edit exits before its post-command hook can run.  Let that
    ;; package-owned timer perform the deferred export setup.
    (with-timeout
        (2 (error "Timed out settling public Embark export: %S"
                  embark--run-after-command-functions))
      (while embark--run-after-command-functions
        (sit-for 0.01)))
    (let ((export
           (seq-find
            (lambda (buffer)
              (with-current-buffer buffer
                (and (derived-mode-p mode)
                     (string-prefix-p name-prefix
                                      (buffer-name buffer)))))
            (buffer-list))))
      (when export
        (pop-to-buffer export)))
    (unless (and (null unread-command-events)
                 (zerop (minibuffer-depth))
                 (derived-mode-p mode)
                 (string-prefix-p name-prefix
                                  (buffer-name)))
      (error "Incomplete public Embark Consult export: %S"
             (list unread-command-events (minibuffer-depth)
                   major-mode (buffer-name) ec408-test-export-ledger
                   embark--run-after-command-functions
                   ec408-test-boundary-trace)))
    (list :input input
          :setup (nreverse ec408-test-export-ledger))))

(defun ec408-test-drive-consult-line-export (input)
  (ec408-test-drive-public-export
   #'consult-line input (lambda () (call-interactively #'consult-line))
   'occur-mode "*Embark Export: consult-line - "))

(defun ec408-test-drive-public-rerun (binding export)
  (unless (eq binding #'embark-rerun-collect-or-export)
    (error "Public Embark Consult export has wrong rerun binding: %S"
           binding))
  (let ((executing-kbd-macro t)
        prompt input)
    (minibuffer-with-setup-hook
        (lambda ()
          (setq prompt (minibuffer-prompt)
                input (minibuffer-contents-no-properties)
                unread-command-events (listify-key-sequence (kbd "C-g"))))
      (condition-case condition
          (call-interactively binding)
        (quit nil)
        (error (signal (car condition) (cdr condition)))))
    (list :prompt prompt :input input
          :export-killed (not (buffer-live-p export))
          :events-consumed (null unread-command-events)
          :minibuffer-closed (zerop (minibuffer-depth)))))

(defun ec408-test-drive-consult-grep-export (root)
  (let* ((tool (expand-file-name "bin/grep" root))
         (expected-prefix
          (list tool "--null" "--line-buffered" "--color=never"
                "--ignore-case" "--with-filename" "--line-number"
                "-I" "-r" "-P" "-e"))
         (process-ledger nil)
         (capability-ledger nil)
         (grep-find-ignored-files nil)
         (grep-find-ignored-directories nil)
         (consult-grep-args
          (append (butlast expected-prefix 2) nil))
         (consult-async-input-throttle 0)
         (consult-async-input-debounce 0)
         (consult-async-refresh-delay 0)
         (ec408-test-grep-processes nil)
         (ec408-test-grep-terminals nil)
         (ec408-test-boundary-trace nil)
         (ec408-test-process-file-shell-plan
          (lambda (command arguments)
            (let ((expected
                   (concat
                    "echo xaxbx | "
                    (mapconcat #'shell-quote-argument
                               (list tool "-P" "^(?=.*b)(?=.*a)") " "))))
              (unless (and (null arguments)
                           (equal command expected)
                           (equal default-directory root))
                (error "Unexpected Consult grep capability process: %S"
                       (list command arguments default-directory)))
              (push (list :command command :cwd "[ROOT]" :exit 0)
                    capability-ledger)
              (push (list :capability command) ec408-test-boundary-trace)
              0)))
         (ec408-test-make-process-plan
          (lambda (arguments)
            (let* ((command (plist-get arguments :command))
                   (input (and (equal (butlast command 2) expected-prefix)
                               (nth (- (length command) 2) command)))
                   (keys (cl-loop for (key _value) on arguments by #'cddr
                                  collect key)))
              (unless
                  (and (= (length arguments) 16)
                       (equal keys
                              '(:file-handler :connection-type :name :stderr
                                :noquery :command :filter :sentinel))
                       (eq (plist-get arguments :file-handler) t)
                       (eq (plist-get arguments :connection-type) 'pipe)
                       (equal (plist-get arguments :name) tool)
                       (bufferp (plist-get arguments :stderr))
                       (eq (plist-get arguments :noquery) t)
                       (member input '("projec" "project"))
                       (equal (car (last command)) ".")
                       (functionp (plist-get arguments :filter))
                       (functionp (plist-get arguments :sentinel))
                       (equal default-directory root))
                (error "Unexpected Consult grep process boundary: %S"
                       (list arguments default-directory)))
              (push
               (list :command
                     (append (list "[TOOL]") (cdr command))
                     :cwd "[ROOT]"
                     :file-handler t
                     :connection-type 'pipe
                     :name "[TOOL]"
                     :stderr-buffer t
                     :noquery t
                     :filter t
                     :sentinel t)
               process-ledger)
              (push (list :process command) ec408-test-boundary-trace)
              (condition-case condition
                  (let* ((real-sentinel (plist-get arguments :sentinel))
                         (wrapped
                          (apply-partially #'ec408-test-grep-sentinel
                                           real-sentinel))
                         (process
                          (apply ec408-test-original-make-process
                                 (plist-put arguments :sentinel wrapped))))
                    (unless (seq-find
                             (lambda (entry) (eq (cdr entry) process))
                             ec408-test-grep-processes)
                      (push (cons input process) ec408-test-grep-processes))
                    (push (list :registered input (process-status process))
                          ec408-test-boundary-trace)
                    process)
                (error
                 (push (list :process-error condition)
                       ec408-test-boundary-trace)
                 (signal (car condition) (cdr condition)))))))
         launch export before rerun)
    (setq launch
          (ec408-test-drive-public-export
           #'consult-grep "project"
           (lambda () (consult-grep root "projec"))
           'grep-mode "*Embark Export: consult-grep - " "projec" t)
          export (current-buffer)
          before (ec408-test-export-state)
          rerun (ec408-test-drive-public-rerun
                 (key-binding (kbd "g")) export))
    (let ((owned-processes
           (seq-filter
            (lambda (process)
              (let ((command (process-command process)))
                (and (consp command) (equal (car command) tool))))
            (process-list))))
      (with-timeout
          (2 (error "Timed out reaping Consult grep processes: %S"
                    owned-processes))
        (while (seq-some #'process-live-p owned-processes)
          (dolist (process owned-processes)
            (when (process-live-p process)
              (accept-process-output process 0.05))))))
    (list :launch launch :capability-ledger (nreverse capability-ledger)
          :process-ledger (nreverse process-ledger) :export before
          :terminals (nreverse ec408-test-grep-terminals)
          :rerun rerun)))

(defun ec408-test-forbid-external (kind &rest arguments)
  (error "Unexpected Embark Consult external boundary: %S"
         (cons kind arguments)))

(defun ec408-test-silent-message (format-string &rest arguments)
  (when format-string
    (apply #'format-message format-string arguments)))

(defun ec408-test-run (body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "embark-consult/" sandbox))))
         (window-before (current-window-configuration))
         (window-state-before (ec408-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (next-error-before next-error-last-buffer)
         (vc-handled-backends nil)
         (enable-dir-local-variables nil)
         (unread-command-events nil)
         (executing-kbd-macro nil)
         (embark--run-after-command-functions nil)
         (print-circle nil)
         (message-log-max nil)
         (inhibit-message t)
         (ring-bell-function #'ignore)
         (root-owned nil)
         (parked nil)
         fixture-before fixture-after result body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Embark Consult sandbox root"))
              (when (file-exists-p root)
                (error "Embark Consult sandbox root already exists: %s" root))
              (dolist (name '("*Embark Export Occur*" "*Embark Export Grep*"
                              "alpha.el" "beta.el"
                              "*ec408-memory Ω*" "*ec408-outline Ω*"
                              "*ec408-public-line Ω*"))
                (when-let* ((entry (ec408-test-park-buffer name)))
                  (push entry parked)))
              (make-directory root t)
              (setq root-owned t)
              (ec408-test-write-file
               root "project Ω/alpha.el"
               ";;; Alpha\n(defun alpha-界 ()\n  \"Café alpha project.\"\n  t)\n\n;;; Section Ω\n(defun beta () t)\n")
              (ec408-test-write-file
               root "project Ω/beta.el"
               ";;; Beta\n(defun gamma ()\n  \"Gamma.\"\n  nil)\n")
              (let ((tool
                     (ec408-test-write-file
                      root "bin/grep"
                      (concat
                       "#!/bin/sh\n"
                       "[ \"$#\" -eq 12 ] || exit 64\n"
                       "[ \"$1\" = --null ] || exit 64\n"
                       "[ \"$2\" = --line-buffered ] || exit 64\n"
                       "[ \"$3\" = --color=never ] || exit 64\n"
                       "[ \"$4\" = --ignore-case ] || exit 64\n"
                       "[ \"$5\" = --with-filename ] || exit 64\n"
                       "[ \"$6\" = --line-number ] || exit 64\n"
                       "[ \"$7\" = -I ] || exit 64\n"
                       "[ \"$8\" = -r ] || exit 64\n"
                       "[ \"$9\" = -P ] || exit 64\n"
                       "[ \"${10}\" = -e ] || exit 64\n"
                       "{ [ \"${11}\" = projec ] || [ \"${11}\" = project ]; } || exit 64\n"
                       "[ \"${12}\" = . ] || exit 64\n"
                       "printf 'project Ω/alpha.el\\0003:  \"Café alpha project.\"\\n'\n"))))
                (set-file-modes tool #o700)
                (unless (file-executable-p tool)
                  (error "Embark Consult grep stand-in is not executable: %s"
                         tool)))
              (setq fixture-before (ec408-test-manifest root))
              (let ((default-directory root)
                    (next-error-last-buffer nil))
                (setq result
                      (cl-letf (((symbol-function 'message)
                                 #'ec408-test-silent-message)
                                ((symbol-function 'ding) #'ignore)
                                ((symbol-function 'start-process)
                                 (lambda (&rest arguments)
                                   (apply #'ec408-test-forbid-external
                                          'start-process arguments)))
                                ((symbol-function 'call-process)
                                 (lambda (&rest arguments)
                                   (apply #'ec408-test-forbid-external
                                          'call-process arguments)))
                                ((symbol-function 'process-file)
                                 (lambda (&rest arguments)
                                   (apply #'ec408-test-forbid-external
                                          'process-file arguments)))
                                ((symbol-function 'process-file-shell-command)
                                 #'ec408-test-process-file-shell-boundary)
                                ((symbol-function 'make-process)
                                 #'ec408-test-make-process-boundary)
                                ((symbol-function 'make-network-process)
                                 (lambda (&rest arguments)
                                   (apply #'ec408-test-forbid-external
                                          'make-network-process arguments)))
                                ((symbol-function 'url-retrieve)
                                 (lambda (&rest arguments)
                                   (apply #'ec408-test-forbid-external
                                          'url-retrieve arguments))))
                        (funcall body root))))
              (setq fixture-after (ec408-test-manifest root)))
          (error (setq body-error condition)))
      (dolist (process (seq-difference (process-list) processes-before #'eq))
        (condition-case condition
            (when (process-live-p process) (delete-process process))
          (error (push condition cleanup-errors))))
      (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
        (condition-case condition
            (when (buffer-live-p buffer) (kill-buffer buffer))
          (error (push condition cleanup-errors))))
      (dolist (timer (seq-difference timer-list timers-before #'eq))
        (condition-case condition
            (cancel-timer timer)
          (error (push condition cleanup-errors))))
      (dolist (frame (seq-difference (frame-list) frames-before #'eq))
        (condition-case condition
            (when (frame-live-p frame) (delete-frame frame t))
          (error (push condition cleanup-errors))))
      (condition-case condition
          (set-window-configuration window-before)
        (error (push condition cleanup-errors)))
      (condition-case condition
          (when (buffer-live-p buffer-before) (set-buffer buffer-before))
        (error (push condition cleanup-errors)))
      (dolist (entry parked)
        (condition-case condition
            (if (buffer-live-p (car entry))
                (with-current-buffer (car entry)
                  (rename-buffer (cdr entry) t))
              (error "Parked Embark Consult buffer died: %s" (cdr entry)))
          (error (push condition cleanup-errors))))
      (condition-case condition
          (when root-owned (delete-directory root t))
        (error (push condition cleanup-errors))))
    (let ((cleanup
           (list :fixture-stable (equal fixture-before fixture-after)
                 :root-removed (not (and root (file-exists-p root)))
                 :residual-buffers
                 (mapcar #'buffer-name
                         (seq-difference (buffer-list) buffers-before #'eq))
                 :buffers-restored (null (seq-difference
                                          (buffer-list) buffers-before #'eq))
                 :baseline-buffers-live (cl-every #'buffer-live-p buffers-before)
                 :processes-restored (null (seq-difference
                                            (process-list) processes-before #'eq))
                 :timers-restored (null (seq-difference
                                         timer-list timers-before #'eq))
                 :frames-restored (equal (frame-list) frames-before)
                 :windows-restored
                 (equal (ec408-test-window-state) window-state-before)
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :next-error-restored (eq next-error-last-buffer next-error-before)
                 :input-clean (and (null unread-command-events)
                                   (zerop (minibuffer-depth)))
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Embark Consult workflow failed: %S" (list result cleanup))
        (ec408-test-normalize
         (list :source (copy-tree ec408-test-source-manifest)
               :result result :cleanup cleanup)
         root)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EMBARK_CONSULT_MELPA_PIN, "embark-consult.el")
        .expect("prepare exact shallow Embark Consult source below ./tmp")
        .with_melpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare exact Compat dependency below ./tmp")
        .with_melpa_dependency(CONSULT_MELPA_PIN)
        .expect("prepare exact Consult dependency below ./tmp")
        .with_melpa_dependency(EMBARK_MELPA_PIN)
        .expect("prepare exact Embark dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn consult_line_public_export_reruns_and_navigates() -> ParityBatchCase {
    ParityBatchCase::value(
        "consult_line_public_export_reruns_and_navigates",
        r####"
(ec408-test-run
 (lambda (root)
   (let* ((source (generate-new-buffer "*ec408-public-line Ω*"))
          launch export before first second back rerun)
     (with-current-buffer source
       (fundamental-mode)
       (insert "alpha one\nbeta two\nalpha three\n"))
     (switch-to-buffer source)
     (setq launch (ec408-test-drive-consult-line-export "alpha")
           export (current-buffer)
           before (ec408-test-export-state))
     (unless (eq (alist-get 'consult-location embark-exporters-alist)
                 #'embark-consult-export-location-occur)
       (error "Embark Consult location exporter is not registered"))
     (progn
       (goto-char (point-min))
       (next-error 1)
       (setq first (ec408-test-locus))
       (next-error 1)
       (setq second (ec408-test-locus))
       (previous-error 1)
       (setq back (ec408-test-locus)))
     (pop-to-buffer export)
     (setq rerun
           (ec408-test-drive-public-rerun
            (command-remapping #'revert-buffer) export))
     (list :launch launch :export before
           :first first :second second :back back
           :rerun rerun))))
"####,
        expect![[
            r#"OK (:source (("embark-consult-pkg.el" . "c25307a35ea66daf4d144636410ae42df8cb5ab52a8732602d5a18493ec3c350") ("embark-consult.el" . "1c33aeee234e19282ddb3d6b255911dc22b764001e53d69a589b5af73e1948f7")) :result (:launch (:input "alpha" :setup ((:prompt "Go to line: " :initial "" :category consult-location) (:command consult-line :keys [] :input "") (:command self-insert-command :keys [97] :input "a") (:command self-insert-command :keys [108] :input "al") (:command self-insert-command :keys [112] :input "alp") (:command self-insert-command :keys [104] :input "alph") (:command self-insert-command :keys [97] :input "alpha"))) :export (:mode occur-mode :name "*Embark Export: consult-line - alpha*" :text "lines from buffer: *ec408-public-line Ω*\n      1:alpha one\n      3:alpha three\n" :properties ((1 42 "lines from buffer: *ec408-public-line Ω*\n" ((face underline) (read-only t))) (42 50 "      1:" ((occur-prefix t) (occur-target :marker) (font-lock-face shadow) (read-only t) (follow-link t) (help-echo "mouse-2: go to this occurrence") (mouse-face highlight))) (50 51 "a" ((occur-target :marker) (occur-match t) (face completions-common-part) (follow-link t) (help-echo "mouse-2: go to this occurrence") (mouse-face highlight))) (51 55 "lpha" ((occur-target :marker) (occur-match t) (face completions-common-part) (follow-link t) (help-echo "mouse-2: go to this occurrence") (mouse-face highlight))) (55 56 " " ((occur-target :marker) (occur-match t) (face completions-first-difference) (follow-link t) (help-echo "mouse-2: go to this occurrence") (mouse-face highlight))) (56 59 "one" ((occur-target :marker) (occur-match t) (follow-link t) (help-echo "mouse-2: go to this occurrence") (mouse-face highlight))) (59 60 "\n" ((occur-target :marker))) (60 68 "      3:" ((occur-prefix t) (occur-target :marker) (font-lock-face shadow) (read-only t) (follow-link t) (help-echo "mouse-2: go to this occurrence") (mouse-face highlight))) (68 69 "a" ((occur-target :marker) (occur-match t) (face completions-common-part) (follow-link t) (help-echo "mouse-2: go to this occurrence") (mouse-face highlight))) (69 73 "lpha" ((occur-target :marker) (occur-match t) (face completions-common-part) (follow-link t) (help-echo "mouse-2: go to this occurrence") (mouse-face highlight))) (73 74 " " ((occur-target :marker) (occur-match t) (face completions-first-difference) (follow-link t) (help-echo "mouse-2: go to this occurrence") (mouse-face highlight))) (74 79 "three" ((occur-target :marker) (occur-match t) (follow-link t) (help-echo "mouse-2: go to this occurrence") (mouse-face highlight))) (79 80 "\n" ((occur-target :marker)))) :matches 0 :next-error t :g-binding embark-rerun-collect-or-export :revert-remap embark-rerun-collect-or-export) :first (:buffer "*ec408-public-line Ω*" :file nil :point 1 :line 1 :text "alpha one") :second (:buffer "*ec408-public-line Ω*" :file nil :point 20 :line 3 :text "alpha three") :back (:buffer "*ec408-public-line Ω*" :file nil :point 1 :line 1 :text "alpha one") :rerun (:prompt "Go to line: " :input "alpha" :export-killed t :events-consumed t :minibuffer-closed t)) :cleanup (:fixture-stable t :root-removed t :residual-buffers nil :buffers-restored t :baseline-buffers-live t :processes-restored t :timers-restored t :frames-restored t :windows-restored t :buffer-restored t :next-error-restored t :input-clean t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn consult_grep_public_export_reruns() -> ParityBatchCase {
    ParityBatchCase::value(
        "consult_grep_public_export_reruns",
        r####"
(ec408-test-run
 (lambda (root)
   (ec408-test-drive-consult-grep-export root)))
"####,
        expect![[r##"OK (:value (:source (("embark-consult-pkg.el" . "c25307a35ea66daf4d144636410ae42df8cb5ab52a8732602d5a18493ec3c350") ("embark-consult.el" . "1c33aeee234e19282ddb3d6b255911dc22b764001e53d69a589b5af73e1948f7")) :result (:launch (:input "project" :setup ((:prompt "Grep (…/consult-grep-public…/embark-consult): " :initial "projec" :category consult-grep) (:command consult-grep :keys [] :input "#projec") (:command self-insert-command :keys [116] :input "#project") (:command ec408-test-export-settle :keys "F13" :input "#project") (:command ec408-test-export-ready :keys "F14" :input "#project"))) :capability-ledger ((:command "echo xaxbx | [ROOT]/bin/grep -P \\^\\(\\?\\=.\\*b\\)\\(\\?\\=.\\*a\\)" :cwd "[ROOT]" :exit 0) (:command "echo xaxbx | [ROOT]/bin/grep -P \\^\\(\\?\\=.\\*b\\)\\(\\?\\=.\\*a\\)" :cwd "[ROOT]" :exit 0)) :process-ledger ((:command ("[TOOL]" "--null" "--line-buffered" "--color=never" "--ignore-case" "--with-filename" "--line-number" "-I" "-r" "-P" "-e" "projec" ".") :cwd "[ROOT]" :file-handler t :connection-type pipe :name "[TOOL]" :stderr-buffer t :noquery t :filter t :sentinel t) (:command ("[TOOL]" "--null" "--line-buffered" "--color=never" "--ignore-case" "--with-filename" "--line-number" "-I" "-r" "-P" "-e" "project" ".") :cwd "[ROOT]" :file-handler t :connection-type pipe :name "[TOOL]" :stderr-buffer t :noquery t :filter t :sentinel t)) :export (:mode grep-mode :name "*Embark Export: consult-grep - #project*" :text "Exported grep results:\n\nproject Ω/alpha.el:3:  \"Café alpha project.\"\n" :properties ((1 25 "Exported grep results:\n\n" ((wgrep-header t))) (25 43 "project Ω/alpha.el" ((font-lock-face (compilation-info underline)) (face consult-file) (help-echo "mouse-2: visit this file, line and column") (mouse-face highlight) (compilation-message :message))) (43 44 ":" ((font-lock-face (underline)) (help-echo "mouse-2: visit this file, line and column") (mouse-face highlight) (compilation-message :message))) (44 45 "3" ((font-lock-face (compilation-line-number underline)) (face consult-line-number) (help-echo "mouse-2: visit this file, line and column") (mouse-face highlight) (compilation-message :message))) (45 46 ":" ((font-lock-face (underline)) (help-echo "mouse-2: visit this file, line and column") (mouse-face highlight) (compilation-message :message))) (60 67 "project" ((font-lock-face match) (face consult-highlight-match)))) :matches 1 :next-error t :g-binding embark-rerun-collect-or-export :revert-remap embark-rerun-collect-or-export) :terminals ((:input "projec" :status signal :exit 9 :event "killed\n" :callback-error nil) (:input "project" :status exit :exit 0 :event "finished\n" :callback-error nil)) :rerun (:prompt "Grep (…/consult-grep-public…/embark-consult): " :input "#project" :export-killed t :events-consumed t :minibuffer-closed t)) :cleanup (:fixture-stable t :root-removed t :residual-buffers nil :buffers-restored t :baseline-buffers-live t :processes-restored t :timers-restored t :frames-restored t :windows-restored t :buffer-restored t :next-error-restored t :input-clean t :body-error nil :cleanup-errors nil)) :stdout "" :stderr "")"##]],
    )
    .direct_command_loop()
}

fn location_export_to_grep_filters_non_file_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "location_export_to_grep_filters_non_file_buffers",
        r####"
(ec408-test-run
 (lambda (root)
   (let* ((alpha (find-file-noselect (expand-file-name "project Ω/alpha.el" root)))
          (memory (generate-new-buffer "*ec408-memory Ω*"))
          (lines nil)
          message)
     (with-current-buffer memory (insert "Memory café\n"))
     (setq lines (list (ec408-test-location alpha 2 "(defun alpha-界 ()" " λ")
                       (ec408-test-location memory 1 "Memory café" " λ")))
     (cl-letf (((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (setq message (apply #'format format-string arguments)))))
       (embark-consult-export-location-grep lines))
     (let* ((export (current-buffer))
            (before (ec408-test-export-state)))
       (goto-char (point-min))
       (next-error 1)
       (list :export before :message message
             :destination (ec408-test-locus))))))
"####,
        expect![[
            r#"OK (:source (("embark-consult-pkg.el" . "c25307a35ea66daf4d144636410ae42df8cb5ab52a8732602d5a18493ec3c350") ("embark-consult.el" . "1c33aeee234e19282ddb3d6b255911dc22b764001e53d69a589b5af73e1948f7")) :result (:export (:mode grep-mode :name "*Embark Export Grep*" :text "Exported line search results (file-backed buffers only):\n\nproject Ω/alpha.el:2:(defun alpha-界 ()\n\nSome results were in buffers with no associated file and are missing\nfrom the exported result:\n- *ec408-memory Ω*\n\nEither save the buffers or use the `embark-consult-export-location-occur'\nexporter." :properties ((1 59 "Exported line search results (file-backed buffers only):\n\n" ((wgrep-header t))) (59 77 "project Ω/alpha.el" ((font-lock-face (compilation-info underline)) (help-echo "mouse-2: visit this file and line") (mouse-face highlight) (compilation-message :message))) (77 78 ":" ((font-lock-face (underline)) (help-echo "mouse-2: visit this file and line") (mouse-face highlight) (compilation-message :message))) (78 79 "2" ((font-lock-face (compilation-line-number underline)) (help-echo "mouse-2: visit this file and line") (mouse-face highlight) (compilation-message :message))) (79 80 ":" ((font-lock-face (underline)) (help-echo "mouse-2: visit this file and line") (mouse-face highlight) (compilation-message :message))) (98 297 "\nSome results were in buffers with no associated file and are missing\nfrom the exported result:\n- *ec408-memory Ω*\n\nEither save the buffers or use the `embark-consult-export-location-occur'\nexporter." ((read-only t) (wgrep-footer t)))) :matches 1 :next-error t :g-binding embark-rerun-collect-or-export :revert-remap nil) :message "This exporter does not support non-file buffers: (*ec408-memory Ω*)" :destination (:buffer "alpha.el" :file "[ROOT]/project Ω/alpha.el" :point 11 :line 2 :text "(defun alpha-界 ()")) :cleanup (:fixture-stable t :root-removed t :residual-buffers nil :buffers-restored t :baseline-buffers-live t :processes-restored t :timers-restored t :frames-restored t :windows-restored t :buffer-restored t :next-error-restored t :input-clean t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn grep_export_preserves_matches_and_navigation() -> ParityBatchCase {
    ParityBatchCase::value(
        "grep_export_preserves_matches_and_navigation",
        r####"
(ec408-test-run
 (lambda (root)
   (let* ((relative "project Ω/alpha.el")
          (line (concat relative ":2:(defun alpha-界 ()"))
          (start (+ (length relative) 3 (length "(defun ")))
          (end (+ start (length "alpha-界"))))
     (put-text-property start end 'face 'consult-highlight-match line)
     (embark-consult-export-grep (list line))
     (let* ((export (current-buffer))
            (before (ec408-test-export-state)))
       (goto-char (point-min))
       (next-error 1)
       (list :export before :destination (ec408-test-locus))))))
"####,
        expect![[
            r#"OK (:source (("embark-consult-pkg.el" . "c25307a35ea66daf4d144636410ae42df8cb5ab52a8732602d5a18493ec3c350") ("embark-consult.el" . "1c33aeee234e19282ddb3d6b255911dc22b764001e53d69a589b5af73e1948f7")) :result (:export (:mode grep-mode :name "*Embark Export Grep*" :text "Exported grep results:\n\nproject Ω/alpha.el:2:(defun alpha-界 ()\n" :properties ((1 25 "Exported grep results:\n\n" ((wgrep-header t))) (25 43 "project Ω/alpha.el" ((font-lock-face (compilation-info underline)) (help-echo "mouse-2: visit this file, line and column") (mouse-face highlight) (compilation-message :message))) (43 44 ":" ((font-lock-face (underline)) (help-echo "mouse-2: visit this file, line and column") (mouse-face highlight) (compilation-message :message))) (44 45 "2" ((font-lock-face (compilation-line-number underline)) (help-echo "mouse-2: visit this file, line and column") (mouse-face highlight) (compilation-message :message))) (45 46 ":" ((font-lock-face (underline)) (help-echo "mouse-2: visit this file, line and column") (mouse-face highlight) (compilation-message :message))) (53 60 "alpha-界" ((font-lock-face match) (face consult-highlight-match)))) :matches 1 :next-error t :g-binding embark-rerun-collect-or-export :revert-remap nil) :destination (:buffer "alpha.el" :file "[ROOT]/project Ω/alpha.el" :point 18 :line 2 :text "(defun alpha-界 ()")) :cleanup (:fixture-stable t :root-removed t :residual-buffers nil :buffers-restored t :baseline-buffers-live t :processes-restored t :timers-restored t :frames-restored t :windows-restored t :buffer-restored t :next-error-restored t :input-clean t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_collectors_return_real_imenu_and_outline_locations() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_collectors_return_real_imenu_and_outline_locations",
        r####"
(ec408-test-run
 (lambda (root)
   (let* ((alpha (find-file-noselect (expand-file-name "project Ω/alpha.el" root)))
          (outline (generate-new-buffer "*ec408-outline Ω*"))
          imenu outline-items combined-prog combined-outline)
     (with-current-buffer alpha
       (emacs-lisp-mode)
       (setq imenu (embark-consult-imenu-candidates)
             combined-prog (embark-consult-imenu-or-outline-candidates)))
     (with-current-buffer outline
       (insert "* Release Ω\nBody\n** Verify café\nDetails\n")
       (outline-mode)
       (setq outline-items (embark-consult-outline-candidates)
             combined-outline (embark-consult-imenu-or-outline-candidates)))
     (cl-labels
         ((semantic (value)
            (cons (car value)
                  (mapcar
                   (lambda (candidate)
                     (let* ((location (get-text-property
                                       0 'consult-location candidate))
                            (position (car location))
                            (buffer (if (markerp position)
                                        (marker-buffer position)
                                      (car position)))
                            (point (if (markerp position)
                                       (marker-position position)
                                     (cdr position)))
                            (transformed
                             (funcall
                              (alist-get 'consult-location
                                         embark-transformer-alist)
                              'consult-location candidate)))
                       (list :text (substring-no-properties candidate)
                             :clean (substring-no-properties (cdr transformed))
                             :strip-property
                             (and (text-property-not-all
                                   0 (length candidate) 'consult-strip nil candidate)
                                  t)
                             :line (cdr location)
                             :point point
                             :buffer (and (buffer-live-p buffer)
                                          (buffer-name buffer)))))
                   (cdr value)))))
       (list :imenu (list (car imenu)
                          (mapcar #'substring-no-properties (cdr imenu)))
             :outline (semantic outline-items)
             :combined-prog (list (car combined-prog)
                                  (mapcar #'substring-no-properties
                                          (cdr combined-prog)))
             :combined-outline (semantic combined-outline))))))
"####,
        expect![[
            r#"OK (:source (("embark-consult-pkg.el" . "c25307a35ea66daf4d144636410ae42df8cb5ab52a8732602d5a18493ec3c350") ("embark-consult.el" . "1c33aeee234e19282ddb3d6b255911dc22b764001e53d69a589b5af73e1948f7")) :result (:imenu (imenu ("Functions alpha-界" "Functions beta")) :outline (consult-location (:text "* Release Ω􀀁" :clean "* Release Ω" :strip-property t :line 1 :point 1 :buffer "*ec408-outline Ω*") (:text "** Verify café􀀃" :clean "** Verify café" :strip-property t :line 3 :point 18 :buffer "*ec408-outline Ω*")) :combined-prog (imenu ("Functions alpha-界" "Functions beta")) :combined-outline (consult-location (:text "* Release Ω􀀁" :clean "* Release Ω" :strip-property t :line 1 :point 1 :buffer "*ec408-outline Ω*") (:text "** Verify café􀀃" :clean "** Verify café" :strip-property t :line 3 :point 18 :buffer "*ec408-outline Ω*"))) :cleanup (:fixture-stable t :root-removed t :residual-buffers nil :buffers-restored t :baseline-buffers-live t :processes-restored t :timers-restored t :frames-restored t :windows-restored t :buffer-restored t :next-error-restored t :input-clean t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn embark_consult_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        consult_line_public_export_reruns_and_navigates(),
        consult_grep_public_export_reruns(),
        location_export_to_grep_filters_non_file_buffers(),
        grep_export_preserves_matches_and_navigation(),
        public_collectors_return_real_imenu_and_outline_locations(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        std::thread::current()
            .name()
            .unwrap_or("unnamed Embark Consult parity test"),
        "embark_consult_parity",
        &cases,
    );
}
