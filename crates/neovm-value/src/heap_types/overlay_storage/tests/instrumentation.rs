//! Test-only storage counters; no production statics or TLS.

#[cfg(any(test, feature = "test-support"))]
static OVERLAYS_AT_NODE_VISITS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub(crate) fn reset_overlays_at_node_visit_count() {
    OVERLAYS_AT_NODE_VISITS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn overlays_at_node_visit_count() -> usize {
    OVERLAYS_AT_NODE_VISITS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn record_overlays_at_node_visit() {
    OVERLAYS_AT_NODE_VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, feature = "test-support"))]
static OVERLAY_ITERATOR_FRAME_PUSHES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub(crate) fn reset_overlay_iterator_frame_push_count() {
    OVERLAY_ITERATOR_FRAME_PUSHES.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn overlay_iterator_frame_push_count() -> usize {
    OVERLAY_ITERATOR_FRAME_PUSHES.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn record_overlay_iterator_frame_push() {
    OVERLAY_ITERATOR_FRAME_PUSHES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, feature = "test-support"))]
static OVERLAY_SHIFT_NODE_VISITS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub(crate) fn reset_overlay_shift_node_visit_count() {
    OVERLAY_SHIFT_NODE_VISITS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn overlay_shift_node_visit_count() -> usize {
    OVERLAY_SHIFT_NODE_VISITS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn record_overlay_shift_node_visit() {
    OVERLAY_SHIFT_NODE_VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, feature = "test-support"))]
static ENDPOINT_SEARCH_NODE_VISITS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub(crate) fn reset_endpoint_search_node_visit_count() {
    ENDPOINT_SEARCH_NODE_VISITS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn endpoint_search_node_visit_count() -> usize {
    ENDPOINT_SEARCH_NODE_VISITS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn record_endpoint_search_node_visit() {
    ENDPOINT_SEARCH_NODE_VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, feature = "test-support"))]
static ENDPOINT_SEARCH_SUMMARY_SHIFTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub(crate) fn reset_endpoint_search_summary_shift_count() {
    ENDPOINT_SEARCH_SUMMARY_SHIFTS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn endpoint_search_summary_shift_count() -> usize {
    ENDPOINT_SEARCH_SUMMARY_SHIFTS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn record_endpoint_search_summary_shift() {
    ENDPOINT_SEARCH_SUMMARY_SHIFTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, feature = "test-support"))]
static OVERLAY_FULL_ENUMERATION_VISITS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub(crate) fn reset_overlay_full_enumeration_visit_count() {
    OVERLAY_FULL_ENUMERATION_VISITS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn overlay_full_enumeration_visit_count() -> usize {
    OVERLAY_FULL_ENUMERATION_VISITS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn record_overlay_full_enumeration_visit() {
    OVERLAY_FULL_ENUMERATION_VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
