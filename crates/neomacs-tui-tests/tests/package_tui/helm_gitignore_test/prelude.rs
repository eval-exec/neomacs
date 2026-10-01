use std::time::Duration;

pub(super) const STAGE_TIMEOUT: Duration = Duration::from_secs(45);

/// `helm-gitignore' is a thin interactive client, so its public seam is the
/// complete `M-x helm-gitignore' session.  The package, Helm, Request,
/// url-retrieve, JSON parser, callbacks, generated buffer, and file writing all
/// remain real.  Only the retired gitignore.io HTTP service is replayed by a
/// fail-closed loopback server inside each editor process.
///
/// The candidate objects and generated payloads were recorded verbatim from
/// the maintained official Toptal Gitignore API on 2026-08-10. On that date,
/// both legacy `www.gitignore.io' URLs returned `301 Moved Permanently' with a
/// `Location' under `https://www.toptal.com/developers/gitignore'. The fixture
/// preserves that status and path transition while rewriting only the authority
/// to its fail-closed loopback server, so Request's redirect path remains real.
/// Candidate JSON was recorded with `curl --silent --show-error --location' from
/// these exact public URLs:
/// `https://www.gitignore.io/dropdown/templates.json?term=visual',
/// `https://www.gitignore.io/dropdown/templates.json?term=linux',
/// `https://www.gitignore.io/dropdown/templates.json?term=python', and
/// `https://www.gitignore.io/dropdown/templates.json?term=neomacsnomatch'.
/// Generated bodies came from `https://www.gitignore.io/api/visualstudiocode'
/// (SHA-256
/// `2b00aab2b425e9282ac41a70b1972fa6d748834414ad0720f40d4968e6cf7d21') and
/// `https://www.gitignore.io/api/linux,archlinuxpackages' (SHA-256
/// `1c18134495a91386256373d06c8ed31d5685d987d7c1ff98170c58353f5a73cf').
pub(super) const HELM_GITIGNORE_TUI_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'helm-gitignore)

