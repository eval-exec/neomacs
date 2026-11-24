use crate::*;

#[test]
fn queue_state_prioritizes_interactive() {
    let mut queue = QueueState::default();
    queue.push(TaskHandle(1), TaskPriority::Background);
    queue.push(TaskHandle(2), TaskPriority::Default);
    queue.push(TaskHandle(3), TaskPriority::Interactive);

    assert_eq!(queue.pop(), Some(TaskHandle(3)));
    assert_eq!(queue.pop(), Some(TaskHandle(2)));
    assert_eq!(queue.pop(), Some(TaskHandle(1)));
    assert_eq!(queue.pop(), None);
}

#[test]
fn spawn_and_cancel_task() {
    let rt = WorkerRuntime::new(WorkerConfig::default());
    let task = rt
        .spawn(LispValue::default(), TaskOptions::default())
        .expect("task should enqueue");
    assert_eq!(rt.task_status(task), Some(TaskStatus::Queued));
    assert!(rt.cancel(task));
    assert_eq!(rt.task_status(task), Some(TaskStatus::Cancelled));
}

#[test]
fn reject_main_only_task_on_worker_runtime() {
    let rt = WorkerRuntime::new(WorkerConfig::default());
    let opts = TaskOptions {
        affinity: Affinity::MainOnly,
        ..TaskOptions::default()
    };
    let err = rt
        .spawn(LispValue::default(), opts)
        .expect_err("must reject");
    assert!(matches!(err, EnqueueError::MainAffinityUnsupported));
}

#[test]
fn scheduler_trait_maps_queue_full_to_signal() {
    let rt = WorkerRuntime::new(WorkerConfig {
        threads: 1,
        queue_capacity: 0,
    });
    let err = TaskScheduler::spawn_task(&rt, LispValue::default(), TaskOptions::default())
        .expect_err("must map queue pressure to signal");
    assert_eq!(err.symbol, "task-queue-full");
}

#[test]
fn dummy_worker_marks_task_completed() {
    let rt = WorkerRuntime::new(WorkerConfig {
        threads: 1,
        queue_capacity: 16,
    });
    let workers = rt.start_dummy_workers();
    let task = rt
        .spawn(LispValue::default(), TaskOptions::default())
        .expect("task should enqueue");

    let mut completed = false;
    for _ in 0..100 {
        if rt.task_status(task) == Some(TaskStatus::Completed) {
            completed = true;
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }

    rt.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }

    assert!(completed, "task should complete on dummy worker");
    let stats = rt.stats();
    assert_eq!(stats.enqueued, 1);
    assert_eq!(stats.dequeued, 1);
    assert_eq!(stats.completed, 1);
}

#[test]
fn scheduler_await_reports_cancelled_task() {
    let rt = WorkerRuntime::new(WorkerConfig::default());
    let task = rt
        .spawn(LispValue::default(), TaskOptions::default())
        .expect("task should enqueue");
    assert!(rt.cancel(task));

    let err = TaskScheduler::task_await(&rt, task, None).expect_err("task should cancel");
    assert!(matches!(err, TaskError::Cancelled));
}

#[test]
fn scheduler_await_wakes_on_completion() {
    let rt = WorkerRuntime::new(WorkerConfig {
        threads: 1,
        queue_capacity: 16,
    });
    let workers = rt.start_dummy_workers();
    let expected = LispValue {
        bytes: vec![10, 20, 30],
    };
    let task = rt
        .spawn(expected.clone(), TaskOptions::default())
        .expect("task should enqueue");

    let result = TaskScheduler::task_await(&rt, task, Some(Duration::from_millis(50)))
        .expect("task should complete");
    rt.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }

    assert_eq!(result.bytes, expected.bytes);
}

