//! The per-terminal display registry (remote-display M3).
//!
//! `Context.display_terminals` replaces the single display-host slot. These
//! tests pin the routing rules: frame-addressed calls follow the frame's
//! terminal, frame-less calls follow GNU `check_x_display_info (Qnil)`
//! (src/xfns.c:176-185 at a360712c, UNATTESTED: the selected frame's display,
//! else the head of `x_display_list`, which `x_term_init` pushes,
//! src/xterm.c:30940-30942), broadcast settings reach every connection and
//! replay on registration, and native terminal backing stays in the registry.
use super::*;
use neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities;
use neovm_core::emacs_core::terminal::pure::{
    OpenedTtyFrameHost, TerminalHost, TtyFrameHostFactory, TtyFrameOpenRequest, TtyFrameSize,
    swap_world_state, world_state_placeholder,
};
use neovm_eval::emacs_core::eval::{
    DisplayConnection, DisplayHostPlacement, DisplayServices, TerminalBacking,
    TerminalDeviceEffect, TerminalDeviceReply, TerminalDeviceRequest, TerminalNumber, TtyHost,
};
use std::cell::RefCell;
use std::rc::Rc;

const TERMINAL_B: TerminalNumber = TerminalNumber::new(1);
const TERMINAL_C: TerminalNumber = TerminalNumber::new(2);
const TTY_TERMINAL: TerminalNumber = TerminalNumber::new(3);

/// A connection that records what reached it, shared through `Rc`s so the
/// test keeps a view after the registry takes the box.
#[derive(Clone, Default)]
struct ProbeConnection {
    clipboard: Rc<RefCell<Vec<Option<String>>>>,
    popups: Rc<RefCell<Vec<PopupMenuRequest>>>,
}

impl DisplayConnection for ProbeConnection {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn set_clipboard_text(&mut self, text: Option<&str>) -> Result<(), String> {
        self.clipboard.borrow_mut().push(text.map(str::to_owned));
        Ok(())
    }

    fn clipboard_text(&mut self) -> Result<Option<String>, String> {
        Ok(self.clipboard.borrow().last().cloned().flatten())
    }

    fn show_popup_menu(&mut self, menu: PopupMenuRequest) -> Result<(), String> {
        self.popups.borrow_mut().push(menu);
        Ok(())
    }
}

/// Inert per-process services, as `TtyPopupDisplayHost`'s are: the probe can
/// stand in for the production `-nw` host, one object with both halves.
impl DisplayServices for ProbeConnection {}

/// Per-process services with every answer at its trait default.
struct InertServices;

impl DisplayServices for InertServices {}

/// The selected-frame / host combination every probe below starts from: a
/// window-system frame on terminal A (the initial terminal) and one on B.
struct TwoDisplays {
    eval: Context,
    frame_a: crate::window::FrameId,
    frame_b: crate::window::FrameId,
    host_a: RecordingDisplayHost,
    host_b: RecordingDisplayHost,
}

fn two_window_system_terminals() -> TwoDisplays {
    let mut eval = crate::test_utils::new_context();
    let scratch = eval.edit.buffers.create_buffer("*scratch*");
    eval.edit.buffers.set_current(scratch);
    let frame_a = eval.frame_manager_mut().create_frame_on_terminal(
        "A",
        TerminalNumber::INITIAL.get(),
        800,
        600,
        scratch,
    );
    let frame_b =
        eval.frame_manager_mut()
            .create_frame_on_terminal("B", TERMINAL_B.get(), 800, 600, scratch);
    for frame in [frame_a, frame_b] {
        eval.frame_manager_mut()
            .get_mut(frame)
            .expect("frame")
            .set_window_system(Some(Value::symbol("neo")));
    }
    eval.frame_manager_mut().select_frame(frame_a);
    let host_a = RecordingDisplayHost::new();
    let host_b = RecordingDisplayHost::new();
    eval.set_display_host(Box::new(host_a.clone()));
    eval.register_display_terminal(
        TERMINAL_B,
        TerminalBacking::window_system(Box::new(host_b.clone())),
    );
    TwoDisplays {
        eval,
        frame_a,
        frame_b,
        host_a,
        host_b,
    }
}

fn alpha_frames(host: &RecordingDisplayHost) -> Vec<crate::window::FrameId> {
    host.alpha_operations
        .borrow()
        .iter()
        .map(|(frame, _, _)| *frame)
        .collect()
}

