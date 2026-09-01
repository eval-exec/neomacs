use std::time::Duration;

use crate::{AC_ISPELL_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_ISPELL_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ac-ispell offers English word completions to auto-complete from two very
/// different sources, and both are driven for real here.
///
/// `ac-source-ispell' searches a plain word-list file: `ispell-lookup-words'
/// runs `ispell-grep-command' over `ispell-complete-word-dict'.  The word list
/// is a real sandbox file, the search is real `grep', and only the command name
/// is redirected to a recorder that logs its exact argv and then execs the real
/// `grep', so the search the package asks for is visible without being faked.
///
/// `ac-source-ispell-fuzzy' talks to a real speller subprocess over the
/// `ispell -a' pipe protocol.  The speller is the one true external boundary:
/// `ac-ispell-test-speller' is a small shell program answering `-vv' with a
/// banner ispell.el accepts, `*' for a word it knows, and `& WORD COUNT OFFSET:
/// near misses' for the fixture's misspellings, recording every line it
/// receives.  ispell.el keeps doing its own real process handling, filtering
/// and `ispell-parse-output' parsing, and auto-complete keeps building real
/// menus from real keys in a window-displayed buffer.
const AC_ISPELL_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'ispell)

(defun ac-ispell-test-path (name)
  "Return the absolute sandbox path of NAME."
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ac-ispell-test-write (name text)
  "Write TEXT to sandbox file NAME and return its path."
  (let ((path (ac-ispell-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun ac-ispell-test-write-executable (name body)
  (let ((path (ac-ispell-test-write (concat "bin/" name) body)))
    (set-file-modes path #o755)
    path))

;; A real word list, in the order a `grep' over the file returns it.
(defconst ac-ispell-test-word-list
  "recall\nreceive\nreceiver\nreception\nrecess\nrecipe\nrecipient\nreciprocal\nrecite\nreckon\nrecommend\n")

;; WORD TAB near misses, in the order the speller offers them.
(defconst ac-ispell-test-misspellings
  "recieve\treceive, relieve, reprieve\nrecipiant\trecipient, recipients, recipe\n")

(defconst ac-ispell-test-grep-recorder
  "#!/bin/sh\n{ printf 'grep'; for a in \"$@\"; do printf '|%s' \"$a\"; done; printf '\\n'; } >> \"$AC_ISPELL_LOOKUP_LOG\"\nexec grep \"$@\"\n")

(defconst ac-ispell-test-speller
  "#!/bin/sh
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
      misses=$(awk -F'\\t' -v w=\"$word\" '$1 == w { print $2; found = 1 }
                                           END { if (!found) print \"@@correct@@\" }' \"$ISPELL_DICTIONARY\")
      if [ \"$misses\" = '@@correct@@' ]; then
        printf '*\\n\\n'
      elif [ -z \"$misses\" ]; then
        printf '# %s 1\\n\\n' \"$word\"
      else
        count=$(printf '%s' \"$misses\" | awk -F', ' '{ print NF }')
        printf '& %s %s 1: %s\\n\\n' \"$word\" \"$count\" \"$misses\"
      fi ;;
  esac
done
")

(defun ac-ispell-test-setup (&optional word-list-name)
  "Install the real word list, the recording grep and the speller.
The word list is always written as words.txt; WORD-LIST-NAME chooses what
`ispell-complete-word-dict' points at, so a caller can point the package at
a word list that does not exist."
  (ac-ispell-test-write "words.txt" ac-ispell-test-word-list)
  (ac-ispell-test-write "misspellings.tsv" ac-ispell-test-misspellings)
  (setenv "AC_ISPELL_LOOKUP_LOG" (ac-ispell-test-path "lookup.log"))
  (setenv "ISPELL_LOG" (ac-ispell-test-path "ispell.log"))
  (setenv "ISPELL_DICTIONARY" (ac-ispell-test-path "misspellings.tsv"))
  (setq ispell-grep-command
        (ac-ispell-test-write-executable "grep-recorder" ac-ispell-test-grep-recorder)
        ispell-program-name
        (ac-ispell-test-write-executable "ispell" ac-ispell-test-speller)
        ispell-complete-word-dict
        (ac-ispell-test-path (or word-list-name "words.txt"))
        ispell-personal-dictionary nil
        ispell-dictionary nil
        ispell-local-dictionary nil))

(defun ac-ispell-test-log (name)
  (let ((path (ac-ispell-test-path name)))
    (if (file-exists-p path)
        (with-temp-buffer
          (insert-file-contents path)
          (split-string (buffer-string) "\n" t))
      'nothing-recorded)))

(defun ac-ispell-test-lookups ()
  "Return every word-list search the package asked for, in order."
  (ac-ispell-test-log "lookup.log"))

(defun ac-ispell-test-speller-log ()
  "Return every line the speller subprocess received, in order."
  (ac-ispell-test-log "ispell.log"))

(defmacro ac-ispell-test-with-live-buffer (mode text &rest body)
  "Run BODY in a MODE buffer holding TEXT, displayed in the selected window.
`ac-sources' starts empty so only the sources ac-ispell installs can
contribute candidates, and typed keys reach the buffer."
  `(let ((buffer (generate-new-buffer "*ac-ispell-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (funcall ,mode)
           (setq-local ac-sources nil)
           (insert ,text)
           ,@body)
       (with-current-buffer buffer
         (ignore-errors (ac-abort))
         (set-buffer-modified-p nil))
       (kill-buffer buffer))))

(defun ac-ispell-test-menu ()
  "Report every candidate auto-complete built, in menu order."
  (mapcar (lambda (candidate)
            (list (substring-no-properties candidate)
                  (popup-item-symbol candidate)
                  (get-text-property 0 'popup-face candidate)))
          ac-candidates))

(defun ac-ispell-test-session ()
  "Report the completion state auto-complete is holding."
  (list :prefix ac-prefix
        :prefix-start (and ac-point (- ac-point (point-min)))
        :common (and (stringp ac-common-part)
                     (substring-no-properties ac-common-part))
        :menu-live (and (ac-menu-live-p) t)
        :selected (and (ac-menu-live-p)
                       (substring-no-properties (popup-selected-item ac-menu)))))

(defun ac-ispell-test-buffer-state ()
  "Report the editing state the user can see."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (- (point) (point-min))
        :mode major-mode
        :auto-complete auto-complete-mode
        :sources ac-sources))
"##;

fn ac_ispell_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_ISPELL_MELPA_PIN, "ac-ispell.el")
        .expect("prepare pinned ac-ispell source below ./tmp")
        .with_prelude(AC_ISPELL_TEST_PRELUDE)
        .with_timeout(AC_ISPELL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-ispell parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_ispell_parity` cases (2a).
pub(crate) fn assert_ac_ispell_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_ispell_oracle(), &name, "ac_ispell_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_ispell_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_ispell_batch(&cases);
}

// END generated package batch tests
