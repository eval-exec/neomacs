//! Check both the raw Context pointer and the environment extent. A
//! borrow-refusing observer, not a way to null a live module pointer.
//! Eval cannot call this module. It installs the function pointer below.

use super::{ACTIVE_ENVS, MODULE_CTX};
use neovm_eval::emacs_core::eval::{SourceModuleObservation, install_source_module_scope_probe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceModuleScope {
    Inactive,
    Active,
    Borrowed,
}

#[cold]
pub(crate) fn observe_source_module_scope() -> SourceModuleScope {
    if MODULE_CTX.with(|slot| !slot.get().is_null()) {
        return SourceModuleScope::Active;
    }
    ACTIVE_ENVS.with(|slot| match slot.try_borrow_mut() {
        Ok(environments) => {
            if environments.is_empty() {
                SourceModuleScope::Inactive
            } else {
                SourceModuleScope::Active
            }
        }
        Err(_) => SourceModuleScope::Borrowed,
    })
}

fn observe_for_eval() -> SourceModuleObservation {
    match observe_source_module_scope() {
        SourceModuleScope::Inactive => SourceModuleObservation::Inactive,
        SourceModuleScope::Active => SourceModuleObservation::Active,
        SourceModuleScope::Borrowed => SourceModuleObservation::Borrowed,
    }
}

/// Store the process-wide probe. A binary that links neovm-system and later
/// runs a live source-entry caller must call this; an unregistered probe
/// reads Inactive.
#[cold]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E2 L3 dormant module-scope probe install; no production caller enters it"
    )
)]
pub(crate) fn register_source_module_scope_probe() {
    // SAFETY: `observe_for_eval` only reads the existing module TLS. The
    // function pointer remains valid for the process. This stores no new
    // TLS slot and names no World.
    unsafe { install_source_module_scope_probe(observe_for_eval) }
}

#[cfg(test)]
mod tests {
    use super::super::{ACTIVE_ENVS, ActiveModuleEnvEntry, MODULE_CTX};
    use super::*;
    use neovm_eval::emacs_core::eval::{Context, source_module_observation};

    fn clear() {
        MODULE_CTX.with(|slot| slot.set(std::ptr::null_mut()));
        ACTIVE_ENVS.with(|slot| slot.borrow_mut().clear());
    }

    #[test]
    fn pentry_e2b_source_entry_module_raw_context_without_env_is_active() {
        clear();
        register_source_module_scope_probe();
        MODULE_CTX.with(|slot| {
            slot.set(std::ptr::without_provenance_mut::<Context>(1));
        });
        assert_eq!(source_module_observation(), SourceModuleObservation::Active);
        MODULE_CTX.with(|slot| slot.set(std::ptr::null_mut()));
        assert_eq!(
            source_module_observation(),
            SourceModuleObservation::Inactive
        );
    }

    #[test]
    fn pentry_e2b_source_entry_module_env_without_context_is_active() {
        clear();
        register_source_module_scope_probe();
        ACTIVE_ENVS.with(|slot| {
            slot.borrow_mut().push(ActiveModuleEnvEntry {
                env_priv: std::ptr::null_mut(),
                heap_identity: 0,
            });
        });
        assert_eq!(source_module_observation(), SourceModuleObservation::Active);
        clear();
        assert_eq!(
            source_module_observation(),
            SourceModuleObservation::Inactive
        );
    }

    #[test]
    fn pentry_e2b_source_entry_module_env_reader_is_borrowed() {
        clear();
        register_source_module_scope_probe();
        ACTIVE_ENVS.with(|slot| {
            let view = slot.borrow();
            assert!(view.is_empty());
            assert_eq!(
                source_module_observation(),
                SourceModuleObservation::Borrowed
            );
            assert!(view.is_empty());
        });
    }

    #[test]
    fn pentry_e2b_source_entry_module_env_writer_is_borrowed() {
        clear();
        register_source_module_scope_probe();
        ACTIVE_ENVS.with(|slot| {
            let view = slot.borrow_mut();
            assert!(view.is_empty());
            assert_eq!(
                source_module_observation(),
                SourceModuleObservation::Borrowed
            );
            assert!(view.is_empty());
        });
    }
}
