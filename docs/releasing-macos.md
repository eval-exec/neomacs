# Releasing Neomacs for macOS

The distributed artifacts are built and verified on GitHub's pinned `macos-15`
arm64 runner. NixOS is useful for reviewing scripts and auditing an existing
Mach-O bundle, but it cannot perform Apple's code-signing, notarization,
stapling, Gatekeeper, or final-container launch checks.

## Release invariant

Every public macOS binary artifact must have one workflow run proving these
properties on the exact files uploaded to GitHub Releases:

1. The pinned official GStreamer SDK was checksum-verified before use.
2. Every non-system Mach-O dependency and required runtime resource is inside
   `neomacs.app`; load commands are bundle-relative.
3. Every nested executable, library, and plug-in is signed inside-out before
   the app is sealed. Ad-hoc builds and Developer ID builds both include
   Neomacs' JIT entitlement.
4. A second clean macOS runner mounts or extracts the `.dmg`, `.zip`, and
   `.tar.gz` without installing the build SDK; it audits each dependency
   closure, verifies each signature, starts every app in batch mode, and
   requires all three apps to have the same code-directory hash.

## Artifact formats

The release publishes three binary containers made from one signed,
self-contained `neomacs.app`:

- `.dmg` is the primary Finder installation experience and includes an
  `/Applications` shortcut.
- `.zip` is Apple's recommended archive alternative and is created with
  `ditto --keepParent` so bundle metadata is preserved.
- `.tar.gz` is a portable archive for users and automation that expect tar.
  It contains the complete app, not the old loose binaries.

The ZIP and tarball use a versioned top-level directory. In ad-hoc mode every
container also includes `If macOS blocks NEO Emacs.txt`. GitHub's automatic
`Source code (tar.gz)` download remains source code and is not a binary macOS
artifact.

The workflow supports two explicit distribution-trust modes:

- **Ad-hoc (the default):** requires no Apple account. CI still proves that the
  bundle is complete, signed consistently, and launchable. Because Apple
  cannot identify or notarize the publisher, a downloaded copy may require the
  user to approve NEO Emacs in **System Settings > Privacy & Security > Open
  Anyway**. Every container includes those instructions.
- **Developer ID:** when all signing/notary secrets are present, CI additionally
  applies a Developer ID Application signature with hardened runtime and a
  secure timestamp. It notarizes and staples the app before creating the public
  archives, then signs, notarizes, and staples the DMG. The clean runner
  verifies Gatekeeper policy for every extracted app and the DMG itself.

The release workflow is the orchestration boundary: it constructs the artifacts
through `scripts/package-macos-app.sh`, then verifies every exact container
through `scripts/test-macos-release-artifact.sh` on a clean runner. Keep app
construction behind the package script so local packaging and CI cannot
silently acquire different behavior.

## Optional Developer ID upgrade

Apple only issues Developer ID certificates and accepts notarization requests
for members of the paid Apple Developer Program (or Enterprise Program).
Neomacs therefore does not require these credentials for a release.

Create a Developer ID Application certificate in the Apple Developer account,
export the certificate and private key as a password-protected `.p12`, and add
these repository or protected-environment secrets:

| Secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_P12_BASE64` | Base64-encoded `.p12` bytes |
| `MACOS_CERTIFICATE_PASSWORD` | Password used when exporting the `.p12` |
| `APPLE_NOTARY_KEY_P8_BASE64` | Base64-encoded App Store Connect API `.p8` key |
| `APPLE_NOTARY_KEY_ID` | App Store Connect API key ID |
| `APPLE_NOTARY_ISSUER_ID` | App Store Connect issuer ID |

On macOS, encode a file for a GitHub secret with `base64 -i FILE | pbcopy`.
The workflow imports the certificate into a job-local keychain only after the
build completes. With no secrets, tag and manual builds use the ad-hoc mode. A
partial secret set always fails rather than producing an ambiguously signed
artifact. After configuring all five secrets, optionally set the repository
variable `MACOS_REQUIRE_SIGNING=1` to make missing credentials fail closed.

## Local checks from NixOS

To reproduce the v0.0.15 class of failure without launching macOS, extract the
app below the repository's ignored `./tmp/` directory and run:

```bash
nix shell nixpkgs#llvm -c \
  ./scripts/audit-macos-app.sh ./tmp/release/neomacs.app
```

This catches absolute Homebrew paths and omitted bundled libraries. A passing
static audit is necessary but not sufficient: the macOS workflow remains the
authority for signing and launch behavior, plus notarization and Gatekeeper
policy when Developer ID mode is configured.

## Maintenance policy

- Pin the macOS runner and GStreamer SDK version; update each deliberately.
- Keep the runtime relocation logic in-tree. Generic dylib bundlers commonly
  skip paths containing `.framework`, while the official GStreamer SDK uses
  exactly those paths.
- Record SHA-256 digests for both GStreamer installer packages in
  `scripts/setup-macos-gstreamer.sh`.
- Never sign with `codesign --deep`; sign each nested code object inside-out.
  Using `--deep` to verify the completed bundle is acceptable.
- In Developer ID mode, notarize and staple the app before creating ZIP/tar
  archives. Separately sign, notarize, and staple the outer DMG. Inspect every
  notary log even when the submission is accepted.
- In ad-hoc mode, describe the artifact as **unnotarized**. Direct users to
  Apple's per-app **Open Anyway** flow; never recommend globally disabling
  Gatekeeper.
- Test every mounted or extracted release artifact on a second clean runner,
  not `target/release` and not only the pre-container app. The build runner's
  installed SDK and source tree must not mask missing files.
- The current public package is Apple Silicon only. An Intel or universal
  release needs its own build and an explicit merge/verification policy; do
  not relabel a single-architecture binary.

Primary references:

- [GitHub: Installing an Apple certificate on macOS runners](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications)
- [Apple: Creating distribution-signed code for macOS](https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac/)
- [Apple: Packaging Mac software for distribution](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution)
- [Apple: Notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- [Apple: Open an app from an unknown developer](https://support.apple.com/guide/mac-help/mh40616/mac)
- [GStreamer: macOS deployment](https://gstreamer.freedesktop.org/documentation/deploying/mac-osx.html)
