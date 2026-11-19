//! Compile-fail contracts for the split's mint seals (peer seal N8, N9;
//! PEER-RULINGS.md 2026-11-02/03).
//!
//! Today the sealed items are `pub(crate)`: naming them from this external
//! contract crate fails as *private*. When the neovm-value split promotes
//! them to `pub`, the same fixtures flip to the intended first error — a
//! SAFE call of an `unsafe` constructor or installer fails with E0133 — and
//! the goldens are re-blessed in the same commit. A fixture that starts to
//! COMPILE at any point is a sealing bug, never a golden to re-bless.

/// Named `*_compile_contracts` so the nextest overrides for trybuild
/// runners (compiler memory wrapper, all test threads, 900s slow-timeout)
/// apply to it as they do to every other contract binary.
#[test]
fn mint_seal_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/mint_seal_ui/*.rs");
}
