#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/neomacs-info-manuals-test.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

# Exercise real packager preflight without building or executing an editor.
package_root="$scratch/package"
mkdir -p "$package_root/scripts/lib" "$package_root/target/release" \
  "$package_root/lisp" "$package_root/etc" "$package_root/leim" "$scratch/bin"
# The macOS preflight is portable shell; keep its host identity deterministic
# when this test runs on Linux ARM, whose architecture is named aarch64.
cat >"$scratch/bin/uname" <<'UNAME'
#!/usr/bin/env bash
case "$1" in
  -m) printf 'x86_64\n' ;;
  -s) printf 'Darwin\n' ;;
  *) exit 2 ;;
esac
UNAME
chmod +x "$scratch/bin/uname"
export PATH="$scratch/bin:$PATH"
cp "$repo_root/scripts/package-release.sh" "$package_root/scripts/"
cp "$repo_root/scripts/package-macos-app.sh" "$package_root/scripts/"
cp "$repo_root/scripts/lib/archlib.sh" "$package_root/scripts/lib/"
cp "$repo_root/scripts/lib/macos-tarball.sh" "$package_root/scripts/lib/"
if [[ -f "$repo_root/scripts/lib/info-manuals.sh" ]]; then
  cp "$repo_root/scripts/lib/info-manuals.sh" "$package_root/scripts/lib/"
fi
printf '[workspace.package]\nversion = "0.0.0"\n' >"$package_root/Cargo.toml"
printf 'fixture\n' >"$package_root/README.md"
printf 'fixture\n' >"$package_root/COPYING"
for binary in neomacs neomacsclient neomacs.pdump; do
  printf 'unused fixture artifact\n' >"$package_root/target/release/$binary"
done

expect_packager_failure() {
  local script="$1"
  shift
  if env -u MACOS_SIGNING_IDENTITY -u MACOS_NOTARY_KEY_PATH \
    -u MACOS_NOTARY_KEY_ID -u MACOS_NOTARY_ISSUER_ID \
    MACOS_DISTRIBUTION_MODE=adhoc \
    NEOMACS_RELEASE_DIR="$package_root/target/release" \
    bash "$package_root/scripts/$script" --skip-build --no-smoke "$@" \
    >"$scratch/package.log" 2>&1; then
    echo "$script accepted missing Info manuals with --skip-build" >&2
    exit 1
  fi
  if ! grep -Fq 'cargo xtask build-info --binary' "$scratch/package.log"; then
    cat "$scratch/package.log" >&2
    echo "$script did not explain how to rebuild Info manuals" >&2
    exit 1
  fi
  [[ ! -d "$package_root/dist" ]]
}

expect_packager_failure package-release.sh --target x86_64-apple-darwin
expect_packager_failure package-macos-app.sh

# shellcheck source=./scripts/lib/info-manuals.sh
source "$repo_root/scripts/lib/info-manuals.sh"

info_dir="$scratch/info"
make_valid_info() {
  rm -rf "$info_dir"
  mkdir -p "$info_dir"
  printf 'Info directory fixture\n' >"$info_dir/dir"
  printf '%s\n' emacs.info elisp.info eintr.info cl.info org.info modus-themes.info \
    extra_manual-1.info >"$info_dir/.neomacs-info-manifest"
  while IFS= read -r manual; do
    printf 'Info manual fixture\n' >"$info_dir/$manual"
  done <"$info_dir/.neomacs-info-manifest"
}

expect_invalid_info() {
  local label="$1" rebuild
  if neomacs_validate_info_manuals "$info_dir" "$scratch/custom release/neomacs" \
    >"$scratch/validator.log" 2>&1; then
    echo "validator accepted $label" >&2
    exit 1
  fi
  printf -v rebuild 'cargo xtask build-info --binary %q' "$scratch/custom release/neomacs"
  grep -Fq "$rebuild" "$scratch/validator.log"
}

expect_invalid_info 'a missing Info directory'
make_valid_info
neomacs_validate_info_manuals "$info_dir"

rm "$info_dir/.neomacs-info-manifest"
expect_invalid_info 'a missing manifest'
make_valid_info
: >"$info_dir/.neomacs-info-manifest"
expect_invalid_info 'an empty manifest'
make_valid_info
printf '\n' >>"$info_dir/.neomacs-info-manifest"
expect_invalid_info 'an empty manifest entry'

for unsafe in '../outside.info' '/absolute.info' '-option.info' 'bad name.info' 'manual.txt'; do
  make_valid_info
  printf '%s\n' "$unsafe" >>"$info_dir/.neomacs-info-manifest"
  expect_invalid_info "unsafe manifest entry $unsafe"
done

for required in dir emacs.info elisp.info eintr.info cl.info org.info modus-themes.info extra_manual-1.info; do
  make_valid_info
  rm "$info_dir/$required"
  expect_invalid_info "missing $required"
  make_valid_info
  : >"$info_dir/$required"
  expect_invalid_info "empty $required"
done

for required in emacs.info elisp.info eintr.info cl.info org.info modus-themes.info; do
  make_valid_info
  grep -Fxv "$required" "$info_dir/.neomacs-info-manifest" >"$scratch/manifest"
  mv "$scratch/manifest" "$info_dir/.neomacs-info-manifest"
  expect_invalid_info "$required omitted from the manifest"
done

make_valid_info
rm "$info_dir/extra_manual-1.info"
mkdir "$info_dir/extra_manual-1.info"
expect_invalid_info 'a directory in place of a manual'

# A complete tree must survive the real archive staging and extraction path.
make_valid_info
cp -a "$info_dir" "$package_root/info"
NEOMACS_RELEASE_DIR="$package_root/target/release" \
  bash "$package_root/scripts/package-release.sh" \
    --target x86_64-apple-darwin --skip-build --no-smoke \
    >"$scratch/package.log" 2>&1
mkdir "$scratch/extracted"
archives=("$package_root"/dist/neomacs-*-x86_64-apple-darwin.tar.gz)
[[ ${#archives[@]} == 1 ]]
tar -xzf "${archives[0]}" -C "$scratch/extracted"
archive_name="${archives[0]##*/}"
neomacs_validate_info_manuals \
  "$scratch/extracted/${archive_name%.tar.gz}/share/neomacs/info"

echo 'Info validator and packager tests passed'
