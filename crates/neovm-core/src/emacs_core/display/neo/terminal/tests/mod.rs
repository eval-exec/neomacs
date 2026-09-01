use crate::emacs_core::display_host::{
    DisplayHost, TerminalCreateRequest, TerminalDisplayTarget, TerminalFloatPlacement,
    TerminalGridSize, TerminalId,
};
use crate::emacs_core::eval::{Context, GuiFrameHostRequest};
use crate::emacs_core::value::{Value, list_to_vec};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
enum TerminalHostEvent {
    Create(TerminalCreateRequest),
    Write {
        id: TerminalId,
        data: Vec<u8>,
    },
    Resize {
        id: TerminalId,
        size: TerminalGridSize,
    },
    Float {
        id: TerminalId,
        placement: TerminalFloatPlacement,
    },
    Destroy {
        id: TerminalId,
    },
}

#[derive(Clone, Default)]
struct RecordingTerminalDisplayHost {
    events: Arc<Mutex<Vec<TerminalHostEvent>>>,
}

impl DisplayHost for RecordingTerminalDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn create_terminal(&self, request: TerminalCreateRequest) -> Result<TerminalId, String> {
        self.events
            .lock()
            .expect("terminal host events")
            .push(TerminalHostEvent::Create(request));
        Ok(TerminalId::new(41).expect("nonzero terminal id"))
    }

    fn write_terminal(&self, id: TerminalId, data: Vec<u8>) -> Result<(), String> {
        self.events
            .lock()
            .expect("terminal host events")
            .push(TerminalHostEvent::Write { id, data });
        Ok(())
    }

    fn resize_terminal(&self, id: TerminalId, size: TerminalGridSize) -> Result<(), String> {
        self.events
            .lock()
            .expect("terminal host events")
            .push(TerminalHostEvent::Resize { id, size });
        Ok(())
    }

    fn set_floating_terminal(
        &self,
        id: TerminalId,
        placement: TerminalFloatPlacement,
    ) -> Result<(), String> {
        self.events
            .lock()
            .expect("terminal host events")
            .push(TerminalHostEvent::Float { id, placement });
        Ok(())
    }

    fn destroy_terminal(&self, id: TerminalId) -> Result<(), String> {
        self.events
            .lock()
            .expect("terminal host events")
            .push(TerminalHostEvent::Destroy { id });
        Ok(())
    }

    fn terminal_text(&self, id: TerminalId) -> Result<Option<String>, String> {
        if id.get() == 41 {
            Ok(Some("ready\n$".to_owned()))
        } else {
            Err(format!("unknown test terminal id {id}"))
        }
    }
}

