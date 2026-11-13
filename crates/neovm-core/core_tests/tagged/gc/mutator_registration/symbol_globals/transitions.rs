//! Small, deterministic fixtures also suitable for Miri (no timed stress).
use super::tests::{fixture_case, oracle};
use super::*;
use crate::tagged::gc::{PlainSymbolBacking, RegistrationError};
use crate::tagged::value::TaggedValue as Value;
use neovm_value::test_support::authority::{
    owner_root_slot_with_index_for_test, plant_scalar_root_for_test,
};

fn immediate(run: &mut RunAccess<'_, '_>, root: &OwnerRootSlot<'_>, value: Value) {
    assert!(value.is_fixnum() || value.is_nil() || value.is_t());
    root.check(run).unwrap();
    // Assertion-only planting of same-heap scalar input; no raw heap pointer.
    // SAFETY: `value` is an immediate from this owner's heap and `root` names
    // one of its rooted homes; the owner is exclusively borrowed by `run`.
    unsafe { plant_scalar_root_for_test(run, root, value) };
}

#[test]
fn p3sym_c3_nil_void_and_first_projected_assignment() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
        PlainSymbolBacking::VoidProjection,
    ] {
        fixture_case(kind, |run, handle, src, dst, id| {
            run.sym_boundp_into(handle, dst).unwrap();
            assert_eq!(
                dst.word_for_test(run),
                Value::bool_val(kind != PlainSymbolBacking::VoidProjection).bits()
            );
            run.sym_makunbound_into(handle, dst).unwrap();
            assert_eq!(dst.word_for_test(run), Value::from_sym_id(id).bits());
            let before = dst.word_for_test(run);
            assert_eq!(
                run.sym_value_into(handle, dst),
                Err(GlobalError::VoidVariable)
            );
            assert_eq!(
                dst.word_for_test(run),
                before,
                "void read leaves destination rooted"
            );
            run.sym_boundp_into(handle, dst).unwrap();
            assert_eq!(dst.word_for_test(run), Value::NIL.bits());
            let before_issue = run.owner_gc_snapshot_for_test().bytes_since_gc;
            immediate(run, src, Value::NIL);
            run.sym_set_into(handle, src, dst).unwrap();
            assert_eq!(dst.word_for_test(run), Value::NIL.bits());
            if kind == PlainSymbolBacking::VoidProjection {
                // Serial VOID setup reserved a page and issued-list capacity.
                // Materialization reuses them, so no new storage is charged.
                assert_eq!(
                    run.owner_gc_snapshot_for_test().bytes_since_gc,
                    before_issue,
                    "first VOID assignment reuses already reserved storage"
                );
            }
            run.sym_boundp_into(handle, dst).unwrap();
            assert_eq!(dst.word_for_test(run), Value::T.bits());
            run.sym_value_into(handle, dst).unwrap();
            assert_eq!(dst.word_for_test(run), Value::NIL.bits());
            immediate(run, src, Value::fixnum(101));
            run.sym_set_into(handle, src, dst).unwrap();
            run.sym_value_into(handle, dst).unwrap();
            assert_eq!(dst.word_for_test(run), Value::fixnum(101).bits());
        })
        .unwrap();
    }
}

#[test]
fn p3sym_c3_aliased_destination_and_retained_set_result() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        fixture_case(kind, |run, handle, src, dst, _| {
            immediate(run, src, Value::fixnum(11));
            run.sym_set_into(handle, src, dst).unwrap();
            let alias = dst.clone();
            run.sym_set_into(handle, &alias, dst).unwrap();
            assert_eq!(dst.word_for_test(run), Value::fixnum(11).bits());
            immediate(run, src, Value::fixnum(22));
            let mut scratch = src.clone();
            run.sym_set_into(handle, src, &mut scratch).unwrap();
            run.sym_value_into(handle, &mut scratch).unwrap();
            let result = format!(
                "({} {})",
                unsafe { Value::from_bits(dst.word_for_test(run)) }
                    .as_fixnum()
                    .unwrap(),
                unsafe { Value::from_bits(scratch.word_for_test(run)) }
                    .as_fixnum()
                    .unwrap()
            );
            assert_eq!(result, oracle()[9]);
        })
        .unwrap();
    }
}

#[test]
fn p3sym_c3_invalid_and_cleared_homes_refuse_before_access() {
    fixture_case(
        PlainSymbolBacking::Unprojected,
        |run, handle, src, dst, _| {
            let mut invalid = owner_root_slot_with_index_for_test(dst, usize::MAX);
            assert_eq!(
                run.sym_value_into(handle, &mut invalid),
                Err(GlobalError::Authority(RegistrationError::StaleRegistration))
            );
            // SAFETY: UNBOUND is an immediate; `src` names this owner's home
            // and the owner is exclusively borrowed by `run`.
            unsafe { plant_scalar_root_for_test(run, src, Value::UNBOUND) };
            assert_eq!(
                run.sym_set_into(handle, src, dst),
                Err(GlobalError::Authority(RegistrationError::Publication))
            );
            run.sym_value_into(handle, dst).unwrap();
            assert_eq!(dst.word_for_test(run), Value::fixnum(73).bits());
            run.clear_root_slots().unwrap();
            assert_eq!(
                run.sym_set_into(handle, src, dst),
                Err(GlobalError::Authority(RegistrationError::StaleRegistration))
            );
            assert_eq!(
                run.sym_boundp_into(handle, dst),
                Err(GlobalError::Authority(RegistrationError::StaleRegistration))
            );
            assert_eq!(
                run.sym_makunbound_into(handle, dst),
                Err(GlobalError::Authority(RegistrationError::StaleRegistration))
            );
        },
    )
    .unwrap();
}
