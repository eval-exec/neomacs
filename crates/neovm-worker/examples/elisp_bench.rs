use neovm_core::TaskScheduler;
use neovm_host_abi::{LispValue, TaskOptions};
use neovm_worker::{WorkerConfig, WorkerRuntime};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
struct BenchConfig {
    tasks: u64,
    threads: usize,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            tasks: 50_000,
            threads: thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
        }
    }
}

fn parse_config() -> BenchConfig {
    let mut cfg = BenchConfig::default();
    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--tasks=") {
            if let Ok(parsed) = value.parse::<u64>() {
                cfg.tasks = parsed.max(1);
            }
        } else if let Some(value) = arg.strip_prefix("--threads=") {
            if let Ok(parsed) = value.parse::<usize>() {
                cfg.threads = parsed.max(1);
            }
        }
    }
    cfg
}

fn main() {
    let cfg = parse_config();
    println!(
        "benchmark config: tasks={}, threads={}",
        cfg.tasks, cfg.threads
    );

    let runtime = WorkerRuntime::with_elisp_executor(WorkerConfig {
        threads: cfg.threads,
        queue_capacity: cfg.tasks as usize + 64,
    });
    let workers = runtime.start_dummy_workers();

    let warmup = runtime
        .spawn(
            LispValue {
                bytes: b"(defun add2 (x) (+ x 2))".to_vec(),
            },
            TaskOptions::default(),
        )
        .expect("warmup task should enqueue");
    let _ = TaskScheduler::task_await(&runtime, warmup, Some(Duration::from_secs(1)))
        .expect("warmup should succeed");

    let start = Instant::now();
    let mut handles = Vec::with_capacity(cfg.tasks as usize);
    for _ in 0..cfg.tasks {
        let task = runtime
            .spawn(
                LispValue {
                    bytes: b"(add2 40)".to_vec(),
                },
                TaskOptions::default(),
            )
            .expect("task should enqueue");
        handles.push(task);
    }

    let mut completed = 0u64;
    for handle in handles {
        let value = TaskScheduler::task_await(&runtime, handle, Some(Duration::from_secs(5)))
            .expect("task should complete");
        if value.bytes == b"42" {
            completed += 1;
        }
    }

    let elapsed = start.elapsed();
    let throughput = if elapsed.as_secs_f64() > 0.0 {
        cfg.tasks as f64 / elapsed.as_secs_f64()
    } else {
        f64::INFINITY
    };

    println!(
        "elisp task pipeline: completed={}/{} elapsed={:?} throughput={:.2} tasks/s",
        completed, cfg.tasks, elapsed, throughput
    );
    println!("runtime stats: {:?}", runtime.stats());

    runtime.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }
}
