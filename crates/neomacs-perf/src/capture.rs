use crate::Frontend;

/// Where native profiling or hardware-counter capture must be attached.
///
/// TUI and hermetic GUI workloads have an adapter process which launches the
/// editor, while batch and physical-display workloads launch the editor
/// directly. Keeping that distinction typed prevents a new frontend path from
/// silently configuring an adapter hook that does not exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CaptureRoute {
    Direct,
    Adapter(&'static str),
}

impl CaptureRoute {
    pub(crate) const fn for_frontend(frontend: Frontend, uses_native_display: bool) -> Self {
        match frontend {
            Frontend::Batch => Self::Direct,
            Frontend::Tui { .. } => Self::Adapter("PTY"),
            Frontend::Gui { .. } if uses_native_display => Self::Direct,
            Frontend::Gui { .. } => Self::Adapter("GUI"),
        }
    }

    /// Role of the process whose exit status the harness observes.
    pub(crate) const fn process_role(self) -> &'static str {
        match self {
            Self::Direct => "workload process",
            Self::Adapter(_) => "adapter",
        }
    }
}

#[cfg(test)]
mod tests;
