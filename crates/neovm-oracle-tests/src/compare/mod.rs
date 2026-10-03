//! Compare oracle parity tests.

mod strings;
mod strings_advanced;
mod strings_comprehensive;

#[cfg(test)]
#[path = "tests/position_cache.rs"]
mod position_cache;

#[cfg(test)]
#[path = "tests/case_table.rs"]
mod case_table;
