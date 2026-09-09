# GNU menu-bar parity: audit and proposed design

Status: implementation in progress, not a completed parity claim.
Date: 2026-09-10.
Neomacs baseline: `11baa121e`, plus two uncommitted red regression tests.
GNU source baseline: `a360712c9d2` in the local emacs-mirror checkout.

## Scope and evidence

The target is GNU-compatible menu semantics and a complete usable presentation,
not copying GTK's pixel styling onto every platform. Menu-bar headings, their
descendant menus, and the shared popup machinery are in scope. Toolbar-only
properties are not automatically menu-bar requirements. Existing native popup
lifetime/placement work remains; there must be no editor-window overlay fallback.

Evidence labels:

- **Reproduced**: a repository test has run and failed on the reported behavior.
- **Source gap**: the actual producer/consumer path omits the behavior; a dedicated
  regression is still required before implementation.
- **Partial**: implementation exists, but does not establish full GNU parity.
- **Verify**: an interaction or platform comparison remains to be performed.

The original two reproductions are in
`crates/neovm-core/src/emacs_core/display/display/tests/`:
`menu_buttons_test.rs` and `menu_submenu_test.rs`. Both now pass. Test
sources belong in the repository, never `/tmp`. Logs may use `/tmp`.

No claim of an interactive GTK-versus-Neomacs visual reproduction is made from
these tests. They reproduce the missing information in menu snapshots. Linux
Wayland is the only available native verification environment.

## Audit ledger

The findings in this table describe the baseline. The implementation progress
below records which portions have since been covered; a green slice does not
close all requirements of a ledger row.

