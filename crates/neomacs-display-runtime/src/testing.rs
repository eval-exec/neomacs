//! Recorded evaluator output and seam values for tests that span the
//! evaluator and this runtime (DECISIONS D44).
//!
//! A test whose two halves meet at a protocol value is split there: this
//! crate's half, which reads private runtime items, consumes the values
//! below; the display host's contract tests pin the evaluator's half to the
//! same values (`neomacs-display-host`, `src/tests/`). Neither half can drift
//! from the other without one of them failing.

pub mod key_transport;
pub mod raw_bytes;
pub mod real_layout;
