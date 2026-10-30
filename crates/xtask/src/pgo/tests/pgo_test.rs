use super::*;

#[test]
fn skip_build_never_selects_training_even_for_a_consuming_profile() {
    for name in ["release-pgo", "release-pgo-profiling"] {
        let options = FreshBuildOptions::parse(
            PathBuf::from("/repo"),
            ["--profile", name, "--skip-build"].map(OsString::from),
        )
        .unwrap();
        assert_eq!(
            FreshBuildOperation::for_options(&options),
            FreshBuildOperation::ExistingProduct
        );
    }
}

#[test]
fn native_training_rejects_cross_and_non_linux_hosts() {
    assert!(validate_native_host(HOST, None).is_ok());
    assert!(validate_native_host(HOST, Some(OsStr::new(HOST))).is_ok());
    assert!(validate_native_host(HOST, Some(OsStr::new("aarch64-unknown-linux-gnu"))).is_err());
    assert!(validate_native_host("aarch64-apple-darwin", None).is_err());
    assert!(validate_native_host("x86_64-pc-windows-msvc", None).is_err());
}

#[test]
fn profdata_lookup_uses_the_actual_host_sysroot() {
    assert_eq!(
        profdata_path(Path::new("/toolchain"), HOST),
        Path::new("/toolchain/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata")
    );
}

#[cfg(unix)]
#[test]
fn failed_or_empty_rustc_queries_cannot_select_a_tool() {
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;
    let failed = Output {
        status: ExitStatus::from_raw(1 << 8),
        stdout: b"fake-sysroot\n".to_vec(),
        stderr: b"query failed".to_vec(),
    };
    assert!(checked_query_output(Path::new("rustc"), &["--print", "sysroot"], failed).is_err());
    let empty = Output {
        status: ExitStatus::from_raw(0),
        stdout: b"\n".to_vec(),
        stderr: vec![],
    };
    assert!(checked_query_output(Path::new("rustc"), &["--print", "host-tuple"], empty).is_err());
}

#[test]
fn encoded_flags_preserve_spaces_and_override_plain_flags() {
    let flags = RustFlags::parse(
        Some("-Clink-arg=/a path/lib.a\u{1f}-Ctarget-cpu=x86-64"),
        Some("-Cpanic=abort"),
        None,
    )
    .unwrap();
    assert!(flags.0.contains(&"-Clink-arg=/a path/lib.a".to_owned()));
    assert!(!flags.0.iter().any(|s| s.contains("panic=abort")));
    let use_flags = flags.with_treatment("profile-use", Path::new("/new profile/merged.profdata"));
    assert!(
        use_flags
            .split('\u{1f}')
            .any(|s| s == "-Cprofile-use=/new profile/merged.profdata")
    );
    assert!(use_flags.contains("align-all-functions=6"));
    assert!(!use_flags.contains("--emit-relocs"));
    assert!(use_flags.contains("--build-id"));
    // The shipping link must stay 2 MiB-congruent even though the encoded
    // flags replace config rustflags (text_huge_pages depends on it).
    assert!(use_flags.contains("-z,max-page-size=0x200000"));
    assert!(use_flags.contains("-z,common-page-size=0x200000"));
}

#[test]
fn profile_ownership_rejects_an_inherited_treatment_or_alignment_conflict() {
    assert!(RustFlags::parse(None, Some("-C profile-use=old.profdata"), None).is_err());
    assert!(RustFlags::parse(Some("-Cprofile-generate=/old"), None, None).is_err());
    assert!(RustFlags::parse(None, Some("-Cllvm-args=-align-all-functions=5"), None).is_err());
}

#[test]
fn default_and_vm_children_have_distinct_jit_and_raw_ownership() {
    for mode in TrainingMode::iter() {
        let env = ChildEnvironment::training(
            mode,
            Path::new("/runtime"),
            Path::new(mode.name()),
            Path::new("/work"),
        );
        let mut command = Command::new("not-executed");
        env.apply(&mut command);
        let effective: BTreeMap<_, _> = command
            .get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(OsStr::to_owned)))
            .collect();
        match mode {
            TrainingMode::Default => assert_eq!(effective[OsStr::new("NEOVM_JIT")], None),
            TrainingMode::Vm => assert_eq!(
                effective[OsStr::new("NEOVM_JIT")],
                Some(OsString::from("0"))
            ),
        }
        assert_eq!(
            effective[OsStr::new("NEOMACS_PGO_TRAIN_PASS")],
            Some(OsString::from(mode.name()))
        );
        assert!(
            effective[OsStr::new("LLVM_PROFILE_FILE")]
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .contains(mode.name())
        );
    }
}

