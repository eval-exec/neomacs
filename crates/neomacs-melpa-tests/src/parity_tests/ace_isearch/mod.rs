use std::time::Duration;

use crate::{ACE_ISEARCH_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACE_ISEARCH_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ace-isearch decides between a jump backend, plain isearch and a
/// line-searching backend from `isearch-update-post-hook`, so every workflow
/// has to run a real isearch session driven by real keys.
/// `execute-kbd-macro` delivers them to the buffer of the selected window,
/// which is why the fixture displays the work buffer instead of merely making
/// it current.
///
/// `avy`, `helm-swoop` and `swiper` are interactive overlay/popup UIs that read
/// a selection key from the user and are not installed alongside the package
/// (`ace-isearch` requires only Emacs 24), so they are the only doubles here.
/// They record what ace-isearch handed them and then perform the jump or the
/// line listing the user would have picked; every ace-isearch function --
/// `ace-isearch--jumper-function`, the `ace-isearch-*-from-isearch` adapters,
/// `ace-isearch-pop-mark`, `ace-isearch-jump-during-isearch` and both minor
/// modes -- runs for real.
const ACE_ISEARCH_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar ace-isearch-test-events nil)
(defvar ace-isearch-test-avy-nth 1)
(defvar ace-isearch-test-avy-marks nil)
(defvar avy-all-windows t)

(defvar ace-isearch-test-text
  "Release notes for the parser rewrite\nthe tokenizer now handles Unicode identifiers\nthe parser reports a precise column number\nfixture: naïve café resumé [see docs]\ntrailing summary line\n")

(defun ace-isearch-test--record (event)
  (setq ace-isearch-test-events (append ace-isearch-test-events (list event))))

(defun ace-isearch-test--candidates (regexp)
  (let ((found nil))
    (save-excursion
      (goto-char (point-min))
      (while (re-search-forward regexp nil t)
        (push (match-beginning 0) found)))
    (nreverse found)))

(defun ace-isearch-test--jump (backend regexp args)
  "Stand in for an avy jump: record the call, then select a candidate."
  (let ((candidates (ace-isearch-test--candidates regexp)))
    (ace-isearch-test--record
     (list backend args (point) isearch-string avy-all-windows candidates))
    (let ((target (nth ace-isearch-test-avy-nth candidates)))
      (when target
        (push (point) ace-isearch-test-avy-marks)
        (goto-char target))
      (point))))

(defun avy-goto-word-1 (char &optional _arg _beg _end _symbol)
  (ace-isearch-test--jump 'avy-goto-word-1
                          (concat "\\b" (regexp-quote (char-to-string char)))
                          (list char)))

(defun avy-goto-char (char &optional _arg _beg _end)
  (ace-isearch-test--jump 'avy-goto-char
                          (regexp-quote (char-to-string char))
                          (list char)))

(defun avy-goto-char-2 (char1 char2 &optional _arg _beg _end)
  (ace-isearch-test--jump 'avy-goto-char-2
                          (regexp-quote (string char1 char2))
                          (list char1 char2)))

(defun avy-isearch (&optional _arg)
  (ace-isearch-test--jump 'avy-isearch (regexp-quote isearch-string) nil))

(defun avy-pop-mark ()
  (ace-isearch-test--record (list 'avy-pop-mark (point)))
  (when ace-isearch-test-avy-marks
    (goto-char (pop ace-isearch-test-avy-marks)))
  (point))

(defun ace-isearch-test--swoop (backend query)
  "Stand in for a swoop-style UI: record the call, list matching lines."
  (let ((lines nil))
    (ace-isearch-test--record
     (list backend query (buffer-name) (point) isearch-mode))
    (save-excursion
      (goto-char (point-min))
      (while (not (eobp))
        (let ((line (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position))))
          (when (string-match-p query line)
            (push (format "%d: %s" (line-number-at-pos) line) lines)))
        (forward-line 1)))
    (with-current-buffer (get-buffer-create "*ace-isearch-swoop*")
      (erase-buffer)
      (insert (mapconcat #'identity (nreverse lines) "\n"))
      (buffer-string))))

(defun helm-swoop (&rest args)
  (ace-isearch-test--swoop 'helm-swoop (plist-get args :query)))

(defun swiper (&optional query)
  (ace-isearch-test--swoop 'swiper query))

(provide 'avy)
(provide 'helm-swoop)
(provide 'swiper)

(defmacro ace-isearch-test-with-live-buffer (&rest body)
  "Run BODY in a real, window-displayed buffer so typed keys reach it."
  `(let ((buffer (generate-new-buffer "*ace-isearch-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (insert ace-isearch-test-text)
           (goto-char (point-min))
           (setq ace-isearch-test-events nil)
           (when (get-buffer "*ace-isearch-swoop*")
             (kill-buffer "*ace-isearch-swoop*"))
           ,@body)
       (kill-buffer buffer))))

(defun ace-isearch-test-swoop-buffer ()
  (let ((buffer (get-buffer "*ace-isearch-swoop*")))
    (and buffer (with-current-buffer buffer (buffer-string)))))

(defun ace-isearch-test-last-message ()
  (with-current-buffer (get-buffer-create "*Messages*")
    (car (last (split-string (buffer-string) "\n" t)))))
"##;

fn ace_isearch_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACE_ISEARCH_MELPA_PIN, "ace-isearch.el")
        .expect("prepare pinned ace-isearch source below ./tmp")
        .with_prelude(ACE_ISEARCH_TEST_PRELUDE)
        .with_timeout(ACE_ISEARCH_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ace-isearch parity test")
        .into()
}

/// Multi-probe batch for `assert_ace_isearch_parity` cases (2a).
pub(crate) fn assert_ace_isearch_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ace_isearch_oracle(), &name, "ace_isearch_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ace_isearch_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ace_isearch_batch(&cases);
}

// END generated package batch tests
