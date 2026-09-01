use std::time::Duration;

use crate::{CachedMelpaOracle, LSP_JAVA_MELPA_PIN};

pub(crate) use super::batch_support::ParityBatchCase;
use super::batch_support::assert_oracle_batch_cases;

mod workflows;

const LSP_JAVA_TEST_TIMEOUT: Duration = Duration::from_secs(240);

const LSP_JAVA_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)

;; The pinned Treemacs dependency loads the generated YAML grammar as one
;; intentionally deep macro form.  GNU Emacs 31's default expansion depth is
;; lower than that generated source requires, before `lsp-java.el' itself is
;; loaded.
(setq max-lisp-eval-depth 10000)

(defun neomacs-lsp-java-test-root (case-name)
  "Return CASE-NAME's disposable directory below the Rust-owned sandbox."
  (file-name-as-directory
   (expand-file-name case-name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun neomacs-lsp-java-test-write-file (path contents)
  "Write CONTENTS to PATH, creating its parent directory."
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents)))

(defun neomacs-lsp-java-test-write-server (root)
  "Install a deterministic JDT-LS protocol peer below ROOT."
  (let* ((server-root (expand-file-name "jdtls/" root))
         (program (expand-file-name "bin/jdtls" server-root))
         (launcher (expand-file-name
                    "plugins/org.eclipse.equinox.launcher_test.jar"
                    server-root)))
    (neomacs-lsp-java-test-write-file launcher "protocol fixture\n")
    (neomacs-lsp-java-test-write-file
     program
     "#!/usr/bin/env python3
import json
import os
import socket
import sys
import threading
import time
from urllib.parse import unquote, urlparse

log_path = os.environ['NEOMACS_LSP_JAVA_LOG']
dap_log_path = os.environ['NEOMACS_LSP_JAVA_DAP_LOG']
gate_root = os.environ['NEOMACS_LSP_JAVA_GATE']

def record_to(path, message):
    with open(path, 'a', encoding='utf-8') as stream:
        stream.write(json.dumps(message, ensure_ascii=False, sort_keys=True) + '\\n')

def record(message):
    record_to(log_path, message)

def record_dap(message):
    record_to(dap_log_path, message)

def receive_message(stream):
    headers = {}
    while True:
        line = stream.readline()
        if not line:
            return None
        if line in (b'\\r\\n', b'\\n'):
            break
        name, value = line.decode('ascii').split(':', 1)
        headers[name.lower()] = value.strip()
    length = int(headers['content-length'])
    return json.loads(stream.read(length).decode('utf-8'))

def send_message(stream, message):
    body = json.dumps(message, ensure_ascii=False, separators=(',', ':')).encode('utf-8')
    stream.write(b'Content-Length: ' + str(len(body)).encode('ascii') + b'\\r\\n\\r\\n' + body)
    stream.flush()

def receive():
    return receive_message(sys.stdin.buffer)

def send(message):
    send_message(sys.stdout.buffer, message)

def serve_dap(listener):
    connection, _address = listener.accept()
    listener.close()
    stream = connection.makefile('rwb', buffering=0)
    sequence = 1000
    while True:
        message = receive_message(stream)
        if message is None:
            break
        record_dap(message)
        command = message.get('command')
        request_sequence = message.get('seq')
        body = {}
        if command == 'initialize':
            body = {'supportsConfigurationDoneRequest': False}
        elif command == 'launch':
            arguments = message.get('arguments', {})
            parts = arguments.get('args', '').split()
            port = int(parts[parts.index('-port') + 1])
            with socket.create_connection(('127.0.0.1', port)) as junit:
                junit.sendall(
                    b'%TESTS  1,deploysRelease(example.DeploymentServiceTest)\\n'
                    b'%TESTE  1,deploysRelease(example.DeploymentServiceTest)\\n')
                junit.shutdown(socket.SHUT_WR)
            while not os.path.exists(gate_root + '-junit-observed'):
                time.sleep(0.005)
        else:
            body = None
        response = {
            'seq': sequence,
            'type': 'response',
            'request_seq': request_sequence,
            'success': command in ('initialize', 'launch'),
            'command': command
        }
        if body is not None:
            response['body'] = body
        if not response['success']:
            response['message'] = 'unexpected DAP request: ' + str(command)
        send_message(stream, response)
        sequence += 1
        if command == 'launch':
            send_message(stream, {
                'seq': sequence,
                'type': 'event',
                'event': 'initialized',
                'body': {}
            })
            sequence += 1
            send_message(stream, {
                'seq': sequence,
                'type': 'event',
                'event': 'terminated',
                'body': {}
            })
            break
        if not response['success']:
            break
    stream.close()
    connection.close()

while True:
    message = receive()
    if message is None:
        break
    record(message)
    method = message.get('method')
    request_id = message.get('id')
    if method == 'initialize':
        send({'jsonrpc': '2.0', 'id': request_id, 'result': {
            'capabilities': {
                'positionEncoding': 'utf-16',
                'textDocumentSync': 1,
                'codeActionProvider': True,
                'executeCommandProvider': {'commands': [
                    'java.navigate.openTypeHierarchy',
                    'java.navigate.resolveTypeHierarchy'
                ]}
            },
            'serverInfo': {'name': 'neomacs-jdt-fixture', 'version': '1.57.0'}
        }})
    elif method == 'shutdown':
        send({'jsonrpc': '2.0', 'id': request_id, 'result': None})
    elif method == 'exit':
        break
    elif method == 'java/buildWorkspace':
        gate = gate_root + ('-full' if message.get('params') else '-incremental')
        while not os.path.exists(gate):
            time.sleep(0.005)
        send({'jsonrpc': '2.0', 'id': request_id,
              'result': 2 if message.get('params') else 1})
    elif method == 'textDocument/codeAction':
        params = message['params']
        kind = params['context']['only'][0]
        commands = {
            'source.organizeImports': 'java.action.organizeImports',
            'source.generate.toString': 'java.action.generateToStringPrompt'
        }
        if kind in commands:
            action = {
                'title': 'Organize imports' if kind == 'source.organizeImports'
                         else 'Generate toString()',
                'kind': kind,
                'command': {
                    'title': kind,
                    'command': commands[kind],
                    'arguments': [{
                        'textDocument': params['textDocument'],
                        'range': params['range']
                    }]
                }
            }
        else:
            action = None
        send({'jsonrpc': '2.0', 'id': request_id,
              'result': [] if action is None else [action]})
    elif method == 'java/organizeImports':
        uri = message['params']['textDocument']['uri']
        send({'jsonrpc': '2.0', 'id': request_id, 'result': {
            'changes': {uri: [{
                'range': {
                    'start': {'line': 2, 'character': 0},
                    'end': {'line': 4, 'character': 0}
                },
                'newText': ''
            }]}
        }})
    elif method == 'java/checkToStringStatus':
        send({'jsonrpc': '2.0', 'id': request_id, 'result': {
            'exists': False,
            'fields': [
                {'name': 'release', 'type': 'String'},
                {'name': 'region', 'type': 'String'}
            ]
        }})
    elif method == 'java/generateToString':
        uri = message['params']['context']['textDocument']['uri']
        fields = message['params']['fields']
        if fields != [{'name': 'region', 'type': 'String'}]:
            send({'jsonrpc': '2.0', 'id': request_id, 'error': {
                'code': -32602,
                'message': 'unexpected toString fields: ' + repr(fields)
            }})
            raise SystemExit('unexpected toString fields: ' + repr(fields))
        send({'jsonrpc': '2.0', 'id': request_id, 'result': {
            'changes': {uri: [{
                'range': {
                    'start': {'line': 9, 'character': 0},
                    'end': {'line': 9, 'character': 0}
                },
                'newText': '\\n    @Override\\n    public String toString() {\\n        return \"DeploymentService{region=\\'\" + region + \"\\'}\";\\n    }\\n'
            }]}
        }})
    elif method == 'java/findLinks':
        send({'jsonrpc': '2.0', 'id': request_id, 'result': [{
            'uri': 'jdt://contents/java.base/java/lang/Object.class?=java.base/java/lang/Object.class',
            'range': {
                'start': {'line': 1, 'character': 13},
                'end': {'line': 1, 'character': 19}
            }
        }]})
    elif method == 'java/classFileContents':
        send({'jsonrpc': '2.0', 'id': request_id, 'result':
              'package java.lang;\\n'
              'public class Object {\\n'
              '    public String toString() {\\n'
              '        return getClass().getName();\\n'
              '    }\\n'
              '}\\n'})
    elif method == 'workspace/executeCommand':
        params = message.get('params', {})
        command = params.get('command')
        arguments = params.get('arguments', [])
        if command == 'java.navigate.openTypeHierarchy':
            position = json.loads(arguments[0])
            uri = position['textDocument']['uri']
            if os.path.exists(gate_root + '-no-hierarchy'):
                result = None
            else:
                result = {
                    'name': 'DeploymentService',
                    'kind': 5,
                    'uri': uri,
                    'range': {
                        'start': {'line': 4, 'character': 0},
                        'end': {'line': 11, 'character': 1}
                    },
                    'selectionRange': {
                        'start': {'line': 4, 'character': 13},
                        'end': {'line': 4, 'character': 30}
                    }
                }
            send({'jsonrpc': '2.0', 'id': request_id, 'result': result})
        elif command == 'java.navigate.resolveTypeHierarchy':
            item = json.loads(arguments[0])
            uri = item['uri']
            child_uri = uri.replace(
                'DeploymentService.java',
                'RegionalDeploymentService.java')
            result = dict(item)
            result['children'] = [{
                'name': 'RegionalDeploymentService',
                'kind': 5,
                'uri': child_uri,
                'range': {
                    'start': {'line': 2, 'character': 0},
                    'end': {'line': 6, 'character': 1}
                },
                'selectionRange': {
                    'start': {'line': 2, 'character': 13},
                    'end': {'line': 2, 'character': 38}
                }
            }]
            result['parents'] = [{
                'name': 'java.lang.Object',
                'kind': 5,
                'uri': uri,
                'range': {
                    'start': {'line': 4, 'character': 0},
                    'end': {'line': 11, 'character': 1}
                },
                'selectionRange': {
                    'start': {'line': 4, 'character': 13},
                    'end': {'line': 4, 'character': 30}
                }
            }]
            send({'jsonrpc': '2.0', 'id': request_id, 'result': result})
        elif command == 'vscode.java.test.search.items':
            query = json.loads(arguments[0])
            uri = query['uri']
            level = query['level']
            if level == 1:
                test_uri = (uri.rstrip('/') +
                            '/src/test/java/example/DeploymentServiceTest.java')
                result = [{
                    'id': '[engine:junit-jupiter]/[package:example]',
                    'displayName': 'example',
                    'fullName': 'example',
                    'level': 2,
                    'kind': 1,
                    'project': 'deployment-service',
                    'location': {
                        'uri': test_uri,
                        'range': {
                            'start': {'line': 0, 'character': 0},
                            'end': {'line': 0, 'character': 16}
                        }
                    }
                }]
            elif level == 2:
                result = [{
                    'id': '[engine:junit-jupiter]/[class:example.DeploymentServiceTest]',
                    'displayName': 'DeploymentServiceTest',
                    'fullName': 'example.DeploymentServiceTest',
                    'level': 3,
                    'kind': 1,
                    'project': 'deployment-service',
                    'location': {
                        'uri': uri,
                        'range': {
                            'start': {'line': 2, 'character': 13},
                            'end': {'line': 2, 'character': 38}
                        }
                    }
                }]
            else:
                result = [{
                    'id': 'deployment-service@example.DeploymentServiceTest#deploysRelease',
                    'displayName': 'deploysRelease()',
                    'fullName': 'example.DeploymentServiceTest#deploysRelease()',
                    'level': 4,
                    'kind': 1,
                    'project': 'deployment-service',
                    'location': {
                        'uri': uri,
                        'range': {
                            'start': {'line': 3, 'character': 16},
                            'end': {'line': 3, 'character': 30}
                        }
                    }
                }]
            send({'jsonrpc': '2.0', 'id': request_id, 'result': result})
        elif command == 'vscode.java.test.junit.argument':
            query = json.loads(arguments[0])
            test_path = unquote(urlparse(query['uri']).path)
            project_path = test_path.split('/src/test/', 1)[0]
            send({'jsonrpc': '2.0', 'id': request_id, 'result': {
                'workingDirectory': project_path,
                'mainClass': 'org.eclipse.jdt.internal.junit.runner.RemoteTestRunner',
                'projectName': query['project'],
                'classpath': [project_path + '/target/test-classes',
                              project_path + '/target/classes'],
                'modulepath': [],
                'vmArguments': ['-ea', '-Dtest.env=parity'],
                'programArguments': [
                    '-version', '3', '-port', '0',
                    '-test', query['classFullName'] + '#' + query['testName']
                ]
            }})
        elif command == 'vscode.java.startDebugSession':
            listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            listener.bind(('127.0.0.1', 0))
            listener.listen(1)
            port = listener.getsockname()[1]
            threading.Thread(target=serve_dap, args=(listener,), daemon=True).start()
            send({'jsonrpc': '2.0', 'id': request_id, 'result': port})
        else:
            send({'jsonrpc': '2.0', 'id': request_id, 'error': {
                'code': -32601,
                'message': 'unexpected execute command: ' + str(command)
            }})
            raise SystemExit('unexpected execute command: ' + str(command))
    elif request_id is not None:
        send({'jsonrpc': '2.0', 'id': request_id, 'error': {
            'code': -32601,
            'message': 'unexpected request: ' + str(method)
        }})
        raise SystemExit('unexpected request: ' + str(method))
    elif method not in (
            'initialized',
            'workspace/didChangeConfiguration',
            'textDocument/didOpen',
            'textDocument/didChange',
            'textDocument/didClose',
            'java/projectConfigurationUpdate'):
        raise SystemExit('unexpected notification: ' + str(method))
")
    (set-file-modes program #o755)
    server-root))

(defun neomacs-lsp-java-test-wait (predicate description)
  "Drive subprocesses until PREDICATE succeeds or signal DESCRIPTION."
  (let ((deadline (+ (float-time) 20.0)))
    (while (and (not (funcall predicate))
                (< (float-time) deadline))
      (accept-process-output nil 0.02))
    (unless (funcall predicate)
      (error "Timed out waiting for %s" description))))

(defun neomacs-lsp-java-test-read-wire-log (path)
  "Read complete JSON messages recorded at PATH."
  (when (file-readable-p path)
    (with-temp-buffer
      (insert-file-contents path)
      (goto-char (point-min))
      (let (messages)
        (while (not (eobp))
          (push (json-parse-string
                 (buffer-substring-no-properties
                  (line-beginning-position) (line-end-position))
                 :object-type 'alist
                 :array-type 'list
                 :null-object nil
                 :false-object :json-false)
                messages)
          (forward-line 1))
        (nreverse messages)))))

(defun neomacs-lsp-java-test-json (object key)
  "Return KEY from JSON alist OBJECT."
  (alist-get key object nil nil #'string=))

(defun neomacs-lsp-java-test-wire-messages-by-method (path method)
  "Return messages from PATH whose JSON-RPC method equals METHOD."
  (seq-filter
   (lambda (message)
     (equal (neomacs-lsp-java-test-json message "method") method))
   (neomacs-lsp-java-test-read-wire-log path)))

(defun neomacs-lsp-java-test-document-uri (params)
  "Return the text-document URI nested in PARAMS."
  (neomacs-lsp-java-test-json
   (neomacs-lsp-java-test-json params "textDocument") "uri"))

(defun neomacs-lsp-java-test-position (position)
  "Return POSITION as an exact zero-based line/character pair."
  (list (neomacs-lsp-java-test-json position "line")
        (neomacs-lsp-java-test-json position "character")))

(defun neomacs-lsp-java-test-range (range)
  "Return RANGE as exact zero-based start/end position pairs."
  (list
   (neomacs-lsp-java-test-position
    (neomacs-lsp-java-test-json range "start"))
   (neomacs-lsp-java-test-position
    (neomacs-lsp-java-test-json range "end"))))

(defun neomacs-lsp-java-test-context-summary (context root)
  "Return CONTEXT's normalized document URI and exact range below ROOT."
  (list
   :uri (neomacs-lsp-java-test-normalize-uri
         (neomacs-lsp-java-test-document-uri context) root)
   :range (neomacs-lsp-java-test-range
           (neomacs-lsp-java-test-json context "range"))))

(defun neomacs-lsp-java-test-new-idle-timers (before)
  "Return idle timers not present in the identity set BEFORE."
  (let (new-timers)
    (dolist (timer timer-idle-list (nreverse new-timers))
      (unless (memq timer before)
        (push timer new-timers)))))

(defun neomacs-lsp-java-test-normalize-uri (uri root)
  "Express file URI URI relative to ROOT."
  (file-relative-name (lsp--uri-to-path uri) root))

(defmacro neomacs-lsp-java-test-with-project (case-name &rest body)
  "Run BODY in a real Java project against an isolated JDT protocol peer."
  (declare (indent 1) (debug t))
  `(let* ((root (neomacs-lsp-java-test-root ,case-name))
          (project-root (expand-file-name "deployment-service/" root))
          (source-file (expand-file-name
                        "src/main/java/example/DeploymentService.java"
                        project-root))
          (child-file (expand-file-name
                       "src/main/java/example/RegionalDeploymentService.java"
                       project-root))
          (test-file (expand-file-name
                      "src/test/java/example/DeploymentServiceTest.java"
                      project-root))
          (pom-file (expand-file-name "pom.xml" project-root))
          (wire-log (expand-file-name "jdt-wire.jsonl" root))
          (dap-log (expand-file-name "dap-wire.jsonl" root))
          (gate-root (expand-file-name "jdt-gate" root))
          (server-root (neomacs-lsp-java-test-write-server root))
          (lsp--session (make-lsp-session))
          (lsp-session-file nil)
          (lsp-auto-guess-root t)
          (lsp-enable-file-watchers nil)
          (lsp-auto-configure nil)
          (lsp-enable-suggest-server-download nil)
          (lsp-restart 'ignore)
          (lsp-java-server-install-dir server-root)
          (lsp-java-jdt-ls-prefer-native-command t)
          (lsp-java-jdt-ls-command "jdtls")
          (lsp-java-workspace-dir (expand-file-name "workspace/" root))
          (lsp-java-workspace-cache-dir (expand-file-name "cache/" root))
          (enable-dir-local-variables nil)
          (process-environment (copy-sequence process-environment))
          (origin-buffer (current-buffer))
          (window-configuration (current-window-configuration))
          (source-buffer nil)
          (workspace nil))
     (make-directory (expand-file-name ".git/" project-root) t)
     (neomacs-lsp-java-test-write-file
      pom-file
     "<project><modelVersion>4.0.0</modelVersion><groupId>example</groupId><artifactId>deployment-service</artifactId><version>1.0</version></project>\n")
     (neomacs-lsp-java-test-write-file
      source-file
     "package example;\n\nimport java.util.List;\n\npublic class DeploymentService {\n    private String release;\n    private String region;\n\n    public String deploy(String release) {\n        return \"ready:\" + release;\n    }\n}\n")
     (neomacs-lsp-java-test-write-file
      child-file
      "package example;\n\npublic class RegionalDeploymentService extends DeploymentService {\n    public String region() {\n        return \"north\";\n    }\n}\n")
     (neomacs-lsp-java-test-write-file
      test-file
      "package example;\n\npublic class DeploymentServiceTest {\n    public void deploysRelease() {\n        String result = new DeploymentService().deploy(\"release-42\");\n        if (!result.equals(\"ready:release-42\")) throw new AssertionError(result);\n    }\n}\n")
     (setenv "NEOMACS_LSP_JAVA_LOG" wire-log)
     (setenv "NEOMACS_LSP_JAVA_DAP_LOG" dap-log)
     (setenv "NEOMACS_LSP_JAVA_GATE" gate-root)
     (unwind-protect
         (progn
           (setq source-buffer (find-file-noselect source-file))
           (with-current-buffer source-buffer
             (java-mode)
             (lsp)
             (neomacs-lsp-java-test-wait
              (lambda ()
                (and (setq workspace (lsp-find-workspace 'jdtls source-file))
                     (eq (lsp--workspace-status workspace) 'initialized)
                     lsp-mode))
              "the public lsp-java workspace to initialize")
             ,@body))
       (when (and workspace
                  (process-live-p (lsp--workspace-proc workspace)))
         (ignore-errors (lsp-workspace-shutdown workspace))
         (neomacs-lsp-java-test-wait
          (lambda () (not (process-live-p (lsp--workspace-proc workspace))))
          "the JDT-LS process to stop"))
       (dolist (buffer (buffer-list))
         (when (or (and source-buffer (eq buffer source-buffer))
                   (and (buffer-file-name buffer)
                        (file-in-directory-p (buffer-file-name buffer)
                                             project-root))
                   (string-prefix-p "*jdtls" (buffer-name buffer))
                   (string-prefix-p "*lsp-java" (buffer-name buffer)))
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

fn lsp_java_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LSP_JAVA_MELPA_PIN, "lsp-java.el")
        .expect("prepare revision-pinned LSP Java source below ./tmp")
        .with_prelude(LSP_JAVA_TEST_PRELUDE)
        .with_timeout(LSP_JAVA_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed LSP Java parity test")
        .into()
}

pub(crate) fn assert_lsp_java_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        lsp_java_oracle(),
        &current_test_name(),
        "lsp_java_parity",
        cases,
    );
}

#[test]
fn lsp_java_package_batch() {
    assert_lsp_java_batch(&workflows::workflow_batch_cases());
}
