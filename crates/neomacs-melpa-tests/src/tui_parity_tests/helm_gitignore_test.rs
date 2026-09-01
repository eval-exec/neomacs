use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use expect_test::expect;
use neomacs_tui_tests::TuiSession;

use crate::{CachedMelpaOracle, HELM_GITIGNORE_MELPA_PIN};

use super::support::PackageTuiPair;

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
const HELM_GITIGNORE_TUI_PRELUDE: &str = r####"
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

fn send_to_both<F>(pair: &mut PackageTuiPair, operation: F)
where
    F: Fn(&mut TuiSession),
{
    operation(&mut pair.gnu);
    operation(&mut pair.neo);
}

fn wait_for_editor<F>(session: &mut TuiSession, timeout: Duration, predicate: F) -> bool
where
    F: Fn(&[String]) -> bool,
{
    session.read_until(timeout, |grid| predicate(grid));
    predicate(&session.text_grid())
}

fn wait_for_progress<F>(
    pair: &mut PackageTuiPair,
    stage: &str,
    timeout: Duration,
    predicate: F,
    divergences: &mut Vec<String>,
) -> bool
where
    F: Fn(&[String]) -> bool + Copy,
{
    assert!(
        wait_for_editor(&mut pair.gnu, timeout, predicate),
        "GNU {stage} screen did not reach expected state:\n{}",
        pair.gnu.text_grid().join("\n")
    );
    let neo_reached = wait_for_editor(&mut pair.neo, timeout, predicate);
    if !neo_reached {
        divergences.push(format!(
            "Neomacs {stage} screen did not reach the GNU state:\n{}",
            pair.neo.text_grid().join("\n")
        ));
    }
    neo_reached
}

fn open_helm_gitignore(pair: &mut PackageTuiPair, divergences: &mut Vec<String>) -> bool {
    send_to_both(pair, |session| {
        session.send_key("M-x");
        session.send(b"helm-gitignore");
        session.send_key("RET");
    });
    wait_for_progress(
        pair,
        "Helm startup",
        Duration::from_secs(12),
        |grid| {
            grid.iter().any(|row| row.contains("*helm-gitignore*"))
                && grid.iter().any(|row| row.contains("pattern:"))
        },
        divergences,
    )
}

fn type_query_and_wait(
    pair: &mut PackageTuiPair,
    query: &str,
    candidate: &str,
    divergences: &mut Vec<String>,
) -> bool {
    send_to_both(pair, |session| session.send(query.as_bytes()));
    wait_for_progress(
        pair,
        &format!("query {query:?}"),
        Duration::from_secs(15),
        |grid| {
            grid.iter()
                .any(|row| row.contains("pattern:") && row.contains(query))
                && grid.iter().any(|row| row.trim().contains(candidate))
        },
        divergences,
    )
}

