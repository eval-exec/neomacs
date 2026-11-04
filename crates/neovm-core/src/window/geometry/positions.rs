//! Private numeric position storage for an immutable presented window.
//!
//! Deferred publications retain only shared point-row descriptors and numeric
//! mappings. Their OnceLock publishes one complete vector to concurrent readers;
//! cloning shares the immutable source and cache. No live window or Lisp value is
//! retained. Equality and Debug intentionally materialize the original vector.
//!
//! | Knob | Default | Gate |
//! | --- | --- | --- |
//! | `NEOMACS_PRESENT_GEOMETRY_LAZY=off\|on` | on | Defer the private presented position vector for shared point rows. |
//!
//! The mode is read once per process. Publications and their numeric caches may
//! be cloned and read concurrently; every caller observes the same completed
//! vector and no cache state participates in semantic equality.
