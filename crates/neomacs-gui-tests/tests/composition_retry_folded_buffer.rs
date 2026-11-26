//! Issue #830: an invisible cloud+VS16 span must survive graphical redisplay.
//!
//! At pinned GNU a360712c9d, xdisp.c::handle_invisible_prop skips hidden text;
//! composite.c::composition_reseat_it and xdisp.c::next_element_from_composition
//! handle composition during the display walk. This row checks the reported
//! public redisplay outcome, not Neo's internal retry count or snapshot layout.
#![cfg(target_os = "linux")]

use neomacs_gui_tests::{
    DisplayHarness, GuiBackend, GuiRunOptions, GuiRunStatus, GuiScenario, GuiTestPlan,
    ProcessGuiCommandRunner,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

// Both editors load this exact reusable fixture, also saved among artifacts.
// Keep hidden-start before the visible-lead insertion, as in issue #830.
const FIXTURE: &str = r#";;; Issue #830 folded automatic-composition coverage retry -*- lexical-binding: t; -*-
(unless (display-graphic-p)
  (error "Issue #830 requires an actual graphical frame"))

(defun repro/crash ()
  "Lay out the issue's folded buffer with cloud and variation selector 16."
  (interactive)
  (let ((inhibit-read-only t)
        (inhibit-modification-hooks t))
    (switch-to-buffer (get-buffer-create "*repro*"))
    (erase-buffer)
    (let* ((cols (max 1 (ceiling (window-body-width nil t) (frame-char-width))))
           (rows (max 1 (ceiling (window-body-height nil t) (frame-char-height))))
           (screenful (* rows cols))
           (span-at (+ screenful (floor screenful 2)))
           (hidden-start (point))
           (hidden (concat (make-string span-at ?a)
                           "☁️" ; U+2601 followed by U+FE0F
                           (make-string (- (* 2 screenful) span-at 2) ?a))))
      (insert "visible lead\n")
      (insert hidden)
      (put-text-property hidden-start (point) 'invisible t)
      (insert "visible tail\n")
      (goto-char (point-min))
      (message "repro: rows=%d cols=%d screenful=%d span-at=%d point-max=%d"
               rows cols screenful (1+ span-at) (point-max)))
    (redisplay t)
    (message "repro: redisplay survived")))

(repro/crash)
;; Let subsequent native redisplay run as in the reporter's GNU comparison;
;; a mere return from loading the fixture is not the success witness.
(run-at-time
 3 nil
 (lambda ()
   (let ((messages (with-current-buffer "*Messages*" (buffer-string))))
     (with-temp-file (getenv "NEOMACS_COMPOSITION_RETRY_RESULT")
       (insert messages)))
   (kill-emacs 0)))
"#;

#[test]
fn folded_cloud_variation_selector_redisplay_survives_against_gnu() {
    assert_eq!(std::env::var("NEOVM_ORACLE_MODE").as_deref(), Ok("verify"));
    let forced = PathBuf::from(
        std::env::var_os("NEOVM_FORCE_ORACLE_PATH").expect("set NEOVM_FORCE_ORACLE_PATH"),
    );
    let oracle = neomacs_test_oracle::gnu::gnu_oracle();
    assert!(
        oracle.is_attested(),
        "paired row requires the pinned GNU oracle"
    );
    let gnu = fs::canonicalize(forced).expect("GNU executable");
    assert_eq!(gnu, oracle.executable_path());
    let backend = neomacs_gui_tests::requested_backend().expect("GUI backend");
    assert!(matches!(
        backend,
        GuiBackend::LinuxX11 | GuiBackend::LinuxWayland
    ));
    let root = neomacs_infra::workspace_root();
    let neo = PathBuf::from(
        std::env::var_os("NEOMACS_GUI_TEST_BINARY").expect("set NEOMACS_GUI_TEST_BINARY"),
    );
    assert!(
        neo.is_file(),
        "Neomacs executable must already exist: {neo:?}"
    );
    let runtime = std::env::var_os("NEOMACS_RUNTIME_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.clone());
    let nonce = neomacs_gui_tests::unique_nonce();
    let artifacts = root.join(format!(
        "tmp/neomacs-gui-tests/composition-retry-830-{}-{nonce}",
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
    let fixture = artifacts.join("repro-duplicate-window-snapshot.el");
    fs::write(&fixture, FIXTURE).unwrap();
    let fontconfig = private_fonts(&artifacts);
    // The attested GNU recipe is GTK3/X11, not PGTK. Neo exercises the selected
    // backend; its Wayland result does not establish GNU Wayland behavior.
    for (editor, binary, backend) in [("gnu", &gnu, GuiBackend::LinuxX11), ("neo", &neo, backend)] {
        let owned = artifacts.join(editor);
        fs::create_dir_all(&owned).unwrap();
        let session = DisplayHarness::for_backend(backend)
            .start_session(&owned.join("display"))
            .expect("private GUI display");
        let receipt = owned.join("messages.txt");
        let mut plan = GuiTestPlan::new(
            backend,
            &root,
            &owned,
            GuiScenario::new(format!("{editor}-composition-retry-830"), &fixture),
        )
        .without_paint_proof()
        .with_program(binary)
        .with_env(
            "NEOMACS_COMPOSITION_RETRY_RESULT",
            receipt.to_string_lossy(),
        )
        .with_env("FONTCONFIG_FILE", fontconfig.to_string_lossy())
        .with_env("GSETTINGS_BACKEND", "memory");
        if backend == GuiBackend::LinuxX11 {
            plan = plan.with_env("GDK_BACKEND", "x11");
        }
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
            .expect("owned editor process");
        assert!(!result.timed_out, "{editor} redisplay stalled: {result:#?}");
        assert_eq!(result.exit_code, Some(0), "{editor}: {result:#?}");
        assert_eq!(result.status, GuiRunStatus::Passed, "{editor}: {result:#?}");
        let messages = fs::read_to_string(&receipt).expect("actual redisplay survival receipt");
        assert!(messages.contains("repro: rows="), "{editor}: {messages}");
        assert!(
            messages.contains("repro: redisplay survived"),
            "{editor}: {messages}"
        );
    }
}

fn private_fonts(artifacts: &Path) -> PathBuf {
    let fonts = artifacts.join("emoji-font");
    let cache = artifacts.join("fontconfig-cache");
    fs::create_dir_all(&fonts).unwrap();
    fs::create_dir_all(&cache).unwrap();
    fs::copy(
        neomacs_test_fonts::noto_color_emoji_2_051(),
        fonts.join("NotoColorEmoji.ttf"),
    )
    .expect("pinned emoji font");
    let config = artifacts.join("fonts.conf");
    fs::write(
        &config,
        format!(
            "<fontconfig><dir>{}</dir><dir>{}</dir><cachedir>{}</cachedir></fontconfig>",
            neomacs_test_fonts::desktop_fonts().directory().display(),
            fonts.display(),
            cache.display()
        ),
    )
    .unwrap();
    config
}