(defconst neomacs-helm-gitignore-tui-vscode
  "# Created by https://www.toptal.com/developers/gitignore/api/visualstudiocode
# Edit at https://www.toptal.com/developers/gitignore?templates=visualstudiocode

### VisualStudioCode ###
.vscode/*
!.vscode/settings.json
!.vscode/tasks.json
!.vscode/launch.json
!.vscode/extensions.json
!.vscode/*.code-snippets

# Local History for Visual Studio Code
.history/

# Built Visual Studio Code Extensions
*.vsix

### VisualStudioCode Patch ###
# Ignore all local history of files
.history
.ionide

# End of https://www.toptal.com/developers/gitignore/api/visualstudiocode
")

(defconst neomacs-helm-gitignore-tui-linux-archlinuxpackages
  "# Created by https://www.toptal.com/developers/gitignore/api/linux,archlinuxpackages
# Edit at https://www.toptal.com/developers/gitignore?templates=linux,archlinuxpackages

### ArchLinuxPackages ###
*.tar
*.tar.*
*.jar
*.exe
*.msi
*.zip
*.tgz
*.log
*.log.*
*.sig

pkg/
src/

### Linux ###
*~

# temporary files which can be created if a process still has a handle open of a deleted file
.fuse_hidden*

# KDE directory preferences
.directory

# Linux trash folder which might appear on any partition or disk
.Trash-*

# .nfs files are created when an open file is removed but is still being accessed
.nfs*

# End of https://www.toptal.com/developers/gitignore/api/linux,archlinuxpackages
")

(defvar neomacs-helm-gitignore-tui-server nil)
(defvar neomacs-helm-gitignore-tui-origin nil)
(defvar neomacs-helm-gitignore-tui-expected-host nil)
(defvar neomacs-helm-gitignore-tui-clients nil)
(defvar neomacs-helm-gitignore-tui-requests nil)
(defvar neomacs-helm-gitignore-tui-misses nil)
(defvar neomacs-helm-gitignore-tui-expected-plan nil)
(defvar neomacs-helm-gitignore-tui-held-responses nil)
(defvar neomacs-helm-gitignore-tui-cache-events nil)
(defvar neomacs-helm-gitignore-tui-request-urls nil)

(defun neomacs-helm-gitignore-tui-expect (&rest entries)
  (setq neomacs-helm-gitignore-tui-expected-plan
        (mapcar (lambda (entry)
                  (list "GET" (nth 0 entry) (nth 1 entry)))
                entries))
  nil)

(defun neomacs-helm-gitignore-tui-header (name header-lines)
  (let ((case-fold-search t)
        (prefix (concat (regexp-quote name) ":[ \t]*")))
    (catch 'value
      (dolist (line header-lines)
        (when (string-match (concat "^" prefix "\\(.*?\\)\r?$") line)
          (throw 'value (match-string 1 line))))
      nil)))

(defun neomacs-helm-gitignore-tui-write-state (name value)
  (with-temp-file (expand-file-name name (getenv "HOME"))
    (let ((print-length nil)
          (print-level nil))
      (prin1 value (current-buffer)))))

(defun neomacs-helm-gitignore-tui-observe-request (original url &rest arguments)
  (setq neomacs-helm-gitignore-tui-request-urls
        (append neomacs-helm-gitignore-tui-request-urls
                (list (replace-regexp-in-string
                       (regexp-quote neomacs-helm-gitignore-tui-origin)
                       "<origin>" url))))
  (neomacs-helm-gitignore-tui-write-state
   "request-urls.state" neomacs-helm-gitignore-tui-request-urls)
  (apply original url arguments))

(defun neomacs-helm-gitignore-tui-cache-watcher
    (_symbol new-value operation _where)
  (when (eq operation 'set)
    (setq neomacs-helm-gitignore-tui-cache-events
          (append neomacs-helm-gitignore-tui-cache-events
                  (list (copy-tree new-value))))
    (neomacs-helm-gitignore-tui-write-state
     "cache-events.state"
     (list :count (length neomacs-helm-gitignore-tui-cache-events)
           :latest new-value
           :events neomacs-helm-gitignore-tui-cache-events))))

(defun neomacs-helm-gitignore-tui-release-held ()
  (let ((held (pop neomacs-helm-gitignore-tui-held-responses)))
    (unless held
      (error "No held helm-gitignore fixture response"))
    (process-send-string (nth 0 held) (nth 1 held))
    (process-send-eof (nth 0 held)))
  nil)

(defun neomacs-helm-gitignore-tui-http-response
    (status reason content-type body)
  (concat (format "HTTP/1.1 %d %s\r\n" status reason)
          (format "Content-Type: %s\r\n" content-type)
          (format "Content-Length: %d\r\n" (string-bytes body))
          "Connection: close\r\n\r\n"
          body))

(defun neomacs-helm-gitignore-tui-redirect-response (path)
  (concat "HTTP/1.1 301 Moved Permanently\r\n"
          "Location: " neomacs-helm-gitignore-tui-origin
          "/developers/gitignore" path "\r\n"
          "Content-Length: 0\r\n"
          "Connection: close\r\n\r\n"))

(defun neomacs-helm-gitignore-tui-route (response-key method path)
  (cond
   ((not (equal method "GET"))
    (push (list method path) neomacs-helm-gitignore-tui-misses)
    (neomacs-helm-gitignore-tui-http-response
     405 "Method Not Allowed" "text/plain" "fixture method miss\n"))
   ((eq response-key 'legacy-redirect)
    (neomacs-helm-gitignore-tui-redirect-response path))
   ((eq response-key 'visual-list)
    (neomacs-helm-gitignore-tui-http-response
     200 "OK" "application/json; charset=utf-8"
     "[{\"text\":\"VisualBasic\",\"id\":\"visualbasic\"},{\"text\":\"VisualStudio\",\"id\":\"visualstudio\"},{\"text\":\"KonyVisualizer\",\"id\":\"konyvisualizer\"},{\"text\":\"VisualStudioCode\",\"id\":\"visualstudiocode\"},{\"text\":\"OpenFrameworks+VisualStudio\",\"id\":\"openframeworks+visualstudio\"}]"))
   ((eq response-key 'linux-list)
    (neomacs-helm-gitignore-tui-http-response
     200 "OK" "application/json; charset=utf-8"
     "[{\"id\":\"linux\",\"text\":\"Linux\"},{\"id\":\"archlinuxpackages\",\"text\":\"ArchLinuxPackages\"}]"))
   ((memq response-key '(python-list held-python-list))
    (neomacs-helm-gitignore-tui-http-response
     200 "OK" "application/json; charset=utf-8"
     "[{\"text\":\"Python\",\"id\":\"python\"},{\"text\":\"CircuitPython\",\"id\":\"circuitpython\"},{\"text\":\"PythonVanilla\",\"id\":\"pythonvanilla\"}]"))
   ((memq response-key '(empty-list held-empty-list))
    (neomacs-helm-gitignore-tui-http-response
     200 "OK" "application/json; charset=utf-8" "[]"))
   ((eq response-key 'vscode)
    (neomacs-helm-gitignore-tui-http-response
     200 "OK" "text/plain; charset=utf-8"
     neomacs-helm-gitignore-tui-vscode))
   ((eq response-key 'linux-archlinuxpackages)
    (neomacs-helm-gitignore-tui-http-response
     200 "OK" "text/plain; charset=utf-8"
     neomacs-helm-gitignore-tui-linux-archlinuxpackages))
   (t
    (push (list method path) neomacs-helm-gitignore-tui-misses)
    (neomacs-helm-gitignore-tui-http-response
     404 "Not Found" "text/plain" "fixture route miss\n"))))

(defun neomacs-helm-gitignore-tui-client-filter (client chunk)
  (let ((wire (concat (or (process-get client 'wire) "") chunk)))
    (process-put client 'wire wire)
    (when (string-match "\r?\n\r?\n" wire)
      (let* ((header-end (match-end 0))
             (header-lines
              (split-string (substring wire 0 (match-beginning 0))
                            "\r?\n" t))
             (request-line (car header-lines))
             (parts (split-string request-line " " t))
             (method (nth 0 parts))
             (path (nth 1 parts))
             (host (neomacs-helm-gitignore-tui-header "Host" header-lines))
             (normalized-headers
              (sort
               (mapcar
                (lambda (line)
                  (cond
                   ((string-match-p "\\`Host:" line)
                    "Host: 127.0.0.1:<port>")
                   ((string-match-p "\\`User-Agent:" line)
                    "User-Agent: <editor>")
                   (t line)))
                (cdr header-lines))
               #'string-lessp))
             (expected-headers
              '("Accept-encoding: gzip"
                "Accept: */*"
                "Connection: close"
                "Host: 127.0.0.1:<port>"
                "MIME-Version: 1.0"
                "User-Agent: <editor>"))
             (body-bytes (string-bytes (substring wire header-end)))
             (expected (car neomacs-helm-gitignore-tui-expected-plan))
             (response-key (nth 2 expected))
             (valid
              (and (equal (list method path) (seq-take expected 2))
                   (equal request-line (format "GET %s HTTP/1.1" path))
                   (equal host neomacs-helm-gitignore-tui-expected-host)
                   (equal normalized-headers expected-headers)
                   (= body-bytes 0))))
        (push (list :request-line request-line
                    :headers normalized-headers
                    :body-bytes body-bytes)
              neomacs-helm-gitignore-tui-requests)
        (if valid
            (setq neomacs-helm-gitignore-tui-expected-plan
                  (cdr neomacs-helm-gitignore-tui-expected-plan))
          (push (list :expected expected
                      :actual (list method path)
                      :request-line request-line
                      :actual-host host
                      :expected-host neomacs-helm-gitignore-tui-expected-host
                      :headers normalized-headers
                      :expected-headers expected-headers
                      :body-bytes body-bytes)
                neomacs-helm-gitignore-tui-misses))
        (if (and valid (eq response-key 'connection-close))
            (delete-process client)
          (let ((response
                 (if valid
                     (neomacs-helm-gitignore-tui-route response-key method path)
                   (neomacs-helm-gitignore-tui-http-response
                    409 "Fixture Plan Mismatch" "text/plain"
                    "fixture plan mismatch\n"))))
            (if (memq response-key '(held-python-list held-empty-list))
                (progn
                  (setq neomacs-helm-gitignore-tui-held-responses
                        (append neomacs-helm-gitignore-tui-held-responses
                                (list (list client response response-key))))
                  (neomacs-helm-gitignore-tui-write-state
                   "held-response.state"
                   (list :response response-key
                         :request-line request-line
                         :held-count
                         (length neomacs-helm-gitignore-tui-held-responses))))
              (process-send-string client response)
              (process-send-eof client))))))))

