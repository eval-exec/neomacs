//! A foreign completion token can never authorize another World's retirement.
//! Native fail-stop is tested in isolated children, not by unwinding through a
//! parked owner. The reference child establishes the host's exact abort status.

use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::threads::{CompletionExit, ThreadName};
use crate::emacs_core::value::Value;

const CHILD_CASE: &str = "P15_COMPLETION_ISOLATED_CHILD";
const TEST_NAME: &str = "emacs_core::eval::green::p15_completion_tests::p15_foreign_completion_cannot_retire_through_another_baton";

#[derive(Clone, Copy)]
enum CompletionChild {
    ReferenceAbort,
    ForeignHeap,
}

impl CompletionChild {
    fn as_env_str(self) -> &'static str {
        match self {
            Self::ReferenceAbort => "reference",
            Self::ForeignHeap => "foreign",
        }
    }
}

#[derive(Debug)]
struct UnknownCompletionChild;

impl TryFrom<&std::ffi::OsStr> for CompletionChild {
    type Error = UnknownCompletionChild;

    fn try_from(value: &std::ffi::OsStr) -> Result<Self, Self::Error> {
        match value.to_str() {
            Some("reference") => Ok(Self::ReferenceAbort),
            Some("foreign") => Ok(Self::ForeignHeap),
            Some(_) | None => Err(UnknownCompletionChild),
        }
    }
}

fn foreign_completion_probe() -> ! {
    let mut first = Context::new_with_threading(ThreadingMode::Sim);
    let native = first.threads.create_thread(Value::NIL, ThreadName::UNNAMED);
    let id = first.threads.get_thread(native).unwrap().id();
    first.threads.finish_thread(native, Value::fixnum(17));
    let completion = first
        .threads
        .prepare_thread_completion(id, CompletionExit::Executed)
        .expect("first World's actual completion token");
    let mut second = Context::new_with_threading(ThreadingMode::Sim);
    let mut baton = second.issue_green_baton().expect("second World's keeper");
    assert_ne!(completion.heap_identity(), baton.parallel_heap_identity());
    assert_eq!(first.threads.all_thread_ids(), vec![0]);
    // The first registry still owns its terminal object (the token itself is
    // not a root). Neither registry may be touched after the mismatch branch.
    assert!(first.threads.thread_handle(native).is_some());
    assert!(second.threads.thread_handle(0).is_some());
    println!("P15_FOREIGN_COMPLETION_AUTHORITY_MISMATCH");
    let _impossible = baton.retire_thread_completion(completion);
    println!("P15_UNEXPECTED_RETIREMENT_RETURNED");
    std::process::exit(99)
}

fn child(case: CompletionChild) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("lib-test executable"))
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env(CHILD_CASE, case.as_env_str())
        .output()
        .expect("isolated completion child")
}

#[test]
fn p15_foreign_completion_cannot_retire_through_another_baton() {
    // This selector exists only in this cfg(test) source file, never in the
    // runtime. It selects two different probes, not a retry of a failed one.
    if let Some(case) = std::env::var_os(CHILD_CASE) {
        match CompletionChild::try_from(case.as_os_str()).expect("test-owned child selector") {
            CompletionChild::ReferenceAbort => {
                println!("P15_REFERENCE_ABORT");
                std::process::abort();
            }
            CompletionChild::ForeignHeap => foreign_completion_probe(),
        }
    }
    let reference = child(CompletionChild::ReferenceAbort);
    assert!(!reference.status.success());
    assert!(String::from_utf8_lossy(&reference.stdout).contains("P15_REFERENCE_ABORT"));
    let refused = child(CompletionChild::ForeignHeap);
    let stdout = String::from_utf8_lossy(&refused.stdout);
    assert!(
        stdout.contains("P15_FOREIGN_COMPLETION_AUTHORITY_MISMATCH"),
        "{refused:?}"
    );
    assert!(!stdout.contains("P15_UNEXPECTED_RETIREMENT_RETURNED"));
    assert_eq!(
        refused.status, reference.status,
        "foreign authority must abort before retirement, never panic or return: {refused:?}"
    );
}
