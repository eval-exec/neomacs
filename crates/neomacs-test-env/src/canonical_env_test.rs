use super::*;

fn oracle_env() -> CanonicalEnvironment {
    CanonicalEnvironment::new(
        CanonicalProfile::Oracle,
        "/tmp/nv-home",
        "/tmp/nv-tmp",
        "/workspace/checkout",
    )
    .expect("oracle environment")
}

fn env_value(command: &PinnedCommand, name: &str) -> Option<OsString> {
    command
        .command
        .get_envs()
        .find_map(|(key, value)| (key == name).then(|| value.map(OsStr::to_os_string)))
        .flatten()
}

/// Every host-keyed input the turn-8 audit named is pinned by the oracle
/// profile, with the values its expectations were recorded under.
#[test]
fn oracle_profile_pins_every_host_keyed_input() {
    let environment = oracle_env();
    let pairs: Vec<(String, String)> = environment
        .env_pairs()
        .into_iter()
        .map(|(name, value)| (name.to_string(), value.to_string_lossy().into_owned()))
        .collect();
    let want = [
        ("LANG", "en_US.UTF-8"),
        ("LC_ALL", "en_US.UTF-8"),
        ("TZ", "America/New_York"),
        ("HOME", "/tmp/nv-home"),
        ("TMPDIR", "/tmp/nv-tmp"),
        ("TMP", "/tmp/nv-tmp"),
        ("TEMP", "/tmp/nv-tmp"),
        ("USER", "exec"),
        ("LOGNAME", "exec"),
        ("NAME", ""),
        ("HOSTNAME", "oracle-host"),
        ("EMAIL", "exec@oracle-host"),
        ("TERM", "dumb"),
        ("NEOVM_ORACLE_SYSTEM_NAME", "oracle-host"),
        ("GIT_CEILING_DIRECTORIES", "/workspace/checkout"),
    ];
    for (name, value) in want {
        assert!(
            pairs
                .iter()
                .any(|(pinned, pinned_value)| pinned == name && pinned_value == value),
            "missing canonical pin {name}={value:?}; got {pairs:?}"
        );
    }
    assert_eq!(environment.removed_vars(), ["LANGUAGE", "EMACSLOADPATH"]);
}

#[test]
fn package_and_tui_profiles_keep_their_recorded_values() {
    let package = CanonicalEnvironment::package(
        "/tmp/mh",
        "/long/package/case/tmp",
        PackagePaths {
            scratch_root: "/workspace/case".into(),
            workspace_root: "/workspace".into(),
            runtime_dir: "/workspace/runtime".into(),
            melpa_cache_root: "/workspace/target/melpa".into(),
        },
    )
    .expect("package environment");
    let pairs = package.env_pairs();
    for (name, value) in [
        ("LANG", "C.UTF-8"),
        ("LC_ALL", "C.UTF-8"),
        ("TZ", "UTC"),
        ("USER", "melpa-test"),
        ("NAME", "melpa-test"),
        ("HOSTNAME", "melpa-host"),
        ("EMAIL", "melpa-test@melpa-host"),
        ("PYTHONNODEBUGRANGES", "1"),
    ] {
        assert!(
            pairs
                .iter()
                .any(|(pinned, pinned_value)| *pinned == name
                    && pinned_value.to_string_lossy() == value),
            "package profile lost {name}={value:?}"
        );
    }
    for (name, value) in [
        ("XDG_RUNTIME_DIR", "/workspace/runtime"),
        ("NEOMACS_TEST_SANDBOX_ROOT", "/workspace/case"),
        ("NEOMACS_TEST_WORKSPACE_ROOT", "/workspace"),
        ("NEOMACS_TEST_SCRATCH_ROOT", "/workspace/target/melpa"),
        (
            "MIMETYPES",
            "/workspace/crates/neomacs-melpa-test-support/fixtures/mime.types",
        ),
        ("GIT_CEILING_DIRECTORIES", "/workspace"),
    ] {
        assert!(
            pairs
                .iter()
                .any(|(pinned, pinned_value)| *pinned == name
                    && pinned_value.to_string_lossy() == value),
            "package profile lost {name}={value:?}"
        );
    }
    let tui = CanonicalEnvironment::new(CanonicalProfile::Tui, "/tmp/th", "/tmp/tt", "/workspace")
        .expect("tui environment");
    assert_eq!(tui.profile().locale(), "C.UTF-8");
    assert_eq!(tui.profile().full_name(), "");
}

