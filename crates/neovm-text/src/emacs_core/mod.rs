//! Stable `emacs_core` module roots moved from the core crate.

#[path = "text/casefiddle/mod.rs"]
pub mod casefiddle;
#[path = "text/casetab/mod.rs"]
pub mod casetab;
#[path = "text/category/mod.rs"]
pub mod category;
#[path = "text/ccl/mod.rs"]
pub mod ccl;
#[path = "text/character/mod.rs"]
pub mod character;
#[path = "text/charset/mod.rs"]
pub mod charset;
#[path = "text/chartable/mod.rs"]
pub mod chartable;
#[path = "text/coding/mod.rs"]
pub mod coding;
#[path = "text/composite/mod.rs"]
pub mod composite;
#[path = "text/format/mod.rs"]
pub mod format;
#[path = "text/json/mod.rs"]
pub mod json;
#[path = "text/regex/mod.rs"]
pub mod regex;
#[path = "text/regex/emacs.rs"]
pub mod regex_emacs;
#[path = "text/search/mod.rs"]
pub mod search;
#[path = "text/string_escape/mod.rs"]
pub mod string_escape;
#[path = "text/substrate_hooks.rs"]
pub mod substrate_hooks;
#[path = "text/syntax/mod.rs"]
pub mod syntax;
#[path = "text/host/mod.rs"]
pub mod text_host;
#[path = "text/treesit/mod.rs"]
pub mod treesit;
#[path = "text/xml/mod.rs"]
pub mod xml;
#[path = "text/zlib/mod.rs"]
pub mod zlib;

#[cfg(test)]
#[path = "text/syntax/tests/category_property_test.rs"]
mod syntax_category_property_test;

#[cfg(test)]
#[path = "text/syntax/tests/gnu_parity_regression_test.rs"]
mod syntax_gnu_parity_regression_test;

#[cfg(test)]
#[path = "text/string_pos_cache/tests/string_pos_cache_test.rs"]
mod string_pos_cache_test;

#[cfg(test)]
#[path = "text/string_pos_cache/tests/gc_tls_ownership_test.rs"]
mod string_pos_cache_gc_ownership_test;

#[cfg(test)]
#[path = "text/string_pos_cache/tests/gc_collection_epoch_test.rs"]
mod string_pos_cache_gc_epoch_test;

#[cfg(test)]
#[path = "text/string_pos_cache/tests/gc_collection_epoch_minor_test.rs"]
mod string_pos_cache_gc_epoch_minor_test;

#[cfg(test)]
#[path = "text/string_pos_cache/tests/signed_string_positions.rs"]
mod string_pos_cache_signed_positions_test;
