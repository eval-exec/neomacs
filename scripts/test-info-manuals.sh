#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/neomacs-info-manuals-test.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

# Exercise real packager preflight without building or executing an editor.
package_root="$scratch/package"
mkdir -p "$package_root/scripts/lib" "$package_root/target/release" \
  "$package_root/lisp" "$package_root/etc" "$package_root/leim" \
  "$package_root/docs" "$scratch/bin"
cp "$repo_root/docs/info-manuals.list" "$package_root/docs/"
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
    echo "$script accepted invalid Info manuals with --skip-build" >&2
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
  cp "$repo_root/docs/info-manuals.list" "$info_dir/.neomacs-info-manifest"
  printf 'Info directory fixture\n\037\nFile: dir,\tNode: Top\tThe Info directory\n\n* Menu:\n\n' >"$info_dir/dir"
  while IFS= read -r manual; do
    printf 'Info manual fixture\n' >"$info_dir/$manual"
    printf '* %s: (%s). Manual\n    fixture.\n' "${manual%.info}" "${manual%.info}" >>"$info_dir/dir"
  done <"$info_dir/.neomacs-info-manifest"
}

expect_invalid_info() {
  local label="$1" diagnostic="${2:-}" rebuild
  if neomacs_validate_info_manuals "$info_dir" "$scratch/custom release/neomacs" \
    >"$scratch/validator.log" 2>&1; then
    echo "validator accepted $label" >&2
    exit 1
  fi
  printf -v rebuild 'cargo xtask build-info --binary %q' "$scratch/custom release/neomacs"
  grep -Fq "$rebuild" "$scratch/validator.log"
  if [[ -n "$diagnostic" ]] && ! grep -Fq "$diagnostic" "$scratch/validator.log"; then
    cat "$scratch/validator.log" >&2
    echo "validator rejected $label without expected diagnostic: $diagnostic" >&2
    exit 1
  fi
}

expect_invalid_info 'a missing Info directory'
make_valid_info
neomacs_validate_info_manuals "$info_dir"

# Generated Info trees contain regular files, never links. Even a link whose
# target currently exists can become dangling when cp -a stages the package.
mv "$info_dir" "$scratch/external-info"
ln -s "$scratch/external-info" "$info_dir"
expect_invalid_info 'a symlinked Info directory' 'symlinks are not allowed'
info_dir="$scratch/info///"
expect_invalid_info 'a symlinked Info directory with trailing slashes' 'symlinks are not allowed'
info_dir="$scratch/info"
for linked in .neomacs-info-manifest dir wisent.info; do
  make_valid_info
  mv "$info_dir/$linked" "$scratch/external-$linked"
  ln -s "$scratch/external-$linked" "$info_dir/$linked"
  expect_invalid_info "an external symlink for $linked" 'symlinks are not allowed'
  make_valid_info
  mv "$info_dir/$linked" "$info_dir/internal-$linked"
  ln -s "internal-$linked" "$info_dir/$linked"
  expect_invalid_info "an internal symlink for $linked" 'symlinks are not allowed'
done
make_valid_info
ln -s "$scratch/missing-target" "$info_dir/extra-asset"
expect_invalid_info 'a dangling ancillary asset symlink' 'symlinks are not allowed'

make_valid_info
rm "$info_dir/.neomacs-info-manifest"
expect_invalid_info 'a missing manifest'
make_valid_info
: >"$info_dir/.neomacs-info-manifest"
expect_invalid_info 'an empty manifest'
make_valid_info
printf '\n' >>"$info_dir/.neomacs-info-manifest"
expect_invalid_info 'an empty manifest entry' 'invalid manual basename'

for unsafe in '../outside.info' '/absolute.info' '-option.info' 'bad name.info' 'manual.txt'; do
  make_valid_info
  printf '%s\n' "$unsafe" >>"$info_dir/.neomacs-info-manifest"
  expect_invalid_info "unsafe manifest entry $unsafe" 'invalid manual basename'
done

for required in dir emacs.info elisp.info eintr.info cl.info org.info modus-themes.info wisent.info; do
  make_valid_info
  rm "$info_dir/$required"
  expect_invalid_info "missing $required"
  make_valid_info
  : >"$info_dir/$required"
  expect_invalid_info "empty $required"
done

for required in emacs.info elisp.info eintr.info cl.info org.info modus-themes.info wisent.info; do
  make_valid_info
  grep -Fxv "$required" "$info_dir/.neomacs-info-manifest" >"$scratch/manifest"
  mv "$scratch/manifest" "$info_dir/.neomacs-info-manifest"
  expect_invalid_info "$required omitted from the manifest" 'manifest does not match canonical inventory'
done

make_valid_info
printf '%s\n' extra_manual-1.info >>"$info_dir/.neomacs-info-manifest"
printf 'Extra manual fixture\n' >"$info_dir/extra_manual-1.info"
expect_invalid_info 'an unexpected manifest entry' 'manifest does not match canonical inventory'
make_valid_info
printf '%s\n' emacs.info >>"$info_dir/.neomacs-info-manifest"
expect_invalid_info 'a duplicate manifest entry' 'manifest does not match canonical inventory'

