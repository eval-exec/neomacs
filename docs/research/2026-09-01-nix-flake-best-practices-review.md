# Neomacs Nix flake best-practices review

Date: 2026-09-01

Scope: the current local Nix refactor: [`flake.nix`](../../flake.nix), `flake.lock`, and all files under [`nix/`](../../nix/).
This review compares the implementation with official Nix, Nixpkgs, Home Manager, Crane, and flake-parts documentation and with several established projects' current source trees.

## Verdict

The current refactor follows the recognized Nix flake schema and modern project organization well.
It is now a strong flake rather than merely a working one: the public entrypoint is 59 lines, substantial policies live behind explicit function/module boundaries, the lock graph has no accidental Rust-overlay Nixpkgs revision, `nix fmt` is supported, the formatter is enforced as a check, and the public overlay actually exposes `pkgs.neomacs`.

There is no official “flake standard” that requires `flake-parts`, `flake-utils`, or a specific directory layout.
The standardized/recognized part is the input/output shape and the value types under names such as `packages`, `apps`, `checks`, `devShells`, `formatter`, and `overlays` ([Nix flake-check schema](https://nix.dev/manual/nix/2.35/command-ref/new-cli/nix3-flake-check.html), [nix.dev flakes](https://nix.dev/concepts/flakes.html)).
Neomacs conforms to that surface.

The remaining issues are mostly policy and interface hardening rather than structural defects.
The highest-priority one is the imminent end of `x86_64-darwin` support in rolling Nixpkgs.

## What is strong now

- **Thin composition root.** `flake.nix` declares inputs, systems, focused modules, and additive cache settings. Package construction, dependencies, development shell behavior, capability validation, and checks are separate.
- **Correct discoverable outputs.** All four advertised systems expose `packages.<system>.default`, `apps.<system>.default`, `devShells.<system>.default`, `formatter.<system>`, and derivations under `checks.<system>`. The plural/default names are the current schema; no deprecated `defaultPackage`, `defaultApp`, or singular `devShell` outputs remain ([Nix flake-check schema](https://nix.dev/manual/nix/2.35/command-ref/new-cli/nix3-flake-check.html)).
- **Input coherence with a documented exception.** `rust-overlay` and Home Manager follow the root `nixpkgs`. The lock graph now contains two Nixpkgs nodes: the project revision and the deliberately independent WPE cache revision. rust-overlay's own example recommends the follows relationship now used here ([rust-overlay README](https://github.com/oxalica/rust-overlay/blob/6a84c41e705533dcc5569ac0731f18406b4cdf86/README.md#nix-flakes)). Home Manager documents both the deduplication benefit and compatibility tradeoff of following the root Nixpkgs input ([Home Manager flake guide](https://nix-community.github.io/home-manager/nix-flakes.html)).
- **Reusable package boundary.** `nix/package.nix` explicitly accepts `pkgs`, `crane`, the Rust toolchain, the platform WPE package, `source`, and `version`; it is not coupled to flake-parts or to hidden package-set attributes. This preserves ordinary Nix-function reuse and makes the overlay/package paths share one constructor.
- **Useful, scoped public overlay.** `overlays.default` composes rust-overlay with the Neomacs overlay and exposes the project toolchain, `pkgs.neomacs`, and the pinned WPE package as `pkgs.neomacs-wpewebkit`. The project-specific WPE name preserves cache reuse without replacing an unrelated consumer's generic `pkgs.wpewebkit`.
- **Explicit Nixpkgs import.** The per-system import supplies both `config = { };` and the project overlay, keeping evaluation independent of ambient Nixpkgs configuration as recommended by official Nix guidance ([nix.dev reproducible Nixpkgs configuration](https://nix.dev/guides/best-practices.html#reproducible-nixpkgs-configuration)).
- **Official formatting interface.** `formatter.<system> = pkgs.nixfmt-tree` makes `nix fmt` work, and `checks.nix-format` enforces official `nixfmt`. `nix fmt` is defined to run a flake's formatter output, and nixfmt is the official Nix language formatter ([Nix formatter command](https://nix.dev/manual/nix/2.34/command-ref/new-cli/nix3-formatter-run.html), [nix.dev formatter FAQ](https://nix.dev/guides/faq.html#how-to-format-nix-language-code-automatically)).
- **Cross-build discipline.** The package enables `strictDeps` and keeps native tools separate from host libraries. Nixpkgs documents `strictDeps` as the mechanism that catches incorrect dependency placement and improves cross-compilation behavior ([Nixpkgs dependency section](https://nixos.org/manual/nixpkgs/unstable/#ssec-stdenv-dependencies)).
- **Effective Crane artifact reuse.** `buildDepsOnly` receives the Cargo-filtered source and the final package reuses `cargoArtifacts`. This is Crane's documented pattern for avoiding repeated dependency builds ([Crane quick start](https://crane.dev/examples/quick-start.html), [artifact reuse](https://crane.dev/introduction/artifact-reuse.html)).
- **One production-capability authority.** Nix reads the production capability schema from `Cargo.toml`, rejects unknown schema/features/backends during evaluation, and derives Cargo features and native dependencies from the validated result.
- **Behavioral package checks.** The checks cover flake output contracts, the installed runtime layout, portable-dump fingerprinting, actual batch startup, Home Manager's wrapped `finalPackage`, and formatting. Home Manager officially allows an arbitrary editor package through `programs.emacs.package`, so a Neomacs-specific Home Manager module is not needed yet ([Home Manager Emacs options](https://nix-community.github.io/home-manager/options/home-manager/programs/emacs.html)).
- **Additive cache configuration.** `extra-substituters` and `extra-trusted-public-keys` append rather than replace user configuration. Nix requires trusted signatures for ordinary substituted store paths and prompts before accepting flake configuration by default ([Nix configuration reference](https://nix.dev/manual/nix/2.18/command-ref/conf-file.html#conf-accept-flake-config), [binary-cache signing guide](https://nix.dev/guides/recipes/post-build-hook.html)).

## Remaining recommendations

### P1: decide the Intel macOS contract before Nixpkgs 26.11

The systems list still advertises `x86_64-darwin`, and all-system evaluation emits the current Nixpkgs deprecation warning.
Nixpkgs 26.05 is the last release supporting Intel macOS; 26.11 will no longer support building it from source ([Nixpkgs 26.05 release notes](https://github.com/NixOS/nixpkgs/blob/5aad24e6372ad85e2b27b0c8ef1382bf686deb3c/doc/release-notes/rl-2605.section.md#x86_64-darwin-2605)).

Choose and document one honest policy:

1. remove `x86_64-darwin` from the rolling-unstable flake before it stops evaluating; or
2. maintain a distinct 26.05-pinned Intel-Darwin package path until that branch reaches end of support.

Merely suppressing the warning would advertise a system that rolling Nixpkgs no longer supports.

### P2: partition development-only inputs, or state why they remain public

The refactor uses flake-parts modules but not partitions.
Home Manager is used only by checks, and WPE is used by the all-capabilities development shell while today's production manifest excludes `webview`.
Nevertheless both inputs remain in the root lock graph inherited by downstream flake consumers.

Flake-parts partitions are specifically intended to keep development-only inputs and module evaluation out of ordinary package access ([flake-parts partitions](https://flake.parts/options/flake-parts-partitions.html)).
Consider a small development-input subflake and partition `checks`, `devShells`, and `formatter` as Zed does.
Do this for downstream lock/fetch isolation, not simply to add more modules.

### P2: constrain metadata to the interface that owns it

The package metadata has `description`, `homepage`, `license`, and `mainProgram`, but no `meta.platforms`, even though the flake explicitly declares a support set.
Nixpkgs uses `meta.platforms` to state where a package is supported and to control availability/build selection ([Nixpkgs package metadata](https://nixos.org/manual/nixpkgs/unstable/#chap-meta)).
Move the systems list to one shared policy value and reuse it for both flake systems and `meta.platforms`.

The app now publishes only `meta.description`, matching the current Nix app schema rather than copying the package's entire metadata set ([Nix app schema](https://nix.dev/manual/nix/2.33/command-ref/new-cli/nix3-run#apps)).

Also consider using `workspace.package.version` (`0.0.16`) as the Nix package version and exposing the Git revision separately.
The current revision/date value is reproducible, but it does not report the release version users see in Cargo and Neomacs releases.

### P2: narrow the final package source

The dependency derivation uses `cleanCargoSource`, but the final package receives the complete flake source.
That is correct and preserves the Lisp/runtime assets, but unrelated tracked documentation or CI edits invalidate the final package build.

Use `lib.fileset.toSource` to union Crane's common Cargo sources with the runtime material required by `postBuild` and `postInstall` (`lisp`, `etc`, and required scripts/assets).
Crane documents filesets as the composable solution for Cargo plus non-Cargo inputs, and Nixpkgs filesets give explicit source membership ([Crane source filtering](https://crane.dev/source-filtering.html), [Nixpkgs filesets](https://nixos.org/manual/nixpkgs/unstable/#sec-functions-library-fileset)).
Retain the installed-package and Home Manager checks as regression guards while tightening it.

### P2: document both trusted caches

`nixConfig` asks users to accept the `eval-exec` and `nix-wpe-webkit` caches, but `docs/building.md` only explains the WPE cache and key in its system-wide example.
Document what each cache serves, who controls it, and why it is part of the default trust request.
This is not a schema violation, but `--accept-flake-config` approves both keys together.

### P3: keep application checks distinct from packaging checks

`doCheck = false` is reasonable because the repository already has large dedicated Rust/oracle/TUI/GUI/MELPA suites and the Nix checks are packaging contracts.
Keep that scope explicit in names and documentation.
If Nix-native CI later becomes a goal, Crane can expose separate `cargoClippy`, `cargoFmt`, and test derivations reusing `cargoArtifacts`; do not make the production package's build run the entire suite.

## Comparison with established projects

There is no single dominant implementation framework, even among mature Rust desktop projects:

| Project | Current shape | Relevant lesson |
|---|---|---|
| [Helix](https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/flake.nix) | About 100 lines of plain Nix; its derivation lives in [`default.nix`](https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/default.nix) | Plain Nix is sufficient when the output graph is small; filesets prevent irrelevant rebuilds. |
| [Zed](https://github.com/zed-industries/zed/blob/ce48461eaadd16c65c31f835511ab96bd3b6e746/flake.nix) | A 39-line flake-parts root importing focused modules | Its [`packages`](https://github.com/zed-industries/zed/blob/ce48461eaadd16c65c31f835511ab96bd3b6e746/nix/modules/packages.nix), [`overlays`](https://github.com/zed-industries/zed/blob/ce48461eaadd16c65c31f835511ab96bd3b6e746/nix/modules/overlays.nix), and [`partitions`](https://github.com/zed-industries/zed/blob/ce48461eaadd16c65c31f835511ab96bd3b6e746/nix/modules/partitions.nix) mirror Neomacs' new direction; development-only inputs are isolated. |
| [WezTerm](https://github.com/wez/wezterm/blob/4fbd6b8e90e2326b8e25c589768f98bd71ddd047/nix/flake.nix) | About 297 lines using flake-utils, with package, shell, formatter, and VM outputs largely together | A monolith and flake-utils remain valid; it also exposes the standard formatter output. |
| [Nix](https://github.com/NixOS/nix/blob/5438b85837c95849089100b6ee0c7c5e012f3389/flake.nix) | About 541 lines of hybrid plain Nix; flake-parts is limited to development outputs | Complex matrices can justify a large root, but packaging components, dependencies, Hydra jobs, checks, and dev shells are imported from focused files. Nix deliberately keeps development inputs away from ordinary outputs. |

The recurring practice is not a particular library.
It is a stable public output schema, explicit systems, reusable package construction, focused checks, source filtering, and named boundaries around substantial packaging/development logic.

## Is flake-parts justified here?

**Yes.** The current usage is reasonable and closely resembles Zed's structure.
Neomacs has enough cross-system outputs and independently evolving package, overlay, development-shell, formatter, and check policy to benefit from typed module composition and merging.
The 59-line root is materially easier to audit than the former 564-line entrypoint.

It is still optional, not a compliance requirement.
If Neomacs used only one package and one shell, ordinary imported functions like Helix would be simpler.
At the current scale, keep flake-parts and gain its second concrete benefit by partitioning development-only inputs when practical.

## Local evidence collected

- `nix flake check --all-systems --no-build --option allow-import-from-derivation false` completed successfully against the refactored working tree.
- `nix flake show --all-systems --json` exposed packages, apps, development shells, formatters, checks, and the default overlay for all four systems.
- The only evaluation warning was the Nixpkgs `x86_64-darwin` deprecation.
- `nixfmt 1.2.0 --check` accepted `flake.nix` and every `nix/**/*.nix` file.
- `flake.lock` now contains two Nixpkgs revisions: the root revision and the deliberately independent WPE revision. The accidental rust-overlay revision is gone.
- The evaluated package has `strictDeps = true`, `meta.mainProgram = "neomacs"`, and no `meta.platforms`.
- The native `x86_64-linux` package completed release compilation, fresh-build generation, full Lisp byte compilation, final pdump creation, installation, and Nix fixup.
- The installed-package startup/fingerprint contract and Home Manager wrapping/activation contract both built successfully against the realized package.
