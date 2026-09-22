//! Scenario implementations: each module owns one workload family's
//! preparation, result schema, invariants, and measurements.

pub(crate) mod bounded_search;
pub(crate) mod builtin_call;
pub(crate) mod bytecode;
pub(crate) mod editor_workload;
pub(crate) mod elisp_benchmarks;
pub(crate) mod mx_tab;
pub(crate) mod org_journal_open;
pub(crate) mod rust_lsp;
pub(crate) mod search_shape;
pub(crate) mod sustained_native_video;
pub(crate) mod vm_loop;
