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
pub(crate) fn apply_platform_window_identity(attrs: WindowAttributes) -> WindowAttributes {
    winit::platform::wayland::WindowAttributesExtWayland::with_name(
        attrs,
        NEOMACS_APPLICATION.app_id().as_str(),
        NEOMACS_APPLICATION.app_id().as_str(),
    )
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn apply_platform_window_identity(attrs: WindowAttributes) -> WindowAttributes {
    attrs
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use winit::window::Window;

    #[test]
    fn packaged_desktop_entry_matches_typed_runtime_identity() {
        let desktop_entry = include_str!("../assets/neomacs.desktop");

        assert_eq!(NEOMACS_APPLICATION.app_id().as_str(), "neomacs");
        assert_eq!(
            NEOMACS_APPLICATION.desktop_file_id().as_str(),
            "neomacs.desktop"
        );
        assert_eq!(NEOMACS_APPLICATION.icon_name().as_str(), "neomacs");
        assert!(desktop_entry.contains("\nIcon=neomacs\n"));
        assert!(desktop_entry.contains("\nStartupWMClass=neomacs\n"));
    }

    #[test]
    fn linux_window_attributes_use_packaged_desktop_id() {
        let attrs = apply_platform_window_identity(Window::default_attributes());

        assert!(format!("{attrs:?}").contains(NEOMACS_APPLICATION.app_id().as_str()));
    }
}