#[test]
fn generation_and_consumer_have_matched_shipping_codegen_and_honest_profile_labels() {
    let generation = ChildEnvironment::cargo(
        "generate".into(),
        &BuildProfile::ReleasePgoGen,
        CodegenContract::Shipping,
    );
    let consumer = ChildEnvironment::cargo(
        "consume".into(),
        &BuildProfile::ReleasePgo,
        CodegenContract::Shipping,
    );
    assert_eq!(
        generation.set[OsStr::new("CARGO_PROFILE_RELEASE_PGO_GEN_DEBUG")],
        "0"
    );
    assert_eq!(
        consumer.set[OsStr::new("CARGO_PROFILE_RELEASE_PGO_DEBUG")],
        "0"
    );
    assert_eq!(
        generation.set[OsStr::new("NEOMACS_BUILD_PROFILE")],
        "release-pgo-gen"
    );
    assert_eq!(
        consumer.set[OsStr::new("NEOMACS_BUILD_PROFILE")],
        "release-pgo"
    );
    let diagnostic = ChildEnvironment::cargo(
        "generate".into(),
        &BuildProfile::ReleasePgoGen,
        CodegenContract::Diagnostic,
    );
    assert_eq!(
        diagnostic.set[OsStr::new("CARGO_PROFILE_RELEASE_PGO_GEN_DEBUG")],
        "2"
    );
}

#[test]
fn each_run_has_fresh_persistent_profile_paths_without_deleting_earlier_data() {
    let directory = tempfile::tempdir().unwrap();
    let first = RunPaths::new(directory.path()).unwrap();
    fs::write(first.preparation_raw.join("retained.profraw"), b"first").unwrap();
    let second = RunPaths::new(directory.path()).unwrap();
    assert_ne!(first.merged, second.merged);
    assert!(first.preparation_raw.join("retained.profraw").is_file());
    assert!(!second.merged.exists());
}

#[test]
fn merge_requires_both_exact_pass_inventories_and_excludes_preparation() {
    let directory = tempfile::tempdir().unwrap();
    let paths = RunPaths::new(directory.path()).unwrap();
    fs::write(paths.preparation_raw.join("prep.profraw"), b"not-training").unwrap();
    let mut passes = vec![];
    for mode in TrainingMode::iter() {
        let raw = paths.raw(mode);
        fs::create_dir_all(&raw).unwrap();
        fs::write(raw.join("owned.profraw"), mode.name()).unwrap();
        passes.push(json!({"mode":mode,"raw_inventory":inventory(&raw).unwrap()}));
    }
    let args = merge_arguments(&paths, &json!(passes)).unwrap();
    assert!(!args.iter().any(|p| p == paths.preparation_raw.as_os_str()));
    assert!(
        args.iter()
            .any(|p| p == paths.raw(TrainingMode::Default).as_os_str())
    );
    assert!(
        args.iter()
            .any(|p| p == paths.raw(TrainingMode::Vm).as_os_str())
    );
    assert!(merge_arguments(&paths, &json!([passes[0]])).is_err());
    fs::write(
        paths.raw(TrainingMode::Vm).join("owned.profraw"),
        b"changed",
    )
    .unwrap();
    assert!(merge_arguments(&paths, &json!(passes)).is_err());
}

#[test]
fn product_publication_keeps_native_outputs_and_includes_the_shipped_client() {
    let directory = tempfile::tempdir().unwrap();
    let native = directory.path().join("target/host/release-pgo");
    let public = directory.path().join("target/release-pgo");
    fs::create_dir_all(&native).unwrap();
    for name in ["neomacs", "neomacsclient"] {
        fs::write(native.join(crate::executable_name(name)), name).unwrap();
    }
    copy_products(&native, &public, Products::EditorAndClient).unwrap();
    for name in ["neomacs", "neomacsclient"] {
        assert_eq!(
            fs::read(native.join(crate::executable_name(name))).unwrap(),
            fs::read(public.join(crate::executable_name(name))).unwrap()
        );
    }
}

