//! GNU nsfns.m's native fixed-pitch default, without a hard-coded family.

use neovm_core::emacs_core::display_host::SystemFontName;
use objc2::MainThreadMarker;
use objc2::rc::autoreleasepool;
use objc2_app_kit::NSFont;

pub(super) fn fixed_pitch_font() -> Option<SystemFontName> {
    // Keep AppKit discovery on native startup, never on the evaluator thread.
    let _main_thread = MainThreadMarker::new()?;
    autoreleasepool(|_| {
        let font = NSFont::userFixedPitchFontOfSize(-1.0)?;
        let name = font.displayName()?.to_string();
        // Match GNU: use the display name, but do not import AppKit's returned
        // point size. Cocoa's font opening policy supplies the backend default.
        SystemFontName::new(name.strip_suffix(" Regular").unwrap_or(&name).to_owned())
    })
}
