//! Facade: the errno boundary moved to `neovm_value::system_vocab::errno`
//! with the stage-4 W1/E3 TYPE-DOWN move.  Historical paths keep resolving.

pub use neovm_value::system_vocab::errno::*;

#[cfg(test)]
#[path = "tests/errno_test.rs"]
mod tests;
