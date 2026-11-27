//! Regression probes run in sandboxed children: an unmapped callback crashes.

use neovm_system::emacs_core::dynamic_module::*;
use neovm_value::error::FlowKind;
use neovm_value::value::Value;
use std::path::PathBuf;
use std::process::{Command, Output};

const CHILD_CASE: &str = "NEOVM_MODULE_LIFETIME_CASE";
const CHILD_LIBRARY: &str = "NEOVM_MODULE_LIFETIME_LIBRARY";
const CHILD_EXPECT: &str = "NEOVM_MODULE_LIFETIME_EXPECT";

/// Tracked portable sandbox launcher (capability-probes systemd scope / PID
/// namespace / direct exec; also used by `tests/thread_soak.rs`).
const TRACKED_SANDBOX_REL: &str = "crates/neovm-core/tests/soak/sandbox-run.sh";

const MODULE_SOURCE: &str = r#"
#include <emacs-module.h>
int plugin_is_GPL_compatible;
static emacs_value hello (emacs_env *env, ptrdiff_t n, emacs_value *args, void *data)
{
  return env->make_integer (env, RESULT_CODE);
}
int emacs_module_init (struct emacs_runtime *rt)
{
  emacs_env *env = rt->get_environment (rt);
  emacs_value fn = env->make_function (env, 0, 0, hello, "doc", NULL);
  emacs_value args[] = { env->intern (env, "FUNCTION_NAME"), fn };
  env->funcall (env, env->intern (env, "defalias"), 2, args);
  PENDING_EXIT
  return INIT_STATUS;
}
"#;

fn checked_output(mut command: Command) -> Output {
    let output = command.output().expect("start module lifetime child");
    assert!(
        output.status.success(),
        "module lifetime child failed: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

fn sandbox() -> Option<PathBuf> {
    if let Some(override_path) = std::env::var_os("NEOVM_SANDBOX_RUN").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(override_path));
    }
    let root = neomacs_infra::workspace_root();
    let tracked = root.join(TRACKED_SANDBOX_REL);
    if tracked.is_file() {
        return Some(tracked);
    }
    root.ancestors()
        .map(|ancestor| ancestor.join("tmp/v10x/sandbox-run.sh"))
        .find(|path| path.is_file())
}

/// Resolve the directory containing GNU Emacs's `emacs-module.h`:
/// 1. `GNU_EMACS_SOURCE` (either `<dir>/src` or `<dir>`),
/// 2. `NEOVM_FORCE_ORACLE_PATH` sibling `<prefix>/include`,
/// 3. `<workspace_root>` (or an ancestor)'s `tmp/gnu-emacs-oracle/include`,
/// 4. CI's attested oracle prefix `/opt/gnu-emacs-oracle/include`.
fn emacs_module_header_dir() -> Option<PathBuf> {
    if let Some(src) = std::env::var_os("GNU_EMACS_SOURCE").filter(|v| !v.is_empty()) {
        let base = PathBuf::from(src);
        let src_dir = base.join("src");
        if src_dir.join("emacs-module.h").is_file() {
            return Some(src_dir);
        }
        if base.join("emacs-module.h").is_file() {
            return Some(base);
        }
        return None;
    }
    if let Some(oracle_bin) = std::env::var_os("NEOVM_FORCE_ORACLE_PATH").filter(|v| !v.is_empty())
    {
        let bin_path = PathBuf::from(oracle_bin);
        if let Some(prefix) = bin_path.parent().and_then(|p| p.parent()) {
            let include = prefix.join("include");
            if include.join("emacs-module.h").is_file() {
                return Some(include);
            }
        }
    }
    let root = neomacs_infra::workspace_root();
    if let Some(include) = root
        .ancestors()
        .map(|ancestor| ancestor.join("tmp/gnu-emacs-oracle/include"))
        .find(|dir| dir.join("emacs-module.h").is_file())
    {
        return Some(include);
    }
    let ci_include = PathBuf::from("/opt/gnu-emacs-oracle/include");
    if ci_include.join("emacs-module.h").is_file() {
        return Some(ci_include);
    }
    None
}

/// A child either inherits the runner's PID isolation or enters it through
/// the campaign launcher. A systemd scope cannot be nested from that PID
/// namespace: its bus peer cannot authenticate the namespace-local process.
enum ChildIsolation {
    Inherited,
    Launch(PathBuf),
}

