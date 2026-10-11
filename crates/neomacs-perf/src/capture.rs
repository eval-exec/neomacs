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
            // The GUI frontend launches the editor directly out of the
            // harness process since the bench display session moved into
            // neomacs-infra (neomacs-infra::display::WestonBenchSession), so
            // perf wraps only the editor -- the compositor is a harness-owned
            // sibling process that was never part of the measured tree.
            Frontend::Gui { .. } if uses_native_display => Self::Direct,
            Frontend::Gui { .. } => Self::Direct,
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
#[path = "capture/tests/capture_test.rs"]
mod tests;
