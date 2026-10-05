//! CLIPBOARD reads through the `ext-data-control-v1` protocol.
//!
//! Neomacs has two `wl_data_device` objects on its single Wayland
//! connection: winit's (drag and drop) and smithay-clipboard's.  Wayland
//! lets a client create several devices, and wlroots sends the selection to
//! all of the focused client's devices.  Hyprland sends it only to the first
//! device the client created, which is winit's, so smithay-clipboard's device
//! never receives a selection offer and every read reports an empty
//! clipboard, even while Neomacs has keyboard focus.
//!
//! A data-control device is not a `wl_data_device` and is not tied to
//! keyboard focus, so it observes the selection regardless of how the
//! compositor routes data-device offers.  The reader is used only when
//! smithay-clipboard reports no offer; it creates one short-lived device per
//! read, takes the selection the compositor sends on creation, transfers it
//! and destroys every object it was given.

use std::ffi::c_void;
use std::io::{ErrorKind, Read};
use std::os::fd::AsFd;
use std::ptr::NonNull;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use wayland_backend::client::{Backend, WaylandError};
use wayland_client::globals::{BindError, GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop, event_created_child,
};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1, ext_data_control_manager_v1, ext_data_control_offer_v1,
};

/// Upper bound for one transfer.  It stays below the GUI host's reply wait so
/// a stalled selection owner fails this read instead of the next request.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(3);

/// Text MIME types in smithay-clipboard's preference order, so both read
/// paths choose the same representation of a selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextMime {
    TextPlainUtf8,
    Utf8String,
    TextPlain,
}

impl TextMime {
    fn as_str(self) -> &'static str {
        match self {
            Self::TextPlainUtf8 => "text/plain;charset=utf-8",
            Self::Utf8String => "UTF8_STRING",
            Self::TextPlain => "text/plain",
        }
    }

    /// The first UTF-8 type offered, else plain text as a fallback.
    pub(super) fn choose(offered: &[String]) -> Option<Self> {
        let mut fallback = None;
        for mime in offered {
            match mime.as_str() {
                "text/plain;charset=utf-8" => return Some(Self::TextPlainUtf8),
                "UTF8_STRING" => return Some(Self::Utf8String),
                "text/plain" => fallback = Some(Self::TextPlain),
                _ => {}
            }
        }
        fallback
    }

    /// Decode transferred bytes the way smithay-clipboard does: lossy UTF-8,
    /// and CR/CRLF line ends normalized to LF for the `text/plain` types.
    pub(super) fn decode(self, bytes: &[u8]) -> String {
        let text = String::from_utf8_lossy(bytes).into_owned();
        match self {
            Self::TextPlainUtf8 | Self::TextPlain => text.replace("\r\n", "\n").replace('\r', "\n"),
            Self::Utf8String => text,
        }
    }
}

#[derive(Default)]
struct ReaderState {
    /// Every offer introduced while the per-read device existed.
    offers: Vec<ext_data_control_offer_v1::ExtDataControlOfferV1>,
    selection: Option<ext_data_control_offer_v1::ExtDataControlOfferV1>,
}

#[derive(Default)]
struct OfferMimes(Mutex<Vec<String>>);

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ReaderState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(ReaderState: ignore wl_seat::WlSeat);
delegate_noop!(ReaderState: ignore ext_data_control_manager_v1::ExtDataControlManagerV1);

impl Dispatch<ext_data_control_device_v1::ExtDataControlDeviceV1, ()> for ReaderState {
    fn event(
        state: &mut Self,
        _: &ext_data_control_device_v1::ExtDataControlDeviceV1,
        event: ext_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_device_v1::Event::DataOffer { id } => state.offers.push(id),
            ext_data_control_device_v1::Event::Selection { id } => state.selection = id,
            // PRIMARY offers are collected in `offers` and destroyed with
            // them; `finished` leaves no selection, which reads as empty.
            _ => {}
        }
    }

    event_created_child!(ReaderState, ext_data_control_device_v1::ExtDataControlDeviceV1, [
        ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE =>
            (ext_data_control_offer_v1::ExtDataControlOfferV1, OfferMimes::default()),
    ]);
}

impl Dispatch<ext_data_control_offer_v1::ExtDataControlOfferV1, OfferMimes> for ReaderState {
    fn event(
        _: &mut Self,
        _: &ext_data_control_offer_v1::ExtDataControlOfferV1,
        event: ext_data_control_offer_v1::Event,
        mimes: &OfferMimes,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_data_control_offer_v1::Event::Offer { mime_type } = event {
            mimes
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(mime_type);
        }
    }
}

