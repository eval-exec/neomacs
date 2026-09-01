use std::time::Duration;

use crate::{AC_SLY_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_SLY_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// ac-sly is an auto-complete source backed by a *live* SLY connection, so the
/// workflows run one.  There is no Common Lisp on the test machine, and the
/// Lisp is the external boundary here, so the prelude starts a stand-in slynk
/// server in process -- `make-network-process' with `:server t', speaking the
/// real framed protocol (six hex length digits followed by an s-expression) --
/// and connects to it with the real `sly-connect', exactly as a user connects
/// to a remote Lisp.  It answers `slynk:connection-info',
/// `slynk:slynk-add-load-paths', `slynk:slynk-require',
/// `slynk-mrepl:create-mrepl' (following it with the listener channel's
/// `:prompt' message, so the REPL really gets a prompt),
/// `slynk-completion:simple-completions', `slynk-completion:flex-completions'
/// and `slynk:documentation-symbol', and records every request it receives.
///
/// It also models the one thing a real slynk image cannot do: a form in a
/// package the image does not have -- anything but `slynk...' here -- comes
/// back as `(:abort "READER-ERROR...")', which is what a Lisp with no SWANK
/// package answers.  That matters because ac-sly is a translation of ac-slime
/// and its documentation function still asks for `swank:documentation-symbol'.
///
/// Everything above the socket is real: SLY's handshake, contrib loading, REPL
/// creation, `sly-eval' round trips, `sly-symbol-start-pos', auto-complete's
/// prefix detection, candidate propertizing and insertion, and ac-sly itself.
/// No ac-sly function is stubbed.
const AC_SLY_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar ac-sly-test-requests nil
  "Every RPC form the stand-in Lisp received, newest first.")

(defvar ac-sly-test-server nil)

(defvar ac-sly-test-listener-channel nil)

(defvar ac-sly-test-contribs nil
  "Contribs the next `ac-sly-test-connect' should activate.")

(defconst ac-sly-test-completions
  '(("ca" . ("car" "cadr" "case" "catch"))
    ("str" . ("string" "string=" "stringp"))
    ("zzz" . ()))
  "What the stand-in Lisp's completer knows, in the order it reports it.")

