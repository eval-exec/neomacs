//! Stopped retention identities, never raw descriptor or payload authority.
use super::{ObjectArena, ObjectPage, PagedObject, RegistrationError};
use rustc_hash::FxHashSet;

pub(super) struct RetainedPage {
    pub(super) base: usize,
    pub(super) frontier: usize,
}

#[derive(Default)]
pub(in crate::tagged::gc) struct NumericRetention {
    pub(super) floats: Vec<RetainedPage>,
    pub(super) bignums: Vec<RetainedPage>,
}

impl NumericRetention {
    pub(in crate::tagged::gc) fn is_empty(&self) -> bool {
        self.floats.is_empty() && self.bignums.is_empty()
    }
}

/// Reconcile identities against the actual stopped arena before mutation.
/// Initialized bits are authoritative: serial work may raise the frontier,
/// collection may lower it, and a missing/fully dead page starts at zero.
/// Every fallible reservation and validation precedes arena mutation.
pub(super) fn refresh_retained<T: PagedObject>(
    arena: &ObjectArena<T>,
    retained: &[RetainedPage],
) -> Result<Vec<RetainedPage>, RegistrationError> {
    let mut seen = FxHashSet::default();
    seen.try_reserve(retained.len())
        .map_err(|_| RegistrationError::Capacity)?;
    let mut refreshed = Vec::new();
    refreshed
        .try_reserve_exact(retained.len())
        .map_err(|_| RegistrationError::Capacity)?;
    for page in retained {
        if !seen.insert(page.base) {
            return Err(RegistrationError::Publication);
        }
        let frontier = arena
            .numeric_retained_page_frontier(page.base)
            .ok_or(RegistrationError::Publication)?;
        if frontier < ObjectPage::<T>::SLOTS {
            refreshed.push(RetainedPage {
                base: page.base,
                frontier,
            });
        }
    }
    Ok(refreshed)
}
