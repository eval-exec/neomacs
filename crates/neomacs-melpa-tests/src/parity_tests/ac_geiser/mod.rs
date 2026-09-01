use std::time::Duration;

use crate::{AC_GEISER_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_GEISER_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Fixtures shared by the workflows.
///
/// ac-geiser is an auto-complete source backed by a *live* Geiser REPL, so the
/// workflows run a real one.  Geiser core ships no Scheme implementation, so
/// the prelude registers one with geiser's public
/// `define-geiser-implementation' extension point and points it at a stand-in
/// Scheme: a small shell program that prints a banner and a prompt, records
/// every request it receives, and answers each with the retort its reply table
/// associates with it.  Everything above that boundary is real -- geiser's
/// connection, transaction queue, request marshalling, retort parsing,
/// completion, autodoc and documentation rendering, auto-complete's prefix
/// detection, candidate propertizing and insertion, and of course ac-geiser
/// itself.  Nothing in ac-geiser is stubbed.
///
/// Geiser's own autodoc is turned off (`geiser-mode-autodoc-p' and
/// `geiser-repl-autodoc-p' are public options) so that idle eldoc traffic
/// cannot race the request log: every recorded request is then one the package
/// itself caused.
const AC_GEISER_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)
(require 'auto-complete)
(require 'geiser)
(require 'geiser-impl)
(require 'geiser-repl)

(defvar acg-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar acg-test-binary (expand-file-name "bin/fake-scheme" acg-test-root))
(defvar acg-test-log (expand-file-name "requests.log" acg-test-root))
(defvar acg-test-table (expand-file-name "replies.tsv" acg-test-root))

(defconst acg-test-scheme-program
  "#!/bin/sh
# A minimal Scheme REPL speaking just enough of the Geiser wire protocol:
# it prints a banner and a prompt, records every request, and answers each
# one with the retort the reply table associates with it.
printf 'Fake Scheme 1.0\\n'
printf 'fake@(fake-user)> '
while IFS= read -r line; do
  printf '%s\\n' \"$line\" >> \"$GEISER_LOG\"
  awk -F'\\t' -v request=\"$line\" '
    index(request, $1) > 0 && !done { print $2; done = 1 }
    END { if (!done) print \"((result \\\"()\\\") (output . \\\"\\\"))\" }
  ' \"$GEISER_TABLE\"
  printf 'fake@(fake-user)> '
done
")

(defun acg-test-retort (datum)
  "Return the wire form of a Geiser retort carrying DATUM."
  (format "((result %S) (output . \"\"))" (prin1-to-string datum)))

(defconst acg-test-replies
  ;; PATTERN in the request . DATUM the Scheme side answers with.  A request
  ;; matching nothing here is answered with the empty list, which is what a
  ;; Scheme says when it knows nothing about a symbol.
  '(("geiser:completions \"ca\"" . ("call-with-values" "car" "case" "cadr"))
    ("geiser:completions \"str\"" . ("string-append" "string->list"
                                     "string-length"))
    ("geiser:completions \"zzz\"" . ())
    ("geiser:module-completions" . ("(ice-9 popen)" "(ice-9 rdelim)"))
    ("geiser:symbol-documentation (quote car)"
     . (("signature" car ("args" (("required" pair)
                                  ("optional")
                                  ("key"))))
        ("docstring" . "Return the contents of the car of PAIR.")))
    ("geiser:symbol-documentation (quote cadr)"
     . (("signature" cadr ("args" (("required" pair)
                                   ("optional")
                                   ("key"))))))
    ("geiser:symbol-documentation (quote case)"
     . (("signature" case ("args" (("required" key clauses)
                                   ("optional")
                                   ("key"))))
        ("docstring" . "Evaluate the clause whose datum matches KEY.")))))

(defun acg-test-write-file (path text)
  (make-directory (file-name-directory path) t)
  (with-temp-buffer
    (insert text)
    (write-region (point-min) (point-max) path nil 'silent))
  path)

(defun acg-test-install-scheme ()
  "Install the stand-in Scheme and its reply table."
  (acg-test-write-file acg-test-binary acg-test-scheme-program)
  (set-file-modes acg-test-binary #o755)
  (acg-test-write-file
   acg-test-table
   (mapconcat (lambda (entry)
                (concat (car entry) "\t" (acg-test-retort (cdr entry))))
              acg-test-replies
              "\n"))
  (setenv "GEISER_LOG" acg-test-log)
  (setenv "GEISER_TABLE" acg-test-table)
  acg-test-binary)

(defvar geiser-fake--binary nil)
(defvar geiser-fake--arglist '())
(defvar geiser-fake--prompt-regexp "fake@([^)]*)> ")
(defvar geiser-fake--binding-forms '(let let* lambda))

(defun geiser-fake--procedure (proc &rest args)
  (cond ((memq proc '(eval compile))
         (format "(geiser:eval %s %s)" (or (car args) "#f") (cadr args)))
        (t (format "(geiser:%s %s)" proc
                   (mapconcat #'identity args " ")))))

(defun geiser-fake--find-module (&optional _module) :f)

(define-geiser-implementation fake
  (binary geiser-fake--binary)
  (arglist geiser-fake--arglist)
  (prompt-regexp geiser-fake--prompt-regexp)
  (marshall-procedure geiser-fake--procedure)
  (find-module geiser-fake--find-module)
  (binding-forms geiser-fake--binding-forms)
  (exit-command "(exit)")
  (case-sensitive t))

;; Real implementation packages end their file with this; geiser `require's
;; the feature before starting a REPL.
(provide 'geiser-fake)

(defun acg-test-requests ()
  "Return the requests the package caused, in order.

Geiser's own startup traffic (`add-to-load-path', whose argument is the
project directory) is dropped: it is not sent on behalf of ac-geiser."
  (if (file-exists-p acg-test-log)
      (with-temp-buffer
        (insert-file-contents acg-test-log)
        (seq-remove (lambda (line) (string-search "add-to-load-path" line))
                    (split-string (buffer-string) "\n" t)))
    'no-request))

(defun acg-test-configure ()
  "Install the stand-in Scheme and make it the only known implementation."
  (setq geiser-fake--binary (acg-test-install-scheme))
  (setq geiser-active-implementations '(fake)
        geiser-repl-query-on-kill-p nil
        geiser-repl-skip-version-check-p t
        geiser-repl-startup-time 10000
        geiser-mode-start-repl-p nil
        geiser-repl-autodoc-p nil
        geiser-mode-autodoc-p nil))

(defun acg-test-start-repl ()
  "Start the stand-in Scheme REPL and return its buffer, current."
  (acg-test-configure)
  (geiser-fake)
  (current-buffer))

(defun acg-test-scheme-buffer (text)
  "Create a displayed scheme-mode buffer holding TEXT."
  (let ((buffer (generate-new-buffer "*acg-scheme*")))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (scheme-mode)
    (insert text)
    buffer))

(defun acg-test-complete ()
  "Run auto-complete at point the way typing would."
  (auto-complete-mode 1)
  (ac-start :force-init t)
  (ac-update t))

(defun acg-test-candidates ()
  (mapcar #'substring-no-properties ac-candidates))

(defun acg-test-candidate (name)
  (car (seq-filter (lambda (candidate)
                     (equal (substring-no-properties candidate) name))
                   ac-candidates)))

(defun acg-test-line ()
  (buffer-substring-no-properties (line-beginning-position)
                                  (line-end-position)))
"##;

fn ac_geiser_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_GEISER_MELPA_PIN, "ac-geiser.el")
        .expect("prepare pinned ac-geiser source below ./tmp")
        .with_prelude(AC_GEISER_TEST_PRELUDE)
        .with_timeout(AC_GEISER_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-geiser parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_geiser_parity` cases (2a).
pub(crate) fn assert_ac_geiser_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_geiser_oracle(), &name, "ac_geiser_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_geiser_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_geiser_batch(&cases);
}

// END generated package batch tests
