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
    let dependencies = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/nix/dependencies.nix"
    ));
    let dev_shell = include_str!(concat!(env!("CARGO_WORKSPACE_DIR"), "/nix/dev-shell.nix"));

    assert!(
        dependencies.contains("stdenv.cc.cc.lib"),
        "Neomacs links libstdc++, so the Nix runtime closure must own it"
    );
    assert!(
        dev_shell.contains("lib.remove pkgs.ncurses dependencies.developmentBuildInputs"),
        "the development LD_LIBRARY_PATH must derive from the packaged runtime closure"
    );
}

#[test]
fn nix_ci_automates_evaluation_and_runs_the_public_package_contracts() {
    let workflow = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/nix-smoke.yml"
    ));

    assert!(workflow.contains("pull_request:"));
    assert!(workflow.contains("uses: ./.github/actions/setup-nix"));
    assert!(workflow.contains("rust-toolchain.toml"));
    assert!(workflow.contains(".github/actions/setup-nix/**"));
    assert!(workflow.contains("nix flake check --all-systems --no-build"));
    assert!(workflow.contains("allow-import-from-derivation false"));
    assert!(workflow.contains(".#checks.x86_64-linux.installed-package-contract"));
    assert!(workflow.contains(".#checks.x86_64-linux.minimal-installed-package-contract"));
    assert!(workflow.contains(".#checks.x86_64-linux.home-manager-contract"));
    assert!(
        !workflow.contains("./result/bin/neomacs --batch --quick"),
        "the workflow must not bypass site-start like issue #60's workaround"
    );
}

#[test]
#[cfg(unix)]
fn linux_desktop_assets_install_the_runtime_window_identity() {
    let repo_root = repository_root();
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
        fs::read_to_string(repo_root.join("crates/neomacs-display-runtime/assets/neomacs.desktop"))
            .unwrap();
    assert_eq!(installed_desktop, canonical_desktop);
    assert!(installed_desktop.contains("\nExec=neomacs %F\n"));
    assert!(installed_desktop.contains("\nIcon=neomacs\n"));

    let installed_icon =
        fs::read(fixture.join("share/icons/hicolor/scalable/apps/neomacs.svg")).unwrap();
    let runtime_icon =
        fs::read(repo_root.join("crates/neomacs-display-runtime/assets/window-icon.svg")).unwrap();
    assert_eq!(
        installed_icon, runtime_icon,
        "packaging must install the exact SVG embedded by the runtime"
    );
}

#[test]
fn every_linux_package_uses_the_canonical_desktop_asset_installer() {
    for (name, script) in [
        (
            "tar",
            include_str!(concat!(
                env!("CARGO_WORKSPACE_DIR"),
                "/scripts/package-release.sh"
            )),
        ),
        (
            "Debian",
            include_str!(concat!(
                env!("CARGO_WORKSPACE_DIR"),
                "/scripts/package-deb.sh"
            )),
        ),
        (
            "AppImage",
            include_str!(concat!(
                env!("CARGO_WORKSPACE_DIR"),
                "/scripts/package-appimage.sh"
            )),
        ),
        (
            "RPM",
            include_str!(concat!(
                env!("CARGO_WORKSPACE_DIR"),
                "/scripts/package-rpm.sh"
            )),
        ),
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
fn linux_release_links_gstreamer_without_a_private_adapter() {
    let release = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/scripts/package-release.sh"
    ));
    let audit = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/scripts/test-linux-release-artifacts.sh"
    ));
    let rpm = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/scripts/package-rpm.sh"
    ));
    let deb = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/scripts/package-deb.sh"
    ));
    let ci = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/ci.yml"
    ));
    let video_manifest = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/crates/neomacs-video/Cargo.toml"
    ));

    assert!(!release.contains("libneomacs_video_gstreamer.so"));
    assert!(audit.contains("release contains obsolete private GStreamer adapter"));
    assert!(!rpm.contains("__requires_exclude"));
    assert!(!rpm.contains("^libgst.*[.]so[.].*$"));
    assert!(deb.contains("dpkg-shlibdeps -O \"${shlib_args[@]}\""));
    assert!(audit.contains("full executable does not link GStreamer"));
    assert!(ci.contains("minimal executable unexpectedly links GStreamer"));
    assert!(video_manifest.contains("features = [\"v1_20\"]"));
    assert!(!video_manifest.contains("features = [\"v1_24\"]"));
}

#[test]
fn linux_release_publishes_verified_full_and_minimal_products() {
    let release = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/scripts/package-release.sh"
    ));
    let appimage = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/scripts/package-appimage.sh"
    ));
    let audit = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/scripts/test-linux-release-artifacts.sh"
    ));
    let workflow = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/release.yml"
    ));

    assert!(release.contains("--minimal"));
    assert!(release.contains("minimal executable unexpectedly links GStreamer"));
    assert!(release.contains("full executable does not link GStreamer"));
    assert!(appimage.contains("neomacs-minimal"));
    assert!(audit.contains("minimal-tar"));
    assert!(workflow.contains("fresh-build --release --minimal"));
    assert!(workflow.contains("package-release.sh --minimal"));
}

#[test]
#[cfg(unix)]
fn linux_ci_setup_profiles_expose_capabilities_and_reject_unknown_profiles() {
    let repo_root = repository_root();
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
    assert!(build.lines().any(|package| package == "libncurses-dev"));
    assert!(
        build
            .lines()
            .any(|package| package == "libgstreamer1.0-dev")
    );
    assert!(!build.lines().any(|package| package == "emacs-nox"));

    let no_gstreamer = packages("build-no-gstreamer");
    assert!(
        no_gstreamer
            .lines()
            .any(|package| package == "liblcms2-dev")
    );
    assert!(
        !no_gstreamer
            .lines()
            .any(|package| package.contains("gstreamer"))
    );

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
    let workspace_manifest = include_str!(concat!(env!("CARGO_WORKSPACE_DIR"), "/Cargo.toml"));
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

    let crate_manifest = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/crates/neovm-core/Cargo.toml"
    ));
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
fn every_workspace_package_has_one_predictable_home_under_crates() {
    let repo_root = repository_root();
    let workspace_manifest_text =
        fs::read_to_string(repo_root.join("Cargo.toml")).expect("read workspace manifest");
    let workspace_manifest: toml::Value =
        toml::from_str(&workspace_manifest_text).expect("parse workspace manifest");
    let members = workspace_manifest["workspace"]["members"]
        .as_array()
        .expect("workspace members must be an array");

    assert!(!members.is_empty(), "workspace must contain Cargo packages");
    let declared_members: BTreeSet<PathBuf> = members
        .iter()
        .map(|member| PathBuf::from(member.as_str().expect("workspace member must be a string")))
        .collect();
    let discovered_members: BTreeSet<PathBuf> = fs::read_dir(repo_root.join("crates"))
        .expect("read crates directory")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().join("Cargo.toml").is_file())
        .map(|entry| PathBuf::from("crates").join(entry.file_name()))
        .collect();
    assert_eq!(
        declared_members, discovered_members,
        "workspace.members must exactly match the direct Cargo packages under crates/"
    );

    for relative in &declared_members {
        assert_eq!(
            relative.components().next(),
            Some(std::path::Component::Normal(OsStr::new("crates"))),
            "workspace package {} must live under crates/",
            relative.display()
        );
        assert_eq!(
            relative.components().count(),
            2,
            "workspace packages use the flat crates/<package> layout: {}",
            relative.display()
        );

        let package_manifest_text = fs::read_to_string(repo_root.join(relative).join("Cargo.toml"))
            .unwrap_or_else(|error| panic!("read {} manifest: {error}", relative.display()));
        let package_manifest: toml::Value = toml::from_str(&package_manifest_text)
            .unwrap_or_else(|error| panic!("parse {} manifest: {error}", relative.display()));
        let package_name = package_manifest["package"]["name"]
            .as_str()
            .expect("workspace member must declare package.name");
        assert_eq!(
            relative.file_name().and_then(OsStr::to_str),
            Some(package_name),
            "crate directory and Cargo package names must agree"
        );
    }
}

