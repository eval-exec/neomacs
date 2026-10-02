//! Line oracle parity tests.

mod edit_helper_semantics;
mod number_misc_strict_edge_semantics;
mod position_advanced;
mod selective_display_count;
#[cfg(test)]
mod text_index_parity;

#[cfg(test)]
#[path = "tests/text_index_conversion_parity.rs"]
mod text_index_conversion_parity;
