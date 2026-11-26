//! Small Miri entry for the real core numeric-storage era regression.
//!
//! Reuse the existing core fixture and shell bodies. This target avoids
//! enumerating the full core lib-test tree; it adds no fixture authority.
#![cfg(miri)]

pub use neovm_core::emacs_core;

mod test_utils {
    /// The same one-forward canonical boot as core's unit-test helper.
    pub fn new_context() -> neovm_core::Context {
        neovm_core::emacs_core::eval_hook_installers::new_context()
    }
}

// Restore only the historical facade paths the unchanged shell imports.
mod tagged {
    pub use neovm_value::tagged::{transport, value};

    pub mod gc {
        pub mod mutator_registration {
            pub(crate) use crate::era_fixtures;
        }
    }
}

#[path = "../core_tests/tagged/gc/mutator_registration/cons_allocation_tests.rs"]
mod cons_allocation_tests;
#[path = "../core_tests/tagged/gc/mutator_registration/era_fixtures.rs"]
mod era_fixtures;
