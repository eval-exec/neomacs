//! Connection watches — GNU `xd_add_watch` / `xd_read_queued_messages`.

use std::time::Duration;

use dbus::channel::Channel;

use crate::emacs_core::error::Flow;

/// Read whatever the connection has queued, without blocking.
///
/// GNU calls `dbus_connection_read_write (connection, 0)` and **ignores its
/// result** (`src/dbusbind.c:1949`); its read callback also swallows every
/// Lisp error raised during the read (`xd_read_queued_messages`,
/// `src/dbusbind.c:1975-1982`). libdbus returns FALSE only when the connection
/// is closed — a zero timeout with no data returns TRUE
/// (`dbus_connection_read_write`'s contract in `dbus-connection.c`) — so a bus
/// that went away must not become a `dbus-error` here. The `Disconnected`
/// signal handler in `lisp/net/dbus.el` owns the reporting.
///
/// Returning `Err` on that FALSE aborted the whole multi-bus pump from
/// `read-event`, so a dead bus (or one the host daemon disconnected under
/// load) made an unrelated registration look like it had errored.
pub(super) fn pump(channel: &Channel) -> Result<(), Flow> {
    let _ = channel.read_write(Some(Duration::ZERO));
    Ok(())
}
