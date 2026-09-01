use std::time::Duration;

use crate::{ASN1_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ASN1_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// asn1-mode is a pair of major modes for editing ITU-T X.680 ASN.1 modules
/// and X.722 GDMO documents.  Everything it does is local: a syntax table, a
/// SMIE grammar over its own tokenizer, font-lock rules, an abbrev table
/// derived from the keyword list, an outline regexp over section comments and
/// an imenu expression over assignments.  There is no subprocess and no
/// network, so the workflows below drive the real modes over real files in
/// the sandbox and nothing at all is stubbed.
///
/// Each workflow reaches the mode the way a user does -- by visiting a file
/// whose name `auto-mode-alist' maps to `asn1-mode' or `gdmo-mode' -- rather
/// than by calling the mode function in a temporary buffer.  That distinction
/// turned out to matter, and it is why `asn1-test-visit' binds
/// `enable-dir-local-variables' to nil: the sandbox lives inside the neomacs
/// worktree, whose top-level `.dir-locals.el' applies `tab-width' 8,
/// `fill-column' 72 and `sentence-end-double-space' t to every file below it.
/// Directory-local variables are applied after the major mode has run, so
/// they silently overwrite what the mode set for itself -- asn1-mode asks for
/// `tab-width' 4 and a visited file was getting 8, which changes every
/// indentation result in this file.  A user editing an .asn1 file in their
/// own project has no such directory above them, so the binding restores the
/// realistic configuration rather than removing one.
const ASN1_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(setq make-backup-files nil create-lockfiles nil)

(defun asn1-test-write (relative contents)
  "Write CONTENTS to RELATIVE below the sandbox.  Return the file."
  (let ((file (expand-file-name relative (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (write-region contents nil file nil 'silent))
    file))

(defun asn1-test-visit (relative contents)
  "Visit RELATIVE below the sandbox and return the buffer.
The major mode is whatever `auto-mode-alist' selects for the file name,
which is how a user reaches asn1-mode."
  (let* ((file (asn1-test-write relative contents))
         ;; The sandbox sits inside the neomacs worktree, whose top-level
         ;; .dir-locals.el sets tab-width 8 for every file below it.  Those
         ;; land after the major mode and would overwrite the tab-width 4 that
         ;; asn1-mode sets for itself.
         (enable-dir-local-variables nil)
         (buffer (find-file-noselect file)))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defun asn1-test-text ()
  (buffer-substring-no-properties (point-min) (point-max)))

(defun asn1-test-faces ()
  "Fontified runs of the buffer as (TEXT FACE), skipping unfaced text."
  (font-lock-mode 1)
  (font-lock-ensure)
  (let ((position (point-min)) runs)
    (while (< position (point-max))
      (let ((face (get-text-property position 'face))
            (next (next-single-property-change position 'face nil (point-max))))
        (when face
          (push (list (buffer-substring-no-properties position next) face) runs))
        (setq position next)))
    (nreverse runs)))

(defun asn1-test-visible-text ()
  "The buffer text a reader can actually see, invisible regions dropped."
  (let ((position (point-min)) parts)
    (while (< position (point-max))
      (let ((next (next-single-char-property-change position 'invisible)))
        (unless (invisible-p position)
          (push (buffer-substring-no-properties position next) parts))
        (setq position next)))
    (apply #'concat (nreverse parts))))

(defun asn1-test-headings ()
  "Every outline heading in the buffer as (LINE LEVEL)."
  (save-excursion
    (goto-char (point-min))
    (let (headings)
      (while (re-search-forward (concat "^\\(?:" outline-regexp "\\)") nil t)
        (goto-char (match-beginning 0))
        (looking-at outline-regexp)
        (push (list (string-trim (buffer-substring-no-properties
                                  (line-beginning-position) (line-end-position)))
                    (funcall outline-level))
              headings)
        (forward-line 1))
      (nreverse headings))))

(defun asn1-test-line ()
  (list (line-number-at-pos) (current-column)
        (buffer-substring-no-properties
         (line-beginning-position) (line-end-position))))

;; A realistic ASN.1 module: numbered section comments, an IMPORTS clause, a
;; SEQUENCE with a constrained INTEGER and an OPTIONAL field, a SEQUENCE OF
;; with a SIZE constraint, an ENUMERATED with numbered values, a CHOICE, and a
;; value assignment whose string literal is not ASCII.
(defconst asn1-test-module
  (concat
   "-- 1 Bestellsystem\n"
   "-- 1.1 Grundtypen\n"
   "Bestellung DEFINITIONS AUTOMATIC TAGS ::=\n"
   "BEGIN\n"
   "IMPORTS\n"
   "Kunde, Adresse\n"
   "FROM Kundenverwaltung;\n"
   "Auftrag ::= SEQUENCE {\n"
   "nummer INTEGER (1..65535),\n"
   "kunde Kunde,\n"
   "posten Postenliste,\n"
   "hinweis UTF8String OPTIONAL\n"
   "}\n"
   "Postenliste ::= SEQUENCE SIZE (1..99) OF Posten\n"
   "-- 1.2 Zustände\n"
   "Zustand ::= ENUMERATED {\n"
   "offen (0),\n"
   "versandt (1),\n"
   "storniert (2)\n"
   "}\n"
   "-- 1.2.1 Zahlungsarten\n"
   "Zahlung ::= CHOICE {\n"
   "rechnung Rechnung,\n"
   "lastschrift Lastschrift\n"
   "}\n"
   "standardHinweis UTF8String ::= \"Grüße aus München\"\n"
   "END\n"))

;; A GDMO document in the shape X.722 defines: a managed object class and the
;; package it is characterised by, both registered under an object identifier.
(defconst asn1-test-gdmo
  (concat
   "-- 2 Verwaltete Objekte\n"
   "kunde MANAGED OBJECT CLASS\n"
   "DERIVED FROM \"Rec. X.721 | ISO/IEC 10165-2 : 1992\":top;\n"
   "CHARACTERIZED BY kundenPaket;\n"
   "REGISTERED AS { joint-iso-itu-t ms(9) smi(3) part2(2) managedObjectClass(3) 1 };\n"
   "kundenPaket PACKAGE\n"
   "BEHAVIOUR kundenVerhalten;\n"
   "ATTRIBUTES kundenNummer GET,\n"
   "kundenName GET-REPLACE;\n"
   "REGISTERED AS { joint-iso-itu-t ms(9) smi(3) part2(2) package(4) 1 };\n"))
"##;

fn asn1_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ASN1_MODE_MELPA_PIN, "asn1-mode.el")
        .expect("prepare pinned asn1-mode source below ./tmp")
        .with_prelude(ASN1_MODE_TEST_PRELUDE)
        .with_timeout(ASN1_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed asn1-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_asn1_mode_parity` cases (2a).
pub(crate) fn assert_asn1_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(asn1_mode_oracle(), &name, "asn1_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn asn1_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_asn1_mode_batch(&cases);
}

// END generated package batch tests
