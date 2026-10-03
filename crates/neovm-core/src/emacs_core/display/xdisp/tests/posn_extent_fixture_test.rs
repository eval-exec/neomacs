//! Explicit legacy bare-snapshot controls and simulated accepted TTY matrix
//! facts for the eight existing posn fixtures. Full live GNU expectations stay
//! in neomacs-tui-tests::posn_object_extent_oracle; these assertions isolate
//! the dimension cell without changing any fake fixture's physical geometry.
use super::*;
use crate::window::{FrameId, PosnObjectExtentMode, WindowId};
use neomacs_display_protocol::posn_object_extent::{PosnMatrixRow, PosnMatrixSnapshot};

/// Restores the numeric test selector on normal return and Rust unwinding.
/// The !Send/!Sync marker keeps the guard on its owning test mutator thread;
/// it owns no Lisp state and production has no override or additional TLS.
pub(super) struct PosnExtentFixtureGuard {
    previous: Option<PosnObjectExtentMode>,
    _same_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl PosnExtentFixtureGuard {
    pub(super) fn set(mode: PosnObjectExtentMode) -> Self {
        Self {
            previous: crate::window::force_posn_object_extent_for_test(Some(mode)),
            _same_thread: std::marker::PhantomData,
        }
    }
}

impl Drop for PosnExtentFixtureGuard {
    fn drop(&mut self) {
        crate::window::force_posn_object_extent_for_test(self.previous);
    }
}

/// Attach explicitly accepted numeric TTY rows to an already retained fixture.
/// ROW is the raw output-matrix row, including chrome, not the body-relative
/// row reported to Lisp. USED is an explicit materialized TEXT glyph count,
/// never inferred from point width, rectangle extents, or source length.
/// Other slots are disabled. A stored TTY glyph has width1/ascent+descent0;
/// row height is1 (GNU term.c append_glyph and dispnew.c buffer_posn_from_coords).
/// The old physical geometry remains unchanged to test that distinction.
/// These immutable numeric snapshots belong to this test Context only.
pub(super) fn install_accepted_tty_fixture_rows(
    eval: &mut Context,
    frame_id: FrameId,
    windows: &[(WindowId, usize, usize)],
) {
    let frame = eval.frames.get_mut(frame_id).expect("frame");
    let snapshots = windows
        .iter()
        .map(|&(window, row, used)| {
            let mut snapshot = frame
                .redisplay_snapshot(window)
                .expect("retained fixture")
                .clone();
            let mut rows = (0..=row)
                .map(|index| PosnMatrixRow {
                    enabled: false,
                    y: index as i64,
                    height: 1,
                    areas: [0; 3],
                })
                .collect::<Vec<_>>();
            rows[row].enabled = true;
            rows[row].areas[neomacs_display_protocol::glyph_matrix::GlyphArea::Text.index()] = used;
            snapshot.posn_matrix = Some(std::sync::Arc::new(PosnMatrixSnapshot { rows }));
            snapshot
        })
        .collect();
    frame.replace_redisplay_cache_for_test(snapshots);
}

#[test]
fn posn_extent_fixture_guard_restores_nested_policy_on_unwind() {
    let initial = crate::window::posn_object_extent_mode();
    {
        let _outer = PosnExtentFixtureGuard::set(PosnObjectExtentMode::Off);
        assert_eq!(
            crate::window::posn_object_extent_mode(),
            PosnObjectExtentMode::Off
        );
        let result = std::panic::catch_unwind(|| {
            let _inner = PosnExtentFixtureGuard::set(PosnObjectExtentMode::On);
            assert_eq!(
                crate::window::posn_object_extent_mode(),
                PosnObjectExtentMode::On
            );
            panic!("exercise test-selector scope cleanup");
        });
        assert!(result.is_err());
        assert_eq!(
            crate::window::posn_object_extent_mode(),
            PosnObjectExtentMode::Off
        );
    }
    assert_eq!(crate::window::posn_object_extent_mode(), initial);
}