#[test]
fn dependabot_groups_cranelift_release_train() {
    let config = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/dependabot.yml"
    ));
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

    let repo_root = repository_root();
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
    let workflow = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/ci.yml"
    ));
    let job = github_workflow_job(workflow, "doom-install-compatibility");

    assert!(job.contains("needs: neomacs-test-runtime"));
    assert!(job.contains("- *download_test_runtime"));
    assert!(job.contains("- *unpack_test_runtime"));
    assert!(job.contains("NEOMACS_BIN: ${{ github.workspace }}/target/release/neomacs"));
    assert!(job.contains("run: ./scripts/test-doom-install.sh"));
}

#[test]
fn ci_lints_every_github_actions_workflow() {
    let workflow = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/ci.yml"
    ));
    let job = github_workflow_job(workflow, "workflow-lint");

    assert!(job.contains("github.event_name != 'schedule'"));
    assert!(job.contains("github.com/rhysd/actionlint/cmd/actionlint@v1.7.12"));
    assert!(job.contains(".github/workflows/*.yml"));
}

#[test]
fn rust_ci_setup_uses_the_workspace_toolchain_and_owns_test_tooling() {
    let action = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/actions/setup-rust/action.yml"
    ));

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
        include_str!(concat!(
            env!("CARGO_WORKSPACE_DIR"),
            "/.github/workflows/docker-release.yml"
        )),
        include_str!(concat!(
            env!("CARGO_WORKSPACE_DIR"),
            "/.github/workflows/nextest-shards.yml"
        )),
        include_str!(concat!(
            env!("CARGO_WORKSPACE_DIR"),
            "/.github/workflows/ci.yml"
        )),
        include_str!(concat!(
            env!("CARGO_WORKSPACE_DIR"),
            "/.github/workflows/codeql.yml"
        )),
        include_str!(concat!(
            env!("CARGO_WORKSPACE_DIR"),
            "/.github/workflows/linux.yml"
        )),
        include_str!(concat!(
            env!("CARGO_WORKSPACE_DIR"),
            "/.github/workflows/nix-smoke.yml"
        )),
        include_str!(concat!(
            env!("CARGO_WORKSPACE_DIR"),
            "/.github/workflows/release.yml"
        )),
        include_str!(concat!(
            env!("CARGO_WORKSPACE_DIR"),
            "/.github/workflows/tmp_mac_test.yml"
        )),
        include_str!(concat!(
            env!("CARGO_WORKSPACE_DIR"),
            "/.github/workflows/window-oracle-nightly.yml"
        )),
        include_str!(concat!(
            env!("CARGO_WORKSPACE_DIR"),
            "/.github/workflows/windows-installer.yml"
        )),
        include_str!(concat!(
            env!("CARGO_WORKSPACE_DIR"),
            "/.github/actions/setup-rust/action.yml"
        )),
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

    let dependabot = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/dependabot.yml"
    ));
    assert!(dependabot.contains("package-ecosystem: github-actions"));
    assert!(dependabot.contains("directory: /"));
}

#[test]
fn docker_release_publishes_one_verified_image_to_docker_hub_and_ghcr() {
    let workflow = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/docker-release.yml"
    ));
    let manifest_job = github_workflow_job(workflow, "publish-manifest");
    let release_workflow = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/release.yml"
    ));
    let release_job = github_workflow_job(release_workflow, "publish-docker");
    let docker_docs = include_str!(concat!(env!("CARGO_WORKSPACE_DIR"), "/docs/docker.md"));

    assert!(workflow.contains("packages: write"));
    assert!(workflow.contains("GHCR_IMAGE: ghcr.io/${{ github.repository }}"));
    assert!(manifest_job.contains("name: container-release"));
    assert!(
        manifest_job
            .contains("url: https://github.com/${{ github.repository }}/pkgs/container/neomacs")
    );
    assert!(manifest_job.contains("registry: ghcr.io"));
    assert!(manifest_job.contains("password: ${{ secrets.GITHUB_TOKEN }}"));
    assert!(manifest_job.contains("ghcr_exact_ref=\"$GHCR_IMAGE:$RELEASE_VERSION\""));
    assert!(manifest_job.contains("docker buildx imagetools create"));
    assert!(manifest_job.contains("\"$dockerhub_exact_ref\""));
    assert!(manifest_job.contains("docker logout ghcr.io"));
    assert!(release_job.contains("packages: write"));
    assert!(docker_docs.contains("Registry pushes alone do not create GitHub Deployments"));
    assert!(docker_docs.contains("anonymous registry read"));
}

#[test]
fn ci_uses_one_typed_sharded_nextest_workflow_for_core_oracle_and_tui() {
    let reusable = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/nextest-shards.yml"
    ));
    assert!(reusable.contains("workflow_call:"));
    assert!(reusable.contains("suite:"));
    assert!(reusable.contains("core|oracle|tui"));
    assert!(reusable.contains("package(neovm-core)"));
    assert!(reusable.contains("package(neovm-oracle-tests)"));
    assert!(reusable.contains("package(neomacs-tui-tests)"));
    assert!(reusable.contains("NEOMACS_TUI_NEOMACS_BIN"));
    assert!(reusable.contains("--partition slice:${{ matrix.partition }}/20"));
    assert_eq!(
        reusable.matches("case \"$SHARD_SUITE\" in").count(),
        1,
        "the closed suite selector must be decoded exactly once"
    );

    let workflow = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/ci.yml"
    ));
    let core = github_workflow_job(workflow, "neovm-core-tests");
    assert!(core.contains("needs: [neomacs-test-runtime, neomacs-workspace-test-archive]"));
    assert!(core.contains("uses: ./.github/workflows/nextest-shards.yml"));
    assert!(core.contains("suite: core"));

    let oracle = github_workflow_job(workflow, "neovm-oracle-tests");
    assert!(oracle.contains("if: github.event_name != 'schedule'"));
    assert!(oracle.contains("needs: [neomacs-test-runtime, neomacs-workspace-test-archive]"));
    assert!(oracle.contains("uses: ./.github/workflows/nextest-shards.yml"));
    assert!(oracle.contains("suite: oracle"));
    let tui = github_workflow_job(workflow, "neomacs-tui-tests");
    assert!(tui.contains("needs: [neomacs-test-runtime, neomacs-workspace-test-archive]"));
    assert!(tui.contains("uses: ./.github/workflows/nextest-shards.yml"));
    assert!(tui.contains("suite: tui"));
}

#[test]
fn ci_builds_shared_test_artifacts_on_github_hosted_runners() {
    let workflow = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/ci.yml"
    ));

    for job_name in ["neomacs-workspace-test-archive", "neomacs-test-runtime"] {
        let job = github_workflow_job(workflow, job_name);
        assert!(
            job.contains("runs-on: ubuntu-24.04"),
            "{job_name} must use a GitHub-hosted Ubuntu runner"
        );
        assert!(
            !job.contains("cache: ${{ github.event_name == 'pull_request' }}"),
            "{job_name} must not condition cache behavior on the event"
        );
    }

    let runtime = github_workflow_job(workflow, "neomacs-test-runtime");
    assert!(runtime.contains("name: Packaged Neomacs Runtime (linux x86_64)"));

    let archive = github_workflow_job(workflow, "neomacs-workspace-test-archive");
    assert!(archive.contains("CARGO_BUILD_JOBS: \"1\""));
    assert!(!workflow.contains("neovm-oracle-tests-self-hosted"));
}

#[test]
fn ci_runs_offline_melpa_parity_from_shared_artifacts() {
    let workflow = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/ci.yml"
    ));
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
    assert!(job.contains(
        "filter: binary_id(=neomacs-melpa-test-support)|(binary_id(=neomacs-melpa-tests)-test(~gui_parity_tests::))"
    ));
    assert!(job.contains("filter: binary_id(=neomacs-melpa-tests::melpa_tui)"));
    assert!(job.contains("filter: binary_id(=neomacs-melpa-tests)&test(~gui_parity_tests::)"));
    assert!(!job.contains("filter: binary_id(neomacs-melpa-tests) and"));
    assert!(job.contains("-E \"$NEXTEST_FILTER\""));
    assert!(!job.contains("LIBTEST_ARGS"));
    assert!(!job.contains("--skip tui_parity_tests::"));
    assert!(job.contains("--success-output immediate"));
}

