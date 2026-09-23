use crate::{NativeChromeSupport, NativeTitlebarStyle, WindowChromePolicy};

#[test]
fn overlay_is_supported_only_when_native_backend_can_preserve_controls() {
    let policy = WindowChromePolicy::Native(NativeTitlebarStyle::Overlay);
    assert_eq!(policy.resolve(NativeChromeSupport::AppKit), Ok(policy));
    assert!(policy.resolve(NativeChromeSupport::SystemOnly).is_err());
    assert_eq!(
        WindowChromePolicy::ClientDecorated.resolve(NativeChromeSupport::SystemOnly),
        Ok(WindowChromePolicy::ClientDecorated)
    );
}
