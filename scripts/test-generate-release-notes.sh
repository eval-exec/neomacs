#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/neomacs-release-notes-test.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT

dist_dir="$work_dir/dist"
output="$work_dir/release-notes.md"
generated_notes="$work_dir/generated-notes.md"
mkdir -p "$dist_dir"

cat >"$generated_notes" <<'NOTES'
## What's Changed

* Add the release download guide by @eval-exec in https://github.com/eval-exec/neomacs/pull/999

## New Contributors
* @someone made their first contribution in https://github.com/eval-exec/neomacs/pull/998

**Full Changelog**: https://github.com/eval-exec/neomacs/compare/v9.8.6...v9.8.7
NOTES

assets=(
  neomacs-9.8.7-x86_64-unknown-linux-gnu.AppImage
  neomacs-9.8.7-aarch64-unknown-linux-gnu.AppImage
  neomacs_9.8.7_amd64.deb
  neomacs_9.8.7_arm64.deb
  neomacs-9.8.7-1.x86_64.rpm
  neomacs-9.8.7-1.aarch64.rpm
  neomacs-9.8.7-x86_64-unknown-linux-gnu.tar.gz
  neomacs-9.8.7-aarch64-unknown-linux-gnu.tar.gz
  neomacs-9.8.7-aarch64-apple-darwin.dmg
  neomacs-9.8.7-aarch64-apple-darwin.zip
  neomacs-9.8.7-aarch64-apple-darwin.tar.gz
  neomacs-9.8.7-x86_64-pc-windows-msvc-user-setup.exe
  neomacs-9.8.7-x86_64-pc-windows-msvc.zip
  neomacs-9.8.7-aarch64-pc-windows-msvc-user-setup.exe
  neomacs-9.8.7-aarch64-pc-windows-msvc.zip
)

touch "$dist_dir/install.sh" "$dist_dir/SHA256SUMS"
for asset in "${assets[@]}"; do
  touch "$dist_dir/$asset"
done

"$repo_root/scripts/generate-release-notes.sh" \
  --repo eval-exec/neomacs \
  --tag v9.8.7 \
  --dist-dir "$dist_dir" \
  --generated-notes "$generated_notes" \
  --output "$output"

assert_contains() {
  local expected="$1"
  if ! grep -Fq "$expected" "$output"; then
    echo "generated release notes are missing: $expected" >&2
    exit 1
  fi
}

assert_contains '## Install Neomacs — Choose a Method'
assert_contains '<th>Distribution</th>'
assert_contains '<th>Architecture</th>'
assert_contains '<th>Install / download</th>'
assert_contains '<td rowspan="11"><img'
assert_contains '<td rowspan="3" colspan="2">Apple Silicon<br><code>aarch64</code></td>'
assert_contains '<td rowspan="2" colspan="2"><code>x86_64</code></td>'
assert_contains 'alt="Archive file icon"> <strong>Portable archive</strong></td>'
assert_contains 'alt="AppImage logo"> <strong>AppImage</strong></td>'
assert_contains 'alt="Debian logo"> <strong>Debian</strong><br><img'
assert_contains 'alt="Ubuntu logo"> <strong>Ubuntu</strong></td>'
assert_contains 'alt="Fedora logo"> <strong>Fedora</strong><br><img'
assert_contains 'alt="Red Hat logo"> <strong>RHEL</strong><br><img'
assert_contains 'alt="openSUSE logo"> <strong>openSUSE</strong></td>'
assert_contains 'alt="NixOS logo"> <strong>Nix flake</strong>'
assert_contains '<code>nix run --accept-flake-config github:eval-exec/neomacs/v9.8.7</code>'
assert_contains 'alt="Docker logo"> <strong>Docker</strong>'
assert_contains 'https://github.com/eval-exec/neomacs/pkgs/container/neomacs'
assert_contains 'https://hub.docker.com/r/evalexec/neomacs/tags?name=9.8.7'
assert_contains 'alt="Arch Linux logo"> <strong>ArchLinux</strong></td>'
assert_contains 'href="https://aur.archlinux.org/packages/neomacs-bin"><code>neomacs-bin</code></a>'
assert_contains 'https://github.com/eval-exec/neomacs/releases/download/v9.8.7/SHA256SUMS'
assert_contains '<details>'
assert_contains "<summary><strong>What's Changed</strong></summary>"
assert_contains '* Add the release download guide by @eval-exec'
assert_contains '</details>'
assert_contains '## New Contributors'
assert_contains '**Full Changelog**: https://github.com/eval-exec/neomacs/compare/v9.8.6...v9.8.7'

for asset in "${assets[@]}"; do
  assert_contains "href=\"https://github.com/eval-exec/neomacs/releases/download/v9.8.7/$asset\"><code>$asset</code></a>"
