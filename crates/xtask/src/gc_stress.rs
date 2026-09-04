//! `cargo xtask gc-stress` — run the shipped binary under `NEOVM_GC_STRESS=1`.
//!
//! WHY THIS EXISTS. The collector is precise: there is no conservative stack
//! scan and no API to configure one (see
//! `crates/neovm-core/src/tagged/CONCURRENT_GC.md`, "precise-rooting precondition"),
//! so a Lisp value that rides RUST control flow — a `Flow`, an `EvalError`, a
//! value held in a Rust local across a call that runs Lisp — is invisible to
//! `trace_roots` unless someone rooted it deliberately. GNU never has to think
//! about this: `signal_or_quit` and `unwind_to_catch` leave the payload on the
//! C stack and `mark_stack` scans it conservatively.
//!
//! A missing root of that kind is silent. It needs a collection to land in a
//! specific window, so it shows up as an intermittent crash thirty frames
//! away, in the printer or the symbol resolver, and a green test suite proves
//! nothing (DIVERGENCES.md 161 §2: the whole 38k-pin oracle suite was green on
//! both sides of the bug).
//!
//! `NEOVM_GC_STRESS=1` collects at EVERY allocation-bearing safe point, which
//! turns "sometimes, in a specific window" into "always". Ledger 161 found its
//! bug in eighteen seconds with it and then recorded, as a residual, that
//! nothing in the tree ever pointed it at a real `neomacs` process. This is
//! that detector.
//!
//! THE ADDRESS-SPACE CAP IS LOAD-BEARING. When a garbage id reaches
//! `thread_local_record_canonical`, the symbol cache — a `Vec` indexed by
//! symbol id — resizes to `id * 16` bytes, which for an id near `2^32` is
//! 64 GiB. Without a cap the process touches it page by page and is OOM-killed
//! with no diagnosis; with one, the allocation fails fast and Rust prints the
//! backtrace that names the caller.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

type DynError = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, DynError>;

/// Default cap on the child's virtual address space, in KiB (8 GiB).
const DEFAULT_ADDRESS_LIMIT_KB: u64 = 8_000_000;

/// Default wall-clock budget per probe. Generous: every allocation-bearing
/// safe point runs a full collection, so a probe is orders of magnitude slower
/// than the same forms unstressed.
const DEFAULT_TIMEOUT_SECS: u64 = 300;

pub(crate) fn usage_text() -> &'static str {
    "\
       cargo xtask gc-stress [--editor PATH] [--probe-dir DIR] [--out-dir DIR]
                             [--filter SUBSTR] [--address-limit-kb N]
                             [--timeout-secs N] [--list]

  --editor PATH is the ONLY way to point this at a binary other than
  target/release/neomacs. NEOVM_BINARY_PATH (which the oracle harness reads)
  is not consulted here; the 'against <path>' line printed at startup is the
  authoritative statement of what is being stressed.
  Probe stdout/stderr and the child's TMPDIR all land under --out-dir
  (default ./tmp/gc-stress).
"
}

struct Options {
    editor: PathBuf,
    probe_dir: PathBuf,
    /// Where each probe's stdout/stderr land. Files, not pipes: the runner
    /// polls for the child's exit so it can KILL a hang, and a pipe that fills
    /// while nobody is reading it would deadlock the very case this is for.
    out_dir: PathBuf,
    filter: Option<String>,
    address_limit_kb: u64,
    /// Wall-clock budget per probe. A missing root can present as a hang as
    /// easily as a crash, and an unbounded probe in CI is a wedged job, so the
    /// timeout is a FAILURE mode of the harness rather than an accident.
    timeout: Duration,
    list: bool,
}

struct Probe {
    name: String,
    path: PathBuf,
    /// Exact text the probe's stdout must contain, from a `;;; expect:` line.
    expect: Option<String>,
}

pub(crate) fn run(repo_root: &Path, args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let options = parse(repo_root, args)?;
    let probes = collect_probes(&options)?;
    if probes.is_empty() {
        return Err(format!(
            "no gc-stress probes found in {}",
            options.probe_dir.display()
        )
        .into());
    }
    if options.list {
        for probe in &probes {
            println!("{}\t{}", probe.name, probe.path.display());
        }
        return Ok(());
    }

    if !options.editor.exists() {
        return Err(format!(
            "editor not found: {}\n\nBuild it first:\n    cargo xtask fresh-build --release",
            options.editor.display()
        )
        .into());
    }

    std::fs::create_dir_all(&options.out_dir)
        .map_err(|err| format!("cannot create {}: {err}", options.out_dir.display()))?;

    println!(
        "gc-stress: {} probe(s) against {} (NEOVM_GC_STRESS=1, ulimit -v {} KiB, \
         {} s/probe)",
        probes.len(),
        options.editor.display(),
        options.address_limit_kb,
        options.timeout.as_secs(),
    );

    let mut failures = Vec::new();
    for probe in &probes {
        match run_probe(&options, probe) {
            Ok(()) => println!("  PASS {}", probe.name),
            Err(err) => {
                println!("  FAIL {}\n{err}", probe.name);
                failures.push(probe.name.clone());
            }
        }
    }

    println!(
        "gc-stress: {}/{} probes passed",
        probes.len() - failures.len(),
        probes.len()
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("gc-stress failures: {}", failures.join(", ")).into())
    }
}

