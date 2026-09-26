# Maintainer: Yangtse Su <yangtsesu@gmail.com>
#
# The AUR package (S16). It builds from the release tag tarball — the retired
# plan's decision table: "package name `pixlay`; the release tag vX.Y.Z is pushed
# to GitHub, and the PKGBUILD's source= uses the tag tarball" — and packs the
# shape the plan of 2026-09-25 produces: two binaries, an app icon with its
# symbolic variant, the desktop entry and the metainfo (both generated from
# their templates with `msgfmt`, S16), the `.pixlay` MIME type, and the language
# packs `po/LINGUAS` lists.
pkgname=pixlay
pkgver=0.1.0
pkgrel=1
pkgdesc="Make a collage out of one to nine photos and export it for printing"
# x86_64 only: every measurement in the repository was taken on this
# architecture and nothing here has ever been built on another one.
arch=('x86_64')
url="https://yangtse.org/pixlay"
license=('GPL-3.0-or-later')
# gtk4 and libadwaita are the shell (`AGENTS.md`, "Module boundaries"); `glycin`
# is the decoding backend S4 measured — the sandboxed loader, whose own
# dependencies (bubblewrap, libseccomp, the loaders) come with that package.
depends=('gtk4' 'libadwaita' 'glycin')
# `cargo vendor` needs the network once and the three cargo calls below are
# frozen and offline; gettext extracts and compiles the catalogs and generates
# the desktop entry and the metainfo; desktop-file-utils and appstream are the
# validators check() runs; libheif is what the HEIC and AVIF fixtures need (the
# decoder is optional at runtime, which is why it is an optdepend as well);
# Xvfb with mesa's software GL is the display `check()` gives the GUI tests.
makedepends=('cargo' 'rust' 'gettext' 'desktop-file-utils' 'appstream' 'libheif' 'xorg-server-xvfb' 'mesa')
optdepends=('libheif: decode HEIC and AVIF photos')
# The tag does not exist yet: S16 packs the tree at 0.1.0, and pushing `v0.1.0`
# is the release step. Until then this SKIP is also what lets a builder — CI
# included — hand makepkg a tarball of their own (`git archive` with the
# `pixlay-$pkgver/` prefix, in SRCDEST). The release replaces it: push the tag,
# run `updpkgsums`, then `makepkg --printsrcinfo > .SRCINFO`.
source=("$pkgname-$pkgver.tar.gz::https://github.com/YangtseSu/pixlay/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

# The two generated artifacts, from their templates plus the catalogs in po/.
#
# `-d po` is what makes one file carry every language: gettext reads the
# `po/LINGUAS` list and merges each language's entries in (`Name[de]`,
# `<summary xml:lang="de">`), so `msgfmt` runs once and not once per language.
# With no languages the templates come out unchanged, which is the English text
# the package ships today.
_generate_data() {
  local out=$1
  install -d "$out"
  msgfmt --desktop --template=data/org.yangtse.Pixlay.desktop.in -d po \
    -o "$out/org.yangtse.Pixlay.desktop"
  msgfmt --xml --template=data/org.yangtse.Pixlay.metainfo.xml.in -L metainfo -d po \
    -o "$out/org.yangtse.Pixlay.metainfo.xml"
}

# The flags both build() and check() compile with.
#
# The release profile keeps `debug = 1` (Cargo.toml), so every Rust binary
# carries the path of the source it was built from — which would be this build's
# `$srcdir`, and makepkg reports a package that names its build directory as a
# packaging issue. Remapping the prefix rewrites those strings to the directory
# the tarball extracts to, so the binary names a path the package itself has.
#
# Exported here rather than passed to the two cargo calls: a changed `RUSTFLAGS`
# changes cargo's fingerprint of every dependency, so setting it once keeps
# check() from recompiling the tree build() already built.
prepare() {
  export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--remap-path-prefix=$srcdir=$pkgname-$pkgver"
}

build() {
  cd "$pkgname-$pkgver"

  # The registry, vendored: every cargo call below is `--frozen --offline` (AUR
  # discipline), so nothing may reach the network while they run. `cargo vendor`
  # prints the source replacement it wants, and that goes where cargo looks for
  # it — `.cargo/config.toml` in the extracted tree.
  install -d .cargo
  cargo vendor --locked vendor > .cargo/config.toml
  cargo build --release --frozen --offline

  # Generated here rather than in check(), so check() validates the very files
  # package() installs.
  _generate_data generated
}

check() {
  cd "$pkgname-$pkgver"

  # The installed artifacts, validated as themselves: `desktop-file-validate`
  # wants a `.desktop` name, which is why the template cannot be handed to it.
  desktop-file-validate generated/org.yangtse.Pixlay.desktop
  appstreamcli validate --no-net generated/org.yangtse.Pixlay.metainfo.xml

  # Every catalog is a valid catalog. `po/` has none until a language pack is
  # added, and a glob that matches nothing is not an error here.
  local catalogs=(po/*.po)
  if [[ -e ${catalogs[0]} ]]; then
    msgfmt --check --check-format -o /dev/null "${catalogs[@]}"
  fi

  # The workspace's tests, offline against the vendored registry and the
  # repository's own fixtures. The GUI half needs a display, and `check()` has
  # none: `PIXLAY_TEST_CHILD=1` tells the harness to use the display this
  # process has instead of starting its own headless mutter (S16, verified in a
  # chroot), and Xvfb with mesa's software GL is that display. The harness'
  # mutter wants a GPU node, which a chroot built without `/dev/dri` has not.
  #
  # **`-screen 0 1920x1200x24` is not decoration.** It is the size and depth the
  # harness' own virtual monitor gives the tests (tests/support/mod.rs), and the
  # suite measures window geometry against this application's own default
  # window: measured in a chroot, Xvfb's defaults (1280x1024 at depth 8) leave
  # the window's canvases small enough that `tests/compose.rs`'s "the strip is
  # inside its cell" fails on a window the size tests never see.
  PIXLAY_TEST_CHILD=1 xvfb-run -a -s "-screen 0 1920x1200x24" \
    cargo test --workspace --frozen --offline
}

package() {
  cd "$pkgname-$pkgver"

  install -Dm755 target/release/pixlay "$pkgdir/usr/bin/pixlay"
  install -Dm755 target/release/pixlay-render "$pkgdir/usr/bin/pixlay-render"
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"

  # The app icon and its symbolic variant, named by the app-id the desktop
  # entry, the metainfo and the About dialog all carry.
  install -Dm644 data/icons/hicolor/scalable/apps/org.yangtse.Pixlay.svg \
    "$pkgdir/usr/share/icons/hicolor/scalable/apps/org.yangtse.Pixlay.svg"
  install -Dm644 data/icons/hicolor/symbolic/apps/org.yangtse.Pixlay-symbolic.svg \
    "$pkgdir/usr/share/icons/hicolor/symbolic/apps/org.yangtse.Pixlay-symbolic.svg"

  install -Dm644 generated/org.yangtse.Pixlay.desktop \
    "$pkgdir/usr/share/applications/org.yangtse.Pixlay.desktop"
  install -Dm644 generated/org.yangtse.Pixlay.metainfo.xml \
    "$pkgdir/usr/share/metainfo/org.yangtse.Pixlay.metainfo.xml"
  # `update-mime-database` (shared-mime-info's own pacman hook) turns this into
  # the `.pixlay` type; the desktop entry's MimeType line is what makes this
  # application the handler for it.
  install -Dm644 data/org.yangtse.Pixlay.mime.xml \
    "$pkgdir/usr/share/mime/packages/org.yangtse.Pixlay.xml"

  # The language packs: one catalog per language in po/LINGUAS, in the domain
  # the shell binds (`i18n.rs`, `pixlay`) at the prefix it looks in
  # (`/usr/share/locale`). Nothing listed means nothing installed, and every
  # string stays the English source string — gettext's own fallback.
  local lang
  while read -r lang; do
    case "$lang" in '' | '#'*) continue ;; esac
    msgfmt --check -o "$pkgdir/usr/share/locale/$lang/LC_MESSAGES/$pkgname.mo" \
      "po/$lang.po"
  done < po/LINGUAS
}
