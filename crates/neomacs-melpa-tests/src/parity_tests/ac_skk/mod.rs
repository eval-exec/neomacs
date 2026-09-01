use std::time::Duration;

use crate::{AC_SKK_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_SKK_TEST_TIMEOUT: Duration = Duration::from_secs(240);

/// ac-skk turns DDSKK, the Japanese input method, into auto-complete sources.
/// `ac-source-skk' fires while SKK is in henkan mode (`▽よみ') and offers every
/// conversion of the typed reading *and* of the readings the personal
/// dictionary completes it to; choosing one runs `ac-skk-kakutei', which
/// re-inserts the reading and drives `skk-start-henkan' to the chosen index.
/// `ac-source-skk-hiracomp' fires on plain kana, segments it with
/// tinysegmenter and offers conversions of the trailing segment.
///
/// Nothing here needs an external program: DDSKK reads its personal dictionary
/// (`skk-jisyo') straight off disk, so the workflows write a real SKK-JISYO
/// into the sandbox and let the package's own search, completion, conversion
/// and learning run against it.  All input is real Japanese typed through
/// `execute-kbd-macro', so a wrong conversion or a mangled encoding fails the
/// test rather than passing quietly.
const AC_SKK_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'skk)
(require 'auto-complete)

(defvar ac-skk-test-root
  (file-name-as-directory
   (expand-file-name "skk" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(make-directory ac-skk-test-root t)

;; A self-contained DDSKK: personal dictionary in the sandbox, no large
;; dictionary, no dictionary server, no init file, no annotations.
(setq skk-jisyo (expand-file-name "jisyo" ac-skk-test-root)
      skk-init-file (expand-file-name "skk-init.el" ac-skk-test-root)
      skk-large-jisyo nil
      skk-cdb-large-jisyo nil
      skk-aux-large-jisyo nil
      skk-server-host nil
      skk-show-annotation nil)

(defconst ac-skk-test-jisyo-entries
  (concat ";; okuri-ari entries.\n"
          ";; okuri-nasi entries.\n"
          "かんじ /漢字/感じ/幹事/監事/\n"
          "かんじゃ /患者/\n"
          "かんじょう /勘定/感情/\n"
          "かんきょう /環境/\n"
          "にほんご /日本語/\n"
          "にほん /日本/二本/\n"
          "ご /語/五/\n")
  "A real SKK-JISYO: readings mapped to their conversion candidates.")

(defconst ac-skk-test-document
  (concat "# 会議メモ\n"
          "\n"
          "本日の議題\n"
          "\n"
          "参加者\n"
          "\n"
          "場所\n"
          "\n"
          "時間\n"
          "\n"
          "決定事項\n"
          "\n"
          "次回の予定\n"
          "\n"
          "以上\n")
  "A short Japanese document with room below point for the completion menu.")

(defun ac-skk-test-write (path text)
  "Write TEXT to PATH as UTF-8 and return PATH."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun ac-skk-test-write-bytes (path text coding)
  "Write TEXT to PATH as raw CODING bytes and return PATH.
`encode-coding-string' produces the bytes and they are written with `binary',
so a fixture in a legacy Japanese encoding does not depend on the file writer."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'binary))
    (with-temp-buffer
      (set-buffer-multibyte nil)
      (insert (encode-coding-string text coding))
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun ac-skk-test-file-bytes (path)
  "Return the exact bytes of PATH as a list of integers."
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (append (buffer-string) nil)))

(defun ac-skk-test-install-jisyo (&optional coding)
  "Write the fixture SKK-JISYO and make DDSKK read it back correctly.
With no CODING the dictionary is UTF-8 and `skk-jisyo-code' is set to match,
which is what a modern user configures.  Passing a coding writes the fixture
in it and leaves `skk-jisyo-code' alone, so DDSKK's own default still applies."
  (if coding
      (ac-skk-test-write-bytes skk-jisyo ac-skk-test-jisyo-entries coding)
    (setq skk-jisyo-code 'utf-8)
    (ac-skk-test-write skk-jisyo ac-skk-test-jisyo-entries)))

(defun ac-skk-test-open (name)
  "Visit a fresh Japanese document called NAME with ac-skk armed.
Turning on `skk-mode' is what installs ac-skk: its `skk-mode-hook' entry
swaps `ac-sources' over to `ac-skk-special-sources'."
  (let ((buffer (find-file-noselect
                 (ac-skk-test-write
                  (expand-file-name name ac-skk-test-root)
                  ac-skk-test-document))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (ac-skk-enable)
    (skk-mode 1)
    (auto-complete-mode 1)
    (goto-char (point-min))
    (forward-line 2)
    (end-of-line)
    buffer))

(defun ac-skk-test-line ()
  (buffer-substring-no-properties
   (line-beginning-position) (line-end-position)))

(defun ac-skk-test-candidate-details (candidates)
  "Return each candidate with the reading, index and action ac-skk attached."
  (mapcar (lambda (candidate)
            (list (substring-no-properties candidate)
                  (get-text-property 0 'henkan-key candidate)
                  (get-text-property 0 'skk-count candidate)
                  (get-text-property 0 'action candidate)))
          candidates))

(defun ac-skk-test-menu ()
  (and (ac-menu-live-p)
       (mapcar #'substring-no-properties (popup-list ac-menu))))

(defun ac-skk-test-selected ()
  (let ((candidate (and (ac-menu-live-p) (ac-selected-candidate))))
    (and candidate (substring-no-properties candidate))))

(defun ac-skk-test-state ()
  "Return the buffer-visible input state a user would be looking at."
  (list :line (ac-skk-test-line)
        :point (point)
        :henkan-mode skk-henkan-mode
        :j-mode skk-j-mode))

(defun ac-skk-test-jisyo-buffer ()
  "Return the personal dictionary as DDSKK currently holds it in memory."
  (let ((buffer (skk-get-jisyo-buffer skk-jisyo 'nomsg)))
    (and buffer (with-current-buffer buffer
                  (buffer-substring-no-properties (point-min) (point-max))))))

(defun ac-skk-test-ac-state ()
  "Return the auto-complete configuration ac-skk installs or restores."
  (list :sources ac-sources
        :trigger-head (seq-take ac-trigger-commands 2)
        :saved-sources (and (local-variable-p 'ac-skk-ac-sources-orig)
                            ac-skk-ac-sources-orig)
        :trigger-is-local (local-variable-p 'ac-trigger-commands)))

(defun ac-skk-test-messages ()
  "Return ac-skk's own echo-area lines, in order."
  (with-current-buffer (get-buffer-create "*Messages*")
    (cl-remove-if-not
     (lambda (line) (string-match-p "\\`\\(enabled\\|disabled\\) ac-skk\\." line))
     (split-string
      (buffer-substring-no-properties (point-min) (point-max)) "\n" t))))
"##;

fn ac_skk_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_SKK_MELPA_PIN, "ac-skk.el")
        .expect("prepare pinned ac-skk source below ./tmp")
        .with_prelude(AC_SKK_TEST_PRELUDE)
        .with_timeout(AC_SKK_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed ac-skk parity test").into()
}

/// Multi-probe batch for `assert_ac_skk_parity` cases (2a).
pub(crate) fn assert_ac_skk_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_skk_oracle(), &name, "ac_skk_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_skk_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_skk_batch(&cases);
}

// END generated package batch tests
