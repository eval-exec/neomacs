# Frame transparency

`alpha-background` changes frame background paint without fading foreground
text. `alpha` controls the complete frame picture, with active/inactive values,
`frame-alpha-lower-limit`, focus redirection and GNU's numeric-then-nil behavior.
For example:

```elisp
(modify-frame-parameters nil '((alpha-background . 70)))
(modify-frame-parameters nil '((alpha . (90 60))))
;; Restore opaque background paint; nil alpha retains the applied frame opacity.
(modify-frame-parameters nil '((alpha-background . nil) (alpha . nil)))
```

The compositor keeps completed pictures premultiplied. Background alpha applies
to background layers, and frame/lifecycle opacity applies once to the completed
picture. Complementary transition pictures add their weighted colors and alpha;
ordered effects such as PageCurl still use source-over. Where a transition's
pictures leave part of its region uncovered, the frame background shows there
at its own alpha. Source-over effects that fade a picture partway (Cascade,
CylinderRoll, TypewriterReveal) still show that picture's covered pixels at
its partial weight. Native output converts
at the final boundary, including pane motion and uncovered child fill.

Fractional child and pane pictures need scratch textures. Before mandatory
admission, the renderer retires owners that can no longer contribute and updates
its external-texture census. If required pictures do not fit the existing budget,
the frame retries without presenting or advancing interaction state. These tests
cover bounded refusal/recovery, resize and a recreated renderer, not an exhaustive
OS/device-loss recovery contract.

## Focused tests

Use the build prerequisites in [building.md](../building.md), or the repository's
`nix develop` shell. The bootstrap-dependent CPU cases also need generated Lisp.
On a clean source checkout, prepare the early charsets and compile-only autoloads
with the repository's GNU generators (the files are ignored by Git):

```sh
awk -f admin/charsets/cp51932.awk < etc/charsets/CP932-2BYTE.map \
  > lisp/international/cp51932.el
gzip -dc admin/charsets/glibc/EUC-JP-MS.gz \
  | awk -f admin/charsets/eucjp-ms.awk > lisp/international/eucjp-ms.el
emacs --batch -Q -l lisp/emacs-lisp/loaddefs-gen.el \
  --eval '(loaddefs-generate "lisp/emacs-lisp" "lisp/transparency-loaddefs.el")'
```

The last command uses GNU Emacs to run the checked-in autoload generator; it is
fixture preparation, not a GNU compatibility oracle. It generates
`lisp/emacs-lisp/cl-loaddefs.el`, which runtime bootstrap cleanup reads even when
no bytecode build is requested. Its partial default output has a separate ignored
filename, so it does not replace the complete `loaddefs.el` or the normal
`ldefs-boot.el` fallback. The normal core Cargo build generates
`charscript.el` and `emoji-zwj.el`. A completed `cargo xtask fresh-build --release`
already supplies these prerequisites; do not repeat that full build just for
the focused tests.

Install the nextest version declared in `.config/nextest.toml`. The selection in
`transparency-tests.txt` lists each package and exact test name. It includes
baseline controls as well as new regressions; it is not the full display or
evaluator suite.

From the repository root, run the CPU selection:

```sh
filter=$(python3 -c 'from pathlib import Path; rows=[s.split() for s in Path("docs/testing/transparency-tests.txt").read_text().splitlines()]; print(" | ".join(f"(package(={p}) & test(={n}))" for k,p,n in rows if k == "cpu"))')
cargo nextest run --locked \
  -p neovm-core -p neomacs-display-protocol -p neomacs-display-runtime \
  -p neomacs-layout-engine -p neomacs-renderer-wgpu \
  -E "$filter" --test-threads 1 --no-fail-fast
```

For the GPU selection, replace `k == "cpu"` with `k == "gpu"`. A working
wgpu adapter is required. On Linux, a software Vulkan implementation can exercise
the production shaders and render-pass dispatch without a desktop compositor:

```sh
export WGPU_BACKEND=vulkan
export LP_NUM_THREADS=2
# If several Vulkan drivers are installed, set VK_DRIVER_FILES to the desired ICD.
```

CPU coverage includes accepted opacity operations independent of redisplay,
focus/minibuffer projection, idle repaint and glyph snapshot materialization.
GPU coverage includes opaque foreground/stipple, colored fractional backgrounds,
child lifetime/resize, pane placement, retained-owner admission, native output
conversion and transition dispatch. FadeEdges checks both directions at five
progress values for fractional and opaque pictures, edge attenuation and every
pixel outside the scissor. PageCurl is the ordered-occlusion control.

The existing display-stack CI selection includes the four display packages;
core cases belong to the existing core suite. Adapter availability, hosted CI,
native window compositors and non-Linux platforms need separate verification.