#[test]
fn two_window_system_terminals_route_frame_setters_to_their_own_host() {
    let TwoDisplays {
        mut eval,
        frame_a,
        frame_b,
        host_a,
        host_b,
    } = two_window_system_terminals();

    // A frame object prints unreadably; bind it instead of printing it.
    eval.set_variable("m3-frame-b", Value::make_frame(frame_b.0));
    // `set-frame-parameter' is frame.el Lisp; this bare evaluator has the subr.
    eval.eval_str(
        "(progn (modify-frame-parameters m3-frame-b '((alpha . 50)))
                (set-frame-position m3-frame-b 10 20))",
    )
    .expect("frame setters on B");

    assert_eq!(alpha_frames(&host_b), vec![frame_b]);
    assert_eq!(*host_b.positions.borrow(), vec![(frame_b, 10, 20)]);
    assert!(alpha_frames(&host_a).is_empty(), "A saw B's alpha");
    assert!(host_a.positions.borrow().is_empty(), "A saw B's position");

    eval.set_variable("m3-frame-a", Value::make_frame(frame_a.0));
    eval.eval_str("(modify-frame-parameters m3-frame-a '((alpha . 70)))")
        .expect("frame setter on A");
    assert_eq!(alpha_frames(&host_a), vec![frame_a]);
    assert_eq!(alpha_frames(&host_b), vec![frame_b], "B saw A's alpha");
}

#[test]
fn selection_routes_to_the_selected_frames_terminal() {
    let mut eval = crate::test_utils::new_context();
    let scratch = eval.edit.buffers.create_buffer("*scratch*");
    eval.edit.buffers.set_current(scratch);
    let frame_a = eval.frame_manager_mut().create_frame_on_terminal(
        "A",
        TerminalNumber::INITIAL.get(),
        800,
        600,
        scratch,
    );
    let frame_b =
        eval.frame_manager_mut()
            .create_frame_on_terminal("B", TERMINAL_B.get(), 800, 600, scratch);
    let tty_frame =
        eval.frame_manager_mut()
            .create_frame_on_terminal("T", TTY_TERMINAL.get(), 80, 25, scratch);
    for frame in [frame_a, frame_b] {
        eval.frame_manager_mut()
            .get_mut(frame)
            .expect("frame")
            .set_window_system(Some(Value::symbol("neo")));
    }
    let a = ProbeConnection::default();
    let b = ProbeConnection::default();
    eval.install_display(
        Box::new(InertServices),
        TerminalNumber::INITIAL,
        TerminalBacking::window_system(Box::new(a.clone())),
    );
    eval.register_display_terminal(
        TERMINAL_B,
        TerminalBacking::window_system(Box::new(b.clone())),
    );

    eval.frame_manager_mut().select_frame(frame_b);
    eval.eval_str("(neomacs-clipboard-set \"to-b\")")
        .expect("clipboard set on B");
    assert_eq!(*b.clipboard.borrow(), vec![Some("to-b".to_owned())]);
    assert!(a.clipboard.borrow().is_empty(), "A saw B's selection");

    eval.frame_manager_mut().select_frame(frame_a);
    eval.eval_str("(neomacs-clipboard-set \"to-a\")")
        .expect("clipboard set on A");
    assert_eq!(*a.clipboard.borrow(), vec![Some("to-a".to_owned())]);
    assert_eq!(
        eval.eval_str("(neomacs-clipboard-get)").unwrap(),
        Value::string("to-a")
    );

    // A frame whose terminal has no display: GNU's check_x_display_info(nil)
    // falls to the head of x_display_list, the most recently opened display.
    eval.frame_manager_mut().select_frame(tty_frame);
    eval.eval_str("(neomacs-clipboard-set \"from-tty\")")
        .expect("clipboard set from a TTY frame");
    assert_eq!(
        *b.clipboard.borrow(),
        vec![Some("to-b".to_owned()), Some("from-tty".to_owned())]
    );
    assert_eq!(a.clipboard.borrow().len(), 1);
}

#[test]
fn broadcast_config_reaches_every_window_system_terminal_and_replays_on_registration() {
    let TwoDisplays {
        mut eval,
        host_a,
        host_b,
        ..
    } = two_window_system_terminals();
    eval.eval_str("(setq frame-alpha-lower-limit 30)")
        .expect("publish the alpha lower limit");
    assert_eq!(host_a.alpha_limits.borrow().as_slice(), &[0.3]);
    assert_eq!(host_b.alpha_limits.borrow().as_slice(), &[0.3]);

    let host_c = RecordingDisplayHost::new();
    eval.register_display_terminal(
        TERMINAL_C,
        TerminalBacking::window_system(Box::new(host_c.clone())),
    );
    assert_eq!(
        host_c.alpha_limits.borrow().as_slice(),
        &[0.3],
        "a connection registered later receives the published setting"
    );
    assert_eq!(host_a.alpha_limits.borrow().len(), 1, "no replay to A");
    assert_eq!(host_b.alpha_limits.borrow().len(), 1, "no replay to B");
}

