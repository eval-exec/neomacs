//! The evaluator's half of the runtime's real-layout pointer-routing tests
//! (DECISIONS D44, ruling (b)): the runtime consumes recorded layout output
//! (`neomacs_display_runtime::testing::real_layout`), and these tests lay
//! the same scenarios out live and require each recorded state to
//! materialize to the `FrameGlyphBuffer` the live state materializes to,
//! which is the value the runtime consumes.
//!
//! With `NEOMACS_BLESS_REAL_LAYOUT_FIXTURES` set, each test writes the live
//! states over the recorded fixture and then fails: the runtime embeds the
//! fixtures when it is compiled, so a blessing run compares nothing, and a
//! leaked variable must not pass for verification. A second run without it
//! verifies the result.

use neomacs_display_protocol::SealedFramePresentation;
use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
use neomacs_display_runtime::testing::real_layout::RealLayoutScenario;
use neomacs_layout_engine::engine::FrameLayoutAttempt;
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::eval::Context;
use neovm_core::window::{DisplayFrameManagerExt, FrameId};

const BLESS: &str = "NEOMACS_BLESS_REAL_LAYOUT_FIXTURES";

/// Lay `frames` out in order through one engine without native font
/// metrics, as the runtime tests' scenarios were laid out.
fn layout(eval: &mut Context, frames: [FrameId; 3]) -> [SealedFramePresentation; 3] {
    let mut engine = neomacs_layout_engine::LayoutEngine::new_without_font_metrics();
    frames.map(
        |frame_id| match engine.redisplay_frame_attempt(eval, frame_id) {
            FrameLayoutAttempt::Prepared(frame) => frame,
            FrameLayoutAttempt::Aborted => panic!("real-layout fixture layout aborted"),
        },
    )
}

/// A 200x120 root frame and two 80x60 children at (20, 15), at z-orders 2
/// and 9, over a buffer holding "child".
fn live_overlapping_children() -> [SealedFramePresentation; 3] {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("child");
    let root = eval
        .frame_manager_mut()
        .create_frame("root-hit", 200, 120, buffer);
    let lower = eval
        .frame_manager_mut()
        .create_frame("lower-hit", 80, 60, buffer);
    let upper = eval
        .frame_manager_mut()
        .create_frame("upper-hit", 80, 60, buffer);
    for (frame_id, parent, z_order) in [
        (root, None, 0),
        (lower, Some(root), 2),
        (upper, Some(root), 9),
    ] {
        let frame = eval.frame_manager_mut().get_mut(frame_id).unwrap();
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.z_order = z_order;
        if let Some(parent) = parent {
            frame.parent_frame = Value::make_frame(parent.0);
            frame.left_pos = 20;
            frame.top_pos = 15;
        }
    }
    layout(&mut eval, [root, lower, upper])
}

/// An 800x600 root frame, a 300x200 child at (100, 80) and a 100x80
/// grandchild at (15, 12) inside the child.
fn live_nested_child() -> [SealedFramePresentation; 3] {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let root = eval
        .frame_manager_mut()
        .create_frame("root-nested", 800, 600, buffer);
    let parent = eval
        .frame_manager_mut()
        .create_frame("parent-nested", 300, 200, buffer);
    let nested = eval
        .frame_manager_mut()
        .create_frame("nested", 100, 80, buffer);
    for frame_id in [root, parent, nested] {
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .unwrap()
            .set_window_system(Some(Value::symbol("neo")));
    }
    {
        let frame = eval.frame_manager_mut().get_mut(parent).unwrap();
        frame.parent_frame = Value::make_frame(root.0);
        frame.left_pos = 100;
        frame.top_pos = 80;
    }
    {
        let frame = eval.frame_manager_mut().get_mut(nested).unwrap();
        frame.parent_frame = Value::make_frame(parent.0);
        frame.left_pos = 15;
        frame.top_pos = 12;
    }
    layout(&mut eval, [root, parent, nested])
}

fn fixture_path(scenario: RealLayoutScenario) -> std::path::PathBuf {
    neomacs_infra::crate_root!()
        .parent()
        .expect("crates directory")
        .join("neomacs-display-runtime/src/testing/real_layout")
        .join(scenario.file_name())
}

fn assert_fixture_matches(scenario: RealLayoutScenario, live: [SealedFramePresentation; 3]) {
    if std::env::var_os(BLESS).is_some() {
        let states: Vec<&FrameDisplayState> = live.iter().map(|frame| frame.state()).collect();
        // Compact: the fixtures are generated, and pretty-printing more than
        // doubles their size.
        let mut json = serde_json::to_string(&states).expect("serializable frame states");
        json.push('\n');
        let path = fixture_path(scenario);
        std::fs::write(&path, json).expect("write the real-layout fixture");
        panic!(
            "{BLESS} is set: rewrote {} from live layout and compared nothing; \
             rerun without {BLESS} to verify it",
            path.display()
        );
    }
    for (index, (recorded, live)) in scenario.frames().iter().zip(&live).enumerate() {
        assert_eq!(
            recorded.materialize(),
            live.materialize(),
            "frame {index} of {scenario:?} no longer matches live layout; \
             regenerate the fixture with {BLESS}=1"
        );
    }
}

/// Backs `render_thread::input::tests::real_layout_publication_routes_overlapping_children_by_published_z_order`.
#[test]
fn real_layout_fixture_overlapping_children_matches_live_layout() {
    assert_fixture_matches(
        RealLayoutScenario::OverlappingChildren,
        live_overlapping_children(),
    );
}

/// Backs `render_thread::input::tests::real_layout_publication_keeps_nested_child_parent_relative_and_runtime_places_it_in_root`.
#[test]
fn real_layout_fixture_nested_child_matches_live_layout() {
    assert_fixture_matches(RealLayoutScenario::NestedChild, live_nested_child());
}
