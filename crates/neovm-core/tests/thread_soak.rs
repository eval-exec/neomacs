//! P11 SOAK: bounded, seeded, checked thread-runtime soak.
//!
//! The harness is an orchestrator, not a Lisp evaluator: every scenario is a
//! row file run by the *built* editor binary inside a sandbox launcher (never
//! in-process), so the same sandbox wraps neomacs and the GNU 31.1 oracle.
//! The launcher is tracked (`crates/neovm-core/tests/soak/sandbox-run.sh`,
//! capability-detected: systemd scope -> user/PID namespace -> direct exec) and
//! can be replaced with the soak lane's own wrapper via `SOAK_SANDBOX`. One
//! child process = one scenario = one receipt line.
//!
//! Arms (chosen by `SOAK_ARM`, one arm per invocation):
//!   sim          `NEOVM_THREADS` unset, GEN1 default, JIT on (the reference;
//!                each scenario's deterministic `state=` digest is captured here)
//!   green        `NEOVM_THREADS=green`, GEN1 default, JIT on
//!   gen1verify   green + `NEOVM_GC_VERIFY_GENERATIONAL=1`
//!   gen0         green + `NEOVM_GC_GENERATIONAL=0`
//!   inlineoff    green + `NEOVM_JIT_INLINE=off`
//!
//! Every arm run must print the same `state=` digest as the same-seed Sim
//! reference; scenarios marked `gnu` additionally run the same row under the
//! pinned GNU 31.1 and must agree. Diagnostics (wake counts, joined counts,
//! iteration counters) are receipt columns, not part of the digest.
//!
//! Receipt: `SOAK_RECEIPT` (default `tmp/v10x/soak/receipts/`), one TSV row
//! per (scenario, arm) with seed, iterations, wall, perf instructions, RSS
//! slope and the failure reason. The test panics if any row fails.
//!
//! Duration: `SOAK_SECONDS` is the per-arm budget across all scenarios
//! (default 1800; `SOAK_MODE=smoke` uses `SOAK_SMOKE_SECS` per scenario,
//! default 1, and is the <= 2 min gate). The long arms are run by
//! executing the built `core` test binary directly
//! (`--exact thread_soak::soak --nocapture`), which is a plain run of a
//! built target and has no nextest timeout.
//!
//! Portability: this module lives in the single `core` integration binary (the
//! one-binary-per-crate lint). The rows and the launcher are tracked under
//! `crates/neovm-core/tests/soak/`, and the editor is resolved from
//! `NEOVM_BINARY_PATH` / `NEOMACS_BIN` (both exported by CI for the runtime
//! artifact) or the workspace's `target/release/neomacs`; a missing editor
//! fails with the actionable build message. `SOAK_CPUS` is empty by default
//! (no `taskset`) because CI runners have no CPUs 14-31; the soak lane sets
//! `0-3,14-31`. Default runs therefore execute the smoke; long arms and the
//! reference file stay explicit lane settings.
//!
//! RSS: `SOAK_RSS_LIMIT_KIB_PER_MIN` (default 2048) bounds the growth of the
//! *editor process's* VmRSS. The gate has a minimum-window rule (only runs of
//! at least 60 s are classified: shorter windows extrapolate a startup
//! transient) and compares quarter means, requiring the last quarter's slope
//! to exceed the bar, more than 8 MiB of growth from Q2 to Q4, and at least
//! half of the previous quarter's slope (a decaying warmup is not a leak).
//!
//! Which memory grows: every receipt row also carries the row's boundary GC
//! marks -- total `gcs-done`, retained heap from the boundary full
//! collections, and the non-forcing heap-layout census (managed live bytes,
//! page backing, allocated objects, live thread objects) -- and the run
//! artifacts carry a per-minute `SOAK-PROGRESS` curve sidecar. RSS rising
//! with flat live bytes points at native state; live bytes and objects
//! rising points at Lisp data the collector either cannot reach or is never
//! triggered for.
//!
//! No product code, no product knob, no TLS byte, no Context/World field:
//! this target only reads env and spawns processes.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// One scenario: a row file plus the knobs the row reads.
struct Scenario {
    name: &'static str,
    row: &'static str,
    /// Extra env for the row (`SOAK_N`, `SOAK_KIND`, ...).
    row_env: &'static [(&'static str, &'static str)],
    /// Whether the pinned GNU 31.1 can run this row.
    gnu: bool,
    /// `SANDBOX_MEM_GB` for this row: a green Lisp thread holds a collector
    /// and allocator state besides its stack, so high-thread rows need a
    /// stated budget larger than the 8 GiB default. `SOAK_MEM_GB` overrides.
    mem_gb: u32,
}

