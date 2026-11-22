//! Row 1: real native command dispatch and core publication, without a GPU.
//! Ordinary key conversion uses the same public converter as input_bridge.
//! This does not exercise its upper quit/resize/font multiplexing policies.

use super::RenderApp;
use crate::thread_comm::{
    AssetCommand, InputEvent, LifecycleCommand, RenderCommand, ThreadComms, WindowCommand,
};
use neomacs_display_protocol::ImageCacheUsage;
use neovm_core::keyboard::{FrontendKey, InputEvent as CoreInput, Key};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

const FIXTURE: &str = "render_thread::input_backpressure_test::owned_native_progress_fixture";
const DIRECTORY: &str = "NEOMACS_TEST_INPUT_BACKPRESSURE_DIRECTORY";
const DEADLINE: Duration = Duration::from_secs(20);
const DETECTOR_CONTROL: Duration = Duration::from_millis(500);
const EXTRA_KEYS: usize = 17;

#[test]
fn native_cache_commands_progress_with_saturated_input_and_unread_core_events() {
    let mut fixture = OwnedFixture::spawn("progress");
    let armed = fixture.wait_json("armed.json");
    assert_eq!(armed["input_capacity"], 4096);
    assert_eq!(armed["command_capacity"], 64);
    assert_eq!(armed["input_full"], true);
    assert_eq!(armed["commands_full"], true);
    fixture.resume_bridge();
    let result = fixture.wait_json("complete.json");
    assert_complete(&result);
    fixture.finish();
    write_json(
        &fixture.path.join("detector.json"),
        &json!({
            "initial_outcome": "completed", "cleanup_rescue": false,
        }),
    );
}

#[test]
fn blocked_native_sink_is_detected_before_cleanup_resumes_the_bridge() {
    let mut fixture = OwnedFixture::spawn("detector-control");
    fixture.wait_json("armed.json");
    // The parent detector owns no channel endpoint and cannot service commands.
    // A withheld independent bridge must prevent this SAME native owner from
    // returning from durable publication to its production command pump.
    let deadline = Instant::now() + DETECTOR_CONTROL;
    while Instant::now() < deadline {
        fixture.require_running();
        assert!(!fixture.path.join("native-burst-complete.json").exists());
        assert!(!fixture.path.join("complete.json").exists());
        thread::sleep(Duration::from_millis(10));
    }
    // Record the missing-progress result BEFORE any cleanup release. Recovery
    // afterwards is not allowed to turn this observation into product progress.
    write_json(
        &fixture.path.join("detector.json"),
        &json!({
            "initial_outcome": "timeout", "native_dispatch_blocked": true,
            "cleanup_rescue": true,
        }),
    );
    fixture.resume_bridge();
    assert_complete(&fixture.wait_json("complete.json"));
    fixture.finish();
}

fn assert_complete(result: &Value) {
    assert_eq!(result["complete"], true);
    assert_eq!(result["core_reads_before_usage_ack"], 0);
    // No GPU cache exists: this witnesses the real usage handler and ack,
    // not GPU cache eviction or ClearImageCache ordering correctness.
    assert_eq!(result["published_bytes"], 0);
    assert_eq!(result["usage_scope"], "headless-publication-progress");
    assert_eq!(result["input_count"], 4096 + EXTRA_KEYS);
    assert_eq!(result["ordered_exactly_once"], true);
    assert_eq!(
        result["native_publication_owner"],
        result["native_command_owner"]
    );
}

struct OwnedFixture {
    child: Child,
    path: PathBuf,
    reaped: bool,
}

impl OwnedFixture {
    fn spawn(label: &str) -> Self {
        let root = neomacs_infra::workspace_root().join("tmp/input-backpressure");
        fs::create_dir_all(&root).unwrap();
        let path = tempfile::Builder::new()
            .prefix(label)
            .tempdir_in(root)
            .unwrap()
            .keep();
        eprintln!("input backpressure artifacts: {}", path.display());
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                FIXTURE,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(DIRECTORY, &path)
            .stdin(Stdio::null())
            .stdout(fs::File::create(path.join("stdout.log")).unwrap())
            .stderr(fs::File::create(path.join("stderr.log")).unwrap())
            .spawn()
            .unwrap();
        write_json(
            &path.join("owned-child.json"),
            &json!({"pid":child.id(),"fixture":FIXTURE}),
        );
        Self {
            child,
            path,
            reaped: false,
        }
    }

