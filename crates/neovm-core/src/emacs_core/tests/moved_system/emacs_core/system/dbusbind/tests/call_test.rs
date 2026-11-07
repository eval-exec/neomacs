//! `dbus-call-method` through `read-event` / `dbus-event`.
//!
//! Skipped when the session bus is not available, same as `connection.rs`.

use neovm_eval::emacs_core::eval::Context;

fn load_dbus(eval: &mut Context) -> bool {
    match eval
        .eval_str("(progn (require 'dbus) (dbus-ignore-errors (dbus-get-unique-name :session)))")
    {
        Ok(name) if name.is_string() => true,
        Ok(_) => false,
        Err(err) => panic!("(require 'dbus) failed: {err:?}"),
    }
}

#[test]
fn session_bus_get_id_returns_through_read_event() {
    // Hermetic bus: this test needs a live session daemon, and the ambient
    // session bus is shared with every other process of the user (and can be
    // absent). Bring a private daemon and point both ambient buses at it; a
    // missing dbus-daemon fails here with the spawn error instead of being
    // skipped.
    let bus = crate::emacs_core::moved_system::private_bus::PrivateBus::start("get-id");
    let _bus_env = bus.install_env();
    crate::test_utils::init_test_tracing();
    crate::emacs_core::dbusbind::reset_thread_locals();
    let mut eval = crate::test_utils::runtime_startup_context();
    assert!(
        load_dbus(&mut eval),
        "the private session bus must accept the dbus client (no silent skip)"
    );

    let id = eval
        .eval_str(
            r#"(dbus-call-method
                :session "org.freedesktop.DBus" "/org/freedesktop/DBus"
                "org.freedesktop.DBus" "GetId" :timeout 2000)"#,
        )
        .unwrap_or_else(|err| {
            panic!("GetId through dbus-event failed (event loop or type mapping): {err:?}")
        });
    let text = id.as_utf8_str().unwrap_or("");
    assert!(
        !text.is_empty(),
        "GetId should return a non-empty machine-id string, got {id:?}"
    );

    crate::emacs_core::dbusbind::reset_thread_locals();
}
