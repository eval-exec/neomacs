//! Facade: PATH_EXEC resolution moved to
//! `neovm_value::system_vocab::path_exec` with the stage-4 W1/E3 TYPE-DOWN
//! move.  Historical paths keep resolving.

pub use neovm_value::system_vocab::path_exec::*;

#[cfg(test)]
#[path = "tests/path_exec_test.rs"]
mod tests;