#[test]
fn ci_executes_display_stack_and_real_gui_tests_from_shared_artifacts() {
    let workflow = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/ci.yml"
    ));

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
    let workflow = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/ci.yml"
    ));
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
    let nextest = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.config/nextest.toml"
    ));
    assert!(nextest.contains("filter = 'package(neomacs-melpa-tests)'"));
    assert!(nextest.contains("threads-required = \"num-test-threads\""));
}

#[test]
fn windows_installer_removes_the_legacy_path_rewrite_implementation() {
    let installer = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/assets/windows-installer.nsi"
    ));

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
    let installer = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/assets/windows-installer.nsi"
    ));

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
    let installer = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/assets/windows-installer.nsi"
    ));

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
    let installer = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/assets/windows-installer.nsi"
    ));

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
    let installer = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/assets/windows-installer.nsi"
    ));

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
    let installer = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/assets/windows-installer.nsi"
    ));

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
    let repo_root = repository_root();
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
fn windows_releases_ship_the_gnu_compatible_shell_proxy() {
    let neomacs_manifest = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/crates/neomacs/Cargo.toml"
    ));
    let release_script = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/scripts/package-release.sh"
    ));
    let release_workflow = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.github/workflows/release.yml"
    ));
    let installed_contract = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/scripts/test-windows-installer.ps1"
    ));

    assert!(
        neomacs_manifest.contains("name = \"cmdproxy\""),
        "the Windows shell adapter must be a Cargo-built executable"
    );
    assert!(
        release_script.contains("install_binary_if_present \"cmdproxy\""),
        "the GNU-shaped release tree must install cmdproxy in its private archlib"
    );
    assert!(
        release_workflow.contains("cp target/release/cmdproxy.exe \"$STAGING/\""),
        "the portable Windows zip must include cmdproxy beside neomacs.exe"
    );
    assert!(
        installed_contract.contains("Remove-Item Env:SHELL")
            && installed_contract.contains("shell-command-to-string \"whoami\"")
            && installed_contract.contains("cmdproxy\\.exe"),
        "the installed Windows contract must exercise M-! without SHELL"
    );
}

