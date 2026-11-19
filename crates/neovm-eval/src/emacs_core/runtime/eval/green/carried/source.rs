//! Local source effects after the census and observation withdrawal.
//! Native payloads stay on this thread. No payload destructor restores a slot.

use super::{Any, CarrierId, Placeholder, SOURCE_DECLARATIONS, Swap};
use std::mem::ManuallyDrop;
use std::panic::{AssertUnwindSafe, catch_unwind};

enum SourceRoute {
    WithdrawLocal,
    SeparateObservations,
}

impl CarrierId {
    // A new declaration generates an enum variant and must be classified here.
    fn source_route(self) -> SourceRoute {
        match self {
            Self::Observations => SourceRoute::SeparateObservations,
            Self::Composition
            | Self::Processes
            | Self::Sqlite
            | Self::Dbus
            | Self::Selection
            | Self::Lossage
            | Self::Undo
            | Self::Terminal
            | Self::AllocationCounts
            | Self::PropertyRevision
            | Self::DisplayEpochs
            | Self::TimeZone => SourceRoute::WithdrawLocal,
        }
    }
}

pub(super) struct CarrierDeclaration {
    pub(super) id: CarrierId,
    pub(super) placeholder: Placeholder,
    pub(super) swap: Swap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PieceStage {
    Prepared,
    Swapping,
    Withdrawn,
}

struct Piece {
    declaration: &'static CarrierDeclaration,
    payload: ManuallyDrop<Box<dyn Any>>,
    stage: PieceStage,
}

struct WithdrawingCarriers {
    pieces: Vec<Piece>,
}

impl std::fmt::Debug for WithdrawingCarriers {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never format native or Lisp-bearing payloads.
        formatter
            .debug_list()
            .entries(
                self.pieces
                    .iter()
                    .map(|piece| (piece.declaration.id, piece.stage)),
            )
            .finish()
    }
}

#[must_use]
pub(crate) enum SourceCarrierFailure {
    HeapInstalled,
    SwapPanicked(RetainedCarrierFailure),
}

#[must_use]
pub(crate) struct RetainedCarrierFailure {
    withdrawal: WithdrawingCarriers,
    _panic: ManuallyDrop<Box<dyn Any + Send>>,
}

impl std::fmt::Debug for SourceCarrierFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HeapInstalled => formatter.write_str("HeapInstalled"),
            Self::SwapPanicked(failure) => formatter
                .debug_tuple("SwapPanicked")
                .field(&failure.withdrawal)
                .finish(),
        }
    }
}

/// Complete local withdrawal of the twelve non-observation carriers.
/// The caller keeps the original source and the observation receipt.
#[must_use]
pub(crate) struct SourceWorldCarriers {
    withdrawal: WithdrawingCarriers,
}

impl std::fmt::Debug for SourceWorldCarriers {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("SourceWorldCarriers")
            .field(&self.withdrawal)
            .finish()
    }
}

static_assertions::assert_not_impl_any!(SourceWorldCarriers: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(SourceCarrierFailure: Send, Sync, Clone, Copy);

impl SourceWorldCarriers {
    #[cold]
    #[inline(never)]
    pub(crate) fn withdraw(
        _observations: &neovm_value::tagged::collection_reads::SourceObservationsDetached,
    ) -> Result<Self, SourceCarrierFailure> {
        if neovm_value::tagged::gc::tagged_heap_is_installed() {
            return Err(SourceCarrierFailure::HeapInstalled);
        }
        // Arm every placeholder before the first swap. The callbacks allocate
        // native containers only. Managed roots were captured before this.
        let pieces = SOURCE_DECLARATIONS
            .iter()
            .filter_map(|declaration| match declaration.id.source_route() {
                SourceRoute::SeparateObservations => None,
                SourceRoute::WithdrawLocal => Some(Piece {
                    declaration,
                    payload: ManuallyDrop::new((declaration.placeholder)()),
                    stage: PieceStage::Prepared,
                }),
            })
            .collect();
        let mut withdrawal = WithdrawingCarriers { pieces };
        for piece in &mut withdrawal.pieces {
            piece.stage = PieceStage::Swapping;
            let result = catch_unwind(AssertUnwindSafe(|| {
                (piece.declaration.swap)(&mut **piece.payload);
            }));
            if let Err(panic) = result {
                // A callback may have changed some of its own slots. Own the
                // partial payload, every preceding payload, and the panic.
                return Err(SourceCarrierFailure::SwapPanicked(RetainedCarrierFailure {
                    withdrawal,
                    _panic: ManuallyDrop::new(panic),
                }));
            }
            piece.stage = PieceStage::Withdrawn;
        }
        Ok(Self { withdrawal })
    }
}

#[cfg(test)]
#[path = "tests/source_carriers_test.rs"]
mod tests;