#[test]
fn custom_executor_failure_propagates_to_await() {
    let rt = WorkerRuntime::with_executor(
        WorkerConfig {
            threads: 1,
            queue_capacity: 16,
        },
        |_form, _opts, _ctx| {
            Err(TaskError::Failed(Signal {
                symbol: "executor-failed".to_string(),
                data: Some("boom".to_string()),
            }))
        },
    );
    let workers = rt.start_dummy_workers();
    let task = rt
        .spawn(LispValue::default(), TaskOptions::default())
        .expect("task should enqueue");

    let result = TaskScheduler::task_await(&rt, task, Some(Duration::from_millis(50)))
        .expect_err("task should surface execution failure");

    rt.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }

    match result {
        TaskError::Failed(signal) => {
            assert_eq!(signal.symbol, "executor-failed");
            assert_eq!(signal.data.as_deref(), Some("boom"));
        }
        _ => panic!("expected task execution failure"),
    }
}

#[test]
fn custom_executor_retains_configured_worker_concurrency() {
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let worker_release = Arc::clone(&release);
    let (entered, entries) = std::sync::mpsc::channel();
    let rt = WorkerRuntime::with_executor(
        WorkerConfig {
            threads: 3,
            queue_capacity: 16,
        },
        move |form, _opts, _ctx| {
            entered.send(thread::current().id()).expect("report entry");
            let (released, ready) = &*worker_release;
            let mut released = released.lock().expect("release lock");
            while !*released {
                released = ready.wait(released).expect("release wait");
            }
            Ok(form.clone())
        },
    );
    let workers = rt.try_start_workers().expect("custom workers ready");
    let tasks: Vec<_> = (0..3)
        .map(|_| {
            rt.spawn(LispValue::default(), TaskOptions::default())
                .expect("task enqueued")
        })
        .collect();
    let mut owner_threads = std::collections::HashSet::new();
    for _ in 0..3 {
        match entries.recv_timeout(Duration::from_secs(2)) {
            Ok(owner) => {
                owner_threads.insert(owner);
            }
            Err(_) => break,
        }
    }

    // Release even on a failed concurrency check so all owned workers can join.
    let (released, ready) = &*release;
    *released.lock().expect("release lock") = true;
    ready.notify_all();
    rt.close();
    for worker in workers {
        worker.join().expect("worker joined");
    }
    for task in tasks {
        TaskScheduler::task_await(&rt, task, Some(Duration::from_secs(1)))
            .expect("custom task completed");
    }
    assert_eq!(owner_threads.len(), 3, "all configured workers overlap");
}

#[test]
fn elisp_factory_constructs_context_only_on_its_owner() {
    use neovm_core::tagged::gc::tagged_heap_is_installed;

    assert!(!tagged_heap_is_installed(), "caller has no Lisp heap");
    let caller = thread::current().id();
    let constructed_on = Arc::new(Mutex::new(Vec::new()));
    let owner_records = Arc::clone(&constructed_on);
    let rt = WorkerRuntime::with_executor_factory(
        WorkerConfig {
            threads: 4,
            queue_capacity: 16,
        },
        Executor::Elisp(Arc::new(move || {
            assert!(!tagged_heap_is_installed(), "fresh owner has no heap");
            owner_records
                .lock()
                .expect("owner records")
                .push(thread::current().id());
            // Same public boot path as production: `eval_hook_installers::install_eval_hooks`
            // mints the proof, and `Context::new` takes it by value.
            Ok(Box::new(Context::new(
                neovm_core::emacs_core::eval_hook_installers::install_eval_hooks(),
            )))
        })),
    );
    assert!(constructed_on.lock().expect("owner records").is_empty());
    assert!(
        !tagged_heap_is_installed(),
        "factory leaves caller TLS alone"
    );

    let workers = rt.try_start_workers().expect("owner ready");
    assert_eq!(workers.len(), 1, "one shared Lisp state has one owner");
    let owners = constructed_on.lock().expect("owner records");
    assert_eq!(owners.len(), 1);
    assert_ne!(owners[0], caller);
    drop(owners);
    assert!(
        !tagged_heap_is_installed(),
        "readiness leaves caller TLS alone"
    );

    let task = rt
        .spawn(
            LispValue {
                bytes: b"(+ 20 22)".to_vec(),
            },
            TaskOptions::default(),
        )
        .expect("task enqueued");
    let result = TaskScheduler::task_await(&rt, task, Some(Duration::from_secs(1)))
        .expect("owner reader and evaluator have installed TLS");
    rt.close();
    for worker in workers {
        worker.join().expect("owner joined");
    }
    assert_eq!(result.bytes.as_slice(), b"42");
    assert!(
        !tagged_heap_is_installed(),
        "owner teardown leaves caller TLS alone"
    );
}

