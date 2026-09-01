#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/test-macos-release-artifact.sh ARTIFACT...

Extract every `.dmg`, `.zip`, or `.tar.gz` macOS artifact intended for
distribution. The test audits Mach-O dependency closure, verifies signatures,
launches each contained application in batch mode, and requires every format
to contain the same signed app. Set MACOS_DISTRIBUTION_MODE to `adhoc` or
`developer-id`; Developer ID mode also requires Gatekeeper and stapled-ticket
validation.

All scratch data is created below ./tmp.
USAGE
}

if (($# == 0)); then
  usage >&2
  exit 2
fi

distribution_mode="${MACOS_DISTRIBUTION_MODE:-}"
case "$distribution_mode" in
  adhoc|developer-id)
    ;;
  *)
    echo "MACOS_DISTRIBUTION_MODE must be explicitly set to adhoc or developer-id" >&2
    exit 1
    ;;
esac
app_bundle_name=neomacs

if [[ "$(uname -s)" != Darwin ]]; then
  echo "macOS artifact verification must run on macOS" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$repo_root/tmp"
work_dir="$(mktemp -d "$repo_root/tmp/macos-release-test.XXXXXX")"
active_mount=

cleanup() {
  if [[ -n "$active_mount" ]]; then
    hdiutil detach "$active_mount" -force >/dev/null 2>&1 || true
  fi
  rm -rf "$work_dir"
}
trap cleanup EXIT

dmg_count=0
zip_count=0
tarball_count=0
for artifact in "$@"; do
  if [[ ! -f "$artifact" ]]; then
    echo "release artifact does not exist: $artifact" >&2
    exit 1
  fi
  case "$artifact" in
    *.dmg)
      dmg_count=$((dmg_count + 1))
      ;;
    *.zip)
      zip_count=$((zip_count + 1))
      ;;
    *.tar.gz)
      tarball_count=$((tarball_count + 1))
      ;;
    *)
      echo "unsupported macOS artifact format: $artifact" >&2
      exit 1
      ;;
  esac
done
if ((dmg_count != 1 || zip_count != 1 || tarball_count != 1)); then
  echo "verification requires exactly one .dmg, one .zip, and one .tar.gz" >&2
  echo "  found: $dmg_count DMG, $zip_count ZIP, $tarball_count tarball" >&2
  exit 1
fi

assert_directory_entries() {
  local directory="$1"
  local artifact="$2"
  shift 2
  local actual=()
  local expected=("$@")
  local entry
  local expected_entry
  local actual_entry
  local found

  while IFS= read -r -d '' entry; do
    actual+=("$(basename "$entry")")
  done < <(find "$directory" -mindepth 1 -maxdepth 1 -print0)

  if ((${#actual[@]} != ${#expected[@]})); then
    echo "$artifact has an unexpected container layout" >&2
    printf '  expected: %s\n' "${expected[*]}" >&2
    printf '  actual:   %s\n' "${actual[*]}" >&2
    return 1
  fi
  for expected_entry in "${expected[@]}"; do
    found=0
    for actual_entry in "${actual[@]}"; do
      if [[ "$actual_entry" == "$expected_entry" ]]; then
        found=1
        break
      fi
    done
    if ((found == 0)); then
      echo "$artifact is missing expected entry: $expected_entry" >&2
      return 1
    fi
  done
}

assert_payload_layout() {
  local payload_root="$1"
  local artifact="$2"
  local artifact_name
  local archive_root
  local content_root
  local expected_app
  local instructions
  local payload_entries=("$app_bundle_name.app")

  if [[ "$distribution_mode" == adhoc ]]; then
    payload_entries+=("If macOS blocks NEO Emacs.txt")
  fi

  artifact_name="$(basename "$artifact")"
  case "$artifact" in
    *.dmg)
      payload_entries+=(Applications)
      assert_directory_entries "$payload_root" "$artifact" "${payload_entries[@]}"
      content_root="$payload_root"
      if [[ ! -L "$payload_root/Applications" \
        || "$(readlink "$payload_root/Applications")" != /Applications ]]; then
        echo "$artifact must contain an Applications symlink to /Applications" >&2
        return 1
      fi
      ;;
    *.zip)
      archive_root="${artifact_name%.zip}"
      assert_directory_entries "$payload_root" "$artifact" "$archive_root"
      [[ -d "$payload_root/$archive_root" && ! -L "$payload_root/$archive_root" ]] || {
        echo "$artifact must contain a directory named $archive_root" >&2
        return 1
      }
      content_root="$payload_root/$archive_root"
      assert_directory_entries \
        "$content_root" "$artifact" "${payload_entries[@]}"
      ;;
    *.tar.gz)
      archive_root="${artifact_name%.tar.gz}"
      assert_directory_entries "$payload_root" "$artifact" "$archive_root"
      [[ -d "$payload_root/$archive_root" && ! -L "$payload_root/$archive_root" ]] || {
        echo "$artifact must contain a directory named $archive_root" >&2
        return 1
      }
      content_root="$payload_root/$archive_root"
      assert_directory_entries \
        "$content_root" "$artifact" "${payload_entries[@]}"
      ;;
  esac

  expected_app="$content_root/$app_bundle_name.app"
  if [[ ! -d "$expected_app" || -L "$expected_app" ]]; then
    echo "$artifact must contain $app_bundle_name.app as a direct, non-symlink directory" >&2
    return 1
  fi

  if [[ "$distribution_mode" == adhoc ]]; then
    instructions="$content_root/If macOS blocks NEO Emacs.txt"
    if [[ ! -f "$instructions" || -L "$instructions" ]]; then
      echo "$artifact must contain Open Anyway instructions as a regular file" >&2
      return 1
    fi
    if ! cmp -s "$repo_root/scripts/macos-unnotarized-readme.txt" "$instructions"; then
      echo "$artifact contains modified Open Anyway instructions" >&2
      return 1
    fi
  fi

  printf '%s\n' "$expected_app"
}

