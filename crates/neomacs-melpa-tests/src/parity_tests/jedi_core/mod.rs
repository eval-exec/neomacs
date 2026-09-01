//! Practical parity for jedi-core's public EPC client.
//!
//! These cases start the documented Jedi server through a planted EPC
//! stand-in, complete at a real Python buffer, jump to a definition and
//! pop the marker, render a docstring, and recover after a missing
//! server command.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EPC_MELPA_PIN, JEDI_CORE_MELPA_PIN, PYTHON_ENVIRONMENT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn prelude() -> String {
    format!(
        r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'deferred)
(require 'epc)
(require 'python-environment)
(require 'jedi-core)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")
(setq make-backup-files nil create-lockfiles nil)

(defconst jc450-test-tree
  "3581cfd6a9072ee5ae511c8d211852d9e1b0473d")
(defconst jc450-test-manifest
  '(("jedi-core-pkg.el" . "f11c47caf00f2b1c821d9ff7a94dd11bd8afe2189ca9d7f1ff6807a5be5017b2")
    ("jedi-core.el" . "65c7d05cb7c5b80866a4bc79e43148e1c9fa62ba278ba7b6d95ccb563c3d46f6")))
(defconst jc450-test-standin-b64
  "{b64}")

(defvar jc450-test-root nil)
(defvar jc450-test-bin nil)
(defvar jc450-test-fixtures nil)
(defvar jc450-test-log nil)
(defvar jc450-test-path-before nil)
(defvar jc450-test-exec-before nil)
(defvar jc450-test-command-before nil)
(defvar jc450-test-messages nil)

(defun jc450-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun jc450-test-source-state ()
  (let* ((located (locate-library "jedi-core.el"))
         (main (and located (file-truename located)))
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
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (jc450-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/jedi-core.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car jc450-test-manifest)))
      (error "Unexpected installed jedi-core payload: %S"
             (or manifest files)))
    (dolist (entry jc450-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (jc450-test-sha file) expected))
          (error "Unexpected installed jedi-core source: %S"
                 (cons entry manifest)))))
    (list :tree jc450-test-tree
          :manifest manifest
          :feature (featurep 'jedi-core)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'jedi-core package-alist)))))))

(defun jc450-test-write (path text)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun jc450-test-read (path)
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents path)
      (buffer-string))))

(defun jc450-test-mask (text)
  (setq text (replace-regexp-in-string
              (regexp-quote (directory-file-name jc450-test-root))
              "@@ROOT@@" text t t))
  (setq text (replace-regexp-in-string
              (regexp-quote
               (expand-file-name invocation-name invocation-directory))
              "@@EMACS@@" text t t))
  (setq text (replace-regexp-in-string "epc:server:[0-9]+" "epc:server:N" text t t))
  (replace-regexp-in-string "(call [0-9]+ " "(call N " text t t))

(defun jc450-test-app ()
  "import os\n\ndef greet(name):\n    \"\"\"Return a friendly greeting.\"\"\"\n    return \"hello \" + name\n\nclass Counter(object):\n    \"\"\"A simple counter.\"\"\"\n    def __init__(self, start=0):\n        self.value = start\n\n    def increment(self):\n        self.value += 1\n        return self.value\n\n\ndef main():\n    print(greet(\"world\"))\n    c = Counter()\n    c.increment()\n    print(os.getcwd())\n")

(defun jc450-test-names ()
  "def greet(name):\n    \"\"\"Return a friendly greeting.\"\"\"\n    return \"hello \" + name\n\nclass Counter(object):\n    \"\"\"A simple counter.\"\"\"\n    def __init__(self, start=0):\n        self.value = start\n\n    def increment(self):\n        self.value += 1\n        return self.value\n")

(defun jc450-test-open (relpath content)
  (let* ((path (expand-file-name relpath jc450-test-fixtures))
         (name (file-name-nondirectory relpath)))
    (when (get-buffer name)
      (with-current-buffer (get-buffer name)
        (set-buffer-modified-p nil)
        (kill-buffer)))
    (when (file-exists-p jc450-test-log)
      (delete-file jc450-test-log))
    (jc450-test-write path content)
    (find-file path)))

(defun jc450-test-at (line col)
  (goto-char (point-min))
  (forward-line line)
  (forward-char col))

(defun jc450-test-pump ()
  (dotimes (_ 10)
    (sit-for 0.05)))

