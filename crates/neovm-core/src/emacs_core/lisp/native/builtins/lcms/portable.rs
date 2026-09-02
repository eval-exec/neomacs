//! Color-management subrs for hosts without native shared libraries.

#[path = "subrs.rs"]
mod subrs;

#[cfg(test)]
pub(crate) use self::subrs::SUBRS;
pub(crate) use self::subrs::register_subrs;
