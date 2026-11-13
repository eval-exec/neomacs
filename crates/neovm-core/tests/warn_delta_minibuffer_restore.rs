//! Warn-delta P2 row 5 oracle (GNU minibuf.c): on minibuffer exit GNU restores
//! the mini-window's previous buffer, window-start and window-point
//! (`minibuffer_unwind`, src/minibuf.c:1253-1281: set_window_buffer +
//! Fset_window_start + Fset_window_point from `w->prev_buffers`) and the
//! calling frame's selected window (`read_minibuf_unwind`, src/minibuf.c:1100
//! and its Fset_frame_selected_window block).
//!
//! Neomacs restores the buffer/start/point half from the separately
//! registered `MinibufferBuffer` specpdl entry (`unwind_minibuffer_buffer` ->
//! `redisplay::restore_buffer`, the analogue of GNU's separately registered
//! `minibuffer_unwind`); the Legacy path additionally restores it through
//! `restore_minibuffer_window_in_state`. This test drives a real minibuffer
//! read under the GNU redisplay policy and pins that GNU contract; it also
//! shows the deleted `restore_minibuffer_window` duplicate was not needed.

use neovm_core::host_input::HostInputPublisher;
use neovm_core::keyboard::{InputEvent, KeyEvent};
use neovm_eval::emacs_core::eval::RedisplayHookPolicyGuard;

#[test]
fn minibuffer_exit_restores_the_previous_minibuffer_window_state_under_gnu_hooks() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = neovm_core::emacs_core::load_boot::create_runtime_startup_evaluator_cached()
        .expect("runtime startup evaluator");

    eval.eval_str(
        r#"(progn
             (let ((mb (minibuffer-window)))
               (set-window-buffer mb (get-buffer-create " *Minibuf-0*"))
               (with-current-buffer " *Minibuf-0*"
                 (erase-buffer)
                 (insert "zero-one\ntwo\nthree\n"))
               (set-window-start mb 2)
               (set-window-point mb 6))
             nil)"#,
    )
    .expect("seed the inactive minibuffer window state");

    let before = format!(
        "{}",
        eval.eval_str(
            "(list (buffer-name (window-buffer (minibuffer-window)))
                   (window-start (minibuffer-window))
                   (window-point (minibuffer-window)))"
        )
        .expect("capture the previous minibuffer window state")
    );

    let tx = HostInputPublisher::install(&mut eval);
    tx.send(InputEvent::key_press(KeyEvent::char(' ')))
        .expect("queue the minibuffer exit key");

    eval.eval_str(
        r#"(let ((map (make-sparse-keymap)))
             (define-key map " " #'exit-minibuffer)
             (read-from-minibuffer "Prompt: " nil map))"#,
    )
    .expect("read-from-minibuffer with a real exit key");

    let after = format!(
        "{}",
        eval.eval_str(
            "(list (buffer-name (window-buffer (minibuffer-window)))
                   (window-start (minibuffer-window))
                   (window-point (minibuffer-window)))"
        )
        .expect("capture the restored minibuffer window state")
    );

    assert_eq!(
        after, before,
        "GNU minibuffer_unwind restores the mini-window buffer, start and point"
    );
}
