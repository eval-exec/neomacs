//! A GUI frame with its menu and tool bars ON has GNU's Lisp geometry.
//!
//! GNU's Gtk+ build keeps the menu bar and tool bar outside the native frame
//! (`Fframe_native_height`'s doc, src/frame.c:4401-4418: the native height
//! "does not include the tool bar or menu bar"; `Fframe_geometry`'s
//! `outer-size` does include them).  Consequently `window-pixel-top` starts at
//! the top of the window area whatever those bars' pixel height is, and
//! `frame-inner-height` stays at the text rows' pixel height.
//!
//! Neomacs draws the same bars inside its own window, so this row is the
//! contract that the Lisp-visible frame geometry is still GNU's: the bars are
//! backend chrome for drawing and frame-outer-* measures, never part of the
//! native frame or of window coordinates.  The bar heights themselves are
//! toolkit chrome and are deliberately not compared; only their effect on the
//! Lisp geometry is.
//!
//! The `(cells 1 1)` anchor is checked here with the bars explicitly ON so a
//! child-frame user sees GNU's 17 px one-cell offset.

use std::time::Duration;

use expect_test::expect;

use super::support::PackageGuiPair;
use crate::CachedMelpaOracle;

const GUI_PROBE: &str = r####"
(progn
  (require 'cl-lib)
  (set-face-attribute 'default nil :family "DejaVu Sans Mono" :height 100)
  (setq line-spacing nil)
  ;; Make the chrome state explicit rather than inherited from the toolkit's
  ;; defaults, so this row always measures a frame with both bars on.
  (menu-bar-mode 1)
  (tool-bar-mode 1)
  (redisplay t)
  (let* ((window (selected-window))
         (chrome (list :menu-bar-lines (frame-parameter nil 'menu-bar-lines)
                       :tool-bar-lines (frame-parameter nil 'tool-bar-lines)
                       :window-pixel-top (window-pixel-top window)
                       :window-pixel-left (window-pixel-left window)
                       :window-inside-pixel-edges (window-inside-pixel-edges window)
                       :frame-inner-height (frame-inner-height)
                       :frame-inner-width (frame-inner-width)
                       :frame-height (frame-height)
                       :frame-char-height (frame-char-height)
                       :body-height (window-body-height window)
                       :body-width (window-body-width window))))
    (require 'etaf-posframe)
    (let* ((panel (etaf-posframe-show
                   (etaf-view (text "anchor"))
                   :key 'chrome-anchor
                   :at '(cells 1 1)
                   :width 20
                   :height 4
                   :theme 'light))
           (frame (etaf-posframe-panel-frame panel)))
      (redisplay t)
      ;; The panel's viewport settles through ebox's timer after mounting.
      (sleep-for 0.25)
      (let ((anchor (list :top (frame-parameter frame 'top)
                          :left (frame-parameter frame 'left)
                          :child-window-pixel-top
                          (window-pixel-top (frame-root-window frame))
                          :child-window-pixel-left
                          (window-pixel-left (frame-root-window frame))
                          :child-frame-inner-height (frame-inner-height frame))))
        (etaf-posframe-hide panel)
        (list :chrome chrome :anchor anchor)))))
"####;

#[test]
fn gui_frame_with_bars_on_has_gnu_window_geometry() {
    let oracle = CachedMelpaOracle::new(("etaf-posframe", "0.3.0"), "etaf-posframe.el")
        .expect("prepare pinned etaf-posframe source below ./tmp")
        .with_timeout(Duration::from_secs(180));
    let pair = PackageGuiPair::run(
        "gui-frame-chrome-geometry",
        oracle.prepared_packages(),
        GUI_PROBE,
    )
    .expect("run real graphical GNU Emacs and Neomacs sequentially");

    // GNU 31.1, GTK3 oracle, one pinned FONTCONFIG_FILE for both engines,
    // `--quick --geometry 100x35 --load ...`.
    let expected_gnu = expect![[
        r#"OK (:chrome (:menu-bar-lines 1 :tool-bar-lines 1 :window-pixel-top 0 :window-pixel-left 0 :window-inside-pixel-edges (8 0 808 561) :frame-inner-height 595 :frame-inner-width 832 :frame-height 35 :frame-char-height 17 :body-height 33 :body-width 100) :anchor (:top 17 :left 16 :child-window-pixel-top 0 :child-window-pixel-left 0 :child-frame-inner-height 22))"#
    ]];
    expected_gnu.assert_eq(&pair.gnu_behavior.to_string());
    assert_eq!(
        pair.neomacs_behavior.to_string(),
        pair.gnu_behavior.to_string(),
        "Neomacs must put the menu/tool bars outside the Lisp-visible native \
         frame and window area, as the GTK3 reference does"
    );
}
