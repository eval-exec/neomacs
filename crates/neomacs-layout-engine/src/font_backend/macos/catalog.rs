//! CoreText registration-change observer.
//!
//! This mirrors GNU Emacs' `macfont_init_font_change_handler`, while also
//! observing the distributed center Apple documents for session/persistent
//! font registration. The callbacks only advance one atomic counter; all
//! cache and object ownership changes stay on the evaluator thread.

use crate::font::catalog::{FontCatalogChange, FontCatalogChangeCounter, FontCatalogChangeCursor};
use objc2_core_foundation::{
    CFDictionary, CFNotificationCenter, CFNotificationName, CFNotificationSuspensionBehavior,
};
use objc2_core_text::kCTFontManagerRegisteredFontsChangedNotification;
use std::ffi::c_void;
use std::sync::OnceLock;

static CATALOG_CHANGES: FontCatalogChangeCounter = FontCatalogChangeCounter::new();
static OBSERVER_TOKEN: u8 = 0;
static INSTALLED: OnceLock<()> = OnceLock::new();

unsafe extern "C-unwind" fn font_catalog_changed(
    _center: *mut CFNotificationCenter,
    _observer: *mut c_void,
    _name: *const CFNotificationName,
    _object: *const c_void,
    _user_info: *const CFDictionary,
) {
    CATALOG_CHANGES.mark_changed();
}

fn install_observers() {
    INSTALLED.get_or_init(|| {
        let observer = std::ptr::addr_of!(OBSERVER_TOKEN).cast::<c_void>();
        let centers = [
            CFNotificationCenter::local_center(),
            CFNotificationCenter::distributed_center(),
        ];
        for center in centers.into_iter().flatten() {
            // SAFETY: `observer` names process-lifetime static storage; the
            // callback never dereferences it or any callback argument. Both
            // centers are CoreFoundation singletons and the registration is
            // intentionally process-lifetime, so no dangling owner is created.
            unsafe {
                center.add_observer(
                    observer,
                    Some(font_catalog_changed),
                    Some(kCTFontManagerRegisteredFontsChangedNotification),
                    std::ptr::null(),
                    CFNotificationSuspensionBehavior::Coalesce,
                );
            }
        }
    });
}

#[derive(Debug)]
pub(super) struct CoreTextCatalogMonitor {
    changes: FontCatalogChangeCursor,
}

impl Default for CoreTextCatalogMonitor {
    fn default() -> Self {
        install_observers();
        Self {
            changes: CATALOG_CHANGES.cursor(),
        }
    }
}

impl CoreTextCatalogMonitor {
    pub(super) fn poll(&mut self) -> FontCatalogChange {
        self.changes.poll(&CATALOG_CHANGES)
    }
}
