//! System vocabulary moved down from core for the stage-4 eval split (W1/E3).
//!
//! Pure std data and helpers with no evaluator, display, or host authority:
//! the errno boundary, platform facts, the C-feature table, path_exec, the
//! `sleep-for` wait vocabulary, the reader/command-loop input vocabulary, the
//! pure file-name conversions, the Windows process/file identity query and
//! startup constants.  Core keeps every historical module path as a re-export
//! in `neovm-core`.

pub mod c_features;
pub mod errno;
pub mod file_names;
pub mod input;
pub mod path_exec;
pub mod platform;
pub mod platform_facts;
pub mod time;
#[cfg(windows)]
pub mod w32_security;