/// A private event queue on the shared display, bound to data-control.
pub(super) struct DataControlReader {
    connection: Connection,
    queue: EventQueue<ReaderState>,
    manager: ext_data_control_manager_v1::ExtDataControlManagerV1,
    seat: wl_seat::WlSeat,
}

impl DataControlReader {
    /// Bind data-control on `display`, or `Ok(None)` when the compositor
    /// does not offer it (or offers no seat).
    ///
    /// # Safety
    ///
    /// `display` must be a live `wl_display` that outlives the reader.
    pub(super) unsafe fn connect(display: NonNull<c_void>) -> Result<Option<Self>, String> {
        // SAFETY: guaranteed by the caller.  A foreign backend never
        // disconnects the display it borrows.
        let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<ReaderState>(&connection)
            .map_err(|err| format!("data-control registry: {err}"))?;
        let handle = queue.handle();
        let manager: ext_data_control_manager_v1::ExtDataControlManagerV1 =
            match globals.bind(&handle, 1..=1, ()) {
                Ok(manager) => manager,
                Err(BindError::NotPresent | BindError::UnsupportedVersion) => return Ok(None),
            };
        // Read the first advertised seat.  Desktop compositors expose one;
        // on a multi-seat wlroots setup this may differ from the seat
        // smithay-clipboard last saw active.
        let seat = globals.contents().with_list(|list| {
            list.iter()
                .find(|global| global.interface == wl_seat::WlSeat::interface().name)
                .map(|global| global.name)
        });
        let Some(seat) = seat else {
            manager.destroy();
            return Ok(None);
        };
        let seat = globals
            .registry()
            .bind::<wl_seat::WlSeat, _, _>(seat, 1, &handle, ());
        Ok(Some(Self {
            connection,
            queue,
            manager,
            seat,
        }))
    }

    /// The current CLIPBOARD selection as text: `Ok(None)` when there is no
    /// selection or it offers no text type.
    pub(super) fn read_text(&mut self) -> Result<Option<String>, String> {
        let mut state = ReaderState::default();
        let device = self
            .manager
            .get_data_device(&self.seat, &self.queue.handle(), ());
        // The compositor sends the current selection when the device binds.
        let result = self
            .queue
            .roundtrip(&mut state)
            .map_err(|err| format!("data-control roundtrip: {err}"))
            .and_then(|_| self.transfer(state.selection.as_ref()));
        // Collect offers announced during the transfer so they are destroyed
        // too; anything still in flight is ignored once the device is gone.
        let _ = self.queue.dispatch_pending(&mut state);
        for offer in state.offers.drain(..) {
            offer.destroy();
        }
        device.destroy();
        let _ = self.connection.flush();
        result
    }

    fn transfer(
        &self,
        selection: Option<&ext_data_control_offer_v1::ExtDataControlOfferV1>,
    ) -> Result<Option<String>, String> {
        let Some(offer) = selection else {
            return Ok(None);
        };
        let mime = {
            let mimes = offer
                .data::<OfferMimes>()
                .map(|mimes| mimes.0.lock().unwrap_or_else(|p| p.into_inner()).clone())
                .unwrap_or_default();
            TextMime::choose(&mimes)
        };
        let Some(mime) = mime else {
            return Ok(None);
        };
        let (mut reader, writer) =
            std::io::pipe().map_err(|err| format!("data-control pipe: {err}"))?;
        offer.receive(mime.as_str().to_owned(), writer.as_fd());
        // Only the selection owner may hold the write end, or EOF never comes.
        drop(writer);
        match self.connection.flush() {
            // A full socket sends the request later; the deadline still holds.
            Ok(()) => {}
            Err(WaylandError::Io(err)) if err.kind() == ErrorKind::WouldBlock => {}
            Err(err) => return Err(format!("data-control flush: {err}")),
        }
        let bytes = read_to_end_before(&mut reader, Instant::now() + TRANSFER_TIMEOUT)?;
        Ok(Some(mime.decode(&bytes)))
    }
}

impl Drop for DataControlReader {
    fn drop(&mut self) {
        self.manager.destroy();
        let _ = self.connection.flush();
    }
}

fn read_to_end_before(
    reader: &mut std::io::PipeReader,
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("data-control transfer timed out".to_owned());
        }
        let timeout = Timespec::try_from(remaining).map_err(|err| err.to_string())?;
        let mut fds = [PollFd::new(reader, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => continue,
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(err) => return Err(format!("data-control poll: {err}")),
        }
        match reader.read(&mut chunk) {
            Ok(0) => return Ok(bytes),
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => return Err(format!("data-control read: {err}")),
        }
    }
}

#[cfg(test)]
#[path = "tests/wayland_data_control_test.rs"]
mod tests;
