use neomacs_display_protocol::frame_time::{EventTime, observe_platform_now};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::{collections::VecDeque, fs, path::PathBuf, time::Duration};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::wp::presentation_time::client::{wp_presentation, wp_presentation_feedback};
use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

#[derive(Clone, Copy, Debug)]
struct Submission {
    serial: u64,
    frame: u64,
    layout: neomacs_display_protocol::PresentationId,
    width: u32,
    height: u32,
    scale: f64,
}

#[derive(Clone, Copy, Debug)]
struct CompositorTimestamp {
    // Deliberately not EventTime: this clock belongs to the compositor and
    // must not be substituted for a scheduler observation or predicted tick.
    clock_id: u32,
    seconds: u64,
    nanoseconds: u32,
}

#[derive(Clone, Copy, Debug)]
struct ConfirmedPresentation {
    submission: Submission,
    timestamp: CompositorTimestamp,
}

#[derive(Clone, Copy, Debug)]
enum NativeFeedback {
    Presented(ConfirmedPresentation),
    Discarded(Submission),
}

#[derive(Default)]
struct PendingReceipts(VecDeque<(u64, EventTime)>);
impl PendingReceipts {
    fn requested(&mut self, serial: u64, now: EventTime) {
        if self.0.len() == 256 {
            self.0.pop_front();
        }
        self.0.push_back((serial, now));
    }
    fn received(&mut self, serial: u64) {
        self.0.retain(|&(id, _)| id != serial);
    }
    fn deadline(&mut self, now: EventTime) -> Option<EventTime> {
        // Missing feedback from an occluded/destroyed surface must not keep
        // diagnostics waking forever. This never asks for another frame.
        self.0
            .retain(|&(_, requested)| now.saturating_since(requested) < Duration::from_secs(1));
        (!self.0.is_empty()).then(|| now.plus(Duration::from_millis(4)))
    }
}

struct Receipts {
    path: PathBuf,
    latest_presented: u64,
    clock_id: Option<u32>,
    pending: PendingReceipts,
}

impl Receipts {
    fn observe(&mut self, feedback: NativeFeedback) {
        let serial = match feedback {
            NativeFeedback::Presented(confirmed) => confirmed.submission.serial,
            NativeFeedback::Discarded(submission) => submission.serial,
        };
        self.pending.received(serial);
        let confirmed = match feedback {
            NativeFeedback::Presented(confirmed) => confirmed,
            NativeFeedback::Discarded(submission) => {
                tracing::debug!(?submission, "native presentation discarded");
                return;
            }
        };
        self.publish(confirmed);
    }

