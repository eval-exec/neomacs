//! Test bridge: actual owner operations, then actual stopped reconciliation.
//! No Context, header view, evaluator or native entry is admitted here.
use super::retry::{self as contention, Deadline, Retry, retry_owned};
use super::*;
use crate::emacs_core::intern::SymId;
use crate::tagged::gc::{HeapIdentity, Mutator, MutatorPermit, ParallelHeapEra};
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue;
use std::ops::ControlFlow;

#[derive(Clone, Copy, Debug)]
pub(crate) enum FunctionEraEdit {
    Same { symbol: usize },
    Replace { symbol: usize },
    Unbind { symbol: usize },
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum FunctionBoundaryPoint {
    BeforeReconcile,
    AfterReconcile,
}
#[derive(Debug)]
pub(crate) struct FunctionOwnerSnapshot {
    pub(crate) words: [usize; 2],
    pub(crate) facts: [(u64, u64, bool); 2],
}
#[derive(Debug)]
pub(crate) struct FunctionEraReceipt {
    pub(crate) before: FunctionOwnerSnapshot,
    pub(crate) writer: FunctionOwnerSnapshot,
    pub(crate) writer_answer: usize,
    pub(crate) after: FunctionOwnerSnapshot,
}

fn attempt<'era, T>(
    owner: &mut Mutator<'era>,
    mut body: impl FnMut(&mut RunAccess<'_, 'era>) -> Result<T, FunctionError>,
) -> Result<T, String> {
    contention::retry(
        Deadline::contention(),
        || match owner.run_access() {
            Ok(mut run) => body(&mut run),
            Err(error) => Err(error.into()),
        },
        |error| match error {
            FunctionError::Authority(error) => contention::registration(error),
            FunctionError::VoidFunction
            | FunctionError::UnsupportedDefinition
            | FunctionError::EpochExhausted
            | FunctionError::ActiveActivation
            | FunctionError::InactiveActivation => Retry::Refuse,
        },
    )
    .map_err(|error| format!("function owner: {error:?}"))
}

fn activate<'era>(
    permit: MutatorPermit<'era>,
    heap: HeapIdentity,
    id: u64,
) -> Result<Mutator<'era>, String> {
    retry_owned(
        Deadline::contention(),
        permit,
        |permit| permit.activate(heap, id),
        |failed| match contention::registration(&failed.reason) {
            Retry::Busy => ControlFlow::Continue(failed.into_permit()),
            Retry::Refuse => ControlFlow::Break(failed),
        },
    )
    .map_err(|failed| format!("function activation: {failed:?}"))
}

fn guarded<'era, T>(
    mut owner: Mutator<'era>,
    body: impl FnOnce(&mut Mutator<'era>) -> Result<T, String>,
) -> Result<T, String> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&mut owner)))
        .unwrap_or_else(|_| Err("function owner assertion failed".into()));
    // All body homes and loans have ended, regardless of its outcome. Clear
    // the actual canonical bank, then consume the real retirement capability.
    let cleared = attempt(&mut owner, |run| run.clear_root_slots().map_err(Into::into));
    retry_owned(
        Deadline::contention(),
        owner,
        |owner| owner.retire_rooted(&[]),
        |failed| match contention::rendezvous(&failed.reason) {
            Retry::Busy => ControlFlow::Continue(failed.into_owner()),
            Retry::Refuse => ControlFlow::Break(failed),
        },
    )
    .map_err(|failed| format!("function owner teardown: {failed:?}"))?;
    cleared.and(result)
}

fn snapshot(
    owner: &mut Mutator<'_>,
    handles: &[SymbolHandle; 2],
    nil: &SharedRoot,
) -> Result<FunctionOwnerSnapshot, String> {
    let mut dst = attempt(owner, |run| run.root_slot(nil).map_err(Into::into))?;
    let mut words = [0; 2];
    let mut facts = [(0, 0, false); 2];
    for (index, key) in handles.iter().enumerate() {
        (words[index], facts[index]) = attempt(owner, |run| {
            run.sym_function_into(key, &mut dst)?;
            Ok((dst.word_for_test(run), run.function_facts_for_test(key)?))
        })?;
    }
    Ok(FunctionOwnerSnapshot { words, facts })
}

