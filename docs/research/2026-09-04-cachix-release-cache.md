# Cachix release-cache integration for Neomacs

Date: 2026-09-04

Scope: publish only the runtime closures of explicitly named Nix package outputs for an exact GitHub release tag. Development shells, checks, ordinary CI builds, dependency-only derivations, flake-input archives, and incidental store paths are deliberately out of scope.

## Recommendation

Use the existing `eval-exec` Cachix cache as a **public, release-only cache** with Cachix-managed signing. Give only the post-release publishing step a per-cache write token, build on a native runner for each system advertised by [`flake.nix`](../../flake.nix), explicitly push only `.#neomacs` and `.#neomacs-minimal`, and pin every tag/package/system root.

Add these jobs to the existing [`release.yml`](../../.github/workflows/release.yml) with `needs: create-release` and the same tag-only condition already used by the AUR and Docker publishers. Do not use a separate `on: release: types: [published]` workflow with the current pipeline: GitHub does not start a new workflow for an event caused by the repository `GITHUB_TOKEN`, apart from narrow documented exceptions. The existing release is created with that token. A same-workflow `needs` edge both runs after successful release creation and avoids another credential solely to trigger automation ([GitHub job dependencies](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idneeds), [`GITHUB_TOKEN` event behavior](https://docs.github.com/en/actions/concepts/security/github_token#when-github_token-triggers-workflow-runs)).

The publish matrix should be:

| Flake system | Native GitHub runner | Publish |
|---|---|---|
| `x86_64-linux` | `ubuntu-22.04` or `ubuntu-24.04` | `neomacs`, `neomacs-minimal` |
| `aarch64-linux` | `ubuntu-22.04-arm` or `ubuntu-24.04-arm` | `neomacs`, `neomacs-minimal` |
| `aarch64-darwin` | `macos-15` | `neomacs`, `neomacs-minimal` |
| `x86_64-darwin` | `macos-15-intel` | `neomacs`, `neomacs-minimal` while the flake continues to advertise Intel Darwin |

GitHub currently offers all four native runner architectures ([GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)). `x86_64-darwin` needs an explicit lifecycle decision soon: GitHub says its Intel label is available only through August 2027, and Nixpkgs has announced the end of rolling Intel-Darwin support around 26.11 ([GitHub runner announcement](https://github.com/actions/runner-images/issues/13045), [Nixpkgs 26.05 release note](https://github.com/NixOS/nixpkgs/blob/5aad24e6372ad85e2b27b0c8ef1382bf686deb3c/doc/release-notes/rl-2605.section.md#x86_64-darwin-2605)). Remove that matrix row when the flake removes the system; do not publish a cache target the flake can no longer evaluate.

## Why both package outputs count

The flake exposes `neomacs` and `neomacs-minimal` as first-class packages on every advertised system in [`nix/modules/packages.nix`](../../nix/modules/packages.nix). The GitHub release already publishes the full Linux product plus a minimal Linux tarball/AppImage. Therefore both named outputs are release products and should be cached.

This is still narrow: two named package roots per system, not “all packages” and not every path produced while building them. If the project later stops treating the minimal package as a released product, remove it explicitly from the publisher instead of switching to an automatic store watcher.

## Cachix service and platform boundary

Cachix is the binary-cache/storage service here, not the builder. Its documented model is that CI builds a project and pushes the result, after which users substitute the binaries ([Cachix binary-cache overview](https://docs.cachix.org/what-is-a-binary-cache)). Consequently:

- Linux outputs must be built on Linux and Darwin outputs on macOS, unless Neomacs deliberately introduces cross-compilation or remote builders. Native GitHub-hosted runners are simpler and match the current flake contract.
- The Cachix client and the official Nix installer action support Linux and macOS ([Nix supported platforms](https://nix.dev/manual/nix/2.34/installation/supported-platforms.html), [`install-nix-action`](https://github.com/cachix/install-nix-action)).
- Native Windows packages are not flake outputs in this repository. Nix on Windows is documented through WSL2, which produces Linux rather than MSVC closures ([Nix download page](https://nixos.org/download/#nix-install-windows)). Keep `.exe` and `.zip` files in GitHub Releases; Cachix does not replace that Windows distribution channel.

## Public/private access and signing

A public cache is the correct fit for an open-source release: anyone can read it, while pushing still requires authentication. A private cache would require consumer authentication for reads and would make the public flake-install story worse. Cachix documents that private caches require tokens for both reads and writes ([Cachix security](https://docs.cachix.org/security)).

The repository already declares `https://eval-exec.cachix.org` and its trusted public key in `flake.nix`, and its `nix-cache-info` endpoint is anonymously readable. No second cache is needed merely to publish Neomacs releases. The setup work on app.cachix.org is:

1. Confirm `eval-exec` remains public and that its displayed public key matches `flake.nix`.
2. Use Cachix-managed signing, the default and Cachix's recommendation for most users. This requires no signing-key secret in GitHub. Self-signed mode is available, but it adds a private signing key that must be backed up and supplied alongside authentication ([Cachix getting started](https://docs.cachix.org/getting-started#signing-key-advanced), [managed versus self-signed caches](https://docs.cachix.org/security#secrets)).
3. Generate a **per-cache, write-only** token in the cache settings, rather than a personal token with full account access, and save it as the repository secret `CACHIX_AUTH_TOKEN` ([Cachix token types](https://docs.cachix.org/getting-started#authenticating)). Reference that secret only from the authenticated push-and-pin step of the tag-only release job; the Nix build step must not receive it. A protected GitHub environment remains an optional later hardening measure ([GitHub environments and environment secrets](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments)).

### No documented trusted publishing/OIDC

As of this review, Cachix does not document a GitHub OIDC or “trusted publisher” token exchange. This is an interface-based finding: the current official action exposes `authToken` and optional `signingKey`, and its write example uses a stored `CACHIX_AUTH_TOKEN`; it exposes no OIDC input ([`cachix-action` v17 inputs](https://github.com/cachix/cachix-action/blob/38b082610b782e7e93e209c35fd730d399dee866/action.yml), [`cachix-action` v17 examples](https://github.com/cachix/cachix-action/blob/38b082610b782e7e93e209c35fd730d399dee866/README.md#examples)). Do not grant `id-token: write` for this integration. Revisit this conclusion if Cachix adds an official short-lived credential flow.

## Exact-closure publishing pattern

Cachix documents the flake runtime-closure pattern as `nix build --no-link --print-out-paths ... | cachix push CACHE`; multiple explicitly named packages may be supplied ([Cachix pushing with flakes](https://docs.cachix.org/pushing#flakes)). Build each root separately so the full and minimal paths are unambiguous for pinning, then push those two roots:

```sh
full_path="$(nix build --accept-flake-config --no-link --print-out-paths ".#packages.${NIX_SYSTEM}.neomacs")"
minimal_path="$(nix build --accept-flake-config --no-link --print-out-paths ".#packages.${NIX_SYSTEM}.neomacs-minimal")"

printf '%s\n' "$full_path" "$minimal_path" | cachix push eval-exec

pin_system="${NIX_SYSTEM//_/-}"
cachix pin eval-exec "neomacs-${GITHUB_REF_NAME}-${pin_system}" "$full_path"
cachix pin eval-exec "neomacs-minimal-${GITHUB_REF_NAME}-${pin_system}" "$minimal_path"
```

The checkout must remain the workflow's exact tag commit. `--accept-flake-config` is appropriate in this trusted release job because the flake itself declares the two project-approved substituters and public keys. Do not add `nix flake archive`, `nix flake check`, `nix develop`, `cachix watch-store`, or `cachix watch-exec` to this publisher.

The official action's default daemon mode installs a post-build hook and uploads newly built paths. Its fallback store-scan mode can capture unrelated paths and is explicitly unsafe on multi-user stores. Those modes conflict with the release-only boundary ([`cachix-action` push modes](https://github.com/cachix/cachix-action/blob/38b082610b782e7e93e209c35fd730d399dee866/README.md#push-modes)). If the action is used to install/configure the CLI, set `skipPush: true` and perform the explicit `cachix push` above.

A release job should install Cachix with every automatic upload path disabled, build without the secret, and expose the two store roots as step outputs. Only the final step receives the repository secret. The authoritative implementation is `publish-cachix-release` in [`release.yml`](../../.github/workflows/release.yml); keep the rationale here instead of duplicating executable workflow YAML.

The implementation uses the repository's pinned checkout commit. GitHub states that a full commit SHA is the only immutable way to reference an action; the same policy covers both Cachix actions ([GitHub secure-use guidance](https://docs.github.com/en/actions/reference/security/secure-use#using-third-party-actions)). The job's GitHub permission remains `contents: read`; the only external write credential is the Cachix per-cache token.

## Retention and garbage collection

Unpinned Cachix paths are collected when the cache reaches its storage limit, oldest by last access (or creation if never accessed). Cachix warns at 85% and at the limit, checks upstream caches before storing paths, and says it will not serve a path whose complete closure is unavailable ([Cachix garbage collection](https://docs.cachix.org/garbage-collection)).

Pins are therefore part of the release contract, not an optional optimization. A pinned path is immune from garbage collection by default. Pin history is indefinite unless bounded with `--keep-days` or `--keep-revisions` ([Cachix pins](https://docs.cachix.org/pins)). The tag/package/system names above create one permanent pin per release root, making ownership and deletion auditable. If storage must be bounded, adopt and document a release-retention policy before deleting pins; otherwise an older Git tag may still evaluate but rebuild from source.

## Consumer commands

Because `flake.nix` already embeds the public cache URL and trusted key, a user can install or run an exact cached release directly:

```sh
nix profile install --accept-flake-config \
  'github:eval-exec/neomacs/v0.0.16#neomacs'

nix run --accept-flake-config \
  'github:eval-exec/neomacs/v0.0.16#neomacs'
```

Use `#neomacs-minimal` for the minimal product. Nix documents `nix profile install` as installing a flake package and `nix run` as selecting `apps.<system>.<name>` or `packages.<system>.<name>` ([Nix profile install](https://nix.dev/manual/nix/latest/command-ref/new-cli/nix3-profile-install.html), [Nix run](https://nix.dev/manual/nix/latest/command-ref/new-cli/nix3-run.html)). `--accept-flake-config` opts into the cache configuration; its default is otherwise false ([Nix `accept-flake-config`](https://nix.dev/manual/nix/latest/command-ref/conf-file.html#conf-accept-flake-config)).

Users who prefer a persistent machine configuration can install the Cachix CLI and run:

```sh
cachix use eval-exec
```

For a public cache, no read token is required. `cachix use` writes the substituter and trusted-public-key configuration in the appropriate Nix configuration location ([Cachix getting started](https://docs.cachix.org/getting-started#using-binaries-with-nix), [Cachix FAQ](https://docs.cachix.org/faq#what-happens-when-i-run-cachix-use-both-immediately-and-any-stateful-effects-for-the-future)).

## Release cache versus CI cache

The release namespace should have exactly one writer path: successful, tag-triggered jobs that depend on `create-release`. Ordinary CI may read the public release cache, but it must not receive the write token and must not upload dev shells, checks, dependency builds, branch builds, or incidental outputs.

If broad CI caching is ever desired, create a different cache and token with separate trust and retention policy. Cachix recommends separating caches according to who can write/read them and explicitly identifies separate CI/development caches as a way to avoid polluting a main cache ([Cachix cache organization](https://docs.cachix.org/getting-started#organizing-your-caches)). That future option is not part of this release-only design.
