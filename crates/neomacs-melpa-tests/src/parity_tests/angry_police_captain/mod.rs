use std::time::Duration;

use crate::{ANGRY_POLICE_CAPTAIN_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANGRY_POLICE_CAPTAIN_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// The package is one command: fetch <http://theangrypolicecaptain.com>,
/// scrape the text of the first link back to that address, and show it in the
/// echo area.  The boundary is HTTP, a documented wire protocol, so the
/// workflows stand the counterparty up for real -- a listening
/// `make-network-process' answering the request -- and reach it the way a
/// user behind a proxy does, through `url-proxy-services'.  The URL is
/// hard-coded in the package with no option to override it, and that is
/// exactly why the proxy is the honest route: `url-retrieve' still runs,
/// url.el still builds and sends the request, and the package's callback
/// still parses a real response buffer.
///
/// Nothing about the package is stubbed.  `angry-police-captain' is called
/// as the command it is.
const ANGRY_POLICE_CAPTAIN_TEST_PRELUDE: &str = r##"
(require 'url)
(require 'seq)

(defvar apc-test-responder nil
  "Function returning the raw HTTP response for the next request.")

(defvar apc-test-requests nil
  "Every request line the server has received, oldest first.")

(defvar apc-test-server nil)

(defun apc-test-response (body &optional status headers)
  "Return a complete HTTP response carrying BODY."
  (concat "HTTP/1.1 " (or status "200 OK") "\r\n"
          "Content-Type: text/html; charset=utf-8\r\n"
          (or headers "")
          (format "Content-Length: %d\r\n" (string-bytes body))
          "Connection: close\r\n"
          "\r\n"
          body))

(defun apc-test-listen ()
  "Start the counterparty and route url.el to it, as a proxy would."
  (setq apc-test-requests nil)
  (setq apc-test-server
        (make-network-process
         :name "apc-test-server" :server t :host "127.0.0.1"
         :service t :family 'ipv4
         :filter (lambda (connection request)
                   (setq apc-test-requests
                         (append apc-test-requests (list request)))
                   (process-send-string
                    connection (funcall apc-test-responder request))
                   (process-send-eof connection))))
  (setq url-proxy-services
        (list (cons "http"
                    (format "127.0.0.1:%d"
                            (process-contact apc-test-server :service)))))
  apc-test-server)

(defun apc-test-request ()
  "Return the requests received, with url.el's version stamp normalised."
  (mapcar (lambda (request)
            (replace-regexp-in-string
             "Emacs/[0-9.]+" "Emacs/<VERSION>"
             (substring-no-properties request)))
          apc-test-requests))

(defun apc-test-echo-area (from)
  "Return what the package has echoed since position FROM of `*Messages*'.
url.el's own \"Contacting host:\" progress line is dropped: it is not the
package's output, and whether a redirect's second connection reports it
before or after a capture is a matter of timing -- GNU Emacs produced it
on one run of this suite and not on the next."
  (with-current-buffer (get-buffer-create "*Messages*")
    (seq-remove
     (lambda (line) (string-prefix-p "Contacting host:" line))
     (split-string
      (buffer-substring-no-properties (min from (point-max)) (point-max))
      "\n" t))))

(defun apc-test-leftover-buffers ()
  "Return the response buffers url.el has left behind."
  (seq-filter (lambda (name) (string-match-p "theangrypolicecaptain" name))
              (mapcar #'buffer-name (buffer-list))))

(defun apc-test-fetch (responder)
  "Run `angry-police-captain' against RESPONDER and report what happened.
The command is invoked the way the menu invokes it; see the workflow
that pins why no other invocation can finish."
  (setq apc-test-responder responder)
  (setq apc-test-requests nil)
  (let ((echoed-from (with-current-buffer (get-buffer-create "*Messages*")
                       (point-max))))
    (setq last-command-event 'kill-buffer)
    (angry-police-captain)
    ;; The callback runs from url.el's process filter, so waiting for the
    ;; connection to close is too early: the response still has to be read,
    ;; parsed, echoed and its buffer killed.  Wait for both of those to stop
    ;; changing instead.
    (let ((rounds 0) (stable 0) (previous nil))
      (while (and (< rounds 400) (< stable 8))
        (accept-process-output nil 0.02)
        (let ((now (list (apc-test-echo-area echoed-from)
                         (apc-test-leftover-buffers))))
          (setq stable (if (equal now previous) (1+ stable) 0))
          (setq previous now))
        (setq rounds (1+ rounds))))
    (list :echoed (apc-test-echo-area echoed-from)
          :requests-served (length apc-test-requests)
          :leftover-buffers (apc-test-leftover-buffers))))

(defun apc-test-quote (page)
  "Return only the line PAGE causes to be echoed."
  (car (last (plist-get (apc-test-fetch (lambda (_request)
                                          (apc-test-response page)))
                        :echoed))))
"##;

fn angry_police_captain_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANGRY_POLICE_CAPTAIN_MELPA_PIN, "angry-police-captain.el")
        .expect("prepare pinned angry-police-captain source below ./tmp")
        .with_prelude(ANGRY_POLICE_CAPTAIN_TEST_PRELUDE)
        .with_timeout(ANGRY_POLICE_CAPTAIN_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed angry-police-captain parity test")
        .into()
}

/// Multi-probe batch for `assert_angry_police_captain_parity` cases (2a).
pub(crate) fn assert_angry_police_captain_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        angry_police_captain_oracle(),
        &name,
        "angry_police_captain_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn angry_police_captain_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_angry_police_captain_batch(&cases);
}

// END generated package batch tests