| ID | GNU capability | Neomacs finding | Evidence / completion requirement |
| --- | --- | --- | --- |
| M01 | Toggle and radio selection | Reproduced: checked and unchecked items publish identical data | `popup_menu_item_from_binding` omits `:button`; both menu entry types lack indicator state; painter lacks indicators. Cover computed state, off/on, radio, disabled-but-checked, and reopening after command execution. |
| M02 | Symbolic submenu keymaps | Reproduced: `Text Properties` becomes `submenu: false` | `facemenu.el` supplies `(menu-item "Text Properties" facemenu-menu)`. GNU resolves the function cell; Neomacs checks only a literal keymap list. Also cover aliases and keymap autoloads, not just this symbol. |
| M03 | Shortcut equivalents | Source gap; user reports missing File-menu shortcuts | Keymap entries always receive `String::new()` for `shortcut`. Cover automatic lookup, `:keys` strings/functions and substitution, verified `:key-sequence`, `menu-alias`, remapping/context, and rebinding. |
| M04 | Dynamic item names | Source gap | Parser accepts runtime strings, not label expressions. Easy Menu dynamic labels/suffixes must pass through the same evaluator semantics. |
| M05 | `:visible` | Source gap | Popup parser does not evaluate visibility or exclude hidden entries/subtrees. Test before evaluating later properties, including invisible separators. |
| M06 | `:enable` and legacy `menu-enable` | Source gap | Popup enablement is merely `!command.is_nil()`. Preserve disabled display/help while preventing activation; top-level disabled-heading handling must follow GNU. |
| M07 | `:filter` | Source gap in popup presentation | Key lookup has filter support, but popup item construction does not use it. Cover filters returning a command, submenu, or nil and the filtered binding's eventual dispatch. |
| M08 | Separators and nonselectable labels | Partial | Painter can draw separators, but keymap parser always publishes `separator: false`. Parse GNU separator syntax, retain label-only rows, support spacer versus line semantics, and document toolkit-equivalent rendering of specialized styles. |
| M09 | Parent/composed keymaps and duplicate precedence | Source gap / further reproduction needed | Popup traversal uses the nonrecursive keymap iterator, which ignores embedded/parent submaps. Canonical traversal must preserve precedence and suppress duplicates rather than concatenate blindly. |
| M10 | Active-map merging and heading/content agreement | Partial | Heading collector implements ordering, suppression and final items, but separately from popup resolution. Its local resolver and structural label extraction are incomplete. `neomacs-menu-bar-open` uses a global-first fallback chain; verify local/minor-mode contributions and commands as headings. |
| M11 | Dynamic menu update lifecycle | Source gap in inspected path | Definitions for `menu-bar-update-hook` exist, but no invocation was found in the inspected native/redisplay flow. GNU runs activation/update hooks with context/restoration guards. Test Buffers-menu freshness and rebuild ordering; do not run hooks on every hover. |
| M12 | Frame/buffer context and safe Lisp evaluation | Partial | Heading invalidation/context work exists. Popup property evaluation needs owning-frame selected-buffer semantics, dynamic binding, match-data policy, GC roots, error handling, quit propagation, and restoration after callbacks mutate editor state. |
| M13 | Menu keymap prompts, panes and legacy popup forms | Partial | Batch decoding accepts more forms than the interactive branch, which only enters for a literal keymap; title is set to `None`. Shared machinery must handle supported keymap/list/pane forms and their distinct selection contracts. Do not confuse batch argument acceptance with GUI rendering support. |
| M14 | Revision-scoped selection and dispatch | Partial | Session/revision validation exists. Native selection indexes are returned without the selectability check used by keyboard selection. Validate item membership and kind/availability at the evaluator seam; preserve GNU event paths, prefix argument and command context. |
| M15 | Native surfaces and nested lifetime | Partial, existing Wayland verification | Keep compositor-relative placement, parent lifetime, post-event commits, stale-response rejection and release ownership. Do not replace these while fixing semantic data. Stress map changes, rapid switches and dismissals. |
| M16 | Keyboard interaction | Partial | Arrows, Enter, Home/End, Escape, C-g and heading switching exist. Character navigation, Space, paging and toolkit-specific access-key behavior need a GTK behavior comparison; unsupported cases must not be silently swallowed. Avoid inventing Windows-style mnemonic conventions for GTK. |
| M17 | Pointer interaction and long menus | Partial / Verify | Hover-open, release activation, wheel scrolling and selection reveal exist. Opening delay, diagonal submenu travel, edge autoscroll, touch/pen activation and width constraints need explicit tests. Winit unified pointer events alone do not prove touch/pen parity. |
| M18 | Text measurement and columns | Source gap | Runtime uses UTF-8 byte lengths times a fixed advance, while painter places characters individually. Use shared shaped runs and measured indicator/label/shortcut/arrow columns; test proportional text, combining marks, CJK, RTL and fractional scaling. |
| M19 | Menu appearance | Partial | Popup painter has default-derived colors; host sends no explicit foreground/background and uses default face binding. Resolve menu appearance deliberately, including disabled/selected states, theme changes and contrast. GTK styling parity must be defined per platform, not assumed. |
| M20 | Narrow-window heading reachability | Source gap / behavior verification | Heading layout stops at the first item that does not fit. Define/test overflow or platform-equivalent access so all headings remain reachable. Preserve `menu-bar-final-items` semantics separately from visual placement. |
| M21 | Accessibility | Source gap in menu path | No menu accessibility model/adapter found. Expose menu roles, names, checked state, disabled state, submenu expansion, focus and actions from the same semantic snapshot. Rendering visible text alone is insufficient. |
| M22 | Platform conventions and availability | Incomplete / unverified | Wayland popup path exists. The pinned X11 backend rejects popup creation; Android/WASM adapters are explicitly unsupported; macOS/Windows are unverified. A macOS application/global-menu adapter is distinct from an in-frame heading row. No unavailable platform may be marked verified from a Wayland test. |

### Primary source anchors

GNU:

- `src/keyboard.c:parse_menu_item`, `menu_item_eval_property`, `menu_bar_item`,
  `menu_bar_items`: interpretation, evaluation order, shortcuts and merging.
