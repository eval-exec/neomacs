//! A GUI frame with its menu and tool bars ON has GNU's Lisp geometry.
//!
//! GNU's Gtk+ build keeps the menu bar and tool bar outside the native frame
//! (`Fframe_native_height`'s doc, src/frame.c:4401-4418: the native height
//! "does not include the tool bar or menu bar"), while `frame-geometry`'s
//! `outer-size` and `frame-outer-height` do include them.  Consequently
//! `window-pixel-top` starts at the top of the window area whatever those
//! bars' pixel height is, and `frame-inner-height` stays at the text rows'
//! pixel height.
//!
//! Neomacs draws the same bars inside its own window, so this row is the
//! contract that the Lisp-visible frame geometry is still GNU's: the bars are
//! backend chrome for drawing and outer measures, never part of the native
//! frame or of window coordinates.  The absolute bar heights are toolkit
//! chrome and differ between the engines, so only GNU's live values are
//! pinned; the engines are compared on the facts and on an engine-relative
//! contract (outer height = inner + tab + menu + tool + 2 * border, bar sizes
//! present with the frame's width, bars external, tool bar at the top).
//!
//! The same probe also checks the two names GNU builds from one
//! `frame_geometry` call (src/xfns.c:7008-7027): `x-frame-edges` must answer
//! what `frame-edges` answers for native, outer and inner edges, and a
//! deleted frame must signal `frame-live-p` (`decode_live_frame`,
//! src/xfns.c:6783).
//!
//! Known toolkit divergence, in this row's pinned numbers: the GTK3 oracle
//! realizes a 66 px external band (menu-bar-size 25 + tool-bar-size 41) and
//! `frame-outer-height` 661 (678 with `tool-bar-style' `both').  A separate
//! `--quick` probe at DejaVu Sans Mono 10 pt measured 69 px (28 + 41) and 664,
//! i.e. the absolute band follows toolkit chrome and font metrics.  Neomacs
//! realizes its own band; both engines count their own, which is why the row
//! pins GNU's absolute numbers only and compares the contract across engines.
//!
//! The `(cells 1 1)` anchor is checked with the bars explicitly ON so a
//! child-frame user sees GNU's 17 px one-cell offset.

use std::time::Duration;

use expect_test::expect;

use super::support::PackageGuiPair;
use crate::CachedMelpaOracle;

