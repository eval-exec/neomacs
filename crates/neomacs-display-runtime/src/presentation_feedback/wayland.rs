use std::{fs, path::PathBuf};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_surface};
use wayland_protocols::wp::presentation_time::client::{wp_presentation, wp_presentation_feedback};
use winit::window::Window;

#[derive(Clone, Copy, Debug)]
struct Submission {
    serial: u64,
    frame: u64,
    width: u32,
    height: u32,
    scale: f64,
}

#[derive(Clone, Copy, Debug)]
enum Outcome {
    Presented,
    Discarded,
}

struct Receipts {
    path: PathBuf,
    latest_presented: u64,
}

impl Receipts {
    fn observe(&mut self, submission: &Submission, outcome: Outcome) {
        tracing::debug!(?submission, ?outcome, "native presentation feedback");
        if !matches!(outcome, Outcome::Presented) || submission.serial <= self.latest_presented {
            return;
        }
        // Atomic replacement prevents the independent Lisp test reader from
        // seeing half a receipt. Discarded submissions never advance readiness.
        let receipt = format!(
            "(:submission {} :frame {} :width {} :height {} :scale {} :outcome presented)\n",
            submission.serial, submission.frame, submission.width, submission.height, submission.scale
        );
        let temporary = self.path.with_extension("pending");
        match fs::write(&temporary, receipt).and_then(|()| fs::rename(&temporary, &self.path)) {
            Ok(()) => self.latest_presented = submission.serial,
            Err(error) => tracing::warn!(%error, "cannot write native presentation receipt"),
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Receipts {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
delegate_noop!(Receipts: ignore wp_presentation::WpPresentation);

impl Dispatch<wp_presentation_feedback::WpPresentationFeedback, Submission> for Receipts {
    fn event(state: &mut Self, _: &wp_presentation_feedback::WpPresentationFeedback, event: wp_presentation_feedback::Event, submission: &Submission, _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wp_presentation_feedback::Event::Presented { .. } => state.observe(submission, Outcome::Presented),
            wp_presentation_feedback::Event::Discarded => state.observe(submission, Outcome::Discarded),
            _ => {}
        }
    }
}

struct Session {
    connection: Connection,
    queue: EventQueue<Receipts>,
    receipts: Receipts,
    presentation: wp_presentation::WpPresentation,
    next_submission: u64,
}

impl Session {
    fn connect(window: &dyn Window, path: PathBuf) -> Result<Self, String> {
        let RawDisplayHandle::Wayland(display) = window.display_handle().map_err(|error| error.to_string())?.as_raw() else {
            return Err("presentation receipts require a Wayland display".into());
        };
        // SAFETY: borrowed from a live winit Window on the render thread. The
        // guest never disconnects the display and is shut down before winit.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<Receipts>(&connection).map_err(|error| error.to_string())?;
        let presentation = globals.bind(&queue.handle(), 1..=1, ()).map_err(|error| error.to_string())?;
        Ok(Self { connection, queue, receipts: Receipts { path, latest_presented: 0 }, presentation, next_submission: 0 })
    }

    fn request(&mut self, window: &dyn Window, frame: u64, size: (u32, u32), scale: f64) -> Result<(), String> {
        let RawWindowHandle::Wayland(handle) = window.window_handle().map_err(|error| error.to_string())?.as_raw() else {
            return Err("presentation receipts require a Wayland surface".into());
        };
        // SAFETY: this surface belongs to the same live winit display. Borrow
        // only to request feedback; never commit, destroy, or dispatch it.
        let id = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), handle.surface.as_ptr().cast()) }.map_err(|error| error.to_string())?;
        let surface = wl_surface::WlSurface::from_id(&self.connection, id).map_err(|error| error.to_string())?;
        self.next_submission += 1;
        let submission = Submission { serial: self.next_submission, frame, width: size.0, height: size.1, scale };
        self.presentation.feedback(&surface, &self.queue.handle(), submission);
        tracing::debug!(?submission, "requested native presentation feedback");
        Ok(())
    }
}

enum ObserverState {
    Disabled,
    Uninitialized(PathBuf),
    Active(Session),
}

pub(crate) struct PresentationObserver {
    state: ObserverState,
}

impl PresentationObserver {
    pub(crate) fn new() -> Self {
        Self { state: std::env::var_os("NEOMACS_GUI_PRESENTATION_RECEIPT").map_or(ObserverState::Disabled, |path| ObserverState::Uninitialized(path.into())) }
    }

    pub(crate) fn before_present(&mut self, window: &dyn Window, frame: u64, size: (u32, u32), scale: f64) {
        if matches!(self.state, ObserverState::Uninitialized(_)) {
            let ObserverState::Uninitialized(path) = std::mem::replace(&mut self.state, ObserverState::Disabled) else { unreachable!() };
            match Session::connect(window, path) {
                Ok(session) => self.state = ObserverState::Active(session),
                Err(error) => tracing::warn!(%error, "native presentation feedback unavailable"),
            }
        }
        if let ObserverState::Active(session) = &mut self.state {
            if let Err(error) = session.request(window, frame, size, scale) {
                tracing::warn!(%error, "cannot request native presentation feedback");
            }
        }
    }

    pub(crate) fn dispatch_pending(&mut self) {
        if let ObserverState::Active(session) = &mut self.state {
            // Winit reads the shared display. Dispatch only our own queue; no
            // roundtrip, blocking read, redraw request, or parent commit here.
            if let Err(error) = session.queue.dispatch_pending(&mut session.receipts) {
                tracing::warn!(%error, "native presentation feedback dispatch failed");
            }
        }
    }

    pub(crate) fn shutdown(&mut self) {
        if let ObserverState::Active(mut session) = std::mem::replace(&mut self.state, ObserverState::Disabled) {
            let _ = session.queue.dispatch_pending(&mut session.receipts);
            session.presentation.destroy();
            let _ = session.connection.flush();
        }
    }
}

impl Drop for PresentationObserver {
    fn drop(&mut self) {
        // On abnormal winit teardown the foreign display may already be gone.
        // Normal shutdown explicitly releases the guest while it is live.
        if let ObserverState::Active(session) = std::mem::replace(&mut self.state, ObserverState::Disabled) {
            std::mem::forget(session);
        }
    }
}
