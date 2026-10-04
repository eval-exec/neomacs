#![cfg(unix)]
//! Public terminal coverage of the body-invalidation contract. TTY frames
//! cannot show avatars, so use an in-place mutation of a buffer display spec.

use crate::support::*;
use std::time::Duration;

#[test]
fn force_window_update_repaints_mutated_display_spec_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(progn
          (switch-to-buffer (get-buffer-create "*forced-body*"))
          (erase-buffer)
          (insert "xFORCED-BODY\n")
          (setq neo-forced-body-spec (list 'space :width 2))
          (put-text-property 1 2 'display neo-forced-body-spec)
          (goto-char (point-max))
          nil)"#,
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.starts_with("  FORCED-BODY"))
    });
    for (name, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            session
                .text_grid()
                .iter()
                .any(|row| row.starts_with("  FORCED-BODY")),
            "{name} must render the initial two-cell display spec:\n{}",
            session.text_grid().join("\n")
        );
    }

    // No text/property assignment: the same cons changes behind the buffer
    // ticks. The explicit force must repaint without a further keypress.
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (setcar (nthcdr 2 neo-forced-body-spec) 6) (force-window-update (current-buffer)) nil)",
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.starts_with("      FORCED-BODY"))
    });
    for (name, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            session
                .text_grid()
                .iter()
                .any(|row| row.starts_with("      FORCED-BODY")),
            "{name} must render the forced six-cell display spec:\n{}",
            session.text_grid().join("\n")
        );
    }
    assert_pair_exact_display("forced mutable display spec", &gnu, &neo);
}
