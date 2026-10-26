//! Native scheduling is not bounded by a count of yield calls. Keep a finite
//! shared elapsed-time deadline and name the stalled op.
use super::*;
use std::time::Duration;

pub(super) fn diagnostics(owner: &Mutator<'_>) -> String {
    format!(
        "owner={owner:?} authority_ready={:?} admission={} phase={}",
        owner.registry.symbols.ready(),
        owner.entry.admission.load(Ordering::Acquire),
        owner.registry.phase.load()
    )
}

#[test]
fn p3sym_c5_native_retry_waits_for_a_held_registry_then_observes_one_winner() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::VoidProjection,
    ] {
        fixture_owner(kind, Query::Fresh, |_, owner, homes| {
            // Readiness must never hold the registry while waiting for a
            // receiver that may already have reached its bounded deadline.
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let mut release = Some(release_tx);
            let mut refusals = 0;
            let before = crate::emacs_core::intern::parallel::identity_census_for_test();
            let (result, held) = std::thread::scope(|scope| {
                let guard = scope.spawn(move || {
                    let mut result = Ok(());
                    crate::emacs_core::intern::parallel::with_registry_writer_for_test(|| {
                        result = ready_tx.send(()).map_err(|e| e.to_string()).and_then(|()| {
                            release_rx
                                .recv_timeout(Duration::from_secs(10))
                                .map_err(|e| e.to_string())
                        });
                    });
                    result
                });
                let result = ready_rx
                    .recv_timeout(Duration::from_secs(10))
                    .map_err(|e| format!("C5 held-registry setup: {e}"))
                    .and_then(|()| {
                        attempt_observed(
                            owner,
                            "intern after held registry",
                            |run| {
                                run.initial_intern_into(&homes.name, &mut homes.dst)?;
                                Ok(Value::from_bits(homes.dst.word_for_test(run)).as_bare_symbol_id())
                            },
                            |owner, error| {
                                // This callback runs AFTER the actual failed RunAccess
                                // drops, while the other actor still holds the registry.
                                if !matches!(error, Failure::Intern(InitialInternError::Authority(RegistrationError::Busy)))
                                    || owner.registry.symbols.ready() != Ok(())
                                    || owner.entry.admission.load(Ordering::Acquire)
                                        != crate::tagged::gc::mutator_registration::admission::AdmissionWord::OUT {
                                    return Err(format!("C5 refused intern retained admission: {error:?}; {}", diagnostics(owner)));
                                }
                                refusals += 1;
                                if refusals == 10_001 {
                                    // Drop the sender on every outcome before joining the
                                    // guard, so an early operation error cannot strand it.
                                    release
                                        .take()
                                        .ok_or("C5 release sender absent")?
                                        .send(())
                                        .map_err(|_| {
                                            "C5 registry holder ended early".to_owned()
                                        })?;
                                }
                                Ok(())
                            },
                        )
                    });
                drop(release.take());
                (result, guard.join())
            });
            held.expect("C5 held-registry join")
                .expect("C5 registry released by retrying actor");
            let id = result
                .expect("C5 intern after held registry")
                .expect("C5 rooted winner ID");
            assert!(
                refusals >= 10_001,
                "must exceed the obsolete yield-count limit"
            );
            let after = crate::emacs_core::intern::parallel::identity_census_for_test();
            assert_eq!(after, (before.0 + 1, before.1 + 1));
            let repeated = attempt(owner, "intern repeat after held registry", |run| {
                run.initial_intern_into(&homes.name, &mut homes.dst)?;
                Ok(Value::from_bits(homes.dst.word_for_test(run)).as_bare_symbol_id())
            })
            .expect("C5 repeat after held registry");
            assert_eq!(repeated, Some(id));
            assert_eq!(
                crate::emacs_core::intern::parallel::identity_census_for_test(),
                after
            );
            eprintln!("P3SYM_C5_HELD_REGISTRY_RETRY {kind:?} refusals={refusals} winners=1");
        })
        .unwrap();
    }
}
