//! Native desktop preference discovery, separate from font realization.
//!
//! No native settings handles cross into the evaluator or rendering protocol.

use neovm_core::emacs_core::display_host::SystemFonts;

cfg_select! {
    target_os = "linux" => { mod linux; }
    _ => {}
}

/// Read effective preferences before selecting a GUI frame's initial font.
/// Platforms without a discovery adapter retain their existing font fallback.
pub fn read_system_fonts() -> SystemFonts {
    cfg_select! {
        target_os = "linux" => { linux::read_system_fonts() }
        _ => { SystemFonts::default() }
    }
}
