//! CG-5: release builds cannot construct a diagnostic cache-root snapshot.
//!
//! `CacheRootScan::Snapshot` carries a `SnapshotToken` payload whose only
//! constructor exists under `cfg(any(test, feature = "test-support"))`, and
//! the public `CacheRootScan::snapshot` helper is gated the same way.  The
//! variant itself is unconditional, so cross-crate matches never depend on
//! two crates' feature agreement.
//!
//! The fixture `#[path]`-includes the real module instead of linking the
//! workspace crate: trybuild's scratch project inherits this workspace's
//! dev-dependencies (with `neovm-value`'s test-support feature enabled), so
//! a dependency build would still expose the gated constructor.  A bin
//! target has neither `cfg(test)` nor the feature, which is exactly the
//! release configuration this contract pins.
//!
//! Both construction entry points are pinned: the public `snapshot` helper
//! and the token constructor itself must both be absent here.

#[path = "../../../neovm-value/src/tagged/gc/cache_root_scan.rs"]
#[allow(dead_code, unexpected_cfgs)]
mod cache_root_scan;

fn main() {
    let _scan = cache_root_scan::CacheRootScan::snapshot(false);
    let _token = cache_root_scan::SnapshotToken::new();
}
