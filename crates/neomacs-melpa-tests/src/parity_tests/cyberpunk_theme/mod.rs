use std::time::Duration;

use crate::{CYBERPUNK_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const CYBERPUNK_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Every workflow enters through `load-theme', the way a user enables a theme.
///
/// One property of this theme decides how it can be observed: its face
/// settings are written for the display `((class color) (min-colors 89))'.
/// A batch editor's frame is a 0-colour `mono' display, so that clause
/// matches nothing and the palette is registered but never realised -- in
/// both editors, which the first workflow pins explicitly with
/// `face-spec-set-match-display' so the reason is on the record.  The
/// workflows therefore assert the palette where it exists here: the specs
/// the theme registers on each face, which carry the exact colour strings,
/// so a wrong palette, a wrong display clause or a dropped face all fail.
/// They also assert resolved appearance with `face-attribute ... nil t'
/// around `load-theme'/`disable-theme', which is what proves the theme
/// leaves nothing behind, including the `custom-theme-set-variables'
/// entries (`ansi-color-names-vector', `fci-rule-color').
///
/// The theme reads its `cyberpunk-transparent-background' toggle while the
/// file is being loaded, so a toggle only takes effect on the next
/// `load-theme'.  On this platform the toggle is a documented no-op: it
/// only rewrites `cyberpunk-black' when the display is a darwin terminal,
/// so the toggle workflow pins the agreed same-colour reload instead.
const CYBERPUNK_THEME_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
;; The theme's `custom-theme-set-variables' entry for
;; `ansi-color-names-vector' only applies to a variable that already exists:
;; GNU's `load-theme' leaves the variable unbound when `ansi-color' is not
;; loaded, so the live set/restore workflow requires it first.
(require 'ansi-color)

(defconst cyberpunk-test-upstream-tree
  "ba60092e03567df0d581b1d8504f7d814c6de122"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst cyberpunk-test-manifest
  '(("cyberpunk-theme-pkg.el"
     . "91c663ac6775cdda247f498047208cdb2c781c89a871094d7c161bfcedc092b6")
    ("cyberpunk-theme.el"
     . "9fb69436c074b82a62b78b8d733e6274d0bd16d156f7b094e2afe4345c040c49"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defconst cyberpunk-test-probed-faces
  '(default cursor fringe highlight
    mode-line mode-line-inactive minibuffer-prompt
    vertical-border trailing-whitespace secondary-selection
    link link-visited header-line
    font-lock-keyword-face font-lock-string-face font-lock-comment-face
    font-lock-function-name-face font-lock-variable-name-face
    font-lock-constant-face font-lock-type-face font-lock-builtin-face
    font-lock-doc-face font-lock-preprocessor-face font-lock-warning-face
    font-lock-reference-face c-annotation-face
    isearch isearch-fail lazy-highlight
    show-paren-match show-paren-mismatch
    whitespace-tab whitespace-line
    org-level-1 org-level-2 org-level-3 org-link
    dired-symlink-face
    ;; Quirks worth pinning verbatim:
    mc/cursor-face        ; stray `nil,' symbol in the attribute plist
    gnus-summary-low-read ; a plain `(t ...)' clause among the `class' ones
    button                ; `(t (:underline t))'
    border-glyph          ; registered with nil attributes
    toolbar)
  "The faces an editing session actually shows, probed in every workflow.")

(defun cyberpunk-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "cyberpunk-theme.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/cyberpunk-theme.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed cyberpunk-theme location: %S" located))
    (dolist (entry cyberpunk-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed cyberpunk-theme source: %S"
                   (car entry))))))
    (list :upstream-tree cyberpunk-test-upstream-tree
          :feature (featurep 'cyberpunk-theme)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'cyberpunk-theme package-alist))))
          :theme-load-path
          (and (member directory custom-theme-load-path) t))))

(defun cyberpunk-test-registered (faces)
  "Return (FACE DISPLAY ATTRIBUTES) for each of FACES the theme has registered."
  (mapcar (lambda (face)
            (let ((entry (assq 'cyberpunk (get face 'theme-face))))
              (if (null entry)
                  (list face :not-registered)
                (let ((clause (car (cadr entry))))
                  (list face (car clause) (cadr clause))))))
          faces))

(defun cyberpunk-test-resolved (faces)
  "Resolved appearance of FACES on this display, following `:inherit'.
Faces the session has not defined (obsolete or package-owned names the
theme still registers) report `:defined nil' instead of signalling
`face-attribute's \"Invalid face\"."
  (mapcar (lambda (face)
            (if (not (facep face))
                (list face :defined nil)
              (list face
                    :foreground (face-attribute face :foreground nil t)
                    :background (face-attribute face :background nil t)
                    :weight (face-attribute face :weight nil t)
                    :box (copy-tree (face-attribute face :box nil t)))))
          faces))

(defun cyberpunk-test-display-facts ()
  "Why this display does or does not match the theme's display clause."
  (list :graphic (display-graphic-p)
        :daemon (daemonp)
        :display-type (frame-parameter nil 'display-type)
        :color-cells (display-color-cells)
        :matches-theme-clause
        (face-spec-set-match-display '((class color) (min-colors 89))
                                     (selected-frame))
        :matches-any-display (face-spec-set-match-display t (selected-frame))))

(defun cyberpunk-test-reset ()
  "Disable every theme the workflows may have enabled."
  (dolist (theme (copy-sequence custom-enabled-themes))
    (disable-theme theme))
  (dolist (theme '(cyberpunk cyberpunk-test-overlay))
    (when (custom-theme-enabled-p theme)
      (disable-theme theme))))

(defun cyberpunk-test-registered-with (variable value faces)
  "Load the theme with VARIABLE set to VALUE and return the registered FACES.
The theme reads its toggles as the file is loaded, so each one needs its own
`load-theme'."
  (cyberpunk-test-reset)
  (set variable value)
  (unwind-protect
      (progn
        (load-theme 'cyberpunk t)
        (cyberpunk-test-registered faces))
    (cyberpunk-test-reset)
    (set variable nil)))
"##;

fn cyberpunk_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CYBERPUNK_THEME_MELPA_PIN, "cyberpunk-theme.el")
        .expect("prepare pinned cyberpunk-theme source below ./tmp")
        .with_prelude(CYBERPUNK_THEME_TEST_PRELUDE)
        .with_timeout(CYBERPUNK_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed cyberpunk-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_cyberpunk_theme_parity` cases (2a).
pub(crate) fn assert_cyberpunk_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        cyberpunk_theme_oracle(),
        &name,
        "cyberpunk_theme_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn cyberpunk_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_cyberpunk_theme_batch(&cases);
}

// END generated package batch tests
