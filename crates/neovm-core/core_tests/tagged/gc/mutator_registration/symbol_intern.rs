//! Symbol-intern tests re-homed out of value. They name the evaluator's
//! symbol-name era. Assertions are unchanged; table, registry, and owner
//! censuses go through copied observers. InternOp stays in value.
// The parent file is mounted with #[path], so child modules resolve next to
// this file rather than in a symbol_intern/ directory.
// The facade re-exports the value registration types. Child modules glob this
// parent, so the import has to live here for `RunAccess` and `OwnerRootSlot`.
use super::*;
#[path = "symbol_intern/fixture.rs"]
mod fixture;
#[path = "symbol_intern/tests/publication.rs"]
mod publication;
#[path = "symbol_intern/tests/table_failures.rs"]
mod table_failures;
#[cfg(not(feature = "gc-memory-telemetry"))]
#[path = "symbol_intern/tests.rs"]
mod tests;
