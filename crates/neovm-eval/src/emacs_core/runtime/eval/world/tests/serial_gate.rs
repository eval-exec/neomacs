//! cfg(test) keeper surface for the E2 safety gate (#7712).
//!
//! The four names sit beside `Context::with_test_symbol_functions_init_era`.
//! There is no production constructor and no `contract-fixtures` re-export.
//! The product `Send` serial ticket still waits on keeper commits 30–37.
//! This ticket holds the real `AllStopped` proof, so it stays on the keeper
//! thread. A second `stop` while it is out is the existing
//! `RendezvousError::Busy` from `Phase::Tracing`. `serial_gc_for_test` reads
//! the current attached roots and the same proof's one trace refuses another
//! keeper collection.

#![forbid(unsafe_code)]

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

use neovm_value::tagged::gc::{AllStopped, CollectionReport, RegistrationError, RendezvousError};

use super::{Baton, ParallelHeapEraWorldCollectExt, WorldProofEra};

/// Serial loan minted only from an `AllStopped` proof. No public constructor.
pub(crate) struct SerialTicket<'era, 'heap> {
    era: &'era WorldProofEra<'heap>,
    owner: &'era Baton,
    stopped: AllStopped<'era>,
    collecting: Cell<bool>,
    collected: Cell<bool>,
    symbol_roots: Vec<usize>,
    projection_roots: Vec<usize>,
    _thread: PhantomData<Rc<()>>,
}

/// `end_serial_for_test` could not finish the epoch. The ticket is unchanged.
pub(crate) struct RetainedSerialFailure<'era, 'heap> {
    pub(crate) reason: RendezvousError,
    pub(crate) ticket: SerialTicket<'era, 'heap>,
}

impl<'era, 'heap> SerialTicket<'era, 'heap> {
    /// Roots copied at `serial_gc_for_test`, not at lend time.
    pub(crate) fn attached_roots(&self) -> (&[usize], &[usize]) {
        (&self.symbol_roots, &self.projection_roots)
    }
}

impl std::fmt::Debug for SerialTicket<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SerialTicket")
            .field("collecting", &self.collecting.get())
            .field("collected", &self.collected.get())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for RetainedSerialFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetainedSerialFailure")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

static_assertions::assert_not_impl_any!(SerialTicket<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(
    RetainedSerialFailure<'static, 'static>: Send,
    Sync,
    Clone,
    Copy
);

/// Forward to the era's `stop`. Self-stop on the keeper thread is the existing
/// `RendezvousError::Busy`. This adds no error type.
pub(crate) fn stop_for_test<'era, 'heap>(
    era: &'era WorldProofEra<'heap>,
) -> Result<AllStopped<'era>, RendezvousError> {
    era.stop()
}

/// Mint a ticket only after `AllStopped`. `owner` must be this era's keeper,
/// held on the calling thread. The stopped proof moves into the ticket.
pub(crate) fn lend_serial_for_test<'era, 'heap>(
    era: &'era WorldProofEra<'heap>,
    owner: &'era Baton,
    stopped: AllStopped<'era>,
) -> Result<SerialTicket<'era, 'heap>, RendezvousError> {
    if !owner.held_here() {
        return Err(RegistrationError::WrongThread.into());
    }
    stopped.check_world_roots(owner.parallel_heap_identity())?;
    let keeper = era.keeper_for_test().ok_or(RendezvousError::Publication)?;
    if !std::ptr::eq(keeper, owner) {
        return Err(RendezvousError::Identity);
    }
    Ok(SerialTicket {
        era,
        owner,
        stopped,
        collecting: Cell::new(false),
        collected: Cell::new(false),
        symbol_roots: Vec::new(),
        projection_roots: Vec::new(),
        _thread: PhantomData,
    })
}

/// Read the current attached roots, then run the one stopped collection.
/// A second call, or a call re-entered while this one is running, is `Busy`.
pub(crate) fn serial_gc_for_test(
    ticket: &mut SerialTicket<'_, '_>,
) -> Result<CollectionReport, RendezvousError> {
    if ticket.collecting.get() || ticket.collected.get() {
        return Err(RendezvousError::Busy);
    }
    ticket.collecting.set(true);
    let owner = ticket.owner;
    let era = ticket.era;
    let report = (|| {
        let (symbols, projections) = owner.symbol_root_ledger_for_test(&mut ticket.stopped)?;
        let report = ParallelHeapEraWorldCollectExt::collect_stopped(era, &mut ticket.stopped)?;
        ticket.symbol_roots = symbols;
        ticket.projection_roots = projections;
        Ok(report)
    })();
    ticket.collecting.set(false);
    match report {
        Ok(report) => {
            ticket.collected.set(true);
            Ok(report)
        }
        Err(error) => Err(error),
    }
}

/// Finish the stopped epoch after the serial collection. Before that
/// collection, or if completion fails, the ticket comes back unresolved.
pub(crate) fn end_serial_for_test<'era, 'heap>(
    ticket: SerialTicket<'era, 'heap>,
) -> Result<(), RetainedSerialFailure<'era, 'heap>> {
    if ticket.collecting.get() {
        return Err(RetainedSerialFailure {
            reason: RendezvousError::Busy,
            ticket,
        });
    }
    if !ticket.collected.get() {
        return Err(RetainedSerialFailure {
            reason: RendezvousError::Publication,
            ticket,
        });
    }
    let SerialTicket {
        era,
        owner,
        stopped,
        collecting,
        collected,
        symbol_roots,
        projection_roots,
        _thread,
    } = ticket;
    match stopped.complete() {
        Ok(()) => Ok(()),
        Err(failure) => {
            let reason = failure.reason;
            Err(RetainedSerialFailure {
                reason,
                ticket: SerialTicket {
                    era,
                    owner,
                    stopped: failure.into_stopped(),
                    collecting,
                    collected,
                    symbol_roots,
                    projection_roots,
                    _thread,
                },
            })
        }
    }
}

#[cfg(not(feature = "gc-memory-telemetry"))]
#[test]
fn serial_gate_reads_current_roots_and_refuses_a_second_collection() {
    let _harness = crate::test_utils::unit_harness();
    crate::emacs_core::eval::Context::with_test_symbol_functions_init_era(
        neovm_value::tagged::gc::PlainSymbolBacking::Unprojected,
        |_, _| {},
        |era, _ids| {
            let owner = era.keeper_for_test().expect("fixture keeper");
            let stopped = stop_for_test(&era).expect("empty membership stops");
            let mut ticket = lend_serial_for_test(&era, owner, stopped).expect("lend after stop");
            assert!(
                ticket.attached_roots().0.is_empty() && ticket.attached_roots().1.is_empty(),
                "roots are read at serial_gc, not at lend"
            );
            assert!(matches!(stop_for_test(&era), Err(RendezvousError::Busy)));
            let retained = end_serial_for_test(ticket).expect_err("untraced end retains");
            assert_eq!(retained.reason, RendezvousError::Publication);
            ticket = retained.ticket;
            let report = serial_gc_for_test(&mut ticket).expect("serial collection");
            assert!(report.major);
            assert!(
                !ticket.attached_roots().0.is_empty(),
                "current symbol roots are attached at serial_gc"
            );
            assert!(matches!(
                serial_gc_for_test(&mut ticket),
                Err(RendezvousError::Busy)
            ));
            end_serial_for_test(ticket).expect("end after the serial collection");
        },
    );
}
