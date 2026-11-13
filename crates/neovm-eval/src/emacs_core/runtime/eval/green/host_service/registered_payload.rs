//! Lossless record positions backed by the existing traced root batch.
//!
//! Immediates need no registration. Every other position names a word in the
//! batch's stable traced subsequence; no raw heap value is retained here.

use super::HostRootError;
use neovm_value::tagged::transport::{ImmediateValue, NotImmediate};
use neovm_value::tagged::value::TaggedValue;

#[derive(Debug, Clone, Copy)]
struct RegisteredRootIndex(usize);

#[derive(Debug)]
struct RegisteredRootCount(usize);

#[derive(Debug)]
enum RegisteredValueSlot {
    Immediate(ImmediateValue),
    Traced(RegisteredRootIndex),
}

#[derive(Debug)]
pub(super) struct RegisteredPayload {
    slots: Box<[RegisteredValueSlot]>,
    traced_count: RegisteredRootCount,
}

impl RegisteredPayload {
    /// Capture positions while the original owner's live values are admitted
    /// to the matching batch. Valid input values and their traced subsequence
    /// are supplied by the registry's existing registration contract.
    #[doc(hidden)]
    pub fn new(values: &[TaggedValue]) -> Self {
        let mut traced_count = 0;
        let slots = values
            .iter()
            .copied()
            .map(|value| match ImmediateValue::try_from(value) {
                Ok(immediate) => RegisteredValueSlot::Immediate(immediate),
                Err(NotImmediate::HeapObject | NotImmediate::SymbolNeedsRoot) => {
                    let index = RegisteredRootIndex(traced_count);
                    traced_count += 1;
                    RegisteredValueSlot::Traced(index)
                }
            })
            .collect();
        Self {
            slots,
            traced_count: RegisteredRootCount(traced_count),
        }
    }

    #[doc(hidden)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Fill already reserved owner-local storage while the batch reader is
    /// live. The caller roots this storage before ending that reader.
    pub(crate) fn copy_into(
        &self,
        traced: &[TaggedValue],
        destination: &mut [TaggedValue],
    ) -> Result<(), HostRootError> {
        if destination.len() != self.len() || traced.len() != self.traced_count.0 {
            return Err(HostRootError::ForeignRecord);
        }
        for (slot, destination) in self.slots.iter().zip(destination) {
            *destination = match slot {
                RegisteredValueSlot::Immediate(immediate) => immediate.value(),
                RegisteredValueSlot::Traced(index) => traced
                    .get(index.0)
                    .copied()
                    .ok_or(HostRootError::ForeignRecord)?,
            };
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/registered_payload.rs"]
mod tests;