(defun jc450-test-calls ()
  (jc450-test-mask
   (if (file-exists-p jc450-test-log)
       (jc450-test-read jc450-test-log)
     "")))

(defun jc450-test-install-standin ()
  (let ((program (expand-file-name "python" jc450-test-bin)))
    (jc450-test-write
     program
     (decode-coding-string (base64-decode-string jc450-test-standin-b64)
                           'utf-8-unix))
    (set-file-modes program #o755)
    (setq jc450-test-command-before jedi:server-command
          jedi:server-command
          (list program
                (expand-file-name "jediepcserver.py"
                                  (file-name-directory
                                   (locate-library "jedi-core.el")))))
    (setenv "JEDI_STANDIN_LOG" jc450-test-log)
    (setq jc450-test-path-before (getenv "PATH")
          jc450-test-exec-before (copy-sequence exec-path))
    (setenv "PATH" (concat jc450-test-bin
                           path-separator (getenv "PATH")))
    (setq exec-path (cons jc450-test-bin exec-path))
    program))

(defun jc450-test-reset ()
  (jedi:stop-all-servers)
  (dolist (buf (list jedi:doc-buffer-name "app.py" "names.py" "*Warnings*"))
    (when (get-buffer buf)
      (with-current-buffer (get-buffer buf)
        (set-buffer-modified-p nil)
        (kill-buffer))))
  (setq jedi:use-shortcuts nil
        jedi:tooltip-method '(pos-tip popup)
        jedi:complete-reply nil
        jedi:goto-definition--cache nil)
  (when (file-exists-p jc450-test-log)
    (delete-file jc450-test-log)))

(let ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
  (unless (and sandbox (file-name-absolute-p sandbox))
    (error "Missing absolute jedi-core sandbox root"))
  (setq jc450-test-root (file-name-as-directory sandbox)
        jc450-test-bin (file-name-as-directory
                        (expand-file-name "bin" jc450-test-root))
        jc450-test-fixtures (file-name-as-directory
                             (expand-file-name "jedi-fixtures" jc450-test-root))
        jc450-test-log (expand-file-name "jedi-calls.log" jc450-test-root))
  (jc450-test-install-standin)
  (jc450-test-source-state))
"####,
        b64 = include_str!("standin.b64").trim()
    )
}

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(JEDI_CORE_MELPA_PIN, "jedi-core.el")
        .expect("prepare pinned jedi-core source below ./tmp")
        .with_melpa_dependency(EPC_MELPA_PIN)
        .expect("prepare pinned epc dependency below ./tmp")
        .with_melpa_dependency(PYTHON_ENVIRONMENT_MELPA_PIN)
        .expect("prepare pinned python-environment dependency below ./tmp")
        .with_prelude(prelude())
        .with_timeout(TEST_TIMEOUT)
}

fn completion_request_records_os_get_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "completion_request_records_os_get_candidates",
        r####"
(unwind-protect
    (progn
      (jc450-test-open "app.py" (jc450-test-app))
      (with-current-buffer "app.py"
        (jedi-mode 1)
        (jedi:start-server)
        (jc450-test-at 20 16)
        (deferred:sync! (jedi:complete-request))
        (list :source (jc450-test-source-state)
              :reply-words (mapcar (lambda (x) (plist-get x :word))
                                   jedi:complete-reply)
              :reply-count (length jedi:complete-reply)
              :first (car jedi:complete-reply)
              :request-point jedi:complete-request-point
              :mode jedi-mode
              :calls (jc450-test-calls))))
  (jc450-test-reset))
