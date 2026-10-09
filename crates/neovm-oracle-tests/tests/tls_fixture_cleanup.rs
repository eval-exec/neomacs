//! Loopback fixtures must cancel even if the editor never connects.
#[path = "../../../test/support/tls_loopback.rs"]
mod tls_loopback;
use std::time::{Duration, Instant};

#[test]
fn stalled_peer_cleanup_cancels_an_unaccepted_listener() {
    let peer = tls_loopback::StalledPeer::spawn();
    let started = Instant::now();
    drop(peer);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "cleanup waited for the two-second accept deadline instead of cancellation"
    );
}
