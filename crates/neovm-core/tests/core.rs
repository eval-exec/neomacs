//! Core integration suites in one test binary.
//!
//! Each former per-file binary linked neovm-core and Cranelift again.
//! nextest runs every test in its own process, so one binary is equivalent
//! at run time. Tests that need a custom harness or a process-wide global
//! allocator stay separate targets in Cargo.toml.

mod common;

#[path = "dump_environment.rs"]
mod dump_environment;

#[path = "compat_all.rs"]
mod compat_all;

#[path = "aot_call_bearing.rs"]
mod aot_call_bearing;

#[path = "aot_pgo.rs"]
mod aot_pgo;

#[path = "aot_spec.rs"]
mod aot_spec;

#[path = "fringe_standard_indicators.rs"]
mod fringe_standard_indicators;

#[path = "gdm_casing.rs"]
mod gdm_casing;

#[path = "gdm_columns.rs"]
mod gdm_columns;

#[path = "gdm_region.rs"]
mod gdm_region;

#[path = "gdm_widths.rs"]
mod gdm_widths;

#[path = "p2ht_contracts.rs"]
mod p2ht_contracts;

#[path = "pixel_measurement.rs"]
mod pixel_measurement;

#[path = "coding_safety.rs"]
mod coding_safety;

#[path = "coding_detection.rs"]
mod coding_detection;

#[path = "font_coverage.rs"]
mod font_coverage;
