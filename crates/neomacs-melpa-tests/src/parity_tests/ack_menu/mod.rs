use std::time::Duration;

use crate::{ACK_MENU_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACK_MENU_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Sandbox helpers shared by the workflows.
///
/// ack-menu is a front end for the `ack' command line tool, so the external
/// boundary the workflows fake is `ack' itself: `ack-test-setup' writes a
/// recording stand-in below `NEOMACS_TEST_SANDBOX_ROOT' and puts it on
/// `exec-path'/`PATH'.  The stand-in answers `--version' the way ack 2.14
/// does, records its exact argument vector and working directory, and then
/// performs a real search over the sandbox tree, printing the grouped,
/// `--color' SGR output shape that ack produces (`ESC[1;32m' file names,
/// `ESC[1;33m' line numbers, `ESC[30;43m' matches).  Everything else -- the
/// mag-menu interface, the option assembly in `ack-process-args', the process
/// handling, the SGR parser, the faces and text properties, the results buffer
/// and its navigation -- is the package's own code.
///
/// `ack-test-restore-ansi-color-constants' restores two constants that
/// ansi-color.el deleted in Emacs 26.1 (commit 35ed01dfb3f, 2017-06-15):
/// `ansi-color-regexp' and `ansi-color-drop-regexp', with their historical
/// definitions.  `ack-parse-sgr-sequences' still reads both, so without them
/// every ack process filter signals `void-variable' and no output ever reaches
/// the results buffer -- which in batch GNU Emacs kills the editor outright
/// (see the commit message; Neomacs instead swallows the filter error
/// silently, a divergence in its own right).  Restoring the two constants is
/// the workaround a user of this 2015 package needs today, and it is the only
/// thing the workflows patch outside the `ack' executable.
const ACK_MENU_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar ack-test-root
  (file-name-as-directory
   (expand-file-name "project" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defvar ack-test-bin
  (file-name-as-directory
   (expand-file-name "bin" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defvar ack-test-log
  (expand-file-name "ack-invocations.log" (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar ack-test-pristine-buffers nil)

(defun ack-test-reset-case-state ()
  "Restore editor and package state changed by an ack-menu workflow."
  ;; The oracle evaluates the prelude before loading package source, so take
  ;; the baseline lazily when the first case begins.
  (unless ack-test-pristine-buffers
    (setq ack-test-pristine-buffers (buffer-list)))
  (when (processp ack-process)
    (delete-process ack-process))
  (setq ack-process nil
        ack-buffer--rerun-args nil
        ack-parse-sgr-context nil
        ack-error-pos nil
        ack-menu-current-state nil
        ack-menu-match-history nil
        ack-directory-history nil
        ack-literal-history nil
        ack-regexp-history nil
        mag-menu-current-args nil
        mag-menu-current-options nil
        mag-menu-previous-window-config nil
        mag-menu-prefix nil
        next-error-last-buffer nil)
  (dolist (buffer (buffer-list))
    (unless (memq buffer ack-test-pristine-buffers)
      (kill-buffer buffer)))
  (delete-other-windows)
  (when-let ((scratch (get-buffer "*scratch*")))
    (set-window-buffer (selected-window) scratch)
    (set-buffer scratch))
  (when-let ((messages (get-buffer "*Messages*")))
    (with-current-buffer messages
      (let ((inhibit-read-only t))
        (erase-buffer)))))

;; Recording stand-in for ack.  Records argv and the working directory, then
;; searches the sandbox tree and prints ack's grouped --color output.
(defconst ack-test-ack-program
  "#!/bin/sh
case \"$1\" in
  --version)
    printf 'ack 2.14\\n'
    printf 'Running under Perl 5.30.0 at /usr/bin/perl\\n'
    exit 0 ;;
esac
{ printf 'argv'
  for argument in \"$@\"; do printf '\\037%s' \"$argument\"; done
  printf '\\n'
  printf 'cwd\\037%s\\n' \"$PWD\"
} >> \"$ACK_LOG\"

pattern=
flags=
sedflags=g
only_files=
list_files=
print0=
for argument in \"$@\"; do
  case \"$argument\" in
    --match=*) pattern=${argument#--match=} ;;
    --ignore-case) flags=\"$flags -i\"; sedflags=gI ;;
    --word-regexp) flags=\"$flags -w\" ;;
    --literal) flags=\"$flags -F\" ;;
    --files-with-matches) only_files=1 ;;
    -f) list_files=1 ;;
    --print0) print0=1 ;;
    --color|--nopager|--all|--no-recurse) ;;
    -*) printf 'ack: Unknown option: %s\\n' \"$argument\" >&2; exit 2 ;;
  esac
