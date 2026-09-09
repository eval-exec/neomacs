//! Public AppKit APIs only. Borrow the native view on the OS event-loop thread;
//! no native handle escapes this module or becomes shared renderer state.
use super::ChromeError;
use neomacs_display_protocol::{Color, NativeTitlebarStyle, WindowChromePolicy};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSColor, NSView, NSWindowStyleMask};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::platform::macos::WindowAttributesMacOS;
use winit::window::{Window, WindowAttributes};

pub(super) fn prepare(attrs: WindowAttributes, policy: WindowChromePolicy) -> WindowAttributes {
    let transparent = matches!(
        policy,
        WindowChromePolicy::Native(
            NativeTitlebarStyle::FrameBackground | NativeTitlebarStyle::Overlay
        )
    );
    attrs.with_platform_attributes(Box::new(
        WindowAttributesMacOS::default()
            .with_titlebar_transparent(transparent)
            .with_fullsize_content_view(matches!(
                policy,
                WindowChromePolicy::Native(NativeTitlebarStyle::Overlay)
            ))
            .with_titlebar_buttons_hidden(false),
    ))
}

pub(super) fn apply(
    window: &dyn Window,
    policy: WindowChromePolicy,
    color: Color,
) -> Result<(), ChromeError> {
    let _main_thread = MainThreadMarker::new().ok_or(ChromeError::Unavailable)?;
    let handle = window
        .window_handle()
        .map_err(|_| ChromeError::Unavailable)?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(ChromeError::Unavailable);
    };
    // SAFETY: the borrowed winit handle guarantees a live NSView for this
    // borrow; MainThreadMarker above checks AppKit's thread requirement.
    let view = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
    let native = view.window().ok_or(ChromeError::Unavailable)?;
    let overlay = matches!(
        policy,
        WindowChromePolicy::Native(NativeTitlebarStyle::Overlay)
    );
    let mut mask = native.styleMask();
    mask.set(NSWindowStyleMask::FullSizeContentView, overlay);
    if native.styleMask() != mask {
        // AppKit may clear the keyboard responder when changing style masks.
        // Preserve the existing native responder instead of stealing focus.
        let responder = native.firstResponder();
        native.setStyleMask(mask);
        if let Some(responder) = responder.as_deref() {
            let _ = native.makeFirstResponder(Some(responder));
        }
    }
    native.setTitlebarAppearsTransparent(matches!(
        policy,
        WindowChromePolicy::Native(
            NativeTitlebarStyle::FrameBackground | NativeTitlebarStyle::Overlay
        )
    ));
    // A full-size GPU surface paints its own background. An opaque native
    // fill behind it would double-composite translucent editor backgrounds.
    let native_color = match policy {
        WindowChromePolicy::Native(NativeTitlebarStyle::FrameBackground) => {
            let srgb = color.linear_to_srgb();
            NSColor::colorWithSRGBRed_green_blue_alpha(
                srgb.r as _,
                srgb.g as _,
                srgb.b as _,
                srgb.a as _,
            )
        }
        _ => NSColor::clearColor(),
    };
    native.setOpaque(color.a >= 1.0);
    native.setBackgroundColor(Some(&native_color));
    Ok(())
}
