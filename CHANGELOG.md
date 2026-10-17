<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# Changelog

All notable changes to Pixlay are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); dates are ISO 8601, and versions are the
`vX.Y.Z` release tags.

## [Unreleased]

## [0.3.0] - 2026-10-17

### Added

- **Every sheet gets its set.** All six sheet shapes — 1:1, 3:4, 4:3, 2:3, 3:2 and 16:9 — now carry the same
  arrangements (the equal and unequal pairs, the band splits, the strips, the brick pairs, the three-band
  layouts, the nine-grids and the 5-and-4 bands), and every photo count from one to nine offers all six: the
  collage library is **287 templates / 1522 slots**, generated from one *arrangement × sheets* matrix rather
  than one recipe per member. Count 1 carries one sheet per shape, so a single photo can be framed on any of
  them.
- **One order across a count's six cells**: the user's list first, then the members the list does not name,
  then the irregular (L/T) cells last — so a strip shows the same sequence whichever sheet is clicked, and
  `crates/pixlay-core/tests/templates.rs` asserts it over the whole library.
- **ID photo sheets**: one photo repeated at one of six sizes on a 6-inch (4R) print — 一寸 25×35 mm
  (295×413 px at 300 dpi), 小一寸 22×32, 大一寸 33×48, 二寸 35×49, 大二寸 35×53 and 小二寸 35×45 — with a 2 mm
  white gap between the copies and the count each size fits. **The six are listed smallest photo first**,
  and **each is printed the way that holds more copies** (4R has no upright side): 小一寸 eighteen and 一寸
  twelve on the upright page, 小二寸, 大一寸 and 二寸 eight each on the turned one, 大二寸 four either way —
  58 copies against the 50 the upright page holds. Reached from the layout band's ratio row at **one photo** (the member after 16:9, *ID photo* /
  证件照) or from `pixlay-render init --template id-1cun-12-2x3 --photo you.jpg`. The document holds **one
  cell** and the template's four to eighteen slots are the copies the sheet prints, so the photo, its framing
  and its undo step are one thing and `Add photos…` is a single-photo choice on such a sheet; the count
  control stays, and its `+` leaves the sheet for a collage of two cells. Each size reads by name on hover and
  to a screen reader. Six new members (`Template::onePhoto`, the `id` family) put the library at **293
  templates / 1580 slots**, and a template's own slot ceiling (`MAX_TEMPLATE_SLOTS`, 18) is now separate from
  the nine a document holds.

### Changed

- **A new collage opens on the 3:4 portrait page** (`DEFAULT_TEMPLATE`), and growing it keeps that shape:
  the 1→2 switch lands on the portrait page's own layout rather than the album page's, and the ratio row
  leads with 3:4 and 1:1 at every count. The sets' walk (2026-10-17) also aligned the obviously symmetric
  6-photo pairs, set every proportion the list leaves unnamed to the **2:3** split, gave the nine-grid an
  **even** member (exact thirds are not dyadic) and put `mosaic-7-elbow` last in its cell.
- **The thin sheets are no longer thin**: 2:3, 3:2, 4:3 and 16:9 stop offering a single layout at the counts
  they cover — each of their cells holds the count's whole set.

## [0.2.0] - 2026-10-16

### Added

- **AVIF export, and it is the default.** The export's format row offers AVIF / JPEG / PNG
  (`crates/pixlay/src/dialogs.rs`) and a fresh account exports AVIF: measured on the verification
  project it is 2.4x smaller than the JPEG at 4000 px and 3.0x at 147.9 MP, at RMSE 0.013 against
  the same render's PNG where the JPEG's is 0.005. libheif writes it through glycin's encoder API —
  the same `glycin-heif` loader that reads AVIF and HEIC photos — so the sRGB profile goes into the
  file's `colr` box in the same pass as the pixels, and AVIF costs no new dependency. The CLI
  writes it too (`pixlay-render --out x.avif`; the extension has always decided the format), and a
  machine without the heif loader is told what is missing rather than being written another format —
  as is one whose `libheif` has no AV1 encoder (Arch links libaom; a distribution that splits
  libheif's codecs into plugins needs the `aomenc` one, `libheif-plugin-aomenc` on Ubuntu).
- **Ten more layouts, and the sheet keeps its shape.** Every photo count from two to nine now offers
  at least four layouts over at least four shapes, including the album page (4:3), the square and a
  portrait one; **3:4 joins as the sixth sheet ratio** (the portrait of the album page). The library
  reached 37 layouts and 200 cells with that change, and a layout that shipped keeps its geometry —
  nothing an existing project points at moved. The window opens on a 4:3 sheet and a first photo used
  to turn it 3:2 and the second 16:9; the new `grid-2-2x1` closes that chain, so the sheet keeps its
  shape from one photo to nine.
- **Five more arrangements: the poster at the counts that lacked one, and the first cells that are not
  rectangles.** A **poster** — the layout where one photo dominates, Xiaomi's 海报 — is now measured
  rather than assumed (the largest cell at least twice the median one, which the shipped `-hero` family
  and `mosaic-8-s14` satisfy and, measured, `mosaic-7-hero` does not: its two largest cells tie), and
  every count from two to nine now offers one: `mosaic-2-hero` and `mosaic-7-l1r6` are the two that were
  missing, and count 8's is the eight-cell layout the app has always shipped, so it needed no second
  member. Three layouts also carry the first **irregular cells** beyond that one L: an L on the album
  page (`mosaic-3-elbow`), a T on the square (`mosaic-5-tee`) and an L on the portrait page
  (`mosaic-7-elbow`). The library is **42 layouts and 224 cells**; nothing that shipped before moved,
  and the sheet still keeps its shape from one photo to nine.
- **The sheet's shape is the layout stage's first choice.** The band gains a **ratio row**: the shapes
  the current photo count offers — 2:3, 3:4, 1:1, 4:3, 3:2 and 16:9, whichever the count carries — with
  the document's own shape marked, and the layout strip under it narrowed to that shape's arrangements.
  Choosing a shape re-lays the document in one undo step, keeping every photo and its framing, and a
  count that offers one shape (a single-photo collage) draws no row. This is the reference's own model
  — the shape first, then how its cells are arranged — and it is what keeps a count's list of layouts
  navigable as the library grows. The CLI's half is `pixlay-render edit --aspect 1:1`; a shape the
  count has no layout for is refused (exit 2, naming the shapes the count does offer) rather than
  answered with the nearest one, which is where the shape's own report field (`edit`'s `aspect`) comes
  from too — a template's name does not carry its ratio.