- `src/menu.c:single_menu_item`, `keymap_panes`, `digest_single_submenu`,
  `x_popup_menu_1`: semantic menu representation, pane/selection contracts.
- `src/gtkutil.c:make_menu_item` and button creation around
  line 3269: toolkit check/radio state and menu shell presentation.
- `src/xdisp.c:update_menu_bar` around line 14410 and
  `src/pgtkmenu.c` around line 282: selected context and hook lifecycle.
- `doc/lispref/keymaps.texi`: Simple/Extended Menu Items, Separators, Alias
  Menu Items, Mouse Menus, Menu Bar and Easy Menu.
- `lisp/facemenu.el:245-253`, `lisp/menu-bar.el`: real default menu definitions.

Neomacs:

- `crates/neovm-core/src/emacs_core/display/display/mod.rs`:
  `popup_menu_item_from_binding`, `popup_menu_from_keymap`,
  `x_popup_menu_interactive`, `x_popup_menu_interactive_loop`, `builtin_x_popup_menu`.
- `crates/neovm-core/src/emacs_core/runtime/eval/mod.rs:PopupMenuEntry`.
- `crates/neomacs/src/main.rs:show_popup_menu`.
- `crates/neomacs-display-protocol/src/menu.rs:PopupMenuItem`.
- `crates/neomacs-layout-engine/src/tty_menu_bar.rs` and `gui_chrome.rs`.
- `crates/neomacs-display-runtime/src/menus/{session,layout,interaction,controller}.rs`.
- `crates/neomacs-renderer-wgpu/src/renderer/paint/menu.rs`.
- `lisp/term/neo-win.el:neomacs-menu-bar-open`.

## Implementation progress

The evaluator now owns `emacs_core/display/menu/`: parsing, dynamic evaluation,
shortcuts and separator recognition. VM menu entries are the shared protocol
type, so the host no longer manually copies presentation fields. Lisp event
values remain rooted on the evaluator thread through preparation and selection.

The protocol now distinguishes `Command`, `Submenu`, `Label` and `Separator`
with `MenuItemKind`. Only command variants carry `MenuIndicator`; labels and
separators cannot be enabled. Availability and toggle/radio state are enums.
The flat hierarchy is **not yet** the proposed validated immutable tree.

Verified slices at the publication seam (each newly fixed behavior had a failing
test first, with additional real-default-menu integration coverage):

- M01: typed toggle state, evaluated predicates, disabled-but-checked state;
  GPU readback distinguishes all four toggle/radio off/on appearances.
  native visual/reopen coverage remains.
- M02: symbolic submenu resolution and symbolic root maps, including aliases
  to the real autoloaded `kmacro-keymap`. The real Edit menu publishes Text
  Properties as a submenu.
- M03: automatic shortcuts, explicit strings/functions, verified key hints,
  rebinding and alternate-command prefix/suffix forms. The real File menu
  publishes Save's `C-x C-s`. Context/remapping and pixel alignment need more
  coverage.
- M04–M07: dynamic names, visibility ordering, enable predicates, legacy
  `menu-enable`, the user enable override, filters returning submenus, ordinary
  property errors versus quit. This is popup coverage; headings are still a
  separate migration.
- M08: recognized GNU separator names and nonselectable labels; label-only rows
  do not evaluate command-only button predicates. GNU GTK uses the same
  `gtk_separator_menu_item` widget for its recognized separator names.
- M09: inherited items, duplicate precedence, and merging inherited submenu
  children. The latter also exposed `map-keymap` dropping later composed maps;
  its shared traversal now resumes the enclosing spine. Runtime canonicalization
  uses GNU's Lisp `keymap-canonicalize` from `subr.el`.
- M12: dynamic property evaluation with inhibited redisplay and error/quit
  distinction; help key substitution and its inhibition text property.
  GC coverage includes ephemeral labels and a button predicate detached from
  its mutable parent by a later callback. Canonicalization uses the existing
  GNU-compatible `safe_funcall`, whose signaled-condition policy deliberately
  differs from item-property evaluation. Broader frame/context restoration
  remains.
