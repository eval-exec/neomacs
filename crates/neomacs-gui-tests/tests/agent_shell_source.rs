//! #832: restore a real pinned agent-shell session.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiScenario, GuiTestPlan, ProcessGuiCommandRunner,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

// MELPA source commits; only package-version header comments differ.
const PACKAGES: [(&str, &str, &str); 3] = [
    (
        "agent-shell",
        "dde9392902d750375bedaf7347c31cc15c66ca8f",
        "ce80538b1d80d42788d31df09b3c295df603ea47258a69a1c67865b9fac7af51",
    ),
    (
        "shell-maker",
        "cba42efd503bbdf19a7244b9119efd8764e99ecb",
        "3fc16eb486976d34da26a082327bfafc6054d316f287f5f0da70ac1ef837b1e8",
    ),
    (
        "acp.el",
        "041470ecff003630e03f061584d41071c36f0124",
        "283fe1c2f47d5338eb79f8cbcbe5b64b1b60a74182d77d47cd88de0cd82f28dc",
    ),
];

// Copy only validated regular .el files; never extract archive links.
const EXTRACT: &str = r#"
import pathlib, sys, tarfile
folder = pathlib.Path(sys.argv[2]) / sys.argv[4]
with tarfile.open(sys.argv[1], 'r:gz') as archive:
    members = archive.getmembers()
    assert len(members) < 512 and sum(m.size for m in members) < 16 * 1024 * 1024
    selected = []
    for member in members:
        name = pathlib.PurePosixPath(member.name)
        assert name.parts and not name.is_absolute() and '..' not in name.parts
        assert name.parts[0] == sys.argv[3]
        if len(name.parts) == 2 and name.suffix == '.el':
            assert member.isfile()
            selected.append(member)
    assert selected
    folder.mkdir()
    for member in selected:
        with archive.extractfile(member) as source, (folder / pathlib.PurePosixPath(member.name).name).open('xb') as output:
            output.write(source.read())
"#;

#[test]
fn melpa_agent_shell_resume_paints_without_false_source_mutation_like_gnu() {
    assert_eq!(std::env::var("NEOVM_ORACLE_MODE").as_deref(), Ok("verify"));
    let gnu = fs::canonicalize(PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH").expect("set GNU executable"),
    ))
    .unwrap();
    let neo = PathBuf::from(std::env::var_os("NEOMACS_GUI_TEST_BINARY").unwrap());
    // This reproduced package/oracle pair uses owned X11 sessions. Native
    // Wayland and the reporter's macOS/Doom setup need their own receipts.
    let backend = GuiBackend::LinuxX11;
    let root = neomacs_infra::workspace_root();
    let runtime = std::env::var_os("NEOMACS_RUNTIME_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.clone());
    let nonce = neomacs_gui_tests::unique_nonce();
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/agent-shell-832-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let prefix = gnu.parent().unwrap().parent().unwrap();
    let attestation = Command::new("bash")
        .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
        .arg(&gnu)
        .arg("31.1")
        .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
        .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
        .output()
        .unwrap();
    fs::write(artifacts.join("oracle-check.stdout"), &attestation.stdout).unwrap();
    fs::write(artifacts.join("oracle-check.stderr"), &attestation.stderr).unwrap();
    assert!(attestation.status.success(), "{attestation:?}");
    let proof = String::from_utf8_lossy(&attestation.stdout);
    assert!(proof.contains("oracle profile: linux-gtk3"));
    eprintln!("{proof}");
    let packages = artifacts.join("packages");
    fs::create_dir_all(&packages).unwrap();
    for (name, commit, sha) in PACKAGES {
        let archive = neomacs_infra::pinned::pinned_file(
            &format!("{name}-{commit}.tar.gz"),
            &format!("https://codeload.github.com/xenodium/{name}/tar.gz/{commit}"),
            sha,
        )
        .unwrap();
        let output = Command::new("python3")
            .current_dir(&root)
            .args(["-c", EXTRACT])
            .arg(archive)
            .arg(&packages)
            .arg(format!("{name}-{commit}"))
            .arg(name)
            .output()
            .unwrap();
        assert!(output.status.success(), "package extraction: {output:?}");
    }
    let fonts = artifacts.join("fonts.conf");
    fs::write(
        &fonts,
        format!(
            "<fontconfig><dir>{}</dir><cachedir>{}</cachedir></fontconfig>",
            neomacs_test_fonts::desktop_fonts().directory().display(),
            artifacts.join("fc-cache").display()
        ),
    )
    .unwrap();
    let observations = [("gnu", &gnu, GuiBackend::LinuxX11), ("neo", &neo, backend)].map(
        |(editor, binary, backend)| {
            run_editor(
                &root, &artifacts, &packages, &fonts, &runtime, editor, binary, backend,
            )
        },
    );
    assert_eq!(
        observations[0], observations[1],
        "GNU/Neo settled session and body"
    );
}