const GUI_PROBE: &str = r####"
(progn
  (require 'cl-lib)
  (require 'etaf-posframe)
  (set-face-attribute 'default nil :family "DejaVu Sans Mono" :height 100)
  (setq line-spacing nil)
  ;; Make the chrome state explicit rather than inherited from the toolkit's
  ;; defaults, so this row always measures a frame with both bars on.
  (menu-bar-mode 1)
  (tool-bar-mode 1)
  (redisplay t)

  ;; Engine-relative geometry contract.  GNU's frame_geometry (src/xfns.c:
  ;; 6778-6958) defines outer = inner + tab + menu + tool + 2 * border when
  ;; the bars are external, and reports each bar's realized size.
  (defun chrome-snap ()
    (let* ((g (frame-geometry))
           (outer (cdr (assq 'outer-size g)))
           (menu (cdr (assq 'menu-bar-size g)))
           (tool (cdr (assq 'tool-bar-size g)))
           (tab (cdr (assq 'tab-bar-size g)))
           (inner (frame-inner-height))
           (border (frame-internal-border-width))
           (outer-height (frame-outer-height))
           (outer-width (frame-outer-width)))
      (list :outer-size outer
            :menu-bar-size menu
            :tool-bar-size tool
            :tab-bar-size tab
            :outer-height outer-height
            :outer-width outer-width
            :contract
            (and g
                 (equal outer (cons outer-width outer-height))
                 (= outer-height
                    (+ inner (cdr menu) (cdr tool) (cdr tab) (* 2 border)))
                 (> (cdr menu) 0)
                 (> (cdr tool) 0)
                 (eq (cdr (assq 'menu-bar-external g)) t)
                 (eq (cdr (assq 'tool-bar-external g)) t)
                 (eq (cdr (assq 'tool-bar-position g)) 'top)
                 (= (car menu) outer-width)
                 (= (car tool) outer-width)))))

  (let* ((window (selected-window))
         (facts (list :menu-bar-lines (frame-parameter nil 'menu-bar-lines)
                      :tool-bar-lines (frame-parameter nil 'tool-bar-lines)
                      :window-pixel-top (window-pixel-top window)
                      :window-pixel-left (window-pixel-left window)
                      :window-inside-pixel-edges (window-inside-pixel-edges window)
                      :frame-inner-height (frame-inner-height)
                      :frame-inner-width (frame-inner-width)
                      :frame-height (frame-height)
                      :frame-char-height (frame-char-height)
                      :body-height (window-body-height window)
                      :body-width (window-body-width window)
                      ;; GNU `frame-geometry' dispatches to `x-frame-geometry'
                      ;; on an X frame (lisp/frame.el:2164-2214), so both
                      ;; names must answer one alist on both engines.
                      :x-frame-eq (equal (x-frame-geometry) (frame-geometry))
                      ;; GNU `Fx_frame_edges' is `frame_geometry' with the
                      ;; edges type (src/xfns.c:7008-7027), so the X name and
                      ;; the dispatcher must answer one list, and a dead
                      ;; frame must signal `frame-live-p' (decode_live_frame,
                      ;; src/xfns.c:6783).  PARITY compares these between the
                      ;; engines; the absolute edges stay engine-relative.
                      :x-frame-edges-eq (equal (x-frame-edges) (frame-edges))
                      :x-frame-edges-outer-eq
                      (equal (x-frame-edges nil 'outer-edges)
                             (frame-edges nil 'outer-edges))
                      :x-frame-edges-inner-eq
                      (equal (x-frame-edges nil 'inner-edges)
                             (frame-edges nil 'inner-edges))
                      :dead-frame-edge
                      (let ((selected (selected-frame))
                            (dead (make-frame '((visibility . nil)))))
                        (unwind-protect
                            (progn
                              (delete-frame dead)
                              (condition-case nil
                                  (progn (x-frame-edges dead) 'answered)
                                (wrong-type-argument 'frame-live-p)
                                (error 'other-error)))
                          (when (frame-live-p selected) (select-frame selected))
                          (when (frame-live-p dead) (delete-frame dead))))))
         (image (chrome-snap)))
    ;; A second toolbar style changes the external bar's pixel height in GNU;
    ;; the window origin and the inner height must not move, and the bar sizes
    ;; stay the frame's realized ones.
    (setq-default tool-bar-style 'both)
    (set-frame-parameter nil 'tool-bar-style 'both)
    (set-frame-parameter nil 'tool-bar-lines 1)
    (redisplay t)
    ;; GTK applies an external tool-bar style through the toolkit event loop;
    ;; give it one pass so the new bar height is observable.
    (sit-for 0.2)
    (let* ((both (chrome-snap))
           (panel (etaf-posframe-show
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
        ;; PARITY is compared between the engines; GEOMETRY is GNU's live
        ;; absolute geometry, pinned as GNU-only evidence.
        (list (list :chrome facts
                    :contract-image (plist-get image :contract)
                    :contract-both (plist-get both :contract)
                    :anchor anchor)
              (list :geometry (list image both)))))))
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
    // `--quick --geometry 100x35 --load ...`.  The geometry half carries
    // GNU's live outer measures: 661 px with the image tool bar, 678 px with
    // style `both` (tool-bar-size 41 -> 58), menu-bar-size (832 . 25), and the
    // window area and inner height unchanged at 0 / 595.
    let expected_gnu = expect![[
        r#"OK ((:chrome (:menu-bar-lines 1 :tool-bar-lines 1 :window-pixel-top 0 :window-pixel-left 0 :window-inside-pixel-edges (8 0 808 561) :frame-inner-height 595 :frame-inner-width 832 :frame-height 35 :frame-char-height 17 :body-height 33 :body-width 100 :x-frame-eq t :x-frame-edges-eq t :x-frame-edges-outer-eq t :x-frame-edges-inner-eq t :dead-frame-edge frame-live-p) :contract-image t :contract-both t :anchor (:top 17 :left 16 :child-window-pixel-top 0 :child-window-pixel-left 0 :child-frame-inner-height 22)) (:geometry ((:outer-size (832 . 661) :menu-bar-size (832 . 25) :tool-bar-size (832 . 41) :tab-bar-size (0 . 0) :outer-height 661 :outer-width 832 :contract t) (:outer-size (832 . 678) :menu-bar-size (832 . 25) :tool-bar-size (832 . 58) :tab-bar-size (0 . 0) :outer-height 678 :outer-width 832 :contract t))))"#
    ]];
    expected_gnu.assert_eq(&pair.gnu_behavior.to_string());

    // Compare only the PARITY half: the geometry half's absolute sizes are
    // toolkit chrome and legitimately differ between Gtk+ and Neomacs.
    let parity = |behavior: &str| {
        behavior
            .split(" (:geometry ")
            .next()
            .unwrap_or(behavior)
            .to_owned()
    };
    assert_eq!(
        parity(&pair.neomacs_behavior.to_string()),
        parity(&pair.gnu_behavior.to_string()),
        "Neomacs must put the menu/tool bars outside the Lisp-visible native \
         frame and window area, and report the same engine-relative geometry \
         contract, as the GTK3 reference does"
    );
}
