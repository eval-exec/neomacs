//! A routed selection request names the requester's display (remote-display
//! M3, u4 S6). `select-frame` lives in the display crate, so this test cannot
//! stay in eval.
//!
//! The five selection primitives take no terminal, so they address GNU's
//! `check_x_display_info (Qnil)`: the selected frame's display, else the
//! head of `x_display_list` (src/xfns.c:176-185 at a360712c, UNATTESTED).
//! A Green worker's request crosses to main, which owns the displays; the
//! selected frame is the requester's to read, so it travels with the request.

use std::cell::RefCell;
use std::rc::Rc;

use neovm_eval::test_support::authority::start_green_threads_for_test;

use crate::emacs_core::Value;
use crate::emacs_core::eval::{
    DisplayConnection, DisplayServices, GuiFrameHostRequest, TerminalBacking, TerminalNumber,
};
use crate::window::DisplayFrameManagerExt;

/// A display that records every clipboard text it receives.
#[derive(Clone, Default)]
struct ClipboardProbe(Rc<RefCell<Vec<Option<String>>>>);

impl DisplayConnection for ClipboardProbe {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn set_clipboard_text(&mut self, text: Option<&str>) -> Result<(), String> {
        self.0.borrow_mut().push(text.map(str::to_owned));
        Ok(())
    }
}

impl DisplayServices for ClipboardProbe {}

#[test]
fn routed_selection_follows_the_requesters_selected_frame_terminal() {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut context = crate::test_utils::new_context();
    context.set_green_threading_mode_at_construction_for_test();
    start_green_threads_for_test(&mut context).unwrap();

    let scratch = context.buffer_manager_mut().create_buffer("*scratch*");
    context.buffer_manager_mut().set_current(scratch);
    let frame_a = context.frame_manager_mut().create_frame_on_terminal(
        "A",
        TerminalNumber::INITIAL.get(),
        800,
        600,
        scratch,
    );
    let terminal_b = TerminalNumber::new(1);
    let frame_b = context.frame_manager_mut().create_frame_on_terminal(
        "B",
        terminal_b.get(),
        800,
        600,
        scratch,
    );
    assert!(context.frame_manager_mut().select_frame(frame_a));
    let display_a = ClipboardProbe::default();
    let display_b = ClipboardProbe::default();
    context.set_display_host(Box::new(display_a.clone()));
    context.register_display_terminal(
        terminal_b,
        TerminalBacking::window_system(Box::new(display_b.clone())),
    );
    context.set_variable("m3-frame-b", Value::make_frame(frame_b.0));

    // One worker, one park of main: the first request is served while A's
    // frame is selected, the second after the worker selected B's frame.
    context
        .eval_str(
            "(thread-join
               (make-thread
                 (lambda ()
                   (neomacs-clipboard-set \"on-a\")
                   (select-frame m3-frame-b)
                   (neomacs-clipboard-set \"on-b\"))))",
        )
        .expect("worker selection requests");

    assert_eq!(*display_a.0.borrow(), vec![Some("on-a".to_owned())]);
    assert_eq!(
        *display_b.0.borrow(),
        vec![Some("on-b".to_owned())],
        "the second request follows the frame the worker selected"
    );
}
