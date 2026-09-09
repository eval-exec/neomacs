//! Graphical connection identity shared by platform startup, terminals, and frames.
//!
//! A display label is not a unique ID. In particular an inherited Wayland
//! connection has a stable backend label, not a fabricated socket pathname.

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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicalDisplayIdentity {
    backend: GraphicalBackend,
    name: DisplayName,
}

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
            backend,
            name: DisplayName::Connection(name),
        })
    }

    pub fn anonymous_connection(backend: GraphicalBackend) -> Self {
        Self {
            backend,
            name: DisplayName::BackendLabel,
        }
    }

    pub fn backend(&self) -> GraphicalBackend {
        self.backend
    }
    pub fn terminal_name(&self) -> &str {
        match &self.name {
            DisplayName::Connection(name) => name,
            DisplayName::BackendLabel => self.backend.into(),
        }
    }
    pub fn x_display(&self) -> Option<&str> {
        match self.backend {
            GraphicalBackend::X11 => match &self.name {
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
#[path = "display_identity_test.rs"]
mod tests;
