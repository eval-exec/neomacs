//! Strict combination divergence tests, isolated to bound release-build memory.
#![allow(non_snake_case)]

use crate::common;
#[path = "../src/divergence/combos/strict/mod.rs"]
mod divergence_combo_strict;
