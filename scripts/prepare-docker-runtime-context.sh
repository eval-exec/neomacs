#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/prepare-docker-runtime-context.sh \
  --archive FILE --target TRIPLE --release-git SHA --output DIR

Validate and extract one canonical Linux release tarball into a Docker build
context. The output directory must not already exist and will contain rootfs/.

All scratch data is created below ./tmp, never /tmp.
USAGE
}

archive=""
target_triple=""
release_git=""
output_dir=""

while (($#)); do
  case "$1" in
    --archive)
      archive="${2:?--archive requires a value}"
      shift 2
      ;;
    --target)
      target_triple="${2:?--target requires a value}"
      shift 2
      ;;
    --release-git)
      release_git="${2:?--release-git requires a value}"
      shift 2
      ;;
    --output)
      output_dir="${2:?--output requires a value}"
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

if [[ -z "$archive" || -z "$target_triple" || -z "$release_git" || -z "$output_dir" ]]; then
  usage >&2
  exit 2
fi
if [[ ! -f "$archive" ]]; then
  echo "release archive not found: $archive" >&2
  exit 1
fi
case "$target_triple" in
  x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu) ;;
  *)
    echo "unsupported Docker release target: $target_triple" >&2
    exit 1
    ;;
esac
if [[ ! "$release_git" =~ ^[0-9a-f]{40}$ ]]; then
  echo "release git identity must be a full commit SHA: $release_git" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
archive="$(cd "$(dirname "$archive")" && pwd)/$(basename "$archive")"
archive_name="$(basename "$archive")"
case "$archive_name" in
  neomacs-*-${target_triple}.tar.gz) ;;
  *)
    echo "archive name does not identify target $target_triple: $archive_name" >&2
    exit 1
    ;;
esac
package_name="${archive_name%.tar.gz}"

if [[ -e "$output_dir" || -L "$output_dir" ]]; then
  echo "output already exists: $output_dir" >&2
  exit 1
fi
output_parent="$(dirname "$output_dir")"
output_name="$(basename "$output_dir")"
mkdir -p "$output_parent"
output_parent="$(cd "$output_parent" && pwd)"
output_dir="$output_parent/$output_name"
if [[ -e "$output_dir" || -L "$output_dir" ]]; then
  echo "output already exists: $output_dir" >&2
  exit 1
fi

mkdir -p "$repo_root/tmp"
staging="$(mktemp -d "$repo_root/tmp/docker-runtime-context.XXXXXX")"
cleanup() {
  rm -rf "$staging"
}
trap cleanup EXIT