#[test]
fn settings_published_before_any_display_are_not_replayed() {
    let mut eval = crate::test_utils::new_context();
    crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    // Startup Lisp can publish a setting before any display exists (the GUI
    // loads term/neo-win before its host). That reached no display, so the
    // display installed next receives nothing, as before the registry.
    eval.eval_str("(setq frame-alpha-lower-limit 30)")
        .expect("publish with no display");
    let host_a = RecordingDisplayHost::new();
    eval.set_display_host(Box::new(host_a.clone()));
    assert!(
        host_a.alpha_limits.borrow().is_empty(),
        "a publication no display received is not replayed"
    );

    eval.eval_str("(setq frame-alpha-lower-limit 40)")
        .expect("publish to A");
    assert_eq!(host_a.alpha_limits.borrow().as_slice(), &[0.4]);
    let host_b = RecordingDisplayHost::new();
    eval.register_display_terminal(
        TERMINAL_B,
        TerminalBacking::window_system(Box::new(host_b.clone())),
    );
    assert_eq!(
        host_b.alpha_limits.borrow().as_slice(),
        &[0.4],
        "a publication a display received replays on registration"
    );
}

#[derive(Clone, Default)]
struct DeviceLog(Rc<RefCell<Vec<&'static str>>>);

struct RecordingDevice(DeviceLog);

impl TerminalHost for RecordingDevice {
    fn suspend_tty(&mut self) -> Result<(), String> {
        self.0.0.borrow_mut().push("suspend");
        Ok(())
    }

    fn resume_tty(&mut self) -> Result<(), String> {
        self.0.0.borrow_mut().push("resume");
        Ok(())
    }
}

struct RecordingFactory(DeviceLog);

impl TtyFrameHostFactory for RecordingFactory {
    fn open_tty(&mut self, _request: TtyFrameOpenRequest) -> Result<OpenedTtyFrameHost, String> {
        Ok(OpenedTtyFrameHost::new(
            TtyFrameSize::new(80, 25).expect("non-zero test dimensions"),
            TtyAttributeCapabilities::full_with_color_cells(256),
            127,
            Box::new(RecordingDevice(self.0.clone())),
        ))
    }
}

#[test]
fn tty_host_is_owned_by_the_registry_not_the_carried_terminal_manager() {
    let mut eval = crate::test_utils::new_context();
    crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let log = DeviceLog::default();
    eval.set_tty_frame_host_factory(Box::new(RecordingFactory(log.clone())));
    let frame = eval
        .eval_str("(make-terminal-frame '((tty . \"/dev/pts/m3\") (tty-type . \"xterm\")))")
        .expect("open a text terminal");
    let terminal = eval
        .frame_manager()
        .get(crate::window::FrameId(
            frame.as_frame_id().expect("frame object"),
        ))
        .expect("opened frame")
        .terminal_id;
    let terminal = TerminalNumber::new(terminal);
    assert!(
        matches!(
            eval.display_terminals.backing(terminal),
            Some(TerminalBacking::Termcap(_))
        ),
        "the opened device belongs to the registry"
    );

    // Park the World-carried terminal state the way a Green switch does. The
    // native device must not travel with it (host-effect-projections.md:72-78):
    // main still drives it while the carried manager is away.
    let mut carried = world_state_placeholder();
    swap_world_state(&mut *carried);
    assert!(
        matches!(
            eval.display_terminals.backing(terminal),
            Some(TerminalBacking::Termcap(_))
        ),
        "the device stays with the main-owned registry"
    );
    device_effect(&mut eval, terminal, TerminalDeviceEffect::Resume);
    swap_world_state(&mut *carried);
    assert_eq!(*log.0.borrow(), vec!["resume"]);

    eval.set_variable("m3-tty-frame", frame);
    eval.eval_str("(suspend-tty (frame-terminal m3-tty-frame))")
        .expect("suspend the opened terminal");
    assert_eq!(*log.0.borrow(), vec!["resume", "suspend"]);
}

/// Run EFFECT on TERMINAL's device through the registry's one entry point.
fn device_effect(eval: &mut Context, terminal: TerminalNumber, effect: TerminalDeviceEffect) {
    match eval.tty_device_request(terminal, TerminalDeviceRequest::Effect(effect)) {
        Ok(TerminalDeviceReply::Done) => {}
        Ok(other) => panic!("the device did not run the effect: {other:?}"),
        Err(flow) => panic!("the device request signalled: {flow:?}"),
    }
}

/// The production `-nw` installer (`NM/main.rs`, `-nw` startup): the primary
/// TTY's device and one host that is both its popup half and the process
/// display services.
fn install_primary_tty(eval: &mut Context, device: DeviceLog, host: ProbeConnection) {
    eval.install_display_host(
        TerminalNumber::INITIAL,
        DisplayHostPlacement::Termcap(Box::new(RecordingDevice(device))),
        Box::new(host),
    );
}

#[test]
fn primary_tty_popup_half_is_owned_by_the_termcap_backing() {
    let mut eval = crate::test_utils::new_context();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);
    let popup = ProbeConnection::default();
    let log = DeviceLog::default();
    install_primary_tty(&mut eval, log.clone(), popup.clone());
    match eval.display_terminals.backing(TerminalNumber::INITIAL) {
        Some(TerminalBacking::Termcap(tty)) => {
            assert!(tty.display().is_some(), "the popup half is the TTY's")
        }
        _ => panic!("the primary TTY is a termcap backing"),
    }
    // The popup host is the process display: class 3 sees it, and the
    // services resolve through the TTY's display half.
    assert!(eval.display_terminals.hosts_display());
    assert!(
        eval.display_terminals.services().is_some(),
        "the process services are the TTY popup host's"
    );
    // The device half sits beside it.
    device_effect(
        &mut eval,
        TerminalNumber::INITIAL,
        TerminalDeviceEffect::Suspend,
    );
    assert_eq!(*log.0.borrow(), vec!["suspend"]);

    // A dialog takes its choice from the queued selection; a TTY popup menu
    // would wait for keyboard navigation instead.
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::MenuSelection {
            index: 1,
            token: None,
        },
    )
    .unwrap();
    let contents = Value::list(vec![
        Value::string("Confirm?"),
        Value::cons(Value::string("Yes"), Value::T),
        Value::cons(Value::string("No"), Value::symbol("declined")),
    ]);
    let chosen = super::builtin_x_popup_dialog(&mut eval, vec![Value::T, contents, Value::NIL])
        .expect("x-popup-dialog on the primary TTY");
    assert_eq!(chosen, Value::symbol("declined"));
    assert_eq!(popup.popups.borrow().len(), 1);
    assert_eq!(popup.popups.borrow()[0].frame_id, frame);

    // The halves have their own lifetimes: dropping the process display
    // leaves the TTY's device.
    eval.display_terminals.uninstall_display();
    assert!(!eval.display_terminals.hosts_display());
    match eval.display_terminals.backing(TerminalNumber::INITIAL) {
        Some(TerminalBacking::Termcap(tty)) => {
            assert!(
                tty.display().is_none(),
                "the popup half went with the display"
            )
        }
        _ => panic!("the primary TTY keeps its termcap backing"),
    }
    device_effect(
        &mut eval,
        TerminalNumber::INITIAL,
        TerminalDeviceEffect::Resume,
    );
    assert_eq!(*log.0.borrow(), vec!["suspend", "resume"]);
}

