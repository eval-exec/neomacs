//! Caller-side controls for the native cache service policy. These use real
//! catalog requests, queues and wait bounds, without a GPU or OS event loop.
//! Lifecycle wiring and the two native parking owners have separate rows.
//! Timeout controls require the declared deadline to elapse and an independent
//! 30-second detector bounds the child. They do not assert an exact wall time.
//! Every clear is issued before this fixture submits any CPU image job: seeded
//! byte counters are synthetic, so the exact logical admission cutoff is zero.
//! A later eager decode must not move that cutoff while native service waits.

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
fn timed_out_clear_retries_before_the_next_publish() {
    run_owned("retry-clear");
}

#[test]
fn lookup_after_a_queued_clear_does_not_wait_for_native_queue_space() {
    run_owned("lookup-after-pending-clear");
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
            fs::write(
                directory.join("reaped.json"),
                serde_json::to_vec_pretty(&json!({"pid":child.0.id(),"success":status.success()}))
                    .unwrap(),
            )
            .unwrap();
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
}

fn catalog(sender: RenderCommandSender, metadata: SharedImageRenderState) -> AsyncImageCatalog {
    AsyncImageCatalog::new(sender, None, metadata, None, ImageDecoder::new().client())
}

#[test]
fn retained_clear_precedes_a_later_public_lookup_and_usage_publish() {
    run_owned("retry-clear-then-load");
}

fn retry_load_file_request(
    _context: &neovm_core::emacs_core::Context,
    path: &str,
) -> ImageResolveRequest {
    use neovm_core::emacs_core::Value;
    use neovm_core::emacs_core::image::image_load_identity;
    use neovm_core::emacs_core::image_catalog::{AxisSize, ImageSizeSpec, ImageSpecIdentity};
    use neovm_core::emacs_core::value::list_to_vec;

    // The caller retains the Context until after the catalog and request die.
    // Derive the owned request exactly as existing file_request tests do.
    let spec = Value::list(vec![
        Value::symbol("image"),
        Value::keyword(":type"),
        Value::symbol("png"),
        Value::keyword(":file"),
        Value::string(path),
    ]);
    let items = list_to_vec(&spec).expect("owned PNG image specification");
    ImageResolveRequest {
        spec: ImageSpecIdentity::from_lisp_spec(&spec).expect("PNG image spec identity"),
        identity: image_load_identity(&spec, &items),
        source: ImageResolveSource::File(ImageFileName::from_utf8(path)),
        size: ImageSizeSpec::new(AxisSize::Native, AxisSize::Native),
        rotation: Default::default(),
        colors: Default::default(),
        mask: Default::default(),
        animation: neomacs_display_protocol::ImageAnimationPolicy::disabled(),
        frame: Default::default(),
        realization: Default::default(),
    }
}

#[test]
fn retained_clear_preserves_relative_lookup_resolution_before_usage_publish() {
    run_owned("retry-clear-relative-load");
}

#[test]
fn usage_read_waits_for_a_delayed_ordered_submission_within_its_deadline() {
    run_owned("retry-clear-relative-load-delayed");
}

#[test]
fn ordered_submission_wait_cancels_on_exit_with_completion_sender_alive() {
    run_owned("ordered-submission-exit");
}

