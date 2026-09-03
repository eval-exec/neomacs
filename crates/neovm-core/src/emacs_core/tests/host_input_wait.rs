use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crossbeam_channel::Sender;

use crate::emacs_core::wait::{HostInputWaitBackend, HostInputWaitError};
use crate::emacs_core::{Context, format_eval_result};
use crate::keyboard::{InputEvent, KeyEvent};

struct OneShotHostInputWait {
    input: Sender<InputEvent>,
    calls: Arc<AtomicUsize>,
}

impl HostInputWaitBackend for OneShotHostInputWait {
    fn wait_for_input(&mut self, timeout: Duration) -> Result<(), HostInputWaitError> {
        assert!(!timeout.is_zero());
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.input
            .send(InputEvent::key_press(KeyEvent::char('x')))
            .map_err(|_| HostInputWaitError::new("editor input receiver disconnected"))
    }
}

#[test]
fn read_char_uses_an_installed_host_wait_backend_without_replacing_editor_input_semantics() {
    let mut evaluator = Context::new();
    let (input, receiver) = crossbeam_channel::unbounded();
    let calls = Arc::new(AtomicUsize::new(0));
    evaluator.init_input_system(receiver);
    evaluator.install_host_input_wait_backend(OneShotHostInputWait {
        input,
        calls: Arc::clone(&calls),
    });

    let result = evaluator.eval_str("(read-char)");

    assert_eq!(format_eval_result(&result), "OK 120");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}
