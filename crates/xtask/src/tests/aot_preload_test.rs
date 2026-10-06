use crate::*;

fn options(args: &[&str]) -> FreshBuildOptions {
    FreshBuildOptions::parse(repository_root(), args.iter().map(OsString::from)).unwrap()
}

fn artifact_fixture() -> (tempfile::TempDir, FreshBuildOptions, PipelinePaths) {
    let scratch = repository_root().join("tmp");
    fs::create_dir_all(&scratch).unwrap();
    let dir = tempfile::Builder::new()
        .prefix("xtask-aot-preload-")
        .tempdir_in(scratch)
        .unwrap();
    let mut options = options(&["--release", "--aot-preload"]);
    options.bin_dir = dir.path().to_path_buf();
    let paths = pipeline_paths(&options);
    (dir, options, paths)
}

fn write_artifacts(paths: &PipelinePaths) {
    for artifact in aot_preload_artifact_paths(paths) {
        fs::write(artifact, b"fixture").unwrap();
    }
}

#[test]
fn aot_preload_defaults_off_and_can_be_enabled() {
    let default = options(&["--release"]);
    assert_eq!(default.aot_preload, AotPreloadMode::Disabled);
    assert!(!default.aot_preload.enabled());
    assert_eq!(default.profile, BuildProfile::Release);
    assert!(!default.dry_run);
    assert!(!default.skip_build);

    let explicit = options(&["--release", "--aot-preload"]);
    assert_eq!(explicit.aot_preload, AotPreloadMode::Explicit);
    assert!(explicit.aot_preload.enabled());

    let skipped = options(&["--release", "--no-aot-preload"]);
    assert_eq!(skipped.aot_preload, AotPreloadMode::Disabled);
    assert!(!skipped.aot_preload.enabled());
    assert_eq!(skipped.profile, default.profile);
}

#[test]
fn aot_preload_last_explicit_flag_selects_mode() {
    assert_eq!(
        options(&["--release", "--aot-preload", "--no-aot-preload"]).aot_preload,
        AotPreloadMode::Disabled
    );
    assert_eq!(
        options(&["--release", "--no-aot-preload", "--aot-preload"]).aot_preload,
        AotPreloadMode::Explicit
    );
    let explicit = options(&["--release", "--aot-preload", "--dry-run"]);
    assert_eq!(explicit.aot_preload, AotPreloadMode::Explicit);
    assert!(explicit.dry_run);
}

#[test]
fn aot_preload_ordinary_dry_run_does_not_execute_or_verify_dump() {
    let (_dir, mut options, paths) = artifact_fixture();
    options.aot_preload = AotPreloadMode::Disabled;
    options.dry_run = true;
    options.skip_build = true;
    options.no_byte_compile = true;
    assert!(!paths.temacs.exists());

    run_fresh_build_inner(&options, &[]).unwrap();
    for artifact in aot_preload_artifact_paths(&paths) {
        assert!(!artifact.exists());
    }
}

#[test]
fn aot_preload_explicit_dry_run_retains_candidate_enumeration() {
    let (_dir, mut options, paths) = artifact_fixture();
    options.aot_preload = AotPreloadMode::Explicit;
    options.dry_run = true;
    options.skip_build = true;
    options.no_byte_compile = true;
    assert!(!paths.temacs.exists());

    let error = run_fresh_build_inner(&options, &[])
        .unwrap_err()
        .to_string();
    assert!(error.contains("aot-preload dry-run: missing"), "{error}");
}

#[test]
fn aot_preload_dump_environment_requires_opt_in_and_honors_skip() {
    let base = [(
        OsString::from("NEOMACS_RUNTIME_ROOT"),
        OsString::from("/runtime"),
    )];
    let enabled = final_dump_envs(&options(&["--release", "--aot-preload"]), &base);
    assert_eq!(
        enabled,
        [
            base[0].clone(),
            (OsString::from(AOT_PRELOAD_ENV), OsString::from("1")),
        ]
    );
    assert_eq!(final_dump_envs(&options(&["--release"]), &base), base);
    assert_eq!(
        final_dump_envs(&options(&["--release", "--no-aot-preload"]), &base),
        base
    );
}