#[cfg(unix)]
#[test]
fn failed_owned_command_retains_its_real_exit_and_streams() {
    let directory = tempfile::tempdir().unwrap();
    let args = [
        OsString::from("-c"),
        OsString::from("printf failure >&2; exit 7"),
    ];
    assert!(
        recorded(
            directory.path(),
            "failure",
            directory.path(),
            Path::new("/bin/sh"),
            &args,
            &ChildEnvironment::default()
        )
        .is_err()
    );
    let row: Value =
        serde_json::from_slice(&fs::read(directory.path().join("failure-command.json")).unwrap())
            .unwrap();
    assert_eq!(row["returncode"], 7);
    assert_eq!(
        fs::read(directory.path().join("failure.err")).unwrap(),
        b"failure"
    );
}

#[test]
fn natural_training_removes_all_inherited_policy_and_diagnostic_controls() {
    let names = [
        "NEOVM_JIT",
        "NEOVM_JIT_T2_WORK",
        "NEOVM_GC_ALLOCATOR",
        "NEOMACS_PERF_DIAGNOSTICS",
        "NEOMACS_PGO_TRAIN_PASS",
        "NEOMACS_PGO_TTY",
        "LLVM_PROFILE_FILE",
        "PATH",
        "RUSTC",
    ];
    let removed = training_controls_to_remove(names.map(OsString::from));
    for name in names.into_iter().take(7) {
        assert!(removed.contains(&OsString::from(name)), "retained {name}");
    }
    assert!(!removed.contains(&OsString::from("PATH")));
    assert!(!removed.contains(&OsString::from("RUSTC")));
}

#[test]
fn vm_receipt_rejects_repeated_phases_and_self_consistent_wrong_checksums() {
    let directory = tempfile::tempdir().unwrap();
    let run = directory.path().join("run-owned");
    fs::create_dir(&run).unwrap();
    let phases = VmPhase::iter().map(|phase| json!({"phase":phase.name(),"status":"complete","result":{"calls":8192,"results_verified":true,"checksum":phase.checksum(),"expected_checksum":phase.checksum()}})).collect::<Vec<_>>();
    let mut receipt = json!({"status":"complete","pass":"vm","jit":"0","rounds":8192,"helpers":VmPhase::iter().count(),"phases":phases});
    let path = run.join("vm-result.json");
    write_json(&path, &receipt).unwrap();
    assert_eq!(
        check_training_receipts(directory.path(), TrainingMode::Vm)
            .unwrap()
            .len(),
        1
    );
    receipt["phases"][1] = receipt["phases"][0].clone();
    write_json(&path, &receipt).unwrap();
    assert!(check_training_receipts(directory.path(), TrainingMode::Vm).is_err());
    receipt["phases"][1]["phase"] = json!("vm-vectors");
    receipt["phases"][1]["result"]["checksum"] = json!(1);
    receipt["phases"][1]["result"]["expected_checksum"] = json!(1);
    write_json(&path, &receipt).unwrap();
    assert!(check_training_receipts(directory.path(), TrainingMode::Vm).is_err());
}

fn default_training_receipt_fixture(root: &Path) -> (PathBuf, Value) {
    let run = root.join("run-owned");
    fs::create_dir_all(run.join("tty")).unwrap();
    let phases = DefaultPhase::iter()
        .map(|phase| json!({"phase":phase.name(),"status":"complete"}))
        .collect::<Vec<_>>();
    let receipt = json!({"status":"complete","phases":phases});
    write_json(&run.join("batch-result.json"), &receipt).unwrap();
    write_json(
        &run.join("tty/driver-result.json"),
        &json!({"status":"complete","returncode":0,"commands":vec![json!({});104]}),
    )
    .unwrap();
    write_json(
        &run.join("tty/tty-result.json"),
        &json!({"status":"complete","commands":104,"buffers_restored":true}),
    )
    .unwrap();
    (run, receipt)
}

#[test]
fn default_receipt_requires_the_drained_tty_child_and_restored_buffers() {
    let directory = tempfile::tempdir().unwrap();
    let (run, _) = default_training_receipt_fixture(directory.path());
    assert_eq!(
        check_training_receipts(directory.path(), TrainingMode::Default)
            .unwrap()
            .len(),
        3
    );
    let tty = run.join("tty/tty-result.json");
    write_json(
        &tty,
        &json!({"status":"complete","commands":104,"buffers_restored":false}),
    )
    .unwrap();
    assert!(check_training_receipts(directory.path(), TrainingMode::Default).is_err());
    write_json(
        &tty,
        &json!({"status":"complete","commands":104,"buffers_restored":true}),
    )
    .unwrap();
    write_json(
        &run.join("tty/driver-result.json"),
        &json!({"status":"complete","returncode":1,"commands":vec![json!({});104]}),
    )
    .unwrap();
    assert!(check_training_receipts(directory.path(), TrainingMode::Default).is_err());
}

