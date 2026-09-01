# Org journal overlay redisplay validation

Date: 2026-09-01

Status: the buffer-wide absent-`mouse-face` traversal is fixed and closed at
the algorithmic/regression-test level. A repeat of the original private Org
journal workload remains pending because its buffer, configuration helper, and
`perf.data` capture are no longer present on this machine.

## Original failure

The reported buffer had 4,269 overlays. Of those, 4,268 were empty
`git-gutter` overlays carrying `before-string`, and none carried `mouse-face`.
Repeated cursor movement nevertheless asked for the exact extent of an absent
`mouse-face` property and swept unrelated overlay endpoints across the buffer.

The original Linux `perf` capture attributed 95.88% of performance-core cycles
to the evaluator thread. Its leading overlay/index symbols included:

| Symbol | Self cycles |
|---|---:|
| `OverlayList::previous_boundary_before_since_emacs_byte_pos` | 19.77% |
| `slice::partition_point` | 12.68% |
| `OrderedShiftTree::record` | 9.24% |
| `EndpointBPlusTree::values_at` | 6.12% |
| `OverlayData::current_range` | 6.09% |
| `OverlayList::next_boundary_after_until_emacs_byte_pos` | 3.27% |

The root cause was an abstraction mismatch: the display cursor needed a cheap
property-at-position answer, but called an API that also proved the property's
maximal extent. Proving absence forced a whole-buffer boundary sweep.

## Fix under test

Commit `580ff8481` separates absent and present property answers, bounds a
negative answer by the next already-known display-property boundary, and uses
a monotonic property-aware sweep only for a positive overlay winner. Generic
overlay B+ summaries use conservative property-key signatures to skip subtrees
that cannot contain the requested property while retaining GNU-compatible
alias, category, window, and identity semantics.

The regression suite checks both correctness and work performed:

- an absent `mouse-face` result does not traverse unrelated overlay endpoints;
- a positive winner advances monotonically rather than restarting its search;
- aliases, categories, window filtering, overlay priority, and GNU identity
  behavior remain intact;
- property summaries are conservative under mutation and collision.

Before this follow-up, the focused suites passed 29 mouse-face tests and 48
overlay tests. The complete `neovm-core` suite passed 9,573 tests.

## Post-fix synthetic profile

The post-fix build was produced with:

```sh
cargo xtask fresh-build --profile profiling
```

The controlled fixture ran under a private Xvfb/X11 session. It contained
4,268 lines and 4,268 zero-length overlays, each with `before-string` and
`git-gutter`, and no `mouse-face`. Point was placed at line 3,201. Each
iteration moved one line in each direction and synchronously requested a full
`neomacs--frame-snapshot`.

The 12.527-second evaluator capture contained 6,150 samples and lost none. The
old hot path disappeared:

| Evaluator symbol | Original | Post-fix synthetic |
|---|---:|---:|
| `previous_boundary_before_since_emacs_byte_pos` | 19.77% core | absent |
| `EndpointBPlusTree::values_at` | 6.12% core | absent |
| `slice::partition_point` | 12.68% core | 0.11% core / 0.12% atom |

The new leading overlay work was `OverlayIndex::attach_batch` at 13.18% of
performance-core and 10.53% of atom-core samples. That is expected from this
fixture's explicit full-frame snapshot loop: it repeatedly rebuilds and
attaches layout snapshot overlay state. It is a separate cost from the removed
absent-property boundary traversal and should be investigated independently if
normal interactive redisplay profiles show the same weight.

## Timing control

Overlay and no-overlay buffers were alternated in the same private Neomacs
process. For five 30-round pairs, the medians were:

| Fixture | Median wall time | Median process CPU time |
|---|---:|---:|
| No overlays | 1.317 s | 3.941 s |
| 4,268 unrelated overlays | 1.929 s | 4.708 s |

The overlay fixture's median gap was about 46.5% wall time and 19.5% process
CPU time. These are diagnostic numbers, not a product benchmark or a claimed
speedup: Xvfb used llvmpipe, the forced snapshot loop includes asynchronous
renderer and full snapshot/index construction, individual pairs were noisy,
and some pairs reversed ordering. The symbol profile is the reliable result:
the target buffer-wide `mouse-face` traversal is no longer present.

## Missing end-to-end replay inputs

The original report named these inputs, but none exists now:

```text
/home/exec/Documents/journal/2026.org
/home/exec/.config/emacs/elisp/neomacs-perf.el
/home/exec/.cache/neomacs/perf/neomacs-20260901-105645.perf.data
```

A filesystem search found no replacement copy. Reconstructing a different Org
file or configuration would not be a faithful before/after validation, so no
real-workload latency claim is made here.

To finish end-to-end validation when the inputs are available, use the same
profiling build, open the original journal with its original configuration,
confirm approximately 4,268 `git-gutter` overlays and zero `mouse-face`
overlays, then record the same duration and cursor movement. Acceptance is:

- interactive movement no longer stalls;
- the evaluator is no longer dominated by overlay endpoint traversal;
- the old boundary/index symbol family remains absent or negligible;
- any remaining cost is attributed separately, especially snapshot rebuild,
  Org fontification, garbage collection, or software rendering.

## Conclusion

The diagnosed complexity defect is fixed: absence now scales to the next
display boundary instead of the total overlay population. The synthetic
profile directly falsifies the old hotspot, and regression tests preserve the
algorithmic guarantee. The original Org-journal report should be considered
closed for that defect, with a clearly recorded limitation that private
end-to-end replay has not yet been repeated.
