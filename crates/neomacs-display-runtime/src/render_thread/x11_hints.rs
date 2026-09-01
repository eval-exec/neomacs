use neovm_core::window::GuiFrameGeometryHints;
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub(crate) fn apply_window_geometry_hints(window: &Window, geometry_hints: GuiFrameGeometryHints) {
    window.set_resize_increments(Some(PhysicalSize::new(
        geometry_hints.width_inc.max(1),
        geometry_hints.height_inc.max(1),
    )));

    #[cfg(target_os = "linux")]
    apply_x11_geometry_hints(window, geometry_hints);
}

#[cfg(target_os = "linux")]
fn apply_x11_geometry_hints(window: &Window, geometry_hints: GuiFrameGeometryHints) {
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    use std::ptr;
    use std::sync::OnceLock;
    use x11_dl::xlib;

    static XLIB: OnceLock<Option<xlib::Xlib>> = OnceLock::new();

    let Some(xlib) = XLIB.get_or_init(|| xlib::Xlib::open().ok()).as_ref() else {
        tracing::warn!("failed to load Xlib; skipping WM_NORMAL_HINTS update");
        return;
    };

    let Ok(display_handle) = window.display_handle() else {
        return;
    };
    let Ok(window_handle) = window.window_handle() else {
        return;
    };

    let xwindow = match window_handle.as_raw() {
        RawWindowHandle::Xlib(window) => window.window as xlib::Window,
        RawWindowHandle::Xcb(window) => window.window.get() as xlib::Window,
        _ => return,
    };

    let mut owned_display = None;
    let display_ptr = match display_handle.as_raw() {
        RawDisplayHandle::Xlib(display) => display
            .display
            .map(|display_ptr| display_ptr.as_ptr() as *mut xlib::Display),
        RawDisplayHandle::Xcb(_) => None,
        _ => return,
    }
    .or_else(|| unsafe {
        let display_ptr = (xlib.XOpenDisplay)(ptr::null());
        if display_ptr.is_null() {
            None
        } else {
            owned_display = Some(display_ptr);
            Some(display_ptr)
        }
    });

    let Some(display_ptr) = display_ptr else {
        tracing::warn!("failed to open X display; skipping WM_NORMAL_HINTS update");
        return;
    };

    unsafe {
        let mut size_hints: xlib::XSizeHints = std::mem::zeroed();
        let mut supplied_return: libc::c_long = 0;
        let value =
            (xlib.XGetWMNormalHints)(display_ptr, xwindow, &mut size_hints, &mut supplied_return);
        if value == 0 {
            size_hints.flags = 0;
        }

        size_hints.flags |=
            (xlib::PResizeInc | xlib::PMinSize | xlib::PBaseSize | xlib::PWinGravity)
                as libc::c_long;
        size_hints.width_inc = geometry_hints.width_inc.max(1) as i32;
        size_hints.height_inc = geometry_hints.height_inc.max(1) as i32;
        size_hints.base_width = geometry_hints.base_width.max(1) as i32;
        size_hints.base_height = geometry_hints.base_height.max(1) as i32;
        size_hints.min_width = geometry_hints.min_width.max(1) as i32;
        size_hints.min_height = geometry_hints.min_height.max(1) as i32;
        size_hints.win_gravity = xlib::NorthWestGravity;

        (xlib.XSetWMNormalHints)(display_ptr, xwindow, &mut size_hints);
        (xlib.XFlush)(display_ptr);

        if let Some(display_ptr) = owned_display {
            (xlib.XCloseDisplay)(display_ptr);
        }
    }
}
