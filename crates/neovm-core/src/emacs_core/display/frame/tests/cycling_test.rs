//! GNU frame.c candidate filtering and cyclic frame-list traversal.

use crate::emacs_core::{Context, Value};
use crate::window::{FrameId, FrameVisibility};

struct Frames {
    eval: Context,
    first: FrameId,
    second: FrameId,
    third: FrameId,
}

impl Frames {
    fn new() -> Self {
        Self::with_context(Context::new())
    }

    fn with_context(mut eval: Context) -> Self {
        let buffer = eval.buffers.create_buffer("*frame-cycle*");
        eval.buffers.set_current(buffer);
        let first = eval.frames.create_frame("first", 800, 600, buffer);
        let second = eval.frames.create_frame("second", 800, 600, buffer);
        let third = eval.frames.create_frame("third", 800, 600, buffer);
        for (name, id) in [
            ("cycle-first", first),
            ("cycle-second", second),
            ("cycle-third", third),
        ] {
            eval.obarray_mut()
                .set_symbol_value(name, Value::make_frame(id.0));
        }
        Self {
            eval,
            first,
            second,
            third,
        }
    }

    fn run(&mut self, source: &str) -> String {
        crate::emacs_core::print_value(&self.eval.eval_str(source).unwrap())
    }

    fn exclude(&mut self, id: FrameId) {
        self.eval
            .frames
            .get_mut(id)
            .unwrap()
            .set_parameter(Value::symbol("no-other-frame"), Value::T);
    }
}

#[test]
fn frame_list_follows_gnu_newest_first_creation_order() {
    let mut frames = Frames::new();
    assert_eq!(
        frames.run("(equal (frame-list) (list cycle-third cycle-second cycle-first))"),
        "t"
    );
}

#[test]
fn frame_cycling_wraps_forward_and_backward_in_frame_list_order() {
    let mut frames = Frames::new();
    assert_eq!(
        frames.run(
            "(list (eq (next-frame cycle-first) cycle-third)
               (eq (next-frame cycle-third) cycle-second)
               (eq (next-frame cycle-second) cycle-first)
               (eq (previous-frame cycle-first) cycle-second)
               (eq (previous-frame cycle-second) cycle-third)
               (eq (previous-frame cycle-third) cycle-first))"
        ),
        "(t t t t t t)"
    );
}

#[test]
fn explicit_cycle_origin_does_not_change_current_selection() {
    let mut frames = Frames::new();
    assert_eq!(
        frames.run(
            "(let ((window (selected-window)) (buffer (current-buffer)))
           (list (eq (next-frame cycle-second) cycle-first)
                 (eq (previous-frame cycle-second) cycle-third)
                 (eq (next-frame) cycle-third)
                 (eq (previous-frame nil) cycle-second)
                 (eq (selected-frame) cycle-first)
                 (eq (selected-window) window)
                 (eq (current-buffer) buffer)))"
        ),
        "(t t t t t t t)"
    );
}