fn helm_semantic_snapshot(session: &TuiSession, pattern: &str, candidates: &[&str]) -> String {
    let grid = session.text_grid();
    let pattern_line = grid
        .iter()
        .find(|row| row.trim_start().starts_with("pattern:"))
        .map(|row| row.trim())
        .expect("Helm terminal snapshot has a pattern line");
    assert_eq!(pattern_line, format!("pattern: {pattern}"));
    let source = if grid.iter().any(|row| row.trim() == "gitignore.io") {
        r#""gitignore.io""#
    } else {
        "nil"
    };
    let status = grid
        .iter()
        .find(|row| row.contains("*helm-gitignore*"))
        .expect("Helm terminal snapshot has a status line");
    let selected = status
        .split_whitespace()
        .find_map(|word| word.strip_prefix('L')?.parse::<usize>().ok())
        .expect("Helm terminal snapshot reports the selected candidate index");
    let marked = status
        .split_whitespace()
        .find_map(|word| word.strip_prefix('M')?.parse::<usize>().ok())
        .unwrap_or(0);
    let candidate_count = status
        .split_whitespace()
        .find_map(|word| word.strip_prefix('[')?.parse::<usize>().ok())
        .unwrap_or(0);
    let visible = grid
        .iter()
        .filter_map(|row| {
            let trimmed = row.trim();
            let (is_marked, label) = match trimmed.strip_prefix('*') {
                Some(label) => (true, label),
                None => (false, trimmed),
            };
            candidates
                .contains(&label)
                .then(|| format!("({} {label:?})", if is_marked { "t" } else { "nil" }))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        visible.len(),
        candidates.len(),
        "Helm terminal snapshot has every expected candidate exactly once"
    );
    assert_eq!(
        candidate_count,
        candidates.len(),
        "Helm terminal snapshot rejects unexpected extra candidates"
    );
    format!(
        "(:pattern {pattern:?} :source {source} :candidates ({}) :candidate-count {candidate_count} :selected-index {selected} :marked-count {marked})",
        visible.join(" ")
    )
}

fn assert_helm_semantic_snapshot(
    pair: &PackageTuiPair,
    stage: &str,
    pattern: &str,
    candidates: &[&str],
    expected: expect_test::Expect,
    divergences: &mut Vec<String>,
) {
    let gnu = helm_semantic_snapshot(&pair.gnu, pattern, candidates);
    expected.assert_eq(&gnu);
    let neo = helm_semantic_snapshot(&pair.neo, pattern, candidates);
    if neo != gnu {
        divergences.push(format!(
            "{stage} terminal snapshot differs:\nGNU:\n{gnu}\nNeomacs:\n{neo}"
        ));
    }
}

fn terminal_failure_snapshot(session: &TuiSession) -> String {
    session
        .text_grid()
        .into_iter()
        .filter_map(|row| {
            let row = row.trim();
            (row.contains("Keyword argument")
                || row.contains("Wrong number of arguments")
                || row.contains("wrong-number-of-arguments"))
            .then(|| row.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_terminal_failure_parity(
    pair: &PackageTuiPair,
    stage: &str,
    expected: expect_test::Expect,
    divergences: &mut Vec<String>,
) {
    let gnu = terminal_failure_snapshot(&pair.gnu);
    expected.assert_eq(&gnu);
    let neo = terminal_failure_snapshot(&pair.neo);
    if neo != gnu {
        divergences.push(format!(
            "{stage} terminal failure differs:\nGNU:\n{gnu}\nNeomacs:\n{neo}"
        ));
    }
}

fn navigate_to_visual_studio_code(
    pair: &mut PackageTuiPair,
    stage: &str,
    divergences: &mut Vec<String>,
) -> bool {
    open_helm_gitignore(pair, divergences);
    type_query_and_wait(pair, "visual", "VisualStudioCode", divergences);
    assert_helm_semantic_snapshot(
        pair,
        &format!("{stage} candidates"),
        "visual",
        &[
            "VisualBasic",
            "VisualStudio",
            "KonyVisualizer",
            "VisualStudioCode",
            "OpenFrameworks+VisualStudio",
        ],
        expect![[
            r#"(:pattern "visual" :source "gitignore.io" :candidates ((nil "VisualBasic") (nil "VisualStudio") (nil "KonyVisualizer") (nil "VisualStudioCode") (nil "OpenFrameworks+VisualStudio")) :candidate-count 5 :selected-index 1 :marked-count 0)"#
        ]],
        divergences,
    );
    send_to_both(pair, |session| session.send_keys("C-n C-n C-n"));
    let selection_reached = wait_for_progress(
        pair,
        &format!("{stage} VisualStudioCode selection"),
        Duration::from_secs(5),
        |grid| {
            grid.iter().any(|row| {
                row.contains("*helm-gitignore*")
                    && row.contains(" L4 ")
                    && row.contains("[5 Candidate(s)]")
            })
        },
        divergences,
    );
    if selection_reached {
        assert_helm_semantic_snapshot(
            pair,
            &format!("{stage} selected candidate"),
            "visual",
            &[
                "VisualBasic",
                "VisualStudio",
                "KonyVisualizer",
                "VisualStudioCode",
                "OpenFrameworks+VisualStudio",
            ],
            expect![[
                r#"(:pattern "visual" :source "gitignore.io" :candidates ((nil "VisualBasic") (nil "VisualStudio") (nil "KonyVisualizer") (nil "VisualStudioCode") (nil "OpenFrameworks+VisualStudio")) :candidate-count 5 :selected-index 4 :marked-count 0)"#
            ]],
            divergences,
        );
    }
    selection_reached
}

fn eval_both(pair: &mut PackageTuiPair, form: &str) {
    send_to_both(pair, |session| {
        session.send_key("M-:");
        session.send(form.as_bytes());
        session.send_key("RET");
    });
}

fn capture_editor(session: &mut TuiSession, stage: &str, editor: &str) -> String {
    session.send_key("M-:");
    session.send(
        format!(
            r#"(progn (neomacs-helm-gitignore-tui-await-http-idle) (neomacs-helm-gitignore-tui-capture {stage:?}))"#
        )
        .as_bytes(),
    );
    session.send_key("RET");
    let path = session.home_dir().join(format!("{stage}.state"));
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.read(Duration::from_millis(10));
        if let Ok(state) = fs::read_to_string(&path) {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "{editor} did not capture helm-gitignore stage {stage:?}"
        );
        thread::yield_now();
    }
}

fn wait_for_state_file(
    session: &mut TuiSession,
    name: &str,
    expected_fragment: &str,
    editor: &str,
) -> String {
    let path = session.home_dir().join(name);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        session.read(Duration::from_millis(10));
        if let Ok(state) = fs::read_to_string(&path)
            && state.contains(expected_fragment)
        {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "{editor} did not write {name:?} containing {expected_fragment:?}"
        );
        thread::yield_now();
    }
}

fn capture_reached_stage<F>(
    pair: &mut PackageTuiPair,
    stage: &str,
    timeout: Duration,
    predicate: F,
    divergences: &mut Vec<String>,
) -> (String, Option<String>)
where
    F: Fn(&[String]) -> bool + Copy,
{
    assert!(
        wait_for_editor(&mut pair.gnu, timeout, predicate),
        "GNU {stage} screen did not reach expected state:\n{}",
        pair.gnu.text_grid().join("\n")
    );
    let gnu = capture_editor(&mut pair.gnu, stage, "GNU");

    if wait_for_editor(&mut pair.neo, timeout, predicate) {
        let neo = capture_editor(&mut pair.neo, stage, "Neomacs");
        (gnu, Some(neo))
    } else {
        divergences.push(format!(
            "Neomacs {stage} screen did not reach the GNU state:\n{}",
            pair.neo.text_grid().join("\n")
        ));
        (gnu, None)
    }
}

fn assert_gnu_literal_and_parity(
    stage: &str,
    gnu: &str,
    neo: Option<&str>,
    expected: expect_test::Expect,
    divergences: &mut Vec<String>,
) {
    expected.assert_eq(gnu);
    if let Some(neo) = neo.filter(|neo| *neo != gnu) {
        divergences.push(format!("{stage} differs:\nGNU:\n{gnu}\nNeomacs:\n{neo}"));
    }
}

fn spawn_helm_gitignore_pair(label: &str) -> PackageTuiPair {
    let oracle = CachedMelpaOracle::new(HELM_GITIGNORE_MELPA_PIN, "helm-gitignore.el")
        .expect("prepare exact revision-pinned helm-gitignore source")
        .with_prelude(HELM_GITIGNORE_TUI_PRELUDE);
    PackageTuiPair::spawn(label, oracle.prepared_packages())
        .expect("spawn helm-gitignore GNU/Neomacs TUI pair")
}

fn ready_helm_gitignore_pair(label: &str, divergences: &mut Vec<String>) -> PackageTuiPair {
    let mut pair = spawn_helm_gitignore_pair(label);
    wait_for_progress(
        &mut pair,
        "fixture startup",
        Duration::from_secs(20),
        |grid| {
            grid.iter()
                .any(|row| row.contains("Release engineering scratchpad"))
        },
        divergences,
    );
    pair
}

fn assert_no_divergences(divergences: &[String]) {
    assert!(
        divergences.is_empty(),
        "helm-gitignore GNU/Neomacs divergences:\n{}",
        divergences.join("\n\n")
    );
}

#[test]
fn helm_gitignore_public_workflows_match_gnu() {
    let mut divergences = Vec::new();
    let mut pair = ready_helm_gitignore_pair("helm-gitignore-workflows", &mut divergences);

    // A blank pattern is not a service request: the source requires one input
    // character before it contacts the dropdown API.
    open_helm_gitignore(&mut pair, &mut divergences);
    send_to_both(&mut pair, |session| session.send_key("C-g"));
    let gnu = capture_editor(&mut pair.gnu, "blank-pattern", "GNU");
    let neo = capture_editor(&mut pair.neo, "blank-pattern", "Neomacs");
    assert_gnu_literal_and_parity(
        "blank pattern",
        &gnu,
        Some(&neo),
        expect![
            "(:buffer nil :requests nil :request-urls nil :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"
        ],
        &mut divergences,
    );
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-reset)");
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=visual" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=visual" visual-list) '("/api/visualstudiocode" legacy-redirect) '("/developers/gitignore/api/visualstudiocode" vscode))"#,
    );

    // Select the human-facing “Visual Studio Code” row.  The server transcript
    // proves that Helm handed the distinct `visualstudiocode' ID to Request.
    navigate_to_visual_studio_code(&mut pair, "single selection", &mut divergences);
    send_to_both(&mut pair, |session| session.send_key("RET"));
    let (gnu, neo) = capture_reached_stage(
        &mut pair,
        "single-selection",
        Duration::from_secs(15),
        |grid| {
            grid.iter().any(|row| {
                row.contains("# Created by https://www.toptal.com/developers/gitignore/api/visual")
            }) && grid.iter().any(|row| row.contains("*gitignore*"))
        },
        &mut divergences,
    );
    assert_gnu_literal_and_parity(
        "single selection",
        &gnu,
        neo.as_deref(),
        expect![[r##"
            (:buffer (:text "# Created by https://www.toptal.com/developers/gitignore/api/visualstudiocode
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
            " :point 1 :mode gitignore-mode :modified t :file nil :selected t) :requests ((:request-line "GET /dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /api/visualstudiocode HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/api/visualstudiocode HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=visual" "<origin>/api/visualstudiocode") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"##]],
        &mut divergences,
    );
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-reset)");

    // Ordered multi-selection replaces the package's existing generated
    // buffer, matching the public workflow used to regenerate a project file.
    eval_both(
        &mut pair,
        "(neomacs-helm-gitignore-tui-seed-unsaved-buffer)",
    );
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=linux" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=linux" linux-list) '("/api/linux,archlinuxpackages" legacy-redirect) '("/developers/gitignore/api/linux,archlinuxpackages" linux-archlinuxpackages))"#,
    );
    open_helm_gitignore(&mut pair, &mut divergences);
    type_query_and_wait(&mut pair, "linux", "ArchLinuxPackages", &mut divergences);
    assert_helm_semantic_snapshot(
        &pair,
        "multi-selection candidates",
        "linux",
        &["Linux", "ArchLinuxPackages"],
        expect![[
            r#"(:pattern "linux" :source "gitignore.io" :candidates ((nil "Linux") (nil "ArchLinuxPackages")) :candidate-count 2 :selected-index 1 :marked-count 0)"#
        ]],
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_keys("C-SPC C-SPC"));
    wait_for_progress(
        &mut pair,
        "ordered multi-selection marks",
        Duration::from_secs(5),
        |grid| {
            grid.iter().any(|row| {
                row.contains("*helm-gitignore*") && row.contains(" L2 ") && row.contains(" M2 ")
            })
        },
        &mut divergences,
    );
    assert_helm_semantic_snapshot(
        &pair,
        "multi-selection marks",
        "linux",
        &["Linux", "ArchLinuxPackages"],
        expect![[
            r#"(:pattern "linux" :source "gitignore.io" :candidates ((t "Linux") (t "ArchLinuxPackages")) :candidate-count 2 :selected-index 2 :marked-count 2)"#
        ]],
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_key("RET"));
    let (gnu, neo) = capture_reached_stage(
        &mut pair,
        "ordered-multi-selection",
        Duration::from_secs(15),
        |grid| {
            grid.iter().any(|row| {
                row.contains("# Created by https://www.toptal.com/developers/gitignore/api/linux")
            }) && grid.iter().any(|row| row.contains("*gitignore*"))
        },
        &mut divergences,
    );
    assert_gnu_literal_and_parity(
        "ordered multi-selection",
        &gnu,
        neo.as_deref(),
        expect![[r##"
            (:buffer (:text "# Created by https://www.toptal.com/developers/gitignore/api/linux,archlinuxpackages
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
            " :point 1 :mode gitignore-mode :modified t :file nil :selected t) :requests ((:request-line "GET /dropdown/templates.json?term=linux HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=linux HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /api/linux,archlinuxpackages HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/api/linux,archlinuxpackages HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=linux" "<origin>/api/linux,archlinuxpackages") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"##]],
        &mut divergences,
    );
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-reset)");

    // Refine a live session from one real service result to another. The
    // second snapshot's exact total and rows prove that Helm dropped every
    // Python candidate instead of appending the Visual results to stale UI.
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=python" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=python" python-list) '("/dropdown/templates.json?term=visual" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=visual" visual-list))"#,
    );
    open_helm_gitignore(&mut pair, &mut divergences);
    type_query_and_wait(&mut pair, "python", "PythonVanilla", &mut divergences);
    assert_helm_semantic_snapshot(
        &pair,
        "live refinement Python candidates",
        "python",
        &["Python", "CircuitPython", "PythonVanilla"],
        expect![[
            r#"(:pattern "python" :source "gitignore.io" :candidates ((nil "Python") (nil "CircuitPython") (nil "PythonVanilla")) :candidate-count 3 :selected-index 1 :marked-count 0)"#
        ]],
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_keys("C-a C-k"));
    type_query_and_wait(
        &mut pair,
        "visual",
        "OpenFrameworks+VisualStudio",
        &mut divergences,
    );
    assert_helm_semantic_snapshot(
        &pair,
        "live refinement Visual candidates",
        "visual",
        &[
            "VisualBasic",
            "VisualStudio",
            "KonyVisualizer",
            "VisualStudioCode",
            "OpenFrameworks+VisualStudio",
        ],
        expect![[
            r#"(:pattern "visual" :source "gitignore.io" :candidates ((nil "VisualBasic") (nil "VisualStudio") (nil "KonyVisualizer") (nil "VisualStudioCode") (nil "OpenFrameworks+VisualStudio")) :candidate-count 5 :selected-index 1 :marked-count 0)"#
        ]],
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_key("C-g"));
    let gnu = capture_editor(&mut pair.gnu, "live-refinement", "GNU");
    let neo = capture_editor(&mut pair.neo, "live-refinement", "Neomacs");
    assert_gnu_literal_and_parity(
        "live query refinement",
        &gnu,
        Some(&neo),
        expect![[
            r#"(:buffer nil :requests ((:request-line "GET /dropdown/templates.json?term=python HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=python HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=python" "<origin>/dropdown/templates.json?term=visual") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"#
        ]],
        &mut divergences,
    );
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-reset)");

    // A response which arrives after abort populates the package-global cache
    // and is observable through a second public invocation.
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=python" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=python" held-python-list))"#,
    );
    open_helm_gitignore(&mut pair, &mut divergences);
    send_to_both(&mut pair, |session| session.send(b"python"));

    let gnu_held =
        wait_for_state_file(&mut pair.gnu, "held-response.state", ":held-count 1", "GNU");
    expect![[r#"(:response held-python-list :request-line "GET /developers/gitignore/dropdown/templates.json?term=python HTTP/1.1" :held-count 1)"#]]
        .assert_eq(&gnu_held);
    let neo_held = wait_for_state_file(
        &mut pair.neo,
        "held-response.state",
        ":held-count 1",
        "Neomacs",
    );
    if neo_held != gnu_held {
        divergences.push(format!(
            "held Python response differs:\nGNU:\n{gnu_held}\nNeomacs:\n{neo_held}"
        ));
    }
    assert_helm_semantic_snapshot(
        &pair,
        "held Python query",
        "python",
        &[],
        expect![[
            r#"(:pattern "python" :source nil :candidates () :candidate-count 0 :selected-index 1 :marked-count 0)"#
        ]],
        &mut divergences,
    );

    // Abort while Request still owns the response.  The pinned source's
    // success callback writes its package-global cache even though Helm is no
    // longer alive; only the subsequent `helm-update' is guarded.
    send_to_both(&mut pair, |session| session.send_key("C-g"));
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-release-held)");
    let gnu_late_cache =
        wait_for_state_file(&mut pair.gnu, "cache-events.state", ":count 1", "GNU");
    expect![[r#"(:count 1 :latest (("Python" . "python") ("CircuitPython" . "circuitpython") ("PythonVanilla" . "pythonvanilla")) :events ((("Python" . "python") ("CircuitPython" . "circuitpython") ("PythonVanilla" . "pythonvanilla"))))"#]]
        .assert_eq(&gnu_late_cache);
    let neo_late_cache =
        wait_for_state_file(&mut pair.neo, "cache-events.state", ":count 1", "Neomacs");
    if neo_late_cache != gnu_late_cache {
        divergences.push(format!(
            "late Python cache differs:\nGNU:\n{gnu_late_cache}\nNeomacs:\n{neo_late_cache}"
        ));
    }

    // A second public invocation consumes the late global cache.  Its new
    // `circuit' pattern deliberately has no route-plan entry: any fresh HTTP
    // query is therefore a strict fixture miss.
    open_helm_gitignore(&mut pair, &mut divergences);
    type_query_and_wait(&mut pair, "circuit", "CircuitPython", &mut divergences);
    assert_helm_semantic_snapshot(
        &pair,
        "stale cache in second session",
        "circuit",
        &["CircuitPython"],
        expect![[
            r#"(:pattern "circuit" :source "gitignore.io" :candidates ((nil "CircuitPython")) :candidate-count 1 :selected-index 1 :marked-count 0)"#
        ]],
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_key("C-g"));

    let gnu_consumed_cache =
        wait_for_state_file(&mut pair.gnu, "cache-events.state", ":count 2", "GNU");
    expect![[r#"(:count 2 :latest nil :events ((("Python" . "python") ("CircuitPython" . "circuitpython") ("PythonVanilla" . "pythonvanilla")) nil))"#]]
        .assert_eq(&gnu_consumed_cache);
    let neo_consumed_cache =
        wait_for_state_file(&mut pair.neo, "cache-events.state", ":count 2", "Neomacs");
    if neo_consumed_cache != gnu_consumed_cache {
        divergences.push(format!(
            "consumed late cache differs:\nGNU:\n{gnu_consumed_cache}\nNeomacs:\n{neo_consumed_cache}"
        ));
    }

    let gnu = capture_editor(&mut pair.gnu, "late-global-cache", "GNU");
    let neo = capture_editor(&mut pair.neo, "late-global-cache", "Neomacs");
    assert_gnu_literal_and_parity(
        "late global cache",
        &gnu,
        Some(&neo),
        expect![[
            r#"(:buffer nil :requests ((:request-line "GET /dropdown/templates.json?term=python HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=python HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=python") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"#
        ]],
        &mut divergences,
    );
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-reset)");

    // Exact empty JSON exercises nil's dual role as result and cache-miss
    // sentinel without allowing the real callback to create a request herd.
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=neomacsnomatch" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=neomacsnomatch" empty-list) '("/dropdown/templates.json?term=neomacsnomatch" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=neomacsnomatch" held-empty-list))"#,
    );
    open_helm_gitignore(&mut pair, &mut divergences);
    send_to_both(&mut pair, |session| session.send(b"neomacsnomatch"));

    // The official dropdown's exact empty JSON decodes to nil.  Since nil is
    // also this package's cache-miss sentinel, the callback's `helm-update'
    // performs one identical retry.  Holding that retry makes the count exact
    // and prevents a test-induced request herd.
    let gnu_held =
        wait_for_state_file(&mut pair.gnu, "held-response.state", ":held-count 1", "GNU");
    expect![[r#"(:response held-empty-list :request-line "GET /developers/gitignore/dropdown/templates.json?term=neomacsnomatch HTTP/1.1" :held-count 1)"#]]
        .assert_eq(&gnu_held);
    let neo_held = wait_for_state_file(
        &mut pair.neo,
        "held-response.state",
        ":held-count 1",
        "Neomacs",
    );
    if neo_held != gnu_held {
        divergences.push(format!(
            "held empty response differs:\nGNU:\n{gnu_held}\nNeomacs:\n{neo_held}"
        ));
    }
    assert_helm_semantic_snapshot(
        &pair,
        "empty-result retry",
        "neomacsnomatch",
        &[],
        expect![[
            r#"(:pattern "neomacsnomatch" :source nil :candidates () :candidate-count 0 :selected-index 1 :marked-count 0)"#
        ]],
        &mut divergences,
    );

    send_to_both(&mut pair, |session| session.send_key("C-g"));
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-release-held)");
    let gnu_cache_events =
        wait_for_state_file(&mut pair.gnu, "cache-events.state", ":count 2", "GNU");
    expect![[r#"(:count 2 :latest nil :events (nil nil))"#]].assert_eq(&gnu_cache_events);
    let neo_cache_events =
        wait_for_state_file(&mut pair.neo, "cache-events.state", ":count 2", "Neomacs");
    if neo_cache_events != gnu_cache_events {
        divergences.push(format!(
            "empty-result cache events differ:\nGNU:\n{gnu_cache_events}\nNeomacs:\n{neo_cache_events}"
        ));
    }

    let gnu = capture_editor(&mut pair.gnu, "empty-result", "GNU");
    let neo = capture_editor(&mut pair.neo, "empty-result", "Neomacs");
    assert_gnu_literal_and_parity(
        "empty result",
        &gnu,
        Some(&neo),
        expect![[
            r#"(:buffer nil :requests ((:request-line "GET /dropdown/templates.json?term=neomacsnomatch HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=neomacsnomatch HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /dropdown/templates.json?term=neomacsnomatch HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=neomacsnomatch HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=neomacsnomatch" "<origin>/dropdown/templates.json?term=neomacsnomatch") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"#
        ]],
        &mut divergences,
    );
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-reset)");

    // The generated buffer remains a normal editable gitignore-mode buffer and
    // can be saved through the real interactive write-file command.
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=visual" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=visual" visual-list) '("/api/visualstudiocode" legacy-redirect) '("/developers/gitignore/api/visualstudiocode" vscode))"#,
    );
    navigate_to_visual_studio_code(&mut pair, "save", &mut divergences);
    send_to_both(&mut pair, |session| session.send_key("RET"));
    let neo_generated = wait_for_progress(
        &mut pair,
        "generated buffer before save",
        Duration::from_secs(15),
        |grid| {
            grid.iter().any(|row| {
                row.contains("# Created by https://www.toptal.com/developers/gitignore/api/visual")
            }) && grid.iter().any(|row| row.contains("*gitignore*"))
        },
        &mut divergences,
    );
    let gnu_save_path = pair.gnu.home_dir().join(".gitignore");
    let neo_save_path = pair.neo.home_dir().join(".gitignore");
    pair.gnu.send_keys("M->");
    pair.gnu
        .send(b"\n# Project-local release artifacts\nrelease-output/\n");
    pair.gnu.send_keys("C-x C-w");
    pair.gnu.send(gnu_save_path.as_os_str().as_encoded_bytes());
    pair.gnu.send_key("RET");
    if neo_generated {
        pair.neo.send_keys("M->");
        pair.neo
            .send(b"\n# Project-local release artifacts\nrelease-output/\n");
        pair.neo.send_keys("C-x C-w");
        pair.neo.send(neo_save_path.as_os_str().as_encoded_bytes());
        pair.neo.send_key("RET");
    }
    let saved_relative = ".gitignore";
    let deadline = Instant::now() + Duration::from_secs(12);
    let gnu_file = loop {
        pair.gnu.read(Duration::from_millis(10));
        if let Ok(text) = fs::read_to_string(pair.gnu.home_dir().join(saved_relative))
            && text.ends_with("release-output/\n")
        {
            break text;
        }
        assert!(
            Instant::now() < deadline,
            "GNU did not save the edited .gitignore"
        );
        thread::yield_now();
    };
    let neo_file = if neo_generated {
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            pair.neo.read(Duration::from_millis(10));
            if let Ok(text) = fs::read_to_string(pair.neo.home_dir().join(saved_relative))
                && text.ends_with("release-output/\n")
            {
                break Some(text);
            }
            if Instant::now() >= deadline {
                divergences.push("Neomacs did not save the edited .gitignore".to_string());
                break None;
            }
            thread::yield_now();
        }
    } else {
        None
    };
    assert_gnu_literal_and_parity(
        "edited and saved file",
        &gnu_file,
        neo_file.as_deref(),
        expect![[
            r####"# Created by https://www.toptal.com/developers/gitignore/api/visualstudiocode
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

# Project-local release artifacts
release-output/
"####
        ]],
        &mut divergences,
    );
    let gnu = capture_editor(&mut pair.gnu, "edited-and-saved", "GNU");
    let neo = neo_generated.then(|| capture_editor(&mut pair.neo, "edited-and-saved", "Neomacs"));
    assert_gnu_literal_and_parity(
        "edited and saved buffer",
        &gnu,
        neo.as_deref(),
        expect![[r##"
            (:buffer (:text "# Created by https://www.toptal.com/developers/gitignore/api/visualstudiocode
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

            # Project-local release artifacts
            release-output/
            " :point 617 :mode gitignore-mode :modified nil :file ".gitignore" :selected t) :requests ((:request-line "GET /dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /api/visualstudiocode HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/api/visualstudiocode HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=visual" "<origin>/api/visualstudiocode") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"##]],
        &mut divergences,
    );
    drop(pair);

    // Failure callbacks can leave asynchronous state behind, so failures use
    // scoped fresh editor pairs while still sharing this package-level test.
    let mut pair = ready_helm_gitignore_pair("helm-gitignore-list-failure", &mut divergences);
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=visual" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=visual" connection-close))"#,
    );
    open_helm_gitignore(&mut pair, &mut divergences);
    send_to_both(&mut pair, |session| session.send(b"visual"));
    let failure = |grid: &[String]| {
        grid.iter().any(|row| {
            row.contains("Keyword argument")
                || row.contains("Wrong number of arguments")
                || row.contains("wrong-number-of-arguments")
        })
    };
    wait_for_progress(
        &mut pair,
        "list HTTP failure",
        Duration::from_secs(15),
        failure,
        &mut divergences,
    );
    assert_terminal_failure_parity(
        &pair,
        "list connection-close failure",
        expect![
            "pattern: visual [error in process sentinel: Keyword argument :data not one of (:error-thrown :&allow-other-keys&rest :)]"
        ],
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_key("C-g"));
    let gnu = capture_editor(&mut pair.gnu, "list-failure", "GNU");
    let neo = capture_editor(&mut pair.neo, "list-failure", "Neomacs");
    assert_gnu_literal_and_parity(
        "list HTTP failure",
        &gnu,
        Some(&neo),
        expect![[
            r#"(:buffer nil :requests ((:request-line "GET /dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=visual") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"#
        ]],
        &mut divergences,
    );
    drop(pair);

    let mut pair = ready_helm_gitignore_pair("helm-gitignore-generation-failure", &mut divergences);
    eval_both(
        &mut pair,
        "(neomacs-helm-gitignore-tui-seed-unsaved-buffer)",
    );
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=visual" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=visual" visual-list))"#,
    );
    navigate_to_visual_studio_code(&mut pair, "generation failure", &mut divergences);
    send_to_both(&mut pair, |session| session.send_keys("C-c C-z"));
    let gnu_stopped = wait_for_state_file(
        &mut pair.gnu,
        "server-stopped.state",
        ":server-live nil",
        "GNU",
    );
    expect![[r#"(:server-live nil)"#]].assert_eq(&gnu_stopped);
    let neo_stopped = wait_for_state_file(
        &mut pair.neo,
        "server-stopped.state",
        ":server-live nil",
        "Neomacs",
    );
    if neo_stopped != gnu_stopped {
        divergences.push(format!(
            "stopped generation fixture differs:\nGNU:\n{gnu_stopped}\nNeomacs:\n{neo_stopped}"
        ));
    }
    send_to_both(&mut pair, |session| session.send_key("RET"));
    let gnu_attempted = wait_for_state_file(
        &mut pair.gnu,
        "request-urls.state",
        "<origin>/api/visualstudiocode",
        "GNU",
    );
    expect![[
        r#"("<origin>/dropdown/templates.json?term=visual" "<origin>/api/visualstudiocode")"#
    ]]
    .assert_eq(&gnu_attempted);
    let neo_attempted = wait_for_state_file(
        &mut pair.neo,
        "request-urls.state",
        "<origin>/api/visualstudiocode",
        "Neomacs",
    );
    if neo_attempted != gnu_attempted {
        divergences.push(format!(
            "generation request URL differs:\nGNU:\n{gnu_attempted}\nNeomacs:\n{neo_attempted}"
        ));
    }
    wait_for_progress(
        &mut pair,
        "generation connection-refused failure",
        Duration::from_secs(15),
        |grid| {
            grid.iter().any(|row| {
                row.contains("Keyword argument")
                    || row.contains("Wrong number of arguments")
                    || row.contains("wrong-number-of-arguments")
            })
        },
        &mut divergences,
    );
    assert_terminal_failure_parity(
        &pair,
        "generation connection-refused failure",
        expect![
            "error in process sentinel: Keyword argument :data not one of (:error-thrown :&allow-other-keys&rest :)"
        ],
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_keys("C-g C-g"));
    wait_for_progress(
        &mut pair,
        "generation failure cleanup",
        Duration::from_secs(5),
        |grid| {
            grid.iter()
                .any(|row| row.contains("Release engineering scratchpad"))
                && !grid.iter().any(|row| row.contains("pattern: visual"))
        },
        &mut divergences,
    );
    let gnu = capture_editor(&mut pair.gnu, "generation-failure", "GNU");
    let neo = capture_editor(&mut pair.neo, "generation-failure", "Neomacs");
    assert_gnu_literal_and_parity(
        "generation failure preserves unsaved buffer",
        &gnu,
        Some(&neo),
        expect![[r##"
            (:buffer (:text "# Unsaved incident-specific exclusions
            secret-release-token.txt
            " :point 3 :mode gitignore-mode :modified t :file nil :selected nil) :requests ((:request-line "GET /dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=visual" "<origin>/api/visualstudiocode") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"##]],
        &mut divergences,
    );
    assert_no_divergences(&divergences);
}