    // Raw submissions cannot cross this interface. Only the native Presented
    // event constructs a confirmation with the compositor's timestamp.
    fn publish(&mut self, confirmed: ConfirmedPresentation) {
        tracing::debug!(?confirmed, "native presentation confirmed");
        let ConfirmedPresentation {
            submission,
            timestamp,
        } = confirmed;
        if let Some(nanoseconds) = timestamp
            .seconds
            .checked_mul(1_000_000_000)
            .and_then(|seconds| seconds.checked_add(u64::from(timestamp.nanoseconds)))
        {
            neomacs_display_protocol::input_latency::projected_confirmed(
                submission.frame,
                submission.serial,
                neomacs_display_protocol::input_latency::PlatformTimestamp {
                    clock_id: timestamp.clock_id,
                    nanoseconds,
                },
            );
            neomacs_display_protocol::input_latency::confirmed(
                submission.layout,
                neomacs_display_protocol::input_latency::PlatformTimestamp {
                    clock_id: timestamp.clock_id,
                    nanoseconds,
                },
            );
        }
        if submission.serial <= self.latest_presented {
            return;
        }
        // Atomic replacement prevents the independent Lisp test reader from
        // seeing half a receipt. Discarded submissions never advance readiness.
        let receipt = format!(
            "(:submission {} :frame {} :presentation {} :width {} :height {} :scale {} :outcome presented :clock-id {} :seconds {} :nanoseconds {})\n",
            submission.serial,
            submission.frame,
            submission.layout.get(),
            submission.width,
            submission.height,
            submission.scale,
            timestamp.clock_id,
            timestamp.seconds,
            timestamp.nanoseconds
        );
        let temporary = self.path.with_extension("pending");
        match fs::write(&temporary, receipt).and_then(|()| fs::rename(&temporary, &self.path)) {
            Ok(()) => self.latest_presented = submission.serial,
            Err(error) => tracing::warn!(%error, "cannot write native presentation receipt"),
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Receipts {
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
impl Dispatch<wp_presentation::WpPresentation, ()> for Receipts {
    fn event(
        state: &mut Self,
        _: &wp_presentation::WpPresentation,
        event: wp_presentation::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event {
            state.clock_id = Some(clk_id);
        }
    }
}

impl Dispatch<wp_presentation_feedback::WpPresentationFeedback, Submission> for Receipts {
    fn event(
        state: &mut Self,
        _: &wp_presentation_feedback::WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        submission: &Submission,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                ..
            } => {
                let Some(clock_id) = state.clock_id else {
                    tracing::warn!("presentation timestamp arrived without its clock domain");
                    return;
                };
                if tv_nsec >= 1_000_000_000 {
                    tracing::warn!(tv_nsec, "invalid compositor presentation timestamp");
                    return;
                }
                state.observe(NativeFeedback::Presented(ConfirmedPresentation {
                    submission: *submission,
                    timestamp: CompositorTimestamp {
                        clock_id,
                        seconds: (u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo),
                        nanoseconds: tv_nsec,
                    },
                }));
            }
            wp_presentation_feedback::Event::Discarded => {
                state.observe(NativeFeedback::Discarded(*submission))
            }
            _ => {}
        }
    }
}

struct DisplayScoped<T, D = OwnedDisplayHandle> {
    // Field order is significant: `inner` (wp_presentation proxy, EventQueue,
    // and foreign-display Connection) drops before `_display_owner` releases
    // Winit's `wl_display`.
    inner: T,
    _display_owner: D,
}

impl<T, D> DisplayScoped<T, D> {
    const fn new(inner: T, display_owner: D) -> Self {
        Self {
            inner,
            _display_owner: display_owner,
        }
    }
}

struct PresentationProtocol {
    display: NonNull<c_void>,
    presentation: wp_presentation::WpPresentation,
    queue: EventQueue<Receipts>,
    receipts: Receipts,
    connection: Connection,
    next_submission: u64,
}

impl Drop for PresentationProtocol {
    fn drop(&mut self) {
        let _ = self.queue.dispatch_pending(&mut self.receipts);
        self.presentation.destroy();
        let _ = self.connection.flush();
    }
}

type Session = DisplayScoped<PresentationProtocol, OwnedDisplayHandle>;

impl PresentationProtocol {
    fn connect(display_owner: OwnedDisplayHandle, path: PathBuf) -> Result<Session, String> {
        // `display_owner` is bound ahead of every Wayland guest object below so
        // reverse-declaration local drop keeps the `wl_display` alive across
        // early error returns as well as steady-state teardown.
        let RawDisplayHandle::Wayland(display) = display_owner
            .display_handle()
            .map_err(|error| error.to_string())?
            .as_raw()
        else {
            return Err("presentation receipts require a Wayland display".into());
        };
        // SAFETY: `display.display` was extracted from `display_owner`, which
        // keeps Winit's `wl_display` alive until after `PresentationProtocol`
        // (and any error-path locals) have dropped. The guest never disconnects
        // the foreign display.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, queue) =
            registry_queue_init::<Receipts>(&connection).map_err(|error| error.to_string())?;
        let presentation = globals
            .bind(&queue.handle(), 1..=1, ())
            .map_err(|error| error.to_string())?;
        Ok(DisplayScoped::new(
            Self {
                display: display.display,
                presentation,
                queue,
                receipts: Receipts {
                    path,
                    latest_presented: 0,
                    clock_id: None,
                    pending: PendingReceipts::default(),
                },
                connection,
                next_submission: 0,
            },
            display_owner,
        ))
    }

    fn request(
        &mut self,
        window: &dyn Window,
        frame: u64,
        layout: neomacs_display_protocol::PresentationId,
        size: (u32, u32),
        scale: f64,
        projected: &[neomacs_display_protocol::input_latency::InputToken],
    ) -> Result<(), String> {
        let RawDisplayHandle::Wayland(window_display) = window
            .display_handle()
            .map_err(|error| error.to_string())?
            .as_raw()
        else {
            return Err("presentation receipts require a Wayland display".into());
        };
        if window_display.display != self.display {
            return Err(
                "presentation receipts window display does not match session display".into(),
            );
        }
        let RawWindowHandle::Wayland(handle) = window
            .window_handle()
            .map_err(|error| error.to_string())?
            .as_raw()
        else {
            return Err("presentation receipts require a Wayland surface".into());
        };
        // SAFETY: verified above that `window` belongs to the same live
        // `wl_display` owned by the enclosing `DisplayScoped`'s
        // `OwnedDisplayHandle`. Borrow only to request feedback; never commit,
        // destroy, or dispatch it.
        let id = unsafe {
            ObjectId::from_ptr(
                wl_surface::WlSurface::interface(),
                handle.surface.as_ptr().cast(),
            )
        }
        .map_err(|error| error.to_string())?;
        let surface = wl_surface::WlSurface::from_id(&self.connection, id)
            .map_err(|error| error.to_string())?;
        self.next_submission += 1;
        let submission = Submission {
            serial: self.next_submission,
            frame,
            layout,
            width: size.0,
            height: size.1,
            scale,
        };
        neomacs_display_protocol::input_latency::projected_requested(
            projected,
            frame,
            submission.serial,
        );
        self.receipts
            .pending
            .requested(submission.serial, observe_platform_now());
        self.presentation
            .feedback(&surface, &self.queue.handle(), submission);
        tracing::debug!(?submission, "requested native presentation feedback");
        Ok(())
    }
}

enum ObserverState {
    Disabled,
    Uninitialized {
        path: PathBuf,
        display_owner: Option<OwnedDisplayHandle>,
    },
    Active(Session),
}

pub(crate) struct PresentationObserver {
    state: ObserverState,
}

impl PresentationObserver {
    pub(crate) fn new() -> Self {
        Self {
            state: std::env::var_os("NEOMACS_GUI_PRESENTATION_RECEIPT")
                .or_else(|| {
                    std::env::var_os("NEOMACS_INPUT_LATENCY_FILE").map(|path| {
                        PathBuf::from(path)
                            .with_extension("receipt")
                            .into_os_string()
                    })
                })
                .map_or(ObserverState::Disabled, |path| {
                    ObserverState::Uninitialized {
                        path: path.into(),
                        display_owner: None,
                    }
                }),
        }
    }

    pub(crate) fn attach_display(&mut self, display: OwnedDisplayHandle) {
        if let ObserverState::Uninitialized { display_owner, .. } = &mut self.state {
            *display_owner = Some(display);
        }
    }

    fn take_uninitialized_display(&mut self) -> Option<(PathBuf, OwnedDisplayHandle)> {
        let ObserverState::Uninitialized {
            path,
            display_owner,
        } = std::mem::replace(&mut self.state, ObserverState::Disabled)
        else {
            return None;
        };
        match display_owner {
            Some(display_owner) => Some((path, display_owner)),
            None => {
                tracing::warn!(
                    "native presentation feedback requires an attached OwnedDisplayHandle"
                );
                None
            }
        }
    }

    pub(crate) fn before_present(
        &mut self,
        window: &dyn Window,
        frame: u64,
        layout: neomacs_display_protocol::PresentationId,
        size: (u32, u32),
        scale: f64,
        projected: &[neomacs_display_protocol::input_latency::InputToken],
    ) {
        if matches!(self.state, ObserverState::Uninitialized { .. })
            && let Some((path, display_owner)) = self.take_uninitialized_display()
        {
            match PresentationProtocol::connect(display_owner, path) {
                Ok(session) => self.state = ObserverState::Active(session),
                Err(error) => tracing::warn!(%error, "native presentation feedback unavailable"),
            }
        }
        if let ObserverState::Active(session) = &mut self.state
            && let Err(error) = session
                .inner
                .request(window, frame, layout, size, scale, projected)
        {
            tracing::warn!(%error, "cannot request native presentation feedback");
        }
    }

    pub(crate) fn dispatch_deadline(&mut self, now: EventTime) -> Option<EventTime> {
        match &mut self.state {
            ObserverState::Active(session) => session.inner.receipts.pending.deadline(now),
            _ => None,
        }
    }

    pub(crate) fn dispatch_pending(&mut self) {
        if let ObserverState::Active(session) = &mut self.state {
            // Winit reads the shared display. Dispatch only our own queue; no
            // roundtrip, blocking read, redraw request, or parent commit here.
            if let Err(error) = session
                .inner
                .queue
                .dispatch_pending(&mut session.inner.receipts)
            {
                tracing::warn!(%error, "native presentation feedback dispatch failed");
            }
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.state = ObserverState::Disabled;
    }
}

#[cfg(test)]
#[path = "tests/wayland_test.rs"]
mod tests;