#[test]
fn class3_answers_are_unchanged() {
    // GUI: one window-system host provides both halves.
    let mut gui = crate::test_utils::new_context();
    crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut gui);
    gui.set_display_host(Box::new(RecordingDisplayHost::new()));
    // `-nw`, as production installs it: the primary TTY's popup host is the
    // termcap backing's display half and the process services.
    let mut tty = crate::test_utils::new_context();
    crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut tty);
    install_primary_tty(&mut tty, DeviceLog::default(), ProbeConnection::default());
    // The same shape from separate halves (`Context::install_display`).
    let mut tty_split = crate::test_utils::new_context();
    crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut tty_split);
    tty_split.install_display(
        Box::new(InertServices),
        TerminalNumber::INITIAL,
        TerminalBacking::Termcap(TtyHost::new(
            Box::new(RecordingDevice(DeviceLog::default())),
            Some(Box::new(ProbeConnection::default())),
        )),
    );
    // Batch and headless daemons: no display at all.
    let mut batch = crate::test_utils::new_context();
    crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut batch);

    // Before the registry each of these answered `display_host.is_some()`.
    for (name, eval, hosted) in [
        ("gui", &mut gui, true),
        ("tty", &mut tty, true),
        ("tty-split", &mut tty_split, true),
        ("batch", &mut batch, false),
    ] {
        assert_eq!(eval.display_terminals.hosts_display(), hosted, "{name}");
        assert_eq!(
            eval.eval_str("(neomacs-surface-available-p)").unwrap(),
            Value::bool_val(hosted),
            "{name}: neomacs-surface-available-p"
        );
    }
}
