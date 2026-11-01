//! Dormant contracts for owner-selected native host effects.
//!
//! Primitive dispatch stays unchanged. Native effects terminate in existing
//! projection owners; hosted activation and routed implementations are later
//! stages. No owner provenance or host service can be constructed here.

pub mod contracts;

pub(crate) mod wake;

pub(crate) mod native_outcome;
pub(crate) mod native_pending;
