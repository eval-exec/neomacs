//! Caller-side controls for the native cache service policy. These use real
//! catalog requests, queues and wait bounds, without a GPU or OS event loop.
//! Lifecycle wiring and the two native parking owners have separate rows.
//! Timeout controls require the declared deadline to elapse and an independent
//! 30-second detector bounds the child. They do not assert an exact wall time.

use super::*;
use neomacs_display_protocol::ImageCacheUsage;
use neomacs_display_runtime::render_thread::{ImageDecoder, ImageRenderState};
use neomacs_display_runtime::thread_comm::{RenderCommandSender, WindowCommand};
use serde_json::json;
use std::{
    fs,
    num::NonZeroUsize,
    process::{Child, Command, Stdio},
    thread,
};

const FIXTURE: &str = "startup::image_catalog::cache_liveness_test::owned_cache_policy_fixture";
const CASE_ENV: &str = "NEOMACS_TEST_CACHE_POLICY_CASE";
const RESULT_ENV: &str = "NEOMACS_TEST_CACHE_POLICY_RESULT";

#[test]
fn pre_gpu_cache_query_does_not_wait_for_later_evaluator_readiness() {
    run_owned("before-readiness");
}

#[test]
fn withheld_native_ack_reaches_declared_timeout_and_cannot_ack_a_later_read() {
    run_owned("withheld-ack");
}

#[test]
fn saturated_publish_admission_reaches_declared_timeout_without_committing() {
    run_owned("saturated-admission");
}

#[test]
fn terminal_phase_retires_an_admitted_wait_with_command_endpoints_and_ack_alive() {
    run_owned("exit-race");
}

#[test]
fn a_cache_reply_completes_during_a_concurrent_terminal_transition() {
    run_owned("concurrent-completion-exit");
}

