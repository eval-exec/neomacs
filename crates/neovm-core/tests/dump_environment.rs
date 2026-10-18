//! Build images exclude host state; activation supplies the runtime environment.
use neovm_core::emacs_core::{
    format_eval_result,
    load::{
        LoadupDumpInvocation, LoadupDumpMode, LoadupInvocation, RuntimeImageRole,
        activate_runtime_evaluator, create_bootstrap_evaluator_for_loadup,
    },
    pdump::load_from_dump,
};
use std::{fs, path::PathBuf, process::Command};
use tempfile::tempdir;

#[test]
fn production_dump_does_not_capture_the_build_environment() {
    assert_dump_environment(
        LoadupDumpMode::Pdump,
        "production_dump_does_not_capture_the_build_environment",
    );
}

#[test]
fn bootstrap_dump_does_not_capture_the_build_environment() {
    assert_dump_environment(
        LoadupDumpMode::Pbootstrap,
        "bootstrap_dump_does_not_capture_the_build_environment",
    );
}

fn assert_dump_environment(mode: LoadupDumpMode, test_name: &str) {
    const MARKER: &str = "/nix/store/00000000000000000000000000000000-issue578-build-only";
    const CHILD: &str = "NEOMACS_DUMP_ENVIRONMENT_TEST_CHILD";
    let (dump_name, role) = match mode {
        LoadupDumpMode::Pdump => ("neomacs.pdump", RuntimeImageRole::Final),
        LoadupDumpMode::Pbootstrap => ("bootstrap-neomacs.pdump", RuntimeImageRole::Bootstrap),
    };
    if std::env::var_os(CHILD).is_none() {
        // Context initializes invocation-directory from the executable. Run
        // the real dump path in a private process and directory, without
        // mutating this test process's environment or build artifacts.
        let directory = tempdir().unwrap();
        let executable = directory
            .path()
            .join(format!("neomacs-temacs{}", std::env::consts::EXE_SUFFIX));
        fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        fs::write(
            directory
                .path()
                .join(format!("neomacs{}", std::env::consts::EXE_SUFFIX)),
            b"dump fixture",
        )
        .unwrap();
        let output = Command::new(&executable)
            .args(["--exact", test_name, "--nocapture"])
            .env(CHILD, "dump")
            .env("NEOMACS_ISSUE578_BUILD_ONLY", MARKER)
            .env(
                "PATH",
                std::env::join_paths(
                    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                        .chain(std::iter::once(PathBuf::from(MARKER))),
                )
                .unwrap(),
            )
            .env("SHELL", format!("{MARKER}/bin/sh"))
            .output()
            .expect("run isolated dump construction");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let bytes = fs::read(directory.path().join(dump_name)).unwrap();
        assert!(
            !bytes
                .windows(MARKER.len())
                .any(|window| window == MARKER.as_bytes()),
            "production dump retains a build-only environment value",
        );
        let runtime = Command::new(&executable)
            .args(["--exact", test_name, "--nocapture"])
            .env(CHILD, "runtime")
            .env_remove("NEOMACS_ISSUE578_BUILD_ONLY")
            .env("NEOMACS_ISSUE578_RUNTIME", "runtime")
            .env("NEOMACS_ISSUE578_RUNTIME_PATH", "issue578-runtime-bin")
            .env(
                "PATH",
                std::env::join_paths(
                    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                        .chain(std::iter::once(PathBuf::from("issue578-runtime-bin"))),
                )
                .unwrap(),
            )
            .env("SHELL", "issue578-runtime-shell")
            .output()
            .expect("activate the image in a different runtime environment");
        assert!(
            runtime.status.success(),
            "{}",
            String::from_utf8_lossy(&runtime.stderr)
        );
        return;
    }
    let temacs = std::env::current_exe().unwrap();
    let dump_path = temacs.parent().unwrap().join(dump_name);
    if std::env::var(CHILD).as_deref() == Ok("dump") {
        let invocation = LoadupInvocation::Dump(LoadupDumpInvocation::new(
            mode,
            vec![
                temacs.to_string_lossy().into_owned(),
                "--batch".into(),
                "-l".into(),
                "loadup".into(),
                format!("--temacs={}", mode.as_gnu_string()),
            ],
        ));
        let eval = create_bootstrap_evaluator_for_loadup(&[], &invocation)
            .expect("build the dump through loadup");
        assert_eq!(
            eval.shutdown_request().map(|request| request.exit_code),
            Some(0)
        );
        drop(eval);
        let mut restored = load_from_dump(&dump_path).expect("restore the dump");

        if mode == LoadupDumpMode::Pdump {
            let version = Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .output();
            if let Ok(version) = version
                && version.status.success()
            {
                let expected = String::from_utf8(version.stdout).unwrap();
                assert_eq!(
                    format_eval_result(&restored.eval_str("emacs-repository-version")),
                    format!("OK {:?}", expected.trim()),
                    "loadup must still be able to query Git before serializing",
                );
            }
        }
        // GNU 31.1 emacs.c guards set_initial_environment with !will_dump_p().
        assert_eq!(
            format_eval_result(
                &restored.eval_str("(and (null initial-environment) (null process-environment))",)
            ),
            "OK t"
        );
    } else {
        let mut restored = load_from_dump(&dump_path).expect("restore the dump");
        activate_runtime_evaluator(&mut restored, role).expect("activate the runtime environment");
        assert_eq!(
            format_eval_result(&restored.eval_str(
                r#"
          (and (equal (getenv "NEOMACS_ISSUE578_RUNTIME") "runtime")
               (null (getenv "NEOMACS_ISSUE578_BUILD_ONLY"))
               (not (eq initial-environment process-environment))
               (member (getenv "NEOMACS_ISSUE578_RUNTIME_PATH") exec-path)
               (equal shell-file-name "issue578-runtime-shell")
               (equal (getenv-internal "NEOMACS_ISSUE578_RUNTIME" initial-environment)
                      "runtime"))
        "#
            )),
            "OK t"
        );
        #[cfg(unix)]
        assert_eq!(
            format_eval_result(&restored.eval_str(
                r#"
          (with-temp-buffer
            (list (call-process "/bin/sh" nil t nil "-c" "printf %s \"$NEOMACS_ISSUE578_RUNTIME\"")
                  (buffer-string)))
        "#
            )),
            "OK (0 \"runtime\")"
        );
        // A restored runtime must not inherit the producer's construction
        // state: ordinary user redumps preserve their shell and search path.
        let redump = dump_path.with_file_name("runtime-redump.pdump");
        let form = format!(
            "(progn (setq noninteractive t) (dump-emacs-portable {:?}))",
            redump.to_string_lossy(),
        );
        assert_eq!(format_eval_result(&restored.eval_str(&form)), "OK nil");
        drop(restored);
        let mut redumped = load_from_dump(&redump).expect("restore an ordinary runtime redump");
        assert_eq!(
            format_eval_result(&redumped.eval_str(
                r#"(and (equal shell-file-name "issue578-runtime-shell")
                         (member "issue578-runtime-bin" exec-path) t)"#,
            )),
            "OK t",
        );
    }
}
