use neovm_core::{TaskHandle, TaskScheduler, TaskStatus};
use neovm_host_abi::{
    Affinity, ChannelId, LispValue, SelectOp, SelectResult, Signal, TaskError, TaskOptions,
    TaskPriority,
};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerConfig {
    pub threads: usize,
    pub queue_capacity: usize,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            threads: 1,
            queue_capacity: 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeStats {
    pub enqueued: u64,
    pub dequeued: u64,
    pub completed: u64,
    pub cancelled: u64,
    pub rejected_closed: u64,
    pub rejected_full: u64,
    pub rejected_affinity: u64,
}

#[derive(Default)]
struct RuntimeMetrics {
    enqueued: AtomicU64,
    dequeued: AtomicU64,
    completed: AtomicU64,
    cancelled: AtomicU64,
    rejected_closed: AtomicU64,
    rejected_full: AtomicU64,
    rejected_affinity: AtomicU64,
}

impl RuntimeMetrics {
    fn snapshot(&self) -> RuntimeStats {
        RuntimeStats {
            enqueued: self.enqueued.load(Ordering::Relaxed),
            dequeued: self.dequeued.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            cancelled: self.cancelled.load(Ordering::Relaxed),
            rejected_closed: self.rejected_closed.load(Ordering::Relaxed),
            rejected_full: self.rejected_full.load(Ordering::Relaxed),
            rejected_affinity: self.rejected_affinity.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TaskContext {
    cancelled: Arc<AtomicBool>,
}

impl TaskContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnqueueError {
    Closed,
    QueueFull,
    MainAffinityUnsupported,
}

#[derive(Debug)]
struct TaskEntry {
    form: LispValue,
    opts: TaskOptions,
    context: TaskContext,
    status: Mutex<TaskStatus>,
}

impl TaskEntry {
    fn new(form: LispValue, opts: TaskOptions) -> Self {
        Self {
            form,
            opts,
            context: TaskContext {
                cancelled: Arc::new(AtomicBool::new(false)),
            },
            status: Mutex::new(TaskStatus::Queued),
        }
    }

    fn status(&self) -> TaskStatus {
        *self.status.lock().expect("task status mutex poisoned")
    }

    fn mark_running(&self) -> bool {
        let mut status = self.status.lock().expect("task status mutex poisoned");
        if *status == TaskStatus::Queued {
            *status = TaskStatus::Running;
            true
        } else {
            false
        }
    }

    fn mark_cancelled(&self) -> bool {
        let mut status = self.status.lock().expect("task status mutex poisoned");
        match *status {
            TaskStatus::Completed | TaskStatus::Cancelled => false,
            TaskStatus::Queued | TaskStatus::Running => {
                *status = TaskStatus::Cancelled;
                true
            }
        }
    }

    fn mark_completed(&self) -> bool {
        let mut status = self.status.lock().expect("task status mutex poisoned");
        match *status {
            TaskStatus::Cancelled | TaskStatus::Completed => false,
            TaskStatus::Queued | TaskStatus::Running => {
                *status = TaskStatus::Completed;
                true
            }
        }
    }
}

#[derive(Default)]
struct QueueState {
    interactive: VecDeque<TaskHandle>,
    default: VecDeque<TaskHandle>,
    background: VecDeque<TaskHandle>,
    closed: bool,
}

impl QueueState {
    fn len(&self) -> usize {
        self.interactive.len() + self.default.len() + self.background.len()
    }

    fn is_empty(&self) -> bool {
        self.interactive.is_empty() && self.default.is_empty() && self.background.is_empty()
    }

    fn push(&mut self, handle: TaskHandle, priority: TaskPriority) {
        match priority {
            TaskPriority::Interactive => self.interactive.push_back(handle),
            TaskPriority::Default => self.default.push_back(handle),
            TaskPriority::Background => self.background.push_back(handle),
        }
    }

    fn pop(&mut self) -> Option<TaskHandle> {
        self.interactive
            .pop_front()
            .or_else(|| self.default.pop_front())
            .or_else(|| self.background.pop_front())
    }
}

#[derive(Default)]
struct SharedQueue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChannelError {
    Closed,
    TimedOut,
}

#[derive(Default)]
struct ChannelState {
    queue: VecDeque<LispValue>,
    closed: bool,
}

struct Channel {
    state: Mutex<ChannelState>,
    space_or_data: Condvar,
    capacity: usize,
}

impl Channel {
    fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(ChannelState::default()),
            space_or_data: Condvar::new(),
            // Unbuffered channels require rendezvous semantics; keep this first
            // implementation bounded-buffer only.
            capacity: capacity.max(1),
        }
    }

    fn try_send(&self, value: LispValue) -> Result<(), ChannelError> {
        let mut state = self.state.lock().expect("channel mutex poisoned");
        if state.closed {
            return Err(ChannelError::Closed);
        }
        if state.queue.len() >= self.capacity {
            return Err(ChannelError::TimedOut);
        }

        state.queue.push_back(value);
        drop(state);
        self.space_or_data.notify_one();
        Ok(())
    }

    fn try_recv(&self) -> Result<Option<LispValue>, ChannelError> {
        let mut state = self.state.lock().expect("channel mutex poisoned");
        if let Some(value) = state.queue.pop_front() {
            drop(state);
            self.space_or_data.notify_one();
            return Ok(Some(value));
        }
        if state.closed {
            return Ok(None);
        }
        Err(ChannelError::TimedOut)
    }

    fn send(&self, value: LispValue, timeout: Option<Duration>) -> Result<(), ChannelError> {
        let mut state = self.state.lock().expect("channel mutex poisoned");
        if state.closed {
            return Err(ChannelError::Closed);
        }
        if state.queue.len() < self.capacity {
            state.queue.push_back(value);
            drop(state);
            self.space_or_data.notify_one();
            return Ok(());
        }

        let Some(timeout) = timeout else {
            return Err(ChannelError::TimedOut);
        };
        let deadline = Instant::now() + timeout;
        let pending = value;

        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(ChannelError::TimedOut);
            }
            let wait_for = deadline.saturating_duration_since(now);
            let (next_state, wait_result) = self
                .space_or_data
                .wait_timeout(state, wait_for)
                .expect("channel condvar wait failed");
            state = next_state;

            if state.closed {
                return Err(ChannelError::Closed);
            }

            if state.queue.len() < self.capacity {
                state.queue.push_back(pending);
                drop(state);
                self.space_or_data.notify_one();
                return Ok(());
            }

            if wait_result.timed_out() {
                return Err(ChannelError::TimedOut);
            }
        }
    }

    fn recv(&self, timeout: Option<Duration>) -> Result<Option<LispValue>, ChannelError> {
        let mut state = self.state.lock().expect("channel mutex poisoned");
        if let Some(value) = state.queue.pop_front() {
            drop(state);
            self.space_or_data.notify_one();
            return Ok(Some(value));
        }
        if state.closed {
            return Ok(None);
        }

        let Some(timeout) = timeout else {
            return Err(ChannelError::TimedOut);
        };
        let deadline = Instant::now() + timeout;

        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(ChannelError::TimedOut);
            }
            let wait_for = deadline.saturating_duration_since(now);
            let (next_state, wait_result) = self
                .space_or_data
                .wait_timeout(state, wait_for)
                .expect("channel condvar wait failed");
            state = next_state;

            if let Some(value) = state.queue.pop_front() {
                drop(state);
                self.space_or_data.notify_one();
                return Ok(Some(value));
            }
            if state.closed {
                return Ok(None);
            }

            if wait_result.timed_out() {
                return Err(ChannelError::TimedOut);
            }
        }
    }

    fn close(&self) {
        let mut state = self.state.lock().expect("channel mutex poisoned");
        state.closed = true;
        drop(state);
        self.space_or_data.notify_all();
    }
}

