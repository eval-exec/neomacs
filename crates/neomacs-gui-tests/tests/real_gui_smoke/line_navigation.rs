//! #607: ten line moves per second must not repaint all text at display rate.
use super::*;

#[test]
fn timer_line_navigation_reuses_scene_during_cursor_motion() {
    let Some(backend) = requested_backend() else {
        return;
    };
    let root = workspace_root();
    let artifacts = root.join("target/neomacs-gui-tests");
    if backend == GuiBackend::LinuxWayland {
        // Pixman exposes only software surface capabilities. Use the existing
        // GPU-capable benchmark display for this animation regression.
        use neomacs_infra::display::{WestonBenchConfig, WestonBenchSession, WestonDesktop};
        let session = WestonBenchSession::start(
            &artifacts.join("line-navigation-display"),
            WestonBenchConfig {
                width: 1280,
                height: 800,
                xwayland: false,
                desktop: WestonDesktop::Solid,
            },
        )
        .expect("GPU-capable benchmark display");
        run_navigation(backend, &root, &artifacts, session.env());
    } else {
        let session = DisplayHarness::for_backend(backend)
            .start_session(&artifacts)
            .expect("display session");
        run_navigation(backend, &root, &artifacts, session.env());
    }
}

fn run_navigation(
    backend: GuiBackend,
    root: &std::path::Path,
    artifacts: &std::path::Path,
    env: &[(String, String)],
) {
    let scenario = GuiScenario::new(
        "line-navigation",
        root.join("crates/neomacs-gui-tests/fixtures/line-navigation.el"),
    );
    let mut plan = GuiTestPlan::new(backend, root, artifacts, scenario)
        .with_program(neomacs_binary(root))
        .with_env(
            "RUST_LOG",
            "warn,neomacs_display_runtime::render_thread::frame_stats=debug,neomacs_display_runtime::render_thread::state=info",
        );
    // Preserve the caller's Vulkan loader selection (notably on NixOS).
    // Otherwise the headless child can silently select a software adapter,
    // whose quality policy intentionally disables cursor interpolation.
    for key in ["VK_DRIVER_FILES", "VK_ICD_FILENAMES", "VK_ADD_DRIVER_FILES"] {
        if let Ok(value) = std::env::var(key) {
            plan = plan.with_env(key, value);
        }
    }
    for (key, value) in env {
        plan = plan.with_env(key.clone(), value.clone());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(45)),
        )
        .expect("navigation run");
    assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
    let state: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&result.artifacts.gui_state).expect("navigation state"),
    )
    .unwrap();
    assert_eq!(state["final_line"], 261);
    let snapshot = std::fs::read_to_string(&result.artifacts.frame_snapshot_txt).unwrap();
    assert!(snapshot.contains("*line-navigation*"), "{snapshot}");
    assert!(
        snapshot.contains("Line 259:") && snapshot.contains("L261"),
        "the displayed viewport and mode line must reach EOF: {snapshot}"
    );
    let log = std::fs::read_to_string(&result.artifacts.stdout).unwrap();
    let stats = log
        .lines()
        .rfind(|line| line.contains("frame_sched_stats"))
        .expect("frame counters must be enabled");
    let optional_count = |name: &str| -> Option<u64> {
        stats.split([' ', '"']).find_map(|field| {
            field
                .strip_prefix(&format!("{name}="))
                .and_then(|value| value.parse().ok())
        })
    };
    let count = |name: &str| -> u64 {
        stats
            .split_whitespace()
            .find_map(|field| field.strip_prefix(&format!("{name}=")))
            .unwrap_or_else(|| panic!("missing {name}: {stats}"))
            .parse()
            .unwrap()
    };
    let commits = count("scene_commits");
    assert!(
        commits > 100,
        "workload must publish actual navigation: {stats}"
    );
    // Software compatibility mode suppresses motion. The navigation and
    // snapshot checks above still run there; the animation budget needs an
    // adapter on which the motion producer actually runs.
    let motion = optional_count("cursor_motion").unwrap_or(0)
        + optional_count("cursor_animation").unwrap_or(0);
    if log.contains("quality_mode=SoftwareCompatibility") {
        eprintln!("software adapter: navigation verified without the animation budget: {stats}");
        return;
    }
    assert!(
        log.contains("quality_mode=Full"),
        "render quality must be observable"
    );
    assert!(
        motion > 100,
        "the hardware run must exercise cursor interpolation: {stats}"
    );
    assert!(
        count("composite_only_frames") > commits,
        "cursor interpolation must reuse the scene: {stats}"
    );
    assert!(
        count("root_glyph_passes") <= commits * 3,
        "root glyph work must follow scene commits, not the display-rate cursor clock: {stats}"
    );
}
