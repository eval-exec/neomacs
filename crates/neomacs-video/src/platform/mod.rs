#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod unsupported;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub(crate) use linux::LinuxPlatform as CurrentPlatform;
#[cfg(target_os = "macos")]
pub(crate) use macos::MacPlatform as CurrentPlatform;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(crate) use unsupported::UnsupportedPlatform as CurrentPlatform;
#[cfg(windows)]
pub(crate) use windows::WindowsPlatform as CurrentPlatform;
