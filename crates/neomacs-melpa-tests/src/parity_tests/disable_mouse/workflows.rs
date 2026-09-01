use expect_test::expect;

use super::ParityBatchCase;

/// The generated binding universe: counts for the plain and
/// target-including forms, membership of representative events, and the
/// payload.
fn the_generated_binding_universe() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_generated_binding_universe",
        r####"(unwind-protect
    (progn
      (dm93a-test-reset)
      (let ((plain (disable-mouse--all-bindings nil))
            (targeted (disable-mouse--all-bindings t)))
        (list
         :source (dm93a-test-source-state)
         :plain-count (length plain)
         :targeted-count (length targeted)
         :has-plain (and (member (read-kbd-macro "<mouse-1>") plain) t)
         :has-modified (and (member (read-kbd-macro "C-M-<double-down-mouse-3>")
                                    plain)
                            t)
         :has-wheel (and (member (read-kbd-macro "<wheel-up>") plain) t)
         :has-target (and (member (read-kbd-macro "<mode-line> <mouse-1>")
                                  targeted)
                          t)
         :target-only-in-targeted
         (and (not (member (read-kbd-macro "<mode-line> <mouse-1>") plain))
              t))))
  (dm93a-test-reset))"####,
        expect![[
            r#"OK (:source (:upstream-tree "e6555acae15270b6a2e54a32d5c2f217d67e16c3" :feature t :version "20240604.900") :plain-count 576 :targeted-count 2880 :has-plain t :has-modified t :has-wheel t :has-target t :target-only-in-targeted t)"#
        ]],
    )
}

/// The two keymaps are pre-populated at load: the buffer map covers the
/// plain events and the global map also covers the GUI targets, every
/// one bound to the handler.
fn the_keymaps_are_prepopulated_with_the_handler() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_keymaps_are_prepopulated_with_the_handler",
        r####"(unwind-protect
    (progn
      (dm93a-test-reset)
      (list
       :buffer-map
       (list :mouse-1 (lookup-key disable-mouse-mode-map
                                  (read-kbd-macro "<mouse-1>"))
             :wheel (lookup-key disable-mouse-mode-map
                                (read-kbd-macro "S-<triple-drag-mouse-2>"))
             :mode-line (lookup-key disable-mouse-mode-map
                                    (read-kbd-macro "<mode-line> <mouse-1>")))
       :global-map
       (list :mouse-1 (lookup-key disable-mouse-global-mode-map
                                  (read-kbd-macro "<mouse-1>"))
             :mode-line (lookup-key disable-mouse-global-mode-map
                                    (read-kbd-macro "<mode-line> <mouse-1>"))
             :vertical-line (lookup-key disable-mouse-global-mode-map
                                        (read-kbd-macro
                                         "<vertical-line> <C-wheel-down>")))
       :size (list (length disable-mouse-mode-map)
                   (length disable-mouse-global-mode-map))))
  (dm93a-test-reset))"####,
        expect![
            "OK (:buffer-map (:mouse-1 disable-mouse--handle :wheel disable-mouse--handle :mode-line 1) :global-map (:mouse-1 disable-mouse--handle :mode-line disable-mouse--handle :vertical-line disable-mouse--handle) :size (577 581))"
        ],
    )
}

/// The modes: the buffer-local mode clears `mouse-highlight' locally
/// while on and restores it off; the global mode and its alias toggle;
/// the lighters are the documented strings.
fn the_modes_toggle_mouse_highlight_and_carry_the_lighters() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_modes_toggle_mouse_highlight_and_carry_the_lighters",
        r####"(unwind-protect
    (progn
      (dm93a-test-reset)
      (let ((base mouse-highlight))
        (disable-mouse-mode 1)
        (let ((on (list :mode disable-mouse-mode
                        :lighter (cdr (assq 'disable-mouse-mode
                                            minor-mode-alist))
                        :local (local-variable-p 'mouse-highlight)
                        :value mouse-highlight)))
          (disable-mouse-mode -1)
          (let ((off (list :mode disable-mouse-mode
                           :local (local-variable-p 'mouse-highlight)
                           :restored (eq mouse-highlight base))))
            (disable-mouse-global-mode 1)
            (let ((global-on (list :mode disable-mouse-global-mode
                                   :lighter (cdr (assq 'disable-mouse-global-mode
                                                       minor-mode-alist))
                                   :alias (and (fboundp
                                                'global-disable-mouse-mode)
                                               t))))
              (disable-mouse-global-mode -1)
              (list :on on
                    :off off
                    :global-on global-on
                    :global-off disable-mouse-global-mode))))))
  (dm93a-test-reset))"####,
        expect![
            "OK (:on (:mode t :lighter (disable-mouse-mode-lighter) :local t :value nil) :off (:mode nil :local nil :restored t) :global-on (:mode t :lighter (disable-mouse-mode-global-lighter) :alias t) :global-off nil)"
        ],
    )
}

/// The handler delegates to `disable-mouse-command': with the default
/// `ignore' it returns nil silently; with a custom command the custom
/// one runs.
fn the_handler_delegates_to_the_configured_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_handler_delegates_to_the_configured_command",
        r####"(unwind-protect
    (progn
      (dm93a-test-reset)
      (let ((calls nil))
        (unwind-protect
            (progn
              (setq disable-mouse-command
                    (lambda () (interactive) (push :called calls)))
              (let ((default-result
                     (progn
                       (setq disable-mouse-command 'ignore)
                       (call-interactively #'disable-mouse--handle))))
                (setq disable-mouse-command
                      (lambda () (interactive) (push :called calls)))
                (call-interactively #'disable-mouse--handle)
                (list :default-result default-result
                      :custom-called (and (memq :called calls) t))))
          (setq disable-mouse-command 'ignore))))
  (dm93a-test-reset))"####,
        expect!["OK (:default-result nil :custom-called t)"],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_generated_binding_universe(),
        the_keymaps_are_prepopulated_with_the_handler(),
        the_modes_toggle_mouse_highlight_and_carry_the_lighters(),
        the_handler_delegates_to_the_configured_command(),
    ]
}
