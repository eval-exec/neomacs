#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use std::time::Duration;

use crate::{
    EmacsRuntime, MelpaSandbox, PackageActivation, SHALLOW_GIT_FETCH_ARGS, SourceBuild,
    locked_melpa_install_plan, locked_melpa_source, locked_melpa_sources, package_activation_elisp,
    run_elisp_oracle, workspace_root,
};
#[cfg(unix)]
use crate::{
    ErtScenario, OracleBatchFailure, PackageScenario, PackageSource, ScenarioPhase,
    run_elisp_oracle_batch, run_ert_scenario, run_oracle_scenario, run_scenario,
};
#[cfg(unix)]
use neomacs_test_oracle::BatchProbe;
use neomacs_test_oracle::EvalOutcome;

#[test]
fn installed_autoload_activation_never_loads_the_package_source() {
    assert_eq!(
        package_activation_elisp(PackageActivation::InstalledAutoloads),
        "nil"
    );
    assert!(
        package_activation_elisp(PackageActivation::SourceFile).contains("NEOMACS_PACKAGE_SOURCE")
    );
}

#[test]
fn sandbox_keeps_process_state_under_workspace_tmp_and_socket_paths_bounded() {
    let sandbox = MelpaSandbox::new("environment-contract").expect("create MELPA sandbox");
    let scratch_base = workspace_root().join("tmp/melpa");

    assert!(sandbox.root().starts_with(&scratch_base));
    assert!(sandbox.home().starts_with(sandbox.root()));
    assert!(sandbox.tmp_dir().starts_with(sandbox.root()));
    assert!(sandbox.home().is_dir());
    assert!(sandbox.tmp_dir().is_dir());

    #[cfg(unix)]
    {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            r##"printf '%s\n' "$HOME" "$TMPDIR" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$XDG_RUNTIME_DIR" "$PWD" "$USER" "$LOGNAME" "$HOSTNAME" "$EMAIL" "$TZ" "$LC_ALL" "$TERM""##,
        ]);
        sandbox.configure(&mut command);
        let output = command.output().expect("inspect sandbox environment");
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).expect("environment output is UTF-8");
        let paths = stdout
            .lines()
            .take(8)
            .map(Path::new)
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();

        assert_eq!(paths[0], sandbox.home());
        assert_eq!(paths[1], sandbox.tmp_dir());
        assert!(
            paths[2..6]
                .iter()
                .all(|path| path.starts_with(sandbox.root()))
        );
        let runtime_dir = &paths[6];
        assert!(runtime_dir.is_dir());
        use std::os::unix::ffi::OsStrExt;
        assert!(
            runtime_dir
                .join("emacs/server")
                .as_os_str()
                .as_bytes()
                .len()
                <= 100,
            "XDG runtime path must leave safe headroom below Unix-domain socket limits: {}",
            runtime_dir.display()
        );
        assert_eq!(paths[7], sandbox.root());

        let values = stdout.lines().collect::<Vec<_>>();
        assert_eq!(
            &values[8..],
            [
                "melpa-test",
                "melpa-test",
                "melpa-host",
                "melpa-test@melpa-host",
                "UTC",
                "C.UTF-8",
                "dumb",
            ]
        );
    }
}