- M14: native results cannot select disabled entries or submenu headers; TTY
  submenu selections retain their distinct behavior. Disabled submenu children
  are not prepared. The modal transaction borrows private entry/event slices
  from their rooted owner; it cannot outlive that owner.
- M16: Right cannot open a disabled submenu after pointer hover.
- M18: panel-wide independent label/shortcut maxima, a shared shortcut right
  edge, and indicator gutter. Character counting replaces byte counting, but
  this is **not** shaped text and does not close M18.

Submenu descent rejects an ancestor cycle and explicitly reports exhaustion of its
64-level/65,536-item resource budget. No partial snapshot is published on those
errors. This is a deliberate defensive policy, not a claim to copy GNU's own
silent ten-level pane cutoff. Tests cover a cycle, a complete 40-level menu,
and exhaustion at 64 levels. The shared composed/parent-keymap iterator retains
its own pre-existing silent limits; universal keymap-cycle/resource reporting
is still open, and is not established by the submenu-descent tests.

The M03 baseline mentions the documented `menu-alias` property. Further source
inspection found a documentation/runtime disagreement: GNU's current
`parse_menu_item` does not consult it, and `src/ChangeLog.11` (2009-09-10)
explicitly records its removal. Do not add speculative legacy-property handling
to claim current GNU parity. Verified `:key-sequence` hints still accept the
command's immediate function alias, as the current C implementation does.

Tests live in the display publication tests, runtime `menus/*_test.rs`, and
renderer `tests/offscreen_frame/menu_test.rs`. Full native verification, heading
merging/update lifecycle, typed tree validation, shaped layout, appearance,
overflow, accessibility and platform rollout remain open. No non-Wayland target
has been verified by this work. `lisp/ldefs-boot.el` is a pre-existing user change
and is not part of this implementation.

### Foundation verification checkpoint

- `cargo check -p neomacs`: passed (existing warnings remain).
- Focused `cargo nextest` core menu/keymap/architecture run: **38 passed**.
- Focused runtime and offscreen menu painter run: **70 passed**. The painter
  tests require a real offscreen GPU adapter; they do not silently skip it.
- Broad affected-package run: **14,969 passed, 10 failed, 62 skipped**. Two
  failures were introduced by this work (a bootstrap fixture and the module's
  initial directory placement); both are fixed and passed the focused rerun.
  The two process tests passed when rerun serially. Six failures remain: three
  default-feature video tests expect unavailable opt-in support, one requires
  absent `loaddefs.elc`/`neomacs-effects.elc`, and two startup tests still fail.
  This is **not a green full-suite result**. No baseline run established the
  provenance of those six failures, and their tests were not weakened.
- Native Linux Wayland: **not verified**. Three isolated Weston 15 headless
  runs (rapid replacement, ordinary smoke, then smoke without `--fake-seat`)
  aborted in the compositor at `weston_coord_global_to_surface` with
  `!view->transform.dirty`. The client subsequently observed a broken pipe.
  No root cause or attribution to Neomacs/winit has been established. These
  runs did not inject input into the user's desktop compositor.
- Independent Standards/Spec re-review found the reported foundation defects
  resolved; the wider parity ledger remains incomplete. See
  [foundation review](menu-bar-foundation-review.md).

No release binary or pdump was rebuilt at this checkpoint. A normal fresh-build
also regenerates tracked Lisp sources even with `--no-byte-compile`; the
pre-existing `ldefs-boot.el` edit must be preserved when arranging that build.

## Proposed long-term design

### 1. One evaluator-owned semantic module

Introduce a menu module in `neovm-core` with a small interface for preparing
menu-bar headings, resolving an opened menu, and interpreting its result.
Its implementation owns GNU property interpretation, keymap resolution,
shortcut lookup, selected context and error/quit policy. Layout must not parse
Lisp menu-item lists or resolve symbol values/function cells independently.

