//! Graphical connection identity shared by platform startup, terminals, and frames.
//!
//! A display label is not a unique ID. In particular an inherited Wayland
//! connection has a stable backend label, not a fabricated socket pathname.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumIter, strum::IntoStaticStr)]
pub enum GraphicalBackend {
    #[strum(serialize = "wayland")]
    Wayland,
    #[strum(serialize = "x11")]
    X11,
    #[strum(serialize = "ns")]
    Cocoa,
    #[strum(serialize = "w32")]
    Windows,
    #[strum(serialize = "android")]
    Android,
    #[strum(serialize = "web")]
    Web,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidDisplayName {
    Empty,
    ContainsNul,
    BootstrapName,
}

/// Cannot represent an unnamed graphical terminal or the bootstrap terminal.
/// Fields are private and there is deliberately no `Default` implementation.
#[derive(Clone, Debug)]
pub struct GraphicalDisplayIdentity {
    connection: Arc<GraphicalConnection>,
}

/// Opened-font observations belong to a native connection, shared by its
/// frames and font services. Standalone font services have no connection.
/// Keeping this state behind the existing identity handle adds no evaluator
/// field or static TLS storage.
#[derive(Debug)]
struct GraphicalConnection {
    backend: GraphicalBackend,
    name: DisplayName,
    /// Zero means no font has opened yet; the public pre-open minimum is one.
    /// A live graphical connection retains its opened font caches and frame
    /// fonts. Ordinary cache invalidation must not raise this minimum: GNU
    /// font_close_object decrements n_fonts without recomputing it.
    smallest_opened_font_height: AtomicU32,
}

impl PartialEq for GraphicalDisplayIdentity {
    fn eq(&self, other: &Self) -> bool {
        // Preserve label equality. Equal labels need not be the same native
        // connection and must never be used to share mutable font state.
        self.connection.backend == other.connection.backend
            && self.connection.name == other.connection.name
    }
}

impl Eq for GraphicalDisplayIdentity {}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DisplayName {
    Connection(String),
    BackendLabel,
}

impl GraphicalDisplayIdentity {
    pub fn named(
        backend: GraphicalBackend,
        name: impl Into<String>,
    ) -> Result<Self, InvalidDisplayName> {
        let name = name.into();
        if name.is_empty() {
            return Err(InvalidDisplayName::Empty);
        }
        if name.contains('\0') {
            return Err(InvalidDisplayName::ContainsNul);
        }
        if name == "initial_terminal" {
            return Err(InvalidDisplayName::BootstrapName);
        }
        Ok(Self {
            connection: Arc::new(GraphicalConnection {
                backend,
                name: DisplayName::Connection(name),
                smallest_opened_font_height: AtomicU32::new(0),
            }),
        })
    }

    pub fn anonymous_connection(backend: GraphicalBackend) -> Self {
        Self {
            connection: Arc::new(GraphicalConnection {
                backend,
                name: DisplayName::BackendLabel,
                smallest_opened_font_height: AtomicU32::new(0),
            }),
        }
    }

    /// GNU's display-wide smallest opened font ascent plus descent.
    ///
    /// The fallback before the first successful open is one, as initialized
    /// by the graphical backends. Layout must capture this numeric value in
    /// its input/freshness keys rather than compare mutable identities.
    pub fn smallest_opened_font_height(&self) -> u32 {
        self.connection
            .smallest_opened_font_height
            .load(Ordering::Relaxed)
            .max(1)
    }

    /// Record metrics of an actual successfully opened font, after the
    /// backend's ascent/descent normalization. Projected frame line heights
    /// and failed/degraded metric guesses are not opened-font observations.
    ///
    /// GNU font_open_entity (font.c:2994-3012) installs the first height
    /// outright, then only lowers it. The return value reports that input
    /// change; existing frame retry/freshness checks consume the numeric
    /// minimum without introducing a Lisp observer.
    pub fn observe_opened_font_metrics(&self, ascent: i32, descent: i32) -> bool {
        let height = (i64::from(ascent) + i64::from(descent)).clamp(1, i64::from(i32::MAX)) as u32;
        self.connection
            .smallest_opened_font_height
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |previous| {
                if previous == 0 || height < previous {
                    Some(height)
                } else {
                    None
                }
            })
            .is_ok()
    }

    pub fn backend(&self) -> GraphicalBackend {
        self.connection.backend
    }
    pub fn terminal_name(&self) -> &str {
        match &self.connection.name {
            DisplayName::Connection(name) => name,
            DisplayName::BackendLabel => self.connection.backend.into(),
        }
    }
    pub fn x_display(&self) -> Option<&str> {
        match self.connection.backend {
            GraphicalBackend::X11 => match &self.connection.name {
                DisplayName::Connection(name) => Some(name),
                DisplayName::BackendLabel => None,
            },
            GraphicalBackend::Wayland
            | GraphicalBackend::Cocoa
            | GraphicalBackend::Windows
            | GraphicalBackend::Android
            | GraphicalBackend::Web => None,
        }
    }
}

#[cfg(test)]
#[path = "display_identity/tests/display_identity_test.rs"]
mod tests;
