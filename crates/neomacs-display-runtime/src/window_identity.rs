//! Platform window identity helpers.

use winit::window::WindowAttributes;

#[cfg(target_os = "linux")]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/application_identity.rs"));
}

/// Wayland application IDs and icon-theme names are both strings at the
/// protocol boundary, but they are not interchangeable concepts.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ApplicationId(&'static str);

#[cfg(target_os = "linux")]
impl ApplicationId {
    pub(crate) const fn as_str(self) -> &'static str {
        self.0
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IconName(&'static str);

#[cfg(target_os = "linux")]
impl IconName {
    pub(crate) const fn as_str(self) -> &'static str {
        self.0
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DesktopFileId(&'static str);

#[cfg(target_os = "linux")]
impl DesktopFileId {
    pub(crate) const fn as_str(self) -> &'static str {
        self.0
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ApplicationIdentity {
    app_id: ApplicationId,
    desktop_file_id: DesktopFileId,
    icon_name: IconName,
}

#[cfg(target_os = "linux")]
impl ApplicationIdentity {
    pub(crate) const fn app_id(self) -> ApplicationId {
        self.app_id
    }

    pub(crate) const fn desktop_file_id(self) -> DesktopFileId {
        self.desktop_file_id
    }

    pub(crate) const fn icon_name(self) -> IconName {
        self.icon_name
    }
}

#[cfg(target_os = "linux")]
pub(crate) const NEOMACS_APPLICATION: ApplicationIdentity = ApplicationIdentity {
    app_id: ApplicationId(generated::GENERATED_APP_ID),
    desktop_file_id: DesktopFileId(generated::GENERATED_DESKTOP_FILE_ID),
    icon_name: IconName(generated::GENERATED_ICON_NAME),
};

#[cfg(target_os = "linux")]
pub(crate) fn apply_platform_window_identity(
    attrs: WindowAttributes,
    event_loop: &dyn winit::event_loop::ActiveEventLoop,
) -> WindowAttributes {
    use winit::platform::wayland::ActiveEventLoopExtWayland;
    linux_window_identity(attrs, event_loop.is_wayland())
}

#[cfg(target_os = "linux")]
fn linux_window_identity(attrs: WindowAttributes, wayland: bool) -> WindowAttributes {
    let name = NEOMACS_APPLICATION.app_id().as_str();
    if wayland {
        attrs.with_platform_attributes(Box::new(
            winit::platform::wayland::WindowAttributesWayland::default().with_name(name, name),
        ))
    } else {
        attrs.with_platform_attributes(Box::new(
            winit::platform::x11::WindowAttributesX11::default().with_name(name, name),
        ))
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn apply_platform_window_identity(
    attrs: WindowAttributes,
    _event_loop: &dyn winit::event_loop::ActiveEventLoop,
) -> WindowAttributes {
    // Lisp's mouse-wheel-scroll-amount owns lines per detent. Request wheel
    // units so a system multiplier is not applied a second time by mwheel.el.
    attrs.with_platform_attributes(Box::new(
        winit::platform::windows::WindowAttributesWindows::default()
            .with_use_system_scroll_speed(false)
            .with_precision_touchpad(true),
    ))
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub(crate) fn apply_platform_window_identity(
    attrs: WindowAttributes,
    _event_loop: &dyn winit::event_loop::ActiveEventLoop,
) -> WindowAttributes {
    attrs
}

#[cfg(all(test, target_os = "linux"))]
#[path = "tests/window_identity_test.rs"]
mod tests;