#[test]
fn existing_pending_lookup_retires_completed_submission_and_flushes_queue() {
    run_owned("existing-lookup-completed-submission");
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
        "existing-lookup-completed-submission" => {
            let context = neovm_core::emacs_core::eval_hook_installers::new_context();
            let owned_decoder = ImageDecoder::new();
            let catalog = AsyncImageCatalog::new(
                retained_sender.clone(),
                None,
                Arc::clone(&state),
                None,
                owned_decoder.client(),
            );
            let path = fs::canonicalize(
                neomacs_infra::workspace_root().join("test/data/image/blank-100x200.png"),
            )
            .unwrap();
            let request = retry_load_file_request(&context, path.to_str().unwrap());
            state.mark_renderer_ready();
            assert!(matches!(
                catalog.lookup(request.clone(), ImageSizeLimit::UNLIMITED),
                ImageLookup::Pending(_)
            ));
            assert!(matches!(
                receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
                RenderCommand::Asset(AssetCommand::ImageLoadFile { .. })
            ));
            assert!(receiver.is_empty());

            // Control only the submission completion. The entry came from
            // public lookup and stays Pending because no native owner has
            // published its terminal metadata. No new entry or cache-size
            // read may rescue work stranded behind this completed gate.
            let (completion, wait) = std::sync::mpsc::sync_channel(1);
            *catalog.ordered_in_flight.borrow_mut() = Some(wait);
            catalog
                .pending_invalidations
                .borrow_mut()
                .push_back(QueuedRenderWork {
                    command: RenderCommand::Window(WindowCommand::SetWindowTitle {
                        title: "existing-lookup-drain-witness".to_owned(),
                    }),
                    resolution: None,
                });
            completion.send(()).unwrap();
            let lookup = catalog.lookup(request, ImageSizeLimit::UNLIMITED);
            let gate_retired = catalog.ordered_in_flight.borrow().is_none();
            let queue_empty = catalog.pending_invalidations.borrow().is_empty();
            let witness = matches!(
                receiver.try_recv(),
                Ok(RenderCommand::Window(WindowCommand::SetWindowTitle { title }))
                    if title == "existing-lookup-drain-witness"
            );
            fs::write(
                result.parent().unwrap().join("lookup-progress.json"),
                serde_json::to_vec_pretty(&json!({
                    "scope":"public-existing-pending-entry-controlled-completion-gate",
                    "gate_retired":gate_retired,"queue_empty":queue_empty,
                    "native_witness_received":witness,
                    "cache_size_or_new_lookup_rescue":false,
                }))
                .unwrap(),
            )
            .unwrap();
            assert!(matches!(lookup, ImageLookup::Pending(_)));
            assert!(
                gate_retired,
                "existing lookup must retire a completed submission gate"
            );
            assert!(
                queue_empty,
                "existing lookup must flush queued ordered work"
            );
            assert!(witness, "native queue must receive the ordered witness");
            assert!(receiver.is_empty());
            drop(completion);
            drop(catalog);
            drop(owned_decoder);
            drop(context);
        }
        "before-readiness" => {
            // This is the evaluator's real clear/read pair before it could
            // publish later first-frame readiness. Nobody services commands.
            assert!(!state.renderer_can_retain_images());
            let _ = catalog.invalidate(ImageInvalidation::All);
            assert_eq!(catalog.cached_size_bytes(), 0);
            assert!(started.elapsed() < Duration::from_secs(2));
            assert!(matches!(
                receiver.try_recv().unwrap(),
                RenderCommand::Asset(AssetCommand::ClearImageCache { through: 0 })
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
        "retry-clear" => {
            state.mark_renderer_ready();
            state.publish_cache_usage(ImageCacheUsage::new(4096, 0));
            for _ in 0..64 {
                retained_sender
                    .try_send(RenderCommand::Window(WindowCommand::RefreshFrameOpacity))
                    .unwrap();
            }
            assert!(receiver.is_full());
            let _ = catalog.invalidate(ImageInvalidation::All);
            assert!(started.elapsed() >= CACHE_USAGE_SYNC_DEADLINE);
            assert_eq!(
                receiver.len(),
                64,
                "the clear did not acquire a native slot"
            );
            for command in receiver.try_iter() {
                assert!(matches!(
                    command,
                    RenderCommand::Window(WindowCommand::RefreshFrameOpacity)
                ));
            }
            let next_state = Arc::clone(&state);
            let native = thread::spawn(move || {
                // The original command must survive the admission timeout.
                // Dropping it and publishing the old total would violate the
                // public clear/read contract even after native service resumes.
                let first = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
                assert!(
                    matches!(
                        first,
                        RenderCommand::Asset(AssetCommand::ClearImageCache { through: 0 })
                    ),
                    "a pending clear must precede the usage publish; got {first:?}"
                );
                let ack = take_ack(receiver.recv_timeout(Duration::from_secs(2)).unwrap());
                next_state.publish_cache_usage(ImageCacheUsage::new(0, 0));
                ack.send(()).unwrap();
            });
            assert_eq!(
                catalog.cached_size_bytes(),
                0,
                "the first read after service resumes must observe the retried clear"
            );
            native.join().unwrap();
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
        "retry-clear-then-load" => {
            let context = neovm_core::emacs_core::eval_hook_installers::new_context();
            let owned_decoder = ImageDecoder::new();
            // Shadow the outer fixture catalog: this case retains its decoder owner,
            // rather than relying on a client from an immediately dropped decoder.
            let catalog = AsyncImageCatalog::new(
                retained_sender.clone(),
                None,
                Arc::clone(&state),
                None,
                owned_decoder.client(),
            );
            let path = neomacs_infra::workspace_root().join("test/data/image/blank-100x200.png");
            let path = fs::canonicalize(path).expect("absolute repository PNG fixture");
            assert!(path.is_file());
            let request =
                retry_load_file_request(&context, path.to_str().expect("UTF-8 fixture path"));

            state.mark_renderer_ready();
            state.publish_cache_usage(ImageCacheUsage::new(4096, 0));
            let capacity = receiver.capacity().expect("bounded native command channel");
            assert_eq!(capacity, 64);
            for _ in 0..capacity {
                retained_sender
                    .try_send(RenderCommand::Window(WindowCommand::RefreshFrameOpacity))
                    .unwrap();
            }
            assert!(receiver.is_full());
            let invalidation_started = Instant::now();
            let _ = catalog.invalidate(ImageInvalidation::All);
            assert!(invalidation_started.elapsed() >= CACHE_USAGE_SYNC_DEADLINE);
            assert_eq!(
                receiver.len(),
                capacity,
                "clear never acquired a native slot"
            );
            // Drain exactly the old commands. No synthetic actor is allowed to service
            // the retained clear before the public lookup attempts to schedule its load.
            for _ in 0..capacity {
                assert!(matches!(
                    receiver.try_recv().unwrap(),
                    RenderCommand::Window(WindowCommand::RefreshFrameOpacity)
                ));
            }
            assert!(receiver.is_empty());

            let lookup = catalog.lookup(request, ImageSizeLimit::UNLIMITED);
            let lookup_scheduled =
                matches!(lookup, ImageLookup::Pending(_) | ImageLookup::Ready(_));
            let next_state = Arc::clone(&state);
            let receipt_path = result.parent().unwrap().join("native-order.json");
            let native = thread::spawn(move || {
                let mut order = Vec::<String>::new();
                let mut bytes = 4096_u64;
                let mut publish_seen = false;
                let mut ack_delivered = false;
                let mut receive_error = None;
                let deadline = Instant::now() + Duration::from_secs(15);
                loop {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    let command = match receiver.recv_timeout(remaining) {
                        Ok(command) => command,
                        Err(error) => {
                            receive_error = Some(error.to_string());
                            break;
                        }
                    };
                    match command {
                        RenderCommand::Asset(AssetCommand::ClearImageCache { through: 0 }) => {
                            order.push("clear".to_owned());
                            bytes = 0;
                        }
                        RenderCommand::Asset(AssetCommand::ImageLoadFile { .. }) => {
                            order.push("load".to_owned());
                            // Deliberately synthetic: a distinguishable successful
                            // later load must survive its earlier logical clear.
                            bytes += 1234;
                        }
                        RenderCommand::Asset(AssetCommand::PublishImageCacheUsage { ack }) => {
                            order.push("publish".to_owned());
                            next_state.publish_cache_usage(ImageCacheUsage::new(bytes, 0));
                            publish_seen = true;
                            ack_delivered = ack.send(()).is_ok();
                            break;
                        }
                        other => order.push(format!("unexpected:{other:?}")),
                    }
                }
                let receipt = json!({
                    "case":"retry-clear-then-load",
                    "scope":"synthetic-backend-charge-and-public-catalog-command-order",
                    "order":order,"backend_bytes":bytes,
                    "publish_seen":publish_seen,"ack_delivered":ack_delivered,
                    "receive_error":receive_error,
                });
                // Write the entire observed order even on the expected RED. No order
                // assertion may abort this actor before the Publish witness is saved.
                fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
                receipt
            });

            let observed_bytes = catalog.cached_size_bytes();
            let receipt = native
                .join()
                .expect("synthetic actor completed and wrote its receipt");
            // All semantic assertions follow the native-order.json write, so a RED
            // preserves Load/Clear/Publish instead of only the first mismatched command.
            assert!(lookup_scheduled, "public lookup must schedule the new PNG");
            assert_eq!(receipt["publish_seen"], true);
            assert_eq!(receipt["ack_delivered"], true);
            assert_eq!(receipt["receive_error"], serde_json::Value::Null);
            assert_eq!(receipt["order"], json!(["clear", "load", "publish"]));
            assert_eq!(receipt["backend_bytes"], 1234);
            assert_eq!(
                observed_bytes, 1234,
                "earlier clear must not erase the later load"
            );
            drop(catalog);
            drop(owned_decoder);
            drop(context);
        }
        "lookup-after-pending-clear" => {
            let context = neovm_core::emacs_core::eval_hook_installers::new_context();
            let owned_decoder = ImageDecoder::new();
            let catalog = AsyncImageCatalog::new(
                retained_sender.clone(),
                None,
                Arc::clone(&state),
                None,
                owned_decoder.client(),
            );
            let path = fs::canonicalize(
                neomacs_infra::workspace_root().join("test/data/image/blank-100x200.png"),
            )
            .unwrap();
            let request = retry_load_file_request(&context, path.to_str().unwrap());
            state.mark_renderer_ready();
            for _ in 0..64 {
                retained_sender
                    .try_send(RenderCommand::Window(WindowCommand::RefreshFrameOpacity))
                    .unwrap();
            }
            assert!(receiver.is_full());
            let clear_started = Instant::now();
            let _ = catalog.invalidate(ImageInvalidation::All);
            assert!(clear_started.elapsed() >= CACHE_USAGE_SYNC_DEADLINE);

            // Native service stays withheld throughout this public lookup.
            // Redisplay must enqueue its load without waiting for queue space.
            let lookup_started = Instant::now();
            let lookup = catalog.lookup(request, ImageSizeLimit::UNLIMITED);
            let lookup_elapsed = lookup_started.elapsed();
            let accepted = matches!(lookup, ImageLookup::Pending(_) | ImageLookup::Ready(_));
            let native_still_full = receiver.is_full();
            fs::write(
                result.parent().unwrap().join("lookup-progress.json"),
                serde_json::to_vec_pretty(&json!({
                    "lookup_elapsed_ms":lookup_elapsed.as_millis(),
                    "lookup_accepted":accepted,"native_still_full":native_still_full,
                    "cleanup_release_before_observation":false,
                }))
                .unwrap(),
            )
            .unwrap();
            // Release a possible submission worker only for cleanup, after
            // saving the lookup's original progress verdict. A rescue cannot
            // turn a slow/blocked lookup into the required initial progress.
            drop(receiver);
            drop(catalog);
            drop(owned_decoder);
            drop(context);
            assert!(native_still_full);
            assert!(accepted, "the valid PNG lookup must be admitted");
            assert!(
                lookup_elapsed < Duration::from_secs(2),
                "ordinary lookup waited for native queue space: {lookup_elapsed:?}"
            );
        }
        "retry-clear-relative-load" | "retry-clear-relative-load-delayed" => {
            let delayed = case == "retry-clear-relative-load-delayed";
            let context = neovm_core::emacs_core::eval_hook_installers::new_context();
            let owned_decoder = ImageDecoder::new();
            let mut catalog = AsyncImageCatalog::new(
                retained_sender.clone(),
                None,
                Arc::clone(&state),
                None,
                owned_decoder.client(),
            );
            let directory = result.parent().expect("owned result directory");
            let search_directory = directory.join("private-relative-search");
            fs::create_dir(&search_directory).expect("fresh private image search directory");
            let search_directory = fs::canonicalize(search_directory).unwrap();
            let unique_leaf = directory.file_name().unwrap().to_str().unwrap();
            let basename = format!(
                "neomacs-retry-relative-{}-{unique_leaf}.png",
                std::process::id()
            );
            assert!(
                !std::env::current_dir().unwrap().join(&basename).exists(),
                "relative fixture must not accidentally resolve from the working directory"
            );
            let source = neomacs_infra::workspace_root().join("test/data/image/blank-100x200.png");
            let copied_fixture = search_directory.join(&basename);
            fs::copy(source, &copied_fixture)
                .expect("copy repository PNG into private search path");
            let expected_path = fs::canonicalize(copied_fixture).unwrap();
            let expected_path = expected_path
                .to_str()
                .expect("UTF-8 private PNG path")
                .to_owned();
            catalog.search_path = vec![search_directory.to_str().unwrap().to_owned()];
            let request = retry_load_file_request(&context, &basename);
            // Pure classification only: do not resolve the path in the test on behalf
            // of production, or patch the native command after its public lookup.
            let (_, resolution) = catalog.classify_request(request.clone());
            let resolution = resolution.expect("relative :file has a resolution recipe");
            assert!(matches!(resolution, ImageFileRequest::Search { .. }));
            assert!(resolution.needs_off_thread());

            state.mark_renderer_ready();
            state.publish_cache_usage(ImageCacheUsage::new(4096, 0));
            let capacity = receiver.capacity().expect("bounded native command channel");
            assert_eq!(capacity, 64);
            for _ in 0..capacity {
                retained_sender
                    .try_send(RenderCommand::Window(WindowCommand::RefreshFrameOpacity))
                    .unwrap();
            }
            assert!(receiver.is_full());
            let invalidation_started = Instant::now();
            let _ = catalog.invalidate(ImageInvalidation::All);
            assert!(invalidation_started.elapsed() >= CACHE_USAGE_SYNC_DEADLINE);
            assert_eq!(
                receiver.len(),
                capacity,
                "clear did not acquire a native slot"
            );
            for _ in 0..capacity {
                assert!(matches!(
                    receiver.try_recv().unwrap(),
                    RenderCommand::Window(WindowCommand::RefreshFrameOpacity)
                ));
            }
            assert!(receiver.is_empty());

            // Hold the real, process-wide submission worker behind another
            // full target, then release it after a finite 200 ms. The catalog's
            // actual relative-file job must wait there; do not inject its gate.
            let worker_released = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let mut worker_release = None;
            let mut worker_controller = None;
            if delayed {
                let (blocked_target, blocked_receiver) =
                    RenderCommandSender::bounded(NonZeroUsize::new(1).unwrap());
                blocked_target
                    .send(RenderCommand::Window(WindowCommand::RefreshFrameOpacity))
                    .unwrap();
                let (sent, committed) = std::sync::mpsc::sync_channel(1);
                defer_render_command_with_completion(
                    &blocked_target,
                    None,
                    RenderCommand::Window(WindowCommand::RefreshFrameOpacity),
                    None,
                    &owned_decoder.client(),
                    Some(sent),
                )
                .unwrap();
                let (release, released) = std::sync::mpsc::sync_channel(1);
                let marker = Arc::clone(&worker_released);
                worker_controller = Some(thread::spawn(move || {
                    released.recv_timeout(Duration::from_secs(2)).unwrap();
                    thread::sleep(Duration::from_millis(200));
                    marker.store(true, Ordering::Release);
                    blocked_receiver
                        .recv_timeout(Duration::from_secs(2))
                        .unwrap();
                    committed.recv_timeout(Duration::from_secs(2)).unwrap();
                }));
                worker_release = Some(release);
            }

            // Deliberately pass only the basename: production must preserve and apply
            // its captured Search recipe while ordering the load after retained clear.
            let lookup = catalog.lookup(request, ImageSizeLimit::UNLIMITED);
            let lookup_scheduled =
                matches!(lookup, ImageLookup::Pending(_) | ImageLookup::Ready(_));
            let next_state = Arc::clone(&state);
            let receipt_path = directory.join("native-order.json");
            let actor_expected_path = expected_path.clone();
            let actor_basename = basename.clone();
            let actor_case = case.clone();
            let (clear_seen, cleared) = std::sync::mpsc::sync_channel(1);
            let native = thread::spawn(move || {
                let mut order = Vec::<String>::new();
                let mut load_paths = Vec::<String>::new();
                let mut bytes = 4096_u64;
                let mut publish_seen = false;
                let mut ack_delivered = false;
                let mut receive_error = None;
                let deadline = Instant::now() + Duration::from_secs(15);
                loop {
                    let command = match receiver
                        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    {
                        Ok(command) => command,
                        Err(error) => {
                            receive_error = Some(error.to_string());
                            break;
                        }
                    };
                    match command {
                        RenderCommand::Asset(AssetCommand::ClearImageCache { through: 0 }) => {
                            order.push("clear".to_owned());
                            bytes = 0;
                            next_state.publish_cache_usage(ImageCacheUsage::new(0, 0));
                            let _ = clear_seen.try_send(());
                        }
                        RenderCommand::Asset(AssetCommand::ImageLoadFile { path, .. }) => {
                            order.push("load".to_owned());
                            load_paths.push(path);
                            // Synthetic charge is an ordering witness only. Path
                            // resolution is checked independently after saving receipt.
                            bytes += 1234;
                        }
                        RenderCommand::Asset(AssetCommand::PublishImageCacheUsage { ack }) => {
                            order.push("publish".to_owned());
                            next_state.publish_cache_usage(ImageCacheUsage::new(bytes, 0));
                            publish_seen = true;
                            ack_delivered = ack.send(()).is_ok();
                            break;
                        }
                        other => order.push(format!("unexpected:{other:?}")),
                    }
                }
                let receipt = json!({
                    "case":actor_case,
                    "scope":"synthetic-backend-order-path-and-charge-no-gpu-or-gnu-run",
                    "requested_relative_name":actor_basename,
                    "expected_canonical_file":actor_expected_path,
                    "order":order,"load_paths":load_paths,"backend_bytes":bytes,
                    "publish_seen":publish_seen,"ack_delivered":ack_delivered,
                    "receive_error":receive_error,
                });
                // Always preserve all command kinds THROUGH Publish and the raw load
                // paths before any semantic assertion, including on the expected RED.
                fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
                receipt
            });

            if let Some(release) = worker_release {
                cleared.recv_timeout(Duration::from_secs(2)).unwrap();
                release.send(()).unwrap();
            }
            let read_started = Instant::now();
            let observed_bytes = catalog.cached_size_bytes();
            if delayed {
                fs::write(directory.join("query-progress.json"), serde_json::to_vec_pretty(&json!({
                    "read_elapsed_ms":read_started.elapsed().as_millis(),
                    "observed_bytes":observed_bytes,
                    "worker_release_started_before_read_return":worker_released.load(Ordering::Acquire),
                    "production_deadline_ms":CACHE_USAGE_SYNC_DEADLINE.as_millis(),
                })).unwrap()).unwrap();
            }
            let receipt = native
                .join()
                .expect("synthetic actor saved native-order.json");
            if let Some(controller) = worker_controller {
                controller.join().unwrap();
            }
            assert_eq!(receipt["publish_seen"], true);
            assert_eq!(receipt["ack_delivered"], true);
            assert_eq!(receipt["receive_error"], serde_json::Value::Null);
            assert_eq!(receipt["order"], json!(["clear", "load", "publish"]));
            assert_eq!(receipt["load_paths"], json!([expected_path]));
            assert!(
                lookup_scheduled,
                "public relative lookup must schedule the PNG"
            );
            assert_eq!(receipt["backend_bytes"], 1234);
            assert_eq!(observed_bytes, 1234);
            drop(catalog);
            drop(owned_decoder);
            drop(context);
        }
        "ordered-submission-exit" => {
            state.mark_renderer_ready();
            let (completion, wait) = std::sync::mpsc::sync_channel(1);
            *catalog.ordered_in_flight.borrow_mut() = Some(wait);
            let exit_state = Arc::clone(&state);
            let exit = thread::spawn(move || {
                thread::sleep(Duration::from_millis(50));
                exit_state.mark_renderer_exited();
                completion
            });
            let began = Instant::now();
            let outcome = catalog.wait_for_ordered_in_flight(began + Duration::from_secs(3));
            let held_completion = exit.join().unwrap();
            assert!(
                matches!(outcome, Err(InvalidationDelivery::Unavailable)),
                "exit must cancel the in-flight wait, not mistake one poll for its deadline"
            );
            assert!(began.elapsed() < Duration::from_secs(2));
            assert!(
                held_completion.send(()).is_err(),
                "cancellation must retire the gate while its completion sender remains alive"
            );
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
