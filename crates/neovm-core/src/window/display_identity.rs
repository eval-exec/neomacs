use neomacs_display_protocol::GraphicalDisplayIdentity;

/// Non-graphical frames have no graphical connection. GUI frames share the
/// validated identity used to initialize their terminal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FrameDisplayIdentity {
    #[default]
    None,
    Graphical(GraphicalDisplayIdentity),
}

impl FrameDisplayIdentity {
    #[cfg(test)]
    pub fn wayland(display: impl Into<String>) -> Self {
        Self::Graphical(
            GraphicalDisplayIdentity::named(
                neomacs_display_protocol::GraphicalBackend::Wayland,
                display,
            )
            .expect("valid Wayland display name"),
        )
    }

    pub fn native_display(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Graphical(identity) => Some(identity.terminal_name()),
        }
    }

    pub fn x_display(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Graphical(identity) => identity.x_display(),
        }
    }
}
