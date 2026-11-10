#![forbid(unsafe_code)]

pub(crate) use super::fixture_helpers::*;
pub(crate) use super::rooted_tests;
use super::*;

// Libval's mapping rows stay here until their re-home installs the
// admission-probe runner on the test thread (#1580).
#[path = "mapping_tests.rs"]
mod mapping;
