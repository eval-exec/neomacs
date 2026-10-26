//! Unix TTY control for #398's symbolic bindings, using actual PTY bytes.
//! The explicit input-decode-map entries describe a configured terminal;
//! they do not assert that every terminal emits these sequences by default.
//! Native Wayland/X11 delivery is exercised by the separate GUI regression.
#![cfg(unix)]

use crate::support::*;
use std::time::Duration;

#[test]
fn configured_terminal_sequences_dispatch_extended_function_keys() {
    let (mut gnu, mut neo) = boot_pair("");
    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(progn
             (define-key input-decode-map "\e[25~" [f13])
             (define-key input-decode-map "\e[56~" [f35])
             (defun issue398-tty-f13 ()
               (interactive) (insert "received-f13\n"))
             (defun issue398-tty-f35 ()
               (interactive) (insert "received-f35\n"))
             (local-set-key [f13] #'issue398-tty-f13)
             (local-set-key [f35] #'issue398-tty-f35)
             (erase-buffer)
             (insert "issue398-tty-ready\n"))"#,
    );
    let ready = |grid: &[String]| grid.iter().any(|row| row.trim() == "issue398-tty-ready");
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            ready(&session.text_grid()),
            "{label}: fixture never became ready"
        );
    }
    for (bytes, marker) in [
        (b"\x1b[25~".as_slice(), "received-f13"),
        (b"\x1b[56~".as_slice(), "received-f35"),
    ] {
        send_both_raw(&mut gnu, &mut neo, bytes);
        let received = |grid: &[String]| grid.iter().any(|row| row.trim() == marker);
        gnu.read_until(Duration::from_secs(8), received);
        neo.read_until(Duration::from_secs(12), received);
        for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
            assert!(
                received(&session.text_grid()),
                "{label}: missing {marker} after raw terminal input:\n{}",
                session.text_grid().join("\n")
            );
        }
    }
}
