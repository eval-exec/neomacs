//! Facade: the C-feature table moved to
//! `neovm_value::system_vocab::c_features` with the stage-4 W1/E3 TYPE-DOWN
//! move.  Historical paths keep resolving; the surface test stays in core.

pub use neovm_value::system_vocab::c_features::*;
