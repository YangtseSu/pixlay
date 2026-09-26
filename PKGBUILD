# Maintainer: Yangtse Su <yangtsesu@gmail.com>
pkgname=pixlay
pkgver=0.1.0
pkgrel=1
pkgdesc="Make a collage out of one to nine photos and export it for printing"
arch=('x86_64')
url="https://yangtse.org/pixlay"
license=('GPL-3.0-or-later')
depends=('gtk4' 'libadwaita' 'glycin')
makedepends=('cargo' 'rust' 'meson' 'gettext')
optdepends=('libheif: decode HEIC and AVIF photos')
# The release tag is not pushed yet, so there is no checksum to carry; a release replaces
# this (`updpkgsums`) and writes `.SRCINFO`.
source=("$pkgname-$pkgver.tar.gz::https://github.com/YangtseSu/pixlay/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

# `debug = 1` (Cargo.toml) writes the build path into every binary; the remap rewrites it
# to the directory the package itself has.
prepare() {
  export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--remap-path-prefix=$srcdir=$pkgname-$pkgver"
}

build() {
  cd "$pkgname-$pkgver"

  # Vendored, so the build never reaches the network, and `CARGO_NET_OFFLINE` says so to
  # the cargo call meson makes.
  install -d .cargo
  cargo vendor --locked vendor > .cargo/config.toml

  export CARGO_NET_OFFLINE=true
  meson setup build --prefix=/usr
  meson compile -C build
}

package() {
  cd "$pkgname-$pkgver"

  meson install -C build --destdir "$pkgdir"
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
