//! Display test-support surface: the signed authority fence module.
//!
//! Every item here has one AUTHORITY-ITEMS.tsv row; the module is compiled
//! only under `cfg(any(test, feature = "test-support"))`.

pub mod authority;
