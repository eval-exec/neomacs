use super::*;
use crate::emacs_core::Context;
use crate::emacs_core::environment::builtin_getenv_internal;
use crate::emacs_core::value::list_to_vec;
use crate::heap_types::LispString;
use std::time::Duration;

fn find_bin(name: &str) -> String {
    for dir in ["/bin", "/usr/bin", "/run/current-system/sw/bin"] {
        let path = format!("{dir}/{name}");
        if std::path::Path::new(&path).exists() {
            return path;
        }
    }
    if let Ok(output) = std::process::Command::new("which").arg(name).output() {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }
    name.to_string()
}

#[test]
fn builtin_getenv_internal_preserves_raw_unibyte_process_environment_value() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
        "process-environment",
        Value::list(vec![Value::heap_string(LispString::from_unibyte(
            b"NEOMACS_RAW_ENV=\xFF".to_vec(),
        ))]),
    );

    let result = builtin_getenv_internal(
        &mut eval,
        vec![Value::heap_string(LispString::from_unibyte(
            b"NEOMACS_RAW_ENV".to_vec(),
        ))],
    )
    .expect("getenv-internal should succeed");

    let value = result.as_lisp_string().expect("string result");
    assert!(!value.is_multibyte());
    assert_eq!(value.as_bytes(), &[0xFF]);
}

/// `start-process' is Lisp in GNU (lisp/subr.el:3466) and has no Rust subr
/// since DIVERGENCES.md 149, so the raw-byte question this test asks belongs
/// to the primitive underneath it: `make-process', which GNU does DEFUN
/// (src/process.c:1767).
#[test]
fn builtin_make_process_preserves_raw_unibyte_command_argument_storage() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let echo = find_bin("echo");
    let pid = builtin_make_process(
        &mut eval,
        vec![
            Value::keyword(":name"),
            Value::string("raw-make-process"),
            Value::keyword(":command"),
            Value::list(vec![
                Value::string(echo),
                Value::heap_string(LispString::from_unibyte(vec![0xFF])),
            ]),
        ],
    )
    .expect("make-process should succeed")
    .as_process_id()
    .expect("process object id");

    let proc = eval.processes.get(pid).expect("process should exist");
    let command = list_to_vec(&proc.command).expect("process command list");
    let arg = command
        .get(1)
        .and_then(|value| value.as_lisp_string())
        .expect("raw argument should be stored");
    assert!(!arg.is_multibyte());
    assert_eq!(arg.as_bytes(), &[0xFF]);
}

#[test]
fn spawn_child_with_environment_uses_process_environment_list() {
    crate::test_utils::init_test_tracing();
    let shell = find_bin("sh");
    let mut processes = ProcessManager::new();
    let pid = processes.create_process_lisp(
        LispString::from_utf8("raw-env-child"),
        Value::NIL,
        LispString::from_utf8(&shell),
        vec![
            LispString::from_utf8("-c"),
            LispString::from_utf8("printf %s \"$NEOMACS_CHILD_ENV\""),
        ],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
        "process-environment",
        Value::list(vec![Value::heap_string(LispString::from_unibyte(
            b"NEOMACS_CHILD_ENV=from-lisp".to_vec(),
        ))]),
    );
    let env = crate::emacs_core::environment::ChildEnvironment::materialize(&eval, None);

    processes
        .spawn_child_with_environment(pid, false, Some(env))
        .expect("spawn child");

    // The diagnostic mirror (`get_output`) holds DECODED text and so is only
    // filled by the `Context`-side read; a bare `ProcessManager` has no
    // evaluator to decode with.  What this test is about is the child's
    // environment, which is in the bytes, so it collects those.
    let mut output: Vec<u8> = Vec::new();
    for _ in 0..20 {
        let events = processes.wait_for_process_events(Duration::from_millis(20));
        if events.has_ready_process(pid)
            && let Some(run) = processes.read_process_output_without_decoding(
                pid,
                ProcessOutputDestination::to_filter(),
                &crate::emacs_core::coding::CodingSystemManager::new(),
            )
        {
            output.extend_from_slice(run.undecoded_bytes());
        }
        if output == b"from-lisp" {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(output.as_slice(), b"from-lisp");
}