/// A socket-bearing profile refuses a temp root that would overflow
/// `sun_path`; the package profile does not bind sockets in TMPDIR and keeps
/// its recorded (possibly deep) case root.
#[test]
fn temp_root_length_is_validated_only_where_sockets_bind_in_it() {
    let long = format!("/tmp/{}", "x".repeat(80));
    let error = CanonicalEnvironment::new(
        CanonicalProfile::Oracle,
        "/tmp/nv-home",
        long.as_str(),
        "/workspace",
    )
    .expect_err("oracle temp root must be bounded");
    assert!(matches!(error, CanonicalError::TempRootTooLong { .. }));
    CanonicalEnvironment::package(
        "/tmp/mh",
        long.as_str(),
        PackagePaths {
            scratch_root: "/workspace/case".into(),
            workspace_root: "/workspace".into(),
            runtime_dir: "/workspace/runtime".into(),
            melpa_cache_root: "/workspace/target/melpa".into(),
        },
    )
    .expect("package temp root is not socket-bearing");
    // Absolute roots are not optional: a relative TMPDIR would resolve against
    // whatever working directory the child inherited.
    assert!(matches!(
        CanonicalEnvironment::new(
            CanonicalProfile::Oracle,
            "/tmp/nv-home",
            "relative/tmp",
            "/workspace"
        ),
        Err(CanonicalError::NotAbsolute {
            field: "TMPDIR",
            ..
        })
    ));
}

#[test]
fn pinned_command_carries_pins_and_per_case_overrides_but_no_constructor() {
    let environment = oracle_env();
    let mut command = environment.command("/bin/emacs");
    // Pins are present.
    assert_eq!(
        env_value(&command, "TZ"),
        Some(OsString::from("America/New_York"))
    );
    assert_eq!(env_value(&command, "NAME"), Some(OsString::from("")));
    // Removed variables are marked removed rather than inherited.
    assert_eq!(
        command
            .command
            .get_envs()
            .find_map(|(key, value)| (key == "LANGUAGE").then_some(value)),
        Some(None)
    );
    // A case that deliberately selects another locale applies it after the
    // pins; it is part of the expectation, not a hole in the layer.
    command.env("LC_ALL", "C");
    assert_eq!(env_value(&command, "LC_ALL"), Some(OsString::from("C")));
}

/// The in-process form installs exactly the spawned environment's answers:
/// `getenv` values (through `process-environment`) and the identity variables
/// GNU derives from them.
#[test]
fn in_process_form_matches_the_spawned_environment() {
    let environment = oracle_env();
    let form = environment.lisp_environment_form();
    assert!(form.starts_with("(progn"));
    assert!(form.contains("(setenv \"NAME\" \"\")"));
    assert!(form.contains("(setenv \"TZ\" \"America/New_York\")"));
    assert!(form.contains("(setenv \"LANGUAGE\" nil)"));
    assert!(form.contains("(setenv \"EMACSLOADPATH\" nil)"));
    assert!(form.contains("(setq user-full-name \"\""));
    assert!(form.contains("user-login-name \"exec\""));
    assert!(form.contains("system-name \"oracle-host\""));
    assert!(form.contains("user-mail-address \"exec@oracle-host\""));
    assert!(form.contains("system-time-locale \"C\""));
    assert!(form.contains("(set-language-environment \"UTF-8\")"));
}

/// The bounded temp root never relies on a deep ambient TMPDIR.
#[test]
fn bounded_temp_root_is_short() {
    let directory = bounded_temp_dir("nv-test-").expect("bounded temp dir");
    let bytes = directory.path().as_os_str().len();
    assert!(
        bytes <= CanonicalEnvironment::MAX_TEMP_ROOT_BYTES,
        "bounded temp root is {bytes} bytes: {}",
        directory.path().display()
    );
}
