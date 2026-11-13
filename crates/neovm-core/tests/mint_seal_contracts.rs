//! Compile-fail contracts for the split's mint seals (peer seal N8, N9;
//! PEER-RULINGS.md 2026-11-02/03).
//!
//! Today the sealed items are `pub(crate)`: naming them from this external
//! contract crate fails as *private*. When the neovm-value split promotes
//! them to `pub`, the same fixtures flip to the intended first error — a
//! SAFE call of an `unsafe` constructor or installer fails with E0133 — and
//! the goldens are re-blessed in the same commit. A fixture that starts to
//! COMPILE at any point is a sealing bug, never a golden to re-bless.

#[test]
fn mint_seals_are_not_callable_from_outside_the_crate() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/mint_seal_ui/*.rs");
}
