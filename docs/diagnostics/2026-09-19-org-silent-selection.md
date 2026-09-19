# Issue #379: silent property changes deactivate the region

Report: https://github.com/eval-exec/neomacs/issues/379

## GNU reference

The read-only GNU checkout at `/home/exec/Projects/github.com/emacs-mirror/emacs`
provides the reference:

- `lisp/subr.el`, `with-silent-modifications`, binds
  `inhibit-modification-hooks` and restores the buffer's modified state.
- `src/insdel.c`, `prepare_to_modify_buffer_1`, returns when hooks are
  inhibited **before** setting `deactivate-mark`. Ordinary changes still
  request deactivation even if there are no installed callbacks.
- `src/keyboard.c`, the command loop, consumes that request to deactivate
  the active mark.

A GNU batch probe returns `(nil t)` for `deactivate-mark` after inhibited
and ordinary property changes respectively.

## Reproduction and cause

The new TUI test selects across an Org emphasis line while a post-command
hook changes marker invisibility under `with-silent-modifications`. GNU
retains its region background; the old Neomacs binary loses it. The paired
test fails in approximately 2.5 seconds with `NEO lost region highlighting`.

The GUI test uses the reporter's complete function bodies (only full-line
comments removed), an isolated Sway display and native wtype input. After
`C-SPC C-n C-n`, the old binary fails its `region-active-p` assertion with
`Region deactivated by silent emphasis-marker updates`. This distinguishes
selection loss from a painting-only problem.

Neomacs's modification helper returned early for inhibited hooks, but all
three callers then unconditionally requested mark deactivation. This applied
to insertions, replacements/deletions and property changes alike.

## Ownership and types

`prepare_buffer_change` now owns the entire preparation protocol, including
the inhibition decision and mark-deactivation side effect. The extracted
callback helper retains its callback-free optimization without confusing it
with an inhibited modification. Typed `BufferChangeKind::{Characters,
PropertiesOnly}` keeps parser edits distinct from cosmetic changes; typed
byte ranges continue to enforce the position domain. No Org-specific policy
or renderer workaround is needed.

## Regression boundaries

- Core Lisp calls cover inhibited character/property changes, preserving an
  already-pending deactivation request, and ordinary callback-free changes.
- TUI compares actual selection backgrounds and the complete display with GNU.
- GUI checks the reporter script's active region after native input, then
  checks the rendered red background in a captured frame.

Artifacts and command logs are under `tmp/neomacs-379/`. Early GUI harness
attempts used a nested macro/timer and captured stale pixels, or failed to
connect to a compositor; those are not counted as bug reproductions. The
native-input reproduction is `gui-reporter-native-red.log`.

## Verification

- `cargo xtask fresh-build --release`: passed, including runtime generation.
- Core editfns/text-property/navigation selection: 154 passed.
- New TUI regression plus existing region and Org-block comparisons: 3 passed.
- GUI reporter-script regression: passed; inspected capture shows the first
  two lines red and the final line unselected. Repeated after suppressing the
  fixture's missing-lexical-binding warning: passed.
- Formatting and diff whitespace checks passed.

The existing `org_line_spacing` GUI test is not green: on the rebuilt binary
its timer-driven PNG captures the startup scratch buffer despite an Org
layout snapshot. An older available binary (`4b8f353974`, September 16) also
fails this test, but later, at its assertion that selection changes the
background. This establishes a pre-existing test failure, not identical
failure modes or proof that all presentation behavior is unchanged. This
separate timer/readback problem is not claimed fixed. The new regression
uses native keyboard input and checks actual painted pixels.