#[test]
fn default_receipt_rejects_duplicate_or_missing_retained_phases() {
    let directory = tempfile::tempdir().unwrap();
    let (run, mut receipt) = default_training_receipt_fixture(directory.path());
    assert!(check_training_receipts(directory.path(), TrainingMode::Default).is_ok());
    let original = receipt.clone();
    receipt["phases"][1] = receipt["phases"][0].clone();
    write_json(&run.join("batch-result.json"), &receipt).unwrap();
    assert!(check_training_receipts(directory.path(), TrainingMode::Default).is_err());
    receipt = original;
    receipt["phases"].as_array_mut().unwrap().remove(1);
    write_json(&run.join("batch-result.json"), &receipt).unwrap();
    assert!(check_training_receipts(directory.path(), TrainingMode::Default).is_err());
}

#[test]
fn preparation_and_training_use_the_same_module_pool_with_separate_owners() {
    let directory = tempfile::tempdir().unwrap();
    let paths = RunPaths::new(directory.path()).unwrap();
    assert_eq!(
        RunPaths::profile_file(&paths.preparation_raw),
        paths.preparation_raw.join("default_%4m.profraw")
    );
    for mode in TrainingMode::iter() {
        let raw = paths.raw(mode);
        let child =
            ChildEnvironment::training(mode, Path::new("/runtime"), &raw, &paths.work(mode));
        assert_eq!(
            child.set[OsStr::new("LLVM_PROFILE_FILE")],
            RunPaths::profile_file(&raw).into_os_string()
        );
        assert_ne!(
            RunPaths::profile_file(&raw),
            RunPaths::profile_file(&paths.preparation_raw)
        );
    }
}

fn cleanup_fixture(target: &Path) -> (TargetOwnership, RunPaths, Value, Value) {
    let owner = TargetOwnership::acquire(target).unwrap();
    let paths = RunPaths::new(target).unwrap();
    fs::write(paths.preparation_raw.join("prep.profraw"), b"preparation").unwrap();
    let mut passes = vec![];
    for mode in TrainingMode::iter() {
        let raw = paths.raw(mode);
        fs::create_dir_all(&raw).unwrap();
        fs::write(raw.join("train.profraw"), mode.name()).unwrap();
        let work = paths.work(mode).join("run-owned");
        fs::create_dir_all(&work).unwrap();
        let result = work.join(mode.result_name());
        fs::write(&result, b"retained unit-test trainer proof").unwrap();
        passes.push(json!({"mode":mode,"raw_inventory":inventory(&raw).unwrap(),"receipts":[identity(&result).unwrap()]}));
    }
    let passes = json!(passes);
    let args = merge_arguments(&paths, &passes).unwrap();
    fs::write(&paths.merged, b"unit-test merged output").unwrap();
    let command = json!({"returncode":0,"args":args.iter().map(|arg|arg.to_string_lossy().into_owned()).collect::<Vec<_>>()});
    (owner, paths, passes, command)
}

fn cleanup_receipt(proof: &VerifiedMerge) -> Value {
    json!({"merged_profile":proof.profile,"preparation_profiles":proof.inputs["preparation"],"training_passes":proof.inputs["training"]})
}

