//! Window point-row data facade (stage-4 W2/E5 FM-6a).
//!
//! The row/iterator records and their data algorithms moved to
//! emacs_core::eval::data::point_rows; this module re-exports them at the
//! old `window::point_rows::*` paths and keeps the test modules.

pub use neovm_eval::emacs_core::eval::data::point_rows::{
    DisplayPointRow, DisplayPointRowIter, DisplayPointRows, DisplayPointRowsIter,
    DisplayPointRowsMode, PointCell, display_point_rows_mode,
};

#[cfg(test)]
#[path = "tests/point_rows_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/point_rows_iterator_test.rs"]
mod iterator_tests;

#[cfg(test)]
#[path = "tests/point_rows_scratch_test.rs"]
mod scratch_tests;