// The full P11 mix. Each scenario's checked `state=` digest is
// duration-independent; per-scenario knobs ride on the row env.
const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "spawn_join_live_16",
        row: "soak_spawn_join.el",
        row_env: &[("SOAK_N", "16")],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "spawn_join_live_100",
        row: "soak_spawn_join.el",
        row_env: &[("SOAK_N", "100")],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "spawn_join_live_300",
        row: "soak_spawn_join.el",
        row_env: &[("SOAK_N", "300")],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "mutex_ordered",
        row: "soak_mutex_ordered.el",
        row_env: &[],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "cond_notify_all",
        row: "soak_cond.el",
        row_env: &[("SOAK_KIND", "all")],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "cond_notify_one",
        row: "soak_cond.el",
        row_env: &[("SOAK_KIND", "one")],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "yield_loop",
        row: "soak_yield.el",
        row_env: &[],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "timers",
        row: "soak_timers.el",
        row_env: &[],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "accept_process_output",
        row: "soak_proc.el",
        row_env: &[],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "signal_from_callee",
        row: "soak_signal.el",
        row_env: &[],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "redefine_inlined_parked",
        row: "soak_redefine.el",
        row_env: &[],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "weak_hash_match_cache",
        row: "soak_weak_cache.el",
        row_env: &[],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "many_gated_100",
        row: "soak_many_gated.el",
        row_env: &[("SOAK_N", "100")],
        gnu: true,
        mem_gb: 8,
    },
    Scenario {
        name: "many_gated_300",
        row: "soak_many_gated.el",
        row_env: &[("SOAK_N", "300")],
        gnu: true,
        mem_gb: 8,
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arm {
    Sim,
    Green,
    Gen1Verify,
    Gen0,
    InlineOff,
}

impl Arm {
    fn parse(text: &str) -> Arm {
        match text {
            "sim" => Arm::Sim,
            "green" => Arm::Green,
            "gen1verify" => Arm::Gen1Verify,
            "gen0" => Arm::Gen0,
            "inlineoff" => Arm::InlineOff,
            other => panic!("unknown SOAK_ARM {other} (sim|green|gen1verify|gen0|inlineoff)"),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Arm::Sim => "sim",
            Arm::Green => "green",
            Arm::Gen1Verify => "gen1verify",
            Arm::Gen0 => "gen0",
            Arm::InlineOff => "inlineoff",
        }
    }

    /// The child's arm environment. Built from scratch over the inherited env
    /// so a stale caller knob cannot silently change the arm.
    fn apply(self, command: &mut Command) {
        for var in [
            "NEOVM_THREADS",
            "NEOVM_GC_GENERATIONAL",
            "NEOVM_GC_VERIFY_GENERATIONAL",
            "NEOVM_JIT_INLINE",
            "NEOVM_JIT_THRESHOLD",
        ] {
            command.env_remove(var);
        }
        match self {
            Arm::Sim => {}
            Arm::Green => {
                command.env("NEOVM_THREADS", "green");
            }
            Arm::Gen1Verify => {
                command.env("NEOVM_THREADS", "green");
                command.env("NEOVM_GC_VERIFY_GENERATIONAL", "1");
            }
            Arm::Gen0 => {
                command.env("NEOVM_THREADS", "green");
                command.env("NEOVM_GC_GENERATIONAL", "0");
            }
            Arm::InlineOff => {
                command.env("NEOVM_THREADS", "green");
                command.env("NEOVM_JIT_INLINE", "off");
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// `SOAK_MODE=smoke`: short sim + arm over every scenario (<= 2 min gate).
    Smoke,
    /// `SOAK_MODE=reference`: short Sim (and GNU where runnable) run that
    /// writes the same-seed deterministic `state=` reference every arm uses.
    Reference,
    /// `SOAK_MODE=arm`: one long arm run per scenario, compared against the
    /// reference file.
    Arm,
}

struct Config {
    role: Role,
    mode: String,
    arm: Arm,
    seed: u64,
    scenario_secs: u64,
    bin: PathBuf,
    rows: PathBuf,
    sandbox: PathBuf,
    cpus: String,
    receipt: PathBuf,
    reference: PathBuf,
    gnu_bin: PathBuf,
    run_gnu: bool,
    rss_limit_kib_per_min: f64,
    /// Debug/selection filter: `SOAK_ONLY=a,b` runs only those scenarios.
    only: Option<Vec<String>>,
    /// `SOAK_MEM_GB` overrides every scenario's sandbox budget.
    mem_gb_override: Option<u32>,
    /// `SOAK_ROW_ENV=K=V,K2=V2` extra row knobs (e.g. SOAK_FORCE_GC=1).
    row_env: Vec<(String, String)>,
}

/// Editor directory for the sandboxed run: `NEOVM_BINARY_PATH` (the path CI
/// exports for the runtime artifact) or `NEOMACS_BIN`, else the workspace's
/// release build. Accepts either the binary itself or its directory.
fn resolve_editor_dir(repo: &Path) -> PathBuf {
    for var in ["NEOVM_BINARY_PATH", "NEOMACS_BIN"] {
        if let Some(value) = env::var_os(var) {
            let path = PathBuf::from(value);
            return if path.is_dir() {
                path
            } else {
                path.parent().map(Path::to_path_buf).unwrap_or(path)
            };
        }
    }
    repo.join("target/release")
}

impl Config {
    fn from_env() -> Config {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("crates/neovm-core lives under the repo")
            .to_path_buf();
        let mode = env::var("SOAK_MODE").unwrap_or_else(|_| "smoke".to_owned());
        let role = match mode.as_str() {
            "smoke" => Role::Smoke,
            "reference" => Role::Reference,
            "arm" => Role::Arm,
            other => panic!("unknown SOAK_MODE {other} (smoke|reference|arm)"),
        };
        let seed: u64 = env::var("SOAK_SEED")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        // One explicit per-scenario duration wins over the role default; the
        // diagnostic mode uses it to watch a single scenario's RSS trend.
        let scenario_secs = env::var("SOAK_SCENARIO_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(match role {
                Role::Smoke => env::var("SOAK_SMOKE_SECS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1),
                Role::Reference => env::var("SOAK_REF_SECS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(10),
                Role::Arm => {
                    let total: u64 = env::var("SOAK_SECONDS")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1800);
                    (total / SCENARIOS.len() as u64).max(2)
                }
            });
        let receipt = env::var_os("SOAK_RECEIPT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                repo.join("tmp/v10x/soak/receipts").join(format!(
                    "{}-seed{}-{}.tsv",
                    env::var("SOAK_ARM").unwrap_or_else(|_| "sim".to_owned()),
                    seed,
                    mode,
                ))
            });
        Config {
            role,
            mode,
            arm: Arm::parse(&env::var("SOAK_ARM").unwrap_or_else(|_| "green".to_owned())),
            seed,
            scenario_secs,
            bin: env::var_os("SOAK_BIN")
                .map(PathBuf::from)
                .unwrap_or_else(|| resolve_editor_dir(&repo)),
            rows: env::var_os("SOAK_ROWS")
                .map(PathBuf::from)
                .unwrap_or_else(|| repo.join("crates/neovm-core/tests/soak/rows")),
            sandbox: env::var_os("SOAK_SANDBOX")
                .map(PathBuf::from)
                .unwrap_or_else(|| repo.join("crates/neovm-core/tests/soak/sandbox-run.sh")),
            cpus: env::var("SOAK_CPUS").unwrap_or_default(),
            receipt,
            reference: env::var_os("SOAK_REFERENCE")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    repo.join("tmp/v10x/soak/reference")
                        .join(format!("seed{seed}.tsv"))
                }),
            gnu_bin: env::var_os("TB_GNU_BIN")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/home/exec/.local/bin/emacs")),
            run_gnu: env::var("SOAK_GNU")
                .map(|v| v != "0")
                .unwrap_or(role != Role::Smoke),
            rss_limit_kib_per_min: env::var("SOAK_RSS_LIMIT_KIB_PER_MIN")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2048.0),
            only: env::var("SOAK_ONLY").ok().map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .collect()
            }),
            mem_gb_override: env::var("SOAK_MEM_GB").ok().and_then(|v| v.parse().ok()),
            row_env: env::var("SOAK_ROW_ENV")
                .ok()
                .map(|value| {
                    value
                        .split(',')
                        .filter_map(|pair| pair.split_once('='))
                        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    fn mem_gb_for(&self, scenario: &Scenario) -> u32 {
        self.mem_gb_override.unwrap_or(scenario.mem_gb)
    }

    fn selected(&self) -> Vec<&'static Scenario> {
        SCENARIOS
            .iter()
            .filter(|scenario| {
                self.only
                    .as_ref()
                    .is_none_or(|names| names.iter().any(|name| name == scenario.name))
            })
            .collect()
    }
}

