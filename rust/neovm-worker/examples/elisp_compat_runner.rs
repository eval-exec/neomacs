use neovm_core::elisp::{parse_forms, print_expr};
use neovm_core::TaskScheduler;
use neovm_host_abi::{LispValue, Signal, TaskError, TaskOptions};
use neovm_worker::{WorkerConfig, WorkerRuntime};
use std::fs;
use std::io::{self, Write};
use std::time::Duration;

fn render_task_error(err: TaskError) -> String {
    match err {
        TaskError::Cancelled => "ERR (task-cancelled nil)".to_string(),
        TaskError::TimedOut => "ERR (task-timeout nil)".to_string(),
        TaskError::Failed(signal) => format!("ERR {}", render_signal(signal)),
    }
}

fn render_signal(signal: Signal) -> String {
    let payload = signal.data.unwrap_or_else(|| "nil".to_string());
    format!("({} {})", signal.symbol, payload)
}

fn write_status_line(index: usize, rendered_form: &str, status_bytes: &[u8]) {
    let mut out = io::stdout().lock();
    out.write_all((index + 1).to_string().as_bytes())
        .expect("failed writing case index");
    out.write_all(b"\t")
        .expect("failed writing TSV separator");
    out.write_all(rendered_form.as_bytes())
        .expect("failed writing rendered form");
    out.write_all(b"\t")
        .expect("failed writing TSV separator");
    out.write_all(status_bytes)
        .expect("failed writing status payload");
    out.write_all(b"\n")
        .expect("failed writing line terminator");
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: elisp_compat_runner <forms-file>");
        std::process::exit(2);
    };

    let source = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!("failed to read {path}: {err}");
            std::process::exit(2);
        }
    };

    let forms = match parse_forms(&source) {
        Ok(forms) => forms,
        Err(err) => {
            eprintln!("failed to parse forms: {err}");
            std::process::exit(2);
        }
    };

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .max(1);
    let rt = WorkerRuntime::with_elisp_executor(WorkerConfig {
        threads,
        queue_capacity: forms.len().max(64),
    });
    let workers = rt.start_dummy_workers();

    for (index, form) in forms.iter().enumerate() {
        let rendered_form = print_expr(form);
        let task = match rt.spawn(
            LispValue {
                bytes: rendered_form.clone().into_bytes(),
            },
            TaskOptions::default(),
        ) {
            Ok(handle) => handle,
            Err(err) => {
                let signal = match err {
                    neovm_worker::EnqueueError::Closed => Signal {
                        symbol: "task-queue-closed".to_string(),
                        data: None,
                    },
                    neovm_worker::EnqueueError::QueueFull => Signal {
                        symbol: "task-queue-full".to_string(),
                        data: None,
                    },
                    neovm_worker::EnqueueError::MainAffinityUnsupported => Signal {
                        symbol: "task-main-affinity-unsupported".to_string(),
                        data: None,
                    },
                };
                write_status_line(
                    index,
                    &rendered_form,
                    format!("ERR {}", render_signal(signal)).as_bytes(),
                );
                continue;
            }
        };

        let result = TaskScheduler::task_await(&rt, task, Some(Duration::from_secs(1)));
        match result {
            Ok(value) => {
                let mut status = b"OK ".to_vec();
                status.extend_from_slice(&value.bytes);
                write_status_line(index, &rendered_form, &status);
            }
            Err(err) => {
                write_status_line(index, &rendered_form, render_task_error(err).as_bytes());
            }
        }
    }

    rt.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }
}
