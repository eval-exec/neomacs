use std::num::NonZeroU32;
use std::ptr::NonNull;

use raw_window_handle::{RawWindowHandle, WaylandWindowHandle, XcbWindowHandle, XlibWindowHandle};

use super::*;

fn kwin_reproduction_hints() -> GuiFrameGeometryHints {
    GuiFrameGeometryHints {
        base_width: 34,
        base_height: 89,
        min_width: 34,
        min_height: 89,
        width_inc: 9,
        height_inc: 24,
    }
}

fn admits_client_size(hints: GuiFrameGeometryHints, width: u32, height: u32) -> bool {
    width >= hints.min_width
        && height >= hints.min_height
        && width >= hints.base_width
        && height >= hints.base_height
        && (width - hints.base_width) % hints.width_inc == 0
        && (height - hints.base_height) % hints.height_inc == 0
}

#[test]
fn x11_native_hints_admit_the_entire_maximized_workarea() {
    let original = kwin_reproduction_hints();

    // KWin on a 1280x800 screen with a 36px decoration allocates 1280x764.
    // The old 9x24 character hints instead admit 1276x761, leaving 4px/3px gaps.
    assert!(admits_client_size(original, 1276, 761));
    assert!(!admits_client_size(original, 1280, 764));

    for handle in [
        RawWindowHandle::Xlib(XlibWindowHandle::new(1)),
        RawWindowHandle::Xcb(XcbWindowHandle::new(NonZeroU32::new(1).unwrap())),
    ] {
        let policy = NativeResizeGrid::for_handle(handle);
        assert_eq!(policy, NativeResizeGrid::Pixels);
        let native = policy.project(original);

        assert_eq!(
            native,
            GuiFrameGeometryHints {
                base_width: 34,
                base_height: 89,
                min_width: 34,
                min_height: 89,
                width_inc: 1,
                height_inc: 1,
            }
        );
        for (width, height) in [(1276, 761), (1280, 764), (1280, 800), (1279, 763)] {
            assert!(
                admits_client_size(native, width, height),
                "{handle:?} must admit native allocation {width}x{height}"
            );
        }
    }

    // Native projection must not turn the editor's requested cell grid into pixels.
    assert_eq!(original.width_inc, 9);
    assert_eq!(original.height_inc, 24);
}

#[test]
fn pixelwise_native_hints_preserve_distinct_base_and_minimum_sizes() {
    let editor = GuiFrameGeometryHints {
        base_width: 34,
        base_height: 89,
        min_width: 106,
        min_height: 137,
        width_inc: 9,
        height_inc: 24,
    };
    assert_eq!(
        NativeResizeGrid::Pixels.project(editor),
        GuiFrameGeometryHints {
            base_width: 34,
            base_height: 89,
            min_width: 106,
            min_height: 137,
            width_inc: 1,
            height_inc: 1,
        }
    );
}

#[test]
fn wayland_preserves_character_increments_for_state_aware_native_sizing() {
    // This synthetic handle is used only to classify the backend; it is never dereferenced.
    let handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(NonNull::dangling()));
    let policy = NativeResizeGrid::for_handle(handle);
    assert_eq!(policy, NativeResizeGrid::Characters);
    let editor = kwin_reproduction_hints();
    assert_eq!(policy.project(editor), editor);
}
