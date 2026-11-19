//! Behavioural parity for the pinned `etaf-desktop` controller.
//!
//! The controller is an unmounted state machine: windows, workspaces, focus,
//! z-order, geometry policy and the minimize/maximize lifecycle are all plain
//! data.  Probes read the public detached state plists and the mounted canvas
//! text; the demo applications (clocks, monitors) stay out.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
"####;

fn desktop_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-desktop", "0.1.7"), "etaf-desktop.el")
        .expect("prepare pinned etaf-desktop source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_desktop_batch() {
    let cases = vec![
        // Window lifecycle, focus order, z-order and detached snapshots.
        ParityBatchCase::value(
            "window_lifecycle_and_state",
            r####"
(let ((desktop (etaf-desktop-create)))
  (etaf-desktop-open-window desktop 'notes :title "Notes" :x 2 :y 3)
  (etaf-desktop-open-window desktop 'clock :title "Clock")
  (let ((first-active (etaf-desktop-active-window desktop)))
    (etaf-desktop-focus-window desktop 'notes)
    (let ((notes (etaf-desktop-window-state desktop 'notes))
          (clock (etaf-desktop-window-state desktop 'clock))
          (snapshot (etaf-desktop-window-state desktop 'notes)))
      (setf (plist-get snapshot :x) 99)
      (list first-active
            (etaf-desktop-active-window desktop)
            (> (plist-get notes :z-index) (plist-get clock :z-index))
            (plist-get (etaf-desktop-window-state desktop 'notes) :x)
            (etaf-desktop-close-window desktop 'clock)
            (etaf-desktop-close-window desktop 'clock)
            (condition-case err
                (etaf-desktop-window-state desktop 'clock)
              (error (list (car err) (cdr err))))))))
"####,
            expect![[
                r#"OK (clock notes t 2 t nil (etaf-desktop-error ("Unknown desktop window: clock")))"#
            ]],
        ),
        // Geometry clamp, maximize/restore rectangle, minimize and restore.
        ParityBatchCase::value(
            "geometry_lifecycle",
            r####"
(let ((desktop (etaf-desktop-create :canvas-width 120 :canvas-height 36)))
  (etaf-desktop-open-window desktop 'notes :x 2 :y 3 :width 40 :height 12
                            :min-width 20 :min-height 6
                            :max-width 70 :max-height 20)
  (etaf-desktop-resize-window desktop 'notes 5 80)
  (let ((clamped (etaf-desktop-window-state desktop 'notes)))
    (etaf-desktop-move-window desktop 'notes 6 8)
    (etaf-desktop-maximize-window desktop 'notes)
    (etaf-desktop-maximize-window desktop 'notes)
    (let ((maximized (etaf-desktop-window-state desktop 'notes))
          (maximized-p (etaf-desktop-window-maximized-p desktop 'notes)))
      (etaf-desktop-minimize-window desktop 'notes)
      (let ((minimized (etaf-desktop-window-state desktop 'notes)))
        (etaf-desktop-restore-window desktop 'notes)
        (let ((restored (etaf-desktop-window-state desktop 'notes)))
          (list (list (plist-get clamped :width) (plist-get clamped :height))
                (list (plist-get maximized :width) (plist-get maximized :height)
                      (plist-get maximized :normal-geometry) maximized-p)
                (plist-get minimized :minimized)
                (list (plist-get restored :minimized)
                      (plist-get restored :maximized)
                      (plist-get restored :visible)
                      (plist-get restored :x)
                      (plist-get restored :y)
                      (plist-get restored :width)
                      (plist-get restored :height))))))))
"####,
            expect!["OK ((20 20) (120 36 (6 8 20 20) t) t (nil nil t 6 8 20 20))"],
        ),
        // Floating windows may cross the canvas edges but stay reachable; a
        // fractional move is the documented error.
        ParityBatchCase::value(
            "overflow_and_errors",
            r####"
(let ((desktop (etaf-desktop-create :canvas-width 300 :canvas-height 20)))
  (etaf-desktop-open-window desktop 'notes :x 10 :y 2 :width 200 :height 6
                            :min-width 40 :min-height 2)
  (let* ((left (etaf-desktop-move-window desktop 'notes -120 3))
         (far (etaf-desktop-move-window desktop 'notes -190 60))
         (error (condition-case err
                    (etaf-desktop-move-window desktop 'notes 1.5 0)
                  (error (list (car err) (cdr err))))))
    (list (list (plist-get left :x) (plist-get left :y))
          (list (plist-get far :x) (plist-get far :y))
          error)))
"####,
            expect![[
                r#"OK ((-120 3) (-152 19) (etaf-desktop-error ("x must be an integer, got 1.5")))"#
            ]],
        ),
        // Workspace switching, focus fallback after minimize, and workspace
        // removal.
        ParityBatchCase::value(
            "workspaces_and_focus",
            r####"
(let ((desktop (etaf-desktop-create :workspaces '(main second)
                                    :canvas-width 100 :canvas-height 20)))
  (etaf-desktop-open-window desktop 'one :workspace 'main)
  (etaf-desktop-open-window desktop 'two :workspace 'second)
  (etaf-desktop-switch-workspace desktop 'main)
  (etaf-desktop-minimize-window desktop 'one)
  (let ((empty (etaf-desktop-active-window desktop)))
    (etaf-desktop-restore-window desktop 'one)
    (etaf-desktop-open-window desktop 'three :workspace 'main)
    (etaf-desktop-minimize-window desktop 'three)
    (let ((fallback (etaf-desktop-active-window desktop)))
      (etaf-desktop-restore-window desktop 'three)
      (etaf-desktop-switch-workspace desktop 'second)
      (etaf-desktop-remove-workspace desktop 'second)
      (list empty fallback
            (etaf-desktop-active-workspace desktop)
            (etaf-desktop-active-window desktop)))))
"####,
            expect!["OK (nil one main three)"],
        ),
        // Tiling minimizes without a blank tile and restores the leaf.
        ParityBatchCase::value(
            "tiling_layout",
            r####"
(let ((desktop (etaf-desktop-create :canvas-width 100 :canvas-height 20)))
  (etaf-desktop-open-window desktop 'one :min-width 10 :min-height 2)
  (etaf-desktop-open-window desktop 'two :min-width 10 :min-height 2)
  (etaf-desktop-set-layout-mode desktop 'tiling)
  (etaf-desktop-minimize-window desktop 'two)
  (let ((one (etaf-desktop-window-state desktop 'one))
        (tree (flatten-tree (etaf-desktop-layout-tree desktop))))
    (etaf-desktop-restore-window desktop 'two)
    (list (list (plist-get one :y) (plist-get one :height))
          tree
          (flatten-tree (etaf-desktop-layout-tree desktop)))))
"####,
            expect!["OK ((1 18) (:leaf one) (:split vertical 0.5 :leaf one :leaf two))"],
        ),
        // The mounted canvas text is the user-visible layout.
        ParityBatchCase::value(
            "mounted_layout",
            r####"
(let ((desktop (etaf-desktop-create :canvas-width 80 :canvas-height 20)))
  (unwind-protect
      (progn
        (etaf-desktop-open-window
         desktop 'notes :title "Notes" :x 2 :y 2 :width 30 :height 8 :min-width 20 :min-height 2
         :view (etaf-view (text "Hello desktop")))
        (etaf-desktop-open-window
         desktop 'todo :title "Todo" :x 40 :y 4 :width 30 :height 8 :min-width 20 :min-height 2
         :view (etaf-view (text "Task one")))
        (let ((buffer (etaf-desktop-mount desktop)))
          (with-current-buffer buffer
            (list (buffer-substring-no-properties (point-min) (point-max))
                  (count-lines (point-min) (point-max))))))
    (etaf-desktop-close desktop)))
"####,
            expect![[
                r#"OK ("  main  Todo   Floating    Settings  \n│ \n│ \n│ │        │ \n│ │Hello desktop  │ \n│ │   │ │        │ \n│ │   │ │Task one  │ \n│ │   │ │   │ \n│ │   │ │   │ \n│ │   │ │   │ \n│ │    │ │   │ \n│ │   │ \n│ │    │ \n│ \n│ \n│ \n│ \n│ \n│ \n│ \n│ \n  ⊞ Applications   Todo   Notes   " 22)"#
            ]],
        ),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("etaf-desktop parity batch");
    assert_oracle_batch_cases(desktop_oracle(), test_name, "etaf-desktop parity", &cases);
}
