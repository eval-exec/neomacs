//! Display's local test-helper hub.
//!
//! TEST-SUPPORT-DESIGN rev 2 rule 8: every crate's test tree re-exports the
//! shared, upper-free `neovm_value::test_utils` helpers instead of copying
//! them. This keeps the historical `crate::test_utils::init_test_tracing()`
//! call sites in display's own tests resolving without a core edge (an upward
//! dependency is forbidden). Compiled only for display's tests and for an
//! upper crate's test-support build; absent from every product profile.

pub use neovm_value::test_utils::*;