#[test]
fn public_terminal_builtins_route_typed_requests_through_the_display_host() {
    crate::test_utils::init_test_tracing();
    let host = RecordingTerminalDisplayHost::default();
    let mut eval = Context::new();
    eval.set_display_host(Box::new(host.clone()));

    let result = eval
        .eval_str(
            r#"
(list
 (mapcar #'fboundp
         '(neomacs-terminal-create
           neomacs-terminal-write
           neomacs-terminal-resize
           neomacs-terminal-destroy
           neomacs-terminal-set-float
           neomacs-terminal-get-text))
 (let ((id (neomacs-terminal-create 80 24 2 "/bin/sh")))
   (list id
         (neomacs-terminal-write id "echo ready\r")
         (neomacs-terminal-resize id 120 40)
         (neomacs-terminal-set-float id 10.5 20 0.85)
         (neomacs-terminal-get-text id)
         (neomacs-terminal-destroy id)
         (condition-case err
             (neomacs-terminal-get-text 999)
           (error (car err))))))
"#,
        )
        .expect("terminal public workflow should evaluate");

    let outer = list_to_vec(&result).expect("outer result list");
    assert_eq!(
        list_to_vec(&outer[0]).expect("fboundp result list"),
        vec![Value::T; 6]
    );
    let values = list_to_vec(&outer[1]).expect("workflow result list");
    assert_eq!(values[0], Value::fixnum(41));
    assert_eq!(&values[1..4], &[Value::T, Value::T, Value::T]);
    assert_eq!(values[4].as_utf8_str(), Some("ready\n$"));
    assert_eq!(values[5], Value::T);
    assert_eq!(values[6], Value::symbol("error"));

    assert_eq!(
        *host.events.lock().expect("terminal host events"),
        vec![
            TerminalHostEvent::Create(TerminalCreateRequest {
                size: TerminalGridSize {
                    cols: std::num::NonZeroU16::new(80).unwrap(),
                    rows: std::num::NonZeroU16::new(24).unwrap(),
                },
                target: TerminalDisplayTarget::Floating,
                shell: Some("/bin/sh".to_owned()),
            }),
            TerminalHostEvent::Write {
                id: TerminalId::new(41).unwrap(),
                data: b"echo ready\r".to_vec(),
            },
            TerminalHostEvent::Resize {
                id: TerminalId::new(41).unwrap(),
                size: TerminalGridSize {
                    cols: std::num::NonZeroU16::new(120).unwrap(),
                    rows: std::num::NonZeroU16::new(40).unwrap(),
                },
            },
            TerminalHostEvent::Float {
                id: TerminalId::new(41).unwrap(),
                placement: TerminalFloatPlacement::new(10.5, 20.0, 0.85).unwrap(),
            },
            TerminalHostEvent::Destroy {
                id: TerminalId::new(41).unwrap(),
            },
        ]
    );
}

#[test]
fn window_terminal_target_carries_the_current_buffer_identity() {
    let host = RecordingTerminalDisplayHost::default();
    let mut eval = Context::new();
    let owner = eval.buffers.current_buffer_id().expect("current buffer");
    eval.set_display_host(Box::new(host.clone()));

    eval.eval_str("(neomacs-terminal-create 80 24 0 \"/bin/sh\")")
        .expect("create window terminal");

    let events = host.events.lock().expect("terminal host events");
    let TerminalHostEvent::Create(request) = &events[0] else {
        panic!("expected terminal create event");
    };
    assert_eq!(
        request.target,
        TerminalDisplayTarget::Window { buffer: owner }
    );
}

#[test]
fn invalid_lisp_terminal_values_never_reach_the_display_host() {
    crate::test_utils::init_test_tracing();
    let host = RecordingTerminalDisplayHost::default();
    let mut eval = Context::new();
    eval.set_display_host(Box::new(host.clone()));

    for form in [
        "(neomacs-terminal-create 0 24 0)",
        "(neomacs-terminal-create 80 65536 0)",
        "(neomacs-terminal-create 80 24 3)",
        "(neomacs-terminal-create 80 24 0 7)",
        "(neomacs-terminal-write 0 \"data\")",
        "(neomacs-terminal-resize 41 0 24)",
        "(neomacs-terminal-set-float 41 0 0 -0.1)",
        "(neomacs-terminal-set-float 41 0 0 1.1)",
    ] {
        let guarded = format!("(condition-case err {form} (error (car err)))");
        let result = eval.eval_str(&guarded).expect("condition-case result");
        assert_ne!(result, Value::symbol("unexpected"), "form: {form}");
    }

    assert!(host.events.lock().expect("terminal host events").is_empty());
    assert!(TerminalGridSize::new(0, 24).is_none());
    assert!(TerminalGridSize::new(80, 0).is_none());
    assert!(TerminalFloatPlacement::new(f32::NAN, 0.0, 1.0).is_none());
    assert!(TerminalFloatPlacement::new(0.0, f32::INFINITY, 1.0).is_none());
    assert!(TerminalFloatPlacement::new(0.0, 0.0, -0.1).is_none());
    assert!(TerminalFloatPlacement::new(0.0, 0.0, 1.1).is_none());

    let mut no_host = Context::new();
    let result = no_host
        .eval_str("(condition-case err (neomacs-terminal-create 80 24 0) (error (car err)))")
        .expect("missing-host condition-case result");
    assert_eq!(result, Value::symbol("error"));
}
