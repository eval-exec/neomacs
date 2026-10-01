use super::{WindowChromeController, controller::NativeAppearance};
use neomacs_display_protocol::{Color, NativeTitlebarStyle, WindowChromePolicy};

#[test]
fn native_background_is_restored_after_an_external_style_reset() {
    let mut chrome = WindowChromeController::default();
    let desired = NativeAppearance {
        policy: WindowChromePolicy::Native(NativeTitlebarStyle::FrameBackground),
        background: Color::rgb(1.0, 0.0, 1.0),
    };
    let mut native_background = Color::BLACK;
    chrome
        .apply(desired, |appearance| {
            native_background = appearance.background;
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(native_background, desired.background);
    // Simulate AppKit/winit replacing the native background during a style change.
    native_background = Color::BLACK;
    chrome.invalidate();
    chrome
        .apply(desired, |appearance| {
            native_background = appearance.background;
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(native_background, desired.background);
}

#[test]
fn failed_native_application_remains_retryable_and_frames_are_independent() {
    let desired = NativeAppearance {
        policy: WindowChromePolicy::Native(NativeTitlebarStyle::Overlay),
        background: Color::rgb(1.0, 0.0, 1.0),
    };
    let mut primary = WindowChromeController::default();
    let mut secondary = WindowChromeController::default();
    assert_eq!(
        primary.apply(desired, |_| Err("native unavailable")),
        Err("native unavailable")
    );
    let mut primary_color = Color::BLACK;
    primary
        .apply(desired, |appearance| {
            primary_color = appearance.background;
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(primary_color, desired.background);
    primary
        .apply(desired, |_| {
            panic!("unchanged native state must not be reapplied")
        })
        .unwrap_or_else(|_: ()| unreachable!());
    let mut secondary_color = Color::BLACK;
    secondary
        .apply(desired, |appearance| {
            secondary_color = appearance.background;
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(
        secondary_color, desired.background,
        "another window must receive its own initial appearance"
    );
    let changed = NativeAppearance {
        background: Color::WHITE,
        ..desired
    };
    primary
        .apply(changed, |appearance| {
            primary_color = appearance.background;
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(primary_color, Color::WHITE);
    assert_eq!(secondary_color, desired.background);
}
