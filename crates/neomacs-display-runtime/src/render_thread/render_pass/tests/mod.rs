use super::*;
use neomacs_display_protocol::{DeviceScale, SurfaceState};

fn drawable() -> SurfaceState {
    SurfaceState::from_device_size(800, 600, DeviceScale::new(1.0).unwrap()).unwrap()
}

fn render_with_frame() -> GuiFrameRenderState {
    let mut render = GuiFrameRenderState::new_without_device(
        42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(
        Some(crate::core::frame_glyphs::FrameGlyphBuffer::with_size(
            800.0, 600.0,
        )),
        None,
        Default::default(),
        Default::default(),
    );
    render
}

#[test]
fn suspended_native_surface_is_window_not_ready_with_or_without_content() {
    let mut render = render_with_frame();
    render.set_surface_state(SurfaceState::Suspended);
    assert_eq!(
        composition_surface(&mut render, SurfaceState::Suspended),
        Err(FrameRenderFailure::WindowNotReady)
    );
    assert!(render.present_mapping().is_none());
    let mut empty = GuiFrameRenderState::new_without_device(
        42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    assert_eq!(
        composition_surface(&mut empty, SurfaceState::Suspended),
        Err(FrameRenderFailure::WindowNotReady)
    );
}

#[test]
fn drawable_native_surface_refreshes_suspended_cached_mapping() {
    let mut render = render_with_frame();
    render.set_surface_state(SurfaceState::Suspended);
    let SurfaceState::Drawable(surface) = drawable() else {
        unreachable!()
    };
    assert_eq!(composition_surface(&mut render, drawable()), Ok(surface));
    assert!(render.present_mapping().is_some());
}

#[test]
fn drawable_surface_without_editor_content_is_still_awaiting_content() {
    let mut render = GuiFrameRenderState::new_without_device(
        42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    assert_eq!(
        composition_surface(&mut render, drawable()),
        Err(FrameRenderFailure::AwaitingContent)
    );
}

/// Run the test named `exact` in a child test process under a 1 MiB GPU
/// budget, so the budget never leaks into concurrently running tests.
/// Returns true inside that child, where the caller runs its body. In the
/// parent it requires the child to have run exactly one test and passed:
/// libtest exits successfully when `--exact` matches nothing.
pub(super) fn in_gpu_budget_child(exact: &str) -> bool {
    if std::env::var("NEOMACS_GPU_BUDGET_MB").as_deref() == Ok("1") {
        return true;
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", exact, "--nocapture"])
        .env("NEOMACS_GPU_BUDGET_MB", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && ran_one_passing_test(&stdout),
        "budget child for {exact} did not run exactly one passing test\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    false
}

/// Whether libtest output reports exactly one passed and no failed test.
fn ran_one_passing_test(stdout: &str) -> bool {
    let mut results = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("test result: ok. "));
    let (Some(counts), None) = (results.next(), results.next()) else {
        return false;
    };
    let count = |label: &str| {
        counts
            .split("; ")
            .find_map(|part| part.strip_suffix(label)?.parse::<u32>().ok())
    };
    count(" passed") == Some(1) && count(" failed") == Some(0)
}

#[test]
fn budget_child_guard_rejects_runs_that_match_no_test() {
    let summary = |passed, filtered| {
        format!(
            "running {passed} test\n\ntest result: ok. {passed} passed; 0 failed; \
             0 ignored; 0 measured; {filtered} filtered out; finished in 0.01s\n"
        )
    };
    assert!(ran_one_passing_test(&summary(1, 41)));
    assert!(!ran_one_passing_test(&summary(0, 42)));
    assert!(!ran_one_passing_test(&summary(2, 40)));
    assert!(!ran_one_passing_test(""));
}
