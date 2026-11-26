//! `PollableResource::{modify_on_poller, unregister_from_poller}` are `unsafe fn`,
//! so safe external code cannot invoke them directly or replay a token across
//! objects or after drop.
use neovm_eval::emacs_core::eval::data::process_state::PollableResource;

fn call_plumbing(pty: &Box<dyn portable_pty::MasterPty + Send>, poller: &polling::Poller) {
    let token = pty.capture_registration_token();
    let _ = pty.modify_on_poller(
        poller,
        token,
        polling::Event::readable(1),
        polling::PollMode::Level,
    );
    pty.unregister_from_poller(poller, token);
}

fn main() {
    let _ = call_plumbing;
}
