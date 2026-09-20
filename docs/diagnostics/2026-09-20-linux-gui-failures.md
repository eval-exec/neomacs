# Linux GUI failure investigation

The full GUI rerun after `8d7c76786b` reported 94 passes and 25 failures.
Five failures were missing `wtype`/`grim` on the invoking shell's PATH; adding
their existing Nix-store directories made those tests pass. The remaining 20
failures had four causes, not 20 independent runtime defects.

## Font catalog: 15 failures

The active Fontconfig catalog did not include the installed Ubuntu Classic
package. `fc-match 'Ubuntu Mono'` returned Noto Sans. GNU Emacs explicitly
reported `Font not available`; Neomacs reported the requested family but used
fallback metrics of 10×24 instead of Ubuntu Mono's 9×18. Assertions derived
from the initial cell size then failed in startup, fullscreen, and live-font
tests.

GNU `src/ftfont.c:ftfont_list` enumerates the configured Fontconfig catalog,
and `src/xfaces.c` reports `Font not available` when the requested face cannot
be loaded. The package's presence in `/nix/store` alone does not register it
with Fontconfig.

An isolated `tmp/gui-investigation/fonts.conf` includes `/etc/fonts/fonts.conf`
and the installed Ubuntu Classic font directory. No system font configuration
was changed. The startup test failed before this environment correction and
passed afterward. With this catalog, 35/36 desktop-font tests passed; the only
remaining failure was the slow compositor presentation deadline below.

## Presentation fixture deadline: 1 failure

`hidpi_8k_slow_desktop_fullscreen_after_confirmed_presentation` used one
12-second deadline for startup, resize, fullscreen, and restoration combined.
The protocol trace showed `unset_fullscreen`, the correct restored configure,
and the restored window size. The deadline expired before the final confirmed
presentation on the software compositor's 8K patterned desktop.

GNU's `src/pgtkterm.c:set_fullscreen_state` submits fullscreen state changes
to GTK; this is asynchronous native-window behavior. A requested resize does
not itself prove presentation.

The fixture now resets its deadline only when a confirmed receipt advances
the stage. Each stage still requires a newer submission, scale 2, correct
geometry, and a compositor timestamp. A separate 60-second harness timeout
bounds the four 12-second stages plus startup/teardown. The previously failing
case passed in 13.7 seconds, including the restored 844×689 presentation.

## GNU weight scoring: 2 failures

The full Noto font oracle and its focused semi-light case selected Regular
instead of GNU's Light. Both editors enumerated the variable Noto font; this
was not another missing-family problem.

GNU `src/font.c:font_score` compares weight-table numeric values and caps
each style distance at 127. CSS uses 300/350/400 for Light/SemiLight/Regular,
creating an apparent tie. GNU uses 50/55/80, so Light is closer. Neomacs was
subtracting CSS weights and allowing native discovery order to choose Regular.

The shared selection function now requires `FontWeight` enum values rather
than untyped weight integers. GNU conversion and the distance cap happen at
that boundary. Both the resolver and the legacy Fontconfig scoring path use
it. Native discovery order still breaks actual equal-score ties.

The regression exercises the public font resolver against a controlled native
candidate catalog: semi-light prefers Light, Medium prefers Regular over
SemiBold, and saturated distances retain discovery order. It failed before
the fix. Afterward, 24 resolver/scoring tests, 193 font tests, and the broader
2,387-test layout suite passed (three existing exclusions in the last run).

## Missing WebView build capability: 2 failures

The ordinary Linux production build enables `video`, not `webview`. Both
xwidget tests reached their semantic glyph assertions but had no browser
pixels; the runtime explicitly logged `this build has no WebView backend;
dropping command`.

Building with `cargo xtask fresh-build --release --features webview` made both
the oversized-page and renderer-device-replacement tests pass. No browser
implementation change or assertion relaxation was needed. The existing Linux
CI runtime command also omits this feature; this investigation does not change
the production capability policy or provision WPE in CI.

## Reproduction and evidence

The first complete rerun with these corrections passed all original 20 failing
tests (`original-failures-audit.json` records the name-by-name comparison).
Its overall result was 118/119 because it exposed an additional capture race.

## Additional capture publication race

The subpixel glyph-coverage fixture intermittently copied a partially encoded
PNG and failed decoding with `UnexpectedEof`. The renderer called `image.save`
on the published path for every readback, truncating that file while the
evaluator's `copy-file` could be reading it. This is a diagnostic-artifact
publication bug, not a glyph-coverage mismatch.

A deterministic regression opens one published capture and then publishes the
next capture before reading the original descriptor. Before the fix that
reader saw the next image; the descriptor did not represent an immutable
snapshot. The renderer now encodes into an owned `NamedTempFile` in the
destination directory and atomically persists it only after encoding finishes.
The temporary-file type cleans up unpublished files on failure. The regression
passes, and the GUI pixel assertions are unchanged.

`readback-red.log` and `readback-green.log` record this red/green sequence.
The display-runtime suite passed 975 tests (five existing exclusions), and
`readback-gui-stress.log` records five successful iterations of both grayscale
and subpixel coverage tests: ten real GUI captures with unchanged assertions.

## Final verification

After `cargo xtask fresh-build --release --features webview`, the final run in
`verified-gui-suite.log` completed in 348.6 seconds: **119 reported passes,
zero failures**. Two existing Doom/Spacemacs checks returned early because
their sealed config fixtures were absent; 117 checks actually executed.
All original 20 failing cases passed, as did both glyph-coverage checks.

The final runtime build is recorded in `atomic-readback-build.log`.
`cargo fmt --all --check` and `git diff --check` also passed. The font catalog
override and tool paths were process-local; system configuration was unchanged.

## Test commands and logs

All local evidence is under `tmp/gui-investigation/`; the original full run is
`tmp/gui-rerun/gui-tests.log`.

- `font-baseline.log` / `font-config-retry.log`: missing-font red/green.
- `font-suite.log`: 35/36 desktop-font checks with the corrected catalog.
- `slow-8k-repeat.log` / `slow-8k-green.log`: deadline red/green.
- `weight-red.log` / `weight-green.log`: resolver red/green.
- `font-unit-suite.log` / `layout-suite.log`: broader regression checks.
- `webview-build.log` / `webview-green.log`: feature-enabled build and two passes.

Use an environment with the tools/fonts listed in
`scripts/ci/setup-linux.sh`'s ecosystem profile, and WPE development/runtime
dependencies for the feature-enabled build. On this machine the Ubuntu package
is already installed but requires the isolated Fontconfig override above.

```sh
TMPDIR="$PWD/tmp" cargo xtask fresh-build --release --features webview
TMPDIR="$PWD/tmp" \
  FONTCONFIG_FILE="$PWD/tmp/gui-investigation/fonts.conf" \
  NEOMACS_GUI_TEST_BACKEND=x11 \
  cargo nextest run -p neomacs-gui-tests --test-threads 1 --no-fail-fast
```
