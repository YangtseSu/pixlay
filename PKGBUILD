# Maintainer: Yangtse Su <yangtsesu@gmail.com>
#
# The AUR package (S16): the two binaries, the app icon with its symbolic variant, the
# desktop entry and the metainfo (both generated from their templates with `msgfmt`),
# the `.pixlay` MIME type, and the language packs `po/LINGUAS` lists.
pkgname=pixlay
pkgver=0.1.0
pkgrel=1
pkgdesc="Make a collage out of one to nine photos and export it for printing"
arch=('x86_64')
url="https://yangtse.org/pixlay"
license=('GPL-3.0-or-later')
# The runtime libraries the binaries link: the shell's toolkit, and the decoding backend,
# which is a loader whose own dependencies (bubblewrap, libseccomp, the format loaders)
# come with that package.
depends=('gtk4' 'libadwaita' 'glycin')
# Build-time only: cargo and rust build the workspace, gettext compiles the catalogs and
# generates the two data files, desktop-file-utils and appstream are the validators
# check() runs, libheif is what the HEIC and AVIF fixtures decode through — and mutter,
# dbus and mesa are the display check() runs the GUI tests on.
makedepends=('cargo' 'rust' 'gettext' 'desktop-file-utils' 'appstream' 'libheif' 'mutter' 'dbus' 'mesa')
optdepends=('libheif: decode HEIC and AVIF photos')
# The tag does not exist yet: pushing `v0.1.0` is the release step, and until then this
# SKIP is what lets a builder hand makepkg a tarball of their own (`git archive` with the
# `pixlay-$pkgver/` prefix, in SRCDEST). The release replaces it: push the tag, run
# `updpkgsums`, then `makepkg --printsrcinfo > .SRCINFO`.
source=("$pkgname-$pkgver.tar.gz::https://github.com/YangtseSu/pixlay/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

# The two generated artifacts, from their templates plus the catalogs in po/. `-d po` is
# what makes one file carry every language: gettext reads the `po/LINGUAS` list and merges
# each language's entries in (`Name[de]`, `<summary xml:lang="de">`).
_generate_data() {
  local out=$1
  install -d "$out"
  msgfmt --desktop --template=data/org.yangtse.Pixlay.desktop.in -d po \
    -o "$out/org.yangtse.Pixlay.desktop"
  msgfmt --xml --template=data/org.yangtse.Pixlay.metainfo.xml.in -L metainfo -d po \
    -o "$out/org.yangtse.Pixlay.metainfo.xml"
}

# `debug = 1` (Cargo.toml) makes every binary carry the path it was built from, which
# would be this build's `$srcdir` and is a path the package does not have; remapping the
# prefix rewrites it to the directory the tarball extracts to. Exported here rather than
# passed to the cargo calls, because a changed `RUSTFLAGS` changes cargo's fingerprint of
# every dependency.
prepare() {
  export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--remap-path-prefix=$srcdir=$pkgname-$pkgver"
}

build() {
  cd "$pkgname-$pkgver"

  # The registry, vendored: every cargo call below is `--frozen --offline` (AUR
  # discipline), so nothing may reach the network while they run. `cargo vendor` prints
  # the source replacement it wants, and that goes where cargo looks for it.
  install -d .cargo
  cargo vendor --locked vendor > .cargo/config.toml
  cargo build --release --frozen --offline

  # Generated here rather than in check(), so check() validates the very files package()
  # installs.
  _generate_data generated
}

check() {
  cd "$pkgname-$pkgver"

  # The installed artifacts, validated as themselves: `desktop-file-validate` wants a
  # `.desktop` name, which is why the template cannot be handed to it.
  desktop-file-validate generated/org.yangtse.Pixlay.desktop
  appstreamcli validate --no-net generated/org.yangtse.Pixlay.metainfo.xml

  # Every catalog is a valid catalog. `po/` has none until a language pack is added, and a
  # glob that matches nothing is not an error here.
  local catalogs=(po/*.po)
  if [[ -e ${catalogs[0]} ]]; then
    msgfmt --check --check-format -o /dev/null "${catalogs[@]}"
  fi

  # The workspace's tests, offline against the vendored registry and the repository's own
  # fixtures. The GUI half needs a display, and check() has none: the harness starts its
  # own (`tests/support/mod.rs`), which needs a session bus and a runtime directory of its
  # own — and a machine-id `dbus-daemon` refuses to start a bus without.
  [ -e /etc/machine-id ] || dbus-uuidgen --ensure=/etc/machine-id
  local runtime
  runtime=$(mktemp -d)
  XDG_RUNTIME_DIR="$runtime" dbus-run-session -- \
    cargo test --workspace --frozen --offline
}

package() {
  cd "$pkgname-$pkgver"

  install -Dm755 target/release/pixlay "$pkgdir/usr/bin/pixlay"
  install -Dm755 target/release/pixlay-render "$pkgdir/usr/bin/pixlay-render"
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"

  # The app icon and its symbolic variant, named by the app-id the desktop entry and the
  # metainfo carry.
  install -Dm644 data/icons/hicolor/scalable/apps/org.yangtse.Pixlay.svg \
    "$pkgdir/usr/share/icons/hicolor/scalable/apps/org.yangtse.Pixlay.svg"
  install -Dm644 data/icons/hicolor/symbolic/apps/org.yangtse.Pixlay-symbolic.svg \
    "$pkgdir/usr/share/icons/hicolor/symbolic/apps/org.yangtse.Pixlay-symbolic.svg"

  install -Dm644 generated/org.yangtse.Pixlay.desktop \
    "$pkgdir/usr/share/applications/org.yangtse.Pixlay.desktop"
  install -Dm644 generated/org.yangtse.Pixlay.metainfo.xml \
    "$pkgdir/usr/share/metainfo/org.yangtse.Pixlay.metainfo.xml"
  # `update-mime-database` (shared-mime-info's own pacman hook) turns this into the
  # `.pixlay` type; the desktop entry's MimeType line is what makes this application the
  # handler for it.
  install -Dm644 data/org.yangtse.Pixlay.mime.xml \
    "$pkgdir/usr/share/mime/packages/org.yangtse.Pixlay.xml"

  # The language packs: one catalog per language in po/LINGUAS, installed in the domain
  # the shell binds at the prefix it looks in. Nothing listed means nothing installed.
  local lang
  while read -r lang; do
    case "$lang" in '' | '#'*) continue ;; esac
    msgfmt --check -o "$pkgdir/usr/share/locale/$lang/LC_MESSAGES/$pkgname.mo" \
      "po/$lang.po"
  done < po/LINGUAS
}
