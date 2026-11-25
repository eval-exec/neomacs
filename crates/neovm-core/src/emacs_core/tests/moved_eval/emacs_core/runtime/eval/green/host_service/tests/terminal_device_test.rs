//! A Green worker reaches the text-terminal devices main owns (remote-display
//! M3, fix round 1).
//!
//! M3 moved every terminal's native device out of the World-carried terminal
//! manager into main's display registry. A worker owner's registry is empty,
//! so its `send-string-to-terminal`, `suspend-tty`, `resume-tty`,
//! `delete-terminal` and the winsize query must cross to main, which performs
//! them on its own OS thread. GNU runs these primitives on whichever Lisp
//! thread calls them (src/term.c `Fsuspend_tty`, `Fresume_tty`;
//! src/dispnew.c `Fsend_string_to_terminal`; src/terminal.c
//! `Fdelete_terminal`; a360712c, UNATTESTED). `make-terminal-frame` and the
//! terminal primitives live in the display crate, so this test cannot stay in
//! eval.

use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

use neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities;

use crate::emacs_core::eval::TerminalNumber;
use crate::emacs_core::terminal::pure::{
    OpenedTtyFrameHost, TerminalHost, TtyFrameHostFactory, TtyFrameOpenRequest, TtyFrameSize,
};

/// The winsize the device reports, distinct from the size it opened with.
const DEVICE_WINSIZE: (u32, u32) = (100, 40);

/// Every effect that reached the device, with the OS thread it ran on.
#[derive(Clone, Default)]
struct DeviceLog {
    effects: Arc<Mutex<Vec<(&'static str, ThreadId)>>>,
    output: Arc<Mutex<Vec<u8>>>,
}

impl DeviceLog {
    fn record(&self, effect: &'static str) {
        self.effects
            .lock()
            .unwrap()
            .push((effect, std::thread::current().id()));
    }

    fn effects(&self) -> Vec<&'static str> {
        self.effects
            .lock()
            .unwrap()
            .iter()
            .map(|(effect, _)| *effect)
            .collect()
    }

    fn threads(&self) -> Vec<ThreadId> {
        self.effects
            .lock()
            .unwrap()
            .iter()
            .map(|(_, thread)| *thread)
            .collect()
    }
}

struct RecordingDevice(DeviceLog);

impl TerminalHost for RecordingDevice {
    fn suspend_tty(&mut self) -> Result<(), String> {
        self.0.record("suspend");
        Ok(())
    }

    fn resume_tty(&mut self) -> Result<(), String> {
        self.0.record("resume");
        Ok(())
    }

    fn delete_terminal(&mut self) -> Result<(), String> {
        self.0.record("delete");
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.0.record("write");
        self.0.output.lock().unwrap().extend_from_slice(bytes);
        Ok(())
    }

    fn tty_winsize_cells(&mut self) -> Option<(u32, u32)> {
        self.0.record("winsize");
        Some(DEVICE_WINSIZE)
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
fn green_worker_tty_device_effects_reach_the_device_main_owns() {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut context = crate::test_utils::new_context();
    context.set_green_threading_mode_at_construction_for_test();
    crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut context);
    let log = DeviceLog::default();
    context.set_tty_frame_host_factory(Box::new(RecordingFactory(log.clone())));

    // Main opens the terminal: its device joins main's display registry.
    let frame = context
        .eval_str("(make-terminal-frame '((tty . \"/dev/pts/m3\") (tty-type . \"xterm\")))")
        .expect("open a text terminal on main");
    let terminal = TerminalNumber::new(
        context
            .frame_manager()
            .get(crate::window::FrameId(
                frame.as_frame_id().expect("frame object"),
            ))
            .expect("opened frame")
            .terminal_id,
    );
    context.set_variable("m3-tty-frame", frame);
    assert!(
        context.display_terminals.backing(terminal).is_some(),
        "the opened device belongs to main's registry"
    );

    // Each effect runs in its own Lisp thread, a Green worker; main waits in
    // `thread-join`. A second frame on the open device sizes itself from the
    // device's winsize (GNU `get_tty_size`, src/frame.c:1850-1863). A signal
    // in a worker becomes that worker's result. The form is lexical, so each
    // thread function closes over its effect and the terminal (a worker does
    // not see main's dynamic bindings).
    let results = context
        .eval_str(
            r#"(eval
                '(let ((term (frame-terminal m3-tty-frame)))
                   (format "%S"
                    (mapcar
                     (lambda (effect)
                       (thread-join
                        (make-thread
                         (lambda ()
                           (condition-case err (funcall effect) (error err))))))
                     (list
                      (lambda ()
                        (frame-parameter
                         (make-terminal-frame
                          '((tty . "/dev/pts/m3") (tty-type . "xterm")))
                         'width))
                      (lambda () (send-string-to-terminal "from-worker" term) 'written)
                      (lambda () (suspend-tty term) 'suspended)
                      (lambda () (resume-tty term) 'resumed)
                      (lambda () (delete-terminal term) 'deleted)
                      (lambda () (terminal-live-p term))))))
                t)"#,
        )
        .expect("worker device effects");

    assert_eq!(
        results.as_runtime_string_owned().as_deref(),
        Some(
            format!(
                "({} written suspended resumed deleted nil)",
                DEVICE_WINSIZE.0
            )
            .as_str()
        ),
        "every worker effect completed as on main"
    );
    assert_eq!(
        log.effects(),
        vec!["winsize", "write", "suspend", "resume", "delete"],
        "each worker effect reached the device"
    );
    assert_eq!(*log.output.lock().unwrap(), b"from-worker".to_vec());
    let main_thread = std::thread::current().id();
    assert!(
        log.threads().iter().all(|thread| *thread == main_thread),
        "the device ran only on main's OS thread"
    );
    assert!(
        context.display_terminals.backing(terminal).is_none(),
        "the worker's delete-terminal released the device from main's registry"
    );
}