/// One row-reported `SOAK-PROGRESS` minute mark: total GC cycles so far, the
/// cumulative allocation counters, and (Neomacs only) the non-forcing heap
/// census that attributes RSS growth either to live Lisp data or to native
/// page backing. GNU rows carry only gcs/use.
#[derive(Default, Clone)]
struct Progress {
    seconds: u64,
    gcs: Option<i64>,
    use_: String,
    heap_kib: Option<u64>,
    live_bytes: Option<i64>,
    backing_bytes: Option<i64>,
    objects: Option<i64>,
    thread_objs: Option<i64>,
}

/// One child invocation's raw observations.
struct Run {
    scenario: &'static str,
    arm: &'static str,
    rc: Option<i32>,
    timed_out: bool,
    wall_ms: u128,
    instructions: Option<u128>,
    state: Option<String>,
    diag: BTreeMap<String, String>,
    rss_start_kib: Option<u64>,
    rss_end_kib: Option<u64>,
    rss_slope_kib_per_min: Option<f64>,
    /// Quarter means (q1..q4) of the editor's VmRSS, and the Q2->Q3 and
    /// Q3->Q4 slopes; the leak gate requires the last slope to be sustained
    /// (not a warmup transient) before it fails.
    rss_quarters_kib: [Option<f64>; 4],
    rss_slope_prev_kib_per_min: Option<f64>,
    rss_slope_last_kib_per_min: Option<f64>,
    sandbox_mem_gb: u32,
    /// Failure-instant classification evidence for thread-spawn EAGAIN:
    /// the editor's transient scope (memory.peak/max, pids.current/max,
    /// memory.events), the editor's own rlimits, and host pressure.
    scope_peak_kib: u64,
    scope_memory_max: String,
    scope_pids_current: u64,
    scope_pids_max: String,
    scope_max_events: u64,
    scope_oom_events: u64,
    rlimit_nproc: Option<u64>,
    rlimit_as: String,
    host_available_kib: u64,
    host_committed_kib: u64,
    scope_sample_ms: u128,
    /// The launcher's capability line (`soak-sandbox: mode=...`), so portable
    /// runs record whether they got the memory-capped scope, a bare namespace
    /// or a direct exec.
    sandbox_mode: String,
    /// Row-reported GC marks: `SOAK-GC t=start|end gcs=.. heap=.. use=..`.
    gc_start: Option<i64>,
    gc_end: Option<i64>,
    heap_start_kib: Option<u64>,
    heap_end_kib: Option<u64>,
    use_start: String,
    use_end: String,
    /// Non-forcing heap census at the two marks (Neomacs rows only):
    /// managed live bytes, page backing bytes, allocated objects, live
    /// thread objects.  Native RSS rising with flat live bytes is a native
    /// leak; live bytes and thread objects rising is a GC-trigger gap.
    live_start_bytes: Option<i64>,
    live_end_bytes: Option<i64>,
    backing_start_bytes: Option<i64>,
    backing_end_bytes: Option<i64>,
    objects_start: Option<i64>,
    objects_end: Option<i64>,
    thread_objs_start: Option<i64>,
    thread_objs_end: Option<i64>,
    stderr_tail: String,
}

impl Run {
    fn ok(&self) -> bool {
        self.rc == Some(0) && !self.timed_out && self.state.is_some()
    }
}

