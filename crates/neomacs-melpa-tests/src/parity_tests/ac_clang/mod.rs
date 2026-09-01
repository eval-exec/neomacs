use std::time::Duration;

use crate::{AC_CLANG_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_CLANG_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// ac-clang is a client for an out-of-process `clang-server' binary: the
/// package launches it with `start-process', frames every request as
/// `PacketSize:N\n<plist>' and parses replies terminated by `$'.  That binary
/// embeds libclang and is not available here, so it is the one boundary the
/// workflows fake: they install a recording stand-in on `exec-path' which
/// writes the exact argument vector and the exact request bytes it receives
/// into the sandbox and answers with realistic canned server output.
/// Everything above the pipe — session management, packet encoding, response
/// decoding, candidate construction, auto-complete rendering, yasnippet
/// template expansion and jump handling — is the package's own real code.
const AC_CLANG_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'auto-complete)

(defvar ac-clang-test-root
  (file-name-as-directory
   (expand-file-name "clang" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defvar ac-clang-test-bin
  (file-name-as-directory (expand-file-name "bin" ac-clang-test-root)))

(defvar ac-clang-test-requests
  (file-name-as-directory (expand-file-name "requests" ac-clang-test-root)))

(defvar ac-clang-test-responses
  (file-name-as-directory (expand-file-name "responses" ac-clang-test-root)))

(make-directory ac-clang-test-requests t)
(make-directory ac-clang-test-responses t)

(defmacro ac-clang-test-workflow (&rest body)
  "Run BODY with the printer settings a user's Emacs actually has.
The parity harness binds `print-escape-newlines' and friends so it can print
its own result on one line, but ac-clang serializes every request packet with
`format \"%S\"' and derives `PacketSize' from that string, so the package has
to be observed under the default settings instead of the harness's."
  `(let ((print-circle nil)
         (print-escape-newlines nil)
         (print-escape-control-characters nil))
     ,@body))

(defun ac-clang-test-write (path text)
  "Write TEXT to PATH as UTF-8 and return PATH."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun ac-clang-test-use-bin (directory)
  "Make DIRECTORY the only `exec-path' entry and return it."
  (let ((directory (file-name-as-directory
                    (expand-file-name directory ac-clang-test-root))))
    (make-directory directory t)
    (setq exec-path (list (directory-file-name directory)))
    directory))

(defun ac-clang-test-install-server (&optional version directory)
  "Install a recording stand-in `clang-server' in DIRECTORY.
It answers `--version' with VERSION, records its argument vector and every
`PacketSize' framed request below `ac-clang-test-requests', and replies to a
request named COMMAND with the contents of `ac-clang-test-responses'/COMMAND
\(or COMMAND.N for the Nth occurrence) followed by the `$' terminator."
  (let* ((directory (ac-clang-test-use-bin (or directory ac-clang-test-bin)))
         (path (expand-file-name "clang-server" directory)))
    (ac-clang-test-write
     path
     (concat
      "#!/bin/sh\n"
      "root=" ac-clang-test-root "\n"
      "requests=" ac-clang-test-requests "\n"
      "responses=" ac-clang-test-responses "\n"
      "record() {\n"
      "  n=1\n"
      "  [ -f \"$root/.total\" ] && n=$(($(cat \"$root/.total\") + 1))\n"
      "  printf '%s' \"$n\" > \"$root/.total\"\n"
      "  record_file=$(printf '%s%02d-%s' \"$requests\" \"$n\" \"$1\")\n"
      "}\n"
      "if [ \"$1\" = --version ]; then\n"
      "  record VERSION\n"
      "  printf 'clang-server version " (or version "2.1.3") "\\n' > \"$record_file\"\n"
      "  echo 'clang-server version " (or version "2.1.3") "'\n"
      "  exit 0\n"
      "fi\n"
      "record LAUNCH\n"
      "printf '%s\\n' \"$@\" > \"$record_file\"\n"
      "while IFS= read -r header; do\n"
      "  case $header in\n"
      "    PacketSize:*) size=${header#PacketSize:} ;;\n"
      "    *) continue ;;\n"
      "  esac\n"
      "  packet=$(dd bs=1 count=\"$size\" 2>/dev/null)\n"
      "  first=$(printf '%s\\n' \"$packet\" | head -n 1)\n"
      "  id=$(printf '%s\\n' \"$first\" | sed -n 's/^(:RequestId \\([0-9][0-9]*\\).*/\\1/p')\n"
      "  name=$(printf '%s\\n' \"$first\" | sed -n 's/.*:CommandName \"\\([A-Z_]*\\)\".*/\\1/p')\n"
      "  seq=1\n"
      "  counter=$root/.seq.$name\n"
      "  [ -f \"$counter\" ] && seq=$(($(cat \"$counter\") + 1))\n"
      "  printf '%s' \"$seq\" > \"$counter\"\n"
      "  record \"$name\"\n"
      "  { printf '%s\\n' \"$header\"; printf '%s' \"$packet\"; } > \"$record_file\"\n"
      "  reply=$responses/$name.$seq\n"
      "  [ -f \"$reply\" ] || reply=$responses/$name\n"
      "  [ -f \"$reply\" ] && printf '%s$' \"$(sed \"s/@REQUESTID@/$id/g\" \"$reply\")\"\n"
      "done\n"
      "exit 0\n"))
    (set-file-modes path #o755)
    path))

(defun ac-clang-test-reply (command text)
  "Make the stand-in server answer COMMAND with TEXT."
  (ac-clang-test-write (expand-file-name command ac-clang-test-responses) text))

(defun ac-clang-test-file-bytes (path)
  "Return the exact bytes of PATH as a unibyte string."
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (buffer-string)))

(defun ac-clang-test-describe-record (raw)
  "Describe one event the stand-in server recorded.
A `PacketSize' framed request is reported as the outcome of checking the
declared size against the real body length — the size itself is the byte
length of a packet quoting the sandbox path, so only the invariant is
portable — followed by the packet text.  Anything else is reported verbatim."
  (if (string-match "\\`PacketSize:\\([0-9]+\\)\n" raw)
      (let ((size (string-to-number (match-string 1 raw)))
            (body (substring raw (match-end 0))))
        (list (if (= size (length body))
                  'packet-size-matches-body
                (list 'packet-size-mismatch size (length body)))
              (decode-coding-string body 'utf-8)))
    (decode-coding-string raw 'utf-8)))

(defun ac-clang-test-recorded ()
  "Return every record the stand-in server wrote, in the order it saw them."
  (mapcar (lambda (file)
            (cons (file-name-nondirectory file)
                  (ac-clang-test-describe-record
                   (ac-clang-test-file-bytes file))))
          (sort (directory-files ac-clang-test-requests t "\\`[0-9]")
                #'string<)))

(defun ac-clang-test-recorded-count ()
  (length (directory-files ac-clang-test-requests nil "\\`[0-9]")))

(defun ac-clang-test-wait (predicate)
  "Pump clang-server output until PREDICATE returns non-nil.
Returns that value, or nil once the deadline passes, so an asynchronous
workflow that never completes fails on its expectation instead of hanging."
  (let ((process (get-process "Clang-Server"))
        (deadline (+ (float-time) 15.0))
        value)
    (while (and (not (setq value (funcall predicate)))
                (< (float-time) deadline))
      (accept-process-output process 0.05))
    value))

(defun ac-clang-test-wait-records (n)
  "Wait until the stand-in server has recorded at least N events."
  (ac-clang-test-wait (lambda () (>= (ac-clang-test-recorded-count) n))))

(defun ac-clang-test-messages (regexp)
  "Return the echo-area lines matching REGEXP, in order."
  (with-current-buffer (get-buffer-create "*Messages*")
    (cl-remove-if-not
     (lambda (line) (string-match-p regexp line))
     (split-string
      (buffer-substring-no-properties (point-min) (point-max)) "\n" t))))

(defun ac-clang-test-warnings ()
  "Return the `clang-server' warning lines the user would see."
  (let ((buffer (get-buffer "*Warnings*")))
    (and buffer
         (with-current-buffer buffer
           (cl-remove-if-not
            (lambda (line) (string-match-p "clang-server" line))
            (split-string
             (buffer-substring-no-properties (point-min) (point-max)) "\n" t))))))

(defun ac-clang-test-candidate-details (candidates)
  "Return the name, `:detail' and `:indices' ac-clang attached to CANDIDATES."
  (mapcar (lambda (candidate)
            (list (substring-no-properties candidate)
                  (get-text-property 0 :detail candidate)
                  (get-text-property 0 :indices candidate)))
          candidates))

(defconst ac-clang-test-widget-header
  (concat "#pragma once\n"
          "#include <string>\n"
          "\n"
          "namespace ui {\n"
          "class Widget {\n"
          "public:\n"
          "    int area(int scale) const;\n"
          "};\n"
          "}\n"))

(defun ac-clang-test-open (relative text)
  "Write TEXT to RELATIVE below the sandbox and visit it in the live window."
  (let ((buffer (find-file-noselect
                 (ac-clang-test-write
                  (expand-file-name relative ac-clang-test-root) text))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defun ac-clang-test-here ()
  "Return the buffer, line and column the user is looking at."
  (with-current-buffer (window-buffer (selected-window))
    (list :window-buffer (buffer-name)
          :line (line-number-at-pos)
          :column (current-column)
          :text (buffer-substring-no-properties
                 (line-beginning-position) (line-end-position)))))
"##;

fn ac_clang_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_CLANG_MELPA_PIN, "ac-clang.el")
        .expect("prepare pinned ac-clang source below ./tmp")
        .with_prelude(AC_CLANG_TEST_PRELUDE)
        .with_timeout(AC_CLANG_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-clang parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_clang_parity` cases (2a).
pub(crate) fn assert_ac_clang_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_clang_oracle(), &name, "ac_clang_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_clang_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_clang_batch(&cases);
}

// END generated package batch tests
