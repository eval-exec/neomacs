//! Production wake owner and its real tests; only deadline vocabulary is shimmed.
#![allow(dead_code)]

mod contracts {
    use std::time::Instant;

    /// Mirrors the existing owned deadline DTO, not a wait implementation.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ReadDeadline {
        Immediate,
        At(Instant),
        Unbounded,
    }
}

// The source's own cfg(test) modules remain at their production paths. No
// epoch, registration, lease, session, socket or failure logic is duplicated.
#[path = "../../../src/host_effect/wake.rs"]
mod wake;
