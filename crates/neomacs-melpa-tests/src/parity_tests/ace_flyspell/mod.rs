use std::time::Duration;

use crate::{ACE_FLYSPELL_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACE_FLYSPELL_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Sandbox helpers shared by the workflows.
///
/// ace-flyspell jumps to a flyspell-flagged word with `avy' and then runs
/// flyspell's correction UI, so every workflow needs three real things: a
/// window-displayed buffer (`execute-kbd-macro' delivers keys to the selected
/// window's buffer, not to a temp buffer), real flyspell overlays, and a real
/// speller subprocess.
///
/// The only stand-in is the speller itself: `afly-test-ispell-program' is a
/// small shell program that speaks the real `ispell -a' pipe protocol -- it
/// answers `-vv' with a version banner ispell.el accepts, replies `*' for known
/// words, `& WORD COUNT OFFSET: near misses' for the fixture's misspellings and
/// `# WORD OFFSET' for a misspelling with no suggestion, honours `*WORD' and
/// `#' for personal-dictionary additions, and records every line it receives.
/// flyspell and ispell.el keep doing their own real parsing, overlay
/// management, correction ring and process handling, and avy keeps reading real
/// keys.
const ACE_FLYSPELL_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'flyspell)
(require 'ispell)

(defvar afly-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar afly-test-bin
  (file-name-as-directory (expand-file-name "bin" afly-test-root)))

(defvar afly-test-log (expand-file-name "ispell.log" afly-test-root))
(defvar afly-test-dictionary (expand-file-name "misspellings.tsv" afly-test-root))
(defvar afly-test-personal (expand-file-name "personal.dict" afly-test-root))

;; WORD TAB near misses, in the order the speller offers them.  `Umlaut' is a
;; misspelling for which the speller has no suggestion at all.
(defconst afly-test-misspellings
  "recieve\treceive, relieve, reprieve
seperate\tseparate, desperate, temperate
occured\toccurred, occurs, occupied
definately\tdefinitely, definitively, defiantly
Umlaut\t
")

(defun afly-test-write-executable (name body)
  (let ((path (expand-file-name name afly-test-bin)))
    (make-directory afly-test-bin t)
    (with-temp-buffer
      (insert body)
      (write-region (point-min) (point-max) path nil 'silent))
    (set-file-modes path #o755)
    path))

(defconst afly-test-ispell-program
  "#!/bin/sh
# Recording stand-in speaking the ispell -a pipe protocol.
case \"$1\" in
  -v|-vv|--version)
    printf '@(#) International Ispell Version 3.4.00 (but really Ispell 3.4.00, 1 Jan 2020)\\n'
    printf 'LIBDIR = \"/usr/lib/ispell\"\\n'
    exit 0 ;;
esac
{ printf 'run'; for argument in \"$@\"; do printf '|%s' \"$argument\"; done; printf '\\n'
} >> \"$ISPELL_LOG\"
printf '@(#) International Ispell Version 3.4.00\\n'
while IFS= read -r line; do
  printf '%s\\n' \"$line\" >> \"$ISPELL_LOG\"
  case \"$line\" in
    '^'*)
      word=${line#?}
      if grep -Fqx \"$word\" \"$ISPELL_PERSONAL\" 2>/dev/null; then
        printf '*\\n\\n'
      else
        misses=$(awk -F'\\t' -v w=\"$word\" '$1 == w { print $2; found = 1 }
                                             END { if (!found) print \"@@correct@@\" }' \\
                   \"$ISPELL_DICTIONARY\")
        if [ \"$misses\" = '@@correct@@' ]; then
          printf '*\\n\\n'
        elif [ -z \"$misses\" ]; then
          printf '# %s 1\\n\\n' \"$word\"
        else
          count=$(printf '%s' \"$misses\" | awk -F', ' '{ print NF }')
          printf '& %s %s 1: %s\\n\\n' \"$word\" \"$count\" \"$misses\"
        fi
      fi ;;
    '*'*) printf '%s\\n' \"${line#?}\" >> \"$ISPELL_PERSONAL\" ;;
    '#') printf 'saved\\n' >> \"$ISPELL_LOG\" ;;
  esac
done
")

(defun afly-test-setup ()
  "Install the recording speller and pin avy's reading keys."
  (make-directory afly-test-bin t)
  (with-temp-buffer
    (insert afly-test-misspellings)
    (write-region (point-min) (point-max) afly-test-dictionary nil 'silent))
  (with-temp-buffer
    (write-region (point-min) (point-max) afly-test-personal nil 'silent))
  (setenv "ISPELL_LOG" afly-test-log)
  (setenv "ISPELL_DICTIONARY" afly-test-dictionary)
  (setenv "ISPELL_PERSONAL" afly-test-personal)
  (setq ispell-program-name
        (afly-test-write-executable "ispell" afly-test-ispell-program))
  (setq ispell-personal-dictionary nil
        ispell-dictionary nil
        ispell-local-dictionary nil
        flyspell-issue-welcome-flag nil
        flyspell-issue-message-flag nil
        avy-keys '(?a ?s ?d ?f ?g ?h ?j ?k ?l)
        avy-style 'at-full
        avy-all-windows t))

(defconst afly-test-prose
  "The commitee will recieve the report.
We must seperate the two lists.
It occured twice, and it is definately wrong.
")

(defun afly-test-buffer (&optional text)
  "Create the work buffer and display it, so typed keys reach it."
  (let ((buffer (generate-new-buffer "*ace-flyspell-workflow*")))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (insert (or text afly-test-prose))
    (goto-char (point-min))
    buffer))

(defun afly-test-log-lines ()
  (if (file-exists-p afly-test-log)
      (with-temp-buffer
        (insert-file-contents afly-test-log)
        (split-string (buffer-string) "\n" t))
    'nothing-recorded))

(defun afly-test-queries ()
  "Return every word the package asked the speller about, in order."
  (delq nil (mapcar (lambda (line)
                      (and (string-prefix-p "^" line) (substring line 1)))
                    (afly-test-log-lines))))

(defun afly-test-session ()
  "Return the speller session lines that are not word queries."
  (delq nil (mapcar (lambda (line)
                      (and (not (string-prefix-p "^" line))
                           (not (string-prefix-p "%" line))
                           line))
                    (afly-test-log-lines))))

(defun afly-test-flyspell-overlays ()
  "Return (START END TEXT FACE) for every flyspell overlay, in order."
  (sort
   (delq nil
         (mapcar (lambda (overlay)
                   (when (flyspell-overlay-p overlay)
                     (list (overlay-start overlay)
                           (overlay-end overlay)
                           (buffer-substring-no-properties
                            (overlay-start overlay) (overlay-end overlay))
                           (overlay-get overlay 'face))))
                 (overlays-in (point-min) (point-max))))
   (lambda (a b) (< (car a) (car b)))))

(defun afly-test-box-overlay ()
  "Return the bounds and face of the overlay ace-flyspell boxes the word with."
  (list (overlay-start ace-flyspell--ov)
        (overlay-end ace-flyspell--ov)
        (overlay-get ace-flyspell--ov 'face)))

(defun afly-test-message-mark ()
  (with-current-buffer (get-buffer-create "*Messages*") (point-max)))

(defun afly-test-messages-since (mark)
  (with-current-buffer (get-buffer-create "*Messages*")
    (split-string
     (buffer-substring-no-properties (min mark (point-max)) (point-max))
     "\n" t)))
"##;

fn ace_flyspell_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACE_FLYSPELL_MELPA_PIN, "ace-flyspell.el")
        .expect("prepare pinned ace-flyspell source below ./tmp")
        .with_prelude(ACE_FLYSPELL_TEST_PRELUDE)
        .with_timeout(ACE_FLYSPELL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ace-flyspell parity test")
        .into()
}

/// Multi-probe batch for `assert_ace_flyspell_parity` cases (2a).
pub(crate) fn assert_ace_flyspell_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ace_flyspell_oracle(), &name, "ace_flyspell_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ace_flyspell_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ace_flyspell_batch(&cases);
}

// END generated package batch tests