fn lifetime_case(case: &str, result_code: i32, init_status: i32, pending_exit: &str) {
    let function = format!("{case}-hello");
    if std::env::var(CHILD_CASE).as_deref() == Ok(case) {
        let path = PathBuf::from(std::env::var_os(CHILD_LIBRARY).expect("child library"));
        let expected = std::env::var(CHILD_EXPECT).expect("GNU oracle result");
        let (expected_condition, expected_integer) = expected.trim().split_once(':').unwrap();
        let mut context = crate::emacs_core::eval_hook_installers::new_context();
        let failure = load_module(&mut context, path.clone()).unwrap_err();
        let FlowKind::Signal(failure) = failure.into_kind() else {
            panic!("expected module initialization signal");
        };
        assert_eq!(failure.symbol_name(), expected_condition);
        let callback = context
            .eval_str(&format!("(symbol-function '{function})"))
            .unwrap();
        let result = apply_module_function(&mut context, callback, Vec::new()).unwrap();
        assert_eq!(result.as_int().unwrap().to_string(), expected_integer);
        context
            .eval_str(&format!(
                "(defalias 'saved-{function} (symbol-function '{function}))"
            ))
            .unwrap();
        // Reloading and replacing registry entries must also leave saved
        // module functions callable.
        // Exercise Lisp signal normalization using the saved GNU oracle's
        // error handler. Bind the path as a real Lisp string, avoiding source
        // escaping assumptions for quotes, backslashes or non-ASCII paths.
        context.obarray_mut().set_symbol_value(
            "tsb-module-lifetime-reload-path",
            Value::string(path.to_str().expect("fixture path is Unicode")),
        );
        let observed = context
            .eval_str(
                "(condition-case e (module-load tsb-module-lifetime-reload-path) (error (car e)))",
            )
            .unwrap();
        assert_eq!(
            neovm_eval::emacs_core::print::print_value(&observed),
            expected_condition,
            "Lisp dispatch must preserve the saved GNU module condition",
        );
        context.gc_collect_exact();
        let result = apply_module_function(&mut context, callback, Vec::new()).unwrap();
        assert_eq!(result.as_int().unwrap().to_string(), expected_integer);
        println!(
            "library_lifetime child verified: case={case} condition={expected_condition} integer={expected_integer}"
        );
        return;
    }

    let isolation = if std::env::var("CARFRAME_TEST_IN_SANDBOX").as_deref() == Ok("1") {
        ChildIsolation::Inherited
    } else {
        let sandbox = sandbox().unwrap_or_else(|| {
            panic!(
                "module lifetime probe ({case}) requires a sandbox launcher: \
                 expected tracked {TRACKED_SANDBOX_REL} under {} \
                 (or set NEOVM_SANDBOX_RUN or CARFRAME_TEST_IN_SANDBOX=1)",
                neomacs_infra::workspace_root().display(),
            )
        });
        assert!(
            sandbox.is_file(),
            "module lifetime probe ({case}) sandbox launcher does not exist: {}",
            sandbox.display(),
        );
        ChildIsolation::Launch(sandbox)
    };

    let header_directory = emacs_module_header_dir().unwrap_or_else(|| {
        panic!(
            "module lifetime probe ({case}) requires emacs-module.h: \
             checked GNU_EMACS_SOURCE, NEOVM_FORCE_ORACLE_PATH sibling include/, \
             tmp/gnu-emacs-oracle/include under {} (and ancestors), and \
             /opt/gnu-emacs-oracle/include (build the oracle prefix via \
             scripts/ci/build-gnu-oracle.sh)",
            neomacs_infra::workspace_root().display(),
        )
    });

    let tempdir = tempfile::Builder::new()
        .prefix(&format!("tsb-module-{case}-"))
        .tempdir()
        .expect("create temporary directory for module lifetime probe");
    let source = tempdir.path().join("module.c");
    let library = tempdir.path().join("module.so");
    std::fs::write(
        &source,
        MODULE_SOURCE
            .replace("RESULT_CODE", &result_code.to_string())
            .replace("FUNCTION_NAME", &function)
            .replace("PENDING_EXIT", pending_exit)
            .replace("INIT_STATUS", &init_status.to_string()),
    )
    .unwrap();
    let mut compiler = Command::new("cc");
    compiler
        .args(["-shared", "-fPIC", "-I"])
        .arg(&header_directory)
        .arg(&source)
        .arg("-o")
        .arg(&library);
    let output = compiler.output().unwrap_or_else(|error| {
        panic!("module lifetime probe ({case}) requires a C compiler (`cc`): {error}")
    });
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut expected = match case {
        "failinit" => include_str!("library_lifetime/failinit.expect").to_owned(),
        "signalinit" => include_str!("library_lifetime/signalinit.expect").to_owned(),
        "combinedinit" => include_str!("library_lifetime/combinedinit.expect").to_owned(),
        _ => panic!("unknown module lifetime fixture"),
    };

    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let form = format!(
            "(progn (condition-case e (module-load {:?}) (error (prin1 (car e)))) (princ \":\") (princ ({function})))",
            library.to_str().unwrap(),
        );
        let mut oracle = crate::test_utils::gnu_refresh_command();
        oracle.args(["-Q", "--batch", "--eval"]).arg(form);
        let output = oracle.output().expect("start sandboxed GNU oracle");
        assert!(
            output.status.success(),
            "GNU module lifetime oracle failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        expected = String::from_utf8(output.stdout).unwrap();
        let fixture = neomacs_infra::crate_root!()
            .join("src/emacs_core/tests/moved_system/emacs_core/system/dynamic_module/tests/library_lifetime")
            .join(format!("{case}.expect"));
        std::fs::write(fixture, format!("{}\n", expected.trim())).unwrap();
    }

    let test_name = format!(
        "{}::{case}_keeps_published_function_callable",
        module_path!()
    );
    let executable = std::env::current_exe().unwrap();
    let (program, prefix) = match isolation {
        ChildIsolation::Inherited => (executable, Vec::new()),
        ChildIsolation::Launch(sandbox) => (sandbox, vec![executable]),
    };
    let mut child = Command::new(program);
    child
        .args(prefix)
        .args([
            "--exact",
            test_name.strip_prefix("neovm_core::").unwrap(),
            "--nocapture",
        ])
        .env(CHILD_CASE, case)
        .env(CHILD_LIBRARY, library)
        .env(CHILD_EXPECT, expected.trim())
        .env("UPDATE_EXPECT", "0");
    let output = checked_output(child);
    let child_stdout = String::from_utf8_lossy(&output.stdout);
    let (expected_condition, expected_integer) = expected.trim().split_once(':').unwrap();
    let expected_marker = format!(
        "library_lifetime child verified: case={case} condition={expected_condition} integer={expected_integer}"
    );
    print!("{child_stdout}");
    assert!(
        child_stdout.contains("1 passed; 0 failed"),
        "sandbox child did not run its selected regression test: {child_stdout}",
    );
    assert!(
        child_stdout.contains(&expected_marker),
        "sandbox child did not emit verification marker `{expected_marker}`: {child_stdout}",
    );
}