    fn wait_json(&mut self, name: &str) -> Value {
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Ok(data) = fs::read(self.path.join(name))
                && let Ok(value) = serde_json::from_slice(&data)
            {
                return value;
            }
            self.require_running();
            assert!(
                Instant::now() < deadline,
                "detector deadline waiting for {name}: {}",
                self.path.display()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn require_running(&mut self) {
        if let Some(status) = self.child.try_wait().unwrap() {
            self.reaped = true;
            panic!(
                "owned fixture exited unexpectedly: {status}; {}",
                self.path.display()
            );
        }
    }

    fn resume_bridge(&self) {
        fs::write(
            self.path.join("resume-bridge"),
            b"cleanup or progress release",
        )
        .unwrap();
    }

    fn finish(&mut self) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                self.reaped = true;
                assert!(
                    status.success(),
                    "owned fixture failed: {status}; {}",
                    self.path.display()
                );
                write_json(
                    &self.path.join("reaped.json"),
                    &json!({"pid":self.child.id(),"success":true}),
                );
                return;
            }
            assert!(
                Instant::now() < deadline,
                "owned fixture did not exit: {}",
                self.path.display()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for OwnedFixture {
    fn drop(&mut self) {
        if !self.reaped {
            // Only this exact child is owned. Killing it also stops the fixture's
            // Rust threads if a regression makes their cleanup joins impossible.
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn write_json(path: &Path, value: &Value) {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    fs::rename(temporary, path).unwrap();
}

fn key(sequence: usize) -> InputEvent {
    InputEvent::Key {
        key: FrontendKey::Character(char::from_u32(0xe000 + sequence as u32).unwrap()),
        modifiers: 0,
        pressed: true,
        emacs_frame_id: 1,
    }
}

#[test]
#[ignore = "owned subprocess fixture; parent validates execution and reaps it"]
fn owned_native_progress_fixture() {
    let path = PathBuf::from(std::env::var_os(DIRECTORY).expect("parent-owned fixture directory"));
    assert!(path.is_absolute() && path.is_dir());
    let (emacs, render) = ThreadComms::new().split();
    let input_capacity = emacs.input_rx.capacity().unwrap();
    let command_capacity = render.cmd_rx.capacity().unwrap();
    let total = input_capacity + EXTRA_KEYS;
    let input_probe = emacs.input_rx.clone();
    let command_probe = render.cmd_rx.clone();
    let usage = Arc::new(super::ImageRenderState::default());
    usage.publish_cache_usage(ImageCacheUsage::new(4096, 0));

    let (publisher_tx, publisher_rx) = mpsc::sync_channel(1);
    let (commands_full_tx, commands_full_rx) = mpsc::sync_channel(1);
    let (native_full_tx, native_full_rx) = mpsc::sync_channel(1);
    let (start_query_tx, start_query_rx) = mpsc::sync_channel::<thread::Thread>(1);
    let (query_started_tx, query_started_rx) = mpsc::sync_channel(1);

    let evaluator_usage = Arc::clone(&usage);
    let evaluator = thread::Builder::new()
        .name("backpressure-evaluator".into())
        .spawn(move || {
            let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
            let publisher = neovm_core::host_input::HostInputPublisher::install(&mut context);
            publisher_tx.send(publisher).unwrap();
            let commands = emacs.cmd_tx;
            for _ in 0..command_capacity {
                commands
                    .try_send(RenderCommand::Window(WindowCommand::RefreshFrameOpacity))
                    .unwrap();
            }
            commands_full_tx.send(()).unwrap();
            let native = start_query_rx.recv().unwrap();
            query_started_tx.send(()).unwrap();
            commands
                .send(RenderCommand::Asset(AssetCommand::ClearImageCache))
                .unwrap();
            native.unpark();
            let (ack, wait) = mpsc::sync_channel(1);
            commands
                .send(RenderCommand::Asset(AssetCommand::PublishImageCacheUsage {
                    ack,
                }))
                .unwrap();
            native.unpark();
            wait.recv()
                .expect("production usage publication acknowledged");
            let bytes = evaluator_usage.cached_size_bytes();
            // First core receive is strictly AFTER the usage acknowledgement.
            for sequence in 0..total {
                let input = context.receive_host_input_for_test(DEADLINE).unwrap();
                let CoreInput::KeyPress {
                    key,
                    emacs_frame_id,
                } = input
                else {
                    panic!("expected ordinary key");
                };
                assert_eq!(
                    key.key,
                    Key::Char(char::from_u32(0xe000 + sequence as u32).unwrap())
                );
                assert_eq!(emacs_frame_id, 1);
            }
            assert!(context.receive_host_input_for_test(Duration::ZERO).is_err());
            commands
                .send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown))
                .unwrap();
            native.unpark();
            json!({"input_count":total,"published_bytes":bytes,
            "core_reads_before_usage_ack":0,"ordered_exactly_once":true})
        })
        .unwrap();
    commands_full_rx.recv_timeout(DEADLINE).unwrap();

    let bridge_path = path.clone();
    let bridge = thread::Builder::new()
        .name("backpressure-input-bridge".into())
        .spawn(move || {
            let publisher = publisher_rx.recv().unwrap();
            // This held consumer is independent of evaluator readiness/replies.
            // Only the parent detector releases it, after observing full queues.
            while !bridge_path.join("resume-bridge").exists() {
                thread::sleep(Duration::from_millis(10));
            }
            for _ in 0..total {
                let InputEvent::Key {
                    key,
                    modifiers,
                    pressed,
                    emacs_frame_id,
                } = emacs.input_rx.recv().unwrap()
                else {
                    panic!("expected native key");
                };
                let converted = neovm_core::keyboard::render_key_transport_to_input_event(
                    key,
                    modifiers,
                    pressed,
                    emacs_frame_id,
                )
                .unwrap();
                publisher.send(converted).unwrap();
            }
        })
        .unwrap();

    let native_path = path.clone();
    let native = thread::Builder::new()
        .name("backpressure-native-owner".into())
        .spawn(move || {
            let owner = thread::current().id();
            let mut app = RenderApp::new(
                render,
                800,
                600,
                "backpressure fixture".into(),
                usage,
                Arc::new((Mutex::new(Vec::new()), Condvar::new())),
                false,
                #[cfg(feature = "neo-term")]
                crate::terminal::new_shared_terminals(),
            );
            for sequence in 0..input_capacity {
                app.comms.send_input_with_receipt(key(sequence));
            }
            native_full_tx.send(()).unwrap();
            for sequence in input_capacity..total {
                app.comms.send_input_with_receipt(key(sequence));
            }
            write_json(
                &native_path.join("native-burst-complete.json"),
                &json!({"owner":format!("{owner:?}")}),
            );
            let command_owner = thread::current().id();
            assert_eq!(owner, command_owner);
            // This is the production command pump, on the SAME native owner that
            // published the full input burst. No independent command drainer exists.
            while !app.process_commands() {
                thread::park();
            }
            json!({"native_publication_owner":format!("{owner:?}"),
            "native_command_owner":format!("{command_owner:?}")})
        })
        .unwrap();
    native_full_rx.recv_timeout(DEADLINE).unwrap();
    start_query_tx.send(native.thread().clone()).unwrap();
    query_started_rx.recv_timeout(DEADLINE).unwrap();
    assert!(input_probe.is_full());
    assert!(command_probe.is_full());
    write_json(
        &path.join("armed.json"),
        &json!({"pid":std::process::id(),
        "input_capacity":input_capacity,"command_capacity":command_capacity,
        "input_full":true,"commands_full":true}),
    );
    drop(input_probe);
    drop(command_probe);

    let evaluator_result = evaluator.join().unwrap();
    bridge.join().unwrap();
    let native_result = native.join().unwrap();
    write_json(
        &path.join("complete.json"),
        &json!({"complete":true,
        "input_count":evaluator_result["input_count"],
        "published_bytes":evaluator_result["published_bytes"],
        "usage_scope":"headless-publication-progress",
        "core_reads_before_usage_ack":evaluator_result["core_reads_before_usage_ack"],
        "ordered_exactly_once":evaluator_result["ordered_exactly_once"],
        "native_publication_owner":native_result["native_publication_owner"],
        "native_command_owner":native_result["native_command_owner"]}),
    );
}
