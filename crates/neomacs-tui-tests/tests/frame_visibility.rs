#![cfg(unix)]
//! TUI tests for child frame visibility lifecycle.
//!
//! Exercises the `set-frame-parameter 'visibility` → host notification
//! path that was previously broken (GUI child frames were not removed
//! from the compositor cache when hidden via `modify-frame-parameters`).
//!
//! These tests use `eval_expression` to drive both GNU Emacs and Neomacs
//! through the same elisp, then verify the echo area or buffer content
//! matches. In TUI mode child frames are not composited as floating
//! overlays, but the underlying frame visibility state and the
//! `(frame-visible-p)` predicate are still meaningful and must match.

use crate::support;

use std::time::Duration;
use support::*;

/// `make-frame-invisible` then `make-frame-visible` should work
/// and report correct visibility state via `frame-visible-p`.
#[test]
fn frame_visibility_toggle_via_make_frame_invisible_visible() {
    let (mut gnu, mut neo) = boot_pair("");

    // Check initial visibility
    eval_expression(&mut gnu, &mut neo, "(frame-visible-p (selected-frame))");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().rev().take(4).any(|row| row.contains("t"))
    });

    // Make invisible (force since it's the only frame)
    eval_expression(
        &mut gnu,
        &mut neo,
        "(make-frame-invisible (selected-frame) t)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Verify invisible
    eval_expression(&mut gnu, &mut neo, "(frame-visible-p (selected-frame))");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().rev().take(4).any(|row| row.contains("nil"))
    });

    // Make visible again
    eval_expression(&mut gnu, &mut neo, "(make-frame-visible (selected-frame))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Verify visible again
    eval_expression(&mut gnu, &mut neo, "(frame-visible-p (selected-frame))");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().rev().take(4).any(|row| row.contains("t"))
    });
    assert_pair_exact_display(
        "frame_visibility_toggle_via_make_frame_invisible_visible",
        &gnu,
        &neo,
    );
}

/// `set-frame-parameter 'visibility nil` should hide the frame,
/// matching `make-frame-invisible` behavior.
#[test]
fn frame_visibility_via_set_frame_parameter_nil() {
    let (mut gnu, mut neo) = boot_pair("");

    // Verify initially visible
    eval_expression(&mut gnu, &mut neo, "(frame-visible-p (selected-frame))");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().rev().take(4).any(|row| row.contains("t"))
    });

    // Hide via set-frame-parameter (this was the broken path)
    eval_expression(
        &mut gnu,
        &mut neo,
        "(set-frame-parameter nil 'visibility nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Verify invisible
    eval_expression(&mut gnu, &mut neo, "(frame-visible-p (selected-frame))");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().rev().take(4).any(|row| row.contains("nil"))
    });

    // Restore via set-frame-parameter
    eval_expression(
        &mut gnu,
        &mut neo,
        "(set-frame-parameter nil 'visibility t)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Verify visible
    eval_expression(&mut gnu, &mut neo, "(frame-visible-p (selected-frame))");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().rev().take(4).any(|row| row.contains("t"))
    });
    assert_pair_exact_display("frame_visibility_via_set_frame_parameter_nil", &gnu, &neo);
}

/// `iconify-frame` should hide the frame.
#[test]
fn frame_visibility_via_iconify_frame() {
    let (mut gnu, mut neo) = boot_pair("");

    eval_expression(&mut gnu, &mut neo, "(iconify-frame (selected-frame))");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Verify invisible (iconified frames report nil for frame-visible-p)
    eval_expression(&mut gnu, &mut neo, "(frame-visible-p (selected-frame))");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().rev().take(4).any(|row| row.contains("nil"))
    });
    assert_pair_exact_display("frame_visibility_via_iconify_frame", &gnu, &neo);
}

/// Create a child frame, verify it exists, hide it via
/// `set-frame-parameter 'visibility nil`, then verify its
/// visibility state matches GNU.
#[test]
fn child_frame_visibility_lifecycle() {
    let (mut gnu, mut neo) = boot_pair("");

    // Exercise the complete lifecycle in one expression so the temporary
    // frame cannot become the input frame for a later M-: invocation.
    eval_expression(
        &mut gnu,
        &mut neo,
        "(let ((cf (make-frame `((parent-frame . ,(selected-frame)) \
                                  (width . 30) (height . 5) \
                                  (minibuffer . nil))))) \
          (unwind-protect \
              (progn \
                (set-frame-parameter cf 'visibility nil) \
                (prin1-to-string \
                  (list (frame-live-p cf) (frame-visible-p cf)))) \
            (delete-frame cf)))",
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), |grid| {
        grid.iter().rev().take(4).any(|row| row.contains("(t nil)"))
    });
    assert_pair_exact_display("child_frame_visibility_lifecycle", &gnu, &neo);
}

/// Visibility parameter should be stored and retrievable via
/// `frame-parameter`.
#[test]
fn frame_parameter_visibility_roundtrip() {
    let (mut gnu, mut neo) = boot_pair("");

    // Set visibility to nil via set-frame-parameter
    eval_expression(
        &mut gnu,
        &mut neo,
        "(set-frame-parameter nil 'visibility nil)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Read it back
    eval_expression(
        &mut gnu,
        &mut neo,
        "(prin1-to-string (frame-parameter nil 'visibility))",
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().rev().take(4).any(|row| row.contains("nil"))
    });

    // Set back to t
    eval_expression(
        &mut gnu,
        &mut neo,
        "(set-frame-parameter nil 'visibility t)",
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Read it back
    eval_expression(
        &mut gnu,
        &mut neo,
        "(prin1-to-string (frame-parameter nil 'visibility))",
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().rev().take(4).any(|row| row.contains("t"))
    });
    assert_pair_exact_display("frame_parameter_visibility_roundtrip", &gnu, &neo);
}

/// Multiple visibility toggles in sequence should be consistent.
#[test]
fn frame_visibility_multiple_toggles() {
    let (mut gnu, mut neo) = boot_pair("");

    for _ in 0..3 {
        // Hide
        eval_expression(
            &mut gnu,
            &mut neo,
            "(make-frame-invisible (selected-frame) t)",
        );
        read_both(&mut gnu, &mut neo, Duration::from_millis(500));

        eval_expression(
            &mut gnu,
            &mut neo,
            "(prin1-to-string (frame-visible-p (selected-frame)))",
        );
        wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
            grid.iter().rev().take(4).any(|row| row.contains("nil"))
        });

        // Show
        eval_expression(&mut gnu, &mut neo, "(make-frame-visible (selected-frame))");
        read_both(&mut gnu, &mut neo, Duration::from_millis(500));

        eval_expression(
            &mut gnu,
            &mut neo,
            "(prin1-to-string (frame-visible-p (selected-frame)))",
        );
        wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
            grid.iter().rev().take(4).any(|row| row.contains("t"))
        });
    }
    assert_pair_exact_display("frame_visibility_multiple_toggles", &gnu, &neo);
}
