//! Owner-only observations of every string projected by a canonical window walk.
//! Numeric snapshots and worker rows deliberately carry no such certificate.

#[cfg(test)]
#[path = "tests/cursor_source_dependencies_test.rs"]
mod tests;
use neovm_core::{
    emacs_core::{
        Context, Value,
        owned_roots::{OwnedRootRetainer, OwnedRoots},
    },
    tagged::collection_reads::{self, CollectionReads},
};
use std::{cell::RefCell, rc::Rc};

#[derive(Debug)]
struct Observation {
    reads: Vec<CollectionReads>,
    roots: Vec<OwnedRoots>,
    strings: rustc_hash::FxHashSet<Value>,
    retainer: OwnedRootRetainer,
    owner: Option<CollectionReads>,
    complete: bool,
}
thread_local! { static ACTIVE: RefCell<Option<Observation>> = const { RefCell::new(None) }; }

/// Complete original string observations, kept alive by evaluator-owned roots.
/// Rc clones preserve the original read certificate across unchanged replay.
#[derive(Clone, Debug)]
pub(crate) struct CursorSourceDependencies(Rc<DependencyAuthority>);
#[derive(Debug)]
enum DependencyAuthority {
    Canonical {
        owner: CollectionReads,
        reads: Vec<CollectionReads>,
        _roots: Vec<OwnedRoots>,
        strings: rustc_hash::FxHashSet<Value>,
    },
    // Validate a partial walk with newly introduced sources for this frame,
    // then require a canonical walk. Never grow an unbounded history chain.
    FrameOnly {
        previous: CursorSourceDependencies,
        fresh: CursorSourceDependencies,
    },
}
impl CursorSourceDependencies {
    /// Validate original mutator identity and bounded projected-write history;
    /// propagate reads transitively, never rebase a skipped source walk.
    pub(crate) fn unchanged(&self) -> bool {
        match self.0.as_ref() {
            DependencyAuthority::Canonical { owner, reads, .. } => {
                owner.unchanged_and_observe()
                    && reads.iter().all(CollectionReads::unchanged_and_observe)
            }
            DependencyAuthority::FrameOnly { previous, fresh } => {
                previous.unchanged() && fresh.unchanged()
            }
        }
    }
    /// Expanded partial-source domains validate this frame only; the next walk
    /// resets them instead of indefinitely retaining removed strings/leases.
    pub(crate) fn can_replay(&self) -> bool {
        matches!(self.0.as_ref(), DependencyAuthority::Canonical { .. })
    }
    /// Unchanged original inputs certify retained rows, while actual new reads
    /// certify walked rows. Keep the ORIGINAL certificate for a known domain;
    /// new sources require a bounded frame-only union, not a rebased witness.
    pub(crate) fn after_partial_walk(&self, fresh: Self) -> Option<Self> {
        let DependencyAuthority::Canonical {
            strings: previous, ..
        } = self.0.as_ref()
        else {
            return None;
        };
        let DependencyAuthority::Canonical {
            strings: current, ..
        } = fresh.0.as_ref()
        else {
            return None;
        };
        if previous.is_empty() {
            // Retained rows had no string inputs; all current source inputs
            // were actually observed in the newly walked rows.
            Some(fresh)
        } else if current.is_subset(previous) {
            Some(self.clone())
        } else {
            Some(Self(Rc::new(DependencyAuthority::FrameOnly {
                previous: self.clone(),
                fresh,
            })))
        }
    }
}

/// Chrome strings cannot override the buffer cursor. Nested canonical window
/// walks inside chrome callbacks still install their own independent capture.
pub(crate) struct Pause(Option<Observation>);
/// Suspend only the enclosing window's observation while producing chrome.
pub(crate) fn pause() -> Pause {
    Pause(ACTIVE.with(|active| active.borrow_mut().take()))
}
impl Drop for Pause {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            active.replace(self.0.take());
        });
    }
}

/// Lexical producer scope; retries and unwinding discard speculative reads.
/// Strings receive original-evaluator roots immediately, before callbacks or GC;
/// neither scratch-stack slots nor borrowed Context pointers escape the walk.
pub(crate) struct Capture {
    previous: Option<Observation>,
}
impl Capture {
    /// Start an original owner-world witness and a weak evaluator root factory.
    /// A nested walk replaces, rather than merges into, its caller's capture.
    pub(crate) fn begin(context: &Context) -> Self {
        let (_, owner) = collection_reads::capture(|| ());
        let previous = ACTIVE.with(|active| {
            active.replace(Some(Observation {
                reads: Vec::new(),
                roots: Vec::new(),
                strings: rustc_hash::FxHashSet::default(),
                complete: owner.is_some(),
                owner,
                retainer: context.owned_root_retainer(),
            }))
        });
        Self { previous }
    }
    /// Detect mutations among successfully observed inputs. Completeness is
    /// checked separately by finish: missing instrumentation grants no reuse.
    pub(crate) fn unchanged(&self) -> bool {
        ACTIVE.with(|active| {
            active.borrow().as_ref().is_some_and(|observation| {
                observation
                    .reads
                    .iter()
                    .all(CollectionReads::unchanged_and_observe)
            })
        })
    }
    /// Seal original rooted observations after callbacks and row admission.
    /// Unknown/overflowed captures cannot become an empty absence certificate.
    pub(crate) fn finish(self) -> Option<CursorSourceDependencies> {
        if !self.unchanged() {
            return None;
        }
        let observation = ACTIVE.with(|active| active.borrow_mut().take())?;
        if !observation.complete {
            return None;
        }
        Some(CursorSourceDependencies(Rc::new(
            DependencyAuthority::Canonical {
                owner: observation.owner?,
                reads: observation.reads,
                _roots: observation.roots,
                strings: observation.strings,
            },
        )))
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            active.replace(self.previous.take());
        });
    }
}

/// Observe presence AND absence of intervals, before selection filters or
/// early winner selection. Property-less competing strings may gain `cursor`.
pub(crate) fn string_properties(
    value: Value,
) -> Option<neovm_core::emacs_core::value::text_props::TextPropertyTable> {
    let retainer = ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let observation = active.as_mut()?;
        if !observation
            .owner
            .as_ref()
            .is_some_and(CollectionReads::unchanged_and_observe)
        {
            observation.complete = false;
            return None;
        }
        Some(observation.retainer.clone())
    });
    let Some(roots) = retainer.and_then(|retainer| retainer.retain([value])) else {
        ACTIVE.with(|active| {
            if let Some(observation) = active.borrow_mut().as_mut() {
                observation.complete = false;
            }
        });
        return neovm_core::emacs_core::value::get_string_text_properties_table_for_value(value);
    };
    let (properties, reads) = collection_reads::capture(|| {
        // The cursor property is indexed in the original string. Observe its
        // contents/storage too, without copying/scanning bytes, so in-place
        // string edits cannot retain an old source slot or glyph extent.
        if let Some(string) = value.as_lisp_string() {
            let _ = string.as_bytes();
            let _ = string.is_multibyte();
            let _ = string.schars();
        }
        neovm_core::emacs_core::value::get_string_text_properties_table_for_value(value)
    });
    ACTIVE.with(|active| {
        if let Some(observation) = active.borrow_mut().as_mut() {
            observation.roots.push(roots);
            observation.strings.insert(value);
            if let Some(reads) = reads {
                observation.reads.push(reads);
            } else {
                observation.complete = false;
            }
        }
    });
    properties
}
