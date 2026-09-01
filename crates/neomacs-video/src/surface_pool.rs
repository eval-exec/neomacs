use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

/// Bounded pool whose checked-out values are affine leases. Values return to
/// the pool only when their lease drops, which lets the renderer tie reuse to
/// GPU submission retirement without exposing platform handles publicly.
pub(crate) struct BoundedSurfacePool<K, V> {
    shared: Arc<Mutex<SurfacePoolState<K, V>>>,
}

struct SurfacePoolState<K, V> {
    capacity: usize,
    allocated: usize,
    /// Idle entries in least-recently-returned order. Decoder surface pools
    /// rotate through several stable identities, so a miss is not evidence
    /// that the other identities are stale.
    idle: VecDeque<(K, V)>,
}

pub(crate) enum SurfacePoolAcquire<K, V> {
    Reused(SurfaceLease<K, V>),
    Allocate(SurfaceReservation<K, V>),
    Backpressured,
}

pub(crate) struct SurfaceReservation<K, V> {
    key: Option<K>,
    shared: Arc<Mutex<SurfacePoolState<K, V>>>,
}

pub(crate) struct SurfaceLease<K, V> {
    entry: Option<(K, V)>,
    shared: Arc<Mutex<SurfacePoolState<K, V>>>,
}

impl<K, V> BoundedSurfacePool<K, V>
where
    K: Eq,
{
    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "a surface pool must retain at least one slot");
        Self {
            shared: Arc::new(Mutex::new(SurfacePoolState {
                capacity,
                allocated: 0,
                idle: VecDeque::with_capacity(capacity),
            })),
        }
    }

    pub(crate) fn acquire(&self, key: K) -> SurfacePoolAcquire<K, V> {
        let mut key = Some(key);
        loop {
            let mut state = lock_unpoisoned(&self.shared);
            let wanted = key.as_ref().expect("surface key is consumed once");
            if let Some(index) = state
                .idle
                .iter()
                .position(|(candidate, _)| candidate == wanted)
            {
                let entry = state
                    .idle
                    .remove(index)
                    .expect("the matching idle surface index remains valid");
                return SurfacePoolAcquire::Reused(SurfaceLease {
                    entry: Some(entry),
                    shared: Arc::clone(&self.shared),
                });
            }

            if state.allocated < state.capacity {
                state.allocated += 1;
                return SurfacePoolAcquire::Allocate(SurfaceReservation {
                    key: key.take(),
                    shared: Arc::clone(&self.shared),
                });
            }

            // The pool is full. Evict exactly one least-recently-returned
            // idle identity outside the lock, then retry. Heterogeneous idle
            // identities remain cached while spare capacity exists.
            if let Some(stale) = state.idle.pop_front() {
                state.allocated -= 1;
                drop(state);
                drop(stale);
                continue;
            }

            return SurfacePoolAcquire::Backpressured;
        }
    }
}

impl<K, V> SurfaceReservation<K, V> {
    pub(crate) fn fulfill(mut self, value: V) -> SurfaceLease<K, V> {
        let key = self
            .key
            .take()
            .expect("a surface reservation can only be fulfilled once");
        SurfaceLease {
            entry: Some((key, value)),
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<K, V> Drop for SurfaceReservation<K, V> {
    fn drop(&mut self) {
        if self.key.is_some() {
            let mut state = lock_unpoisoned(&self.shared);
            state.allocated -= 1;
        }
    }
}

impl<K, V> SurfaceLease<K, V> {
    pub(crate) fn value(&self) -> &V {
        &self
            .entry
            .as_ref()
            .expect("a live surface lease owns one entry")
            .1
    }
}

impl<K, V> Drop for SurfaceLease<K, V> {
    fn drop(&mut self) {
        let entry = self
            .entry
            .take()
            .expect("a surface lease returns its entry exactly once");
        lock_unpoisoned(&self.shared).idle.push_back(entry);
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
