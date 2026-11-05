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
#[path = "text/string_pos_cache/tests/string_pos_cache_test.rs"]
mod string_pos_cache_test;
