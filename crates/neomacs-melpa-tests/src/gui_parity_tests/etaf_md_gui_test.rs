//! Real graphical parity for the pinned `etaf-md` preview page.
//!
//! The batch module already compares the page text.  This probe adds what only
//! a graphical frame shows: the resolved heading face, the page's pixel size,
//! and the frame/default-face metrics the page is painted with.  The probe
//! asserts strict GNU parity: Neomacs must produce GNU's exact observation and
//! there is no pinned Neomacs outcome to re-record.

use std::time::Duration;

use expect_test::expect;

use super::support::PackageGuiPair;
use crate::CachedMelpaOracle;

const GUI_PROBE: &str = r####"
(progn
  (require 'cl-lib)
  (set-face-attribute 'default nil :family "DejaVu Sans Mono" :height 100)
  (setq line-spacing nil)
  (require 'etaf-md)

  (defun etaf-md-gui-face-key (value key)
    "Resolve KEY from a face property VALUE: symbol, plist, or a list of both."
    (cond
     ((null value) nil)
     ((and (listp value) (keywordp (car value))) (plist-get value key))
     ((listp value)
      (seq-some (lambda (item) (etaf-md-gui-face-key item key)) value))
     (t (face-attribute value key nil t))))

  (defconst etaf-md-gui-document
    "# Title

Body text for parity.

## Section

- one
- two
")

  (let* ((session (etaf-md-create :text etaf-md-gui-document :theme 'dark))
         (buffer (etaf-md-open session)))
    (unwind-protect
        (with-current-buffer buffer
          (redisplay t)
          (let* ((window (get-buffer-window buffer t))
                 (title-pos (save-excursion
                              (goto-char (point-min))
                              (search-forward "Title" nil t)
                              (and (match-beginning 0) (match-beginning 0))))
                 (face (and title-pos (get-text-property title-pos 'face))))
            (list :text (buffer-substring-no-properties (point-min) (point-max))
                  :has-window (and window t)
                  :pixel-size (and window (window-text-pixel-size
                                           window (point-min) (point-max)))
                  :body-width (and window (window-body-width window))
                  :body-height (and window (window-body-height window))
                  :title-face face
                  :title-foreground (etaf-md-gui-face-key face :foreground)
                  :title-height (etaf-md-gui-face-key face :height)
                  :title-weight (etaf-md-gui-face-key face :weight)
                  :default-family (face-attribute 'default :family nil t)
                  :default-height (face-attribute 'default :height nil t)
                  :default-foreground (face-attribute 'default :foreground nil t)
                  :default-background (face-attribute 'default :background nil t)
                  :frame-width (frame-parameter nil 'width)
                  :frame-height (frame-parameter nil 'height)
                  :frame-background (frame-parameter nil 'background-color)
                  :frame-foreground (frame-parameter nil 'foreground-color))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (ignore-errors (etaf-md-close)))
        (when (buffer-live-p buffer) (kill-buffer buffer))))))
"####;

#[test]
fn etaf_md_real_gui_page_matches_gnu() {
    let oracle = CachedMelpaOracle::new(("etaf-md", "0.2.0"), "etaf-md.el")
        .expect("prepare pinned etaf-md source below ./tmp")
        .with_timeout(Duration::from_secs(180));
    let pair = PackageGuiPair::run(
        "etaf-md-real-gui-page",
        oracle.prepared_packages(),
        GUI_PROBE,
    )
    .expect("run real graphical GNU Emacs and Neomacs sequentially");

    let expected_gnu = expect![[
        r##"OK (:text "​ \n​ Title  \n​   \n​    \n​ Body text for parity.  \n​    \n​ Section  \n​   \n​    \n​ • one  \n​ • two  \n​    \n​ " :has-window t :pixel-size (800 . 300) :body-width 100 :body-height 33 :title-face ((:height 202 :weight bold) (:foreground "#e6edf3") (:background "#191a1c")) :title-foreground "#e6edf3" :title-height 202 :title-weight bold :default-family "DejaVu Sans Mono" :default-height 101 :default-foreground "black" :default-background "white" :frame-width 100 :frame-height 35 :frame-background "white" :frame-foreground "black")"##
    ]];
    expected_gnu.assert_eq(&pair.gnu_behavior.to_string());
    // Strict parity: the probe must produce GNU's exact observation.  There
    // is deliberately no pinned Neomacs outcome, so a divergence cannot be
    // blessed by re-recording it.
    assert_eq!(
        pair.neomacs_behavior.to_string(),
        pair.gnu_behavior.to_string(),
        "Neomacs must match GNU exactly; open ETAF GUI divergences are bugs to \
         fix, not values to re-pin"
    );
}
