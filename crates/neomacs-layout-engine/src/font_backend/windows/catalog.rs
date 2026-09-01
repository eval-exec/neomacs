//! DirectWrite system-collection refresh polling.
//!
//! `GetSystemFontCollection(TRUE)` is DirectWrite's supported missed-event
//! recovery path: it checks for installed-font updates immediately and returns
//! the current cached collection. This avoids subclassing every HWND merely to
//! receive `WM_FONTCHANGE`, and confines the crate's one raw COM observation to
//! this adapter leaf.

use crate::font::catalog::{
    FontCatalogChange, FontCatalogChangeCounter, FontCatalogPollAction,
    RateLimitedFontCatalogPoller,
};
use dwrote::FontCollection;
use std::time::Duration;

const CATALOG_POLL_INTERVAL: Duration = Duration::from_secs(1);
static CATALOG_CHANGES: FontCatalogChangeCounter = FontCatalogChangeCounter::new();

#[derive(Debug)]
pub(super) struct DirectWriteCatalogMonitor {
    poller: RateLimitedFontCatalogPoller,
    snapshot: DirectWriteCatalogSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DirectWriteCatalogSnapshot {
    collection_identity: usize,
    family_count: u32,
}

impl Default for DirectWriteCatalogMonitor {
    fn default() -> Self {
        Self {
            poller: RateLimitedFontCatalogPoller::new(&CATALOG_CHANGES, CATALOG_POLL_INTERVAL),
            snapshot: DirectWriteCatalogSnapshot::capture(false),
        }
    }
}

impl DirectWriteCatalogMonitor {
    pub(super) fn poll(&mut self) -> FontCatalogChange {
        match self.poller.begin(&CATALOG_CHANGES) {
            FontCatalogPollAction::PublishedChange => {
                // A peer already asked DirectWrite to update the process-wide
                // collection. Advance this monitor's baseline as it consumes
                // that edge, otherwise its next probe would republish the same
                // replacement as a second change.
                self.snapshot = DirectWriteCatalogSnapshot::capture(false);
                return FontCatalogChange::Changed;
            }
            FontCatalogPollAction::Wait => return FontCatalogChange::Unchanged,
            FontCatalogPollAction::ProbeNativeCatalog => {}
        }

        let current = DirectWriteCatalogSnapshot::capture(true);
        let changed = current != self.snapshot;
        self.snapshot = current;
        if changed {
            self.poller.publish_detected(&CATALOG_CHANGES)
        } else {
            FontCatalogChange::Unchanged
        }
    }
}

impl DirectWriteCatalogSnapshot {
    fn capture(check_for_updates: bool) -> Self {
        let collection = FontCollection::get_system(check_for_updates);
        Self {
            collection_identity: collection_identity(&collection),
            family_count: collection.get_font_family_count(),
        }
    }
}

fn collection_identity(collection: &FontCollection) -> usize {
    // SAFETY: `as_ptr` only observes the live collection's COM identity for
    // the duration of this call. The pointer is never dereferenced or used as
    // ownership; DirectWrite documents stable cached collection objects until
    // `GetSystemFontCollection(TRUE)` detects a replacement.
    unsafe { collection.as_ptr().cast::<()>() as usize }
}
