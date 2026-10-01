//! Matching default/debug runtime preparation for real Linux daemon tests.
use super::*;

const TEST_ARGS: &[&str] = &[
    "test",
    "--locked",
    "-p",
    "neomacs",
    "--test",
    "daemon_lifecycle",
];
const BOOTSTRAP_ARGS: &[&str] = &["--batch", "-l", "loadup", "--temacs=pbootstrap"];

pub(crate) fn run(repo_root: PathBuf, args: impl IntoIterator<Item = OsString>) -> Result<()> {
    if let Some(arg) = args.into_iter().next() {
        return Err(format!("test-daemon-lifecycle takes no arguments; found {arg:?}").into());
    }
    if !cfg!(target_os = "linux") {
        return Err("test-daemon-lifecycle currently verifies Linux only".into());
    }
    let options = FreshBuildOptions {
        bin_dir: default_bin_dir(&repo_root, &BuildProfile::Test),
        runtime_root: repo_root.clone(),
        repo_root,
        profile: BuildProfile::Test,
        production_capabilities: ProductionCapabilities::for_host()?,
        cargo_jobs: CargoJobBudget::Inherit,
        dry_run: false,
        native_comp: false,
        skip_build: false,
        no_byte_compile: true,
        features: Vec::new(),
        aot_preload: false,
    };
    // Never silently select an old final image over the freshly prepared
    // bootstrap. Use a dedicated CARGO_TARGET_DIR when testing a release tree.
    if options.bin_dir.exists() {
        for entry in fs::read_dir(&options.bin_dir)? {
            let path = entry?.path();
            if path.extension() == Some(OsStr::new("pdump"))
                && !path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| {
                        name == "bootstrap-neomacs.pdump" || name.starts_with("bootstrap-neomacs-")
                    })
            {
                return Err(format!(
                    "conflicting runtime image {}; use a clean CARGO_TARGET_DIR",
                    path.display()
                )
                .into());
            }
        }
    }
    let paths = pipeline_paths(&options);
    ensure_runtime_inputs(&paths)?;
    // Generate before Cargo compilation, not between its two invocations:
    // neomacs/build.rs watches Lisp inputs and would rebuild the image owner.
    run_early_international_generation(&options, &paths)?;
    run_update_subdirs(&options, &paths)?;
    let envs = vec![(
        OsString::from("NEOMACS_RUNTIME_ROOT"),
        options.runtime_root.as_os_str().to_owned(),
    )];
    let cargo = Path::new("cargo");
    let mut prepare = os_args(TEST_ARGS);
    prepare.push(OsString::from("--no-run"));
    run_command(&options, &options.repo_root, cargo, &prepare, &envs)?;
    copy_executable_role_image(&paths.final_bin, &paths.temacs)?;
    let image = options.bin_dir.join("bootstrap-neomacs.pdump");
    // Prove this invocation produced the image, rather than retaining a stale
    // bootstrap after a producer which returned success without dumping.
    remove_file_if_exists(&image)?;
    let home = tempfile::Builder::new().prefix("dl-").tempdir()?;
    let mut bootstrap_env = envs.clone();
    for key in [
        "HOME",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
    ] {
        bootstrap_env.push((OsString::from(key), home.path().as_os_str().to_owned()));
    }
    bootstrap_env.push((
        OsString::from("NEOMACS_LOG_FILE"),
        home.path().join("bootstrap.log").into_os_string(),
    ));
    run_command(
        &options,
        &options.repo_root,
        &paths.temacs,
        &os_args(BOOTSTRAP_ARGS),
        &bootstrap_env,
    )?;
    if fs::metadata(&image)?.len() == 0 {
        return Err("native bootstrap produced an empty runtime image".into());
    }
    let smoke = vec![
        OsString::from("--batch"),
        OsString::from("-Q"),
        OsString::from("--dump-file"),
        image.into_os_string(),
        OsString::from("--eval"),
        OsString::from("(unless (= (+ 20 22) 42) (kill-emacs 1))"),
    ];
    run_command(
        &options,
        &options.repo_root,
        &paths.final_bin,
        &smoke,
        &bootstrap_env,
    )?;
    let before = Sha256::digest(fs::read(&paths.final_bin)?);
    let mut test = os_args(TEST_ARGS);
    test.extend(os_args(&["--", "--test-threads=1"]));
    run_command(&options, &options.repo_root, cargo, &test, &envs)?;
    if before != Sha256::digest(fs::read(&paths.final_bin)?) {
        return Err("Cargo changed the editor after runtime preparation".into());
    }
    Ok(())
}

fn os_args(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_uses_matching_debug_target_and_serial_full_suite() {
        assert_eq!(BuildProfile::Test.target_subdir(), "debug");
        assert_eq!(
            TEST_ARGS,
            [
                "test",
                "--locked",
                "-p",
                "neomacs",
                "--test",
                "daemon_lifecycle"
            ]
        );
        assert_eq!(
            BOOTSTRAP_ARGS,
            ["--batch", "-l", "loadup", "--temacs=pbootstrap"]
        );
        assert!(usage_text().contains("cargo xtask test-daemon-lifecycle"));
    }

    #[test]
    fn options_are_rejected_before_runtime_mutation() {
        let root = tempfile::tempdir().unwrap();
        let error = run(root.path().to_owned(), [OsString::from("--release")]).unwrap_err();
        assert!(error.to_string().contains("takes no arguments"));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