#[test]
fn nextest_runs_melpa_infrastructure_preflight_once_before_parity_tests() {
    let nextest = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/.config/nextest.toml"
    ));
    let preflight = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/scripts/melpa-infra-preflight.sh"
    ));

    assert!(nextest.contains(r#"experimental = ["wrapper-scripts", "setup-scripts"]"#));
    assert!(nextest.contains("[scripts.setup.melpa-infra-preflight]"));
    assert!(nextest.contains("scripts/melpa-infra-preflight.sh"));
    assert!(nextest.contains(
        "filter = 'package(neomacs-melpa-tests) and not (test(~parity_tests::harness_contract::) or test(~source_lock::tests::))'"
    ));
    assert!(nextest.contains("setup = 'melpa-infra-preflight'"));
    assert!(preflight.contains("NEXTEST_WORKSPACE_ROOT"));
    assert!(preflight.contains(r#"mktemp -d "$scratch_parent/preflight.XXXXXX""#));
    assert!(preflight.contains("resolve_executable Git git"));
    assert!(preflight.contains("NEOMACS-MELPA-PREFLIGHT:ready"));
    assert!(!preflight.contains("mktemp -d /tmp"));
    assert!(!preflight.contains("TMPDIR=/tmp"));
}

#[cfg(unix)]
#[test]
fn dependency_regeneration_preserves_every_source_lock_field() {
    let fixture =
        MelpaSandbox::new("dependency-regeneration-contract").expect("create fixture sandbox");
    let manifest = fixture.root().join("melpa-package-lock.tsv");
    let cache = fixture.root().join("package-cache");
    let dependency_dir = cache.join("dependency-1.0");
    let dash_dir = cache.join("dash-1.0");
    let git_commit_mode_dir = cache.join("git-commit-mode-20141106.1722");
    let multiline_dir = cache.join("multiline-3.0");
    let root_dir = cache.join("root-2.0");
    let with_editor_dir = cache.join("with-editor-1.0");
    fs::create_dir_all(&dependency_dir).expect("create dependency cache fixture");
    fs::create_dir_all(&dash_dir).expect("create dash cache fixture");
    fs::create_dir_all(&git_commit_mode_dir).expect("create git-commit-mode cache fixture");
    fs::create_dir_all(&multiline_dir).expect("create multiline cache fixture");
    fs::create_dir_all(&root_dir).expect("create root cache fixture");
    fs::create_dir_all(&with_editor_dir).expect("create with-editor cache fixture");
    fs::write(
        dependency_dir.join("dependency.el"),
        ";;; dependency.el --- fixture\n",
    )
    .expect("write dependency package fixture");
    fs::write(dash_dir.join("dash.el"), ";;; dash.el --- fixture\n")
        .expect("write dash package fixture");
    fs::write(
        git_commit_mode_dir.join("git-commit-mode.el"),
        ";;; git-commit-mode.el --- historical fixture\n\
         (require 'dash)\n\
         (require 'log-edit)\n\
         (require 'with-editor)\n",
    )
    .expect("write git-commit-mode package fixture");
    fs::write(
        multiline_dir.join("multiline.el"),
        ";;; multiline.el --- fixture\n\
         ;; Package-Requires: (\n\
         ;;     (emacs \"29.1\")\n\
         ;;     (dependency \"1.0\"))\n",
    )
    .expect("write multiline package fixture");
    fs::write(
        root_dir.join("root.el"),
        ";;; root.el --- fixture\n;; Package-Requires: ((dependency \"1.0\"))\n",
    )
    .expect("write root package fixture");
    fs::write(
        with_editor_dir.join("with-editor.el"),
        ";;; with-editor.el --- fixture\n",
    )
    .expect("write with-editor package fixture");

    let before = "package\tversion\tupstream\tupstream-revision\trepository\trevision\tfallback-repository\tbuild\tdependencies\n\
                  dash\t1.0\thttps://upstream.invalid/dash\taaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\thttps://mirror.invalid/dash\tbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\t\tsource-default\t-\n\
                  dependency\t1.0\thttps://upstream.invalid/dependency\t0123456789abcdef0123456789abcdef01234567\thttps://mirror.invalid/dependency\t89abcdef0123456789abcdef0123456789abcdef\t\tsource-default\t-\n\
                  git-commit-mode\t20141106.1722\thttps://upstream.invalid/git-modes\t7138eecb882e58466079d79925ccf85e3c24e866\thttps://mirror.invalid/git-modes\t7138eecb882e58466079d79925ccf85e3c24e866\thttps://fallback.invalid/git-modes\tsource-glob:git-commit-mode.el\t-\n\
                  multiline\t3.0\thttps://upstream.invalid/multiline\t3333333333333333333333333333333333333333\thttps://mirror.invalid/multiline\t4444444444444444444444444444444444444444\t\tsource-default\t-\n\
                  root\t2.0\thttps://upstream.invalid/root\t1111111111111111111111111111111111111111\thttps://mirror.invalid/root\t2222222222222222222222222222222222222222\thttps://fallback.invalid/root\tmelpa-recipe\t-\n\
                  with-editor\t1.0\thttps://upstream.invalid/with-editor\tcccccccccccccccccccccccccccccccccccccccc\thttps://mirror.invalid/with-editor\tdddddddddddddddddddddddddddddddddddddddd\t\tsource-default\t-\n";
    fs::write(&manifest, before).expect("write package-lock fixture");

    let output = Command::new("python3")
        .arg(workspace_root().join("scripts/melpa-derive-dependencies.py"))
        .args(["--manifest"])
        .arg(&manifest)
        .args(["--cache"])
        .arg(&cache)
        .arg("--write")
        .output()
        .expect("run dependency regeneration");

    assert!(
        output.status.success(),
        "dependency regeneration failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let after = "package\tversion\tupstream\tupstream-revision\trepository\trevision\tfallback-repository\tbuild\tdependencies\n\
                 dash\t1.0\thttps://upstream.invalid/dash\taaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\thttps://mirror.invalid/dash\tbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\t\tsource-default\t-\n\
                 dependency\t1.0\thttps://upstream.invalid/dependency\t0123456789abcdef0123456789abcdef01234567\thttps://mirror.invalid/dependency\t89abcdef0123456789abcdef0123456789abcdef\t\tsource-default\t-\n\
                 git-commit-mode\t20141106.1722\thttps://upstream.invalid/git-modes\t7138eecb882e58466079d79925ccf85e3c24e866\thttps://mirror.invalid/git-modes\t7138eecb882e58466079d79925ccf85e3c24e866\thttps://fallback.invalid/git-modes\tsource-glob:git-commit-mode.el\tdash,with-editor\n\
                 multiline\t3.0\thttps://upstream.invalid/multiline\t3333333333333333333333333333333333333333\thttps://mirror.invalid/multiline\t4444444444444444444444444444444444444444\t\tsource-default\tdependency\n\
                 root\t2.0\thttps://upstream.invalid/root\t1111111111111111111111111111111111111111\thttps://mirror.invalid/root\t2222222222222222222222222222222222222222\thttps://fallback.invalid/root\tmelpa-recipe\tdependency\n\
                 with-editor\t1.0\thttps://upstream.invalid/with-editor\tcccccccccccccccccccccccccccccccccccccccc\thttps://mirror.invalid/with-editor\tdddddddddddddddddddddddddddddddddddddddd\t\tsource-default\t-\n";
    assert_eq!(
        fs::read_to_string(&manifest).expect("read regenerated package lock"),
        after
    );

    let second = Command::new("python3")
        .arg(workspace_root().join("scripts/melpa-derive-dependencies.py"))
        .args(["--manifest"])
        .arg(&manifest)
        .args(["--cache"])
        .arg(&cache)
        .arg("--write")
        .output()
        .expect("rerun dependency regeneration");
    assert!(second.status.success(), "second regeneration failed");
    assert_eq!(
        fs::read_to_string(&manifest).expect("read idempotent package lock"),
        after,
        "source-backed dependency regeneration must be idempotent"
    );
}

#[cfg(unix)]
#[test]
fn scenario_installs_then_probes_in_a_fresh_process() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = MelpaSandbox::new("runtime-contract").expect("create fixture sandbox");
    let invocation_log = fixture.root().join("invocations");
    let runtime_script = fixture.root().join("fake-emacs");
    fs::write(
        &runtime_script,
        format!(
            r##"#!/bin/sh
printf '%s\n' invoke >> '{}'
printf 'NEOMACS-MELPA-INSTALLED:simple-single\t1.3\n'
printf '%s\n' 'NEOMACS-MELPA-OUTCOME:OK (:package simple-single :value 42)' >&2
"##,
            invocation_log.display()
        ),
    )
    .expect("write fake runtime");
    fs::set_permissions(&runtime_script, fs::Permissions::from_mode(0o755))
        .expect("make fake runtime executable");

    let runtime = EmacsRuntime::new("fake", runtime_script);
    let source =
        PackageSource::frozen(workspace_root().join("test/lisp/emacs-lisp/package-resources"));
    let scenario = PackageScenario::new(
        "two-process-contract",
        ["simple-single"],
        r##"'(:package simple-single :value 42)"##,
    );

    let report = run_scenario(&runtime, &source, &scenario).expect("run fake scenario");

    let invocations = fs::read_to_string(invocation_log).expect("read runtime invocations");
    assert_eq!(invocations.lines().count(), 2);
    assert_eq!(report.phases.len(), 2);
    assert_eq!(report.phases[0].phase, ScenarioPhase::Install);
    assert_eq!(report.phases[1].phase, ScenarioPhase::RestartProbe);
    assert_eq!(
        report.outcome,
        EvalOutcome::Value("(:package simple-single :value 42)".to_string())
    );
    assert_eq!(report.installed_packages.len(), 1);
    assert_eq!(report.installed_packages[0].name, "simple-single");
    assert_eq!(report.installed_packages[0].version, "1.3");
}

#[cfg(unix)]
#[test]
fn oracle_scenario_compares_matching_lisp_signals() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = MelpaSandbox::new("oracle-signal-contract").expect("create fixture sandbox");
    let runtime_script = fixture.root().join("signal-emacs");
    fs::write(
        &runtime_script,
        r##"#!/bin/sh
printf 'NEOMACS-MELPA-INSTALLED:simple-single\t1.3\n'
if grep -q 'NEOMACS-MELPA-OUTCOME' "$NEOMACS_MELPA_ORACLE_FORM_FILE"; then
  printf '%s\n' 'NEOMACS-MELPA-OUTCOME:ERR (wrong-type-argument numberp "x")' >&2
fi
"##,
    )
    .expect("write signal runtime");
    fs::set_permissions(&runtime_script, fs::Permissions::from_mode(0o755))
        .expect("make signal runtime executable");

    let runtime = EmacsRuntime::new("fake", runtime_script);
    let source =
        PackageSource::frozen(workspace_root().join("test/lisp/emacs-lisp/package-resources"));
    let scenario = PackageScenario::new("signal-parity", ["simple-single"], "(+ 1 \"x\")");

    let report = run_oracle_scenario(&runtime, &runtime, &source, &scenario)
        .expect("matching signals have oracle parity");

    assert_eq!(
        report.neomacs.outcome,
        EvalOutcome::Signal(r##"(wrong-type-argument numberp "x")"##.to_string())
    );
    assert_eq!(report.neomacs.outcome, report.gnu_emacs.outcome);
}

#[cfg(unix)]
#[test]
fn oracle_scenario_reports_a_value_divergence() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = MelpaSandbox::new("oracle-divergence-contract").expect("create fixture sandbox");
    let neo_script = fixture.root().join("neo-emacs");
    let gnu_script = fixture.root().join("gnu-emacs");
    for (script, value) in [(&neo_script, "42"), (&gnu_script, "43")] {
        fs::write(
            script,
            format!(
                r##"#!/bin/sh
printf 'NEOMACS-MELPA-INSTALLED:simple-single\t1.3\n'
printf '%s\n' 'NEOMACS-MELPA-OUTCOME:OK {value}' >&2
"##
            ),
        )
        .expect("write divergent runtime");
        fs::set_permissions(script, fs::Permissions::from_mode(0o755))
            .expect("make divergent runtime executable");
    }

    let source =
        PackageSource::frozen(workspace_root().join("test/lisp/emacs-lisp/package-resources"));
    let scenario = PackageScenario::new("value-divergence", ["simple-single"], "42");
    let error = run_oracle_scenario(
        &EmacsRuntime::new("neomacs", neo_script),
        &EmacsRuntime::new("gnu-emacs", gnu_script),
        &source,
        &scenario,
    )
    .expect_err("different values must fail oracle parity");

    assert!(error.contains("value-divergence"));
    assert!(error.contains("OK 42"));
    assert!(error.contains("OK 43"));
}

#[cfg(unix)]
#[test]
fn direct_elisp_oracle_runs_one_form_without_a_package_install() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = MelpaSandbox::new("direct-oracle-contract").expect("create fixture sandbox");
    let runtime_script = fixture.root().join("direct-emacs");
    fs::write(
        &runtime_script,
        r##"#!/bin/sh
if grep -q 'dash-sentinel' "$NEOMACS_MELPA_ORACLE_FORM_FILE"; then
  printf '%s\n' 'NEOMACS-MELPA-OUTCOME:OK (:dash direct)' >&2
else
  exit 9
fi
"##,
    )
    .expect("write direct runtime");
    fs::set_permissions(&runtime_script, fs::Permissions::from_mode(0o755))
        .expect("make direct runtime executable");

    let runtime = EmacsRuntime::new("fake", runtime_script);
    let report = run_elisp_oracle(&runtime, &runtime, "direct-dash-form", "", "'dash-sentinel")
        .expect("run direct differential form");

    assert_eq!(
        report.neomacs,
        EvalOutcome::Value("(:dash direct)".to_string())
    );
    assert_eq!(report.neomacs, report.gnu_emacs);
}

#[cfg(unix)]
#[test]
fn batch_timeout_names_the_probe_that_started_but_did_not_finish() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = MelpaSandbox::new("batch-timeout-contract").expect("create fixture sandbox");
    let runtime_script = fixture.root().join("hanging-emacs");
    fs::write(
        &runtime_script,
        r##"#!/bin/sh
printf '%s\n' 'NEOMACS-MELPA-BEGIN:hanging-case' >&2
exec sleep 30
"##,
    )
    .expect("write hanging runtime");
    fs::set_permissions(&runtime_script, fs::Permissions::from_mode(0o755))
        .expect("make hanging runtime executable");

    let runtime = EmacsRuntime::new("fake", runtime_script).with_timeout(Duration::from_millis(50));
    let error = run_elisp_oracle_batch(
        &runtime,
        &runtime,
        "timeout-batch",
        "",
        &[BatchProbe {
            id: "hanging-case",
            probe: "(sleep-for 30)",
        }],
    )
    .expect_err("a timed-out batch must fail");

    assert!(error.contains("hanging-case"), "unexpected error: {error}");
    assert!(error.contains("active case"), "unexpected error: {error}");
}

