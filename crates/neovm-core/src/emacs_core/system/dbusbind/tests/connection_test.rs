//! Live session-bus checks. Skipped when the bus is not available.

use crate::emacs_core::error::{FlowKind, FlowResultExt};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[test]
fn session_bus_unique_name_has_gnu_shape() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let refs = match super::super::connection::init_bus(
        &mut ctx,
        vec![Value::keyword_by_name(":session")],
    )
    .kinded()
    {
        Ok(value) => value,
        Err(FlowKind::Signal(signal)) if signal.symbol_name() == "dbus-error" => return,
        Err(err) => panic!("unexpected init-bus failure: {err:?}"),
    };
    assert_eq!(refs, Value::fixnum(1));
    let name = super::super::connection::get_unique_name(vec![Value::keyword_by_name(":session")])
        .expect("unique name");
    let text = name.as_utf8_str().expect("unique name string");
    assert!(
        text.starts_with(':') && text.contains('.'),
        "GNU unique names look like :1.54, got {text}"
    );
}

/// A connection that went away is not a Lisp error in the read path.
///
/// GNU calls `dbus_connection_read_write (connection, 0)` and ignores the
/// result (`src/dbusbind.c:1949`); `xd_read_queued_messages` also swallows
/// every Lisp error raised while reading (`src/dbusbind.c:1975-1982`).
/// libdbus returns FALSE only when the connection is closed — a zero timeout
/// with no data returns TRUE — so a dead bus must leave the pump successful
/// and let `lisp/net/dbus.el`'s `Disconnected` handler own the reporting.
///
/// This is deterministic: the test starts its own daemon, connects, kills the
/// daemon, and drives the read that observes the EOF. The old pump turned that
/// EOF into `(dbus-error "Cannot read D-Bus connection")` and aborted the whole
/// multi-bus pump from `read-event`.
#[test]
fn closed_connection_is_not_a_pump_error() {
    crate::test_utils::init_test_tracing();
    let mut bus = super::private_bus::PrivateBus::start("closed-pump");
    let mut channel = dbus::channel::Channel::open_private(&bus.address)
        .expect("open the private bus");
    channel
        .register()
        .expect("register with the private bus");
    bus.kill();

    // Drive reads until libdbus observes the closed socket. No sleeps: each
    // iteration performs the read that detects the EOF.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut outcome = Ok(());
    loop {
        outcome = super::super::watch::pump(&channel);
        if outcome.is_err() || !channel.is_connected() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "killing the private daemon should close its connection"
        );
        std::thread::yield_now();
    }
    assert!(
        outcome.is_ok(),
        "a closed D-Bus connection must not signal from the read path: {outcome:?}"
    );
    assert!(
        !channel.is_connected(),
        "the killed daemon should leave the connection closed"
    );
}
