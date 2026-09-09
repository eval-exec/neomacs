use neomacs_display_protocol::{Color, NativeChromeSupport, UnsupportedChrome, WindowChromePolicy};
use winit::window::{Window, WindowAttributes};

#[cfg(target_os = "macos")]
mod macos;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ChromeError {
    #[error("unsupported native chrome: {0:?}")]
    Unsupported(UnsupportedChrome),
    #[cfg(target_os = "macos")]
    #[error("AppKit window is unavailable or called outside the main thread")]
    Unavailable,
}

pub(super) fn prepare(attrs: WindowAttributes, policy: WindowChromePolicy) -> WindowAttributes {
    std::cfg_select! {
        target_os = "macos" => { macos::prepare(attrs, policy) }
        _ => { let _ = policy; attrs }
    }
}

pub(super) fn apply(
    window: &dyn Window,
    policy: WindowChromePolicy,
    color: Color,
) -> Result<(), ChromeError> {
    let support = std::cfg_select! {
        target_os = "macos" => { NativeChromeSupport::AppKit }
        _ => { NativeChromeSupport::SystemOnly }
    };
    policy.resolve(support).map_err(ChromeError::Unsupported)?;
    std::cfg_select! {
        target_os = "macos" => { macos::apply(window, policy, color) }
        _ => { let _ = (window, color); Ok(()) }
    }
}
