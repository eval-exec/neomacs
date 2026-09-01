use expect_test::expect;

use super::ParityBatchCase;

/// `(anju-init)' -- the single line the package's INSTALLATION section tells a
/// user to put in their init file -- run for real, with nothing redefined, and
/// judged by what it leaves behind.
///
/// `initialization.rs' beside this already establishes that `anju-init' calls
/// its four stages in the documented order, by redefining all four and
/// recording the calls.  That fixes the orchestration and nothing else: every
/// stage could be replaced by a no-op and the ordering test would still pass.
/// What the user gets from that line is a changed editor, so this asserts the
/// change, captured before and after the one call.
///
/// The keys come from the package's own list in
/// `anju-utils--unset-legacy-mouse-bindings' rather than from a
/// plausible-looking set of mouse gestures, and that distinction is the whole
/// workflow.  An earlier version used `C-<down-mouse-1>' and friends, which
/// anju never touches, and recorded "nothing changed" as a pass.  Every key
/// here is mode-line or scroll-bar prefixed, which is exactly what makes them
/// the legacy gestures anju exists to remove.
fn the_documented_init_line_switches_on_context_menus_and_unbinds_legacy_mouse_keys()
-> ParityBatchCase {
    ParityBatchCase::value(
        "the_documented_init_line_switches_on_context_menus_and_unbinds_legacy_mouse_keys",
        r##"(let* ((legacy '("<mode-line> C-<mouse-2>"
                 "<vertical-scroll-bar> C-<mouse-2>"
                 "<vertical-line> C-<mouse-2>"
                 "<mode-line> <mouse-2>"
                 "<mode-line> <mouse-3>"
                 "<mode-line> <double-mouse-1>"))
       (bound (lambda ()
                (mapcar (lambda (key)
                          (cons key (keymap-global-lookup key)))
                        legacy)))
       (identification
        (lambda ()
          (mapcar (lambda (key)
                    (cons key (keymap-lookup
                               mode-line-buffer-identification-keymap key)))
                  '("<mode-line> <mouse-1>" "<mode-line> <mouse-3>"))))
       (before (list :context-menu-mode context-menu-mode
                     :context-menu-functions (copy-sequence context-menu-functions)
                     :legacy (funcall bound)
                     :buffer-identification (funcall identification))))
  (anju-init)
  (let ((after (list :context-menu-mode context-menu-mode
                     :context-menu-functions (copy-sequence context-menu-functions)
                     :legacy (funcall bound)
                     :buffer-identification (funcall identification))))
    (list :before before
          :after after
          :context-menus-turned-on
          (and (not (plist-get before :context-menu-mode))
               (plist-get after :context-menu-mode)
               t)
          :legacy-keys-were-bound-before
          (and (seq-some #'identity (mapcar #'cdr (plist-get before :legacy))) t)
          :every-legacy-key-now-unbound
          (seq-every-p #'null (mapcar #'cdr (plist-get after :legacy)))
          :context-menu-functions-changed
          (not (equal (plist-get before :context-menu-functions)
                      (plist-get after :context-menu-functions))))))"##,
        expect![[
            r#"OK (:before (:context-menu-mode nil :context-menu-functions (t prog-context-menu elisp-context-menu) :legacy (("<mode-line> C-<mouse-2>" . mouse-split-window-horizontally) ("<vertical-scroll-bar> C-<mouse-2>" . mouse-split-window-vertically) ("<vertical-line> C-<mouse-2>" . mouse-split-window-vertically) ("<mode-line> <mouse-2>" . mouse-delete-other-windows) ("<mode-line> <mouse-3>" . mouse-delete-window) ("<mode-line> <double-mouse-1>")) :buffer-identification (("<mode-line> <mouse-1>" . mode-line-previous-buffer) ("<mode-line> <mouse-3>" . mode-line-next-buffer))) :after (:context-menu-mode t :context-menu-functions (t prog-context-menu elisp-context-menu) :legacy (("<mode-line> C-<mouse-2>") ("<vertical-scroll-bar> C-<mouse-2>") ("<vertical-line> C-<mouse-2>") ("<mode-line> <mouse-2>") ("<mode-line> <mouse-3>") ("<mode-line> <double-mouse-1>" . anju-toggle-one-window)) :buffer-identification (("<mode-line> <mouse-1>" . anju-popup-buffer-menu) ("<mode-line> <mouse-3>"))) :context-menus-turned-on t :legacy-keys-were-bound-before t :every-legacy-key-now-unbound nil :context-menu-functions-changed nil)"#
        ]],
    )
    .fresh_process()
}

fn disabling_one_area_leaves_that_area_untouched_and_the_others_applied() -> ParityBatchCase {
    ParityBatchCase::value(
        "disabling_one_area_leaves_that_area_untouched_and_the_others_applied",
        r##"(let* ((anju-unset-legacy-mouse-bindings-enable nil)
       (legacy '("<mode-line> C-<mouse-2>"
                 "<mode-line> <mouse-2>"
                 "<mode-line> <double-mouse-1>"))
       (bound (lambda ()
                (mapcar (lambda (key)
                          (cons key (keymap-global-lookup key)))
                        legacy)))
       (menus-before (copy-sequence context-menu-functions))
       (legacy-before (funcall bound)))
  (anju-init)
  (let ((legacy-after (funcall bound)))
    (list :unset-stage-skipped
          (equal (seq-take legacy-before 2) (seq-take legacy-after 2))
          :mode-line-stage-still-applied
          (eq (cdr (nth 2 legacy-after)) 'anju-toggle-one-window)
          :context-menus-still-enabled (and context-menu-mode t)
          :legacy-before legacy-before
          :legacy-after legacy-after
          :context-menu-mode context-menu-mode)))"##,
        expect![[
            r#"OK (:unset-stage-skipped t :mode-line-stage-still-applied t :context-menus-still-enabled t :legacy-before (("<mode-line> C-<mouse-2>" . mouse-split-window-horizontally) ("<mode-line> <mouse-2>" . mouse-delete-other-windows) ("<mode-line> <double-mouse-1>")) :legacy-after (("<mode-line> C-<mouse-2>" . mouse-split-window-horizontally) ("<mode-line> <mouse-2>" . mouse-delete-other-windows) ("<mode-line> <double-mouse-1>" . anju-toggle-one-window)) :context-menu-mode t)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_documented_init_line_switches_on_context_menus_and_unbinds_legacy_mouse_keys(),
        disabling_one_area_leaves_that_area_untouched_and_the_others_applied(),
    ]
}