(defun neomacs-helm-gitignore-tui-server-log (_server client _message)
  (push client neomacs-helm-gitignore-tui-clients)
  (set-process-query-on-exit-flag client nil)
  (set-process-coding-system client 'binary 'binary))

(defun neomacs-helm-gitignore-tui-start-server ()
  (setq neomacs-helm-gitignore-tui-server
        (make-network-process
         :name "helm-gitignore-fixture"
         :server t :host "127.0.0.1" :service t :family 'ipv4
         :noquery t
         :filter #'neomacs-helm-gitignore-tui-client-filter
         :log #'neomacs-helm-gitignore-tui-server-log))
  (let ((origin (format "http://127.0.0.1:%d"
                        (process-contact neomacs-helm-gitignore-tui-server
                                         :service))))
    (setq neomacs-helm-gitignore-tui-origin origin
          neomacs-helm-gitignore-tui-expected-host
          (substring origin (length "http://"))
          helm-gitignore--list-url
          (concat origin "/dropdown/templates.json?term=%s")
          helm-gitignore--api-url (concat origin "/api/%s"))))

(defun neomacs-helm-gitignore-tui-stop-server ()
  (interactive)
  (when (process-live-p neomacs-helm-gitignore-tui-server)
    (delete-process neomacs-helm-gitignore-tui-server))
  (neomacs-helm-gitignore-tui-write-state
   "server-stopped.state"
   (list :server-live (process-live-p neomacs-helm-gitignore-tui-server)))
  nil)

(defun neomacs-helm-gitignore-tui-live-response-buffers ()
  (sort
   (delq nil
         (mapcar
          (lambda (buffer)
            (let ((name (buffer-name buffer))
                  (process (get-buffer-process buffer)))
              (and (string-match-p "127\\.0\\.0\\.1" name)
                   process
                   (process-live-p process)
                   name)))
          (buffer-list)))
   #'string-lessp))

(defun neomacs-helm-gitignore-tui-await-http-idle ()
  (let ((deadline (+ (float-time) 8.0)))
    (while (and (< (float-time) deadline)
                (or (seq-some #'process-live-p
                              neomacs-helm-gitignore-tui-clients)
                    (neomacs-helm-gitignore-tui-live-response-buffers)))
      (accept-process-output nil 0.01))))

(defun neomacs-helm-gitignore-tui-capture (stage)
  (let ((buffer
         (or (get-buffer "*gitignore*")
             (and buffer-file-name
                  (equal (file-name-nondirectory buffer-file-name) ".gitignore")
                  (current-buffer)))))
    (with-temp-file (expand-file-name (concat stage ".state") (getenv "HOME"))
      (let ((print-length nil)
            (print-level nil))
        (prin1
         (list
          :buffer
          (and buffer
               (with-current-buffer buffer
                 (list :text (buffer-substring-no-properties
                              (point-min) (point-max))
                       :point (point)
                       :mode major-mode
                       :modified (buffer-modified-p)
                       :file (and buffer-file-name
                                  (file-name-nondirectory buffer-file-name))
                       :selected (eq buffer
                                     (window-buffer (selected-window))))))
          :requests (nreverse (copy-tree neomacs-helm-gitignore-tui-requests))
          :request-urls (copy-tree neomacs-helm-gitignore-tui-request-urls)
          :remaining-plan (copy-tree neomacs-helm-gitignore-tui-expected-plan)
          :misses (nreverse (copy-tree neomacs-helm-gitignore-tui-misses))
          :live-clients
          (mapcar #'process-name
                  (seq-filter #'process-live-p
                              neomacs-helm-gitignore-tui-clients))
          :response-buffers
          (neomacs-helm-gitignore-tui-live-response-buffers))
         (current-buffer))))))

(defun neomacs-helm-gitignore-tui-reset ()
  (dolist (name '("*helm-gitignore*" "*gitignore*" "*Warnings*"))
    (when (get-buffer name)
      (kill-buffer name)))
  (dolist (client neomacs-helm-gitignore-tui-clients)
    (when (process-live-p client)
      (delete-process client)))
  (remove-variable-watcher 'helm-gitignore--cache
                           #'neomacs-helm-gitignore-tui-cache-watcher)
  (setq helm-gitignore--cache nil
        neomacs-helm-gitignore-tui-cache-events nil
        neomacs-helm-gitignore-tui-request-urls nil
        neomacs-helm-gitignore-tui-requests nil
        neomacs-helm-gitignore-tui-misses nil
        neomacs-helm-gitignore-tui-clients nil
        neomacs-helm-gitignore-tui-expected-plan nil
        neomacs-helm-gitignore-tui-held-responses nil)
  (add-variable-watcher 'helm-gitignore--cache
                        #'neomacs-helm-gitignore-tui-cache-watcher)
  (dolist (name '("cache-events.state" "held-response.state"
                  "request-urls.state" "server-stopped.state"))
    (let ((path (expand-file-name name (getenv "HOME"))))
      (when (file-exists-p path)
        (delete-file path))))
  nil)

(defun neomacs-helm-gitignore-tui-seed-unsaved-buffer ()
  (with-current-buffer (get-buffer-create "*gitignore*")
    (erase-buffer)
    (insert "# Unsaved incident-specific exclusions\nsecret-release-token.txt\n")
    (gitignore-mode)
    (goto-char 3)
    (set-buffer-modified-p t)))

(defun neomacs-helm-gitignore-tui-setup ()
  (setq request-backend 'url-retrieve
        request-log-level -1
        request-message-level -1
        url-proxy-services nil
        url-http-attempt-keepalives nil
        url-cookie-file nil
        url-cookie-save-interval nil
        helm-input-idle-delay 0.05
        helm-candidate-number-limit 20)
  (add-variable-watcher 'helm-gitignore--cache
                        #'neomacs-helm-gitignore-tui-cache-watcher)
  (advice-add 'request :around
              #'neomacs-helm-gitignore-tui-observe-request)
  (define-key helm-map (kbd "C-c C-z")
              #'neomacs-helm-gitignore-tui-stop-server)
  (neomacs-helm-gitignore-tui-start-server)
  (with-current-buffer (get-buffer-create "release-notes.txt")
    (setq default-directory (file-name-as-directory (getenv "HOME")))
    (erase-buffer)
    (insert "Release engineering scratchpad\n")
    (set-buffer-modified-p nil)
    (switch-to-buffer (current-buffer))))

(add-hook 'emacs-startup-hook #'neomacs-helm-gitignore-tui-setup 100)
"####;
