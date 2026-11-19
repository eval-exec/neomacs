//! Real graphical parity for the pinned `etaf-posframe` floating panel.
//!
//! The probe opens one panel at a deterministic anchor and size, reads the
//! child frame's geometry, resolved palette and buffer text, then hides it and
//! checks that the frame and registry entry are gone.  GNU Emacs and Neomacs
//! run sequentially on the same owned X11 display.

use std::time::Duration;

use expect_test::expect;

use super::support::PackageGuiPair;
use crate::CachedMelpaOracle;

const GUI_PROBE: &str = r####"
(progn
  (require 'cl-lib)
  (set-face-attribute 'default nil :family "DejaVu Sans Mono" :height 100)
  (setq line-spacing nil)
  (require 'etaf-posframe)

  (defun etaf-pf-parity-facts (frame)
    "Return the observable facts of the panel FRAME."
    (let* ((root (frame-root-window frame))
           (buffer (window-buffer root)))
      (list :live (frame-live-p frame)
            :has-parent (and (frame-parent frame) t)
            :visible (frame-visible-p frame)
            :width (frame-parameter frame 'width)
            :height (frame-parameter frame 'height)
            :left (frame-parameter frame 'left)
            :top (frame-parameter frame 'top)
            :window-width (window-body-width root)
            :window-height (window-body-height root)
            :pixel-width (window-body-width root t)
            :pixel-height (window-body-height root t)
            :char-width (frame-char-width frame)
            :char-height (frame-char-height frame)
            :background (frame-parameter frame 'background-color)
            :foreground (frame-parameter frame 'foreground-color)
            :text (with-current-buffer buffer
                    (buffer-substring-no-properties (point-min) (point-max))))))

  (let* ((panel (etaf-posframe-show
                 (etaf-view (text "panel parity"))
                 :key 'parity
                 :at '(cells 1 1)
                 :width 20
                 :height 4
                 :theme 'light))
         (frame (etaf-posframe-panel-frame panel)))
    (redisplay t)
    (let ((before (etaf-pf-parity-facts frame))
          (panels (length (etaf-posframe-panels))))
      (etaf-posframe-hide panel)
      (redisplay t)
      (list :before before
            :panels-before panels
            :frame-after-hide (etaf-posframe-panel-frame panel)
            :panels-after-hide (length (etaf-posframe-panels))
            :frame-live-after-hide (frame-live-p frame)))))
"####;

#[test]
fn etaf_posframe_real_gui_panel_matches_gnu() {
    let oracle = CachedMelpaOracle::new(("etaf-posframe", "0.3.0"), "etaf-posframe.el")
        .expect("prepare pinned etaf-posframe source below ./tmp")
        .with_timeout(Duration::from_secs(180));
    let pair = PackageGuiPair::run(
        "etaf-posframe-real-gui-panel",
        oracle.prepared_packages(),
        GUI_PROBE,
    )
    .expect("run real graphical GNU Emacs and Neomacs sequentially");

    // Both sides are pinned while these known divergences are open:
    //   - the same `(cells 1 1)` anchor places the child frame at top 17 in
    //     GNU and top 68 here;
    //   - the default line is 17 px in GNU and 18 px here;
    //   - the panel buffer text starts with U+200B here.
    // `frame-live-p` reports each backend's terminal type (`x` vs `neo`),
    // which is GNU's own Fframep-based contract; the panel palettes agree.
    // When a divergence is fixed the Neomacs pin fails first; flip this test
    // to a single assert_eq and delete the note.
    let expected_gnu = expect![[
        r##"OK (:before (:live x :has-parent t :visible t :width 20 :height 1 :left 16 :top 17 :window-width 20 :window-height 1 :pixel-width 160 :pixel-height 17 :char-width 8 :char-height 17 :background "#f7f7f7" :foreground "#000000" :text "panel parity") :panels-before 1 :frame-after-hide nil :panels-after-hide 0 :frame-live-after-hide nil)"##
    ]];
    expected_gnu.assert_eq(&pair.gnu_behavior.to_string());
    let expected_neomacs = expect![[
        r##"OK (:before (:live neo :has-parent t :visible t :width 20 :height 1 :left 16 :top 68 :window-width 20 :window-height 1 :pixel-width 160 :pixel-height 18 :char-width 8 :char-height 17 :background "#f7f7f7" :foreground "#000000" :text "​panel parity") :panels-before 1 :frame-after-hide nil :panels-after-hide 0 :frame-live-after-hide nil)"##
    ]];
    expected_neomacs.assert_eq(&pair.neomacs_behavior.to_string());
    assert_ne!(
        pair.neomacs_behavior, pair.gnu_behavior,
        "the pinned ETAF posframe divergences are FIXED: flip this test to \
         assert_eq and re-record both snapshots; full outcomes:\nNeomacs: {}\nGNU Emacs: {}",
        pair.neomacs, pair.gnu_emacs,
    );
}
