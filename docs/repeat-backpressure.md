# Native repeat backpressure

Native key repeats are admitted per window and physical key only after the
previous delivery was read or cancelled by the evaluator. The read boundary is
separate from command completion: a command may still be waiting in `read-char`
or accumulating a prefix. This bounds unread repeat debt without consuming the
scroll-preview completion ledger. Ordinary physical presses, release/repress,
other keys and immediate C-g remain lossless; release/focus loss/window teardown
retire the corresponding repeat authority.

The policy applies to events which the native windowing backend classifies as
repeats. An input-method producer which synthesizes fresh release/press pairs
must preserve repeat provenance itself; this patch does not guess from timing
or coalesce real presses.

Focused CPU tests, from the repository root:

```sh
cargo nextest run --locked -p neomacs-display-protocol --lib --test-threads 1 -E 'test(input_progress::repeat_tests)'
cargo nextest run --locked -p neomacs-display-runtime --lib --test-threads 1 -E 'test(key_repeat::tests)'
cargo nextest run --locked -p neomacs-display-runtime --lib --test-threads 1 -E 'test(thread_comm::tests)'
cargo nextest run --locked -p neovm-core --lib --test-threads 1 -E 'test(keyboard::tests::repeat_backpressure)'
cargo nextest run --locked -p neomacs --bin neomacs --test-threads 1 -E 'test(input_bridge::tests)'
```

No display/GPU is required for these selections. They cover slow-reader
backpressure, cancellation, re-press identity, focus/window retirement, nested
input wrappers and immediate quit without consuming observational receipts.
