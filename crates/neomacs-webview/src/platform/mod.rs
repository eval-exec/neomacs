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

/// Configure platform WebView environment variables before spawning any threads.
///
/// # Safety
/// The caller must ensure no other threads exist in the current process.
#[cfg(all(feature = "webview", target_os = "linux"))]
pub(crate) unsafe fn configure_before_threads() {
    // SAFETY: caller guarantees this process is single-threaded.
    unsafe { linux::configure_before_threads() };
}

/// Configure platform WebView environment variables before spawning any threads.
///
/// # Safety
/// The caller must ensure no other threads exist in the current process.
#[cfg(not(all(feature = "webview", target_os = "linux")))]
pub(crate) unsafe fn configure_before_threads() {}
