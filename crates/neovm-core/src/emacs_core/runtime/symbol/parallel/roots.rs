//! Checked keyless keeper census. No serial heap/Context/policy TLS is read.
use super::*;
use crate::emacs_core::symbol::roots::SymbolRootSet;
use crate::emacs_core::value::Value;
use crate::tagged::gc::{KeeperSymbols, RegistrationError};

impl SymbolBacking {
    /// Compare original designators without dereferencing either manager.
    /// The caller obtains both from its retained Baton, not a Context view.
    pub(crate) fn check_root_designators(
        &self,
        obarray: NonNull<Obarray>,
        projections: NonNull<RefCell<ThreadProjectionTable>>,
    ) -> Result<(), RegistrationError> {
        if self.obarray != obarray || self.projections != projections {
            return Err(RegistrationError::StaleRegistration);
        }
        Ok(())
    }

    fn check_root_generation(&self, obarray: &Obarray) -> Result<(), RegistrationError> {
        if obarray.generation() != self.generation {
            return Err(RegistrationError::StaleRegistration);
        }
        Ok(())
    }

    /// # Safety
    /// The keeper exclusively retains its real stop and the original World.
    /// All identity/authority checks precede the first manager dereference;
    /// live generation is checked before any symbol cell or pool is read.
    unsafe fn keeper_obarray(
        &self,
        authority: &KeeperSymbols<'_>,
        keeper: &KeeperWorldDesignator<'_>,
    ) -> Result<&Obarray, RegistrationError> {
        authority.check_symbol_backing(self, keeper)?;
        // SAFETY: original allocation designator, matching retained Baton,
        // exclusive stopped keeper borrow and zero symbol authority word.
        let obarray = unsafe { self.obarray.as_ref() };
        self.check_root_generation(obarray)?;
        Ok(obarray)
    }

    /// # Safety
    /// Matching live AllStopped is held by authority; no reference may escape.
    pub(crate) unsafe fn append_current_symbol_roots(
        &self,
        authority: &KeeperSymbols<'_>,
        keeper: &KeeperWorldDesignator<'_>,
        out: &mut Vec<Value>,
    ) -> Result<(), RegistrationError> {
        // SAFETY: caller retains the stopped keeper; the leaf rechecks its
        // original backing, World roots, live generation and zero word.
        let obarray = unsafe { self.keeper_obarray(authority, keeper)? };
        obarray.append_symbol_roots(SymbolRootSet::Complete, out);
        Ok(())
    }

    /// # Safety
    /// Matching stopped authority covers the entire projection-table loan.
    pub(crate) unsafe fn append_current_projection_roots(
        &self,
        authority: &KeeperSymbols<'_>,
        keeper: &KeeperWorldDesignator<'_>,
        out: &mut Vec<Value>,
    ) -> Result<(), RegistrationError> {
        // SAFETY: same retained stop/World and no concurrent writer; generation
        // refusal precedes even borrowing the canonical descriptor table.
        unsafe { self.keeper_obarray(authority, keeper)? };
        // SAFETY: original World designator checked above; no table loan or
        // descriptor escapes this exclusive keeper census.
        let projections = unsafe { self.projections.as_ref() }
            .try_borrow()
            .map_err(|_| RegistrationError::Busy)?;
        projections.trace_roots(&mut |value| out.push(value));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn p3sym_c6_keeper_generation_leaf_refuses_stale_capture() {
        let obarray = Box::new(std::cell::UnsafeCell::new(Obarray::new()));
        let projections = RefCell::new(ThreadProjectionTable::default());
        // SAFETY: sole serial test owner retains both original roots until
        // after the backing and its short shared generation check are gone.
        let mut backing = unsafe {
            SymbolBacking::new(
                NonNull::new(obarray.get()).unwrap(),
                NonNull::from(&projections),
                1,
            )
        };
        // SAFETY: original live allocation designator; no exclusive aggregate
        // reborrow overlaps this shared, scalar-only generation observation.
        let current = unsafe { &*obarray.get() };
        assert_eq!(backing.check_root_generation(current), Ok(()));
        backing.generation = backing.generation.checked_add(1).unwrap();
        assert_eq!(
            backing.check_root_generation(current),
            Err(RegistrationError::StaleRegistration)
        );
        eprintln!("P3SYM_C6_KEEPER_REFUSAL stale_generation_before_cells");
    }
}
