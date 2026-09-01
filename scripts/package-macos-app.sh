#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/package-macos-app.sh [--skip-build] [--no-smoke]

Build and package NEO Emacs as a complete macOS .app bundle inside `.dmg`,
`.zip`, and `.tar.gz` distribution containers.

The binary auto-detects the .app bundle layout via the
Resources/neomacs/ path (see load.rs:runtime_project_root) and its dump image
via Contents/MacOS/libexec, GNU's ns_applibexecdir (see path_exec.rs).

Environment:
  MACOS_APP_ONLY
           If set to "1", produce only the .app directory without creating
           distribution containers. NO_DMG=1 remains a deprecated alias.
  MACOS_DISTRIBUTION_MODE
           `adhoc` or `developer-id`. If unset, the mode is derived from the
           signing/notary variables; incomplete combinations are rejected.
  MACOS_SIGNING_IDENTITY
           Developer ID Application identity used to sign nested code, the
           app, and the DMG. Local builds fall back to ad-hoc app signing.
  MACOS_NOTARY_KEY_PATH, MACOS_NOTARY_KEY_ID, MACOS_NOTARY_ISSUER_ID
           App Store Connect API key used to notarize/staple the app and DMG.

Output:
  dist/neomacs-{version}-aarch64-apple-darwin.dmg
  dist/neomacs-{version}-aarch64-apple-darwin.zip
  dist/neomacs-{version}-aarch64-apple-darwin.tar.gz
  dist/neomacs.app
USAGE
}

skip_build=0
smoke=1

while (($#)); do
  case "$1" in
    --skip-build)
      skip_build=1
      shift
      ;;
    --no-smoke)
      smoke=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

get_version() {
  local v
  v="$(git describe --tags --abbrev=0 2>/dev/null)" && echo "${v#v}" && return
  v="$(git rev-parse --short=12 HEAD 2>/dev/null)" && echo "$v" && return
  echo "0.0.0-dev"
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# shellcheck source=./scripts/lib/archlib.sh
source "$repo_root/scripts/lib/archlib.sh"

dist_dir="$repo_root/dist"
version="$(get_version)"
product_name="NEO Emacs"
app_bundle_name="neomacs"
app_bundle="$dist_dir/$app_bundle_name.app"
artifact_stem="neomacs-${version}-aarch64-apple-darwin"
dmg="$dist_dir/$artifact_stem.dmg"
zip="$dist_dir/$artifact_stem.zip"
tarball="$dist_dir/$artifact_stem.tar.gz"

notary_values=(
  "${MACOS_NOTARY_KEY_PATH:-}"
  "${MACOS_NOTARY_KEY_ID:-}"
  "${MACOS_NOTARY_ISSUER_ID:-}"
)
notary_values_present=0
for value in "${notary_values[@]}"; do
  [[ -n "$value" ]] && notary_values_present=$((notary_values_present + 1))
done

distribution_mode="${MACOS_DISTRIBUTION_MODE:-}"
if [[ -z "$distribution_mode" ]]; then
  if [[ -z "${MACOS_SIGNING_IDENTITY:-}" ]] && ((notary_values_present == 0)); then
    distribution_mode=adhoc
  elif [[ -n "${MACOS_SIGNING_IDENTITY:-}" ]] && ((notary_values_present == 3)); then
    distribution_mode=developer-id
  else
    echo "cannot derive macOS distribution mode from incomplete credentials" >&2
    exit 1
  fi
fi

case "$distribution_mode" in
  adhoc)
    if [[ -n "${MACOS_SIGNING_IDENTITY:-}" ]] || ((notary_values_present != 0)); then
      echo "adhoc mode does not accept Developer ID or notary credentials" >&2
      exit 1
    fi
    ;;
  developer-id)
    if [[ -z "${MACOS_SIGNING_IDENTITY:-}" ]] || ((notary_values_present != 3)); then
      echo "developer-id mode requires a signing identity and all three notary values" >&2
      exit 1
    fi
    ;;
  *)
    echo "MACOS_DISTRIBUTION_MODE must be adhoc or developer-id" >&2
    exit 1
    ;;