#[test]
fn ordinary_cycling_excludes_minibuffer_only_frames_but_other_filters_include_them() {
    let mut frames = Frames::new();
    let frame = frames.eval.frames.get_mut(frames.third).unwrap();
    frame.minibuffer_window = Some(frame.root_window().id());
    assert_eq!(
        frames.run(
            "(list (eq (next-frame cycle-first) cycle-second)
               (eq (previous-frame cycle-second) cycle-first)
               (eq (next-frame cycle-first t) cycle-third)
               (eq (next-frame cycle-first 'visible) cycle-third)
               (eq (next-frame cycle-first 0) cycle-third))"
        ),
        "(t t t t t)"
    );
}

#[test]
fn no_other_frame_excludes_candidates_for_every_filter() {
    let mut frames = Frames::new();
    frames.exclude(frames.third);
    assert_eq!(
        frames.run(
            "(list (eq (next-frame cycle-first) cycle-second)
               (eq (next-frame cycle-first t) cycle-second)
               (eq (next-frame cycle-first 'visible) cycle-second)
               (eq (next-frame cycle-first 0) cycle-second)
               (eq (previous-frame cycle-second t) cycle-first))"
        ),
        "(t t t t t)"
    );
}

#[test]
fn cycle_visibility_filter_distinguishes_icons_and_other_lisp_values() {
    let mut frames = Frames::new();
    frames.eval.frames.get_mut(frames.third).unwrap().visibility = FrameVisibility::Invisible;
    frames
        .eval
        .frames
        .get_mut(frames.second)
        .unwrap()
        .visibility = FrameVisibility::Iconified;
    assert_eq!(
        frames.run(
            "(list (eq (next-frame cycle-first) cycle-third)
               (eq (next-frame cycle-first 'visible) cycle-first)
               (eq (next-frame cycle-first 0) cycle-second)
               (eq (previous-frame cycle-first 0) cycle-second)
               (eq (next-frame cycle-first t) cycle-third)
               (eq (next-frame cycle-first 0.0) cycle-third)
               (eq (next-frame cycle-first (make-symbol \"visible\")) cycle-third))"
        ),
        "(t t t t t t t)"
    );
}

#[test]
fn window_filter_includes_its_owner_shared_minibuffers_and_focus_redirections() {
    let mut frames = Frames::new();
    let owner = frames.eval.frames.get(frames.third).unwrap();
    let mini = owner.minibuffer_window.unwrap();
    let root = owner.root_window().id();
    frames
        .eval
        .obarray_mut()
        .set_symbol_value("cycle-mini", Value::make_window(mini.0));
    frames
        .eval
        .obarray_mut()
        .set_symbol_value("cycle-root", Value::make_window(root.0));
    let own_mini = frames
        .eval
        .frames
        .get(frames.second)
        .unwrap()
        .minibuffer_window;
    frames
        .eval
        .frames
        .get_mut(frames.second)
        .unwrap()
        .minibuffer_window = Some(mini);
    assert_eq!(
        frames.run(
            "(list (eq (next-frame cycle-first cycle-mini) cycle-third)
               (eq (next-frame cycle-first cycle-root) cycle-third))"
        ),
        "(t t)"
    );
    frames.exclude(frames.third);
    assert_eq!(
        frames.run("(eq (next-frame cycle-first cycle-mini) cycle-second)"),
        "t"
    );
    let redirected = frames.eval.frames.get_mut(frames.second).unwrap();
    redirected.minibuffer_window = own_mini;
    redirected.focus_frame = Value::make_frame(frames.third.0);
    assert_eq!(
        frames.run(
            "(list (eq (next-frame cycle-first cycle-mini) cycle-second)
               (eq (previous-frame cycle-first cycle-root) cycle-second))"
        ),
        "(t t)"
    );
}

#[test]
fn window_filter_retains_the_live_owner_of_a_deleted_window() {
    let mut frames = Frames::with_context(crate::test_utils::runtime_startup_context());
    // GNU accepts a window object as MINIFRAME even after deletion. Its
    // owning frame remains a candidate; window validity is a separate fact.
    assert_eq!(
        frames.run(
            "(let ((doomed (split-window (frame-root-window cycle-third) nil 'right)))
               (delete-window doomed)
               (list (windowp doomed)
                     (window-valid-p doomed)
                     (eq (next-frame cycle-first doomed) cycle-third)
                     (eq (previous-frame cycle-first doomed) cycle-third)))"
        ),
        "(t nil t t)"
    );
}

#[test]
fn window_filter_retains_the_owner_of_a_promoted_away_parent() {
    let mut frames = Frames::with_context(crate::test_utils::runtime_startup_context());
    assert_eq!(
        frames.run(
            "(let* ((survivor (frame-root-window cycle-third))
                    (doomed (split-window survivor nil 'right))
                    (parent (window-parent doomed)))
               (set-window-parameter survivor 'issue395-state 'retained)
               (delete-window doomed)
               (list (window-valid-p parent)
                     (eq (next-frame cycle-first parent) cycle-third)
                     (eq (previous-frame cycle-first parent) cycle-third)
                     (window-live-p survivor)
                     (eq (frame-root-window cycle-third) survivor)
                     (eq (window-parameter survivor 'issue395-state) 'retained)))"
        ),
        "(nil t t t t t)"
    );
}

#[test]
fn window_filter_retains_owners_and_parameters_of_deleted_subtree_nodes() {
    let mut frames = Frames::with_context(crate::test_utils::runtime_startup_context());
    assert_eq!(
        frames.run(
            "(let* ((survivor (frame-root-window cycle-third))
                    (branch (split-window survivor nil 'right))
                    (child (split-window branch nil 'below))
                    (subtree (window-parent child))
                    (parent (window-parent subtree)))
               (set-window-parameter branch 'issue395-state 'branch)
               (set-window-parameter child 'issue395-state 'child)
               (delete-window subtree)
               (list (mapcar (lambda (window)
                              (list (window-valid-p window)
                                    (eq (next-frame cycle-first window) cycle-third)
                                    (eq (previous-frame cycle-first window) cycle-third)))
                            (list subtree branch child parent))
                     (eq (window-parameter branch 'issue395-state) 'branch)
                     (eq (window-parameter child 'issue395-state) 'child)
                     (window-live-p survivor)
                     (eq (frame-root-window cycle-third) survivor)))"
        ),
        "(((nil t t) (nil t t) (nil t t) (nil t t)) t t t t)"
    );
}

#[test]
fn cycling_stays_with_its_terminal_keyboard_and_output_kind() {
    let mut frames = Frames::new();
    frames
        .eval
        .frames
        .get_mut(frames.third)
        .unwrap()
        .set_window_system(Some(Value::symbol("neomacs")));
    let buffer = frames.eval.buffers.current_buffer_id().unwrap();
    let extra = frames
        .eval
        .frames
        .create_frame_on_terminal("other-tty", 17, 800, 600, buffer);
    assert_eq!(
        frames.run(
            "(list (eq (next-frame cycle-first t) cycle-second)
               (eq (previous-frame cycle-first t) cycle-second)
               (eq (next-frame cycle-third t) cycle-third))"
        ),
        "(t t t)"
    );
    frames
        .eval
        .frames
        .get_mut(frames.second)
        .unwrap()
        .set_window_system(Some(Value::symbol("neomacs")));
    frames
        .eval
        .frames
        .get_mut(extra)
        .unwrap()
        .set_window_system(Some(Value::symbol("neomacs")));
    assert_eq!(
        frames.run(
            "(list (eq (next-frame cycle-first t) cycle-first)
               (eq (next-frame cycle-third t) cycle-second)
               (eq (previous-frame cycle-third t) cycle-second))"
        ),
        "(t t t)"
    );
}

#[test]
fn cycling_falls_back_to_its_origin_when_every_candidate_is_excluded() {
    let mut frames = Frames::new();
    for id in [frames.first, frames.second, frames.third] {
        frames.exclude(id);
    }
    assert_eq!(
        frames.run(
            "(list (eq (next-frame cycle-first) cycle-first)
               (eq (previous-frame cycle-second t) cycle-second))"
        ),
        "(t t)"
    );
}

#[test]
fn cycling_rejects_dead_and_non_frame_origins() {
    let mut frames = Frames::new();
    let _ = frames.eval.frames.delete_frame(frames.third);
    assert_eq!(
        frames.run(
            "(list (condition-case err (next-frame cycle-third)
                 (wrong-type-argument (list (car err) (car (cdr err)))))
               (condition-case err (previous-frame 17)
                 (wrong-type-argument (list (car err) (car (cdr err))))))"
        ),
        "((wrong-type-argument frame-live-p) (wrong-type-argument frame-live-p))"
    );
}
