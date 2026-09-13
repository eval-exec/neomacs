//! Native initial-font policy, separate from font realization.
//!
//! No native settings handles cross into the evaluator or rendering protocol.

use neomacs_display_protocol::GraphicalBackend;
use neovm_core::emacs_core::display_host::SystemFonts;

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

/// Startup preferences and their subscription have one owner. The evaluator
/// retains this observer until shutdown; only owned observations enter input.
pub struct FontDefaultsObserver {
    initial: GuiFontDefaults,
    changes: crossbeam_channel::Receiver<SystemFonts>,
    _subscription: NativeSubscription,
}

enum NativeSubscription {
    Unsupported,
    #[cfg(target_os = "linux")]
    Linux {
        _guard: linux::Subscription,
    },
}

impl FontDefaultsObserver {
    fn unsupported(initial: GuiFontDefaults) -> Self {
        Self {
            initial,
            changes: crossbeam_channel::never(),
            _subscription: NativeSubscription::Unsupported,
        }
    }

    pub fn initial(&self) -> &GuiFontDefaults {
        &self.initial
    }

    /// The input bridge is the sole consumer. Unsupported subscriptions never
    /// become ready, so selecting this receiver cannot spin on disconnection.
    pub fn take_changes(&mut self) -> crossbeam_channel::Receiver<SystemFonts> {
        std::mem::replace(&mut self.changes, crossbeam_channel::never())
    }
}

/// Capture preferences before opening fonts. AppKit discovery stays on the
/// calling main thread; Linux owns discovery and monitoring on one GIO thread.
pub fn observe_font_defaults(backend: GraphicalBackend) -> std::io::Result<FontDefaultsObserver> {
    cfg_select! {
        target_os = "linux" => {
            match backend {
                GraphicalBackend::X11 | GraphicalBackend::Wayland => {
                    linux::observe()
                }
                _ => Ok(FontDefaultsObserver::unsupported(GuiFontDefaults::for_backend(backend))),
            }
        }
        target_os = "macos" => {
            Ok(FontDefaultsObserver::unsupported(match backend {
                GraphicalBackend::Cocoa => GuiFontDefaults::Cocoa {
                    fixed_pitch: macos::fixed_pitch_font(),
                },
                _ => GuiFontDefaults::for_backend(backend),
            }))
        }
        _ => { Ok(FontDefaultsObserver::unsupported(GuiFontDefaults::for_backend(backend))) }
    }
}
