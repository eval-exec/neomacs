# Point-dependent geometry after PR #616

PR #616 deferred two geometry corrections: point-dependent window chrome and
absolute line-number gutters with a different current-line face. Both affect
`posn-at-point` / `pos-visible-in-window-p` before redisplay.

## GNU evidence

Local reference: `/home/exec/Projects/github.com/emacs-mirror/emacs`.

- `src/keyboard.c`, `Fposn_at_point`, delegates visibility to
  `Fpos_visible_in_window_p`.
- `src/xdisp.c`, `pos_visible_p`, recomputes mode/header/tab-line heights before
  starting the body iterator. A private-display probe growing the header from
  height 1.0 to 4.0 after moving point returned `(67 t nil)`: the same last-row
  target became invisible without redisplay.
- `src/xdisp.c`, `try_cursor_movement`, rejects the shortcut when line-number
  faces differ, including absolute numbering. A private GNU probe with a 2.0
  current-line face changed position coordinates from `(68, 38)` to `(32, 20)`
  after moving point.
- `src/window.c`, `window_point`, reads the selected window's live buffer point
  and uses the stored marker for other windows. This remains true when the
  current buffer differs from the selected window's buffer.
- `pos_visible_p` restores the saved chrome heights after its temporary walk;
  the observation must not acknowledge a new redisplay partition.

GNU documents that `:eval` can produce propertized chrome strings in
[Properties in Mode](https://www.gnu.org/software/emacs/manual/html_node/elisp/Properties-in-Mode.html).
The format value's identity alone therefore cannot prove that its measured
height remains valid after point changes.

## Reuse boundary

The canonical freshness token owns the permission to ignore point, rather than
leaving each consumer to normalize point independently. A closed enum records
whether native inputs certify independence or require measurement. Executable,
symbolic, conditional, propertized and dynamically substituted chrome formats
need the existing targeted `Position` query. Literal property-free chrome
retains the shortcut. This is conservative: proving additional formats safe
belongs at the same boundary, with geometry evidence.

The existing query adapter remains responsible for evaluation, row production,
clipping and visibility. The token classifier invokes no Lisp and does not add
a second chrome evaluation or a second partial position-query implementation.


Absolute numbering is deliberately not treated as a metric proof. Active line
numbering uses canonical measurement until the producer can certify the
current-line face's effective geometry, including inheritance and remapping.
This trades a broader fast path for correct queries; default and buffer-local
line-number settings are resolved by the canonical native variable lookup.
The old caller-specific numbering predicate is removed.

## Live pipeline corrections

The first real GUI regression failed despite the synthetic adapter tests:
moving point left the freshness token's selected-window marker unchanged.
Admission therefore accepted an exact match before checking point independence.
The token now projects the selected frame/window's live buffer point without
mutating the marker; nonselected windows retain their own stored points.

Synchronizing that marker in a diagnostic probe fixed the line-number case but
still left all three chrome cases wrong. The canonical query adapter treated
position visibility like `window-end` and preserved the accepted chrome heights.
The publication policy now distinguishes those operations with enum variants:
`window-end` preserves its accepted partition, while an initial position query
measures live chrome using the existing renderer. Neither query publishes
start/end markers or replaces the accepted redisplay snapshot.

When the measured chrome heights require a physical retry, a one-use enum phase
passes only native dimensions to the immediately following attempt. It avoids
evaluating observable `:eval` forms twice. Logical, source or face retries must
measure again. No Lisp value is retained by this phase.

A viewport query cannot certify a later position query while chrome exists:
it may have preserved obsolete heights under otherwise current input tokens.
The chrome-free viewport shortcut remains available. Position requests with
active chrome are not memoized, including when a final height retry emits no
chrome strings, because arbitrary scalar Lisp reads are not collection-read
certificates.

Temporary chrome evaluation also writes native thread-local generation,
fingerprint, evaluation-count and column-consumption records. Thread-confined
RAII scopes isolate and restore those records so an unpublished query cannot
alter the enclosing redisplay's bookkeeping.

## Regression coverage

Core tests cover selected live point versus stored markers, dynamic chrome and
absolute current-line number metrics. Real layout-engine tests cover a preceding
`window-end` observation, one evaluation per physical height convergence,
remeasurement on later requests and restoration of enclosing chrome records.
The source-horizon fixture explicitly disables chrome to continue testing only
bounded body acquisition and prefix-cache coverage.

The GUI regression runs GNU and Neomacs on private displays and moves point
without redisplay. Each mode/header/tab-line case checks that a taller live row
hides the old last body target. The current-line-number case checks that moving
the tall current-line face lowers the target's Y coordinate. Font-dependent
absolute pixel coordinates need not match between the two executables.
