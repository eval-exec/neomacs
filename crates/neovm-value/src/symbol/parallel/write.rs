//! Frozen-symbol writes under a sealed per-World writer. No legacy TLS.
use super::*;
use crate::tagged::gc::{RegistrationError, SymbolWriteToken};
use crate::value::Value;

impl SymbolBacking {
    /// Return storage growth even on fallible reservation refusal. No reference
    /// or descriptor leaves the backing bridge, and no operation can poll.
    ///
    /// # Safety
    /// The token's exclusive word must belong to this retained backing's World.
    /// The era excludes legacy views, markers and structural symbol changes.
    pub(crate) unsafe fn write_plain(
        &self,
        id: SymId,
        value: Value,
        authority: &SymbolWriteToken<'_>,
    ) -> (Result<(), RegistrationError>, usize) {
        let result = unsafe { self.write_existing(id, value, authority) };
        match result {
            Ok(grown) => (Ok(()), grown),
            Err((error, grown)) => (Err(error), grown),
        }
    }

    unsafe fn write_existing(
        &self,
        id: SymId,
        value: Value,
        authority: &SymbolWriteToken<'_>,
    ) -> Result<usize, (RegistrationError, usize)> {
        let refused = (RegistrationError::Publication, 0);
        let obarray = unsafe { self.obarray.as_ref() };
        if obarray.generation() != self.generation {
            return Err(refused);
        }
        let symbol = obarray.get_by_id(id).ok_or(refused)?;
        if symbol.trapped_write() != SymbolTrappedWrite::Untrapped {
            return Err(refused);
        }
        match symbol.value_cell() {
            ValueCell::Plain(_) if !symbol.flags().runtime_projected() => symbol
                .parallel_store_plain(value, authority)
                .then_some(0)
                .ok_or(refused),
            ValueCell::Forwarded(fwd) if symbol.flags().runtime_projected() => {
                let projection = fwd.thread_projection().ok_or(refused)?;
                if projection.arm() != CanonicalArm::Plain || projection.host_projected() {
                    return Err(refused);
                }
                if !projection.is_void() {
                    return projection
                        .parallel_store_plain(value, authority)
                        .then_some(0)
                        .ok_or(refused);
                }
                if value.is_unbound() {
                    return Ok(0);
                }
                // Exclusive writer only; readers never touch this RefCell.
                let mut table = unsafe { self.projections.as_ref() }
                    .try_borrow_mut()
                    .map_err(|_| (RegistrationError::Busy, 0))?;
                if !table
                    .projection(fwd)
                    .is_some_and(|p| std::ptr::eq(p, projection))
                {
                    return Err(refused);
                }
                let before = table.parallel_storage_bytes(authority);
                let result = table
                    .issue_plain_parallel(value, authority)
                    .map_err(|_| RegistrationError::Capacity)
                    .and_then(|next| {
                        symbol
                            .parallel_replace_projection(fwd, next.header(), authority)
                            .then_some(())
                            .ok_or(RegistrationError::Poisoned)
                    });
                let grown = table
                    .parallel_storage_bytes(authority)
                    .saturating_sub(before);
                result.map(|()| grown).map_err(|error| (error, grown))
            }
            _ => Err(refused),
        }
    }
}
