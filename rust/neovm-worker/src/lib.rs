use neovm_core::{TaskHandle, TaskStatus};
use neovm_host_abi::{Affinity, LispValue, TaskOptions};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;

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

#[derive(Debug)]
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

    fn set_status(&self, status: TaskStatus) {
        let mut slot = self.status.lock().expect("task status mutex poisoned");
        *slot = status;
    }

    fn status(&self) -> TaskStatus {
        *self.status.lock().expect("task status mutex poisoned")
    }
}

#[derive(Default)]
struct QueueState {
    queue: VecDeque<TaskHandle>,
    closed: bool,
}

#[derive(Default)]
struct SharedQueue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

pub struct WorkerRuntime {
    config: WorkerConfig,
    next_task: AtomicU64,
    queue: Arc<SharedQueue>,
    tasks: Arc<RwLock<HashMap<u64, Arc<TaskEntry>>>>,
}

impl WorkerRuntime {
    pub fn new(config: WorkerConfig) -> Self {
        Self {
            config,
            next_task: AtomicU64::new(1),
            queue: Arc::new(SharedQueue::default()),
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn config(&self) -> WorkerConfig {
        self.config
    }

    pub fn spawn(&self, form: LispValue, opts: TaskOptions) -> Result<TaskHandle, EnqueueError> {
        if opts.affinity == Affinity::MainOnly {
            return Err(EnqueueError::MainAffinityUnsupported);
        }

        let handle = TaskHandle(self.next_task.fetch_add(1, Ordering::Relaxed));
        let task = Arc::new(TaskEntry::new(form, opts));

        {
            let mut state = self.queue.state.lock().expect("worker queue mutex poisoned");
            if state.closed {
                return Err(EnqueueError::Closed);
            }
            if state.queue.len() >= self.config.queue_capacity {
                return Err(EnqueueError::QueueFull);
            }
            state.queue.push_back(handle);
        }

        {
            let mut tasks = self.tasks.write().expect("tasks map rwlock poisoned");
            tasks.insert(handle.0, task);
        }

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

        if task.status() == TaskStatus::Queued {
            task.set_status(TaskStatus::Cancelled);
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
            joins.push(thread::spawn(move || {
                loop {
                    let handle = {
                        let mut state = queue.state.lock().expect("worker queue mutex poisoned");
                        while state.queue.is_empty() && !state.closed {
                            state = queue
                                .ready
                                .wait(state)
                                .expect("worker queue condvar wait failed");
                        }

                        if state.closed && state.queue.is_empty() {
                            return;
                        }

                        state.queue.pop_front()
                    };

                    let Some(handle) = handle else {
                        continue;
                    };

                    let task = {
                        let tasks = tasks.read().expect("tasks map rwlock poisoned");
                        tasks.get(&handle.0).cloned()
                    };

                    let Some(task) = task else {
                        continue;
                    };

                    if task.context.is_cancelled() || task.status() == TaskStatus::Cancelled {
                        task.set_status(TaskStatus::Cancelled);
                        continue;
                    }

                    task.set_status(TaskStatus::Running);

                    // Placeholder execution path: a real runtime would evaluate task.form
                    // inside an isolate and write the result to a completion channel.
                    let _ = task.form.bytes.len();
                    let _ = task.opts.name.as_deref();

                    if task.context.is_cancelled() {
                        task.set_status(TaskStatus::Cancelled);
                    } else {
                        task.set_status(TaskStatus::Completed);
                    }
                }
            }));
        }
        joins
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
