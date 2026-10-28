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
