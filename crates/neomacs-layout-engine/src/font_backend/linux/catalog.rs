//! Fontconfig catalog freshness polling.
//!
//! Fontconfig has no portable process callback. Like Firefox, Neomacs asks
//! whether the current config is up to date and lets Fontconfig rebuild it,
//! but rate-limits that filesystem work off the redisplay hot path.

use crate::font::catalog::{
    FontCatalogChange, FontCatalogChangeCounter, FontCatalogPollAction,
    RateLimitedFontCatalogPoller,
};
use std::time::Duration;

const CATALOG_POLL_INTERVAL: Duration = Duration::from_secs(1);
static CATALOG_CHANGES: FontCatalogChangeCounter = FontCatalogChangeCounter::new();

#[derive(Debug)]
pub(super) struct FontconfigCatalogMonitor {
    poller: RateLimitedFontCatalogPoller,
}

impl Default for FontconfigCatalogMonitor {
    fn default() -> Self {
        Self {
            poller: RateLimitedFontCatalogPoller::new(&CATALOG_CHANGES, CATALOG_POLL_INTERVAL),
        }
    }
}

impl FontconfigCatalogMonitor {
    pub(super) fn poll(&mut self) -> FontCatalogChange {
        match self.poller.begin(&CATALOG_CHANGES) {
            FontCatalogPollAction::PublishedChange => return FontCatalogChange::Changed,
            FontCatalogPollAction::Wait => return FontCatalogChange::Unchanged,
            FontCatalogPollAction::ProbeNativeCatalog => {}
        }

        let Some(current) = ReferencedFontconfigConfig::current() else {
            return FontCatalogChange::Unchanged;
        };
        // A referenced snapshot cannot disappear if another Fontconfig user
        // replaces the process-global current config concurrently.
        let up_to_date = unsafe { fontconfig_sys::FcConfigUptoDate(current.0) } != 0;
        drop(current);
        if up_to_date {
            return FontCatalogChange::Unchanged;
        }

        // A false result means Fontconfig observed changed configuration/cache
        // inputs. Force reinitialization after that positive stale observation:
        // `FcInitBringUptoDate` obeys Fontconfig's own rescan interval and can
        // return true without checking or rebuilding, which would otherwise
        // publish false/repeated Neomacs generations.
        if unsafe { fontconfig_sys::FcInitReinitialize() } == 0 {
            tracing::warn!(target: "font_catalog", "Fontconfig catalog refresh failed");
            return FontCatalogChange::Unchanged;
        }
        self.poller.publish_detected(&CATALOG_CHANGES)
    }
}

struct ReferencedFontconfigConfig(*mut fontconfig_sys::FcConfig);

impl ReferencedFontconfigConfig {
    fn current() -> Option<Self> {
        // SAFETY: Fontconfig documents a null argument as "reference the
        // current configuration". The returned reference remains valid until
        // the matching `FcConfigDestroy` in `Drop`.
        let config = unsafe { fontconfig_sys::FcConfigReference(std::ptr::null_mut()) };
        (!config.is_null()).then_some(Self(config))
    }
}

impl Drop for ReferencedFontconfigConfig {
    fn drop(&mut self) {
        // SAFETY: this is the one matching release for the successful
        // `FcConfigReference` performed by `current`; the pointer is not used
        // after this call.
        unsafe { fontconfig_sys::FcConfigDestroy(self.0) };
    }
}
