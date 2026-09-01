#[cfg(all(feature = "webview", target_os = "linux"))]
mod linux;
#[cfg(all(feature = "webview", target_os = "macos"))]
mod macos;
#[cfg(not(any(
    all(feature = "webview", target_os = "linux"),
    all(feature = "webview", target_os = "macos"),
    all(feature = "webview", target_os = "windows")
)))]
mod unsupported;
#[cfg(all(feature = "webview", target_os = "windows"))]
mod windows;

#[cfg(all(feature = "webview", target_os = "linux"))]
pub(crate) use linux::LinuxPlatform as CurrentPlatform;
#[cfg(all(feature = "webview", target_os = "macos"))]
pub(crate) use macos::MacPlatform as CurrentPlatform;
#[cfg(not(any(
    all(feature = "webview", target_os = "linux"),
    all(feature = "webview", target_os = "macos"),
    all(feature = "webview", target_os = "windows")
)))]
pub(crate) use unsupported::UnsupportedPlatform as CurrentPlatform;
#[cfg(all(feature = "webview", target_os = "windows"))]
pub(crate) use windows::WindowsPlatform as CurrentPlatform;
