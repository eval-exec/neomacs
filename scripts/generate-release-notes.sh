#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/generate-release-notes.sh --repo OWNER/REPO --tag TAG
       --dist-dir DIR --generated-notes FILE --output FILE

Generate a release body containing installation methods and GitHub's generated
changelog. Package links are derived from TAG and checked against DIR. The
What's Changed section from FILE is collapsed by default.
USAGE
}

repository=""
tag=""
dist_dir=""
generated_notes=""
output=""

while (($#)); do
  case "$1" in
    --repo)
      repository="${2:?--repo requires a value}"
      shift 2
      ;;
    --tag)
      tag="${2:?--tag requires a value}"
      shift 2
      ;;
    --dist-dir)
      dist_dir="${2:?--dist-dir requires a value}"
      shift 2
      ;;
    --generated-notes)
      generated_notes="${2:?--generated-notes requires a value}"
      shift 2
      ;;
    --output)
      output="${2:?--output requires a value}"
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

if [[ -z "$repository" || -z "$tag" || -z "$dist_dir" || -z "$generated_notes" || -z "$output" ]]; then
  usage >&2
  exit 2
fi
if [[ ! "$repository" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
  echo "invalid GitHub repository: $repository" >&2
  exit 2
fi
if [[ ! "$tag" =~ ^v[0-9][A-Za-z0-9._-]*$ ]]; then
  echo "release tag must start with v followed by a version: $tag" >&2
  exit 2
fi
if [[ ! -d "$dist_dir" ]]; then
  echo "release artifact directory not found: $dist_dir" >&2
  exit 1
fi
if [[ ! -f "$generated_notes" ]]; then
  echo "generated GitHub release notes not found: $generated_notes" >&2
  exit 1
fi
if ! grep -Fxq "## What's Changed" "$generated_notes"; then
  echo "generated GitHub release notes have no What's Changed section: $generated_notes" >&2
  exit 1
fi

version="${tag#v}"
release_base="https://github.com/$repository/releases/download/$tag"

linux_x86_appimage="neomacs-$version-x86_64-unknown-linux-gnu.AppImage"
linux_arm_appimage="neomacs-$version-aarch64-unknown-linux-gnu.AppImage"
linux_x86_deb="neomacs_${version}_amd64.deb"
linux_arm_deb="neomacs_${version}_arm64.deb"
linux_x86_rpm="neomacs-$version-1.x86_64.rpm"
linux_arm_rpm="neomacs-$version-1.aarch64.rpm"
linux_x86_tarball="neomacs-$version-x86_64-unknown-linux-gnu.tar.gz"
linux_arm_tarball="neomacs-$version-aarch64-unknown-linux-gnu.tar.gz"
macos_dmg="neomacs-$version-aarch64-apple-darwin.dmg"
macos_zip="neomacs-$version-aarch64-apple-darwin.zip"
macos_tarball="neomacs-$version-aarch64-apple-darwin.tar.gz"
windows_x86_installer="neomacs-$version-x86_64-pc-windows-msvc-user-setup.exe"
windows_x86_zip="neomacs-$version-x86_64-pc-windows-msvc.zip"
windows_arm_installer="neomacs-$version-aarch64-pc-windows-msvc-user-setup.exe"
windows_arm_zip="neomacs-$version-aarch64-pc-windows-msvc.zip"

required_assets=(
  install.sh
  SHA256SUMS
  "$linux_x86_appimage"
  "$linux_arm_appimage"
  "$linux_x86_deb"
  "$linux_arm_deb"
  "$linux_x86_rpm"
  "$linux_arm_rpm"
  "$linux_x86_tarball"
  "$linux_arm_tarball"
  "$macos_dmg"
  "$macos_zip"
  "$macos_tarball"
  "$windows_x86_installer"
  "$windows_x86_zip"
  "$windows_arm_installer"
  "$windows_arm_zip"
)

missing_assets=0
for asset in "${required_assets[@]}"; do
  if [[ ! -f "$dist_dir/$asset" ]]; then
    echo "missing release asset: $asset" >&2
    missing_assets=$((missing_assets + 1))
  fi
done
if ((missing_assets > 0)); then
  exit 1
fi

write_installation_methods() {
  cat <<HTML
## Install Neomacs — Choose a Method

<table>
  <thead>
    <tr>
      <th>Platform</th>
      <th>Distribution</th>
      <th>Architecture</th>
      <th>Install / download</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td rowspan="11"><img src="https://cdn.jsdelivr.net/gh/devicons/devicon@v2.17.0/icons/linux/linux-original.svg" width="36" height="36" alt="Linux logo"><br><strong>Linux</strong></td>
      <td rowspan="2"><img src="https://cdn.jsdelivr.net/gh/vscode-icons/vscode-icons@v12.19.0/icons/file_type_zip.svg" width="28" height="28" alt="Archive file icon"> <strong>Portable archive</strong></td>
      <td><code>x86_64</code></td>
      <td><a href="$release_base/$linux_x86_tarball"><code>$linux_x86_tarball</code></a></td>
    </tr>
    <tr>
      <td><code>aarch64</code></td>
      <td><a href="$release_base/$linux_arm_tarball"><code>$linux_arm_tarball</code></a></td>
    </tr>
    <tr>
      <td rowspan="2"><img src="https://cdn.simpleicons.org/appimage" width="28" height="28" alt="AppImage logo"> <strong>AppImage</strong></td>
      <td><code>x86_64</code></td>
      <td><a href="$release_base/$linux_x86_appimage"><code>$linux_x86_appimage</code></a></td>
    </tr>
    <tr>
      <td><code>aarch64</code></td>
      <td><a href="$release_base/$linux_arm_appimage"><code>$linux_arm_appimage</code></a></td>
    </tr>
    <tr>
      <td><img src="https://cdn.jsdelivr.net/gh/devicons/devicon@v2.17.0/icons/nixos/nixos-original.svg" width="28" height="28" alt="NixOS logo"> <strong>Nix flake</strong></td>
      <td><code>x86_64</code><br><code>aarch64</code></td>
      <td><code>nix run --accept-flake-config github:$repository/$tag</code></td>
    </tr>
    <tr>
      <td><img src="https://cdn.jsdelivr.net/gh/devicons/devicon@v2.17.0/icons/docker/docker-original.svg" width="30" height="30" alt="Docker logo"> <strong>Docker</strong></td>
      <td><code>x86_64</code> / <code>amd64</code><br><code>aarch64</code> / <code>arm64</code></td>
      <td><a href="https://github.com/$repository/pkgs/container/neomacs">GHCR</a> · <a href="https://hub.docker.com/r/evalexec/neomacs/tags?name=$version">Docker Hub</a></td>
    </tr>
    <tr>
      <td><img src="https://cdn.jsdelivr.net/gh/devicons/devicon@v2.17.0/icons/archlinux/archlinux-original.svg" width="28" height="28" alt="Arch Linux logo"> <strong>ArchLinux</strong></td>
      <td><code>x86_64</code></td>
      <td><a href="https://aur.archlinux.org/packages/neomacs-bin"><code>neomacs-bin</code></a></td>
    </tr>
    <tr>
      <td rowspan="2"><img src="https://cdn.jsdelivr.net/gh/devicons/devicon@v2.17.0/icons/debian/debian-original.svg" width="24" height="24" alt="Debian logo"> <strong>Debian</strong><br><img src="https://cdn.jsdelivr.net/gh/devicons/devicon@v2.17.0/icons/ubuntu/ubuntu-original.svg" width="24" height="24" alt="Ubuntu logo"> <strong>Ubuntu</strong></td>
      <td><code>x86_64</code></td>
      <td><a href="$release_base/$linux_x86_deb"><code>$linux_x86_deb</code></a></td>
    </tr>
    <tr>
      <td><code>aarch64</code></td>
      <td><a href="$release_base/$linux_arm_deb"><code>$linux_arm_deb</code></a></td>
    </tr>
    <tr>
      <td rowspan="2"><img src="https://cdn.jsdelivr.net/gh/devicons/devicon@v2.17.0/icons/fedora/fedora-original.svg" width="22" height="22" alt="Fedora logo"> <strong>Fedora</strong><br><img src="https://cdn.jsdelivr.net/gh/devicons/devicon@v2.17.0/icons/redhat/redhat-original.svg" width="22" height="22" alt="Red Hat logo"> <strong>RHEL</strong><br><img src="https://cdn.jsdelivr.net/gh/devicons/devicon@v2.17.0/icons/opensuse/opensuse-original.svg" width="22" height="22" alt="openSUSE logo"> <strong>openSUSE</strong></td>
      <td><code>x86_64</code></td>
      <td><a href="$release_base/$linux_x86_rpm"><code>$linux_x86_rpm</code></a></td>
    </tr>
    <tr>
      <td><code>aarch64</code></td>
      <td><a href="$release_base/$linux_arm_rpm"><code>$linux_arm_rpm</code></a></td>
    </tr>
    <tr>
      <td rowspan="3"><img src="https://cdn.simpleicons.org/apple/808080" width="32" height="32" alt="Apple logo"><br><strong>macOS</strong></td>
      <td rowspan="3" colspan="2">Apple Silicon<br><code>aarch64</code></td>
      <td><a href="$release_base/$macos_dmg"><code>$macos_dmg</code></a></td>
    </tr>
    <tr>
      <td><a href="$release_base/$macos_zip"><code>$macos_zip</code></a></td>
    </tr>
    <tr>
      <td><a href="$release_base/$macos_tarball"><code>$macos_tarball</code></a></td>
    </tr>
    <tr>
      <td rowspan="4"><img src="https://cdn.jsdelivr.net/gh/devicons/devicon@v2.17.0/icons/windows11/windows11-original.svg" width="32" height="32" alt="Windows logo"><br><strong>Windows</strong></td>
      <td rowspan="2" colspan="2"><code>x86_64</code></td>
      <td><a href="$release_base/$windows_x86_installer"><code>$windows_x86_installer</code></a></td>
    </tr>
    <tr>
      <td><a href="$release_base/$windows_x86_zip"><code>$windows_x86_zip</code></a></td>
    </tr>
    <tr>
      <td rowspan="2" colspan="2"><code>aarch64</code></td>
      <td><a href="$release_base/$windows_arm_installer"><code>$windows_arm_installer</code></a></td>
    </tr>
    <tr>
      <td><a href="$release_base/$windows_arm_zip"><code>$windows_arm_zip</code></a></td>
    </tr>
  </tbody>
</table>

### Verify your download

SHA-256 checksums for every release asset are available in [SHA256SUMS]($release_base/SHA256SUMS).
HTML
}

in_changes=0
installation_written=0
{
  while IFS= read -r line || [[ -n "$line" ]]; do
    if [[ "$line" == "## What's Changed" ]]; then
      write_installation_methods
      printf '\n<details>\n<summary><strong>What\047s Changed</strong></summary>\n\n'
      in_changes=1
      installation_written=1
    elif ((in_changes)) \
      && { [[ "$line" == "## New Contributors" ]] || [[ "$line" == "**Full Changelog**:"* ]]; }; then
      printf '</details>\n\n%s\n' "$line"
      in_changes=0
    else
      printf '%s\n' "$line"
    fi
  done <"$generated_notes"

  if ((in_changes)); then
    printf '</details>\n'
  fi
} >"$output"

if ((!installation_written)); then
  echo "generated GitHub release notes could not place installation methods" >&2
  exit 1
fi

echo "wrote $output"
