use crate::*;

#[cfg(unix)]
fn run_under_undrained_pty(test_name: &str, child_env: &str) {
    let exe = env::current_exe().expect("current test executable");
    let helper = r#"
import os, pty, signal, subprocess, sys

exe, test_name, child_env = sys.argv[1], sys.argv[2], sys.argv[3]
master, slave = pty.openpty()
env = os.environ.copy()
env[child_env] = "1"
# Hold `master` open and deliberately NEVER read from it so any process
# writing >4 KiB to the inherited pty blocks in kernel file_tty_write.
proc = subprocess.Popen(
    [exe, "--exact", test_name],
    stdin=slave,
    stdout=slave,
    stderr=slave,
    env=env,
    start_new_session=True,
)
os.close(slave)
try:
    rc = proc.wait(timeout=5.0)
except subprocess.TimeoutExpired:
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    proc.wait()
    os.close(master)
    sys.stderr.write("DEADLOCKED_ON_UNDRAINED_PTY\n")
    sys.exit(124)
os.close(master)
sys.exit(rc)
"#;
    let output = Command::new("python3")
        .args(["-I", "-c", helper])
        .arg(&exe)
        .arg(test_name)
        .arg(child_env)
        .output()
        .expect("spawn python3 pty driver");
    assert!(
        output.status.success(),
        "{test_name} deadlocked or failed on undrained pty (status={}):\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[cfg(unix)]
fn write_loud_fake_emacs(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let script = r#"#!/usr/bin/env python3
import re, sys

# Emit >32 KiB to stdout and >32 KiB to stderr to fill any inherited pty/pipe.
chunk = ("W" * 120 + "\n") * 280
sys.stdout.write(chunk)
sys.stdout.flush()
sys.stderr.write(chunk)
sys.stderr.flush()

dest = None
for arg in sys.argv[1:]:
    m = re.search(r'"([^"]+\.elc)"', arg)
    if m:
        dest = m.group(1)
        break
if dest:
    with open(dest, "wb") as f:
        f.write(b";ELC\x17\x00\x00\x00")
"#;
    fs::write(path, script).expect("write fake emacs");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
}

#[cfg(unix)]
#[test]
fn wave_compile_does_not_deadlock_on_undrained_pty() {
    if env::var_os("XTASK_PTY_WAVE_CHILD").is_none() {
        run_under_undrained_pty(
            "tests::wave_stdio_test::wave_compile_does_not_deadlock_on_undrained_pty",
            "XTASK_PTY_WAVE_CHILD",
        );
        return;
    }

    let dir = tempfile::Builder::new()
        .prefix("xtask-pty-wave-")
        .tempdir()
        .expect("tempdir");
    let lisp_root = dir.path().join("lisp");
    fs::create_dir_all(&lisp_root).expect("create lisp_root");

    let mut sources = Vec::new();
    for i in 0..8 {
        let source = lisp_root.join(format!("mod{i}.el"));
        fs::write(&source, format!("(provide 'mod{i})\n")).expect("write .el");
        sources.push(source);
    }

    let fake_bin_dir = dir.path().join("bin");
    fs::create_dir_all(&fake_bin_dir).expect("create bin");
    write_loud_fake_emacs(&fake_bin_dir.join("neomacs"));

    let mut options =
        FreshBuildOptions::parse(dir.path().to_path_buf(), ["--release"].map(OsString::from))
            .expect("parse options");
    options.bin_dir = fake_bin_dir;
    let paths = pipeline_paths(&options);
    let deps = BTreeMap::new();
    let errors = run_compile_main_parallel(&options, &paths, &[], sources.clone(), &deps, 8)
        .expect("run_compile_main_parallel");
    assert!(errors.is_empty(), "unexpected wave errors: {errors:?}");
    for source in &sources {
        assert!(
            source.with_extension("elc").is_file(),
            "missing published .elc for {}",
            source.display()
        );
    }
}

#[cfg(unix)]
#[test]
fn generated_leim_and_preloaded_waves_do_not_deadlock_on_undrained_pty() {
    if env::var_os("XTASK_PTY_ALL_WAVES_CHILD").is_none() {
        run_under_undrained_pty(
            "tests::wave_stdio_test::generated_leim_and_preloaded_waves_do_not_deadlock_on_undrained_pty",
            "XTASK_PTY_ALL_WAVES_CHILD",
        );
        return;
    }

    let dir = tempfile::Builder::new()
        .prefix("xtask-pty-all-waves-")
        .tempdir()
        .expect("tempdir");
    let lisp_root = dir.path().join("lisp");
    fs::create_dir_all(&lisp_root).expect("create lisp_root");
    let fake_bin_dir = dir.path().join("bin");
    fs::create_dir_all(&fake_bin_dir).expect("create bin");
    write_loud_fake_emacs(&fake_bin_dir.join("bootstrap-neomacs"));

    let mut options =
        FreshBuildOptions::parse(dir.path().to_path_buf(), ["--release"].map(OsString::from))
            .expect("parse options");
    options.bin_dir = fake_bin_dir;
    let paths = pipeline_paths(&options);

    let gen_jobs: Vec<GeneratedLispJob> = (0..6)
        .map(|i| GeneratedLispJob {
            name: format!("gen-{i}"),
            args: vec![OsString::from(format!("--gen-{i}"))],
            output: None,
        })
        .collect();
    let gen_errors =
        run_generated_lisp_jobs(&options, &paths, &[], gen_jobs).expect("generated jobs");
    assert!(
        gen_errors.is_empty(),
        "unexpected gen errors: {gen_errors:?}"
    );

    let leim_jobs: Vec<LeimGenerationJob> = (0..6)
        .map(|i| LeimGenerationJob {
            source: dir.path().join(format!("dic-{i}.tit")),
            args: vec![OsString::from(format!("--leim-{i}"))],
        })
        .collect();
    let leim_errors =
        run_leim_generation_jobs(&options, &paths, &[], leim_jobs, 6).expect("leim jobs");
    assert!(
        leim_errors.is_empty(),
        "unexpected leim errors: {leim_errors:?}"
    );

    let mut preloaded = Vec::new();
    for i in 0..6 {
        let source = lisp_root.join(format!("pre{i}.el"));
        fs::write(&source, format!("(provide 'pre{i})\n")).expect("write pre .el");
        preloaded.push(source);
    }
    let wave = compile_wave::PlannedWave::create(&paths.lisp_root, &preloaded).expect("wave");
    let pre_errors: Vec<String> = wave
        .jobs()
        .par_iter()
        .filter_map(|job| {
            run_preloaded_lisp_pending_source(&options, &paths, &[], job)
                .err()
                .map(|err| format!("{} ({err})", job.source().display()))
        })
        .collect();
    assert!(
        pre_errors.is_empty(),
        "unexpected preloaded errors: {pre_errors:?}"
    );
    wave.validate()
        .unwrap()
        .publish()
        .unwrap()
        .finish()
        .unwrap();
}

#[test]
fn read_captured_tail_handles_empty_exact_and_truncated_files() {
    let mut file = tempfile::tempfile().expect("tempfile");
    assert_eq!(read_captured_tail(&mut file, 64).unwrap(), "");

    file.write_all(b"hello world\n").unwrap();
    assert_eq!(read_captured_tail(&mut file, 64).unwrap(), "hello world\n");
    assert_eq!(read_captured_tail(&mut file, 12).unwrap(), "hello world\n");

    let mut big = tempfile::tempfile().expect("tempfile");
    let prefix = "A".repeat(100);
    let suffix = "TAIL_SENTINEL_1234";
    big.write_all(prefix.as_bytes()).unwrap();
    big.write_all(suffix.as_bytes()).unwrap();
    let tail = read_captured_tail(&mut big, 32).unwrap();
    assert!(
        tail.starts_with("[captured output truncated: showing last 32 of 118 bytes]\n"),
        "unexpected truncation header: {tail:?}"
    );
    assert!(tail.ends_with(suffix), "missing tail suffix: {tail:?}");
}

#[cfg(unix)]
#[test]
fn captured_wave_job_detaches_stdin_isolates_workers_and_bounds_failure_tail() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::Builder::new()
        .prefix("xtask-captured-wave-")
        .tempdir()
        .expect("tempdir");
    let worker = dir.path().join("worker.py");
    let script = r#"#!/usr/bin/env python3
import sys

mode = sys.argv[1]
if mode == "stdin-eof":
    data = sys.stdin.read()
    if data != "":
        sys.stderr.write(f"expected empty stdin, got {len(data)} bytes\n")
        sys.exit(3)
    sys.stdout.write("X" * 262144)
    sys.stderr.write("Y" * 262144)
    sys.exit(0)
elif mode == "ok-noisy":
    sys.stdout.write("PASSING_WORKER_NOISE\n" * 4096)
    sys.exit(0)
elif mode == "fail-loud":
    sys.stdout.write("P" * 65536 + "\n")
    sys.stderr.write("ONLY_FAILING_WORKER_MARKER\n")
    sys.exit(7)
else:
    sys.exit(2)
"#;
    fs::write(&worker, script).expect("write worker");
    fs::set_permissions(&worker, fs::Permissions::from_mode(0o755)).expect("chmod");

    let options =
        FreshBuildOptions::parse(dir.path().to_path_buf(), ["--release"].map(OsString::from))
            .expect("parse options");

    // 1. Detached stdin + 512 KiB combined stdout/stderr succeeds cleanly.
    run_captured_wave_job(
        &options,
        dir.path(),
        &worker,
        &[OsString::from("stdin-eof")],
        &[],
    )
    .expect("stdin-eof + large output must succeed");

    // 2. Parallel workers: 7 noisy passers + 1 loud failer -> error contains
    // ONLY the failing worker's bounded tail and zero passing-worker noise.
    let modes = [
        "ok-noisy",
        "ok-noisy",
        "ok-noisy",
        "fail-loud",
        "ok-noisy",
        "ok-noisy",
        "ok-noisy",
        "ok-noisy",
    ];
    let errors: Vec<String> = modes
        .par_iter()
        .filter_map(|mode| {
            run_captured_wave_job(&options, dir.path(), &worker, &[OsString::from(mode)], &[])
                .err()
                .map(|err| err.to_string())
        })
        .collect();
    assert_eq!(
        errors.len(),
        1,
        "expected exactly 1 failing worker: {errors:?}"
    );
    let err = &errors[0];
    assert!(
        err.contains("ONLY_FAILING_WORKER_MARKER"),
        "missing failing worker marker in error: {err}"
    );
    assert!(
        err.contains("[captured output truncated: showing last"),
        "missing truncation header in error: {err}"
    );
    assert!(
        !err.contains("PASSING_WORKER_NOISE"),
        "passing worker output leaked into failing worker error"
    );
    assert!(
        err.len() <= CAPTURED_OUTPUT_TAIL_BYTES + 2048,
        "error output was not bounded: len={}",
        err.len()
    );

    // 3. Dry-run still succeeds without spawning even on a nonexistent program.
    let dry = FreshBuildOptions::parse(
        dir.path().to_path_buf(),
        ["--release", "--dry-run"].map(OsString::from),
    )
    .expect("parse dry options");
    run_captured_step(
        &dry,
        dir.path(),
        &dir.path().join("does-not-exist"),
        &[OsString::from("--batch")],
        &[],
    )
    .expect("dry-run must not spawn");
}
