# Menu foundation review

Baseline: `11baa121e`. Working-tree changes, including new files, were reviewed;
the pre-existing `lisp/ldefs-boot.el` edit was excluded. This is a review of the
implemented foundation, not a declaration that the complete parity ledger is
finished. Both reviews were read-only and independent.

## Standards

1. **GC invariant:** the extracted button predicate must itself be rooted. A
   rooted parent cons can be mutated by a later property callback, detaching
   the saved predicate before collection. The design requires every value
   surviving an evaluation call to be rooted. A mutation-plus-GC publication
   regression was added for this exact case.
2. **Traversal policy:** the inherited silent depth-32 return contradicts the
   approved cycle/resource policy. Cycles and exhausted resources must be
   distinguishable from successful preparation, rather than publish a silently
   incomplete snapshot.
3. **Interface smell (judgment call):** movable public entry/event vectors let
   callers separate the Lisp events from their root guard. The reviewed caller
   retained the owner correctly, but the interface did not express that
   requirement. Private vectors with borrowed views, and a borrowing modal
   transaction, make that lifetime explicit.

The command/submenu/label/separator enum prevents enabled labels and separator
indicators. Strum is used for a closed separator-name domain. Tests are
out-of-line repository sources. The resolver belongs under the existing
`display` domain, as required by the repository's architecture tests.

## Spec

1. **Disabled descendants:** GNU only recursively prepares an enabled submenu.
   Preparing disabled children can run a quit-signaling predicate and publish
   actionable descendants beneath an unavailable ancestor. The parent row
   should remain visible, but its descendants should not be evaluated.
2. **Canonicalization guard:** GNU calls `keymap-canonicalize` through
   `safe_calln`, with redisplay inhibited and signaled conditions muted. Direct
   `funcall` does not provide that contract. This is distinct from menu-item
   properties, where ordinary errors yield nil but quit escapes.
3. **Open work:** the typed immutable tree, heading merging/update hooks,
   selected-context restoration, shaped layout, appearance, overflow,
   accessibility and platform rollout remain incomplete in the parity ledger.
   Silent depth truncation is also specifically prohibited by that design.

No scope creep was identified: fixing composed-map traversal in `map-keymap`
directly supports the inherited-submenu requirement. Native verification was
not claimed; the isolated Weston run aborted in the compositor.

Summary: three Standards findings (worst: unrooted saved predicate); two
implemented-behavior Spec defects (worst: disabled descendants), plus explicitly
open parity requirements. Resolution and verification are recorded in the
main [parity ledger](menu-bar-parity.md).

## Resolution re-review

Standards: all three findings were resolved on inspection. The extracted
predicate is rooted directly; submenu descent signals cycle/resource errors;
private borrowed slices express the rooted owner's lifetime. The mutation/GC
and deep-menu regressions pass. No remaining finding within this limited scope.

Spec: both implemented-behavior defects were resolved on inspection and their
regressions pass. Disabled descendants are skipped and canonicalization uses
`safe_funcall`. One qualification remains: submenu-descent guards do not remove
the shared composed/parent-keymap iterator's existing silent limits. The wider
parity requirements remain explicitly open.

The re-reviews were read-only. Test execution and the non-green broad-suite and
native results are recorded separately in the parity ledger.
