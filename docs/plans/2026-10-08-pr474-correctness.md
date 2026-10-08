# PR #474 correctness follow-up

## Reference behavior and reproduction

GNU `src/image.c:11982` loads file and data SVG through the same static
librsvg path, retaining the resource base URI. Its SVG loader does not use
`:index`. GNU `lisp/image.el:934` mutates the displayed image specification;
a nil playback limit stops after displaying the last frame (`:1069`).

This means a computed finite sequence must include its exact terminal
state, while the disabled policy must remain static even with nonzero
`:index`. The oracle batch adapter cannot call GNU raster metadata on a
terminal frame: GNU signals that a window-system frame is required. The
previous inline expectation did not establish successful image loading.

Regression tests reproduced temporal-priority loss, fractional-repeat
truncation, repeated-key-time interpolation stalls, invalid discrete timing,
incorrect composite periods, missing scheduler end events, malformed XML,
whitespace normalization, cache policy contamination, inconsistent concurrent
publication, and retirement accounting left behind by decoder unwinding.
The finite pixel regression compares the last sample with an independently
authored static document, rather than deriving its expectation from the
animation evaluator under test.

## Boundaries in this correction

| Type | Invariant |
| --- | --- |
| `Repeat::Finite(Duration)` | Compiled finite active duration preserves fractional repeats. |
| `SampleTimeline` | Finite endings and proven repeating periods require separate sampling decisions. |
| `SequenceMaterialization` | SVG colors, resources and policy accompany every cache request and entry. |
| `DecodeLease` | Every registered decode releases accounting on success, refusal and unwind. |
| `ImageSequenceIntroduction` | The repeat boundary and both segment delays travel together. |
| `XmlAttributeValue` | Semantic attribute text passes XML escaping exactly once before splicing. |

Compilation retains competing rules; evaluation chooses the most recently
activated contribution and uses document order to break ties, including
frozen contributions. Interpolation skips elapsed zero-width key-time
segments. Discrete key times need not end at one. Finite sampling reserves
one frame inside the 256-frame limit for the exact terminal state. Repeating
plans retain introductions once, then use checked nanosecond LCM after finite
effects have settled. Metadata keeps the repeat boundary and both segment
delays together; the image.el adapter schedules the currently displayed
frame's segment and wraps only the repeating tail. Reverse playback started
in the tail stays there; reverse playback started in the introduction visits
the preceding prefix frames before wrapping to the tail. The default-delay
marker is normalized before speed arithmetic.
Unsupported periods fall back to the static image rather than inventing a
shorter loop.

The sequence cache retains one materialization variant per source under its
existing memory budget. A variant mismatch recomputes; duplicate publications
return the resident winner before selecting pixels and metadata. This keeps
each result coherent without retaining unbounded per-policy variants.

## Long-term design

The eager compatibility adapter now represents finite endings and one-time
introductions, but independent native playback still needs a separate runtime.

Separate immutable asset identity, playback-instance identity, view identity,
and complete sample-materialization identity. An asset can have multiple
playbacks; two views can explicitly share one playback. Keep GNU timer/index
mutation as a compatibility adapter rather than making it the native clock.

A backend-independent temporal layer should compile and evaluate document
time, with a time-dependent scheduling hint for the next discontinuity,
continuous interval and terminal state. Advertise periodicity only when
proven. Render-thread playback should retain introductions, seek and pause
state, and coalesce visible sample requests. Lazy samples and bounded prefetch
avoid storing a whole long loop; byte admission and cancellation leases bound
work independently of the FPS ceiling. Geometry should continue to distinguish
native decoded pixels from realized output so transforms apply once.

XML patching remains a practical adapter here. A typed vector scene would
avoid repeated XML parsing and permit backend-specific rasterization without
coupling animation semantics to WGPU. Additive, motion-path, event-based,
spline and paced semantics still require separate implementation; this fix
does not broaden that supported subset.

## Primary research

- [librsvg supported features](https://gnome.pages.gitlab.gnome.org/librsvg/devel-docs/features.html): SVG animation is unsupported.
- [SVG animation semantics](https://www.w3.org/TR/SVG11/animate.html): timing, key times, repeat counts and freeze/remove behavior.
- [SMIL sandwich model](https://www.w3.org/TR/2001/REC-smil-animation-20010904/#AnimationSandwichModel): activation priority and document-order tie breaking.
- [XML attribute normalization](https://www.w3.org/TR/xml/#AVNormalize): quote/entity escaping and character-reference whitespace preservation.
- [SVG namespaces](https://www.w3.org/TR/SVG/struct.html) and [SVG media type](https://www.w3.org/TR/SVG/mimereg.html): namespaced SVG and gzip representation.
- [Rust Drop](https://doc.rust-lang.org/std/ops/trait.Drop.html): owned cleanup during unwinding.

## Validation

Commands and red/green logs are retained under `tmp/` in the fix worktree.
Rust tests use `cargo nextest` as requested.

- Final protocol and renderer unit suite: **1,645 / 1,645 passed**
  (`tmp/verification/final-renderer-protocol.log`).
- Lisp playback: **7 / 7 ERT behaviors passed** inside one nextest VM test
  (`tmp/verification/lisp-playback-final-green.log`). The animation policy
  parser regression also passed (1 / 1, `tmp/verification/core-animation-parser.log`).
- GNU oracle: **2 / 2 live comparisons passed**
  (`tmp/verification/oracle-final-run.log`). An independent GNU GUI check
  also confirmed static SVG indices 0 and 19 both render at 100 × 100 with
  no animation metadata.
- TUI: **1 / 1 real terminal test passed**
  (`tmp/verification/tui-final-run.log`).
- GUI: **2 / 2 passed** against the final binary and matching pdump, with
  real timers and GPU readback (`tmp/svg-animation-gui-final.log`).

The delayed GUI test first failed with two frames and no introduction boundary;
the fix expects four frames and the actual playback trace `[0,1,2,3,2]`.
The Lisp regressions first failed for forward wrap, displayed-frame delay and
reverse wrap. A seventh regression independently reproduced arithmetic on the
legal default-delay marker.

Test setup provisions existing malformed/truncated PNG fixtures under
`tmp/imgmsg/fixtures`. Runtime integration uses generated bootstrap Lisp and a
fresh fingerprint-matched development pdump so startup fits the suite timeouts.
