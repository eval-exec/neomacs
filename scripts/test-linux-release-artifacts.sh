#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/test-linux-release-artifacts.sh [options]

Extract, smoke-test, and audit packaged NEO Emacs Linux artifacts.

Options:
  --dist DIR       Artifact directory. Defaults to ./dist.
  --target TRIPLE  Rust target triple. Defaults to x86_64-unknown-linux-gnu.
  --tar-version V  Version component of the tarball name. Defaults to any version.
  --formats LIST   Comma-separated formats. Defaults to
                   tar,appimage,deb,rpm.
  --glibc VERSION  Maximum permitted GLIBC symbol version. Defaults to 2.35.
  -h, --help       Show this help.

All scratch data is created below ./tmp, never /tmp.
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=./scripts/lib/archlib.sh
source "$repo_root/scripts/lib/archlib.sh"
dist_dir="$repo_root/dist"
target_triple="x86_64-unknown-linux-gnu"
tar_version="*"
formats="tar,appimage,deb,rpm"
glibc_baseline="2.35"

while (($#)); do
  case "$1" in
    --dist)
      dist_dir="${2:?--dist requires a value}"
      shift 2
      ;;
    --target)
      target_triple="${2:?--target requires a value}"
      shift 2
      ;;
    --tar-version)
      tar_version="${2:?--tar-version requires a value}"
      shift 2
      ;;
    --formats)
      formats="${2:?--formats requires a value}"
      shift 2
      ;;
    --glibc)
      glibc_baseline="${2:?--glibc requires a value}"
      shift 2
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

if [[ ! -d "$dist_dir" ]]; then
  echo "artifact directory not found: $dist_dir" >&2
  exit 1
fi
dist_dir="$(cd "$dist_dir" && pwd)"

case "$target_triple" in
  x86_64-*)
    deb_arch="amd64"
    rpm_arch="x86_64"
    ;;
  aarch64-*)
    deb_arch="arm64"
    rpm_arch="aarch64"
    ;;
  armv7-*)
    deb_arch="armhf"
    rpm_arch="armv7hl"
    ;;
  *)
    echo "unsupported Linux target: $target_triple" >&2
    exit 1
    ;;
esac

mkdir -p "$repo_root/tmp"
work_dir="$(mktemp -d "$repo_root/tmp/linux-artifact-test.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT

find_one() {
  local label="$1"
  local pattern="$2"
  local matches=()
  mapfile -t matches < <(find "$dist_dir" -maxdepth 1 -type f -name "$pattern" -print | sort)
  if ((${#matches[@]} != 1)); then
    echo "expected exactly one $label matching $dist_dir/$pattern; found ${#matches[@]}" >&2
    printf '  %s\n' "${matches[@]}" >&2
    return 1
  fi
  printf '%s\n' "${matches[0]}"
}

version_is_newer() {
  local candidate="$1"
  [[ "$candidate" != "$glibc_baseline" \
    && "$(printf '%s\n%s\n' "$candidate" "$glibc_baseline" | sort -V | tail -n 1)" == "$candidate" ]]
}

audit_elf() {
  local file="$1"
  local required
  while IFS= read -r required; do
    if version_is_newer "$required"; then
      echo "$file requires GLIBC_$required, newer than GLIBC_$glibc_baseline" >&2
      return 1
    fi
  done < <(
    readelf --version-info "$file" 2>/dev/null \
      | sed -n 's/.*Name: GLIBC_\([0-9][0-9.]*\).*/\1/p' \
      | sort -Vu
  )
}

audit_tree() {
  local root="$1"
  local file
  while IFS= read -r -d '' file; do
    if readelf --file-header "$file" &>/dev/null; then
      audit_elf "$file"
    fi
  done < <(find "$root" -type f -print0)
}

audit_desktop_identity() {
  local prefix="$1"
  local desktop="$prefix/share/applications/neomacs.desktop"
  local icon="$prefix/share/icons/hicolor/scalable/apps/neomacs.svg"

  cmp "$desktop" "$repo_root/crates/neomacs-display-runtime/assets/neomacs.desktop"
  cmp "$icon" "$repo_root/crates/neomacs-display-runtime/assets/window-icon.svg"
  grep -Fxq 'Exec=neomacs %F' "$desktop"
  grep -Fxq 'Icon=neomacs' "$desktop"
}

smoke_binary() {
  local binary="$1"
  local runtime_root="$2"
  echo "smoke-testing $binary"
  NEOMACS_RUNTIME_ROOT="$runtime_root" \
    timeout 30s "$binary" --batch --eval "(kill-emacs 0)"
}

audit_linked_video_backend() {
  local prefix="$1" binary="$2"
  if find "$prefix" -type f -name 'libneomacs_video_gstreamer.so' -print -quit \
    | grep -q .; then
    echo "release contains obsolete private GStreamer adapter" >&2
    return 1
  fi
  if ! readelf --dynamic "$binary" 2>/dev/null | grep -Eq 'Shared library: \[libgstreamer-1[.]0[.]so'; then
    echo "full executable does not link GStreamer: $binary" >&2
    return 1
  fi
}

# The smoke test above already fails if the dump cannot be found, but it fails
# as "the editor did not start".  This says WHICH directory the artifact
# staged and which one the binary looked in, which is the difference between a
# five-minute fix and an afternoon.
audit_archlib() {
  local prefix="$1" binary="$2" runtime_root="$3" archlib_rel
  archlib_rel="$(neomacs_archlib_relpath "$repo_root/Cargo.toml" "$target_triple")"
  if [[ ! -d "$prefix/$archlib_rel" ]]; then
    echo "artifact has no archlib at $prefix/$archlib_rel" >&2
    return 1
  fi
  neomacs_verify_archlib \
    "$binary" \
    "$prefix/$archlib_rel/neomacs.pdump" \
    "$prefix/$archlib_rel" \
    "$runtime_root"
  audit_linked_video_backend "$prefix" "$binary"
}

test_tar() {
  local artifact root package_root binary runtime_root
  artifact="$(find_one tarball "neomacs-${tar_version}-${target_triple}.tar.gz")"
  root="$work_dir/tar"
  mkdir -p "$root"
  tar -C "$root" -xzf "$artifact"
  package_root="$(find "$root" -mindepth 1 -maxdepth 1 -type d -print -quit)"
  test -n "$package_root"

  if [[ -x "$package_root/bin/neomacs" ]]; then
    binary="$package_root/bin/neomacs"
    runtime_root="$package_root/share/neomacs"
  else
    binary="$package_root/neomacs"
    runtime_root="$package_root"
  fi
  smoke_binary "$binary" "$runtime_root"
  if [[ -x "$package_root/bin/neomacs" ]]; then
    audit_archlib "$package_root" "$binary" "$runtime_root"
  fi
  audit_desktop_identity "$package_root"
  audit_tree "$package_root"
}

test_appimage() {
  local artifact root extracted
  artifact="$(find_one AppImage "neomacs-*-${target_triple}.AppImage")"
  audit_elf "$artifact"
  root="$work_dir/appimage"
  mkdir -p "$root"
  (
    cd "$root"
    "$artifact" --appimage-extract >/dev/null
  )
  extracted="$root/squashfs-root"
  test -x "$extracted/AppRun"
  audit_archlib "$extracted/usr" "$extracted/usr/bin/neomacs" \
    "$extracted/usr/share/neomacs"
  audit_desktop_identity "$extracted/usr"
  audit_tree "$extracted"
  echo "smoke-testing $artifact"
  env -u NEOMACS_RUNTIME_ROOT \
    APPIMAGE_EXTRACT_AND_RUN=1 \
    TMPDIR="$work_dir" \
    timeout 30s "$artifact" --batch --eval "(kill-emacs 0)"
}

test_deb() {
  local artifact root
  artifact="$(find_one Debian-package "neomacs_*_${deb_arch}.deb")"
  root="$work_dir/deb"
  mkdir -p "$root"
  dpkg-deb --extract "$artifact" "$root"
  smoke_binary "$root/usr/bin/neomacs" "$root/usr/share/neomacs"
  audit_archlib "$root/usr" "$root/usr/bin/neomacs" "$root/usr/share/neomacs"
  audit_desktop_identity "$root/usr"
  audit_tree "$root"
}

test_rpm() {
  local artifact root
  artifact="$(find_one RPM-package "neomacs-*-1.${rpm_arch}.rpm")"
  root="$work_dir/rpm"
  mkdir -p "$root"
  rpm2cpio "$artifact" | (
    cd "$root"
    cpio --extract --make-directories --quiet
  )
  smoke_binary "$root/usr/bin/neomacs" "$root/usr/share/neomacs"
  audit_archlib "$root/usr" "$root/usr/bin/neomacs" "$root/usr/share/neomacs"
  audit_desktop_identity "$root/usr"
  audit_tree "$root"
}

IFS=',' read -r -a requested_formats <<<"$formats"
for format in "${requested_formats[@]}"; do
  case "$format" in
    tar) test_tar ;;
    appimage) test_appimage ;;
    deb) test_deb ;;
    rpm) test_rpm ;;
    *)
      echo "unsupported artifact format: $format" >&2
      exit 1
      ;;
  esac
done

echo "verified $formats artifacts for $target_triple at GLIBC_$glibc_baseline or older"
