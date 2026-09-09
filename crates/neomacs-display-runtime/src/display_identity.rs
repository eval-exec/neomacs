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

impl DisplayIdentityResolver {
    pub fn capture_environment() -> Self {
        Self {
            wayland: if std::env::var_os("WAYLAND_SOCKET").is_some() {
                WaylandConnectionName::InheritedSocket
            } else {
                WaylandConnectionName::Environment(std::env::var("WAYLAND_DISPLAY").ok())
            },
            x11: std::env::var("DISPLAY").ok(),
        }
    }

    pub fn resolve(
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