done

download_count="$(grep -o 'href="https://github.com/eval-exec/neomacs/releases/download/v9.8.7/[^\"]*"><code>[^<]*</code></a>' "$output" | wc -l | tr -d ' ')"
if [[ "$download_count" != "15" ]]; then
  echo "expected 15 package download links, found $download_count" >&2
  exit 1
fi

if grep -Fq '⬇️' "$output"; then
  echo "generated release notes contain a download emoji" >&2
  exit 1
fi

table_html="$(sed -n '/^<table>$/,/^<\/table>$/p' "$output")"
header_count="$(grep -o '<th>' <<<"$table_html" | wc -l | tr -d ' ')"
if [[ "$header_count" != "4" ]] || grep -Fq '<th>Notes</th>' <<<"$table_html"; then
  echo "generated release table should have four headers and no Notes column" >&2
  exit 1
fi

if grep -Eiq 'For manual installation|Self-contained AppImage|Native Linux package pinned|Portable terminal/batch method|Prebuilt AUR package|Native package for|DMG installer|Application bundle|User installer|Portable ZIP for' <<<"$table_html"; then
  echo "generated release table retains Notes-cell content" >&2
  exit 1
fi

if grep -Eq '<br><code>\.(tar\.gz|deb|rpm)</code>' <<<"$table_html"; then
  echo "distribution/package cells retain standalone file extensions" >&2
  exit 1
fi

if grep -Eq 'docker run --rm -it|paru -S neomacs-bin' <<<"$table_html"; then
  echo "generated release table retains hidden Docker or AUR commands" >&2
  exit 1
fi

if grep -Fq 'Any distribution' <<<"$table_html"; then
  echo "generated release table retains the AppImage distribution subtitle" >&2
  exit 1
fi

if grep -Eiq 'recommended|not recommended|⭐' <<<"$table_html"; then
  echo "generated release notes contain recommendation wording or symbols" >&2
  exit 1
fi

if grep -Fq '## Download Guide — Pick the Right Build' "$output"; then
  echo "generated release notes retain the download-only heading" >&2
  exit 1
fi

if grep -Fq '<th colspan="5">Package managers and containers</th>' "$output"; then
  echo "Linux installation methods should remain in one platform group" >&2
  exit 1
fi

portable_line="$(grep -n 'alt="Archive file icon"' "$output" | cut -d: -f1)"
appimage_line="$(grep -n 'alt="AppImage logo"' "$output" | cut -d: -f1)"
nix_line="$(grep -n 'alt="NixOS logo"' "$output" | cut -d: -f1)"
docker_line="$(grep -n 'alt="Docker logo"' "$output" | cut -d: -f1)"
arch_line="$(grep -n 'alt="Arch Linux logo"' "$output" | cut -d: -f1)"
debian_line="$(grep -n 'alt="Debian logo"' "$output" | cut -d: -f1)"
fedora_line="$(grep -n 'alt="Fedora logo"' "$output" | cut -d: -f1)"
if ! ((portable_line < appimage_line \
  && appimage_line < nix_line \
  && nix_line < docker_line \
  && docker_line < arch_line \
  && arch_line < debian_line \
  && debian_line < fedora_line)); then
  echo "Linux methods are not ordered archive, AppImage, Nix, Docker, AUR, deb, rpm" >&2
  exit 1
fi

install_line="$(grep -n '^## Install Neomacs — Choose a Method$' "$output" | cut -d: -f1)"
details_open_line="$(grep -n '^<details>$' "$output" | cut -d: -f1)"
details_close_line="$(grep -n '^</details>$' "$output" | cut -d: -f1)"
contributors_line="$(grep -n '^## New Contributors$' "$output" | cut -d: -f1)"
if ! ((install_line < details_open_line \
  && details_open_line < details_close_line \
  && details_close_line < contributors_line)); then
  echo "What's Changed should follow installation methods and precede New Contributors" >&2
  exit 1
fi

missing_asset="neomacs-9.8.7-aarch64-pc-windows-msvc.zip"
rm "$dist_dir/$missing_asset"
if "$repo_root/scripts/generate-release-notes.sh" \
  --repo eval-exec/neomacs \
  --tag v9.8.7 \
  --dist-dir "$dist_dir" \
  --generated-notes "$generated_notes" \
  --output "$work_dir/incomplete-release-notes.md" \
  >"$work_dir/missing-asset.log" 2>&1; then
  echo "release-note generation accepted a missing asset: $missing_asset" >&2
  exit 1
fi
if ! grep -Fq "missing release asset: $missing_asset" "$work_dir/missing-asset.log"; then
  echo "missing-asset failure did not identify: $missing_asset" >&2
  exit 1
fi

echo "generated release notes match the installation-method contract"
