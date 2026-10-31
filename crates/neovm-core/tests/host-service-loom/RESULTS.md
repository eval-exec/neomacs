# Execution status

Frozen model run on Linux x86_64 with Rust 1.96.1: **14/14 passed** (four
positive cases and all ten named negative controls). Executed under the
campaign heavy-build admission, in nix develop, pinned to CPUs 0-3,14-31.

```sh
cargo nextest run \
  --manifest-path crates/neovm-core/tests/host-service-loom/Cargo.toml \
  --test protocol --test-threads 1 --retries 0
```

The builder uses four actors, two preemptions and 256 branches per execution;
there is no permutation or duration truncation. The single outstanding-call
model checks the service-only reservation interval. It is an abstract protocol
proof, not a proof of production RootBatch registration, heap retention, actual
channel atomics, panic/fail-stop behavior, hosted activation or host wakeups.
The adjacent production-source transport harness separately exercises the
actual crossbeam FIFO and OS actors, including under ThreadSanitizer.

The new cut-1 `wake` target is **NOT RUN; parent integration pending**. It adds
four positive histories and four named mutation controls. The existing **14/14**
receipt above applies only to `protocol`; it is not a receipt for `wake`.

Pending command:

```sh
cargo nextest run \
  --manifest-path crates/neovm-core/tests/host-service-loom/Cargo.toml \
  --test wake --test-threads 1 --retries 0
```

The wake builder uses at most four threads, two preemptions and 256 branches
per execution with no duration/permutation truncation. Its separate history
checker covers final durable recheck, independent destination epochs, exclusive
polling lease and session retirement. This abstract model does not import
production std atomics, execute a native wait, or prove production socket,
condvar, TSan, census or hosted-admission behavior.