#[test]
fn failinit_keeps_published_function_callable() {
    lifetime_case("failinit", 42, 1, "");
}

#[test]
fn signalinit_keeps_published_function_callable() {
    lifetime_case(
        "signalinit",
        43,
        0,
        "env->non_local_exit_signal (env, env->intern (env, \"error\"), env->intern (env, \"nil\"));",
    );
}

/// GNU processes a nonzero return code before a pending module signal.
#[test]
fn combinedinit_keeps_published_function_callable() {
    lifetime_case(
        "combinedinit",
        44,
        1,
        "env->non_local_exit_signal (env, env->intern (env, \"error\"), env->intern (env, \"nil\"));",
    );
}

#[test]
fn sandbox_resolves_tracked_launcher_by_default() {
    // Only assert the default resolution when no explicit override is set in the
    // caller's environment (so operator overrides via NEOVM_SANDBOX_RUN remain
    // valid under this test too).
    if std::env::var_os("NEOVM_SANDBOX_RUN").is_some_and(|v| !v.is_empty()) {
        return;
    }
    let resolved = sandbox().expect("tracked sandbox-run.sh must resolve on a clean checkout");
    assert!(
        resolved.ends_with(TRACKED_SANDBOX_REL),
        "default sandbox must resolve tracked {TRACKED_SANDBOX_REL}, got {}",
        resolved.display(),
    );
    assert!(resolved.is_file());
}

#[test]
fn emacs_module_header_resolves_from_oracle_or_env() {
    let dir = emacs_module_header_dir().expect(
        "emacs-module.h must resolve from GNU_EMACS_SOURCE, NEOVM_FORCE_ORACLE_PATH, \
         tmp/gnu-emacs-oracle/include, or /opt/gnu-emacs-oracle/include",
    );
    assert!(
        dir.join("emacs-module.h").is_file(),
        "resolved header directory does not contain emacs-module.h: {}",
        dir.display(),
    );
}
