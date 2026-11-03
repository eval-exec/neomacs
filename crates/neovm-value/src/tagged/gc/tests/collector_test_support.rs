//! Heap-keyed start-result injection, compiled only into the GC unit tests.
//! No production knob, TLS slot or Collector/TaggedHeap test-state field.

use super::*;
use std::collections::VecDeque;

pub(super) enum SpawnOutcome {
    Unavailable(std::io::ErrorKind),
    System,
}

static PLANS: std::sync::Mutex<FxHashMap<HeapIdentity, VecDeque<SpawnOutcome>>> =
    std::sync::Mutex::new(FxHashMap::with_hasher(rustc_hash::FxBuildHasher));

/// Removing a plan during panic cleanup prevents it from affecting a later
/// heap. The fixture installs before dispatch and retains ownership until join.
pub(super) struct SpawnPlan {
    heap: HeapIdentity,
}

impl SpawnPlan {
    pub(super) fn install(
        heap: HeapIdentity,
        outcomes: impl IntoIterator<Item = SpawnOutcome>,
    ) -> Self {
        assert!(
            PLANS
                .lock()
                .unwrap()
                .insert(heap, outcomes.into_iter().collect())
                .is_none()
        );
        Self { heap }
    }

    pub(super) fn assert_consumed(&self) {
        let plans = PLANS.lock().unwrap();
        assert!(plans.get(&self.heap).unwrap().is_empty());
    }
}

impl Drop for SpawnPlan {
    fn drop(&mut self) {
        PLANS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.heap);
    }
}

pub(super) fn injected_start_failure(heap: HeapIdentity) -> Option<std::io::Error> {
    let mut plans = PLANS.lock().unwrap();
    let plan = plans.get_mut(&heap)?;
    match plan
        .pop_front()
        .expect("unexpected extra collector start attempt")
    {
        SpawnOutcome::Unavailable(kind) => Some(std::io::Error::from(kind)),
        SpawnOutcome::System => None,
    }
}