#[test]
fn verified_merge_prunes_only_generation_intermediates_and_keeps_sealed_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, paths, passes, command) = cleanup_fixture(directory.path());
    let earlier = RunPaths::new(directory.path()).unwrap();
    fs::write(
        earlier.preparation_raw.join("earlier.profraw"),
        b"keep earlier run",
    )
    .unwrap();
    let proof = VerifiedMerge::seal(&paths, &passes, &command).unwrap();
    for kind in DisposableArtifact::iter() {
        fs::create_dir_all(kind.path(&owner, &paths, &proof.training_fixture)).unwrap();
    }
    fs::write(
        proof.training_fixture.join("notes.txt"),
        b"discard scratch Git repository",
    )
    .unwrap();
    fs::write(
        paths.root.join("generation/raw-linked/neomacs"),
        b"instrumented ELF",
    )
    .unwrap();
    fs::write(
        paths.root.join("generation/staged/neomacs.pdump"),
        b"instrumented dump",
    )
    .unwrap();
    let log = paths.root.join("generation/binary-command.json");
    fs::write(&log, b"keep command evidence").unwrap();
    let training = paths.work(TrainingMode::Default).join("batch-result.json");
    fs::create_dir_all(training.parent().unwrap()).unwrap();
    fs::write(&training, b"keep trainer receipt").unwrap();
    let consumer = owner
        .target
        .join(BuildProfile::ReleasePgo.target_subdir())
        .join("neomacs");
    fs::create_dir_all(consumer.parent().unwrap()).unwrap();
    fs::write(&consumer, b"keep optimized product").unwrap();
    let mut receipt = cleanup_receipt(&proof);
    proof
        .prune(
            &owner,
            &paths,
            &mut receipt,
            &paths.root.join("manifest.json"),
        )
        .unwrap();
    for kind in DisposableArtifact::iter() {
        assert!(!kind.path(&owner, &paths, &proof.training_fixture).exists());
    }
    assert_eq!(fs::read(&consumer).unwrap(), b"keep optimized product");
    assert!(earlier.preparation_raw.join("earlier.profraw").is_file());
    assert!(log.is_file());
    assert!(training.is_file());
    assert!(
        proof
            .training_fixture
            .parent()
            .unwrap()
            .join(TrainingMode::Default.result_name())
            .is_file()
    );
    assert!(!proof.training_fixture.exists());
    assert_eq!(receipt["cleanup"]["status"], "complete");
    assert_eq!(
        receipt["cleanup"]["pre_prune_inventory"]["training"],
        passes
    );
    // End-of-consumption verification uses sealed evidence, not deleted raw paths.
    proof.verify_retained(&paths, &receipt).unwrap();
    fs::write(&paths.merged, b"changed profile").unwrap();
    assert!(proof.verify_retained(&paths, &receipt).is_err());
}

#[test]
fn failed_or_changed_merge_cannot_authorize_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, paths, passes, mut command) = cleanup_fixture(directory.path());
    command["returncode"] = json!(7);
    assert!(VerifiedMerge::seal(&paths, &passes, &command).is_err());
    assert!(paths.preparation_raw.join("prep.profraw").is_file());
    for mode in TrainingMode::iter() {
        assert!(paths.raw(mode).join("train.profraw").is_file());
    }
    command["returncode"] = json!(0);
    let proof = VerifiedMerge::seal(&paths, &passes, &command).unwrap();
    fs::write(&paths.merged, b"changed after merge").unwrap();
    let mut receipt = cleanup_receipt(&proof);
    assert!(
        proof
            .prune(
                &owner,
                &paths,
                &mut receipt,
                &paths.root.join("manifest.json")
            )
            .is_err()
    );
    assert!(paths.preparation_raw.join("prep.profraw").is_file());
    for mode in TrainingMode::iter() {
        assert!(paths.raw(mode).join("train.profraw").is_file());
    }
}

#[test]
fn changed_training_after_merge_preserves_every_raw_before_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, paths, passes, command) = cleanup_fixture(directory.path());
    let proof = VerifiedMerge::seal(&paths, &passes, &command).unwrap();
    fs::write(
        paths.raw(TrainingMode::Vm).join("train.profraw"),
        b"late writer",
    )
    .unwrap();
    let mut receipt = cleanup_receipt(&proof);
    assert!(
        proof
            .prune(
                &owner,
                &paths,
                &mut receipt,
                &paths.root.join("manifest.json")
            )
            .is_err()
    );
    assert!(paths.preparation_raw.join("prep.profraw").is_file());
    for mode in TrainingMode::iter() {
        assert!(paths.raw(mode).join("train.profraw").is_file());
    }
}

#[test]
fn cleanup_rejects_a_different_run_owner_before_deleting_anything() {
    let own = tempfile::tempdir().unwrap();
    let owner = TargetOwnership::acquire(own.path()).unwrap();
    let foreign = tempfile::tempdir().unwrap();
    let (_foreign_owner, paths, passes, command) = cleanup_fixture(foreign.path());
    let proof = VerifiedMerge::seal(&paths, &passes, &command).unwrap();
    let mut receipt = cleanup_receipt(&proof);
    assert!(
        proof
            .prune(
                &owner,
                &paths,
                &mut receipt,
                &paths.root.join("manifest.json")
            )
            .is_err()
    );
    assert!(paths.preparation_raw.join("prep.profraw").is_file());
}

