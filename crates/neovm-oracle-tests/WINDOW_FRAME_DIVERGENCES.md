# Window & frame management divergences — Neomacs vs GNU Emacs

Oracle parity tests (`divergence_window_frame{,2,3}.rs`) probing window/frame
management under the `--batch` harness.

## Run them

```bash
cargo nextest run -p neovm-oracle-tests -E 'test(/div_wf/)|test(/div_wf2/)|test(/div_wf3/)' --no-fail-fast
```

Authoritative result: **61 tests, 23 pass, 38 divergences.**

## Theme 1 — make-frame is allowed under --batch in Neomacs (36 manifestations)
**Root cause**: Neomacs permits `(make-frame)` under `--batch` (a frame object
is created, `frame-list` grows), whereas GNU Emacs ERRORS (no display). Every
operation that creates then inspects/modifies a frame therefore diverges: GNU
returns `(errored . error)`, Neomacs returns the value.

Covered operations: basic make-frame, make-frame with params
(name/width/height/foreground-color/background-color/minibuffer/vertical-scroll-bars/
menu-bar-lines/cursor-color/internal-border-width/fullscreen), delete-frame,
select-frame/focus-frame/raise-frame/iconify-frame/make-frame-invisible/visible,
frame-root-window/first-window/selected-window/minibuffer-window of new frame,
window-list/set-frame-selected-window/split-window on new frame, frame-visible-p,
modify-frame-parameters, current-frame-configuration, frames-on-display-list,
filtered-frame-list, frame-name sequence (F/F2/F3), multiple make-frame.

## Theme 2 — frame `display-type` differs
`(frame-parameter nil 'display-type)` returns `color` in Neomacs vs `mono` in
GNU under the same tty batch environment. `background-mode` (`dark`) and
`window-system` (`nil`) agree.

## Theme 3 — window-object printed ids differ
Raw window objects print as `#<window N>` where Neomacs uses large/pointer
ids (`#<window 281479271677952>`) and GNU uses small sequential ids
(`#<window 2>`). This is a window-numbering/identity difference (mostly noise
when comparing printed forms; reframed tests compare behaviorally via counts/
eq/edges to avoid it).

## Coverage (passes): window model is otherwise solid
split-window (vertical/horizontal), delete-other-windows, count-windows,
window-list, window-tree, window-edges/inside-edges/pixel-edges, window-total/
body/mode-line size, window-buffer/set-window-buffer, get-buffer-window/
get-lru-window/get-mru-window, walk-windows, window parameters, window-dedicated-p,
window configuration round-trip, save-window-excursion, balance-windows,
window-combined-p, terminal-live-p/terminal-name, frame-width/height, 16 of 18
common frame parameters (all but display-type), frame-basics/first/root-window.

## Files
`divergence_window_frame.rs` (calibration), `divergence_window_frame2.rs`,
`divergence_window_frame3.rs`.
