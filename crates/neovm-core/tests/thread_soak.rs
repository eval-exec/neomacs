//! P11 SOAK: bounded, seeded, checked thread-runtime soak.
//!
//! The harness is an orchestrator, not a Lisp evaluator: every scenario is a
//! row file run by the *built* editor binary inside `tmp/v10x/sandbox-run.sh`
//! (never in-process), so the same sandbox wraps neomacs and the GNU 31.1
//! oracle. One child process = one scenario = one receipt line.
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
//! default 2, and is the <= 2 min gate). The long arms are run by
//! executing the built test binary directly (`--exact soak --nocapture`),
//! which is a plain run of a built target and has no nextest timeout.
//!
//! RSS: `SOAK_RSS_LIMIT_KIB_PER_MIN` (default 2048) bounds the slope of the
//! summed VmRSS of the editor's process subtree after the first minute; a
//! slope above it (and more than 8 MiB of growth) fails the scenario.
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
    Scenario { name: "spawn_join_live_16", row: "soak_spawn_join.el", row_env: &[("SOAK_N", "16")], gnu: true, mem_gb: 8 },
    Scenario { name: "spawn_join_live_100", row: "soak_spawn_join.el", row_env: &[("SOAK_N", "100")], gnu: true, mem_gb: 8 },
    Scenario { name: "spawn_join_live_300", row: "soak_spawn_join.el", row_env: &[("SOAK_N", "300")], gnu: true, mem_gb: 8 },
    Scenario { name: "mutex_ordered", row: "soak_mutex_ordered.el", row_env: &[], gnu: true, mem_gb: 8 },
    Scenario { name: "cond_notify_all", row: "soak_cond.el", row_env: &[("SOAK_KIND", "all")], gnu: true, mem_gb: 8 },
    Scenario { name: "cond_notify_one", row: "soak_cond.el", row_env: &[("SOAK_KIND", "one")], gnu: true, mem_gb: 8 },
    Scenario { name: "yield_loop", row: "soak_yield.el", row_env: &[], gnu: true, mem_gb: 8 },
    Scenario { name: "timers", row: "soak_timers.el", row_env: &[], gnu: true, mem_gb: 8 },
    Scenario { name: "accept_process_output", row: "soak_proc.el", row_env: &[], gnu: true, mem_gb: 8 },
    Scenario { name: "signal_from_callee", row: "soak_signal.el", row_env: &[], gnu: true, mem_gb: 8 },
    Scenario { name: "redefine_inlined_parked", row: "soak_redefine.el", row_env: &[], gnu: true, mem_gb: 8 },
    Scenario { name: "weak_hash_match_cache", row: "soak_weak_cache.el", row_env: &[], gnu: true, mem_gb: 8 },
    Scenario { name: "many_gated_100", row: "soak_many_gated.el", row_env: &[("SOAK_N", "100")], gnu: true, mem_gb: 8 },
    Scenario { name: "many_gated_300", row: "soak_many_gated.el", row_env: &[("SOAK_N", "300")], gnu: true, mem_gb: 8 },
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
        let seed: u64 = env::var("SOAK_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        let scenario_secs = match role {
            Role::Smoke => env::var("SOAK_SMOKE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(1),
            Role::Reference => env::var("SOAK_REF_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(10),
            Role::Arm => {
                let total: u64 = env::var("SOAK_SECONDS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1800);
                (total / SCENARIOS.len() as u64).max(2)
            }
        };
        let receipt = env::var_os("SOAK_RECEIPT")
            .map(PathBuf::from)
            .unwrap_or_else(|| repo.join("tmp/v10x/soak/receipts").join(format!(
                "{}-seed{}-{}.tsv",
                env::var("SOAK_ARM").unwrap_or_else(|_| "sim".to_owned()),
                seed,
                mode,
            )));
        Config {
            role,
            mode,
            arm: Arm::parse(&env::var("SOAK_ARM").unwrap_or_else(|_| "green".to_owned())),
            seed,
            scenario_secs,
            bin: env::var_os("SOAK_BIN")
                .map(PathBuf::from)
                .unwrap_or_else(|| repo.join("tmp/v10x/soak-a814-bin")),
            rows: env::var_os("SOAK_ROWS")
                .map(PathBuf::from)
                .unwrap_or_else(|| repo.join("tmp/v10x/threads-bench/soak-rows")),
            sandbox: env::var_os("SOAK_SANDBOX")
                .map(PathBuf::from)
                .unwrap_or_else(|| repo.join("tmp/v10x/sandbox-run.sh")),
            cpus: env::var("SOAK_CPUS").unwrap_or_else(|_| "0-3,14-31".to_owned()),
            receipt,
            reference: env::var_os("SOAK_REFERENCE").map(PathBuf::from).unwrap_or_else(|| {
                repo.join("tmp/v10x/soak/reference").join(format!("seed{seed}.tsv"))
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
                    if columns[2].is_empty() { None } else { Some(columns[2].to_owned()) },
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
    let tag = format!("{}-{}-seed{}-{}", scenario.name, arm.name(), config.seed, if gnu { "gnu" } else { "neo" });
    let perf_path = out_dir.join(format!("{tag}.perf"));
    let stdout_path = out_dir.join(format!("{tag}.out"));
    let stderr_path = out_dir.join(format!("{tag}.err"));

    let mut command = Command::new(&config.sandbox);
    command.arg("taskset").arg("-c").arg(&config.cpus);
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
        .stdout(Stdio::from(fs::File::create(&stdout_path).expect("stdout file")))
        .stderr(Stdio::from(fs::File::create(&stderr_path).expect("stderr file")));
    for &(key, value) in scenario.row_env {
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
        for var in ["NEOVM_THREADS", "NEOVM_GC_GENERATIONAL", "NEOVM_GC_VERIFY_GENERATIONAL", "NEOVM_JIT_INLINE"] {
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
    let stderr_tail = fs::read_to_string(&stderr_path)
        .unwrap_or_default()
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

/// Sum the counted `instructions:u` lines (hybrid PMU splits into atom+core).
fn read_instructions(path: &Path) -> Option<u128> {
    let text = fs::read_to_string(path).ok()?;
    let mut total: u128 = 0;
    let mut any = false;
    for line in text.lines() {
        let columns: Vec<&str> = line.split(',').collect();
        if columns.len() < 3 || !columns[2].contains("instructions") || line.contains("<not counted>") {
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
}

/// Collect (elapsed_ms, summed_kib) VmRSS of the process subtree, unioned
/// with any process whose cmdline names this row (the PID-namespace hop can
/// reparent the editor away from our direct child), plus the editor's scope
/// accounting, rlimits and host memory pressure. The last tick before the
/// child disappears is the failure instant recorded in the receipt.
fn sample_rss(root_pid: u32, needle: &str) -> Sampled {
    let started = Instant::now();
    let mut samples = Vec::new();
    let mut scope = ScopeStats::default();
    let mut editor_pid: Option<u32> = None;
    let mut scope_path: Option<PathBuf> = None;
    while started.elapsed() < Duration::from_secs(24 * 3600) {
        let mut pids = process_tree(root_pid);
        let matched = cmdline_matches(needle);
        for pid in &matched {
            if !pids.contains(pid) {
                pids.push(*pid);
            }
        }
        if pids.is_empty() {
            break;
        }
        if editor_pid.is_none() {
            for pid in &matched {
                if is_editor_process(*pid) {
                    editor_pid = Some(*pid);
                    break;
                }
            }
        }
        if scope_path.is_none() {
            if let Some(pid) = editor_pid.or_else(|| pids.first().copied()) {
                scope_path = scope_of(pid);
            }
        }
        if let Some(path) = &scope_path {
            read_scope(path, &mut scope);
        }
        if let Some(pid) = editor_pid {
            read_limits(pid, &mut scope);
        }
        read_host_memory(&mut scope);
        scope.editor_pid = editor_pid;
        let mut rss = 0u64;
        let mut alive = false;
        for pid in &pids {
            if let Ok(status) = fs::read_to_string(format!("/proc/{pid}/status")) {
                alive = true;
                for line in status.lines() {
                    if let Some(rest) = line.strip_prefix("VmRSS:") {
                        if let Some(kib) = rest.split_whitespace().next().and_then(|v| v.parse::<u64>().ok()) {
                            rss += kib;
                        }
                    }
                }
            }
        }
        if !alive {
            break;
        }
        scope.at_ms = started.elapsed().as_millis();
        samples.push((scope.at_ms, rss));
        // 100 ms: a thread-spawn failure can abort the editor within a few
        // hundred milliseconds, and the receipt must capture the editor's own
        // scope/rlimits before it disappears (500 ms sampled only the parent).
        thread::sleep(Duration::from_millis(100));
    }
    Sampled { samples, scope }
}

/// The editor is the only process in the sandbox whose executable is
/// `neomacs` or `emacs`; its cgroup is the transient scope we must read.
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
fn scope_of(pid: u32) -> Option<PathBuf> {
    let text = fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("0::") {
            return Some(PathBuf::from(format!("/sys/fs/cgroup{rest}")));
        }
    }
    None
}

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

/// Descendants of `root` (inclusive) by walking /proc ppid links.
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

fn write_receipt(config: &Config, runs: &[Run], failures: &[String]) -> std::io::Result<()> {
    if let Some(parent) = config.receipt.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(&config.receipt)?;
    writeln!(
        file,
        "arm\tscenario\tseed\tmode\tsandbox_mem_gb\titers\twall_ms\tinstructions\trss_start_kib\trss_end_kib\trss_slope_kib_per_min\tscope_peak_kib\tscope_mem_max\tscope_pids_current\tscope_pids_max\tscope_max_events\tscope_oom_events\trlimit_nproc\trlimit_as\thost_memavail_kib\thost_committed_as_kib\tscope_sample_ms\trc\tresult\tfailure"
    )?;
    for run in runs {
        let iters = run.diag.get("iters").cloned().unwrap_or_default();
        let slope = run
            .rss_slope_kib_per_min
            .map(|s| format!("{s:.0}"))
            .unwrap_or_default();
        writeln!(
            file,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
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
            run.rc.map(|v| v.to_string()).unwrap_or_else(|| "signal".to_owned()),
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
        if let Some(slope) = run.rss_slope_kib_per_min {
            let growth = run
                .rss_end_kib
                .unwrap_or(0)
                .saturating_sub(run.rss_start_kib.unwrap_or(0));
            if slope > rss_limit && growth > 8 * 1024 {
                failures.push(format!(
                    "{}: {} RSS slope {slope:.0} KiB/min > {rss_limit} over {growth} KiB",
                    run.scenario, run.arm
                ));
            }
        }
    }
}

/// The single test entry point. Smoke mode is the <= 2 min gate; the long
/// arms run the same binary directly (`--exact soak!`) with SOAK_MODE=arm,
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
        "editor binary missing at {} (SOAK_BIN); build it with the P11 build step",
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
        panic!("reference file {} is missing or empty; run SOAK_MODE=reference first", config.reference.display());
    }

    let selected = config.selected();
    for scenario in &selected {
        let row = config.rows.join(scenario.row);
        if !row.is_file() {
            failures.push(format!("{}: row file missing at {}", scenario.name, row.display()));
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
            check_run(&run, true, config.rss_limit_kib_per_min, &mut failures);
            runs.push(run);
            if let Some(gnu_state) = gnu_state {
                if !gnu_state.is_empty() {
                    runs.last_mut().expect("just pushed").diag.insert(
                        "gnu_reference_state".to_owned(),
                        gnu_state.clone(),
                    );
                }
            }
            continue;
        }

        // Smoke and reference roles: run Sim (the reference state).
        let sim = run_row(&config, scenario, Arm::Sim, false);
        let sim_state = sim.state.clone();
        if sim_state.is_none() {
            failures.push(format!("{}: sim reference produced no SOAK-RESULT state", scenario.name));
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
    assert!(failures.is_empty(), "soak failures:\n{}", failures.join("\n"));
}