#[test]
#[cfg(unix)]
fn windows_gstreamer_packager_accepts_official_pango_runtime_shape() {
    let repo_root = repository_root();
    let fixture = tempdir();
    let gst_root = fixture.join("gstreamer");
    let gst_bin = gst_root.join("bin");
    let package_root = fixture.join("package");
    fs::create_dir_all(&gst_bin).unwrap();
    fs::create_dir_all(&package_root).unwrap();

    // This is the Pango runtime shape shipped by GStreamer 1.28.6's official
    // Windows MSVC installer.  Windows uses the native Pangowin32 backend;
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
        .env("GSTREAMER_ROOT", &gst_root)
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
fn product_variant_defaults_to_full_and_can_be_minimal() {
    assert_eq!(
        parse_options(&["--release"]).product_variant,
        ProductVariant::Full
    );
    assert_eq!(
        parse_options(&["--release", "--minimal"]).product_variant,
        ProductVariant::Minimal
    );
}

#[test]
fn minimal_variant_rejects_qualified_or_unqualified_production_capabilities() {
    for feature in [
        "video",
        "neomacs/video",
        "neomacs-display-runtime/video",
        "neomacs-renderer-wgpu/video",
    ] {
        let error = FreshBuildOptions::parse(
            PathBuf::from("/repo"),
            ["--release", "--minimal", "--features", feature]
                .into_iter()
                .map(OsString::from),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("minimal"));
        assert!(error.contains("video"));
    }
}

#[test]
fn minimal_variant_can_bootstrap_a_skipped_build_only_under_minimal_identity() {
    let options = FreshBuildOptions::parse(
        PathBuf::from("/repo"),
        ["--release", "--minimal", "--skip-build"]
            .into_iter()
            .map(OsString::from),
    )
    .unwrap();

    assert_eq!(options.product_variant, ProductVariant::Minimal);
    assert!(options.skip_build);
}

#[test]
#[cfg(target_os = "linux")]
fn linux_production_capabilities_come_from_typed_workspace_metadata() {
    let capabilities = ProductionCapabilities::for_host().unwrap();

    assert_eq!(capabilities.cargo_features(), &[CargoCapability::Video]);
    assert_eq!(
        capabilities.video_backend(),
        ProductionVideoBackend::LinkedGstreamer
    );
}

#[test]
fn parse_aot_preload_composes_with_dry_run() {
    let options = parse_options(&["--release", "--aot-preload", "--dry-run"]);
    assert!(options.aot_preload);
    assert!(options.dry_run);
}

#[test]
fn low_memory_build_owns_a_single_cargo_job_budget() {
    let options = parse_options(&["--release", "--low-memory"]);

    assert_eq!(
        options.cargo_jobs,
        CargoJobBudget::Explicit(CargoJobCount::new(1).unwrap())
    );
    assert!(
        initial_cargo_build_args(&options)
            .windows(2)
            .any(|args| args == [OsString::from("--jobs"), OsString::from("1")])
    );
}

#[test]
fn explicit_zero_cargo_jobs_is_rejected() {
    let error = FreshBuildOptions::parse(
        PathBuf::from("/repo"),
        ["--release", "--jobs", "0"].into_iter().map(OsString::from),
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("positive"), "unexpected error: {error}");
}

#[test]
#[cfg(target_os = "linux")]
fn initial_cargo_build_enables_video_by_default_on_linux() {
    let options = parse_options(&["--release"]);
    let args = initial_cargo_build_args(&options);

    assert_eq!(
        args,
        vec![
            OsString::from("build"),
            OsString::from("--verbose"),
            OsString::from("-p"),
            OsString::from("neomacs"),
            OsString::from("--features"),
            OsString::from("video"),
            OsString::from("--profile"),
            OsString::from("release"),
        ]
    );
}

#[test]
#[cfg(target_os = "linux")]
fn minimal_build_omits_production_video_at_compile_time() {
    let options = parse_options(&["--release", "--minimal"]);

    assert_eq!(
        initial_cargo_build_args(&options),
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
fn cargo_build_environment_carries_the_exact_selected_profile() {
    let options = parse_options(&["--profile", "release-pgo-profiling"]);
    let base = [(
        OsString::from("RUSTFLAGS"),
        OsString::from("-Cprofile-use=profile.profdata"),
    )];

    let envs = cargo_build_envs(&options, &base);

    assert_eq!(envs[0], base[0]);
    assert_eq!(
        envs[1],
        (
            OsString::from("NEOMACS_BUILD_PROFILE"),
            OsString::from("release-pgo-profiling"),
        )
    );
}

#[test]
fn initial_cargo_build_passes_webview_when_requested() {
    let options = parse_options(&["--features", "webview", "--release"]);
    let args = initial_cargo_build_args(&options);

    assert_eq!(
        args,
        vec![
            OsString::from("build"),
            OsString::from("--verbose"),
            OsString::from("-p"),
            OsString::from("neomacs"),
            OsString::from("--features"),
            OsString::from(if cfg!(target_os = "linux") {
                "video,webview"
            } else {
                "webview"
            }),
            OsString::from("--profile"),
            OsString::from("release"),
        ]
    );
}

/// macOS production builds ship the native `WKWebView` inline browser, so the
/// darwin capability row in the workspace manifest requests `webview` and the
/// stage-1 build carries it without anyone passing `--features`.  This is what
/// `xwidget-internal` advertises to Lisp on macOS; a darwin build without the
/// feature would provide the symbol and have no backend behind it.
#[test]
#[cfg(target_os = "macos")]
fn initial_cargo_build_passes_webview_on_darwin() {
    let options = parse_options(&["--release"]);
    let args = initial_cargo_build_args(&options);

    assert_eq!(
        args,
        vec![
            OsString::from("build"),
            OsString::from("--verbose"),
            OsString::from("-p"),
            OsString::from("neomacs"),
            OsString::from("--features"),
            OsString::from("webview"),
            OsString::from("--profile"),
            OsString::from("release"),
        ]
    );
}

#[test]
#[cfg(target_os = "windows")]
fn initial_cargo_build_passes_no_features_on_windows() {
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
    let repo_root = repository_root();
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
    let repo_root = repository_root();
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
    use compile_main_rule::gnu_no_byte_compile_marker_line;

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
    // Ledger 207, the two places this used to be looser than GNU's regexp.
    // `^;` `.*` `[^a-zA-Z]` means the marker cannot begin at index 1: `.*`
    // starts after the anchored `;`, so something must stand between them.
    assert!(!gnu_no_byte_compile_marker_line(";no-byte-compile: t"));
    assert!(gnu_no_byte_compile_marker_line(";;no-byte-compile: t"));
    // `: *t' is spaces, not whitespace -- a tab does not satisfy GNU's grep.
    assert!(!gnu_no_byte_compile_marker_line(";; no-byte-compile:\tt"));
    assert!(gnu_no_byte_compile_marker_line(";; no-byte-compile:   t"));
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
        production_capabilities: ProductionCapabilities::for_host().unwrap(),
        cargo_jobs: CargoJobBudget::Inherit,
        dry_run: false,
        native_comp: false,
        skip_build: false,
        product_variant: ProductVariant::Full,
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

// ---------------------------------------------------------------------------
// Ledger 210: the motion-parity harness must publish the frame it swept.
//
// `scripts/motion-parity-audit.el' asks 3312 questions about windows of a
// particular width, and the answers change with the terminal: the same tree
// answers COLD 130 / WARM 352 at 160 columns and COLD 160 / WARM 444 at 80.
// Ledger 205 published the first pair and ledger 209 the second, and the
// difference was read as a 30-cold / 92-warm motion regression that never
// existed.  The comparator used to SKIP the `CONFIG' lines, so neither its
// output nor its exit status could tell the two sweeps apart.
//
// The rule the fix installs is one sentence: a divergence count taken across
// two geometries is not a parity number, whatever made them differ.  So the two
// files must agree about the frame AND about every window they describe, or the
// comparison is refused -- with the disagreeing rows printed, so a real
// window-geometry divergence stays visible instead of being swallowed.
// ---------------------------------------------------------------------------

/// One `scripts/motion-parity-audit.el` output holding a single probe.
fn motion_parity_fixture(frame: (u32, u32), width: u32, height: u32, value: &str) -> String {
    format!(
        "GEOMETRY frame-width={} frame-height={} probes=1\n\
         CONFIG full-wrap width={width} height={height} tl=nil ww=nil tpww=50 vlm=nil\n\
         full-wrap|1|vm0|{value}\n",
        frame.0, frame.1
    )
}

fn run_motion_parity_compare(
    repo_root: &Path,
    gnu: &Path,
    neo: &Path,
    flags: &[&str],
) -> std::process::Output {
    let mut command = Command::new("python3");
    command
        .arg(repo_root.join("scripts/motion-parity-compare.py"))
        .arg(gnu)
        .arg(neo);
    for flag in flags {
        command.arg(flag);
    }
    command.output().expect("run motion-parity-compare.py")
}

fn motion_parity_repo_root() -> PathBuf {
    repository_root()
}

#[test]
#[cfg(unix)]
fn motion_parity_compare_refuses_two_sweeps_of_different_frames() {
    let fixture = tempdir();
    let wide = fixture.join("gnu-160.txt");
    let narrow = fixture.join("neo-80.txt");
    fs::write(&wide, motion_parity_fixture((160, 49), 160, 47, "(0 1)")).unwrap();
    fs::write(&narrow, motion_parity_fixture((80, 23), 80, 21, "(0 1)")).unwrap();

    let output = run_motion_parity_compare(&motion_parity_repo_root(), &wide, &narrow, &[]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "a count taken across two frames is not a parity number, so the \
         comparator must refuse it:\n{combined}"
    );
    assert!(
        combined.contains("frame 160x49") && combined.contains("frame 80x23"),
        "the refusal must name both frames:\n{combined}"
    );
}

#[test]
#[cfg(unix)]
fn motion_parity_compare_allows_a_geometry_mismatch_only_when_asked() {
    let fixture = tempdir();
    let wide = fixture.join("gnu-160.txt");
    let narrow = fixture.join("neo-80.txt");
    fs::write(&wide, motion_parity_fixture((160, 49), 160, 47, "(0 1)")).unwrap();
    fs::write(&narrow, motion_parity_fixture((80, 23), 80, 21, "(0 1)")).unwrap();

    let output = run_motion_parity_compare(
        &motion_parity_repo_root(),
        &wide,
        &narrow,
        &["--allow-geometry-mismatch"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "the override must let a deliberate geometry study proceed:\n{stdout}"
    );
    assert!(
        stdout.contains("GEOMETRY MISMATCH"),
        "and the headline must keep saying the count is suspect:\n{stdout}"
    );
}

#[test]
#[cfg(unix)]
fn motion_parity_compare_headline_carries_the_frame_it_measured() {
    let fixture = tempdir();
    let gnu = fixture.join("gnu.txt");
    let neo = fixture.join("neo.txt");
    fs::write(&gnu, motion_parity_fixture((80, 23), 80, 21, "(0 1)")).unwrap();
    fs::write(&neo, motion_parity_fixture((80, 23), 80, 21, "(0 8)")).unwrap();

    let output = run_motion_parity_compare(&motion_parity_repo_root(), &gnu, &neo, &[]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "one frame, one sweep, one comparison:\n{stdout}"
    );
    let headline = stdout.lines().next().unwrap_or_default();
    assert!(
        headline.contains("divergent=1"),
        "the probe still has to be counted:\n{stdout}"
    );
    assert!(
        headline.contains("frame 80x23"),
        "a pasted count must carry the frame it was measured in:\n{stdout}"
    );
}

#[test]
#[cfg(unix)]
fn motion_parity_compare_refuses_when_the_two_editors_describe_different_windows() {
    let fixture = tempdir();
    let gnu = fixture.join("gnu.txt");
    let neo = fixture.join("neo.txt");
    // Same frame -- the same question -- but the two editors answer it with
    // different window heights.  A count over windows of different heights is
    // still not a parity number, so it is refused; the row is printed so a real
    // window-geometry divergence stays visible instead of being swallowed.
    fs::write(&gnu, motion_parity_fixture((80, 23), 80, 20, "(0 1)")).unwrap();
    fs::write(&neo, motion_parity_fixture((80, 23), 80, 21, "(0 1)")).unwrap();

    let output = run_motion_parity_compare(&motion_parity_repo_root(), &gnu, &neo, &[]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "windows of different heights are different questions:\n{combined}"
    );
    assert!(
        combined.contains("!! full-wrap"),
        "and the disagreeing row must be shown, marked the way ledger 201's \
         comparator marks its own:\n{combined}"
    );
    assert!(
        combined.contains("height=20") && combined.contains("height=21"),
        "with both answers named:\n{combined}"
    );
}

#[test]
#[cfg(unix)]
fn motion_parity_pty_driver_reports_the_editor_exit_status() {
    // Ledger 210: this driver used to end in an unconditional `sys.exit(0)',
    // so an editor that crashed, was missing, or died on a signal was reported
    // as a successful sweep.  A driver that cannot fail is a false-green
    // generator, and the sweep runner's failure detection sits on top of it.
    let driver = motion_parity_repo_root().join("scripts/motion-parity-pty.py");
    let cases: [(&[&str], i32); 4] = [
        (&["/bin/sh", "-c", "exit 0"], 0),
        (&["/bin/sh", "-c", "exit 3"], 3),
        (&["/bin/sh", "-c", "kill -TERM $$"], 143),
        (&["/bin/no-such-editor"], 127),
    ];
    for (argv, expected) in cases {
        let output = Command::new("python3")
            .arg(&driver)
            .args(argv)
            .output()
            .expect("run motion-parity-pty.py");
        assert_eq!(
            output.status.code(),
            Some(expected),
            "the driver must report {argv:?}'s own status, not its own optimism"
        );
    }
}

#[test]
#[cfg(unix)]
fn motion_parity_audit_run_fails_when_the_sweep_wrote_no_probes() {
    // A sweep that wrote nothing is a failed sweep, even when the editor exits
    // 0 -- the question a check must answer is what it reports when the
    // artifact is EMPTY, not only when it is absent.
    let repo_root = motion_parity_repo_root();
    let fixture = tempdir();
    let out = fixture.join("no-probes.txt");
    let output = Command::new("bash")
        .arg(repo_root.join("scripts/l205-audit-run.sh"))
        .args(["/bin/sh", "scripts/motion-parity-audit.el", "L195_OUT"])
        .arg(&out)
        .args(["L195_REDISPLAY", "0", "80", "24"])
        .current_dir(&repo_root)
        .output()
        .expect("run l205-audit-run.sh");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "an empty sweep must not be reported as a good one:\n{combined}"
    );
    assert!(
        combined.contains("produced no probes"),
        "and it must say which editor produced nothing:\n{combined}"
    );
}

#[test]
#[cfg(unix)]
fn motion_parity_audit_run_says_when_the_editor_itself_could_not_be_RUN() {
    // Ledger 211, from a real incident.  A rebuild in the SHARED GNU mirror
    // deleted `src/emacs' mid-session, so `/home/exec/.local/bin/emacs' became
    // a broken symlink and the sweep's GNU side exited 127 with an EMPTY pty
    // log.
    //
    // Ledger 210's guards did their job and this test is not about them: the
    // driver reported 127 instead of 0, the runner failed instead of
    // publishing, and the sweep printed `SWEEP FAILED (gnu)' -- naming the
    // SIDE -- and `SWEEP INCOMPLETE'.  Nothing was published.  What the runner
    // did NOT do was say WHY: it knows the editor's exit status and does not
    // interpret it, so a missing editor reads as the same generic "produced no
    // probes" as an editor that ran and wrote nothing.  Those are different
    // failures and only one of them is about the sweep.
    //
    // 127 is the shell's own answer for "not found or not executable", and it
    // is what scripts/motion-parity-pty.py deliberately exits with.
    let repo_root = motion_parity_repo_root();
    let fixture = tempdir();
    let out = fixture.join("missing-editor.txt");
    let missing = fixture.join("no-such-editor");
    let output = Command::new("bash")
        .arg(repo_root.join("scripts/l205-audit-run.sh"))
        .arg(&missing)
        .args(["scripts/motion-parity-audit.el", "L195_OUT"])
        .arg(&out)
        .args(["L195_REDISPLAY", "0", "80", "24"])
        .current_dir(&repo_root)
        .output()
        .expect("run l205-audit-run.sh");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(127),
        "a missing editor must keep the shell's own status:\n{combined}"
    );
    assert!(
        combined.contains("could not be RUN"),
        "and the runner must say the EDITOR could not be run, not merely that \
         the sweep is empty -- those are different failures:\n{combined}"
    );
    assert!(
        combined.contains(&missing.display().to_string()),
        "naming the editor it could not run:\n{combined}"
    );
}

#[test]
fn motion_parity_sweep_publishes_every_documented_geometry() {
    // Ledger 210: one width cannot be the whole answer -- at 160 columns the
    // divergent set is a strict subset of the 80-column one -- so the sweep
    // that produces publishable numbers runs a documented SET.
    let sweep = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/scripts/motion-parity-sweep.sh"
    ));
    assert!(
        sweep.contains("WIDTH_SET=\"80x24 160x50\""),
        "80 for coverage, 160 for continuity with every number already published"
    );
    assert!(
        sweep.contains("for prot in cold warm"),
        "both protocols, because a defect in the scanner is invisible under WARM"
    );
    assert!(
        !sweep.contains("--allow-geometry-mismatch"),
        "the sweep that produces published numbers must never override the guard"
    );
    assert!(
        sweep.contains("SWEEP INCOMPLETE -- do not publish a partial set"),
        "a refused or failed cell has to stop the sweep being quoted"
    );
}

#[test]
#[cfg(unix)]
fn motion_parity_compare_refuses_a_sweep_that_did_not_write_its_probes() {
    // The most dangerous answer a comparator can give is a perfect one taken
    // from nothing.  Two empty files used to score `divergent=0' with exit 0,
    // and a header-only file -- geometry stamped, no probes -- did the same
    // while looking legitimate.  Ledger 210.
    let repo_root = motion_parity_repo_root();
    let fixture = tempdir();

    let empty_a = fixture.join("empty-a.txt");
    let empty_b = fixture.join("empty-b.txt");
    fs::write(&empty_a, "").unwrap();
    fs::write(&empty_b, "").unwrap();
    let output = run_motion_parity_compare(&repo_root, &empty_a, &empty_b, &[]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(3),
        "an empty sweep must not score at all:\n{combined}"
    );
    assert!(
        combined.contains("0 probes"),
        "and it must say the sweep wrote nothing:\n{combined}"
    );

    // A sweep that wrote SOME of its probes is just as failed, and the file
    // says so itself: `probes=' is what the sweep declares it wrote.
    let full = fixture.join("full.txt");
    let short = fixture.join("short.txt");
    fs::write(&full, motion_parity_fixture((80, 23), 80, 21, "(0 1)")).unwrap();
    fs::write(
        &short,
        "GEOMETRY frame-width=80 frame-height=23 probes=2\n\
         CONFIG full-wrap width=80 height=21 tl=nil ww=nil tpww=50 vlm=nil\n\
         full-wrap|1|vm0|(0 1)\n",
    )
    .unwrap();
    let output = run_motion_parity_compare(&repo_root, &full, &short, &[]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(3),
        "a truncated sweep must not score either:\n{combined}"
    );
    assert!(
        combined.contains("1 probes, but the sweep says it wrote 2"),
        "and the file's own declaration is what catches it:\n{combined}"
    );
}

#[test]
fn motion_parity_audit_stamps_the_frame_and_the_window_height() {
    let audit = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/scripts/motion-parity-audit.el"
    ));
    assert!(
        audit.contains("\"GEOMETRY frame-width=%s frame-height=%s probes=%s\""),
        "the sweep must record the frame it ran in, because the answers depend on it"
    );
    assert!(
        audit.contains("CONFIG %s width=%s height=%s"),
        "and the window height too: `move-to-window-line' with nil asks for the \
         MIDDLE row, so a taller window is a different question"
    );
}

