//! Practical parity for Pippel's public pip package-menu workflows.
//!
//! The exact installed `pippel.py` runs under pinned CPython 3.13.12 against
//! an owned pip 21.2.3 API tree. Pippel retains ownership of request dispatch,
//! response framing, filtering, sentinel completion, tabulated-list rendering,
//! menu actions, and recovery; only pip's environmental effects are doubled.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PIPPEL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'pippel)

(defconst pippel407-test-el-sha256
  "d347ad05ad87ab6f23c57fd7509a464814573ca15530471d7cbbec8f701b6eb0")
(defconst pippel407-test-py-sha256
  "fc886a45769b3a0daa742cd5616f4306faa519862090e29b6dc881fe38c9df1d")
(defconst pippel407-test-python
  "/nix/store/sdfysgb89zdysrknjavcr0crs4qxpk8r-python3-3.13.12/bin/python3.13")
(defconst pippel407-test-python-sha256
  "e0c3d8a24d57558fbc06d36363a4c2f4e5e51d0f6b932765d0814dfcc11e858c")

(defun pippel407-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let* ((loaded (symbol-file 'pippel-list-packages 'defun))
       (source (and loaded
                    (if (string-suffix-p ".elc" loaded)
                        (concat (file-name-sans-extension loaded) ".el")
                      loaded)))
       (directory (and source (file-name-directory source)))
       (python (and directory (expand-file-name "pippel.py" directory)))
       (payload
        (and directory
             (sort (seq-filter
                    (lambda (name)
                      (and (member (file-name-extension name) '("el" "py"))
                           (not (string-suffix-p "-autoloads.el" name))
                           (not (string-suffix-p "-pkg.el" name))))
                    (directory-files directory nil nil t))
                   #'string<))))
  (unless (and (file-regular-p source)
               (not (file-symlink-p source))
               (file-regular-p python)
               (not (file-symlink-p python))
               (equal payload '("pippel.el" "pippel.py"))
               (equal (pippel407-test-file-sha256 source)
                      pippel407-test-el-sha256)
               (equal (pippel407-test-file-sha256 python)
                      pippel407-test-py-sha256))
    (error "Unexpected installed Pippel payload: %S" payload)))

(defconst pippel407-test-stubs
  '(("python-stubs/pip/__init__.py" . "__version__ = '21.2.3'\n")
    ("python-stubs/pip/_internal/__init__.py" . "")
    ("python-stubs/pip/_internal/utils/__init__.py" . "")
    ("python-stubs/pip/_internal/utils/compat.py" . "stdlib_pkgs = set()\n")
    ("python-stubs/pip/_internal/utils/misc.py" .
     "def get_installed_distributions(**kwargs):\n    raise AssertionError('legacy distribution route used')\n")
    ("python-stubs/pip/_internal/commands/__init__.py" . "")
    ("python-stubs/pip/_internal/commands/install.py" .
     "class InstallCommand:\n    def __init__(self, *args):\n        pass\n")
    ("python-stubs/pip/_internal/commands/uninstall.py" .
     "class UninstallCommand:\n    def __init__(self, *args):\n        pass\n")
    ("python-stubs/pip/_internal/commands/list.py" .
     "import json, os\ndef record(event, value):\n    with open(os.environ['PIPPEL407_PIP_LOG'], 'a', encoding='utf-8') as stream:\n        stream.write(json.dumps([event, value], ensure_ascii=False) + '\\n')\nclass Options:\n    def __init__(self, user):\n        self.local = False; self.user = user; self.editable = False\n        self.include_editable = True; self.path = None\nclass ListCommand:\n    def __init__(self, *args):\n        pass\n    def parse_args(self, args):\n        record('list.parse_args', args)\n        return Options('--user' in args), []\n    def iter_packages_latest_infos(self, packages, options):\n        record('list.iter_latest', [item.canonical_name for item in packages])\n        return packages\n")
    ("python-stubs/pip/_internal/commands/show.py" .
     "import json, os\ndef record(event, value):\n    with open(os.environ['PIPPEL407_PIP_LOG'], 'a', encoding='utf-8') as stream:\n        stream.write(json.dumps([event, value], ensure_ascii=False) + '\\n')\ndef search_packages_info(names):\n    record('show.search', names)\n    from pip._internal.metadata import package_for\n    return [package_for(name) for name in names]\n")
    ("python-stubs/pip/_internal/metadata.py" .
     "import json, os
def record(event, value):
    with open(os.environ['PIPPEL407_PIP_LOG'], 'a', encoding='utf-8') as stream:
        stream.write(json.dumps([event, value], ensure_ascii=False) + '\\n')
class Package:
    def __init__(self, name, version, latest, summary, homepage):
        self.name=name; self.canonical_name=name; self.version=version
        self.latest_version=latest; self.summary=summary; self.homepage=homepage
PACKAGES = {
 'beta': Package('beta','2.0','2.0','Stable café','https://example.test/beta'),
 'alpha': Package('alpha','1.0','2.0','Upgrade 界','https://example.test/alpha'),
 'preview': Package('preview','1.0rc1','1.0rc2','Preview','https://example.test/preview'),
 'user-only': Package('user-only','3.0','3.0','User 界','https://example.test/user'),
 'delta': Package('delta','1.0','1.0','Installed target','https://example.test/delta'),
 'café': Package('café','1.0','1.0','Unicode package','https://example.test/cafe')}
def subprocess_calls():
    path = os.environ['PIPPEL407_PIP_LOG']
    if not os.path.exists(path):
        return []
    with open(path, encoding='utf-8') as stream:
        return [row[1] for row in map(json.loads, stream) if row[0] == 'subprocess.check_call']
def current_names():
    scenario = os.environ['PIPPEL407_SCENARIO']
    calls = subprocess_calls()
    if scenario == 'user':
        return ['user-only']
    if scenario == 'actions' and any('uninstall' in call for call in calls):
        return ['alpha','preview']
    if scenario == 'install' and any('--target' in call for call in calls):
        return ['beta','alpha','preview','delta','café']
    return ['beta','alpha','preview']
def package_for(name):
    package = PACKAGES[name]
    if (os.environ['PIPPEL407_SCENARIO'] == 'actions'
            and subprocess_calls() and name in ('alpha', 'preview')):
        return Package(package.name, package.latest_version,
                       package.latest_version, package.summary, package.homepage)
    return package
class Environment:
    def iter_installed_distributions(self, **kwargs):
        values = {key: sorted(value) if isinstance(value, set) else value
                  for key, value in kwargs.items()}
        record('metadata.iter', values)
        scenario = os.environ['PIPPEL407_SCENARIO']
        if scenario == 'error':
            raise RuntimeError('backend café failed: 界')
        return [package_for(name) for name in current_names()]
def get_environment(path):
    record('metadata.environment', path)
    return Environment()
")
    ("python-stubs/subprocess.py" .
     "import json, os\ndef check_call(argv):\n    with open(os.environ['PIPPEL407_PIP_LOG'], 'a', encoding='utf-8') as stream:\n        stream.write(json.dumps(['subprocess.check_call', argv], ensure_ascii=False) + '\\n')\n    return 0\n")))

(defvar pippel407-test-root nil)
(defvar pippel407-test-launches nil)
(defvar pippel407-test-requests nil)
(defvar pippel407-test-outputs nil)
(defvar pippel407-test-original-process-send-string nil)
(defvar pippel407-test-original-process-sentinel nil)
(defvar pippel407-test-original-start-process nil)

(defun pippel407-test-normalize (value)
  (cond
   ((stringp value)
    (if pippel407-test-root
        (replace-regexp-in-string
         (regexp-quote (directory-file-name pippel407-test-root))
         "[ROOT]" value t t)
      (copy-sequence value)))
   ((consp value)
    (cons (pippel407-test-normalize (car value))
          (pippel407-test-normalize (cdr value))))
   ((vectorp value)
    (apply #'vector (mapcar #'pippel407-test-normalize value)))
   (t value)))

(defun pippel407-test-condition (condition)
  (list :type (car condition)
        :data (pippel407-test-normalize (copy-tree (cdr condition)))
        :message (pippel407-test-normalize
                  (error-message-string condition))))

(defun pippel407-test-write-file (root relative contents &optional executable)
  (let ((file (expand-file-name relative root)))
    (unless (and (file-in-directory-p file root)
                 (not (equal file (directory-file-name root))))
      (error "Unsafe Pippel fixture path: %s" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (write-region contents nil file nil 'silent))
    (when executable (set-file-modes file #o700))
    file))

(defun pippel407-test-read-file (file)
  (when (file-exists-p file)
    (with-temp-buffer
      (insert-file-contents-literally file)
      (decode-coding-string (buffer-string) 'utf-8-unix))))

(defun pippel407-test-lines (file)
  (let ((text (pippel407-test-read-file file)))
    (and text (split-string text "\n" t))))

(defun pippel407-test-stub-manifest (root)
  (mapcar
   (lambda (entry)
     (let ((file (expand-file-name (car entry) root)))
       (unless (and (file-regular-p file) (not (file-symlink-p file)))
         (error "Invalid Pippel pip stub: %s" file))
       (list (car entry) (pippel407-test-file-sha256 file))))
   pippel407-test-stubs))

(defun pippel407-test-process-send-string (process string)
  (unless (and (eq process (get-process pippel-process-name))
               (string-match-p "\\`[^\n]+\n\\'" string))
    (error "Unexpected Pippel request boundary: %S %S" process string))
  (push (substring string 0 -1) pippel407-test-requests)
  (funcall pippel407-test-original-process-send-string process string))

(defun pippel407-test-start-process (name buffer program &rest arguments)
  (unless (and (equal name pippel-process-name)
               (equal buffer pippel-process-buffer)
               (equal program pippel407-test-python)
               (equal arguments
                      (list (expand-file-name "pippel.py"
                                              pippel-package-path))))
    (error "Unexpected Pippel external process: %S"
           (list name buffer program arguments)))
  (let ((process (apply pippel407-test-original-start-process
                        name buffer program arguments)))
    (push (list 'owned-python 'pippel.py) pippel407-test-launches)
    process))

(defun pippel407-test-process-sentinel (process output)
  (unless (and (equal (process-name process) pippel-process-name)
               (equal (process-command process)
                      (list pippel-python-command
                            (expand-file-name "pippel.py"
                                              pippel-package-path))))
    (error "Unexpected Pippel sentinel process: %S" process))
  (when-let* ((buffer (process-buffer process)))
    (with-current-buffer buffer
      (push (buffer-substring-no-properties (point-min) (point-max))
            pippel407-test-outputs)))
  (funcall pippel407-test-original-process-sentinel process output))

(defun pippel407-test-window-state ()
  (mapcar (lambda (window)
            (list (window-buffer window) (window-point window)
                  (window-start window) (window-dedicated-p window)))
          (seq-mapcat (lambda (frame) (window-list frame 'nomini))
                      (frame-list))))

(defun pippel407-test-park-buffer (name)
  (when-let* ((buffer (get-buffer name)))
    (let ((old-name (buffer-name buffer)))
      (with-current-buffer buffer
        (rename-buffer (format " *pippel407-parked-%s*" (sxhash-eq buffer)) t))
      (cons buffer old-name))))

(defun pippel407-test-wait-process ()
  (let ((process (get-process pippel-process-name))
        (attempt 0))
    (unless process (error "Pippel did not create its process"))
    (let ((command (copy-sequence (process-command process))))
      (unless (equal command
                     (list pippel-python-command
                           (expand-file-name "pippel.py" pippel-package-path)))
        (error "Unexpected Pippel process command: %S" command)))
    (while (and (< attempt 200)
                (or (process-live-p process)
                    (get-buffer pippel-process-buffer)))
      (accept-process-output process 0.02)
      (setq attempt (1+ attempt)))
    (when (or (process-live-p process) (get-buffer pippel-process-buffer))
      (error "Pippel process did not settle: %S" (process-status process)))
    (list :status (process-status process)
          :buffer-gone (not (get-buffer pippel-process-buffer)))))

(defun pippel407-test-find-row (id)
  (goto-char (point-min))
  (while (and (not (eobp))
              (not (equal (tabulated-list-get-id) id)))
    (forward-line))
  (unless (equal (tabulated-list-get-id) id)
    (error "Missing Pippel row: %s" id)))

(defun pippel407-test-property-runs (start end property)
  (let ((position start) runs)
    (while (< position end)
      (let* ((next (next-single-property-change position property nil end))
             (value (get-text-property position property)))
        (when value
          (push (list (- position start) (- next start)
                      (pippel407-test-normalize (copy-tree value)))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun pippel407-test-rendered-rows ()
  (save-excursion
    (goto-char (point-min))
    (let (rows)
      (while (not (eobp))
        (let ((start (line-beginning-position))
              (end (line-end-position))
              (id (tabulated-list-get-id)))
          (when id
            (push (list :id (substring-no-properties id)
                        :text (buffer-substring-no-properties start end)
                        :links (pippel407-test-property-runs start end 'link)
                        :faces (pippel407-test-property-runs
                                start end 'font-lock-face))
                  rows)))
        (forward-line))
      (nreverse rows))))

(defun pippel407-test-menu-state ()
  (let ((buffer (get-buffer "*Pip-Packages*")))
    (unless (buffer-live-p buffer) (error "Missing Pippel package menu"))
    (with-current-buffer buffer
      (let ((window (get-buffer-window buffer t)))
        (list
       :mode major-mode
       :displayed (window-live-p window)
       :selected (and (window-live-p window)
                      (eq window (selected-window)))
       :keys (mapcar (lambda (key)
                       (list key (lookup-key pippel-package-menu-mode-map
                                             (kbd key))))
                     '("m" "d" "U" "u" "r" "i" "x" "RET" "q"))
       :header (substring-no-properties
                (format-mode-line header-line-format nil window buffer))
       :format (copy-tree tabulated-list-format)
       :text (buffer-substring-no-properties (point-min) (point-max))
       :rendered-rows (pippel407-test-rendered-rows)
       :point (list (line-number-at-pos) (current-column)
                    (and (tabulated-list-get-id)
                         (substring-no-properties (tabulated-list-get-id))))
       :rows
       (mapcar
        (lambda (entry)
          (let* ((id (car entry))
                 (columns (cadr entry))
                 (name (aref columns 0))
                 (latest (aref columns 2)))
            (list (substring-no-properties id)
                  :name (substring-no-properties name)
                  :link (get-text-property 0 'link name)
                  :version (aref columns 1)
                  :latest (substring-no-properties latest)
                  :latest-face (get-text-property 0 'font-lock-face latest)
                  :description (aref columns 3))))
        tabulated-list-entries)
       :tags
       (save-excursion
         (goto-char (point-min))
         (let (tags)
           (while (not (eobp))
             (when-let* ((id (tabulated-list-get-id)))
               (push (list (substring-no-properties id)
                           (char-to-string (char-after)))
                     tags))
             (forward-line))
           (nreverse tags))))))))

(defun pippel407-test-run (name scenario body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (concat "pippel-" name "/") sandbox))))
         (log (and root (expand-file-name "operations.jsonl" root)))
         (stubs-root (and root (expand-file-name "python-stubs" root)))
         (install-dir (and root
                           (file-name-as-directory
                            (expand-file-name "wheel house" root))))
         (loaded (symbol-file 'pippel-list-packages 'defun))
         (source (and loaded
                      (if (string-suffix-p ".elc" loaded)
                          (concat (file-name-sans-extension loaded) ".el")
                        loaded)))
         (package-path (and source (file-name-directory source)))
         (window-before (current-window-configuration))
         (window-state-before (pippel407-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (post-command-before (copy-sequence post-command-hook))
         (process-environment (copy-sequence process-environment))
         (exec-path (copy-sequence exec-path))
         (process-connection-type nil)
         (python-shell-virtualenv-root nil)
         (pippel-process-name "pippel407-process")
         (pippel-process-buffer " *pippel407-process*")
         (pippel-debug-buffer " *pippel407-debug*")
         (pippel-column-width-package 15)
         (pippel-column-width-version 10)
         (pippel-menu-latest-face "orange")
         (pippel-python-command pippel407-test-python)
         (pippel-package-path package-path)
         (pippel-display-status-reporter t)
         (pippel-buffer-display-method #'display-buffer)
         (pippel-debugging t)
         (post-command-hook (copy-sequence post-command-hook))
         (enable-dir-local-variables nil)
         (pippel407-test-root root)
         (pippel407-test-launches nil)
         (pippel407-test-requests nil)
         (pippel407-test-outputs nil)
         (pippel407-test-original-process-send-string
          (symbol-function 'process-send-string))
         (pippel407-test-original-process-sentinel
          (symbol-function 'pippel-process-sentinel))
         (pippel407-test-original-start-process (symbol-function 'start-process))
         (message-log-max nil)
         (print-circle nil)
         (parked nil)
         (root-owned nil)
         python-digest stub-manifest result body-error
         failure-diagnostics cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Pippel sandbox root"))
              (when (file-exists-p root)
                (error "Pippel sandbox root already exists: %s" root))
              (dolist (buffer-name
                       '("*Pip-Packages*" " *pippel407-process*"
                         " *pippel407-debug*"))
                (when-let* ((entry (pippel407-test-park-buffer buffer-name)))
                  (push entry parked)))
              (make-directory root t)
              (setq root-owned t)
              (make-directory install-dir t)
              (unless (and (file-regular-p pippel407-test-python)
                           (file-executable-p pippel407-test-python)
                           (not (file-symlink-p pippel407-test-python)))
                (error "Pinned Pippel Python is not a direct regular file"))
              (setq python-digest
                    (pippel407-test-file-sha256 pippel407-test-python))
              (unless (equal python-digest pippel407-test-python-sha256)
                (error "Pinned Pippel Python digest mismatch"))
              (dolist (entry pippel407-test-stubs)
                (pippel407-test-write-file root (car entry) (cdr entry)))
              (setq stub-manifest (pippel407-test-stub-manifest root))
              (setenv "PYTHONPATH" stubs-root)
              (setenv "PYTHONNOUSERSITE" "1")
              (setenv "PYTHONDONTWRITEBYTECODE" "1")
              (setenv "PYTHONUNBUFFERED" "1")
              (setenv "LC_ALL" "C.UTF-8")
              (setenv "LANG" "C.UTF-8")
              (setenv "PIPPEL407_PIP_LOG" log)
              (setenv "PIPPEL407_SCENARIO" scenario)
              (cl-letf (((symbol-function 'process-send-string)
                         #'pippel407-test-process-send-string)
                        ((symbol-function 'pippel-process-sentinel)
                         #'pippel407-test-process-sentinel)
                        ((symbol-function 'start-process)
                         #'pippel407-test-start-process))
                (setq result (funcall body root log install-dir)))
              (unless (and (file-regular-p pippel407-test-python)
                           (file-executable-p pippel407-test-python)
                           (not (file-symlink-p pippel407-test-python))
                           (equal python-digest
                                  (pippel407-test-file-sha256
                                   pippel407-test-python)))
                (error "Pinned Pippel Python changed"))
              (unless (equal stub-manifest
                             (pippel407-test-stub-manifest root))
                (error "Pippel pip boundary changed"))
              (let ((files
                     (mapcar (lambda (file) (file-relative-name file root))
                             (sort (directory-files-recursively root "." nil)
                                   #'string-lessp))))
                (unless (equal files
                               (sort (cons "operations.jsonl"
                                           (mapcar #'car pippel407-test-stubs))
                                     #'string-lessp))
                  (error "Unexpected Pippel files: %S" files))))
          (error (setq body-error condition)))
      (when body-error
        (setq failure-diagnostics
              (list :requests (nreverse (copy-sequence pippel407-test-requests))
                    :outputs (nreverse (copy-sequence pippel407-test-outputs))
                    :pip-boundary (and log (pippel407-test-lines log)))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition
              (progn
                (set-process-query-on-exit-flag process nil)
                (delete-process process))
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
            (let ((buffer (car entry)) (old-name (cdr entry)))
              (unless (buffer-live-p buffer)
                (error "Parked Pippel buffer died: %s" old-name))
              (with-current-buffer buffer (rename-buffer old-name t)))
          (error (push (list :restore-buffer condition) cleanup-errors))))
      (condition-case condition
          (when root-owned
            (when (file-exists-p root) (delete-directory root t)))
        (error (push (list :delete-root condition) cleanup-errors)))
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
                                         (pippel407-test-window-state))
                 :buffer-restored (eq buffer-before (current-buffer))
                 :post-command-restored (equal post-command-before
                                               post-command-hook)
                 :body-error (and body-error
                                  (pippel407-test-condition body-error))
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Pippel workflow failed: %S %S"
                 cleanup (pippel407-test-normalize failure-diagnostics))
        (list :result (pippel407-test-normalize result)
              :requests (pippel407-test-normalize
                         (nreverse pippel407-test-requests))
              :outputs (pippel407-test-normalize
                        (nreverse pippel407-test-outputs))
              :launches (nreverse pippel407-test-launches)
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PIPPEL_MELPA_PIN, "pippel.el")
        .expect("prepare exact shallow Pippel source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_list_renders_sorting_links_faces_and_noop_failure() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_list_renders_sorting_links_faces_and_noop_failure",
        r####"(pippel407-test-run
 "list" "list"
 (lambda (_root log _install-dir)
   (call-interactively #'pippel-list-packages)
   (let ((process (pippel407-test-wait-process))
         visit-calls failure before after)
     (with-current-buffer "*Pip-Packages*"
       (pippel407-test-find-row "alpha")
       (cl-letf (((symbol-function 'browse-url)
                  (lambda (url &rest arguments)
                    (push (list url arguments) visit-calls))))
         (call-interactively #'pippel-menu-visit-homepage))
       (setq before (pippel407-test-menu-state))
       (condition-case condition
           (call-interactively #'pippel-menu-execute)
         (error (setq failure (pippel407-test-condition condition))))
       (setq after (pippel407-test-menu-state)))
     (list :process process
           :menu before
           :visit (nreverse visit-calls)
           :no-op (list failure (equal before after))
           :pip-boundary (pippel407-test-lines log)))))"####,
        expect![[
            r#"OK (:result (:process (:status signal :buffer-gone t) :menu (:mode pippel-package-menu-mode :displayed t :selected nil :keys (("m" pippel-menu-mark-unmark) ("d" pippel-menu-mark-delete) ("U" pippel-menu-mark-all-upgrades) ("u" pippel-menu-mark-upgrade) ("r" pippel-list-packages) ("i" pippel-install-package) ("x" pippel-menu-execute) ("RET" pippel-menu-visit-homepage) ("q" quit-window)) :header "" :format [("Package" 15 nil) ("Version" 10 nil) ("Latest" 10 nil) ("Description" 0 nil)] :text "  alpha           1.0        2.0        Upgrade 界\n  beta            2.0        2.0        Stable café\n  preview         1.0rc1     1.0rc2     Preview\n" :rendered-rows ((:id "alpha" :text "  alpha           1.0        2.0        Upgrade 界" :links ((2 7 "https://example.test/alpha")) :faces ((29 32 (:foreground "orange")))) (:id "beta" :text "  beta            2.0        2.0        Stable café" :links ((2 6 "https://example.test/beta")) :faces nil) (:id "preview" :text "  preview         1.0rc1     1.0rc2     Preview" :links ((2 9 "https://example.test/preview")) :faces ((29 35 (:foreground "orange"))))) :point (1 0 "alpha") :rows (("beta" :name "beta" :link "https://example.test/beta" :version "2.0" :latest "2.0" :latest-face nil :description "Stable café") ("alpha" :name "alpha" :link "https://example.test/alpha" :version "1.0" :latest "2.0" :latest-face (:foreground "orange") :description "Upgrade 界") ("preview" :name "preview" :link "https://example.test/preview" :version "1.0rc1" :latest "1.0rc2" :latest-face (:foreground "orange") :description "Preview")) :tags (("alpha" " ") ("beta" " ") ("preview" " "))) :visit (("https://example.test/alpha" nil)) :no-op ((:type user-error :data ("No operations specified") :message "No operations specified") t) :pip-boundary ("[\"list.parse_args\", [\"--outdated\", \"\"]]" "[\"metadata.environment\", null]" "[\"metadata.iter\", {\"local_only\": false, \"user_only\": false, \"editables_only\": false, \"include_editables\": true, \"skip\": []}]" "[\"list.iter_latest\", [\"beta\", \"alpha\", \"preview\"]]" "[\"show.search\", [\"beta\"]]" "[\"show.search\", [\"alpha\"]]" "[\"show.search\", [\"preview\"]]")) :requests ("{\"method\":\"get_installed_packages\",\"arg1\":null,\"arg2\":null}") :outputs ("pip version: (21, 2, 3)\nExecuting: get_installed_packages(None, None)\n[{\"name\": \"beta\", \"version\": \"2.0\", \"latest\": \"2.0\", \"summary\": \"Stable caf\\u00e9\", \"home-page\": \"https://example.test/beta\"}, {\"name\": \"alpha\", \"version\": \"1.0\", \"latest\": \"2.0\", \"summary\": \"Upgrade \\u754c\", \"home-page\": \"https://example.test/alpha\"}, {\"name\": \"preview\", \"version\": \"1.0rc1\", \"latest\": \"1.0rc2\", \"summary\": \"Preview\", \"home-page\": \"https://example.test/preview\"}]\nPip finished\n") :launches ((owned-python pippel.py)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :post-command-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_user_list_sends_the_user_flag_and_replaces_the_menu() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_user_list_sends_the_user_flag_and_replaces_the_menu",
        r####"(pippel407-test-run
 "user" "user"
 (lambda (_root log _install-dir)
   (call-interactively #'pippel-list-user-packages)
   (list :process (pippel407-test-wait-process)
         :menu (pippel407-test-menu-state)
         :pip-boundary (pippel407-test-lines log))))"####,
        expect![[
            r#"OK (:result (:process (:status signal :buffer-gone t) :menu (:mode pippel-package-menu-mode :displayed t :selected nil :keys (("m" pippel-menu-mark-unmark) ("d" pippel-menu-mark-delete) ("U" pippel-menu-mark-all-upgrades) ("u" pippel-menu-mark-upgrade) ("r" pippel-list-packages) ("i" pippel-install-package) ("x" pippel-menu-execute) ("RET" pippel-menu-visit-homepage) ("q" quit-window)) :header "" :format [("Package" 15 nil) ("Version" 10 nil) ("Latest" 10 nil) ("Description" 0 nil)] :text "  user-only       3.0        3.0        User 界\n" :rendered-rows ((:id "user-only" :text "  user-only       3.0        3.0        User 界" :links ((2 11 "https://example.test/user")) :faces nil)) :point (1 0 "user-only") :rows (("user-only" :name "user-only" :link "https://example.test/user" :version "3.0" :latest "3.0" :latest-face nil :description "User 界")) :tags (("user-only" " "))) :pip-boundary ("[\"list.parse_args\", [\"--outdated\", \"--user\"]]" "[\"metadata.environment\", null]" "[\"metadata.iter\", {\"local_only\": false, \"user_only\": true, \"editables_only\": false, \"include_editables\": true, \"skip\": []}]" "[\"list.iter_latest\", [\"user-only\"]]" "[\"show.search\", [\"user-only\"]]")) :requests ("{\"method\":\"get_installed_packages\",\"arg1\":\"--user\",\"arg2\":null}") :outputs ("pip version: (21, 2, 3)\nExecuting: get_installed_packages(--user, None)\n[{\"name\": \"user-only\", \"version\": \"3.0\", \"latest\": \"3.0\", \"summary\": \"User \\u754c\", \"home-page\": \"https://example.test/user\"}]\nPip finished\n") :launches ((owned-python pippel.py)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :post-command-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_marks_execute_upgrade_delete_and_refresh_in_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_marks_execute_upgrade_delete_and_refresh_in_order",
        r####"(pippel407-test-run
 "actions" "actions"
 (lambda (_root log _install-dir)
   (call-interactively #'pippel-list-packages)
   (pippel407-test-wait-process)
   (let (prompt-calls bulk-state marked)
     (with-current-buffer "*Pip-Packages*"
       (call-interactively #'pippel-menu-mark-all-upgrades)
       (setq bulk-state (pippel407-test-menu-state))
       (pippel407-test-find-row "preview")
       (call-interactively #'pippel-menu-mark-unmark)
       (pippel407-test-find-row "preview")
       (call-interactively #'pippel-menu-mark-upgrade)
       (pippel407-test-find-row "beta")
       (call-interactively #'pippel-menu-mark-delete)
       (setq marked (pippel407-test-menu-state))
       (cl-letf (((symbol-function 'yes-or-no-p)
                  (lambda (prompt)
                    (push prompt prompt-calls)
                    t)))
         (call-interactively #'pippel-menu-execute)))
     (let ((refresh (pippel407-test-wait-process)))
       (list :bulk-mark bulk-state
             :marked marked
             :prompts (nreverse prompt-calls)
             :refresh refresh
             :final-menu (pippel407-test-menu-state)
             :pip-boundary (pippel407-test-lines log))))))"####,
        expect![[
            r#"OK (:result (:bulk-mark (:mode pippel-package-menu-mode :displayed t :selected nil :keys (("m" pippel-menu-mark-unmark) ("d" pippel-menu-mark-delete) ("U" pippel-menu-mark-all-upgrades) ("u" pippel-menu-mark-upgrade) ("r" pippel-list-packages) ("i" pippel-install-package) ("x" pippel-menu-execute) ("RET" pippel-menu-visit-homepage) ("q" quit-window)) :header "" :format [("Package" 15 nil) ("Version" 10 nil) ("Latest" 10 nil) ("Description" 0 nil)] :text "U alpha           1.0        2.0        Upgrade 界\n  beta            2.0        2.0        Stable café\nU preview         1.0rc1     1.0rc2     Preview\n" :rendered-rows ((:id "alpha" :text "U alpha           1.0        2.0        Upgrade 界" :links ((2 7 "https://example.test/alpha")) :faces ((29 32 (:foreground "orange")))) (:id "beta" :text "  beta            2.0        2.0        Stable café" :links ((2 6 "https://example.test/beta")) :faces nil) (:id "preview" :text "U preview         1.0rc1     1.0rc2     Preview" :links ((2 9 "https://example.test/preview")) :faces ((29 35 (:foreground "orange"))))) :point (1 0 "alpha") :rows (("beta" :name "beta" :link "https://example.test/beta" :version "2.0" :latest "2.0" :latest-face nil :description "Stable café") ("alpha" :name "alpha" :link "https://example.test/alpha" :version "1.0" :latest "2.0" :latest-face (:foreground "orange") :description "Upgrade 界") ("preview" :name "preview" :link "https://example.test/preview" :version "1.0rc1" :latest "1.0rc2" :latest-face (:foreground "orange") :description "Preview")) :tags (("alpha" "U") ("beta" " ") ("preview" "U"))) :marked (:mode pippel-package-menu-mode :displayed t :selected nil :keys (("m" pippel-menu-mark-unmark) ("d" pippel-menu-mark-delete) ("U" pippel-menu-mark-all-upgrades) ("u" pippel-menu-mark-upgrade) ("r" pippel-list-packages) ("i" pippel-install-package) ("x" pippel-menu-execute) ("RET" pippel-menu-visit-homepage) ("q" quit-window)) :header "" :format [("Package" 15 nil) ("Version" 10 nil) ("Latest" 10 nil) ("Description" 0 nil)] :text "U alpha           1.0        2.0        Upgrade 界\nD beta            2.0        2.0        Stable café\nU preview         1.0rc1     1.0rc2     Preview\n" :rendered-rows ((:id "alpha" :text "U alpha           1.0        2.0        Upgrade 界" :links ((2 7 "https://example.test/alpha")) :faces ((29 32 (:foreground "orange")))) (:id "beta" :text "D beta            2.0        2.0        Stable café" :links ((2 6 "https://example.test/beta")) :faces nil) (:id "preview" :text "U preview         1.0rc1     1.0rc2     Preview" :links ((2 9 "https://example.test/preview")) :faces ((29 35 (:foreground "orange"))))) :point (3 0 "preview") :rows (("beta" :name "beta" :link "https://example.test/beta" :version "2.0" :latest "2.0" :latest-face nil :description "Stable café") ("alpha" :name "alpha" :link "https://example.test/alpha" :version "1.0" :latest "2.0" :latest-face (:foreground "orange") :description "Upgrade 界") ("preview" :name "preview" :link "https://example.test/preview" :version "1.0rc1" :latest "1.0rc2" :latest-face (:foreground "orange") :description "Preview")) :tags (("alpha" "U") ("beta" "D") ("preview" "U"))) :prompts ("Delete 1 package (beta) and Upgrade 2 packages (preview, alpha)") :refresh (:status signal :buffer-gone t) :final-menu (:mode pippel-package-menu-mode :displayed t :selected nil :keys (("m" pippel-menu-mark-unmark) ("d" pippel-menu-mark-delete) ("U" pippel-menu-mark-all-upgrades) ("u" pippel-menu-mark-upgrade) ("r" pippel-list-packages) ("i" pippel-install-package) ("x" pippel-menu-execute) ("RET" pippel-menu-visit-homepage) ("q" quit-window)) :header "" :format [("Package" 15 nil) ("Version" 10 nil) ("Latest" 10 nil) ("Description" 0 nil)] :text "  alpha           2.0        2.0        Upgrade 界\n  preview         1.0rc2     1.0rc2     Preview\n" :rendered-rows ((:id "alpha" :text "  alpha           2.0        2.0        Upgrade 界" :links ((2 7 "https://example.test/alpha")) :faces nil) (:id "preview" :text "  preview         1.0rc2     1.0rc2     Preview" :links ((2 9 "https://example.test/preview")) :faces nil)) :point (1 0 "alpha") :rows (("alpha" :name "alpha" :link "https://example.test/alpha" :version "2.0" :latest "2.0" :latest-face nil :description "Upgrade 界") ("preview" :name "preview" :link "https://example.test/preview" :version "1.0rc2" :latest "1.0rc2" :latest-face nil :description "Preview")) :tags (("alpha" " ") ("preview" " "))) :pip-boundary ("[\"list.parse_args\", [\"--outdated\", \"\"]]" "[\"metadata.environment\", null]" "[\"metadata.iter\", {\"local_only\": false, \"user_only\": false, \"editables_only\": false, \"include_editables\": true, \"skip\": []}]" "[\"list.iter_latest\", [\"beta\", \"alpha\", \"preview\"]]" "[\"show.search\", [\"beta\"]]" "[\"show.search\", [\"alpha\"]]" "[\"show.search\", [\"preview\"]]" "[\"subprocess.check_call\", [\"/nix/store/sdfysgb89zdysrknjavcr0crs4qxpk8r-python3-3.13.12/bin/python3.13\", \"-m\", \"pip\", \"install\", \"preview\", \"alpha\", \"--upgrade\", \"--user\"]]" "[\"subprocess.check_call\", [\"/nix/store/sdfysgb89zdysrknjavcr0crs4qxpk8r-python3-3.13.12/bin/python3.13\", \"-m\", \"pip\", \"uninstall\", \"beta\", \"--yes\"]]" "[\"list.parse_args\", [\"--outdated\", \"\"]]" "[\"metadata.environment\", null]" "[\"metadata.iter\", {\"local_only\": false, \"user_only\": false, \"editables_only\": false, \"include_editables\": true, \"skip\": []}]" "[\"list.iter_latest\", [\"alpha\", \"preview\"]]" "[\"show.search\", [\"alpha\"]]" "[\"show.search\", [\"preview\"]]")) :requests ("{\"method\":\"get_installed_packages\",\"arg1\":null,\"arg2\":null}" "{\"method\":\"install_package\",\"arg1\":\"preview alpha\",\"arg2\":null}" "{\"method\":\"remove_package\",\"arg1\":\"beta\",\"arg2\":null}" "{\"method\":\"get_installed_packages\",\"arg1\":null,\"arg2\":null}") :outputs ("pip version: (21, 2, 3)\nExecuting: get_installed_packages(None, None)\n[{\"name\": \"beta\", \"version\": \"2.0\", \"latest\": \"2.0\", \"summary\": \"Stable caf\\u00e9\", \"home-page\": \"https://example.test/beta\"}, {\"name\": \"alpha\", \"version\": \"1.0\", \"latest\": \"2.0\", \"summary\": \"Upgrade \\u754c\", \"home-page\": \"https://example.test/alpha\"}, {\"name\": \"preview\", \"version\": \"1.0rc1\", \"latest\": \"1.0rc2\", \"summary\": \"Preview\", \"home-page\": \"https://example.test/preview\"}]\nPip finished\n" "pip version: (21, 2, 3)\nExecuting: install_package(preview alpha, None)\nInstallting ['preview', 'alpha'] in the local user space\nPip finished\n" "pip version: (21, 2, 3)\nExecuting: remove_package(beta, None)\nUninstalling beta\nPip finished\n" "pip version: (21, 2, 3)\nExecuting: get_installed_packages(None, None)\n[{\"name\": \"alpha\", \"version\": \"2.0\", \"latest\": \"2.0\", \"summary\": \"Upgrade \\u754c\", \"home-page\": \"https://example.test/alpha\"}, {\"name\": \"preview\", \"version\": \"1.0rc2\", \"latest\": \"1.0rc2\", \"summary\": \"Preview\", \"home-page\": \"https://example.test/preview\"}]\nPip finished\n") :launches ((owned-python pippel.py) (owned-python pippel.py) (owned-python pippel.py) (owned-python pippel.py)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :post-command-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_install_trims_input_and_forwards_the_prefix_directory() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_install_trims_input_and_forwards_the_prefix_directory",
        r####"(pippel407-test-run
 "install" "install"
 (lambda (_root log install-dir)
   (call-interactively #'pippel-list-packages)
   (pippel407-test-wait-process)
   (let (prompt-calls)
     (cl-letf (((symbol-function 'read-from-minibuffer)
                (lambda (prompt &rest arguments)
                  (push (list :package prompt arguments) prompt-calls)
                  "  delta café  "))
               ((symbol-function 'read-file-name)
                (lambda (prompt &rest arguments)
                  (push (list :directory prompt arguments) prompt-calls)
                  install-dir)))
       (with-current-buffer "*Pip-Packages*"
         (let ((current-prefix-arg '(4)))
           (call-interactively #'pippel-install-package))))
     (list :refresh-process (pippel407-test-wait-process)
           :prompts (nreverse prompt-calls)
           :menu (pippel407-test-menu-state)
           :pip-boundary (pippel407-test-lines log)))))"####,
        expect![[
            r#"OK (:result (:refresh-process (:status signal :buffer-gone t) :prompts ((:package "Enter package name: " nil) (:directory "Directory: " nil)) :menu (:mode pippel-package-menu-mode :displayed t :selected nil :keys (("m" pippel-menu-mark-unmark) ("d" pippel-menu-mark-delete) ("U" pippel-menu-mark-all-upgrades) ("u" pippel-menu-mark-upgrade) ("r" pippel-list-packages) ("i" pippel-install-package) ("x" pippel-menu-execute) ("RET" pippel-menu-visit-homepage) ("q" quit-window)) :header "" :format [("Package" 15 nil) ("Version" 10 nil) ("Latest" 10 nil) ("Description" 0 nil)] :text "  alpha           1.0        2.0        Upgrade 界\n  beta            2.0        2.0        Stable café\n  café            1.0        1.0        Unicode package\n  delta           1.0        1.0        Installed target\n  preview         1.0rc1     1.0rc2     Preview\n" :rendered-rows ((:id "alpha" :text "  alpha           1.0        2.0        Upgrade 界" :links ((2 7 "https://example.test/alpha")) :faces ((29 32 (:foreground "orange")))) (:id "beta" :text "  beta            2.0        2.0        Stable café" :links ((2 6 "https://example.test/beta")) :faces nil) (:id "café" :text "  café            1.0        1.0        Unicode package" :links ((2 6 "https://example.test/cafe")) :faces nil) (:id "delta" :text "  delta           1.0        1.0        Installed target" :links ((2 7 "https://example.test/delta")) :faces nil) (:id "preview" :text "  preview         1.0rc1     1.0rc2     Preview" :links ((2 9 "https://example.test/preview")) :faces ((29 35 (:foreground "orange"))))) :point (1 0 "alpha") :rows (("beta" :name "beta" :link "https://example.test/beta" :version "2.0" :latest "2.0" :latest-face nil :description "Stable café") ("alpha" :name "alpha" :link "https://example.test/alpha" :version "1.0" :latest "2.0" :latest-face (:foreground "orange") :description "Upgrade 界") ("preview" :name "preview" :link "https://example.test/preview" :version "1.0rc1" :latest "1.0rc2" :latest-face (:foreground "orange") :description "Preview") ("delta" :name "delta" :link "https://example.test/delta" :version "1.0" :latest "1.0" :latest-face nil :description "Installed target") ("café" :name "café" :link "https://example.test/cafe" :version "1.0" :latest "1.0" :latest-face nil :description "Unicode package")) :tags (("alpha" " ") ("beta" " ") ("café" " ") ("delta" " ") ("preview" " "))) :pip-boundary ("[\"list.parse_args\", [\"--outdated\", \"\"]]" "[\"metadata.environment\", null]" "[\"metadata.iter\", {\"local_only\": false, \"user_only\": false, \"editables_only\": false, \"include_editables\": true, \"skip\": []}]" "[\"list.iter_latest\", [\"beta\", \"alpha\", \"preview\"]]" "[\"show.search\", [\"beta\"]]" "[\"show.search\", [\"alpha\"]]" "[\"show.search\", [\"preview\"]]" "[\"subprocess.check_call\", [\"/nix/store/sdfysgb89zdysrknjavcr0crs4qxpk8r-python3-3.13.12/bin/python3.13\", \"-m\", \"pip\", \"install\", \"delta\", \"café\", \"--upgrade\", \"--target\", \"[ROOT]/wheel house/\"]]" "[\"list.parse_args\", [\"--outdated\", \"\"]]" "[\"metadata.environment\", null]" "[\"metadata.iter\", {\"local_only\": false, \"user_only\": false, \"editables_only\": false, \"include_editables\": true, \"skip\": []}]" "[\"list.iter_latest\", [\"beta\", \"alpha\", \"preview\", \"delta\", \"café\"]]" "[\"show.search\", [\"beta\"]]" "[\"show.search\", [\"alpha\"]]" "[\"show.search\", [\"preview\"]]" "[\"show.search\", [\"delta\"]]" "[\"show.search\", [\"café\"]]")) :requests ("{\"method\":\"get_installed_packages\",\"arg1\":null,\"arg2\":null}" "{\"method\":\"install_package\",\"arg1\":\"delta café\",\"arg2\":\"[ROOT]/wheel house/\"}" "{\"method\":\"get_installed_packages\",\"arg1\":null,\"arg2\":null}") :outputs ("pip version: (21, 2, 3)\nExecuting: get_installed_packages(None, None)\n[{\"name\": \"beta\", \"version\": \"2.0\", \"latest\": \"2.0\", \"summary\": \"Stable caf\\u00e9\", \"home-page\": \"https://example.test/beta\"}, {\"name\": \"alpha\", \"version\": \"1.0\", \"latest\": \"2.0\", \"summary\": \"Upgrade \\u754c\", \"home-page\": \"https://example.test/alpha\"}, {\"name\": \"preview\", \"version\": \"1.0rc1\", \"latest\": \"1.0rc2\", \"summary\": \"Preview\", \"home-page\": \"https://example.test/preview\"}]\nPip finished\n" "pip version: (21, 2, 3)\nExecuting: install_package(delta café, [ROOT]/wheel house/)\nInstalling delta, café to [ROOT]/wheel house/\nPip finished\n" "pip version: (21, 2, 3)\nExecuting: get_installed_packages(None, None)\n[{\"name\": \"beta\", \"version\": \"2.0\", \"latest\": \"2.0\", \"summary\": \"Stable caf\\u00e9\", \"home-page\": \"https://example.test/beta\"}, {\"name\": \"alpha\", \"version\": \"1.0\", \"latest\": \"2.0\", \"summary\": \"Upgrade \\u754c\", \"home-page\": \"https://example.test/alpha\"}, {\"name\": \"preview\", \"version\": \"1.0rc1\", \"latest\": \"1.0rc2\", \"summary\": \"Preview\", \"home-page\": \"https://example.test/preview\"}, {\"name\": \"delta\", \"version\": \"1.0\", \"latest\": \"1.0\", \"summary\": \"Installed target\", \"home-page\": \"https://example.test/delta\"}, {\"name\": \"caf\\u00e9\", \"version\": \"1.0\", \"latest\": \"1.0\", \"summary\": \"Unicode package\", \"home-page\": \"https://example.test/cafe\"}]\nPip finished\n") :launches ((owned-python pippel.py) (owned-python pippel.py) (owned-python pippel.py)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :post-command-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_backend_error_populates_debug_then_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_backend_error_populates_debug_then_recovers",
        r####"(pippel407-test-run
 "recovery" "error"
 (lambda (_root log _install-dir)
   (call-interactively #'pippel-list-packages)
   (let ((failed-process (pippel407-test-wait-process))
         (debug (with-current-buffer " *pippel407-debug*"
                  (buffer-substring-no-properties (point-min) (point-max)))))
     (setenv "PIPPEL407_SCENARIO" "recovery")
     (call-interactively #'pippel-list-packages)
     (list :failure (list failed-process debug
                          (not (get-buffer "*Pip-Packages*")))
           :recovery-process (pippel407-test-wait-process)
           :recovery-menu (pippel407-test-menu-state)
           :pip-boundary (pippel407-test-lines log)))))"####,
        expect![[
            r#"OK (:result (:failure ((:status signal :buffer-gone t) "pip version: (21, 2, 3)\nExecuting: get_installed_packages(None, None)\nPIPPEL_ERROR <<<\nbackend café failed: 界\n\nTraceback (most recent call last):\n  File \"[ORACLE-WORKSPACE]/tmp/melpa/source-install-cache/pippel/20220416.1743/19153aa8845aa95d080f224d4fcaf2d75224bd5a/517749e477c16c0437cae029be71e672061a6c19/d31dec67631f14ef8be3ad6438e172a07298082b/home/.emacs.d/elpa/pippel-20220416.1743/pippel.py\", line 100, in handle_request\n    method(arg1, arg2)\n    ~~~~~~^^^^^^^^^^^^\n  File \"[ORACLE-WORKSPACE]/tmp/melpa/source-install-cache/pippel/20220416.1743/19153aa8845aa95d080f224d4fcaf2d75224bd5a/517749e477c16c0437cae029be71e672061a6c19/d31dec67631f14ef8be3ad6438e172a07298082b/home/.emacs.d/elpa/pippel-20220416.1743/pippel.py\", line 182, in get_installed_packages\n    for d in get_environment(options.path).iter_installed_distributions(\n             ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~^\n        local_only=options.local,\n        ^^^^^^^^^^^^^^^^^^^^^^^^^\n    ...<3 lines>...\n        skip=skip,\n        ^^^^^^^^^^\n    )\n    ^\n  File \"[ROOT]/python-stubs/pip/_internal/metadata.py\", line 46, in iter_installed_distributions\n    raise RuntimeError('backend café failed: 界')\nRuntimeError: backend café failed: 界\n\nPIPPEL_ERROR >>>\n" t) :recovery-process (:status signal :buffer-gone t) :recovery-menu (:mode pippel-package-menu-mode :displayed t :selected nil :keys (("m" pippel-menu-mark-unmark) ("d" pippel-menu-mark-delete) ("U" pippel-menu-mark-all-upgrades) ("u" pippel-menu-mark-upgrade) ("r" pippel-list-packages) ("i" pippel-install-package) ("x" pippel-menu-execute) ("RET" pippel-menu-visit-homepage) ("q" quit-window)) :header "" :format [("Package" 15 nil) ("Version" 10 nil) ("Latest" 10 nil) ("Description" 0 nil)] :text "  alpha           1.0        2.0        Upgrade 界\n  beta            2.0        2.0        Stable café\n  preview         1.0rc1     1.0rc2     Preview\n" :rendered-rows ((:id "alpha" :text "  alpha           1.0        2.0        Upgrade 界" :links ((2 7 "https://example.test/alpha")) :faces ((29 32 (:foreground "orange")))) (:id "beta" :text "  beta            2.0        2.0        Stable café" :links ((2 6 "https://example.test/beta")) :faces nil) (:id "preview" :text "  preview         1.0rc1     1.0rc2     Preview" :links ((2 9 "https://example.test/preview")) :faces ((29 35 (:foreground "orange"))))) :point (1 0 "alpha") :rows (("beta" :name "beta" :link "https://example.test/beta" :version "2.0" :latest "2.0" :latest-face nil :description "Stable café") ("alpha" :name "alpha" :link "https://example.test/alpha" :version "1.0" :latest "2.0" :latest-face (:foreground "orange") :description "Upgrade 界") ("preview" :name "preview" :link "https://example.test/preview" :version "1.0rc1" :latest "1.0rc2" :latest-face (:foreground "orange") :description "Preview")) :tags (("alpha" " ") ("beta" " ") ("preview" " "))) :pip-boundary ("[\"list.parse_args\", [\"--outdated\", \"\"]]" "[\"metadata.environment\", null]" "[\"metadata.iter\", {\"local_only\": false, \"user_only\": false, \"editables_only\": false, \"include_editables\": true, \"skip\": []}]" "[\"list.parse_args\", [\"--outdated\", \"\"]]" "[\"metadata.environment\", null]" "[\"metadata.iter\", {\"local_only\": false, \"user_only\": false, \"editables_only\": false, \"include_editables\": true, \"skip\": []}]" "[\"list.iter_latest\", [\"beta\", \"alpha\", \"preview\"]]" "[\"show.search\", [\"beta\"]]" "[\"show.search\", [\"alpha\"]]" "[\"show.search\", [\"preview\"]]")) :requests ("{\"method\":\"get_installed_packages\",\"arg1\":null,\"arg2\":null}" "{\"method\":\"get_installed_packages\",\"arg1\":null,\"arg2\":null}") :outputs ("pip version: (21, 2, 3)\nExecuting: get_installed_packages(None, None)\nPIPPEL_ERROR <<<\nbackend café failed: 界\n\nTraceback (most recent call last):\n  File \"[ORACLE-WORKSPACE]/tmp/melpa/source-install-cache/pippel/20220416.1743/19153aa8845aa95d080f224d4fcaf2d75224bd5a/517749e477c16c0437cae029be71e672061a6c19/d31dec67631f14ef8be3ad6438e172a07298082b/home/.emacs.d/elpa/pippel-20220416.1743/pippel.py\", line 100, in handle_request\n    method(arg1, arg2)\n    ~~~~~~^^^^^^^^^^^^\n  File \"[ORACLE-WORKSPACE]/tmp/melpa/source-install-cache/pippel/20220416.1743/19153aa8845aa95d080f224d4fcaf2d75224bd5a/517749e477c16c0437cae029be71e672061a6c19/d31dec67631f14ef8be3ad6438e172a07298082b/home/.emacs.d/elpa/pippel-20220416.1743/pippel.py\", line 182, in get_installed_packages\n    for d in get_environment(options.path).iter_installed_distributions(\n             ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~^\n        local_only=options.local,\n        ^^^^^^^^^^^^^^^^^^^^^^^^^\n    ...<3 lines>...\n        skip=skip,\n        ^^^^^^^^^^\n    )\n    ^\n  File \"[ROOT]/python-stubs/pip/_internal/metadata.py\", line 46, in iter_installed_distributions\n    raise RuntimeError('backend café failed: 界')\nRuntimeError: backend café failed: 界\n\nPIPPEL_ERROR >>>\n" "pip version: (21, 2, 3)\nExecuting: get_installed_packages(None, None)\n[{\"name\": \"beta\", \"version\": \"2.0\", \"latest\": \"2.0\", \"summary\": \"Stable caf\\u00e9\", \"home-page\": \"https://example.test/beta\"}, {\"name\": \"alpha\", \"version\": \"1.0\", \"latest\": \"2.0\", \"summary\": \"Upgrade \\u754c\", \"home-page\": \"https://example.test/alpha\"}, {\"name\": \"preview\", \"version\": \"1.0rc1\", \"latest\": \"1.0rc2\", \"summary\": \"Preview\", \"home-page\": \"https://example.test/preview\"}]\nPip finished\n") :launches ((owned-python pippel.py) (owned-python pippel.py)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :post-command-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn pippel_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_list_renders_sorting_links_faces_and_noop_failure(),
        public_user_list_sends_the_user_flag_and_replaces_the_menu(),
        public_marks_execute_upgrade_delete_and_refresh_in_order(),
        public_install_trims_input_and_forwards_the_prefix_directory(),
        public_backend_error_populates_debug_then_recovers(),
    ];
    assert_oracle_batch_cases(oracle(), "pippel-rank407", "pippel", &cases);
}
