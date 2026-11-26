//! #359 Wayland startup and live raster policy. GNU PGTK is UNATTESTED:
//! the canonical Linux oracle recipe explicitly configures --without-pgtk.
//! This row validates Neo's real desktop-settings/rendering path, not GNU parity.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{GuiBackend, GuiScenario, GuiTestPlan};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const READBACK_ENV: [&str; 3] = [
    "NEOMACS_DEBUG_FIRST_FRAME_READBACK",
    "NEOMACS_DEBUG_SURFACE_READBACK",
    "NEOMACS_DEBUG_SURFACE_READBACK_PNG",
];
const FONTS: [(&str, &str); 2] = [
    (
        "IBMPlexSans-Regular.ttf",
        "b47ebd72b5ce53b60f909033ff84d6fc12358cc147e2981f8c321ad3f36246b6",
    ),
    (
        "IBMPlexSans-Bold.ttf",
        "22f59d358e8b9b9fb38ac6ee26a7ea7263ffa1c89a9056c101acff5add91f5d6",
    ),
];

#[derive(Clone, PartialEq)]
struct Observation {
    metrics: Value,
    pixels: Vec<Vec<u8>>,
    ink: Vec<u64>,
}

#[test]
fn wayland_desktop_raster_startup_and_notifications_restore_same_policy_pixels_and_metrics() {
    let root = neomacs_infra::workspace_root();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let artifacts = directory(root.join(format!(
        "tmp/neomacs-gui-tests/wayland-font-raster-{}-{nonce}",
        std::process::id()
    )));
    fs::write(artifacts.join("gnu-pgtk-reference.json"), serde_json::to_vec_pretty(&json!({
        "attestation": "UNATTESTED",
        "observed_gnu_pgtk": false,
        "mirror_commit": "a360712c9d272d950d8d8255ef74570f7e90b7d9",
        "reason": "canonical Linux recipe is linux-gtk3 with --without-pgtk; checker has no PGTK profile",
        "recipe": "scripts/ci/build-gnu-oracle.sh:261-271",
        "source_contract": ["src/xsettings.c:229-240,284-390,445-460,1154-1162", "src/ftcrfont.c:175-187,242-271,650-663"],
        "scope": "Neo-only startup/live same-policy pixel and metric regression; no GNU comparison was run",
        "metric_boundary": "PGTK opens its scaled font with GSettings options, so different policies may change advances and frame metrics"
    })).unwrap()).unwrap();
    eprintln!(
        "GNU PGTK UNATTESTED: canonical oracle is GTK3/X11 --without-pgtk; artifacts={artifacts:?}"
    );
    let schemas = directory(artifacts.join("schemas"));
    fs::copy(
        root.join("crates/neomacs-gui-tests/fixtures/wayland-font-raster.gschema.xml"),
        schemas.join("wayland-font-raster.gschema.xml"),
    )
    .unwrap();
    assert!(
        Command::new("glib-compile-schemas")
            .arg(&schemas)
            .status()
            .unwrap()
            .success()
    );
    let settings = directory(artifacts.join("settings"));
    let fonts = directory(artifacts.join("fonts"));
    for (filename, sha256) in FONTS {
        let url = format!(
            "https://raw.githubusercontent.com/IBM/plex/2e90fe366fdc27c12682db13e5aa20ded3d90908/packages/plex-sans/fonts/complete/ttf/{filename}"
        );
        let source = neomacs_infra::pinned::pinned_file(filename, &url, sha256).unwrap();
        fs::copy(source, fonts.join(filename)).unwrap();
    }
    let config = artifacts.join("fonts.conf");
    fs::write(
        &config,
        format!(
            r#"<fontconfig><dir>{}</dir><cachedir>{}</cachedir>
<alias><family>monospace</family><prefer><family>IBM Plex Sans</family></prefer></alias>
<match target="pattern">
<edit name="hinting" mode="assign"><bool>true</bool></edit>
<edit name="hintstyle" mode="assign"><const>hintslight</const></edit>
<edit name="autohint" mode="assign"><bool>false</bool></edit>
<edit name="antialias" mode="assign"><bool>true</bool></edit>
<edit name="rgba" mode="assign"><const>none</const></edit>
</match></fontconfig>"#,
            xml(&fonts),
            xml(&directory(artifacts.join("font-cache")))
        ),
    )
    .unwrap();
    let settings_env = vec![
        ("GSETTINGS_BACKEND".into(), "keyfile".into()),
        (
            "GSETTINGS_SCHEMA_DIR".into(),
            schemas.to_string_lossy().into_owned(),
        ),
        (
            "XDG_CONFIG_HOME".into(),
            settings.to_string_lossy().into_owned(),
        ),
        (
            "FONTCONFIG_FILE".into(),
            config.to_string_lossy().into_owned(),
        ),
    ];
    let session = neomacs_infra::display::start_sway(
        &artifacts,
        r#"
output * resolution 1000x400
xwayland disable
seat seat0 fallback true
default_border none
focus_follows_mouse no
"#,
    )
    .expect("private Wayland compositor with screencopy");
    let runtime = PathBuf::from(
        session
            .env()
            .iter()
            .find(|(key, _)| key == "XDG_RUNTIME_DIR")
            .unwrap()
            .1
            .clone(),
    );
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    set_policy(
        &settings_env,
        &artifacts,
        "initial-slight",
        "slight",
        "grayscale",
    );
    let live_dir = directory(artifacts.join("live"));
    let mut live = spawn_editor(&root, &live_dir, &binary, &settings_env, session.env());
    let native = mapped_rect(&runtime, &live_dir, live.0.id());
    let slight = settle(
        &mut live,
        &live_dir,
        session.env(),
        &native,
        "startup",
        &fonts,
        |_| true,
    );
    // Monochrome provides a visible control independent of host-specific
    // full-versus-slight hinting differences. Both settings must round trip.
    set_policy(&settings_env, &artifacts, "live-full", "full", "none");
    fs::write(live_dir.join("control"), "full").unwrap();
    let full = settle(
        &mut live,
        &live_dir,
        session.env(),
        &native,
        "full",
        &fonts,
        |observation| observation.ink != slight.ink,
    );
    set_policy(
        &settings_env,
        &artifacts,
        "restore-slight",
        "slight",
        "grayscale",
    );
    fs::write(live_dir.join("control"), "restored").unwrap();
    let restored = settle(
        &mut live,
        &live_dir,
        session.env(),
        &native,
        "restored",
        &fonts,
        |observation| observation == &slight,
    );
    assert!(restored == slight, "same-policy round trip; {artifacts:?}");
    stop_editor(&mut live, &live_dir);

    set_policy(&settings_env, &artifacts, "fresh-full", "full", "none");
    let fresh_dir = directory(artifacts.join("fresh-full"));
    let mut fresh = spawn_editor(&root, &fresh_dir, &binary, &settings_env, session.env());
    let native = mapped_rect(&runtime, &fresh_dir, fresh.0.id());
    let startup_full = settle(
        &mut fresh,
        &fresh_dir,
        session.env(),
        &native,
        "startup",
        &fonts,
        |observation| observation == &full,
    );
    assert!(
        startup_full == full,
        "live and fresh same-policy metrics/pixels; {artifacts:?}"
    );
    stop_editor(&mut fresh, &fresh_dir);
}

