<!--
SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>

SPDX-License-Identifier: GPL-3.0-or-later
-->

# Pixlay

[![Vibe Coded](https://img.shields.io/badge/vibe--coded-%F0%9F%A4%96-8A2BE2)](#how-this-was-built)
[![Release](https://img.shields.io/github/v/release/YangtseSu/pixlay?sort=semver&label=release)](https://github.com/YangtseSu/pixlay/releases/latest)
[![CI](https://github.com/YangtseSu/pixlay/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/YangtseSu/pixlay/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSES/GPL-3.0-or-later.txt)
[![GTK 4](https://img.shields.io/badge/GTK-4-7FE719?logo=gtk)](https://gtk.org)
[![AUR](https://img.shields.io/aur/version/pixlay?label=AUR&logo=archlinux&logoColor=1793d1)](https://aur.archlinux.org/packages/pixlay)

Native Linux photo collage maker designed for GNOME.

Pixlay is a Linux desktop application: a GTK4 + libadwaita window over a Cairo canvas, in Rust. The
target platform is Arch Linux; the license is GPL-3.0-or-later.

The whole application is one window, and the main path is **open → add photos → pick a layout →
adjust → export**, under three minutes. There is no import step: the window opens on the collage,
and photos enter it from outside it — `Add photos…`, a drop from the file manager onto the cell it
is aimed at, the clipboard, or the command line (`pixlay a.jpg b.jpg`, in argument order).

## What it does

- **One to nine photos.** A single photo is a legal collage; a selection or a drop past nine is
  trimmed to the first nine, with one report saying how many were not used.
- **Layouts for every count**: cell counts one … nine over five sheet aspects (4:3, 16:9, 1:1, 3:2,
  2:3), regular and irregular — `pixlay-render templates` lists this build's own. The layout band
  draws each candidate as a *sketch* of its geometry, not a render of your photos.
- **Framing per cell**: pan, zoom and rotation by any angle. The clamp follows the exact angle, so
  the cell stays covered; `Ctrl+0` resets the framing.
- **Swap any two cells** whole — photo and framing both — by dragging one onto the other, with
  `Shift` and a click, or with `Ctrl+Shift` and an arrow key. One undo step.
- **A canvas frame**: the gap between two photos, the corner radius and the colour. White, gapless
  and square-cornered by default, so an older project renders byte-identically.
- **Export as PNG or JPEG** at a long edge you choose, through the platform's own
  save dialog. The file carries its ICC profile and a JPEG is 4:4:4, written in the same pass as
  the pixels.
- **Undo and redo** across every edit, atomic saves, and a check before unsaved work is discarded.
- **`.pixlay` projects**: JSON that embeds its own layout geometry, so a project keeps its pixels
  when the shipped library changes, and relative photo paths that travel with the file.
- **Preview and export are the same renderer.** Resampling happens upstream, in linear light with
  16-bit buffers; the backdrop is opaque; a source photo is never written to.
- **Keyboard-first and accessible**: every action has a shortcut, `Ctrl+?` lists them, every
  control has an accessible name. Dark by default. English source strings, translated through
  gettext — **Simplified Chinese** ships (`po/zh_CN.po`), and anything a catalog does not carry
  stays English.

Pixlay makes a collage and nothing else: no colour grading, no filters, no text layer, no
watermark, no date stamp, no TIFF, no physical sizes or DPI, and no mirroring a cell.

## Install

### Arch Linux

From the AUR, which is where the PKGBUILD is maintained:

    paru -S pixlay

`makepkg` from an AUR checkout builds exactly what `meson` does — the build and the install are the
project's own.

### Prebuilt

Every GitHub release carries the Linux binaries in two tarballs — `pixlay-<version>-linux-amd64.tar.gz`
for x86_64 and `pixlay-<version>-linux-arm64.tar.gz` for aarch64 — each with its `.sha256`, beside the
`x86_64` Arch package. The binaries link the system's GTK, libadwaita and glycin; HEIC and AVIF photos
need `libheif` installed.

### Any Linux, from a checkout

Build it with the project's own build — the requirements, the three commands and everything else a
checkout needs are in [`CONTRIBUTING.md`](CONTRIBUTING.md).

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

## Contributing

Building from a checkout, the tests to run and the project's own rules are in
[`CONTRIBUTING.md`](CONTRIBUTING.md); the release history is in [`CHANGELOG.md`](CHANGELOG.md).
Issues and patches go to the [GitHub repository](https://github.com/YangtseSu/pixlay).

## How this was built

This repository is developed with an AI coding agent: the human sets the requirements and rules at
each gate — the `(…, human)` ruling blocks under `docs/` are the record — and the code is written by
the agent. Commit titles end with 🤖, the marker for AI-written or AI-modified content, which at this
stage is everything. [`AGENTS.md`](AGENTS.md) holds the division of labour and the contracts the
product is held to; [`docs/steps/`](docs/steps/) holds the work, one file per step.

"Looks right" is not a criterion here. A change the product can see runs the verification entry
before it is committed — `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test` — and a claim about pixels becomes a number through `pixlay-render probe` rather than a
glance at an image.

## License

GPL-3.0-or-later — see [`LICENSES/GPL-3.0-or-later.txt`](LICENSES/GPL-3.0-or-later.txt).