"####,
        expect![[
            r#"OK (:source (:tree "3581cfd6a9072ee5ae511c8d211852d9e1b0473d" :manifest (("jedi-core-pkg.el" . "f11c47caf00f2b1c821d9ff7a94dd11bd8afe2189ca9d7f1ff6807a5be5017b2") ("jedi-core.el" . "65c7d05cb7c5b80866a4bc79e43148e1c9fa62ba278ba7b6d95ccb563c3d46f6")) :feature t :version "20250602.2109") :reply-words ("get_blocking" "get_exec_path" "get_handle_inheritable" "get_inheritable" "get_terminal_size" "getcwd" "getcwdb" "getegid" "getenv" "getenvb" "geteuid" "getgid" "getgrouplist" "getgroups" "getloadavg" "getlogin" "getpgid" "getpgrp" "getpid" "getppid" "getpriority" "getrandom" "getresgid" "getresuid" "getsid" "getuid" "getxattr") :reply-count 27 :first (:word "get_blocking" :doc "get_blocking(fd: int, /) -> bool\n\nGet the blocking mode of the file descriptor.\n\nReturn False if the O_NONBLOCK flag is set, True if the flag is cleared." :description "def get_blocking" :symbol "function") :request-point 374 :mode t :calls "(call N complete (\"import os\\12\\12def greet(name):\\12    \\\"\\\"\\\"Return a friendly greeting.\\\"\\\"\\\"\\12    return \\\"hello \\\" + name\\12\\12class Counter(object):\\12    \\\"\\\"\\\"A simple counter.\\\"\\\"\\\"\\12    def __init__(self, start=0):\\12        self.value = start\\12\\12    def increment(self):\\12        self.value += 1\\12        return self.value\\12\\12\\12def main():\\12    print(greet(\\\"world\\\"))\\12    c = Counter()\\12    c.increment()\\12    print(os.getcwd())\\12\" 21 16 \"@@ROOT@@/jedi-fixtures/app.py\"))\n\n")"#
        ]],
    )
}

fn goto_definition_jumps_and_pops_the_marker() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_definition_jumps_and_pops_the_marker",
        r####"
(unwind-protect
    (progn
      (jc450-test-open "app.py" (jc450-test-app))
      (with-current-buffer "app.py"
        (jedi-mode 1)
        (jedi:start-server)
        (jc450-test-at 17 11)
        (let ((call-site (point)))
          (jedi:goto-definition-push-marker)
          (jedi:goto-definition)
          (jc450-test-pump)
          (let ((definition
                 (list :line (line-number-at-pos)
                       :column (- (point) (line-beginning-position))
                       :text (buffer-substring-no-properties
                              (line-beginning-position)
                              (line-end-position)))))
            (jedi:goto-definition-pop-marker)
            (list :source (jc450-test-source-state)
                  :definition definition
                  :returned (and (= (point) call-site)
                                 (eq (current-buffer) (get-buffer "app.py")))
                  :calls (jc450-test-calls))))))
  (jc450-test-reset))
"####,
        expect![[
            r#"OK (:source (:tree "3581cfd6a9072ee5ae511c8d211852d9e1b0473d" :manifest (("jedi-core-pkg.el" . "f11c47caf00f2b1c821d9ff7a94dd11bd8afe2189ca9d7f1ff6807a5be5017b2") ("jedi-core.el" . "65c7d05cb7c5b80866a4bc79e43148e1c9fa62ba278ba7b6d95ccb563c3d46f6")) :feature t :version "20250602.2109") :definition (:line 3 :column 4 :text "def greet(name):") :returned t :calls "(call N goto (\"import os\\12\\12def greet(name):\\12    \\\"\\\"\\\"Return a friendly greeting.\\\"\\\"\\\"\\12    return \\\"hello \\\" + name\\12\\12class Counter(object):\\12    \\\"\\\"\\\"A simple counter.\\\"\\\"\\\"\\12    def __init__(self, start=0):\\12        self.value = start\\12\\12    def increment(self):\\12        self.value += 1\\12        return self.value\\12\\12\\12def main():\\12    print(greet(\\\"world\\\"))\\12    c = Counter()\\12    c.increment()\\12    print(os.getcwd())\\12\" 18 11 \"@@ROOT@@/jedi-fixtures/app.py\"))\n\n")"#
        ]],
    )
}

