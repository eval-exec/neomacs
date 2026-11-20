//! Real graphical parity for the pinned `etaf-org-preview` page.
//!
//! Complements the batch module's text/state probes with the resolved title
//! face, page pixel height and the frame/default-face metrics the page is
//! painted with in a real X11 frame.  The probe asserts strict GNU parity:
//! Neomacs must produce GNU's exact observation, with no Neomacs pin.

use std::time::Duration;

use expect_test::expect;

use super::support::PackageGuiPair;
use crate::CachedMelpaOracle;

const GUI_PROBE: &str = r####"
(progn
  (require 'cl-lib)
  (set-face-attribute 'default nil :family "DejaVu Sans Mono" :height 100)
  (setq line-spacing nil)
  (require 'etaf-org-preview)

  (defun etaf-org-gui-face-key (value key)
    "Resolve KEY from a face property VALUE: symbol, plist, or a list of both."
    (cond
     ((null value) nil)
     ((and (listp value) (keywordp (car value))) (plist-get value key))
     ((listp value)
      (seq-some (lambda (item) (etaf-org-gui-face-key item key)) value))
     (t (face-attribute value key nil t))))

  (defconst etaf-org-gui-document
    "#+TITLE: Demo
#+AUTHOR: Ada

* TODO First heading :work:
Body text for parity.
** DONE Second heading :home:
- [X] done item
- [ ] open item
")

  (let* ((session (etaf-org-preview-create :text etaf-org-gui-document :theme 'light))
         (buffer (generate-new-buffer " *etaf-org-gui*")))
    (unwind-protect
        (progn
          (etaf-mount buffer (etaf-org-preview-view session))
          (set-window-buffer (selected-window) buffer)
          (with-current-buffer buffer
            (redisplay t)
            (let* ((window (get-buffer-window buffer t))
                   (title-pos (save-excursion
                                (goto-char (point-min))
                                (search-forward "Demo" nil t)
                                (and (match-beginning 0) (match-beginning 0))))
                   (face (and title-pos (get-text-property title-pos 'face))))
              (list :text (buffer-substring-no-properties (point-min) (point-max))
                    :has-window (and window t)
                    :pixel-size (and window (window-text-pixel-size
                                             window (point-min) (point-max)))
                    :body-width (and window (window-body-width window))
                    :body-height (and window (window-body-height window))
                    :title-face face
                    :title-foreground (etaf-org-gui-face-key face :foreground)
                    :title-height (etaf-org-gui-face-key face :height)
                    :title-weight (etaf-org-gui-face-key face :weight)
                    :default-family (face-attribute 'default :family nil t)
                    :default-height (face-attribute 'default :height nil t)
                    :default-foreground (face-attribute 'default :foreground nil t)
                    :default-background (face-attribute 'default :background nil t)
                    :frame-width (frame-parameter nil 'width)
                    :frame-height (frame-parameter nil 'height)
                    :frame-background (frame-parameter nil 'background-color)
                    :frame-foreground (frame-parameter nil 'foreground-color)))))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))
"####;

#[test]
fn etaf_org_preview_real_gui_page_matches_gnu() {
    let oracle = CachedMelpaOracle::new(("etaf-org-preview", "0.3.1"), "etaf-org-preview.el")
        .expect("prepare pinned etaf-org-preview source below ./tmp")
        .with_timeout(Duration::from_secs(180));
    let pair = PackageGuiPair::run(
        "etaf-org-preview-real-gui-page",
        oracle.prepared_packages(),
        GUI_PROBE,
    )
    .expect("run real graphical GNU Emacs and Neomacs sequentially");

    let expected_gnu = expect![[
        r##"OK (:text "   \n  Demo   \n     \n  Ada   \n     \n     \n    目录   \n    First heading   \n     Second heading   \n     \n     \n   TODO  First heading    :work:   \n     \n     \n  Body text for parity.   \n     \n   DONE  Second heading    :home:   \n     \n     \n  完 done item   \n  待 open item   \n   " :has-window t :pixel-size (800 . 417) :body-width 100 :body-height 33 :title-face ((:height 217 :weight bold) (:foreground "#1b5a7a") (:background "#ffffff")) :title-foreground "#1b5a7a" :title-height 217 :title-weight bold :default-family "DejaVu Sans Mono" :default-height 101 :default-foreground "black" :default-background "white" :frame-width 100 :frame-height 35 :frame-background "white" :frame-foreground "black")"##
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
