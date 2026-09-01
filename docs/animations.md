# Animations

Animations run on the GPU render thread independently of Emacs redisplay. The centralized
frame scheduler drives interactive animation up to the display rate while allowing ambient
effects to declare a lower cadence; the cursor color cycle defaults to 24 Hz. Everything is
configurable from Elisp — see [Configuration](#configuration).

## Cursor

**8 particle/visual modes** (Neovide-inspired):

| Mode | Description |
|------|-------------|
| `none` | No animation, instant movement |
| `smooth` | Smooth interpolated movement (default) |
| `railgun` | Particles shoot backward from cursor |
| `torpedo` | Comet-like trail follows cursor |
| `pixiedust` | Sparkly particles scatter around cursor |
| `sonicboom` | Shockwave ring expands from cursor |
| `ripple` | Concentric rings emanate outward |
| `wireframe` | Animated outline glow |

**7 movement styles** controlling how the cursor interpolates between positions:

| Style | Description |
|-------|-------------|
| `exponential` | Smooth deceleration, no fixed duration (uses speed param) |
| `spring` | Critically-damped spring, Neovide-like feel (default) |
| `ease-out-quad` | Gentle deceleration curve |
| `ease-out-cubic` | Stronger deceleration curve |
| `ease-out-expo` | Sharp deceleration curve |
| `ease-in-out-cubic` | Smooth S-curve |
| `linear` | Constant speed |

The spring style also supports a **4-corner trail effect** where leading corners snap
ahead and trailing corners stretch behind, controlled by a `trail-size` parameter (0.0-1.0).

## Buffer Switch

Buffer switches use the shared transition catalog described below. Effect,
orientation, and direction are independent typed settings:

- `crossfade` and `scale-zoom` have no direction.
- `slide`, `parallax`, and `card-flip` support either axis. `auto` chooses the
  horizontal axis for buffer switches.
- `page-curl` resolves the selected axis and direction to a concrete edge.
- Effects whose geometry is intrinsically vertical or horizontal ignore an
  incompatible axis instead of receiving an invalid renderer state.

`forward` moves outgoing content left or up; `backward` reverses that motion.

## Scroll

**21 scroll animation effects** organized into categories:

| # | Effect | Category | Description |
|---|--------|----------|-------------|
| 0 | `slide` | 2D | Content slides in scroll direction (default) |
| 1 | `crossfade` | 2D | Alpha blend between old and new positions |
| 2 | `scale-zoom` | 2D | Destination zooms from 95% to 100% |
| 3 | `fade-edges` | 2D | Lines fade at viewport edges |
| 4 | `cascade` | 2D | Lines drop in with stagger delay |
| 5 | `parallax` | 2D | Layers scroll at different speeds |
| 6 | `tilt` | 3D | Subtle 3D perspective tilt |
| 7 | `page-curl` | 3D | Page turning effect |
| 8 | `card-flip` | 3D | Card flips around X-axis |
| 9 | `cylinder-roll` | 3D | Content wraps around cylinder |
| 10 | `wobbly` | Deformation | Jelly-like deformation |
| 11 | `wave` | Deformation | Sine-wave distortion |
| 12 | `per-line-spring` | Deformation | Each line springs independently |
| 13 | `liquid` | Deformation | Noise-based fluid distortion |
| 14 | `motion-blur` | Post-process | Vertical blur during scroll |
| 15 | `chromatic-aberration` | Post-process | RGB channel separation |
| 16 | `ghost-trails` | Post-process | Semi-transparent afterimages |
| 17 | `color-temperature` | Post-process | Warm/cool tint by direction |
| 18 | `crt-scanlines` | Post-process | Retro scanline overlay |
| 19 | `depth-of-field` | Post-process | Center sharp, edges dim |
| 20 | `typewriter-reveal` | Creative | Lines appear left-to-right |

**5 scroll easing functions:**

| # | Easing | Description |
|---|--------|-------------|
| 0 | `ease-out-quad` | Standard deceleration (default) |
| 1 | `ease-out-cubic` | Stronger deceleration |
| 2 | `spring` | Critically damped spring with overshoot |
| 3 | `linear` | Constant speed |
| 4 | `ease-in-out-cubic` | Smooth S-curve |

## Configuration

```elisp
;; One typed, name-based profile configures effects and animation policy.
(neomacs-effects-apply
 '((cursor-motion :enabled t :speed 15.0
                  :style critically-damped-spring :duration 0.15
                  :trail-size 0.7)
   (buffer-transition :enabled t :duration 0.2
                      :effect slide :easing ease-out-quad
                      :axis auto :direction forward)
   (scroll-transition :enabled t :duration 0.15
                      :effect page-curl :easing spring)))

;; Incremental changes use the same names and properties.
(neomacs-effect-set 'cursor-motion
                    :speed 20.0 :style 'linear :duration 0.1)
(neomacs-effect-set 'buffer-transition
                    :effect 'page-curl
                    :axis 'vertical :direction 'backward)
(neomacs-effect-set 'scroll-transition
                    :effect 'wobbly :easing 'ease-out-quad)
```
