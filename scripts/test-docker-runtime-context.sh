#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
prepare_script="$repo_root/scripts/prepare-docker-runtime-context.sh"
runtime_dockerfile="$repo_root/docker/Dockerfile.runtime"
target_triple="x86_64-unknown-linux-gnu"
package_name="neomacs-9.8.7-$target_triple"
release_git="abcdef1234567890abcdef1234567890abcdef12"

mkdir -p "$repo_root/tmp"
work_dir="$(mktemp -d "$repo_root/tmp/docker-runtime-test.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT

make_release_fixture() {
  local fixture_root="$1"
  local fixture_target="$2"
  local root="$fixture_root/$package_name"

  mkdir -p "$root/bin" "$root/share/neomacs/lisp" "$root/share/neomacs/etc"
  printf '#!/bin/sh\nprintf "Neomacs fixture\\n"\n' >"$root/bin/neomacs"
  printf '#!/bin/sh\nexit 0\n' >"$root/bin/neomacsclient"
  chmod 0755 "$root/bin/neomacs" "$root/bin/neomacsclient"
  # The dump lives in the ARCHLIB, GNU's ${libexecdir}/emacs/${version}/
  # ${configuration}, not beside the binary.  This fixture said bin/ long after
  # packaging moved it, so the test stayed green while the real release archive
  # was rejected - the fixture was describing a layout that no longer shipped.
  mkdir -p "$root/libexec/neomacs/9.8.7/$fixture_target"
  printf 'portable dump fixture\n' \
    >"$root/libexec/neomacs/9.8.7/$fixture_target/neomacs.pdump"
  printf 'lisp fixture\n' >"$root/share/neomacs/lisp/loadup.el"
  printf 'etc fixture\n' >"$root/share/neomacs/etc/NEWS"
  printf 'name: neomacs\ntarget: %s\ngit: abcdef123456\nbuilt: 2026-08-30T00:00:00Z\n' \
    "$fixture_target" >"$root/VERSION"
}

fixture="$work_dir/fixture"
make_release_fixture "$fixture" "$target_triple"
archive="$work_dir/$package_name.tar.gz"
tar -C "$fixture" -czf "$archive" "$package_name"

context="$work_dir/context"
"$prepare_script" \
  --archive "$archive" \
  --target "$target_triple" \
  --release-git "$release_git" \
  --output "$context"

test -x "$context/rootfs/bin/neomacs"
test -x "$context/rootfs/bin/neomacsclient"
test -f "$context/rootfs/libexec/neomacs/9.8.7/$target_triple/neomacs.pdump"
test -d "$context/rootfs/share/neomacs/lisp"
test -d "$context/rootfs/share/neomacs/etc"
test -f "$context/rootfs/VERSION"
test ! -e "$context/rootfs/$package_name"

if "$prepare_script" \
  --archive "$archive" \
  --target "$target_triple" \
  --release-git "$release_git" \
  --output "$context" 2>"$work_dir/existing-output.err"
then
  echo "preparation unexpectedly overwrote an existing context" >&2
  exit 1
fi
grep -Fq 'output already exists' "$work_dir/existing-output.err"

wrong_target_fixture="$work_dir/wrong-target-fixture"
make_release_fixture "$wrong_target_fixture" "aarch64-unknown-linux-gnu"
wrong_target_archive="$work_dir/wrong-target/$package_name.tar.gz"
mkdir -p "$(dirname "$wrong_target_archive")"
tar -C "$wrong_target_fixture" -czf "$wrong_target_archive" "$package_name"
if "$prepare_script" \
  --archive "$wrong_target_archive" \
  --target "$target_triple" \
  --release-git "$release_git" \
  --output "$work_dir/wrong-target-context" 2>"$work_dir/wrong-target.err"
then
  echo "preparation accepted mismatched VERSION target metadata" >&2
  exit 1
fi
grep -Fq 'VERSION target does not match' "$work_dir/wrong-target.err"

if "$prepare_script" \
  --archive "$archive" \
  --target "$target_triple" \
  --release-git "fedcba9876543210fedcba9876543210fedcba98" \
  --output "$work_dir/wrong-git-context" 2>"$work_dir/wrong-git.err"
then
  echo "preparation accepted mismatched VERSION git metadata" >&2
  exit 1
fi
grep -Fq 'VERSION git identity does not match' "$work_dir/wrong-git.err"

extra_root="$work_dir/extra-root"
mkdir -p "$extra_root"
printf 'not part of the release\n' >"$extra_root/unexpected"
cp -a "$fixture/$package_name" "$extra_root/"
extra_archive="$work_dir/extra/$package_name.tar.gz"
mkdir -p "$(dirname "$extra_archive")"
tar -C "$extra_root" -czf "$extra_archive" "$package_name" unexpected
if "$prepare_script" \
  --archive "$extra_archive" \
  --target "$target_triple" \
  --release-git "$release_git" \
  --output "$work_dir/extra-context" 2>"$work_dir/extra-root.err"
then
  echo "preparation accepted an archive with an extra top-level entry" >&2
  exit 1
fi
grep -Fq 'outside the expected release root' "$work_dir/extra-root.err"

legacy_fixture="$work_dir/legacy-fixture/$package_name"
mkdir -p "$legacy_fixture/lisp" "$legacy_fixture/etc"
printf '#!/bin/sh\nexit 0\n' >"$legacy_fixture/neomacs"
printf '#!/bin/sh\nexit 0\n' >"$legacy_fixture/neomacsclient"
chmod 0755 "$legacy_fixture/neomacs" "$legacy_fixture/neomacsclient"
printf 'portable dump fixture\n' >"$legacy_fixture/neomacs.pdump"
chmod 0600 "$legacy_fixture/neomacs.pdump"
printf 'lisp fixture\n' >"$legacy_fixture/lisp/loadup.el"
printf 'etc fixture\n' >"$legacy_fixture/etc/NEWS"
printf 'license fixture\n' >"$legacy_fixture/COPYING"
legacy_archive="$work_dir/legacy/$package_name.tar.gz"
mkdir -p "$(dirname "$legacy_archive")"
tar -C "$(dirname "$legacy_fixture")" -czf "$legacy_archive" "$package_name"
legacy_context="$work_dir/legacy-context"
"$prepare_script" \
  --archive "$legacy_archive" \
  --target "$target_triple" \
  --release-git "$release_git" \
  --output "$legacy_context"
test -x "$legacy_context/rootfs/bin/neomacs"
test -f "$legacy_context/rootfs/bin/neomacs.pdump"
test "$(stat -c '%a' "$legacy_context/rootfs/bin/neomacs.pdump")" = 644
test -f "$legacy_context/rootfs/share/neomacs/lisp/loadup.el"
test -f "$legacy_context/rootfs/share/neomacs/etc/NEWS"
grep -Fxq "git: $release_git" "$legacy_context/rootfs/VERSION"
grep -Fxq 'source-layout: legacy-flat' "$legacy_context/rootfs/VERSION"

grep -Fq 'FROM ubuntu:22.04@sha256:' "$runtime_dockerfile"
grep -Fq 'COPY --chown=root:root rootfs/ /opt/neomacs/' "$runtime_dockerfile"
grep -Fq '/home/neomacs/.emacs.d' "$runtime_dockerfile"
grep -Fq 'USER neomacs' "$runtime_dockerfile"
grep -Fq 'ENTRYPOINT ["/opt/neomacs/bin/neomacs"]' "$runtime_dockerfile"
grep -Fq 'CMD ["-nw"]' "$runtime_dockerfile"

echo "Docker runtime context contract passed"