#[test]
fn aot_preload_child_environment_discards_inherited_producer_flags() {
    let mut command = Command::new("neomacs-temacs");
    command
        .env(AOT_PRELOAD_ENV, "0")
        .env(AOT_PRELOAD_DRY_RUN_ENV, "1")
        .env("EMACSLOADPATH", "/user/packages")
        .env("UNRELATED", "retained");
    configure_fresh_build_environment(&mut command, &[]);
    let envs: BTreeMap<_, _> = command.get_envs().collect();
    assert_eq!(envs.get(OsStr::new(AOT_PRELOAD_ENV)), Some(&None));
    assert_eq!(envs.get(OsStr::new(AOT_PRELOAD_DRY_RUN_ENV)), Some(&None));
    assert_eq!(envs.get(OsStr::new("EMACSLOADPATH")), Some(&None));
    assert_eq!(
        envs.get(OsStr::new("UNRELATED")),
        Some(&Some(OsStr::new("retained")))
    );

    let dump_envs = final_dump_envs(&options(&["--release", "--aot-preload"]), &[]);
    configure_fresh_build_environment(&mut command, &dump_envs);
    let envs: BTreeMap<_, _> = command.get_envs().collect();
    assert_eq!(
        envs.get(OsStr::new(AOT_PRELOAD_ENV)),
        Some(&Some(OsStr::new("1")))
    );
    assert_eq!(envs.get(OsStr::new(AOT_PRELOAD_DRY_RUN_ENV)), Some(&None));
}

#[test]
fn aot_preload_verifier_accepts_both_files_beside_binary() {
    let (_dir, _options, paths) = artifact_fixture();
    write_artifacts(&paths);
    verify_aot_preload_artifacts(&paths).unwrap();
}

#[test]
fn aot_preload_verifier_requires_shared_object() {
    let (_dir, _options, paths) = artifact_fixture();
    let [so, manifest] = aot_preload_artifact_paths(&paths);
    fs::write(manifest, b"fixture").unwrap();
    let error = verify_aot_preload_artifacts(&paths)
        .unwrap_err()
        .to_string();
    assert!(error.contains(&so.display().to_string()), "{error}");
}

#[test]
fn aot_preload_verifier_requires_manifest() {
    let (_dir, _options, paths) = artifact_fixture();
    let [so, manifest] = aot_preload_artifact_paths(&paths);
    fs::write(so, b"fixture").unwrap();
    let error = verify_aot_preload_artifacts(&paths)
        .unwrap_err()
        .to_string();
    assert!(error.contains(&manifest.display().to_string()), "{error}");
}

#[test]
fn aot_preload_verifier_rejects_artifact_directories() {
    let (_dir, _options, paths) = artifact_fixture();
    write_artifacts(&paths);
    for artifact in aot_preload_artifact_paths(&paths) {
        fs::remove_file(&artifact).unwrap();
        fs::create_dir(&artifact).unwrap();
        let error = verify_aot_preload_artifacts(&paths)
            .unwrap_err()
            .to_string();
        assert!(error.contains(&artifact.display().to_string()), "{error}");
        fs::remove_dir(&artifact).unwrap();
        fs::write(artifact, b"fixture").unwrap();
    }
}

#[test]
fn aot_preload_prepare_removes_only_old_preload_files() {
    let (_dir, options, paths) = artifact_fixture();
    write_artifacts(&paths);
    fs::write(&paths.final_bin, b"binary").unwrap();
    let pdump = paths.final_bin.with_extension("pdump");
    fs::write(&pdump, b"dump").unwrap();

    prepare_aot_preload_artifacts(&options, &paths).unwrap();
    for artifact in aot_preload_artifact_paths(&paths) {
        assert!(!artifact.exists());
    }
    assert_eq!(fs::read(&paths.final_bin).unwrap(), b"binary");
    assert_eq!(fs::read(pdump).unwrap(), b"dump");
    let error = verify_aot_preload_artifacts(&paths)
        .unwrap_err()
        .to_string();
    assert!(error.contains("expected artifact not found"), "{error}");
}

#[test]
fn aot_preload_prepare_preserves_artifacts_when_skipped_or_dry_run() {
    let (_dir, mut options, paths) = artifact_fixture();
    write_artifacts(&paths);
    options.aot_preload = AotPreloadMode::Disabled;
    prepare_aot_preload_artifacts(&options, &paths).unwrap();
    options.aot_preload = AotPreloadMode::Explicit;
    options.dry_run = true;
    prepare_aot_preload_artifacts(&options, &paths).unwrap();
    for artifact in aot_preload_artifact_paths(&paths) {
        assert_eq!(fs::read(artifact).unwrap(), b"fixture");
    }
}

#[test]
fn aot_preload_prepare_refuses_to_remove_an_artifact_directory() {
    let (_dir, options, paths) = artifact_fixture();
    let [so, _manifest] = aot_preload_artifact_paths(&paths);
    fs::create_dir(&so).unwrap();
    let child = so.join("keep");
    fs::write(&child, b"retained").unwrap();
    assert!(prepare_aot_preload_artifacts(&options, &paths).is_err());
    assert_eq!(fs::read(child).unwrap(), b"retained");
}
