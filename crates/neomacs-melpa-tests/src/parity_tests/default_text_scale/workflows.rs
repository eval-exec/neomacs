use expect_test::expect;

use super::ParityBatchCase;

/// The surface: the three autoloaded commands, the global minor mode
/// with its keymap, the increment defcustom, and the payload.
fn the_surface_keymap_and_configuration() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_surface_keymap_and_configuration",
        r####"(list
 :source (dtsbfc-test-source-state)
 :commands
 (mapcar (lambda (command) (list :command command :commandp (commandp command)))
         '(default-text-scale-increase
           default-text-scale-decrease
           default-text-scale-reset
           default-text-scale-increment))
 :keymap
 (list :increase (lookup-key default-text-scale-mode-map (kbd "C-M-="))
       :decrease (lookup-key default-text-scale-mode-map (kbd "C-M--"))
       :reset (lookup-key default-text-scale-mode-map (kbd "C-M-0")))
 :amount (eval (car (get 'default-text-scale-amount 'standard-value)))
 :amount-type (get 'default-text-scale-amount 'custom-type))"####,
        expect![[
            r#"OK (:source (:upstream-tree "224204197a626e852e5afb38691fbb222549bc56" :feature t :version "20191226.2234") :commands ((:command default-text-scale-increase :commandp t) (:command default-text-scale-decrease :commandp t) (:command default-text-scale-reset :commandp t) (:command default-text-scale-increment :commandp t)) :keymap (:increase default-text-scale-increase :decrease default-text-scale-decrease :reset default-text-scale-reset) :amount 10 :amount-type integer)"#
        ]],
    )
}

/// The documented batch behavior: adjusting the scale ERRORS from a
/// non-graphical frame, and the autoloaded increase/decrease commands
/// propagate that error.
fn adjusting_errors_from_a_non_graphical_frame() -> ParityBatchCase {
    ParityBatchCase::value(
        "adjusting_errors_from_a_non_graphical_frame",
        r####"(unwind-protect
    (progn
      (dtsbfc-test-reset)
      (list
       :multi-font (display-multi-font-p (selected-frame))
       :increment
       (condition-case err
           (progn (default-text-scale-increment 10) :no-error)
         (error (list (car err) (cadr err))))
       :increase
       (condition-case err
           (progn (default-text-scale-increase) :no-error)
         (error (list (car err) (cadr err))))
       :decrease
       (condition-case err
           (progn (default-text-scale-decrease) :no-error)
         (error (list (car err) (cadr err))))
       :complement-after default-text-scale--complement))
  (dtsbfc-test-reset))"####,
        expect![[
            r#"OK (:multi-font nil :increment (error "Cannot adjust default text scale from a non-graphical frame") :increase (error "Cannot adjust default text scale from a non-graphical frame") :decrease (error "Cannot adjust default text scale from a non-graphical frame") :complement-after 0)"#
        ]],
    )
}

/// The mode lifecycle: enabling adds the new-frame hook and resets the
/// complement; disabling removes the hook first and then signals the
/// same non-graphical error from its internal reset call, leaving the
/// hook removed.
fn the_mode_lifecycle_manages_the_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_mode_lifecycle_manages_the_hook",
        r####"(unwind-protect
    (progn
      (dtsbfc-test-reset)
      (default-text-scale-mode 1)
      (let ((enabled
             (list :mode default-text-scale-mode
                   ;; Membership only: the editors' stock
                   ;; after-make-frame-functions differ (GNU's batch
                   ;; also carries x-dnd-init-frame), which is not this
                   ;; package's doing.
                   :hook (and (memq #'default-text-scale--update-for-new-frame
                                    after-make-frame-functions)
                              t)
                   :complement default-text-scale--complement)))
        (let ((disable-error
               (condition-case err
                   (progn (default-text-scale-mode -1) :no-error)
                 (error (list (car err) (cadr err))))))
          (list :enabled enabled
                :disable-error disable-error
                :hook-after (and (memq #'default-text-scale--update-for-new-frame
                                       after-make-frame-functions)
                                 t)))))
  (dtsbfc-test-reset))"####,
        expect![[
            r#"OK (:enabled (:mode t :hook t :complement 0) :disable-error (error "Cannot adjust default text scale from a non-graphical frame") :hook-after nil)"#
        ]],
    )
}

/// The reset-with-prefix path works in batch: it only sets the current
/// size as the new baseline (message + complement zero), and the
/// no-prefix reset signals the graphical-frame error because it tries
/// to apply the complement.
fn the_reset_paths_split_on_the_prefix_argument() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_reset_paths_split_on_the_prefix_argument",
        r####"(unwind-protect
    (progn
      (dtsbfc-test-reset)
      (let ((with-prefix (progn
                           (default-text-scale-reset t)
                           (list :message (current-message)
                                 :complement default-text-scale--complement)))
            (without-prefix
             (condition-case err
                 (progn (default-text-scale-reset) :no-error)
               (error (list (car err) (cadr err))))))
        (list :with-prefix with-prefix
              :without-prefix without-prefix)))
  (dtsbfc-test-reset))"####,
        expect![[
            r#"OK (:with-prefix (:message nil :complement 0) :without-prefix (error "Cannot adjust default text scale from a non-graphical frame"))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_surface_keymap_and_configuration(),
        adjusting_errors_from_a_non_graphical_frame(),
        the_mode_lifecycle_manages_the_hook(),
        the_reset_paths_split_on_the_prefix_argument(),
    ]
}
