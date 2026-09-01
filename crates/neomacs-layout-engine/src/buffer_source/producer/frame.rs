//! The producer's replacement frame.
//!
//! GNU pushes an iterator frame when a `display` property replaces buffer text
//! (`push_it (it, &E)`, xdisp.c:5974 + 7385-7416) and pops back to E when the
//! replacement is exhausted. E — the covered range's end — is not a detail of
//! the push: getting it wrong double-displays the replacement or swallows text
//! that is its own replacement, which is what GNU's own comment at
//! xdisp.c:6337-6363 warns about and what this engine reproduced in both
//! directions before `ebb21d354`.
//!
//! [`ReplacementCoveredSpan`] exists so that E cannot be computed by anyone
//! else. Its producer-side constructor is the ONLY way to derive one from a
//! `display` property, and it takes a [`CharPropertySource`] rather than a bare
//! value, so the caller cannot reach a resume position without having said
//! which source won. That is the whole point of the type: before it, the
//! resolver discarded the winning overlay and no caller COULD branch correctly.
//!
//! Since P4.8 that is literally true: the routed row planner used to walk the
//! same-object scan inline in byte space and hand the result over through a
//! second constructor. It now calls [`ReplacementCoveredSpan::for_property_source`]
//! through its own [`DisplayReplacementExtentLookup`] and CARRIES the span into
//! its plan, so there is exactly one statement of the rule and no way to
//! assemble a span out of two loose positions.

use crate::neovm_bridge::CharPropertySource;
use neovm_core::buffer::CharPos0;
use neovm_core::emacs_core::Value;

/// The buffer lookups deriving E needs, as seen from the producer's cursor.
/// A narrow trait rather than the cursor itself keeps the rule readable and
/// keeps [`ReplacementCoveredSpan`] free of the cursor's lifetimes.
pub(crate) trait DisplayReplacementExtentLookup {
    /// The walk's end bound; every answer is clipped to it, as GNU clips an
    /// overlay end to the accessible portion.
    fn extent_scan_end(&self) -> CharPos0;

    /// Where `overlay` ends, in char positions.
    fn extent_overlay_end(&self, overlay: Value) -> Option<CharPos0>;

    /// The `display` property in effect at `at`, resolved the same way the
    /// producer resolves it.
    fn extent_display_prop_at(&self, at: CharPos0) -> Option<Value>;

    /// The next position at which any property changes (this engine's
    /// `compute_stop_pos` mirror).
    fn extent_next_property_change(&self, at: CharPos0) -> CharPos0;
}

/// The buffer range one replacement stands for: `[start, resume)`.
///
/// `start` is GNU's B (what the replacement's glyphs are stamped with, design
/// section 4.7) and `resume` is GNU's E (where the walk continues once the
/// replacement is done). Both are producer-owned; the renderer applies the
/// resume but never derives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReplacementCoveredSpan {
    start: CharPos0,
    resume: CharPos0,
}

impl ReplacementCoveredSpan {
    /// Derive E for a `display` property, branching on which source supplied
    /// it. This is the sole entry from a property, and it takes the SOURCE
    /// rather than the value precisely so the branch cannot be skipped.
    ///
    /// An OVERLAY-sourced property covers exactly its overlay (GNU
    /// xdisp.c:6337-6363 takes `OVERLAY_END` unconditionally). GNU's comment
    /// names both failure directions of the alternative, and both were live
    /// here: scanning forward "will happily find another display property
    /// coming from some other overlay or text property on buffer positions
    /// before this overlay's end", so we "risk displaying this overlay's
    /// display string/image twice, or fail to display text that should be".
    ///
    /// A TEXT-property one ends at the next `display` change, i.e. the
    /// same-object scan below (GNU `display_prop_end` ->
    /// `Fnext_single_char_property_change`).
    pub(crate) fn for_property_source(
        source: CharPropertySource,
        start: CharPos0,
        property_end: CharPos0,
        lookup: &impl DisplayReplacementExtentLookup,
    ) -> Self {
        let scan_end = lookup.extent_scan_end();
        let resume = match source.overlay {
            Some(overlay) => lookup
                .extent_overlay_end(overlay)
                .unwrap_or(property_end)
                .max(property_end)
                .min(scan_end),
            None => {
                let mut extent = property_end;
                while extent < scan_end {
                    match lookup.extent_display_prop_at(extent) {
                        Some(next) if next.bits() == source.value.bits() => {
                            extent = lookup
                                .extent_next_property_change(extent)
                                .max(extent.add_len(neovm_core::buffer::CharLen::new(1)))
                                .min(scan_end);
                        }
                        _ => break,
                    }
                }
                extent
            }
        };
        Self { start, resume }
    }

    /// The covered range of a replacement that spans exactly ONE property run.
    ///
    /// Used by the inline source-items path, which resolves a replacement
    /// string without the typed item's whole-value extent because it pushes the
    /// string onto the source stack rather than handing a covered range to the
    /// renderer. It is not a way around
    /// [`Self::for_property_source`]: there is no `display` value to take an
    /// extent over here, the run's own end IS the range.
    pub(crate) fn for_single_property_run(start: CharPos0, property_end: CharPos0) -> Self {
        Self {
            start,
            resume: property_end,
        }
    }

    /// How many buffer chars the replacement stands for.
    pub(crate) fn covered_char_len(self) -> usize {
        self.resume.get().saturating_sub(self.start.get())
    }

    /// GNU's B: the position the replacement's glyphs are attributed to.
    pub(crate) fn start(self) -> CharPos0 {
        self.start
    }

    /// GNU's E: where the walk resumes once the replacement is done.
    pub(crate) fn resume(self) -> CharPos0 {
        self.resume
    }
}
