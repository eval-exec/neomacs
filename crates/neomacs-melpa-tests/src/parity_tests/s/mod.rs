use std::time::Duration;

use crate::{CachedMelpaOracle, S_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const S_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// `s' is a string library, so a workflow here is a piece of text going
/// through a real pipeline rather than a call to one function.  Every fixture
/// below is text a user would actually have: a spreadsheet export pasted in
/// with mixed line endings and empty fields, log lines from a real format,
/// headings typed by hand with accents and punctuation, a form's worth of
/// user input.  Each workflow runs several `s-' functions over one of them in
/// the order a program would, and asserts the whole result.
///
/// That matters more here than for most packages, because `s' sits in the
/// dependency closure of a large part of this suite.  A difference in
/// `s-format' or `s-split' would surface as a failure in whichever package
/// happened to call it, and be credited there.  So the fixtures are built to
/// exercise the parts most likely to differ between two implementations -
/// multibyte characters and combining marks, the difference between a
/// character count and a display width, CRLF against LF, null-field policy,
/// case conversion outside ASCII, and regexp match data - rather than to
/// enumerate the API.
const S_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defun s-test-copy (value)
  (if (stringp value) (copy-sequence value) value))

(defun s-test-copy-tree (value)
  (cond ((stringp value) (copy-sequence value))
        ((consp value) (mapcar #'s-test-copy-tree value))
        (t value)))

;; A spreadsheet export as it arrives from a browser: CRLF from the download,
;; one LF line where somebody edited it by hand, a trailing blank line, padded
;; cells, an empty field, and names outside ASCII.
(defconst s-test-export
  (concat "sku , name , price , note\r\n"
          "WH-001,  Gruesse Widget ,  12.50 ,\r\n"
          "WH-002,\tねじ回し\t,3.00,  keeps   its   spacing  \r\n"
          "WH-003, Café Cup ,7.25,naive\r\n"
          "WH-004,,0.00,missing name\n"
          "\r\n"))

;; Real log output: a level, a bracketed subsystem, a message, and one line
;; that does not fit the shape at all.
(defconst s-test-log
  (concat "2026-07-28 09:14:02 INFO  [inventory] loaded 3 widgets in 12ms\n"
          "2026-07-28 09:14:03 WARN  [sync] bucket cache is stale (age=91s)\n"
          "2026-07-28 09:14:04 ERROR [sync] upload failed: connection refused\n"
          "  at com.warehouse.Sync.upload(Sync.java:42)\n"
          "2026-07-28 09:14:09 INFO  [inventory] reloaded 3 widgets in 9ms\n"))

;; Headings typed by a person: mixed case, punctuation, runs of spaces,
;; accents, and one that is only punctuation.
(defconst s-test-headings
  '("  Getting Started with Widgets  "
    "API reference: the HTTP endpoints"
    "Gruesse & Groessen"
    "already-dashed-heading"
    "MixedCASE wordsHere"
    "---"))

;; A submitted form, including the empty and whitespace-only fields a real one
;; produces, and a non-ASCII digit.
(defconst s-test-form
  '(("sku" . "WH-001")
    ("quantity" . "12")
    ("discount" . "")
    ("note" . "   ")
    ("price" . "12.50")
    ("owner" . "Zoë")
    ("count" . "٣")))

(defun s-test-report (pairs)
  "Render PAIRS as (LABEL VALUE) with everything copied out of the fixture."
  (mapcar (lambda (pair) (list (s-test-copy (car pair)) (s-test-copy-tree (cdr pair))))
          pairs))
"##;

fn s_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(S_MELPA_PIN, "s.el")
        .expect("prepare pinned s source below ./tmp")
        .with_prelude(S_TEST_PRELUDE)
        .with_timeout(S_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed s parity test").into()
}

/// Multi-probe batch for `assert_s_parity` cases (2a).
pub(crate) fn assert_s_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(s_oracle(), &name, "s_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn s_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_s_batch(&cases);
}

// END generated package batch tests