esac

app_only="${MACOS_APP_ONLY:-${NO_DMG:-0}}"
case "$app_only" in
  0|1)
    ;;
  *)
    echo "MACOS_APP_ONLY must be 0 or 1" >&2
    exit 1
    ;;
esac
if [[ -n "${NO_DMG:-}" && -z "${MACOS_APP_ONLY:-}" ]]; then
  echo "warning: NO_DMG is deprecated; use MACOS_APP_ONLY=1" >&2
fi

if ((skip_build == 0)); then
  cargo xtask fresh-build --release
fi

release_dir="$repo_root/target/release"

for required in "$release_dir/neomacs" "$release_dir/neomacsclient" "$release_dir/neomacs.pdump"; do
  if [[ ! -f "$required" ]]; then
    echo "missing required release artifact: $required" >&2
    echo "run cargo xtask fresh-build --release first, or pass --skip-build" >&2
    exit 1
  fi
done

rm -rf "$app_bundle"

# GNU's self-contained NS bundle, verbatim (configure.ac:2790-2793):
#
#   ns_appbindir     = Contents/MacOS
#   ns_applibexecdir = Contents/MacOS/libexec     <- libexecdir AND archlibdir
#   ns_appresdir     = Contents/Resources
#
# The archlib is where the dump image goes -- GNU installs exactly one file
# there for this layout, `${libexecdir}/Emacs.pdmp' (Makefile.in:639), which
# `load_pdump' finds on its fourth rung, `PATH_EXEC/basename(argv0).pdmp'
# (src/emacs.c:1096-1120).  Ours is Contents/MacOS/libexec/neomacs.pdump.
#
# It also has to go somewhere other than Contents/MacOS itself, and that is
# not a matter of taste.  Apple's default resource rules seal
#
#   '^(Frameworks|SharedFrameworks|PlugIns|Plug-ins|XPCServices|Helpers|MacOS
#     |Library/(Automator|Spotlight|LoginItems))/' = {nested=#T, weight=10}
#
# (Security, OSX/libsecurity_codesigning/lib/bundlediskrep.cpp, the V2 rules
# in BundleDiskRep::defaultResourceRules), and TN2206 says of those places
# that they "are expected to contain only code.  Putting arbitrary data files
# there will cause them to be rejected (since they're unsigned)."  Moving the
# dump one directory down does NOT escape that rule -- the pattern is matched
# with regexec, i.e. as a search, so it covers MacOS/ at any depth -- so
# sign-macos-app.sh signs every regular file under the code roots, which is
# what codesign --deep does for the Emacs.app builds that ship notarized with
# this same layout.
macos_dir="$app_bundle/Contents/MacOS"
archlib_dir="$macos_dir/libexec"

mkdir -p "$macos_dir"
mkdir -p "$archlib_dir"
mkdir -p "$app_bundle/Contents/Resources/neomacs"
mkdir -p "$app_bundle/Contents/Frameworks"

# GNU splits its helper programs the same way (lib-src/Makefile.in): the
# user-facing INSTALLABLES go to bindir, the private UTILITIES to archlibdir.
# neomacsclient is our emacsclient, so it stays beside the main executable
# where install.sh symlinks it onto $PATH; the build-internal binaries move
# into the archlib, which is what `exec-directory' now names.
for binary in neomacs neomacsclient; do
  if [[ -f "$release_dir/$binary" ]]; then
    install -m 0755 "$release_dir/$binary" "$macos_dir/$binary"
  fi
done
for binary in neomacs-temacs bootstrap-neomacs mock-display; do
  if [[ -f "$release_dir/$binary" ]]; then
    install -m 0755 "$release_dir/$binary" "$archlib_dir/$binary"
  fi
done

install -m 0644 "$release_dir/neomacs.pdump" "$archlib_dir/neomacs.pdump"