#[test]
fn elisp_startup_failure_finishes_pending_tasks() {
    let rt = WorkerRuntime::with_executor_factory(
        WorkerConfig {
            threads: 4,
            queue_capacity: 16,
        },
        Executor::Elisp(Arc::new(|| {
            Err(WorkerStartError::Initialization(
                "test factory failure".to_string(),
            ))
        })),
    );
    let task = rt
        .spawn(LispValue::default(), TaskOptions::default())
        .expect("task enqueued before startup");
    let err = rt.try_start_workers().expect_err("failure reaches caller");
    assert!(matches!(err, WorkerStartError::Initialization(_)));
    let result = TaskScheduler::task_await(&rt, task, Some(Duration::from_secs(1)))
        .expect_err("pending task gets a startup failure");
    match result {
        TaskError::Failed(signal) => {
            assert_eq!(signal.symbol, "worker-startup-failed");
            assert!(
                signal
                    .data
                    .expect("failure detail")
                    .contains("test factory failure")
            );
        }
        other => panic!("expected startup failure, got {other:?}"),
    }
    assert_eq!(rt.task_status(task), Some(TaskStatus::Completed));
    assert_eq!(rt.stats().completed, 1);
    assert_eq!(rt.reap_finished(1), 1);
    assert_eq!(
        rt.spawn(LispValue::default(), TaskOptions::default()),
        Err(EnqueueError::Closed)
    );
}

#[test]
fn elisp_startup_panic_reaches_caller_and_pending_awaiter() {
    let rt = WorkerRuntime::with_executor_factory(
        WorkerConfig::default(),
        Executor::Elisp(Arc::new(|| panic!("test factory panic"))),
    );
    let task = rt
        .spawn(LispValue::default(), TaskOptions::default())
        .expect("task enqueued before startup");
    assert!(matches!(
        rt.try_start_workers(),
        Err(WorkerStartError::InitializationPanicked)
    ));
    assert!(matches!(
        TaskScheduler::task_await(&rt, task, Some(Duration::from_secs(1))),
        Err(TaskError::Failed(Signal { symbol, .. })) if symbol == "worker-startup-failed"
    ));
}

#[test]
fn elisp_zero_workers_finishes_pending_tasks() {
    let rt = WorkerRuntime::with_elisp_executor(WorkerConfig {
        threads: 0,
        queue_capacity: 16,
    });
    let task = rt
        .spawn(LispValue::default(), TaskOptions::default())
        .expect("task enqueued before startup");
    assert!(matches!(
        rt.try_start_workers(),
        Err(WorkerStartError::NoWorkers)
    ));
    assert!(matches!(
        TaskScheduler::task_await(&rt, task, Some(Duration::from_secs(1))),
        Err(TaskError::Failed(Signal { symbol, .. })) if symbol == "worker-startup-failed"
    ));
}

#[test]
fn channel_send_recv_round_trip() {
    let rt = WorkerRuntime::new(WorkerConfig::default());
    let channel = rt.make_channel(2);
    rt.channel_send(channel, LispValue { bytes: vec![7, 8] }, None)
        .expect("send should succeed");

    let value = rt
        .channel_recv(channel, None)
        .expect("recv should succeed")
        .expect("channel should produce a value");
    assert_eq!(value.bytes, vec![7, 8]);
}

#[test]
fn select_reports_ready_recv() {
    let rt = WorkerRuntime::new(WorkerConfig::default());
    let channel = rt.make_channel(1);
    rt.channel_send(channel, LispValue { bytes: vec![1] }, None)
        .expect("send should succeed");

    let result = TaskScheduler::select(
        &rt,
        &[SelectOp::Recv(channel)],
        Some(Duration::from_millis(5)),
    );
    match result {
        SelectResult::Ready {
            op_index: 0,
            value: Some(value),
        } => assert_eq!(value.bytes, vec![1]),
        _ => panic!("expected ready recv"),
    }
}

