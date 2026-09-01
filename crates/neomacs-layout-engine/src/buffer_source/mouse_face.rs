//! Effective `mouse-face` runs for buffer redisplay.
//!
//! Redisplay already knows a short run over which no text property or overlay
//! boundary occurs. This module combines that local stability proof with GNU
//! char-property resolution and monotonic overlay-property sweeps bounded to
//! the relevant positive semantic range. Negative results cache only the
//! caller's local proof; they never trigger an exact buffer-wide extent query.

use crate::neovm_bridge::{LayoutBufferView, LayoutCharPropertyLookup};
use neovm_core::buffer::overlay::{
    NonNilPropertyValue, OverlayPropertyAtPoint, OverlayPropertyFilter, OverlayPropertyResolver,
    OverlayPropertySweep,
};
use neovm_core::buffer::{EmacsBytePos, EmacsByteRange};
use neovm_core::emacs_core::Value;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
static TEXT_MOUSE_FACE_EXTENT_QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
static OVERLAY_MOUSE_FACE_SWEEP_START_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
static OVERLAY_MOUSE_FACE_PROPERTY_QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_text_mouse_face_extent_query_count() {
    TEXT_MOUSE_FACE_EXTENT_QUERY_COUNT.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn text_mouse_face_extent_query_count() -> usize {
    TEXT_MOUSE_FACE_EXTENT_QUERY_COUNT.load(Ordering::Relaxed)
}

#[cfg(test)]
pub(crate) fn reset_overlay_mouse_face_sweep_start_count() {
    OVERLAY_MOUSE_FACE_SWEEP_START_COUNT.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn overlay_mouse_face_sweep_start_count() -> usize {
    OVERLAY_MOUSE_FACE_SWEEP_START_COUNT.load(Ordering::Relaxed)
}

#[cfg(test)]
pub(crate) fn reset_overlay_mouse_face_property_query_count() {
    OVERLAY_MOUSE_FACE_PROPERTY_QUERY_COUNT.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn overlay_mouse_face_property_query_count() -> usize {
    OVERLAY_MOUSE_FACE_PROPERTY_QUERY_COUNT.load(Ordering::Relaxed)
}

fn record_text_extent_query() {
    #[cfg(test)]
    TEXT_MOUSE_FACE_EXTENT_QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
}

fn record_overlay_sweep_start() {
    #[cfg(test)]
    OVERLAY_MOUSE_FACE_SWEEP_START_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// A non-empty redisplay run beginning at the point being resolved.
///
/// This is a local cache-validity proof, not the semantic pointer identity
/// range.  Keeping those concepts distinct prevents unrelated boundaries from
/// changing pointer identity while still forcing re-resolution when an overlay
/// can start or stop winning.
#[derive(Clone, Copy)]
pub(super) struct MouseFaceStableRun(EmacsByteRange);

impl MouseFaceStableRun {
    pub(super) fn starting_at(start: EmacsBytePos, end: EmacsBytePos) -> Self {
        assert!(start < end, "a mouse-face stable run must be non-empty");
        Self(EmacsByteRange::new(start, end))
    }

    fn range(self) -> EmacsByteRange {
        self.0
    }
}

#[derive(Clone, Copy)]
struct NonNilMouseFace(NonNilPropertyValue);

impl NonNilMouseFace {
    fn new(value: Value) -> Option<Self> {
        NonNilPropertyValue::new(value).map(Self)
    }

    fn value(self) -> Value {
        self.0.value()
    }

    fn property_value(self) -> NonNilPropertyValue {
        self.0
    }
}

#[derive(Clone, Copy)]
enum MouseFaceOwner {
    Text,
    Overlay(Value),
}

impl MouseFaceOwner {
    fn overlay(self) -> Option<Value> {
        match self {
            Self::Text => None,
            Self::Overlay(overlay) => Some(overlay),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ResolvedMouseFace {
    value: NonNilMouseFace,
    range: EmacsByteRange,
    owner: MouseFaceOwner,
}

impl ResolvedMouseFace {
    pub(super) fn value(self) -> Value {
        self.value.value()
    }

    pub(super) fn range(self) -> EmacsByteRange {
        self.range
    }

    pub(super) fn overlay_owner(self) -> Option<Value> {
        self.owner.overlay()
    }
}

#[derive(Clone, Copy)]
struct CachedMouseFaceRun {
    valid: EmacsByteRange,
    resolved: Option<ResolvedMouseFace>,
}

impl CachedMouseFaceRun {
    fn absent(valid: EmacsByteRange) -> Self {
        Self {
            valid,
            resolved: None,
        }
    }

    fn active(valid: EmacsByteRange, resolved: ResolvedMouseFace) -> Self {
        Self {
            valid,
            resolved: Some(resolved),
        }
    }

    fn contains(self, bytepos: EmacsBytePos) -> bool {
        self.valid.start() <= bytepos && bytepos < self.valid.end()
    }
}

struct MouseFaceOverlayProperty<'a, B: LayoutBufferView + ?Sized> {
    buffer: &'a B,
    property: LayoutCharPropertyLookup,
}

impl<B: LayoutBufferView + ?Sized> OverlayPropertyResolver for MouseFaceOverlayProperty<'_, B> {
    fn value_for_overlay(&mut self, overlay: Value) -> Option<Value> {
        #[cfg(test)]
        OVERLAY_MOUSE_FACE_PROPERTY_QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
        self.property.effective_overlay_value(self.buffer, overlay)
    }

    fn endpoint_filter(&self) -> OverlayPropertyFilter {
        self.property.overlay_endpoint_filter()
    }
}

/// Stateful resolver for effective buffer `mouse-face` runs.
pub(super) struct MouseFaceRuns<'a, B: LayoutBufferView + ?Sized> {
    buffer: &'a B,
    display_bounds: EmacsByteRange,
    semantic_bounds: EmacsByteRange,
    window_id: Option<u64>,
    cached: Option<CachedMouseFaceRun>,
    property: LayoutCharPropertyLookup,
    overlays: Option<OverlayPropertySweep<'a, MouseFaceOverlayProperty<'a, B>>>,
}

impl<'a, B: LayoutBufferView + ?Sized> MouseFaceRuns<'a, B> {
    pub(super) fn new(buffer: &'a B, bounds: EmacsByteRange, window_id: Option<u64>) -> Self {
        let property = LayoutCharPropertyLookup::new(buffer, Value::symbol("mouse-face"));
        Self {
            buffer,
            display_bounds: bounds,
            semantic_bounds: EmacsByteRange::new(
                buffer.layout_point_min_emacs_byte_pos(),
                buffer.layout_point_max_emacs_byte_pos(),
            ),
            window_id,
            cached: None,
            property,
            overlays: None,
        }
    }

    pub(super) fn resolve(
        &mut self,
        bytepos: EmacsBytePos,
        stable: MouseFaceStableRun,
    ) -> Option<ResolvedMouseFace> {
        debug_assert_eq!(stable.range().start(), bytepos);
        debug_assert!(self.display_bounds.start() <= bytepos);
        debug_assert!(stable.range().end() <= self.display_bounds.end());
        if let Some(cached) = self.cached.filter(|cached| cached.contains(bytepos)) {
            return cached.resolved;
        }

        let mut overlay_run = self
            .overlays
            .as_mut()
            .and_then(|sweep| sweep.partition_at(bytepos));
        let mut discovered_text = None;
        let mut discovered_text_extent = None;
        if overlay_run.is_none() {
            // A display retry may seek before the point where a formerly lazy
            // positive sweep began. Recreate it from the new cheap at-point
            // proof instead of making the core cursor support unbounded seeks.
            self.overlays = None;
            let buffer: &'a B = self.buffer;
            let property_value = MouseFaceOverlayProperty {
                buffer,
                property: self.property.clone(),
            };
            let at_point = buffer
                .layout_overlays()
                .resolve_overlay_property_at_emacs_byte_pos(
                    bytepos,
                    self.window_id,
                    property_value,
                );
            let sweep = match at_point {
                OverlayPropertyAtPoint::Present(resolution) => {
                    resolution.sweep(self.semantic_bounds)
                }
                OverlayPropertyAtPoint::Vacant(vacancy) => {
                    discovered_text = self
                        .property
                        .text_value_at(buffer, bytepos)
                        .and_then(NonNilMouseFace::new);
                    let Some(text) = discovered_text else {
                        let cached = CachedMouseFaceRun::absent(stable.range());
                        self.cached = Some(cached);
                        return None;
                    };
                    record_text_extent_query();
                    let text_extent = self
                        .property
                        .effective_text_extent_at(buffer, bytepos, self.semantic_bounds)
                        .expect("an active text property has a non-empty extent");
                    discovered_text_extent = Some(text_extent);
                    vacancy
                        .with_fallback(text.property_value())
                        .sweep(text_extent)
                }
            }
            .expect("mouse-face lookup lies inside its semantic bounds");
            record_overlay_sweep_start();
            self.overlays = Some(sweep);
            overlay_run = self
                .overlays
                .as_mut()
                .and_then(|sweep| sweep.partition_at(bytepos));
        }
        let mut overlay_run =
            overlay_run.expect("mouse-face lookup lies inside its bounded overlay sweep");

        // A text-bounded vacancy sweep deliberately stops at the text extent
        // so a short property cannot scan unrelated later endpoints. If an
        // overlay starts inside that extent and may continue beyond it,
        // promote that positive owner to a semantic-bounds sweep before
        // publishing its pointer identity.
        let needs_overlay_promotion = overlay_run.winner().is_some()
            && self
                .overlays
                .as_ref()
                .is_some_and(|sweep| sweep.traversal_end() < self.semantic_bounds.end());
        if needs_overlay_promotion {
            self.overlays = None;
            let buffer: &'a B = self.buffer;
            let property_value = MouseFaceOverlayProperty {
                buffer,
                property: self.property.clone(),
            };
            let OverlayPropertyAtPoint::Present(resolution) = buffer
                .layout_overlays()
                .resolve_overlay_property_at_emacs_byte_pos(
                    bytepos,
                    self.window_id,
                    property_value,
                )
            else {
                unreachable!("the bounded sweep proved an overlay winner at this point");
            };
            let sweep = resolution
                .sweep(self.semantic_bounds)
                .expect("positive mouse-face lies inside semantic bounds");
            record_overlay_sweep_start();
            self.overlays = Some(sweep);
            overlay_run = self
                .overlays
                .as_mut()
                .and_then(|sweep| sweep.partition_at(bytepos))
                .expect("promoted overlay sweep contains its starting point");
        }

        let cached = match overlay_run.winner() {
            Some(winner) => {
                let value = NonNilMouseFace::new(winner.value())
                    .expect("effective mouse-face resolution is non-nil");
                let owner = winner.overlay();
                let resolved = ResolvedMouseFace {
                    value,
                    range: overlay_run.range(),
                    owner: MouseFaceOwner::Overlay(owner),
                };
                CachedMouseFaceRun::active(overlay_run.range(), resolved)
            }
            None => {
                let text_value = discovered_text.or_else(|| {
                    self.property
                        .text_value_at(self.buffer, bytepos)
                        .and_then(NonNilMouseFace::new)
                });
                if let Some(value) = text_value {
                    let text_extent = discovered_text_extent.unwrap_or_else(|| {
                        record_text_extent_query();
                        self.property
                            .effective_text_extent_at(self.buffer, bytepos, self.semantic_bounds)
                            .expect("an active text property has a non-empty extent")
                    });
                    let effective_extent = EmacsByteRange::new(
                        text_extent.start().max(overlay_run.range().start()),
                        text_extent.end().min(overlay_run.range().end()),
                    );
                    debug_assert!(!effective_extent.is_empty());
                    let resolved = ResolvedMouseFace {
                        value,
                        range: effective_extent,
                        owner: MouseFaceOwner::Text,
                    };
                    CachedMouseFaceRun::active(effective_extent, resolved)
                } else {
                    // The caller's redisplay run is the complete local proof
                    // needed for absence.  A maximal negative overlay extent
                    // would inspect unrelated endpoints throughout the buffer.
                    CachedMouseFaceRun::absent(stable.range())
                }
            }
        };
        self.cached = Some(cached);
        cached.resolved
    }
}
