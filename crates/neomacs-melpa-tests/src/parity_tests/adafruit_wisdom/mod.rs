use std::time::Duration;

use crate::{ADAFRUIT_WISDOM_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ADAFRUIT_WISDOM_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Fixtures shared by the workflows.
///
/// adafruit-wisdom fetches an RSS feed of engineering quotes over HTTP with the
/// `request' package, caches it below `user-emacs-directory' for a day, and
/// picks one quote at random.  The network is the only boundary faked: the
/// prelude runs a real HTTP server in-process (`make-network-process' with
/// `:server t'), serving a realistic quote feed and recording the exact request
/// line and headers it receives, and points `adafruit-wisdom-quote-url' at it.
/// The real service is never contacted.
///
/// Everything else is real -- `request's two transports, the cache file and its
/// time-to-live, `xml-parse-region', `dom-by-tag' and the package's own
/// commands.  Two values are redacted from the recorded headers because they
/// cannot be fixed: the `User-Agent', which carries the editor and its version,
/// and the `Host' port the server was assigned.
///
/// The random pick is the one other nondeterminism, and randomness is an
/// explicitly permitted double: workflows that name a specific quote bind
/// `random' for the duration of the pick, always with the feed already cached so
/// no other code is affected.
const ADAFRUIT_WISDOM_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar adaw-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar adaw-test-requests nil
  "Request lines and headers the stand-in feed server received.")

(defvar adaw-test-server nil)
(defvar adaw-test-status "200 OK")
(defvar adaw-test-content-type "application/rss+xml; charset=utf-8")
(defvar adaw-test-body nil)

(defconst adaw-test-feed
  "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<rss version=\"2.0\">
  <channel>
    <title>Adafruit Industries quotes</title>
    <item><title>Make it work, then make it beautiful.</title></item>
    <item><title>Solder &amp; patience &#8212; na&#239;ve questions win.</title></item>
    <item><title>Ingénierie: mesure deux fois, coupe une fois.</title></item>
  </channel>
</rss>
")

(defun adaw-test-server-filter (process string)
  (let ((pending (concat (or (process-get process 'adaw-pending) "") string)))
    (if (not (string-match "\r\n\r\n" pending))
        (process-put process 'adaw-pending pending)
      (push (split-string (substring pending 0 (match-beginning 0)) "\r\n" t)
            adaw-test-requests)
      (let* ((body (encode-coding-string (or adaw-test-body adaw-test-feed)
                                         'utf-8-unix))
             (response (concat "HTTP/1.1 " adaw-test-status "\r\n"
                               "Content-Type: " adaw-test-content-type "\r\n"
                               (format "Content-Length: %d\r\n" (length body))
                               "Connection: close\r\n\r\n"
                               body)))
        (process-send-string process response)
        (process-send-eof process)))))

(defun adaw-test-start-server ()
  "Start the stand-in quote feed and return its port."
  (when (process-live-p adaw-test-server)
    (delete-process adaw-test-server))
  (setq adaw-test-requests nil)
  (setq adaw-test-server
        (make-network-process :name "adaw-feed"
                              :server t
                              :host 'local
                              :service t
                              :family 'ipv4
                              :coding 'binary
                              :filter #'adaw-test-server-filter))
  (process-contact adaw-test-server :service))

(defun adaw-test-requests ()
  (reverse adaw-test-requests))

(defun adaw-test-request-lines ()
  "Return only the request lines the server saw."
  (mapcar #'car (adaw-test-requests)))

(defun adaw-test-setup (&optional backend)
  "Point the package at the stand-in feed and return its URL."
  ;; Batch cases share an Emacs process for speed, but a package cache is part
  ;; of this fixture's external state and must never leak into the next case.
  (adaw-test-forget-cache)
  (setq request-backend (or backend 'url-retrieve)
        request-log-level -1
        request-message-level -1)
  (let ((port (adaw-test-start-server)))
    (setq adafruit-wisdom-quote-url
          (format "http://127.0.0.1:%d/feed/quotes.xml" port))))

(defun adaw-test-cache-exists ()
  (and (file-exists-p adafruit-wisdom-cache-file) t))

(defun adaw-test-cache-contents ()
  (if (file-exists-p adafruit-wisdom-cache-file)
      (with-temp-buffer
        (insert-file-contents adafruit-wisdom-cache-file)
        (buffer-substring-no-properties (point-min) (point-max)))
    'no-cache))

(defun adaw-test-age-cache (seconds)
  "Make the cache file look SECONDS old."
  (set-file-times adafruit-wisdom-cache-file
                  (time-subtract (current-time) seconds)))

(defun adaw-test-buffer ()
  (let ((buffer (generate-new-buffer "*adaw-work*")))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (insert "notes:\n")
    buffer))

(defun adaw-test-messages-since (mark)
  (with-current-buffer (get-buffer-create "*Messages*")
    (split-string
     (buffer-substring-no-properties (min mark (point-max)) (point-max))
     "\n" t)))

(defun adaw-test-message-mark ()
  (with-current-buffer (get-buffer-create "*Messages*") (point-max)))


(defun adaw-test-headers ()
  "Return each recorded request, redacting the two values that cannot be fixed.

The `User-Agent\' carries the editor and its version, and `Host\' carries the
port the stand-in server happened to be given."
  (mapcar (lambda (request)
            (mapcar (lambda (line)
                      (cond ((string-prefix-p "User-Agent:" line)
                             "User-Agent: <editor>")
                            ((string-prefix-p "Host:" line)
                             "Host: 127.0.0.1:<port>")
                            (t line)))
                    request))
          (adaw-test-requests)))

(defun adaw-test-forget-cache ()
  (when (file-exists-p adafruit-wisdom-cache-file)
    (delete-file adafruit-wisdom-cache-file)))
"##;

fn adafruit_wisdom_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ADAFRUIT_WISDOM_MELPA_PIN, "adafruit-wisdom.el")
        .expect("prepare pinned adafruit-wisdom source below ./tmp")
        .with_prelude(ADAFRUIT_WISDOM_TEST_PRELUDE)
        .with_timeout(ADAFRUIT_WISDOM_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed adafruit-wisdom parity test")
        .into()
}

/// Multi-probe batch for `assert_adafruit_wisdom_parity` cases (2a).
pub(crate) fn assert_adafruit_wisdom_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        adafruit_wisdom_oracle(),
        &name,
        "adafruit_wisdom_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn adafruit_wisdom_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_adafruit_wisdom_batch(&cases);
}

// END generated package batch tests
