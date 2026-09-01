use std::time::Duration;

use crate::{ABYSS_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ABYSS_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// abyss is a plain `deftheme` with a single `((t ...))` display spec, so every
/// workflow can enter through `load-theme'/`abyss-theme' and read back the
/// appearance a user actually sees.  The helpers below resolve `:inherit'
/// (`face-attribute ... nil t') because several abyss faces only become
/// visible through inheritance, and they read both the `face' and the
/// `font-lock-face' text property because Emacs only aliases the latter into
/// the former once `font-lock-mode' is on, which a batch job never allows.
const ABYSS_THEME_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defun neomacs-abyss-test-primary-face (value)
  "Return the first real face named by a `face' text-property VALUE."
  (cond
   ((and (symbolp value) (facep value)) value)
   ((listp value)
    (cl-find-if
     (lambda (candidate)
       (and (symbolp candidate) (facep candidate)))
     value))))

(defun neomacs-abyss-test-face-state (faces)
  "Resolved appearance of FACES, following `:inherit'."
  (mapcar
   (lambda (face)
     (if (not (facep face))
         (list :face face :defined nil)
       (list
        :face face
        :defined t
        :foreground (face-attribute face :foreground nil t)
        :background (face-attribute face :background nil t)
        :weight (face-attribute face :weight nil t)
        :slant (face-attribute face :slant nil t)
        :underline (face-attribute face :underline nil t)
        :box (copy-tree (face-attribute face :box nil nil))
        :inherit (copy-tree (face-attribute face :inherit nil nil)))))
   faces))

(defun neomacs-abyss-test-token-state (tokens)
  "Face and the colours that face resolves to at each TOKEN in the buffer."
  (save-excursion
    (mapcar
     (lambda (token)
       (goto-char (point-min))
       (search-forward token)
       (let* ((position (match-beginning 0))
              (displayed (get-char-property position 'face))
              (font-lock (get-text-property position 'font-lock-face))
              (face
               (neomacs-abyss-test-primary-face
                (or displayed font-lock))))
         (list
          :token token
          :face (copy-tree displayed)
          :font-lock-face (copy-tree font-lock)
          :foreground (and face (face-attribute face :foreground nil t))
          :background (and face (face-attribute face :background nil t))
          :weight (and face (face-attribute face :weight nil t))
          :slant (and face (face-attribute face :slant nil t)))))
     tokens)))

(defun neomacs-abyss-test-cleanup (root)
  (dolist (theme '(abyss neomacs-abyss-baseline))
    (when (custom-theme-enabled-p theme)
      (disable-theme theme)))
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer)))
      (when (and file (string-prefix-p root file))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (ignore-errors (kill-buffer buffer)))))
  (when (file-exists-p root)
    (delete-directory root t)))
"####;

fn abyss_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ABYSS_THEME_MELPA_PIN, "abyss-theme.el")
        .expect("prepare pinned abyss-theme source below ./tmp")
        .with_prelude(ABYSS_THEME_TEST_PRELUDE)
        .with_timeout(ABYSS_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed abyss-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_abyss_theme_parity` cases (2a).
pub(crate) fn assert_abyss_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(abyss_theme_oracle(), &name, "abyss_theme_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn abyss_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_abyss_theme_batch(&cases);
}

// END generated package batch tests