#[test]
fn select_reports_timeout_when_blocked() {
    let rt = WorkerRuntime::new(WorkerConfig::default());
    let channel = rt.make_channel(1);
    let result = TaskScheduler::select(
        &rt,
        &[SelectOp::Recv(channel)],
        Some(Duration::from_millis(2)),
    );
    assert!(matches!(result, SelectResult::TimedOut));
}

#[test]
fn select_wakes_when_channel_becomes_ready() {
    let rt = Arc::new(WorkerRuntime::new(WorkerConfig::default()));
    let channel = rt.make_channel(1);

    let rt_sender = Arc::clone(&rt);
    let sender = thread::spawn(move || {
        thread::sleep(Duration::from_millis(2));
        rt_sender
            .channel_send(
                channel,
                LispValue {
                    bytes: vec![42, 24],
                },
                Some(Duration::from_millis(50)),
            )
            .expect("sender should publish value");
    });

    let result = TaskScheduler::select(
        &*rt,
        &[SelectOp::Recv(channel)],
        Some(Duration::from_millis(100)),
    );
    sender.join().expect("sender thread should join");

    match result {
        SelectResult::Ready {
            op_index: 0,
            value: Some(value),
        } => assert_eq!(value.bytes, vec![42, 24]),
        _ => panic!("expected select to wake with recv"),
    }
}

#[test]
fn close_channel_returns_none_on_recv() {
    let rt = WorkerRuntime::new(WorkerConfig::default());
    let channel = rt.make_channel(1);
    assert!(rt.close_channel(channel));
    let value = rt
        .channel_recv(channel, Some(Duration::from_millis(1)))
        .expect("recv on closed channel should return gracefully");
    assert_eq!(value, None);
}

#[test]
fn runtime_stats_track_rejections_and_cancellation() {
    let rt = WorkerRuntime::new(WorkerConfig {
        threads: 0,
        queue_capacity: 1,
    });

    let queued = rt
        .spawn(LispValue::default(), TaskOptions::default())
        .expect("first task should enqueue");

    rt.spawn(LispValue::default(), TaskOptions::default())
        .expect_err("second task should hit queue limit");

    rt.spawn(
        LispValue::default(),
        TaskOptions {
            affinity: Affinity::MainOnly,
            ..TaskOptions::default()
        },
    )
    .expect_err("main-only task should be rejected");

    rt.close();
    rt.spawn(LispValue::default(), TaskOptions::default())
        .expect_err("closed runtime should reject new tasks");

    assert!(rt.cancel(queued));

    let stats = rt.stats();
    assert_eq!(stats.enqueued, 1);
    assert_eq!(stats.rejected_full, 1);
    assert_eq!(stats.rejected_affinity, 1);
    assert_eq!(stats.rejected_closed, 1);
    assert_eq!(stats.cancelled, 1);
}

#[test]
fn reap_finished_removes_completed_task_entries() {
    let rt = WorkerRuntime::new(WorkerConfig {
        threads: 1,
        queue_capacity: 16,
    });
    let workers = rt.start_dummy_workers();

    let task = rt
        .spawn(
            LispValue {
                bytes: vec![3, 1, 4],
            },
            TaskOptions::default(),
        )
        .expect("task should enqueue");

    let _ = TaskScheduler::task_await(&rt, task, Some(Duration::from_millis(50)))
        .expect("task should complete");
    assert_eq!(rt.task_status(task), Some(TaskStatus::Completed));

    rt.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }

    // Await publishes the task result before the worker queues its finished
    // handle. Join establishes that this separate bookkeeping is complete.
    let reaped = rt.reap_finished(8);
    assert_eq!(reaped, 1);
    assert_eq!(rt.task_status(task), None);
}

#[test]
fn reap_finished_removes_cancelled_task_entries() {
    let rt = WorkerRuntime::new(WorkerConfig {
        threads: 0,
        queue_capacity: 16,
    });
    let task = rt
        .spawn(LispValue::default(), TaskOptions::default())
        .expect("task should enqueue");
    assert!(rt.cancel(task));
    assert_eq!(rt.task_status(task), Some(TaskStatus::Cancelled));

    let reaped = rt.reap_finished(8);
    assert_eq!(reaped, 1);
    assert_eq!(rt.task_status(task), None);
}

