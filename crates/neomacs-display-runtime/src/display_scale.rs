//! Native display observation adapter for frame-scale policy.

use neomacs_display_protocol::{
    DisplayHeightGeometry, DisplayObservation, Dpi, X11DisplayObservation, XServerKind,
};
use std::sync::OnceLock;
use winit::event_loop::EventLoop;

#[cfg(target_os = "linux")]
use std::ffi::CStr;
#[cfg(target_os = "linux")]
use std::ptr;
#[cfg(target_os = "linux")]
use winit::platform::wayland::EventLoopExtWayland;
#[cfg(target_os = "linux")]
use x11_dl::xlib;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowCoordinateSystem {
    WinitLogical,
    X11Physical,
}

static ACTIVE_WINDOW_COORDINATE_SYSTEM: OnceLock<WindowCoordinateSystem> = OnceLock::new();

fn coordinate_system_for_observation(observation: DisplayObservation) -> WindowCoordinateSystem {
    match observation {
        DisplayObservation::X11(_) => WindowCoordinateSystem::X11Physical,
        _ => WindowCoordinateSystem::WinitLogical,
    }
}

pub(crate) fn active_window_coordinate_system() -> Option<WindowCoordinateSystem> {
    ACTIVE_WINDOW_COORDINATE_SYSTEM.get().copied()
}

fn publish_window_coordinate_system(system: WindowCoordinateSystem) {
    // The native event loop is constructed once per process. Keep that
    // bootstrap invariant in the storage type instead of encoding it as
    // mutable magic integers.
    if let Err(system) = ACTIVE_WINDOW_COORDINATE_SYSTEM.set(system) {
        debug_assert_eq!(ACTIVE_WINDOW_COORDINATE_SYSTEM.get().copied(), Some(system));
    }
}

#[cfg(target_os = "linux")]
const X11_DISPLAY_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectedLinuxBackend {
    Wayland,
    X11,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
struct RawX11DisplayObservation {
    has_xwayland_extension: bool,
    vendor: Option<String>,
    xft_dpi: Option<f32>,
    display_height_px: i32,
    display_height_mm: i32,
}

#[cfg(target_os = "linux")]
impl RawX11DisplayObservation {
    fn validate(self) -> X11DisplayObservation {
        let xft_dpi = self.xft_dpi.and_then(|dpi| Dpi::new(dpi).ok());
        let geometry = u32::try_from(self.display_height_px)
            .ok()
            .zip(u32::try_from(self.display_height_mm).ok())
            .and_then(|(height_px, height_mm)| {
                DisplayHeightGeometry::new(height_px, height_mm).ok()
            });
        X11DisplayObservation::new(
            classify_x_server(self.has_xwayland_extension, self.vendor.as_deref()),
            xft_dpi,
            geometry,
        )
    }
}

#[cfg(target_os = "linux")]
fn classify_x_server(has_xwayland_extension: bool, vendor: Option<&str>) -> XServerKind {
    if has_xwayland_extension {
        XServerKind::Xwayland
    } else if vendor.is_some_and(|vendor| vendor.contains("X.Org")) {
        XServerKind::Xorg
    } else {
        XServerKind::Unknown
    }
}

#[cfg(target_os = "linux")]
fn fallback_x11_observation() -> X11DisplayObservation {
    RawX11DisplayObservation::default().validate()
}

#[cfg(target_os = "linux")]
fn observe_linux_backend(
    backend: SelectedLinuxBackend,
    x11_probe: impl FnOnce() -> X11DisplayObservation,
) -> DisplayObservation {
    match backend {
        SelectedLinuxBackend::Wayland => DisplayObservation::Wayland,
        SelectedLinuxBackend::X11 => DisplayObservation::X11(x11_probe()),
    }
}

/// Observe the backend that winit actually selected, then gather native facts
/// without choosing a font-DPI policy.
#[must_use]
pub fn observe_event_loop_display<T: 'static>(event_loop: &EventLoop<T>) -> DisplayObservation {
    #[cfg(target_os = "linux")]
    let observation = {
        let backend = if event_loop.is_wayland() {
            SelectedLinuxBackend::Wayland
        } else {
            SelectedLinuxBackend::X11
        };
        observe_linux_backend(backend, query_x11_display_bounded)
    };

    #[cfg(target_os = "macos")]
    let observation = {
        let _ = event_loop;
        DisplayObservation::Cocoa
    };

    #[cfg(windows)]
    let observation = {
        let _ = event_loop;
        DisplayObservation::Windows
    };

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    let observation = {
        let _ = event_loop;
        DisplayObservation::Wayland
    };

    publish_window_coordinate_system(coordinate_system_for_observation(observation));
    observation
}

#[cfg(target_os = "linux")]
fn query_x11_display_bounded() -> X11DisplayObservation {
    if std::env::var_os("DISPLAY").is_none() {
        return fallback_x11_observation();
    }
    query_x11_display_with_timeout(X11_DISPLAY_PROBE_TIMEOUT, query_x11_display)
}

#[cfg(target_os = "linux")]
fn query_x11_display_with_timeout(
    timeout: std::time::Duration,
    probe: impl FnOnce() -> X11DisplayObservation + Send + 'static,
) -> X11DisplayObservation {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("x11-display-probe".into())
        .spawn(move || {
            let _ = tx.send(probe());
        });
    if spawned.is_err() {
        tracing::warn!("failed to spawn X11 display probe; using fallback DPI");
        return fallback_x11_observation();
    }
    match rx.recv_timeout(timeout) {
        Ok(observation) => observation,
        Err(_) => {
            tracing::warn!(
                timeout_ms = timeout.as_millis(),
                "X11 display probe timed out; using fallback DPI"
            );
            fallback_x11_observation()
        }
    }
}

#[cfg(target_os = "linux")]
fn query_x11_display() -> X11DisplayObservation {
    let Ok(xlib) = xlib::Xlib::open() else {
        return fallback_x11_observation();
    };
    let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
    if display.is_null() {
        return fallback_x11_observation();
    }

    let raw = unsafe {
        let mut opcode = 0;
        let mut first_event = 0;
        let mut first_error = 0;
        let has_xwayland_extension = (xlib.XQueryExtension)(
            display,
            c"XWAYLAND".as_ptr(),
            &mut opcode,
            &mut first_event,
            &mut first_error,
        ) != 0;
        let vendor = (xlib.XServerVendor)(display);
        let vendor = if vendor.is_null() {
            None
        } else {
            CStr::from_ptr(vendor).to_str().ok().map(str::to_owned)
        };
        let resource = (xlib.XGetDefault)(display, c"Xft".as_ptr(), c"dpi".as_ptr());
        let xft_dpi = if resource.is_null() {
            None
        } else {
            CStr::from_ptr(resource)
                .to_str()
                .ok()
                .and_then(|value| value.trim().parse::<f32>().ok())
        };
        let screen = (xlib.XDefaultScreen)(display);
        RawX11DisplayObservation {
            has_xwayland_extension,
            vendor,
            xft_dpi,
            display_height_px: (xlib.XDisplayHeight)(display, screen),
            display_height_mm: (xlib.XDisplayHeightMM)(display, screen),
        }
    };
    unsafe { (xlib.XCloseDisplay)(display) };

    raw.validate()
}

#[cfg(test)]
#[path = "display_scale_test.rs"]
mod tests;
