# Renderer effects

Neomacs exposes renderer effects through one name-based Elisp interface.  An
effect has its own typed property set; unrelated effects do not pretend to
share one universal options struct.

```elisp
(neomacs-effect-set 'cursor-glow
                    :enabled t
                    :color "#66CCFF"
                    :radius 48)

(neomacs-effect-get 'cursor-glow)
(neomacs-effect-reset 'cursor-glow)
(neomacs-effect-names)
```

`neomacs-effect-names` accepts an optional discovery scope: `shader` lists
renderer shader configs, `cursor` lists configs valid in a buffer-local cursor
profile, and `behavior` lists cursor/window animation policy.  With no scope it
returns the complete `VisualConfig` registry.

The same interface owns animation behavior that used to be split across
positional setters:

```elisp
(neomacs-effect-set 'cursor-motion
                    :enabled t :speed 2.4
                    :style 'critically-damped-spring
                    :duration 0.15 :trail-size 0.7)
(neomacs-effect-set 'cursor-color-cycle :enabled t :fps 24)
(neomacs-effect-set 'cursor-blink :enabled t :interval 0.5)
(neomacs-effect-set 'cursor-size-transition :enabled t :duration 0.15)
(neomacs-effect-set 'buffer-transition
                    :enabled t :duration 0.2
                    :effect 'slide :easing 'ease-out-quad
                    :axis 'auto :direction 'forward)
(neomacs-effect-set 'scroll-transition
                    :enabled t :duration 0.15
                    :effect 'page-curl :easing 'spring)
```

Unknown effects, unknown properties, invalid colors, wrong value types, and
non-integral values for integer properties signal an Elisp error.  Property
names use Lisp kebab-case and map to the Rust config field with the same name
in snake_case.  Colors use sRGB `#RRGGBB` or `#RRGGBBAA` strings and are
converted to and from the renderer's linear color storage at the protocol
boundary.  Time fields whose names end in `-ms` are milliseconds, while Rust
`Duration` fields are exposed as seconds.  Opacity, saturation/lightness,
cursor trail size, flicker, and normalized thumb radius are constrained to
0–1; dimensions, speeds, and other unsigned quantities reject negative values
before reaching the renderer.  Frame rates are positive integers, so `:fps 0`
is rejected rather than silently coerced by the scheduler.

## Profiles

### Default cursor motion

Cursor motion defaults to the `neovide` style in both Lisp and the renderer:

```elisp
(setopt neomacs-cursor-motion-enabled t
        neomacs-cursor-motion-style 'neovide
        neomacs-cursor-motion-duration 0.06
        neomacs-cursor-motion-trail-size 0.7
        neomacs-cursor-motion-distance-length-adjust t)
```

This follows Neovide's cursor implementation at commit `2f922dd`: each corner
uses exponential ease-out, with a base duration of 60 ms multiplied by the
base-10 logarithm of its initial distance to the destination center in pixels.
Leading corners advance faster than trailing corners, creating a stretching
quadrilateral. Consequently, 60 ms is a base parameter, not a fixed arrival
time. The speed multiplier is unused by this mode. Cursor particle effects
remain disabled by default. Rendering and editor-specific cursor shapes still
use Neomacs's renderer.

Other motion styles remain available: `exponential`, `critically-damped-spring`,
`linear`, `ease-out-quad`, `ease-out-cubic`, `ease-out-expo`, and
`ease-in-out-cubic`. The plain `ease-out-expo` style moves one rectangle;
`neovide` independently animates its corners.

### Applying profiles

`neomacs-effects-apply` replaces the complete profile, starting from the
Rust-defined defaults and applying every entry before publishing anything:

```elisp
(neomacs-effects-apply
 '((cursor-glow :enabled t :color "#66CCFF" :radius 48)
   (rain-effect :enabled t :drop-count 30 :speed 120.0)))
```

If any entry is invalid, neither evaluator state nor renderer state changes.
An empty profile restores every Rust default.  `neomacs-effects`, the Custom
option in `term/neo-win.el`, has the same format.

`neomacs-cursor-effect` accepts one cursor-effect entry or a list of entries
in this format as a buffer-local variable or window parameter.  The layout
engine validates it through the same protocol registry used by global
configuration.

## Ownership and update flow

1. `VisualConfig` in `neomacs-display-protocol` is the typed control-plane
   source of truth.  It contains the focused renderer `EffectsConfig` plus
   cursor and transition policy configs.
2. The evaluator owns the desired `VisualConfig`.  It validates an update on
   a clone and commits only after the display host accepts the new snapshot.
3. `ConfigCommand::SetVisualConfig` sends that complete snapshot to the render
   thread.  The render thread distributes it to its shader, cursor, and
   transition subsystems and marks the display dirty.
4. The renderer owns transient execution state such as particles and elapsed
   animation time; it does not define the user-facing configuration API.

The registry is derived from the serialized shape of `EffectsConfig`, so
adding a normal field to a new effect config does not require another Lisp
setter, positional decoder, command variant, or match table.  Adapters exist
only where a public value deliberately differs from Rust storage: colors,
durations, and the variable-length indent-guide color list.  Such adapters
must have focused round-trip tests.

Effect updates are control-plane operations, not frame-loop operations.  The
small serialization cost buys one source of truth and strict, atomic
validation without affecting rendering performance.

The default cursor color cycle is driven by the centralized frame scheduler,
not an Elisp timer.  Its `:fps` setting defaults to 24 and is capped to the
current display rate.  Frames use compositor-only cursor damage, and color is
derived from elapsed presentation time, so missed or deliberately skipped
ticks do not change the animation speed.  Demand pauses while the window is
unfocused or cursors are blinked off.  Hollow cursor visuals do not contribute
to the frame rate; split windows use the fastest enabled non-hollow cursor.
