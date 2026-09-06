#!/usr/bin/env bash
set -euo pipefail

readonly usage="usage: $0 [--list] {build|oracle|ecosystem|release}"

list_only=false
if [[ ${1:-} == "--list" ]]; then
    list_only=true
    shift
fi

readonly profile=${1:-}
if [[ -z $profile || $# -ne 1 ]]; then
    echo "$usage" >&2
    exit 2
fi

readonly -a build_packages=(
    build-essential
    git
    pkg-config
    cmake
    m4
    libssl-dev
    fontconfig
    libfontconfig1-dev
    libfreetype-dev
    libncurses-dev
    libglib2.0-dev
    libunwind-dev
    libxkbcommon-dev
    libxkbcommon-x11-dev
    libwayland-dev
    wayland-protocols
    libxcb1-dev
    libxrandr-dev
    libxinerama-dev
    libxi-dev
    libxcursor-dev
    mesa-vulkan-drivers
    libvulkan-dev
    libdbus-1-dev
    libsystemd-dev
    libsqlite3-dev
    libxml2-dev
    liblcms2-dev
    gnutls-bin
    libgnutls28-dev
    zlib1g-dev
)

readonly -a video_backend_packages=(
    libgstreamer1.0-dev
    libgstreamer-plugins-base1.0-dev
)

declare -a profile_packages=()
declare -a required_commands=()
requires_libfaketime=false
case "$profile" in
    build)
        ;;
    # GNU Emacs is NOT an apt package here: every GNU-vs-Neomacs comparison
    # runs against the pinned build installed by
    # .github/actions/setup-gnu-emacs (the apt emacs-nox 29.3 version-skewed
    # against the Emacs 31 reference the lisp tree and local pin track).
    oracle)
        profile_packages=(libfaketime)
        requires_libfaketime=true
        ;;
    ecosystem)
        profile_packages=(
            gnupg
            libfaketime
            xvfb
            xauth
            x11-utils
            xdotool
            imagemagick
            weston
            fonts-noto-core
        )
        required_commands=(gpg Xvfb xauth xdpyinfo xdotool import weston)
        requires_libfaketime=true
        ;;
    release)
        profile_packages=(rpm binutils cpio file dpkg-dev)
        required_commands=(rpm objdump cpio file dpkg-shlibdeps)
        ;;
    *)
        echo "unknown profile: $profile" >&2
        echo "$usage" >&2
        exit 2
        ;;
esac

# Every product this repo ships declares `video`, so the GStreamer development
# files are part of every profile's build environment.
declare -a packages=(
    "${build_packages[@]}"
    "${video_backend_packages[@]}"
    "${profile_packages[@]}"
)
if $list_only; then
    printf '%s\n' "${packages[@]}"
    exit 0
fi

if [[ $(uname -s) != "Linux" ]] || ! command -v apt-get >/dev/null 2>&1; then
    echo "setup-linux.sh requires an apt-based Linux runner" >&2
    exit 1
fi

sudo apt-get update
sudo apt-get install -y --no-install-recommends "${packages[@]}"

# Fail at the environment seam instead of silently compiling out optional
# primitives and discovering the mismatch much later in an oracle test.
pkg-config --modversion lcms2
pkg-config --modversion gstreamer-1.0

if $requires_libfaketime; then
    dpkg -L libfaketime | grep -q '/libfaketime\.so\.1$'
fi
for program in "${required_commands[@]}"; do
    command -v "$program" >/dev/null
done