/// Read the same-seed reference written by `SOAK_MODE=reference`.
fn read_reference(path: &Path) -> BTreeMap<String, (String, Option<String>)> {
    let mut out = BTreeMap::new();
    let text = fs::read_to_string(path).unwrap_or_default();
    for line in text.lines() {
        if line.starts_with('#') || line.starts_with("scenario\t") {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() >= 3 {
            out.insert(
                columns[0].to_owned(),
                (
                    columns[1].to_owned(),
                    if columns[2].is_empty() {
                        None
                    } else {
                        Some(columns[2].to_owned())
                    },
                ),
            );
        }
    }
    out
}

fn write_reference(path: &Path, rows: &[(String, String, Option<String>)]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    writeln!(file, "scenario\tsim_state\tgnu_state")?;
    for (scenario, sim_state, gnu_state) in rows {
        writeln!(
            file,
            "{scenario}\t{sim_state}\t{}",
            gnu_state.clone().unwrap_or_default()
        )?;
    }
    Ok(())
}

/// Run one row in one arm. `sandbox-run.sh taskset <cpus> perf stat ... editor`.
fn run_row(config: &Config, scenario: &Scenario, arm: Arm, gnu: bool) -> Run {
    let row = config.rows.join(scenario.row);
    let out_dir = config
        .receipt
        .parent()
        .expect("receipt has a parent")
        .join("runs");
    let _ = fs::create_dir_all(&out_dir);
    let tag = format!(
        "{}-{}-seed{}-{}",
        scenario.name,
        arm.name(),
        config.seed,
        if gnu { "gnu" } else { "neo" }
    );
    let perf_path = out_dir.join(format!("{tag}.perf"));
    let stdout_path = out_dir.join(format!("{tag}.out"));
    let stderr_path = out_dir.join(format!("{tag}.err"));

    let mut command = Command::new(&config.sandbox);
    // Pinning is a soak-lane setting; portable runs (CI) have no 14-31 CPUs.
    if !config.cpus.is_empty() {
        command.arg("taskset").arg("-c").arg(&config.cpus);
    }
    command
        .arg("perf")
        .arg("stat")
        .arg("-e")
        .arg("instructions:u")
        .arg("-x,")
        .arg("-o")
        .arg(&perf_path);
    if gnu {
        command.arg(&config.gnu_bin);
    } else {
        command.arg(config.bin.join("neomacs"));
    }
    command
        .arg("-Q")
        .arg("--batch")
        .arg("-l")
        .arg(&row)
        .env("SOAK_SEED", config.seed.to_string())
        .env("SOAK_SECS", config.scenario_secs.to_string())
        .env("SANDBOX_MEM_GB", config.mem_gb_for(scenario).to_string())
        // The launcher refuses SOAK_MODE=arm without a memory-capped scope.
        .env("SOAK_MODE", &config.mode)
        .stdout(Stdio::from(
            fs::File::create(&stdout_path).expect("stdout file"),
        ))
        .stderr(Stdio::from(
            fs::File::create(&stderr_path).expect("stderr file"),
        ));
    for &(key, value) in scenario.row_env {
        command.env(key, value);
    }
    for (key, value) in &config.row_env {
        command.env(key, value);
    }
    // The workspace `.cargo/config.toml` sets RUST_MIN_STACK=128 MiB for the
    // *test* process (cargo injects it into test binaries). The editor must
    // run with the environment a shipped editor has: `std::thread` uses
    // RUST_MIN_STACK as the default stack size, so 100 green Lisp threads
    // would reserve 12.8 GiB of address space and the kernel can refuse the
    // clone with EAGAIN. Strip the harness-only knobs from every child.
    for var in [
        "RUST_MIN_STACK",
        "RUST_LOG",
        "RUST_BACKTRACE",
        "RUST_LIB_BACKTRACE",
        "NEXTEST",
        "NEXTEST_RUN_ID",
        "NEXTEST_TEST_NAME",
        "NEXTEST_TEST_GROUP",
    ] {
        command.env_remove(var);
    }
    if gnu {
        // GNU has no neomacs arm knobs; expose the seed to the row only.
        for var in [
            "NEOVM_THREADS",
            "NEOVM_GC_GENERATIONAL",
            "NEOVM_GC_VERIFY_GENERATIONAL",
            "NEOVM_JIT_INLINE",
        ] {
            command.env_remove(var);
        }
    } else {
        arm.apply(&mut command);
    }

    let started = Instant::now();
    let mut child = command.spawn().expect("spawn sandboxed editor");
    // RSS sampler: sum the VmRSS of the child's process subtree twice a second.
    // The PID-namespace hop can reparent the editor, so the sampler also
    // matches processes whose cmdline names this row file (unique per run).
    let sample_pid = child.id();
    let needle = scenario.row.to_owned();
    let sampling = thread::spawn(move || sample_rss(sample_pid, &needle));
    let timeout = Duration::from_secs(config.scenario_secs + 180);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait().expect("wait") {
            Some(status) => break Some(status),
            None if started.elapsed() > timeout => {
                // Belt and braces: kill the sandbox leader and every
                // descendant we can see, so nothing survives a hung run.
                for pid in process_tree(sample_pid) {
                    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
                }
                let _ = child.kill();
                let _ = child.wait();
                timed_out = true;
                break None;
            }
            None => thread::sleep(Duration::from_millis(100)),
        }
    };
    let wall_ms = started.elapsed().as_millis();
    let sampled = sampling.join().expect("rss sampler");
    let samples = &sampled.samples;

    let stdout = fs::read_to_string(&stdout_path).unwrap_or_default();
    let (state, diag) = parse_result(&stdout);
    let (gc_start, gc_end) = parse_gc_marks(&stdout);
    let progress = parse_progress(&stdout);
    write_curve(&out_dir, &tag, samples, &progress);
    let stderr_text = fs::read_to_string(&stderr_path).unwrap_or_default();
    let sandbox_mode = stderr_text
        .lines()
        .find_map(|line| line.strip_prefix("soak-sandbox: "))
        .map(str::to_owned)
        .unwrap_or_default();
    let stderr_tail = stderr_text
        .lines()
        .rev()
        .take(5)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" | ");
    Run {
        scenario: scenario.name,
        arm: arm.name(),
        rc: status.and_then(|s| s.code()),
        timed_out,
        wall_ms,
        instructions: read_instructions(&perf_path),
        state,
        diag,
        rss_start_kib: samples.first().map(|(_, rss)| *rss),
        rss_end_kib: samples.last().map(|(_, rss)| *rss),
        rss_slope_kib_per_min: rss_slope(samples),
        rss_quarters_kib: sampled.quarters.map(|quarter| quarter.map(|(_, rss)| rss)),
        rss_slope_prev_kib_per_min: quarter_slope(&sampled.quarters, 1, 2),
        rss_slope_last_kib_per_min: quarter_slope(&sampled.quarters, 2, 3),
        sandbox_mem_gb: config.mem_gb_for(scenario),
        scope_peak_kib: sampled.scope.peak_kib,
        scope_memory_max: sampled.scope.memory_max_raw,
        scope_pids_current: sampled.scope.pids_current,
        scope_pids_max: sampled.scope.pids_max_raw,
        scope_max_events: sampled.scope.max_events,
        scope_oom_events: sampled.scope.oom_events,
        rlimit_nproc: sampled.scope.rlimit_nproc,
        rlimit_as: sampled.scope.rlimit_as_raw,
        host_available_kib: sampled.scope.host_available_kib,
        host_committed_kib: sampled.scope.host_committed_kib,
        scope_sample_ms: sampled.scope.at_ms,
        sandbox_mode,
        gc_start: gc_start.get("gcs").and_then(|v| v.parse().ok()),
        gc_end: gc_end.get("gcs").and_then(|v| v.parse().ok()),
        heap_start_kib: gc_start.get("heap").and_then(|v| v.parse().ok()),
        heap_end_kib: gc_end.get("heap").and_then(|v| v.parse().ok()),
        use_start: gc_start.get("use").cloned().unwrap_or_default(),
        use_end: gc_end.get("use").cloned().unwrap_or_default(),
        live_start_bytes: gc_start.get("live").and_then(|v| v.parse().ok()),
        live_end_bytes: gc_end.get("live").and_then(|v| v.parse().ok()),
        backing_start_bytes: gc_start.get("backing").and_then(|v| v.parse().ok()),
        backing_end_bytes: gc_end.get("backing").and_then(|v| v.parse().ok()),
        objects_start: gc_start.get("objs").and_then(|v| v.parse().ok()),
        objects_end: gc_end.get("objs").and_then(|v| v.parse().ok()),
        thread_objs_start: gc_start.get("thread-objs").and_then(|v| v.parse().ok()),
        thread_objs_end: gc_end.get("thread-objs").and_then(|v| v.parse().ok()),
        stderr_tail,
    }
}

/// Parse the row protocol: `SOAK-RESULT <name> seed=<n> iters=<n> state=<s> key=value ...`.
fn parse_result(stdout: &str) -> (Option<String>, BTreeMap<String, String>) {
    let mut state = None;
    let mut diag = BTreeMap::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("SOAK-RESULT ") else {
            continue;
        };
        for field in rest.split_whitespace().skip(1) {
            if let Some((key, value)) = field.split_once('=') {
                if key == "state" {
                    state = Some(value.to_owned());
                } else {
                    diag.insert(key.to_owned(), value.to_owned());
                }
            }
        }
    }
    (state, diag)
}

type Fields = BTreeMap<String, String>;

