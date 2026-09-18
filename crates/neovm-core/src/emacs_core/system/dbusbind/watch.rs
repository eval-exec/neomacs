//! Connection watches — GNU `xd_add_watch` / `xd_read_queued_messages`.

use std::time::Duration;

use dbus::channel::Channel;

use crate::emacs_core::error::Flow;

use super::connection::dbus_error;

pub(super) fn pump(channel: &Channel) -> Result<(), Flow> {
    channel
        .read_write(Some(Duration::ZERO))
        .map_err(|()| dbus_error("Cannot read D-Bus connection"))
}