fn parse(repo_root: &Path, args: impl IntoIterator<Item = OsString>) -> Result<Options> {
    let mut editor = repo_root.join("target/release/neomacs");
    let mut probe_dir = repo_root.join("crates/xtask/gc-stress");
    let mut filter = None;
    let mut out_dir = repo_root.join("tmp/gc-stress");
    let mut address_limit_kb = DEFAULT_ADDRESS_LIMIT_KB;
    let mut timeout_secs = DEFAULT_TIMEOUT_SECS;
    let mut list = false;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut next = |what: &str| -> Result<OsString> {
            args.next()
                .ok_or_else(|| format!("{what} requires a value").into())
        };
        match arg.to_string_lossy().as_ref() {
            "--editor" => editor = PathBuf::from(next("--editor")?),
            "--probe-dir" => probe_dir = PathBuf::from(next("--probe-dir")?),
            "--out-dir" => out_dir = PathBuf::from(next("--out-dir")?),
            "--filter" => filter = Some(next("--filter")?.to_string_lossy().into_owned()),
            "--address-limit-kb" => {
                address_limit_kb = next("--address-limit-kb")?
                    .to_string_lossy()
                    .parse()
                    .map_err(|_| "--address-limit-kb takes a number of KiB".to_string())?;
            }
            "--timeout-secs" => {
                timeout_secs = next("--timeout-secs")?
                    .to_string_lossy()
                    .parse()
                    .map_err(|_| "--timeout-secs takes a number of seconds".to_string())?;
            }
            "--list" => list = true,
            "--help" | "-h" => {
                println!("{}", usage_text());
                std::process::exit(0);
            }
            other => return Err(format!("unknown gc-stress option: {other}").into()),
        }
    }

    Ok(Options {
        editor,
        probe_dir,
        out_dir,
        filter,
        address_limit_kb,
        timeout: Duration::from_secs(timeout_secs),
        list,
    })
}

fn collect_probes(options: &Options) -> Result<Vec<Probe>> {
    let mut probes = Vec::new();
    let entries = std::fs::read_dir(&options.probe_dir)
        .map_err(|err| format!("cannot read {}: {err}", options.probe_dir.display()))?;
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("el") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        if let Some(filter) = &options.filter
            && !name.contains(filter)
        {
            continue;
        }
        let source = std::fs::read_to_string(&path)?;
        let expect = source.lines().find_map(|line| {
            line.strip_prefix(";;; expect:")
                .map(|rest| rest.trim().to_string())
        });
        probes.push(Probe { name, path, expect });
    }
    probes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(probes)
}

fn run_probe(options: &Options, probe: &Probe) -> Result<()> {
    let stdout_path = options.out_dir.join(format!("{}.out", probe.name));
    let stderr_path = options.out_dir.join(format!("{}.err", probe.name));
    let stdout_file = std::fs::File::create(&stdout_path)?;
    let stderr_file = std::fs::File::create(&stderr_path)?;

    // A probe that calls `make-temp-file` / `with-temp-file` writes wherever
    // `temporary-file-directory` points, which is `$TMPDIR` — inherited from
    // whoever ran the harness, i.e. `/tmp`, a mount this project treats as
    // unusable (feedback_never_slash_tmp). Route the child's temp files under
    // the harness's own out-dir so every artifact a probe leaves behind lands
    // in `./tmp/` with the probe's stdout and stderr.
    let probe_tmp = options.out_dir.join("tmp");
    std::fs::create_dir_all(&probe_tmp)
        .map_err(|err| format!("cannot create {}: {err}", probe_tmp.display()))?;

    // `ulimit -v` has no portable Rust equivalent without a libc dependency,
    // and the cap has to apply to the CHILD, so go through the shell.
    let script = format!(
        "ulimit -v {limit}; exec \"$1\" --batch -Q -l \"$2\"",
        limit = options.address_limit_kb
    );
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&script)
        .arg("gc-stress")
        .arg(&options.editor)
        .arg(&probe.path)
        .env("NEOVM_GC_STRESS", "1")
        // Debug-only inside neovm-core; harmless on a release binary, and the
        // right thing when someone points this at a debug build.
        .env("NEOVM_GC_VERIFY_MARKED", "1")
        .env("TMPDIR", &probe_tmp)
        .stdout(stdout_file)
        .stderr(stderr_file)
        .spawn()
        .map_err(|err| format!("failed to spawn {}: {err}", options.editor.display()))?;

    let started = Instant::now();
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None if started.elapsed() >= options.timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "    TIMEOUT after {} s (killed). A missing root presents as a hang \
                     as readily as a crash.\n    stdout: {}\n    stderr: {}",
                    options.timeout.as_secs(),
                    read_trimmed(&stdout_path),
                    read_trimmed(&stderr_path),
                )
                .into());
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    };

    let stdout = read_trimmed(&stdout_path);
    let stderr = read_trimmed(&stderr_path);
    if !status.success() {
        return Err(
            format!("    exit: {status}\n    stdout: {stdout}\n    stderr: {stderr}").into(),
        );
    }
    if let Some(expect) = &probe.expect
        && !stdout.contains(expect.as_str())
    {
        return Err(format!(
            "    expected stdout to contain: {expect}\n    stdout: {stdout}\n    stderr: {stderr}"
        )
        .into());
    }
    Ok(())
}

fn read_trimmed(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_string()
}