cp -a lisp "$app_bundle/Contents/Resources/neomacs/"
cp -a etc "$app_bundle/Contents/Resources/neomacs/"
cp -a leim "$app_bundle/Contents/Resources/neomacs/"
cp -a info "$app_bundle/Contents/Resources/neomacs/" 2>/dev/null || true

cat >"$app_bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>neomacs</string>
  <key>CFBundleDisplayName</key>
  <string>${product_name}</string>
  <key>CFBundleExecutable</key>
  <string>neomacs</string>
  <key>CFBundleIdentifier</key>
  <string>org.neomacs</string>
  <key>CFBundleVersion</key>
  <string>${version}</string>
  <key>CFBundleShortVersionString</key>
  <string>${version}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>CFBundleIconFile</key>
  <string>neomacs</string>
</dict>
</plist>
PLIST

if [[ -f assets/logo-128.png ]]; then
  mkdir -p "$app_bundle/Contents/Resources"
  sips -s format icns assets/logo-128.png \
    --out "$app_bundle/Contents/Resources/neomacs.icns" \
    2>/dev/null || true
fi

install -m 0644 README.md "$app_bundle/Contents/Resources/README.md"
install -m 0644 COPYING "$app_bundle/Contents/Resources/COPYING"

scripts/vendor-macos-runtime.sh "$app_bundle"
scripts/sign-macos-app.sh "$app_bundle"

# Unconditional, and after signing: vendoring rewrites load commands and so
# invalidates the linker's ad-hoc signature, which on Apple Silicon makes the
# binary refuse to run until it is signed again.  This is the check that the
# PATH_EXEC probe compiled into the binary and the directory this script
# staged are the same directory, and that the dump-lookup rungs reach the
# image without being told where it is.
neomacs_verify_archlib \
  "$macos_dir/neomacs" \
  "$archlib_dir/neomacs.pdump" \
  "$archlib_dir" \
  "$app_bundle/Contents/Resources/neomacs"

if ((smoke)); then
  echo "smoke-testing .app bundle..."
  APP_BUNDLE="$app_bundle" python3 <<'PY'
import os
import subprocess

app = os.environ["APP_BUNDLE"]
environment = os.environ.copy()
environment["NEOMACS_RUNTIME_ROOT"] = os.path.join(
    app, "Contents", "Resources", "neomacs"
)
subprocess.run(
    [
        os.path.join(app, "Contents", "MacOS", "neomacs"),
        "--batch",
        "--eval",
        "(kill-emacs 0)",
    ],
    check=True,
    env=environment,
    timeout=30,
)
PY
fi

notarize_target() {
  local target="$1"
  local label="$2"
  local submission="$notary_dir/$label-submission.json"
  local log="$notary_dir/$label-log.json"
  local submission_id

  if ! xcrun notarytool submit "$target" \
    --key "$MACOS_NOTARY_KEY_PATH" \
    --key-id "$MACOS_NOTARY_KEY_ID" \
    --issuer "$MACOS_NOTARY_ISSUER_ID" \
    --wait \
    --output-format json >"$submission"; then
    echo "notarytool rejected $target" >&2
    return 1
  fi

  submission_id="$(python3 - "$submission" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    submission = json.load(stream)
print(submission["id"])
PY
)"
  xcrun notarytool log "$submission_id" \
    --key "$MACOS_NOTARY_KEY_PATH" \
    --key-id "$MACOS_NOTARY_KEY_ID" \
    --issuer "$MACOS_NOTARY_ISSUER_ID" \
    "$log"
  python3 - "$submission" "$log" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    submission = json.load(stream)
with open(sys.argv[2], encoding="utf-8") as stream:
    log = json.load(stream)
issues = log.get("issues", [])
if issues:
    for issue in issues:
        print(
            f"notary issue: {issue.get('severity', 'unknown')}: "
            f"{issue.get('path', '<unknown path>')}: {issue.get('message', '')}",
            file=sys.stderr,
        )
    raise SystemExit("notarization log contains issues")
