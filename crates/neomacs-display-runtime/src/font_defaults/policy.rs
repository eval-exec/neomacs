//! Platform defaults are not desktop preferences or explicit user overrides.

use neomacs_display_protocol::GraphicalBackend;
use neovm_core::emacs_core::display_host::{
    FrameFontSize, SystemFontName, SystemFontRole, SystemFonts,
};
use strum::IntoEnumIterator;

/// Native facts captured before the evaluator starts. No native handles or
/// Lisp values cross threads. Windows cannot accidentally expose a fallback
/// font as a GSettings-style system-font preference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GuiFontDefaults {
    Desktop(SystemFonts),
    Cocoa { fixed_pitch: Option<SystemFontName> },
    Windows,
    Portable,
}

impl GuiFontDefaults {
    pub fn for_backend(backend: GraphicalBackend) -> Self {
        match backend {
            GraphicalBackend::X11 | GraphicalBackend::Wayland => {
                Self::Desktop(SystemFonts::default())
            }
            GraphicalBackend::Cocoa => Self::Cocoa { fixed_pitch: None },
            GraphicalBackend::Windows => Self::Windows,
            GraphicalBackend::Android | GraphicalBackend::Web => Self::Portable,
        }
    }

    pub fn system_fonts(&self) -> SystemFonts {
        match self {
            Self::Desktop(fonts) => fonts.clone(),
            Self::Cocoa { .. } | Self::Windows | Self::Portable => SystemFonts::default(),
        }
    }

    /// Open the first usable candidate through the native font catalog.
    /// Callers cannot reorder platform defaults or forget the last resort.
    /// Selection happens before Lisp configuration, never during redisplay.
    pub fn select<T>(&self, open: impl FnMut(InitialFontCandidate) -> Option<T>) -> Option<T> {
        self.candidates().into_iter().find_map(open)
    }

    fn candidates(&self) -> Vec<InitialFontCandidate> {
        let mut candidates = Vec::new();
        match self {
            Self::Desktop(fonts) => {
                if let Some(name) = fonts.get(SystemFontRole::Monospace) {
                    candidates.push(InitialFontCandidate::Desktop(name.clone()));
                }
            }
            Self::Cocoa { fixed_pitch } => {
                if let Some(name) = fixed_pitch {
                    candidates.push(InitialFontCandidate::Cocoa(name.clone()));
                }
            }
            Self::Windows => {
                candidates.extend(WindowsFontFallback::iter().map(InitialFontCandidate::Windows))
            }
            Self::Portable => {}
        }
        // Unlike GNU's fatal all-fonts-missing path, retain Neomacs's generic
        // last resort if none of the platform's named fonts can be opened.
        candidates.push(InitialFontCandidate::Monospace(match self {
            Self::Cocoa { .. } => InitialFontSize::CocoaBackendDefault,
            Self::Desktop(_) | Self::Windows | Self::Portable => InitialFontSize::MonospaceFallback,
        }));
        candidates
    }
}

/// Keep the Core Text backend default distinct from a font's explicit size.
/// The native zero-size sentinel never becomes an invalid positive font size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialFontSize {
    NamedFontDefault,
    CocoaBackendDefault,
    MonospaceFallback,
}

impl InitialFontSize {
    pub fn font_size(self) -> FrameFontSize {
        let points = match self {
            Self::NamedFontDefault | Self::CocoaBackendDefault => 12.0,
            Self::MonospaceFallback => 10.0,
        };
        FrameFontSize::points(points)
            .expect("platform defaults are positive, representable point sizes")
    }
}

/// GNU w32_default_font_parameter's ordered fallback chain. Enum iteration
/// keeps the policy exhaustive without stringly typed platform branching.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumIter, strum::AsRefStr)]
pub enum WindowsFontFallback {
    #[strum(serialize = "Courier New-10")]
    CourierNew,
    #[strum(serialize = "-*-Courier-normal-r-*-*-13-*-*-*-c-*-iso8859-1")]
    Courier,
    #[strum(serialize = "-*-Fixedsys-normal-r-*-*-12-*-*-*-c-*-iso8859-1")]
    FixedsysPixels,
    #[strum(serialize = "Fixedsys")]
    Fixedsys,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InitialFontCandidate {
    Desktop(SystemFontName),
    Cocoa(SystemFontName),
    Windows(WindowsFontFallback),
    Monospace(InitialFontSize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialFontFamilyMatch {
    /// Missing a named family advances the ordered platform fallback list.
    RequireNamedFamily,
    /// Native aliases/substitution may satisfy a desktop or generic request.
    NativeSubstitution,
}

impl InitialFontCandidate {
    pub fn family_match(&self) -> InitialFontFamilyMatch {
        match self {
            Self::Windows(_) => InitialFontFamilyMatch::RequireNamedFamily,
            Self::Desktop(_) | Self::Cocoa(_) | Self::Monospace(_) => {
                InitialFontFamilyMatch::NativeSubstitution
            }
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Desktop(name) | Self::Cocoa(name) => name.as_str(),
            Self::Windows(font) => font.as_ref(),
            Self::Monospace(_) => "monospace",
        }
    }

    pub fn default_size(&self) -> InitialFontSize {
        match self {
            Self::Desktop(_) | Self::Windows(_) => InitialFontSize::NamedFontDefault,
            Self::Cocoa(_) => InitialFontSize::CocoaBackendDefault,
            Self::Monospace(size) => *size,
        }
    }
}
