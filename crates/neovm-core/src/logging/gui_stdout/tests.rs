use super::*;
use std::sync::mpsc;
use std::time::Duration;

struct GatedSink {
    entered: Option<mpsc::Sender<()>>,
    release: mpsc::Receiver<()>,
    flushed: mpsc::Sender<Vec<u8>>,
    bytes: Vec<u8>,
}

impl Write for GatedSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(entered) = self.entered.take() {
            entered.send(()).unwrap();
            self.release.recv().unwrap();
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushed.send(self.bytes.clone()).unwrap();
        Ok(())
    }
}

fn emit(writer: &GuiWriter, bytes: &[u8]) {
    writer.make_writer().write_all(bytes).unwrap();
}

#[test]
fn full_sink_bounds_queue_counts_loss_and_does_not_block_shutdown() {
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let (flushed_tx, flushed) = mpsc::channel();
    let (writer, guard) = start(
        GatedSink {
            entered: Some(entered_tx),
            release: release_rx,
            flushed: flushed_tx,
            bytes: Vec::new(),
        },
        2,
    )
    .unwrap();
    emit(&writer, b"in flight\n");
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    emit(&writer, b"queued one\n");
    emit(&writer, b"queued two\n");
    for _ in 0..5 {
        emit(&writer, b"rejected\n");
    }
    assert_eq!(guard.rejected_records(), 5);
    assert_eq!(writer.sender.len(), 2);
    assert_eq!(guard.write_failures(), 0);

    let counters = guard.counters.clone();
    let (dropped_tx, dropped) = mpsc::channel();
    let owner = std::thread::spawn(move || {
        drop(guard);
        dropped_tx.send(()).unwrap();
    });
    let returned_while_blocked = dropped.recv_timeout(Duration::from_millis(100)).is_ok();
    emit(&writer, b"after shutdown\n");
    release.send(()).unwrap();
    owner.join().unwrap();
    let output = String::from_utf8(flushed.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap();
    assert!(returned_while_blocked, "guard waited on the blocked sink");
    assert_eq!(counters.rejected.load(Ordering::Relaxed), 6);
    assert!(output.starts_with("in flight\n"));
    assert!(output.contains("GUI logging rejected 6 records"));
    assert!(output.ends_with("queued one\nqueued two\n"));
    assert!(!output.contains("after shutdown"));
    assert!(!output.contains("rejected\n"));
}

#[test]
fn size_limit_rejects_whole_records_without_truncation() {
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let (flushed_tx, flushed) = mpsc::channel();
    let (writer, guard) = start(
        GatedSink {
            entered: Some(entered_tx),
            release: release_rx,
            flushed: flushed_tx,
            bytes: Vec::new(),
        },
        2,
    )
    .unwrap();
    emit(&writer, b"first\n");
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    emit(&writer, &vec![b'x'; RECORD_BYTES]);
    let mut oversized = writer.make_writer();
    oversized.write_all(b"must not leak").unwrap();
    oversized.write_all(&vec![b'y'; RECORD_BYTES]).unwrap();
    drop(oversized);
    assert_eq!(guard.rejected_records(), 1);
    assert_eq!(writer.sender.len(), 1);
    drop(guard);
    release.send(()).unwrap();
    let output = flushed.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(!output.windows(13).any(|part| part == b"must not leak"));
    assert_eq!(
        output.iter().filter(|&&byte| byte == b'x').count(),
        RECORD_BYTES
    );
}

#[test]
fn rejection_after_last_record_is_reported_on_shutdown() {
    let (_release, release_rx) = mpsc::channel();
    let (flushed_tx, flushed) = mpsc::channel();
    let (writer, guard) = start(
        GatedSink {
            entered: None,
            release: release_rx,
            flushed: flushed_tx,
            bytes: Vec::new(),
        },
        2,
    )
    .unwrap();
    emit(&writer, &vec![b'x'; RECORD_BYTES + 1]);
    assert_eq!(guard.rejected_records(), 1);
    drop(guard);
    let output = String::from_utf8(flushed.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap();
    assert!(
        output.contains("GUI logging rejected 1 records"),
        "{output:?}"
    );
}

#[test]
fn sink_failure_closes_admission_and_is_counted() {
    struct BrokenSink(mpsc::Sender<()>);
    impl Write for BrokenSink {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.send(()).unwrap();
            Ok(())
        }
    }
    let (tx, done) = mpsc::channel();
    let (writer, guard) = start(BrokenSink(tx), 2).unwrap();
    emit(&writer, b"broken sink\n");
    done.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(guard.write_failures(), 1);
    emit(&writer, b"rejected after failure\n");
    assert_eq!(guard.rejected_records(), 1);
}

#[test]
fn rejection_counter_saturates() {
    let counter = AtomicUsize::new(usize::MAX - 1);
    increment(&counter);
    increment(&counter);
    assert_eq!(counter.load(Ordering::Relaxed), usize::MAX);
}
