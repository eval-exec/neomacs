//! Native initial-font policy, separate from font realization.
//!
//! No native settings handles cross into the evaluator or rendering protocol.

use neomacs_display_protocol::GraphicalBackend;

mod policy;
pub use policy::{
    GuiFontDefaults, InitialFontCandidate, InitialFontFamilyMatch, InitialFontSize,
    WindowsFontFallback,
};

cfg_select! {
    target_os = "linux" => { mod linux; }
    target_os = "macos" => { mod macos; }
    _ => {}
}

/// Capture owned defaults on the native startup thread. The display backend,
/// not environment variables, determines which policy will select the font.
pub fn read_font_defaults(backend: GraphicalBackend) -> GuiFontDefaults {
    cfg_select! {
        target_os = "linux" => {
            match backend {
                GraphicalBackend::X11 | GraphicalBackend::Wayland => {
                    GuiFontDefaults::Desktop(linux::read_system_fonts())
                }
                _ => GuiFontDefaults::for_backend(backend),
            }
        }
        target_os = "macos" => {
            match backend {
                GraphicalBackend::Cocoa => GuiFontDefaults::Cocoa {
                    fixed_pitch: macos::fixed_pitch_font(),
                },
                _ => GuiFontDefaults::for_backend(backend),
            }
        }
        _ => { GuiFontDefaults::for_backend(backend) }
    }
}
