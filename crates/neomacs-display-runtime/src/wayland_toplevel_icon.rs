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
use winit::event_loop::OwnedDisplayHandle;
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

struct DisplayScoped<T, D = OwnedDisplayHandle> {
    // Field order is significant: `inner` (Wayland proxies, event queue, and
    // foreign-display Connection) drops before `_display_owner` releases
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

struct WaylandIconProtocol {
    display: NonNull<c_void>,
    manager: xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1,
    shm: wl_shm::WlShm,
    event_queue: EventQueue<ProtocolState>,
    state: ProtocolState,
    connection: Connection,
}

impl Drop for WaylandIconProtocol {
    fn drop(&mut self) {
        let _ = self.event_queue.dispatch_pending(&mut self.state);
        self.manager.destroy();
        let _ = self.connection.flush();
    }
}

type WaylandIconSession = DisplayScoped<WaylandIconProtocol, OwnedDisplayHandle>;

enum WaylandIconBackend {
    Protocol(WaylandIconSession),
    DesktopEntryOnly { display: NonNull<c_void> },
}

impl WaylandIconBackend {
    fn display(&self) -> NonNull<c_void> {
        match self {
            Self::Protocol(session) => session.inner.display,
            Self::DesktopEntryOnly { display } => *display,
        }
    }
}

impl WaylandIconProtocol {
    fn connect(display_owner: OwnedDisplayHandle) -> Result<WaylandIconBackend, ToplevelIconError> {
        // `display_owner` is bound ahead of every Wayland guest object below so
        // reverse-declaration local drop keeps the `wl_display` alive across
        // both early error returns and the `DesktopEntryOnly` path.
        let display = match display_owner
            .display_handle()
            .map_err(|_| ToplevelIconError::DisplayHandle)?
            .as_raw()
        {
            RawDisplayHandle::Wayland(handle) => handle.display,
            _ => return Err(ToplevelIconError::DisplayHandle),
        };
        // SAFETY: `display` was extracted from `display_owner`, which keeps
        // Winit's `wl_display` alive until after `WaylandIconProtocol` (and any
        // error-path locals) have dropped. The guest backend never disconnects
        // the foreign display.
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

        Ok(WaylandIconBackend::Protocol(DisplayScoped::new(
            Self {
                display,
                manager,
                shm,
                event_queue,
                state: ProtocolState,
                connection,
            },
            display_owner,
        )))
    }

    /// # Safety
    /// `raw_toplevel` must be a live `xdg_toplevel` proxy belonging to the same
    /// `wl_display` that `self` connected to, remaining valid for the duration
    /// of this call.
    unsafe fn set_icon(
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

fn checked_matching_wayland_display(
    window_display: RawDisplayHandle,
    owner_display: RawDisplayHandle,
) -> Result<NonNull<c_void>, ToplevelIconError> {
    match (window_display, owner_display) {
        (RawDisplayHandle::Wayland(window), RawDisplayHandle::Wayland(owner))
            if window.display == owner.display =>
        {
            Ok(owner.display)
        }
        _ => Err(ToplevelIconError::DisplayHandle),
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
        window: &dyn Window,
        display_owner: OwnedDisplayHandle,
        icon_pixels: &RasterizedWindowIcon,
    ) -> Result<(), ToplevelIconError> {
        let Some(raw_toplevel) = window.xdg_toplevel() else {
            return Ok(());
        };
        let window_display = window
            .display_handle()
            .map_err(|_| ToplevelIconError::DisplayHandle)?
            .as_raw();
        let owner_display = display_owner
            .display_handle()
            .map_err(|_| ToplevelIconError::DisplayHandle)?
            .as_raw();
        let owner_display = checked_matching_wayland_display(window_display, owner_display)?;

        if self
            .backend
            .as_ref()
            .is_none_or(|current| current.display() != owner_display)
        {
            self.shutdown();
            self.backend = Some(WaylandIconProtocol::connect(display_owner)?);
        }
        match self
            .backend
            .as_mut()
            .expect("backend was initialized above")
        {
            WaylandIconBackend::Protocol(session) => {
                // SAFETY: `raw_toplevel` comes from `window.xdg_toplevel()`,
                // verified above to belong to the same `wl_display` owned by
                // `session._display_owner`.
                unsafe { session.inner.set_icon(raw_toplevel, icon_pixels) }
            }
            WaylandIconBackend::DesktopEntryOnly { .. } => Ok(()),
        }
    }

    /// Release the guest protocol queue and its owned display handle.
    pub(crate) fn shutdown(&mut self) {
        self.backend = None;
    }
}

#[cfg(test)]
mod drop_order_tests {
    use super::{DisplayScoped, ToplevelIconError, checked_matching_wayland_display};
    use raw_window_handle::{RawDisplayHandle, WaylandDisplayHandle, XlibDisplayHandle};
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::rc::Rc;

    #[test]
    fn matching_wayland_display_guard_rejects_mismatch_and_non_wayland() {
        let mut a = 1u8;
        let mut b = 2u8;
        let ptr_a = NonNull::from(&mut a).cast::<c_void>();
        let ptr_b = NonNull::from(&mut b).cast::<c_void>();
        let wl_a = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(ptr_a));
        let wl_b = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(ptr_b));
        let xlib = RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0));

