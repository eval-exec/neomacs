//! #395 control: real terminal commands must edit the selected virtual frame.
//! Native compositor focus is exercised separately by the GUI tests.
#![cfg(unix)]

use crate::support::*;
use neomacs_tui_tests::TuiSession;
use std::time::Duration;

#[test]
fn tty_new_and_reselected_frames_route_keys_to_their_own_buffers() {
    let (mut gnu, mut neo) = boot_pair("");
    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(progn
             (setq issue395-primary-frame (selected-frame)
                   issue395-created-frame nil)
             (with-current-buffer (get-buffer-create "*issue395-secondary*")
               (erase-buffer) (insert "secondary-ready\n"))
             (defun issue395-frame-created (frame)
               (setq issue395-created-frame frame)
               (set-window-buffer (frame-selected-window frame) "*issue395-secondary*")
               (set-window-point (frame-selected-window frame)
                 (with-current-buffer "*issue395-secondary*" (point-max))))
             (add-hook 'after-make-frame-functions #'issue395-frame-created)
             (switch-to-buffer (get-buffer-create "*issue395-primary*"))
             (erase-buffer) (insert "primary-ready\n"))"#,
    );
    expect_line(&mut gnu, &mut neo, "primary-ready");
    send_both_raw(&mut gnu, &mut neo, b"p");
    expect_line(&mut gnu, &mut neo, "p");

    send_both(&mut gnu, &mut neo, "C-x 5 2");
    expect_line(&mut gnu, &mut neo, "secondary-ready");
    send_both_raw(&mut gnu, &mut neo, b"n");
    expect_line(&mut gnu, &mut neo, "n");

    send_both(&mut gnu, &mut neo, "C-x 5 o");
    expect_line(&mut gnu, &mut neo, "primary-ready");
    send_both_raw(&mut gnu, &mut neo, b"a");
    expect_line(&mut gnu, &mut neo, "pa");
    send_both(&mut gnu, &mut neo, "C-x 5 o");
    expect_line(&mut gnu, &mut neo, "secondary-ready");
    send_both_raw(&mut gnu, &mut neo, b"b");
    expect_line(&mut gnu, &mut neo, "nb");

    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(message "ROUTED:%s:%s:%S"
             (with-current-buffer "*issue395-primary*"
               (buffer-substring-no-properties (line-beginning-position) (line-end-position)))
             (with-current-buffer "*issue395-secondary*"
               (buffer-substring-no-properties (line-beginning-position) (line-end-position)))
             (eq (selected-frame) issue395-created-frame))"#,
    );
    expect_line(&mut gnu, &mut neo, "\"ROUTED:pa:nb:t\"");
}

fn expect_line(gnu: &mut TuiSession, neo: &mut TuiSession, expected: &str) {
    let received = |grid: &[String]| grid.iter().any(|row| row.trim() == expected);
    wait_for_both(gnu, neo, Duration::from_secs(12), received);
    for (label, session) in [("GNU", gnu), ("Neomacs", neo)] {
        assert!(
            received(&session.text_grid()),
            "{label}: missing {expected:?}:\n{}",
            session.text_grid().join("\n")
        );
    }
}