Reuse the existing keymap resolver and `MenuItemProperty`/`MenuButtonKind`
enums. Preserve GNU property ordering and evaluation rules, including dynamic
binding and returning nil for ordinary property errors while allowing quit to
escape. GC-root every Lisp value that survives an evaluation/autoload call.
Use explicit cycle detection and bounded resources, not silent depth-32 truncation
as a substitute for correct keymap traversal.

Prepare semantics at GNU-compatible update/activation points. Arbitrary Lisp
forms cannot be cached correctly by guessing all variables they might read.
Reuse an immutable snapshot during hover/redraw, and rebuild at defined editor
invalidation/activation events. An explicit reason enum should separate these
events from a physical surface repaint. Guard reentrancy and restore context
even if callbacks change the current buffer, delete a frame or signal.

### 2. Typed snapshots, not independent flags

Replace the boolean combination `separator + submenu + enabled` and the
duplicated VM/protocol presentation structs with one owned presentation model.
The evaluator's GC-rooted event/selection table is separate and never travels
to the GUI thread. The item kind describes valid states, conceptually:

```rust
enum MenuEntry {
    Command {
        text: MenuText,
        availability: Availability,
        indicator: MenuIndicator,
        action: MenuActionId,
    },
    Submenu {
        text: MenuText,
        availability: Availability,
        children: MenuId,
    },
    Label(MenuText),
    Separator(SeparatorStyle),
}

enum MenuIndicator {
    None,
    Toggle(CheckState),
    Radio(CheckState),
}

enum CheckState { Off, On }
enum Availability { Enabled, Disabled }
```

Names are proposed; types must earn their use in the implementation. This
model prevents selectable separators and command-only submenus at compile time.
Snapshot/menu/item/action IDs have distinct types and private constructors.
A builder validates child references and ownership, so consumers cannot
construct inconsistent flat depth sequences. Keep one authoritative tree;
do not maintain an independently editable tree and flattened list.

Visibility is resolved by including/omitting entries. Selection, hover and
keyboard focus are not the same state as a toggle being on. User command/event
names remain open-ended Lisp symbols; do not turn them into a closed Rust enum.
Use `strum` for actual closed keyword domains, not arbitrary user labels.

Result decoding must validate snapshot revision, membership, availability and
entry kind. Then the evaluator follows GNU-compatible event/command semantics;
the renderer must not directly execute a command or optimistically change a
Lisp-owned checkbox variable. Legacy value selections and key-sequence
selections need explicit variants rather than accidental coercion.

### 3. One measured presentation for paint, hit testing and accessibility

Measure menu rows using real shaped text. Share measured geometry and glyph runs
between the painter and hit tester. Reserve panel-wide columns for indicator,
label, shortcut and submenu arrow. The right edge of the shortcut column is
common to every row, and the arrow column cannot overlap it.

Paint checks/radio dots/arrows as scalable primitives rather than requiring a
particular font's checkbox glyph. Resolve direction, device scale, clipping,
scrolling and appearance into the measured result. Reuse existing shaping and
surface geometry facilities; do not build another font engine for menus.

Publish accessibility from the semantic snapshot plus measured bounds and
session focus. Native toolkit menu adapters can consume the same semantics
without routing through WGPU painting.

### 4. Keep interaction and native ownership separate

The runtime owns a typed interaction session: focus, open submenu path, pointer
press/release ownership, opening deadlines and scrolling. Feed it explicit input
and time, then return typed effects such as open, close, activate and reveal.
This makes repeated hover idempotence and cancellation independently testable.

The existing presentation module continues to own native windows, compositor
placement and GPU lifetimes. Platform adapters translate the common semantics
into native popup/global-menu/browser presentation as appropriate. Unsupported
capabilities return typed errors; `cfg_select!` selects compiled adapters.
Compile-time selection does not prove runtime compositor capability.

