//! #607 Stage 2A receipt-driven Wayland presentation & pacing acceptance tests.
//!
//! Split into two independent rows so software-only CI runners can never report
//! a misleading green `PASS` for GPU-only cursor-motion pacing:
//!
//! 1. `wayland_redisplay_presents_1_to_1_with_receipts_and_no_spurious_platform_redraw`
//!    (runs on both GPU hosts and pixman-only CI):
//!    - Proves 10 static cursor moves produce `platform_redraw == 0`, exact `1:1`
//!      `commits == 10` and `presents == 10`, and 10 distinct strictly-increasing
//!      `wp_presentation_feedback::Event::Presented` wire receipts.
//!    - Tries GL Weston first and falls back to Pixman Weston ONLY when `weston`
//!      reports a missing GL renderer module (e.g. CI's `-Drenderer-gl=false`
//!      build missing `gl-renderer.so`). Unexpected GPU/EGL/timeout failures
//!      panic loudly instead of silently downgrading.
//!
//! 2. `wayland_gpu_only_cursor_motion_presents_paced_receipts_without_unpaced_storm`
//!    (GPU-only row; excluded on software-only CI via `not test(~gpu_only_)`):
//!    - Unconditionally requires GL Weston (`WestonBenchSession::start`) and
//!      `quality_mode=Full` (zero software fallback, zero early return).
//!    - Proves an animated 20-line cursor jump drives frames via paced
//!      `cursor_motion` (`composite_only >= 3`, `cursor_motion >= 3`) without
//!      unpaced `cursor_animation` storms (`(1..=2).contains(&cursor_animation)`),
//!      with `platform_redraw == 0` and multiple distinct wire-verified
//!      `Presented` receipts.
//!    - On software-only CI runners, the cursor-motion pacing invariant is
//!      guarded by the CPU-only unit test
//!      `render_thread::tests::cursor_motion_mid_animation_stays_paced_without_unpaced_cursor_animation`.

#![cfg(target_os = "linux")]

use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

use neomacs_gui_tests::{HangCap, WaylandOutput, start_weston_with_desktop, unique_nonce};
use neomacs_infra::display::{
    DisplaySession, WestonBenchConfig, WestonBenchSession, WestonDesktop,
};
use serde_json::Value;

struct KillOnDrop(Option<Child>);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn is_missing_gl_renderer_error(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    let missing_gl_so = lower.contains("gl-renderer.so")
        && (lower.contains("no such file or directory")
            || lower.contains("cannot open shared object file")
            || lower.contains("failed to load module"));
    let unsupported_gl_renderer = (lower.contains("unknown renderer")
        || lower.contains("unrecognized renderer")
        || lower.contains("unsupported renderer"))
        && lower.contains("gl");
    missing_gl_so || unsupported_gl_renderer
}

enum WestonSession {
    Gl(WestonBenchSession),
    Pixman {
        session: DisplaySession,
        gl_fallback_reason: String,
    },
}

impl WestonSession {
    fn start_gl_only(artifacts: &Path) -> Self {
        let session = WestonBenchSession::start(
            &artifacts.join("display"),
            WestonBenchConfig {
                width: 1280,
                height: 800,
                xwayland: false,
                desktop: WestonDesktop::Solid,
            },
        )
        .unwrap_or_else(|err| {
            panic!("GPU-only presentation pacing row requires GL Weston (--renderer=gl): {err}")
        });
        Self::Gl(session)
    }

    fn start_gl_or_pixman_if_gl_module_missing(artifacts: &Path) -> Self {
        match WestonBenchSession::start(
            &artifacts.join("display"),
            WestonBenchConfig {
                width: 1280,
                height: 800,
                xwayland: false,
                desktop: WestonDesktop::Solid,
            },
        ) {
            Ok(session) => Self::Gl(session),
            Err(gl_err) => {
                let err_text = gl_err.to_string();
                assert!(
                    is_missing_gl_renderer_error(&err_text),
                    "GL Weston failed for a reason other than a missing GL renderer module; refusing to silently fall back to Pixman: {err_text}"
                );
                let gl_fallback_reason = err_text.lines().take(4).collect::<Vec<_>>().join(" | ");
                let session = start_weston_with_desktop(
                    &artifacts.join("display-pixman"),
                    WaylandOutput::Standard,
                    WestonDesktop::Solid,
                )
                .unwrap_or_else(|pixman_err| {
                    panic!(
                        "failed to start both GL Weston ({gl_fallback_reason}) and Pixman fallback Weston ({pixman_err})"
                    )
                });
                Self::Pixman {
                    session,
                    gl_fallback_reason,
                }
            }
        }
    }

