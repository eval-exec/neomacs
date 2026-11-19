//! Check both the raw Context pointer and the environment extent. A
//! borrow-refusing observer, not a way to null a live module pointer.
//! Eval cannot call this module. It installs the function pointer below.

use super::{ACTIVE_ENVS, MODULE_CTX};
use neovm_eval::emacs_core::eval::{SourceModuleObservation, install_source_module_scope_probe};
use neovm_value::{HookInstallError, HookInstallStatus};

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

static CANONICAL_SOURCE_MODULE_PROBE: fn() -> SourceModuleObservation = observe_for_eval;

/// Publish the process-wide probe. Core's eval-hook boot calls this beside
/// the sqlite finalizer hook, so a runtime that links neovm-system does not
/// stay unregistered. The same function again is `AlreadyInstalled`. A
/// different function is `HookInstallError::DifferentTable`.
#[doc(hidden)]
#[cold]
#[must_use = "handle a conflicting source-module probe publication"]
pub fn register_source_module_scope_probe() -> Result<HookInstallStatus, HookInstallError> {
    install_source_module_scope_probe(&CANONICAL_SOURCE_MODULE_PROBE)
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
        register_source_module_scope_probe().expect("canonical source-module probe");
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
        register_source_module_scope_probe().expect("canonical source-module probe");
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
        register_source_module_scope_probe().expect("canonical source-module probe");
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
        register_source_module_scope_probe().expect("canonical source-module probe");
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

    #[test]
    fn pentry_e2b_source_entry_different_module_probe_is_refused() {
        register_source_module_scope_probe().expect("canonical source-module probe");
        fn other() -> SourceModuleObservation {
            SourceModuleObservation::Active
        }
        static OTHER: fn() -> SourceModuleObservation = other;
        assert_eq!(
            install_source_module_scope_probe(&OTHER),
            Err(neovm_value::HookInstallError::DifferentTable)
        );
        clear();
        assert_eq!(
            source_module_observation(),
            SourceModuleObservation::Inactive
        );
    }
}