fn directory(path: PathBuf) -> PathBuf {
    fs::create_dir_all(&path).unwrap();
    path
}

fn xml(path: &Path) -> String {
    path.to_str()
        .unwrap()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
}

fn set_policy(
    env: &[(String, String)],
    artifacts: &Path,
    name: &str,
    hinting: &str,
    antialias: &str,
) {
    for (key, value) in [("font-hinting", hinting), ("font-antialiasing", antialias)] {
        let result = Command::new("gsettings")
            .args(["set", "org.gnome.desktop.interface", key, value])
            .envs(env.iter().cloned())
            .output()
            .expect("private keyfile GSettings writer");
        fs::write(
            artifacts.join(format!("{name}-{key}.stdout")),
            &result.stdout,
        )
        .unwrap();
        fs::write(
            artifacts.join(format!("{name}-{key}.stderr")),
            &result.stderr,
        )
        .unwrap();
        assert!(
            result.status.success(),
            "private settings writer: {result:?}"
        );
    }
}

fn spawn_editor(
    root: &Path,
    dir: &Path,
    binary: &Path,
    settings: &[(String, String)],
    display: &[(String, String)],
) -> OwnedEditor {
    fs::write(dir.join("control"), "startup").unwrap();
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxWayland,
        root,
        dir,
        GuiScenario::new(
            "wayland-font-raster",
            root.join("crates/neomacs-gui-tests/fixtures/wayland-font-raster.el"),
        ),
    )
    .with_program(binary)
    .with_env(
        "NEOMACS_RASTER_CONTROL",
        dir.join("control").to_string_lossy(),
    )
    .with_env(
        "NEOMACS_GUI_STATE_JSON",
        dir.join("state.json").to_string_lossy(),
    )
    .with_env("WINIT_UNIX_BACKEND", "wayland")
    .with_env("WGPU_BACKEND", "vulkan");
    for (key, value) in settings.iter().chain(display) {
        plan = plan.with_env(key, value);
    }
    let mut spec = plan.command_spec();
    spec.env
        .retain(|(key, _)| !READBACK_ENV.contains(&key.as_str()));
    fs::write(dir.join("command.json"), serde_json::to_vec_pretty(&json!({
        "program": spec.program, "args": spec.args, "env": spec.env,
        "env_remove": ["DISPLAY", "WAYLAND_SOCKET", READBACK_ENV[0], READBACK_ENV[1], READBACK_ENV[2]]
    })).unwrap()).unwrap();
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .envs(spec.env.iter().cloned())
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .stdin(Stdio::null())
        .stdout(fs::File::create(dir.join("editor.stdout")).unwrap())
        .stderr(fs::File::create(dir.join("editor.stderr")).unwrap());
    for variable in READBACK_ENV {
        command.env_remove(variable);
    }
    OwnedEditor(command.spawn().expect("Wayland editor"))
}

