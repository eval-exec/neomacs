//! Hash oracle parity tests.

mod equal_lookup_bounded_semantics;
#[cfg(test)]
#[path = "tests/maphash_bytecode.rs"]
mod maphash_bytecode;
mod table;
mod table_advanced;
mod table_comprehensive_patterns;
mod table_contains_semantics;
mod table_deep_edge_semantics;
mod table_extended;
mod table_mutate_strict_edge_semantics;
mod table_operations_comprehensive;
mod table_operations_extended;
mod table_patterns;
mod table_strict_edge_semantics;

#[cfg(test)]
#[path = "tests/hash_callback_mutation.rs"]
mod hash_callback_mutation;
