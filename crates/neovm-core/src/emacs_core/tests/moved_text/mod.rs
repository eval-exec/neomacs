//! Scope mirror for re-homed tests (lead ruling 02:1xZ option A').
//! Real module files (not inline mods) so #[path] is relative to the
//! scope file, and `super`/`super::super` resolve against the lower
//! crate modules via `pub use`.

pub mod emacs_core;
pub mod encoding;
