//! Product API retarget onto `serial_gate`. Every case runs on the keeper
//! thread and prints `kind=PRODUCT-SINGLE-THREAD`. These checks are not the
//! TSan gate. The second-thread cases are `serial_gate_threads`.
//! `WorldProofEra` is `!Send + !Sync`, so `stop` stays on the keeper.

#![forbid(unsafe_code)]

use neovm_value::tagged::gc::RendezvousError;

use super::serial_gate::{
    SerialEndError, end_serial_for_test, lend_serial_for_test, serial_gc_for_test, stop_for_test,
};

fn on_keeper(body: impl for<'heap> FnOnce(&crate::emacs_core::eval::world::WorldProofEra<'heap>)) {
    let _harness = crate::test_utils::unit_harness();
    crate::emacs_core::eval::Context::with_test_symbol_functions_init_era(
        neovm_value::tagged::gc::PlainSymbolBacking::Unprojected,
        |_, _| {},
        |era, _ids| body(&era),
    );
}

/// Keeper thread, ticket out: the second stop is the existing `Busy`.
#[test]
fn self_stop_vs_main() {
    on_keeper(|era| {
        let owner = era.keeper_for_test().expect("fixture keeper");
        let stopped = stop_for_test(era).expect("first stop on the keeper thread");
        let mut ticket = lend_serial_for_test(era, owner, stopped).expect("lend after stop");
        assert!(matches!(stop_for_test(era), Err(RendezvousError::Busy)));
        serial_gc_for_test(&mut ticket).expect("release the stop");
        end_serial_for_test(ticket).expect("end after the serial collection");
        eprintln!("PENTRY_TSAN case=self_stop_vs_main kind=PRODUCT-SINGLE-THREAD");
    });
}

/// An early end returns that same ticket. No second lend.
#[test]
fn nested_serial_errors() {
    on_keeper(|era| {
        let owner = era.keeper_for_test().expect("fixture keeper");
        let stopped = stop_for_test(era).expect("stop");
        let ticket = lend_serial_for_test(era, owner, stopped).expect("one ticket");
        let retained = end_serial_for_test(ticket).expect_err("untraced end returns the ticket");
        assert_eq!(retained.reason, SerialEndError::NotCollected);
        let mut ticket = retained.ticket;
        serial_gc_for_test(&mut ticket).expect("the returned ticket still collects");
        assert!(matches!(
            serial_gc_for_test(&mut ticket),
            Err(RendezvousError::Busy)
        ));
        end_serial_for_test(ticket).expect("end the same ticket");
        eprintln!("PENTRY_TSAN case=nested_serial_errors kind=PRODUCT-SINGLE-THREAD");
    });
}

/// I5 refusal. End before collection keeps the ticket and leaves stop closed.
/// Roots are not read after that decision.
#[test]
fn retained_loan_is_refused() {
    on_keeper(|era| {
        let owner = era.keeper_for_test().expect("fixture keeper");
        let stopped = stop_for_test(era).expect("stop");
        let ticket = lend_serial_for_test(era, owner, stopped).expect("lend");
        let retained = end_serial_for_test(ticket).expect_err("retained loan refuses reopen");
        assert_eq!(retained.reason, SerialEndError::NotCollected);
        assert!(matches!(stop_for_test(era), Err(RendezvousError::Busy)));
        let _held = retained.ticket;
        eprintln!(
            "PENTRY_TSAN case=retained_loan_is_refused kind=PRODUCT-SINGLE-THREAD refused=true"
        );
    });
}

/// Release path: collect, return the ticket, then stop may run again.
#[test]
fn retired_loan_reopens() {
    on_keeper(|era| {
        let owner = era.keeper_for_test().expect("fixture keeper");
        let stopped = stop_for_test(era).expect("stop");
        let mut ticket = lend_serial_for_test(era, owner, stopped).expect("lend");
        serial_gc_for_test(&mut ticket).expect("serial collection");
        end_serial_for_test(ticket).expect("end after the serial collection");
        stop_for_test(era).expect("reopen after the ticket returns");
        eprintln!("PENTRY_TSAN case=retired_loan_reopens kind=PRODUCT-SINGLE-THREAD refused=false");
    });
}
