//! Owner-only observations of original source objects and decoded payloads.
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

/// Original allocation identity, not Lisp `equal`: equal objects may have
/// different contents, intervals, or decoded payloads. Matching roots protect
/// their graphs while the original observed ancestor edges remain unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SourceObjectIdentity(usize);

// Replaces the former completeness bool without growing the TLS observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaptureState {
    Ordinary,
    SourceSelectionRequired,
    Incomplete,
}
const _: () = assert!(std::mem::size_of::<CaptureState>() == std::mem::size_of::<bool>());

#[derive(Debug)]
struct Observation {
    reads: Vec<CollectionReads>,
    roots: Vec<OwnedRoots>,
    // Source strings AND directly interpreted payload operands.
    strings: rustc_hash::FxHashSet<SourceObjectIdentity>,
    retainer: OwnedRootRetainer,
    owner: Option<CollectionReads>,
    state: CaptureState,
}
thread_local! { static ACTIVE: RefCell<Option<Observation>> = const { RefCell::new(None) }; }

/// Complete original string observations, kept alive by evaluator-owned roots.
/// Rc clones preserve the original read certificate across unchanged replay.
#[derive(Clone, Debug)]
pub(crate) struct CursorSourceDependencies(Rc<DependencyAuthority>);

/// Original producer evidence that replay needs no Lisp-string cursor override.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OrdinaryCursorSourceProof(());
#[derive(Debug)]
enum DependencyAuthority {
    Canonical {
        owner: CollectionReads,
        reads: Vec<CollectionReads>,
        _roots: Vec<OwnedRoots>,
        strings: rustc_hash::FxHashSet<SourceObjectIdentity>,
        ordinary: Option<OrdinaryCursorSourceProof>,
    },
    // Original prepared-row producer evidence and its original source leases.
    // The domain is opaque, so partial walks cannot claim subset inclusion.
    PreparedRows {
        reads: CollectionReads,
        _roots: Vec<OwnedRoots>,
    },
    PreparedUnion(Vec<CursorSourceDependencies>),
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
            DependencyAuthority::PreparedRows { reads, .. } => reads.unchanged_and_observe(),
            DependencyAuthority::PreparedUnion(sources) => sources.iter().all(Self::unchanged),
            DependencyAuthority::FrameOnly { previous, fresh } => {
                previous.unchanged() && fresh.unchanged()
            }
        }
    }
    /// Expanded partial-source domains validate this frame only; the next walk
    /// resets them instead of indefinitely retaining removed strings/leases.
    pub(crate) fn can_replay(&self) -> bool {
        !matches!(self.0.as_ref(), DependencyAuthority::FrameOnly { .. })
    }
    /// Complete canonical walks with observed absence of explicit cursors, or successful owned
    /// row programs (which reject all non-nil string `cursor` properties), allow
    /// moved-point reconstruction. This proof applies only to unchanged rows.
    pub(crate) fn ordinary_cursor_source_proof(&self) -> Option<OrdinaryCursorSourceProof> {
        let ordinary = match self.0.as_ref() {
            DependencyAuthority::Canonical { ordinary, .. } => ordinary.is_some(),
            DependencyAuthority::PreparedRows { .. } => true,
            DependencyAuthority::PreparedUnion(parts) => parts
                .iter()
                .all(|part| part.ordinary_cursor_source_proof().is_some()),
            DependencyAuthority::FrameOnly { .. } => false,
        };
        (ordinary && self.unchanged()).then_some(OrdinaryCursorSourceProof(()))
    }
    /// Preserve a prepared page producer's original evidence and leases. This
    /// does not recapture or rebase dependencies when a cached page is selected.
    pub(crate) fn from_prepared_rows(reads: CollectionReads, roots: Vec<OwnedRoots>) -> Self {
        Self(Rc::new(DependencyAuthority::PreparedRows {
            reads,
            _roots: roots,
        }))
    }
    /// Combine actual original page authorities, flattening and deduplicating
    /// them under a fixed budget. Never build an unbounded historical chain.
    pub(crate) fn combine_originals(sources: impl IntoIterator<Item = Self>) -> Option<Self> {
        fn append(
            source: CursorSourceDependencies,
            leaves: &mut Vec<CursorSourceDependencies>,
        ) -> bool {
            match source.0.as_ref() {
                DependencyAuthority::PreparedUnion(parts) => {
                    parts.iter().all(|part| append(part.clone(), leaves))
                }
                DependencyAuthority::FrameOnly { .. } => false,
                _ => {
                    if !leaves.iter().any(|leaf| Rc::ptr_eq(&leaf.0, &source.0)) {
                        if leaves.len() == 32 {
                            return false;
                        }
                        leaves.push(source);
                    }
                    true
                }
            }
        }
        let mut leaves = Vec::new();
        for source in sources {
            if !source.unchanged() || !append(source, &mut leaves) {
                return None;
            }
        }
        if leaves.is_empty() {
            None
        } else {
            Some(Self(Rc::new(DependencyAuthority::PreparedUnion(leaves))))
        }
    }
    /// Unchanged original inputs certify retained rows, while actual new reads
    /// certify walked rows. Keep the ORIGINAL certificate for a known domain;
    /// new sources require a bounded frame-only union, not a rebased witness.
    pub(crate) fn after_partial_walk(&self, fresh: Self) -> Option<Self> {
        if matches!(
            self.0.as_ref(),
            DependencyAuthority::PreparedRows { .. } | DependencyAuthority::PreparedUnion(_)
        ) {
            return Some(Self(Rc::new(DependencyAuthority::FrameOnly {
                previous: self.clone(),
                fresh,
            })));
        }
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
                state: if owner.is_some() {
                    CaptureState::Ordinary
                } else {
                    CaptureState::Incomplete
                },
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
        if observation.state == CaptureState::Incomplete {
            return None;
        }
        Some(CursorSourceDependencies(Rc::new(
            DependencyAuthority::Canonical {
                owner: observation.owner?,
                reads: observation.reads,
                _roots: observation.roots,
                strings: observation.strings,
                ordinary: (observation.state == CaptureState::Ordinary)
                    .then_some(OrdinaryCursorSourceProof(())),
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

/// Root an original operand BEFORE its callback-free native projection and
/// observe the actual decoder, including unsuccessful/absent lookups. Rooted
/// ancestor graphs protect child identities while observed edges stay valid;
/// changing an edge invalidates the original certificate before reuse.
/// Operand identities also participate in partial-domain inclusion: observing
/// a new face plist is not covered merely by having seen its string before.
pub(crate) fn observe_projection<T>(value: Value, project: impl FnOnce() -> T) -> T {
    let rooted = ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let observation = active.as_mut()?;
        if !observation
            .owner
            .as_ref()
            .is_some_and(CollectionReads::unchanged_and_observe)
        {
            observation.state = CaptureState::Incomplete;
            return None;
        }
        let identity = SourceObjectIdentity(value.bits());
        if !observation.strings.contains(&identity) {
            let Some(roots) = observation.retainer.retain([value]) else {
                observation.state = CaptureState::Incomplete;
                return None;
            };
            // Root each original object once, before its first projection.
            // Repeated projections must still record mutation observations.
            observation.roots.push(roots);
            observation.strings.insert(identity);
        }
        Some(())
    });
    if rooted.is_none() {
        return project();
    }
    let (result, reads) = collection_reads::capture(project);
    ACTIVE.with(|active| {
        if let Some(observation) = active.borrow_mut().as_mut() {
            if let Some(reads) = reads {
                observation.reads.push(reads);
            } else {
                observation.state = CaptureState::Incomplete;
            }
        }
    });
    result
}

/// Observe presence AND absence of intervals before candidate selection.
/// Property-less competing strings may gain `cursor` independently of ticks.
pub(crate) fn string_properties(
    value: Value,
) -> Option<neovm_core::emacs_core::value::text_props::TextPropertyTable> {
    let mut requires_selection = false;
    let properties = observe_projection(value, || {
        let mut chars = 0;
        if let Some(string) = value.as_lisp_string() {
            let _ = string.as_bytes();
            let _ = string.is_multibyte();
            chars = string.schars();
        }
        let properties =
            neovm_core::emacs_core::value::get_string_text_properties_table_for_value(value);
        // Observe the ORIGINAL interval lookup while its collection scope is
        // active. Category-derived behavior remains conservatively unproven.
        requires_selection = properties.as_ref().is_some_and(|props| {
            props.has_any_non_nil_property_in_char_range(
                neovm_core::buffer::CharRange::new(
                    neovm_core::buffer::CharPos0::ZERO,
                    neovm_core::buffer::CharPos0::new(chars),
                ),
                &[Value::symbol("cursor"), Value::symbol("category")],
            )
        });
        properties
    });
    if requires_selection {
        ACTIVE.with(|active| {
            if let Some(observation) = active.borrow_mut().as_mut() {
                if observation.state == CaptureState::Ordinary {
                    observation.state = CaptureState::SourceSelectionRequired;
                }
            }
        });
    }
    properties
}
