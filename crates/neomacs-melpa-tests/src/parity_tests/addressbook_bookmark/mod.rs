use std::time::Duration;

use crate::{ADDRESSBOOK_BOOKMARK_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ADDRESSBOOK_BOOKMARK_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// addressbook-bookmark keeps contacts in Emacs's own bookmark machinery, so
/// these workflows use real bookmark files below the per-case sandbox and let
/// the package write, render, edit and reload them for real.
///
/// The one faked boundary is the human at the keyboard: every contact field is
/// read with `read-string' and every confirmation with `y-or-n-p', so
/// `ab-test-with-answers' hands those two a scripted queue and records what was
/// asked - including the initial input, which is how the edit command offers
/// the current value of each field.  Contact data is deliberately not ASCII,
/// and one workflow puts the bookmark file itself under an accented name, so
/// that a file-name or coding-system defect shows up as what it would be for a
/// user: a corrupted address book.
const ADDRESSBOOK_BOOKMARK_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defvar ab-test-answers nil
  "Queue of answers the scripted minibuffer hands out, in order.")

(defvar ab-test-prompts nil
  "Every prompt the package asked, in order.")

(defmacro ab-test-with-answers (answers &rest body)
  "Run BODY answering each `read-string' prompt from ANSWERS.
The human at the keyboard is the one boundary these workflows fake:
the package reads every contact field with `read-string' and asks to
continue with `y-or-n-p'."
  `(let ((ab-test-answers ,answers)
         (ab-test-prompts nil))
     (cl-letf (((symbol-function 'read-string)
                (lambda (prompt &optional initial &rest _)
                  (push (cons prompt (or initial "")) ab-test-prompts)
                  (or (pop ab-test-answers) "")))
               ((symbol-function 'y-or-n-p)
                (lambda (prompt)
                  (push (cons prompt :y-or-n-p) ab-test-prompts)
                  (equal (pop ab-test-answers) "yes"))))
       ,@body)))

(defun ab-test-asked ()
  "Return the prompts the package asked, oldest first."
  (reverse ab-test-prompts))

(defun ab-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defconst ab-test-zoe
  '("Zoë Müller"                        ; Name
    "Freunde" ""                        ; Group (read until empty)
    "zoe@example.org" "z.mueller@example.net" "" ; Mail
    "+49 221 4711" ""                   ; Phone
    "https://zoë.example" ""            ; Web
    "Hauptstraße 7"                     ; Street
    "Köln"                              ; City
    "NRW"                               ; State
    "50667"                             ; Zipcode
    "Deutschland"                       ; Country
    "Grüße aus Köln"                    ; Note
    ""                                  ; Image path
    "no")                               ; Add another contact?
  "Scripted answers creating one contact whose data is not ASCII.")

(defconst ab-test-ann
  '("Ann Smith"                         ; Name
    "Work" ""                           ; Group
    "ann@example.com" ""                ; Mail
    ""                                  ; Phone
    ""                                  ; Web
    "12 Main Street"                    ; Street
    "Springfield"                       ; City
    "IL"                                ; State
    "62704"                             ; Zipcode
    "USA"                               ; Country
    ""                                  ; Note
    ""                                  ; Image path
    "no")                               ; Add another contact?
  "Scripted answers creating a second, plain ASCII contact.")

(defun ab-test-record (name)
  "Return NAME's bookmark record with its fields in a stable order."
  (let ((record (assoc name bookmark-alist)))
    (and record
         (cons (car record)
               (sort (copy-sequence (cdr record))
                     (lambda (a b) (string< (format "%S" a) (format "%S" b))))))))

(defun ab-test-file-contents (path)
  (with-temp-buffer
    (insert-file-contents path)
    (buffer-substring-no-properties (point-min) (point-max))))

(defun ab-test-file-bytes (path)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (buffer-string)))

(defun ab-test-normalize (text)
  "Replace the volatile parts of TEXT: the record's last-modified stamp."
  (replace-regexp-in-string
   "(last-modified [0-9 .]+)" "(last-modified <TIME>)" text t t))

(defun ab-test-record-text (name)
  "Return NAME's bookmark record printed, with volatile fields normalised."
  (let ((record (ab-test-record name)))
    (and record (ab-test-normalize (prin1-to-string record)))))

(defconst ab-test-zoe-edit
  '("Zoë Müller"                        ; Name
    "Freunde"                           ; Group
    "zoe@example.org"                   ; Mail (dropped the second address)
    "+49 221 4711"                      ; Phone
    "https://zoë.example"               ; Web
    "Sendlinger Straße 1"               ; Street
    "München"                           ; City
    "Bayern"                            ; State
    "80331"                             ; Zipcode
    "Deutschland"                       ; Country
    "Umgezogen"                         ; Note
    ""                                  ; Image path
    "yes")                              ; Save changes?
  "Scripted answers for editing the first contact.
`addressbook-bookmark-edit' offers the current value of each field as
the minibuffer's initial input, which the recorded prompts show.")
"##;

fn addressbook_bookmark_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ADDRESSBOOK_BOOKMARK_MELPA_PIN, "addressbook-bookmark.el")
        .expect("prepare pinned addressbook-bookmark source below ./tmp")
        .with_prelude(ADDRESSBOOK_BOOKMARK_TEST_PRELUDE)
        .with_timeout(ADDRESSBOOK_BOOKMARK_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed addressbook-bookmark parity test")
        .into()
}

/// Multi-probe batch for `assert_addressbook_bookmark_parity` cases (2a).
pub(crate) fn assert_addressbook_bookmark_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        addressbook_bookmark_oracle(),
        &name,
        "addressbook_bookmark_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn addressbook_bookmark_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_addressbook_bookmark_batch(&cases);
}

// END generated package batch tests
