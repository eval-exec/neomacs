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
    // A case that deliberately selects another locale says so with the typed
    // override; it is part of the expectation, not a hole in the layer.
    command.override_pin(CanonicalPin::Locale, "C");
    assert_eq!(env_value(&command, "LANG"), Some(OsString::from("C")));
    assert_eq!(env_value(&command, "LC_ALL"), Some(OsString::from("C")));
}

/// Rule 1: a raw `env` may not overwrite a pin; the panic names the variable
/// and the typed override to use instead.
#[test]
fn raw_env_refuses_a_pinned_name() {
    let environment = oracle_env();
    let mut command = environment.command("/bin/emacs");
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        command.env("TMPDIR", "/tmp/elsewhere");
    }))
    .expect_err("env must refuse a pinned name");
    let message = panic_message(&panic);
    assert!(
        message.contains("TMPDIR"),
        "panic did not name TMPDIR: {message}"
    );
    assert!(
        message.contains("override_pin(CanonicalPin::TmpDir"),
        "panic did not point at override_pin: {message}"
    );
    // The pinned value is untouched.
    assert_eq!(
        env_value(&command, "TMPDIR"),
        Some(OsString::from("/tmp/nv-tmp"))
    );
}

/// Rule 2: the bulk `envs` form cannot silently undo the pins either.
#[test]
fn raw_envs_refuses_a_pinned_name() {
    let environment = oracle_env();
    let mut command = environment.command("/bin/emacs");
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        command.envs([("A_CASE_VAR", "ok"), ("LC_ALL", "C")]);
    }))
    .expect_err("envs must refuse a pinned name");
    let message = panic_message(&panic);
    assert!(
        message.contains("LC_ALL"),
        "panic did not name LC_ALL: {message}"
    );
    assert!(
        message.contains("override_pin(CanonicalPin::Locale"),
        "panic did not point at override_pin: {message}"
    );
}

/// Rule 3: removing a pin is an override too, and is refused.
#[test]
fn raw_env_remove_refuses_a_pinned_name() {
    let environment = oracle_env();
    let mut command = environment.command("/bin/emacs");
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        command.env_remove("HOME");
    }))
    .expect_err("env_remove must refuse a pinned name");
    let message = panic_message(&panic);
    assert!(
        message.contains("HOME"),
        "panic did not name HOME: {message}"
    );
}

/// Rule 4: `override_pin` is the deliberate statement, and it sets every
/// environment variable the pin owns.
#[test]
fn override_pin_sets_every_name_of_the_pin() {
    let environment = oracle_env();
    let mut command = environment.command("/bin/emacs");
    command.override_pin(CanonicalPin::TmpDir, "/tmp/case-tmp");
    for name in ["TMPDIR", "TMP", "TEMP"] {
        assert_eq!(
            env_value(&command, name),
            Some(OsString::from("/tmp/case-tmp"))
        );
    }
    command.override_pin(CanonicalPin::LoginName, "case-user");
    for name in ["USER", "LOGNAME"] {
        assert_eq!(env_value(&command, name), Some(OsString::from("case-user")));
    }
    command.override_pin(CanonicalPin::TimeZone, "UTC");
    assert_eq!(env_value(&command, "TZ"), Some(OsString::from("UTC")));
}

/// Rule 5: non-pinned per-case variables are still ordinary `env` entries.
#[test]
fn raw_env_allows_unpinned_names() {
    let environment = oracle_env();
    let mut command = environment.command("/bin/emacs");
    command
        .env("NEOVM_ORACLE_FORM_FILE", "/tmp/form.el")
        .env_remove("LD_PRELOAD")
        .envs([("FAKETIME", "@2026-06-15 12:00:00")]);
    assert_eq!(
        env_value(&command, "NEOVM_ORACLE_FORM_FILE"),
        Some(OsString::from("/tmp/form.el"))
    );
    assert_eq!(
        env_value(&command, "FAKETIME"),
        Some(OsString::from("@2026-06-15 12:00:00"))
    );
}