#[cfg(unix)]
#[test]
fn batch_timeout_reports_a_malformed_partial_protocol() {
    use std::os::unix::fs::PermissionsExt;

    let fixture =
        MelpaSandbox::new("batch-timeout-malformed-contract").expect("create fixture sandbox");
    let runtime_script = fixture.root().join("malformed-hanging-emacs");
    fs::write(
        &runtime_script,
        r##"#!/bin/sh
printf '%s\n' 'NEOMACS-MELPA-BEGIN:bad id' >&2
exec sleep 30
"##,
    )
    .expect("write malformed hanging runtime");
    fs::set_permissions(&runtime_script, fs::Permissions::from_mode(0o755))
        .expect("make malformed hanging runtime executable");

    let runtime = EmacsRuntime::new("fake", runtime_script).with_timeout(Duration::from_millis(50));
    let error = run_elisp_oracle_batch(
        &runtime,
        &runtime,
        "malformed-timeout-batch",
        "",
        &[BatchProbe {
            id: "expected-case",
            probe: "(sleep-for 30)",
        }],
    )
    .expect_err("a malformed timed-out batch must fail");

    assert!(
        error.contains("invalid partial batch protocol"),
        "unexpected error: {error}"
    );
    assert!(error.contains("bad id"), "unexpected error: {error}");
}

