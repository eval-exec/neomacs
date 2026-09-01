use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, LSP_PYTHON_MS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)

;; These two functions are the optional lsp-ui boundary to which the package
;; attaches filter-return advice.  Keeping the boundary tiny lets the workflow
;; exercise the real package advice without adding undeclared lsp-ui coverage.
(defun lsp-ui-doc--extract (value)
  (concat "DOC<" value ">"))
(defun lsp-ui-sideline--format-info (value)
  (concat "SIDE<" value ">"))

(setq lsp-python-ms-extra-major-modes '(python-ts-mode))
(require 'lsp-python-ms)

(defvar lsp373-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar lsp373-test-locate-original nil)
(defvar lsp373-test-locate-root nil)
(defvar conda-env-executables-dir nil)
(defvar conda-env-current-name nil)

(defun lsp373-test-case-root (name)
  (let ((root (file-name-as-directory (expand-file-name name lsp373-test-root))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun lsp373-test-write (path bytes &optional executable)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert bytes)
      (write-region (point-min) (point-max) path nil 'silent)))
  (set-file-modes path (if executable #o755 #o644))
  path)

(defun lsp373-test-file-bytes (path)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (buffer-string)))

(defun lsp373-test-normalize-string (value root)
  (replace-regexp-in-string
   "mspyls[[:alnum:]]+\\.zip"
   "[ARCHIVE]"
   (replace-regexp-in-string
    (regexp-quote (file-name-as-directory root))
    "[ROOT]/" value t t)
   t t))

(defun lsp373-test-stable (value root)
  (cond
   ((hash-table-p value)
    (sort
     (mapcar
      (lambda (key)
        (cons (lsp373-test-stable key root)
              (lsp373-test-stable (gethash key value) root)))
      (hash-table-keys value))
     (lambda (left right)
       (string< (format "%S" (car left)) (format "%S" (car right))))))
   ((vectorp value)
    (vconcat (mapcar (lambda (item) (lsp373-test-stable item root)) value)))
   ((consp value)
    (mapcar (lambda (item) (lsp373-test-stable item root)) value))
   ((stringp value) (lsp373-test-normalize-string (copy-sequence value) root))
   ((bufferp value) (buffer-name value))
   (t value)))

(defun lsp373-test-condition (thunk)
  (condition-case err
      (list :value (funcall thunk))
    (error
     (list :signal (car err)
           :data (lsp373-test-stable (cdr err) lsp373-test-root)
           :message (error-message-string err)))))

(defun lsp373-test-lines (path root)
  (if (not (file-exists-p path))
      nil
    (with-temp-buffer
      (insert-file-contents path)
      (mapcar
       (lambda (line) (lsp373-test-normalize-string line root))
       (split-string (buffer-string) "\n" t)))))

(defun lsp373-test-contained-locate (file predicate)
  "Delegate lookup while refusing a result above the owned case root."
  (let ((result (funcall lsp373-test-locate-original file predicate)))
    (and result
         (or (equal (directory-file-name result)
                    (directory-file-name lsp373-test-locate-root))
             (file-in-directory-p result lsp373-test-locate-root))
         result)))

(defun lsp373-test-client-handlers (client)
  (sort
   (mapcar
    (lambda (method)
      (list (copy-sequence method)
            (gethash method (lsp--client-notification-handlers client))))
    (hash-table-keys (lsp--client-notification-handlers client)))
   (lambda (left right) (string< (car left) (car right)))))

(defun lsp373-test-http-buffer (header body)
  (let ((buffer (generate-new-buffer " *lsp373-http*")))
    (with-current-buffer buffer
      (set-buffer-multibyte nil)
      (insert header body)
      (goto-char (point-min)))
    buffer))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LSP_PYTHON_MS_MELPA_PIN, "lsp-python-ms.el")
        .expect("prepare pinned lsp-python-ms source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

// Loading this historical client mutates process-global LSP client/settings
// tables and installs two optional lsp-ui advices without an unload API.  A
// fresh editor per story is therefore the truthful isolation boundary.

fn registration_configuration_and_download_policy_are_real() -> ParityBatchCase {
    let form = r####"
(let* ((root (lsp373-test-case-root "registration"))
       (server (lsp373-test-write
                (expand-file-name "server/Microsoft.Python.LanguageServer" root)
                "#!/bin/sh\nexit 0\n" t))
       (client (gethash 'mspyls lsp-clients))
       (connection (lsp--client-new-connection client))
       (notifications nil)
       (download-events nil)
       (workspace (make-lsp--workspace :root root :client client :buffers nil))
       (lsp-python-ms-executable server)
       (lsp-python-ms-completion-add-brackets nil)
       (lsp-python-ms-cache "Library")
       (lsp-python-ms-errors ["undefined-variable" "unicode-界"])
       (lsp-python-ms-warnings ["unresolved-import"])
       (lsp-python-ms-information ["analysis-info"])
       (lsp-python-ms-disabled ["too-many-function-arguments"])
       (lsp-python-ms-extra-paths ["src" "vendor types/λ"]))
  (unwind-protect
      (list
       :client
       (list :id (lsp--client-server-id client)
             :priority (lsp--client-priority client)
             :major-modes (copy-sequence (lsp--client-major-modes client))
             :initialization (lsp--client-initialization-options client)
             :handlers (lsp373-test-client-handlers client)
             :download (functionp (lsp--client-download-server-fn client)))
       :language-id (copy-tree (assq 'python-ts-mode lsp-language-id-configuration))
       :connection
       (list :keys (sort (mapcar (lambda (key) (substring (symbol-name key) 1))
                                 (seq-filter #'keywordp connection))
                         #'string<)
             :present (and (funcall (plist-get connection :test?)) t)
             :missing
             (let ((lsp-python-ms-executable
                    (expand-file-name "server/missing" root)))
               (funcall (plist-get connection :test?))))
       :settings
       (lsp373-test-stable (lsp-configuration-section "python") root)
       :initialized
       (cl-letf (((symbol-function 'lsp-notify)
                  (lambda (method params)
                    (push (list method (lsp373-test-stable params root))
                          notifications))))
         (funcall (lsp--client-initialized-fn client) workspace)
         (nreverse notifications))
       :disabled-download
       (let ((lsp-python-ms-auto-install-server nil))
         (funcall (lsp--client-download-server-fn client)
                  client
                  (lambda () (push 'success download-events))
                  (lambda (&rest _) (push 'error download-events))
                  t)
         (nreverse download-events)))
    (when (file-exists-p root) (delete-directory root t))))
"####;
    ParityBatchCase::value(
        "registration_configuration_and_download_policy_are_real",
        form,
        expect![[r#"OK (:client (:id mspyls :priority 1 :major-modes (python-mode python-ts-mode) :initialization lsp-python-ms--extra-init-params :handlers (("python/beginProgress" lsp-python-ms--begin-progress-callback) ("python/endProgress" lsp-python-ms--end-progress-callback) ("python/languageServerStarted" lsp-python-ms--language-server-started-callback) ("python/reportProgress" lsp-python-ms--report-progress-callback) ("telemetry/event" ignore)) :download t) :language-id (python-ts-mode . "python") :connection (:keys ("connect" "test?") :present t :missing nil) :settings (("python" ("analysis" ("autoSearchPaths" . :json-false) ("cachingLevel" . "Library") ("disabled" . ["too-many-function-arguments"]) ("errors" . ["undefined-variable" "unicode-界"]) ("information" . ["analysis-info"]) ("warnings" . ["unresolved-import"])) ("autoComplete" ("addBrackets" . :json-false) ("extraPaths" . ["src" "vendor types/λ"])))) :initialized (("workspace/didChangeConfiguration" (:settings (("python" ("analysis" ("autoSearchPaths" . :json-false) ("cachingLevel" . "Library") ("disabled" . ["too-many-function-arguments"]) ("errors" . ["undefined-variable" "unicode-界"]) ("information" . ["analysis-info"]) ("warnings" . ["unresolved-import"])) ("autoComplete" ("addBrackets" . :json-false) ("extraPaths" . ["src" "vendor types/λ"]))))))) :disabled-download nil)"#]],
    )
    .fresh_process()
}

fn python_environment_precedence_uses_owned_executables() -> ParityBatchCase {
    let form = r####"
(let* ((root (lsp373-test-case-root "environment-precedence"))
       (project (file-name-as-directory (expand-file-name "project Ω/" root)))
       (deep (file-name-as-directory (expand-file-name "src/deep/" project)))
       (bin (file-name-as-directory (expand-file-name "tools/" root)))
       (trace (expand-file-name "manager.log" root))
       (explicit (lsp373-test-write (expand-file-name "explicit/python" root)
                                    "#!/bin/sh\nexit 0\n" t))
       (venv (lsp373-test-write (expand-file-name "venv-owned/bin/python" project)
                                "#!/bin/sh\nexit 0\n" t))
       (asdf-python (lsp373-test-write (expand-file-name "asdf/python" root)
                                       "#!/bin/sh\nexit 0\n" t))
       (pyenv-python (lsp373-test-write (expand-file-name "pyenv/python" root)
                                        "#!/bin/sh\nexit 0\n" t))
       (conda-home (file-name-as-directory (expand-file-name "conda/owned-env/" root)))
       (conda-python (lsp373-test-write (expand-file-name "bin/python" conda-home)
                                        "#!/bin/sh\nexit 0\n" t))
       (system-python (lsp373-test-write (expand-file-name "python" bin)
                                         "#!/bin/sh\nexit 0\n" t))
       (_asdf (lsp373-test-write
               (expand-file-name "asdf" bin)
               "#!/bin/sh\nprintf 'asdf %s\\n' \"$*\" >> \"$LSP373_MANAGER_TRACE\"\nprintf '%s\\n' \"$LSP373_ASDF_PYTHON\"\n" t))
       (_pyenv (lsp373-test-write
                (expand-file-name "pyenv" bin)
                "#!/bin/sh\nprintf 'pyenv %s\\n' \"$*\" >> \"$LSP373_MANAGER_TRACE\"\nprintf '%s\\n' \"$LSP373_PYENV_PYTHON\"\n" t))
       (process-environment (copy-sequence process-environment))
       (exec-path (list (directory-file-name bin)))
       (shell-file-name "/bin/sh")
       (conda-env-executables-dir "bin")
       (conda-env-current-name nil)
       (conda-calls nil)
       (lsp-python-ms-python-executable-cmd "python")
       (lsp-python-ms-prefer-remote-env t))
  (unwind-protect
      (progn
        (make-directory deep t)
        (lsp373-test-write (expand-file-name ".tool-versions" project)
                           "python 3.11.9\n")
        (lsp373-test-write (expand-file-name ".python-version" project)
                           "3.12.4\n")
        (lsp373-test-write (expand-file-name "environment.yml" project)
                           "name: owned-env\ndependencies: [python=3.11]\n")
        (setenv "PATH" bin)
        (setenv "LSP373_MANAGER_TRACE" trace)
        (setenv "LSP373_ASDF_PYTHON" asdf-python)
        (setenv "LSP373_PYENV_PYTHON" pyenv-python)
        (let ((lsp373-test-locate-original
               (symbol-function 'locate-dominating-file))
              (lsp373-test-locate-root root))
          (cl-letf (((symbol-function 'locate-dominating-file)
                     #'lsp373-test-contained-locate)
                    ((symbol-function 'conda--get-name-from-env-yml)
                     (lambda (file)
                       (push (list 'parse
                                   (lsp373-test-normalize-string file root))
                             conda-calls)
                       "owned-env"))
                    ((symbol-function 'conda-env-name-to-dir)
                     (lambda (name)
                       (push (list 'resolve (copy-sequence name)) conda-calls)
                       conda-home)))
            (let* ((lsp-python-ms-guess-env t)
               (explicit-result
                (let ((lsp-python-ms-python-executable explicit))
                  (lsp-python-ms-locate-python deep)))
               (venv-result
                (let ((lsp-python-ms-python-executable nil))
                  (lsp-python-ms-locate-python deep))))
          (set-file-modes venv #o644)
          (let ((asdf-result
                 (let ((lsp-python-ms-python-executable nil))
                   (lsp-python-ms-locate-python deep))))
            (delete-file (expand-file-name ".tool-versions" project))
            (let ((pyenv-result
                   (let ((lsp-python-ms-python-executable nil))
                     (lsp-python-ms-locate-python deep))))
              (delete-file (expand-file-name ".python-version" project))
              (let ((conda-result
                     (let ((lsp-python-ms-python-executable nil))
                       (lsp-python-ms-locate-python deep))))
                (delete-file (expand-file-name "environment.yml" project))
                (let ((system-result
                       (let ((lsp-python-ms-python-executable nil))
                         (lsp-python-ms-locate-python deep))))
                (set-file-modes venv #o755)
                (lsp373-test-write (expand-file-name ".tool-versions" project)
                                   "python 3.11.9\n")
                (let ((guess-disabled
                       (let ((lsp-python-ms-guess-env nil)
                             (lsp-python-ms-python-executable explicit))
                         (lsp-python-ms-locate-python deep))))
                  (set-file-modes explicit #o644)
                  (set-file-modes venv #o644)
                  (delete-file (expand-file-name ".tool-versions" project))
                  (let ((invalid-explicit
                         (let ((lsp-python-ms-python-executable explicit))
                           (lsp-python-ms-locate-python deep))))
                    (list
                     :results
                     (mapcar
                      (lambda (entry)
                        (list (car entry)
                              (lsp373-test-normalize-string (cdr entry) root)))
                      `((explicit . ,explicit-result)
                        (venv . ,venv-result)
                        (asdf . ,asdf-result)
                        (pyenv . ,pyenv-result)
                        (conda . ,conda-result)
                        (system . ,system-result)
                        (guess-disabled . ,guess-disabled)
                        (invalid-explicit . ,invalid-explicit)))
                     :manager-calls (lsp373-test-lines trace root)
                     :conda-calls (nreverse conda-calls)
                     :system-executable
                     (lsp373-test-normalize-string system-python root)
                     :prefer-remote lsp-python-ms-prefer-remote-env)))))))))))
    (when (file-exists-p root) (delete-directory root t))))
"####;
    ParityBatchCase::value(
        "python_environment_precedence_uses_owned_executables",
        form,
        expect![[r#"OK (:results ((explicit "[ROOT]/explicit/python") (venv "[ROOT]/project Ω/venv-owned/bin/python") (asdf "[ROOT]/asdf/python") (pyenv "[ROOT]/pyenv/python") (conda "[ROOT]/conda/owned-env/bin/python") (system "[ROOT]/tools/python") (guess-disabled "[ROOT]/tools/python") (invalid-explicit "[ROOT]/tools/python")) :manager-calls ("pyenv which python" "asdf which python" "pyenv which python" "asdf which python" "pyenv which python" "asdf which python" "pyenv which python" "asdf which python") :conda-calls ((parse "[ROOT]/project Ω/environment.yml") (resolve "owned-env") (parse "[ROOT]/project Ω/environment.yml") (resolve "owned-env") (parse "[ROOT]/project Ω/environment.yml") (resolve "owned-env") (parse "[ROOT]/project Ω/environment.yml") (resolve "owned-env") (parse "[ROOT]/project Ω/environment.yml") (resolve "owned-env")) :system-executable "[ROOT]/tools/python" :prefer-remote t)"#]],
    )
    .fresh_process()
}

fn initialization_uses_python_adapter_workspace_and_dot_env() -> ParityBatchCase {
    let form = r####"
(let* ((root (lsp373-test-case-root "initialization"))
       (project (file-name-as-directory (expand-file-name "project space Ω/" root)))
       (source (expand-file-name "src/app.py" project))
       (adapter (expand-file-name "bin/python" root))
       (trace (expand-file-name "python-argv.log" root))
       (site (expand-file-name "site packages/界" project))
       (typeshed (file-name-as-directory (expand-file-name "mspyls/" root)))
       (process-environment (copy-sequence process-environment))
       (lsp--session (make-lsp-session))
       (lsp-session-file nil)
       (lsp-python-ms-python-executable adapter)
       (lsp-python-ms-guess-env t)
       (lsp-python-ms-extra-paths
        (vector "src" (expand-file-name "vendor types/λ" project)))
       (lsp-python-ms-dir typeshed)
       (lsp-python-ms-log-level "Trace")
       (lsp-python-ms-parse-dot-env-enabled t)
       (client (gethash 'mspyls lsp-clients))
       buffer)
  (unwind-protect
      (progn
        (make-directory (file-name-directory source) t)
        (lsp373-test-write source "print('hello, 界')\n")
        (lsp373-test-write
         (expand-file-name ".env" project)
         "IGNORED=value\nPYTHONPATH=src:vendor types/λ\n")
        (lsp373-test-write
         adapter
         "#!/bin/sh\nif [ \"$#\" -ne 2 ] || [ \"$1\" != \"-c\" ]; then exit 71; fi\nprintf '%s\\n%s\\n' \"$1\" \"$2\" > \"$LSP373_PYTHON_TRACE\"\nprintf '{\"version\":\"3.11\",\"paths\":[\"%s\",\"/stdlib\",\"%s\"],\"executable\":\"%s\"}\\n' \"$LSP373_PROJECT\" \"$LSP373_SITE\" \"$LSP373_PYTHON_EXEC\"\n"
         t)
        (setenv "LSP373_PYTHON_TRACE" trace)
        (setenv "LSP373_PROJECT" project)
        (setenv "LSP373_SITE" site)
        (setenv "LSP373_PYTHON_EXEC" adapter)
        (setenv "PYTHONPATH" nil)
        (setf (lsp-session-folders lsp--session) (list project))
        (setq buffer (find-file-noselect source))
        (with-current-buffer buffer
          (let ((default-directory project))
            (let ((options
                   (funcall (lsp--client-initialization-options client))))
              (list
               :callback (lsp--client-initialization-options client)
               :workspace-root
               (lsp373-test-normalize-string (lsp-workspace-root) root)
               :options (lsp373-test-stable options root)
               :python-argv (lsp373-test-lines trace root)
               :environment
               (list :pythonpath (getenv "PYTHONPATH")))))))
    (when (buffer-live-p buffer) (kill-buffer buffer))
    (when (file-exists-p root) (delete-directory root t))))
"####;
    ParityBatchCase::value(
        "initialization_uses_python_adapter_workspace_and_dot_env",
        form,
        expect![[r#"OK (:callback lsp-python-ms--extra-init-params :workspace-root "[ROOT]/project space Ω/" :options (:interpreter (:properties (:InterpreterPath "[ROOT]/bin/python" :UseDefaultDatabase t :Version "3.11")) :displayOptions (:preferredFormat "markdown" :trimDocumentationLines :json-false :maxDocumentationLineLength 0 :trimDocumentationText :json-false :maxDocumentationTextLength 0) :searchPaths ["src" "[ROOT]/project space Ω/vendor types/λ" "[ROOT]/project space Ω/" "/stdlib" "[ROOT]/project space Ω/site packages/界"] :analysisUpdates t :asyncStartup t :logLevel "Trace" :typeStubSearchPaths ["[ROOT]/mspyls/Typeshed"]) :python-argv ("-c" "from __future__ import print_function; import sys; sys.path = list(filter(lambda p: p != '', sys.path)); import json;v=(\"%s.%s\" % (sys.version_info[0], sys.version_info[1]));sys.path.insert(0, '[ROOT]/project space Ω/'); p=sys.path;e=sys.executable;print(json.dumps({\"version\":v,\"paths\":p,\"executable\":e}))") :environment (:pythonpath "src:vendor types/λ"))"#]],
    )
    .fresh_process()
}

fn documentation_and_progress_handlers_drive_visible_boundaries() -> ParityBatchCase {
    let form = r####"
(let* ((client (gethash 'mspyls lsp-clients))
       (handlers (lsp--client-notification-handlers client))
       (first (generate-new-buffer "lsp373-progress-a"))
       (second (generate-new-buffer "lsp373-progress-界"))
       (dead (generate-new-buffer "lsp373-progress-dead"))
       (workspace
        (make-lsp--workspace :root lsp373-test-root :client client
                             :buffers (list first dead second)))
       (spinner-events nil)
       (logs nil)
       (infos nil))
  (kill-buffer dead)
  (unwind-protect
      (cl-letf (((symbol-function 'lsp--spinner-start)
                 (lambda ()
                   (push (list 'start (copy-sequence (buffer-name)))
                         spinner-events)))
                ((symbol-function 'lsp--spinner-stop)
                 (lambda ()
                   (push (list 'stop (copy-sequence (buffer-name)))
                         spinner-events)))
                ((symbol-function 'lsp-log)
                 (lambda (format &rest args)
                   (push (apply #'format format args) logs)))
                ((symbol-function 'lsp--info)
                 (lambda (format &rest args)
                   (push (apply #'format format args) infos))))
        (funcall (gethash "python/reportProgress" handlers) workspace [])
        (funcall (gethash "python/reportProgress" handlers)
                 workspace ["Indexed 42 modules" "ignored"])
        (funcall (gethash "python/languageServerStarted" handlers)
                 workspace nil)
        (funcall (gethash "python/beginProgress" handlers) workspace nil)
        (funcall (gethash "python/endProgress" handlers) workspace nil)
        (list
         :renderer
         (list :function lsp-render-markdown-markup-content
               :value (funcall lsp-render-markdown-markup-content
                               "left&nbsp;middle&nbsp;界"))
         :optional-ui
         (list :doc (lsp-ui-doc--extract "A&nbsp;B")
               :sideline (lsp-ui-sideline--format-info "C&nbsp;D")
               :doc-advice
               (and (advice-member-p #'lsp-python-ms--filter-nbsp
                                     'lsp-ui-doc--extract) t)
               :sideline-advice
               (and (advice-member-p #'lsp-python-ms--filter-nbsp
                                     'lsp-ui-sideline--format-info) t))
         :spinners (nreverse spinner-events)
         :logs (nreverse logs)
         :infos (nreverse infos)
         :workspace-buffers
         (mapcar (lambda (buffer)
                   (list (and (buffer-live-p buffer)
                              (copy-sequence (buffer-name buffer)))
                         (buffer-live-p buffer)))
                 (lsp--workspace-buffers workspace))))
    (when (buffer-live-p first) (kill-buffer first))
    (when (buffer-live-p second) (kill-buffer second))))
"####;
    ParityBatchCase::value(
        "documentation_and_progress_handlers_drive_visible_boundaries",
        form,
        expect![[r#"OK (:renderer (:function lsp-python-ms--filter-nbsp :value "left middle 界") :optional-ui (:doc "DOC<A B>" :sideline "SIDE<C D>" :doc-advice t :sideline-advice t) :spinners ((start "lsp373-progress-a") (start "lsp373-progress-界") (stop "lsp373-progress-a") (stop "lsp373-progress-界")) :logs ("Indexed 42 modules") :infos ("Microsoft Python language server started" "Microsoft Python language server is analyzing..." "Microsoft Python language server is analyzing...done") :workspace-buffers (("lsp373-progress-a" t) (nil nil) ("lsp373-progress-界" t)))"#]],
    )
    .fresh_process()
}

fn public_installer_handles_failure_then_recovers() -> ParityBatchCase {
    let form = r####"
(let* ((root (lsp373-test-case-root "installer"))
       (install-dir (file-name-as-directory (expand-file-name "server/mspyls/" root)))
       (executable (expand-file-name "Microsoft.Python.LanguageServer" install-dir))
       (temp-dir (file-name-as-directory (expand-file-name "temp/" root)))
       (client (gethash 'mspyls lsp-clients))
       (listing-url nil)
       (download-urls nil)
       (process-calls nil)
       (messages nil)
       (restarts nil)
       (failure-callbacks nil)
       (success-callbacks nil)
       (http-buffers nil)
       (fail-next t)
       (process-environment (copy-sequence process-environment))
       (temporary-file-directory temp-dir)
       (lsp-python-ms-dir install-dir)
       (lsp-python-ms-executable executable)
       (lsp-python-ms-base-url "https://packages.invalid/container")
       (lsp-python-ms-nupkg-channel "stable")
       (lsp-python-ms-auto-install-server t)
       (lsp-mode t)
       (xml
        "<EnumerationResults ContainerName=\"python-language-server-stable\"><Prefix>Python-Language-Server-linux-x64</Prefix><Blobs><Blob><Name>old.zip</Name><Url>https://packages.invalid/old.zip</Url><Properties><Last-Modified>Mon, 01 Jan 2024 00:00:00 GMT</Last-Modified><Etag>old</Etag></Properties></Blob><Blob><Name>new.zip</Name><Url>https://packages.invalid/new.zip</Url><Properties><Last-Modified>Tue, 02 Jan 2024 00:00:00 GMT</Last-Modified><Etag>new</Etag></Properties></Blob></Blobs><NextMarker /></EnumerationResults>"))
  (unwind-protect
      (progn
        (make-directory temp-dir t)
        (make-directory install-dir t)
        (lsp373-test-write (expand-file-name "stale.txt" install-dir) "stale\n")
        (let ((invalid
               (lsp373-test-condition
                (lambda () (lsp-python-ms-latest-nupkg-url "canary"))))
              (disabled-events nil)
              (after-failure nil))
          (let ((lsp-python-ms-auto-install-server nil))
            (funcall (lsp--client-download-server-fn client)
                     client
                     (lambda () (push 'success disabled-events))
                     (lambda (&rest _) (push 'error disabled-events))
                     t))
          (cl-letf
              (((symbol-function 'url-retrieve-synchronously)
                (lambda (url &rest _)
                  (setq listing-url url)
                  (let ((buffer (lsp373-test-http-buffer "HTTP/1.1 200 OK\n\n" xml)))
                    (push buffer http-buffers)
                    buffer)))
               ((symbol-function 'url-retrieve)
                (lambda (url callback &optional _cbargs _silent _inhibit-cookies)
                  (push url download-urls)
                  (let ((buffer
                         (lsp373-test-http-buffer
                          "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\n\r\n"
                          "PK\003\004owned-mspyls-archive")))
                    (push buffer http-buffers)
                    (with-current-buffer buffer (funcall callback nil))
                    buffer)))
               ((symbol-function 'executable-find)
                (lambda (command &optional _remote)
                  (and (equal command "unzip") "/owned/bin/unzip")))
               ((symbol-function 'lsp-async-start-process)
                (lambda (success error program &rest args)
                  (push (list program args) process-calls)
                  (if fail-next
                      (progn (setq fail-next nil)
                             (funcall error 'extract-failed))
                    (lsp373-test-write executable "#!/bin/sh\necho mspyls\n")
                    (set-file-modes executable #o600)
                    (funcall success))))
               ((symbol-function 'lsp--info)
                (lambda (format &rest args)
                  (push (apply #'format format args) messages)))
               ((symbol-function 'lsp)
                (lambda () (interactive) (push 'lsp restarts))))
            ;; First drive the registered installer callback through its real
            ;; download and extraction-error path.
            (funcall (lsp--client-download-server-fn client)
                     client
                     (lambda () (push 'success success-callbacks))
                     (lambda (&rest args) (push args failure-callbacks))
                     t)
            (setq after-failure
                  (list :install-dir (file-directory-p install-dir)
                        :executable (file-exists-p executable)
                        :stale (file-exists-p
                                (expand-file-name "stale.txt" install-dir))))
            ;; The public updater must recover through the same package code.
            (make-directory install-dir t)
            (lsp373-test-write (expand-file-name "stale-again.txt" install-dir)
                               "stale again\n")
            (call-interactively #'lsp-python-ms-update-server)
            (let* ((archives
                    (sort (directory-files temp-dir t "\\.zip\\'") #'string<))
                   (mode (file-modes executable)))
              (list
               :invalid-channel invalid
               :disabled-events (nreverse disabled-events)
               :listing-url listing-url
               :download-urls (nreverse download-urls)
               :process-calls
               (lsp373-test-stable (nreverse process-calls) root)
               :callbacks
               (list :failures (nreverse failure-callbacks)
                     :successes (nreverse success-callbacks))
               :after-failure after-failure
               :archives (mapcar #'lsp373-test-file-bytes archives)
               :installed
               (list :exists (file-exists-p executable)
                     :bytes (lsp373-test-file-bytes executable)
                     :mode (logand mode #o777)
                     :stale (directory-files install-dir nil "stale"))
               :restarts (nreverse restarts)
               :messages (nreverse messages))))))
    (dolist (buffer http-buffers)
      (when (buffer-live-p buffer) (kill-buffer buffer)))
    (when (file-exists-p root) (delete-directory root t))))
"####;
    ParityBatchCase::value(
        "public_installer_handles_failure_then_recovers",
        form,
        expect![[r##"OK (:invalid-channel (:signal user-error :data ("Unknown channel: canary") :message "Unknown channel: canary") :disabled-events nil :listing-url "https://packages.invalid/container/python-language-server-stable?restype=container&comp=list&prefix=Python-Language-Server-linux-x64" :download-urls ("https://packages.invalid/new.zip" "https://packages.invalid/new.zip") :process-calls (("sh" ("-c" "mkdir -p [ROOT]/server/mspyls/ && unzip -qq [ROOT]/temp/[ARCHIVE] -d [ROOT]/server/mspyls/")) ("sh" ("-c" "mkdir -p [ROOT]/server/mspyls/ && unzip -qq [ROOT]/temp/[ARCHIVE] -d [ROOT]/server/mspyls/"))) :callbacks (:failures ((extract-failed)) :successes nil) :after-failure (:install-dir nil :executable nil :stale nil) :archives ("PK\3\4owned-mspyls-archive" "PK\3\4owned-mspyls-archive") :installed (:exists t :bytes "#!/bin/sh\necho mspyls\n" :mode 493 :stale nil) :restarts (lsp) :messages ("Downloading Microsoft Python Language Server..." "Downloading Microsoft Python Language Server...done" "Downloading Microsoft Python Language Server..." "Downloading Microsoft Python Language Server...done" "Extracted Microsoft Python Language Server"))"##]],
    )
    .fresh_process()
}

#[test]
fn lsp_python_ms_practical_workflows_batch() {
    let cases = vec![
        registration_configuration_and_download_policy_are_real(),
        python_environment_precedence_uses_owned_executables(),
        initialization_uses_python_adapter_workspace_and_dot_env(),
        documentation_and_progress_handlers_drive_visible_boundaries(),
        public_installer_handles_failure_then_recovers(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "lsp_python_ms_practical_workflows_batch",
        "lsp_python_ms_parity",
        &cases,
    );
}
