//! Atomic initialization of bootstrap, TTY, and graphical terminals.

use neomacs_display_protocol::{
    GraphicalDisplayIdentity, tty_capabilities::TtyAttributeCapabilities,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalRuntimeConfig {
    Bootstrap,
    Tty(TtyTerminalConfig),
    Graphical(GraphicalDisplayIdentity),
}

/// TTY-only configuration methods cannot be called on graphical initialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtyTerminalConfig {
    pub(super) name: Option<String>,
    pub(super) tty_type: Option<String>,
    pub(super) attribute_capabilities: TtyAttributeCapabilities,
}

impl TerminalRuntimeConfig {
    pub fn inactive() -> Self {
        Self::Bootstrap
    }

    pub fn interactive(
        tty_type: Option<String>,
        attribute_capabilities: TtyAttributeCapabilities,
    ) -> TtyTerminalConfig {
        TtyTerminalConfig {
            name: None,
            tty_type,
            attribute_capabilities,
        }
    }

    pub fn window_system(identity: GraphicalDisplayIdentity) -> Self {
        Self::Graphical(identity)
    }
}

impl TtyTerminalConfig {
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_attribute_capabilities(mut self, capabilities: TtyAttributeCapabilities) -> Self {
        self.attribute_capabilities = capabilities;
        self
    }
}

impl From<TtyTerminalConfig> for TerminalRuntimeConfig {
    fn from(config: TtyTerminalConfig) -> Self {
        Self::Tty(config)
    }
}
