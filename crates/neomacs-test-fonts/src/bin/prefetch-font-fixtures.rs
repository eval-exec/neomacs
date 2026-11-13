//! `cargo xtask font-fixtures` entry point.
//!
//! Fills the shared font-fixture cache. A failed pin aborts with
//! `FIXTURE-FETCH:`.

fn main() {
    neomacs_test_fonts::prefetch_all_fixtures();
}