done

escape=$(printf '\\033')
result=\"$ACK_LOG.out\"
: > \"$result\"

if [ -n \"$list_files\" ]; then
  find . -type f -not -path './.git/*' | LC_ALL=C sort | while IFS= read -r file; do
    if [ -n \"$print0\" ]; then printf '%s\\0' \"${file#./}\"
    else printf '%s\\n' \"${file#./}\"; fi
  done
  exit 0
fi

find . -type f -not -path './.git/*' | LC_ALL=C sort | while IFS= read -r file; do
  hits=$(grep -n $flags -e \"$pattern\" \"$file\" 2>/dev/null) || continue
  [ -n \"$hits\" ] || continue
  printf '%s[1;32m%s%s[0m\\n' \"$escape\" \"${file#./}\" \"$escape\" >> \"$result\"
  if [ -z \"$only_files\" ]; then
    printf '%s\\n' \"$hits\" | while IFS= read -r hit; do
      number=${hit%%:*}
      text=${hit#*:}
      printf '%s[1;33m%s%s[0m:%s\\n' \"$escape\" \"$number\" \"$escape\" \\
        \"$(printf '%s' \"$text\" | sed \"s|$pattern|$escape[30;43m&$escape[0m|$sedflags\")\" \\
        >> \"$result\"
    done
    printf '\\n' >> \"$result\"
  fi
done

cat \"$result\"
if [ -s \"$result\" ]; then exit 0; fi
exit 1
")

;; A small project: a Unicode document, a file name with a space, and a
;; capitalised match that only a case insensitive search finds.
(defconst ack-test-files
  '(("src/main.el" . ";;; main.el --- demo project\n(defun handler (request)\n  (message \"handler ready\"))\n")
    ("src/notes with space.txt" . "the handler notes\nsecond line without a match\n")
    ("docs/readme.md" . "# Résumé\nThe café handler serves naïve clients.\n")
    ("docs/CHANGELOG" . "Handler rewritten\nnothing here\n")))

(defun ack-test-write-executable (name body)
  (let ((path (expand-file-name name ack-test-bin)))
    (make-directory ack-test-bin t)
    (with-temp-buffer
      (insert body)
      (write-region (point-min) (point-max) path nil 'silent))
    (set-file-modes path #o755)
    path))

(defun ack-test-setup ()
  "Build the search tree and install the recording `ack' stand-in."
  (ack-test-reset-case-state)
  (when (file-directory-p ack-test-root)
    (delete-directory (directory-file-name ack-test-root) t))
  (make-directory ack-test-root t)
  (make-directory (expand-file-name ".git" ack-test-root) t)
  (dolist (entry ack-test-files)
    (let ((path (expand-file-name (car entry) ack-test-root))
          (coding-system-for-write 'utf-8-unix))
      (make-directory (file-name-directory path) t)
      (with-temp-buffer
        (insert (cdr entry))
        (write-region (point-min) (point-max) path nil 'silent))))
  (when (file-exists-p ack-test-log)
    (delete-file ack-test-log))
  (setenv "ACK_LOG" ack-test-log)
  (setenv "PATH" (concat (directory-file-name ack-test-bin)
                         path-separator (getenv "PATH")))
  (add-to-list 'exec-path (directory-file-name ack-test-bin))
  (setq ack-executable (ack-test-write-executable "ack" ack-test-ack-program)
        ack-menu-options '(("--ignore-case"))
        ack-arguments nil)
  ack-test-root)

(defun ack-test-restore-ansi-color-constants ()
  "Restore the constants ansi-color.el deleted in Emacs 26.1."
  (eval '(defvar ansi-color-regexp "\033\\[\\([0-9;]*m\\)") t)
  (eval '(defvar ansi-color-drop-regexp
           "\033\\[\\([ABCDsuK]\\|[12][JK]\\|=[0-9]+[hI]\\|[0-9;]*[Hf]\\|\\?[0-9]+[hl]\\)")
        t))

(defun ack-test-open (relative-path search)
  "Visit RELATIVE-PATH in a window, with point on SEARCH."
  (let ((buffer (find-file-noselect (expand-file-name relative-path ack-test-root))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (goto-char (point-min))
    (search-forward search)
    (goto-char (match-beginning 0))
    buffer))

(defun ack-test-wait ()
  "Wait for the ack process and its sentinel."
  (let ((limit 400))
    (while (and (> limit 0)
                (processp ack-process)
                (process-live-p ack-process))
      (setq limit (1- limit))
      (accept-process-output ack-process 0.05))
    (accept-process-output nil 0.05)
    (sit-for 0.05)
    (> limit 0)))

(defun ack-test-invocations ()
  "Every argument vector and working directory the stand-in recorded."
  (if (file-exists-p ack-test-log)
      (with-temp-buffer
        (let ((coding-system-for-read 'utf-8-unix))
          (insert-file-contents ack-test-log))
        (mapcar (lambda (line)
                  (let ((fields (split-string line "\037")))
                    (if (equal (car fields) "cwd")
                        (list "cwd" (file-relative-name (cadr fields) ack-test-root))
                      fields)))
                (split-string (buffer-string) "\n" t)))
    'nothing-recorded))

(defun ack-test-results-text ()
  (let ((buffer (get-buffer "*ack*")))
    (and buffer
         (with-current-buffer buffer
           (buffer-substring-no-properties (point-min) (point-max))))))

(defun ack-test-results-segments ()
  "Return (TEXT FONT-LOCK-FACE ACK-FILE ACK-LINE ACK-MATCH) for every run."
  (let ((buffer (get-buffer "*ack*")))
    (and buffer
         (with-current-buffer buffer
           (let (segments (pos (point-min)))
             (while (< pos (point-max))
               (let ((next (next-property-change pos nil (point-max))))
                 (push (list (buffer-substring-no-properties pos next)
                             (get-text-property pos 'font-lock-face)
                             (and (get-text-property pos 'ack-file)
                                  (substring-no-properties
                                   (get-text-property pos 'ack-file)))
                             (and (get-text-property pos 'ack-line)
                                  (substring-no-properties
                                   (get-text-property pos 'ack-line)))
                             (get-text-property pos 'ack-match))
                       segments)
                 (setq pos next)))
             (nreverse segments))))))

(defun ack-test-results-state ()
  (let ((buffer (get-buffer "*ack*")))
    (and buffer
         (with-current-buffer buffer
           (list :mode major-mode
                 :read-only buffer-read-only
                 :directory (file-relative-name default-directory ack-test-root)
                 :next-error-function next-error-function
                 :size (buffer-size))))))

(defun ack-test-window-state ()
  "Where the user ends up: the selected window's buffer, line and column."
  (let ((buffer (window-buffer (selected-window))))
    (with-current-buffer buffer
      (list (buffer-name buffer)
            (point)
            (line-number-at-pos)
            (current-column)
            (buffer-substring-no-properties
             (line-beginning-position) (line-end-position))))))

(defun ack-test-message-mark ()
  (with-current-buffer (get-buffer-create "*Messages*") (point-max)))

(defun ack-test-messages-since (mark)
  (with-current-buffer (get-buffer-create "*Messages*")
    (split-string
     (buffer-substring-no-properties (min mark (point-max)) (point-max))
     "\n" t)))

(defun ack-test-menu-text ()
  (let ((buffer (get-buffer "*mag-menu*")))
    (and buffer
         (with-current-buffer buffer
           (buffer-substring-no-properties (point-min) (point-max))))))

(defun ack-test-menu-state ()
  (let ((buffer (get-buffer "*mag-menu*")))
    (and buffer
         (with-current-buffer buffer
           (list mag-menu-current-options
                 (let (args)
                   (maphash (lambda (name value)
                              (push (cons name (ack-test-relative value)) args))
                            mag-menu-current-args)
                   (sort args (lambda (a b) (string< (car a) (car b))))))))))

(defun ack-test-relative (value)
  (if (and (stringp value) (string-prefix-p ack-test-root value))
      (let ((relative (file-relative-name value ack-test-root)))
        (if (equal relative "./") "./" (concat "./" relative)))
    value))

(defun ack-test-options ()
  (mapcar (lambda (option)
            (cons (car option) (ack-test-relative (cdr option))))
          ack-menu-options))
"##;

fn ack_menu_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACK_MENU_MELPA_PIN, "ack-menu.el")
        .expect("prepare pinned ack-menu source below ./tmp")
        .with_prelude(ACK_MENU_TEST_PRELUDE)
        .with_timeout(ACK_MENU_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ack-menu parity test")
        .into()
}

/// Multi-probe batch for `assert_ack_menu_parity` cases (2a).
pub(crate) fn assert_ack_menu_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ack_menu_oracle(), &name, "ack_menu_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ack_menu_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ack_menu_batch(&cases);
}

// END generated package batch tests