GUI and TUI share semantic interpretation, not physical presentation. Rename
and relocate the currently shared `tty_menu_bar` semantics as consumers migrate;
TTY-specific cell layout/navigation remain TTY-specific.

### 5. File/module layout

Split files only where responsibility warrants it; do not create empty layers.

```text
crates/neovm-core/src/emacs_core/display/menu/
  mod.rs                    evaluator-facing interface and rooted session
  resolve.rs                item interpretation and GNU property evaluation
  keymaps.rs                canonical merging, aliases, autoloads, cycle policy
  shortcuts.rs              equivalent key discovery and explicit hints
  update.rs                 context, hook ordering and invalidation
  tests/                    evaluator → published snapshot / result tests

crates/neomacs-display-protocol/src/menu/
  mod.rs                    public presentation vocabulary
  model.rs                  typed entries, menu tree, validated snapshot builder
  identity.rs               revision-scoped menu/item/action identity
  geometry.rs               measured rows and columns
  tests/                    model invariants and protocol validation

crates/neomacs-display-runtime/src/menus/
  session.rs                navigation and input ownership
  interaction.rs            platform input → semantic input
  layout.rs                 measured row/column preparation
  controller.rs             session effects → native presentation
  *_test.rs                 observable session/layout behavior

crates/neomacs-renderer-wgpu/src/renderer/paint/menu/
  mod.rs                    paint a measured menu panel
  indicators.rs             checkbox/radio/arrow primitives, when warranted
  *_test.rs                 paint-plan or GPU readback assertions

crates/neomacs-gui-tests/
  fixtures/menu-bar.el       real default and constructed menus
  tests/menu_bar.rs          native GUI input, snapshots and visual artifacts

crates/neomacs-tui-tests/tests/menu_bar.rs
                            terminal-only presentation and input
```

Existing regression tests stay in their current repository test directory until
their production module moves, then move with it. Upgrade the submenu diagnostic
from its private helper to the published-snapshot seam during that migration.

## Delivery sequence and exit criteria

This is a multi-slice migration, not permission to declare every ledger row
fixed after adding three painter features. Keep all rows tracked to resolution.

1. **Semantic foundation and visible regressions**: symbolic/aliased/autoloaded
   submenus, typed indicators, dynamic labels/visibility/enablement/filters,
   separators and shortcut equivalents. Each feature is a red → green slice
   through the actual publication path, followed by its presentation slice.
2. **Menu preparation and dispatch**: merge/inheritance correctness, heading
   agreement, hooks/invalidation, selected context, safe callbacks, all supported
   source forms and validated selection. Preserve ordinary Lisp command dispatch.
3. **Presentation completeness**: shaped columns, menu appearance, keyboard and
   pointer comparison, overflow, long menus and accessibility. Measure GTK
   interactions before declaring a particular delay/access-key policy required.
4. **Platform rollout**: native adapters and capability matrix. Linux Wayland
   gets live verification here; the other targets remain explicitly unverified
   until tested in suitable environments. Browser/mobile presentation must obey
   their platform constraints rather than pretend to be a desktop popup.

Approved test seams (confirmed by the user before implementation):

- Lisp definitions and editor context → `DisplayHost` menu snapshots/results.
- Typed snapshots and semantic input → measured rows, paint output and session
  effects, without a compositor where possible.
- Real Neomacs process on Linux Wayland → menu contents, visual indicators,
  shortcut alignment, pointer/keyboard activation and resulting Lisp state.

TUI tests assert only terminal behavior. Use `cargo nextest`, never invoke
`cargo test` directly. Native smoke tests must not substitute for checked-state
or shortcut-pixel assertions. A screenshot alone does not prove command
dispatch; a command result alone does not prove the correct menu was displayed.

For each ledger row, record the failing test, implementation, passing tests,
and any remaining native/manual verification. No source-only observation is
promoted to reproduced without a runnable assertion, and no unavailable
platform is promoted to complete on the strength of conditional compilation.
