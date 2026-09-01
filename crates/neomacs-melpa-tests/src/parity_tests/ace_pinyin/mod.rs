use std::time::Duration;

use crate::{ACE_PINYIN_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACE_PINYIN_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ace-pinyin makes avy's jump commands find a Chinese character by the first
/// letter of its pinyin, so every workflow needs a real window-displayed buffer
/// of mixed Chinese and Latin text, a real avy key binding, and real key input:
/// the command prompts for the query letter with `read-char', then avy reads one
/// more key to pick a candidate.
///
/// Those two reads are answered through `unread-command-events', the standard
/// scripted-input path, rather than `execute-kbd-macro'.  Nothing of the package
/// is stubbed: `key-binding' resolves the user's real avy binding,
/// `call-interactively' runs the real command with its real `interactive' form,
/// pinyinlib builds the real regexp and avy collects, labels and jumps to the
/// real candidates.
///
/// `avy-last-candidates' is avy's own record of the candidate list it just
/// offered (it is what `avy-next'/`avy-prev' resume from), which lets a workflow
/// pin the complete ordered set of jumpable positions without pressing keys that
/// do not exist.
const ACE_PINYIN_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

;; Realistic mixed notes: simplified Chinese next to Latin words, Chinese and
;; ASCII punctuation, and one line of traditional Chinese.
(defconst apy-test-notes
  (concat "北京大学 Peking University\n"
          "上海交通大学 Shanghai Jiao Tong\n"
          "中文输入法 Chinese input method\n"
          "你好，世界！Hello, world.\n"
          "《汉语大词典》 traditional 學習漢語。\n"))

(defun apy-test-setup ()
  "Pin avy's reading keys and style so candidate labels are stable."
  (setq avy-keys '(?a ?s ?d ?f ?g ?h ?j ?k ?l)
        avy-style 'at-full
        avy-all-windows t
        avy-single-candidate-jump t))

(defun apy-test-buffer (&optional text name)
  "Display a work buffer holding TEXT so the avy keys reach it."
  (let ((buffer (generate-new-buffer (or name "*ace-pinyin-workflow*"))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (insert (or text apy-test-notes))
    (goto-char (point-min))
    (set-buffer-modified-p nil)
    buffer))

(defun apy-test-where ()
  "Describe where point is: position, character, line and column."
  (list (point)
        (if (char-after) (char-to-string (char-after)) 'end-of-buffer)
        (line-number-at-pos)
        (- (point) (line-beginning-position))))

(defun apy-test-jumpable ()
  "Return (POSITION CHARACTER) for every candidate avy last offered, in order."
  (mapcar (lambda (candidate)
            (let* ((where (car candidate))
                   (start (if (consp where) (car where) where)))
              (list start
                    (if (char-after start)
                        (char-to-string (char-after start))
                      'end-of-buffer))))
          avy-last-candidates))

(defun apy-test-press (binding keys &optional origin)
  "Invoke the command bound to BINDING from ORIGIN, answering reads with KEYS.

KEYS supplies both the query character the command prompts for and the
key avy reads to pick a candidate."
  (goto-char (or origin (point-min)))
  (unwind-protect
      (progn
        (setq unread-command-events (listify-key-sequence (kbd keys)))
        (call-interactively (key-binding (kbd binding)))
        (apy-test-where))
    (setq unread-command-events nil)))

(defun apy-test-offer (binding keys &optional origin)
  "Press BINDING with KEYS and report the landing plus every offered candidate."
  (let ((landing (apy-test-press binding keys origin)))
    (list :landing landing :candidates (apy-test-jumpable))))

(defun apy-test-owner (command original)
  "Say who owns COMMAND's function cell: ace-pinyin's replacement or avy's."
  (let ((cell (symbol-function command)))
    (cond ((symbolp cell) cell)
          ((eq cell (symbol-value original)) 'avy-original)
          (t 'unknown))))

(defun apy-test-message-mark ()
  (with-current-buffer (get-buffer-create "*Messages*") (point-max)))

(defun apy-test-messages-since (mark)
  (with-current-buffer (get-buffer-create "*Messages*")
    (split-string
     (buffer-substring-no-properties (min mark (point-max)) (point-max))
     "\n" t)))
"##;

fn ace_pinyin_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACE_PINYIN_MELPA_PIN, "ace-pinyin.el")
        .expect("prepare pinned ace-pinyin source below ./tmp")
        .with_prelude(ACE_PINYIN_TEST_PRELUDE)
        .with_timeout(ACE_PINYIN_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ace-pinyin parity test")
        .into()
}

/// Multi-probe batch for `assert_ace_pinyin_parity` cases (2a).
pub(crate) fn assert_ace_pinyin_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ace_pinyin_oracle(), &name, "ace_pinyin_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ace_pinyin_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ace_pinyin_batch(&cases);
}

// END generated package batch tests