#[cfg(unix)]
#[test]
fn batch_report_keeps_every_case_after_differential_mismatches() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = MelpaSandbox::new("batch-report-contract").expect("create fixture sandbox");
    let gnu_script = fixture.root().join("gnu-emacs");
    let neomacs_script = fixture.root().join("neomacs");
    fs::write(
        &gnu_script,
        r##"#!/bin/sh
printf '%s\n' 'NEOMACS-MELPA-BEGIN:first' 'NEOMACS-MELPA-OUTCOME:first:OK 1' 'NEOMACS-MELPA-COMPLETE:first' 'NEOMACS-MELPA-BEGIN:second' 'NEOMACS-MELPA-OUTCOME:second:ERR (user-error "gnu")' 'NEOMACS-MELPA-COMPLETE:second' >&2
"##,
    )
    .expect("write GNU runtime");
    fs::write(
        &neomacs_script,
        r##"#!/bin/sh
printf '%s\n' 'NEOMACS-MELPA-BEGIN:first' 'NEOMACS-MELPA-OUTCOME:first:OK 2' 'NEOMACS-MELPA-COMPLETE:first' 'NEOMACS-MELPA-BEGIN:second' 'NEOMACS-MELPA-OUTCOME:second:OK 3' 'NEOMACS-MELPA-COMPLETE:second' >&2
"##,
    )
    .expect("write Neomacs runtime");
    for script in [&gnu_script, &neomacs_script] {
        fs::set_permissions(script, fs::Permissions::from_mode(0o755))
            .expect("make runtime executable");
    }

    let report = run_elisp_oracle_batch(
        &EmacsRuntime::new("Neomacs", neomacs_script),
        &EmacsRuntime::new("GNU Emacs", gnu_script),
        "differential-batch",
        "",
        &[
            BatchProbe {
                id: "first",
                probe: "1",
            },
            BatchProbe {
                id: "second",
                probe: "2",
            },
        ],
    )
    .expect("differential mismatches are report data, not infrastructure errors");

    assert_eq!(report.cases.len(), 2);
    assert_eq!(report.failures.len(), 2);
    assert!(matches!(
        &report.failures[0],
        OracleBatchFailure::OutcomeMismatch { id, .. } if id == "first"
    ));
    assert!(matches!(
        &report.failures[1],
        OracleBatchFailure::OutcomeMismatch { id, .. } if id == "second"
    ));
}

