//! Coding oracle parity tests.

mod advanced;
mod alias_conversion_hooks;
mod basic;
mod detection_state;
mod eol_decode_matrix;
mod find_coding_systems;
mod metadata;
mod null_detection;
mod sjis_big5_char;
mod string;
mod string_advanced;
mod system_comprehensive;
mod system_put_advanced;
mod textprop_strict_edge_semantics;
mod unibyte_destination;
mod utf8_test_availability_semantics;

#[cfg(test)]
#[path = "tests/prepared_mule.rs"]
mod prepared_mule;