pub struct WorkerRuntime {
    config: WorkerConfig,
    next_task: AtomicU64,
    next_channel: AtomicU64,
    select_cursor: AtomicU64,
    queue: Arc<SharedQueue>,
    tasks: Arc<RwLock<HashMap<u64, Arc<TaskEntry>>>>,
    channels: Arc<RwLock<HashMap<u64, Arc<Channel>>>>,
    metrics: Arc<RuntimeMetrics>,
}

impl WorkerRuntime {
    pub fn new(config: WorkerConfig) -> Self {
        Self {
            config,
            next_task: AtomicU64::new(1),
            next_channel: AtomicU64::new(1),
            select_cursor: AtomicU64::new(0),
            queue: Arc::new(SharedQueue::default()),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            channels: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RuntimeMetrics::default()),
        }
    }

    pub fn config(&self) -> WorkerConfig {
        self.config
    }

    pub fn stats(&self) -> RuntimeStats {
        self.metrics.snapshot()
    }

    pub fn make_channel(&self, capacity: usize) -> ChannelId {
        let id = ChannelId(self.next_channel.fetch_add(1, Ordering::Relaxed));
        let mut channels = self.channels.write().expect("channels map rwlock poisoned");
        channels.insert(id.0, Arc::new(Channel::new(capacity)));
        id
    }

    pub fn close_channel(&self, id: ChannelId) -> bool {
        let channel = {
            let channels = self.channels.read().expect("channels map rwlock poisoned");
            channels.get(&id.0).cloned()
        };
        let Some(channel) = channel else {
            return false;
        };
        channel.close();
        true
    }

    pub fn channel_send(
        &self,
        id: ChannelId,
        value: LispValue,
        timeout: Option<Duration>,
    ) -> Result<(), Signal> {
        let channel = {
            let channels = self.channels.read().expect("channels map rwlock poisoned");
            channels.get(&id.0).cloned()
        }
        .ok_or_else(|| Signal {
            symbol: "channel-not-found".to_string(),
            data: None,
        })?;

        channel
            .send(value, timeout)
            .map_err(channel_error_to_signal)
    }

    pub fn channel_recv(
        &self,
        id: ChannelId,
        timeout: Option<Duration>,
    ) -> Result<Option<LispValue>, Signal> {
        let channel = {
            let channels = self.channels.read().expect("channels map rwlock poisoned");
            channels.get(&id.0).cloned()
        }
        .ok_or_else(|| Signal {
            symbol: "channel-not-found".to_string(),
            data: None,
        })?;

        channel.recv(timeout).map_err(channel_error_to_signal)
    }

    pub fn spawn(&self, form: LispValue, opts: TaskOptions) -> Result<TaskHandle, EnqueueError> {
        if opts.affinity == Affinity::MainOnly {
            self.metrics.rejected_affinity.fetch_add(1, Ordering::Relaxed);
            return Err(EnqueueError::MainAffinityUnsupported);
        }

        let handle = TaskHandle(self.next_task.fetch_add(1, Ordering::Relaxed));
        let priority = opts.priority;
        let task = Arc::new(TaskEntry::new(form, opts));

        {
            let mut state = self.queue.state.lock().expect("worker queue mutex poisoned");
            if state.closed {
                self.metrics.rejected_closed.fetch_add(1, Ordering::Relaxed);
                return Err(EnqueueError::Closed);
            }
            if state.len() >= self.config.queue_capacity {
                self.metrics.rejected_full.fetch_add(1, Ordering::Relaxed);
                return Err(EnqueueError::QueueFull);
            }

            // Register task before releasing the queue lock so workers cannot
            // observe a handle that has no task entry yet.
            let mut tasks = self.tasks.write().expect("tasks map rwlock poisoned");
            tasks.insert(handle.0, task);
            state.push(handle, priority);
        }

        self.metrics.enqueued.fetch_add(1, Ordering::Relaxed);
        self.queue.ready.notify_one();
        Ok(handle)
    }

    pub fn cancel(&self, handle: TaskHandle) -> bool {
        let task = {
            let tasks = self.tasks.read().expect("tasks map rwlock poisoned");
            tasks.get(&handle.0).cloned()
        };

        let Some(task) = task else {
            return false;
        };

        task.context.cancel();
        if task.mark_cancelled() {
            self.metrics.cancelled.fetch_add(1, Ordering::Relaxed);
        }
        true
    }

    pub fn task_status(&self, handle: TaskHandle) -> Option<TaskStatus> {
        let tasks = self.tasks.read().expect("tasks map rwlock poisoned");
        tasks.get(&handle.0).map(|entry| entry.status())
    }

    pub fn close(&self) {
        let mut state = self.queue.state.lock().expect("worker queue mutex poisoned");
        state.closed = true;
        drop(state);
        self.queue.ready.notify_all();
    }

    pub fn start_dummy_workers(&self) -> Vec<thread::JoinHandle<()>> {
        let mut joins = Vec::with_capacity(self.config.threads);
        for _ in 0..self.config.threads {
            let queue = Arc::clone(&self.queue);
            let tasks = Arc::clone(&self.tasks);
            let metrics = Arc::clone(&self.metrics);
            joins.push(thread::spawn(move || loop {
                let handle = {
                    let mut state = queue.state.lock().expect("worker queue mutex poisoned");
                    while state.is_empty() && !state.closed {
                        state = queue
                            .ready
                            .wait(state)
                            .expect("worker queue condvar wait failed");
                    }

                    if state.closed && state.is_empty() {
                        return;
                    }

                    state.pop()
                };

                let Some(handle) = handle else {
                    continue;
                };
                metrics.dequeued.fetch_add(1, Ordering::Relaxed);

                let task = {
                    let tasks = tasks.read().expect("tasks map rwlock poisoned");
                    tasks.get(&handle.0).cloned()
                };

                let Some(task) = task else {
                    continue;
                };

                if task.context.is_cancelled() {
                    if task.mark_cancelled() {
                        metrics.cancelled.fetch_add(1, Ordering::Relaxed);
                    }
                    continue;
                }

                if !task.mark_running() {
                    continue;
                }

                // Placeholder execution path: a real runtime would evaluate task.form
                // inside an isolate and write the result to a completion channel.
                let _ = task.form.bytes.len();
                let _ = task.opts.name.as_deref();

                if task.context.is_cancelled() {
                    if task.mark_cancelled() {
                        metrics.cancelled.fetch_add(1, Ordering::Relaxed);
                    }
                } else if task.mark_completed() {
                    metrics.completed.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }
        joins
    }

    fn task_await_result(
        &self,
        handle: TaskHandle,
        timeout: Option<Duration>,
    ) -> Result<LispValue, TaskError> {
        fn map_status(status: Option<TaskStatus>) -> Result<LispValue, TaskError> {
            match status {
                Some(TaskStatus::Completed) => Ok(LispValue::default()),
                Some(TaskStatus::Cancelled) => Err(TaskError::Cancelled),
                Some(TaskStatus::Queued) | Some(TaskStatus::Running) | None => {
                    Err(TaskError::TimedOut)
                }
            }
        }

        let Some(timeout) = timeout else {
            return map_status(self.task_status(handle));
        };

        let deadline = Instant::now() + timeout;
        loop {
            match self.task_status(handle) {
                Some(TaskStatus::Completed) => return Ok(LispValue::default()),
                Some(TaskStatus::Cancelled) => return Err(TaskError::Cancelled),
                _ => {
                    if Instant::now() >= deadline {
                        return Err(TaskError::TimedOut);
                    }
                    thread::yield_now();
                }
            }
        }
    }

    fn select_once(&self, ops: &[SelectOp], start: usize) -> Option<SelectResult> {
        if ops.is_empty() {
            return None;
        }

        for offset in 0..ops.len() {
            let index = (start + offset) % ops.len();
            match &ops[index] {
                SelectOp::Recv(channel_id) => {
                    let channel = {
                        let channels = self.channels.read().expect("channels map rwlock poisoned");
                        channels.get(&channel_id.0).cloned()
                    };
                    let Some(channel) = channel else {
                        continue;
                    };

                    match channel.try_recv() {
                        Ok(value) => {
                            return Some(SelectResult::Ready {
                                op_index: index,
                                value,
                            });
                        }
                        Err(ChannelError::TimedOut) => {}
                        Err(ChannelError::Closed) => {
                            return Some(SelectResult::Cancelled);
                        }
                    }
                }
                SelectOp::Send(channel_id, value) => {
                    let channel = {
                        let channels = self.channels.read().expect("channels map rwlock poisoned");
                        channels.get(&channel_id.0).cloned()
                    };
                    let Some(channel) = channel else {
                        continue;
                    };

                    match channel.try_send(value.clone()) {
                        Ok(()) => {
                            return Some(SelectResult::Ready {
                                op_index: index,
                                value: None,
                            });
                        }
                        Err(ChannelError::TimedOut) => {}
                        Err(ChannelError::Closed) => {
                            return Some(SelectResult::Cancelled);
                        }
                    }
                }
            }
        }

        None
    }

    fn select_ops(&self, ops: &[SelectOp], timeout: Option<Duration>) -> SelectResult {
        if ops.is_empty() {
            return SelectResult::TimedOut;
        }

        let start = (self.select_cursor.fetch_add(1, Ordering::Relaxed) as usize) % ops.len();
        if let Some(result) = self.select_once(ops, start) {
            return result;
        }

        let Some(timeout) = timeout else {
            return SelectResult::TimedOut;
        };

        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                return SelectResult::TimedOut;
            }
            if let Some(result) = self.select_once(ops, start) {
                return result;
            }
            thread::yield_now();
        }
    }
}

fn enqueue_error_to_signal(err: EnqueueError) -> Signal {
    match err {
        EnqueueError::Closed => Signal {
            symbol: "task-queue-closed".to_string(),
            data: None,
        },
        EnqueueError::QueueFull => Signal {
            symbol: "task-queue-full".to_string(),
            data: None,
        },
        EnqueueError::MainAffinityUnsupported => Signal {
            symbol: "task-main-affinity-unsupported".to_string(),
            data: None,
        },
    }
}

fn channel_error_to_signal(err: ChannelError) -> Signal {
    match err {
        ChannelError::Closed => Signal {
            symbol: "channel-closed".to_string(),
            data: None,
        },
        ChannelError::TimedOut => Signal {
            symbol: "channel-timeout".to_string(),
            data: None,
        },
    }
}

impl TaskScheduler for WorkerRuntime {
    fn spawn_task(&self, form: LispValue, opts: TaskOptions) -> Result<TaskHandle, Signal> {
        self.spawn(form, opts).map_err(enqueue_error_to_signal)
    }

    fn task_cancel(&self, handle: TaskHandle) -> bool {
        self.cancel(handle)
    }

    fn task_status(&self, handle: TaskHandle) -> Option<TaskStatus> {
        WorkerRuntime::task_status(self, handle)
    }

    fn task_await(
        &self,
        handle: TaskHandle,
        timeout: Option<Duration>,
    ) -> Result<LispValue, TaskError> {
        self.task_await_result(handle, timeout)
    }

    fn select(&self, ops: &[SelectOp], timeout: Option<Duration>) -> SelectResult {
        self.select_ops(ops, timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let err = rt.spawn(LispValue::default(), opts).expect_err("must reject");
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
}
