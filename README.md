# Pixlay

Make a collage out of one to nine photos and export it for printing.

Pixlay is a Linux desktop application: a GTK4 + libadwaita window over a Cairo canvas, in Rust. The
target platform is Arch Linux; the license is GPL-3.0-or-later.

The whole application is one window, and the main path is **open → add photos → pick a layout →
adjust → export**, under three minutes. There is no import step: the window opens on the collage,
and photos enter it from outside it — `Add photos…`, a drop from the file manager onto the cell it
is aimed at, the clipboard, or the command line (`pixlay a.jpg b.jpg`, in argument order).

## What it does

- **One to nine photos.** A single photo is a legal collage; a selection or a drop past nine is
  trimmed to the first nine, with one report saying how many were not used.
- **27 layouts** over cell counts one … nine and five sheet aspects (4:3, 16:9, 1:1, 3:2, 2:3),
  regular and irregular — `pixlay-render templates` lists this build's own. The layout band draws
  each candidate as a *sketch* of its geometry, not a render of your photos.
- **Framing per cell**: pan, zoom and rotation by any angle. The clamp follows the exact angle, so
  the cell stays covered; `Ctrl+0` resets the framing.
- **Swap any two cells** whole — photo and framing both — by dragging one onto the other, with
  `Shift` and a click, or with `Ctrl+Shift` and an arrow key. One undo step.
- **A canvas frame**: the gap between two photos, the corner radius and the colour. White, gapless
  and square-cornered by default, so an older project renders byte-identically.
- **Export as PNG or JPEG** at a long edge you choose, 1 … 30000 pixels, through the platform's own
  save dialog. The file carries its ICC profile and a JPEG is 4:4:4, written in the same pass as
  the pixels.
- **Undo and redo** across every edit, atomic saves, and a check before unsaved work is discarded.
- **`.pixlay` projects**: JSON that embeds its own layout geometry, so a project keeps its pixels
  when the shipped library changes, and relative photo paths that travel with the file.
- **Preview and export are the same renderer.** Resampling happens upstream, in linear light with
  16-bit buffers; the backdrop is opaque; a source photo is never written to.
- **Keyboard-first and accessible**: every action has a shortcut, `Ctrl+?` lists them, every
  control has an accessible name. Dark by default. English source strings, translated through
  gettext.

Pixlay makes a collage and nothing else: no colour grading, no filters, no text layer, no
watermark, no date stamp, no TIFF, no physical sizes or DPI, and no mirroring a cell.

## Install

### Arch Linux

The PKGBUILD under `packaging/arch` builds the tree and installs it:

    cd packaging/arch
    makepkg -si

### Any Linux, from source

Needs GTK 4.12+, libadwaita 1.8+, glycin 2, libseccomp, glib and gio (the runtime libraries), plus a
Rust toolchain, meson, ninja and gettext. `meson setup` names anything that is missing:

    meson setup build --prefix=/usr
    meson compile -C build
    sudo meson install -C build

### Prebuilt

Every GitHub release carries `pixlay-<version>-linux-amd64.tar.gz` with both binaries and its
`.sha256`, and the `x86_64` Arch package. The binaries link the system's GTK, libadwaita and
glycin; HEIC and AVIF photos need `libheif` installed.

## Use

Launch `pixlay`, pass photos to it (`pixlay a.jpg b.jpg` adds them in argument order), or open a
`.pixlay` project — the file type is registered, so a double click opens it too.

The window's own path: `Add photos…` in the header bar, a layout from the band under the canvas, the
cell's controls or a drag to frame it, then Export. `Ctrl+?` lists every shortcut; the ones on the
main path are `Ctrl+I` (add photos), `Ctrl+E` (export), `Ctrl+S` / `Ctrl+Shift+S` (save / save as),
`Ctrl+Z` / `Ctrl+Shift+Z` (undo / redo), `Ctrl+Shift+←/→/↑/↓` (swap with the neighbouring cell),
`Ctrl+0` (reset the framing), `Ctrl+,` (settings).

The export's format, its long edge and the last export folder are remembered in
`~/.config/pixlay/settings.json`.

### The command line

`pixlay-render` is the windowless half — everything above is reachable from a shell, with its result
on stdout and diagnostics on stderr:

    pixlay-render templates --slots 2                     # layouts a two-photo collage can pick from
    pixlay-render init --template strip-2-2x1 --photo a.jpg --photo b.jpg --out trip.pixlay
    pixlay-render render --project trip.pixlay --out trip.jpg --long-edge 4000
    pixlay-render edit --project trip.pixlay --slot 0 --zoom 1.2 --rotate 3 --out framed.pixlay
    pixlay-render probe --project framed.pixlay           # numbers: coverage, backdrop, seams, gap
    pixlay-render --help

The subcommands are `render`, `probe`, `image`, `scan`, `thumb`, `templates`, `init`, `edit`, `save`,
`hit`, `gesture` and `switch`. `--json` prints one JSON object instead of `key = value` lines,
`--stats` adds measurements, and the exit codes are fixed: 0 success, 1 usage error, 2 failure.
Output never depends on the locale.

## Development

|Crate|What it is|
|---|---|
|`pixlay-core`|`CollageDoc`, templates, geometry, framing, history. No GTK, no Cairo|
|`pixlay-imaging`|Decoding (glycin), resampling, colour, encoding (PNG/JPEG). No GTK, no Cairo|
|`pixlay-render`|The one `draw(doc, images, target)`, on Cairo|
|`pixlay-cli`|`pixlay-render`: the windowless entry point and the probe surface|
|`pixlay`|`pixlay`: the GTK4 + libadwaita shell|

The verification entry, run before every commit that the product can see:

    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test
    cargo run --release -p pixlay-cli -- render --project crates/pixlay-cli/tests/fixtures/verify.pixlay --long-edge 14043 --stats --out /var/tmp/a.jpg

The GUI tests need a display and a compositor: they start their own private headless `mutter`, which
needs a session bus and a machine-id but no GPU. Where mutter cannot run, the harness can be told to
use the display the process already has, at the cost of that display's own window geometry:

    PIXLAY_TEST_CHILD=1 xvfb-run -a cargo test

The rules the project runs by are in [`AGENTS.md`](AGENTS.md), the shapes it promises are in
[`docs/CONTRACT.md`](docs/CONTRACT.md), the plan lives in [`docs/steps/`](docs/steps/), and
[`docs/ROADMAP.md`](docs/ROADMAP.md) holds directions that are not scheduled. The release history is
in [`CHANGELOG.md`](CHANGELOG.md).

## License

GPL-3.0-or-later — see [`LICENSE`](LICENSE). Issues and patches go to the
[GitHub repository](https://github.com/YangtseSu/pixlay).
