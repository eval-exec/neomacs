//! Real graphical parity for the pinned `etaf-posframe` floating panel.
//!
//! The probe opens one panel at a deterministic anchor and size, reads the
//! child frame's geometry, resolved palette and buffer text, then hides it and
//! checks that the frame and registry entry are gone.  GNU Emacs and Neomacs
//! run sequentially on the same owned X11 display and their observations must
//! be identical; there is no pinned Neomacs outcome.

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
      ;; `frame-live-p' reports the backend's terminal type (GNU Fframep's
      ;; value), which is backend identity (`x' vs `neo'), not a parity fact.
      ;; Record the contract that it agrees with `framep' instead.
      (list :live (eq (frame-live-p frame) (framep frame))
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
    ;; ebox refits mounted viewports through its settle timer after a
    ;; buffer/window change (ebox-viewport-settle-delay).
    (sleep-for 0.25)
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

    // Strict parity: Neomacs must produce GNU's exact observation.  Under the
    // harness's one pinned FONTCONFIG_FILE both engines resolve the same Han
    // fallback, so the row strut and the 22 px line agree.  The `(cells 1 1)`
    // anchor now also agrees: `window-pixel-top' is window-area-relative in
    // GNU, so the child frame's 1-row offset is 17 px in both engines rather
    // than a 68 px offset that included Neomacs' own menu and tool bars.
    // `:live` records that `frame-live-p' agrees with `framep'; the terminal
    // type itself is backend identity (`x' vs `neo').
    let expected_gnu = expect![[
        r##"OK (:before (:live t :has-parent t :visible t :width 20 :height 1 :left 16 :top 17 :window-width 20 :window-height 1 :pixel-width 160 :pixel-height 22 :char-width 8 :char-height 17 :background "#f7f7f7" :foreground "#000000" :text "​panel parity") :panels-before 1 :frame-after-hide nil :panels-after-hide 0 :frame-live-after-hide nil)"##
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
