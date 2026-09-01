use std::time::Duration;

use crate::{ANACONDA_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANACONDA_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// anaconda-mode edits Python by talking to a JSON-RPC server it starts as a
/// subprocess: `pythonic-start-process' launches a Python interpreter on the
/// packaged `anaconda-mode.py', the server prints `anaconda_mode port NNNN' on
/// stdout, and every command POSTs a JSON-RPC 2.0 body to that port with
/// `url-retrieve'.  These workflows drive that whole path for real - a real
/// `.py' file in the per-case sandbox, real key bindings, a real subprocess, a
/// real listening socket, real HTTP, real JSON.
///
/// One boundary is faked, and only one.  The server's brain is jedi, which is
/// not installed here and is only obtainable by letting `anaconda-mode.py' pip
/// install it from PyPI at first use - a network fetch inside a parity test.
/// So the interpreter is a recording stand-in installed in the sandbox and
/// selected through `pythonic-interpreter', the package's own documented knob:
/// it writes down the exact argv and working directory anaconda-mode chose,
/// then announces the port of a JSON-RPC server that answers with payloads
/// recorded off the wire from the real thing.  Those payloads are not
/// invented - every one was produced by running this pinned `anaconda-mode.py'
/// against this exact fixture with jedi 0.19.2 and service_factory 0.1.6, and
/// the HTTP framing mirrors service_factory's `BaseHTTPRequestHandler'
/// provider, down to the status codes and the JSON-RPC error objects.
///
/// Two harness details live here rather than in each workflow.
/// `ana-test-invoke' restores `print-circle' to nil while a command runs,
/// because the oracle turns it on to print its own result and anaconda-mode
/// `format's the url status into `*anaconda-response*', where reader back
/// references are text no user would ever see.  And `ana-test-normalize'
/// replaces the installed package directory in recorded arguments, so the
/// source lock's cache hashes stay out of the expectations.
const ANACONDA_MODE_TEST_PRELUDE: &str = r##"(require 'cl-lib)
(require 'json)

(defun ana-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ana-test-copy (value)
  (if (stringp value) (copy-sequence value) value))

;;; The Python project the user is editing.

(defconst ana-test-fixture-source
  "\"\"\"Warehouse inventory helpers for the anaconda-mode parity fixture.\"\"\"


class Widget:
    \"\"\"A catalogue item with a name and a price.\"\"\"

    def __init__(self, name, price):
        self.name = name
        self.price = price

    def discounted(self, percent):
        \"\"\"Return the price with PERCENT taken off.\"\"\"
        return self.price * (100 - percent) / 100

    def display_name(self):
        \"\"\"Return the name shown to a customer.\"\"\"
        return self.name.upper()

    def duplicate(self):
        \"\"\"Return an independent copy of this widget.\"\"\"
        return Widget(self.name, self.price)


def build_catalogue(names, price):
    \"\"\"Create one Widget per entry of NAMES, each costing PRICE.\"\"\"
    return [Widget(name, price) for name in names]


def total_price(widgets):
    \"\"\"Sum the price of every widget in WIDGETS.\"\"\"
    return sum(widget.price for widget in widgets)


catalogue = build_catalogue([\"bolt\", \"nut\", \"washer\"], 12)
first = catalogue[0]
print(first.dis)
print(total_price(catalogue))
print(first.dup)
sample = Widget(\"bolt\", 12)
")

(defun ana-test-write-fixture ()
  (let ((path (ana-test-path "project/inventory.py")))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert ana-test-fixture-source)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

;;; Recorded server answers.
;;
;; Every payload below came off the wire from the real server: the pinned
;; anaconda-mode.py running this exact fixture under jedi 0.19.2 and
;; service_factory 0.1.6.  "<FIXTURE>" stands for the `path' the request
;; carried, which is what jedi reports for a definition in the file being
;; edited.  The `show_doc' module name is the dotted name of a top-level
;; module in the project root.

(defconst ana-test-recordings
  '(("complete" 36 15 . [["discounted" "function"] ["display_name" "function"]])
    ("complete" 38 15 . [["duplicate" "function"]])
    ("complete" 37 29 . [])
    ("infer" 36 8 . [["<FIXTURE>" 4 6 "class Widget:"]])
    ("infer" 37 12 . [["<FIXTURE>" 29 4 "def total_price(widgets):"]])
    ("infer" 39 11 . [["<FIXTURE>" 4 6 "class Widget:"]])
    ("goto" 36 8 . [["<FIXTURE>" 35 0 "first = catalogue[0]"]])
    ("get_references" 37 12 . [["<FIXTURE>" 29 4 "def total_price(widgets):"]
                               ["<FIXTURE>" 37 6 "print(total_price(catalogue))"]])
    ("get_references" 36 8 . [["<FIXTURE>" 35 0 "first = catalogue[0]"]
                              ["<FIXTURE>" 36 6 "print(first.dis)"]
                              ["<FIXTURE>" 38 6 "print(first.dup)"]])
    ("show_doc" 37 12 . [["inventory" "total_price(widgets)\n\nSum the price of every widget in WIDGETS."]])
    ("show_doc" 36 8 . [["inventory" "A catalogue item with a name and a price."]])
    ("show_doc" 39 11 . [["inventory" "Widget(name, price)\n\nA catalogue item with a name and a price."]])
    ("show_doc" 37 29 . [])
    ("eldoc" 37 18 . [["total_price" 0 ["widgets"]]])
    ("eldoc" 39 17 . [["Widget" 0 ["name" "price"]]])
    ("eldoc" 39 24 . [["Widget" 1 ["name" "price"]]])
    ("eldoc" 36 15 . [["print" 0 ["*values: object" "sep: Optional[str]=..."
                                  "end: Optional[str]=..."
                                  "file: Optional[SupportsWrite[str]]=..."
                                  "flush: bool=..."]]])))

(defun ana-test-substitute-fixture (value path)
  (cond ((equal value "<FIXTURE>") (copy-sequence path))
        ((vectorp value) (cl-map 'vector (lambda (it) (ana-test-substitute-fixture it path)) value))
        (t value)))

(defun ana-test-recorded-result (method line column path)
  (let ((entry (assoc (list method line column)
                      (mapcar (lambda (it) (cons (list (nth 0 it) (nth 1 it) (nth 2 it)) (cdddr it)))
                              ana-test-recordings))))
    (if entry
        (ana-test-substitute-fixture (cdr entry) path)
      :unrecorded)))

;;; The stand-in server, speaking service_factory's HTTP JSON-RPC.

(defvar ana-test-server nil)
(defvar ana-test-requests nil)
(defvar ana-test-connections nil)
(defvar ana-test-deferred nil)
(defvar ana-test-behavior 'ok
  "One of `ok', `defer', `malformed' or `rpc-error'.")

(defun ana-test-server-port ()
  (process-contact ana-test-server :service))

(defun ana-test-server-start (&optional host)
  (setq ana-test-requests nil
        ana-test-connections nil
        ana-test-deferred nil
        ana-test-server
        (make-network-process
         :name "ana-test-jsonrpc" :server t
         :host (or host anaconda-mode-localhost-address)
         :service t :family 'ipv4 :coding 'binary :noquery t
         :filter #'ana-test-server-filter
         :log (lambda (_server connection _message)
                (push connection ana-test-connections)
                (set-process-query-on-exit-flag connection nil))))
  (ana-test-server-port))

(defun ana-test-server-stop ()
  (dolist (connection ana-test-connections)
    (when (process-live-p connection) (delete-process connection)))
  (setq ana-test-connections nil ana-test-deferred nil)
  (when (and ana-test-server (process-live-p ana-test-server))
    (delete-process ana-test-server))
  (setq ana-test-server nil))

(defun ana-test-server-filter (connection chunk)
  (process-put connection 'inbox (concat (or (process-get connection 'inbox) "") chunk))
  (let* ((text (process-get connection 'inbox))
         (header-end (string-match "\r\n\r\n" text)))
    (when header-end
      (let* ((headers (substring text 0 header-end))
             (body-start (+ header-end 4))
             (size (if (string-match "[Cc]ontent-[Ll]ength: *\\([0-9]+\\)" headers)
                       (string-to-number (match-string 1 headers))
                     0)))
        (when (>= (- (length text) body-start) size)
          (process-put connection 'inbox "")
          (ana-test-server-answer
           connection headers
           (decode-coding-string (substring text body-start (+ body-start size)) 'utf-8)))))))

(defun ana-test-server-answer (connection headers body)
  (let* ((json-object-type 'alist)
         (json-array-type 'vector)
         (request (json-read-from-string body))
         (params (cdr (assq 'params request)))
         (method (cdr (assq 'method request)))
         (line (cdr (assq 'line params)))
         (column (cdr (assq 'column params)))
         (path (cdr (assq 'path params)))
         (id (cdr (assq 'id request))))
    (push (list :body (copy-sequence body)
                :request-line (copy-sequence (car (split-string headers "\r\n")))
                :jsonrpc (ana-test-copy (cdr (assq 'jsonrpc request)))
                :id id
                :method (ana-test-copy method)
                :line line
                :column column
                :path (ana-test-copy path)
                :source (ana-test-copy (cdr (assq 'source params))))
          ana-test-requests)
    (pcase ana-test-behavior
      ('defer (push (list connection method line column path) ana-test-deferred))
      ('malformed (ana-test-server-send connection 200 "<html>anaconda is not running here</html>"))
      ('rpc-error
       (ana-test-server-send
        connection 500
        (json-encode `((jsonrpc . "2.0") (id . ,id)
                       (error . ((code . -32000) (message . "Server error")
                                 (data . "AttributeError(\"'NoneType' object has no attribute 'start_pos'\")")))))))
      (_ (ana-test-server-respond connection id method line column path)))))

(defun ana-test-server-respond (connection id method line column path)
  (let ((result (ana-test-recorded-result method line column path)))
    (if (eq result :unrecorded)
        (ana-test-server-send
         connection 400
         (json-encode `((jsonrpc . "2.0") (id . ,id)
                        (error . ((code . -32601) (message . "Method not found"))))))
      (ana-test-server-send
       connection 200
       (json-encode `((jsonrpc . "2.0") (id . ,id) (result . ,result)))))))

(defun ana-test-server-send (connection status payload)
  (let ((bytes (encode-coding-string payload 'utf-8))
        (reason (pcase status (200 "OK") (400 "Bad Request") (500 "Internal Server Error"))))
    (process-send-string
     connection
     (concat (format "HTTP/1.1 %d %s\r\n" status reason)
             "Server: BaseHTTP/0.6 Python/3.13.12\r\n"
             "Date: Mon, 28 Jul 2026 00:00:00 GMT\r\n"
             (format "Content-Length: %d\r\n" (length bytes))
             "\r\n"
             bytes))))

(defun ana-test-release ()
  "Answer every request the server held back, oldest first."
  (let ((held (reverse ana-test-deferred)))
    (setq ana-test-deferred nil)
    (dolist (entry held)
      (cl-destructuring-bind (connection method line column path) entry
        (ana-test-server-respond connection 1 method line column path)))
    (length held)))

(defun ana-test-server-requests ()
  "Every recorded request, oldest first, with the source summarised."
  (mapcar (lambda (request)
            (let ((copy (copy-sequence request)))
              (plist-put copy :source
                         (and (plist-get copy :source)
                              (list :length (length (plist-get copy :source)))))
              (plist-put copy :body nil)))
          (reverse ana-test-requests)))

(defun ana-test-request-methods ()
  (mapcar (lambda (request)
            (list (ana-test-copy (plist-get request :method))
                  (plist-get request :line)
                  (plist-get request :column)))
          (ana-test-server-requests)))

(defun ana-test-server-bodies ()
  "The verbatim JSON bodies the package posted, oldest first."
  (mapcar (lambda (request) (copy-sequence (plist-get request :body)))
          (reverse ana-test-requests)))

;;; The stand-in interpreter.

(defconst ana-test-interpreter-script "\
#!/bin/sh
# Stands in for the Python interpreter anaconda-mode launches.  Records the
# exact argv and working directory the package chose, announces the recording
# server's port in service_factory's own format, then stays alive.
for arg; do printf 'arg %s\\n' \"$arg\"; done >> \"$ANA_TEST_ARGV\"
printf 'cwd %s\\n' \"$PWD\" >> \"$ANA_TEST_ARGV\"
printf -- '--\\n' >> \"$ANA_TEST_ARGV\"
printf '%s\\n' \"$ANA_TEST_BANNER\"
exec cat > /dev/null
")

(defconst ana-test-failing-interpreter-script "\
#!/bin/sh
# Stands in for a Python that cannot bring the server up, which is what a user
# without jedi sees: pip output, a traceback, and a dead process.
for arg; do printf 'arg %s\\n' \"$arg\"; done >> \"$ANA_TEST_ARGV\"
printf 'cwd %s\\n' \"$PWD\" >> \"$ANA_TEST_ARGV\"
printf -- '--\\n' >> \"$ANA_TEST_ARGV\"
printf 'Collecting jedi==0.19.2\\n'
printf '\\033[31mERROR: No matching distribution found for jedi==0.19.2\\033[0m\\n'
printf 'Traceback (most recent call last):\\n'
printf '  File \"anaconda-mode.py\", line 113, in <module>\\n'
printf '    import jedi\\n'
printf \"ModuleNotFoundError: No module named 'jedi'\\n\"
exit 1
")

(defun ana-test-install-interpreter (&optional name script)
  (let ((path (ana-test-path (concat "bin/" (or name "python")))))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert (or script ana-test-interpreter-script))
      (write-region (point-min) (point-max) path nil 'silent))
    (set-file-modes path #o755)
    path))

(defun ana-test-normalize (value)
  "Replace the installed package directory in VALUE, so the source lock's
cache hashes never reach an expectation."
  (if (stringp value)
      (replace-regexp-in-string
       (regexp-quote (file-name-directory (locate-library "anaconda-mode")))
       "[PACKAGE]/" (copy-sequence value) t t)
    value))

(defun ana-test-unsandbox (value)
  "Replace the sandbox path in VALUE, absolute or workspace relative.
xref groups its results by project, and the project here is the Neomacs
checkout, so the heading it renders is the per-case sandbox's own random name
spelled relative to the repository root - which the oracle's absolute-path
normaliser cannot see."
  (if (stringp value)
      (let* ((sandbox (directory-file-name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
             (workspace (getenv "NEOMACS_TEST_WORKSPACE_ROOT"))
             (relative (and workspace (file-relative-name sandbox workspace)))
             (text (copy-sequence value)))
        (when relative
          (setq text (replace-regexp-in-string (regexp-quote relative) "[SANDBOX]" text t t)))
        (replace-regexp-in-string (regexp-quote sandbox) "[SANDBOX]" text t t))
    value))

(defun ana-test-argv ()
  "Every launch the package made, oldest first, as (ARGUMENT... CWD)."
  (let ((path (ana-test-path "argv.txt")))
    (when (file-exists-p path)
      (mapcar
       (lambda (record)
         (mapcar (lambda (line)
                   (ana-test-normalize
                    (cond ((string-prefix-p "arg " line) (substring line 4))
                          (t line))))
                 (split-string record "\n" t)))
       (split-string
        (with-temp-buffer (insert-file-contents path) (buffer-string))
        "^--\n" t)))))

;;; Driving the editor.

(defun ana-test-setup (&optional address)
  "Install the stand-in interpreter and start the recording server."
  (setq anaconda-mode-installation-directory (ana-test-path "anaconda-install"))
  (when address (setq anaconda-mode-localhost-address address))
  (let ((port (ana-test-server-start)))
    (setenv "ANA_TEST_ARGV" (ana-test-path "argv.txt"))
    (setenv "ANA_TEST_BANNER" (format "anaconda_mode port %d" port))
    (setq pythonic-interpreter (ana-test-install-interpreter))
    (setq python-shell-interpreter pythonic-interpreter)
    port))

(defun ana-test-teardown ()
  (ignore-errors (anaconda-mode-stop))
  (ana-test-server-stop)
  (dolist (name (list anaconda-mode-process-buffer anaconda-mode-response-buffer
                      "*Anaconda*" "*Completions*" "*xref*" "*eldoc*"))
    (when (get-buffer name) (kill-buffer name))))

(defun ana-test-wait (predicate &optional seconds)
  (let ((deadline (+ (float-time) (or seconds 20))))
    (while (and (not (funcall predicate)) (< (float-time) deadline))
      (accept-process-output nil 0.02))
    (and (funcall predicate) t)))

(defun ana-test-invoke (keys predicate &optional seconds)
  "Type KEYS in the selected window and pump until PREDICATE holds."
  (let ((print-circle nil))
    (execute-kbd-macro (kbd keys))
    (ana-test-wait predicate seconds)))

(defun ana-test-start-server ()
  "Start the package's server and wait until its port is bound."
  (let ((started nil))
    (anaconda-mode-start (lambda () (setq started t)))
    (list :callback (ana-test-wait (lambda () started))
          :running (and (anaconda-mode-running-p) t)
          :bound (anaconda-mode-bound-p))))

(defun ana-test-visit ()
  "Visit the fixture in the selected window with `anaconda-mode' on."
  (let ((buffer (find-file-noselect (ana-test-write-fixture))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (anaconda-mode 1)
    buffer))

(defmacro ana-test-with-project (&rest body)
  "Edit the fixture with the package and the recording server both running."
  `(let ((buffer nil))
     (unwind-protect
         (progn
           (ana-test-setup)
           (setq buffer (ana-test-visit))
           (ana-test-start-server)
           ,@body)
       (when (buffer-live-p buffer) (kill-buffer buffer))
       (ana-test-teardown))))

(defun ana-test-goto (line column)
  (goto-char (point-min))
  (forward-line (1- line))
  (forward-char column)
  (list (line-number-at-pos) (- (point) (line-beginning-position))))

(defun ana-test-here ()
  (list :line (line-number-at-pos)
        :column (- (point) (line-beginning-position))
        :text (buffer-substring-no-properties (line-beginning-position) (line-end-position))))

(defun ana-test-faces (start end &optional object property)
  "Every run of PROPERTY between START and END, as (VALUE TEXT)."
  (let ((property (or property 'face)) (position start) runs)
    (while (< position end)
      (let ((next (if object
                      (next-single-property-change position property object end)
                    (next-single-property-change position property nil end))))
        (push (list (get-text-property position property object)
                    (if object
                        (substring-no-properties object position next)
                      (buffer-substring-no-properties position next)))
              runs)
        (setq position next)))
    (nreverse runs)))

(defun ana-test-messages (regexp)
  (let (matches)
    (with-current-buffer "*Messages*"
      (save-excursion
        (goto-char (point-min))
        (while (re-search-forward regexp nil t)
          (push (match-string-no-properties 0) matches))))
    (nreverse matches)))
"##;

fn anaconda_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANACONDA_MODE_MELPA_PIN, "anaconda-mode.el")
        .expect("prepare pinned anaconda-mode source below ./tmp")
        .with_prelude(ANACONDA_MODE_TEST_PRELUDE)
        .with_timeout(ANACONDA_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed anaconda-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_anaconda_mode_parity` cases (2a).
pub(crate) fn assert_anaconda_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(anaconda_mode_oracle(), &name, "anaconda_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn anaconda_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_anaconda_mode_batch(&cases);
}

// END generated package batch tests
