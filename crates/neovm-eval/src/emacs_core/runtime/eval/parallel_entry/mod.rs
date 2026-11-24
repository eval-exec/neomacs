//! Dormant production-entry types. E1 installs no execution path or authority.
pub(crate) mod capability;
pub(crate) mod driver_core;
pub(crate) mod keeper_custody;
pub(crate) mod keeper_driver;
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
pub(crate) mod world_turn;