#[test]
fn close_cancels_queued_work_and_interrupts_running_work() {
    let (entered, entered_rx) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let task_release_rx = Arc::clone(&release_rx);
    let rt = WorkerRuntime::with_executor(
        WorkerConfig {
            threads: 1,
            queue_capacity: 4,
        },
        move |form, _opts, _ctx| {
            if form.bytes == [1] {
                entered.send(()).expect("report running task");
                task_release_rx
                    .lock()
                    .expect("release receiver")
                    .recv()
                    .expect("release running task");
            }
            Ok(form.clone())
        },
    );
    let workers = rt.start_dummy_workers();
    let running = rt
        .spawn(LispValue { bytes: vec![1] }, TaskOptions::default())
        .expect("running task queued");
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("worker entered running task");
    let queued = rt
        .spawn(LispValue { bytes: vec![2] }, TaskOptions::default())
        .expect("queued task accepted before close");

    let release_on_close = release.clone();
    rt.tasks
        .read()
        .expect("task map")
        .get(&running.0)
        .expect("running task registered")
        .interrupt
        .lock()
        .expect("interrupt hook")
        .replace(Interrupt(Arc::new(move || {
            release_on_close.send(()).expect("release running task");
        })));

    rt.close();
    assert_eq!(rt.task_status(queued), Some(TaskStatus::Cancelled));
    assert_eq!(rt.task_status(running), Some(TaskStatus::Cancelled));
    for worker in workers {
        worker.join().expect("worker should finish");
    }
    assert_eq!(
        TaskScheduler::task_await(&rt, running, Some(Duration::from_secs(1)))
            .expect_err("running work is interrupted during close"),
        TaskError::Cancelled
    );
}

#[test]
fn cancel_running_task_calls_interrupt_once_and_clears_hook() {
    let (entered, entered_rx) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let task_release_rx = Arc::clone(&release_rx);
    let rt = WorkerRuntime::with_executor(
        WorkerConfig {
            threads: 1,
            queue_capacity: 4,
        },
        move |form, _opts, _ctx| {
            if form.bytes == [1] {
                entered.send(()).expect("report running task");
                task_release_rx
                    .lock()
                    .expect("release receiver")
                    .recv()
                    .expect("release running task");
            }
            Ok(form.clone())
        },
    );
    let workers = rt.start_dummy_workers();
    let task = rt
        .spawn(LispValue { bytes: vec![1] }, TaskOptions::default())
        .expect("task enqueued");
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("worker entered task");
    let interrupts = Arc::new(AtomicU64::new(0));
    let interrupt_count = Arc::clone(&interrupts);
    rt.tasks
        .read()
        .expect("task map")
        .get(&task.0)
        .expect("task registered")
        .interrupt
        .lock()
        .expect("interrupt hook")
        .replace(Interrupt(Arc::new(move || {
            interrupt_count.fetch_add(1, Ordering::Relaxed);
        })));

    assert!(rt.cancel(task));
    assert!(rt.cancel(task));
    release.send(()).expect("release cancelled task");
    assert_eq!(
        TaskScheduler::task_await(&rt, task, Some(Duration::from_secs(1))),
        Err(TaskError::Cancelled)
    );
    let mut hook_cleared = false;
    for _ in 0..1000 {
        if rt
            .tasks
            .read()
            .expect("task map")
            .get(&task.0)
            .expect("task retained until wait")
            .interrupt
            .lock()
            .expect("interrupt hook")
            .is_none()
        {
            hook_cleared = true;
            break;
        }
        thread::yield_now();
    }
    assert!(
        hook_cleared,
        "owner clears the interrupt hook before accepting its next task"
    );
    let next = rt
        .spawn(LispValue { bytes: vec![2] }, TaskOptions::default())
        .expect("next task enqueued after hook cleared");
    assert_eq!(
        TaskScheduler::task_await(&rt, next, Some(Duration::from_secs(1)))
            .expect("next task completes"),
        LispValue { bytes: vec![2] }
    );
    rt.close();
    for worker in workers {
        worker.join().expect("worker should finish");
    }
    assert_eq!(interrupts.load(Ordering::Relaxed), 1);
}