fn show_doc_renders_the_counter_docstring() -> ParityBatchCase {
    ParityBatchCase::value(
        "show_doc_renders_the_counter_docstring",
        r####"
(unwind-protect
    (progn
      (jc450-test-open "app.py" (jc450-test-app))
      (with-current-buffer "app.py"
        (jedi-mode 1)
        (jedi:start-server)
        (jc450-test-at 18 9)
        (jedi:show-doc)
        (jc450-test-pump)
        (list :source (jc450-test-source-state)
              :doc (with-current-buffer (get-buffer jedi:doc-buffer-name)
                     (buffer-substring-no-properties (point-min) (point-max)))
              :calls (jc450-test-calls))))
  (jc450-test-reset))
"####,
        expect![[
            r#"OK (:source (:tree "3581cfd6a9072ee5ae511c8d211852d9e1b0473d" :manifest (("jedi-core-pkg.el" . "f11c47caf00f2b1c821d9ff7a94dd11bd8afe2189ca9d7f1ff6807a5be5017b2") ("jedi-core.el" . "65c7d05cb7c5b80866a4bc79e43148e1c9fa62ba278ba7b6d95ccb563c3d46f6")) :feature t :version "20250602.2109") :doc "Docstring for __main__.Counter\n\nCounter(start=0)\n\nA simple counter." :calls "(call N get_definition (\"import os\\12\\12def greet(name):\\12    \\\"\\\"\\\"Return a friendly greeting.\\\"\\\"\\\"\\12    return \\\"hello \\\" + name\\12\\12class Counter(object):\\12    \\\"\\\"\\\"A simple counter.\\\"\\\"\\\"\\12    def __init__(self, start=0):\\12        self.value = start\\12\\12    def increment(self):\\12        self.value += 1\\12        return self.value\\12\\12\\12def main():\\12    print(greet(\\\"world\\\"))\\12    c = Counter()\\12    c.increment()\\12    print(os.getcwd())\\12\" 19 9 \"@@ROOT@@/jedi-fixtures/app.py\"))\n\n")"#
        ]],
    )
}

fn defined_names_build_imenu_and_missing_server_disables_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "defined_names_build_imenu_and_missing_server_disables_mode",
        r####"
(unwind-protect
    (progn
      (jc450-test-open "names.py" (jc450-test-names))
      (with-current-buffer "names.py"
        (jedi-mode 1)
        (jedi:start-server)
        (deferred:sync! (jedi:defined-names-deferred))
        (let ((index (jedi:create-nested-imenu-index))
              (names (mapcar (lambda (x) (plist-get (car x) :local_name))
                             jedi:defined-names--cache)))
          (jedi:stop-server)
          (setq jedi:server-command
                (list (expand-file-name "no-such-jedi-python" jc450-test-root)
                      "jediepcserver.py"))
          (let ((bad nil))
            (condition-case err
                (jedi:start-server)
              (error (setq bad (list (car err) (cadr err)))))
            (list :source (jc450-test-source-state)
                  :names names
                  :index
                  (mapcar
                   (lambda (entry)
                     (if (consp (cdr entry))
                         (list :name (car entry)
                               :children (mapcar #'car (cddr entry))
                               :line (line-number-at-pos (cdr (cadr entry))))
                       (list :name (car entry)
                             :line (line-number-at-pos (cdr entry)))))
                   index)
                  :bad-start bad
                  :mode jedi-mode
                  :calls (jc450-test-calls))))))
  (jc450-test-reset))
"####,
        expect![[
            r#"OK (:source (:tree "3581cfd6a9072ee5ae511c8d211852d9e1b0473d" :manifest (("jedi-core-pkg.el" . "f11c47caf00f2b1c821d9ff7a94dd11bd8afe2189ca9d7f1ff6807a5be5017b2") ("jedi-core.el" . "65c7d05cb7c5b80866a4bc79e43148e1c9fa62ba278ba7b6d95ccb563c3d46f6")) :feature t :version "20250602.2109") :names ("greet" "Counter") :index ((:name "greet" :line 1) (:name "Counter" :children ("__init__" "increment") :line 5)) :bad-start (wrong-type-argument epc:manager) :mode nil :calls "(call N defined_names (\"def greet(name):\\12    \\\"\\\"\\\"Return a friendly greeting.\\\"\\\"\\\"\\12    return \\\"hello \\\" + name\\12\\12class Counter(object):\\12    \\\"\\\"\\\"A simple counter.\\\"\\\"\\\"\\12    def __init__(self, start=0):\\12        self.value = start\\12\\12    def increment(self):\\12        self.value += 1\\12        return self.value\\12\" \"@@ROOT@@/jedi-fixtures/names.py\"))\n\n")"#
        ]],
    )
}

#[test]
fn jedi_core_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        completion_request_records_os_get_candidates(),
        goto_definition_jumps_and_pops_the_marker(),
        show_doc_renders_the_counter_docstring(),
        defined_names_build_imenu_and_missing_server_disables_mode(),
    ];
    assert_oracle_batch_cases(oracle(), "jedi-core-rank450", "jedi_core_parity", &cases);
}
