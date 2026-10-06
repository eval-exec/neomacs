# Nonblocking GUI diagnostics

The final GUI frontend routes tracing stdout through a lossy bounded queue:
256 records, at most 64 KiB per record, plus one in-flight record. Oversized or
full-queue records are rejected whole and counted; the worker reports the
cumulative loss on a subsequent successful write and once more when it stops,
unless the sink has already failed. Guard drop requests a bounded
best-effort drain without joining or writing to the blocked sink. Process exit
may discard accepted diagnostics. This is not a durable delivery receipt.

`RUST_LOG` filtering, including INFO, and additive `NEOMACS_LOG_FILE` output
remain available. Build/bootstrap synchronous stdout and final TTY/batch file
routing remain unchanged. Lisp/data output is never sent through this queue.
Formatting allocations and the existing optional-file queue have separate
resource bounds. Third-party direct stderr writes (including Mesa) are not
covered; joining stderr to a saturated stdout can still block those writers.

Focused CPU tests, from the repository root:

```sh
cargo nextest run --locked -p neovm-core --lib --test-threads 1 -E 'test(logging::)'
cargo check --locked -p neomacs
```

The Linux unread-PTY regression fills a real private PTY before launching fresh
subscriber processes, with and without optional file output. Producer and guard
must both return while no master reader drains it. Other tests deterministically
hold a sink behind a channel and check queue limits, whole-record rejection,
sink failure, counter saturation and nonwaiting optional-file guard shutdown.
