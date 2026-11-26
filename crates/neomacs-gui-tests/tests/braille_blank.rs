//! #817: a mapped zero-ink glyph is a successful lookup, not a retryable miss.
//!
//! GNU 31.1 a360712c ftcrfont.c:94-116 caches zero ink bounds independently
//! of advance; :604-641 draws the encoded glyph and advances even without ink.
//! Use one pinned, single-face TrueType font so fallback cannot hide a miss.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiArtifactSet, GuiBackend, GuiScenario, GuiTestPlan, HangCap,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

const READBACK_ENV: [&str; 3] = [
    "NEOMACS_DEBUG_FIRST_FRAME_READBACK",
    "NEOMACS_DEBUG_SURFACE_READBACK",
    "NEOMACS_DEBUG_SURFACE_READBACK_PNG",
];

#[test]
fn blank_braille_keeps_advance_without_ink_or_rasterize_retries_like_gnu() {
    assert_eq!(std::env::var("NEOVM_ORACLE_MODE").as_deref(), Ok("verify"));
    let gnu = PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH").expect("set the pinned GNU GUI executable"),
    );
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(oracle.is_attested(), "attested GNU oracle required");
    assert_eq!(fs::canonicalize(&gnu).unwrap(), oracle.executable_path());
    let root = neomacs_infra::workspace_root();
    let nonce = neomacs_gui_tests::unique_nonce();
    let dir = root.join(format!(
        "tmp/neomacs-gui-tests/braille-blank-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let prefix = gnu.parent().unwrap().parent().unwrap();
    let check = Command::new("bash")
        .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
        .arg(&gnu)
        .arg("31.1")
        .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
        .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
        .output()
        .unwrap();
    fs::write(dir.join("oracle-check.stdout"), &check.stdout).unwrap();
    fs::write(dir.join("oracle-check.stderr"), &check.stderr).unwrap();
    fs::write(dir.join("oracle-stamp.txt"), oracle.stamp()).unwrap();
    assert!(check.status.success(), "{check:?}");
    assert!(String::from_utf8_lossy(&check.stdout).contains("oracle profile: linux-gtk3"));
    eprintln!("{}", String::from_utf8_lossy(&check.stdout));

    let fonts = dir.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    // The accessor checks the release archive and member SHA. This .ttf has
    // exactly one face (index zero); it is not a TrueType collection.
    let font = fonts.join("DejaVuSans.ttf");
    fs::copy(neomacs_test_fonts::dejavu_2_37().sans_regular(), &font).unwrap();
    let font = fs::canonicalize(font).unwrap();
    let config = dir.join("fonts.conf");
    let xml = |path: &Path| {
        path.to_str()
            .unwrap()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
    };
    fs::write(&config, format!(
        "<fontconfig><dir>{}</dir><cachedir>{}</cachedir><alias><family>monospace</family><prefer><family>DejaVu Sans</family></prefer></alias><match target=\"pattern\"><edit name=\"rgba\" mode=\"assign\"><const>none</const></edit></match></fontconfig>",
        xml(&fonts), xml(&dir.join("fc-cache")))).unwrap();
    let display = DisplayHarness::Xvfb.start_session(&dir).unwrap();
    let resources = dir.join("Xresources");
    fs::write(&resources, "Xft.dpi: 96\nXft.rgba: none\n").unwrap();
    let xrdb = Command::new("xrdb")
        .arg("-load")
        .arg(resources)
        .envs(display.env().iter().cloned())
        .output()
        .unwrap();
    assert!(xrdb.status.success(), "{xrdb:?}");
    let neo = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let mut observations = Vec::new();
    for (editor, binary) in [("gnu", &gnu), ("neo", &neo)] {
        observations.push(run_editor(
            &root,
            &dir,
            editor,
            binary,
            &config,
            display.env(),
        ));
    }
    let matrix = json!({"gnu": observations[0], "neo": observations[1]});
    fs::write(
        dir.join("matrix.json"),
        serde_json::to_vec_pretty(&matrix).unwrap(),
    )
    .unwrap();

    for editor in ["gnu", "neo"] {
        let state = &matrix[editor]["state"];
        assert_eq!(state["complete"], true, "{editor}; {dir:?}");
        assert_eq!(
            state["edits"], 600,
            "repaint stress must complete; {editor}; {dir:?}"
        );
        assert_eq!(state["edit_interval"], 0.02);
        let rows = state["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        for (row, character) in rows.iter().zip([32, 10240, 10241]) {
            assert_eq!(row["character"], character);
            assert_eq!(row["family"], "DejaVu Sans", "{editor}; {dir:?}");
            assert_eq!(row["size"], 18, "{editor}; {dir:?}");
            assert_eq!(
                fs::canonicalize(row["file"].as_str().unwrap()).unwrap(),
                font
            );
            assert_eq!(
                row["has_character"], true,
                "must be mapped, not fallback; {row}"
            );
            if editor == "gnu" || !row["font_index"].is_null() {
                assert_eq!(row["font_index"], 0, "single-face fixture; {row}");
            }
            // Neo currently does not expose :font-entity. Exact canonical
            // single-face TTF identity still permits only face index zero.
            let advances = advances(&row["positions"]);
            assert_eq!(advances.len(), 8);
            assert!(advances.iter().all(|width| *width > 0), "{row}");
            assert_eq!(advances, vec![advances[0]; 8], "{row}");
        }
        let ink = matrix[editor]["row_ink"].as_array().unwrap();
        assert_eq!(ink[0], 0.0, "spaces must be blank; {editor}; {dir:?}");
        assert_eq!(ink[1], 0.0, "U+2800 must be blank; {editor}; {dir:?}");
        assert!(
            ink[2].as_f64().unwrap() > 0.0,
            "visible braille control; {editor}; {dir:?}"
        );
        let neighbor_ink = matrix[editor]["neighbor_ink"].as_array().unwrap();
        assert!(neighbor_ink[0].as_f64().unwrap() > 0.0);
        assert_eq!(
            neighbor_ink[1], 0.0,
            "blank between visible neighbors; {editor}; {dir:?}"
        );
        assert!(neighbor_ink[2].as_f64().unwrap() > 0.0);
        let neighbor_advances = advances(&state["neighbor_positions"]);
        assert!(neighbor_advances.iter().all(|width| *width > 0));
        assert_eq!(neighbor_advances[1], advances(&rows[1]["positions"])[0]);
        assert_eq!(
            matrix[editor]["rasterize_failed_2800"], 0,
            "valid blank glyph must not repeatedly fail; {editor}; {dir:?}"
        );
    }
    for index in 0..3 {
        assert_eq!(
            advances(&matrix["neo"]["state"]["rows"][index]["positions"]),
            advances(&matrix["gnu"]["state"]["rows"][index]["positions"]),
            "same mapped glyph advances; row {index}; {dir:?}"
        );
    }
    assert_eq!(
        advances(&matrix["neo"]["state"]["neighbor_positions"]),
        advances(&matrix["gnu"]["state"]["neighbor_positions"]),
        "{dir:?}"
    );
}

fn advances(positions: &Value) -> Vec<i64> {
    positions
        .as_array()
        .unwrap()
        .windows(2)
        .map(|pair| {
            assert_eq!(pair[0][1], pair[1][1], "positions must remain on one row");
            pair[1][0].as_i64().unwrap() - pair[0][0].as_i64().unwrap()
        })
        .collect()
}

fn run_editor(
    root: &Path,
    dir: &Path,
    editor: &str,
    binary: &Path,
    config: &Path,
    display_env: &[(String, String)],
) -> Value {
    let mut plan = GuiTestPlan::new(
        GuiBackend::LinuxX11,
        root,
        dir,
        GuiScenario::new(
            editor,
            root.join("crates/neomacs-gui-tests/fixtures/braille-blank.el"),
        ),
    )
    .with_program(binary)
    .with_env("FONTCONFIG_FILE", config.display().to_string())
    .with_env("GDK_BACKEND", "x11")
    .with_env("GDK_SCALE", "1")
    .with_env("GDK_DPI_SCALE", "1")
    .with_env("GSETTINGS_BACKEND", "memory")
    .with_env("RUST_LOG", "warn");
    for (key, value) in display_env {
        plan = plan.with_env(key, value);
    }
    let files = GuiArtifactSet::new(dir, GuiBackend::LinuxX11, editor);
    plan.write_manifest().unwrap();
    let mut spec = plan.command_spec();
    spec.env
        .retain(|(key, _)| !READBACK_ENV.contains(&key.as_str()));
    fs::write(
        dir.join(format!("{editor}-capture-command.json")),
        serde_json::to_vec_pretty(&json!({"program":spec.program,"args":spec.args,"env":spec.env}))
            .unwrap(),
    )
    .unwrap();
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .envs(spec.env.iter().cloned())
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .stdin(Stdio::null())
        .stdout(fs::File::create(&files.stdout).unwrap())
        .stderr(fs::File::create(&files.stderr).unwrap());
    for variable in READBACK_ENV {
        command.env_remove(variable);
    }
    let mut child = OwnedEditor(command.spawn().unwrap());
    let deadline = neomacs_gui_tests::HangCap::new(Duration::from_secs(45));
    let state = loop {
        if let Ok(bytes) = fs::read(&files.gui_state)
            && let Ok(state) = serde_json::from_slice::<Value>(&bytes)
        {
            break state;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "{editor} exited before final state; {dir:?}"
        );
        assert!(
            !deadline.is_expired(),
            "{editor} final state timeout; {dir:?}"
        );
        thread::sleep(Duration::from_millis(20));
    };
    let search = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--pid",
            &child.0.id().to_string(),
        ])
        .envs(display_env.iter().cloned())
        .output()
        .unwrap();
    assert!(search.status.success(), "{search:?}; {dir:?}");
    let windows = String::from_utf8(search.stdout).unwrap();
    let window = windows.lines().next().expect("visible editor window");
    let capture_cap = HangCap::from_secs(10);
    let mut previous = None;
    let (row_ink, neighbor_ink) = loop {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "{editor} exited before capture; {dir:?}"
        );
        let capture = Command::new("import")
            .arg("-window")
            .arg(window)
            .arg(&files.png)
            .envs(display_env.iter().cloned())
            .output()
            .unwrap();
        assert!(capture.status.success(), "{capture:?}; {dir:?}");
        let pixels = image::open(&files.png).unwrap().to_rgba8();
        let row_ink: Vec<_> = state["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| integrated_ink(&pixels, &row["bounds"]))
            .collect();
        let neighbor_ink: Vec<_> = state["neighbor_bounds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|bounds| integrated_ink(&pixels, bounds))
            .collect();
        let observation = (row_ink, neighbor_ink);
        if observation.0[2] > 0.0
            && observation.1[0] > 0.0
            && observation.1[2] > 0.0
            && previous.as_ref() == Some(&observation)
        {
            break observation;
        }
        capture_cap
            .assert_not_expired(format!("{editor} visible controls did not settle; {dir:?}"));
        previous = Some(observation);
        thread::sleep(Duration::from_millis(50));
    };
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "{editor} exit {status}; {dir:?}");
            break;
        }
        assert!(!deadline.is_expired(), "{editor} exit timeout; {dir:?}");
        thread::sleep(Duration::from_millis(20));
    }
    // Neo's default logger writes warnings to stdout. Read both streams only
    // after successful exit, so a late retry cannot escape the assertion.
    let log = format!(
        "{}\n{}",
        fs::read_to_string(&files.stdout).unwrap(),
        fs::read_to_string(&files.stderr).unwrap()
    );
    let log = strip_ansi(&log);
    fs::write(dir.join(format!("{editor}-combined.log")), &log).unwrap();
    // Single-glyph failures carry charcode; composed failures carry text.
    // Both must stay failures in this test, even when the renderer memoizes
    // the first failure and suppresses subsequent identical diagnostics.
    let failures = log
        .lines()
        .filter(|line| {
            line.contains("RasterizeFailed")
                && (line
                    .split_whitespace()
                    .any(|field| field == "charcode=10240")
                    || line.contains('\u{2800}')
                    || line.contains("\\u{2800}")
                    || line.contains("\\u2800"))
        })
        .count();
    json!({"state":state,"row_ink":row_ink,"neighbor_ink":neighbor_ink,
           "rasterize_failed_2800":failures,"png":files.png,"exit_code":0})
}

fn integrated_ink(pixels: &image::RgbaImage, bounds: &Value) -> f64 {
    let edges: Vec<u32> = serde_json::from_value(bounds.clone()).unwrap();
    assert_eq!(edges.len(), 4);
    let [left, top, right, bottom] = [edges[0], edges[1], edges[2], edges[3]];
    assert!(
        left < right && top < bottom && right <= pixels.width() && bottom <= pixels.height(),
        "{bounds}"
    );
    let mut ink = 0.0;
    for y in top..bottom {
        for x in left..right {
            let [r, g, b, a] = pixels.get_pixel(x, y).0;
            assert_eq!(a, 255, "opaque native window capture");
            ink += (765.0 - f64::from(r) - f64::from(g) - f64::from(b)) / 765.0;
        }
    }
    ink
}

fn strip_ansi(text: &str) -> String {
    let mut output = String::new();
    let mut escape = false;
    let mut csi = false;
    for character in text.chars() {
        if csi {
            if ('@'..='~').contains(&character) {
                csi = false;
            }
            continue;
        }
        if escape {
            csi = character == '[';
            escape = false;
            continue;
        }
        if character == '\u{1b}' {
            escape = true;
        } else {
            output.push(character);
        }
    }
    output
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
