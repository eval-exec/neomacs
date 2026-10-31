# Instrumented production host wake owner

**Status: NOT RUN.** This harness and runner have been prepared; no ordinary
or TSan test pass is claimed.

This isolated workspace imports the actual production `src/host_effect/wake.rs`
with `#[path]`, including all of its existing `cfg(test)` unit and native
controls. Only the `ReadDeadline` DTO is shimmed. The epoch publication,
registration, Condvar fallback, exclusive native poll lease, native wake socket,
session identity and failure/closure implementation are the production source.
Its tests use explicit wait-registration observations and channels, with no
sleep-based readiness assumptions. Receive timeouts bound broken tests; they
are not wakeup witnesses.

`polling` and `static_assertions` match the root lockfile exactly. The standalone
lockfile retains the root versions and checksums for their dependency closure.
The runner pins SHA256 hashes for the production owner and both imported test
files. A source change requires review and an explicit pin update; the runner
fails before execution if a pin differs.

Run ordinary tests only within the lane's build/test admission:

```sh
CARGO_TARGET_DIR=/path/to/lane-private-target/host-wake-transport \
  cargo nextest run --locked --offline \
  --manifest-path crates/neovm-core/tests/host-wake-transport/Cargo.toml \
  --test wake --test-threads 1 --retries 0 --no-tests fail
```

For TSan, invoke the runner within the admitted pinned environment:

```sh
CARGO_TARGET_DIR=/path/to/lane-private-target \
  crates/neovm-core/tests/host-wake-transport/run-tsan.sh
```

The runner requires the already-installed `nightly-2026-07-03` toolchain,
its existing `rust-src`, and `cargo-nextest`. It never installs a compiler or
component and runs Cargo offline. It rebuilds std with `-Z build-std` and
instruments std, the real wake source, `polling` and the harness with
`-Zsanitizer=thread`. Its private build directory is the supplied target root
plus `/host-wake-transport-tsan`. Test concurrency is one and retries are zero.
The admission wrapper supplies the overall execution deadline.

Missing prerequisites exit 77 with `TSAN_UNAVAILABLE`; a source mismatch fails
with `TSAN_SOURCE_MISMATCH`. Neither is a passing sanitizer receipt. Only a
successful instrumented run prints `TSAN_REAL_WAKE_PASS`, together with the
checked source hashes and compiler identity.

This covers instrumented wake-owner histories, including kernel wake and named
negative controls. It contains no evaluator, scheduler, process manager, input
publisher, heap, or host object. It does not establish their integration,
runtime migration, GNU semantics, GUI/TUI behavior, hosted admission, or race
freedom on platforms where the harness has not actually run.
