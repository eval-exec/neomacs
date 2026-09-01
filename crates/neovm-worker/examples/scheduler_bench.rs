use neovm_host_abi::{LispValue, TaskOptions};
use neovm_worker::{WorkerConfig, WorkerRuntime};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
struct BenchConfig {
    tasks: u64,
    channel_ops: u64,
    threads: usize,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            tasks: 200_000,
            channel_ops: 200_000,
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
        } else if let Some(value) = arg.strip_prefix("--channel-ops=") {
            if let Ok(parsed) = value.parse::<u64>() {
                cfg.channel_ops = parsed.max(1);
            }
        } else if let Some(value) = arg.strip_prefix("--threads=") {
            if let Ok(parsed) = value.parse::<usize>() {
                cfg.threads = parsed.max(1);
            }
        }
    }

    cfg
}

fn wait_for_completed(rt: &WorkerRuntime, expected: u64, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let stats = rt.stats();
        if stats.completed + stats.cancelled >= expected {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::yield_now();
    }
}

fn main() {
    let cfg = parse_config();

    println!(
        "benchmark config: tasks={}, channel_ops={}, threads={}",
        cfg.tasks, cfg.channel_ops, cfg.threads
    );

    let queue_capacity = (cfg.tasks as usize).max(1024);
    let rt = WorkerRuntime::new(WorkerConfig {
        threads: cfg.threads,
        queue_capacity,
    });

    let workers = rt.start_dummy_workers();

    let task_start = Instant::now();
    for _ in 0..cfg.tasks {
        rt.spawn(LispValue::default(), TaskOptions::default())
            .expect("task enqueue should succeed");
    }

    let finished = wait_for_completed(&rt, cfg.tasks, Duration::from_secs(10));
    let task_elapsed = task_start.elapsed();

    let task_ops_per_sec = if task_elapsed.as_secs_f64() > 0.0 {
        cfg.tasks as f64 / task_elapsed.as_secs_f64()
    } else {
        f64::INFINITY
    };

    println!(
        "task pipeline: completed={} elapsed={:?} throughput={:.2} tasks/s",
        finished, task_elapsed, task_ops_per_sec
    );
    println!("task stats: {:?}", rt.stats());

    let channel = rt.make_channel(4096);
    let channel_start = Instant::now();
    for i in 0..cfg.channel_ops {
        rt.channel_send(
            channel,
            LispValue {
                bytes: vec![(i as u8).wrapping_mul(31)],
            },
            None,
        )
        .expect("channel send should succeed");
        let _ = rt
            .channel_recv(channel, None)
            .expect("channel recv should succeed")
            .expect("channel recv should return value");
    }
    let channel_elapsed = channel_start.elapsed();
    let channel_ops_per_sec = if channel_elapsed.as_secs_f64() > 0.0 {
        cfg.channel_ops as f64 / channel_elapsed.as_secs_f64()
    } else {
        f64::INFINITY
    };

    println!(
        "channel round-trip: elapsed={:?} throughput={:.2} ops/s",
        channel_elapsed, channel_ops_per_sec
    );

    rt.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }
}
