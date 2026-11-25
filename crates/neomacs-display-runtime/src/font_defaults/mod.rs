//! Native initial-font policy, separate from font realization.
//!
//! No native settings handles cross into the evaluator or rendering protocol.

use neomacs_display_protocol::SystemFonts;
use neomacs_display_protocol::{Dpi, GraphicalBackend};

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
    initial_text_dpi: Option<Dpi>,
    changes: crossbeam_channel::Receiver<SystemFonts>,
    #[cfg(target_os = "linux")]
    initial_raster_options: neomacs_font_materializer::fontconfig::SurfaceFontOptions,
    #[cfg(target_os = "linux")]
    raster_changes:
        crossbeam_channel::Receiver<neomacs_font_materializer::fontconfig::SurfaceFontOptions>,
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
            initial_text_dpi: None,
            changes: crossbeam_channel::never(),
            #[cfg(target_os = "linux")]
            initial_raster_options: Default::default(),
            #[cfg(target_os = "linux")]
            raster_changes: crossbeam_channel::never(),
            _subscription: NativeSubscription::Unsupported,
        }
    }

    pub fn initial(&self) -> &GuiFontDefaults {
        &self.initial
    }

    /// Startup logical text DPI, before compositor backing scale is applied.
    /// Font-name subscriptions remain independent of this initial observation.
    pub fn initial_text_dpi(&self) -> Option<Dpi> {
        self.initial_text_dpi
    }

    /// The input bridge is the sole consumer. Unsupported subscriptions never
    /// become ready, so selecting this receiver cannot spin on disconnection.
    pub fn take_changes(&mut self) -> crossbeam_channel::Receiver<SystemFonts> {
        std::mem::replace(&mut self.changes, crossbeam_channel::never())
    }

    /// PGTK's startup font-rendering preferences. Only the Wayland owner applies
    /// these; X11 takes its surface policy from X resources instead.
    #[cfg(target_os = "linux")]
    pub fn initial_raster_options(
        &self,
    ) -> neomacs_font_materializer::fontconfig::SurfaceFontOptions {
        self.initial_raster_options
    }

    /// One owned observation per native changed signal, independent of the
    /// font-name channel. Equal effective options still require a Lisp event.
    #[cfg(target_os = "linux")]
    pub fn take_raster_changes(
        &mut self,
    ) -> crossbeam_channel::Receiver<neomacs_font_materializer::fontconfig::SurfaceFontOptions>
    {
        std::mem::replace(&mut self.raster_changes, crossbeam_channel::never())
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