        assert_eq!(checked_matching_wayland_display(wl_a, wl_a).unwrap(), ptr_a);
        assert!(matches!(
            checked_matching_wayland_display(wl_a, wl_b),
            Err(ToplevelIconError::DisplayHandle)
        ));
        assert!(matches!(
            checked_matching_wayland_display(xlib, wl_a),
            Err(ToplevelIconError::DisplayHandle)
        ));
        assert!(matches!(
            checked_matching_wayland_display(wl_a, xlib),
            Err(ToplevelIconError::DisplayHandle)
        ));
    }

    struct DropRecorder {
        label: &'static str,
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Drop for DropRecorder {
        fn drop(&mut self) {
            self.log.borrow_mut().push(self.label);
        }
    }

    struct FakeProtocol {
        _manager: DropRecorder,
        _connection: DropRecorder,
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Drop for FakeProtocol {
        fn drop(&mut self) {
            self.log.borrow_mut().push("protocol_drop");
        }
    }

    #[test]
    fn display_scoped_drops_protocol_before_display_owner_on_drop_and_unwind() {
        let log = Rc::new(RefCell::new(Vec::new()));
        {
            let _scoped = DisplayScoped::new(
                FakeProtocol {
                    _manager: DropRecorder {
                        label: "manager",
                        log: Rc::clone(&log),
                    },
                    _connection: DropRecorder {
                        label: "connection",
                        log: Rc::clone(&log),
                    },
                    log: Rc::clone(&log),
                },
                DropRecorder {
                    label: "display_owner",
                    log: Rc::clone(&log),
                },
            );
        }
        assert_eq!(
            *log.borrow(),
            vec!["protocol_drop", "manager", "connection", "display_owner"]
        );

        log.borrow_mut().clear();
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _scoped = DisplayScoped::new(
                FakeProtocol {
                    _manager: DropRecorder {
                        label: "manager",
                        log: Rc::clone(&log),
                    },
                    _connection: DropRecorder {
                        label: "connection",
                        log: Rc::clone(&log),
                    },
                    log: Rc::clone(&log),
                },
                DropRecorder {
                    label: "display_owner",
                    log: Rc::clone(&log),
                },
            );
            panic!("simulated unwind");
        }));
        assert!(unwind.is_err());
        assert_eq!(
            *log.borrow(),
            vec!["protocol_drop", "manager", "connection", "display_owner"]
        );
    }
}

#[cfg(all(test, target_os = "linux", feature = "wayland-live-tests"))]
#[path = "wayland_toplevel_icon/tests/wayland_toplevel_icon_test.rs"]
mod tests;
