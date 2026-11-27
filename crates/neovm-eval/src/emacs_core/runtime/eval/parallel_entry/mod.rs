//! Dormant production-entry types. E1 installs no execution path or authority.
pub(crate) mod between_commands;
pub(crate) mod capability;
pub(crate) mod driver_core;
pub(crate) mod keeper_custody;
pub(crate) mod keeper_driver;
pub(crate) mod live_admission;
pub(crate) mod owned_source;
mod quit_requester;
pub(crate) mod serial_detach;
pub(crate) mod serial_park;
pub(crate) mod serial_ticket;
pub(crate) mod serial_turn;
pub(crate) mod source_entry;
pub(crate) mod source_jit_backend;
pub(crate) mod source_jit_cache;
pub(crate) mod source_jit_compiler;
mod sync;
mod text_caches;
pub(crate) mod thread_control;
pub(crate) mod world_seat;
pub(crate) mod world_turn;

#[cfg(test)]
#[path = "../tests/e53a_thread_control_test.rs"]
mod e53a_thread_control_tests;
#[cfg(test)]
#[path = "../tests/e53b_thread_join_retire_test.rs"]
mod e53b_thread_join_retire_tests;
#[cfg(test)]
#[path = "../tests/e54a_mutex_condvar_test.rs"]
mod e54a_mutex_condvar_tests;
#[cfg(test)]
#[path = "../tests/green_era_switch_test.rs"]
mod green_era_switch_tests;
#[cfg(test)]
#[path = "tests/legacy_jit_withdrawal_test.rs"]
mod legacy_jit_withdrawal_tests;
#[cfg(test)]
#[path = "../tests/owner_slab_lease_test.rs"]
mod owner_slab_lease_tests;
