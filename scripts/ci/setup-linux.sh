#!/usr/bin/env bash
set -euo pipefail

readonly usage="usage: $0 [--list] {build|oracle|ecosystem|fonts|release}"

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
    # neomacs-layout-engine's GNU CJK metric test (linux only) links libcairo to
    # reproduce GNU's ftcr backend (ftcrfont.c); the -dev package provides libcairo.so.
    libcairo2-dev
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
    # The workspace test archive links with mold; it is a drop-in ELF linker
    # and is faster than lld for the many test binaries this repo links.
    mold
)

readonly -a video_backend_packages=(
    libgstreamer1.0-dev
    libgstreamer-plugins-base1.0-dev
)

# Fonts the layout-engine, renderer and GUI tests resolve through the system
# fontconfig.  They are the environment contract of those tests, not build
# inputs: char_width_dejavu_* names DejaVu, the face-realization tests name
# Noto Sans (Mono), and the CJK families (gb2312 candidate queries, double-
# width metrics, offscreen CJK glyph runs) need a wide CJK face; without one
# every fallback collapses to the primary font's .notdef advance.
readonly -a font_packages=(
    fonts-noto-core
    fonts-dejavu-core
    fonts-ubuntu
    fonts-noto-cjk
)

declare -a profile_packages=()
declare -a required_commands=()
requires_libfaketime=false
install_nerd_fonts=false
case "$profile" in
    build)
        ;;
    # GNU Emacs is NOT an apt package here: every GNU-vs-Neomacs comparison
    # runs against the pinned build installed by
    # .github/actions/setup-gnu-emacs (the apt emacs-nox 29.3 version-skewed
    # against the Emacs 31 reference the lisp tree and local pin track).
    #
    # The GNU oracle's own packages live in .github/actions/setup-gnu-emacs,
    # not here: three of that action's callers use `ecosystem` rather than
    # `oracle`, so a profile-side list would leave them restoring a GTK-linked
    # Emacs with no GTK to run it. The action installs what it needs on every
    # run, hit or miss.
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
            x11-xkb-utils
            xdotool
            wmctrl
            kwin-x11
            # KWin's decoration plugin.  Without one KWin loads neither
            # org.kde.breeze nor aurorae's Plastik fallback (org.kde.plasma.core
            # is absent), so it never publishes _NET_FRAME_EXTENTS -- the
            # property the issue-582 GNU fullscreen oracle reads to measure the
            # GTK3 GNU window's decorated outer edge.  Neomacs's own windows
            # are undecorated, which is why only the GNU arm failed.
            kwin-style-breeze
            dbus
            imagemagick
            ripgrep
            weston
            sway
            wtype
            grim
            # native_frame_focus::ime drives a real fcitx5 session through the
            # XIM entry point; pinyin and the unicode addon are the two inputs
            # it sends, and fcitx5-remote is how the test switches between them.
            fcitx5
            fcitx5-chinese-addons
            fcitx5-modules
            "${font_packages[@]}"
        )
        required_commands=(gpg rg Xvfb xauth xdpyinfo xprop xwininfo xkbcomp xdotool wmctrl kwin_x11 dbus-run-session fc-match import weston sway wtype grim fcitx5)
        requires_libfaketime=true
        install_nerd_fonts=true
        ;;
    # Jobs that run the font-sensitive Rust tests (the display stack shards)
    # but need no window system of their own.
    fonts)
        profile_packages=("${font_packages[@]}")
        required_commands=(fc-match)
        install_nerd_fonts=true
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

# The layout, renderer and GUI suites resolve the default family
# "JetBrainsMono Nerd Font" and the Nerd Fonts glyph ranges through the system
# fontconfig.  Ubuntu packages only the unpatched JetBrains Mono, so without a
# patched family those tests degrade to the fontconfig fallback (Lato) and the
# Nerd Font probes never publish a binding.  Nerd Fonts ships patched families
# only as release artifacts; pin the artifact by version and sha256 instead of
# trusting whichever font a host happens to have.
if $install_nerd_fonts; then
    nerd_fonts_version=3.4.0
    nerd_fonts_sha256=ef552a3e638f25125c6ad4c51176a6adcdce295ab1d2ffacf0db060caf8c1582
    nerd_fonts_dir=/usr/share/fonts/truetype/nerd-fonts
    nerd_fonts_archive=$(mktemp)
    curl -fsSL --retry 3 -o "$nerd_fonts_archive" \
        "https://github.com/ryanoasis/nerd-fonts/releases/download/v${nerd_fonts_version}/JetBrainsMono.tar.xz"
    printf '%s  %s\n' "$nerd_fonts_sha256" "$nerd_fonts_archive" | sha256sum -c -
    sudo mkdir -p "$nerd_fonts_dir"
    sudo tar -xJf "$nerd_fonts_archive" -C "$nerd_fonts_dir" \
        JetBrainsMonoNerdFont-Regular.ttf \
        JetBrainsMonoNerdFont-Bold.ttf \
        JetBrainsMonoNerdFont-Italic.ttf \
        JetBrainsMonoNerdFont-BoldItalic.ttf \
        OFL.txt
    rm -f "$nerd_fonts_archive"
    sudo fc-cache -f "$nerd_fonts_dir"
    fc-match -f '%{file}\n' "JetBrainsMono Nerd Font" | grep -Fq "$nerd_fonts_dir/"
fi

for program in "${required_commands[@]}"; do
    command -v "$program" >/dev/null
done
