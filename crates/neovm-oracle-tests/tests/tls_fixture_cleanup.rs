//! Loopback fixtures must cancel even if the editor never connects.
#[path = "../../../test/support/tls_loopback.rs"]
mod tls_loopback;
use std::{
    io::Read,
    net::TcpStream,
    thread,
    time::{Duration, Instant},
};

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

/// A loaded machine can delay the Lisp client by seconds.  The listener must
/// outlive any fixed accept deadline, and a held connection must stay open
/// until the owner cancels it, or the client sees ECONNREFUSED / EOF.
#[test]
fn stalled_peer_accepts_a_late_client_and_holds_it_open() {
    let peer = tls_loopback::StalledPeer::spawn();
    thread::sleep(Duration::from_millis(2500));
    let mut client = TcpStream::connect(("127.0.0.1", peer.port))
        .expect("listener must still be accepting after a long client delay");
    thread::sleep(Duration::from_millis(1500));
    client
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let mut byte = [0; 1];
    match client.read(&mut byte) {
        Ok(0) => panic!("stalled peer closed the connection before cancellation"),
        Ok(_) => panic!("stalled peer must not send data"),
        Err(error) => assert!(matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        )),
    }
    drop(peer);
}