fn run_editor(
    root: &Path,
    artifacts: &Path,
    packages: &Path,
    fonts: &Path,
    runtime: &Path,
    editor: &str,
    binary: &Path,
    backend: GuiBackend,
) -> [Value; 4] {
    let owned = artifacts.join(editor);
    fs::create_dir_all(&owned).unwrap();
    let session = DisplayHarness::for_backend(backend)
        .start_session(&owned.join("display"))
        .unwrap();
    let mut plan = GuiTestPlan::new(
        backend,
        root,
        &owned,
        GuiScenario::new(
            format!("{editor}-agent-shell-832"),
            root.join("crates/neomacs-gui-tests/fixtures/agent-shell-source-observation.el"),
        ),
    )
    .with_program(binary)
    .with_env("NEOMACS_832_ROOT", owned.to_string_lossy())
    .with_env("NEOMACS_832_PACKAGE_ROOT", packages.to_string_lossy())
    .with_env("FONTCONFIG_FILE", fonts.to_string_lossy())
    .with_env("GSETTINGS_BACKEND", "memory")
    .with_env("NEOMACS_832_HEADER_STYLE", "graphical")
    .with_env("NEOMACS_832_SECONDS", "8")
    // ACP history arrives asynchronously; capture subsequent native frames.
    .with_env("NEOMACS_DEBUG_SURFACE_READBACK", "10000")
    .with_env("RUST_LOG", "warn");
    for key in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "TMPDIR",
    ] {
        let directory = owned.join(key.to_ascii_lowercase());
        fs::create_dir_all(&directory).unwrap();
        plan = plan.with_env(key, directory.to_string_lossy());
    }
    for (key, value) in session.env() {
        plan = plan.with_env(key, value);
    }
    if backend == GuiBackend::LinuxX11 {
        plan = plan.with_env("GDK_BACKEND", "x11");
    }
    if editor == "neo" {
        plan = plan.with_env("NEOMACS_RUNTIME_ROOT", runtime.to_string_lossy());
    }
    let result = plan
        .run_with(
            &mut ProcessGuiCommandRunner,
            GuiRunOptions::with_timeout(Duration::from_secs(35)),
        )
        .unwrap();
    assert!(
        !result.timed_out && result.exit_code == Some(0),
        "{editor}: exit {:?}, timeout {}",
        result.exit_code,
        result.timed_out
    );
    for file in [
        &result.artifacts.stdout,
        &result.artifacts.stderr,
        &result.artifacts.neomacs_log,
    ] {
        let log = fs::read_to_string(file).unwrap_or_default();
        assert!(
            !log.contains("layout failed to converge"),
            "{editor}: {}\n{}",
            file.display(),
            tail(&log)
        );
    }
    let samples: Vec<Value> =
        serde_json::from_slice(&fs::read(owned.join("samples.json")).unwrap()).unwrap();
    let settled: Vec<_> = samples
        .iter()
        .filter(|s| matches!(s["label"].as_str(), Some("1.5" | "3" | "3.0")))
        .collect();
    assert_eq!(settled.len(), 2, "{editor}: {samples:?}");
    for sample in &settled {
        assert!(
            sample["initialized"] == true
                && sample["session"] == "832-local-session"
                && sample["body_seen"] == true,
            "{editor}: {sample}"
        );
        assert!(
            sample["end"].as_u64().is_some_and(|end| end > 1),
            "cached presented end: {sample}"
        );
        assert!(sample["window"].as_str().unwrap().starts_with("#<window"));
    }
    for field in ["buffer", "window", "body_hash"] {
        assert_eq!(
            settled[0][field], settled[1][field],
            "{editor}: stable {field}"
        );
    }
    let observation =
        ["initialized", "session", "body_seen", "body_hash"].map(|field| settled[1][field].clone());
    let backend_log = fs::read_to_string(owned.join("backend.jsonl")).unwrap();
    let methods: Vec<Value> = backend_log
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for method in ["initialize", "session/resume"] {
        assert!(
            methods.iter().any(|entry| entry["method"] == method),
            "{editor}: {}",
            tail(&backend_log)
        );
    }
    let receipts = fs::read_to_string(owned.join("receipt.el")).unwrap();
    let cleanup: Vec<_> = receipts
        .lines()
        .filter(|line| line.starts_with("(:event owned-process-cleanup "))
        .collect();
    assert!(
        cleanup.len() >= 2 && cleanup.iter().all(|line| line.contains(":live nil")),
        "ACP/comint cleanup: {}",
        tail(&receipts)
    );
    assert!(receipts.contains("(:event finished ") && receipts.contains(":exit-code 0"));
    let png = image::open(&result.artifacts.png)
        .expect("real GUI PNG")
        .to_rgba8();
    assert!(png.width() > 200 && png.height() > 100);
    let first = *png.get_pixel(0, 0);
    assert!(
        png.pixels().any(|pixel| *pixel != first),
        "{editor}: blank readback"
    );
    observation
}

fn tail(text: &str) -> String {
    let mut lines: Vec<_> = text.lines().rev().take(30).collect();
    lines.reverse();
    lines.join("\n")
}
