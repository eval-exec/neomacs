use neovm_core::TaskScheduler;
use neovm_core::emacs_core::print_value;
use neovm_core::emacs_core::symbol::Obarray;
use neovm_core::emacs_core::value_reader::read_all;
use neovm_host_abi::{LispValue, Signal, TaskError, TaskOptions};
use neovm_worker::{WorkerConfig, WorkerRuntime};
use std::fs;
use std::io::{self, Write};
use std::time::Duration;

const CASE_PREFIX: &str = "__NEOVM_CASE__\t";

fn render_task_error(err: TaskError) -> String {
    match err {
        TaskError::Cancelled => "ERR (task-cancelled nil)".to_string(),
        TaskError::TimedOut => "ERR (task-timeout nil)".to_string(),
        TaskError::Failed(signal) => format!("ERR {}", render_signal(signal)),
        // Not this case's result: the case asked the process to exit, and this
        // runner is the process. Reported for the transcript, then obeyed by
        // the caller (see the shutdown_request check in the case loop).
        TaskError::Shutdown(order) => format!("ERR (kill-emacs {})", order.exit_code),
    }
}

fn render_signal(signal: Signal) -> String {
    let payload = signal.data.unwrap_or_else(|| "nil".to_string());
    format!("({} {})", signal.symbol, payload)
}

fn escape_case_bytes(input: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(input.len());
    for (idx, byte) in input.iter().copied().enumerate() {
        match byte {
            b'\n' => escaped.extend_from_slice(br"\\n"),
            b'\r' => escaped.extend_from_slice(br"\\r"),
            b'\t' => escaped.extend_from_slice(br"\\t"),
            // NeoVM printers encode control escapes in strings as single-slash
            // sequences (\n/\r/\t). Oracle emits raw controls and then escapes
            // them at case emission time, so expand single-slash sequences here.
            b'\\'
                if idx + 1 < input.len()
                    && matches!(input[idx + 1], b'n' | b'r' | b't')
                    && (idx == 0 || input[idx - 1] != b'\\') =>
            {
                escaped.extend_from_slice(br"\\")
            }
            other => escaped.push(other),
        }
    }
    escaped
}

fn build_status_line(index: usize, rendered_form: &[u8], status: &[u8]) -> Vec<u8> {
    let mut line = Vec::with_capacity(
        CASE_PREFIX.len()
            + 24
            + rendered_form.len().saturating_mul(2)
            + status.len().saturating_mul(2)
            + 3,
    );
    line.extend_from_slice(CASE_PREFIX.as_bytes());
    line.extend_from_slice((index + 1).to_string().as_bytes());
    line.push(b'\t');
    line.extend_from_slice(&escape_case_bytes(rendered_form));
    line.push(b'\t');
    line.extend_from_slice(&escape_case_bytes(status));
    line.push(b'\n');
    line
}

fn write_status_line(index: usize, rendered_form: &[u8], status: &[u8]) {
    let mut out = io::stdout().lock();
    let line = build_status_line(index, rendered_form, status);
    out.write_all(&line)
        .expect("failed writing vm-compat case line");
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

    // Create the worker runtime first, which creates the Context and sets
    // the main thread's interner. This must happen BEFORE parse_forms so that
    // SymIds are interned into the Context's interner and remain valid.
    let threads = 1usize;
    let rt = WorkerRuntime::with_elisp_executor(WorkerConfig {
        threads,
        queue_capacity: 1024,
    });
    let workers = rt.start_dummy_workers();

    let parse_obarray = Obarray::new();
    let forms = match read_all(&source, &parse_obarray) {
        Ok(forms) => forms,
        Err(err) => {
            eprintln!("failed to parse forms: {}", err.message);
            std::process::exit(2);
        }
    };

    for (index, form) in forms.iter().enumerate() {
        let rendered_form = print_value(form);
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
                let status = format!("ERR {}", render_signal(signal));
                write_status_line(index, rendered_form.as_bytes(), status.as_bytes());
                continue;
            }
        };

        let result = TaskScheduler::task_await(&rt, task, Some(Duration::from_secs(1)));
        match result {
            Ok(value) => {
                let mut status = b"OK ".to_vec();
                status.extend_from_slice(&value.bytes);
                write_status_line(index, rendered_form.as_bytes(), &status);
            }
            Err(err) => {
                let status = render_task_error(err);
                write_status_line(index, rendered_form.as_bytes(), status.as_bytes());
            }
        }

        // GNU exits when a Lisp thread calls kill-emacs, and so does this
        // host: remaining cases are not run.
        if let Some(order) = rt.shutdown_request() {
            let _ = io::stdout().flush();
            rt.close();
            for worker in workers {
                worker.join().expect("worker thread should join");
            }
            std::process::exit(order.exit_code);
        }
    }

    rt.close();
    for worker in workers {
        worker.join().expect("worker thread should join");
    }
}

#[cfg(test)]
mod tests {
    use super::{CASE_PREFIX, build_status_line, escape_case_bytes};

    #[test]
    fn escape_case_bytes_escapes_literal_control_bytes() {
        assert_eq!(escape_case_bytes(b"\"a\tb\""), b"\"a\\\\tb\"");
        assert_eq!(escape_case_bytes(b"\"a\nb\""), b"\"a\\\\nb\"");
        assert_eq!(escape_case_bytes(b"\"a\rb\""), b"\"a\\\\rb\"");
    }

    #[test]
    fn escape_case_bytes_expands_single_slash_control_sequences() {
        assert_eq!(escape_case_bytes(b"\"a\\tb\""), b"\"a\\\\tb\"");
        assert_eq!(escape_case_bytes(b"\"a\\nb\""), b"\"a\\\\nb\"");
        assert_eq!(escape_case_bytes(b"\"a\\rb\""), b"\"a\\\\rb\"");
    }

    #[test]
    fn escape_case_bytes_keeps_existing_double_slash_sequences() {
        assert_eq!(escape_case_bytes(b"\"a\\\\tb\""), b"\"a\\\\tb\"");
        assert_eq!(escape_case_bytes(b"\"a\\\\nb\""), b"\"a\\\\nb\"");
        assert_eq!(escape_case_bytes(b"\"a\\\\rb\""), b"\"a\\\\rb\"");
    }

    #[test]
    fn build_status_line_preserves_non_utf8_status_payload_bytes() {
        let rendered_form = b"(string 2097152)";
        let status = [b'O', b'K', b' ', b'"', 0xF8, 0x88, 0x80, 0x80, 0x80, b'"'];

        let mut expected = Vec::new();
        expected.extend_from_slice(CASE_PREFIX.as_bytes());
        expected.extend_from_slice(b"1");
        expected.push(b'\t');
        expected.extend_from_slice(rendered_form);
        expected.push(b'\t');
        expected.extend_from_slice(&status);
        expected.push(b'\n');

        assert_eq!(build_status_line(0, rendered_form, &status), expected);
    }
}
