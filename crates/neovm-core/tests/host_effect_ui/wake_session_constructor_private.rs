#[path = "support/wake_owner.rs"]
mod owner;

use owner::wake::{ControlWake, HostSessionControl};
use std::sync::Weak;

// Even a holder of a legitimate session cannot reconstruct it over another
// backend. Both the weak backend and exact attachment identity stay private.
fn forge_session(owner: HostSessionControl, wake: Weak<ControlWake>) -> HostSessionControl {
    HostSessionControl { wake, ..owner }
}

fn main() {
    let _ = forge_session;
}
