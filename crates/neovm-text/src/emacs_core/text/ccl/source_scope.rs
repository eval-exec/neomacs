//! Observe the real scoped variable pointer without clearing or dereferencing
//! it. The consuming source adapter must refuse the active raw-view extent.

use super::CCL_VARIABLES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum SourceCclScope {
    Inactive,
    Active,
}

#[cold]
#[doc(hidden)]
pub fn source_ccl_scope() -> SourceCclScope {
    CCL_VARIABLES.with(|slot| {
        if slot.get().is_null() {
            SourceCclScope::Inactive
        } else {
            SourceCclScope::Active
        }
    })
}

#[cfg(test)]
mod tests {
    use super::super::CCL_VARIABLES;
    use super::*;

    #[test]
    fn pentry_e2b_source_ccl_scope_null_is_inactive_and_nonzero_is_active() {
        CCL_VARIABLES.with(|slot| {
            let previous = slot.replace(std::ptr::null());
            assert_eq!(source_ccl_scope(), SourceCclScope::Inactive);
            slot.set(std::ptr::without_provenance(1));
            assert_eq!(source_ccl_scope(), SourceCclScope::Active);
            slot.set(previous);
        });
    }
}
