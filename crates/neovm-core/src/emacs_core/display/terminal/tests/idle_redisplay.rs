//! GNU `resume-tty` (term.c) revives the terminal frames, requiring a fresh
//! presentation even if no text or window state changed while suspended.

use super::*;
use std::cell::Cell;

/// Owns test-only terminal and redisplay state on the current test thread;
/// neither the fixture nor its policy is shared between Lisp mutators.
struct TerminalIdleFixture;

impl Drop for TerminalIdleFixture {
    fn drop(&mut self) {
        crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(None);
        reset_terminal_thread_locals();
    }
}

#[test]
fn resumed_tty_invalidates_idle_redisplay() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(Some(true));
    let _fixture = TerminalIdleFixture;
    let mut eval = Context::new();
    configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_owned()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    let buffer = eval.buffers.current_buffer_id().unwrap();
    eval.frames
        .create_frame("resume-idle-tty", 640, 384, buffer);
    let log = Rc::new(RefCell::new(Vec::new()));
    set_terminal_host(Box::new(RecordingTerminalHost {
        log: Rc::clone(&log),
    }));
    let layouts = Rc::new(Cell::new(0));
    let observed = Rc::clone(&layouts);
    eval.redisplay_fn = Some(Box::new(move |eval| {
        observed.set(observed.get() + 1);
        eval.buffers
            .current_buffer()
            .unwrap()
            .reset_unchanged_region();
    }));
    eval.redisplay_with_force(true);
    eval.redisplay_with_force(true);
    assert_eq!(layouts.get(), 1, "unchanged terminal must skip layout");
    builtin_suspend_tty(&mut eval, vec![]).unwrap();
    builtin_resume_tty(&mut eval, vec![]).unwrap();
    assert_eq!(log.borrow().as_slice(), &["suspend", "resume"]);
    eval.redisplay_with_force(true);
    assert_eq!(
        layouts.get(),
        2,
        "resume must consume the host's repaint request"
    );
    builtin_resume_tty(&mut eval, vec![]).unwrap();
    eval.redisplay_with_force(true);
    assert_eq!(layouts.get(), 2, "resuming an active terminal is a no-op");
}
