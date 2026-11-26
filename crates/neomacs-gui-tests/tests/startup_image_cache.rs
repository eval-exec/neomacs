//! Real user startup must not wait on a renderer that is awaiting initial geometry.
//!
//! GNU 31.1's startup.el loads early-init before frame-initialize; image.c's
//! clear-image-cache and image-cache-size inspect the owned caches synchronously.
//! This is a semantic startup/liveness row, not a GPU or pixel-rendering claim.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[test]
fn early_init_image_cache_queries_complete_through_gui_startup_against_gnu() {
    assert_eq!(std::env::var("NEOVM_ORACLE_MODE").as_deref(), Ok("verify"));
    let forced = PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH").expect("set NEOVM_FORCE_ORACLE_PATH"),
    );
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "this paired row requires the pinned GNU oracle"
    );
    let gnu = fs::canonicalize(&forced).expect("GNU executable");
    assert_eq!(gnu, oracle.executable_path());
    // CI supplies its freshly built executable. A local run can override the
    // runtime root; otherwise use this checkout, as the GUI harness does.
    let neo = fs::canonicalize(
        std::env::var_os("NEOMACS_GUI_TEST_BINARY").expect("set NEOMACS_GUI_TEST_BINARY"),
    )
    .expect("Neomacs executable");
    let root = neomacs_infra::workspace_root();
    let runtime = fs::canonicalize(
        std::env::var_os("NEOMACS_RUNTIME_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.clone()),
    )
    .expect("Neomacs runtime root");
    assert!(neo.is_file() && runtime.is_dir());
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/startup-image-cache-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&artifacts).unwrap();
    let prefix = gnu.parent().unwrap().parent().unwrap();
    let check = Command::new("bash")
        .arg(root.join("scripts/ci/check-gnu-oracle.sh"))
        .arg(&gnu)
        .arg("31.1")
        .arg(prefix.join("share/gnu-emacs-oracle/build-manifest"))
        .arg("a360712c9d272d950d8d8255ef74570f7e90b7d9")
        .output()
        .expect("GNU attestation command");
    fs::write(artifacts.join("oracle-check.stdout"), &check.stdout).unwrap();
    fs::write(artifacts.join("oracle-check.stderr"), &check.stderr).unwrap();
    assert!(check.status.success(), "GNU attestation failed: {check:?}");
    let attestation = String::from_utf8_lossy(&check.stdout);
    assert!(attestation.contains("oracle profile: linux-gtk3"));
    eprintln!("{attestation}");
    let session = DisplayHarness::Xvfb
        .start_session(&artifacts.join("display"))
        .unwrap();
    let mut states = Vec::new();
    for (editor, binary) in [("gnu", &gnu), ("neo", &neo)] {
        let owned = artifacts.join(editor);
        let init = owned.join("init");
        fs::create_dir_all(&init).unwrap();
        for file in ["early-init.el", "init.el"] {
            fs::copy(
                root.join("crates/neomacs-gui-tests/fixtures/startup-image-cache")
                    .join(file),
                init.join(file),
            )
            .unwrap();
        }
        let receipt = owned.join("phases.jsonl");
        let mut plan = GuiTestPlan::new(
            GuiBackend::LinuxX11,
            &root,
            &owned,
            GuiScenario::new(
                format!("{editor}-startup-image-cache"),
                init.join("init.el"),
            ),
        )
        .without_paint_proof()
        .with_program(binary)
        .with_args([
            "--no-site-file".to_owned(),
            "--no-site-lisp".to_owned(),
            format!("--init-directory={}", init.display()),
        ])
        .with_env(
            "NEOMACS_STARTUP_IMAGE_CACHE_RECEIPT",
            receipt.to_string_lossy(),
        )
        .with_env(
            "FONTCONFIG_FILE",
            crate::desktop_font_pins::desktop_font_fontconfig(&owned).to_string_lossy(),
        )
        .with_env("GDK_BACKEND", "x11")
        .with_env("GSETTINGS_BACKEND", "memory");
        for key in [
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_CACHE_HOME",
            "XDG_DATA_HOME",
            "XDG_STATE_HOME",
        ] {
            let directory = owned.join(key.to_ascii_lowercase());
            fs::create_dir_all(&directory).unwrap();
            plan = plan.with_env(key, directory.to_string_lossy());
        }
        for (key, value) in session.env() {
            plan = plan.with_env(key.clone(), value.clone());
        }
        if editor == "neo" {
            plan = plan.with_env("NEOMACS_RUNTIME_ROOT", runtime.to_string_lossy());
        }
        let result = plan
            .run_with(
                &mut ProcessGuiCommandRunner,
                GuiRunOptions::with_timeout(Duration::from_secs(30)),
            )
            .expect("owned GUI startup process");
        let trace = fs::read_to_string(&receipt).unwrap_or_default();
        assert!(
            !result.timed_out,
            "{editor} startup stalled; {trace}\n{result:#?}"
        );
        assert_eq!(result.exit_code, Some(0), "{editor}: {trace}\n{result:#?}");
        assert_eq!(result.status, GuiRunStatus::Passed, "{result:#?}");
        states.push(read_phases(&receipt));
    }
    for (index, phase) in ["early-init", "after-init", "window-setup"]
        .iter()
        .enumerate()
    {
        let gnu = &states[0][index];
        let neo = &states[1][index];
        let expected = if index == 0 { "(nil 0)" } else { "(x 0)" };
        assert_eq!(gnu["value"], expected, "GNU {phase}: {gnu}");
        // Preserve the raw backend symbols in artifacts. Neo's graphical
        // backend identity need not be GNU's `x`; the phase boundary must match.
        assert_eq!(
            neo["window_system"] == "nil",
            gnu["window_system"] == "nil",
            "{phase}: GNU={gnu}; Neo={neo}"
        );
        assert_eq!(
            neo["cache_size"], gnu["cache_size"],
            "{phase}: GNU={gnu}; Neo={neo}"
        );
    }
}

fn read_phases(receipt: &Path) -> Vec<Value> {
    let trace = fs::read_to_string(receipt).expect("actual startup phase receipts");
    let events: Vec<Value> = trace
        .lines()
        .map(|line| serde_json::from_str(line).expect("phase JSON"))
        .collect();
    assert_eq!(events.len(), 6, "{trace}");
    for (index, phase) in ["early-init", "after-init", "window-setup"]
        .iter()
        .enumerate()
    {
        let entered = &events[index * 2];
        let completed = &events[index * 2 + 1];
        assert_eq!(entered["phase"], *phase, "{trace}");
        assert_eq!(entered["status"], "entered", "{trace}");
        assert_eq!(completed["phase"], *phase, "{trace}");
        assert_eq!(completed["status"], "completed", "{trace}");
        assert_eq!(completed["cache_size"], 0, "{trace}");
        let backend = completed["window_system"]
            .as_str()
            .expect("printed backend symbol");
        assert!(!backend.is_empty(), "{trace}");
        let printed = format!("({backend} 0)");
        assert_eq!(
            completed["value"].as_str(),
            Some(printed.as_str()),
            "{trace}"
        );
    }
    events.into_iter().skip(1).step_by(2).collect()
}
