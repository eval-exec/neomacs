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

(defun neomacs-desktop-window (desktop id)
  "Return ID's detached geometry facts in DESKTOP."
  (let ((state (etaf-desktop-window-state desktop id)))
    (list (plist-get state :x) (plist-get state :y)
          (plist-get state :width) (plist-get state :height))))

(defun neomacs-desktop-facts (desktop)
  "Return DESKTOP's focus, workspace and panel booleans."
  (list :active (etaf-desktop-active-window desktop)
        :workspace (etaf-desktop-active-workspace desktop)
        :mode (etaf-desktop-layout-mode desktop)
        :launcher (etaf-desktop-launcher-open-p desktop)
        :settings-open (etaf-desktop-settings-open-p desktop)
        :switcher (etaf-desktop-switcher-open-p desktop)))

(defun neomacs-desktop-setting (desktop key)
  "Return DESKTOP's public setting KEY."
  (plist-get (etaf-desktop-settings desktop) key))
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
        // A new desktop is an empty floating workspace with the documented
        // defaults, and invalid constructor arguments are rejected.
        ParityBatchCase::value(
            "create_defaults_and_validation",
            r####"
(let ((desktop (etaf-desktop-create)))
  (list (neomacs-desktop-facts desktop)
        (etaf-desktop-windows desktop)
        (neomacs-desktop-setting desktop :theme)
        (neomacs-desktop-setting desktop :layout-mode)
        (neomacs-desktop-setting desktop :tile-outer-gap)
        (neomacs-desktop-setting desktop :drag-lock-modifier)
        (neomacs-desktop-setting desktop :resize-enabled)
        (condition-case err
            (etaf-desktop-create :name "")
          (etaf-desktop-error (list (car err) (cadr err))))
        (condition-case err
            (etaf-desktop-create :fit 'window)
          (etaf-desktop-error (list (car err) (cadr err))))
        (condition-case err
            (etaf-desktop-create :canvas-width 0)
          (etaf-desktop-error (list (car err) (cadr err))))
        (condition-case err
            (etaf-desktop-create :canvas-height 7)
          (etaf-desktop-error (list (car err) (cadr err))))
        (condition-case err
            (etaf-desktop-create :workspaces nil)
          (etaf-desktop-error (list (car err) (cadr err))))
        (condition-case err
            (etaf-desktop-create :workspaces '(main main))
          (etaf-desktop-error (list (car err) (cadr err))))))
"####,
            expect![[
                r#"OK ((:active nil :workspace main :mode floating :launcher nil :settings-open nil :switcher nil) nil system floating 1 shift t (etaf-desktop-error "Desktop name must be a nonempty string") (etaf-desktop-error "Desktop fit must be nil or `region', got window") (etaf-desktop-error "canvas-width must be an integer >= 1, got 0") (etaf-desktop-error "canvas-height must be an integer >= 8, got 7") (etaf-desktop-error "Workspaces must be a nonempty list of unique keys") (etaf-desktop-error "Workspaces must be a nonempty list of unique keys"))"#
            ]],
        ),
        // Settings validate their keys and values, panels toggle independently,
        // and reset restores the defaults.
        ParityBatchCase::value(
            "settings_and_panels",
            r####"
(let ((desktop (etaf-desktop-create)))
  (etaf-desktop-set-setting desktop :theme 'dark)
  (etaf-desktop-set-setting desktop :tile-outer-gap 3)
  (let ((changed (list (neomacs-desktop-setting desktop :theme)
                       (neomacs-desktop-setting desktop :tile-outer-gap))))
    (let ((toggled (list (etaf-desktop-toggle-settings desktop)
                         (etaf-desktop-toggle-launcher desktop)
                         (etaf-desktop-toggle-switcher desktop)
                         (etaf-desktop-settings-open-p desktop)
                         (etaf-desktop-launcher-open-p desktop)
                         (etaf-desktop-switcher-open-p desktop))))
      (list changed toggled
            (condition-case err
                (etaf-desktop-set-setting desktop :nope 1)
              (etaf-desktop-error (list (car err) (cadr err))))
            (condition-case err
                (etaf-desktop-set-setting desktop :theme 'bogus)
              (etaf-desktop-error (list (car err) (cadr err))))
            (etaf-desktop-toggle-settings desktop nil)
            (etaf-desktop-settings-open-p desktop)
            (progn (etaf-desktop-reset-settings desktop) t)
            (neomacs-desktop-setting desktop :theme)
            (neomacs-desktop-setting desktop :tile-outer-gap)
            (etaf-desktop-settings-open-p desktop)))))
"####,
            expect![[
                r#"OK ((dark 3) (t t t t t t) (etaf-desktop-error "Invalid Desktop setting :nope=1") (etaf-desktop-error "Invalid Desktop setting :theme=bogus") nil nil t system 1 nil)"#
            ]],
        ),
        // Removing a workspace moves its windows to the active workspace;
        // unknown workspaces and removing the last one are errors.
        ParityBatchCase::value(
            "workspace_removal_policy",
            r####"
(let ((desktop (etaf-desktop-create :workspaces '(main second))))
  (etaf-desktop-open-window desktop 'one :workspace 'main)
  (etaf-desktop-open-window desktop 'two :workspace 'second)
  (etaf-desktop-switch-workspace desktop 'main)
  (etaf-desktop-add-workspace desktop 'third)
  (let ((before (list (etaf-desktop-active-workspace desktop)
                      (etaf-desktop-active-window desktop))))
    (etaf-desktop-remove-workspace desktop 'second)
    (let ((after (list (etaf-desktop-active-workspace desktop)
                       (plist-get (etaf-desktop-window-state desktop 'two)
                                  :workspace)
                       (etaf-desktop-active-window desktop))))
      (list before after
            (condition-case err
                (etaf-desktop-switch-workspace desktop 'missing)
              (etaf-desktop-error (list (car err) (cadr err))))
            (etaf-desktop-remove-workspace desktop 'main)
            (etaf-desktop-active-workspace desktop)
            (let ((last (etaf-desktop-create :workspaces '(only))))
              (condition-case err
                  (etaf-desktop-remove-workspace last 'only)
                (etaf-desktop-error (list (car err) (cadr err)))))))))
"####,
            expect![[
                r#"OK ((main one) (main main one) (etaf-desktop-error "Unknown desktop workspace: missing") third third (etaf-desktop-error "Cannot remove the last desktop workspace"))"#
            ]],
        ),
        // A tiling split publishes its weighted tree and reflows both leaves.
        ParityBatchCase::value(
            "layout_split_tree",
            r####"
(let ((desktop (etaf-desktop-create :canvas-width 120 :canvas-height 36)))
  (etaf-desktop-open-window
   desktop 'one :title "One" :min-width 20 :min-height 6)
  (etaf-desktop-open-window
   desktop 'two :title "Two" :min-width 20 :min-height 6)
  (list (etaf-desktop-layout-mode desktop)
        (progn (etaf-desktop-set-layout-mode desktop 'tiling)
               (etaf-desktop-layout-mode desktop))
        (progn
          (etaf-desktop-layout-split desktop 'one 'two 'horizontal 0.7)
          (list (etaf-desktop-layout-tree desktop)
                (neomacs-desktop-window desktop 'one)
                (neomacs-desktop-window desktop 'two)))
        (condition-case err
            (etaf-desktop-set-layout-mode desktop 'bogus)
          (etaf-desktop-error (list (car err) (cadr err))))))
"####,
            expect![[
                r#"OK (floating tiling ((:split horizontal 0.7 (:leaf one) (:leaf two)) (1 1 81 34) (85 1 34 34)) (etaf-desktop-error "Unknown desktop layout mode: bogus"))"#
            ]],
        ),
        // A saved session restores the tiling tree and window geometry into a
        // fresh desktop, whether passed as state or written to a file.
        ParityBatchCase::value(
            "session_roundtrip",
            r####"
(let ((source (etaf-desktop-create :canvas-width 120 :canvas-height 36))
      (from-state (etaf-desktop-create :canvas-width 120 :canvas-height 36))
      (from-file (etaf-desktop-create :canvas-width 120 :canvas-height 36)))
  (etaf-desktop-open-window
   source 'one :title "One" :x 3 :y 4 :width 40 :height 12
   :min-width 20 :min-height 6)
  (etaf-desktop-open-window
   source 'two :title "Two" :x 5 :y 6 :width 50 :height 14
   :min-width 20 :min-height 6)
  (etaf-desktop-set-layout-mode source 'tiling)
  (etaf-desktop-layout-split source 'one 'two 'horizontal 0.7)
  (let ((session (etaf-desktop-session-state source))
        (file (make-temp-file "neomacs-desktop-session-" nil ".eld")))
    (unwind-protect
        (progn
          (etaf-desktop-save-session source file)
          (etaf-desktop-restore-session from-state session)
          (etaf-desktop-restore-session from-file file)
          (list (list (etaf-desktop-layout-mode from-state)
                      (etaf-desktop-layout-tree from-state)
                      (neomacs-desktop-window from-state 'one)
                      (neomacs-desktop-window from-state 'two))
                (list (etaf-desktop-layout-mode from-file)
                      (etaf-desktop-layout-tree from-file)
                      (neomacs-desktop-window from-file 'one)
                      (neomacs-desktop-window from-file 'two))))
      (delete-file file))))
"####,
            expect![
                "OK ((tiling (:split horizontal 0.7 (:leaf one) (:leaf two)) (1 1 81 34) (85 1 34 34)) (tiling (:split horizontal 0.7 (:leaf one) (:leaf two)) (1 1 81 34) (85 1 34 34)))"
            ],
        ),
        // An application registers a manifest, launches one instance with its
        // windows, observes suspend/restore/close callbacks, and unregisters.
        ParityBatchCase::value(
            "app_registry_lifecycle",
            r####"
(let* ((app 'neomacs-desktop-probe-app)
       (opened nil)
       (closed nil)
       (desktop (etaf-desktop-create :name "*neomacs-desktop-probe*"))
       (result nil))
  (unwind-protect
      (progn
        (etaf-desktop-register-app
         app :title "Probe" :capabilities '(multiple-instances resizable)
         :open (lambda (context)
                 (setq opened (plist-get context :instance-id))
                 (list :state '(:counter 1)
                       :windows (list (list :id 'main :title "Probe"
                                            :x 3 :y 2 :width 240 :height 8
                                            :min-width 20))))
         :close (lambda (context reason)
                  (setq closed (list (plist-get context :instance-id) reason))))
        (let* ((manifest (etaf-desktop-app app))
               (instance (etaf-desktop-launch desktop app :options '(:mode test)))
               (id (plist-get instance :id)))
          (setq result
                (list (list (plist-get manifest :title)
                            (plist-get manifest :capabilities)
                            (etaf-desktop-app-registered-p app)
                            (memq app (mapcar (lambda (item)
                                                (plist-get item :id))
                                              (etaf-desktop-apps))))
                      (list id (eq opened id)
                            (plist-get instance :status)
                            (mapcar (lambda (window)
                                      (car (last (split-string window "/"))))
                                    (plist-get instance :windows)))
                      (progn
                        (etaf-desktop-set-instance-state desktop id '(:counter 1))
                        (plist-get (etaf-desktop-instance desktop id) :state))
                      (plist-get (etaf-desktop-suspend-instance desktop id)
                                 :status)
                      (plist-get (etaf-desktop-restore-instance desktop id)
                                 :status)
                      (plist-get (etaf-desktop-close-instance desktop id 'quit)
                                 :status)
                      closed
                      (plist-get (etaf-desktop-instance desktop id) :status)))))
    (etaf-desktop-unregister-app app t))
  (append result
          (list (etaf-desktop-app-registered-p app)
                (condition-case err
                    (etaf-desktop-app app)
                  (etaf-desktop-error (list (car err) (cadr err)))))))
"####,
            expect![[
                r#"OK (("Probe" (multiple-instances resizable) t (neomacs-desktop-probe-app)) ("neomacs-desktop-probe-app-1" t active ("main")) (:counter 1) suspended active closed ("neomacs-desktop-probe-app-1" quit) closed nil (etaf-desktop-error "Unknown ETAF application: neomacs-desktop-probe-app"))"#
            ]],
        ),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("etaf-desktop parity batch");
    assert_oracle_batch_cases(desktop_oracle(), test_name, "etaf-desktop parity", &cases);
}
