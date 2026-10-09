//! Scoped, thread-confined simulation of an unsupported host let layout.
//!
//! This stores only a test policy, not Lisp state. Each test's mutator thread
//! owns its scope, and no background worker inherits it. The host's shared
//! OnceLock is untouched, so other threads retain their actual probe verdict.

use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LetLayoutPolicy {
    Host,
    Unavailable,
}

thread_local! {
    static POLICY: Cell<LetLayoutPolicy> = const { Cell::new(LetLayoutPolicy::Host) };
}

pub(super) fn is_unavailable() -> bool {
    POLICY.with(|policy| policy.get() == LetLayoutPolicy::Unavailable)
}

pub(crate) fn with_unavailable_let_layout_for_test<R>(f: impl FnOnce() -> R) -> R {
    let _scope = crate::tls_scope::TlsScope::new(&POLICY, LetLayoutPolicy::Unavailable);
    f()
}

#[test]
fn unavailable_let_layout_scope_preserves_the_host_probe() {
    let host = super::let_layout();
    with_unavailable_let_layout_for_test(|| {
        assert_eq!(super::let_layout(), None);
        with_unavailable_let_layout_for_test(|| assert_eq!(super::let_layout(), None));
        assert_eq!(super::let_layout(), None);
        assert_eq!(std::thread::spawn(super::let_layout).join().unwrap(), host);
    });
    assert_eq!(super::let_layout(), host);

    let unwound = std::panic::catch_unwind(|| {
        with_unavailable_let_layout_for_test(|| {
            assert_eq!(super::let_layout(), None);
            panic!("unwind the unavailable layout scope");
        });
    });
    assert!(unwound.is_err());
    assert_eq!(super::let_layout(), host);
}
