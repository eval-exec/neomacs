use expect_test::expect;

use super::ParityBatchCase;

fn starting_on_title_page_hides_markup_and_runs_navigation_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "starting_on_title_page_hides_markup_and_runs_navigation_hooks",
        r####"
(neomacs-org-present-test-with-buffer
  (org-present)
  (list :state (neomacs-org-present-test-state)
        :events (nreverse neomacs-org-present-test-events)
        :invisibility (member '(org-present) buffer-invisibility-spec)
        :bindings (mapcar (lambda (key) (cons key (lookup-key org-present-mode-keymap (kbd key))))
                          '("<left>" "<right>" "C-c C-q" "C-c <" "C-c >" "C-c C-1"))))
"####,
        expect![[
            r##"OK (:state (:mode t :text "#+title: Release Ω\n#+author: Platform\n#+options: toc:nil\nIntro *overview* and =code=.\n\n" :restriction (1 88) :point (1 1 0) :heading nil :read-only nil :cursor t :scale 0 :one-page nil :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) :events (("org-present-release" "#+title: Release Ω" 1 88)) :invisibility ((org-present) (org-babel-hide-result . t) (org-hide-block . t) (org-fold-outline . "...") (org-hide-block . "...") (org-hide-drawer . "...") (org-link) (outline . t) t) :bindings (("<left>" . org-present-prev) ("<right>" . org-present-next) ("C-c C-q" . org-present-quit) ("C-c <" . org-present-beginning) ("C-c >" . org-present-end) ("C-c C-1" . org-present-toggle-one-big-page)))"##
        ]],
    )
}

fn next_previous_beginning_and_end_navigate_top_level_slides_with_wraparound() -> ParityBatchCase {
    ParityBatchCase::value(
        "next_previous_beginning_and_end_navigate_top_level_slides_with_wraparound",
        r####"
(neomacs-org-present-test-with-buffer
  (org-present)
  (let (states)
    (dolist (operation '(org-present-next org-present-next org-present-next
                         org-present-next org-present-prev org-present-beginning
                         org-present-end))
      (funcall operation)
      (push (cons operation (neomacs-org-present-test-state)) states))
    (list :states (nreverse states)
          :events (nreverse neomacs-org-present-test-events))))
"####,
        expect![[
            r##"OK (:states ((org-present-next :mode t :text "* Plan\n** Scope\nShip *candidate*.\n" :restriction (88 122) :point (88 1 0) :heading "Plan" :read-only nil :cursor t :scale 0 :one-page nil :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) (org-present-next :mode t :text "* Build\nCompile =binary=.\n" :restriction (123 149) :point (123 1 0) :heading "Build" :read-only nil :cursor t :scale 0 :one-page nil :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) (org-present-next :mode t :text "* Release\nPublish /safely/.\n" :restriction (150 178) :point (150 1 0) :heading "Release" :read-only nil :cursor t :scale 0 :one-page nil :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) (org-present-next :mode t :text "* Release\nPublish /safely/.\n" :restriction (150 178) :point (150 1 0) :heading "Release" :read-only nil :cursor t :scale 0 :one-page nil :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) (org-present-prev :mode t :text "* Build\nCompile =binary=.\n" :restriction (123 149) :point (123 1 0) :heading "Build" :read-only nil :cursor t :scale 0 :one-page nil :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) (org-present-beginning :mode t :text "#+title: Release Ω\n#+author: Platform\n#+options: toc:nil\nIntro *overview* and =code=.\n\n" :restriction (1 88) :point (1 1 0) :heading nil :read-only nil :cursor t :scale 0 :one-page nil :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) (org-present-end :mode t :text "* Release\nPublish /safely/.\n" :restriction (150 178) :point (150 1 0) :heading "Release" :read-only nil :cursor t :scale 0 :one-page nil :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present)))) :events (("org-present-release" "#+title: Release Ω" 1 88) ("org-present-release" "Plan" 88 122) ("org-present-release" "Build" 123 149) ("org-present-release" "Release" 150 178) ("org-present-release" "Release" 150 178) ("org-present-release" "Build" 123 149) ("org-present-release" "#+title: Release Ω" 1 88) ("org-present-release" "Release" 150 178)))"##
        ]],
    )
}

fn one_big_page_and_text_scaling_toggle_without_losing_the_current_slide() -> ParityBatchCase {
    ParityBatchCase::value(
        "one_big_page_and_text_scaling_toggle_without_losing_the_current_slide",
        r####"
(neomacs-org-present-test-with-buffer
  (let ((org-present-text-scale 3))
    (org-present)
    (org-present-next)
    (org-present-next)
    (org-present-big)
    (let ((large (neomacs-org-present-test-state)))
      (org-present-toggle-one-big-page)
      (let ((wide (neomacs-org-present-test-state)))
        (org-present-toggle-one-big-page)
        (let ((narrow (neomacs-org-present-test-state)))
          (org-present-small)
          (list :large large :wide wide :narrow narrow
                :small (neomacs-org-present-test-state)))))))
"####,
        expect![[
            r##"OK (:large (:mode t :text "* Build\nCompile =binary=.\n" :restriction (123 149) :point (123 1 0) :heading "Build" :read-only nil :cursor t :scale 3 :one-page nil :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) :wide (:mode t :text "#+title: Release Ω\n#+author: Platform\n#+options: toc:nil\nIntro *overview* and =code=.\n\n* Plan\n** Scope\nShip *candidate*.\n\n* Build\nCompile =binary=.\n\n* Release\nPublish /safely/.\n" :restriction (1 178) :point (123 10 0) :heading "Build" :read-only nil :cursor t :scale 3 :one-page t :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) :narrow (:mode t :text "* Build\nCompile =binary=.\n" :restriction (123 149) :point (123 1 0) :heading "Build" :read-only nil :cursor t :scale 3 :one-page nil :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) :small (:mode t :text "* Build\nCompile =binary=.\n" :restriction (123 149) :point (123 1 0) :heading "Build" :read-only nil :cursor t :scale 0 :one-page nil :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))))"##
        ]],
    )
}

fn read_only_cursor_and_space_policies_restore_editability_on_quit() -> ParityBatchCase {
    ParityBatchCase::value(
        "read_only_cursor_and_space_policies_restore_editability_on_quit",
        r####"
(neomacs-org-present-test-with-buffer
  (let ((cursor-type 'box))
    (org-present)
    (org-present-read-only)
    (let ((locked (neomacs-org-present-test-state)))
      (org-present-show-cursor)
      (let ((shown (neomacs-org-present-test-state)))
        (org-present-read-write)
        (let ((editable (neomacs-org-present-test-state)))
          (org-present-hide-cursor)
          (let ((hidden (neomacs-org-present-test-state)))
            (org-present-quit)
            (list :locked locked :shown shown :editable editable :hidden hidden
                  :quit (neomacs-org-present-test-state))))))))
"####,
        expect![[
            r##"OK (:locked (:mode t :text "#+title: Release Ω\n#+author: Platform\n#+options: toc:nil\nIntro *overview* and =code=.\n\n" :restriction (1 88) :point (1 1 0) :heading nil :read-only t :cursor nil :scale 0 :one-page nil :space org-present-next :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) :shown (:mode t :text "#+title: Release Ω\n#+author: Platform\n#+options: toc:nil\nIntro *overview* and =code=.\n\n" :restriction (1 88) :point (1 1 0) :heading nil :read-only t :cursor box :scale 0 :one-page nil :space org-present-next :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) :editable (:mode t :text "#+title: Release Ω\n#+author: Platform\n#+options: toc:nil\nIntro *overview* and =code=.\n\n" :restriction (1 88) :point (1 1 0) :heading nil :read-only nil :cursor box :scale 0 :one-page nil :space self-insert-command :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) :hidden (:mode t :text "#+title: Release Ω\n#+author: Platform\n#+options: toc:nil\nIntro *overview* and =code=.\n\n" :restriction (1 88) :point (1 1 0) :heading nil :read-only nil :cursor nil :scale 0 :one-page nil :space self-insert-command :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) :quit (:mode nil :text "#+title: Release Ω\n#+author: Platform\n#+options: toc:nil\nIntro *overview* and =code=.\n\n* Plan\n** Scope\nShip *candidate*.\n\n* Build\nCompile =binary=.\n\n* Release\nPublish /safely/.\n" :restriction (1 178) :point (1 1 0) :heading nil :read-only nil :cursor nil :scale 0 :one-page nil :space self-insert-command :overlays nil))"##
        ]],
    )
}

fn configured_start_and_quit_hooks_apply_the_documented_presentation_recipe() -> ParityBatchCase {
    ParityBatchCase::value(
        "configured_start_and_quit_hooks_apply_the_documented_presentation_recipe",
        r####"
(neomacs-org-present-test-with-buffer
  (let ((org-present-mode-hook
         '(org-present-big org-present-hide-cursor org-present-read-only))
        (org-present-mode-quit-hook
         '(org-present-show-cursor org-present-read-write))
        (org-present-text-scale 2))
    (org-present)
    (let ((started (neomacs-org-present-test-state)))
      (org-present-quit)
      (list :started started :quit (neomacs-org-present-test-state)
            :events (nreverse neomacs-org-present-test-events)))))
"####,
        expect![[
            r##"OK (:started (:mode t :text "#+title: Release Ω\n#+author: Platform\n#+options: toc:nil\nIntro *overview* and =code=.\n\n" :restriction (1 88) :point (1 1 0) :heading nil :read-only t :cursor nil :scale 2 :one-page nil :space org-present-next :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present) (:start 64 :end 65 :text "*" :invisible org-present) (:start 73 :end 74 :text "*" :invisible org-present) (:start 79 :end 80 :text "=" :invisible org-present) (:start 84 :end 85 :text "=" :invisible org-present) (:start 88 :end 89 :text "*" :invisible org-present) (:start 95 :end 96 :text "*" :invisible org-present) (:start 95 :end 97 :text "**" :invisible org-present) (:start 119 :end 120 :text "*" :invisible org-present) (:start 123 :end 124 :text "*" :invisible org-present) (:start 139 :end 140 :text "=" :invisible org-present) (:start 146 :end 147 :text "=" :invisible org-present) (:start 150 :end 151 :text "*" :invisible org-present) (:start 168 :end 169 :text "/" :invisible org-present) (:start 175 :end 176 :text "/" :invisible org-present))) :quit (:mode nil :text "#+title: Release Ω\n#+author: Platform\n#+options: toc:nil\nIntro *overview* and =code=.\n\n* Plan\n** Scope\nShip *candidate*.\n\n* Build\nCompile =binary=.\n\n* Release\nPublish /safely/.\n" :restriction (1 178) :point (1 1 0) :heading nil :read-only nil :cursor nil :scale 0 :one-page nil :space self-insert-command :overlays nil) :events (("org-present-release" "#+title: Release Ω" 1 88)))"##
        ]],
    )
}

fn startup_folded_and_overlay_policy_respect_user_customization() -> ParityBatchCase {
    ParityBatchCase::value(
        "startup_folded_and_overlay_policy_respect_user_customization",
        r####"
(neomacs-org-present-test-with-buffer
  (let ((org-present-hide-stars-in-headings nil)
        (org-hide-emphasis-markers t)
        (org-present-startup-folded t))
    (goto-char (point-min))
    (search-forward "* Plan")
    (beginning-of-line)
    (org-present)
    (list :state (neomacs-org-present-test-state)
          :visible (buffer-substring-no-properties (point-min) (point-max))
          :events (nreverse neomacs-org-present-test-events)
          :stars-overlay
          (save-restriction
            (widen)
            (seq-some
             (lambda (overlay)
               (and (eq (overlay-buffer overlay) (current-buffer))
                    (string-match-p
                     "\\`\\*+\\'"
                     (buffer-substring-no-properties
                      (overlay-start overlay) (overlay-end overlay)))))
             org-present-overlays-list)))))
"####,
        expect![[
            r##"OK (:state (:mode t :text "* Plan\n** Scope\nShip *candidate*.\n" :restriction (88 122) :point (88 1 0) :heading "Plan" :read-only nil :cursor t :scale 0 :one-page nil :space nil :overlays ((:start 1 :end 9 :text "#+title:" :invisible org-present) (:start 20 :end 29 :text "#+author:" :invisible org-present) (:start 39 :end 57 :text "#+options: toc:nil" :invisible org-present))) :visible "* Plan\n** Scope\nShip *candidate*.\n" :events (("org-present-release" "Plan" 88 122)) :stars-overlay nil)"##
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        starting_on_title_page_hides_markup_and_runs_navigation_hooks(),
        next_previous_beginning_and_end_navigate_top_level_slides_with_wraparound(),
        one_big_page_and_text_scaling_toggle_without_losing_the_current_slide(),
        read_only_cursor_and_space_policies_restore_editability_on_quit(),
        configured_start_and_quit_hooks_apply_the_documented_presentation_recipe(),
        startup_folded_and_overlay_policy_respect_user_customization(),
    ]
}