/// The typed pin set is closed over what the profiles actually pin: every
/// environment pair a profile emits is a `CanonicalPin` name, every removed
/// variable is not one, and no two pins own the same name.
#[test]
fn every_profile_pair_is_a_typed_pin() {
    let package = CanonicalEnvironment::package(
        "/tmp/mh",
        "/tmp/mt",
        PackagePaths {
            scratch_root: "/workspace/case".into(),
            workspace_root: "/workspace".into(),
            runtime_dir: "/workspace/runtime".into(),
            melpa_cache_root: "/workspace/target/melpa".into(),
        },
    )
    .expect("package environment");
    let tui = CanonicalEnvironment::new(CanonicalProfile::Tui, "/tmp/th", "/tmp/tt", "/workspace")
        .expect("tui environment");
    for environment in [oracle_env(), package, tui] {
        for (name, _) in environment.env_pairs() {
            CanonicalPin::from_env_name(name)
                .unwrap_or_else(|| panic!("{name} is pinned but has no CanonicalPin"));
        }
        for removed in environment.removed_vars() {
            assert!(
                CanonicalPin::from_env_name(removed).is_none(),
                "removed variable {removed} must not be a pin"
            );
        }
    }
    // Every declared name is claimed by exactly one pin group.
    let mut names = std::collections::HashSet::new();
    for pin in [
        CanonicalPin::Locale,
        CanonicalPin::TimeZone,
        CanonicalPin::TmpDir,
        CanonicalPin::Home,
        CanonicalPin::LoginName,
        CanonicalPin::FullName,
        CanonicalPin::HostName,
        CanonicalPin::Email,
        CanonicalPin::Term,
        CanonicalPin::OracleSystemName,
        CanonicalPin::GitCeilingDirectories,
        CanonicalPin::RustBacktrace,
        CanonicalPin::PythonNoDebugRanges,
        CanonicalPin::XdgConfigHome,
        CanonicalPin::XdgCacheHome,
        CanonicalPin::XdgDataHome,
        CanonicalPin::XdgStateHome,
        CanonicalPin::XdgRuntimeDir,
        CanonicalPin::TestSandboxRoot,
        CanonicalPin::TestWorkspaceRoot,
        CanonicalPin::TestScratchRoot,
        CanonicalPin::MimeTypes,
    ] {
        for name in pin.names() {
            assert!(names.insert(*name), "{name} is owned by two pins");
            assert_eq!(CanonicalPin::from_env_name(name), Some(pin));
        }
    }
}

/// The typed memory operation really sets RLIMIT_AS in the child; the crate
/// offers no general pre-exec closure that could do something else.
#[cfg(target_os = "linux")]
#[test]
fn limit_address_space_sets_the_child_rlimit() {
    let environment = oracle_env();
    let limit = 512 * 1024 * 1024u64;
    let mut command = environment.command("/bin/sh");
    let output = command
        .args(["-c", "ulimit -v"])
        .limit_address_space(limit)
        .output()
        .expect("spawn sh under the address-space limit");
    assert!(output.status.success(), "sh failed: {output:?}");
    let reported_kb: u64 = String::from_utf8(output.stdout)
        .expect("ulimit output is ASCII")
        .trim()
        .parse()
        .expect("ulimit -v prints a number");
    assert_eq!(reported_kb * 1024, limit);
}

/// The typed affinity operation really restricts the child to the requested
/// CPU, and it builds the CPU set in the parent.
#[cfg(target_os = "linux")]
#[test]
fn limit_to_cpus_restricts_the_child() {
    let mut allowed: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    // Safety: sched_getaffinity writes only into the stack-local set.
    assert_eq!(
        unsafe {
            libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &raw mut allowed)
        },
        0
    );
    let cpu = (0..libc::CPU_SETSIZE as usize)
        .find(|cpu| unsafe { libc::CPU_ISSET(*cpu, &allowed) })
        .expect("the test process has an allowed CPU");
    let environment = oracle_env();
    let mut command = environment.command("/bin/sh");
    let output = command
        .args(["-c", "grep Cpus_allowed_list /proc/self/status"])
        .limit_to_cpus(&[cpu])
        .output()
        .expect("spawn sh pinned to one CPU");
    assert!(output.status.success(), "grep failed: {output:?}");
    let text = String::from_utf8(output.stdout).expect("proc status is ASCII");
    let list = text
        .split_once(':')
        .expect("Cpus_allowed_list has a colon")
        .1
        .trim();
    assert_eq!(list, cpu.to_string(), "child allowed {list}, wanted {cpu}");
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = panic.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    "<non-string panic>".to_string()
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

/// A symlinked base is resolved before any case directory is created, so a
/// truename and the case root share one spelling (Darwin `/tmp` ->
/// `/private/tmp`).  Linux's `/tmp` is canonical and is unchanged.
#[cfg(unix)]
#[test]
fn bounded_temp_base_resolves_a_symlinked_root() {
    let target = tempfile::tempdir_in("/tmp").expect("short canonical target");
    let link_parent = tempfile::tempdir_in("/tmp").expect("short link parent");
    let link = link_parent.path().join("link");
    std::os::unix::fs::symlink(target.path(), &link).expect("create base symlink");
    let base = bounded_temp_base_for(&link);
    let canonical = std::fs::canonicalize(target.path()).expect("canonical target");
    assert_eq!(base, canonical);
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