fn parse_fields(rest: &str) -> Fields {
    rest.split_whitespace()
        .filter_map(|field| field.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

/// `SOAK-GC t=start|end gcs=<n> heap=<kib> use=<csv>` marks (the row calls
/// `garbage-collect` once at each boundary, then reports the stats buckets).
fn parse_gc_marks(stdout: &str) -> (Fields, Fields) {
    let mut start = Fields::new();
    let mut end = Fields::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("SOAK-GC ") else {
            continue;
        };
        let fields = parse_fields(rest);
        match fields.get("t").map(String::as_str) {
            Some("start") => start = fields,
            Some("end") => end = fields,
            _ => {}
        }
    }
    (start, end)
}

/// `SOAK-PROGRESS t=<sec> gcs=<n> use=<csv>[ heap=<kib>][ live=.. backing=..
/// objs=.. thread-objs=..]` rows, one per minute (heap only in the forced-GC
/// arm, where the row has a fresh reading; the census is non-forcing and
/// present whenever the editor exposes `neomacs--heap-layout-stats`).
fn parse_progress(stdout: &str) -> Vec<Progress> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("SOAK-PROGRESS ") else {
            continue;
        };
        let fields = parse_fields(rest);
        let Some(seconds) = fields.get("t").and_then(|v| v.parse::<u64>().ok()) else {
            continue;
        };
        out.push(Progress {
            seconds,
            gcs: fields.get("gcs").and_then(|v| v.parse::<i64>().ok()),
            use_: fields.get("use").cloned().unwrap_or_default(),
            heap_kib: fields.get("heap").and_then(|v| v.parse::<u64>().ok()),
            live_bytes: fields.get("live").and_then(|v| v.parse::<i64>().ok()),
            backing_bytes: fields.get("backing").and_then(|v| v.parse::<i64>().ok()),
            objects: fields.get("objs").and_then(|v| v.parse::<i64>().ok()),
            thread_objs: fields
                .get("thread-objs")
                .and_then(|v| v.parse::<i64>().ok()),
        });
    }
    out
}

/// Write a per-minute curve next to the run's artifacts: the driver's own
/// RSS means joined with the row's GC/allocation/heap progress marks. This is
/// the "which memory grows" evidence for an RSS-slope finding.
fn write_curve(dir: &Path, tag: &str, samples: &[(u128, u64)], progress: &[Progress]) {
    let mut per_minute: BTreeMap<u64, (f64, usize)> = BTreeMap::new();
    for (t, rss) in samples {
        let minute = (*t as u64) / 60_000;
        let entry = per_minute.entry(minute).or_insert((0.0, 0));
        entry.0 += *rss as f64;
        entry.1 += 1;
    }
    let mut text = String::from(
        "minute\trss_mean_kib\tgcs\tuse\theap_kib\tlive_bytes\tbacking_bytes\tobjects\tthread_objs\n",
    );
    for (minute, (rss_sum, count)) in per_minute {
        let mark = progress.iter().find(|mark| mark.seconds / 60 == minute);
        let num = |value: Option<i64>| value.map(|v| v.to_string()).unwrap_or_default();
        text.push_str(&format!(
            "{minute}\t{:.0}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            rss_sum / count as f64,
            num(mark.and_then(|m| m.gcs)),
            mark.map(|m| m.use_.clone()).unwrap_or_default(),
            mark.and_then(|m| m.heap_kib)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            num(mark.and_then(|m| m.live_bytes)),
            num(mark.and_then(|m| m.backing_bytes)),
            num(mark.and_then(|m| m.objects)),
            num(mark.and_then(|m| m.thread_objs)),
        ));
    }
    let _ = fs::write(dir.join(format!("{tag}.curve.tsv")), text);
}

/// Sum the counted `instructions:u` lines (hybrid PMU splits into atom+core).
fn read_instructions(path: &Path) -> Option<u128> {
    let text = fs::read_to_string(path).ok()?;
    let mut total: u128 = 0;
    let mut any = false;
    for line in text.lines() {
        let columns: Vec<&str> = line.split(',').collect();
        if columns.len() < 3
            || !columns[2].contains("instructions")
            || line.contains("<not counted>")
        {
            continue;
        }
        if let Ok(value) = columns[0].trim().parse::<u128>() {
            total += value;
            any = true;
        }
    }
    any.then_some(total)
}

#[derive(Default, Clone)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct ScopeStats {
    /// The editor's own transient scope (systemd `run-*.scope`), not a parent.
    peak_kib: u64,
    memory_max_raw: String,
    max_events: u64,
    oom_events: u64,
    pids_current: u64,
    pids_max_raw: String,
    /// The editor's own rlimits at the failure instant.
    rlimit_nproc: Option<u64>,
    rlimit_as_raw: String,
    /// Host pressure at the same instant.
    host_available_kib: u64,
    host_committed_kib: u64,
    editor_pid: Option<u32>,
    at_ms: u128,
}

struct Sampled {
    samples: Vec<(u128, u64)>,
    scope: ScopeStats,
    /// Mean (t_ms, VmRSS_kib) over four equal quarters of the run; the leak
    /// gate compares the last quarter's slope with the previous quarter's.
    quarters: [Option<(f64, f64)>; 4],
}

#[cfg(target_os = "linux")]
fn read_vm_rss(pid: u32) -> Option<u64> {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok());
        }
    }
    None
}

/// Collect (elapsed_ms, rss_kib) of the *editor* process, falling back to the
/// subtree sum only while the editor has not been identified yet, plus the
/// editor's scope accounting, rlimits and host memory pressure. The last tick
/// before the child disappears is the failure instant recorded in the receipt.
#[cfg(not(target_os = "linux"))]
fn sample_rss(_root_pid: u32, _needle: &str) -> Sampled {
    // The sampler reads /proc (RSS, scope accounting, rlimits); on macOS and
    // other hosts the smoke still checks every row's state digest, it just
    // records no RSS columns.
    Sampled {
        samples: Vec::new(),
        scope: ScopeStats::default(),
        quarters: [None; 4],
    }
}

#[cfg(not(target_os = "linux"))]
fn process_tree(_root: u32) -> Vec<u32> {
    Vec::new()
}

