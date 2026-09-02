//! Host-neutral frontend lifecycle state machine.

/// Stable application frontend lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleState {
    /// No host window/surface has been created yet.
    Dormant,
    /// The host frontend may own windows and render surfaces.
    Active,
    /// The host temporarily withdrew its native presentation surface.
    Suspended,
    /// The frontend is shutting down and cannot be resumed.
    Exiting,
}

/// Lifecycle observation delivered by a platform adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleEvent {
    /// The platform is ready to present editor windows.
    Resumed,
    /// Native presentation resources must be released temporarily.
    Suspended,
    /// The frontend event loop is terminating.
    ExitRequested,
}

/// Observable work produced by one lifecycle transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleAction {
    /// The event is idempotent in the current state.
    None,
    /// Create the host window and its presentation resources.
    CreateFrontend,
    /// Release host window and presentation resources.
    DestroyFrontend,
    /// Exit the platform event loop.
    Exit,
}

/// Lifecycle coordinator shared by native and browser adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrontendLifecycle {
    state: LifecycleState,
}

impl Default for FrontendLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl FrontendLifecycle {
    /// Create a lifecycle before the platform has resumed its frontend.
    pub const fn new() -> Self {
        Self {
            state: LifecycleState::Dormant,
        }
    }

    /// Current lifecycle state.
    pub const fn state(self) -> LifecycleState {
        self.state
    }

    /// Consume one platform observation and return the required host action.
    pub const fn transition(&mut self, event: LifecycleEvent) -> LifecycleAction {
        match (self.state, event) {
            (LifecycleState::Exiting, _) => LifecycleAction::None,
            (_, LifecycleEvent::ExitRequested) => {
                self.state = LifecycleState::Exiting;
                LifecycleAction::Exit
            }
            (LifecycleState::Dormant | LifecycleState::Suspended, LifecycleEvent::Resumed) => {
                self.state = LifecycleState::Active;
                LifecycleAction::CreateFrontend
            }
            (LifecycleState::Active, LifecycleEvent::Suspended) => {
                self.state = LifecycleState::Suspended;
                LifecycleAction::DestroyFrontend
            }
            _ => LifecycleAction::None,
        }
    }
}