struct OwnedChild(Child, bool);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.1 {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn run_owned(case: &str) {
    let root = neomacs_infra::workspace_root().join("tmp/cache-policy");
    fs::create_dir_all(&root).unwrap();
    let directory = tempfile::Builder::new()
        .prefix(case)
        .tempdir_in(root)
        .unwrap()
        .keep();
    let result_path = directory.join("result.json");
    let output = fs::File::create(directory.join("child.log")).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            FIXTURE,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CASE_ENV, case)
        .env(RESULT_ENV, &result_path)
        .stdout(Stdio::from(output.try_clone().unwrap()))
        .stderr(Stdio::from(output))
        .spawn()
        .unwrap();
    let mut child = OwnedChild(child, false);
    eprintln!("cache policy artifacts: {}", directory.display());
    // Independent of the caller and native actor; a regressed unbounded wait
    // fails without leaving a Rust thread (or an evaluator-held sender) alive.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            child.1 = true;
            assert!(
                status.success(),
                "{case}: {status}; {}",
                directory.display()
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "owned {case} exceeded detector deadline: {}",
            directory.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
    let result: serde_json::Value = serde_json::from_slice(
        &fs::read(&result_path).expect("fixture body ran and published its result"),
    )
    .unwrap();
    assert_eq!(result["case"], case);
    assert_eq!(result["complete"], true);
    fs::write(
        directory.join("reaped.json"),
        serde_json::to_vec_pretty(&json!({"pid":child.0.id(),"success":true})).unwrap(),
    )
    .unwrap();
}

fn catalog(sender: RenderCommandSender, metadata: SharedImageRenderState) -> AsyncImageCatalog {
    AsyncImageCatalog::new(sender, None, metadata, None, ImageDecoder::new().client())
}

fn take_ack(command: RenderCommand) -> std::sync::mpsc::SyncSender<()> {
    match command {
        RenderCommand::Asset(AssetCommand::PublishImageCacheUsage { ack }) => ack,
        command => panic!("expected a usage query, got {command:?}"),
    }
}

#[test]
#[ignore = "owned subprocess fixture; ordinary parent rows validate execution and reap it"]
fn owned_cache_policy_fixture() {
    let case = std::env::var(CASE_ENV).expect("parent-selected fixture case");
    let result =
        std::path::PathBuf::from(std::env::var_os(RESULT_ENV).expect("parent-owned result path"));
    assert!(result.is_absolute());
    let (sender, receiver) = RenderCommandSender::bounded(NonZeroUsize::new(64).unwrap());
    let retained_sender = sender.clone();
    let state = Arc::new(ImageRenderState::default());
    let catalog = catalog(sender, Arc::clone(&state));
    let started = Instant::now();
    match case.as_str() {
        "before-readiness" => {
            // This is the evaluator's real clear/read pair before it could
            // publish later first-frame readiness. Nobody services commands.
            assert!(!state.renderer_can_retain_images());
            let _ = catalog.invalidate(ImageInvalidation::All);
            assert_eq!(catalog.cached_size_bytes(), 0);
            assert!(started.elapsed() < Duration::from_secs(2));
            assert!(matches!(
                receiver.try_recv().unwrap(),
                RenderCommand::Asset(AssetCommand::ClearImageCache)
            ));
            assert!(
                receiver.is_empty(),
                "pre-GPU read must not queue an impossible native reply"
            );
        }
        "withheld-ack" => {
            state.mark_renderer_ready();
            // Model a native owner held in foreign work. No timeout callback
            // or cleanup action can release the request before this returns.
            assert!(matches!(
                catalog.synchronize_cache_usage(),
                CacheUsageSync::TimedOut
            ));
            assert!(started.elapsed() >= CACHE_USAGE_SYNC_DEADLINE);
            let old_ack = take_ack(receiver.try_recv().unwrap());
            let next_state = Arc::clone(&state);
            let (completed, completion) = std::sync::mpsc::sync_channel::<()>(1);
            let native = thread::spawn(move || {
                let ack = take_ack(receiver.recv_timeout(Duration::from_secs(2)).unwrap());
                // The second request is admitted and its own ack is withheld.
                // Deliver the old reply during this interval, not before the
                // second request starts. Its receiver must already be gone.
                assert!(old_ack.send(()).is_err());
                assert_eq!(
                    completion.try_recv(),
                    Err(std::sync::mpsc::TryRecvError::Empty)
                );
                next_state.publish_cache_usage(ImageCacheUsage::new(1234, 56));
                ack.send(()).unwrap();
                completion.recv_timeout(Duration::from_secs(2)).unwrap();
            });
            assert!(matches!(
                catalog.synchronize_cache_usage(),
                CacheUsageSync::Synchronized
            ));
            completed.send(()).unwrap();
            assert_eq!(state.cached_size_bytes(), 1290);
            native.join().unwrap();
        }
        "saturated-admission" => {
            state.mark_renderer_ready();
            for _ in 0..64 {
                retained_sender
                    .try_send(RenderCommand::Window(WindowCommand::RefreshFrameOpacity))
                    .unwrap();
            }
            assert!(receiver.is_full());
            assert!(matches!(
                catalog.synchronize_cache_usage(),
                CacheUsageSync::TimedOut
            ));
            assert!(started.elapsed() >= CACHE_USAGE_SYNC_DEADLINE);
            assert_eq!(
                receiver.len(),
                64,
                "a timed-out query never acquired a queue slot"
            );
            // This tests Publish admission. The public clear's invalidation
            // admission has its own product-owner regression row.
        }
        "exit-race" => {
            state.mark_renderer_ready();
            let exit_state = Arc::clone(&state);
            let (retained_ack, ack_receiver) = std::sync::mpsc::sync_channel(1);
            let native = thread::spawn(move || {
                let ack = take_ack(receiver.recv_timeout(Duration::from_secs(2)).unwrap());
                // A receiver clone survives termination, just as setup can
                // retain one. This is a phase-policy control, not proof that
                // native handle_exiting was called (the exit row covers it).
                exit_state.mark_renderer_exited();
                retained_ack.send((ack, receiver)).unwrap();
            });
            assert!(matches!(
                catalog.synchronize_cache_usage(),
                CacheUsageSync::RenderThreadGone
            ));
            assert!(started.elapsed() < Duration::from_secs(2));
            let (ack, retained_receiver) = ack_receiver.recv().unwrap();
            assert!(ack.send(()).is_err());
            state.mark_renderer_exited();
            state.mark_renderer_ready();
            assert!(
                state.renderer_exited(),
                "a late GPU-ready publication cannot reopen terminal admission"
            );
            assert!(matches!(
                catalog.synchronize_cache_usage(),
                CacheUsageSync::RenderThreadGone
            ));
            assert!(retained_receiver.is_empty());
            native.join().unwrap();
        }
        "concurrent-completion-exit" => {
            state.mark_renderer_ready();
            let completion_state = Arc::clone(&state);
            let native = thread::spawn(move || {
                let ack = take_ack(receiver.recv_timeout(Duration::from_secs(2)).unwrap());
                completion_state.publish_cache_usage(ImageCacheUsage::new(1234, 56));
                ack.send(()).unwrap();
                completion_state.mark_renderer_exited();
            });
            // A concurrent control: publication precedes exit on the native
            // owner, but scheduling may let the evaluator observe the reply
            // before exit. This does not force terminal state to be visible
            // at reply observation and is not a deterministic precedence row.
            assert!(matches!(
                catalog.synchronize_cache_usage(),
                CacheUsageSync::Synchronized
            ));
            native.join().unwrap();
            assert!(state.renderer_exited());
            assert_eq!(state.cached_size_bytes(), 1290);
        }
        _ => panic!("unknown fixture case {case}"),
    }
    drop(retained_sender);
    fs::write(
        result,
        serde_json::to_vec_pretty(&json!({
            "case":case,"complete":true,"elapsed_ms":started.elapsed().as_millis(),
            "scope":"caller-policy-no-native-event-loop-or-gpu",
            "production_deadline_ms":CACHE_USAGE_SYNC_DEADLINE.as_millis(),
        }))
        .unwrap(),
    )
    .unwrap();
}
