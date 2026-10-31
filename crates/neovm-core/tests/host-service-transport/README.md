# Instrumented real host-service transport

This isolated crate imports the **actual production** `host_service/transport.rs`
with `#[path]`. Its existing unit tests are imported with it. It adds two tests
with distinct publisher, service, cancellation and closure OS threads; the
in-flight case also has a native-completion thread. The sole service actor
consumes completion and terminal-acknowledgement receipts. Synchronization uses
barriers and channels, never sleeps, retries or timing-based admission.

The first new case dispatches exactly once, then races cancellation and close
while native work commits. Both control sends complete before the service
actor settles the native result. Either FIFO control order is accepted and
verified; the native effect occurs once. The second case orders cancellation
and close before service admission and verifies that native work is suppressed.
Notification consumption precedes the service actor's metadata acknowledgement.
Receive timeouts are safety bounds, not ordering witnesses.

The only shims are the correlation-key and owned-native-payload vocabulary.
The key shim is a nonzero identity, not the production session/generation
registration authority. The production channel, FIFO, state transitions,
dispatch ticket, cancellation, waiter Drop, completion and acknowledgement
implementations are not copied or replaced.

Run the ordinary tests inside heavy-slot admission:

```sh
CARGO_TARGET_DIR=/path/to/lane-private-target/host-service-transport \
  cargo nextest run \
  --manifest-path crates/neovm-core/tests/host-service-transport/Cargo.toml \
  --test channel --test-threads 1 --retries 0
```

For TSan, invoke `run-tsan.sh` inside the same admitted/pinned environment, with
`CARGO_TARGET_DIR` naming the lane-private target. Optionally supply an exact
already-installed nightly toolchain name. The runner selects an installed
nightly, requires its existing `rust-src`, and instruments std, the harness and
crossbeam with `-Zsanitizer=thread`. It never installs a compiler or component.
Missing prerequisites exit 77 with `TSAN_UNAVAILABLE`; that is **not** a passing
race check. A successful instrumented run ends with
`TSAN_REAL_CHANNEL_PASS` and prints the source hash and compiler identity.
Caller admission and the job wrapper supply the overall execution deadline.

There is no evaluator, heap, scheduler, RootBatch or host object here. The
transport's receipt is explicitly not permission to retire real Lisp roots;
there are none in this harness. A passing run covers instrumented channel
histories only, not scheduler/GC correctness, GNU semantics, UI wakeups,
non-Linux platforms or toolkit race-freedom. No sanitizer result is claimed
until the coordinator actually executes the runner.