#[cfg(unix)]
#[test]
fn cleanup_rejects_symlinked_generation_ancestor_before_any_removal() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, paths, passes, command) = cleanup_fixture(directory.path());
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("retained"), b"outside").unwrap();
    std::os::unix::fs::symlink(outside.path(), paths.root.join("generation")).unwrap();
    let proof = VerifiedMerge::seal(&paths, &passes, &command).unwrap();
    let mut receipt = cleanup_receipt(&proof);
    assert!(
        proof
            .prune(
                &owner,
                &paths,
                &mut receipt,
                &paths.root.join("manifest.json")
            )
            .is_err()
    );
    assert!(paths.preparation_raw.join("prep.profraw").is_file());
    assert_eq!(
        fs::read(outside.path().join("retained")).unwrap(),
        b"outside"
    );
}

#[cfg(unix)]
#[test]
fn cleanup_rejects_symlinked_profile_directory_before_any_removal() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, paths, passes, command) = cleanup_fixture(directory.path());
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("retained"), b"outside").unwrap();
    std::os::unix::fs::symlink(
        outside.path(),
        directory
            .path()
            .join(BuildProfile::ReleasePgoGen.target_subdir()),
    )
    .unwrap();
    let proof = VerifiedMerge::seal(&paths, &passes, &command).unwrap();
    let mut receipt = cleanup_receipt(&proof);
    assert!(
        proof
            .prune(
                &owner,
                &paths,
                &mut receipt,
                &paths.root.join("manifest.json")
            )
            .is_err()
    );
    assert!(paths.preparation_raw.join("prep.profraw").is_file());
    assert_eq!(
        fs::read(outside.path().join("retained")).unwrap(),
        b"outside"
    );
}

#[test]
fn target_owner_lock_is_exclusive_and_released_on_drop() {
    let directory = tempfile::tempdir().unwrap();
    let owner = TargetOwnership::acquire(directory.path()).unwrap();
    let contender = File::options()
        .read(true)
        .write(true)
        .open(directory.path().join("pgo/owner.lock"))
        .unwrap();
    assert!(contender.try_lock().is_err());
    drop(owner);
    contender.try_lock().unwrap();
}

#[cfg(unix)]
#[test]
fn cleanup_does_not_follow_internal_symlinks_and_accepts_trusted_target_alias() {
    let directory = tempfile::tempdir().unwrap();
    let real = directory.path().join("real");
    fs::create_dir(&real).unwrap();
    let alias = directory.path().join("alias");
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let (owner, paths, passes, command) = cleanup_fixture(&alias);
    let outside = tempfile::tempdir().unwrap();
    let retained = outside.path().join("retained");
    fs::write(&retained, b"keep outside data").unwrap();
    let staged = paths.root.join("generation/staged");
    fs::create_dir_all(&staged).unwrap();
    std::os::unix::fs::symlink(outside.path(), staged.join("link")).unwrap();
    let proof = VerifiedMerge::seal(&paths, &passes, &command).unwrap();
    let mut receipt = cleanup_receipt(&proof);
    proof
        .prune(
            &owner,
            &paths,
            &mut receipt,
            &paths.root.join("manifest.json"),
        )
        .unwrap();
    proof.verify_retained(&paths, &receipt).unwrap();
    assert_eq!(fs::read(retained).unwrap(), b"keep outside data");
    assert!(!staged.exists());
    assert!(paths.merged.is_file());
}

#[test]
fn unrelated_batch_receipt_cannot_select_a_training_fixture_for_deletion() {
    let directory = tempfile::tempdir().unwrap();
    let (_owner, paths, mut passes, command) = cleanup_fixture(directory.path());
    let outside = tempfile::tempdir().unwrap();
    let receipt = outside.path().join(TrainingMode::Default.result_name());
    fs::write(&receipt, b"unrelated receipt").unwrap();
    passes[0]["receipts"][0] = identity(&receipt).unwrap();
    assert!(VerifiedMerge::seal(&paths, &passes, &command).is_err());
    assert!(paths.preparation_raw.join("prep.profraw").is_file());
    assert!(receipt.is_file());
}

#[cfg(unix)]
#[test]
fn symlinked_training_repository_is_rejected_before_removing_profiles() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, paths, passes, command) = cleanup_fixture(directory.path());
    let proof = VerifiedMerge::seal(&paths, &passes, &command).unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("retained"), b"outside fixture").unwrap();
    std::os::unix::fs::symlink(outside.path(), &proof.training_fixture).unwrap();
    let mut receipt = cleanup_receipt(&proof);
    assert!(
        proof
            .prune(
                &owner,
                &paths,
                &mut receipt,
                &paths.root.join("manifest.json")
            )
            .is_err()
    );
    assert!(paths.preparation_raw.join("prep.profraw").is_file());
    assert_eq!(
        fs::read(outside.path().join("retained")).unwrap(),
        b"outside fixture"
    );
}