(defconst ac-sly-test-docstrings
  '(("car" . "Return the car of LIST.  Signals TYPE-ERROR otherwise.")))

(defun ac-sly-test-encode (sexp)
  (let ((payload (encode-coding-string (concat (prin1-to-string sexp) "\n")
                                       'utf-8-unix)))
    (concat (format "%06x" (length payload)) payload)))

(defun ac-sly-test-answer (form)
  "Return the value the stand-in Lisp answers FORM with.
The symbol `:no-such-package' means the image cannot even read the form."
  (let ((head (car form))
        (args (cdr form)))
    (cond
     ((eq head 'slynk:connection-info)
      (list :pid 4242
            :style :spawn
            :encoding (list :coding-systems '("utf-8-unix"))
            :lisp-implementation (list :type "SBCL" :name "sbcl"
                                       :version "2.4.0" :program nil)
            :machine (list :instance "melpa-host" :type "X86-64"
                           :version "Linux")
            :features '(:slynk :sbcl)
            :modules '("SLYNK-BASE")
            :package (list :name "COMMON-LISP-USER" :prompt "CL-USER")
            :version sly-protocol-version))
     ((eq head 'slynk:slynk-add-load-paths) nil)
     ((eq head 'slynk:slynk-require)
      (let ((names (if (eq (car-safe (car args)) 'quote)
                       (cadr (car args))
                     (car args))))
        (list (append '("SLYNK-BASE") names) names)))
     ((eq head 'slynk-mrepl:create-mrepl)
      (setq ac-sly-test-listener-channel (car args))
      '(1 1))
     ;; Common Lisp symbols are case insensitive, so a real slynk matches
     ;; "CA" and "ca" alike.
     ((eq head 'slynk-completion:simple-completions)
      (let ((matches (cdr (assoc (downcase (car args)) ac-sly-test-completions))))
        (list matches (try-completion "" (or matches '(""))))))
     ((eq head 'slynk-completion:flex-completions)
      (let ((matches (cdr (assoc (downcase (car args)) ac-sly-test-completions)))
            (score 0.95))
        (list (mapcar (lambda (name)
                        (setq score (- score 0.075))
                        (list name score (list (list 0 (car args))) "-f----" nil))
                      matches)
              nil)))
     ((eq head 'slynk:documentation-symbol)
      (or (cdr (assoc (car args) ac-sly-test-docstrings)) "Not documented."))
     (t :no-such-package))))

(defun ac-sly-test-server-filter (process string)
  (let ((pending (concat (or (process-get process 'ac-sly-pending) "") string)))
    (while (and (>= (length pending) 6)
                (>= (- (length pending) 6)
                    (string-to-number (substring pending 0 6) 16)))
      (let* ((length (string-to-number (substring pending 0 6) 16))
             (payload (decode-coding-string (substring pending 6 (+ 6 length))
                                            'utf-8-unix))
             (message (car (read-from-string payload))))
        (setq pending (substring pending (+ 6 length)))
        (when (eq (car message) :emacs-rex)
          (let* ((form (nth 1 message))
                 (id (nth 4 message))
                 (answer (ac-sly-test-answer form)))
            (push form ac-sly-test-requests)
            (process-send-string
             process
             (ac-sly-test-encode
              (if (eq answer :no-such-package)
                  (list :return
                        (list :abort "READER-ERROR: package does not exist")
                        id)
                (list :return (list :ok answer) id))))
            (when (eq (car form) 'slynk-mrepl:create-mrepl)
              (process-send-string
               process
               (ac-sly-test-encode
                (list :channel-send ac-sly-test-listener-channel
                      (list :prompt "COMMON-LISP-USER" "CL-USER" 0 1)))))))))
    (process-put process 'ac-sly-pending pending)))

(defun ac-sly-test-rpcs ()
  "Return the RPC forms the Lisp received, in order.
The contrib load paths are elided; they are absolute build paths."
  (mapcar (lambda (form)
            (if (eq (car form) 'slynk:slynk-add-load-paths)
                (list (car form) :elided)
              form))
          (reverse ac-sly-test-requests)))

(defun ac-sly-test-connect ()
  "Start the stand-in Lisp and connect SLY to it the way a user does."
  (setq sly-contribs ac-sly-test-contribs
        sly-kill-without-query-p t
        sly-mrepl-pop-sylvester nil)
  (setq ac-sly-test-server
        (make-network-process :name "ac-sly-slynk"
                              :server t
                              :host 'local
                              :service t
                              :family 'ipv4
                              :coding 'binary
                              :filter #'ac-sly-test-server-filter))
  (sly-connect "127.0.0.1" (process-contact ac-sly-test-server :service))
  (let ((deadline (+ (float-time) 30)))
    (while (and (< (float-time) deadline)
                (not (and (sly-connected-p) (sly-lisp-implementation-name))))
      (accept-process-output nil 0.05)))
  (sly-connection))

(defun ac-sly-test-repl-buffer ()
  "Wait for the SLY REPL and its prompt, then select and return it."
  (let ((deadline (+ (float-time) 30))
        buffer)
    (while (and (< (float-time) deadline)
                (not (setq buffer
                           (cl-find-if
                            (lambda (candidate)
                              (string-prefix-p "*sly-mrepl"
                                               (buffer-name candidate)))
                            (buffer-list)))))
      (accept-process-output nil 0.05))
    (when buffer
      (set-buffer buffer)
      (set-window-buffer (selected-window) buffer)
      (while (and (< (float-time) deadline)
                  (not (string-match-p
                        "CL-USER"
                        (buffer-substring-no-properties (point-min) (point-max)))))
        (accept-process-output nil 0.05)))
    buffer))

(defmacro ac-sly-test-session (&rest body)
  "Run BODY, then close the connection and kill the buffers it made."
  `(let ((existing (buffer-list)))
     (setq ac-sly-test-requests nil)
     (unwind-protect
         (progn ,@body)
       (ignore-errors (ac-abort))
       (dolist (connection sly-net-processes)
         (ignore-errors (sly-net-close connection "workflow finished")))
       (when (process-live-p ac-sly-test-server)
         (delete-process ac-sly-test-server))
       (dolist (buffer (buffer-list))
         (unless (memq buffer existing)
           (with-current-buffer buffer
             (set-buffer-modified-p nil))
           (let ((kill-buffer-query-functions nil))
             (kill-buffer buffer)))))))

(defun ac-sly-test-lisp-buffer (text)
  "Return a displayed `lisp-mode' buffer holding TEXT, point at its end.
`ac-sources' starts empty so only the source ac-sly installs contributes."
  (let ((buffer (generate-new-buffer "*ac-sly-workflow*")))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (lisp-mode)
    (setq-local ac-sources nil)
    (insert text)
    buffer))

(defun ac-sly-test-menu ()
  "Report every candidate auto-complete built, in menu order."
  (mapcar (lambda (candidate)
            (list (substring-no-properties candidate)
                  (popup-item-symbol candidate)
                  (get-text-property 0 'summary candidate)
                  (get-text-property (min (1- (length candidate))
                                          (length ac-prefix))
                                     'face candidate)))
          ac-candidates))

(defun ac-sly-test-session-state ()
  "Report the completion state auto-complete is holding.
`ac-prefix' is reported without text properties: after an inline expansion
it is one of SLY's own propertized completion strings."
  (list :prefix (and (stringp ac-prefix) (substring-no-properties ac-prefix))
        :prefix-start (and ac-point (- ac-point (point-min)))
        :common (and (stringp ac-common-part)
                     (substring-no-properties ac-common-part))
        :menu-live (and (ac-menu-live-p) t)
        :selected (and (ac-menu-live-p)
                       (substring-no-properties (popup-selected-item ac-menu)))))

(defun ac-sly-test-buffer-state ()
  "Report the editing state the user can see."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (- (point) (point-min))
        :mode major-mode
        :connected (and (sly-connected-p) t)
        :auto-complete auto-complete-mode
        :sources ac-sources))
"##;

fn ac_sly_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_SLY_MELPA_PIN, "ac-sly.el")
        .expect("prepare pinned ac-sly source below ./tmp")
        .with_prelude(AC_SLY_TEST_PRELUDE)
        .with_timeout(AC_SLY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed ac-sly parity test").into()
}

/// Multi-probe batch for `assert_ac_sly_parity` cases (2a).
pub(crate) fn assert_ac_sly_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_sly_oracle(), &name, "ac_sly_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_sly_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_sly_batch(&cases);
}

// END generated package batch tests