member_count=0
while IFS= read -r member; do
  normalized="${member#./}"
  [[ -n "$normalized" ]] || continue
  member_count=$((member_count + 1))
  if [[ "$normalized" == /* || "/$normalized/" == *"/../"* ]]; then
    echo "unsafe archive member: $member" >&2
    exit 1
  fi
  member_root="${normalized%%/*}"
  if [[ "$member_root" != "$package_name" ]]; then
    echo "archive member is outside the expected release root $package_name: $member" >&2
    exit 1
  fi
done < <(tar -tzf "$archive")

if ((member_count == 0)); then
  echo "release archive is empty: $archive" >&2
  exit 1
fi

mkdir -p "$staging/rootfs"
tar -C "$staging/rootfs" \
  --extract --gzip --file "$archive" \
  --strip-components=1 --no-same-owner --delay-directory-restore

rootfs="$staging/rootfs"

# v0.0.15's arm64 asset predates the canonical bin/share release layout. Its
# checksum is still authoritative, so normalize that one legacy shape at the
# packaging boundary instead of teaching the runtime image two filesystem
# layouts. Reject mixed or extended legacy roots before moving anything.
if [[ -x "$rootfs/bin/neomacs" ]]; then
  archive_layout=canonical
elif [[ -x "$rootfs/neomacs" \
  && -x "$rootfs/neomacsclient" \
  && -f "$rootfs/neomacs.pdump" \
  && -d "$rootfs/lisp" \
  && -d "$rootfs/etc" ]]; then
  archive_layout=legacy-flat
else
  echo "release archive has neither the canonical nor supported legacy layout" >&2
  exit 1
fi

if [[ "$archive_layout" == legacy-flat ]]; then
  while IFS= read -r -d '' legacy_entry; do
    case "$(basename "$legacy_entry")" in
      COPYING|etc|lisp|neomacs|neomacs.pdump|neomacsclient) ;;
      *)
        echo "legacy release archive has an unexpected root entry: ${legacy_entry#"$rootfs/"}" >&2
        exit 1
        ;;
    esac
  done < <(find "$rootfs" -mindepth 1 -maxdepth 1 -print0)

  normalized_rootfs="$staging/normalized-rootfs"
  mkdir -p "$normalized_rootfs/bin" "$normalized_rootfs/share/neomacs"
  mv "$rootfs/neomacs" "$normalized_rootfs/bin/neomacs"
  mv "$rootfs/neomacsclient" "$normalized_rootfs/bin/neomacsclient"
  mv "$rootfs/neomacs.pdump" "$normalized_rootfs/bin/neomacs.pdump"
  mv "$rootfs/lisp" "$normalized_rootfs/share/neomacs/lisp"
  mv "$rootfs/etc" "$normalized_rootfs/share/neomacs/etc"
  if [[ -f "$rootfs/COPYING" ]]; then
    mv "$rootfs/COPYING" "$normalized_rootfs/COPYING"
  fi
  printf 'name: neomacs\ntarget: %s\ngit: %s\nsource-layout: legacy-flat\n' \
    "$target_triple" "$release_git" >"$normalized_rootfs/VERSION"
  rmdir "$rootfs"
  mv "$normalized_rootfs" "$rootfs"
fi

# The dump's location depends on the layout, so it cannot be a fixed path here.
# A canonical archive puts it in the ARCHLIB - GNU's
# ${libexecdir}/emacs/${version}/${configuration} (configure.ac:290) - which is
# where the binary resolves it from via exec-directory.  Only the legacy-flat
# shape, normalized above, keeps it beside the binary.  This check named
# bin/neomacs.pdump unconditionally and so rejected every archive built after
# the dump moved.
if [[ "$archive_layout" == legacy-flat ]]; then
  dump_relative="bin/neomacs.pdump"
else
  dump_relative=""
  while IFS= read -r -d '' candidate; do
    dump_relative="${candidate#"$rootfs"/}"
    break
  done < <(find "$rootfs/libexec" -type f -name neomacs.pdump -print0 2>/dev/null | sort -z)
  if [[ -z "$dump_relative" ]]; then
    echo "release archive has no portable dump under libexec/ (the archlib)" >&2
    exit 1
  fi
fi

for required_file in \
  "$rootfs/bin/neomacs" \
  "$rootfs/bin/neomacsclient" \
  "$rootfs/$dump_relative" \
  "$rootfs/VERSION"
do
  if [[ ! -f "$required_file" ]]; then
    echo "release archive is missing required file: ${required_file#"$rootfs/"}" >&2
    exit 1
  fi
done
for required_dir in "$rootfs/share/neomacs/lisp" "$rootfs/share/neomacs/etc"; do
  if [[ ! -d "$required_dir" ]]; then
    echo "release archive is missing required directory: ${required_dir#"$rootfs/"}" >&2
    exit 1
  fi
done
for required_executable in "$rootfs/bin/neomacs" "$rootfs/bin/neomacsclient"; do
  if [[ ! -x "$required_executable" ]]; then
    echo "release binary is not executable: ${required_executable#"$rootfs/"}" >&2
    exit 1
  fi
done
if ! grep -Fxq "target: $target_triple" "$rootfs/VERSION"; then
  echo "VERSION target does not match requested target $target_triple" >&2
  exit 1
fi
embedded_git="$(sed -n 's/^git: //p' "$rootfs/VERSION")"
if [[ ! "$embedded_git" =~ ^[0-9a-f]{7,40}$ \
  || "$release_git" != "$embedded_git"* ]]; then
  echo "VERSION git identity does not match release commit $release_git" >&2
  exit 1
fi

unexpected_type="$(find "$rootfs" \
  ! -type f ! -type d ! -type l -print -quit)"
if [[ -n "$unexpected_type" ]]; then
  echo "release archive contains an unsupported file type: ${unexpected_type#"$rootfs/"}" >&2
  exit 1
fi

while IFS= read -r -d '' link; do
  resolved="$(realpath -m -- "$link")"
  case "$resolved" in
    "$rootfs"|"$rootfs"/*) ;;
    *)
      echo "release symlink escapes the runtime root: ${link#"$rootfs/"}" >&2
      exit 1
      ;;
  esac
done < <(find "$rootfs" -type l -print0)

# Release payloads are public application data copied into the image as root.
# Preserve executable bits, but guarantee that the non-root runtime user can
# traverse directories and read binaries, the dump, Lisp, and resources. The
# legacy arm64 v0.0.15 dump was archived as 0600.
find "$rootfs" -type d -exec chmod a+rx {} +
find "$rootfs" -type f -exec chmod a+r {} +

mv "$staging" "$output_dir"
trap - EXIT
echo "prepared Docker runtime context at $output_dir"
