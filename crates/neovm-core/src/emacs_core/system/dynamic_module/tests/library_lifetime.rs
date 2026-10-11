//! Regression probes run in sandboxed children: an unmapped callback crashes.

use super::*;
use std::path::PathBuf;
use std::process::{Command, Output};

const CHILD_CASE: &str = "NEOVM_MODULE_LIFETIME_CASE";
const CHILD_LIBRARY: &str = "NEOVM_MODULE_LIFETIME_LIBRARY";
const CHILD_EXPECT: &str = "NEOVM_MODULE_LIFETIME_EXPECT";

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
    std::env::var_os("NEOVM_SANDBOX_RUN")
        .map(PathBuf::from)
        .or_else(|| {
            neomacs_infra::workspace_root()
                .ancestors()
                .map(|root| root.join("tmp/v10x/sandbox-run.sh"))
                .find(|path| path.is_file())
        })
}

fn lifetime_case(case: &str, result_code: i32, init_status: i32, pending_exit: &str) {
    let function = format!("{case}-hello");
    if std::env::var(CHILD_CASE).as_deref() == Ok(case) {
        let path = PathBuf::from(std::env::var_os(CHILD_LIBRARY).expect("child library"));
        let expected = std::env::var(CHILD_EXPECT).expect("GNU oracle result");
        let (expected_condition, expected_integer) = expected.trim().split_once(':').unwrap();
        let mut context = Context::new();
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
            crate::emacs_core::print::print_value(&observed),
            expected_condition,
            "Lisp dispatch must preserve the saved GNU module condition",
        );
        context.gc_collect_exact();
        let result = apply_module_function(&mut context, callback, Vec::new()).unwrap();
        assert_eq!(result.as_int().unwrap().to_string(), expected_integer);
        return;
    }

    let Some(sandbox) = sandbox() else {
        tracing::warn!(
            case,
            "module lifetime probe requires a Lisp sandbox; skipped"
        );
        return;
    };

    let directory = neomacs_infra::workspace_root()
        .join("tmp/tsb-modules")
        .join(format!("{case}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("module.c");
    let library = directory.join("module.so");
    std::fs::write(
        &source,
        MODULE_SOURCE
            .replace("RESULT_CODE", &result_code.to_string())
            .replace("FUNCTION_NAME", &function)
            .replace("PENDING_EXIT", pending_exit)
            .replace("INIT_STATUS", &init_status.to_string()),
    )
    .unwrap();
    let header_directory = PathBuf::from("/home/exec/Projects/github.com/emacs-mirror/emacs/src");
    if !header_directory.join("emacs-module.h").is_file() {
        tracing::warn!(
            case,
            "module lifetime probe requires emacs-module.h; skipped"
        );
        return;
    }
    let mut compiler = Command::new("cc");
    compiler
        .args(["-shared", "-fPIC", "-I"])
        .arg(header_directory)
        .arg(&source)
        .arg("-o")
        .arg(&library);
    let output = match compiler.output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!(case, "module lifetime probe requires a C compiler; skipped");
            return;
        }
        Err(error) => panic!("start C compiler: {error}"),
    };
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
        let emacs = std::env::var_os("EMACS")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/bin/emacs")
            });
        let form = format!(
            "(progn (condition-case e (module-load {:?}) (error (prin1 (car e)))) (princ \":\") (princ ({function})))",
            library.to_str().unwrap(),
        );
        let mut oracle = Command::new(&sandbox);
        oracle
            .arg(emacs)
            .args(["-Q", "--batch", "--eval"])
            .arg(form);
        expected = String::from_utf8(checked_output(oracle).stdout).unwrap();
        let fixture = neomacs_infra::crate_root!()
            .join("src/emacs_core/system/dynamic_module/tests/library_lifetime")
            .join(format!("{case}.expect"));
        std::fs::write(fixture, format!("{}\n", expected.trim())).unwrap();
    }

    let test_name = format!(
        "{}::{case}_keeps_published_function_callable",
        module_path!()
    );
    let mut child = Command::new(sandbox);
    child
        .arg(std::env::current_exe().unwrap())
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
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
        "sandbox child did not run its selected regression test: {}",
        String::from_utf8_lossy(&output.stdout),
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
