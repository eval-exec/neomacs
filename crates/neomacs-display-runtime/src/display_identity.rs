//! Resolve terminal/frame identity from the backend winit actually selected.

use neomacs_display_protocol::{GraphicalBackend, GraphicalDisplayIdentity};
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use winit::event_loop::EventLoop;

#[derive(Debug)]
pub enum DisplayIdentityError {
    Unavailable(raw_window_handle::HandleError),
    UnsupportedBackend,
}

/// Capture connection provenance before winit opens the display: Wayland can
/// consume/remove WAYLAND_SOCKET while connecting. These are naming hints, not
/// a backend selection policy; only the opened display handle selects backend.
pub struct DisplayIdentityResolver {
    wayland: WaylandConnectionName,
    x11: Option<String>,
}

enum WaylandConnectionName {
    InheritedSocket,
    Environment(Option<String>),
}

struct DisplayEnvironment {
    wayland_display: Option<String>,
    wayland_socket: Option<String>,
    x11: Option<String>,
}

impl DisplayEnvironment {
    fn capture() -> Self {
        // Match winit's UTF-8, nonempty environment checks for backend choice.
        let nonempty = |name| std::env::var(name).ok().filter(|value| !value.is_empty());
        Self {
            wayland_display: nonempty("WAYLAND_DISPLAY"),
            wayland_socket: nonempty("WAYLAND_SOCKET"),
            x11: nonempty("DISPLAY"),
        }
    }
}

/// Whether an interactive daemon should retain a native graphical host.
///
/// On Linux, an inherited Wayland descriptor must be a socket before winit
/// can consume it. This check only borrows the descriptor; refusal leaves it
/// and its environment entry available to the daemon and its children.
pub fn should_open_daemon_gui_host() -> bool {
    #[cfg(not(target_os = "linux"))]
    {
        ["WAYLAND_DISPLAY", "DISPLAY"]
            .iter()
            .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
    }
    #[cfg(target_os = "linux")]
    {
        let environment = DisplayEnvironment::capture();
        if let Some(socket) = &environment.wayland_socket {
            let valid = socket.parse::<i32>().ok().is_some_and(|fd| {
                let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
                // SAFETY: fstat writes into suitably aligned storage without
                // taking ownership of fd. Read the result only on success.
                unsafe {
                    libc::fstat(fd, metadata.as_mut_ptr()) == 0
                        && metadata.assume_init().st_mode & libc::S_IFMT == libc::S_IFSOCK
                }
            });
            if !valid {
                tracing::warn!(
                    "daemon continuing without graphical hosting: invalid WAYLAND_SOCKET; expected an open socket descriptor"
                );
                return false;
            }
            return true;
        }
        environment.wayland_display.is_some() || environment.x11.is_some()
    }
}

impl DisplayIdentityResolver {
    pub fn capture_environment() -> Self {
        let environment = DisplayEnvironment::capture();
        Self {
            wayland: if environment.wayland_socket.is_some() {
                WaylandConnectionName::InheritedSocket
            } else {
                WaylandConnectionName::Environment(environment.wayland_display)
            },
            x11: environment.x11,
        }
    }

    pub(crate) fn resolve(
        self,
        event_loop: &EventLoop,
        system_name: &str,
    ) -> Result<GraphicalDisplayIdentity, DisplayIdentityError> {
        let display = event_loop
            .display_handle()
            .map_err(DisplayIdentityError::Unavailable)?;
        let backend = match display.as_raw() {
            RawDisplayHandle::Wayland(_) => GraphicalBackend::Wayland,
            RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_) => GraphicalBackend::X11,
            RawDisplayHandle::AppKit(_) => GraphicalBackend::Cocoa,
            RawDisplayHandle::Windows(_) => GraphicalBackend::Windows,
            RawDisplayHandle::Android(_) => GraphicalBackend::Android,
            RawDisplayHandle::Web(_) => GraphicalBackend::Web,
            _ => return Err(DisplayIdentityError::UnsupportedBackend),
        };
        let name = match backend {
            GraphicalBackend::Wayland => match self.wayland {
                WaylandConnectionName::InheritedSocket => None,
                WaylandConnectionName::Environment(name) => name,
            },
            GraphicalBackend::X11 => self.x11,
            GraphicalBackend::Cocoa => Some(system_name.to_owned()),
            GraphicalBackend::Windows | GraphicalBackend::Android | GraphicalBackend::Web => None,
        };
        Ok(name
            .and_then(|name| GraphicalDisplayIdentity::named(backend, name).ok())
            .unwrap_or_else(|| GraphicalDisplayIdentity::anonymous_connection(backend)))
    }
}