    fn env(&self) -> &[(String, String)] {
        match self {
            Self::Gl(session) => session.env(),
            Self::Pixman { session, .. } => session.env(),
        }
    }

    fn renderer_description(&self) -> String {
        match self {
            Self::Gl(_) => "gl".to_owned(),
            Self::Pixman {
                gl_fallback_reason, ..
            } => format!("pixman (missing GL renderer module: {gl_fallback_reason})"),
        }
    }
}

struct TestRunContext {
    artifacts: PathBuf,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    port: u16,
    session: WestonSession,
    guard: KillOnDrop,
}

impl TestRunContext {
    fn launch(label: &str, session_builder: impl FnOnce(&Path) -> WestonSession) -> Self {
        let root = neomacs_infra::workspace_root();
        let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/release/neomacs"));
        assert!(
            binary.exists(),
            "build {binary:?} before running presentation pacing test"
        );

        let nonce = unique_nonce();
        let artifacts = root.join(format!(
            "target/neomacs-gui-tests/presentation-pacing-{label}-{nonce}"
        ));
        fs::create_dir_all(&artifacts).unwrap();
        let receipt_path = artifacts.join("presentation.sexp");
        let stdout_path = artifacts.join("stdout.log");
        let stderr_path = artifacts.join("stderr.log");
        let port = free_loopback_port();

        let session = session_builder(&artifacts);

        let fixture =
            root.join("crates/neomacs-gui-tests/fixtures/presentation-pacing-receipts.el");
        let mut cmd = Command::new(&binary);
        cmd.args([
            "-Q",
            "--font=DejaVu Sans Mono-13",
            "--geometry=100x35",
            "-l",
            fixture.to_str().unwrap(),
        ])
        .env_remove("DISPLAY")
        .env("NEOMACS_RUNTIME_ROOT", &root)
        .env("NEOMACS_DIAGNOSTICS_PORT", port.to_string())
        .env("NEOMACS_PACING_TEST_DIR", &artifacts)
        .env("NEOMACS_GUI_PRESENTATION_RECEIPT", &receipt_path)
        .env("WAYLAND_DEBUG", "client")
        .env(
            "RUST_LOG",
            "error,neomacs_display_runtime::render_thread::frame_stats=debug,neomacs_display_runtime::render_thread::state=info,neomacs_display_runtime::render_thread::gpu_startup=info",
        )
        .stdin(Stdio::null())
        .stdout(Stdio::from(fs::File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(fs::File::create(&stderr_path).unwrap()));
        // Preserve Vulkan ICD selection (notably on NixOS) so hardware hosts do
        // not silently drop into software compatibility mode.
        for key in ["VK_DRIVER_FILES", "VK_ICD_FILENAMES", "VK_ADD_DRIVER_FILES"] {
            if let Ok(value) = std::env::var(key) {
                cmd.env(key, value);
            }
        }
        for (key, value) in session.env() {
            cmd.env(key, value);
        }

        let guard = KillOnDrop(Some(cmd.spawn().expect("spawn neomacs")));
        Self {
            artifacts,
            stdout_path,
            stderr_path,
            port,
            session,
            guard,
        }
    }

    fn child_mut(&mut self) -> &mut Child {
        self.guard.0.as_mut().expect("editor process active")
    }

    fn wait_for_flag(&mut self, name: &str, timeout: Duration) {
        let path = self.artifacts.join(name);
        wait_for_file(&path, self.child_mut(), timeout, name);
    }

    fn write_flag(&self, name: &str, text: &str) {
        fs::write(self.artifacts.join(name), text).unwrap();
    }

    fn quit_and_collect(mut self) -> RunLogs {
        self.write_flag("quit", "quit\n");
        let child = self.child_mut();
        let cap = HangCap::new(Duration::from_secs(15));
        let status = loop {
            if let Some(status) = child.try_wait().expect("wait for clean exit") {
                break status;
            }
            cap.assert_not_expired("editor did not exit cleanly");
            thread::sleep(Duration::from_millis(20));
        };
        let _ = self.guard.0.take();
        assert!(
            status.success(),
            "editor exited with {status}; stderr={:?}",
            self.stderr_path
        );

        let stdout_raw = fs::read_to_string(&self.stdout_path).unwrap();
        let stdout = neomacs_gui_tests::interaction::strip_ansi(&stdout_raw);
        let device_line = stdout
            .lines()
            .find(|line| line.contains("device-type rank"))
            .unwrap_or("<adapter line absent>")
            .to_owned();
        let final_policy_line = stdout
            .lines()
            .rfind(|line| line.contains("applying render-quality policy"))
            .unwrap_or("<quality_mode line absent>")
            .to_owned();
        let wire_raw = fs::read_to_string(&self.stderr_path).unwrap();
        let wire = wire_raw
            .lines()
            .map(|line| neomacs_gui_tests::interaction::strip_ansi(line).into_owned())
            .collect();

        RunLogs {
            artifacts: self.artifacts,
            renderer: self.session.renderer_description(),
            device_line,
            final_policy_line,
            wire,
        }
    }
}

struct RunLogs {
    artifacts: PathBuf,
    renderer: String,
    device_line: String,
    final_policy_line: String,
    wire: Vec<String>,
}

impl RunLogs {
    fn read_receipts(&self, filename: &str) -> Vec<String> {
        fs::read_to_string(self.artifacts.join(filename))
            .unwrap()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}

fn free_loopback_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral loopback port")
        .local_addr()
        .expect("read ephemeral port")
        .port()
}

fn fetch_metrics(port: u16) -> Value {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect diagnostics server");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    stream
        .write_all(b"GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .expect("write GET /metrics");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read /metrics response");
    let (_, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("malformed HTTP response: {response}"));
    serde_json::from_str(body).unwrap_or_else(|err| panic!("parse /metrics JSON: {err}: {body}"))
}

fn wait_for_file(path: &Path, child: &mut Child, timeout: Duration, label: &str) {
    let cap = HangCap::new(timeout);
    loop {
        if path.exists() {
            return;
        }
        if let Some(status) = child.try_wait().expect("poll editor process") {
            panic!("editor exited early ({status}) while waiting for {label} ({path:?})");
        }
        cap.assert_not_expired(format!("waiting for {label} ({path:?})"));
        thread::sleep(Duration::from_millis(20));
    }
}

fn metric_u64(metrics: &Value, key: &str) -> u64 {
    metrics["frame"][key]
        .as_u64()
        .unwrap_or_else(|| panic!("missing numeric metric frame.{key} in {metrics}"))
}

fn demand_reason_u64(metrics: &Value, reason: &str) -> u64 {
    metrics["frame"]["demand_reasons"][reason]
        .as_u64()
        .unwrap_or_else(|| {
            panic!("missing numeric metric frame.demand_reasons.{reason} in {metrics}")
        })
}

fn parse_receipt_u64(receipt: &str, name: &str) -> u64 {
    let words: Vec<_> = receipt
        .trim()
        .trim_matches(['(', ')'])
        .split_whitespace()
        .collect();
    words
        .windows(2)
        .find(|pair| pair[0] == name)
        .unwrap_or_else(|| panic!("missing {name} in receipt: {receipt}"))[1]
        .parse()
        .unwrap_or_else(|err| panic!("invalid u64 for {name} in {receipt}: {err}"))
}

fn verify_receipts_distinct_and_on_wire(receipts: &[String], wire: &[String], label: &str) {
    let mut prev_submission = 0_u64;
    for receipt in receipts {
        let submission = parse_receipt_u64(receipt, ":submission");
        assert!(
            submission > prev_submission,
            "{label} receipts must have strictly increasing :submission serials: prev={prev_submission}, current={submission}, receipt={receipt}"
        );
        prev_submission = submission;

        let seconds = parse_receipt_u64(receipt, ":seconds");
        let nanoseconds = parse_receipt_u64(receipt, ":nanoseconds");
        let clock_id = parse_receipt_u64(receipt, ":clock-id");
        let timestamp = format!(
            ".presented({}, {}, {},",
            seconds >> 32,
            seconds & 0xffff_ffff,
            nanoseconds
        );
        let clock = format!(".clock_id({clock_id})");
        assert!(
            wire.iter()
                .any(|line| line.contains("wp_presentation_feedback") && line.contains(&timestamp)),
            "{label} receipt timestamp absent from native Presented events: {receipt}"
        );
        assert!(
            wire.iter()
                .any(|line| line.contains("wp_presentation") && line.contains(&clock)),
            "{label} receipt clock absent from native ClockId events: {receipt}"
        );
    }
}

#[test]
fn gl_fallback_classifier_only_matches_missing_gl_renderer_module() {
    assert!(is_missing_gl_renderer_error(
        "weston exited (exit status: 1) before creating Wayland socket /run/user/1000/wayland-0; log: Failed to load module: /usr/local/lib/x86_64-linux-gnu/libweston-16/gl-renderer.so: cannot open shared object file: No such file or directory | fatal: failed to create compositor backend"
    ));
    assert!(is_missing_gl_renderer_error(
        "weston exited (exit status: 1): unknown renderer \"gl\""
    ));
    assert!(!is_missing_gl_renderer_error(
        "weston exited (exit status: 1): failed to initialize egl: EGL_NOT_INITIALIZED"
    ));
    assert!(!is_missing_gl_renderer_error(
        "weston did not create Wayland socket /run/user/1000/wayland-0; log: starting headless backend"
    ));
    assert!(!is_missing_gl_renderer_error(
        "weston exited (exit status: 1): failed to open DRM render node /dev/dri/renderD128"
    ));
}

#[test]
// Prerequisites: requires release binary/pdump and headless Weston (GL, or Pixman when gl-renderer.so is absent).
fn wayland_redisplay_presents_1_to_1_with_receipts_and_no_spurious_platform_redraw() {
    let mut ctx = TestRunContext::launch(
        "phase1",
        WestonSession::start_gl_or_pixman_if_gl_module_missing,
    );

    ctx.wait_for_flag("ready", Duration::from_secs(30));
    let m0 = fetch_metrics(ctx.port);
    ctx.write_flag("phase1-go", "go\n");
    ctx.wait_for_flag("phase1-done", Duration::from_secs(30));
    let m1 = fetch_metrics(ctx.port);
    let logs = ctx.quit_and_collect();

    let phase1_receipts = logs.read_receipts("receipts-phase1.sexp");
    assert_eq!(
        phase1_receipts.len(),
        10,
        "every static cursor move must produce its own Presented receipt"
    );
    verify_receipts_distinct_and_on_wire(&phase1_receipts, &logs.wire, "phase1");

    let p1_commits = metric_u64(&m1, "scene_commits") - metric_u64(&m0, "scene_commits");
    let p1_presents = metric_u64(&m1, "presents") - metric_u64(&m0, "presents");
    let p1_platform_redraw =
        demand_reason_u64(&m1, "platform_redraw") - demand_reason_u64(&m0, "platform_redraw");

    eprintln!(
        "pacing-receipts phase1 weston: renderer={}\npacing-receipts device: {}\npacing-receipts policy: {}\npacing-receipts summary: p1(commits={p1_commits} presents={p1_presents} platform_redraw={p1_platform_redraw} receipts={})",
        logs.renderer,
        logs.device_line,
        logs.final_policy_line,
        phase1_receipts.len()
    );

    assert_eq!(
        p1_platform_redraw, 0,
        "unchanged geometry hints must not trigger spurious platform_redraw repaints"
    );
    assert_eq!(
        p1_commits, 10,
        "10 sequential static cursor moves must produce 10 commits: commits={p1_commits} presents={p1_presents}"
    );
    assert_eq!(
        p1_presents, 10,
        "10 sequential static cursor moves must produce exactly 10 presents (not 2x): presents={p1_presents} commits={p1_commits}"
    );
}

#[test]
// Prerequisites: requires release binary/pdump, GL Weston (--renderer=gl), and a hardware GPU adapter (quality_mode=Full).
fn wayland_gpu_only_cursor_motion_presents_paced_receipts_without_unpaced_storm() {
    let mut ctx = TestRunContext::launch("phase2-gpu", WestonSession::start_gl_only);

    ctx.wait_for_flag("ready", Duration::from_secs(30));
    ctx.write_flag("phase2-setup", "setup\n");
    ctx.wait_for_flag("phase2-ready", Duration::from_secs(30));
    let m2 = fetch_metrics(ctx.port);
    ctx.write_flag("phase2-go", "go\n");
    ctx.wait_for_flag("phase2-done", Duration::from_secs(30));
    let m3 = fetch_metrics(ctx.port);
    let logs = ctx.quit_and_collect();

    let phase2_receipts = logs.read_receipts("receipts-phase2.sexp");
    verify_receipts_distinct_and_on_wire(&phase2_receipts, &logs.wire, "phase2");

    let p2_platform_redraw =
        demand_reason_u64(&m3, "platform_redraw") - demand_reason_u64(&m2, "platform_redraw");
    let p2_cursor_motion =
        demand_reason_u64(&m3, "cursor_motion") - demand_reason_u64(&m2, "cursor_motion");
    let p2_cursor_anim =
        demand_reason_u64(&m3, "cursor_animation") - demand_reason_u64(&m2, "cursor_animation");
    let p2_composite_only =
        metric_u64(&m3, "composite_only_frames") - metric_u64(&m2, "composite_only_frames");
    let p2_presents = metric_u64(&m3, "presents") - metric_u64(&m2, "presents");

    eprintln!(
        "pacing-receipts phase2 weston: renderer={}\npacing-receipts device: {}\npacing-receipts policy: {}\npacing-receipts summary: p2(presents={p2_presents} composite_only={p2_composite_only} cursor_motion={p2_cursor_motion} cursor_animation={p2_cursor_anim} platform_redraw={p2_platform_redraw} receipts={})",
        logs.renderer,
        logs.device_line,
        logs.final_policy_line,
        phase2_receipts.len()
    );

    assert!(
        logs.final_policy_line.contains("quality_mode=Full"),
        "GPU-only cursor-motion pacing row requires quality_mode=Full (refusing software fallback): {}",
        logs.final_policy_line
    );
    assert_eq!(
        p2_platform_redraw, 0,
        "animated cursor move must not trigger spurious platform_redraw repaints"
    );
    assert!(
        phase2_receipts.len() >= 2,
        "quality_mode=Full animated jump must produce multiple distinct wire-verified Presented receipts, got {}",
        phase2_receipts.len()
    );
    assert!(
        p2_composite_only >= 3,
        "quality_mode=Full animated jump must render multiple composite-only cursor frames, got {p2_composite_only}"
    );
    assert!(
        p2_cursor_motion >= 3,
        "quality_mode=Full animated jump must be driven by paced CursorMotion demand, got {p2_cursor_motion}"
    );
    assert!(
        (1..=2).contains(&p2_cursor_anim),
        "quality_mode=Full animated jump must emit 1..=2 final-settle CursorAnimation(NextPresentation) frames without mid-motion storms: got cursor_animation={p2_cursor_anim} (cursor_motion={p2_cursor_motion}, composite_only={p2_composite_only})"
    );
}

fn verify_receipts_monotonic_and_wire_ordered(receipts: &[String], wire: &[String], label: &str) {
    verify_receipts_distinct_and_on_wire(receipts, wire, label);

    let mut prev_ns: Option<u128> = None;
    let mut prev_wire_pos: Option<usize> = None;
    for receipt in receipts {
        let seconds = parse_receipt_u64(receipt, ":seconds");
        let nanoseconds = parse_receipt_u64(receipt, ":nanoseconds");
        let ts_ns = u128::from(seconds) * 1_000_000_000 + u128::from(nanoseconds);
        if let Some(prev) = prev_ns {
            assert!(
                ts_ns >= prev,
                "{label} presentation timestamps must be monotonically non-decreasing: prev_ns={prev}, current_ns={ts_ns}, receipt={receipt}"
            );
        }
        prev_ns = Some(ts_ns);

        let timestamp = format!(
            ".presented({}, {}, {},",
            seconds >> 32,
            seconds & 0xffff_ffff,
            nanoseconds
        );
        let search_start = prev_wire_pos.map_or(0, |pos| pos + 1);
        let wire_pos = wire[search_start..]
            .iter()
            .position(|line| line.contains("wp_presentation_feedback") && line.contains(&timestamp))
            .map(|offset| search_start + offset)
            .unwrap_or_else(|| {
                panic!(
                    "{label} wire wp_presentation_feedback.presented event out of order or missing after line {search_start}: {receipt}"
                )
            });
        prev_wire_pos = Some(wire_pos);
    }

    if let (Some(first), Some(last)) = (receipts.first(), receipts.last())
        && receipts.len() >= 2
    {
        let first_ns = u128::from(parse_receipt_u64(first, ":seconds")) * 1_000_000_000
            + u128::from(parse_receipt_u64(first, ":nanoseconds"));
        let last_ns = u128::from(parse_receipt_u64(last, ":seconds")) * 1_000_000_000
            + u128::from(parse_receipt_u64(last, ":nanoseconds"));
        assert!(
            last_ns > first_ns,
            "{label} multi-frame sequence must span strictly advancing presentation timestamps: first={first_ns}ns, last={last_ns}ns"
        );
    }

    // Verify wire attachment discipline: every `wp_presentation.feedback` is
    // followed by a `wl_surface.commit` (matching present.rs before_present ->
    // queue.present ordering).
    let feedback_attach_count = wire
        .iter()
        .filter(|line| line.contains("wp_presentation") && line.contains(".feedback("))
        .count();
    assert!(
        feedback_attach_count >= receipts.len(),
        "{label} wire log must contain at least as many wp_presentation.feedback attachments ({feedback_attach_count}) as Presented receipts ({})",
        receipts.len()
    );
}

#[test]
// Prerequisites: requires release binary/pdump, GL Weston (--renderer=gl), and a hardware GPU adapter (quality_mode=Full).
// #607 Stage 2B Mechanism A: reverting commit glyph-pass reuse makes p2_root_glyph_passes jump to 2*p2_commits (RED).
fn wayland_gpu_only_cursor_motion_reuses_commit_glyph_pass_without_second_build() {
    let mut ctx = TestRunContext::launch("stage2b-mech-a", WestonSession::start_gl_only);

    ctx.wait_for_flag("ready", Duration::from_secs(30));
    ctx.write_flag("phase2-setup", "setup\n");
    ctx.wait_for_flag("phase2-ready", Duration::from_secs(30));
    let m2 = fetch_metrics(ctx.port);
    ctx.write_flag("phase2-go", "go\n");
    ctx.wait_for_flag("phase2-done", Duration::from_secs(30));
    let m3 = fetch_metrics(ctx.port);
    let logs = ctx.quit_and_collect();

    let phase2_receipts = logs.read_receipts("receipts-phase2.sexp");
    verify_receipts_monotonic_and_wire_ordered(&phase2_receipts, &logs.wire, "stage2b-mech-a");

    let p2_commits = metric_u64(&m3, "scene_commits") - metric_u64(&m2, "scene_commits");
    let p2_root_glyph_passes =
        metric_u64(&m3, "root_glyph_passes") - metric_u64(&m2, "root_glyph_passes");
    let p2_retained_builds =
        metric_u64(&m3, "retained_static_builds") - metric_u64(&m2, "retained_static_builds");
    let p2_composite_only =
        metric_u64(&m3, "composite_only_frames") - metric_u64(&m2, "composite_only_frames");

    eprintln!(
        "stage2b-mech-a weston: renderer={}\nstage2b-mech-a device: {}\nstage2b-mech-a policy: {}\nstage2b-mech-a summary: commits={p2_commits} root_glyph_passes={p2_root_glyph_passes} retained_static_builds={p2_retained_builds} composite_only={p2_composite_only} receipts={}",
        logs.renderer,
        logs.device_line,
        logs.final_policy_line,
        phase2_receipts.len()
    );

    assert!(
        logs.final_policy_line.contains("quality_mode=Full"),
        "GPU-only Mechanism A row requires quality_mode=Full: {}",
        logs.final_policy_line
    );
    assert!(
        p2_commits >= 1,
        "animated jump must publish at least one scene commit, got {p2_commits}"
    );
    assert!(
        p2_composite_only >= 3,
        "animated jump must exercise composite-only cursor frames, got {p2_composite_only}"
    );
    assert_eq!(
        p2_root_glyph_passes, p2_commits,
        "animated cursor motion after a commit must reuse the commit's root glyph pass (1.00x, not 2.00x): root_glyph_passes={p2_root_glyph_passes}, commits={p2_commits}, retained_static_builds={p2_retained_builds}, composite_only={p2_composite_only}"
    );
}

#[test]
// Prerequisites: requires release binary/pdump, GL Weston (--renderer=gl), and a hardware GPU adapter (quality_mode=Full).
// #607 Stage 2B Mechanism B: reverting submit coalescing makes p2_retained_submits jump to 3*p2_composite_only (RED).
fn wayland_gpu_only_retained_static_coalesces_submits_and_preserves_receipt_order() {
    let mut ctx = TestRunContext::launch("stage2b-mech-b", WestonSession::start_gl_only);

    ctx.wait_for_flag("ready", Duration::from_secs(30));
    ctx.write_flag("phase2-setup", "setup\n");
    ctx.wait_for_flag("phase2-ready", Duration::from_secs(30));
    let m2 = fetch_metrics(ctx.port);
    ctx.write_flag("phase2-go", "go\n");
    ctx.wait_for_flag("phase2-done", Duration::from_secs(30));
    let m3 = fetch_metrics(ctx.port);
    let logs = ctx.quit_and_collect();

    let phase2_receipts = logs.read_receipts("receipts-phase2.sexp");
    verify_receipts_monotonic_and_wire_ordered(&phase2_receipts, &logs.wire, "stage2b-mech-b");

    let p2_composite_only =
        metric_u64(&m3, "composite_only_frames") - metric_u64(&m2, "composite_only_frames");
    let p2_retained_submits = metric_u64(&m3, "retained_composite_queue_submits")
        - metric_u64(&m2, "retained_composite_queue_submits");
    let p2_presents = metric_u64(&m3, "presents") - metric_u64(&m2, "presents");

    eprintln!(
        "stage2b-mech-b weston: renderer={}\nstage2b-mech-b device: {}\nstage2b-mech-b policy: {}\nstage2b-mech-b summary: composite_only={p2_composite_only} retained_composite_submits={p2_retained_submits} presents={p2_presents} receipts={}",
        logs.renderer,
        logs.device_line,
        logs.final_policy_line,
        phase2_receipts.len()
    );

    assert!(
        logs.final_policy_line.contains("quality_mode=Full"),
        "GPU-only Mechanism B row requires quality_mode=Full: {}",
        logs.final_policy_line
    );
    assert!(
        p2_composite_only >= 3,
        "animated jump must exercise composite-only cursor frames, got {p2_composite_only}"
    );
    assert!(
        phase2_receipts.len() >= 2,
        "coalesced composite frames must preserve distinct wire-ordered Presented receipts, got {}",
        phase2_receipts.len()
    );
    assert_eq!(
        p2_retained_submits, p2_composite_only,
        "retained_static::draw must coalesce blit + cursor + filled-box cell into exactly 1 Queue::submit per composite-only frame: submits={p2_retained_submits}, composite_only={p2_composite_only}"
    );
}
