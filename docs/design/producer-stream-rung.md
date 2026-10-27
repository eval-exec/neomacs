# Producer stream rung — outstanding acceptance criteria

The producer inversion replaces the renderer's split feeders with a producer
that emits typed elements directly. Until it lands, production still walks
`BufferSourceConsumedItem`, and the element vocabulary is reached only by the
stream-equivalence harness
(`crates/neomacs-layout-engine/src/buffer_source/producer/tests/stream_harness_test.rs`),
which proves the two legs of a walk agree.

The harness corpus contains cases for every rung. Most pin today's contract.
Two — C6 (the combining-extender element class) and C7 (producer-side invisible
elision) — describe behavior that exists only after the rung moves that
decision out of the renderer. They are **acceptance criteria for that rung,
not tests of current behavior**: an assertion the current architecture cannot
satisfy is not a test, and a feature gate around it is still a test that fails
by design. The exact assertions are recorded here instead.

## What is observable today

Both behaviors are already correct on screen. The renderer owns the decisions,
and a strict TTY grid comparison against GNU 31.1 shows the displayed grid is
identical in both editors:

* `crates/neomacs-tui-tests/tests/strict_cluster_invisible.rs::
  strict_combining_cluster_grid_matches_gnu` displays `ae<U+0301>z` in both
  editors and asserts the row is exactly `ae\u{301}z` with the mark attached to
  its base, then compares the full character grid, face partition and cursor.
* `...::strict_invisible_span_grid_matches_gnu` displays `abXXcd` with the
  `invisible` property over `XX` in both editors, asserts a visible row of
  exactly `abcd`, asserts no row contains `XX`, then compares the full grid.

The renderer-side owners today are:

* clusters: `composition.rs` and the extender attach in `glyph_row_writer.rs`;
* invisible text: the checkpoint in `buffer_source/text_source.rs` and
  `buffer_source/walk.rs` (`consume_invisible_checkpoint`), mirroring GNU's
  `xdisp.c handle_invisible_prop`.

The corresponding GNU references for the rung are `xdisp.c
handle_invisible_prop` (an invisible run is skipped before glyph production,
while the current buffer position continues across it) and the display
iterator's char-class handling (`it->method`, composition), where a combining
extender is zero-advance and attaches to the preceding base rather than
starting a run.

## C6 — the combining-extender class is carried on the element

Today's contract is pinned by `c6a_base_and_extender_stay_in_one_stream` and
`c6b_extender_at_a_face_seam_stays_in_one_stream`: the base and its extender
stay in one stream and the extender is its own scan step at
`BufferScanPos::new(2, 2)`, with class `ElementClass::PlainChar`.

The rung must make this pass (exact assertion, moved from the deleted
feature-gated harness case):

```rust
let case = StreamCase::new("C6 classes", "ae\u{301}z\n");
let stream = producer_stream(&case);
// The extender is a zero-advance ComposedExtender and no run ever ends
// between a base and its extender.
assert_ne!(stream[2].class, ElementClass::PlainChar);
```

## C7 — the producer elides invisible text

Today's contract is pinned by
`c7a_invisible_text_is_not_elided_by_the_producer_today`: the producer streams
the hidden characters and the split feeder stays transparent over them; the
renderer's invisible checkpoint elides them.

The rung must make this pass (exact assertions, moved from the deleted
feature-gated harness case):

```rust
let case =
    StreamCase::new("C7a invisible mid-line", "abXXcd\n").with(CaseProperty::invisible(2, 4));
let stream = producer_stream(&case);
assert_eq!(CharObservation::chars(&stream), "abcd\n");
assert_eq!(stream[2].scan_before.charpos(), 4);
```

## When the rung lands

Move each assertion back into `stream_harness_test.rs` as an ordinary test
(no feature gate), replacing the today's-contract counterpart if the two
cannot both hold, and delete this section's entry. Keep the pair tests in
`strict_cluster_invisible.rs`: they are the user-visible contract either way.
