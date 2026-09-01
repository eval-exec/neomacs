use std::time::Duration;

use crate::{AC_MOZC_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_MOZC_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Sandbox helpers shared by the workflows.
///
/// ac-mozc converts romaji to Japanese by driving a real mozc session, so the
/// external boundary the workflows fake is the `mozc_emacs_helper' program
/// mozc.el talks to.  `ac-mozc-test-setup' writes a recording stand-in below
/// `NEOMACS_TEST_SANDBOX_ROOT' and points `mozc-helper-program-name' at it.
/// The stand-in speaks the helper's real line protocol: it prints the
/// `((mozc-emacs-helper . t) (version . ...))' greeting, answers
/// `(SEQ CreateSession)', `(SEQ SendKey SESSION KEY)' and
/// `(SEQ DeleteSession SESSION)' with `mozc::commands::Output' alists carrying
/// `consumed', `preedit' and `all-candidate-words', and appends every request
/// it receives to a log so the workflows can pin the exact key-by-key traffic
/// ac-mozc produced.
///
/// Its conversions come from a table written by `ac-mozc-test-setup' rather
/// than from a romaji engine: the stand-in accumulates the key codes it is
/// sent, looks the accumulated romaji up, and replies with that reading and
/// its candidates -- the reading candidates before the conversion key and the
/// kanji candidates after it, which is the two-phase exchange ac-mozc performs.
/// Only the state after the last key is observable through the package, so
/// intermediate romaji prefixes are answered with the raw accumulation.
///
/// Everything else runs for real: `ac-mozc-prefix', `ac-mozc-match',
/// `ac-mozc-send-word', the `ac-mozc-kana-p' guard, `ac-mozc-action' and its
/// `ac-cleanup' advice, both `ac-define-source' sources, and auto-complete's
/// own `ac-start' / `ac-update' / `ac-complete' cycle in a window-displayed
/// buffer.
const AC_MOZC_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'mozc)
(require 'auto-complete)

(defvar ac-mozc-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar ac-mozc-test-bin
  (file-name-as-directory (expand-file-name "bin" ac-mozc-test-root)))
(defvar ac-mozc-test-log (expand-file-name "mozc-helper.log" ac-mozc-test-root))
(defvar ac-mozc-test-table (expand-file-name "mozc-table.tsv" ac-mozc-test-root))

;; ROMAJI, preedit reading, candidates before conversion, candidates after it.
(defconst ac-mozc-test-conversions
  '(("kanji"   "かんじ"   ("かんじ" "カンジ")     ("漢字" "感じ" "幹事"))
    ("ohayou"  "おはよう" ("おはよう" "オハヨウ") ("お早う" "おはよう"))
    ("nihongo" "にほんご" ("にほんご" "ニホンゴ") ("日本語"))
    ("xyz"     "ｘｙｚ"   nil                     nil)))

(defconst ac-mozc-test-helper-program
  "#!/bin/sh
# Recording stand-in for mozc_emacs_helper.
{ printf 'start'
  for argument in \"$@\"; do printf '\\037%s' \"$argument\"; done
  printf '\\n'
} >> \"$MOZC_LOG\"

printf '((mozc-emacs-helper . t) (version . \"0.0.0.0\"))\\n'

session=0
romaji=

lookup() {
  awk -F'\\t' -v want=\"$2\" -v column=\"$1\" '$1 == want { print $column; found = 1 }
                                               END { if (!found) print \"\" }' \\
    \"$MOZC_TABLE\"
}

candidates_sexp() {
  if [ -z \"$1\" ]; then printf '(candidates)'; return; fi
  printf '(candidates'
  index=0
  printf '%s\\n' \"$1\" | tr '|' '\\n' | while IFS= read -r value; do
    printf ' ((index . %s) (value . \"%s\"))' \"$index\" \"$value\"
    index=$((index + 1))
  done
  printf ')'
}

while IFS= read -r line; do
  printf '%s\\n' \"$line\" >> \"$MOZC_LOG\"
  stripped=$(printf '%s' \"$line\" | tr -d '()')
  # shellcheck disable=SC2086
  set -- $stripped
  sequence=$1
  command=$2
  case \"$command\" in
    CreateSession)
      session=$((session + 1))
      romaji=
      printf '((emacs-event-id . %s) (emacs-session-id . %s) (output (id . %s) (mode . HIRAGANA)))\\n' \\
        \"$sequence\" \"$session\" \"$session\"
      ;;
    DeleteSession)
      romaji=
      printf '((emacs-event-id . %s) (emacs-session-id . %s) (output (id . %s)))\\n' \\
        \"$sequence\" \"$3\" \"$3\"
      ;;
    SendKey)
      key=$4
      if [ \"$key\" = space ]; then
        preedit=$(lookup 2 \"$romaji\")
        values=$(lookup 4 \"$romaji\")
      else
        character=$(printf \"\\\\$(printf '%03o' \"$key\")\")
        romaji=\"$romaji$character\"
        preedit=$(lookup 2 \"$romaji\")
        [ -n \"$preedit\" ] || preedit=$romaji
        values=$(lookup 3 \"$romaji\")
      fi
      printf '((emacs-event-id . %s) (emacs-session-id . %s) (output (id . %s) (consumed . t) (preedit (cursor . 1) (segment ((annotation . UNDERLINE) (value . \"%s\") (value-length . 1) (key . \"%s\")))) (all-candidate-words %s)))\\n' \\
        \"$sequence\" \"$3\" \"$3\" \"$preedit\" \"$preedit\" \"$(candidates_sexp \"$values\")\"
      ;;
    *)
      printf '((emacs-event-id . %s) (emacs-session-id . %s) (output))\\n' \"$sequence\" \"$3\"
      ;;
  esac
done
")

;; Greets, answers CreateSession, then dies: a helper that crashes mid-session.
(defconst ac-mozc-test-dying-helper-program
  "#!/bin/sh
printf '((mozc-emacs-helper . t) (version . \"0.0.0.0\"))\\n'
while IFS= read -r line; do
  printf '%s\\n' \"$line\" >> \"$MOZC_LOG\"
  case \"$line\" in
    *CreateSession*)
      printf '((emacs-event-id . %s) (emacs-session-id . 1) (output (id . 1)))\\n' \\
        \"$(printf '%s' \"$line\" | tr -d '()' | cut -d' ' -f1)\"
      ;;
    *)
      exit 1 ;;
  esac
done
")

(defun ac-mozc-test-write-executable (name body)
  (let ((path (expand-file-name name ac-mozc-test-bin))
        (coding-system-for-write 'utf-8-unix))
    (make-directory ac-mozc-test-bin t)
    (with-temp-buffer
      (insert body)
      (write-region (point-min) (point-max) path nil 'silent))
    (set-file-modes path #o755)
    path))

(defun ac-mozc-test-setup ()
  "Install the recording mozc helper and its conversion table."
  (make-directory ac-mozc-test-root t)
  (with-temp-buffer
    (dolist (entry ac-mozc-test-conversions)
      (insert (format "%s\t%s\t%s\t%s\n"
                      (nth 0 entry) (nth 1 entry)
                      (mapconcat #'identity (nth 2 entry) "|")
                      (mapconcat #'identity (nth 3 entry) "|"))))
    (let ((coding-system-for-write 'utf-8-unix))
      (write-region (point-min) (point-max) ac-mozc-test-table nil 'silent)))
  (when (file-exists-p ac-mozc-test-log)
    (delete-file ac-mozc-test-log))
  (setenv "MOZC_LOG" ac-mozc-test-log)
  (setenv "MOZC_TABLE" ac-mozc-test-table)
  (ac-mozc-test-write-executable "mozc_emacs_helper_dying"
                                 ac-mozc-test-dying-helper-program)
  (setq mozc-helper-program-name
        (ac-mozc-test-write-executable "mozc_emacs_helper"
                                       ac-mozc-test-helper-program)))

(defmacro ac-mozc-test-with-buffer (source text &rest body)
  "Complete with SOURCE in a window-displayed buffer holding TEXT."
  `(let ((buffer (generate-new-buffer "*ac-mozc-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (text-mode)
           (ac-cleanup)
           (setq ac-sources (list ,source))
           (auto-complete-mode 1)
           (insert ,text)
           ,@body)
       (kill-buffer buffer))))

(defun ac-mozc-test-complete ()
  "Start completion at point and return the plain candidate strings."
  (ac-start :force-init t)
  (ac-update t)
  (mapcar #'substring-no-properties ac-candidates))

(defun ac-mozc-test-traffic ()
  "Every request line the helper received, split on its field separator."
  (if (file-exists-p ac-mozc-test-log)
      (with-temp-buffer
        (let ((coding-system-for-read 'utf-8-unix))
          (insert-file-contents ac-mozc-test-log))
        (mapcar (lambda (line) (split-string line "\037"))
                (split-string (buffer-string) "\n" t)))
    'nothing-recorded))

(defun ac-mozc-test-message-mark ()
  (with-current-buffer (get-buffer-create "*Messages*") (point-max)))

(defun ac-mozc-test-messages-since (mark)
  (with-current-buffer (get-buffer-create "*Messages*")
    (split-string
     (buffer-substring-no-properties (min mark (point-max)) (point-max))
     "\n" t)))
"##;

fn ac_mozc_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_MOZC_MELPA_PIN, "ac-mozc.el")
        .expect("prepare pinned ac-mozc source below ./tmp")
        .with_prelude(AC_MOZC_TEST_PRELUDE)
        .with_timeout(AC_MOZC_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-mozc parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_mozc_parity` cases (2a).
pub(crate) fn assert_ac_mozc_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_mozc_oracle(), &name, "ac_mozc_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_mozc_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_mozc_batch(&cases);
}

// END generated package batch tests