if submission.get("status") != "Accepted":
    raise SystemExit(f"notarization was not accepted: {submission}")
PY
}

stage_app_payload() {
  local destination="$1"

  rm -rf "$destination"
  mkdir -p "$destination"
  # Apple recommends ditto because it preserves bundle metadata and symlinks
  # consistently across macOS releases.
  ditto "$app_bundle" "$destination/$app_bundle_name.app"
  if [[ "$distribution_mode" == adhoc ]]; then
    install -m 0644 scripts/macos-unnotarized-readme.txt \
      "$destination/If macOS blocks NEO Emacs.txt"
  fi
}

if [[ "$distribution_mode" == developer-id ]]; then
  # ZIP is an accepted notary input but cannot itself be stapled. Notarize a
  # temporary app-only ZIP, staple the resulting ticket to the app, and only
  # then create every public container from that same stapled app.
  notary_dir="$dist_dir/notary"
  app_notary_zip="$notary_dir/neomacs-app.zip"
  rm -rf "$notary_dir"
  mkdir -p "$notary_dir"
  ditto -c -k --sequesterRsrc --keepParent "$app_bundle" "$app_notary_zip"
  notarize_target "$app_notary_zip" app
  xcrun stapler staple "$app_bundle"
  xcrun stapler validate "$app_bundle"
  rm -f "$app_notary_zip"
fi

if [[ "$app_only" == "1" ]]; then
  echo "wrote $app_bundle (MACOS_APP_ONLY=1, skipping distribution containers)"
  exit 0
fi

echo "creating .zip and .tar.gz archives..."
archive_staging="$dist_dir/$artifact_stem"
stage_app_payload "$archive_staging"
rm -f "$zip" "$tarball"
ditto -c -k --sequesterRsrc --keepParent "$archive_staging" "$zip"
# Keep macOS metadata enabled. The tarball is a macOS binary distribution, and
# the clean-runner extraction verifies that signatures and any stapled app
# ticket survive the round trip.
tar -C "$dist_dir" -czf "$tarball" "$artifact_stem"
rm -rf "$archive_staging"

echo "creating .dmg..."
rm -f "$dmg"

dmg_staging="$dist_dir/dmg-staging"
stage_app_payload "$dmg_staging"
ln -sf /Applications "$dmg_staging/Applications"

# hdiutil can intermittently fail with "hdiutil: create failed - Resource
# busy" on CI runners (a leftover mount of the same volume, or Spotlight/mds
# indexing the source folder while it is read). Detach any stale volume and
# retry a few times before giving up.
create_dmg() {
  local vol="/Volumes/$app_bundle_name"
  [[ -d "$vol" ]] && hdiutil detach "$vol" -force >/dev/null 2>&1 || true
  hdiutil create \
    -volname "$app_bundle_name" \
    -srcfolder "$dmg_staging" \
    -ov \
    -format UDZO \
    "$dmg"
}

dmg_attempts=5
for attempt in $(seq 1 "$dmg_attempts"); do
  if create_dmg; then
    break
  fi
  if [[ "$attempt" -eq "$dmg_attempts" ]]; then
    echo "hdiutil create failed after $dmg_attempts attempts" >&2
    exit 1
  fi
  echo "hdiutil create failed (attempt $attempt/$dmg_attempts); retrying in 5s..." >&2
  sleep 5
done

rm -rf "$dmg_staging"
hdiutil verify "$dmg"

if [[ "$distribution_mode" == developer-id ]]; then
  codesign --force --timestamp \
    --identifier org.neomacs.dmg \
    --sign "$MACOS_SIGNING_IDENTITY" \
    "$dmg"
fi

if [[ "$distribution_mode" == developer-id ]]; then
  notarize_target "$dmg" dmg
  xcrun stapler staple "$dmg"
  xcrun stapler validate "$dmg"
else
  echo "warning: app is ad-hoc signed and unnotarized; users may need Apple's Open Anyway flow" >&2
fi

echo "wrote $dmg"
echo "wrote $zip"
echo "wrote $tarball"
