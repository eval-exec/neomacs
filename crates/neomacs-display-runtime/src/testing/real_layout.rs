//! Real layout-engine output for the runtime's pointer-routing tests
//! (DECISIONS D44, ruling (b)).
//!
//! Each fixture holds the `FrameDisplayState`s the layout engine produced for
//! one evaluator scenario, root frame first, as JSON. The display host's
//! contract tests (`real_layout_fixture_test.rs`) lay the same scenarios out
//! and assert that each recorded state materializes to the
//! `FrameGlyphBuffer` the live layout materializes to, which is the value the
//! runtime consumes. After a layout change, regenerate the fixtures with
//!
//! ```sh
//! NEOMACS_BLESS_REAL_LAYOUT_FIXTURES=1 cargo nextest run -p neomacs-display-host \
//!     --features neovm-core/dbus-vendored -E 'test(~real_layout_fixture)'
//! ```
//!
//! That run rewrites both fixtures and fails by design, because it compares
//! nothing; the same command without the variable verifies them.

use neomacs_display_protocol::glyph_matrix::FrameDisplayState;

/// One recorded evaluator scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealLayoutScenario {
    /// A 200x120 root frame with two 80x60 child frames at (20, 15), z-orders
    /// 2 and 9, laid out without native font metrics.
    OverlappingChildren,
    /// An 800x600 root frame, a 300x200 child at (100, 80) and a 100x80
    /// grandchild at (15, 12) inside the child.
    NestedChild,
}

impl RealLayoutScenario {
    pub const ALL: [Self; 2] = [Self::OverlappingChildren, Self::NestedChild];

    /// The fixture's file name in `src/testing/real_layout/`.
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::OverlappingChildren => "overlapping_children.json",
            Self::NestedChild => "nested_child.json",
        }
    }

    /// The recorded frames: the root, its child, then the third frame (the
    /// upper child, or the grandchild).
    pub fn frames(self) -> [FrameDisplayState; 3] {
        let json = match self {
            Self::OverlappingChildren => include_str!("real_layout/overlapping_children.json"),
            Self::NestedChild => include_str!("real_layout/nested_child.json"),
        };
        serde_json::from_str(json).unwrap_or_else(|error| {
            panic!(
                "the {} fixture is not three FrameDisplayStates ({error}); \
                 regenerate it (see neomacs_display_runtime::testing::real_layout)",
                self.file_name()
            )
        })
    }
}
