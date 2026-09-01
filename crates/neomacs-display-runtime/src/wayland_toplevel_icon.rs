//! Native Wayland toplevel-icon integration.
//!
//! Winit 0.30 intentionally leaves `Window::set_window_icon` as a no-op on
//! Wayland. Keep the staging protocol and its unsafe foreign-connection bridge
//! inside this module so callers only express the application-level intent.

use std::ffi::c_void;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsFd;
use std::ptr::NonNull;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use rustix::fs::{MemfdFlags, memfd_create};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{BindError, GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_buffer, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::xdg::shell::client::xdg_toplevel;
use wayland_protocols::xdg::toplevel_icon::v1::client::{
    xdg_toplevel_icon_manager_v1, xdg_toplevel_icon_v1,
};
use winit::platform::wayland::WindowExtWayland;
use winit::window::Window;

use crate::window_icon::RasterizedWindowIcon;
use crate::window_identity::NEOMACS_APPLICATION;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ToplevelIconError {
    #[error("window has no usable Wayland display handle")]
    DisplayHandle,
    #[error("Wayland toplevel-icon setup failed: {0}")]
    Protocol(String),
    #[error("invalid icon dimensions")]
    InvalidDimensions,
    #[error("icon shared-memory setup failed: {0}")]
    SharedMemory(#[from] std::io::Error),
    #[error("icon memfd setup failed: {0}")]
    Memfd(#[from] rustix::io::Errno),
}

struct ProtocolState;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ProtocolState {
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

delegate_noop!(ProtocolState: ignore wl_shm::WlShm);
delegate_noop!(ProtocolState: ignore wl_shm_pool::WlShmPool);
delegate_noop!(ProtocolState: ignore wl_buffer::WlBuffer);
delegate_noop!(ProtocolState: ignore xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1);
delegate_noop!(ProtocolState: ignore xdg_toplevel_icon_v1::XdgToplevelIconV1);

struct WaylandIconSession {
    display: NonNull<c_void>,
    connection: Connection,
    event_queue: EventQueue<ProtocolState>,
    state: ProtocolState,
    manager: xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1,
    shm: wl_shm::WlShm,
}

enum WaylandIconBackend {
    Protocol(WaylandIconSession),
    DesktopEntryOnly { display: NonNull<c_void> },
}

impl WaylandIconBackend {
    fn display(&self) -> NonNull<c_void> {
        match self {
            Self::Protocol(session) => session.display,
            Self::DesktopEntryOnly { display } => *display,
        }
    }
}

impl WaylandIconSession {
    fn connect(display: NonNull<c_void>) -> Result<WaylandIconBackend, ToplevelIconError> {
        // SAFETY: the raw display comes from the live winit Window. The guest
        // backend never disconnects it, and the session stays on the render
        // thread where the Window outlives icon application.
        let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, event_queue) = registry_queue_init::<ProtocolState>(&connection)
            .map_err(|error| ToplevelIconError::Protocol(error.to_string()))?;
        let queue_handle = event_queue.handle();
        let manager: xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1 =
            match globals.bind(&queue_handle, 1..=1, ()) {
                Ok(manager) => manager,
                Err(BindError::NotPresent | BindError::UnsupportedVersion) => {
                    tracing::debug!(
                        desktop_file_id = NEOMACS_APPLICATION.desktop_file_id().as_str(),
                        "compositor has no xdg-toplevel-icon protocol; using desktop-entry icon"
                    );
                    return Ok(WaylandIconBackend::DesktopEntryOnly { display });
                }
            };
        let shm = globals
            .bind(&queue_handle, 1..=1, ())
            .map_err(|error| ToplevelIconError::Protocol(error.to_string()))?;

        Ok(WaylandIconBackend::Protocol(Self {
            display,
            connection,
            event_queue,
            state: ProtocolState,
            manager,
            shm,
        }))
    }

    fn set_icon(
        &mut self,
        raw_toplevel: NonNull<c_void>,
        icon_pixels: &RasterizedWindowIcon,
    ) -> Result<(), ToplevelIconError> {
        let width =
            i32::try_from(icon_pixels.width()).map_err(|_| ToplevelIconError::InvalidDimensions)?;
        let height = i32::try_from(icon_pixels.height())
            .map_err(|_| ToplevelIconError::InvalidDimensions)?;
        let stride = width
            .checked_mul(4)
            .ok_or(ToplevelIconError::InvalidDimensions)?;
        let bytes = icon_pixels.to_wayland_argb8888();
        let byte_len =
            i32::try_from(bytes.len()).map_err(|_| ToplevelIconError::InvalidDimensions)?;

        let fd = memfd_create(c"neomacs-window-icon", MemfdFlags::CLOEXEC)?;
        let mut file = File::from(fd);
        file.set_len(bytes.len() as u64)?;
        file.write_all(&bytes)?;

        let queue_handle = self.event_queue.handle();
        let pool = self
            .shm
            .create_pool(file.as_fd(), byte_len, &queue_handle, ());
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            stride,
            wl_shm::Format::Argb8888,
            &queue_handle,
            (),
        );
        let protocol_icon = self.manager.create_icon(&queue_handle, ());
        protocol_icon.set_name(NEOMACS_APPLICATION.icon_name().as_str().to_owned());
        protocol_icon.add_buffer(&buffer, 1);

        // SAFETY: winit returned this pointer as the live xdg_toplevel for the
        // same Window and display. We only borrow it to send set_icon; winit
        // retains ownership and remains the sole event dispatcher.
        let toplevel_id = unsafe {
            ObjectId::from_ptr(
                xdg_toplevel::XdgToplevel::interface(),
                raw_toplevel.as_ptr().cast(),
            )
        }
        .map_err(|error| ToplevelIconError::Protocol(error.to_string()))?;
        let toplevel = xdg_toplevel::XdgToplevel::from_id(&self.connection, toplevel_id)
            .map_err(|error| ToplevelIconError::Protocol(error.to_string()))?;

        self.manager.set_icon(&toplevel, Some(&protocol_icon));

        // set_icon snapshots the immutable icon. Destroying these temporary
        // protocol objects also makes their Rust/file resources deterministic.
        protocol_icon.destroy();
        buffer.destroy();
        pool.destroy();
        self.connection
            .flush()
            .map_err(|error| ToplevelIconError::Protocol(error.to_string()))?;
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|error| ToplevelIconError::Protocol(error.to_string()))?;
        Ok(())
    }
}

pub(crate) struct WaylandToplevelIconService {
    backend: Option<WaylandIconBackend>,
}

impl WaylandToplevelIconService {
    pub(crate) const fn new() -> Self {
        Self { backend: None }
    }

    pub(crate) fn apply(
        &mut self,
        window: &Window,
        icon_pixels: &RasterizedWindowIcon,
    ) -> Result<(), ToplevelIconError> {
        let Some(raw_toplevel) = window.xdg_toplevel() else {
            return Ok(());
        };
        let display = match window
            .display_handle()
            .map_err(|_| ToplevelIconError::DisplayHandle)?
            .as_raw()
        {
            RawDisplayHandle::Wayland(handle) => handle.display,
            _ => return Err(ToplevelIconError::DisplayHandle),
        };

        if self
            .backend
            .as_ref()
            .is_none_or(|current| current.display() != display)
        {
            self.shutdown();
            self.backend = Some(WaylandIconSession::connect(display)?);
        }
        match self
            .backend
            .as_mut()
            .expect("backend was initialized above")
        {
            WaylandIconBackend::Protocol(session) => session.set_icon(raw_toplevel, icon_pixels),
            WaylandIconBackend::DesktopEntryOnly { .. } => Ok(()),
        }
    }

    /// Release the guest protocol queue while Winit's display is still live.
    pub(crate) fn shutdown(&mut self) {
        let Some(backend) = self.backend.take() else {
            return;
        };
        if let WaylandIconBackend::Protocol(mut session) = backend {
            let _ = session.event_queue.dispatch_pending(&mut session.state);
            session.manager.destroy();
            let _ = session.connection.flush();
        }
    }
}

impl Drop for WaylandToplevelIconService {
    fn drop(&mut self) {
        // Normal shutdown empties the session in RenderApp::handle_exiting.
        // If winit exits abnormally, its foreign display may already be dead;
        // leaking the tiny guest wrapper is safer than dereferencing it during
        // Drop, and the OS reclaims it with the process.
        if let Some(backend) = self.backend.take() {
            std::mem::forget(backend);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::event_loop::EventLoop;
    use winit::platform::wayland::EventLoopBuilderExtWayland;

    #[test]
    #[ignore = "requires a private Wayland compositor with xdg-toplevel-icon-v1"]
    fn private_wayland_compositor_accepts_native_toplevel_icon() {
        let mut builder = EventLoop::builder();
        EventLoopBuilderExtWayland::with_any_thread(&mut builder, true);
        let event_loop = builder
            .build()
            .expect("connect to private Wayland compositor");
        #[allow(deprecated)]
        let window = event_loop
            .create_window(crate::window_identity::apply_platform_window_identity(
                Window::default_attributes(),
            ))
            .expect("create private Wayland window");
        let pixels = crate::window_icon::load_window_icon().expect("decode canonical icon");
        let mut service = WaylandToplevelIconService::new();

        service
            .apply(&window, &pixels)
            .expect("apply native Wayland toplevel icon");
        assert!(matches!(
            service.backend,
            Some(WaylandIconBackend::Protocol(_))
        ));

        service.shutdown();
    }
}
