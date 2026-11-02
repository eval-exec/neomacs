//! Real-specialization layout and registration-refresh probes.
use super::*;
use crate::tagged::header::{SubrDispatchKind, SubrInteractivity};
use std::sync::mpsc;

#[test]
fn subr_storage_layout_matches_the_original_abi() {
    assert_eq!(
        std::mem::size_of::<SubrObj>(),
        std::mem::size_of::<PreviousSubrLayout>()
    );
    assert_eq!(SubrObj::FUNCTION_OFFSET, SubrStorageLayout::ENTRY_OFFSET);
    assert_eq!(SubrObj::METADATA_OFFSET, 0);
}

fn empty_result(_: Vec<TaggedValue>) -> crate::emacs_core::error::EvalResult {
    Ok(TaggedValue::NIL)
}
struct SharedSubr(*mut SubrObj);
// SAFETY: the allocation is a leaked real specialization. Only the existing
// selected subr RwLock permits payload reads/refresh; it outlives both workers.
unsafe impl Send for SharedSubr {}
// SAFETY: same lock/lifetime condition as Send; no unsynchronized payload view.
unsafe impl Sync for SharedSubr {}
impl SharedSubr {
    fn header(&self) -> *const VecLikeHeader {
        self.0.cast()
    }
    unsafe fn read(&self) -> &SubrObj {
        // SAFETY: caller holds the existing selected read lock.
        unsafe { &*self.0 }
    }
}

#[cfg(feature = "jit")]
#[derive(Debug)]
enum ReaderRefusal {
    Busy,
}
#[cfg(feature = "jit")]
fn selected_read<R>(mut body: impl FnMut() -> R) -> Result<R, ReaderRefusal> {
    use crate::tagged::gc::{ExecutionTestDeadline, ExecutionTestRetry, execution_test_retry};
    // Only acquisition is retried: with_static_subr_entry_read does not call
    // body on Busy, and no assertion or completed operation is ever repeated.
    execution_test_retry(
        ExecutionTestDeadline::contention(),
        || crate::tagged::value::with_static_subr_entry_read(&mut body).ok_or(ReaderRefusal::Busy),
        |error| match error {
            ReaderRefusal::Busy => ExecutionTestRetry::Busy,
        },
    )
}
#[cfg(feature = "jit")]
enum WriterCommand {
    Refresh,
    Cancel,
}
#[cfg(feature = "jit")]
struct WriterStart(mpsc::Sender<WriterCommand>);
#[cfg(feature = "jit")]
impl Drop for WriterStart {
    fn drop(&mut self) {
        // Total cleanup even when acquisition or the body fails before Refresh.
        let _ = self.0.send(WriterCommand::Cancel);
    }
}

#[cfg(feature = "jit")]
#[test]
fn canonical_subr_refresh_while_reading_keeps_the_selected_lock() {
    let sym = crate::emacs_core::intern::intern("split-t2-refresh-while-reading");
    let value = <TaggedValue as SubrValueExt>::subr_from_sym_id(sym);
    let shared = SharedSubr(
        value
            .as_veclike_ptr()
            .expect("actual subr")
            .cast_mut()
            .cast(),
    );
    // Before registration this is the exact canonical fallback.
    assert_eq!(value.as_subr_id(), Some(sym));
    let (start_tx, start_rx) = mpsc::channel();
    let (entered_tx, entered_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let shared = &shared;
        let writer = scope.spawn(move || {
            // Install the same permanently live canonical object in the worker's
            // VALUE-only table, as carrier bootstrap does. It owns no code/root.
            let mut values = vec![None; sym.0 as usize + 1];
            // SAFETY: shared names the real initialized/leaked specialization.
            values[sym.0 as usize] =
                Some(unsafe { TaggedValue::from_veclike_ptr(shared.header()) });
            crate::tagged::value::install_static_subr_objects(values);
            match start_rx.recv() {
                Ok(WriterCommand::Refresh) => {}
                Ok(WriterCommand::Cancel) | Err(_) => return,
            }
            entered_tx.send(()).expect("reader remains present");
            super::super::register_global_subr_entry(
                sym,
                super::super::SubrEntry {
                    function: Some(SubrFn::ManyNoContext(empty_result)),
                    min_args: 2,
                    max_args: Some(3),
                    dispatch_kind: SubrDispatchKind::Builtin,
                    interactive_spec: None,
                },
            );
        });
        let start = WriterStart(start_tx);
        let observed = selected_read(|| {
            start.0.send(WriterCommand::Refresh).expect("writer exists");
            entered_rx.recv().expect("writer entered refresh protocol");
            // SAFETY: this closure retains the actual selected reader lock;
            // the writer may now try refresh but cannot overlap these reads.
            let subr = unsafe { shared.read() };
            assert!(subr.function_ref().is_none());
            assert_eq!(subr.metadata().min_args(), 0);
            assert_eq!(subr.metadata().max_args(), None);
            assert_eq!(
                subr.metadata().interactivity(),
                SubrInteractivity::NonInteractive
            );
        });
        if observed.is_err() {
            let _ = start.0.send(WriterCommand::Cancel);
        }
        writer
            .join()
            .expect("refresh completes after reader releases");
        observed.expect("fixture must acquire selected reader before its deadline");
    });
    selected_read(|| {
        // SAFETY: selected lock held, writer joined.
        let subr = unsafe { shared.read() };
        assert!(matches!(
            *subr.function_ref(),
            Some(SubrFn::ManyNoContext(_))
        ));
        assert_eq!(subr.metadata().min_args(), 2);
        assert_eq!(subr.metadata().max_args(), Some(3));
        assert_eq!(subr.metadata().symbol_id(), sym);
    })
    .expect("post-refresh reader");
    assert_eq!(
        <TaggedValue as SubrValueExt>::subr_from_sym_id(sym).bits(),
        value.bits()
    );
}

#[cfg(feature = "jit")]
#[test]
fn subr_refresh_fixture_early_return_cancels_the_waiting_writer() {
    let (sender, receiver) = mpsc::channel();
    std::thread::scope(|scope| {
        let writer = scope.spawn(move || match receiver.recv() {
            Ok(WriterCommand::Cancel) => (),
            Ok(WriterCommand::Refresh) => panic!("control must not request refresh"),
            Err(error) => panic!("control must explicitly cancel: {error}"),
        });
        let start = WriterStart(sender);
        // The exact RAII guard used by the real fixture resolves an early exit.
        drop(start);
        writer.join().expect("cancelled writer joins cleanly");
    });
}
