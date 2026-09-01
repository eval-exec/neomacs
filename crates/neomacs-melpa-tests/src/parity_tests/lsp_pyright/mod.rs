use std::time::Duration;

use crate::{CachedMelpaOracle, LSP_PYRIGHT_MELPA_PIN};

pub(crate) use super::batch_support::ParityBatchCase;
use super::batch_support::assert_oracle_batch_cases;

mod workflows;

const LSP_PYRIGHT_TEST_TIMEOUT: Duration = Duration::from_secs(240);

const LSP_PYRIGHT_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)

(defvar neomacs-lsp-pyright-test-lsp-log-records nil
  "Selected package-owned progress messages observed by the fixture.")

(defvar neomacs-lsp-pyright-test-demoted-errors nil
  "Message-handler errors demoted by lsp-mode during fixture dispatch.")

(defun neomacs-lsp-pyright-test-observe-lsp-log
    (original format-string &rest arguments)
  "Record relevant Pyright progress logs, then call ORIGINAL."
  (let ((message (apply #'format format-string arguments)))
    (when (or (string-prefix-p "Pyright language server is analyzing" message)
              (equal message "1 file to analyze"))
      (push message neomacs-lsp-pyright-test-lsp-log-records)))
  (apply original format-string arguments))

(defun neomacs-lsp-pyright-test-observe-message
    (original format-string &rest arguments)
  "Record lsp-mode's demoted handler errors, then call ORIGINAL."
  (let ((message (apply #'format format-string arguments)))
    (when (string-prefix-p "Error processing message " message)
      (push message neomacs-lsp-pyright-test-demoted-errors)))
  (apply original format-string arguments))

(defun neomacs-lsp-pyright-test-disable-work-done-capability
    (original &optional custom-capabilities)
  "Build client capabilities without standard work-done progress support."
  ;; lsp-mode byte-compiles the literal window capability subtree.  Mutating
  ;; that subtree in place would permanently disable work-done progress for
  ;; later projects in the same editor process.
  (let* ((capabilities (copy-tree
                        (funcall original custom-capabilities)))
         (window (alist-get 'window capabilities)))
    (setf (alist-get 'workDoneProgress window) :json-false)
    capabilities))

(defun neomacs-lsp-pyright-test-root (case-name)
  "Return CASE-NAME's disposable directory below the Rust-owned sandbox."
  (file-name-as-directory
   (expand-file-name case-name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun neomacs-lsp-pyright-test-write-file (path contents)
  "Write CONTENTS to PATH, creating its parent directory."
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents)))

(defun neomacs-lsp-pyright-test-write-server (root)
  "Install a deterministic, fail-closed Pyright protocol peer below ROOT."
  (let* ((bin-directory (expand-file-name "bin/" root))
         (program
          (expand-file-name
           (concat lsp-pyright-langserver-command "-langserver")
           bin-directory)))
    (neomacs-lsp-pyright-test-write-file
     program
     "#!/usr/bin/env python3
import json
import os
import sys
from urllib.parse import unquote, urlparse

wire_log = os.environ['NEOMACS_LSP_PYRIGHT_WIRE_LOG']
start_log = os.environ['NEOMACS_LSP_PYRIGHT_START_LOG']
scenario = os.environ['NEOMACS_LSP_PYRIGHT_SCENARIO']
project_root = os.path.realpath(os.environ['NEOMACS_LSP_PYRIGHT_PROJECT'])
source_file = os.path.realpath(os.environ['NEOMACS_LSP_PYRIGHT_SOURCE'])
second_project_root = os.path.realpath(os.environ['NEOMACS_LSP_PYRIGHT_SECOND_PROJECT'])
second_source_file = os.path.realpath(os.environ['NEOMACS_LSP_PYRIGHT_SECOND_SOURCE'])
flavor = os.environ['NEOMACS_LSP_PYRIGHT_FLAVOR']

def append_json(path, value):
    payload = (json.dumps(value, ensure_ascii=False, sort_keys=True) + '\\n').encode('utf-8')
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
    try:
        written = os.write(descriptor, payload)
        if written != len(payload):
            raise RuntimeError('short fixture-log write')
    finally:
        os.close(descriptor)

def record(direction, message):
    append_json(wire_log, {'direction': direction, 'message': message})

def fail(message):
    record('fixture->harness', {'fixtureError': message})
    raise SystemExit(message)

def uri_path(uri):
    parsed = urlparse(uri)
    if parsed.scheme != 'file' or parsed.netloc not in ('', 'localhost'):
        fail('not a local file URI: ' + repr(uri))
    return os.path.realpath(unquote(parsed.path))

append_json(start_log, {
    'argv0': os.path.basename(sys.argv[0]),
    'args': sys.argv[1:],
    'cwd': os.path.realpath(os.getcwd()),
    'scenario': scenario,
    'flavor': flavor,
})
if sys.argv[1:] != ['--stdio']:
    fail('unexpected argv: ' + repr(sys.argv[1:]))
if os.path.basename(sys.argv[0]) != flavor + '-langserver':
    fail('unexpected executable: ' + os.path.basename(sys.argv[0]))
if os.path.realpath(os.getcwd()) != project_root:
    fail('unexpected cwd: ' + os.path.realpath(os.getcwd()))

def receive():
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        if line in (b'\\r\\n', b'\\n'):
            break
        try:
            name, value = line.decode('ascii').split(':', 1)
        except (UnicodeDecodeError, ValueError) as error:
            fail('malformed header: ' + repr(line) + ': ' + str(error))
        lowered = name.lower()
        if lowered in headers:
            fail('duplicate header: ' + name)
        headers[lowered] = value.strip()
    if set(headers) != {'content-length'}:
        fail('unexpected headers: ' + repr(headers))
    try:
        length = int(headers['content-length'])
    except ValueError:
        fail('non-decimal content length')
    body = sys.stdin.buffer.read(length)
    if len(body) != length:
        fail('short body')
    try:
        return json.loads(body.decode('utf-8'))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        fail('malformed JSON body: ' + str(error))

def send(message):
    record('server->client', message)
    body = json.dumps(message, ensure_ascii=False, separators=(',', ':')).encode('utf-8')
    frame = b'Content-Length: ' + str(len(body)).encode('ascii') + b'\\r\\n\\r\\n' + body
    sys.stdout.buffer.write(frame)
    sys.stdout.buffer.flush()

initialized = False
workspace_root_uri = None
opened_uri = None
opened_uris = []
second_root_added = False
configuration_stage = 0
pending_execute = None
shutdown_seen = False
terminal = None

while True:
    message = receive()
    if message is None:
        fail('client closed stdin before the terminal exit notification')
    record('client->server', message)
    method = message.get('method')
    request_id = message.get('id')

    if method == 'initialize':
        if initialized or request_id is None:
            fail('duplicate or id-less initialize')
        params = message.get('params', {})
        root_uri = params.get('rootUri', '')
        folders = params.get('workspaceFolders', [])
        client_info = params.get('clientInfo', {})
        capabilities = params.get('capabilities', {})
        workspace_caps = capabilities.get('workspace', {})
        window_caps = capabilities.get('window', {})
        if os.path.realpath(params.get('rootPath', '')) != project_root:
            fail('unexpected rootPath: ' + repr(params.get('rootPath')))
        if uri_path(root_uri) != project_root:
            fail('unexpected rootUri: ' + repr(root_uri))
        if len(folders) != 1 or uri_path(folders[0].get('uri', '')) != project_root:
            fail('unexpected workspaceFolders: ' + repr(folders))
        if client_info.get('name') != 'emacs' or not isinstance(client_info.get('version'), str):
            fail('unexpected clientInfo: ' + repr(client_info))
        if not isinstance(params.get('processId'), int):
            fail('processId is not an integer: ' + repr(params.get('processId')))
        if params.get('initializationOptions', 'missing') is not None:
            fail('initializationOptions is not null')
        if workspace_caps.get('applyEdit') is not True:
            fail('workspace.applyEdit capability is not true')
        if workspace_caps.get('configuration') is not True:
            fail('workspace.configuration capability is not true')
        if workspace_caps.get('workspaceFolders') is not True:
            fail('workspace.workspaceFolders capability is not true')
        expected_work_done = scenario != 'legacy-progress'
        if window_caps.get('workDoneProgress') is not expected_work_done:
            fail('unexpected window.workDoneProgress capability')
        if scenario == 'initialize-error':
            send({'jsonrpc': '2.0', 'id': request_id, 'error': {
                'code': -32002,
                'message': 'fixture rejected project configuration'
            }})
            terminal = 'initialize-error'
            break
        workspace_root_uri = root_uri
        send({'jsonrpc': '2.0', 'id': request_id, 'result': {
            'capabilities': {
                'positionEncoding': 'utf-16',
                'textDocumentSync': 2,
                'executeCommandProvider': {
                    'commands': [flavor + '.organizeimports']
                },
                'workspace': {'workspaceFolders': {
                    'supported': True,
                    'changeNotifications': True
                }}
            },
            'serverInfo': {'name': 'strict-' + flavor + '-fixture',
                           'version': '1.1.411'}
        }})
    elif method == 'initialized':
        if initialized:
            fail('duplicate initialized notification')
        initialized = True
        send({'jsonrpc': '2.0', 'id': 700,
              'method': 'workspace/configuration',
              'params': {'items': [
                  {'scopeUri': workspace_root_uri, 'section': 'python'}
              ]}})
        configuration_stage = 1
    elif method == 'workspace/didChangeConfiguration':
        if not initialized:
            fail('configuration before initialized')
        if message.get('params') != {'settings': {}}:
            fail('initial configuration was not empty: ' + repr(message.get('params')))
    elif method == 'textDocument/didOpen':
        if not initialized:
            fail('didOpen before initialized')
        text_document = message.get('params', {}).get('textDocument', {})
        document_uri = text_document.get('uri')
        document_path = uri_path(document_uri)
        if text_document.get('languageId') != 'python' or text_document.get('version') != 0:
            fail('unexpected didOpen document: ' + repr(text_document))
        if document_uri in opened_uris:
            fail('duplicate didOpen URI: ' + repr(document_uri))
        if not opened_uris:
            if document_path != source_file:
                fail('first didOpen URI did not identify the primary source file')
            opened_uri = document_uri
            opened_uris.append(document_uri)
        elif (scenario == 'multi-root' and second_root_added and
              len(opened_uris) == 1 and document_path == second_source_file):
            opened_uris.append(document_uri)
        else:
            fail('unexpected additional didOpen URI: ' + repr(document_uri))
    elif method == 'workspace/didChangeWorkspaceFolders':
        event = message.get('params', {}).get('event', {})
        added = event.get('added', [])
        if (scenario != 'multi-root' or second_root_added or len(added) != 1 or
                uri_path(added[0].get('uri', '')) != second_project_root or
                added[0].get('name') != os.path.basename(second_project_root) or
                event.get('removed', []) != []):
            fail('unexpected workspace-folder change: ' + repr(event))
        second_root_added = True
    elif method == 'textDocument/didChange':
        if message.get('params', {}).get('textDocument', {}).get('uri') not in opened_uris:
            fail('didChange for an unopened document')
    elif method == 'workspace/executeCommand':
        params = message.get('params', {})
        if request_id is None or pending_execute is not None:
            fail('invalid execute-command request state')
        if params.get('command') != flavor + '.organizeimports':
            fail('unexpected command: ' + repr(params.get('command')))
        arguments = params.get('arguments', [])
        if len(arguments) != 1 or uri_path(arguments[0]) != source_file:
            fail('unexpected command arguments: ' + repr(arguments))
        if arguments[0] == opened_uri:
            fail('organize-imports unexpectedly reused the encoded didOpen URI')
        if scenario == 'organize-error':
            send({'jsonrpc': '2.0', 'id': request_id, 'error': {
                'code': -32602,
                'message': 'fixture refused to organize this module'
            }})
            continue
        pending_execute = request_id
        send({'jsonrpc': '2.0', 'id': 703,
              'method': 'workspace/applyEdit',
              'params': {'label': 'Organize imports', 'edit': {'changes': {
                  opened_uri: [{
                      'range': {
                          'start': {'line': 0, 'character': 0},
                          'end': {'line': 2, 'character': 0}
                      },
                      'newText': 'import os\\nimport sys\\n'
                  }]
              }}}})
    elif method == 'shutdown':
        if request_id is None or shutdown_seen:
            fail('duplicate or id-less shutdown')
        shutdown_seen = True
        send({'jsonrpc': '2.0', 'id': request_id, 'result': None})
    elif method == 'exit':
        if not shutdown_seen:
            fail('exit arrived before shutdown completed')
        terminal = 'exit'
        break
    elif method in ('textDocument/didClose', 'textDocument/didSave'):
        pass
    elif method is None and request_id == 700:
        result = message.get('result')
        if configuration_stage != 1 or not isinstance(result, list) or len(result) != 1:
            fail('invalid python configuration response')
        configuration_stage = 2
        send({'jsonrpc': '2.0', 'id': 701,
              'method': 'workspace/configuration',
              'params': {'items': [
                  {'scopeUri': workspace_root_uri, 'section': flavor}
              ]}})
    elif method is None and request_id == 701:
        result = message.get('result')
        if configuration_stage != 2 or not isinstance(result, list) or len(result) != 1:
            fail('invalid flavor configuration response')
        configuration_stage = 3
        if scenario == 'legacy-progress':
            send({'jsonrpc': '2.0', 'method': flavor + '/beginProgress'})
            send({'jsonrpc': '2.0', 'method': flavor + '/reportProgress',
                  'params': '1 file to analyze'})
            send({'jsonrpc': '2.0', 'method': flavor + '/endProgress'})
        else:
            send({'jsonrpc': '2.0', 'id': 702,
                  'method': 'window/workDoneProgress/create',
                  'params': {'token': 'pyright-analysis'}})
    elif method is None and request_id == 702:
        if configuration_stage != 3 or message.get('result', 'missing') is not None:
            fail('invalid workDoneProgress/create response')
        send({'jsonrpc': '2.0', 'method': '$/progress',
              'params': {'token': 'pyright-analysis', 'value': {
                  'kind': 'begin', 'title': ''
              }}})
        send({'jsonrpc': '2.0', 'method': '$/progress',
              'params': {'token': 'pyright-analysis', 'value': {
                  'kind': 'report', 'message': '1 file to analyze'
              }}})
        send({'jsonrpc': '2.0', 'method': '$/progress',
              'params': {'token': 'pyright-analysis', 'value': {
                  'kind': 'end'
              }}})
    elif method is None and request_id == 703 and pending_execute is not None:
        if message.get('result', {}).get('applied') is not True:
            fail('workspace edit was not applied: ' + repr(message))
        send({'jsonrpc': '2.0', 'id': pending_execute, 'result': None})
        pending_execute = None
    elif request_id is not None:
        send({'jsonrpc': '2.0', 'id': request_id, 'error': {
            'code': -32601, 'message': 'unexpected request: ' + str(method)
        }})
        fail('unexpected request: ' + str(method))
    else:
        fail('unexpected notification: ' + str(method))

if terminal is None:
    fail('protocol loop ended without a terminal state')
if pending_execute is not None:
    fail('client exited with a pending execute command')
if terminal == 'exit' and configuration_stage != 3:
    fail('client exited before both configuration requests completed')
if terminal == 'exit' and scenario == 'multi-root':
    if not second_root_added or len(opened_uris) != 2:
        fail('client exited before the second workspace and document were opened')
record('fixture->harness', {
    'fixtureState': {
        'terminal': terminal,
        'planExhausted': True,
        'misses': [],
        'configurationResponses': configuration_stage
    }
})
")
    (set-file-modes program #o755)
    bin-directory))

(defun neomacs-lsp-pyright-test-write-fake-npm (root)
  "Install the strict npm boundary adapter below ROOT and return its bin dir."
  (let* ((bin-directory (expand-file-name "npm-bin/" root))
         (program (expand-file-name "npm" bin-directory)))
    (neomacs-lsp-pyright-test-write-file
     program
     "#!/usr/bin/env python3
import json
import os
import shutil
import sys

log_path = os.environ['NEOMACS_LSP_PYRIGHT_NPM_LOG']
expected_prefix = os.path.realpath(os.environ['NEOMACS_LSP_PYRIGHT_NPM_PREFIX'])
expected_project = os.path.realpath(os.environ['NEOMACS_LSP_PYRIGHT_PROJECT'])
template = os.path.realpath(os.environ['NEOMACS_LSP_PYRIGHT_SERVER_TEMPLATE'])
flavor = os.environ['NEOMACS_LSP_PYRIGHT_FLAVOR']
scenario = os.environ['NEOMACS_LSP_PYRIGHT_NPM_SCENARIO']
tmp_root = os.path.realpath(os.environ['NEOMACS_LSP_PYRIGHT_TMP_ROOT'])

def record(kind):
    value = {
        'kind': kind,
        'args': sys.argv[1:],
        'cwd': os.path.realpath(os.getcwd()),
        'insideEmacsMode': os.environ.get('INSIDE_EMACS', '').rsplit(',', 1)[-1],
        'pager': os.environ.get('PAGER'),
        'tmpdir': os.path.realpath(os.environ.get('TMPDIR', '')),
    }
    with open(log_path, 'a', encoding='utf-8') as stream:
        stream.write(json.dumps(value, ensure_ascii=False, sort_keys=True) + '\\n')

def reject(message):
    record('rejected')
    print('NEOMACS_FAKE_NPM: ' + message, file=sys.stderr)
    raise SystemExit(91)

if os.path.realpath(os.getcwd()) != expected_project:
    reject('unexpected cwd')
if os.path.commonpath([os.path.realpath(os.environ.get('TMPDIR', '')), tmp_root]) != tmp_root:
    reject('TMPDIR escaped the workspace-local tmp root')

install_args = ['-g', '--prefix', expected_prefix, 'install', flavor]
view_args = ['view', flavor, 'peerDependencies']
if sys.argv[1:] == install_args:
    record('install')
    if scenario == 'failure':
        print('NEOMACS_FAKE_NPM: intentional install failure', file=sys.stderr)
        raise SystemExit(23)
    if scenario != 'success':
        reject('unknown scenario')
    lib_dir = os.path.join(expected_prefix, 'lib')
    bin_dir = os.path.join(expected_prefix, 'bin')
    os.makedirs(lib_dir, exist_ok=True)
    os.makedirs(bin_dir, exist_ok=True)
    temporary = os.path.join(lib_dir, flavor + '-langserver.partial')
    destination = os.path.join(bin_dir, flavor + '-langserver')
    shutil.copyfile(template, temporary)
    os.chmod(temporary, 0o755)
    os.replace(temporary, destination)
    print('installed ' + flavor + ' fixture')
elif sys.argv[1:] == view_args:
    record('view')
    if scenario != 'success':
        reject('view after unsuccessful install')
    print('')
else:
    reject('unexpected argv: ' + repr(sys.argv[1:]))
")
    (set-file-modes program #o755)
    bin-directory))

(defun neomacs-lsp-pyright-test-write-python (path)
  "Install a deterministic executable Python marker at PATH."
  (neomacs-lsp-pyright-test-write-file path "#!/bin/sh\nexit 0\n")
  (set-file-modes path #o755)
  path)

(defun neomacs-lsp-pyright-test-wait (predicate description)
  "Drive subprocesses and timers until PREDICATE succeeds or signal."
  (let ((deadline (+ (float-time) 20.0)))
    (while (and (not (funcall predicate))
                (< (float-time) deadline))
      (accept-process-output nil 0.02))
    (unless (funcall predicate)
      (error "Timed out waiting for %s" description))))

(defun neomacs-lsp-pyright-test-read-json-lines (path)
  "Read the complete JSON values recorded at PATH."
  (when (file-readable-p path)
    (with-temp-buffer
      (insert-file-contents path)
      (goto-char (point-min))
      (let (values)
        (while (not (eobp))
          (push (json-parse-string
                 (buffer-substring-no-properties
                  (line-beginning-position) (line-end-position))
                 :object-type 'alist
                 :array-type 'list
                 :null-object nil
                 :false-object :json-false)
                values)
          (forward-line 1))
        (nreverse values)))))

(defun neomacs-lsp-pyright-test-json (object key)
  "Return KEY from JSON alist OBJECT."
  (alist-get key object nil nil #'string=))

(defun neomacs-lsp-pyright-test-entry-message (entry)
  "Return the JSON-RPC message nested in recorded ENTRY."
  (neomacs-lsp-pyright-test-json entry "message"))

(defun neomacs-lsp-pyright-test-messages (path direction)
  "Return messages recorded at PATH in DIRECTION."
  (mapcar
   #'neomacs-lsp-pyright-test-entry-message
   (seq-filter
    (lambda (entry)
      (equal (neomacs-lsp-pyright-test-json entry "direction") direction))
    (neomacs-lsp-pyright-test-read-json-lines path))))

(defun neomacs-lsp-pyright-test-messages-by-method (path direction method)
  "Return DIRECTION messages from PATH whose method equals METHOD."
  (seq-filter
   (lambda (message)
     (equal (neomacs-lsp-pyright-test-json message "method") method))
   (neomacs-lsp-pyright-test-messages path direction)))

(defun neomacs-lsp-pyright-test-response (path direction id)
  "Return the DIRECTION response with numeric ID from PATH."
  (seq-find
   (lambda (message)
     (and (equal (neomacs-lsp-pyright-test-json message "id") id)
          (not (neomacs-lsp-pyright-test-json message "method"))))
   (neomacs-lsp-pyright-test-messages path direction)))

(defun neomacs-lsp-pyright-test-fixture-errors (path)
  "Return all fail-closed peer errors recorded at PATH."
  (delq nil
        (mapcar
         (lambda (message)
           (neomacs-lsp-pyright-test-json message "fixtureError"))
         (neomacs-lsp-pyright-test-messages path "fixture->harness"))))

(defun neomacs-lsp-pyright-test-fixture-state (path)
  "Return the terminal peer state recorded at PATH."
  (let ((states
         (delq nil
               (mapcar
                (lambda (message)
                  (neomacs-lsp-pyright-test-json message "fixtureState"))
                (neomacs-lsp-pyright-test-messages path "fixture->harness")))))
    (when (= (length states) 1)
      (car states))))

(defun neomacs-lsp-pyright-test-normalize-uri (uri root)
  "Express file URI URI relative to ROOT."
  (file-relative-name
   (lsp--uri-to-path (if (symbolp uri) (symbol-name uri) uri))
   root))

(defun neomacs-lsp-pyright-test-normalize-path (path root)
  "Express absolute PATH relative to ROOT, preserving non-path values."
  (if (and (stringp path) (file-name-absolute-p path))
      (file-relative-name path root)
    path))

(defun neomacs-lsp-pyright-test-install-buffer-summary (buffer root)
  "Return BUFFER's stable public compilation evidence relative to ROOT."
  (when buffer
    (with-current-buffer buffer
      (let* ((text (buffer-substring-no-properties (point-min) (point-max)))
             (lines (split-string text "\n" t))
             (command
              (seq-find (lambda (line)
                          (string-match-p "npm -g --prefix" line))
                        lines))
             (output
              (seq-filter
               (lambda (line)
                 (or (string-prefix-p "installed " line)
                     (string-prefix-p "NEOMACS_FAKE_NPM:" line)))
               lines))
             (status
              (cond
               ((string-match-p "Comint finished" text) "finished")
               ((string-match "Comint exited abnormally with code \\([0-9]+\\)"
                              text)
                (concat "exited-abnormally-"
                        (match-string 1 text)))
               (t "missing"))))
        (list :mode lsp-installation-buffer-mode
              :process (get-buffer-process buffer)
              :default-directory (file-relative-name default-directory root)
              :command (and command
                            (replace-regexp-in-string
                             (regexp-quote root) "[ROOT]/" command t t))
              :output output
              :status status)))))

(defun neomacs-lsp-pyright-test-wire-plan (path)
  "Return exact bidirectional JSON-RPC method ordering from PATH."
  (delq nil
        (mapcar
         (lambda (entry)
           (let* ((message (neomacs-lsp-pyright-test-entry-message entry))
                  (method (neomacs-lsp-pyright-test-json message "method")))
             (when method
               (list (neomacs-lsp-pyright-test-json entry "direction") method))))
         (neomacs-lsp-pyright-test-read-json-lines path))))

(defun neomacs-lsp-pyright-test-stop-workspace (workspace)
  "Shut down WORKSPACE and await process and lsp-session cleanup."
  (when workspace
    (let ((process (lsp--workspace-proc workspace))
          (prefix
           (format "%s"
                   (lsp--client-server-id
                    (lsp--workspace-client workspace)))))
      (when (process-live-p process)
        (with-lsp-workspace workspace
          (let ((lsp-response-timeout 2))
            (lsp-request "shutdown" nil))
          (lsp-notify "exit" nil))
        (neomacs-lsp-pyright-test-wait
         (lambda () (not (process-live-p process)))
         "the Pyright fixture process to exit"))
      ;; The process sentinel owns removal from lsp-session.  Waiting only for
      ;; the fixture's terminal log races that sentinel and can make the next
      ;; project reuse a dead workspace.
      (accept-process-output process 0.01)
      (neomacs-lsp-pyright-test-wait
       (lambda ()
         (not
          (seq-some
           (lambda (workspaces) (memq workspace workspaces))
           (hash-table-values
            (lsp-session-folder->servers (lsp-session))))))
       "lsp-mode to deregister the Pyright workspace")
      (dolist (process (process-list))
        (when (string-prefix-p prefix (process-name process))
          (when (process-live-p process)
            (delete-process process))
          (accept-process-output process 0.01))))))

(defmacro neomacs-lsp-pyright-test-with-project (case-name scenario &rest body)
  "Run BODY in a real Python project against a strict protocol peer."
  (declare (indent 2) (debug t))
  `(let* ((root (neomacs-lsp-pyright-test-root ,case-name))
          (project-root (expand-file-name "analytics Ω/" root))
          (source-file (expand-file-name "src/report.py" project-root))
          (second-root (expand-file-name "shared service Ω/" root))
          (second-source-file (expand-file-name "src/stubs.py" second-root))
          (wire-log (expand-file-name "pyright-wire.jsonl" root))
          (start-log (expand-file-name "pyright-start.jsonl" root))
          (server-bin (neomacs-lsp-pyright-test-write-server root))
          (explicit-python (expand-file-name "envs/bin/python" project-root))
          (lsp--session (make-lsp-session))
          (lsp-session-file nil)
          (lsp-auto-guess-root t)
          (lsp-enable-file-watchers nil)
          (lsp-auto-configure nil)
          (lsp-enable-suggest-server-download nil)
          (lsp-warn-no-matched-clients t)
          (lsp-restart 'ignore)
          (lsp--show-message nil)
          (lsp-idle-delay 0.01)
          (lsp-response-timeout 20)
          (lsp-server-install-dir (expand-file-name "server-install/" root))
          (lsp-pyright-langserver-command-args '("--stdio"))
          (lsp-pyright-disable-language-services nil)
          (lsp-pyright-disable-organize-imports nil)
          (lsp-pyright-disable-tagged-hints t)
          (lsp-pyright-type-checking-mode "strict")
          (lsp-pyright-diagnostic-mode "workspace")
          (lsp-pyright-log-level "warning")
          (lsp-pyright-auto-search-paths nil)
          (lsp-pyright-auto-import-completions nil)
          (lsp-pyright-extra-paths ["src" "vendor types/λ"])
          (lsp-pyright-venv-path (expand-file-name "envs/" project-root))
          (lsp-pyright-basedpyright-inlay-hints-variable-types t)
          (lsp-pyright-basedpyright-inlay-hints-call-argument-names nil)
          (lsp-pyright-basedpyright-inlay-hints-function-return-types t)
          (lsp-pyright-basedpyright-inlay-hints-generic-types nil)
          (lsp-pyright-diagnostic-severity-overrides
           '(("reportMissingImports" . "error")
             ("reportUnusedVariable" . :json-false)))
          (enable-dir-local-variables nil)
          (process-environment (copy-sequence process-environment))
          (exec-path (cons server-bin exec-path))
          (origin-buffer (current-buffer))
          (window-configuration (current-window-configuration))
          (source-buffer nil)
          (second-source-buffer nil)
          (workspace nil)
          (second-workspace nil)
          (case-result nil)
          (case-completed nil))
     (make-directory (expand-file-name ".git/" project-root) t)
     (make-directory (expand-file-name ".git/" second-root) t)
     (neomacs-lsp-pyright-test-write-file
      source-file
      "import sys\nimport os\n\n\ndef release_label(name: str) -> str:\n    return f\"ready:{name}\"\n")
     (neomacs-lsp-pyright-test-write-python explicit-python)
     (neomacs-lsp-pyright-test-write-file
      second-source-file
      "from typing import Protocol\n\n\nclass Release(Protocol):\n    name: str\n")
     (setenv "PATH" (concat server-bin path-separator (getenv "PATH")))
     (setenv "NEOMACS_LSP_PYRIGHT_WIRE_LOG" wire-log)
     (setenv "NEOMACS_LSP_PYRIGHT_START_LOG" start-log)
     (setenv "NEOMACS_LSP_PYRIGHT_SCENARIO" ,scenario)
     (setenv "NEOMACS_LSP_PYRIGHT_PROJECT"
             (directory-file-name (file-truename project-root)))
     (setenv "NEOMACS_LSP_PYRIGHT_SOURCE" (file-truename source-file))
     (setenv "NEOMACS_LSP_PYRIGHT_SECOND_PROJECT"
             (directory-file-name (file-truename second-root)))
     (setenv "NEOMACS_LSP_PYRIGHT_SECOND_SOURCE"
             (file-truename second-source-file))
     (setenv "NEOMACS_LSP_PYRIGHT_FLAVOR" lsp-pyright-langserver-command)
     (setq neomacs-lsp-pyright-test-lsp-log-records nil)
     (setq neomacs-lsp-pyright-test-demoted-errors nil)
     (advice-add 'lsp-log :around #'neomacs-lsp-pyright-test-observe-lsp-log)
     (advice-add 'message :around #'neomacs-lsp-pyright-test-observe-message)
     (unwind-protect
         (progn
           (setq source-buffer (find-file-noselect source-file))
           (with-current-buffer source-buffer
             (python-mode)
             (if (equal ,scenario "legacy-progress")
                 (let ((original-capabilities
                        (symbol-function 'lsp--client-capabilities)))
                   (cl-letf
                       (((symbol-function 'lsp--client-capabilities)
                         (lambda (&optional custom-capabilities)
                           (neomacs-lsp-pyright-test-disable-work-done-capability
                            original-capabilities custom-capabilities))))
                     (lsp)))
               (lsp))
             (setq workspace (lsp-find-workspace 'pyright source-file))
             (setq case-result (progn ,@body)))
           (neomacs-lsp-pyright-test-stop-workspace workspace)
           (neomacs-lsp-pyright-test-wait
            (lambda () (neomacs-lsp-pyright-test-fixture-state wire-log))
            "the terminal fixture state")
           (let* ((errors (neomacs-lsp-pyright-test-fixture-errors wire-log))
                  (state (neomacs-lsp-pyright-test-fixture-state wire-log))
                  (expected-terminal
                   (if (equal ,scenario "initialize-error")
                       "initialize-error"
                     "exit")))
             (unless (null errors)
               (error "Pyright fixture errors: %S" errors))
             (unless (and (equal (neomacs-lsp-pyright-test-json
                                  state "terminal")
                                 expected-terminal)
                          (eq (neomacs-lsp-pyright-test-json
                               state "planExhausted") t)
                          (null (neomacs-lsp-pyright-test-json state "misses")))
               (error "Incomplete Pyright fixture plan: %S" state))
             (setq case-completed t)
             (list :result case-result
                   :fixture-errors errors
                   :terminal state
                   :wire-plan
                   (neomacs-lsp-pyright-test-wire-plan wire-log))))
       (unless case-completed
         (ignore-errors (neomacs-lsp-pyright-test-stop-workspace workspace)))
       (advice-remove 'message #'neomacs-lsp-pyright-test-observe-message)
       (advice-remove 'lsp-log #'neomacs-lsp-pyright-test-observe-lsp-log)
       (dolist (buffer (buffer-list))
         (when (or (and source-buffer (eq buffer source-buffer))
                   (and (buffer-file-name buffer)
                        (file-in-directory-p (buffer-file-name buffer) root))
                   (string-prefix-p "*pyright" (buffer-name buffer))
                   (string-prefix-p "*basedpyright" (buffer-name buffer)))
           (ignore-errors (kill-buffer buffer))))
       (set-window-configuration window-configuration)
       (when (buffer-live-p origin-buffer)
         (set-buffer origin-buffer))
       (when (and (file-directory-p root)
                  (file-in-directory-p
                   root
                   (file-name-as-directory
                    (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (delete-directory root t)))))

(defmacro neomacs-lsp-pyright-test-with-npm-install (case-name scenario &rest body)
  "Run BODY with a strict fake npm and package-local server installation."
  (declare (indent 2) (debug t))
  `(let* ((root (neomacs-lsp-pyright-test-root ,case-name))
          (project-root (expand-file-name "install project/" root))
          (source-file (expand-file-name "main.py" project-root))
          (wire-log (expand-file-name "installed-server-wire.jsonl" root))
          (start-log (expand-file-name "installed-server-start.jsonl" root))
          (npm-log (expand-file-name "npm.jsonl" root))
          (template-bin
           (neomacs-lsp-pyright-test-write-server
            (expand-file-name "server template/" root)))
          (server-template
           (expand-file-name
            (concat lsp-pyright-langserver-command "-langserver")
            template-bin))
          (npm-bin (neomacs-lsp-pyright-test-write-fake-npm root))
          (python3 (or (executable-find "python3")
                       (error "python3 is required by the protocol fixture")))
          (lsp--session (make-lsp-session))
          (lsp-session-file nil)
          (lsp-auto-guess-root t)
          (lsp-enable-file-watchers nil)
          (lsp-auto-configure nil)
          (lsp-enable-suggest-server-download nil)
          (lsp-restart 'ignore)
          (lsp--show-message nil)
          (lsp-idle-delay 0.01)
          (lsp-response-timeout 20)
          (lsp-server-install-dir (expand-file-name "server-install/" root))
          (npm-prefix
           (expand-file-name
            (concat "npm/" lsp-pyright-langserver-command "/")
            lsp-server-install-dir))
          (process-environment (copy-sequence process-environment))
          (exec-path (list npm-bin))
          (origin-buffer (current-buffer))
          (window-configuration (current-window-configuration))
          (source-buffer nil)
          (workspace nil)
          (client (gethash 'pyright lsp-clients)))
     (make-directory (expand-file-name ".git/" project-root) t)
     (neomacs-lsp-pyright-test-write-file source-file "answer: int = 42\n")
     (make-symbolic-link python3 (expand-file-name "python3" npm-bin) t)
     (setenv "PATH" npm-bin)
     (setenv "PAGER" "fixture-pager")
     (setenv "NEOMACS_LSP_PYRIGHT_WIRE_LOG" wire-log)
     (setenv "NEOMACS_LSP_PYRIGHT_START_LOG" start-log)
     (setenv "NEOMACS_LSP_PYRIGHT_SCENARIO" "normal")
     (setenv "NEOMACS_LSP_PYRIGHT_PROJECT"
             (directory-file-name (file-truename project-root)))
     (setenv "NEOMACS_LSP_PYRIGHT_SOURCE" (file-truename source-file))
     (setenv "NEOMACS_LSP_PYRIGHT_SECOND_PROJECT"
             (directory-file-name (file-truename project-root)))
     (setenv "NEOMACS_LSP_PYRIGHT_SECOND_SOURCE"
             (file-truename source-file))
     (setenv "NEOMACS_LSP_PYRIGHT_FLAVOR" lsp-pyright-langserver-command)
     (setenv "NEOMACS_LSP_PYRIGHT_NPM_LOG" npm-log)
     (setenv "NEOMACS_LSP_PYRIGHT_NPM_PREFIX"
             (directory-file-name npm-prefix))
     (setenv "NEOMACS_LSP_PYRIGHT_SERVER_TEMPLATE" server-template)
     (setenv "NEOMACS_LSP_PYRIGHT_NPM_SCENARIO" ,scenario)
     (setenv "NEOMACS_LSP_PYRIGHT_TMP_ROOT"
             (file-truename (or (getenv "TMPDIR")
                                (error "TMPDIR must point below ./tmp"))))
     (unwind-protect
         (progn
           (setq source-buffer (find-file-noselect source-file))
           (with-current-buffer source-buffer
             (python-mode)
             ,@body))
       (ignore-errors (neomacs-lsp-pyright-test-stop-workspace workspace))
       (dolist (process (process-list))
         (when (and (bufferp (process-buffer process))
                    (buffer-local-value 'lsp-installation-buffer-mode
                                        (process-buffer process)))
           (when (process-live-p process)
             (delete-process process))
           (accept-process-output process 0.01)))
       (dolist (buffer (buffer-list))
         (when (or (and source-buffer (eq buffer source-buffer))
                   (and (buffer-file-name buffer)
                        (file-in-directory-p (buffer-file-name buffer) root))
                   (buffer-local-value 'lsp-installation-buffer-mode buffer)
                   (string-prefix-p "*pyright" (buffer-name buffer))
                   (string-prefix-p "*basedpyright" (buffer-name buffer)))
           (ignore-errors (kill-buffer buffer))))
       (set-window-configuration window-configuration)
       (when (buffer-live-p origin-buffer)
         (set-buffer origin-buffer))
       (when (and (file-directory-p root)
                  (file-in-directory-p
                   root
                   (file-name-as-directory
                    (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (delete-directory root t)))))
"####;

fn lsp_pyright_oracle(flavor: &str) -> CachedMelpaOracle {
    let flavor = match flavor {
        "pyright" => "pyright",
        "basedpyright" => "basedpyright",
        unexpected => panic!("unsupported LSP Pyright test flavor `{unexpected}`"),
    };
    let prelude =
        format!("(setq lsp-pyright-langserver-command \"{flavor}\")\n{LSP_PYRIGHT_TEST_PRELUDE}");
    CachedMelpaOracle::new(LSP_PYRIGHT_MELPA_PIN, "lsp-pyright.el")
        .expect("prepare revision-pinned LSP Pyright source below ./tmp")
        .with_prelude(prelude)
        .with_timeout(LSP_PYRIGHT_TEST_TIMEOUT)
}

fn current_test_name(flavor: &str) -> String {
    let thread = std::thread::current();
    format!(
        "{}_{flavor}",
        thread.name().unwrap_or("unnamed LSP Pyright parity test")
    )
}

fn assert_lsp_pyright_batch(flavor: &str, cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        lsp_pyright_oracle(flavor),
        &current_test_name(flavor),
        "lsp_pyright_parity",
        cases,
    );
}

#[test]
fn lsp_pyright_package_batch() {
    assert_lsp_pyright_batch("pyright", &workflows::pyright_workflow_batch_cases());
    assert_lsp_pyright_batch(
        "basedpyright",
        &workflows::basedpyright_workflow_batch_cases(),
    );
}