## [0.1.2] - 2026-09-27

### Fixed

- **The app icon loads again**: both icon files opened with an XML comment above the `<svg>`
  element, which pushed that element past the end of the file that gdk-pixbuf sniffs an SVG
  by — so the desktop could not read either one and gnome-shell drew a blank icon in the app
  grid (`Could not load a pixbuf from icon theme.`), as did this application's own About
  dialog. The files carry no comments any more (the drawing's rationale is the icon rows in
  `docs/HIG-REVIEW.md` §1), and `crates/pixlay-core/tests/packaging.rs` holds the 256-byte
  window to both of them.

### Added

- **Simplified Chinese**: every string the interface shows, the desktop entry and the AppStream
  metainfo are translated (`po/zh_CN.po`, `zh_CN` in `po/LINGUAS`). English stays the source language
  and the fallback for anything a catalog does not carry, and the CLI is not translated at all.

## [0.1.1] - 2026-09-27

### Added

- Linux **arm64** binaries: every release now carries `pixlay-<version>-linux-arm64.tar.gz` with both
  binaries beside the amd64 tarball, each with its `.sha256`, and both built from the tag by
  `release.yml` (`ubuntu-26.04` and `ubuntu-26.04-arm`).

## [0.1.0] - 2026-09-27

The first release.

### Added

- The editor (`pixlay`): one window over the collage, one to nine photos — from `Add photos…`, a
  drop from the file manager onto the cell it is aimed at, `Ctrl+C` / `Ctrl+X` / `Ctrl+V` on the
  selected cell, or the command line (`pixlay a.jpg b.jpg …`, in argument order). A tenth and beyond
  is trimmed to the first nine with one report.
- 27 layouts for one to nine cells over five sheet aspects (4:3, 16:9, 1:1, 3:2, 2:3), regular and
  irregular, drawn in the layout band as sketches of their own geometry.
- Framing per cell: pan, zoom and rotation by any angle, with the clamp recomputed for the exact
  angle so the visible cell stays covered; `Ctrl+0` resets it.
- Swapping any two cells whole — photo and framing — by a drag onto the other cell, `Shift`+click,
  or `Ctrl+Shift`+arrow; one undo step.
- Undo and redo across every document edit, atomic writes, and a check before unsaved work is
  discarded; `Ctrl+S` and `Ctrl+Shift+S`.
- A canvas frame on the document: the gap between two photos, the cell corner radius and the
  backdrop colour — white, gapless and square-cornered by default, so a project written before the
  field existed renders byte-identically.
- Export as PNG or JPEG at a long edge of 1 … 30000 pixels through the platform's own save dialog
  (`Ctrl+E`); the format, the long edge and the last folder are remembered in
  `~/.config/pixlay/settings.json`.
- One renderer for preview and export, resampling in linear light with 16-bit buffers, an opaque
  backdrop, and source photos that are never written to.
- Encoding in one pass: every export carries its ICC profile, and a JPEG is 4:4:4 in its own `SOF0`.
- Dark by default, keyboard-first with every action bound (`Ctrl+?` lists the shortcuts), an
  accessible name on every control, and gettext-based i18n with English as the source language.
- `pixlay-render`, the windowless half: `render`, `probe`, `image`, `scan`, `thumb`, `templates`,
  `init`, `edit`, `save`, `hit`, `gesture` and `switch`, with `--json` and `--stats`, fixed exit
  codes (0 success, 1 usage error, 2 failure) and output unaffected by locale.
- `.pixlay` projects: JSON that embeds its own layout geometry under a `templateVersion`, so a
  project keeps its pixels when the shipped library changes, with photo paths relative to the file.
- Packaging: a meson build that installs both binaries, the desktop entry, the AppStream metainfo,
  the app icon and its symbolic variant, the `application/x-pixlay` MIME registration and the
  catalogs; an Arch PKGBUILD that wraps it; and a GitHub release carrying the amd64 binaries and
  the `x86_64` package.

[Unreleased]: https://github.com/YangtseSu/pixlay/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/YangtseSu/pixlay/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/YangtseSu/pixlay/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/YangtseSu/pixlay/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/YangtseSu/pixlay/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/YangtseSu/pixlay/releases/tag/v0.1.0
