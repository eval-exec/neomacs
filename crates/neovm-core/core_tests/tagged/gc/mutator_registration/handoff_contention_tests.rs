//! Handoff resume after the real control gate.
//! The Context shell is eval's symbol-backing fence. The owner body and its
//! assertions stay in value.
#![cfg(not(miri))]

use super::PlainSymbolBacking;
use neovm_eval::test_support::authority::with_test_symbol_backing_era;
use neovm_value::test_support::authority::{
    handoff_resume_outlasts_yield_budget_for_test, handoff_resume_uncontended_for_test,
};

#[test]
fn handoff_resume_uncontended_control() {
    crate::test_utils::init_test_tracing();
    with_test_symbol_backing_era(PlainSymbolBacking::Projected, |era, _| {
        handoff_resume_uncontended_for_test(era);
    });
}

#[test]
fn handoff_resume_outlasts_yield_budget() {
    crate::test_utils::init_test_tracing();
    with_test_symbol_backing_era(PlainSymbolBacking::Projected, |era, _| {
        handoff_resume_outlasts_yield_budget_for_test(era);
    });
}
