use super::*;
use flate2::{Compression, write::GzEncoder};

fn github_workflow_job<'a>(workflow: &'a str, name: &str) -> &'a str {
    let marker = format!("\n  {name}:\n");
    let (_, tail) = workflow
        .split_once(&marker)
        .unwrap_or_else(|| panic!("workflow must define job {name}"));
    let mut offset = 0;
    for line in tail.split_inclusive('\n') {
        if line.starts_with("  ") && !line.starts_with("   ") {
            return &tail[..offset];
        }
        offset += line.len();
    }
    tail
}

#[test]
fn top_level_dispatch_routes_perf_without_parsing_fresh_build_options() {
    run_xtask(
        PathBuf::from("/repo"),
        [OsString::from("perf"), OsString::from("list")],
    )
    .expect("perf list should not require a fresh-build profile");
}

#[test]
fn nix_runtime_closure_includes_the_cxx_standard_library() {
    let flake = include_str!("../../flake.nix");

    assert!(
        flake.contains("stdenv.cc.cc.lib"),
        "Neomacs links libstdc++, so the Nix runtime closure must own it"
    );
    assert!(
        flake.contains("lib.remove pkgs.ncurses (commonBuildInputsFor pkgs)"),
        "the development LD_LIBRARY_PATH must derive from the packaged runtime closure"
    );
}

#[test]
#[cfg(unix)]
fn linux_desktop_assets_install_the_runtime_window_identity() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .expect("xtask must live under the repository root");
    let fixture = tempdir();

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/install-linux-desktop-assets.sh"))
        .arg(&fixture)
        .output()
        .expect("run Linux desktop asset installer");
    assert!(
        output.status.success(),
        "desktop asset installation failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let installed_desktop =
        fs::read_to_string(fixture.join("share/applications/neomacs.desktop")).unwrap();
    let canonical_desktop =
        fs::read_to_string(repo_root.join("neomacs-display-runtime/assets/neomacs.desktop"))
            .unwrap();
    assert_eq!(installed_desktop, canonical_desktop);
    assert!(installed_desktop.contains("\nExec=neomacs %F\n"));
    assert!(installed_desktop.contains("\nIcon=neomacs\n"));

    let installed_icon =
        fs::read(fixture.join("share/icons/hicolor/scalable/apps/neomacs.svg")).unwrap();
    let runtime_icon =
        fs::read(repo_root.join("neomacs-display-runtime/assets/window-icon.svg")).unwrap();
    assert_eq!(
        installed_icon, runtime_icon,
        "packaging must install the exact SVG embedded by the runtime"
    );
}

#[test]
fn every_linux_package_uses_the_canonical_desktop_asset_installer() {
    for (name, script) in [
        ("tar", include_str!("../../scripts/package-release.sh")),
        ("Debian", include_str!("../../scripts/package-deb.sh")),
        (
            "AppImage",
            include_str!("../../scripts/package-appimage.sh"),
        ),
        ("RPM", include_str!("../../scripts/package-rpm.sh")),
    ] {
        assert!(
            script.contains("scripts/install-linux-desktop-assets.sh"),
            "{name} packaging bypasses the canonical desktop assets"
        );
        assert!(
            !script.contains("assets/logo-128.png"),
            "{name} packaging still uses the legacy unrelated PNG"
        );
        assert!(
            !script.contains("[Desktop Entry]"),
            "{name} packaging duplicates the canonical desktop entry"
        );
    }
}

#[test]
#[cfg(unix)]
fn linux_ci_setup_profiles_expose_capabilities_and_reject_unknown_profiles() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must live under the repository root")
        .to_path_buf();
    let script = repo_root.join("scripts/ci/setup-linux.sh");

    let packages = |profile: &str| {
        let output = Command::new("bash")
            .arg(&script)
            .args(["--list", profile])
            .output()
            .unwrap_or_else(|error| panic!("list {profile} Linux CI packages: {error}"));
        assert!(
            output.status.success(),
            "profile {profile} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("package list must be UTF-8")
    };

    let build = packages("build");
    assert!(build.lines().any(|package| package == "liblcms2-dev"));
    assert!(!build.lines().any(|package| package == "emacs-nox"));

    let oracle = packages("oracle");
    for package in ["liblcms2-dev", "emacs-nox", "libfaketime"] {
        assert!(oracle.lines().any(|candidate| candidate == package));
    }

    let ecosystem = packages("ecosystem");
    for package in [
        "emacs-nox",
        "gnupg",
        "xvfb",
        "xauth",
        "x11-utils",
        "xdotool",
        "imagemagick",
        "weston",
    ] {
        assert!(ecosystem.lines().any(|candidate| candidate == package));
    }

    let release = packages("release");
    for package in ["rpm", "binutils", "cpio", "file"] {
        assert!(release.lines().any(|candidate| candidate == package));
    }

    let invalid = Command::new("bash")
        .arg(script)
        .args(["--list", "typo"])
        .output()
        .expect("reject unknown Linux CI profile");
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("unknown profile: typo"));
}

#[test]
fn cranelift_dependencies_are_workspace_owned_and_share_one_release_line() {
    let workspace_manifest = include_str!("../../Cargo.toml");
    let versions: Vec<(&str, &str)> = workspace_manifest
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("cranelift-"))
        .map(|line| {
            let (name, requirement) = line
                .split_once(" = ")
                .expect("Cranelift dependency must have an inline requirement");
            let version = requirement
                .strip_prefix('"')
                .and_then(|tail| tail.split('"').next())
                .or_else(|| {
                    requirement
                        .split_once("version = \"")
                        .and_then(|(_, tail)| tail.split('"').next())
                })
                .expect("Cranelift dependency must declare a version");
            (name, version)
        })
        .collect();

    assert_eq!(versions.len(), 6, "all Cranelift crates must be covered");
    let release_line = |version: &str| {
        version
            .rsplit_once('.')
            .map(|(line, _)| line.to_owned())
            .expect("Cranelift version must contain a patch component")
    };
    let expected = release_line(versions[0].1);
    assert!(
        versions
            .iter()
            .all(|(_, version)| release_line(version) == expected),
        "Cranelift crates form one API-coupled release train; found {versions:?}"
    );

    let crate_manifest = include_str!("../../neovm-core/Cargo.toml");
    let declarations: Vec<&str> = crate_manifest
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("cranelift-"))
        .collect();
    assert_eq!(
        declarations.len(),
        6,
        "all Cranelift crates must be covered"
    );
    assert!(
        declarations
            .iter()
            .all(|line| line.contains("workspace = true") && line.contains("optional = true")),
        "neovm-core must consume optional workspace-owned Cranelift dependencies: {declarations:?}"
    );
}

#[test]
fn dependabot_groups_cranelift_release_train() {
    let config = include_str!("../../.github/dependabot.yml");
    let groups = config
        .split_once("\n    groups:\n")
        .map(|(_, groups)| groups)
        .expect("Cargo Dependabot updates must define dependency groups");

    assert!(groups.starts_with("      cranelift:\n"));
    assert!(groups.contains("\n          - \"cranelift-*\"\n"));
}