fn tempdir() -> PathBuf {
    let dir = repository_root().join("tmp").join(format!(
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

/// **A generator that rewrites a file with identical bytes must not leave it
/// looking newer than the `.elc` compiled from it.**
///
/// The defect this pins, in its measured form: `cargo xtask fresh-build`
/// deletes and regenerates the CEDET semantic grammars, the LEIM tables and
/// the Unicode property tables on every run, and they come back byte for byte
/// the same with a fresh timestamp.  Nothing notices until an `.elc` is
/// compared against its `.el` -- and then everything does at once.  A peer
/// session measured **2,384 suite failures** thirty seconds after a
/// `--no-byte-compile` fresh-build, every one of them ledger 202's refusal
/// firing correctly on a build fault.
///
/// RED before ledger 206, produced exactly the way the fixture below does it:
/// rewrite two `.el` with their own bytes, and both become newer than the
/// `.elc` beside them, so a freshness sweep answers 2 where it should answer 0.
///
/// The guard is not a list of filenames and not per-generator: it captures
/// every `.el` under the tree and undoes the timestamp on any whose content is
/// unchanged, so a generator added later is covered without being registered.
#[test]
fn regenerating_a_lisp_source_with_identical_bytes_does_not_age_its_bytecode() {
    let root = tempdir();
    let lisp = root.join("lisp");
    fs::create_dir_all(lisp.join("cedet/semantic/bovine")).unwrap();

    let unchanged = lisp.join("cedet/semantic/bovine/c-by.el");
    let changed = lisp.join("cedet/semantic/bovine/make-by.el");
    let untouched = lisp.join("subr.el");
    fs::write(
        &unchanged,
        ";; generated\n(provide 'semantic/bovine/c-by)\n",
    )
    .unwrap();
    fs::write(
        &changed,
        ";; generated\n(provide 'semantic/bovine/make-by)\n",
    )
    .unwrap();
    fs::write(&untouched, "(provide 'subr)\n").unwrap();

    // Their bytecode, compiled from exactly those bytes, one second later --
    // which is what a real build leaves behind.
    for source in [&unchanged, &changed, &untouched] {
        let compiled = source.with_extension("elc");
        fs::write(&compiled, "bytecode\n").unwrap();
        let stamp =
            fs::metadata(source).unwrap().modified().unwrap() + std::time::Duration::from_secs(1);
        fs::File::options()
            .write(true)
            .open(&compiled)
            .unwrap()
            .set_modified(stamp)
            .unwrap();
    }

    let captured = UnchangedSourceMtimes::capture(&lisp).unwrap();

    // The generators run.  One rewrites its output with the SAME bytes, one
    // with different bytes, and one is not regenerated at all.  Every rewrite
    // lands after the `.elc`, as a real regeneration does.
    let later =
        fs::metadata(&unchanged).unwrap().modified().unwrap() + std::time::Duration::from_secs(60);
    for (path, contents) in [
        (
            &unchanged,
            ";; generated\n(provide 'semantic/bovine/c-by)\n",
        ),
        (
            &changed,
            ";; generated\n(provide 'semantic/bovine/make-by)\n;; new\n",
        ),
    ] {
        fs::write(path, contents).unwrap();
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(later)
            .unwrap();
    }

    assert_eq!(
        stale_bytecode_count(&lisp),
        2,
        "the fixture must reproduce the defect before the guard runs, or the \
         assertion below proves nothing"
    );

    let restored = captured.restore_unchanged().unwrap();

    assert_eq!(
        restored, 1,
        "exactly the identical rewrite is undone; the real change and the file \
         nothing touched are left alone"
    );
    assert_eq!(
        stale_bytecode_count(&lisp),
        1,
        "only the file whose CONTENT changed may be newer than its bytecode"
    );
    assert!(
        fs::metadata(&changed).unwrap().modified().unwrap()
            > fs::metadata(changed.with_extension("elc"))
                .unwrap()
                .modified()
                .unwrap(),
        "a genuinely regenerated file must keep its new timestamp, or the next \
         build would not recompile it"
    );
}

/// The capture reads the whole tree, not a registry, so a generated file
/// nobody listed is still covered.
#[test]
fn the_mtime_capture_covers_every_el_under_the_tree_and_no_elc() {
    let root = tempdir();
    let lisp = root.join("lisp");
    fs::create_dir_all(lisp.join("international")).unwrap();
    fs::create_dir_all(lisp.join("nested/deeper")).unwrap();
    fs::write(lisp.join("international/uni-name.el"), "(provide 'x)\n").unwrap();
    fs::write(
        lisp.join("nested/deeper/never-registered.el"),
        "(provide 'y)\n",
    )
    .unwrap();
    fs::write(lisp.join("international/uni-name.elc"), "bytecode\n").unwrap();

    let captured = UnchangedSourceMtimes::capture(&lisp).unwrap();
    let mut captured_paths: Vec<PathBuf> = captured.entries.keys().cloned().collect();
    captured_paths.sort();
    assert_eq!(
        captured_paths,
        vec![
            lisp.join("international/uni-name.el"),
            lisp.join("nested/deeper/never-registered.el"),
        ],
        "every .el and no .elc: a recompiled .elc legitimately gets a new \
         timestamp, and restoring an old one could make it older than the .el \
         it was just compiled from"
    );
}

/// Count `.elc` under ROOT whose `.el` sibling is strictly newer -- the same
/// predicate `neovm-core`'s `stale_lisp_bytecode` and GNU's `%.elc: %.el` use.
fn stale_bytecode_count(root: &Path) -> usize {
    let mut compiled = Vec::new();
    collect_lisp_bytecode_files(root, &mut compiled).unwrap();
    compiled
        .into_iter()
        .filter(|path| {
            let source = path.with_extension("el");
            let Ok(source_mtime) = fs::metadata(&source).and_then(|meta| meta.modified()) else {
                return false;
            };
            let Ok(compiled_mtime) = fs::metadata(path).and_then(|meta| meta.modified()) else {
                return false;
            };
            source_mtime > compiled_mtime
        })
        .count()
}

// ---------------------------------------------------------------------------
// Ledger 207: a `.elc` may only be deleted by a run that will put it back.
// ---------------------------------------------------------------------------

/// **`--no-byte-compile` must not delete bytecode nothing will recompile.**
///
/// `remove_stale_lisp_bytecode`, the bootstrap-clean sweep, has always been
/// behind `if !options.no_byte_compile`.  Two loaddefs cleanup steps were
/// written later and were not: primary and secondary loaddefs had their `.elc`
/// deleted unconditionally, while `run_compile_main` -- the only thing that
/// recreates them -- is gated.
/// So a `--no-byte-compile` run left the whole generated loaddefs set as `.el`
/// with no `.elc`, permanently, and every later `load` of one of them takes
/// `load-with-code-conversion` and rewrites `last-coding-system-used` under
/// its caller (`src/lread.c:1400-1418` -> `src/fileio.c:5172`).
///
/// That is not hypothetical: it is the state a peer session was standing in
/// when `oracle_load_auto_detects_iso_2022_source_without_a_valid_cookie`
/// failed for months, because the missing-lexbind-cookie warning pulls
/// `warnings` -> `icons` -> `cl-lib` -> `cl-loaddefs` *inside* the load being
/// measured.  Ledger 206 §9.2 measured the deletion at 19 files and recorded
/// it; this is the guard.
///
/// RED before ledger 207.
#[test]
fn a_no_byte_compile_run_deletes_no_bytecode_it_will_not_put_back() {
    let repo = tempdir();
    let lisp = repo.join("lisp");
    fs::create_dir_all(lisp.join("emacs-lisp")).unwrap();
    fs::create_dir_all(lisp.join("org")).unwrap();

    // The primary set, plus two secondaries, each as GNU ships it: a generated
    // `.el` with its `.elc` beside it.  `theme-loaddefs.el` is the one file
    // GNU deliberately leaves uncompiled, and it says so itself.
    let pairs = [
        "loaddefs",
        "emacs-lisp/cl-loaddefs",
        "org/org-loaddefs",
        "dired-loaddefs",
    ];
    for stem in pairs {
        fs::write(lisp.join(format!("{stem}.el")), ";; generated\n").unwrap();
        fs::write(lisp.join(format!("{stem}.elc")), ";ELC\n").unwrap();
    }
    fs::write(
        lisp.join("theme-loaddefs.el"),
        ";; Local Variables:\n;; no-byte-compile: t\n;; End:\n",
    )
    .unwrap();

    let options = FreshBuildOptions {
        repo_root: repo.clone(),
        runtime_root: repo.clone(),
        bin_dir: repo.join("target/release"),
        profile: BuildProfile::Release,
        production_capabilities: ProductionCapabilities::for_host().unwrap(),
        cargo_jobs: CargoJobBudget::Inherit,
        dry_run: false,
        native_comp: false,
        skip_build: false,
        product_variant: ProductVariant::Full,
        no_byte_compile: true,
        features: Vec::new(),
        aot_preload: false,
    };
    let paths = PipelinePaths {
        lisp_root: lisp.clone(),
        ..pipeline_paths(&options)
    };
    let plan = BytecodePlan::of(&options);

    remove_primary_loaddefs_for_regeneration(
        plan,
        &options,
        &paths,
        &lisp.join("loaddefs.el"),
        &lisp.join("theme-loaddefs.el"),
    )
    .unwrap();
    let orphaned = pairs
        .iter()
        .filter(|stem| !lisp.join(format!("{stem}.elc")).is_file())
        .map(|stem| (*stem).to_string())
        .collect::<Vec<_>>();

    assert_eq!(
        orphaned,
        Vec::<String>::new(),
        "a --no-byte-compile run deleted bytecode it will never recompile; \
         those files now load as source through load-with-code-conversion, \
         which rewrites last-coding-system-used under whatever called `load'"
    );
    fs::remove_dir_all(&repo).ok();
}

/// On a run that WILL recompile, primary `.elc` files must go, while secondary
/// loaddefs remain as inputs until the scrape has completed.
///
/// Green before ledger 207 -- it states the behaviour the guard must not
/// break, and without it "never delete anything" would satisfy both.
#[test]
fn a_recompiling_run_clears_primary_but_keeps_secondary_loaddefs_bytecode() {
    let repo = tempdir();
    let lisp = repo.join("lisp");
    fs::create_dir_all(lisp.join("emacs-lisp")).unwrap();
    fs::create_dir_all(lisp.join("org")).unwrap();
    for stem in ["loaddefs", "emacs-lisp/cl-loaddefs", "org/org-loaddefs"] {
        fs::write(lisp.join(format!("{stem}.el")), ";; generated\n").unwrap();
        fs::write(lisp.join(format!("{stem}.elc")), ";ELC\n").unwrap();
    }
    fs::write(lisp.join("theme-loaddefs.el"), ";; no-byte-compile: t\n").unwrap();

    let options = FreshBuildOptions {
        repo_root: repo.clone(),
        runtime_root: repo.clone(),
        bin_dir: repo.join("target/release"),
        profile: BuildProfile::Release,
        production_capabilities: ProductionCapabilities::for_host().unwrap(),
        cargo_jobs: CargoJobBudget::Inherit,
        dry_run: false,
        native_comp: false,
        skip_build: false,
        product_variant: ProductVariant::Full,
        no_byte_compile: false,
        features: Vec::new(),
        aot_preload: false,
    };
    let paths = PipelinePaths {
        lisp_root: lisp.clone(),
        ..pipeline_paths(&options)
    };
    let plan = BytecodePlan::of(&options);

    remove_primary_loaddefs_for_regeneration(
        plan,
        &options,
        &paths,
        &lisp.join("loaddefs.el"),
        &lisp.join("theme-loaddefs.el"),
    )
    .unwrap();
    assert!(!lisp.join("loaddefs.elc").is_file());
    assert!(!lisp.join("emacs-lisp/cl-loaddefs.elc").is_file());
    assert!(lisp.join("org/org-loaddefs.elc").is_file());
    fs::remove_dir_all(&repo).ok();
}

/// **Secondary loaddefs are inputs to the full autoload scrape, not merely
/// stale outputs.**
///
/// GNU's `autoloads-force` removes only `lisp/loaddefs.el`.  In particular it
/// leaves `net/tramp-loaddefs.el` loadable because `tramp-compat.el` requires
/// that generated seed while the scraper loads Tramp to expand its custom
/// autoload macros.  On Darwin, deleting the seed makes the load reach
/// `tramp-integration.el` before `tramp-local-host-names` has been defined and
/// fail with `(void-variable tramp-local-host-names)`.
///
/// Keep both source and bytecode until the producer has completed.  If stale
/// secondary outputs ever need pruning, that must be a post-success operation
/// driven by the producer's output manifest, never a pre-scrape filename scan.
///
/// RED before the loaddefs lifecycle fix.
#[test]
fn full_loaddefs_regeneration_keeps_secondary_bootstrap_seeds_loadable() {
    let repo = tempdir();
    let lisp = repo.join("lisp");
    fs::create_dir_all(lisp.join("net")).unwrap();
    fs::write(lisp.join("loaddefs.el"), ";; generated primary\n").unwrap();
    fs::write(lisp.join("loaddefs.elc"), ";ELC\n").unwrap();
    fs::write(
        lisp.join("net/tramp-loaddefs.el"),
        "(defvar tramp-local-host-names nil)\n",
    )
    .unwrap();
    fs::write(lisp.join("net/tramp-loaddefs.elc"), ";ELC\n").unwrap();

    let options = FreshBuildOptions {
        repo_root: repo.clone(),
        runtime_root: repo.clone(),
        bin_dir: repo.join("target/release"),
        profile: BuildProfile::Release,
        production_capabilities: ProductionCapabilities::for_host().unwrap(),
        cargo_jobs: CargoJobBudget::Inherit,
        dry_run: false,
        native_comp: false,
        skip_build: false,
        product_variant: ProductVariant::Full,
        no_byte_compile: false,
        features: Vec::new(),
        aot_preload: false,
    };
    let paths = PipelinePaths {
        lisp_root: lisp.clone(),
        ..pipeline_paths(&options)
    };
    let plan = BytecodePlan::of(&options);

    remove_primary_loaddefs_for_regeneration(
        plan,
        &options,
        &paths,
        &lisp.join("loaddefs.el"),
        &lisp.join("theme-loaddefs.el"),
    )
    .unwrap();
    assert!(!lisp.join("loaddefs.el").exists());
    assert!(
        lisp.join("net/tramp-loaddefs.el").is_file(),
        "the source seed must remain loadable throughout the scrape"
    );
    assert!(
        lisp.join("net/tramp-loaddefs.elc").is_file(),
        "the compiled seed must remain loadable throughout the scrape"
    );
    fs::remove_dir_all(&repo).ok();
}

/// **GNU's `compile-main` rule has one home.**
///
/// `compile_main_should_consider` used to re-spell GNU's grep here while the
/// same question went unasked everywhere else.  Ledger 206's lesson was that
/// two producers of one fact is the defect; this is the same shape for a
/// predicate rather than a file, so xtask and the tree scan in `neovm-core`
/// now read `crates/neovm-core/build_support/compile_main_rule.rs`.
///
/// RED before ledger 207: the module did not exist.
#[test]
fn compile_main_reads_gnus_rule_from_the_shared_module() {
    use compile_main_rule::BytecodeCoverage;

    let repo = tempdir();
    let lisp = repo.join("lisp");
    fs::create_dir_all(&lisp).unwrap();
    fs::write(lisp.join("plain.el"), ";;; plain.el\n").unwrap();
    fs::write(
        lisp.join("exempt.el"),
        ";;; exempt -*- no-byte-compile: t -*-\n",
    )
    .unwrap();
    fs::write(
        lisp.join("kept.el"),
        ";;; kept -*- no-byte-compile: t -*-\n",
    )
    .unwrap();
    fs::write(lisp.join("kept.elc"), ";ELC\n").unwrap();

    assert_eq!(
        BytecodeCoverage::of(&lisp.join("plain.el")).unwrap(),
        BytecodeCoverage::MissingBytecode
    );
    assert_eq!(
        BytecodeCoverage::of(&lisp.join("exempt.el")).unwrap(),
        BytecodeCoverage::ExemptBySourceCookie
    );
    // GNU's `test ! -f $${el}c &&` short-circuits, so a file that already has
    // a `.elc` is compiled again whatever its own text says.
    assert_eq!(
        BytecodeCoverage::of(&lisp.join("kept.el")).unwrap(),
        BytecodeCoverage::Compiled {
            compiled: lisp.join("kept.elc")
        }
    );

    assert!(compile_main_should_consider(&lisp.join("plain.el")).unwrap());
    assert!(!compile_main_should_consider(&lisp.join("exempt.el")).unwrap());
    assert!(compile_main_should_consider(&lisp.join("kept.el")).unwrap());
    fs::remove_dir_all(&repo).ok();
}

// ---------------------------------------------------------------------------
// Ledger 211: a parity count going DOWN is not evidence that a change helped.
//
// A behaviour change in the motion engines can fix twelve probes and break
// three, and the comparator's headline `divergent=' will still fall.  The
// number such a change owes its reader is NEWLY DIVERGENT -- probes the two
// editors agreed on before and disagree on after -- and it must be zero.
// `scripts/motion-parity-delta.py' computes it, and it inherits every refusal
// ledger 210 built into the comparator, because it asks a strictly harder
// question: two EMPTY files must not score `newly divergent 0', and a BEFORE
// taken at 160 columns against an AFTER taken at 80 is a geometry difference
// wearing a regression's clothes.
// ---------------------------------------------------------------------------

/// One `scripts/motion-parity-audit.el` output holding several probes.
fn motion_parity_fixture_probes(
    frame: (u32, u32),
    width: u32,
    height: u32,
    probes: &[(&str, &str)],
) -> String {
    let mut out = format!(
        "GEOMETRY frame-width={} frame-height={} probes={}\n\
         CONFIG full-wrap width={width} height={height} tl=nil ww=nil tpww=50 vlm=nil\n",
        frame.0,
        frame.1,
        probes.len()
    );
    for (motion, value) in probes {
        out.push_str(&format!("full-wrap|1|{motion}|{value}\n"));
    }
    out
}

fn run_motion_parity_delta(
    repo_root: &Path,
    before_gnu: &Path,
    before_neo: &Path,
    after_gnu: &Path,
    after_neo: &Path,
) -> std::process::Output {
    Command::new("python3")
        .arg(repo_root.join("scripts/motion-parity-delta.py"))
        .arg(before_gnu)
        .arg(before_neo)
        .arg(after_gnu)
        .arg(after_neo)
        .output()
        .expect("run motion-parity-delta.py")
}

fn motion_parity_delta_output(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
#[cfg(unix)]
fn motion_parity_delta_refuses_a_sweep_that_wrote_nothing() {
    let fixture = tempdir();
    let before_gnu = fixture.join("before-gnu.txt");
    let before_neo = fixture.join("before-neo.txt");
    let after_gnu = fixture.join("after-gnu.txt");
    let after_neo = fixture.join("after-neo.txt");
    let probes = motion_parity_fixture_probes((80, 23), 80, 21, &[("vm0", "(0 1)")]);
    fs::write(&before_gnu, &probes).unwrap();
    fs::write(&before_neo, &probes).unwrap();
    // The dangerous answer is available here: two EMPTY files hold no
    // disagreements, so a naive delta would report `newly divergent 0' -- a
    // perfect result taken from nothing.
    fs::write(&after_gnu, "").unwrap();
    fs::write(&after_neo, "").unwrap();

    let output = run_motion_parity_delta(
        &motion_parity_repo_root(),
        &before_gnu,
        &before_neo,
        &after_gnu,
        &after_neo,
    );
    let combined = motion_parity_delta_output(&output);
    assert_eq!(
        output.status.code(),
        Some(3),
        "an empty AFTER sweep must be refused, not scored as a clean delta:\n{combined}"
    );
    assert!(
        combined.contains("the sweep wrote nothing"),
        "the refusal must say the sweep wrote nothing:\n{combined}"
    );
}

#[test]
#[cfg(unix)]
fn motion_parity_delta_refuses_a_before_and_after_taken_in_different_frames() {
    let fixture = tempdir();
    let before_gnu = fixture.join("before-gnu.txt");
    let before_neo = fixture.join("before-neo.txt");
    let after_gnu = fixture.join("after-gnu.txt");
    let after_neo = fixture.join("after-neo.txt");
    let wide = motion_parity_fixture_probes((160, 49), 160, 47, &[("vm0", "(0 1)")]);
    let narrow = motion_parity_fixture_probes((80, 23), 80, 21, &[("vm0", "(0 1)")]);
    fs::write(&before_gnu, &wide).unwrap();
    fs::write(&before_neo, &wide).unwrap();
    fs::write(&after_gnu, &narrow).unwrap();
    fs::write(&after_neo, &narrow).unwrap();

    let output = run_motion_parity_delta(
        &motion_parity_repo_root(),
        &before_gnu,
        &before_neo,
        &after_gnu,
        &after_neo,
    );
    let combined = motion_parity_delta_output(&output);
    assert_eq!(
        output.status.code(),
        Some(2),
        "a delta taken across two frames is a geometry difference, not a \
         regression, and must be refused:\n{combined}"
    );
    assert!(
        combined.contains("frame 160x49") && combined.contains("frame 80x23"),
        "the refusal must name both frames:\n{combined}"
    );
}

#[test]
#[cfg(unix)]
fn motion_parity_delta_fails_when_a_probe_that_agreed_becomes_divergent() {
    let fixture = tempdir();
    let before_gnu = fixture.join("before-gnu.txt");
    let before_neo = fixture.join("before-neo.txt");
    let after_gnu = fixture.join("after-gnu.txt");
    let after_neo = fixture.join("after-neo.txt");
    let gnu =
        motion_parity_fixture_probes((80, 23), 80, 21, &[("vm0", "(0 1)"), ("eovl", "(0 80)")]);
    fs::write(&before_gnu, &gnu).unwrap();
    fs::write(&after_gnu, &gnu).unwrap();
    // Before: `vm0' diverges and `eovl' agrees.  After: `vm0' is fixed and
    // `eovl' is broken.  The headline count is 1 in both, which is exactly the
    // case a headline cannot report.
    fs::write(
        &before_neo,
        motion_parity_fixture_probes((80, 23), 80, 21, &[("vm0", "(0 8)"), ("eovl", "(0 80)")]),
    )
    .unwrap();
    fs::write(
        &after_neo,
        motion_parity_fixture_probes((80, 23), 80, 21, &[("vm0", "(0 1)"), ("eovl", "(0 79)")]),
    )
    .unwrap();

    let output = run_motion_parity_delta(
        &motion_parity_repo_root(),
        &before_gnu,
        &before_neo,
        &after_gnu,
        &after_neo,
    );
    let combined = motion_parity_delta_output(&output);
    assert_eq!(
        output.status.code(),
        Some(4),
        "a probe that became divergent must fail the delta even though the \
         headline count did not move:\n{combined}"
    );
    assert!(
        combined.contains("NEWLY DIVERGENT  = 1") && combined.contains("full-wrap|1|eovl"),
        "the failure must count and NAME the newly divergent probe:\n{combined}"
    );
}

#[test]
#[cfg(unix)]
fn motion_parity_delta_scores_a_fixed_probe_as_fixed() {
    let fixture = tempdir();
    let before_gnu = fixture.join("before-gnu.txt");
    let before_neo = fixture.join("before-neo.txt");
    let after_gnu = fixture.join("after-gnu.txt");
    let after_neo = fixture.join("after-neo.txt");
    let gnu = motion_parity_fixture_probes((80, 23), 80, 21, &[("vm0", "(0 1)")]);
    fs::write(&before_gnu, &gnu).unwrap();
    fs::write(&after_gnu, &gnu).unwrap();
    fs::write(
        &before_neo,
        motion_parity_fixture_probes((80, 23), 80, 21, &[("vm0", "(0 8)")]),
    )
    .unwrap();
    fs::write(&after_neo, &gnu).unwrap();

    let output = run_motion_parity_delta(
        &motion_parity_repo_root(),
        &before_gnu,
        &before_neo,
        &after_gnu,
        &after_neo,
    );
    let combined = motion_parity_delta_output(&output);
    assert_eq!(
        output.status.code(),
        Some(0),
        "a delta that fixes a probe and breaks none must pass:\n{combined}"
    );
    assert!(
        combined.contains("fixed            = 1")
            && combined.contains("NEWLY DIVERGENT  = 0")
            && combined.contains("[frame 80x23]"),
        "the delta must report the fix, the zero, and the frame:\n{combined}"
    );
}
