//! Real compilation routes and emission/report independence.
//! Threading: all compiler overrides, report files, leaves and Contexts are
//! owned by one test invocation; no process environment variable is changed.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::super::*;
use super::{Attempt, ConstructionSite, ROW_CAP, test_support::Scope};
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{
    captured_clif, function, mask_code_text,
};
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::stats::{self, CompileOrigin, ObserveOverride};
use crate::emacs_core::jit::tier2::{CompileTier, T2Upgrade};

/// Threading: invocation-owned scalar compiler overrides, restored on drop;
/// no Lisp cache or process-global environment mutation is introduced. The
/// returned prerequisite guard restores the enclosing scalar override; these
/// constructor/report tests intentionally admit normal Opt without prior OSR.
#[derive(Debug)]
#[must_use = "dropping the guard restores this test thread's compiler settings"]
struct Settings(std::marker::PhantomData<*const ()>);
static_assertions::assert_not_impl_any!(Settings: Send, Sync);
impl Settings {
    #[must_use = "the compiler overrides end when the returned guards are dropped"]
    fn enter() -> (Self, impl Drop) {
        let prerequisite = opt_require_osr_scope_for_test(OptOsrRequirement::Optional);
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses::default()));
        force_opt_profit_for_test(Some(OptProfitMode::Loops));
        force_opt_early_for_test(Some(OptEarlyMode::Off));
        force_opt_max_ops_for_test(Some(0));
        force_inline2_for_test(Some(Inline2Mode::Off));
        force_tier2_for_test(Some(Tier2Knob::from_env(|_| None)));
        force_deopt_for_test(false);
        crate::emacs_core::jit::bg::force_mode_for_test(Some(
            crate::emacs_core::jit::bg::BgMode::Legacy,
        ));
        stats::force_observe_for_test(ObserveOverride::default());
        (Self(std::marker::PhantomData), prerequisite)
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        force_opt_profit_for_test(None);
        force_opt_early_for_test(None);
        force_opt_max_ops_for_test(None);
        force_inline2_for_test(None);
        force_tier2_for_test(None);
        force_deopt_for_test(false);
        crate::emacs_core::jit::bg::force_mode_for_test(None);
        stats::force_observe_for_test(ObserveOverride::default());
    }
}