#[cfg(unix)]
#[test]
fn batch_forms_larger_than_the_process_argument_limit_are_loaded_from_workspace_scratch() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = MelpaSandbox::new("large-batch-form-contract").expect("create fixture sandbox");
    let runtime_script = fixture.root().join("file-loading-emacs");
    fs::write(
        &runtime_script,
        r##"#!/bin/sh
form=
while [ "$#" -gt 0 ]; do
  case "$1" in
    --eval)
      shift
      [ "${#1}" -lt 256 ] || exit 8
      ;;
    --load|-l)
      shift
      form=$1
      ;;
  esac
  shift
done
[ -f "$form" ] || exit 9
[ -f "$NEOMACS_MELPA_ORACLE_FORM_FILE" ] || exit 10
[ "$(wc -c < "$NEOMACS_MELPA_ORACLE_FORM_FILE")" -gt 131072 ] || exit 11
grep -q '^(defun neomacs--melpa-oracle-transported-form ()' "$form" || exit 11
printf '%s\n' 'NEOMACS-MELPA-BEGIN:large-form' 'NEOMACS-MELPA-OUTCOME:large-form:OK t' 'NEOMACS-MELPA-COMPLETE:large-form' >&2
"##,
    )
    .expect("write file-loading runtime");
    fs::set_permissions(&runtime_script, fs::Permissions::from_mode(0o755))
        .expect("make file-loading runtime executable");

    let runtime = EmacsRuntime::new("fake", runtime_script);
    let oversized_setup = format!("{}\nnil", " ".repeat(140_000));
    let report = run_elisp_oracle_batch(
        &runtime,
        &runtime,
        "large-form-batch",
        &oversized_setup,
        &[BatchProbe {
            id: "large-form",
            probe: "t",
        }],
    )
    .expect("a large batch form is transported through a sandbox file");

    assert!(report.failures.is_empty());
    assert_eq!(report.cases.len(), 1);
    assert_eq!(report.cases[0].gnu_emacs, EvalOutcome::Value("t".into()));
    assert_eq!(report.cases[0].neomacs, EvalOutcome::Value("t".into()));
}