#[test]
#[cfg(unix)]
fn doom_install_contract_uses_neomacs_in_an_isolated_home() {
    use std::os::unix::fs::PermissionsExt;

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must live under the repository root")
        .to_path_buf();
    let fixture = tempdir();
    let doom_repository = fixture.join("doomemacs");
    let doom_bin = doom_repository.join("bin");
    let fake_neomacs = fixture.join("neomacs");
    let caller_home = fixture.join("caller-home");
    let report = fixture.join("doom-contract-report");

    fs::create_dir_all(&doom_bin).unwrap();
    fs::create_dir_all(&caller_home).unwrap();
    fs::write(
        &fake_neomacs,
        "#!/usr/bin/env bash\nset -euo pipefail\ntest \"$1\" = --batch\n",
    )
    .unwrap();
    fs::set_permissions(&fake_neomacs, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        doom_bin.join("doom"),
        r#"#!/usr/bin/env bash
set -euo pipefail
test "$*" = "--force install"
test "$EMACS" = "$DOOM_TEST_EXPECTED_EMACS"
test "$HOME" != "$DOOM_TEST_CALLER_HOME"
test "$XDG_CONFIG_HOME" = "$HOME/.config"
test "$XDG_CACHE_HOME" = "$HOME/.cache"
test "$XDG_DATA_HOME" = "$HOME/.local/share"
test "$XDG_STATE_HOME" = "$HOME/.local/state"
test "$EMACSDIR" = "$XDG_CONFIG_HOME/emacs"
test "$DOOMDIR" = "$XDG_CONFIG_HOME/doom"
"$EMACS" --batch
mkdir -p "$DOOMDIR"
touch "$DOOMDIR/init.el" "$DOOMDIR/config.el" "$DOOMDIR/packages.el"
printf 'args=%s\nemacs=%s\nhome=%s\n' "$*" "$EMACS" "$HOME" > "$DOOM_TEST_REPORT"
"#,
    )
    .unwrap();
    fs::set_permissions(doom_bin.join("doom"), fs::Permissions::from_mode(0o755)).unwrap();

    for args in [
        ["init", "--initial-branch=main"].as_slice(),
        ["config", "user.email", "ci@example.invalid"].as_slice(),
        ["config", "user.name", "CI"].as_slice(),
        ["add", "."].as_slice(),
        ["commit", "-m", "fixture"].as_slice(),
    ] {
        let status = Command::new("git")
            .args(args)
            .current_dir(&doom_repository)
            .status()
            .expect("run git for Doom fixture");
        assert!(status.success(), "git {args:?} failed");
    }

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/test-doom-install.sh"))
        .env("HOME", &caller_home)
        .env("NEOMACS_BIN", &fake_neomacs)
        .env("DOOM_REPOSITORY", &doom_repository)
        .env("DOOM_TEST_CALLER_HOME", &caller_home)
        .env("DOOM_TEST_EXPECTED_EMACS", &fake_neomacs)
        .env("DOOM_TEST_REPORT", &report)
        .output()
        .expect("run Doom installation contract");
    assert!(
        output.status.success(),
        "contract failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let report = fs::read_to_string(report).unwrap();
    assert!(report.contains("args=--force install\n"));
    assert!(report.contains(&format!("emacs={}\n", fake_neomacs.display())));
    assert!(!report.contains(&format!("home={}\n", caller_home.display())));
}

#[test]
fn ci_runs_the_doom_install_contract_against_the_shared_runtime() {
    let workflow = include_str!("../../.github/workflows/ci.yml");
    let job = github_workflow_job(workflow, "doom-install-compatibility");

    assert!(job.contains("needs: neomacs-test-runtime"));
    assert!(job.contains("- *download_test_runtime"));
    assert!(job.contains("- *unpack_test_runtime"));
    assert!(job.contains("NEOMACS_BIN: ${{ github.workspace }}/target/release/neomacs"));
    assert!(job.contains("run: ./scripts/test-doom-install.sh"));
}

#[test]
fn ci_lints_every_github_actions_workflow() {
    let workflow = include_str!("../../.github/workflows/ci.yml");
    let job = github_workflow_job(workflow, "workflow-lint");

    assert!(job.contains("github.event_name != 'schedule'"));
    assert!(job.contains("github.com/rhysd/actionlint/cmd/actionlint@v1.7.12"));
    assert!(job.contains(".github/workflows/*.yml"));
}

#[test]
fn rust_ci_setup_uses_the_workspace_toolchain_and_owns_test_tooling() {
    let action = include_str!("../../.github/actions/setup-rust/action.yml");

    assert!(action.contains("cache-key:"));
    assert!(action.contains("hashFiles('scripts/ci/setup-linux.sh'"));
    assert!(action.contains("install-nextest:"));
    assert!(action.contains("actions-rust-lang/setup-rust-toolchain@"));
    assert!(
        !action.contains("toolchain:"),
        "omitting a toolchain input makes rust-toolchain.toml the source of truth"
    );
    assert!(action.contains("rustflags: \"\""));
    assert!(action.contains("taiki-e/install-action@"));
    assert!(action.contains("tool: cargo-nextest"));
}

#[test]
fn ci_pins_external_actions_and_enables_automated_updates() {
    let workflows = [
        include_str!("../../.github/workflows/nextest-shards.yml"),
        include_str!("../../.github/workflows/ci.yml"),
        include_str!("../../.github/workflows/codeql.yml"),
        include_str!("../../.github/workflows/linux.yml"),
        include_str!("../../.github/workflows/nix-smoke.yml"),
        include_str!("../../.github/workflows/release.yml"),
        include_str!("../../.github/workflows/tmp_mac_test.yml"),
        include_str!("../../.github/workflows/window-oracle-nightly.yml"),
        include_str!("../../.github/workflows/windows-installer.yml"),
        include_str!("../../.github/actions/setup-rust/action.yml"),
    ];

    for workflow in workflows {
        for line in workflow.lines().map(str::trim) {
            let Some(action) = line.strip_prefix("uses: ") else {
                continue;
            };
            if action.starts_with("./") {
                continue;
            }
            let revision = action
                .split_once('@')
                .unwrap_or_else(|| panic!("external action lacks a revision: {action}"))
                .1
                .split_whitespace()
                .next()
                .unwrap();
            assert_eq!(
                revision.len(),
                40,
                "action is not pinned to a commit: {action}"
            );
            assert!(
                revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "action revision is not hexadecimal: {action}"
            );
        }
    }

    let dependabot = include_str!("../../.github/dependabot.yml");
    assert!(dependabot.contains("package-ecosystem: github-actions"));
    assert!(dependabot.contains("directory: /"));
}

#[test]
fn ci_uses_one_typed_sharded_nextest_workflow_for_core_and_oracle() {
    let reusable = include_str!("../../.github/workflows/nextest-shards.yml");
    assert!(reusable.contains("workflow_call:"));
    assert!(reusable.contains("suite:"));
    assert!(reusable.contains("core|oracle"));
    assert!(reusable.contains("package(neovm-core)"));
    assert!(reusable.contains("package(neovm-oracle-tests)"));
    assert!(reusable.contains("--partition slice:${{ matrix.partition }}/20"));
    assert_eq!(
        reusable.matches("case \"$SHARD_SUITE\" in").count(),
        1,
        "the closed suite selector must be decoded exactly once"
    );

    let workflow = include_str!("../../.github/workflows/ci.yml");
    let core = github_workflow_job(workflow, "neovm-core-tests");
    assert!(core.contains("needs: [neomacs-test-runtime, neomacs-workspace-test-archive]"));
    assert!(core.contains("uses: ./.github/workflows/nextest-shards.yml"));
    assert!(core.contains("suite: core"));

    let oracle = github_workflow_job(workflow, "neovm-oracle-tests");
    assert!(oracle.contains("needs: [neomacs-test-runtime, neomacs-workspace-test-archive]"));
    assert!(oracle.contains("uses: ./.github/workflows/nextest-shards.yml"));
    assert!(oracle.contains("suite: oracle"));
}

#[test]
fn ci_runs_offline_melpa_parity_from_shared_artifacts() {
    let workflow = include_str!("../../.github/workflows/ci.yml");
    let job = github_workflow_job(workflow, "neomacs-melpa-tests");

    assert!(job.contains("needs: [neomacs-test-runtime, neomacs-workspace-test-archive]"));
    assert!(!job.contains("if: ${{ false }}"));
    assert!(job.contains("name: neomacs-test-runtime-linux-x86_64"));
    assert!(job.contains("tar xzf neomacs-test-runtime-linux-x86_64.tar.gz"));
    assert!(job.contains("name: neomacs-workspace-tests-nextest-archive-linux-x86_64"));
    assert!(job.contains("NEOMACS_BIN: ${{ github.workspace }}/target/release/neomacs"));
    assert!(job.contains("NEOMACS_MELPA_ORACLE_EMACS: /usr/bin/emacs"));
    assert!(job.contains("run: scripts/ci/setup-linux.sh ecosystem"));
    for suite in ["batch", "tui", "gui"] {
        assert!(job.contains(&format!("suite: {suite}")));
    }
    assert!(job.contains("--skip tui_parity_tests:: --skip gui_parity_tests::"));
    assert!(job.contains("libtest_args: \"tui_parity_tests::\""));
    assert!(job.contains("libtest_args: \"gui_parity_tests::\""));
    assert!(job.contains("-E 'package(neomacs-melpa-tests)'"));
    assert!(job.contains("-- $LIBTEST_ARGS"));
    assert!(job.contains("--success-output immediate"));
}

#[test]
fn ci_executes_display_stack_and_real_gui_tests_from_shared_artifacts() {
    let workflow = include_str!("../../.github/workflows/ci.yml");

    let display = github_workflow_job(workflow, "neomacs-display-tests");
    assert!(display.contains("needs: neomacs-workspace-test-archive"));
    for package in [
        "neomacs-display-protocol",
        "neomacs-display-runtime",
        "neomacs-layout-engine",
        "neomacs-renderer-wgpu",
    ] {
        assert!(display.contains(&format!("package({package})")));
    }
    assert!(display.contains("-E \"$NEXTEST_FILTER\""));
    assert!(display.contains("protocol)|package(neomacs-display-runtime)"));

    let gui = github_workflow_job(workflow, "neomacs-gui-tests");
    assert!(gui.contains("needs: [neomacs-test-runtime, neomacs-workspace-test-archive]"));
    assert!(gui.contains("NEOMACS_GUI_TEST_BACKEND: x11"));
    assert!(gui.contains("NEOMACS_GUI_TEST_GNU_EMACS: /usr/bin/emacs"));
    assert!(gui.contains("package(neomacs-gui-tests)"));
}

#[test]
fn ci_runs_live_melpa_only_as_an_explicit_canary() {
    let workflow = include_str!("../../.github/workflows/ci.yml");
    let job = github_workflow_job(workflow, "neomacs-melpa-live-canary");

    assert!(workflow.contains("schedule:"));
    assert!(job.contains("needs: [neomacs-test-runtime, neomacs-workspace-test-archive]"));
    assert!(job.contains("github.event_name == 'schedule'"));
    assert!(job.contains("github.event_name == 'workflow_dispatch'"));
    assert!(job.contains("- *download_test_runtime"));
    assert!(job.contains("- *unpack_test_runtime"));
    assert!(job.contains("- *download_workspace_test_archive"));
    assert!(job.contains("--run-ignored only"));
    assert!(job.contains("test(=live_melpa_ecosystem_installs_and_survives_restart)"));
    assert!(job.contains("--success-output immediate"));
}

#[test]
fn nextest_serializes_melpa_package_processes() {
    let nextest = include_str!("../../.config/nextest.toml");
    assert!(nextest.contains("filter = 'package(neomacs-melpa-tests)'"));
    assert!(nextest.contains("threads-required = \"num-test-threads\""));
}

#[test]
fn windows_installer_removes_the_legacy_path_rewrite_implementation() {
    let installer = include_str!("../../assets/windows-installer.nsi");

    for forbidden in [
        "ENVIRONMENT_KEY",
        "WriteRegExpandStr",
        "AddToSystemPath",
        "RemoveFromSystemPath",
        "AddedToPath",
    ] {
        assert!(
            !installer.contains(forbidden),
            "legacy whole-PATH rewrite marker must stay removed; found {forbidden}"
        );
    }
}

#[test]
fn windows_installer_defaults_to_a_non_elevated_user_scope() {
    let installer = include_str!("../../assets/windows-installer.nsi");

    assert!(installer.contains("RequestExecutionLevel user"));
    assert!(installer.contains(r#"InstallDir "$LOCALAPPDATA\Programs\${PRODUCT_NAME}""#));
    assert!(installer.contains("SetShellVarContext current"));
    assert!(
        !installer.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("ReadRegStr HKLM")
                || line.starts_with("WriteRegStr HKLM")
                || line.starts_with("WriteRegDWORD HKLM")
                || line.starts_with("DeleteRegKey HKLM")
        }),
        "default Windows installer must not mutate machine-scoped registration"
    );
}

#[test]
fn windows_installer_owns_app_paths_for_both_commands() {
    let installer = include_str!("../../assets/windows-installer.nsi");

    for executable in ["neomacs.exe", "neomacsclient.exe"] {
        let app_path = format!(r#"App Paths\{executable}"#);
        let installed_executable = format!(r#"$INSTDIR\bin\{executable}"#);
        assert!(
            installer.contains(&app_path),
            "installer must register {executable} with Windows App Paths"
        );
        assert!(
            installer.contains(&installed_executable),
            "App Paths registration must resolve to {installed_executable}"
        );
    }

    assert!(installer.contains("!macro RemoveOwnedAppPath KEY EXECUTABLE"));
    assert!(installer.contains("DeleteRegKey /ifempty HKCU \"${KEY}\""));
    assert!(
        installer.contains(
            "!insertmacro RemoveOwnedAppPath \"${NEOMACS_APP_PATH_KEY}\" \"neomacs.exe\""
        )
    );
    assert!(installer.contains(
        "!insertmacro RemoveOwnedAppPath \"${NEOMACSCLIENT_APP_PATH_KEY}\" \"neomacsclient.exe\""
    ));
}

#[test]
fn windows_installer_owns_current_user_start_menu_shortcuts() {
    let installer = include_str!("../../assets/windows-installer.nsi");

    assert!(installer.contains(
        r#"CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\bin\neomacs.exe""#
    ));
    assert!(installer.contains(
        r#"CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk" "$INSTDIR\uninstall.exe""#
    ));
    assert!(installer.contains(r#"Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk""#));
    assert!(
        installer.contains(r#"Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk""#)
    );
    assert!(installer.contains("Function un.onInit"));
    assert_eq!(installer.matches("SetShellVarContext current").count(), 2);
}

#[test]
fn windows_installer_publishes_complete_owned_uninstall_metadata() {
    let installer = include_str!("../../assets/windows-installer.nsi");

    for field in [
        "DisplayName",
        "DisplayVersion",
        "Publisher",
        "URLInfoAbout",
        "InstallLocation",
        "DisplayIcon",
        "UninstallString",
        "QuietUninstallString",
        "EstimatedSize",
        "NoModify",
        "NoRepair",
    ] {
        assert!(
            installer.contains(&format!(r#""{field}""#)),
            "Apps & Features metadata must include {field}"
        );
    }
    assert!(installer.contains(r#"!define PRODUCT_REGISTRATION_NAME "${PRODUCT_NAME} (User)""#));
    assert!(installer.contains(r#"'"$INSTDIR\uninstall.exe"'"#));
}

#[test]
fn windows_installer_removes_the_previous_owned_payload_before_replacement() {
    let installer = include_str!("../../assets/windows-installer.nsi");

    assert!(installer.contains("Function RemovePreviousUserInstallation"));
    assert!(installer.contains(r#"ExecWait '"$R0\uninstall.exe" /S _?=$R0' $R1"#));

    let initialization = installer
        .split_once("Function .onInit")
        .and_then(|(_, rest)| rest.split_once("FunctionEnd"))
        .map(|(body, _)| body)
        .expect("installer must define .onInit");
    assert!(
        !initialization.contains("Call RemovePreviousUserInstallation"),
        "opening and cancelling the installer must not remove the current version"
    );

    let install_section = installer
        .split_once(r#"Section "!${PRODUCT_NAME}" SEC_MAIN"#)
        .and_then(|(_, rest)| rest.split_once("SectionEnd"))
        .map(|(body, _)| body)
        .expect("installer must define its main installation section");
    let first_instruction = install_section
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .expect("main installation section must not be empty");
    assert_eq!(first_instruction, "Call RemovePreviousUserInstallation");
}

#[test]
#[cfg(unix)]
fn windows_uninstall_manifest_names_only_packaged_files_and_empty_directories() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must live under the repository root")
        .to_path_buf();
    let fixture = tempdir();
    let package = fixture.join("package");
    let output = fixture.join("uninstall-files.nsh");
    fs::create_dir_all(package.join("bin")).unwrap();
    fs::create_dir_all(package.join("share/neomacs/lisp")).unwrap();
    fs::write(package.join("bin/neomacs.exe"), b"fixture").unwrap();
    fs::write(package.join("share/neomacs/lisp/startup.el"), b"fixture").unwrap();

    let result = Command::new("bash")
        .arg(repo_root.join("scripts/generate-nsis-uninstall-include.sh"))
        .arg(&package)
        .arg(&output)
        .output()
        .expect("run uninstall-manifest generator");
    assert!(
        result.status.success(),
        "generator failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let manifest = fs::read_to_string(output).unwrap();
    assert!(manifest.contains(r#"Delete "$INSTDIR\bin\neomacs.exe""#));
    assert!(manifest.contains(r#"Delete "$INSTDIR\share\neomacs\lisp\startup.el""#));
    assert!(manifest.contains(r#"RMDir "$INSTDIR\share\neomacs\lisp""#));
    assert!(manifest.contains(r#"RMDir "$INSTDIR""#));
    assert!(
        !manifest.contains("RMDir /r"),
        "uninstaller must preserve files not owned by its package manifest"
    );
}

#[test]
#[cfg(unix)]
fn windows_gstreamer_packager_accepts_official_pango_runtime_shape() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must live under the repository root")
        .to_path_buf();
    let fixture = tempdir();
    let gst_root = fixture.join("gstreamer");
    let gst_bin = gst_root.join("bin");
    let package_root = fixture.join("package");
    fs::create_dir_all(&gst_bin).unwrap();
    fs::create_dir_all(&package_root).unwrap();

    // This is the Pango runtime shape shipped by GStreamer 1.26.9's official
    // Windows MSVC runtime MSI.  Windows uses the native Pangowin32 backend;
    // the package intentionally does not contain the Unix PangoFT2 backend.
    let runtime_dlls = [
        "glib-2.0-0.dll",
        "gobject-2.0-0.dll",
        "gstreamer-1.0-0.dll",
        "gstvideo-1.0-0.dll",
        "cairo-2.dll",
        "pango-1.0-0.dll",
        "pangocairo-1.0-0.dll",
        "pangowin32-1.0-0.dll",
    ];
    for dll in runtime_dlls {
        fs::write(gst_bin.join(dll), b"fixture").unwrap();
    }

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/vendor-windows-gstreamer-runtime.sh"))
        .arg("--package-root")
        .arg(&package_root)
        .arg("--bin-dir")
        .arg(&package_root)
        .env("GSTREAMER_ROOT_X86_64", &gst_root)
        .output()
        .expect("run Windows GStreamer runtime packager");

    assert!(
        output.status.success(),
        "official Windows Pango runtime shape must be packageable; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for dll in runtime_dlls {
        assert!(
            package_root.join(dll).is_file(),
            "packager must copy {dll} beside neomacs.exe"
        );
    }

    fs::remove_dir_all(fixture).unwrap();
}

fn parse_options(args: &[&str]) -> FreshBuildOptions {
    FreshBuildOptions::parse(PathBuf::from("/repo"), args.iter().map(OsString::from)).unwrap()
}

#[test]
fn parse_without_release_is_rejected() {
    let result = FreshBuildOptions::parse(PathBuf::from("/repo"), std::iter::empty::<OsString>());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("--release"),
        "fresh-build without --release must be rejected with a --release hint; got: {err}"
    );
}

#[test]
fn parse_release_uses_release_bin_dir() {
    let options = parse_options(&["--release"]);
    assert_eq!(options.profile, BuildProfile::Release);
    assert_eq!(options.bin_dir, PathBuf::from("/repo/target/release"));
}

#[test]
fn parse_dev_skips_byte_compile_by_default() {
    let options = parse_options(&["--profile", "dev"]);
    assert_eq!(options.profile, BuildProfile::Dev);
    assert_eq!(options.bin_dir, PathBuf::from("/repo/target/debug"));
    assert!(options.no_byte_compile);
}

#[test]
fn parse_dev_release_skips_byte_compile_by_default() {
    let options = parse_options(&["--profile", "dev-release"]);
    assert_eq!(options.profile, BuildProfile::DevRelease);
    assert_eq!(options.bin_dir, PathBuf::from("/repo/target/dev-release"));
    assert!(options.no_byte_compile);
}

#[test]
fn parse_dev_preserves_no_byte_compile_flag() {
    let options = parse_options(&["--profile", "dev", "--no-byte-compile"]);
    assert_eq!(options.profile, BuildProfile::Dev);
    assert_eq!(options.bin_dir, PathBuf::from("/repo/target/debug"));
    assert!(options.no_byte_compile);
}

#[test]
fn explicit_bin_dir_overrides_release_default() {
    let options = parse_options(&["--release", "--bin-dir", "out/neomacs-bin"]);
    assert_eq!(options.profile, BuildProfile::Release);
    assert_eq!(options.bin_dir, PathBuf::from("/repo/out/neomacs-bin"));
}

#[test]
fn explicit_bin_dir_before_release_stays_in_effect() {
    let options = parse_options(&["--bin-dir", "out/neomacs-bin", "--release"]);
    assert_eq!(options.profile, BuildProfile::Release);
    assert_eq!(options.bin_dir, PathBuf::from("/repo/out/neomacs-bin"));
}

#[test]
fn parse_aot_preload_defaults_off_and_flag_enables() {
    assert!(!parse_options(&["--release"]).aot_preload);
    let options = parse_options(&["--release", "--aot-preload"]);
    assert!(options.aot_preload);
    // The flag is independent of the others (does not perturb defaults).
    assert_eq!(options.profile, BuildProfile::Release);
    assert!(!options.dry_run);
    assert!(!options.skip_build);
}

#[test]
fn parse_aot_preload_composes_with_dry_run() {
    let options = parse_options(&["--release", "--aot-preload", "--dry-run"]);
    assert!(options.aot_preload);
    assert!(options.dry_run);
}

#[test]
fn initial_cargo_build_passes_no_features_by_default_on_linux() {
    let options = parse_options(&["--release"]);
    let args = initial_cargo_build_args(&options);

    assert_eq!(
        args,
        vec![
            OsString::from("build"),
            OsString::from("--verbose"),
            OsString::from("-p"),
            OsString::from("neomacs"),
            OsString::from("--profile"),
            OsString::from("release"),
        ]
    );
}

#[test]
fn initial_cargo_build_passes_wpe_webkit_when_requested() {
    let options = parse_options(&["--features", "wpe-webkit", "--release"]);
    let args = initial_cargo_build_args(&options);

    assert_eq!(
        args,
        vec![
            OsString::from("build"),
            OsString::from("--verbose"),
            OsString::from("-p"),
            OsString::from("neomacs"),
            OsString::from("--features"),
            OsString::from("wpe-webkit"),
            OsString::from("--profile"),
            OsString::from("release"),
        ]
    );
}

#[test]
fn initial_cargo_build_passes_no_features_on_non_linux() {
    let options = parse_options(&["--release"]);
    let args = initial_cargo_build_args(&options);

    assert_eq!(
        args,
        vec![
            OsString::from("build"),
            OsString::from("--verbose"),
            OsString::from("-p"),
            OsString::from("neomacs"),
            OsString::from("--profile"),
            OsString::from("release"),
        ]
    );
}

#[test]
fn compile_main_uses_final_dumped_emacs() {
    let options = parse_options(&["--release"]);
    let paths = pipeline_paths(&options);

    assert_eq!(compile_main_emacs(&paths), paths.final_bin.as_path());
    assert_ne!(compile_main_emacs(&paths), paths.bootstrap.as_path());
}

#[test]
fn gen_lisp_bootstrap_byte_compile_uses_bootstrap_emacs() {
    let options = parse_options(&["--release"]);
    let paths = pipeline_paths(&options);

    assert_eq!(
        bootstrap_byte_compile_emacs(&paths),
        paths.bootstrap.as_path()
    );
    assert_ne!(
        bootstrap_byte_compile_emacs(&paths),
        paths.final_bin.as_path()
    );
}

#[test]
fn usage_places_preloaded_lisp_compile_before_final_pdump() {
    let usage = usage_text();
    let preloaded = usage
        .find("bootstrap-neomacs byte-compiles the GNU src/lisp.mk preloaded Lisp set")
        .unwrap();
    let pdump = usage.find("neomacs-temacs --temacs=pdump").unwrap();
    let compile_main = usage
        .find("neomacs byte-compiles the GNU compile-main")
        .unwrap();

    assert!(preloaded < pdump);
    assert!(pdump < compile_main);
}

#[test]
fn parse_preloaded_lisp_sources_matches_gnu_lisp_mk_shape() {
    let tempdir = tempdir();
    let lisp_root = tempdir.join("lisp");
    fs::create_dir_all(lisp_root.join("progmodes")).unwrap();
    fs::create_dir_all(lisp_root.join("leim")).unwrap();
    fs::write(lisp_root.join("files.el"), "").unwrap();
    fs::write(lisp_root.join("progmodes/elisp-mode.el"), "").unwrap();
    fs::write(lisp_root.join("site-load.el"), "").unwrap();
    fs::write(lisp_root.join("leim/leim-list.el"), "").unwrap();
    fs::write(
        lisp_root.join("no-byte.el"),
        ";; Local Variables:\n;; no-byte-compile: t\n;; End:\n",
    )
    .unwrap();

    let contents = r#"
      (load "files")
(load "progmodes/elisp-mode")
(load "leim/leim-list.el" t)
(load "site-load" t)
(load "no-byte")
"#;

    let parsed = parse_preloaded_lisp_sources_from_str(contents, &lisp_root);

    assert_eq!(
        parsed,
        vec![
            lisp_root.join("files.el"),
            lisp_root.join("progmodes/elisp-mode.el"),
        ]
    );
}

#[test]
fn preloaded_characters_dependencies_match_gnu_makefile_rule() {
    let tempdir = tempdir();
    let lisp_root = tempdir.join("lisp");
    fs::create_dir_all(lisp_root.join("international")).unwrap();
    fs::write(lisp_root.join("international/charscript.el"), "").unwrap();
    fs::write(lisp_root.join("international/emoji-zwj.el"), "").unwrap();

    assert_eq!(
        preloaded_characters_dependency_sources(&lisp_root),
        vec![
            lisp_root.join("international/charscript.el"),
            lisp_root.join("international/emoji-zwj.el"),
        ]
    );
}

#[test]
fn bytecode_rebuild_with_dependencies_follows_newer_dependency_elc() {
    let tempdir = tempdir();
    let source = tempdir.join("characters.el");
    let dependency = tempdir.join("emoji-zwj.el");
    fs::write(&source, "").unwrap();
    fs::write(&dependency, "").unwrap();
    fs::write(source.with_extension("elc"), "target\n").unwrap();
    write_elc_newer_than(&dependency, &source.with_extension("elc"));

    assert!(bytecode_needs_rebuild_with_dependencies(
        &source,
        &[dependency]
    ));
}

#[test]
fn parse_compile_first_skips_native_entries_by_default() {
    let tempdir = tempdir();
    let lisp_root = tempdir.join("lisp");
    fs::create_dir_all(lisp_root.join("emacs-lisp")).unwrap();
    fs::write(lisp_root.join("emacs-lisp/early.el"), "").unwrap();
    fs::write(lisp_root.join("emacs-lisp/native-only.el"), "").unwrap();

    let contents = "\
COMPILE_FIRST = $(lisp)/emacs-lisp/early.elc \\
                $(lisp)/missing.elc
ifeq ($(HAVE_NATIVE_COMP),yes)
COMPILE_FIRST += $(lisp)/emacs-lisp/native-only.elc
endif
";

    let parsed = parse_compile_first_sources_from_str(contents, &lisp_root, false);
    assert_eq!(parsed, vec![lisp_root.join("emacs-lisp/early.el")]);
}

#[test]
fn parse_compile_first_includes_native_entries_when_enabled() {
    let tempdir = tempdir();
    let lisp_root = tempdir.join("lisp");
    fs::create_dir_all(lisp_root.join("emacs-lisp")).unwrap();
    fs::write(lisp_root.join("emacs-lisp/early.el"), "").unwrap();
    fs::write(lisp_root.join("emacs-lisp/native-only.el"), "").unwrap();

    let contents = "\
ifeq ($(HAVE_NATIVE_COMP),yes)
COMPILE_FIRST += $(lisp)/emacs-lisp/native-only.elc
endif
COMPILE_FIRST += $(lisp)/emacs-lisp/early.elc
";

    let parsed = parse_compile_first_sources_from_str(contents, &lisp_root, true);
    assert_eq!(
        parsed,
        vec![
            lisp_root.join("emacs-lisp/native-only.el"),
            lisp_root.join("emacs-lisp/early.el"),
        ]
    );
}

#[test]
fn parse_main_first_sources_handles_gnu_multiline_list() {
    let lisp_root = PathBuf::from("/repo/lisp");
    let contents = "\
MAIN_FIRST = ./emacs-lisp/eieio.el ./emacs-lisp/eieio-base.el \\
  ./org/ox.el ./already-elc.elc
";

    let parsed = parse_main_first_sources_from_str(contents, &lisp_root);

    assert_eq!(
        parsed,
        vec![
            lisp_root.join("emacs-lisp/eieio.el"),
            lisp_root.join("emacs-lisp/eieio-base.el"),
            lisp_root.join("org/ox.el"),
            lisp_root.join("already-elc.el"),
        ]
    );
}

#[test]
fn parse_compile_main_dependencies_reads_gnu_makefile_rules() {
    let lisp_root = PathBuf::from("/repo/lisp");
    let contents = "\
$(lisp)/progmodes/cc-align.elc \\
  $(lisp)/progmodes/cc-cmds.elc: \\
  $(lisp)/progmodes/cc-bytecomp.elc $(lisp)/progmodes/cc-defs.elc
$(lisp)/progmodes/js.elc: $(lisp)/progmodes/cc-mode.elc $(srcdir)/ignored.elc
not-lisp.elc: $(lisp)/ignored.elc
";

    let deps = parse_compile_main_dependencies_from_str(contents, &lisp_root);

    let cc_bytecomp = lisp_root.join("progmodes/cc-bytecomp.el");
    let cc_defs = lisp_root.join("progmodes/cc-defs.el");
    assert_eq!(
        deps.get(&lisp_root.join("progmodes/cc-align.el")).unwrap(),
        &BTreeSet::from([cc_bytecomp.clone(), cc_defs.clone()])
    );
    assert_eq!(
        deps.get(&lisp_root.join("progmodes/cc-cmds.el")).unwrap(),
        &BTreeSet::from([cc_bytecomp, cc_defs])
    );
    assert_eq!(
        deps.get(&lisp_root.join("progmodes/js.el")).unwrap(),
        &BTreeSet::from([lisp_root.join("progmodes/cc-mode.el")])
    );
    assert!(!deps.contains_key(&lisp_root.join("ignored.el")));
}

#[test]
fn compile_main_dependency_waves_follow_gnu_cc_mode_rules() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let lisp_root = repo_root.join("lisp");
    let contents = fs::read_to_string(lisp_root.join("Makefile.in")).unwrap();
    let deps = parse_compile_main_dependencies_from_str(&contents, &lisp_root);
    let source = |rel: &str| lisp_root.join(rel);
    let sources = vec![
        source("progmodes/cc-bytecomp.el"),
        source("progmodes/cc-defs.el"),
        source("progmodes/cc-vars.el"),
        source("progmodes/cc-langs.el"),
        source("progmodes/cc-engine.el"),
        source("progmodes/cc-align.el"),
        source("progmodes/cc-cmds.el"),
        source("progmodes/cc-menus.el"),
        source("progmodes/cc-styles.el"),
        source("progmodes/cc-mode.el"),
        source("progmodes/js.el"),
    ];

    let waves = compile_main_dependency_waves(sources, &deps).unwrap();
    let wave_index = |path: PathBuf| {
        waves
            .iter()
            .position(|wave| wave.contains(&path))
            .unwrap_or_else(|| panic!("{} missing from dependency waves", path.display()))
    };

    let cc_bytecomp = wave_index(source("progmodes/cc-bytecomp.el"));
    let cc_defs = wave_index(source("progmodes/cc-defs.el"));
    let cc_vars = wave_index(source("progmodes/cc-vars.el"));
    let cc_langs = wave_index(source("progmodes/cc-langs.el"));
    let cc_engine = wave_index(source("progmodes/cc-engine.el"));
    let cc_align = wave_index(source("progmodes/cc-align.el"));
    let cc_cmds = wave_index(source("progmodes/cc-cmds.el"));
    let cc_menus = wave_index(source("progmodes/cc-menus.el"));
    let cc_styles = wave_index(source("progmodes/cc-styles.el"));
    let cc_mode = wave_index(source("progmodes/cc-mode.el"));
    let js = wave_index(source("progmodes/js.el"));

    assert!(cc_bytecomp < cc_defs);
    assert!(cc_defs < cc_vars);
    assert!(cc_vars < cc_langs);
    assert!(cc_langs < cc_engine);
    assert!(cc_engine < cc_align);
    assert!(cc_engine < cc_cmds);
    assert!(cc_align < cc_styles);
    for prerequisite in [
        cc_vars, cc_langs, cc_engine, cc_align, cc_cmds, cc_menus, cc_styles,
    ] {
        assert!(prerequisite < cc_mode);
    }
    assert!(cc_mode < js);
}

#[test]
fn compile_main_rebuild_closure_follows_gnu_make_prerequisites() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let lisp_root = repo_root.join("lisp");
    let contents = fs::read_to_string(lisp_root.join("Makefile.in")).unwrap();
    let deps = parse_compile_main_dependencies_from_str(&contents, &lisp_root);
    let source = |rel: &str| lisp_root.join(rel);
    let sources = vec![
        source("progmodes/cc-bytecomp.el"),
        source("progmodes/cc-defs.el"),
        source("progmodes/cc-vars.el"),
        source("progmodes/cc-langs.el"),
        source("progmodes/cc-engine.el"),
        source("progmodes/cc-align.el"),
        source("progmodes/cc-cmds.el"),
        source("progmodes/cc-fonts.el"),
        source("progmodes/cc-menus.el"),
        source("progmodes/cc-styles.el"),
        source("progmodes/cc-mode.el"),
        source("progmodes/js.el"),
    ];

    let rebuild = compile_main_rebuild_closure(
        &sources,
        &deps,
        BTreeSet::from([source("progmodes/cc-vars.el")]),
    );

    for rel in [
        "progmodes/cc-vars.el",
        "progmodes/cc-langs.el",
        "progmodes/cc-engine.el",
        "progmodes/cc-align.el",
        "progmodes/cc-cmds.el",
        "progmodes/cc-fonts.el",
        "progmodes/cc-styles.el",
        "progmodes/cc-mode.el",
        "progmodes/js.el",
    ] {
        assert!(
            rebuild.contains(&source(rel)),
            "{rel} should rebuild after cc-vars.elc changes"
        );
    }

    assert!(!rebuild.contains(&source("progmodes/cc-bytecomp.el")));
    assert!(!rebuild.contains(&source("progmodes/cc-defs.el")));
    assert!(!rebuild.contains(&source("progmodes/cc-menus.el")));
}

#[test]
fn compile_main_sources_needing_rebuild_follows_newer_prerequisite_elc() {
    let tempdir = tempdir();
    let lisp_root = tempdir.join("lisp");
    let progmodes = lisp_root.join("progmodes");
    fs::create_dir_all(&progmodes).unwrap();

    let source = |name: &str| progmodes.join(format!("{name}.el"));
    let dep = source("dep");
    let target = source("target");
    let downstream = source("downstream");
    for source in [&dep, &target, &downstream] {
        fs::write(source, ";;; source\n").unwrap();
    }

    fs::write(target.with_extension("elc"), "target\n").unwrap();
    write_elc_newer_than(&downstream, &target.with_extension("elc"));
    write_elc_newer_than(&dep, &downstream.with_extension("elc"));

    let deps = BTreeMap::from([
        (target.clone(), BTreeSet::from([dep.clone()])),
        (downstream.clone(), BTreeSet::from([target.clone()])),
    ]);
    let rebuild = compile_main_sources_needing_rebuild(
        vec![dep.clone(), target.clone(), downstream.clone()],
        &deps,
    );

    assert_eq!(rebuild, vec![target, downstream]);
}

#[test]
fn generated_lisp_bytecode_files_collects_nested_elc_files() {
    let tempdir = tempdir();
    let lisp_root = tempdir.join("lisp");
    fs::create_dir_all(lisp_root.join("emacs-lisp")).unwrap();
    fs::create_dir_all(lisp_root.join("org")).unwrap();
    fs::write(lisp_root.join("emacs-lisp/macroexp.elc"), "").unwrap();
    fs::write(lisp_root.join("org/org.elc"), "").unwrap();
    fs::write(lisp_root.join("org/org.el"), "").unwrap();

    let files = generated_lisp_bytecode_files(&lisp_root).unwrap();

    assert_eq!(
        files,
        vec![
            lisp_root.join("emacs-lisp/macroexp.elc"),
            lisp_root.join("org/org.elc"),
        ]
    );
}

#[test]
fn generated_leim_source_files_match_gnu_bootstrap_clean_scope() {
    let repo_root = PathBuf::from("/repo");
    let paths = PipelinePaths {
        temacs: repo_root.join("target/debug/neomacs-temacs"),
        bootstrap: repo_root.join("target/debug/bootstrap-neomacs"),
        final_bin: repo_root.join("target/debug/neomacs"),
        etc_root: repo_root.join("etc"),
        lisp_root: repo_root.join("lisp"),
        leim_root: repo_root.join("leim"),
        admin_charsets_root: repo_root.join("admin/charsets"),
        admin_grammars_root: repo_root.join("admin/grammars"),
        admin_unidata_root: repo_root.join("admin/unidata"),
        makefile_in: repo_root.join("lisp/Makefile.in"),
    };

    let files = generated_leim_source_files(&paths);
    let relative = files
        .iter()
        .map(|path| {
            path.strip_prefix(repo_root.join("lisp"))
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();

    assert!(relative.contains(&"leim/quail/CTLau-b5.el".to_string()));
    assert!(relative.contains(&"language/pinyin.el".to_string()));
    assert!(relative.contains(&"leim/leim-list.el".to_string()));
    assert_eq!(files.len(), LEIM_GENERATION_RULES.len() + 3);
}

#[test]
fn generated_custom_finder_source_files_match_gnu_autogen_scope() {
    let repo_root = PathBuf::from("/repo");
    let paths = PipelinePaths {
        temacs: repo_root.join("target/debug/neomacs-temacs"),
        bootstrap: repo_root.join("target/debug/bootstrap-neomacs"),
        final_bin: repo_root.join("target/debug/neomacs"),
        etc_root: repo_root.join("etc"),
        lisp_root: repo_root.join("lisp"),
        leim_root: repo_root.join("leim"),
        admin_charsets_root: repo_root.join("admin/charsets"),
        admin_grammars_root: repo_root.join("admin/grammars"),
        admin_unidata_root: repo_root.join("admin/unidata"),
        makefile_in: repo_root.join("lisp/Makefile.in"),
    };

    assert_eq!(
        generated_custom_finder_source_files(&paths),
        vec![
            repo_root.join("lisp/cus-load.el"),
            repo_root.join("lisp/finder-inf.el"),
        ]
    );
}

#[test]
fn custom_and_finder_dirs_follow_gnu_subdir_filters() {
    let tempdir = tempdir();
    let lisp_root = tempdir.join("lisp");
    for dir in [
        "",
        "calendar",
        "leim",
        "leim/quail",
        "obsolete",
        "term",
        "term/xterm",
    ] {
        fs::create_dir_all(lisp_root.join(dir)).unwrap();
    }

    let custom = lisp_dirs_for_custom_dependencies(&lisp_root)
        .unwrap()
        .into_iter()
        .map(|path| path.strip_prefix(&lisp_root).unwrap().to_path_buf())
        .collect::<Vec<_>>();
    assert!(custom.contains(&PathBuf::from("calendar")));
    assert!(custom.contains(&PathBuf::from("leim")));
    assert!(custom.contains(&PathBuf::from("leim/quail")));
    assert!(!custom.contains(&PathBuf::from("obsolete")));
    assert!(!custom.contains(&PathBuf::from("term")));
    assert!(custom.contains(&PathBuf::from("term/xterm")));

    let finder = lisp_dirs_for_finder_data(&lisp_root)
        .unwrap()
        .into_iter()
        .map(|path| path.strip_prefix(&lisp_root).unwrap().to_path_buf())
        .collect::<Vec<_>>();
    assert!(finder.contains(&PathBuf::from("calendar")));
    assert!(!finder.contains(&PathBuf::from("leim")));
    assert!(!finder.contains(&PathBuf::from("leim/quail")));
    assert!(!finder.contains(&PathBuf::from("obsolete")));
    assert!(!finder.contains(&PathBuf::from("term")));
    assert!(finder.contains(&PathBuf::from("term/xterm")));
}

#[test]
fn loaddefs_dirs_follow_gnu_subdirs_almost_filter() {
    let tempdir = tempdir();
    let lisp_root = tempdir.join("lisp");
    for dir in [
        "",
        "calendar",
        "obsolete",
        "obsolete/child",
        "term",
        "term/xterm",
    ] {
        fs::create_dir_all(lisp_root.join(dir)).unwrap();
    }

    let dirs = loaddefs_dirs(&lisp_root)
        .unwrap()
        .into_iter()
        .map(|path| path.strip_prefix(&lisp_root).unwrap().to_path_buf())
        .collect::<Vec<_>>();

    assert!(dirs.contains(&PathBuf::from("")));
    assert!(dirs.contains(&PathBuf::from("calendar")));
    assert!(!dirs.contains(&PathBuf::from("obsolete")));
    assert!(dirs.contains(&PathBuf::from("obsolete/child")));
    assert!(!dirs.contains(&PathBuf::from("term")));
    assert!(dirs.contains(&PathBuf::from("term/xterm")));
}

#[test]
fn subdirs_update_dirs_follow_gnu_subdirs_subdirs_filter() {
    let tempdir = tempdir();
    let lisp_root = tempdir.join("lisp");
    for dir in [
        "",
        "cedet",
        "cedet/semantic",
        "cedet-extra",
        "leim",
        "leim/quail",
        "leim-extra",
        "org",
        "org/sub",
        "term",
        "term/xterm",
    ] {
        fs::create_dir_all(lisp_root.join(dir)).unwrap();
    }

    let dirs = lisp_dirs_for_subdirs_update(&lisp_root)
        .unwrap()
        .into_iter()
        .map(|path| path.strip_prefix(&lisp_root).unwrap().to_path_buf())
        .collect::<Vec<_>>();

    assert!(dirs.contains(&PathBuf::from("")));
    assert!(dirs.contains(&PathBuf::from("org")));
    assert!(dirs.contains(&PathBuf::from("org/sub")));
    assert!(dirs.contains(&PathBuf::from("term")));
    assert!(dirs.contains(&PathBuf::from("term/xterm")));
    assert!(!dirs.contains(&PathBuf::from("cedet")));
    assert!(!dirs.contains(&PathBuf::from("cedet/semantic")));
    assert!(!dirs.contains(&PathBuf::from("cedet-extra")));
    assert!(!dirs.contains(&PathBuf::from("leim")));
    assert!(!dirs.contains(&PathBuf::from("leim/quail")));
    assert!(!dirs.contains(&PathBuf::from("leim-extra")));
}

#[test]
fn update_subdirs_file_matches_gnu_script_order_and_filters() {
    let tempdir = tempdir();
    let lisp_root = tempdir.join("lisp");
    fs::create_dir_all(&lisp_root).unwrap();
    for dir in [
        ".hidden",
        "=scratch",
        "CVS",
        "Old",
        "RCS",
        "bad.orig",
        "bad.rej",
        "calc",
        "calendar",
        "compiled.elc",
        "obsolete",
        "source.el",
        "term",
        "vc",
        "work~",
    ] {
        fs::create_dir_all(lisp_root.join(dir)).unwrap();
    }

    let change = update_subdirs_file(&lisp_root).unwrap();
    assert_eq!(change, UpdateSubdirsChange::Written);
    assert_eq!(
        fs::read_to_string(lisp_root.join("subdirs.el")).unwrap(),
        update_subdirs_contents("\"vc\" \"calendar\" \"calc\"  \"obsolete\"")
    );
    assert!(!lisp_root.join("subdirs.el~").exists());

    let change = update_subdirs_file(&lisp_root).unwrap();
    assert_eq!(change, UpdateSubdirsChange::Unchanged);
    assert!(!lisp_root.join("subdirs.el~").exists());
}

#[test]
fn update_subdirs_file_removes_stale_file_when_no_subdirs_remain() {
    let tempdir = tempdir();
    let lisp_root = tempdir.join("lisp");
    fs::create_dir_all(&lisp_root).unwrap();
    fs::create_dir_all(lisp_root.join("term")).unwrap();
    fs::write(lisp_root.join("subdirs.el"), "stale\n").unwrap();

    let change = update_subdirs_file(&lisp_root).unwrap();
    assert_eq!(change, UpdateSubdirsChange::Removed);
    assert!(!lisp_root.join("subdirs.el").exists());

    let change = update_subdirs_file(&lisp_root).unwrap();
    assert_eq!(change, UpdateSubdirsChange::Unchanged);
}

#[test]
fn compile_main_sources_follow_gnu_no_byte_compile_filter() {
    let tempdir = tempdir();
    let lisp_root = tempdir.join("lisp");
    fs::create_dir_all(lisp_root.join("sub")).unwrap();
    fs::write(lisp_root.join("a.el"), "").unwrap();
    fs::write(lisp_root.join(".hidden.el"), "").unwrap();
    fs::write(
        lisp_root.join("skip.el"),
        ";;; skip.el -*- no-byte-compile: t -*-\n",
    )
    .unwrap();
    fs::write(
        lisp_root.join("skip-existing.el"),
        ";;; skip-existing.el -*- no-byte-compile: t -*-\n",
    )
    .unwrap();
    fs::write(lisp_root.join("skip-existing.elc"), "").unwrap();
    fs::write(lisp_root.join("sub/b.el"), "").unwrap();

    let sources = compile_main_sources(&lisp_root).unwrap();

    assert_eq!(
        sources,
        vec![
            lisp_root.join("a.el"),
            lisp_root.join("skip-existing.el"),
            lisp_root.join("sub/b.el"),
        ]
    );
}

#[test]
fn compile_main_failure_summary_reports_failed_file_count() {
    assert_eq!(
        compile_main_failure_summary(&["/repo/lisp/simple.el".to_string()]),
        "compile-main failed to byte-compile 1 file"
    );
    assert_eq!(
        compile_main_failure_summary(&[
            "/repo/lisp/simple.el".to_string(),
            "/repo/lisp/calendar/calendar.el".to_string(),
        ]),
        "compile-main failed to byte-compile 2 files"
    );
}

#[test]
fn gnu_no_byte_compile_marker_matches_makefile_grep_shape() {
    assert!(gnu_no_byte_compile_marker_line(
        ";;; file.el -*- no-byte-compile: t -*-"
    ));
    assert!(gnu_no_byte_compile_marker_line(
        ";; Local Variables: no-byte-compile: t"
    ));
    assert!(gnu_no_byte_compile_marker_line(
        ";; local-no-byte-compile: t"
    ));
    assert!(!gnu_no_byte_compile_marker_line(";; ano-byte-compile: t"));
    assert!(gnu_no_byte_compile_marker_line(
        ";; ano-byte-compile: t; no-byte-compile: t"
    ));
    assert!(!gnu_no_byte_compile_marker_line(
        ";;; file.el -*- no-byte-compile: nil -*-"
    ));
    assert!(!gnu_no_byte_compile_marker_line("(setq no-byte-compile t)"));
}

#[test]
fn inject_no_byte_compile_matches_loaddefs_boot_intent() {
    let input = "\
;;; loaddefs.el --- generated -*- lexical-binding:t -*-
;; Local Variables:
;; version-control: never
;; End:
";
    let output = inject_no_byte_compile(input);
    assert!(output.contains(";; Local Variables:\n;; no-byte-compile: t\n"));
}

#[test]
fn validate_primary_loaddefs_accepts_gnu_docstring_layout() {
    let contents = format!(
        "\
;;; loaddefs.el --- generated

{}

\x0c
;;; End of scraped data
;; Local Variables:
;; End:
",
        GNU_EBROWSE_DECLARATION_AUTOLOAD
    );

    validate_primary_loaddefs_contents(&contents).unwrap();
}

#[test]
fn validate_primary_loaddefs_rejects_crlf_output_as_a_gnu_mismatch() {
    let contents = concat!(
        ";;; loaddefs.el --- generated\r\n",
        "\r\n",
        "(autoload 'ebrowse-tags-find-declaration \"ebrowse\"\r\n",
        "\"Find declaration of member at point.\" t)\r\n",
        "\r\n",
        "\x0c\r\n",
        ";;; End of scraped data\r\n",
        ";; Local Variables:\r\n",
        ";; coding: utf-8-emacs-unix\r\n",
        ";; End:\r\n",
    );

    let err = validate_primary_loaddefs_contents(contents).unwrap_err();
    assert!(
        err.to_string().contains("missing GNU end boundary"),
        "CRLF output must remain a surfaced GNU mismatch: {err}"
    );
}

#[test]
fn normalize_lisp_line_endings_rewrites_crlf() {
    let tempdir = tempdir();
    let path = tempdir.join("loaddefs.el");
    fs::write(&path, b"first\r\nsecond\r\n").unwrap();

    normalize_lisp_line_endings(&path).unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"first\nsecond\n");
}

#[test]
fn validate_primary_loaddefs_rejects_moved_docstring_layout() {
    let contents = "\
;;; loaddefs.el --- generated

(autoload 'ebrowse-tags-find-declaration \"ebrowse\" \"\\
 t)

Find declaration of member at point.\"\x0c
;;; End of scraped data
;; Local Variables:
;; End:
";

    let err = validate_primary_loaddefs_contents(contents).unwrap_err();
    assert!(
        err.to_string().contains("moved an ebrowse docstring"),
        "unexpected error: {err}"
    );
}

#[test]
fn compile_first_args_match_gnu_non_native_shape() {
    let args = compile_first_args_for_source(false, Path::new("/tmp/macroexp.el"));
    assert_eq!(
        args,
        vec![
            OsString::from("--batch"),
            OsString::from("-f"),
            OsString::from("batch-byte-compile"),
            OsString::from("/tmp/macroexp.el"),
        ]
    );
}

#[test]
fn compile_first_args_match_gnu_native_shape() {
    let args = compile_first_args_for_source(true, Path::new("/tmp/macroexp.el"));
    assert_eq!(
        args,
        vec![
            OsString::from("--batch"),
            OsString::from("-l"),
            OsString::from("comp"),
            OsString::from("-f"),
            OsString::from("batch-byte-compile"),
            OsString::from("/tmp/macroexp.el"),
        ]
    );
}

#[test]
fn compile_main_args_match_gnu_non_native_shape() {
    let args = compile_main_args_for_source(false, Path::new("/tmp/simple.el"));
    assert_eq!(
        args,
        vec![
            OsString::from("--batch"),
            OsString::from("--no-site-file"),
            OsString::from("--no-site-lisp"),
            OsString::from("--eval"),
            OsString::from("(setq load-prefer-newer t byte-compile-warnings 'all)"),
            OsString::from("--eval"),
            OsString::from("(setq org--inhibit-version-check t)"),
            OsString::from("-f"),
            OsString::from("batch-byte-compile"),
            OsString::from("/tmp/simple.el"),
        ]
    );
}

#[test]
fn compile_main_args_match_gnu_native_shape() {
    let args = compile_main_args_for_source(true, Path::new("/tmp/simple.el"));
    assert_eq!(
        args,
        vec![
            OsString::from("--batch"),
            OsString::from("--no-site-file"),
            OsString::from("--no-site-lisp"),
            OsString::from("--eval"),
            OsString::from("(setq load-prefer-newer t byte-compile-warnings 'all)"),
            OsString::from("--eval"),
            OsString::from("(setq org--inhibit-version-check t)"),
            OsString::from("-l"),
            OsString::from("comp"),
            OsString::from("-f"),
            OsString::from("batch-byte+native-compile"),
            OsString::from("/tmp/simple.el"),
        ]
    );
}

#[test]
fn preloaded_lisp_args_match_gnu_non_native_shape() {
    let args = preloaded_lisp_args_for_source(false, Path::new("/tmp/elisp-mode.el"));
    assert_eq!(
        args,
        vec![
            OsString::from("--batch"),
            OsString::from("--no-site-file"),
            OsString::from("--no-site-lisp"),
            OsString::from("--eval"),
            OsString::from("(setq load-prefer-newer t byte-compile-warnings 'all)"),
            OsString::from("--eval"),
            OsString::from("(setq org--inhibit-version-check t)"),
            OsString::from("-l"),
            OsString::from("bytecomp"),
            OsString::from("-f"),
            OsString::from("byte-compile-refresh-preloaded"),
            OsString::from("-f"),
            OsString::from("batch-byte-compile"),
            OsString::from("/tmp/elisp-mode.el"),
        ]
    );
}

#[test]
fn preloaded_lisp_args_match_gnu_native_shape() {
    let args = preloaded_lisp_args_for_source(true, Path::new("/tmp/elisp-mode.el"));
    assert_eq!(
        args,
        vec![
            OsString::from("--batch"),
            OsString::from("--no-site-file"),
            OsString::from("--no-site-lisp"),
            OsString::from("--eval"),
            OsString::from("(setq load-prefer-newer t byte-compile-warnings 'all)"),
            OsString::from("--eval"),
            OsString::from("(setq org--inhibit-version-check t)"),
            OsString::from("-l"),
            OsString::from("comp"),
            OsString::from("-f"),
            OsString::from("byte-compile-refresh-preloaded"),
            OsString::from("-f"),
            OsString::from("batch-byte+native-compile"),
            OsString::from("/tmp/elisp-mode.el"),
        ]
    );
}

#[test]
fn loaddefs_generation_args_use_gnu_emacs_batch_entrypoint() {
    let loaddefs_gen = Path::new("/repo/lisp/emacs-lisp/loaddefs-gen.el");
    let loaddefs_dirs = vec![
        PathBuf::from("/repo/lisp"),
        PathBuf::from("/repo/lisp/calendar"),
    ];
    let args = loaddefs_generation_args(loaddefs_gen, &loaddefs_dirs);
    let rendered = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(!rendered.contains(&"--eval".to_string()));
    assert!(rendered.contains(&"loaddefs-generate--emacs-batch".to_string()));
    assert_eq!(
        &rendered[rendered.len() - 2..],
        ["/repo/lisp", "/repo/lisp/calendar"]
    );
}

#[test]
fn custom_dependencies_generation_args_match_gnu_shape() {
    let dirs = vec![
        PathBuf::from("/repo/lisp"),
        PathBuf::from("/repo/lisp/calendar"),
    ];
    let args = custom_dependencies_generation_args(
        Path::new("/repo/lisp"),
        Path::new("/repo/lisp/cus-load.el"),
        &dirs,
    );

    assert_eq!(
        args,
        vec![
            OsString::from("--batch"),
            OsString::from("--no-site-file"),
            OsString::from("--no-site-lisp"),
            OsString::from("-l"),
            OsString::from("cus-dep"),
            OsString::from("--eval"),
            OsString::from(
                "(setq generated-custom-dependencies-file (unmsys--file-name \"/repo/lisp/cus-load.el\"))"
            ),
            OsString::from("-f"),
            OsString::from("custom-make-dependencies"),
            OsString::from("/repo/lisp"),
            OsString::from("/repo/lisp/calendar"),
        ]
    );
}

#[test]
fn finder_data_generation_args_match_gnu_shape() {
    let dirs = vec![
        PathBuf::from("/repo/lisp"),
        PathBuf::from("/repo/lisp/calendar"),
    ];
    let args = finder_data_generation_args(
        Path::new("/repo/lisp"),
        Path::new("/repo/lisp/finder-inf.el"),
        &dirs,
    );

    assert_eq!(
        args,
        vec![
            OsString::from("--batch"),
            OsString::from("--no-site-file"),
            OsString::from("--no-site-lisp"),
            OsString::from("-l"),
            OsString::from("finder"),
            OsString::from("--eval"),
            OsString::from(
                "(setq generated-finder-keywords-file (unmsys--file-name \"/repo/lisp/finder-inf.el\"))"
            ),
            OsString::from("-f"),
            OsString::from("finder-compile-keywords-make-dist"),
            OsString::from("/repo/lisp"),
            OsString::from("/repo/lisp/calendar"),
        ]
    );
}

#[test]
fn semantic_grammar_targets_follow_gnu_admin_grammars_makefile() {
    let outputs = SEMANTIC_GRAMMAR_TARGETS
        .iter()
        .map(|target| target.output_rel)
        .collect::<Vec<_>>();

    assert_eq!(
        outputs,
        vec![
            "cedet/semantic/bovine/c-by.el",
            "cedet/semantic/bovine/make-by.el",
            "cedet/semantic/bovine/scm-by.el",
            "cedet/semantic/grammar-wy.el",
            "cedet/semantic/wisent/javat-wy.el",
            "cedet/semantic/wisent/js-wy.el",
            "cedet/semantic/wisent/python-wy.el",
            "cedet/srecode/srt-wy.el",
        ]
    );
}

#[test]
fn semantic_grammar_args_match_gnu_wisent_shape() {
    let args = semantic_grammar_args(
        SemanticGrammarKind::Wisent,
        Path::new("/repo/lisp/cedet/srecode/srt-wy.el"),
        Path::new("/repo/admin/grammars/srecode-template.wy"),
    );

    assert_eq!(
        args,
        vec![
            OsString::from("--batch"),
            OsString::from("--no-site-file"),
            OsString::from("--no-site-lisp"),
            OsString::from("--eval"),
            OsString::from("(setq load-prefer-newer t)"),
            // cl-extra is loaded first so `cl-find-class` is defined on the
            // bootstrap neomacs (GNU relies on the fully-built emacs's autoloads).
            OsString::from("-l"),
            OsString::from("cl-extra"),
            OsString::from("-l"),
            OsString::from("semantic/wisent/grammar"),
            OsString::from("-f"),
            OsString::from("wisent-batch-make-parser"),
            OsString::from("-o"),
            OsString::from("/repo/lisp/cedet/srecode/srt-wy.el"),
            OsString::from("/repo/admin/grammars/srecode-template.wy"),
        ]
    );
}

#[test]
fn leim_generation_args_match_gnu_titdic_shape() {
    let args = leim_generation_args(
        LeimGenerationKind::TitDic,
        Path::new("/repo/lisp/leim/quail"),
        Path::new("/repo/leim/CXTERM-DIC/CCDOSPY.tit"),
        Path::new("/repo/lisp/leim/quail/CCDOSPY.el"),
    );

    assert_eq!(
        args,
        vec![
            OsString::from("--batch"),
            OsString::from("--no-site-file"),
            OsString::from("--no-site-lisp"),
            OsString::from("-l"),
            OsString::from("titdic-cnv"),
            OsString::from("-f"),
            OsString::from("batch-tit-dic-convert"),
            OsString::from("-dir"),
            OsString::from("/repo/lisp/leim/quail"),
            OsString::from("/repo/leim/CXTERM-DIC/CCDOSPY.tit"),
        ]
    );
}

#[test]
fn leim_ext_append_contents_matches_gnu_sed_filter() {
    let input = "\
plain-entry
;comment
;inc one-level
;;inc two-level
";

    assert_eq!(
        leim_ext_append_contents(input),
        "plain-entry\n; one-level\n;; two-level\n"
    );
}

#[test]
fn executable_fingerprint_patch_is_idempotent() {
    let tempdir = tempdir();
    let binary = tempdir.join("neomacs");
    let mut contents = b"prefix".to_vec();
    contents.extend_from_slice(FINGERPRINT_MAGIC_START);
    contents.extend_from_slice(FINGERPRINT_PLACEHOLDER);
    contents.extend_from_slice(FINGERPRINT_MAGIC_END);
    contents.extend_from_slice(b"suffix");
    fs::write(&binary, contents).unwrap();

    let first = executable_fingerprint(binary.as_path()).unwrap();
    patch_executable_fingerprint(&binary, &first).unwrap();
    let patched_once = fs::read(&binary).unwrap();

    let second = executable_fingerprint(binary.as_path()).unwrap();
    assert_eq!(first, second);
    patch_executable_fingerprint(&binary, &second).unwrap();
    assert_eq!(patched_once, fs::read(&binary).unwrap());
}

#[test]
fn executable_fingerprint_patches_all_records() {
    let tempdir = tempdir();
    let binary = tempdir.join("neomacs");
    let mut contents = Vec::new();
    for label in [b"one".as_slice(), b"two".as_slice()] {
        contents.extend_from_slice(label);
        contents.extend_from_slice(FINGERPRINT_MAGIC_START);
        contents.extend_from_slice(FINGERPRINT_PLACEHOLDER);
        contents.extend_from_slice(FINGERPRINT_MAGIC_END);
    }
    fs::write(&binary, contents).unwrap();

    let fingerprint = [0xA5; 32];
    patch_executable_fingerprint(&binary, &fingerprint).unwrap();
    let patched = fs::read(&binary).unwrap();

    for slot in executable_fingerprint_slots(&patched) {
        assert_eq!(&patched[slot..slot + 32], &fingerprint);
    }
}

#[test]
fn executable_role_copy_replaces_existing_file() {
    let tempdir = tempdir();
    let source = tempdir.join("neomacs");
    let destination = tempdir.join("neomacs-temacs");
    fs::write(&source, b"primary executable").unwrap();
    fs::write(&destination, b"stale role executable").unwrap();

    copy_executable_role_image(&source, &destination).unwrap();

    assert_eq!(fs::read(&destination).unwrap(), b"primary executable");
}

#[cfg(unix)]
#[test]
fn executable_role_copy_breaks_existing_hardlink() {
    let tempdir = tempdir();
    let source = tempdir.join("neomacs");
    let cargo_dep_artifact = tempdir.join("deps-neomacs-temacs");
    let destination = tempdir.join("neomacs-temacs");
    fs::write(&source, b"primary executable").unwrap();
    fs::write(&cargo_dep_artifact, b"old cargo artifact").unwrap();
    fs::hard_link(&cargo_dep_artifact, &destination).unwrap();

    copy_executable_role_image(&source, &destination).unwrap();

    assert_eq!(fs::read(&destination).unwrap(), b"primary executable");
    assert_eq!(
        fs::read(&cargo_dep_artifact).unwrap(),
        b"old cargo artifact"
    );
}

#[test]
fn executable_name_uses_platform_suffix() {
    assert_eq!(
        executable_name("neomacs"),
        format!("neomacs{}", std::env::consts::EXE_SUFFIX)
    );
}

#[test]
fn cargo_program_uses_path_lookup() {
    let cargo = cargo_program();
    assert!(cargo.is_absolute(), "{}", cargo.display());
    assert_eq!(
        cargo.file_name().unwrap(),
        executable_name("cargo").as_str()
    );
}

#[test]
fn resolve_program_on_path_returns_absolute_path_from_path() {
    let tempdir = tempdir();
    let bin = tempdir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let cargo = bin.join(executable_name("cargo"));
    fs::write(&cargo, "").unwrap();

    assert_eq!(
        resolve_program_on_path("cargo", Some(bin.as_os_str()), Path::new("/unused")).unwrap(),
        cargo
    );
}

#[cfg(windows)]
#[test]
fn resolve_program_on_path_uses_pathext_before_extensionless_files() {
    let tempdir = tempdir();
    let bin = tempdir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("gunzip"), "not a Windows executable").unwrap();
    let gunzip_exe = bin.join("gunzip.exe");
    fs::write(&gunzip_exe, "").unwrap();

    assert_eq!(
        resolve_program_on_path("gunzip", Some(bin.as_os_str()), Path::new("/unused")).unwrap(),
        gunzip_exe
    );
}

#[test]
fn read_gzip_file_decodes_charset_generation_inputs_without_external_tools() {
    let tempdir = tempdir();
    let gzip_path = tempdir.join("input.gz");
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"charset data\n").unwrap();
    fs::write(&gzip_path, encoder.finish().unwrap()).unwrap();

    assert_eq!(read_gzip_file(&gzip_path).unwrap(), b"charset data\n");
}

#[test]
fn outer_cargo_env_filter_strips_package_build_vars_only() {
    for key in [
        "CARGO",
        "CARGO_BIN_EXE_xtask",
        "CARGO_CFG_TARGET_OS",
        "CARGO_CRATE_NAME",
        "CARGO_FEATURE_DEFAULT",
        "CARGO_MANIFEST_DIR",
        "CARGO_MANIFEST_LINKS",
        "CARGO_MANIFEST_PATH",
        "CARGO_PKG_NAME",
        "CARGO_PRIMARY_PACKAGE",
        "OUT_DIR",
    ] {
        assert!(should_remove_outer_cargo_env(OsStr::new(key)), "{key}");
    }

    for key in [
        "CARGO_BUILD_JOBS",
        "CARGO_HOME",
        "CARGO_NET_OFFLINE",
        "CARGO_PROFILE_RELEASE_LTO",
        "CARGO_TARGET_DIR",
        "CARGO_TERM_COLOR",
        "RUSTFLAGS",
    ] {
        assert!(!should_remove_outer_cargo_env(OsStr::new(key)), "{key}");
    }
}

#[test]
fn build_time_emacs_env_filter_covers_lisp_and_native_load_paths() {
    assert_eq!(
        BUILD_TIME_EMACS_ENV_VARS,
        ["EMACSLOADPATH", "EMACSNATIVELOADPATH"]
    );

    let mut command = Command::new("neomacs");
    for key in BUILD_TIME_EMACS_ENV_VARS {
        command.env(key, "/user/profile");
    }
    remove_build_time_emacs_env(&mut command);

    for key in BUILD_TIME_EMACS_ENV_VARS {
        assert!(
            command
                .get_envs()
                .any(|(candidate, value)| candidate == key && value.is_none()),
            "{key} should be explicitly removed from build subprocesses"
        );
    }
}

#[test]
fn unidata_generated_lisp_file_names_match_gnu_makefile_shape() {
    let contents = r#"
(defconst unidata-file-alist
  '(
    ("uni-name.el"
     name
     1)
    ("uni-category.el"
     category
     2)
    ("not-generated.el"
     ignored)
    ("uni-special-uppercase.el"
     special)))
"#;

    assert_eq!(
        unidata_generated_lisp_file_names_from_str(contents),
        vec![
            "uni-category.el".to_string(),
            "uni-name.el".to_string(),
            "uni-special-uppercase.el".to_string(),
        ]
    );
}

#[test]
fn unidata_generator_args_use_gnu_batch_shape() {
    let args = unidata_generator_args(
        &OsString::from("/repo/admin/unidata"),
        &OsString::from("/repo/admin/unidata/unidata-gen.el"),
        "unidata-gen-file",
    );

    assert_eq!(
        args,
        vec![
            OsString::from("--batch"),
            OsString::from("--no-site-file"),
            OsString::from("--no-site-lisp"),
            OsString::from("-L"),
            OsString::from("/repo/admin/unidata"),
            OsString::from("-l"),
            OsString::from("/repo/admin/unidata/unidata-gen.el"),
            OsString::from("-f"),
            OsString::from("unidata-gen-file"),
        ]
    );
}

#[test]
fn generated_unidata_source_files_match_gnu_gen_clean_shape() {
    let tempdir = tempdir();
    let repo = tempdir.join("repo");
    let lisp = repo.join("lisp");
    let admin = repo.join("admin/unidata");
    fs::create_dir_all(&admin).unwrap();
    fs::write(
        admin.join("unidata-gen.el"),
        r#"
(defconst unidata-file-alist
  '(
    ("uni-name.el"
     name)
    ("uni-category.el"
     category)))
"#,
    )
    .unwrap();
    let options = FreshBuildOptions {
        repo_root: repo.clone(),
        runtime_root: repo.clone(),
        bin_dir: repo.join("target/debug"),
        profile: BuildProfile::Debug,
        dry_run: false,
        native_comp: false,
        skip_build: false,
        no_byte_compile: false,
        features: Vec::new(),
        aot_preload: false,
    };
    let paths = PipelinePaths {
        lisp_root: lisp.clone(),
        admin_unidata_root: admin.clone(),
        ..pipeline_paths(&options)
    };

    let files = generated_unidata_source_files(&paths).unwrap();

    assert!(files.contains(&lisp.join("international/charscript.el")));
    assert!(files.contains(&lisp.join("international/emoji-zwj.el")));
    assert!(files.contains(&lisp.join("international/charprop.el")));
    assert!(files.contains(&lisp.join("international/uni-name.el")));
    assert!(files.contains(&lisp.join("international/uni-category.el")));
    assert!(files.contains(&lisp.join("international/emoji-labels.el")));
    assert!(files.contains(&lisp.join("international/idna-mapping.el")));
    assert!(files.contains(&lisp.join("international/uni-confusable.el")));
    assert!(files.contains(&lisp.join("international/uni-scripts.el")));
}

#[test]
fn generated_unidata_admin_files_match_gnu_clean_shape() {
    let options = parse_options(&["--release"]);
    let paths = pipeline_paths(&options);

    assert_eq!(
        generated_unidata_admin_files(&paths),
        vec![
            PathBuf::from("/repo/admin/unidata/unidata.txt"),
            PathBuf::from("/repo/admin/unidata/unidata-gen.elc"),
            PathBuf::from("/repo/admin/unidata/uvs.elc"),
        ]
    );
}

fn tempdir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives under the repository root")
        .join("tmp")
        .join(format!(
            "xtask-tests-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_elc_newer_than(source: &Path, older: &Path) {
    let older_mtime = fs::metadata(older).unwrap().modified().unwrap();
    let elc = source.with_extension("elc");
    for attempt in 0..200 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        fs::write(&elc, format!("elc {attempt}\n")).unwrap();
        let elc_mtime = fs::metadata(&elc).unwrap().modified().unwrap();
        if elc_mtime > older_mtime {
            return;
        }
    }
    panic!(
        "{} did not become newer than {}",
        elc.display(),
        older.display()
    );
}