/// Threading: a unique invocation-owned tmp path, removed on drop. The counter
/// is only a relaxed test filename sequence, with no Lisp/source identity.
#[derive(Debug)]
#[must_use = "dropping the guard removes its owned test report file"]
struct FileScope(PathBuf);
impl FileScope {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tmp/opt-report-test-files");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!(
            "report-{}-{}.tsv",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(!path.exists());
        Self(path)
    }
    fn text(&self) -> String {
        std::fs::read_to_string(&self.0).unwrap()
    }
}
impl Drop for FileScope {
    fn drop(&mut self) {
        if self.0.is_dir() {
            let _ = std::fs::remove_dir(&self.0);
        } else {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

/// Threading: clears only this test mutator's pre-existing compiled cache;
/// its owned Context/leaf state is never shared with another mutator.
#[derive(Debug)]
#[must_use = "dropping the guard clears this test mutator's compiled cache"]
struct CacheScope(std::marker::PhantomData<*const ()>);
static_assertions::assert_not_impl_any!(CacheScope: Send, Sync);
impl CacheScope {
    fn enter() -> Self {
        cache::clear();
        Self(std::marker::PhantomData)
    }
}
impl Drop for CacheScope {
    fn drop(&mut self) {
        cache::clear();
    }
}

fn loop_body(padding: usize) -> ByteCodeFunction {
    let mut ops = Vec::new();
    for _ in 0..padding {
        ops.extend([Op::Nil, Op::Pop]);
    }
    let base = u32::try_from(ops.len()).unwrap();
    ops.extend([
        Op::Constant(0),
        Op::StackRef(0),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(base + 9),
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(base + 1),
        Op::Return,
    ]);
    function(ops, vec![Value::make_int(0)], 1)
}

fn request() -> CompileRequest {
    CompileRequest {
        regalloc: RegallocPolicy::Full,
        bypass_profit_gate: false,
        origin: CompileOrigin::Retier,
        tier: CompileTier::Upgrade(T2Upgrade::Retier),
    }
}

fn compile(f: &ByteCodeFunction) -> CompiledLeaf {
    compile_bytecode_function_requested(f, None, request()).unwrap()
}

fn normalized(records: Vec<String>) -> Vec<String> {
    records
        .into_iter()
        .map(|text| mask_code_text(&text.replace('_', "")))
        .collect()
}

fn records(text: &str) -> Vec<BTreeMap<String, String>> {
    text.lines()
        .map(|line| {
            let mut parts = line.split('\t');
            let mut fields = BTreeMap::from([("tag".into(), parts.next().unwrap().into())]);
            for part in parts {
                let (key, value) = part.split_once('=').unwrap();
                assert!(fields.insert(key.into(), value.into()).is_none());
            }
            assert_eq!(fields["pid"], std::process::id().to_string());
            fields
        })
        .collect()
}
fn num(record: &BTreeMap<String, String>, key: &str) -> u64 {
    record[key].parse().unwrap()
}

#[test]
fn opt_report_clif_parity_keeps_native_observation_and_source_heat_unchanged() {
    let _settings = Settings::enter();
    let mut context = Context::new();
    for observe in [
        ObserveOverride::default(),
        ObserveOverride {
            stats: true,
            naming: true,
            entry_count: true,
        },
    ] {
        stats::force_observe_for_test(observe);
        for mode in [OptMode::Opt, OptMode::Legacy] {
            force_opt_for_test(Some(mode), Some(OptAdmit::ALL));
            let file = FileScope::new();
            let source = loop_body(0);
            let mut clifs = Vec::new();
            let mut source_facts = Vec::new();
            for enabled in [false, true] {
                let _scope = Scope::enter(enabled.then(|| file.0.clone()));
                clifs.push(normalized(captured_clif(|| {
                    let _label = observe
                        .naming
                        .then(|| stats::perf_map::LeafLabelScope::enter(17, None, &source));
                    let leaf = compile(&source);
                    assert_eq!(leaf.obs.entry_counted, observe.entry_count);
                    assert_eq!(leaf.obs.entry_counter().is_some(), observe.entry_count);
                    assert_eq!(leaf.obs.label.is_some(), observe.naming);
                    assert_eq!(stats::entry_counting_enabled(), observe.entry_count);
                    assert_eq!(stats::naming_enabled(), observe.naming);
                    assert_eq!(
                        leaf.call(
                            &mut context as *mut Context as *mut u8,
                            &[Value::make_int(7)]
                        ),
                        NativeRun::Ok(Value::make_int(7).bits())
                    );
                    assert_eq!(leaf.obs.entries.get(), u64::from(observe.entry_count));
                })));
                source_facts.push((
                    source.jit_runtime().heat(),
                    source.jit_runtime().compiled_id(),
                ));
                stats::report_at_exit(&context);
                if !enabled {
                    assert!(!file.0.exists());
                }
            }
            assert!(!clifs[0].is_empty());
            assert_eq!(clifs[0], clifs[1]);
            assert_eq!(source_facts[0], source_facts[1]);
            let output = records(&file.text());
            let final_row = output.last().unwrap();
            assert_eq!(final_row["tag"], "opt-report-final");
            assert_eq!(num(final_row, "complete"), 1);
            assert_eq!(
                num(final_row, "normal_constructed"),
                u64::from(mode == OptMode::Opt)
            );
        }
    }
}

#[test]
fn opt_report_counts_normal_osr_refusal_and_transient_native_construction() {
    let _settings = Settings::enter();
    let _cache = CacheScope::enter();
    let mut context = Context::new();
    let file = FileScope::new();
    let _scope = Scope::enter(Some(file.0.clone()));
    let source = loop_body(0);
    let normal = compile(&source);
    assert_eq!(normal.selected_tier(), SelectedTier::Opt);
    drop(normal); // Successful construction survives immediate leaf destruction.
    let oversized = loop_body(501);
    assert_eq!(compile(&oversized).selected_tier(), SelectedTier::Mir);
    let snapshot = [Value::make_int(2000), Value::make_int(42)];
    context.bc_buf.clear();
    context.bc_buf.extend_from_slice(&snapshot);
    assert_eq!(
        cache::try_run_osr(&mut context, &source, 1, &snapshot, &[]),
        Some(NativeRun::Ok(Value::make_int(2000).bits()))
    );
    let osr = cache::osr_leaf_ptr_for_test(&source, 1).unwrap();
    assert_eq!(unsafe { &*osr }.selected_tier(), SelectedTier::Opt);
    cache::clear(); // Also count an OSR leaf no longer retained at exit.
    let refused = lower_leaf_full_osr_with_opt(
        oversized.executable_ops(),
        &oversized.constants,
        2,
        oversized.executable_gnu_byte_offset_map(),
        None,
        Some(1003),
        0,
        Some(crate::emacs_core::jit::opt::ir::ParamShape {
            required: 1,
            ..Default::default()
        }),
    );
    assert!(matches!(
        refused,
        Err(CompileError::UnsupportedOp("opt-budget:ops"))
    ));
    stats::report_at_exit(&context);
    let output = records(&file.text());
    let final_row = output.last().unwrap();
    assert_eq!(num(final_row, "complete"), 1);
    assert_eq!(num(final_row, "rows"), 4);
    for route in ["normal", "osr"] {
        assert_eq!(num(final_row, &format!("{route}_attempts")), 2);
        assert_eq!(num(final_row, &format!("{route}_refused")), 1);
        assert_eq!(num(final_row, &format!("{route}_constructed")), 1);
        assert_eq!(num(final_row, &format!("{route}_ready")), 1);
        assert_eq!(num(final_row, &format!("{route}_active")), 0);
    }
    assert_eq!(output[0]["origin"], "retier");
    assert_eq!(output[2]["route"], "osr");
    assert_eq!(output[2]["origin"], "osr");
}

#[test]
fn opt_report_row_budget_preserves_all_refused_attempts() {
    let _settings = Settings::enter();
    let context = Context::new();
    force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::default()));
    let file = FileScope::new();
    let _scope = Scope::enter(Some(file.0.clone()));
    let f = function(
        vec![Op::VarRef(0), Op::Return],
        vec![Value::symbol("opt-report-refused-vars")],
        0,
    );
    for _ in 0..ROW_CAP + 3 {
        let refusal = opt_backend::lower_selected_requested(
            f.executable_ops(),
            &f.constants,
            0,
            None,
            None,
            None,
            0,
            crate::emacs_core::jit::opt::ir::ParamShape::default(),
            request(),
        );
        assert!(matches!(
            refusal,
            Err(CompileError::UnsupportedOp("opt-admit:vars"))
        ));
    }
    stats::report_at_exit(&context);
    let output = records(&file.text());
    assert_eq!(output.len(), ROW_CAP + 1);
    for (index, row) in output[..ROW_CAP].iter().enumerate() {
        assert_eq!(num(row, "seq"), (index + 1) as u64);
        assert_eq!(row["outcome"], "refused");
    }
    let final_row = output.last().unwrap();
    assert_eq!(num(final_row, "complete"), 1);
    assert_eq!(num(final_row, "rows"), ROW_CAP as u64);
    assert_eq!(num(final_row, "row_drops"), 3);
    assert_eq!(num(final_row, "normal_attempts"), (ROW_CAP + 3) as u64);
    assert_eq!(num(final_row, "normal_refused"), (ROW_CAP + 3) as u64);
    assert_eq!(num(final_row, "normal_constructed"), 0);
}

#[test]
fn opt_report_seal_rejects_inflight_and_postseal_compilation() {
    let _settings = Settings::enter();
    let context = Context::new();
    let inflight_file = FileScope::new();
    {
        let scope = Scope::enter(Some(inflight_file.0.clone()));
        // Hold the same invocation-owned guard used by the real constructor
        // while exit seals: this models a concurrent unfinished compilation.
        let active = Attempt::begin(ConstructionSite::Normal, Some(request()), 10).unwrap();
        stats::report_at_exit(&context);
        let output = records(&inflight_file.text());
        let final_row = output.last().unwrap();
        assert_eq!(num(final_row, "complete"), 0);
        assert_eq!(num(final_row, "normal_active"), 1);
        drop(active);
        assert_eq!(
            records(&inflight_file.text()).last().unwrap()["tag"],
            "opt-report-late"
        );
        assert_eq!(scope.snapshot().late, 1);
    }
    let late_file = FileScope::new();
    let _scope = Scope::enter(Some(late_file.0.clone()));
    drop(compile(&loop_body(0)));
    stats::report_at_exit(&context);
    assert_eq!(
        num(records(&late_file.text()).last().unwrap(), "complete"),
        1
    );
    drop(compile(&loop_body(0))); // Actual opt compilation after the seal.
    let output = records(&late_file.text());
    assert_eq!(
        output
            .iter()
            .filter(|row| row["tag"] == "opt-report-final")
            .count(),
        1
    );
    assert_eq!(
        output
            .iter()
            .filter(|row| row["tag"] == "opt-report-late")
            .count(),
        2
    );
    assert_eq!(output.last().unwrap()["tag"], "opt-report-late");
}

#[cfg(unix)]
#[test]
fn opt_report_postseal_io_failure_aborts_owned_capture() {
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    const CHILD: &str = "NEOVM_OPT_REPORT_ABORT_FIXTURE";
    const FILE: &str = "NEOVM_OPT_REPORT_ABORT_FIXTURE_FILE";
    if std::env::var_os(CHILD).is_some() {
        let _settings = Settings::enter();
        let context = Context::new();
        let file = FileScope(PathBuf::from(std::env::var_os(FILE).unwrap()));
        let _scope = Scope::enter(Some(file.0.clone()));
        drop(compile(&loop_body(0)));
        stats::report_at_exit(&context);
        assert_eq!(num(records(&file.text()).last().unwrap(), "complete"), 1);
        std::fs::remove_file(&file.0).unwrap();
        std::fs::create_dir(&file.0).unwrap(); // Late append now returns an IO error.
        drop(compile(&loop_body(0)));
        panic!("late invalidation failure must terminate the owned diagnostic capture");
    }
    let file = FileScope::new();
    let name = format!(
        "{}::opt_report_postseal_io_failure_aborts_owned_capture",
        module_path!().split_once("::").unwrap().1
    );
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", &name, "--nocapture"])
        .env(CHILD, "1")
        .env(FILE, &file.0)
        .current_dir(file.0.parent().unwrap())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Only the owned fixture process disables core dumps. Parent/test-global
    // environment and limits are unchanged; no Lisp state crosses pre_exec.
    unsafe {
        command.pre_exec(|| {
            let limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::setrlimit(libc::RLIMIT_CORE, &limit) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
    let mut child = command.spawn().unwrap();
    let pid = child.id();
    let status = child.wait().unwrap();
    assert_eq!(
        status.signal(),
        Some(libc::SIGABRT),
        "owned fixture pid={pid}: {status}"
    );
}
