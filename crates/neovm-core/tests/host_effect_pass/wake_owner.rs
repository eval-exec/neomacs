#[path = "../host_effect_ui/support/wake_owner.rs"]
mod owner;

use owner::wake::{ControlDestination, ControlWake, NativePollOwner};

// A missing source-import dependency must fail this positive control, rather
// than masquerading as a privacy or linear-ownership compiler rejection.
fn main() {
    let (wake, publisher) = ControlWake::paired();
    let session = wake.attach_host_session();
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    let lease = wake.try_native_poll(NativePollOwner::NativeMain).unwrap();
    drop(lease);
    drop(registration);
    drop(session);
    drop(publisher);
}
