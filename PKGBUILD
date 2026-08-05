# Maintainer: eval-exec <eval-exec@github>
pkgname=neomacs-bin
_pkgname=neomacs
pkgver=0.0.14
pkgrel=1
pkgdesc="GPU powered Emacs written in Rust with a modern display engine (prebuilt binary)"
arch=('x86_64')
url="https://github.com/eval-exec/neomacs"
license=('GPL-3.0-or-later')
depends=(
  'fontconfig'
  'freetype2'
  'gcc-libs'
  'glib2'
  'gstreamer'
  'gst-plugins-base-libs'
  'ncurses'
  'zlib'
)
provides=('neomacs' 'emacs')
conflicts=('neomacs' 'emacs')
source=("${pkgname}-${pkgver}.tar.gz::https://github.com/eval-exec/neomacs/releases/download/v${pkgver}/neomacs-${pkgver}-x86_64-unknown-linux-gnu.tar.gz"
        "https://raw.githubusercontent.com/eval-exec/neomacs/main/assets/logo.png"
        "https://raw.githubusercontent.com/eval-exec/neomacs/main/etc/emacs.desktop")
sha256sums=('4da21cbffa9ec62edb28a72ddfdde76bc286868916a77d10fe405428ada53eb1'
            'd62924ab6f8783beb69eef1b873f2c0b0d2e92f568ec47774423ad0eab3e25d9'
            'f6ae6611d2e54e65d210ad2983df80ac5b65618c83c4190700e90ff455e12464')

package() {
  local _dir="neomacs-${pkgver}-x86_64-unknown-linux-gnu"

  # Install binaries and pdump
  install -dm755 "${pkgdir}/usr/bin"
  install -m755 "${srcdir}/${_dir}/bin/neomacs" "${pkgdir}/usr/bin/neomacs"
  install -m755 "${srcdir}/${_dir}/bin/neomacsclient" "${pkgdir}/usr/bin/neomacsclient"
  install -m755 "${srcdir}/${_dir}/bin/neomacs-temacs" "${pkgdir}/usr/bin/neomacs-temacs"
  install -m755 "${srcdir}/${_dir}/bin/bootstrap-neomacs" "${pkgdir}/usr/bin/bootstrap-neomacs"
  install -m755 "${srcdir}/${_dir}/bin/mock-display" "${pkgdir}/usr/bin/mock-display"
  install -m644 "${srcdir}/${_dir}/bin/neomacs.pdump" "${pkgdir}/usr/bin/neomacs.pdump"

  # Install shared Lisp files
  if [ -d "${srcdir}/${_dir}/share/neomacs" ]; then
    install -dm755 "${pkgdir}/usr/share"
    cp -r "${srcdir}/${_dir}/share/neomacs" "${pkgdir}/usr/share/neomacs"
  fi

  # Install Desktop entry & icon
  install -Dm644 "${srcdir}/emacs.desktop" "${pkgdir}/usr/share/applications/neomacs.desktop"
  sed -i 's/Name=Emacs/Name=Neomacs/' "${pkgdir}/usr/share/applications/neomacs.desktop"
  sed -i 's/Exec=emacs %F/Exec=neomacs %F/' "${pkgdir}/usr/share/applications/neomacs.desktop"
  sed -i 's/Icon=emacs/Icon=neomacs/' "${pkgdir}/usr/share/applications/neomacs.desktop"

  install -Dm644 "${srcdir}/logo.png" "${pkgdir}/usr/share/icons/hicolor/128x128/apps/neomacs.png"

  # License
  install -Dm644 "${srcdir}/${_dir}/COPYING" "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
}
