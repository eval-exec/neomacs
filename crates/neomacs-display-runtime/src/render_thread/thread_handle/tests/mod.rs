use super::RenderThread;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn spawn_with_state_returns_error_when_startup_fails() {
    let err =
        match RenderThread::spawn_with_state::<(), _, _>(|| Err("boom".to_string()), |_| Ok(())) {
            Ok(_) => panic!("startup failure should be surfaced to caller"),
            Err(err) => err,
        };

    assert_eq!(err, "boom");
}

#[test]
fn spawn_with_state_returns_after_successful_startup() {
    let (done_tx, done_rx) = mpsc::sync_channel(0);
    let handle = RenderThread::spawn_with_state(
        || Ok(()),
        move |_| {
            done_rx
                .recv()
                .expect("test should unblock render-thread runner");
            Ok(())
        },
    )
    .expect("startup success should return render handle");

    done_tx
        .send(())
        .expect("spawn should return before runner exits");
    handle.join();
}

#[test]
fn spawn_with_state_returns_timeout_when_startup_never_reports() {
    let err = match RenderThread::spawn_with_state_timeout(
        || {
            std::thread::sleep(Duration::from_millis(50));
            Ok(())
        },
        |_| Ok(()),
        Duration::from_millis(5),
    ) {
        Ok(_) => panic!("startup timeout should be surfaced to caller"),
        Err(err) => err,
    };

    assert!(err.contains("Timed out waiting"));
}