#[cfg(target_os = "linux")]
fn sample_rss(root_pid: u32, needle: &str) -> Sampled {
    let started = Instant::now();
    let mut samples = Vec::new();
    let mut scope = ScopeStats::default();
    let mut editor_pid: Option<u32> = None;
    let mut scope_path: Option<PathBuf> = None;
    while started.elapsed() < Duration::from_secs(24 * 3600) {
        // The run's own transient systemd scope (`run-*.scope`): the sandbox
        // wrapper is moved into it by systemd-run --scope. Once the editor is
        // identified it works too, but the wrapper is visible from the first
        // tick. Only a transient scope is accepted: the user manager's
        // app.slice would leak unrelated accounting (a parent scope's
        // memory.peak was observed as a constant 13,264,428 KiB in receipts).
        let root_scope =
            transient_scope_of(root_pid).or_else(|| editor_pid.and_then(transient_scope_of));
        let mut pids = process_tree(root_pid);
        if editor_pid.is_none() {
            // A bare cmdline match is not enough to own a process: several
            // soak runs can execute the same row concurrently (different
            // receipts and sandboxes), and a global match kept this sampler
            // alive forever on another run's editor. Scope the match to this
            // run's own sandbox scope, and only while the editor is unknown.
            for pid in scoped_cmdline_matches(needle, root_scope.as_deref()) {
                if !pids.contains(&pid) {
                    pids.push(pid);
                }
            }
        }
        if pids.is_empty() {
            break;
        }
        if editor_pid.is_none() {
            for pid in &pids {
                if is_editor_process(*pid) {
                    editor_pid = Some(*pid);
                    break;
                }
            }
        }
        if scope_path.is_none() {
            scope_path = root_scope.clone();
        }
        if let Some(path) = &scope_path {
            read_scope(path, &mut scope);
        }
        if let Some(pid) = editor_pid {
            read_limits(pid, &mut scope);
        }
        read_host_memory(&mut scope);
        scope.editor_pid = editor_pid;
        let mut alive = false;
        let mut rss = match editor_pid.and_then(read_vm_rss) {
            Some(kib) => {
                alive = true;
                kib
            }
            None => {
                // Startup window: the editor is not visible yet, sum the tree.
                let mut sum = 0u64;
                for pid in &pids {
                    if let Some(kib) = read_vm_rss(*pid) {
                        alive = true;
                        sum += kib;
                    }
                }
                sum
            }
        };
        if !alive {
            break;
        }
        if editor_pid.is_some() && read_vm_rss(editor_pid.unwrap()).is_none() {
            rss = 0;
        }
        scope.at_ms = started.elapsed().as_millis();
        samples.push((scope.at_ms, rss));
        // 100 ms: a thread-spawn failure can abort the editor within a few
        // hundred milliseconds, and the receipt must capture the editor's own
        // scope/rlimits before it disappears (500 ms sampled only the parent).
        thread::sleep(Duration::from_millis(100));
    }
    let quarters = quarter_means(&samples);
    Sampled {
        samples,
        scope,
        quarters,
    }
}

#[cfg(target_os = "linux")]
fn quarter_means(samples: &[(u128, u64)]) -> [Option<(f64, f64)>; 4] {
    let mut out = [None; 4];
    if samples.len() < 8 {
        return out;
    }
    let t0 = samples.first().expect("non-empty").0 as f64;
    let t1 = samples.last().expect("non-empty").0 as f64;
    if t1 <= t0 {
        return out;
    }
    let mut sums = [(0.0f64, 0.0f64, 0usize); 4];
    for (t, rss) in samples {
        let quarter = (((*t as f64 - t0) * 4.0 / (t1 - t0)) as usize).min(3);
        sums[quarter].0 += *t as f64;
        sums[quarter].1 += *rss as f64;
        sums[quarter].2 += 1;
    }
    for (quarter, (sum_t, sum_rss, count)) in sums.into_iter().enumerate() {
        if count > 0 {
            out[quarter] = Some((sum_t / count as f64, sum_rss / count as f64));
        }
    }
    out
}

/// The editor is the only process in the sandbox whose executable is
/// `neomacs` or `emacs`; its cgroup is the transient scope we must read.
#[cfg(target_os = "linux")]
fn is_editor_process(pid: u32) -> bool {
    let Ok(exe) = fs::read_link(format!("/proc/{pid}/exe")) else {
        return false;
    };
    matches!(
        exe.file_name().and_then(|name| name.to_str()),
        Some("neomacs") | Some("emacs")
    )
}

/// The systemd scope path from a process's cgroup v2 membership.
#[cfg(target_os = "linux")]
fn scope_of(pid: u32) -> Option<PathBuf> {
    let text = fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("0::") {
            return Some(PathBuf::from(format!("/sys/fs/cgroup{rest}")));
        }
    }
    None
}