fn mapped_rect(runtime: &Path, dir: &Path, pid: u32) -> Value {
    let _mapped = crate::wayland_readiness::wait_for_mapped_child(runtime, dir);
    let tree: Value =
        serde_json::from_slice(&fs::read(dir.join("mapped-editor.json")).unwrap()).unwrap();
    fn find(node: &Value, pid: u32) -> Option<Value> {
        if node["pid"].as_u64() == Some(u64::from(pid))
            && node["visible"] == true
            && node["shell"] == "xdg_shell"
        {
            return Some(node["rect"].clone());
        }
        ["nodes", "floating_nodes"].into_iter().find_map(|key| {
            node[key]
                .as_array()?
                .iter()
                .find_map(|child| find(child, pid))
        })
    }
    find(&tree, pid).expect("native visible view belongs to the exact editor PID")
}

fn settle(
    editor: &mut OwnedEditor,
    dir: &Path,
    display: &[(String, String)],
    native: &Value,
    phase: &str,
    fonts: &Path,
    accept: impl Fn(&Observation) -> bool,
) -> Observation {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut previous = None;
    loop {
        assert!(
            editor.0.try_wait().unwrap().is_none(),
            "editor exited; {dir:?}"
        );
        if let Ok(bytes) = fs::read(dir.join("state.json"))
            && let Ok(state) = serde_json::from_slice::<Value>(&bytes)
            && state["complete"] == true
            && state["phase"] == phase
        {
            let png = dir.join(format!("{phase}.png"));
            let capture =
                Command::new(std::env::var_os("NEOMACS_GUI_GRIM").unwrap_or_else(|| "grim".into()))
                    .arg(&png)
                    .envs(display.iter().cloned())
                    .output()
                    .expect("grim compositor capture");
            assert!(capture.status.success(), "grim: {capture:?}");
            let image = image::open(&png).unwrap().to_rgba8();
            let rows = state["metrics"]["rows"].as_array().expect("font rows");
            assert_eq!(rows.len(), 2);
            let mut pixels = Vec::new();
            let mut ink = Vec::new();
            for (row, (filename, _)) in rows.iter().zip(FONTS) {
                assert_eq!(row["family"], "IBM Plex Sans");
                assert_eq!(row["size"], 16);
                assert_eq!(
                    fs::canonicalize(row["file"].as_str().unwrap()).unwrap(),
                    fs::canonicalize(fonts.join(filename)).unwrap()
                );
                let bounds: Vec<u32> = serde_json::from_value(row["bounds"].clone()).unwrap();
                assert_eq!(bounds.len(), 4);
                let x = native["x"].as_u64().unwrap() as u32 + bounds[0];
                let y = native["y"].as_u64().unwrap() as u32 + bounds[1];
                let width = bounds[2] - bounds[0];
                let height = bounds[3] - bounds[1];
                assert!(
                    width > 0
                        && height > 0
                        && x + width <= image.width()
                        && y + height <= image.height()
                );
                let crop = image::imageops::crop_imm(&image, x, y, width, height).to_image();
                assert!(
                    crop.pixels().all(|p| p.0[3] == 255),
                    "opaque compositor pixels"
                );
                assert!(
                    crop.pixels().any(|p| p.0[..3].iter().any(|v| *v < 128)),
                    "visible text control; {dir:?}"
                );
                // White padding and translated glyph positions contribute no
                // ink. A geometry-only change must not certify raster policy.
                ink.push(
                    crop.pixels()
                        .map(|p| 765 - u64::from(p.0[0]) - u64::from(p.0[1]) - u64::from(p.0[2]))
                        .sum(),
                );
                pixels.push(crop.into_raw());
            }
            let observation = Observation {
                metrics: state["metrics"].clone(),
                pixels,
                ink,
            };
            fs::write(
                dir.join(format!("{phase}-state.json")),
                serde_json::to_vec_pretty(&state).unwrap(),
            )
            .unwrap();
            fs::write(
                dir.join(format!("{phase}-observation.json")),
                serde_json::to_vec_pretty(&json!({
                    "metrics": observation.metrics, "ink_765ths": observation.ink,
                    "capture_source": "grim private compositor output", "native_rect": native
                }))
                .unwrap(),
            )
            .unwrap();
            if accept(&observation) && previous.as_ref() == Some(&observation) {
                return observation;
            }
            previous = Some(observation);
        }
        assert!(
            Instant::now() < deadline,
            "phase {phase} did not settle to expected pixels and metrics; {dir:?}"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

fn stop_editor(editor: &mut OwnedEditor, dir: &Path) {
    fs::write(dir.join("control"), "stop").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = editor.0.try_wait().unwrap() {
            assert!(status.success(), "editor exit: {status}; {dir:?}");
            return;
        }
        assert!(Instant::now() < deadline, "editor exit timeout; {dir:?}");
        thread::sleep(Duration::from_millis(20));
    }
}

struct OwnedEditor(Child);
impl Drop for OwnedEditor {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