#[test]
fn isolate_startup_keeps_successful_owners_when_one_fails() {
    let attempts = Arc::new(AtomicU64::new(0));
    let factory_attempts = Arc::clone(&attempts);
    let rt = WorkerRuntime::with_isolate_executor(
        WorkerConfig {
            threads: 2,
            queue_capacity: 8,
        },
        Arc::new(move || {
            if factory_attempts.fetch_add(1, Ordering::Relaxed) == 0 {
                return Err(WorkerStartError::Initialization(
                    "one owner failed".to_string(),
                ));
            }
            Ok(Box::new(Context::new(
                neovm_core::emacs_core::eval_hook_installers::install_eval_hooks(),
            )))
        }),
    );
    let workers = rt.try_start_workers().expect("one owner is usable");
    assert_eq!(rt.boot_times().len(), 1);
    assert!(
        rt.startup_error()
            .expect("startup failure retained")
            .contains("one owner failed")
    );
    let task = rt
        .spawn(
            LispValue {
                bytes: crate::transfer::encode_call(&crate::transfer::Call {
                    function: crate::transfer::Transfer::Subr("+".to_string()),
                    args: vec![
                        crate::transfer::Transfer::Fixnum(20),
                        crate::transfer::Transfer::Fixnum(22),
                    ],
                }),
            },
            TaskOptions::default(),
        )
        .expect("remaining owner accepts a task");
    let result = TaskScheduler::task_await(&rt, task, Some(Duration::from_secs(2)))
        .expect("remaining owner completes task");
    rt.close();
    for worker in workers {
        worker.join().expect("owner should join");
    }
    assert_eq!(
        crate::transfer::decode(&result.bytes).expect("decode task result"),
        crate::transfer::Transfer::Fixnum(42)
    );
}

#[test]
fn isolate_startup_fails_all_owners_and_reports_pending_task() {
    let rt = WorkerRuntime::with_isolate_executor(
        WorkerConfig {
            threads: 2,
            queue_capacity: 8,
        },
        Arc::new(|| {
            Err(WorkerStartError::Initialization(
                "all owners failed".to_string(),
            ))
        }),
    );
    let task = rt
        .spawn(LispValue::default(), TaskOptions::default())
        .expect("task queued before startup");
    assert!(matches!(
        rt.try_start_workers(),
        Err(WorkerStartError::Initialization(_))
    ));
    assert!(matches!(
        TaskScheduler::task_await(&rt, task, Some(Duration::from_secs(1))),
        Err(TaskError::Failed(Signal { symbol, .. })) if symbol == "worker-startup-failed"
    ));
    assert_eq!(rt.boot_times().len(), 0);
}

#[test]
fn four_cached_isolates_report_boots_and_complete_sixteen_tasks() {
    let pool = Box::leak(Box::new(crate::isolate::IsolatePool::new(
        crate::isolate::IsolatePoolConfig {
            threads: 4,
            queue_capacity: 32,
            boot: crate::isolate::IsolateBoot::CachedBootstrap,
        },
    )));
    pool.ensure_started();
    pool.wait_started();
    let boot_times = pool.runtime().boot_times();
    assert_eq!(boot_times.len(), 4);
    let stats = pool.stats();
    assert_eq!(stats.ready, 4);
    assert_eq!(stats.boot, crate::isolate::IsolateBoot::CachedBootstrap);
    assert!(!stats.final_image_used);

    let call = crate::transfer::Call {
        function: crate::transfer::Transfer::Subr("+".to_string()),
        args: vec![
            crate::transfer::Transfer::Fixnum(20),
            crate::transfer::Transfer::Fixnum(22),
        ],
    };
    for _ in 0..16 {
        let task = crate::isolate::spawn_call(pool, &call).expect("task enqueued");
        assert_eq!(
            crate::transfer::decode(
                &TaskScheduler::task_await(pool.runtime(), task, Some(Duration::from_secs(5)))
                    .expect("task result")
                    .bytes
            )
            .expect("decode task result"),
            crate::transfer::Transfer::Fixnum(42)
        );
    }
    while pool.stats().completed < 16 {
        thread::yield_now();
    }
    let stats = pool.stats();
    eprintln!("isolate boot times: {:?}", boot_times);
    assert_eq!(stats.completed, 16);
    assert_eq!(stats.ready, 4);
    pool.close();
}

