//! Keep unrelated fixture threads out of the process-wide A8 recording.

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProbeScope {
    Recording,
    Suppressed,
}

thread_local! {
    static SCOPE: Cell<ProbeScope> = const { Cell::new(ProbeScope::Recording) };
}

/// Restore the calling thread's probe scope, including when it unwinds.
#[must_use = "hold this guard for the fixture's entire probe-free scope"]
pub(crate) struct Suppression {
    previous: ProbeScope,
    _thread: PhantomData<Rc<()>>,
}

pub(crate) fn suppress() -> Suppression {
    Suppression {
        previous: SCOPE.with(|scope| scope.replace(ProbeScope::Suppressed)),
        _thread: PhantomData,
    }
}

pub(super) fn scope() -> ProbeScope {
    SCOPE.with(Cell::get)
}

impl Drop for Suppression {
    fn drop(&mut self) {
        SCOPE.with(|scope| scope.set(self.previous));
    }
}

static_assertions::assert_not_impl_any!(Suppression: Send, Sync, Clone, Copy);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a8_probe_suppression_restores_nested_scopes() {
        assert_eq!(scope(), ProbeScope::Recording);
        {
            let _outer = suppress();
            assert_eq!(scope(), ProbeScope::Suppressed);
            {
                let _inner = suppress();
                assert_eq!(scope(), ProbeScope::Suppressed);
            }
            assert_eq!(scope(), ProbeScope::Suppressed);
        }
        assert_eq!(scope(), ProbeScope::Recording);
    }

    #[test]
    fn a8_probe_suppression_restores_after_unwind() {
        assert_eq!(scope(), ProbeScope::Recording);
        let outcome = std::panic::catch_unwind(|| {
            let _scope = suppress();
            assert_eq!(scope(), ProbeScope::Suppressed);
            panic!("exercise probe scope cleanup");
        });
        assert!(outcome.is_err());
        assert_eq!(scope(), ProbeScope::Recording);
    }

    #[test]
    fn a8_probe_suppression_is_thread_local() {
        let _scope = suppress();
        std::thread::spawn(|| {
            assert_eq!(
                scope(),
                ProbeScope::Recording,
                "a child thread has its own probe scope"
            );
            let _scope = suppress();
            assert_eq!(scope(), ProbeScope::Suppressed);
        })
        .join()
        .unwrap();
        assert_eq!(scope(), ProbeScope::Suppressed);
    }

    #[test]
    fn a8_probe_suppression_skips_sequence_and_recording() {
        let _scope = suppress();
        assert_eq!(super::super::native_seq(), 0);
        // An enabled recording would need this same lock. The suppressed
        // mark returns before touching it, irrespective of global enablement.
        let events = super::super::EVENTS
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        super::super::mark(super::super::NATIVE_RETURNED, 0, 0);
        drop(events);
    }
}
