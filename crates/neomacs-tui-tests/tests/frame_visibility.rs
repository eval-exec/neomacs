#![cfg(unix)]
//! TUI tests for child frame visibility lifecycle.
//!
//! GNU's `make-frame-invisible` only hides child frames on a text terminal
//! (`frame.c`, `Fmake_frame_invisible`). Each probe therefore creates a child,
//! samples its actual visibility transitions in one expression, and restores
//! the parent before observing the result. No M-: command is sent to a hidden
//! frame. The computed result cannot be mistaken for the expression's input.

use crate::support;

use std::time::Duration;
use support::*;

#[derive(Clone, Copy, Debug)]
enum VisibilityProbe {
    MakeInvisibleVisible,
    SetParameter,
    Iconify,
    ChildLifecycle,
    ParameterRoundtrip,
    MultipleToggles,
}

fn assert_visibility_probe(probe: VisibilityProbe) {
    let (label, body, expected_states) = match probe {
        VisibilityProbe::MakeInvisibleVisible => (
            "frame_visibility_toggle_via_make_frame_invisible_visible",
            r#"(let ((before (frame-visible-p cf)) hidden)
                 (make-frame-invisible cf t)
                 (setq hidden (frame-visible-p cf))
                 (make-frame-visible cf)
                 (list before hidden (frame-visible-p cf)))"#,
            "(t nil t)",
        ),
        VisibilityProbe::SetParameter => (
            "frame_visibility_via_set_frame_parameter_nil",
            r#"(let ((before (frame-visible-p cf)) hidden)
                 (set-frame-parameter cf 'visibility nil)
                 (setq hidden (frame-visible-p cf))
                 (set-frame-parameter cf 'visibility t)
                 (list before hidden (frame-visible-p cf)))"#,
            "(t nil t)",
        ),
        VisibilityProbe::Iconify => (
            "frame_visibility_via_iconify_frame",
            // GNU documents `make-invisible` as a child-frame iconify policy
            // (`frame.c`, `Ficonify_frame`; lispref, `iconify-child-frame`).
            r#"(let ((iconify-child-frame 'make-invisible)
                     (before (frame-visible-p cf)) hidden)
                 (iconify-frame cf)
                 (setq hidden (frame-visible-p cf))
                 (make-frame-visible cf)
                 (list before hidden (frame-visible-p cf)))"#,
            "(t nil t)",
        ),
        VisibilityProbe::ChildLifecycle => (
            "child_frame_visibility_lifecycle",
            r#"(let ((before (frame-visible-p cf)))
                 (set-frame-parameter cf 'visibility nil)
                 (list (frame-live-p cf) before (frame-visible-p cf)))"#,
            "(t t nil)",
        ),
        VisibilityProbe::ParameterRoundtrip => (
            "frame_parameter_visibility_roundtrip",
            r#"(let ((before (frame-parameter cf 'visibility)) hidden)
                 (set-frame-parameter cf 'visibility nil)
                 (setq hidden (frame-parameter cf 'visibility))
                 (set-frame-parameter cf 'visibility t)
                 (list before hidden (frame-parameter cf 'visibility)))"#,
            "(t nil t)",
        ),
        VisibilityProbe::MultipleToggles => (
            "frame_visibility_multiple_toggles",
            r#"(let (states)
                 (dotimes (_ 3)
                   (let ((before (frame-visible-p cf)) hidden)
                     (make-frame-invisible cf t)
                     (setq hidden (frame-visible-p cf))
                     (make-frame-visible cf)
                     (push (list before hidden (frame-visible-p cf)) states)))
                 (nreverse states))"#,
            "((t nil t) (t nil t) (t nil t))",
        ),
    };
    let (mut gnu, mut neo) = boot_pair("");
    let expression = format!(
        r#"(let* ((parent (selected-frame))
                  (cf (make-frame `((parent-frame . ,parent)
                                    (width . 30) (height . 5)
                                    (minibuffer . nil))))
                  (states (unwind-protect
                              {body}
                            (make-frame-visible cf)
                            (select-frame parent)
                            (delete-frame cf))))
             (format "visibility:%S" states))"#
    );
    eval_expression(&mut gnu, &mut neo, &expression);
    let expected = format!("\"visibility:{expected_states}\"");
    let completed = |grid: &[String]| {
        grid.last()
            .is_some_and(|row| row.trim() == expected.as_str())
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), completed);
    // wait_for_both is a bounded observation, not an assertion. Require each
    // editor's actual result before comparing the rest of the terminal.
    for session in [&gnu, &neo] {
        let grid = session.text_grid();
        assert!(
            completed(&grid),
            "{} did not finish {label} with {expected}:\n{}",
            session.name,
            grid.join("\n")
        );
    }
    assert_pair_exact_display(label, &gnu, &neo);
}

/// `make-frame-invisible` and `make-frame-visible` sample visible, hidden,
/// and restored states on the same child.
#[test]
fn frame_visibility_toggle_via_make_frame_invisible_visible() {
    assert_visibility_probe(VisibilityProbe::MakeInvisibleVisible);
}

/// The visibility parameter hides and restores a child frame.
#[test]
fn frame_visibility_via_set_frame_parameter_nil() {
    assert_visibility_probe(VisibilityProbe::SetParameter);
}

/// The explicit `make-invisible` child iconify policy hides the child.
#[test]
fn frame_visibility_via_iconify_frame() {
    assert_visibility_probe(VisibilityProbe::Iconify);
}

/// A created child stays live while hidden, then is cleaned up.
#[test]
fn child_frame_visibility_lifecycle() {
    assert_visibility_probe(VisibilityProbe::ChildLifecycle);
}

/// The visibility parameter records both hidden and restored states.
#[test]
fn frame_parameter_visibility_roundtrip() {
    assert_visibility_probe(VisibilityProbe::ParameterRoundtrip);
}

/// Three complete hide/show cycles retain every transition on the same child.
#[test]
fn frame_visibility_multiple_toggles() {
    assert_visibility_probe(VisibilityProbe::MultipleToggles);
}
