//! Practical parity for the kaolin theme family.
//!
//! Seventeen themes built with autothemer over one shared kaolin-themes
//! core.  Every workflow enters through `load-theme' (with
//! `custom-safe-themes' trusted: the batch editor cannot answer GNU's
//! theme-safety prompt, the documented batch setup) and pins the
//! REGISTERED theme-face specs -- a batch frame is a 0-colour mono
//! display, so `((class color) ...)' clauses never realize in either
//! editor; the palette exists as the specs the themes register.

use std::time::Duration;

use crate::{AUTOTHEMER_MELPA_PIN, CachedMelpaOracle, KAOLIN_THEMES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(setq custom-safe-themes t)

(defconst kaolin-test-upstream-tree
  "e615d9047d53b6d3153af6a0724a1a64f722d769"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst kaolin-test-manifest
  '(
    ("kaolin-aurora-theme.el"
     . "f2f07719428af0572740e5f962677173777b439349ec6abc0db2db8d4ce3de41")
    ("kaolin-blossom-theme.el"
     . "b094d1a522d3ffa94b09811ae6af217bb01579a1f58d13b4f517306d94e6ea4e")
    ("kaolin-breeze-theme.el"
     . "3afa642b2bfcd45a84b2725a87a88f0063b9787162b067bb8e614e7301be69e1")
    ("kaolin-bubblegum-theme.el"
     . "f7a004e3a1f920d79a02988fd20a9af9f5cedb1491918e1ef247bf7481cbe93b")
    ("kaolin-dark-theme.el"
     . "866c42bcd430df48c64e86694fa62737fc0eb2bfbbe6f68d7d95dd9f05d63bb3")
    ("kaolin-eclipse-theme.el"
     . "f5823ed842375c0874f876b352f624ef46f00f7ecdcb43d433f248307b3f6b86")
    ("kaolin-galaxy-theme.el"
     . "ca42424bb1ce63edd27fe89d0958bc78a9e8f6032db1c15b45fbb88cfe823ddd")
    ("kaolin-light-theme.el"
     . "ded0cd1b37a2633465fd30dca9b7aa9b5e57dca6d3337bf80d54c9ebd241e755")
    ("kaolin-mono-dark-theme.el"
     . "69aafaf59656887750bfeeb890d81173d41cca498b362928f5d240be591a0add")
    ("kaolin-mono-light-theme.el"
     . "5de3e12da5f2cc8fbf99923964ad4367720e7fcb97e146e067f4db8799a86f5f")
    ("kaolin-ocean-theme.el"
     . "606d9e3d98ef969d4e9eba9628ee1ce25934ecfe72cd0a995d31c907cb69dafd")
    ("kaolin-shiva-theme.el"
     . "ba7682deadc8cefeeb07a1198ba4b777c9fd1cdab7c7798892a5aab598336baa")
    ("kaolin-temple-theme.el"
     . "b2b5e96a5cdda0a5dce3420e0eac37276231a6a5e79816d20040f38075d81fe3")
    ("kaolin-themes.el"
     . "52a3a680a87775efe6a817aaf6375e685d2f0912e3e4b7ba4dff8f47ed43c000")
    ("kaolin-themes-lib.el"
     . "8c7b8f04690ca98463f3d813da19a900c2a48a1cc95f1738e35d241829d34abb")
    ("kaolin-themes-pkg.el"
     . "fa8825d3db7247e1bd1f22e7d8d0a1a63850d46d679b5e586febd17734f5a833")
    ("kaolin-themes-treemacs.el"
     . "694676b53b4ded30dcfde6e73b2459dd9e96c3b608f3c3b4e8c716f7ea4e0969")
    ("kaolin-valley-dark-theme.el"
     . "8f5b7d56999c86f00982cfbfd8e6cad9b4d6eb0f561c4f66e89d414ceb3bd29c")
    ("kaolin-valley-light-theme.el"
     . "3a8520a563ef5719ef22438ceb5f2ba6c43cf6f7103aaf3686a3c3183365ca6f")
  )
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun kaolin-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "kaolin-themes.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/kaolin-themes.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed kaolin-themes location: %S" located))
    (dolist (entry kaolin-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed kaolin source: %S"
                   (car entry))))))
    (list :upstream-tree kaolin-test-upstream-tree
          :feature (featurep 'kaolin-themes)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'kaolin-themes package-alist))))
          :autothemer (package-version-join
                       (package-desc-version
                        (cadr (assq 'autothemer package-alist))))
          :theme-load-path
          (and (member directory custom-theme-load-path) t))))

(defun kaolin-test-registered (theme faces)
  "Return (FACE ATTRIBUTES) for each of FACES THEME has registered."
  (mapcar (lambda (face)
            (let ((entry (assq theme (get face 'theme-face))))
              (if (null entry)
                  (list face :not-registered)
                (let ((clause (car (cadr entry))))
                  (list face (car clause) (cadr clause))))))
          faces))

(defun kaolin-test-reset ()
  "Disable every theme the workflows may have enabled."
  (dolist (theme (copy-sequence custom-enabled-themes))
    (disable-theme theme)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(KAOLIN_THEMES_MELPA_PIN, "kaolin-themes.el")
        .expect("prepare pinned kaolin-themes source below ./tmp")
        .with_melpa_dependency(AUTOTHEMER_MELPA_PIN)
        .expect("prepare pinned autothemer dependency")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn kaolin_themes_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(
        oracle(),
        "kaolin_themes_package_batch",
        "kaolin_themes_parity",
        &cases,
    );
}