/// `scope_of`, restricted to a transient systemd `run-*.scope` -- the
/// sandbox's own scope. Anything else (the user manager's app.slice, a
/// session scope) mixes unrelated accounting into the receipt.
#[cfg(target_os = "linux")]
fn transient_scope_of(pid: u32) -> Option<PathBuf> {
    let path = scope_of(pid)?;
    let name = path.file_name()?.to_str()?;
    if name.starts_with("run-") {
        Some(path)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn read_scope(path: &Path, scope: &mut ScopeStats) {
    if let Ok(text) = fs::read_to_string(path.join("memory.peak")) {
        if let Ok(bytes) = text.trim().parse::<u64>() {
            scope.peak_kib = scope.peak_kib.max(bytes / 1024);
        }
    }
    if let Ok(text) = fs::read_to_string(path.join("memory.max")) {
        scope.memory_max_raw = text.trim().to_owned();
    }
    if let Ok(text) = fs::read_to_string(path.join("pids.current")) {
        if let Ok(value) = text.trim().parse::<u64>() {
            scope.pids_current = value;
        }
    }
    if let Ok(text) = fs::read_to_string(path.join("pids.max")) {
        scope.pids_max_raw = text.trim().to_owned();
    }
    if let Ok(text) = fs::read_to_string(path.join("memory.events")) {
        for line in text.lines() {
            let mut columns = line.split_whitespace();
            if let (Some(name), Some(value)) = (columns.next(), columns.next()) {
                if let Ok(value) = value.parse::<u64>() {
                    match name {
                        "max" => scope.max_events = scope.max_events.max(value),
                        "oom" => scope.oom_events = scope.oom_events.max(value),
                        _ => {}
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn read_limits(pid: u32, scope: &mut ScopeStats) {
    let Ok(text) = fs::read_to_string(format!("/proc/{pid}/limits")) else {
        return;
    };
    for line in text.lines() {
        let columns: Vec<&str> = line.split_whitespace().collect();
        if line.starts_with("Max processes") {
            scope.rlimit_nproc = columns.get(2).and_then(|value| value.parse().ok());
        } else if line.starts_with("Max address space") {
            scope.rlimit_as_raw = columns.get(3).unwrap_or(&"?").to_string();
        }
    }
}

#[cfg(target_os = "linux")]
fn read_host_memory(scope: &mut ScopeStats) {
    let Ok(text) = fs::read_to_string("/proc/meminfo") else {
        return;
    };
    for line in text.lines() {
        let columns: Vec<&str> = line.split_whitespace().collect();
        match columns.first().copied() {
            Some("MemAvailable:") => {
                scope.host_available_kib = columns.get(1).and_then(|v| v.parse().ok()).unwrap_or(0)
            }
            Some("Committed_AS:") => {
                scope.host_committed_kib = columns.get(1).and_then(|v| v.parse().ok()).unwrap_or(0)
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "linux")]
fn cmdline_matches(needle: &str) -> Vec<u32> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            if let Ok(cmdline) = fs::read(format!("/proc/{pid}/cmdline")) {
                if String::from_utf8_lossy(&cmdline).contains(needle) {
                    out.push(pid);
                }
            }
        }
    }
    out
}

/// Row-name cmdline matches restricted to the run's own sandbox scope.
#[cfg(target_os = "linux")]
fn scoped_cmdline_matches(needle: &str, root_scope: Option<&Path>) -> Vec<u32> {
    let Some(root_scope) = root_scope else {
        return Vec::new();
    };
    cmdline_matches(needle)
        .into_iter()
        .filter(|pid| scope_of(*pid).as_deref() == Some(root_scope))
        .collect()
}

/// Descendants of `root` (inclusive) by walking /proc ppid links.
#[cfg(target_os = "linux")]
fn process_tree(root: u32) -> Vec<u32> {
    let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
                // ppid is field 4; the comm field may contain spaces, so
                // split after the closing ')'.
                if let Some((_, tail)) = stat.rsplit_once(')') {
                    let fields: Vec<&str> = tail.split_whitespace().collect();
                    if let Some(ppid) = fields.get(1).and_then(|v| v.parse::<u32>().ok()) {
                        children.entry(ppid).or_default().push(pid);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        out.push(pid);
        if let Some(kids) = children.get(&pid) {
            stack.extend(kids.iter().copied());
        }
    }
    out
}

/// Least-squares slope (KiB/min) over the second half of the samples.
fn rss_slope(samples: &[(u128, u64)]) -> Option<f64> {
    if samples.len() < 4 {
        return None;
    }
    let half = samples.len() / 2;
    let tail = &samples[half..];
    let n = tail.len() as f64;
    let mean_x = tail.iter().map(|(ms, _)| *ms as f64).sum::<f64>() / n;
    let mean_y = tail.iter().map(|(_, kib)| *kib as f64).sum::<f64>() / n;
    let mut num = 0.0;
    let mut den = 0.0;
    for (ms, kib) in tail {
        let x = *ms as f64 - mean_x;
        num += x * (*kib as f64 - mean_y);
        den += x * x;
    }
    if den == 0.0 {
        return Some(0.0);
    }
    Some(num / den * 60_000.0)
}

/// Slope (KiB/min) between the mean points of two quarters.
fn quarter_slope(quarters: &[Option<(f64, f64)>; 4], from: usize, to: usize) -> Option<f64> {
    let (t0, y0) = quarters[from]?;
    let (t1, y1) = quarters[to]?;
    if t1 <= t0 {
        return None;
    }
    Some((y1 - y0) / (t1 - t0) * 60_000.0)
}

fn write_receipt(config: &Config, runs: &[Run], failures: &[String]) -> std::io::Result<()> {
    if let Some(parent) = config.receipt.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(&config.receipt)?;
    writeln!(
        file,
        "arm\tscenario\tseed\tmode\tsandbox_mem_gb\titers\twall_ms\tinstructions\trss_start_kib\trss_end_kib\trss_slope_kib_per_min\trss_q1_kib\trss_q2_kib\trss_q3_kib\trss_q4_kib\trss_slope_prev_kib_min\trss_slope_last_kib_min\tgc_start\tgc_end\theap_start_kib\theap_end_kib\tuse_start\tuse_end\tlive_start_bytes\tlive_end_bytes\tbacking_start_bytes\tbacking_end_bytes\tobjects_start\tobjects_end\tthread_objs_start\tthread_objs_end\tscope_peak_kib\tscope_mem_max\tscope_pids_current\tscope_pids_max\tscope_max_events\tscope_oom_events\trlimit_nproc\trlimit_as\thost_memavail_kib\thost_committed_as_kib\tscope_sample_ms\tsandbox_mode\trc\tresult\tfailure"
    )?;
    for run in runs {
        let iters = run.diag.get("iters").cloned().unwrap_or_default();
        let slope = run
            .rss_slope_kib_per_min
            .map(|s| format!("{s:.0}"))
            .unwrap_or_default();
        writeln!(
            file,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            run.arm,
            run.scenario,
            config.seed,
            config.mode,
            run.sandbox_mem_gb,
            iters,
            run.wall_ms,
            run.instructions.map(|v| v.to_string()).unwrap_or_default(),
            run.rss_start_kib.map(|v| v.to_string()).unwrap_or_default(),
            run.rss_end_kib.map(|v| v.to_string()).unwrap_or_default(),
            slope,
            run.rss_quarters_kib[0]
                .map(|v| format!("{v:.0}"))
                .unwrap_or_default(),
            run.rss_quarters_kib[1]
                .map(|v| format!("{v:.0}"))
                .unwrap_or_default(),
            run.rss_quarters_kib[2]
                .map(|v| format!("{v:.0}"))
                .unwrap_or_default(),
            run.rss_quarters_kib[3]
                .map(|v| format!("{v:.0}"))
                .unwrap_or_default(),
            run.rss_slope_prev_kib_per_min
                .map(|v| format!("{v:.0}"))
                .unwrap_or_default(),
            run.rss_slope_last_kib_per_min
                .map(|v| format!("{v:.0}"))
                .unwrap_or_default(),
            run.gc_start.map(|v| v.to_string()).unwrap_or_default(),
            run.gc_end.map(|v| v.to_string()).unwrap_or_default(),
            run.heap_start_kib
                .map(|v| v.to_string())
                .unwrap_or_default(),
            run.heap_end_kib.map(|v| v.to_string()).unwrap_or_default(),
            run.use_start,
            run.use_end,
            run.live_start_bytes
                .map(|v| v.to_string())
                .unwrap_or_default(),
            run.live_end_bytes
                .map(|v| v.to_string())
                .unwrap_or_default(),
            run.backing_start_bytes
                .map(|v| v.to_string())
                .unwrap_or_default(),
            run.backing_end_bytes
                .map(|v| v.to_string())
                .unwrap_or_default(),
            run.objects_start.map(|v| v.to_string()).unwrap_or_default(),
            run.objects_end.map(|v| v.to_string()).unwrap_or_default(),
            run.thread_objs_start
                .map(|v| v.to_string())
                .unwrap_or_default(),
            run.thread_objs_end
                .map(|v| v.to_string())
                .unwrap_or_default(),
            run.scope_peak_kib,
            run.scope_memory_max,
            run.scope_pids_current,
            run.scope_pids_max,
            run.scope_max_events,
            run.scope_oom_events,
            run.rlimit_nproc.map(|v| v.to_string()).unwrap_or_default(),
            run.rlimit_as,
            run.host_available_kib,
            run.host_committed_kib,
            run.scope_sample_ms,
            run.sandbox_mode,
            run.rc
                .map(|v| v.to_string())
                .unwrap_or_else(|| "signal".to_owned()),
            if run.ok() { "ok" } else { "FAIL" },
            run.stderr_tail,
        )?;
    }
    if !failures.is_empty() {
        writeln!(file, "\n# FAILURES")?;
        for failure in failures {
            writeln!(file, "# {failure}")?;
        }
    }
    Ok(())
}

/// Record one completed run's transport/RSS health. `enforce_rss` is set for
/// the long arm runs; the short reference/smoke runs do not have a slope.
fn check_run(run: &Run, enforce_rss: bool, rss_limit: f64, failures: &mut Vec<String>) {
    if run.timed_out {
        failures.push(format!("{}: {} timed out", run.scenario, run.arm));
    } else if run.rc != Some(0) {
        failures.push(format!(
            "{}: {} exited {:?} ({})",
            run.scenario, run.arm, run.rc, run.stderr_tail
        ));
    }
    if enforce_rss {
        // Sustained-growth leak gate: the editor's RSS slope over the last
        // quarter must exceed the stated limit AND not be a decaying warmup
        // transient (a warmup shows Q2->Q3 slopes far above Q3->Q4), with at
        // least 8 MiB of growth across the last half. start/end samples are
        // not used: the first tick predates the editor and the last can catch
        // its teardown, which made the old end-minus-start rule noisy.
        let q2 = run.rss_quarters_kib[1];
        let q4 = run.rss_quarters_kib[3];
        let grown = match (q2, q4) {
            (Some(early), Some(late)) => late - early,
            _ => 0.0,
        };
        if let Some(slope_last) = run.rss_slope_last_kib_per_min {
            let prior = run.rss_slope_prev_kib_per_min.unwrap_or(0.0).max(0.0);
            if grown > 8.0 * 1024.0 && slope_last > rss_limit && slope_last > 0.5 * prior {
                failures.push(format!(
                    "{}: {} sustained RSS growth: last-quarter slope {slope_last:.0} KiB/min \
                     (prior quarter {prior:.0}) over {grown:.0} KiB from q2 to q4 (limit {rss_limit})",
                    run.scenario, run.arm
                ));
            }
        }
    }
}

/// The single test entry point. Smoke mode is the <= 2 min gate; the long
/// arms run the same binary directly (`--exact thread_soak::soak`) with SOAK_MODE=arm,
/// compared against the same-seed reference from SOAK_MODE=reference.
#[test]
fn soak() {
    let config = Config::from_env();
    assert!(
        config.sandbox.is_file(),
        "sandbox wrapper missing at {} (SOAK_SANDBOX)",
        config.sandbox.display()
    );
    assert!(
        config.rows.is_dir(),
        "row directory missing at {} (SOAK_ROWS)",
        config.rows.display()
    );
    assert!(
        config.bin.join("neomacs").is_file(),
        "editor binary missing at {}; build it with `cargo xtask fresh-build --release` \
         or point SOAK_BIN (or NEOVM_BINARY_PATH/NEOMACS_BIN) at a built editor",
        config.bin.display()
    );

    let mut runs = Vec::new();
    let mut failures = Vec::new();
    let mut reference_rows: Vec<(String, String, Option<String>)> = Vec::new();
    let reference = if config.role == Role::Arm {
        Some(read_reference(&config.reference))
    } else {
        None
    };
    if config.role == Role::Arm && reference.as_ref().is_some_and(|r| r.is_empty()) {
        panic!(
            "reference file {} is missing or empty; run SOAK_MODE=reference first",
            config.reference.display()
        );
    }

    let selected = config.selected();
    for scenario in &selected {
        let row = config.rows.join(scenario.row);
        if !row.is_file() {
            failures.push(format!(
                "{}: row file missing at {}",
                scenario.name,
                row.display()
            ));
            continue;
        }
        if let Some(reference) = &reference {
            let Some((sim_state, gnu_state)) = reference.get(scenario.name) else {
                failures.push(format!("{}: no reference row", scenario.name));
                continue;
            };
            let run = run_row(&config, scenario, config.arm, false);
            if run.state.as_deref() != Some(sim_state.as_str()) {
                failures.push(format!(
                    "{}: {} state {:?} != sim reference {:?} (seed {})",
                    scenario.name,
                    config.arm.name(),
                    run.state,
                    sim_state,
                    config.seed
                ));
            }
            // Minimum-window rule: the RSS-slope gate classifies only runs of
            // at least 60 s (the quarter means need a real trend; short
            // windows extrapolate a startup transient into a huge slope).
            check_run(
                &run,
                run.wall_ms >= 60_000,
                config.rss_limit_kib_per_min,
                &mut failures,
            );
            runs.push(run);
            if let Some(gnu_state) = gnu_state {
                if !gnu_state.is_empty() {
                    runs.last_mut()
                        .expect("just pushed")
                        .diag
                        .insert("gnu_reference_state".to_owned(), gnu_state.clone());
                }
            }
            continue;
        }

        // Smoke and reference roles: run Sim (the reference state).
        let sim = run_row(&config, scenario, Arm::Sim, false);
        let sim_state = sim.state.clone();
        if sim_state.is_none() {
            failures.push(format!(
                "{}: sim reference produced no SOAK-RESULT state",
                scenario.name
            ));
        }
        check_run(&sim, false, config.rss_limit_kib_per_min, &mut failures);
        runs.push(sim);
        let mut gnu_state = None;
        if config.run_gnu && scenario.gnu {
            let gnu = run_row(&config, scenario, Arm::Sim, true);
            if gnu.state != sim_state {
                failures.push(format!(
                    "{}: gnu state {:?} != sim reference {:?} (seed {})",
                    scenario.name, gnu.state, sim_state, config.seed
                ));
            }
            check_run(&gnu, false, config.rss_limit_kib_per_min, &mut failures);
            gnu_state = gnu.state.clone();
            runs.push(gnu);
        }
        if config.role == Role::Smoke && config.arm != Arm::Sim {
            let arm = run_row(&config, scenario, config.arm, false);
            if arm.state != sim_state {
                failures.push(format!(
                    "{}: {} state {:?} != sim reference {:?} (seed {})",
                    scenario.name,
                    config.arm.name(),
                    arm.state,
                    sim_state,
                    config.seed
                ));
            }
            check_run(&arm, false, config.rss_limit_kib_per_min, &mut failures);
            runs.push(arm);
        }
        if config.role == Role::Reference {
            reference_rows.push((
                scenario.name.to_owned(),
                sim_state.unwrap_or_default(),
                gnu_state,
            ));
        }
    }

    if config.role == Role::Reference {
        write_reference(&config.reference, &reference_rows).expect("write reference");
    }
    write_receipt(&config, &runs, &failures).expect("write receipt");
    eprintln!(
        "SOAK {} arm={} seed={} scenarios={} runs={} failures={} receipt={} reference={}",
        config.mode,
        config.arm.name(),
        config.seed,
        selected.len(),
        runs.len(),
        failures.len(),
        config.receipt.display(),
        config.reference.display()
    );
    assert!(
        failures.is_empty(),
        "soak failures:\n{}",
        failures.join("\n")
    );
}
