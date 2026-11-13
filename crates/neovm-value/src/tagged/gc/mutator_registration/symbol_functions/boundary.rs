//! Test bridge: actual owner operations, then actual stopped reconciliation.
//! No Context, header view, evaluator or native entry is admitted here.
use super::super::GenericProofEra as ParallelHeapEra;
use super::retry::{self as contention, Deadline, Retry, retry_owned};
use super::*;
use crate::intern::SymId;
use crate::tagged::gc::{EraKeeper, HeapIdentity, Mutator, MutatorPermit};
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue;
use std::ops::ControlFlow;

#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) enum FunctionEraEdit {
    Same { symbol: usize },
    Replace { symbol: usize },
    Unbind { symbol: usize },
}
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) enum FunctionBoundaryPoint {
    BeforeReconcile,
    AfterReconcile,
}

/// Writer-turn epoch capture hook for the upper layer that owns the counter
/// type. The testkit calls it once on the WRITER thread immediately before the
/// edit and once immediately after the writer's post-edit snapshot, so a
/// thread-local counter delta is attributed to the actual committing thread.
/// Value itself has no evaluator counter type and never inspects the captured
/// values; the caller reads them back through its own `&mut` recorder.
pub trait WriterEpochRecorder: Send {
    fn before_writer(&mut self);
    fn after_writer(&mut self);
}

/// Recorder for value-only consumers that do not attribute epoch bumps.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct NoEpochRecorder;
impl WriterEpochRecorder for NoEpochRecorder {
    fn before_writer(&mut self) {}
    fn after_writer(&mut self) {}
}

#[derive(Debug)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct FunctionOwnerSnapshot {
    pub(crate) words: [usize; 2],
    pub(crate) facts: [(u64, u64, bool); 2],
}
impl FunctionOwnerSnapshot {
    /// Copied root words; the snapshot is plain data with no interior cell.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn words(&self) -> [usize; 2] {
        self.words
    }

    /// Copied (epoch, stamp, bound) facts per observed key.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn facts(&self) -> [(u64, u64, bool); 2] {
        self.facts
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct FunctionEraReceipt {
    pub(crate) before: FunctionOwnerSnapshot,
    pub(crate) writer: FunctionOwnerSnapshot,
    pub(crate) writer_answer: usize,
    pub(crate) after: FunctionOwnerSnapshot,
}
impl FunctionEraReceipt {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn before(&self) -> &FunctionOwnerSnapshot {
        &self.before
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn writer(&self) -> &FunctionOwnerSnapshot {
        &self.writer
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn after(&self) -> &FunctionOwnerSnapshot {
        &self.after
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn writer_answer(&self) -> usize {
        self.writer_answer
    }
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
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn exercise_function_era<'heap, K: EraKeeper>(
    era: &ParallelHeapEra<'heap, K>,
    ids: [SymId; 2],
    edit: FunctionEraEdit,
    replacement: &SharedRoot,
    mut observe: impl FnMut(FunctionBoundaryPoint),
    epochs: &mut dyn WriterEpochRecorder,
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
                    epochs.before_writer();
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
                    let observed = snapshot(owner, &writer_handles, nil_ref)?;
                    epochs.after_writer();
                    Ok((observed, answer))
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
    let reconciled = era
        .reconcile_functions(
            &mut stopped
                .symbols()
                .map_err(|e| format!("keeper symbols: {e:?}"))?,
        )
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
