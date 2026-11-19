//! Real graphical parity for the pinned `etaf-desktop` canvas.
//!
//! The batch module pins the controller's data state and headless canvas text.
//! This probe mounts a controlled two-window desktop in an X11 frame and reads
//! the painted text, pixel size and window-title face.

use std::time::Duration;

use expect_test::expect;

use super::support::PackageGuiPair;
use crate::CachedMelpaOracle;

const GUI_PROBE: &str = r####"
(progn
  (require 'cl-lib)
  (require 'etaf-desktop)
  (set-face-attribute 'default nil :family "DejaVu Sans Mono" :height 100)
  (setq line-spacing nil)

  (let ((desktop (etaf-desktop-create
                  :name " *etaf-desktop-gui*"
                  :canvas-width 80 :canvas-height 20)))
    (unwind-protect
        (progn
          (etaf-desktop-open-window
           desktop 'notes :title "Notes" :x 2 :y 2 :width 30 :height 8
           :min-width 20 :min-height 2
           :view (etaf-view (text "Hello desktop")))
          (etaf-desktop-open-window
           desktop 'todo :title "Todo" :x 40 :y 4 :width 30 :height 8
           :min-width 20 :min-height 2
           :view (etaf-view (text "Task one")))
          (let ((buffer (etaf-desktop-mount desktop)))
            (set-window-buffer (selected-window) buffer)
            (with-current-buffer buffer
              (redisplay t)
              (let* ((window (get-buffer-window buffer t))
                     (title-pos (save-excursion
                                  (goto-char (point-min))
                                  (search-forward "Notes" nil t)
                                  (and (match-beginning 0) (match-beginning 0))))
                     (face (and title-pos (get-text-property title-pos 'face))))
                (list :text (buffer-substring-no-properties (point-min) (point-max))
                      :pixel-size (and window (window-text-pixel-size
                                               window (point-min) (point-max)))
                      :body-width (and window (window-body-width window))
                      :body-height (and window (window-body-height window))
                      :title-face face
                      :default-family (face-attribute 'default :family nil t)
                      :default-height (face-attribute 'default :height nil t)
                      :default-foreground (face-attribute 'default :foreground nil t)
                      :default-background (face-attribute 'default :background nil t)
                      :frame-width (frame-parameter nil 'width)
                      :frame-height (frame-parameter nil 'height))))))
      (etaf-desktop-close desktop))))
"####;

#[test]
fn etaf_desktop_real_gui_canvas_matches_gnu() {
    let oracle = CachedMelpaOracle::new(("etaf-desktop", "0.1.7"), "etaf-desktop.el")
        .expect("prepare pinned etaf-desktop source below ./tmp")
        .with_timeout(Duration::from_secs(180));
    let pair = PackageGuiPair::run(
        "etaf-desktop-real-gui-canvas",
        oracle.prepared_packages(),
        GUI_PROBE,
    )
    .expect("run real graphical GNU Emacs and Neomacs sequentially");

    let expected_gnu = expect![[
        r##"OK (:text "  main     \n  \n  \n            \n   Hel   \n   lo              \n   des    Tas   \n   kto    k     \n   p     one   \n              \n               \n        \n         \n  \n  \n  \n  \n  \n  \n  \n  \n  ⊞ Appli   " :pixel-size (96 . 374) :body-width 100 :body-height 34 :title-face (:background "#D3DBE4") :default-family "DejaVu Sans Mono" :default-height 101 :default-foreground "black" :default-background "white" :frame-width 100 :frame-height 35)"##
    ]];
    expected_gnu.assert_eq(&pair.gnu_behavior.to_string());
    let expected_neomacs = expect![[
        r##"OK (:text "​  main     \n​  \n​  \n​            \n​   Hel   \n​   lo              \n​   des    Tas   \n​   kto    k     \n​   p     one   \n​              \n​               \n​        \n​         \n​  \n​  \n​  \n​  \n​  \n​  \n​  \n​  \n​  ⊞ Appli   " :pixel-size (97 . 396) :body-width 100 :body-height 34 :title-face (:background "#D3DBE4") :default-family "DejaVu Sans Mono" :default-height 100 :default-foreground "black" :default-background "white" :frame-width 100 :frame-height 35)"##
    ]];
    expected_neomacs.assert_eq(&pair.neomacs_behavior.to_string());
    assert_ne!(
        pair.neomacs_behavior, pair.gnu_behavior,
        "the pinned etaf-desktop GUI divergences are FIXED: flip this test to \
         assert_eq and re-record both snapshots; full outcomes:\nNeomacs: {}\nGNU Emacs: {}",
        pair.neomacs, pair.gnu_emacs,
    );
}
