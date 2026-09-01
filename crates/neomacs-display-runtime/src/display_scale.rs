//! Native display observation adapter for frame-scale policy.

use neomacs_display_protocol::{
    DeviceScale, DisplayGeometry, DisplayObservation, Dpi, X11DisplayObservation, XServerKind,
};
use std::sync::atomic::{AtomicU8, Ordering};
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
    WinitLogical = 1,
    X11Physical = 2,
}

static ACTIVE_WINDOW_COORDINATE_SYSTEM: AtomicU8 = AtomicU8::new(0);

fn coordinate_system_for_observation(observation: DisplayObservation) -> WindowCoordinateSystem {
    match observation {
        DisplayObservation::X11(_) => WindowCoordinateSystem::X11Physical,
        _ => WindowCoordinateSystem::WinitLogical,
    }
}

pub(crate) fn active_window_coordinate_system() -> Option<WindowCoordinateSystem> {
    match ACTIVE_WINDOW_COORDINATE_SYSTEM.load(Ordering::Acquire) {
        1 => Some(WindowCoordinateSystem::WinitLogical),
        2 => Some(WindowCoordinateSystem::X11Physical),
        _ => None,
    }
}

fn publish_window_coordinate_system(system: WindowCoordinateSystem) {
    ACTIVE_WINDOW_COORDINATE_SYSTEM.store(system as u8, Ordering::Release);
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
fn x11_observation_from_raw(
    has_xwayland_extension: bool,
    vendor: Option<&str>,
    xft_dpi: Option<f32>,
    display_height_px: i32,
    display_height_mm: i32,
    device_scale: DeviceScale,
) -> X11DisplayObservation {
    let xft_dpi = xft_dpi.and_then(|dpi| Dpi::new(dpi).ok());
    let geometry = u32::try_from(display_height_px)
        .ok()
        .zip(u32::try_from(display_height_mm).ok())
        .and_then(|(height_px, height_mm)| DisplayGeometry::new(height_px, height_mm).ok());
    X11DisplayObservation::new(
        classify_x_server(has_xwayland_extension, vendor),
        xft_dpi,
        geometry,
        device_scale,
    )
}

/// Observe the backend that winit actually selected, then gather native facts
/// without choosing a font-DPI policy.
#[must_use]
pub fn observe_event_loop_display<T: 'static>(event_loop: &EventLoop<T>) -> DisplayObservation {
    #[cfg(target_os = "linux")]
    let observation = {
        if event_loop.is_wayland() {
            DisplayObservation::Wayland {
                device_scale: DeviceScale::ONE,
            }
        } else {
            DisplayObservation::X11(query_x11_display())
        }
    };

    #[cfg(target_os = "macos")]
    let observation = {
        let _ = event_loop;
        DisplayObservation::Cocoa {
            device_scale: DeviceScale::ONE,
        }
    };

    #[cfg(windows)]
    let observation = {
        let _ = event_loop;
        DisplayObservation::Windows {
            device_scale: DeviceScale::ONE,
        }
    };

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    let observation = {
        let _ = event_loop;
        DisplayObservation::Wayland {
            device_scale: DeviceScale::ONE,
        }
    };

    publish_window_coordinate_system(coordinate_system_for_observation(observation));
    observation
}

#[cfg(target_os = "linux")]
fn query_x11_display() -> X11DisplayObservation {
    let fallback = || x11_observation_from_raw(false, None, None, 0, 0, DeviceScale::ONE);
    let Ok(xlib) = xlib::Xlib::open() else {
        return fallback();
    };
    let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
    if display.is_null() {
        return fallback();
    }

    let (has_xwayland_extension, vendor, xft_dpi, height_px, height_mm) = unsafe {
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
        (
            has_xwayland_extension,
            vendor,
            xft_dpi,
            (xlib.XDisplayHeight)(display, screen),
            (xlib.XDisplayHeightMM)(display, screen),
        )
    };
    unsafe { (xlib.XCloseDisplay)(display) };

    x11_observation_from_raw(
        has_xwayland_extension,
        vendor.as_deref(),
        xft_dpi,
        height_px,
        height_mm,
        DeviceScale::ONE,
    )
}

#[cfg(test)]
#[path = "display_scale_test.rs"]
mod tests;
