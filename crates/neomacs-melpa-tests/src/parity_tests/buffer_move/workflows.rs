use expect_test::expect;

use super::{
    BlockReason, BlockedRoute, Direction, ExistingRoute, MoveBehavior, ParityBatchCase,
    SelectionRequest, WindowLayout, WindowSlot, contiguous_transient_keys,
};

fn route(
    layout: WindowLayout,
    from: WindowSlot,
    direction: Direction,
    to: WindowSlot,
    behavior: MoveBehavior,
    selection: SelectionRequest,
) -> ExistingRoute {
    ExistingRoute::new(layout, from, direction, to, behavior, selection)
        .expect("rank366 route must be adjacent in its typed canonical layout")
}

fn blocked(
    layout: WindowLayout,
    from: WindowSlot,
    direction: Direction,
    target: Option<WindowSlot>,
    reason: BlockReason,
) -> BlockedRoute {
    BlockedRoute::new(layout, from, direction, target, reason)
        .expect("rank366 blocked route must match its typed canonical layout")
}

fn readme_swap_story() -> ParityBatchCase {
    let left = route(
        WindowLayout::ReadmeThreePane,
        WindowSlot::TopRight,
        Direction::Left,
        WindowSlot::TopLeft,
        MoveBehavior::Swap,
        SelectionRequest::FollowDestination,
    );
    let down = route(
        WindowLayout::ReadmeThreePane,
        WindowSlot::TopRight,
        Direction::Down,
        WindowSlot::Bottom,
        MoveBehavior::Swap,
        SelectionRequest::FollowDestination,
    );
    let ordinary_swap = route(
        WindowLayout::HorizontalPair,
        WindowSlot::Left,
        Direction::Right,
        WindowSlot::Right,
        MoveBehavior::Swap,
        SelectionRequest::FollowDestination,
    );
    let requested_stay = route(
        WindowLayout::HorizontalPair,
        WindowSlot::Left,
        Direction::Right,
        WindowSlot::Right,
        MoveBehavior::Swap,
        SelectionRequest::RequestDocumentedStay,
    );

    let probe = format!(
        r###"
(buffer366-test-run
 "readme_swap_story_preserves_complete_views_and_exposes_dead_stay_option"
 (lambda ()
   (let ((provenance (buffer366-test-provenance))
         readme-before readme-left readme-navigate readme-down readme-bytes
         stay-nil stay-nil-bytes stay-t stay-t-bytes
         same-before same-after same-bytes)
     (buffer366-test-readme-layout)
     (setq readme-before (buffer366-test-layout-state))
     (setq readme-left
           (list :route (buffer366-test-route-state {left})
                 :after (buffer366-test-invoke-existing {left})))
     ;; This core navigation is part of the README story, not a replacement
     ;; for Buffer Move's own destination selection.
     (call-interactively #'windmove-right)
     (setq readme-navigate (buffer366-test-layout-state))
     (setq readme-down
           (list :route (buffer366-test-route-state {down})
                 :after (buffer366-test-invoke-existing {down})))
     (setq readme-bytes (buffer366-test-owned-buffer-bytes))

     (buffer366-test-horizontal-layout "stay-nil")
     (setq stay-nil
           (list :route (buffer366-test-route-state {ordinary_swap})
                 :before (buffer366-test-layout-state)
                 :after (buffer366-test-invoke-existing {ordinary_swap})
                 :actual-selection
                 (buffer366-test-window-slot (selected-window))))
     (setq stay-nil-bytes (buffer366-test-owned-buffer-bytes))

     (buffer366-test-horizontal-layout "stay-requested")
     (setq stay-t
           (list :route (buffer366-test-route-state {requested_stay})
                 :before (buffer366-test-layout-state)
                 :after (buffer366-test-invoke-existing {requested_stay})
                 :option buffer-move-stay-after-swap
                 :actual-selection
                 (buffer366-test-window-slot (selected-window))))
     (setq stay-t-bytes (buffer366-test-owned-buffer-bytes))

     (buffer366-test-horizontal-layout "same-view" t)
     (setq same-before (buffer366-test-layout-state))
     (setq same-after
           (list :route (buffer366-test-route-state {requested_stay})
                 :state (buffer366-test-invoke-existing {requested_stay})))
     (setq same-bytes (buffer366-test-owned-buffer-bytes))
     (list :provenance provenance
           :readme (list :before readme-before :after-left readme-left
                         :after-core-navigation readme-navigate
                         :after-down readme-down :buffers readme-bytes)
           :documented-stay
           (list :nil stay-nil :nil-buffers stay-nil-bytes
                 :requested stay-t :requested-buffers stay-t-bytes)
           :same-buffer (list :before same-before :after same-after
                              :buffers same-bytes)))))
"###,
        left = left.elisp(),
        down = down.elisp(),
        ordinary_swap = ordinary_swap.elisp(),
        requested_stay = requested_stay.elisp(),
    );

    ParityBatchCase::value(
        "readme_swap_story_preserves_complete_views_and_exposes_dead_stay_option",
        probe,
        expect![[
            r#"OK (:result (:provenance (:melpa-version "20220512.755" :source-version "0.6.3" :commit "e7800b3ab1bd76ee475ef35507ec51ecd5a3f065" :source-sha256 "f53f8ede64251f2984cfc43e25a5f26927ce53a46be3982602093835ca2477f1" :commands (buf-move-up buf-move-down buf-move-left buf-move-right buf-move) :defaults (swap nil) :dependency-closure nil) :readme (:before (:layout readme-three-pane :selected top-right :current B :windows ((:slot top-left :edges (0 1 40 13) :body (39 11) :buffer A :selected nil :shows-current-buffer nil :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil) (:slot top-right :edges (40 1 80 13) :body (40 11) :buffer B :selected t :shows-current-buffer t :start (:position 189 :line 5 :column 0 :text "B-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 295 :line 7 :column 12 :text "B-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 5 :dedicated nil :minibuffer nil) (:slot bottom :edges (0 13 80 24) :body (80 10) :buffer C :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "C-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 439 :line 10 :column 15 :text "C-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 8 :dedicated nil :minibuffer nil))) :after-left (:route (:layout readme-three-pane :from top-right :direction left :to top-left :command buf-move-left :behavior swap :selection-request follow-destination :block-reason nil) :after (:layout readme-three-pane :selected top-left :current B :windows ((:slot top-left :edges (0 1 40 13) :body (39 11) :buffer B :selected t :shows-current-buffer t :start (:position 189 :line 5 :column 0 :text "B-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 295 :line 7 :column 12 :text "B-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 5 :dedicated nil :minibuffer nil) (:slot top-right :edges (40 1 80 13) :body (40 11) :buffer A :selected nil :shows-current-buffer nil :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil) (:slot bottom :edges (0 13 80 24) :body (80 10) :buffer C :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "C-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 439 :line 10 :column 15 :text "C-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 8 :dedicated nil :minibuffer nil)))) :after-core-navigation (:layout readme-three-pane :selected top-right :current A :windows ((:slot top-left :edges (0 1 40 13) :body (39 11) :buffer B :selected nil :shows-current-buffer nil :start (:position 189 :line 5 :column 0 :text "B-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 295 :line 7 :column 12 :text "B-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 5 :dedicated nil :minibuffer nil) (:slot top-right :edges (40 1 80 13) :body (40 11) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil) (:slot bottom :edges (0 13 80 24) :body (80 10) :buffer C :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "C-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 439 :line 10 :column 15 :text "C-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 8 :dedicated nil :minibuffer nil))) :after-down (:route (:layout readme-three-pane :from top-right :direction down :to bottom :command buf-move-down :behavior swap :selection-request follow-destination :block-reason nil) :after (:layout readme-three-pane :selected bottom :current A :windows ((:slot top-left :edges (0 1 40 13) :body (39 11) :buffer B :selected nil :shows-current-buffer nil :start (:position 189 :line 5 :column 0 :text "B-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 295 :line 7 :column 12 :text "B-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 5 :dedicated nil :minibuffer nil) (:slot top-right :edges (40 1 80 13) :body (40 11) :buffer C :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "C-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 439 :line 10 :column 15 :text "C-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 8 :dedicated nil :minibuffer nil) (:slot bottom :edges (0 13 80 24) :body (80 10) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil)))) :buffers ((:buffer A :text "A-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer B :text "B-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer C :text "C-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n"))) :documented-stay (:nil (:route (:layout horizontal-pair :from left :direction right :to right :command buf-move-right :behavior swap :selection-request follow-destination :block-reason nil) :before (:layout horizontal-pair :selected left :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer B :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "B-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 437 :line 10 :column 13 :text "B-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 6 :dedicated nil :minibuffer nil))) :after (:layout horizontal-pair :selected right :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer B :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "B-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 437 :line 10 :column 13 :text "B-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 6 :dedicated nil :minibuffer nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil))) :actual-selection right) :nil-buffers ((:buffer A :text "A-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer B :text "B-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n")) :requested (:route (:layout horizontal-pair :from left :direction right :to right :command buf-move-right :behavior swap :selection-request request-documented-stay :block-reason nil) :before (:layout horizontal-pair :selected left :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer B :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "B-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 437 :line 10 :column 13 :text "B-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 6 :dedicated nil :minibuffer nil))) :after (:layout horizontal-pair :selected right :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer B :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "B-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 437 :line 10 :column 13 :text "B-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 6 :dedicated nil :minibuffer nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil))) :option t :actual-selection right) :requested-buffers ((:buffer A :text "A-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer B :text "B-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n"))) :same-buffer (:before (:layout horizontal-pair :selected left :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer A :selected nil :shows-current-buffer t :start (:position 471 :line 11 :column 0 :text "A-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 581 :line 13 :column 16 :text "A-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 9 :dedicated nil :minibuffer nil))) :after (:route (:layout horizontal-pair :from left :direction right :to right :command buf-move-right :behavior swap :selection-request request-documented-stay :block-reason nil) :state (:layout horizontal-pair :selected right :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer A :selected nil :shows-current-buffer t :start (:position 471 :line 11 :column 0 :text "A-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 581 :line 13 :column 16 :text "A-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 9 :dedicated nil :minibuffer nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil)))) :buffers ((:buffer A :text "A-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n")))) :cleanup clean)"#
        ]],
    )
}

fn move_uses_real_window_history() -> ParityBatchCase {
    let move_right = route(
        WindowLayout::HorizontalPair,
        WindowSlot::Left,
        Direction::Right,
        WindowSlot::Right,
        MoveBehavior::Move,
        SelectionRequest::FollowDestination,
    );
    let probe = format!(
        r###"
(buffer366-test-run
 "move_behavior_restores_real_previous_buffer_history"
 (lambda ()
   (buffer366-test-horizontal-layout "move")
   (let* ((provenance (buffer366-test-provenance))
          (left (buffer366-test-slot-window 'left))
          (a (car (rassq 'A buffer366-test-buffer-roles)))
          (fallback
           (buffer366-test-new-buffer "bm366-move-fallback" 'fallback "F"))
          before after history-use bytes)
     (select-window left)
     (set-window-prev-buffers left nil)
     (set-window-next-buffers left nil)
     (let ((right (buffer366-test-slot-window 'right)))
       (set-window-prev-buffers right nil)
       (set-window-next-buffers right nil))
     (switch-to-buffer fallback)
     (buffer366-test-set-view left fallback 14 16 11 4)
     ;; Clear fixture setup history, then create the one real fallback entry
     ;; through GNU's public buffer switch.
     (set-window-prev-buffers left nil)
     (set-window-next-buffers left nil)
     (switch-to-buffer a)
     (buffer366-test-set-view left a 3 5 10 3)
     (select-window left)
     (setq before (buffer366-test-layout-state t))
     (setq after
           (list :route (buffer366-test-route-state {move_right})
                 :state (buffer366-test-invoke-existing {move_right})))
     (setq after (append after (list :history (buffer366-test-layout-state t))))
     ;; Prove the origin's real fallback/history remains usable through a
     ;; second public core command rather than inspecting shape alone.
     (call-interactively #'windmove-left)
     (unless (eq (selected-window) left)
       (error "Buffer Move public origin navigation drifted"))
     (call-interactively #'next-buffer)
     (setq history-use (buffer366-test-layout-state t)
           bytes (buffer366-test-owned-buffer-bytes))
     (list :provenance provenance
           :before before :after after
           :public-next-buffer history-use :buffers bytes))))
"###,
        move_right = move_right.elisp(),
    );
    ParityBatchCase::value(
        "move_behavior_restores_real_previous_buffer_history",
        probe,
        expect![[
            r#"OK (:result (:provenance (:melpa-version "20220512.755" :source-version "0.6.3" :commit "e7800b3ab1bd76ee475ef35507ec51ecd5a3f065" :source-sha256 "f53f8ede64251f2984cfc43e25a5f26927ce53a46be3982602093835ca2477f1" :commands (buf-move-up buf-move-down buf-move-left buf-move-right buf-move) :defaults (swap nil) :dependency-closure nil) :before (:layout horizontal-pair :selected left :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer A :selected t :shows-current-buffer t :start (:position 95 :line 3 :column 0 :text "A-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 199 :line 5 :column 10 :text "A-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 3 :dedicated nil :minibuffer nil :prev ((:buffer fallback :start 612 :start-insertion nil :point 717 :point-insertion nil)) :next nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer B :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "B-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 437 :line 10 :column 13 :text "B-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 6 :dedicated nil :minibuffer nil :prev nil :next nil))) :after (:route (:layout horizontal-pair :from left :direction right :to right :command buf-move-right :behavior move :selection-request follow-destination :block-reason nil) :state (:layout horizontal-pair :selected right :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer fallback :selected nil :shows-current-buffer nil :start (:position 612 :line 14 :column 0 :text "F-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 717 :line 16 :column 11 :text "F-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 0 :dedicated nil :minibuffer nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer A :selected t :shows-current-buffer t :start (:position 95 :line 3 :column 0 :text "A-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 199 :line 5 :column 10 :text "A-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 3 :dedicated nil :minibuffer nil))) :history (:layout horizontal-pair :selected right :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer fallback :selected nil :shows-current-buffer nil :start (:position 612 :line 14 :column 0 :text "F-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 717 :line 16 :column 11 :text "F-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 0 :dedicated nil :minibuffer nil :prev ((:buffer A :start 95 :start-insertion nil :point 199 :point-insertion nil)) :next (A)) (:slot right :edges (40 1 80 24) :body (40 22) :buffer A :selected t :shows-current-buffer t :start (:position 95 :line 3 :column 0 :text "A-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 199 :line 5 :column 10 :text "A-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 3 :dedicated nil :minibuffer nil :prev ((:buffer B :start 330 :start-insertion nil :point 437 :point-insertion nil)) :next nil)))) :public-next-buffer (:layout horizontal-pair :selected left :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer A :selected t :shows-current-buffer t :start (:position 95 :line 3 :column 0 :text "A-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 199 :line 5 :column 10 :text "A-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 0 :dedicated nil :minibuffer nil :prev ((:buffer fallback :start 612 :start-insertion nil :point 717 :point-insertion nil)) :next nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer A :selected nil :shows-current-buffer t :start (:position 95 :line 3 :column 0 :text "A-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 199 :line 5 :column 10 :text "A-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 3 :dedicated nil :minibuffer nil :prev ((:buffer B :start 330 :start-insertion nil :point 437 :point-insertion nil)) :next nil))) :buffers ((:buffer A :text "A-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer B :text "B-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer fallback :text "F-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nF-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n"))) :cleanup clean)"#
        ]],
    )
}

fn transient_arrows_repeat_and_fall_through() -> ParityBatchCase {
    let routes = [
        route(
            WindowLayout::FourPaneGrid,
            WindowSlot::TopLeft,
            Direction::Right,
            WindowSlot::TopRight,
            MoveBehavior::Swap,
            SelectionRequest::FollowDestination,
        ),
        route(
            WindowLayout::FourPaneGrid,
            WindowSlot::TopRight,
            Direction::Down,
            WindowSlot::BottomRight,
            MoveBehavior::Swap,
            SelectionRequest::FollowDestination,
        ),
        route(
            WindowLayout::FourPaneGrid,
            WindowSlot::BottomRight,
            Direction::Left,
            WindowSlot::BottomLeft,
            MoveBehavior::Swap,
            SelectionRequest::FollowDestination,
        ),
        route(
            WindowLayout::FourPaneGrid,
            WindowSlot::BottomLeft,
            Direction::Up,
            WindowSlot::TopLeft,
            MoveBehavior::Swap,
            SelectionRequest::FollowDestination,
        ),
    ];
    let route_forms = routes
        .into_iter()
        .map(ExistingRoute::elisp)
        .collect::<Vec<_>>()
        .join(" ");
    let keys = contiguous_transient_keys();
    let probe = format!(
        r###"
(buffer366-test-run
 "transient_arrows_repeat_and_unmatched_a_falls_through"
 (lambda ()
   (buffer366-test-grid-layout)
   (let* ((provenance (buffer366-test-provenance))
          (a (car (rassq 'A buffer366-test-buffer-roles)))
          (routes (list {route_forms}))
          (binding-before (key-binding (kbd "<right>")))
          before after)
     (with-current-buffer a
       (use-local-map (make-sparse-keymap))
       (local-set-key (kbd "C-c m") #'buf-move))
     (setq before
           (list :layout (buffer366-test-layout-state)
                 :user-binding (with-current-buffer a
                                 (local-key-binding (kbd "C-c m")))
                 :right-binding binding-before
                 :routes (mapcar #'buffer366-test-route-state routes)))
     (buffer366-test-enable-transient-observation)
     (buffer366-test-install-command-observer)
     (execute-kbd-macro (kbd {keys:?}))
     (buffer366-test-remove-command-observer)
     (buffer366-test-disable-transient-observation)
     (setq after
           (list :layout (buffer366-test-layout-state)
                 :commands (nreverse buffer366-test-command-events)
                 :transient-calls buffer366-test-transient-calls
                 :map-active
                 (and buffer366-test-transient-map
                      (buffer366-test-tree-contains-eq
                       buffer366-test-transient-map
                       overriding-terminal-local-map))
                 :owned-hook-present
                 (and buffer366-test-transient-hook
                      (memq buffer366-test-transient-hook pre-command-hook))
                 :right-binding (key-binding (kbd "<right>"))
                 :unread unread-command-events
                 :executing executing-kbd-macro
                 :selected-buffer (buffer366-test-buffer-role (current-buffer))
                 :selected-state
                 (with-current-buffer a
                   (list :point (buffer366-test-line-state a (point))
                         :modified (buffer-modified-p)
                         :undo (not (null buffer-undo-list))))
                 :buffers (buffer366-test-owned-buffer-bytes)))
     (list :provenance provenance
           :before before :after after))))
"###,
        route_forms = route_forms,
        keys = keys,
    );
    ParityBatchCase::value(
        "transient_arrows_repeat_and_unmatched_a_falls_through",
        probe,
        expect![[
            r#"OK (:result (:provenance (:melpa-version "20220512.755" :source-version "0.6.3" :commit "e7800b3ab1bd76ee475ef35507ec51ecd5a3f065" :source-sha256 "f53f8ede64251f2984cfc43e25a5f26927ce53a46be3982602093835ca2477f1" :commands (buf-move-up buf-move-down buf-move-left buf-move-right buf-move) :defaults (swap nil) :dependency-closure nil) :before (:layout (:layout four-pane-grid :selected top-left :current A :windows ((:slot top-left :edges (0 1 40 13) :body (39 11) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil) (:slot top-right :edges (40 1 80 13) :body (40 11) :buffer B :selected nil :shows-current-buffer nil :start (:position 189 :line 5 :column 0 :text "B-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 295 :line 7 :column 12 :text "B-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 5 :dedicated nil :minibuffer nil) (:slot bottom-left :edges (0 13 40 24) :body (39 10) :buffer C :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "C-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 439 :line 10 :column 15 :text "C-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 8 :dedicated nil :minibuffer nil) (:slot bottom-right :edges (40 13 80 24) :body (40 10) :buffer D :selected nil :shows-current-buffer nil :start (:position 471 :line 11 :column 0 :text "D-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 583 :line 13 :column 18 :text "D-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 11 :dedicated nil :minibuffer nil))) :user-binding buf-move :right-binding right-char :routes ((:layout four-pane-grid :from top-left :direction right :to top-right :command buf-move-right :behavior swap :selection-request follow-destination :block-reason nil) (:layout four-pane-grid :from top-right :direction down :to bottom-right :command buf-move-down :behavior swap :selection-request follow-destination :block-reason nil) (:layout four-pane-grid :from bottom-right :direction left :to bottom-left :command buf-move-left :behavior swap :selection-request follow-destination :block-reason nil) (:layout four-pane-grid :from bottom-left :direction up :to top-left :command buf-move-up :behavior swap :selection-request follow-destination :block-reason nil))) :after (:layout (:layout four-pane-grid :selected top-left :current A :windows ((:slot top-left :edges (0 1 40 13) :body (39 11) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 152 :line 4 :column 10 :text "A-04 | abacdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil) (:slot top-right :edges (40 1 80 13) :body (40 11) :buffer D :selected nil :shows-current-buffer nil :start (:position 471 :line 11 :column 0 :text "D-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 583 :line 13 :column 18 :text "D-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 11 :dedicated nil :minibuffer nil) (:slot bottom-left :edges (0 13 40 24) :body (39 10) :buffer B :selected nil :shows-current-buffer nil :start (:position 189 :line 5 :column 0 :text "B-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 295 :line 7 :column 12 :text "B-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 5 :dedicated nil :minibuffer nil) (:slot bottom-right :edges (40 13 80 24) :body (40 10) :buffer C :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "C-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 439 :line 10 :column 15 :text "C-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 8 :dedicated nil :minibuffer nil))) :commands ((:command buf-move :selected top-left :buffer A :map-active t :right-binding buf-move-right) (:command buf-move-right :selected top-right :buffer A :map-active t :right-binding buf-move-right :layout (:layout four-pane-grid :selected top-right :current A :windows ((:slot top-left :edges (0 1 40 13) :body (39 11) :buffer B :selected nil :shows-current-buffer nil :start (:position 189 :line 5 :column 0 :text "B-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 295 :line 7 :column 12 :text "B-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 5 :dedicated nil :minibuffer nil) (:slot top-right :edges (40 1 80 13) :body (40 11) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil) (:slot bottom-left :edges (0 13 40 24) :body (39 10) :buffer C :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "C-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 439 :line 10 :column 15 :text "C-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 8 :dedicated nil :minibuffer nil) (:slot bottom-right :edges (40 13 80 24) :body (40 10) :buffer D :selected nil :shows-current-buffer nil :start (:position 471 :line 11 :column 0 :text "D-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 583 :line 13 :column 18 :text "D-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 11 :dedicated nil :minibuffer nil)))) (:command buf-move-down :selected bottom-right :buffer A :map-active t :right-binding buf-move-right :layout (:layout four-pane-grid :selected bottom-right :current A :windows ((:slot top-left :edges (0 1 40 13) :body (39 11) :buffer B :selected nil :shows-current-buffer nil :start (:position 189 :line 5 :column 0 :text "B-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 295 :line 7 :column 12 :text "B-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 5 :dedicated nil :minibuffer nil) (:slot top-right :edges (40 1 80 13) :body (40 11) :buffer D :selected nil :shows-current-buffer nil :start (:position 471 :line 11 :column 0 :text "D-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 583 :line 13 :column 18 :text "D-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 11 :dedicated nil :minibuffer nil) (:slot bottom-left :edges (0 13 40 24) :body (39 10) :buffer C :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "C-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 439 :line 10 :column 15 :text "C-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 8 :dedicated nil :minibuffer nil) (:slot bottom-right :edges (40 13 80 24) :body (40 10) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil)))) (:command buf-move-left :selected bottom-left :buffer A :map-active t :right-binding buf-move-right :layout (:layout four-pane-grid :selected bottom-left :current A :windows ((:slot top-left :edges (0 1 40 13) :body (39 11) :buffer B :selected nil :shows-current-buffer nil :start (:position 189 :line 5 :column 0 :text "B-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 295 :line 7 :column 12 :text "B-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 5 :dedicated nil :minibuffer nil) (:slot top-right :edges (40 1 80 13) :body (40 11) :buffer D :selected nil :shows-current-buffer nil :start (:position 471 :line 11 :column 0 :text "D-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 583 :line 13 :column 18 :text "D-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 11 :dedicated nil :minibuffer nil) (:slot bottom-left :edges (0 13 40 24) :body (39 10) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil) (:slot bottom-right :edges (40 13 80 24) :body (40 10) :buffer C :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "C-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 439 :line 10 :column 15 :text "C-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 8 :dedicated nil :minibuffer nil)))) (:command buf-move-up :selected top-left :buffer A :map-active t :right-binding buf-move-right :layout (:layout four-pane-grid :selected top-left :current A :windows ((:slot top-left :edges (0 1 40 13) :body (39 11) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil) (:slot top-right :edges (40 1 80 13) :body (40 11) :buffer D :selected nil :shows-current-buffer nil :start (:position 471 :line 11 :column 0 :text "D-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 583 :line 13 :column 18 :text "D-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 11 :dedicated nil :minibuffer nil) (:slot bottom-left :edges (0 13 40 24) :body (39 10) :buffer B :selected nil :shows-current-buffer nil :start (:position 189 :line 5 :column 0 :text "B-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 295 :line 7 :column 12 :text "B-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 5 :dedicated nil :minibuffer nil) (:slot bottom-right :edges (40 13 80 24) :body (40 10) :buffer C :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "C-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 439 :line 10 :column 15 :text "C-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 8 :dedicated nil :minibuffer nil)))) (:command self-insert-command :selected top-left :buffer A :map-active nil :right-binding right-char)) :transient-calls 1 :map-active nil :owned-hook-present nil :right-binding right-char :unread nil :executing nil :selected-buffer A :selected-state (:point (:position 152 :line 4 :column 10 :text "A-04 | abacdefghijklmnopqrstuvwxyz 0123456789 界") :modified t :undo t) :buffers ((:buffer A :text "A-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-04 | abacdefghijklmnopqrstuvwxyz 0123456789 界\nA-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer B :text "B-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer C :text "C-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nC-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer D :text "D-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nD-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n")))) :cleanup clean)"#
        ]],
    )
}

fn failures_preserve_write_order_and_recover() -> ParityBatchCase {
    let missing = blocked(
        WindowLayout::SingleWindow,
        WindowSlot::Main,
        Direction::Left,
        None,
        BlockReason::NoNeighbor,
    );
    let target_dedicated = blocked(
        WindowLayout::HorizontalPair,
        WindowSlot::Left,
        Direction::Right,
        Some(WindowSlot::Right),
        BlockReason::TargetDedicated,
    );
    let source_dedicated = blocked(
        WindowLayout::HorizontalPair,
        WindowSlot::Left,
        Direction::Right,
        Some(WindowSlot::Right),
        BlockReason::SourceDedicated,
    );
    let minibuffer = blocked(
        WindowLayout::MainAndMinibuffer,
        WindowSlot::Main,
        Direction::Down,
        Some(WindowSlot::Minibuffer),
        BlockReason::Minibuffer,
    );
    let recover_right = route(
        WindowLayout::HorizontalPair,
        WindowSlot::Left,
        Direction::Right,
        WindowSlot::Right,
        MoveBehavior::Swap,
        SelectionRequest::FollowDestination,
    );
    let probe = format!(
        r###"
(buffer366-test-run
 "directional_failures_preserve_exact_write_order_and_recover"
 (lambda ()
   (let ((provenance (buffer366-test-provenance))
         missing-state target-state minibuffer-state source-state)
     ;; No-neighbor is atomic, then adding a genuine right neighbor makes the
     ;; same process usable through the public wrapper.
     (buffer366-test-single-layout "missing")
     (let ((main (buffer366-test-slot-window 'main))
           before
           condition after recovery)
       (set-window-prev-buffers main nil)
       (set-window-next-buffers main nil)
       (setq before (buffer366-test-layout-state t))
       (setq condition (buffer366-test-invoke-blocked {missing})
             after (buffer366-test-layout-state t))
       (let* ((left (buffer366-test-slot-window 'main))
              (right (split-window left nil 'right))
              (b (buffer366-test-new-buffer "bm366-missing-B" 'B "B")))
         (buffer366-test-register-layout
          'horizontal-pair (list (cons 'left left) (cons 'right right)))
         (buffer366-test-set-view right b 8 10 13 6)
         (set-window-prev-buffers left nil)
         (set-window-next-buffers left nil)
         (set-window-prev-buffers right nil)
         (set-window-next-buffers right nil)
         (select-window left)
         (setq recovery
               (buffer366-test-invoke-existing {recover_right})))
       (setq missing-state
             (list :route (buffer366-test-route-state {missing})
                   :before before :condition condition :after after
                   :unchanged (equal before after) :recovery recovery
                   :buffers (buffer366-test-owned-buffer-bytes))))

     ;; Target dedication is checked by the package before its first write.
     (buffer366-test-horizontal-layout "target-dedicated")
     (let ((left (buffer366-test-slot-window 'left))
           (right (buffer366-test-slot-window 'right))
           before condition after recovery)
       (dolist (window (list left right))
         (set-window-prev-buffers window nil)
         (set-window-next-buffers window nil))
       (set-window-dedicated-p right t)
       (select-window left)
       (setq before (buffer366-test-layout-state t)
             condition (buffer366-test-invoke-blocked {target_dedicated})
             after (buffer366-test-layout-state t))
       (set-window-dedicated-p right nil)
       (setq recovery (buffer366-test-invoke-existing {recover_right}))
       (setq target-state
             (list :route (buffer366-test-route-state {target_dedicated})
                   :before before :condition condition :after after
                   :unchanged (equal before after) :recovery recovery
                   :buffers (buffer366-test-owned-buffer-bytes))))

     ;; Real inactive minibuffer geometry reaches the explicit package guard.
     (buffer366-test-single-layout "minibuffer" t)
     (setq windmove-allow-all-windows t)
     (let ((main (buffer366-test-slot-window 'main))
           before condition after minibuffer-bytes recovery)
       (set-window-prev-buffers main nil)
       (set-window-next-buffers main nil)
       (setq before (buffer366-test-layout-state t)
             condition (buffer366-test-invoke-blocked {minibuffer})
             after (buffer366-test-layout-state t)
             minibuffer-bytes (buffer366-test-owned-buffer-bytes))
       (buffer366-test-horizontal-layout "minibuffer-recovery")
       (setq recovery (buffer366-test-invoke-existing {recover_right}))
       (setq minibuffer-state
             (list :route (buffer366-test-route-state {minibuffer})
                   :before before :condition condition :after after
                   :unchanged (equal before after) :recovery recovery
                   :failed-world-buffers minibuffer-bytes
                   :recovery-buffers (buffer366-test-owned-buffer-bytes))))

     ;; Under explicit default swap, source dedication is checked only after
     ;; the target received A.  Preserve that non-atomic state before repair.
     (buffer366-test-horizontal-layout "swap-source")
     (setq buffer-move-behavior 'swap)
     (let* ((left (buffer366-test-slot-window 'left))
            (right (buffer366-test-slot-window 'right))
            (b (car (rassq 'B buffer366-test-buffer-roles)))
            before condition after recovery)
       (dolist (window (list left right))
         (set-window-prev-buffers window nil)
         (set-window-next-buffers window nil))
       (set-window-dedicated-p left t)
       (select-window left)
       (setq before (buffer366-test-layout-state t)
             condition (buffer366-test-invoke-blocked {source_dedicated})
             after (buffer366-test-layout-state t))
       (set-window-dedicated-p left nil)
       (buffer366-test-set-view right b 8 10 13 6)
       ;; Reconstruct the exact owned pre-failure history before the public
       ;; recovery; the partial write must not dictate recovery by accident.
       (dolist (window (list left right))
         (set-window-prev-buffers window nil)
         (set-window-next-buffers window nil))
       (select-window left)
       (setq recovery (buffer366-test-invoke-existing {recover_right}))
       (setq source-state
             (list :route (buffer366-test-route-state {source_dedicated})
                   :before before :condition condition :after after
                   :partial-mutation (not (equal before after))
                   :recovery recovery
                   :buffers (buffer366-test-owned-buffer-bytes))))
     (list :provenance provenance
           :missing missing-state :target-dedicated target-state
           :minibuffer minibuffer-state :source-dedicated source-state))))
"###,
        missing = missing.elisp(),
        target_dedicated = target_dedicated.elisp(),
        minibuffer = minibuffer.elisp(),
        source_dedicated = source_dedicated.elisp(),
        recover_right = recover_right.elisp(),
    );
    ParityBatchCase::value(
        "directional_failures_preserve_exact_write_order_and_recover",
        probe,
        expect![[
            r#"OK (:result (:provenance (:melpa-version "20220512.755" :source-version "0.6.3" :commit "e7800b3ab1bd76ee475ef35507ec51ecd5a3f065" :source-sha256 "f53f8ede64251f2984cfc43e25a5f26927ce53a46be3982602093835ca2477f1" :commands (buf-move-up buf-move-down buf-move-left buf-move-right buf-move) :defaults (swap nil) :dependency-closure nil) :missing (:route (:layout single-window :from main :direction left :to nil :command buf-move-left :behavior swap :selection-request nil :block-reason no-neighbor) :before (:layout single-window :selected main :current A :windows ((:slot main :edges (0 1 80 24) :body (80 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil :prev nil :next nil))) :condition (:symbol error :data ("No window in this direction") :message "No window in this direction") :after (:layout single-window :selected main :current A :windows ((:slot main :edges (0 1 80 24) :body (80 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil :prev nil :next nil))) :unchanged t :recovery (:layout horizontal-pair :selected right :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer B :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "B-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 437 :line 10 :column 13 :text "B-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 6 :dedicated nil :minibuffer nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil))) :buffers ((:buffer A :text "A-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer B :text "B-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n"))) :target-dedicated (:route (:layout horizontal-pair :from left :direction right :to right :command buf-move-right :behavior swap :selection-request nil :block-reason target-dedicated) :before (:layout horizontal-pair :selected left :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil :prev nil :next nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer B :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "B-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 437 :line 10 :column 13 :text "B-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 6 :dedicated t :minibuffer nil :prev nil :next nil))) :condition (:symbol error :data ("The window in this direction is dedicated") :message "The window in this direction is dedicated") :after (:layout horizontal-pair :selected left :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil :prev nil :next nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer B :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "B-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 437 :line 10 :column 13 :text "B-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 6 :dedicated t :minibuffer nil :prev nil :next nil))) :unchanged t :recovery (:layout horizontal-pair :selected right :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer B :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "B-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 437 :line 10 :column 13 :text "B-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 6 :dedicated nil :minibuffer nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil))) :buffers ((:buffer A :text "A-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer B :text "B-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n"))) :minibuffer (:route (:layout main-and-minibuffer :from main :direction down :to minibuffer :command buf-move-down :behavior swap :selection-request nil :block-reason minibuffer) :before (:layout main-and-minibuffer :selected main :current A :windows ((:slot main :edges (0 1 80 24) :body (80 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil :prev nil :next nil) (:slot minibuffer :edges (0 24 80 25) :body (80 1) :buffer minibuffer-buffer :selected nil :shows-current-buffer nil :start (:position 1 :line 1 :column 0 :text "") :point (:position 1 :line 1 :column 0 :text "") :hscroll 0 :dedicated nil :minibuffer t :prev :not-applicable :next :not-applicable))) :condition (:symbol error :data ("The window in this direction is the Minibuffer") :message "The window in this direction is the Minibuffer") :after (:layout main-and-minibuffer :selected main :current A :windows ((:slot main :edges (0 1 80 24) :body (80 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil :prev nil :next nil) (:slot minibuffer :edges (0 24 80 25) :body (80 1) :buffer minibuffer-buffer :selected nil :shows-current-buffer nil :start (:position 1 :line 1 :column 0 :text "") :point (:position 1 :line 1 :column 0 :text "") :hscroll 0 :dedicated nil :minibuffer t :prev :not-applicable :next :not-applicable))) :unchanged t :recovery (:layout horizontal-pair :selected right :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer B :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "B-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 437 :line 10 :column 13 :text "B-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 6 :dedicated nil :minibuffer nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil))) :failed-world-buffers ((:buffer A :text "A-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n")) :recovery-buffers ((:buffer A :text "A-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer B :text "B-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n"))) :source-dedicated (:route (:layout horizontal-pair :from left :direction right :to right :command buf-move-right :behavior swap :selection-request nil :block-reason source-dedicated) :before (:layout horizontal-pair :selected left :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated t :minibuffer nil :prev nil :next nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer B :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "B-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 437 :line 10 :column 13 :text "B-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 6 :dedicated nil :minibuffer nil :prev nil :next nil))) :condition (:symbol error :data ("Window is dedicated to ‘bm366-swap-source-A’") :message "Window is dedicated to ‘bm366-swap-source-A’") :after (:layout horizontal-pair :selected left :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated t :minibuffer nil :prev nil :next nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer A :selected nil :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil :prev ((:buffer B :start 330 :start-insertion nil :point 437 :point-insertion nil)) :next nil))) :partial-mutation t :recovery (:layout horizontal-pair :selected right :current A :windows ((:slot left :edges (0 1 40 24) :body (39 22) :buffer B :selected nil :shows-current-buffer nil :start (:position 330 :line 8 :column 0 :text "B-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 437 :line 10 :column 13 :text "B-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 6 :dedicated nil :minibuffer nil) (:slot right :edges (40 1 80 24) :body (40 22) :buffer A :selected t :shows-current-buffer t :start (:position 48 :line 2 :column 0 :text "A-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :point (:position 151 :line 4 :column 9 :text "A-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界") :hscroll 2 :dedicated nil :minibuffer nil))) :buffers ((:buffer A :text "A-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nA-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n") (:buffer B :text "B-01 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-02 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-03 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-04 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-05 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-06 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-07 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-08 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-09 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-10 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-11 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-12 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-13 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-14 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-15 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-16 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-17 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-18 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-19 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-20 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-21 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-22 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-23 | abcdefghijklmnopqrstuvwxyz 0123456789 界\nB-24 | abcdefghijklmnopqrstuvwxyz 0123456789 界\n")))) :cleanup clean)"#
        ]],
    )
}

pub(crate) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        readme_swap_story(),
        move_uses_real_window_history(),
        transient_arrows_repeat_and_fall_through(),
        failures_preserve_write_order_and_recover(),
    ]
}
