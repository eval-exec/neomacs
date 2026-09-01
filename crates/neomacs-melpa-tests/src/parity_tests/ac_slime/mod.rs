use std::time::Duration;

use crate::{AC_SLIME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_SLIME_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Fixtures shared by the workflows.
///
/// ac-slime is an auto-complete source backed by a *live* SLIME connection, so
/// the workflows run one.  There is no Common Lisp on the test machine, and the
/// external boundary here is the Lisp itself, so the prelude starts a real
/// swank server -- `make-network-process' with `:server t', speaking the real
/// framed protocol (six hex length digits followed by an s-expression) -- and
/// connects to it with `slime-connect', exactly as a user connects to a
/// remote Lisp.  It answers `swank:connection-info', `swank:swank-require',
/// `swank-repl:create-repl', `swank:simple-completions',
/// `swank:fuzzy-completions' and `swank:documentation-symbol', and records
/// every request it receives.
///
/// Everything above that socket is real: slime's handshake, contrib loading,
/// REPL creation, `slime-eval' round trips, `slime-symbol-start-pos',
/// auto-complete's prefix detection, candidate propertizing, popup
/// documentation and insertion, and of course ac-slime itself.  No ac-slime
/// function is stubbed.
///
/// The contrib order matters: `slime-fuzzy' pulls in `slime-repl', and
/// `slime--setup-contribs' skips the initialiser of any contrib that is
/// already `featurep', so `slime-repl' has to come first or its REPL buffer
/// is never created.
const AC_SLIME_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'auto-complete)

(defvar acs-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar acs-test-requests nil
  "Every swank request the stand-in Lisp received, in order.")

(defvar acs-test-server nil)

;;; The stand-in Lisp: a real swank server, in-process.

(defun acs-test-encode (sexp)
  (let* ((payload (encode-coding-string (concat (prin1-to-string sexp) "\n")
                                        'utf-8-unix)))
    (concat (format "%06x" (length payload)) payload)))

(defconst acs-test-completions
  '(("ca" . ("car" "cadr" "case" "catch"))
    ("str" . ("string" "string=" "stringp"))
    ("zzz" . ())))

(defconst acs-test-docstrings
  '(("car" . "Return the car of LIST.  Signals TYPE-ERROR otherwise.")
    ("case" . "CASE keyform {({key | (key*)} form*)}*")))

(defun acs-test-fuzzy-flags (name)
  (cond ((string-suffix-p "p" name) "-f----")
        ((member name '("case" "catch")) "-m----")
        (t "-f--e-")))

(defun acs-test-answer (form)
  "Return the value the stand-in Lisp answers FORM with."
  (let ((head (car form))
        (args (cdr form)))
    (cond
     ((eq head 'swank:connection-info)
      (list :pid 4242
            :style :spawn
            :encoding (list :coding-systems '("utf-8-unix"))
            :lisp-implementation (list :type "SBCL" :name "sbcl"
                                       :version "2.4.0" :program nil)
            :machine (list :instance "melpa-host" :type "X86-64"
                           :version "Linux")
            :features '(:swank :sbcl)
            :modules '("SWANK-REPL")
            :package (list :name "COMMON-LISP-USER" :prompt "CL-USER")
            :version slime-protocol-version))
     ((eq head 'swank:swank-require)
      '("SWANK-REPL" "SWANK-FUZZY"))
     ((eq head 'swank-repl:create-repl)
      '("COMMON-LISP-USER" "CL-USER"))
     ((eq head 'swank:simple-completions)
      ;; Common Lisp symbols are case insensitive, so a real swank matches
      ;; "CA" and "ca" alike.
      (let ((matches (cdr (assoc (downcase (car args)) acs-test-completions))))
        (list matches (car args))))
     ((eq head 'swank:fuzzy-completions)
      (let ((matches (cdr (assoc (downcase (car args)) acs-test-completions))))
        (list (let ((score 100.0))
                (mapcar (lambda (name)
                          (setq score (- score 7.5))
                          (list name score (list (list 0 (car args)))
                                (acs-test-fuzzy-flags name)))
                        matches))
              nil)))
     ((eq head 'swank:documentation-symbol)
      (or (cdr (assoc (car args) acs-test-docstrings))
          "Not documented."))
     (t nil))))

(defun acs-test-server-filter (process string)
  (let ((pending (concat (or (process-get process 'acs-pending) "") string)))
    (while (and (>= (length pending) 6)
                (>= (- (length pending) 6)
                    (string-to-number (substring pending 0 6) 16)))
      (let* ((length (string-to-number (substring pending 0 6) 16))
             (payload (decode-coding-string (substring pending 6 (+ 6 length))
                                            'utf-8-unix))
             (message (car (read-from-string payload))))
        (setq pending (substring pending (+ 6 length)))
        (push (prin1-to-string message) acs-test-requests)
        (when (eq (car message) :emacs-rex)
          (let ((form (nth 1 message))
                (id (nth 4 message)))
            (process-send-string
             process
             (acs-test-encode
              (list :return (list :ok (acs-test-answer form)) id)))))))
    (process-put process 'acs-pending pending)))

(defun acs-test-start-swank ()
  "Start the stand-in swank server and return its port."
  (setq acs-test-requests nil)
  (setq acs-test-server
        (make-network-process :name "acs-swank"
                              :server t
                              :host 'local
                              :service t
                              :family 'ipv4
                              :coding 'binary
                              :filter #'acs-test-server-filter))
  (process-contact acs-test-server :service))

(defun acs-test-swank-requests ()
  (reverse acs-test-requests))

(defun acs-test-lisp-buffer (text)
  "Create a displayed lisp-mode buffer holding TEXT, point at its end."
  (let ((buffer (generate-new-buffer "*acs-lisp*")))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (lisp-mode)
    (insert text)
    buffer))

(defun acs-test-complete ()
  (auto-complete-mode 1)
  (ac-start :force-init t)
  (ac-update t))

(defun acs-test-candidates ()
  (mapcar #'substring-no-properties ac-candidates))

(defun acs-test-summaries ()
  (mapcar (lambda (candidate)
            (list (substring-no-properties candidate)
                  (get-text-property 0 'summary candidate)
                  (get-text-property 0 'symbol candidate)))
          ac-candidates))

(defun acs-test-line ()
  (buffer-substring-no-properties (line-beginning-position)
                                  (line-end-position)))

(defun acs-test-connect ()
  "Start the stand-in Lisp and connect SLIME to it the way a user does."
  (setq slime-contribs '(slime-repl slime-fuzzy)
        slime-words-of-encouragement '("Test")
        slime-show-words-of-encouragement nil
        slime-auto-select-connection 'always
        slime-kill-without-query-p t)
  (let ((port (acs-test-start-swank)))
    (slime-connect "127.0.0.1" port)
    (let ((deadline (+ (float-time) 20)))
      (while (and (< (float-time) deadline)
                  (not (and (slime-connected-p)
                            (slime-lisp-implementation-name))))
        (accept-process-output nil 0.05)))
    (slime-connection)))
"##;

fn ac_slime_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_SLIME_MELPA_PIN, "ac-slime.el")
        .expect("prepare pinned ac-slime source below ./tmp")
        .with_prelude(AC_SLIME_TEST_PRELUDE)
        .with_timeout(AC_SLIME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-slime parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_slime_parity` cases (2a).
pub(crate) fn assert_ac_slime_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_slime_oracle(), &name, "ac_slime_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_slime_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_slime_batch(&cases);
}

// END generated package batch tests
