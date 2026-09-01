//! Typed lifecycle for the native system-font catalog.
//!
//! Platform callbacks and pollers only report a change. The evaluator/layout
//! thread consumes that report at a redisplay safe point, advances the
//! generation, and owns every resulting cache invalidation.

use neomacs_display_protocol::font::FontCatalogGeneration;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontCatalogChange {
    Unchanged,
    Changed,
}

/// Process-wide publication point for native catalog-change events.
///
/// The counter never carries cache-owning pointers across an OS callback. Each
/// font service keeps its own [`FontCatalogChangeCursor`], so one service
/// observing an event cannot consume it before another service sees it. Any
/// number of notifications between two polls still becomes one typed change.
#[derive(Debug, Default)]
pub struct FontCatalogChangeCounter(AtomicU64);

impl FontCatalogChangeCounter {
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn mark_changed(&self) {
        self.0.fetch_add(1, Ordering::Release);
    }

    #[must_use]
    pub fn cursor(&self) -> FontCatalogChangeCursor {
        FontCatalogChangeCursor {
            observed: self.0.load(Ordering::Acquire),
        }
    }
}

/// Per-consumer position in a process-wide native change counter.
#[derive(Clone, Copy, Debug)]
pub struct FontCatalogChangeCursor {
    observed: u64,
}

impl FontCatalogChangeCursor {
    #[must_use]
    pub fn poll(&mut self, counter: &FontCatalogChangeCounter) -> FontCatalogChange {
        let current = counter.0.load(Ordering::Acquire);
        if current != self.observed {
            self.observed = current;
            FontCatalogChange::Changed
        } else {
            FontCatalogChange::Unchanged
        }
    }
}

/// Result of the cheap first half of a rate-limited native catalog poll.
#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FontCatalogPollAction {
    /// Another service already detected and published a native change.
    PublishedChange,
    /// This service owns the next rate-limited native freshness probe.
    ProbeNativeCatalog,
    /// Neither a published event nor a due native probe exists.
    Wait,
}

/// Shared, correctness-sensitive scheduling around an OS freshness probe.
///
/// Platform adapters retain ownership of the actual native snapshot/query.
/// This type makes fan-out, burst coalescing, and rate limiting identical for
/// every polling backend.
#[derive(Debug)]
pub(crate) struct RateLimitedFontCatalogPoller {
    changes: FontCatalogChangeCursor,
    next_poll: Instant,
    interval: Duration,
}

impl RateLimitedFontCatalogPoller {
    pub(crate) fn new(counter: &FontCatalogChangeCounter, interval: Duration) -> Self {
        Self {
            changes: counter.cursor(),
            next_poll: Instant::now(),
            interval,
        }
    }

    #[must_use]
    pub(crate) fn begin(&mut self, counter: &FontCatalogChangeCounter) -> FontCatalogPollAction {
        let now = Instant::now();
        if let FontCatalogChange::Changed = self.changes.poll(counter) {
            self.next_poll = now + self.interval;
            return FontCatalogPollAction::PublishedChange;
        }
        if now < self.next_poll {
            return FontCatalogPollAction::Wait;
        }
        self.next_poll = now + self.interval;
        FontCatalogPollAction::ProbeNativeCatalog
    }

    #[must_use]
    pub(crate) fn publish_detected(
        &mut self,
        counter: &FontCatalogChangeCounter,
    ) -> FontCatalogChange {
        counter.mark_changed();
        self.changes.poll(counter)
    }
}

#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontCatalogUpdate {
    Unchanged(FontCatalogGeneration),
    Advanced {
        previous: FontCatalogGeneration,
        current: FontCatalogGeneration,
    },
}

impl FontCatalogUpdate {
    #[must_use]
    pub const fn generation(self) -> FontCatalogGeneration {
        match self {
            Self::Unchanged(generation) => generation,
            Self::Advanced { current, .. } => current,
        }
    }

    #[must_use]
    pub const fn changed(self) -> bool {
        matches!(self, Self::Advanced { .. })
    }
}

#[derive(Debug, Default)]
pub struct FontCatalog {
    generation: FontCatalogGeneration,
}

impl FontCatalog {
    #[must_use]
    pub const fn generation(&self) -> FontCatalogGeneration {
        self.generation
    }

    pub fn observe(&mut self, change: FontCatalogChange) -> FontCatalogUpdate {
        match change {
            FontCatalogChange::Unchanged => FontCatalogUpdate::Unchanged(self.generation),
            FontCatalogChange::Changed => {
                let previous = self.generation;
                self.generation = previous.next();
                FontCatalogUpdate::Advanced {
                    previous,
                    current: self.generation,
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "catalog_test.rs"]
mod tests;