# Inventory order is immaterial, but every manual must occur exactly once.
make_valid_info
LC_ALL=C sort -r "$info_dir/.neomacs-info-manifest" >"$scratch/manifest"
mv "$scratch/manifest" "$info_dir/.neomacs-info-manifest"
neomacs_validate_info_manuals "$info_dir"

make_valid_info
rm "$info_dir/wisent.info"
mkdir "$info_dir/wisent.info"
expect_invalid_info 'a directory in place of a manual'

# Package preflight validates the generated install-info format without
# executing an editor, including in --skip-build packaging.
make_valid_info
printf 'not an Info index\n' >"$info_dir/dir"
expect_invalid_info 'arbitrary nonempty index text'
make_valid_info
sed '/File: dir/d' "$info_dir/dir" >"$scratch/dir"
mv "$scratch/dir" "$info_dir/dir"
expect_invalid_info 'an index without a node header'
make_valid_info
sed 's/File: dir/File: other/' "$info_dir/dir" >"$scratch/dir"
mv "$scratch/dir" "$info_dir/dir"
expect_invalid_info 'an index with the wrong file header'
make_valid_info
sed 's/Node: Top/Node: Topical/' "$info_dir/dir" >"$scratch/dir"
mv "$scratch/dir" "$info_dir/dir"
expect_invalid_info 'an index without a Top node'
make_valid_info
tr -d '\037' <"$info_dir/dir" >"$scratch/dir"
mv "$scratch/dir" "$info_dir/dir"
expect_invalid_info 'an index without an Info node separator'
make_valid_info
sed '/^\* Menu:/d' "$info_dir/dir" >"$scratch/dir"
mv "$scratch/dir" "$info_dir/dir"
expect_invalid_info 'a Top node without a menu'
make_valid_info
sed '/^\* wisent:/d' "$info_dir/dir" >"$scratch/dir"
mv "$scratch/dir" "$info_dir/dir"
expect_invalid_info 'a Top menu missing a canonical manual target'
make_valid_info
sed 's/(wisent)\./(wisent/' "$info_dir/dir" >"$scratch/dir"
mv "$scratch/dir" "$info_dir/dir"
expect_invalid_info 'an unterminated manual reference'
make_valid_info
sed 's/(wisent)\./(\/host\/wisent)./' "$info_dir/dir" >"$scratch/dir"
mv "$scratch/dir" "$info_dir/dir"
expect_invalid_info 'a host path in place of a local manual target'
make_valid_info
sed 's/Node: Top/Node: Other/' "$info_dir/dir" >"$scratch/dir"
printf '\037\nFile: dir,\tNode: Top\tThe Info directory\n\n* Menu:\n' >>"$scratch/dir"
mv "$scratch/dir" "$info_dir/dir"
expect_invalid_info 'manual targets present only in another node'
make_valid_info
cat "$info_dir/dir" >"$scratch/dir"
cat "$scratch/dir" >>"$info_dir/dir"
expect_invalid_info 'multiple Top nodes'

# Both forms are accepted by GNU Info; descriptions can wrap onto indented
# continuation lines and are not part of the target filename.
make_valid_info
sed 's/(wisent)\./(wisent.info)Top./' "$info_dir/dir" >"$scratch/dir"
mv "$scratch/dir" "$info_dir/dir"
neomacs_validate_info_manuals "$info_dir"
# Native install-info can use platform line endings without changing the
# controlled LF manifest. Source-time lookup also survives a caller chdir.
awk '{ printf "%s\r\n", $0 }' "$info_dir/dir" >"$scratch/dir"
mv "$scratch/dir" "$info_dir/dir"
saved_directory="$PWD"
cd "$repo_root"
source scripts/lib/info-manuals.sh
cd "$scratch"
neomacs_validate_info_manuals "$info_dir"
cd "$saved_directory"

# Neither package entry point may bypass full validation with --skip-build.
make_valid_info
cp -a "$info_dir" "$package_root/info"
printf 'not an Info index\n' >"$package_root/info/dir"
expect_packager_failure package-release.sh --target x86_64-apple-darwin
expect_packager_failure package-macos-app.sh
rm -rf "$package_root/info"
make_valid_info
grep -Fxv wisent.info "$info_dir/.neomacs-info-manifest" >"$scratch/manifest"
mv "$scratch/manifest" "$info_dir/.neomacs-info-manifest"
cp -a "$info_dir" "$package_root/info"
expect_packager_failure package-release.sh --target x86_64-apple-darwin
expect_packager_failure package-macos-app.sh
rm -rf "$package_root/info"

# Check every symlink entry point before either packager creates staging data.
for linked in info .neomacs-info-manifest dir wisent.info; do
  make_valid_info
  if [[ "$linked" == info ]]; then
    ln -s "$info_dir" "$package_root/info"
  else
    cp -a "$info_dir" "$package_root/info"
    rm "$package_root/info/$linked"
    ln -s "$info_dir/$linked" "$package_root/info/$linked"
  fi
  expect_packager_failure package-release.sh --target x86_64-apple-darwin
  expect_packager_failure package-macos-app.sh
  rm -rf "$package_root/info"
done

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