/// A keeper callback may inspect only its inactive cache metadata. Fixtures
/// invoke no compilation/evaluation and capture integer cache identities only.
pub(crate) fn exercise_function_era(
    era: &ParallelHeapEra<'_>,
    ids: [SymId; 2],
    edit: FunctionEraEdit,
    replacement: &SharedRoot,
    mut observe: impl FnMut(FunctionBoundaryPoint),
) -> Result<FunctionEraReceipt, String> {
    let operations = (|| {
        let handles = ids.map(|id| {
            era.admit_plain_symbol(id)
                .map_err(|e| format!("function admission: {e:?}"))
        });
        let [left, right] = handles;
        let handles = [left?, right?];
        for key in &handles {
            era.prepare_function_symbol(key)
                .map_err(|e| format!("function preparation: {e:?}"))?;
        }
        let mut stopped = era.stop().map_err(|e| format!("seed stop: {e:?}"))?;
        let nil = era
            .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                // SAFETY: matching actual stopped heap, immediate NIL seed only.
                SharedRoot::new(heap, TaggedValue::NIL)
            })
            .map_err(|e| format!("seed heap: {e:?}"))?;
        stopped
            .trace(|_, _| Ok(()))
            .map_err(|e| format!("seed trace: {e:?}"))?;
        stopped
            .complete()
            .map_err(|e| format!("seed complete: {e:?}"))?;
        let reader = era
            .register(7101, 8)
            .map_err(|e| format!("reader registration: {e:?}"))?;
        let writer = era
            .register(7102, 8)
            .map_err(|e| format!("writer registration: {e:?}"))?;
        let heap = era.heap_identity();
        let receipt = std::thread::scope(|scope| {
            let (start, ready) = std::sync::mpsc::channel();
            let writer_handles = handles.clone();
            let nil_ref = &nil;
            let worker = scope.spawn(move || {
                let owner = activate(writer, heap, 7102)?;
                guarded(owner, |owner| {
                    ready.recv().map_err(|e| format!("writer turn: {e:?}"))?;
                    let symbol = match edit {
                        FunctionEraEdit::Same { symbol }
                        | FunctionEraEdit::Replace { symbol }
                        | FunctionEraEdit::Unbind { symbol } => symbol,
                    };
                    let key = writer_handles.get(symbol).ok_or("writer symbol index")?;
                    let mut dst = attempt(owner, |run| run.root_slot(nil_ref).map_err(Into::into))?;
                    let mut src =
                        attempt(owner, |run| run.root_slot(replacement).map_err(Into::into))?;
                    let answer = attempt(owner, |run| {
                        match edit {
                            FunctionEraEdit::Same { .. } => {
                                run.sym_function_into(key, &mut src)?;
                                run.sym_fset_into(key, &src, &mut dst)
                            }
                            FunctionEraEdit::Replace { .. } => {
                                run.sym_fset_into(key, &src, &mut dst)
                            }
                            FunctionEraEdit::Unbind { .. } => {
                                run.sym_fmakunbound_into(key, &mut dst)
                            }
                        }?;
                        Ok(dst.word_for_test(run))
                    })?;
                    Ok((snapshot(owner, &writer_handles, nil_ref)?, answer))
                })
            });
            let owner = activate(reader, heap, 7101);
            let result = match owner {
                Ok(owner) => guarded(owner, |owner| {
                    let before = snapshot(owner, &handles, &nil);
                    // No RunAccess/lease remains while waiting. Even a failed
                    // observation lets the writer finish and clean its roots.
                    let started = start.send(()).map_err(|e| format!("reader turn: {e:?}"));
                    let written = worker
                        .join()
                        .map_err(|_| "writer assertion failed".to_string());
                    let after = snapshot(owner, &handles, &nil);
                    started?;
                    let (writer, writer_answer) = written??;
                    Ok(FunctionEraReceipt {
                        before: before?,
                        writer,
                        writer_answer,
                        after: after?,
                    })
                }),
                Err(error) => Err(error),
            };
            // Also closes a waiting writer after a reader setup/assertion
            // failure; scoped joining cannot strand a waiting fixture owner.
            drop(start);
            result
        });
        drop(nil);
        receipt
    })();
    // Reconcile even when an owner operation/refusal was unexpected. All
    // owners retired above; actual keeper minting still requires word zero.
    let mut stopped = era
        .stop()
        .map_err(|e| format!("reconciliation stop: {e:?}"))?;
    let before = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        observe(FunctionBoundaryPoint::BeforeReconcile)
    }));
    let reconciled = stopped
        .symbols()
        .map_err(|e| format!("keeper symbols: {e:?}"))?
        .reconcile_functions()
        .map_err(|e| format!("reconciliation: {e:?}"));
    let after = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        observe(FunctionBoundaryPoint::AfterReconcile)
    }));
    stopped
        .trace(|_, _| Ok(()))
        .map_err(|e| format!("final trace: {e:?}"))?;
    stopped
        .complete()
        .map_err(|e| format!("final complete: {e:?}"))?;
    reconciled?;
    if before.is_err() || after.is_err() {
        return Err("cache metadata assertion failed after clean reconciliation".into());
    }
    operations
}