#[test]
fn transported_setup_defines_generalized_variables_before_probe_macroexpansion() {
    let runtime = EmacsRuntime::gnu_emacs();
    let report = run_elisp_oracle(
        &runtime,
        &runtime,
        "transport-macroexpansion-contract",
        r##"(let ((source
                    (expand-file-name
                     "transport-record.el"
                     (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
              (with-temp-file source
                (insert
                 "(require 'cl-lib)\n(cl-defstruct neomacs-transport-record value)\n"))
              (load source nil nil t))"##,
        r##"(let ((record (make-neomacs-transport-record :value 1)))
              (setf (neomacs-transport-record-value record) 2)
              (neomacs-transport-record-value record))"##,
    )
    .expect("transported setup runs before the probe is macroexpanded");

    assert_eq!(report.gnu_emacs, EvalOutcome::Value("2".into()));
    assert_eq!(report.neomacs, EvalOutcome::Value("2".into()));
}

#[test]
fn transported_form_runs_after_the_loader_buffer_is_gone() {
    let runtime = EmacsRuntime::gnu_emacs();
    let report = run_elisp_oracle(
        &runtime,
        &runtime,
        "transport-current-buffer-contract",
        "",
        r##"(list (buffer-name) (with-temp-buffer (buffer-name)))"##,
    )
    .expect("transport preserves the current-buffer semantics of --eval");

    assert_eq!(
        report.gnu_emacs,
        EvalOutcome::Value(r##"("*scratch*" " *temp*")"##.into())
    );
    assert_eq!(report.neomacs, report.gnu_emacs);
}

#[test]
fn exact_git_package_uses_upstream_with_an_emacsmirror_fallback() {
    let source = locked_melpa_source(("agent-shell", "20260728.953"))
        .expect("resolve the revision-pinned agent-shell source");

    assert_eq!(source.package(), ("agent-shell", "20260728.953"));
    assert_eq!(
        source.upstream_repository(),
        "https://github.com/xenodium/agent-shell"
    );
    assert_eq!(
        source.upstream_revision(),
        "a59891a9d8f1d26afb8358239346e081708cf2cb"
    );
    assert_eq!(
        source.repository(),
        "https://github.com/xenodium/agent-shell"
    );
    assert_eq!(
        source.revision(),
        "a59891a9d8f1d26afb8358239346e081708cf2cb"
    );
    assert_eq!(
        source.fallback_repository(),
        Some("https://github.com/emacsmirror/agent-shell")
    );
    assert_eq!(source.build(), SourceBuild::MelpaRecipe);

    let error = locked_melpa_source(("agent-shell", "20260724.1019"))
        .expect_err("an obsolete rolling pin must not resolve");
    assert!(error.contains("revision-pinned source lock"));
}

#[test]
fn non_git_upstream_is_acquired_from_an_exact_emacsmirror_git_revision() {
    let source = locked_melpa_source(("2048-game", "20230809.356"))
        .expect("resolve the mirrored 2048-game source");

    assert_eq!(
        source.upstream_repository(),
        "https://hg.sr.ht/~zck/game-2048"
    );
    assert_eq!(
        source.upstream_revision(),
        "8175ca5191175183b9522141dcb55d30673d2323"
    );
    assert_eq!(
        source.repository(),
        "https://github.com/emacsmirror/2048-game"
    );
    assert_eq!(
        source.revision(),
        "8976bb8875fc638806d0db5e0ba9c573f6ca7a25"
    );
    assert_eq!(source.fallback_repository(), None);
    assert_eq!(source.build(), SourceBuild::DefaultFiles);
}

#[test]
fn source_build_can_exclude_upstream_test_code_from_the_runtime_package() {
    let source = locked_melpa_source(("alectryon", "20260525.2000"))
        .expect("resolve the Alectryon runtime source");

    assert_eq!(source.build(), SourceBuild::Files("etc/elisp/alectryon.el"));
}

#[test]
fn runtime_tree_dependencies_preserve_recursive_source_and_precede_the_root_package() {
    let source = locked_melpa_source(("auctex", "14.1.2"))
        .expect("resolve the exact AUCTeX runtime source tree");
    assert_eq!(source.build(), SourceBuild::AuctexRuntime);
    assert_eq!(
        locked_melpa_install_plan(("auctex-cluttex", "20240519.1303"))
            .expect("resolve the auctex-cluttex dependency closure")
            .into_iter()
            .map(|source| source.package())
            .collect::<Vec<_>>(),
        [("auctex", "14.1.2"), ("auctex-cluttex", "20240519.1303")]
    );
}

#[test]
fn exact_source_install_plan_orders_dependencies_before_the_main_package() {
    let plan = locked_melpa_install_plan(("arxiv-citation", "20230713.627"))
        .expect("resolve the source-locked arxiv-citation dependency closure");
    let packages = plan
        .into_iter()
        .map(|source| source.package())
        .collect::<Vec<_>>();

    assert_eq!(
        packages,
        [
            ("dash", "20260221.1346"),
            ("s", "20220902.1511"),
            ("arxiv-citation", "20230713.627"),
        ]
    );
}

#[test]
fn pinned_package_requirements_are_available_before_the_requesting_package() {
    for package in [
        ("agitjo", "20260523.2048"),
        ("ai-code", "20260727.2322"),
        ("aider", "20251201.133"),
        ("aidermacs", "20260726.839"),
        ("casual", "20260718.1803"),
        ("magit", "20260724.2338"),
    ] {
        let plan = locked_melpa_install_plan(package)
            .unwrap_or_else(|error| panic!("resolve {} {}: {error}", package.0, package.1));
        let transient_index = plan
            .iter()
            .position(|source| source.package() == ("transient", "20260725.1105"))
            .unwrap_or_else(|| {
                panic!(
                    "{} {} requires transient in its pinned Package-Requires header",
                    package.0, package.1
                )
            });
        let package_index = plan
            .iter()
            .position(|source| source.package() == package)
            .expect("the requested package must end its own install plan");

        assert!(
            transient_index < package_index,
            "transient must be installed before {} {}",
            package.0,
            package.1
        );
    }
}

#[test]
fn every_exact_package_has_a_complete_acyclic_source_plan() {
    let sources = locked_melpa_sources().expect("parse the source lock");
    assert_eq!(
        sources.len(),
        797,
        "every root package, exact dependency, and legacy all-ext dependency stays pinned"
    );

    for source in sources {
        let package = source.package();
        let plan = locked_melpa_install_plan(package)
            .unwrap_or_else(|error| panic!("resolve {} {}: {error}", package.0, package.1));
        assert_eq!(
            plan.last().map(|planned| planned.package()),
            Some(package),
            "the selected package must be installed after its dependencies"
        );
        assert!(!source.repository().contains("melpa.org"));
        assert!(!source.repository().contains("/releases/download/"));
        if let Some(fallback) = source.fallback_repository() {
            assert!(
                fallback.starts_with("https://github.com/emacsmirror/")
                    || fallback.starts_with("https://github.com/emacsattic/")
            );
            assert!(!fallback.contains("/releases/download/"));
        }
    }

    let all_ext_plan = locked_melpa_install_plan(("all-ext", "20200315.1443"))
        .expect("resolve the legacy source dependency");
    assert_eq!(
        all_ext_plan
            .into_iter()
            .map(|source| source.package())
            .collect::<Vec<_>>(),
        [("all", "1.0"), ("all-ext", "20200315.1443")]
    );
}

#[test]
fn git_source_acquisition_is_shallow_and_never_reads_a_package_catalog() {
    let source_harness = include_str!(concat!(
        env!("CARGO_WORKSPACE_DIR"),
        "/crates/neomacs-melpa-test-support/src/source_lock.rs"
    ));

    assert_eq!(SHALLOW_GIT_FETCH_ARGS, ["fetch", "--depth=1", "--no-tags"]);
    assert!(source_harness.contains("--is-shallow-repository"));
    assert!(!source_harness.contains("package-refresh-contents"));
    assert!(!source_harness.contains("package-archive-contents"));
    assert!(!source_harness.contains("url-copy-file"));
    assert!(!source_harness.contains("melpa.org/packages"));
}

#[cfg(unix)]
#[test]
fn scenario_timeout_identifies_the_stalled_phase() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = MelpaSandbox::new("timeout-contract").expect("create fixture sandbox");
    let runtime_script = fixture.root().join("slow-emacs");
    fs::write(
        &runtime_script,
        r##"#!/bin/sh
exec sleep 5
"##,
    )
    .expect("write deliberately slow runtime");
    fs::set_permissions(&runtime_script, fs::Permissions::from_mode(0o755))
        .expect("make slow runtime executable");

    let runtime = EmacsRuntime::new("slow", runtime_script).with_timeout(Duration::from_millis(50));
    let source =
        PackageSource::frozen(workspace_root().join("test/lisp/emacs-lisp/package-resources"));
    let scenario = PackageScenario::new("timeout-contract", ["simple-single"], "t");

    let error = run_scenario(&runtime, &source, &scenario).expect_err("scenario must time out");
    assert!(
        error.contains("Install"),
        "error did not name phase: {error}"
    );
    assert!(
        error.contains("timed out"),
        "error did not name cause: {error}"
    );
}

