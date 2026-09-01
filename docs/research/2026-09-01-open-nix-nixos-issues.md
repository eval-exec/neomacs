# Open Nix/NixOS issues and the current Neomacs build surface

Research date: 2026-09-01

This note surveys every currently open issue in `eval-exec/neomacs` whose
title, body, or discussion is genuinely about Nix, NixOS, nix-darwin, flakes,
or a build performed inside the Nix development environment. It uses the issue
threads, repository history, current build files, and GitHub Actions runs as
primary sources. Statements labelled **Inference** or **Recommendation** are
conclusions for Neomacs rather than established facts from an issue reporter.

GitHub search also returns
[#109](https://github.com/eval-exec/neomacs/issues/109), but only because its
copied Cargo diagnostics name Rust's `nix` crate. It is a CachyOS/Wayland/wgpu
adapter issue, not a Nix package issue, so it is outside this five-issue set.

## Executive summary

There are five open Nix-related issues, but they are not five current Nix
defects:

| Issue | Original problem | Current assessment |
|---|---|---|
| [#31](https://github.com/eval-exec/neomacs/issues/31) | Flake did not expose Darwin; subsequent Apple Silicon builds found Linux-shaped build and runtime assumptions | Structurally implemented and later repaired, but still needs a current native `aarch64-darwin` build/run confirmation |
| [#60](https://github.com/eval-exec/neomacs/issues/60) | Home Manager launch failed to load the package's original `site-start` | Reproduced on the current Rust package; fixed locally by publishing the standard Emacs package compatibility layout and proved through Home Manager |
| [#65](https://github.com/eval-exec/neomacs/issues/65) | `nix build` had no `packages.x86_64-linux.default` | Fixed and verified by the native installed-package contract |
| [#66](https://github.com/eval-exec/neomacs/issues/66) | A NixOS-built development commit lost its display channel during startup | No supported Nix root cause; stale runtime report, not a packaging design issue |
| [#107](https://github.com/eval-exec/neomacs/issues/107) | Release build was terminated in WSL/Nix | Confirmed memory pressure; `fresh-build --low-memory` now carries a checked one-job budget through every Cargo compilation stage |

The current flake exports default packages, apps, and development shells for
`x86_64-linux`, `aarch64-linux`, `aarch64-darwin`, and `x86_64-darwin`
([current flake systems and outputs](https://github.com/eval-exec/neomacs/blob/f76daf059e594ca59828bcc9bb0af819662a95ee/flake.nix#L39-L41),
[package/app outputs](https://github.com/eval-exec/neomacs/blob/f76daf059e594ca59828bcc9bb0af819662a95ee/flake.nix#L416-L436)).
On this research host, `nix flake show --all-systems --json --offline`
exposed all of those outputs and `nix derivation show .#neomacs` successfully
evaluated the native `x86_64-linux` package. That is output/derivation
validation, not proof that all four packages currently build and start.

The main gap is verification. The Nix workflow is manual-only, runs only on
Ubuntu x86_64, and its most recent successful build/start smoke was
[2026-06-27 at `284429e53`](https://github.com/eval-exec/neomacs/actions/runs/28295567701),
before the August Darwin repair and the current code. Its startup command uses
`--quick`, which disables `site-start` and therefore cannot detect the exact
class of failure reported in #60
([workflow source](https://github.com/eval-exec/neomacs/blob/f76daf059e594ca59828bcc9bb0af819662a95ee/.github/workflows/nix-smoke.yml#L3-L7),
[smoke command](https://github.com/eval-exec/neomacs/blob/f76daf059e594ca59828bcc9bb0af819662a95ee/.github/workflows/nix-smoke.yml#L37-L53)).

The local implementation following this survey closes that verification gap:
flake checks now model output, installed-package, and Home Manager contracts;
CI evaluates every advertised system on relevant pull requests and pushes and
runs the expensive native contracts on a weekly/manual schedule; and both
native startup contracts deliberately retain site initialization. A clean
native build reproduced #60 before the compatibility fix and passed afterward.

## Issue details

### #31 — Flake support for Darwin architecture

**Status:** open; created 2026-02-14; label `os:macos`.

The [original request](https://github.com/eval-exec/neomacs/issues/31) was to
expose `aarch64-darwin` so Neomacs could be installed on nix-darwin-managed
systems. Testing on an Apple M1 then found several distinct blockers:

- configure selected the Nextstep window system and failed because
  `nextstep/templates/Info.plist.in` was absent
  ([report](https://github.com/eval-exec/neomacs/issues/31#issuecomment-3966818069));
- disabling Nextstep exposed a hard EGL requirement in the then-current build
  path
  ([report](https://github.com/eval-exec/neomacs/issues/31#issuecomment-3972980017));
- installing Mesa/MoltenVK made the build succeed, after which winit panicked
  because the macOS event loop was created off the main thread
  ([full Apple M1 report](https://github.com/eval-exec/neomacs/issues/31#issuecomment-3973042810)); and
- a contributor posted a Nix derivation diff that built and launched on ARM
  macOS, although font rendering was blurry
  ([working-package report](https://github.com/eval-exec/neomacs/issues/31#issuecomment-3975727829)).

The original flake-output request was implemented by
[`7f52f19f4`](https://github.com/eval-exec/neomacs/commit/7f52f19f406156d83a6ed9b92c0c0eba23d0af4a).
The macOS main-thread runtime blocker was handled by merged
[#48](https://github.com/eval-exec/neomacs/pull/48). Most recently,
[`1a44856a3`](https://github.com/eval-exec/neomacs/commit/1a44856a302649f21e8e7bc8cf042a243c5cc0a3)
fixed actual `aarch64-darwin` compilation details: BSD `ioctl` request types,
Linux-only `libotf`, and a Nix-sandbox-compatible signing shim.

**Inference:** the source-level work requested by #31 now exists, but the open
issue still records no successful build/run of the current flake after the
August repair. Flake evaluation alone is insufficient because the older
thread already showed failures after both configure and compilation had
advanced.

### #60 — missing `site-start` under Home Manager

**Status:** open; created 2026-03-12; label `bug`; no comments.

The [report](https://github.com/eval-exec/neomacs/issues/60) used
`programs.emacs.package = pkgs.neomacs` on NixOS x86_64. Neomacs 0.0.1, then
the GNU Emacs C backend, tried to load an absent absolute path under
`$out/share/emacs/site-lisp/site-start` and exited. Removing user
configuration did not help; `-Q` and `--no-site-file` did; and
`nix run github:eval-exec/neomacs` worked.

Those observations localize the symptom to the packaged site's startup layout
or its Home Manager wrapper integration. The thread contains no
maintainer-confirmed root cause. It also does not identify where that
`pkgs.neomacs` attribute came from, while the repository flake invocation is
explicitly reported as working.

The package implementation has since been replaced. The current Rust package
installs Lisp and data under `$out/share/neomacs`, installs its portable dump,
and wraps the executable with `NEOMACS_RUNTIME_ROOT`
([current installation layout](https://github.com/eval-exec/neomacs/blob/f76daf059e594ca59828bcc9bb0af819662a95ee/flake.nix#L188-L211)).
That rewrite did not, however, make the original report obsolete.

The new Home Manager fixture reproduced the same failure on current `main`.
Nixpkgs' `emacsPackagesFor` generates a composed `site-start.el` which
unconditionally loads the wrapped editor's original
`$out/share/emacs/site-lisp/site-start`; Neomacs published only its namespaced
`share/neomacs` runtime. It initially also lacked the standard share roots
that Nixpkgs's wrapper links (`applications`, `icons`, `info`, and `man`).

**Conclusion:** the root cause is a missing Emacs-package compatibility layout,
not Neomacs runtime discovery. The local fix retains `share/neomacs` as the
runtime namespace while adding the standard no-op original `site-start.el`,
`subdirs.el`, documentation roots, and canonical Linux desktop assets. Both
the direct installed package and Home Manager's `finalPackage` now start in a
clean home without `--quick`, `-Q`, or `--no-site-file`.

### #65 — `nix build` had no default package output

**Status:** open; created 2026-03-16; label `bug`; no comments.

The [report](https://github.com/eval-exec/neomacs/issues/65) is a precise
flake-schema failure: plain `nix build` could find neither
`packages.x86_64-linux.default` nor `defaultPackage.x86_64-linux`. It failed at
evaluation, before building anything.

The next day's
[`298fd302b`](https://github.com/eval-exec/neomacs/commit/298fd302b9c5a776a405c26087c72e1def0d4357)
added a buildable package and default app/package outputs. Current `main`
still exports `packages.<system>.default` and `packages.<system>.neomacs` for
all four supported systems. Local flake evaluation confirmed that exact
surface.

**Conclusion:** this issue is fixed in source and appears to have remained open
only because the issue was never revisited. A current native `nix build` is the
last evidence needed before a friendly close.

### #66 — successful Nix build, disconnected display at runtime

**Status:** open; created 2026-03-16; label `bug`.

At development commit `0ef80cc9`, the
[#66 report](https://github.com/eval-exec/neomacs/issues/66) built successfully
on NixOS but failed during startup with `failed to update primary window
title: sending on a disconnected channel`, received `quit`, and shut down.
There are no reproduction steps, hardware/display details, or derivation
details. The maintainer replied that the in-development Rust backend was not
ready and directed users to v0.0.2
([response](https://github.com/eval-exec/neomacs/issues/66#issuecomment-4072067766));
no root-cause investigation followed.

**Conclusion:** the log proves a display-channel lifecycle failure at that
commit, not that Nix caused it. The display/runtime architecture has changed
substantially since March. This should be reproduced on current `main` as a
normal runtime issue and closed as stale if it no longer occurs; it should not
block the Nix package contract work.

### #107 — release build terminated in WSL/Nix

**Status:** open; created 2026-05-08; label `bug`.

The [original report](https://github.com/eval-exec/neomacs/issues/107) followed
the Nix quick start in WSL. `cargo xtask fresh-build --release` was terminated
near the final Rust build/link stage. The reporter had 8 GiB RAM plus 2 GiB
swap and confirmed that the build consumed it
([memory report](https://github.com/eval-exec/neomacs/issues/107#issuecomment-4411664506)).
They then succeeded with one Cargo job, disabled release debug data, and one
codegen unit, observing a 2–3 GiB peak
([successful workaround](https://github.com/eval-exec/neomacs/issues/107#issuecomment-4411729847)).
An independent non-Nix, non-WSL reporter also succeeded with the same settings,
so this is a source-build resource problem rather than a Nix-specific compile
failure.

Current `Cargo.toml` already has `debug = false` and `codegen-units = 1`, while
retaining full LTO
([release profile](https://github.com/eval-exec/neomacs/blob/f76daf059e594ca59828bcc9bb0af819662a95ee/Cargo.toml#L310-L314)).
The normal build retains Cargo's machine-sized parallelism, while the new
`--low-memory` option selects one job and `--jobs N` exposes an explicit
positive budget. The later no-window report in the thread was correctly
separated into [#109](https://github.com/eval-exec/neomacs/issues/109):
on Haswell/Wayland, wgpu rejected Vulkan and could not present with GL
([maintainer triage](https://github.com/eval-exec/neomacs/issues/107#issuecomment-4413602307)).

**Conclusion:** #107 has a confirmed root cause and a repository-owned
mitigation. A non-zero job-count type prevents invalid budgets, and xtask
propagates the selected budget to both the main executable and dynamic video
adapter builds rather than asking users to mutate global
`~/.cargo/config.toml`.

## Relationships and current build-design gaps

The issues form four groups rather than one chain:

1. **Flake public contract:** #65 asks for the default package output; #31 asks
   for the same contract on Darwin. Current source exposes both.
2. **Installed runtime contract:** #60 asks whether a packaged executable,
   Lisp tree, site startup, dump, and Home Manager wrapper agree on their
   layout. The installed-package and Home Manager checks now test that complete
   path without startup-bypassing switches.
3. **Build resource contract:** #107 asks how much concurrency and memory an
   ordinary source build may consume.
4. **Unrelated runtime lifecycle:** #66 happened on NixOS but contains no
   evidence of a Nix cause.

The implementation makes the production capability profile the deep seam
between these concerns. `Cargo.toml` owns a versioned, per-platform serialized
policy. Rust decodes its closed feature and backend sets into enums before
planning an xtask build. Nix validates the same schema, selects matching Cargo
features and native dependency closures, publishes the chosen policy on the
package, and asserts it in flake checks. Linux production now explicitly means
`video` plus the dynamic GStreamer adapter and no `webview`; the development
shell continues to expose every optional Linux capability.

## Remaining work outside this local change

1. Run and launch the package natively on Apple Silicon before closing #31;
   Linux cannot turn a successful Darwin derivation evaluation into a runtime
   proof.
2. Reproduce #66, if desired, as a current display-runtime issue. Its historical
   log still does not establish a Nix packaging cause.
3. After review and publication, invite the #60 and #107 reporters to retest the
   documented package and low-memory paths, and close #65 with the native build
   evidence.
4. Measure `--low-memory` under a representative 8 GiB WSL memory limit. Its
   command construction and one-job invariant are tested locally, while an
   environment-matched peak-RSS measurement still requires that environment.