launch_app() {
  APP_BUNDLE="$1" python3 <<'PY'
import os
import subprocess

app = os.environ["APP_BUNDLE"]
subprocess.run(
    [
        os.path.join(app, "Contents", "MacOS", "neomacs"),
        "--batch",
        "--eval",
        "(kill-emacs 0)",
    ],
    check=True,
    timeout=30,
)
PY
}

expected_app_cdhash=
artifact_index=0
for artifact in "$@"; do
  artifact_index=$((artifact_index + 1))
  extract_root="$work_dir/artifact-$artifact_index"
  mkdir -p "$extract_root"

  case "$artifact" in
    *.dmg)
      hdiutil verify "$artifact"
      if [[ "$distribution_mode" == developer-id ]]; then
        codesign --verify --verbose=2 "$artifact"
        xcrun stapler validate "$artifact"
        spctl --assess --type open --context context:primary-signature --verbose=4 "$artifact"
      fi
      active_mount="$extract_root/mount"
      mkdir -p "$active_mount"
      hdiutil attach -readonly -nobrowse -mountpoint "$active_mount" "$artifact" >/dev/null
      payload_root="$active_mount"
      ;;
    *.zip)
      ditto -x -k "$artifact" "$extract_root"
      payload_root="$extract_root"
      ;;
    *.tar.gz)
      tar -C "$extract_root" -xzf "$artifact"
      payload_root="$extract_root"
      ;;
  esac

  app="$(assert_payload_layout "$payload_root" "$artifact")"
  "$repo_root/scripts/audit-macos-app.sh" "$app"
  codesign --verify --deep --strict --verbose=2 "$app"

  app_cdhash="$(codesign --display --verbose=4 "$app" 2>&1 \
    | sed -n 's/^CDHash=//p' \
    | head -n 1)"
  if [[ -z "$app_cdhash" ]]; then
    echo "could not read application signature hash from $artifact" >&2
    exit 1
  fi
  if [[ -z "$expected_app_cdhash" ]]; then
    expected_app_cdhash="$app_cdhash"
  elif [[ "$app_cdhash" != "$expected_app_cdhash" ]]; then
    echo "macOS artifacts do not contain the same signed application" >&2
    echo "  expected CDHash: $expected_app_cdhash" >&2
    echo "  $artifact: $app_cdhash" >&2
    exit 1
  fi

  if [[ "$distribution_mode" == developer-id ]]; then
    xcrun stapler validate "$app"
    spctl --assess --type execute --verbose=4 "$app"
  fi

  launch_app "$app"

  if [[ -n "$active_mount" ]]; then
    hdiutil detach "$active_mount" >/dev/null
    active_mount=
  fi
  echo "verified distributed macOS artifact: $artifact"
done

echo "verified $artifact_index macOS artifacts with app CDHash $expected_app_cdhash"