#[cfg(unix)]
#[test]
fn scenario_error_markers_identify_runtime_scenario_and_phase() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = MelpaSandbox::new("error-marker-contract").expect("create fixture sandbox");
    let runtime_script = fixture.root().join("error-emacs");
    fs::write(
        &runtime_script,
        r##"#!/bin/sh
printf '%s\n' 'Error: deliberate package failure'
printf 'NEOMACS-MELPA-INSTALLED:simple-single\t1.3\n'
"##,
    )
    .expect("write failing fake runtime");
    fs::set_permissions(&runtime_script, fs::Permissions::from_mode(0o755))
        .expect("make failing runtime executable");

    let runtime = EmacsRuntime::new("error-runtime", runtime_script);
    let source =
        PackageSource::frozen(workspace_root().join("test/lisp/emacs-lisp/package-resources"));
    let scenario = PackageScenario::new("error-marker-contract", ["simple-single"], "t");

    let error = run_scenario(&runtime, &source, &scenario).expect_err("scenario must fail");
    for expected in [
        "error-runtime",
        "error-marker-contract",
        "Install",
        "Error:",
    ] {
        assert!(
            error.contains(expected),
            "error did not contain `{expected}`: {error}"
        );
    }
}

#[cfg(unix)]
#[test]
fn ert_scenario_forwards_the_test_file_and_selector() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = MelpaSandbox::new("ert-runtime-contract").expect("create fixture sandbox");
    let invocation_log = fixture.root().join("ert-invocation");
    let runtime_script = fixture.root().join("fake-ert-emacs");
    fs::write(
        &runtime_script,
        format!(
            r##"#!/bin/sh
printf '%s\n' "$*" > '{}'
printf '%s\n' 'Ran 3 tests, 3 results as expected, 0 unexpected, 1 skipped' >&2
"##,
            invocation_log.display()
        ),
    )
    .expect("write fake ERT runtime");
    fs::set_permissions(&runtime_script, fs::Permissions::from_mode(0o755))
        .expect("make fake ERT runtime executable");

    let test_file = workspace_root().join("test/lisp/emacs-lisp/package-tests.el");
    let scenario = ErtScenario::new(
        "upstream-install-contract",
        &test_file,
        r##"'(member package-test-install-single package-test-install-file)"##,
    );
    let report = run_ert_scenario(&EmacsRuntime::new("fake-ert", runtime_script), &scenario)
        .expect("run fake ERT scenario");

    let invocation = fs::read_to_string(invocation_log).expect("read ERT invocation");
    assert!(invocation.contains(&format!("-l {}", test_file.display())));
    assert!(invocation.contains("ert-run-tests-batch"));
    assert!(invocation.contains("package-test-install-single"));
    assert_eq!(report.phase.phase, ScenarioPhase::Ert);
    assert_eq!(report.summary.total, 3);
    assert_eq!(report.summary.expected, 3);
    assert_eq!(report.summary.unexpected, 0);
    assert_eq!(report.summary.skipped, 1);
}