#[test]
fn configured_pool_width_uses_lisp_value_or_environment_default() {
    let pool = crate::isolate::IsolatePool::new(crate::isolate::IsolatePoolConfig {
        threads: 8,
        queue_capacity: 8,
        boot: crate::isolate::IsolateBoot::CachedBootstrap,
    });

    assert_eq!(pool.configured_threads(None), 8);
    assert_eq!(pool.configured_threads(Some(0)), 0);
    assert_eq!(pool.configured_threads(Some(3)), 3);
}

#[test]
fn disabled_pool_keeps_zero_width_and_reports_startup_error() {
    let pool = Box::leak(Box::new(crate::isolate::IsolatePool::new(
        crate::isolate::IsolatePoolConfig {
            threads: 8,
            queue_capacity: 8,
            boot: crate::isolate::IsolateBoot::CachedBootstrap,
        },
    )));

    pool.ensure_started_with(Some(0));
    pool.wait_started();

    assert_eq!(pool.threads(), 0);
    assert_eq!(pool.configured_threads(Some(3)), 0);
    assert!(pool.stats().startup_error.is_some());
}

#[test]
fn elisp_executor_evaluates_source_task() {
    let rt = WorkerRuntime::with_elisp_executor(WorkerConfig {
        threads: 1,
        queue_capacity: 16,
    });
    let workers = rt.start_dummy_workers();

    let task = rt
        .spawn(
            LispValue {
                bytes: b"(+ 20 22)".to_vec(),
            },
            TaskOptions::default(),
        )
        .expect("task should enqueue");
    let result = TaskScheduler::task_await(&rt, task, Some(Duration::from_millis(50)))
        .expect("task should evaluate");

    rt.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }

    assert_eq!(String::from_utf8(result.bytes).expect("utf8 output"), "42");
}

#[test]
fn elisp_executor_maps_signal_errors() {
    let rt = WorkerRuntime::with_elisp_executor(WorkerConfig {
        threads: 1,
        queue_capacity: 16,
    });
    let workers = rt.start_dummy_workers();

    let task = rt
        .spawn(
            LispValue {
                bytes: b"(/ 1 0)".to_vec(),
            },
            TaskOptions::default(),
        )
        .expect("task should enqueue");
    let result = TaskScheduler::task_await(&rt, task, Some(Duration::from_millis(50)))
        .expect_err("arith-error should propagate");

    rt.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }

    match result {
        TaskError::Failed(signal) => assert_eq!(signal.symbol, "arith-error"),
        _ => panic!("expected failed signal"),
    }
}

