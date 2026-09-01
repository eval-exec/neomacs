//! Practical parity for the obsolete Color Theme package's public workflows.
//!
//! The cases preserve the package's prominent obsolescence warning, drive its
//! selection buffer and commands, install and reset an owned theme, exercise
//! frame-local installation, and pin cumulative history behavior.

use std::time::Duration;

use expect_test::expect;

use crate::{COLOR_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)

(defvar color-theme420-test-load-warnings nil)
(defvar color-theme-obsolete t)
(let ((display-warning-before (symbol-function 'display-warning)))
  (cl-letf (((symbol-function 'display-warning)
             (lambda (type message &optional level buffer-name)
               (if (eq type 'color-theme)
                   (push (list :type type :message message
                               :level level :buffer buffer-name)
                         color-theme420-test-load-warnings)
                 (funcall display-warning-before
                          type message level buffer-name)))))
    (require 'color-theme)))
(setq color-theme420-test-load-warnings
      (nreverse color-theme420-test-load-warnings))
(set-window-configuration (current-window-configuration))

(defconst color-theme420-test-upstream-tree
  "851cab0385a2f62447631e0b1c912f08df48dabe")
(defconst color-theme420-test-installed-manifest
  '(("color-theme-pkg.el" . "7b55dc15e862fdbcc46038dfe9ca18f44ed1dba180124f8dd8ebb6d2baf490bd")
    ("color-theme.el" . "ba3ef8b8286a1b80d313a10743ff55bac1d677db1d4829cb35b6ecf6238976cb")
    ("themes/color-theme-example.el" . "67562d9686d7b5c4fb439af21e85e590e5905b1658913a2ab8d821b9bf536721")
    ("themes/color-theme-library.el" . "e3d281717052ef052be239ec0f5db227987d571bc043085b6fb38d9c24f954cb")))

(defun color-theme420-test-file-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun color-theme420-test-source-state ()
  (let* ((main (symbol-file 'color-theme-select 'defun))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<))))
    (unless (and main
                 (not (file-symlink-p main))
                 (equal files (mapcar #'car color-theme420-test-installed-manifest)))
      (error "Unexpected installed Color Theme payload: %S" files))
    (dolist (entry color-theme420-test-installed-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (color-theme420-test-file-sha file) (cdr entry)))
          (error "Unexpected installed Color Theme source: %S" entry))))
    (list :tree color-theme420-test-upstream-tree
          :manifest color-theme420-test-installed-manifest
          :feature (featurep 'color-theme)
          :version "20190220.1115")))

(defvar color-theme420-test-case-index 0)
(defvar color-theme420-test-root nil)
(defvar color-theme420-test-root-owned nil)
(defvar color-theme420-test-install-ledger nil)
(defvar color-theme420-accent-color "ambient")

(defun color-theme420-test-install-compatibility-alias ()
  (unless (fboundp 'user-variable-p)
    ;; Color Theme 6.6.1 predates this obsolete alias's removal.  Install the
    ;; historical alias only after the native public failure is observed.
    (defalias 'user-variable-p #'custom-variable-p)))

(defun color-theme420-owned ()
  "Install the owned Café theme used by the rank-420 parity corpus."
  (interactive)
  (push (list :global color-theme-is-global
              :cumulative color-theme-is-cumulative)
        color-theme420-test-install-ledger)
  (color-theme-install
   '(color-theme420-owned
     ((background-mode . dark))
     ((color-theme420-accent-color . "café 界"))
     (color-theme420-reset-face
      ((t (:foreground "red" :background "black" :weight bold)))))))

(defun color-theme420-frame-local ()
  "Install an owned face only on the selected frame."
  (interactive)
  (push (list :global color-theme-is-global
              :cumulative color-theme-is-cumulative)
        color-theme420-test-install-ledger)
  (color-theme-install
   '(color-theme420-frame-local
     ((foreground-color . "purple")
      (background-color . "gray90")
      (background-mode . light))
     nil
     (color-theme420-local-face
      ((t (:foreground "purple" :background "gray90" :weight bold)))))))

(defun color-theme420-first ())
(defun color-theme420-second ())
(defun color-theme420-third ())

(defun color-theme420-test-condition (condition)
  (list :type (car condition)
        :data (copy-tree (cdr condition))
        :message (error-message-string condition)))

(defun color-theme420-test-window-state ()
  (mapcar
   (lambda (window)
     (list :window window
           :selected (eq window (selected-window))
           :buffer (window-buffer window)
           :point (window-point window)
           :start (window-start window)
           :hscroll (window-hscroll window)
           :dedicated (window-dedicated-p window)
           :edges (window-edges window)))
   (seq-mapcat (lambda (frame) (window-list frame 'nomini)) (frame-list))))

(defun color-theme420-test-park-buffer (name)
  (when-let ((buffer (get-buffer name)))
    (let ((parked (generate-new-buffer-name
                   (format " *color-theme420-parked:%s*" name))))
      (with-current-buffer buffer (rename-buffer parked t))
      (cons buffer name))))

(defun color-theme420-test-prepare-face (face)
  (when (facep face)
    (error "Owned Color Theme face unexpectedly exists: %S" face))
  (make-empty-face face)
  (face-spec-set face
                 '((t (:foreground "green" :background "white"
                                    :weight normal)))))

(defun color-theme420-test-face-state (face &optional frame)
  (list :foreground
        (face-attribute face :foreground frame 'default)
        :background
        (face-attribute face :background frame 'default)
        :weight
        (face-attribute face :weight frame 'default)))

(defun color-theme420-test-selection-row (position)
  (save-excursion
    (goto-char position)
    (let ((start (line-beginning-position))
          (end (line-end-position)))
      (list :text (buffer-substring-no-properties start end)
            :theme (get-text-property start 'color-theme)
            :face (get-text-property start 'face)
            :mouse-face (get-text-property start 'mouse-face)))))

(defun color-theme420-test-selection-state ()
  (list :name (buffer-name)
        :mode major-mode
        :read-only buffer-read-only
        :modified (buffer-modified-p)
        :point (point)
        :rows
        (let (rows)
          (save-excursion
            (goto-char (point-min))
            (while (not (eobp))
              (push (color-theme420-test-selection-row (point)) rows)
              (forward-line 1)))
          (nreverse rows))
        :keys
        (mapcar (lambda (key)
                  (list key (lookup-key (current-local-map) (kbd key))))
                '("RET" "i" "l" "d" "p" "q"))))

(defun color-theme420-test-theme-position (theme)
  (or (text-property-any (point-min) (point-max) 'color-theme theme)
      (error "Theme missing from selection buffer: %S" theme)))

(defun color-theme420-test-run (body)
  (let* ((index (cl-incf color-theme420-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "color-theme-%d" index)
                                       sandbox))))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (buffer-before (current-buffer))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (window-state-before (color-theme420-test-window-state))
         (frame-colors-before
          (mapcar (lambda (frame)
                    (list frame
                          (frame-parameter frame 'foreground-color)
                          (frame-parameter frame 'background-color)
                          (frame-parameter frame 'background-mode)))
                  frames-before))
         (source-before (color-theme420-test-source-state))
         (snapshot-function-before (symbol-function 'color-theme-snapshot))
         (snapshot-plist-before (copy-tree (symbol-plist 'color-theme-snapshot)))
         (compatibility-bound-before (fboundp 'user-variable-p))
         (compatibility-function-before
          (and compatibility-bound-before (symbol-function 'user-variable-p)))
         (color-theme420-test-root root)
         (color-theme420-test-root-owned nil)
         (color-theme420-test-install-ledger nil)
         (color-theme420-accent-color "before")
         (color-theme-initialized t)
         (color-theme-directory nil)
         (color-theme-libraries nil)
         (color-theme-load-all-themes nil)
         (color-theme-mode-hook nil)
         (color-theme-is-global t)
         (color-theme-is-cumulative t)
         (color-theme-history nil)
         (color-theme-history-max-length t)
         (color-theme-counter 0)
         (color-theme-original-frame-alist nil)
         (name nil)
         (color-themes
          '((color-theme420-owned "Café Theme 界" "Parity Maintainer")
            (color-theme420-frame-local "Frame Local" "Parity Maintainer")
            (color-theme-simple-1 "Black" "Jonadab <jonadab@bright.net>")))
         (default-frame-alist (copy-tree default-frame-alist))
         (minibuffer-frame-alist (copy-tree minibuffer-frame-alist))
         parked result body-error cleanup-errors fixture-before fixture-after
         source-after)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Color Theme sandbox root"))
              (when (file-exists-p root)
                (error "Color Theme sandbox root exists: %S" root))
              (when-let ((entry (color-theme420-test-park-buffer
                                 color-theme-buffer-name)))
                (push entry parked))
              (make-directory root)
              (setq color-theme420-test-root-owned t
                    fixture-before (directory-files root nil nil t))
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest args)
                           (error "Unexpected call-process: %S" args)))
                        ((symbol-function 'call-process-region)
                         (lambda (&rest args)
                           (error "Unexpected call-process-region: %S" args)))
                        ((symbol-function 'process-file)
                         (lambda (&rest args)
                           (error "Unexpected process-file: %S" args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (error "Unexpected start-process: %S" args)))
                        ((symbol-function 'start-file-process)
                         (lambda (&rest args)
                           (error "Unexpected start-file-process: %S" args)))
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (error "Unexpected make-process: %S" args)))
                        ((symbol-function 'make-network-process)
                         (lambda (&rest args)
                           (error "Unexpected network process: %S" args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (error "Unexpected URL retrieval: %S" args))))
                (setq result (funcall body))))
          (t (setq body-error (color-theme420-test-condition condition))))
      (condition-case condition
          (setq fixture-after (and root (file-exists-p root)
                                   (directory-files root nil nil t)))
        (t (push (color-theme420-test-condition condition) cleanup-errors)))
      (fset 'color-theme-snapshot snapshot-function-before)
      (setplist 'color-theme-snapshot (copy-tree snapshot-plist-before))
      (if compatibility-bound-before
          (fset 'user-variable-p compatibility-function-before)
        (fmakunbound 'user-variable-p))
      (dolist (entry frame-colors-before)
        (condition-case condition
            (when (frame-live-p (nth 0 entry))
              (modify-frame-parameters
               (nth 0 entry)
               `((foreground-color . ,(nth 1 entry))
                 (background-color . ,(nth 2 entry))
                 (background-mode . ,(nth 3 entry)))))
          (t (push (color-theme420-test-condition condition) cleanup-errors))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition (delete-process process)
            (t (push (color-theme420-test-condition condition) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (or (memq buffer buffers-before) (assq buffer parked))
          (condition-case condition
              (with-current-buffer buffer
                (let ((kill-buffer-hook nil)
                      (kill-buffer-query-functions nil))
                  (set-buffer-modified-p nil)
                  (kill-buffer buffer)))
            (t (push (color-theme420-test-condition condition) cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case condition (cancel-timer timer)
            (t (push (color-theme420-test-condition condition) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case condition (delete-frame frame t)
            (t (push (color-theme420-test-condition condition) cleanup-errors)))))
      (condition-case condition (set-window-configuration window-before)
        (t (push (color-theme420-test-condition condition) cleanup-errors)))
      (dolist (entry parked)
        (condition-case condition
            (progn
              (unless (buffer-live-p (car entry))
                (error "Parked Color Theme buffer died: %S" (cdr entry)))
              (with-current-buffer (car entry) (rename-buffer (cdr entry) t)))
          (t (push (color-theme420-test-condition condition) cleanup-errors))))
      (when (buffer-live-p buffer-before) (set-buffer buffer-before))
      (when color-theme420-test-root-owned
        (condition-case condition (delete-directory root t)
          (t (push (color-theme420-test-condition condition) cleanup-errors))))
      (condition-case condition
          (setq source-after (color-theme420-test-source-state))
        (t (push (color-theme420-test-condition condition) cleanup-errors))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :fixture-accounted (equal fixture-before fixture-after)
                 :new-buffers
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
                 :snapshot-restored
                 (eq (symbol-function 'color-theme-snapshot)
                     snapshot-function-before)
                 :compatibility-restored
                 (and (eq (fboundp 'user-variable-p)
                          compatibility-bound-before)
                      (or (not compatibility-bound-before)
                          (eq (symbol-function 'user-variable-p)
                              compatibility-function-before)))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored
                 (and (eq (selected-window) selected-window-before)
                      (equal (color-theme420-test-window-state)
                             window-state-before))
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Color Theme workflow failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COLOR_THEME_MELPA_PIN, "color-theme.el")
        .expect("prepare exact Color Theme source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_load_warning_and_selection_buffer_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_load_warning_and_selection_buffer_are_exact",
        r####"
(color-theme420-test-run
 (lambda ()
   (when (fboundp 'user-variable-p)
     (error "Native Color Theme compatibility failure is unavailable"))
   (let ((native-failure
          (condition-case condition
              (progn (call-interactively #'color-theme-select) :no-error)
            (t (color-theme420-test-condition condition)))))
     (unless (eq (plist-get native-failure :type) 'void-function)
       (error "Unexpected native Color Theme result: %S" native-failure))
     (color-theme420-test-install-compatibility-alias)
   (call-interactively #'color-theme-select)
   (let ((position (color-theme420-test-theme-position 'color-theme420-owned))
         described)
     (goto-char position)
     (cl-letf (((symbol-function 'describe-function)
                (lambda (function)
                  (setq described
                        (list :function function
                              :doc (car (split-string
                                         (documentation function) "\n")))))))
       (call-interactively #'color-theme-describe))
     (list :native-failure native-failure
           :compatibility (symbol-function 'user-variable-p)
           :warnings (copy-tree color-theme420-test-load-warnings)
           :selection (color-theme420-test-selection-state)
           :described described)))))
"####,
        expect![[
            r#"OK (:source (:tree "851cab0385a2f62447631e0b1c912f08df48dabe" :manifest (("color-theme-pkg.el" . "7b55dc15e862fdbcc46038dfe9ca18f44ed1dba180124f8dd8ebb6d2baf490bd") ("color-theme.el" . "ba3ef8b8286a1b80d313a10743ff55bac1d677db1d4829cb35b6ecf6238976cb") ("themes/color-theme-example.el" . "67562d9686d7b5c4fb439af21e85e590e5905b1658913a2ab8d821b9bf536721") ("themes/color-theme-library.el" . "e3d281717052ef052be239ec0f5db227987d571bc043085b6fb38d9c24f954cb")) :feature t :version "20190220.1115") :result (:native-failure (:type void-function :data (user-variable-p) :message "Symbol’s function definition is void: user-variable-p") :compatibility custom-variable-p :warnings ((:type color-theme :message "This package is obsolete.\n\nSince version 22.1 Emacs has built-in support for themes.  That\nimplementation does not derive from the implementation provided\nby this package.  Back when this was new we referred to the new\nimplementation as `deftheme' themes, as opposed to `color-theme'\nthemes.\n\nThis package comes with a large collection of themes.  If you\nstill use it because you want to use one of those, then you can\nnever-the-less migrate to the \"new\" theme implementation.  The\n`color-theme-modern' package ports all themes that are bundles\nwith `color-theme' to the `deftheme' format.  It also ports a\nfew third-party themes.  Its documentation contains setup\ninstructions.  Don't forget to uninstall `color-theme'." :level nil :buffer nil)) :selection (:name "*Color Theme Selection*" :mode color-theme-mode :read-only t :modified nil :point 94 :rows ((:text "[Reset]                 Undo changes, if possible." :theme color-theme-snapshot :face bold :mouse-face highlight) (:text "[Quit]                  Bury this buffer." :theme bury-buffer :face bold :mouse-face highlight) (:text "Café Theme 界           Parity Maintainer" :theme color-theme420-owned :face bold :mouse-face highlight) (:text "Frame Local             Parity Maintainer" :theme color-theme420-frame-local :face bold :mouse-face highlight) (:text "Black                   Jonadab <jonadab@bright.net>" :theme color-theme-simple-1 :face bold :mouse-face highlight)) :keys (("RET" color-theme-install-at-point) ("i" color-theme-install-at-point) ("l" color-theme-install-at-point-for-current-frame) ("d" color-theme-describe) ("p" color-theme-print) ("q" bury-buffer))) :described (:function color-theme420-owned :doc "Install the owned Café theme used by the rank-420 parity corpus.")) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :snapshot-restored t :compatibility-restored t :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_selection_install_and_snapshot_reset_restore_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_selection_install_and_snapshot_reset_restore_state",
        r####"
(color-theme420-test-run
 (lambda ()
   (color-theme420-test-install-compatibility-alias)
   (color-theme420-test-prepare-face 'color-theme420-reset-face)
   (let ((background-before (frame-parameter nil 'background-mode))
         (face-before (color-theme420-test-face-state
                       'color-theme420-reset-face)))
     (call-interactively #'color-theme-select)
     (goto-char (color-theme420-test-theme-position 'color-theme420-owned))
     (call-interactively #'color-theme-install-at-point)
     (let ((installed
            (list :accent color-theme420-accent-color
                  :face (color-theme420-test-face-state
                         'color-theme420-reset-face)
                  :background (frame-parameter nil 'background-mode)
                  :history (copy-tree color-theme-history)
                  :ledger (reverse color-theme420-test-install-ledger))))
       (goto-char (color-theme420-test-theme-position 'color-theme-snapshot))
       (call-interactively #'color-theme-install-at-point)
       (list :before (list :accent "before" :face face-before
                           :background background-before)
             :installed installed
             :reset
             (list :accent color-theme420-accent-color
                   :face (color-theme420-test-face-state
                          'color-theme420-reset-face)
                   :background (frame-parameter nil 'background-mode)
                   :history (copy-tree color-theme-history)
                   :counter color-theme-counter))))))
"####,
        expect![[r#"OK (:source (:tree "851cab0385a2f62447631e0b1c912f08df48dabe" :manifest (("color-theme-pkg.el" . "7b55dc15e862fdbcc46038dfe9ca18f44ed1dba180124f8dd8ebb6d2baf490bd") ("color-theme.el" . "ba3ef8b8286a1b80d313a10743ff55bac1d677db1d4829cb35b6ecf6238976cb") ("themes/color-theme-example.el" . "67562d9686d7b5c4fb439af21e85e590e5905b1658913a2ab8d821b9bf536721") ("themes/color-theme-library.el" . "e3d281717052ef052be239ec0f5db227987d571bc043085b6fb38d9c24f954cb")) :feature t :version "20190220.1115") :result (:before (:accent "before" :face (:foreground "green" :background "white" :weight normal) :background dark) :installed (:accent "café 界" :face (:foreground "red" :background "black" :weight bold) :background dark :history ((color-theme420-owned t)) :ledger ((:global t :cumulative t))) :reset (:accent "café 界" :face (:foreground "green" :background "white" :weight normal) :background dark :history ((color-theme-snapshot t) (color-theme420-owned t)) :counter 2)) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :snapshot-restored t :compatibility-restored t :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#]],
    )
    // Invoking the legacy snapshot writes specifications for every existing
    // face.  The package has no public inverse for that global mutation.
    .fresh_process()
}

fn public_current_frame_install_uses_the_local_route() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_current_frame_install_uses_the_local_route",
        r####"
(color-theme420-test-run
 (lambda ()
   (color-theme420-test-install-compatibility-alias)
   (color-theme420-test-prepare-face 'color-theme420-local-face)
   (let ((defaults-before (copy-tree default-frame-alist)))
     (call-interactively #'color-theme-select)
     (goto-char (color-theme420-test-theme-position 'color-theme420-frame-local))
     (call-interactively #'color-theme-install-at-point-for-current-frame)
     (list :ledger (reverse color-theme420-test-install-ledger)
           :accent-symbol color-theme420-accent-color
           :accent-frame (frame-parameter nil 'color-theme420-accent-color)
           :frame-colors
           (list :foreground (frame-parameter nil 'foreground-color)
                 :background (frame-parameter nil 'background-color)
                 :mode (frame-parameter nil 'background-mode))
           :face-selected
           (color-theme420-test-face-state
            'color-theme420-local-face (selected-frame))
           :defaults-unchanged (equal default-frame-alist defaults-before)
           :history (copy-tree color-theme-history)))))
"####,
        expect![[r#"OK (:source (:tree "851cab0385a2f62447631e0b1c912f08df48dabe" :manifest (("color-theme-pkg.el" . "7b55dc15e862fdbcc46038dfe9ca18f44ed1dba180124f8dd8ebb6d2baf490bd") ("color-theme.el" . "ba3ef8b8286a1b80d313a10743ff55bac1d677db1d4829cb35b6ecf6238976cb") ("themes/color-theme-example.el" . "67562d9686d7b5c4fb439af21e85e590e5905b1658913a2ab8d821b9bf536721") ("themes/color-theme-library.el" . "e3d281717052ef052be239ec0f5db227987d571bc043085b6fb38d9c24f954cb")) :feature t :version "20190220.1115") :result (:ledger ((:global nil :cumulative t)) :accent-symbol "before" :accent-frame nil :frame-colors (:foreground "purple" :background "gray90" :mode light) :face-selected (:foreground "green" :background "white" :weight normal) :defaults-unchanged t :history ((color-theme420-frame-local t))) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :snapshot-restored t :compatibility-restored t :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#]],
    )
    // Emacs has no public operation that deletes a dynamically created face.
    .fresh_process()
}

fn public_bundled_theme_loads_through_selection_and_installs() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_bundled_theme_loads_through_selection_and_installs",
        r####"
(color-theme420-test-run
 (lambda ()
   (color-theme420-test-install-compatibility-alias)
   (let* ((directory (file-name-directory
                      (symbol-file 'color-theme-select 'defun)))
          (color-theme-initialized nil)
          (color-theme-load-all-themes t)
          (color-theme-libraries
           (list (expand-file-name "themes/color-theme-library.el" directory))))
     (call-interactively #'color-theme-select)
     (let ((position (color-theme420-test-theme-position 'color-theme-simple-1)))
       (goto-char position)
       (let ((row (color-theme420-test-selection-row position)))
         (call-interactively #'color-theme-install-at-point)
         (list :row row
               :background-mode (frame-parameter nil 'background-mode)
               :default-face
               (list :foreground
                     (face-attribute 'default :foreground nil 'default)
                     :background
                     (face-attribute 'default :background nil 'default))
               :history (copy-tree color-theme-history)))))))
"####,
        expect![[r#"OK (:source (:tree "851cab0385a2f62447631e0b1c912f08df48dabe" :manifest (("color-theme-pkg.el" . "7b55dc15e862fdbcc46038dfe9ca18f44ed1dba180124f8dd8ebb6d2baf490bd") ("color-theme.el" . "ba3ef8b8286a1b80d313a10743ff55bac1d677db1d4829cb35b6ecf6238976cb") ("themes/color-theme-example.el" . "67562d9686d7b5c4fb439af21e85e590e5905b1658913a2ab8d821b9bf536721") ("themes/color-theme-library.el" . "e3d281717052ef052be239ec0f5db227987d571bc043085b6fb38d9c24f954cb")) :feature t :version "20190220.1115") :result (:row (:text "Black                   Jonadab <jonadab@bright.net>" :theme color-theme-simple-1 :face bold :mouse-face highlight) :background-mode dark :default-face (:foreground "white" :background "black") :history ((color-theme-simple-1 t))) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :snapshot-restored t :compatibility-restored t :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#]],
    )
    // The bundled legacy theme changes standard faces globally and exposes no
    // public inverse, so this user workflow owns a fresh editor process.
    .fresh_process()
}

fn public_install_layers_owned_specs_and_truncates_history() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_install_layers_owned_specs_and_truncates_history",
        r####"
(color-theme420-test-run
 (lambda ()
   (color-theme420-test-install-compatibility-alias)
   (color-theme420-test-prepare-face 'color-theme420-history-face)
   (let ((color-theme-history-max-length 2))
     (color-theme-install
      '(color-theme420-first nil
        ((color-theme420-accent-color . "first café"))
        (color-theme420-history-face ((t (:foreground "orange"))))))
     (color-theme-install
      '(color-theme420-second nil nil
        (color-theme420-history-face ((t (:background "navy"))))))
     (color-theme-install
      '(color-theme420-third nil
        ((color-theme420-accent-color . "third 界"))))
     (list :accent color-theme420-accent-color
           :face (color-theme420-test-face-state
                  'color-theme420-history-face)
           :history (copy-tree color-theme-history)
           :counter color-theme-counter))))
"####,
        expect![[r#"OK (:source (:tree "851cab0385a2f62447631e0b1c912f08df48dabe" :manifest (("color-theme-pkg.el" . "7b55dc15e862fdbcc46038dfe9ca18f44ed1dba180124f8dd8ebb6d2baf490bd") ("color-theme.el" . "ba3ef8b8286a1b80d313a10743ff55bac1d677db1d4829cb35b6ecf6238976cb") ("themes/color-theme-example.el" . "67562d9686d7b5c4fb439af21e85e590e5905b1658913a2ab8d821b9bf536721") ("themes/color-theme-library.el" . "e3d281717052ef052be239ec0f5db227987d571bc043085b6fb38d9c24f954cb")) :feature t :version "20190220.1115") :result (:accent "third 界" :face (:foreground "orange" :background "navy" :weight normal) :history ((color-theme420-third t) (color-theme420-second t)) :counter 3) :cleanup (:source-unchanged t :fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :snapshot-restored t :compatibility-restored t :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#]],
    )
    .fresh_process()
}

#[test]
fn color_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_load_warning_and_selection_buffer_are_exact(),
        public_current_frame_install_uses_the_local_route(),
        public_bundled_theme_loads_through_selection_and_installs(),
        public_install_layers_owned_specs_and_truncates_history(),
        // The legacy snapshot route rewrites every existing face and has no
        // public inverse, so keep its already-fresh workflow last even in the
        // batch-safety audit.
        public_selection_install_and_snapshot_reset_restore_state(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "color-theme-rank420",
        "color_theme_parity",
        &cases,
    );
}