#[test]
fn elisp_executor_persists_defun_state() {
    use neovm_core::tagged::gc::tagged_heap_is_installed;

    assert!(!tagged_heap_is_installed(), "caller starts without a heap");
    let rt = WorkerRuntime::with_elisp_executor(WorkerConfig {
        threads: 4,
        queue_capacity: 16,
    });
    assert!(
        !tagged_heap_is_installed(),
        "public factory leaves caller TLS alone"
    );
    let workers = rt.start_dummy_workers();
    assert_eq!(workers.len(), 1, "shared Lisp state stays on one owner");
    assert!(
        !tagged_heap_is_installed(),
        "worker readiness leaves caller TLS alone"
    );

    let define = rt
        .spawn(
            LispValue {
                bytes: b"(defun plus1 (x) (+ x 1))".to_vec(),
            },
            TaskOptions::default(),
        )
        .expect("defun task should enqueue");
    let call = rt
        .spawn(
            LispValue {
                bytes: b"(plus1 41)".to_vec(),
            },
            TaskOptions::default(),
        )
        .expect("call task should enqueue");

    let define_out = TaskScheduler::task_await(&rt, define, Some(Duration::from_millis(50)))
        .expect("defun should succeed");
    let call_out = TaskScheduler::task_await(&rt, call, Some(Duration::from_millis(50)))
        .expect("function call should succeed");

    assert!(matches!(
        rt.try_start_workers(),
        Err(WorkerStartError::AlreadyStarted)
    ));
    let later_call = rt
        .spawn(
            LispValue {
                bytes: b"(plus1 99)".to_vec(),
            },
            TaskOptions::default(),
        )
        .expect("second start rejection preserves the live queue");
    let later_out = TaskScheduler::task_await(&rt, later_call, Some(Duration::from_secs(1)))
        .expect("original owner preserves its definitions");

    rt.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }

    assert_eq!(
        String::from_utf8(define_out.bytes).expect("utf8 output"),
        "plus1"
    );
    assert_eq!(
        String::from_utf8(call_out.bytes).expect("utf8 output"),
        "42"
    );
    assert_eq!(later_out.bytes.as_slice(), b"100");
}

#[test]
fn elisp_executor_uses_bootstrap_runtime_surface() {
    let rt = WorkerRuntime::with_elisp_executor(WorkerConfig {
        threads: 1,
        queue_capacity: 16,
    });
    let workers = rt.start_dummy_workers();

    let task = rt
        .spawn(
            LispValue {
                bytes: br#"(list
                              (featurep 'seq)
                              (featurep 'cl-generic)
                              (featurep 'cl-lib)
                              (featurep 'gv)
                              (condition-case err (require 'cl-lib) (error err))
                              (condition-case err (require 'gv) (error err))
                              (autoloadp (symbol-function 'cl-subseq))
                              (macrop 'gv-define-setter))"#
                    .to_vec(),
            },
            TaskOptions::default(),
        )
        .expect("task should enqueue");
    let result = TaskScheduler::task_await(&rt, task, Some(Duration::from_secs(1)))
        .expect("bootstrap runtime probe should succeed");

    rt.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }

    assert_eq!(
        String::from_utf8(result.bytes).expect("utf8 output"),
        "(t t nil nil cl-lib gv t t)"
    );
}

#[test]
fn elisp_executor_propagates_kill_emacs_as_a_shutdown_order() {
    // GNU: a Lisp thread calling kill-emacs kills Emacs. A worker task is this
    // engine's nearest equivalent, so kill-emacs escaping a task is not that
    // task's failure -- it is an order the host must obey. Reporting it as
    // TaskError::Failed let a host log it beside an arith-error and keep
    // running, which is how a shutdown gets absorbed.
    let rt = WorkerRuntime::with_elisp_executor(WorkerConfig {
        threads: 1,
        queue_capacity: 16,
    });
    let workers = rt.start_dummy_workers();

    assert_eq!(rt.shutdown_request(), None);

    let task = rt
        .spawn(
            LispValue {
                bytes: b"(kill-emacs 7)".to_vec(),
            },
            TaskOptions::default(),
        )
        .expect("task should enqueue");
    let result = TaskScheduler::task_await(&rt, task, Some(Duration::from_millis(500)))
        .expect_err("kill-emacs does not produce a task value");

    assert_eq!(
        result,
        TaskError::Shutdown(ShutdownOrder {
            exit_code: 7,
            restart: false
        }),
        "the exit code is the task's outcome, not a signal payload"
    );

    // The runtime latches it, so a host that never inspects an individual task
    // result still learns the process was asked to exit.
    assert_eq!(
        rt.shutdown_request(),
        Some(ShutdownOrder {
            exit_code: 7,
            restart: false
        })
    );

    // A shutdown outranks pending work: nothing further is accepted.
    let rejected = rt.spawn(
        LispValue {
            bytes: b"(+ 1 1)".to_vec(),
        },
        TaskOptions::default(),
    );
    assert!(
        matches!(rejected, Err(EnqueueError::Closed)),
        "a runtime under a shutdown order must not accept more work, got {rejected:?}"
    );

    rt.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }
}
